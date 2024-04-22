mod async_buffer;
mod frame;

use async_buffer::AsyncBuffer;
use frame::{Frame, ParseError};
use futures::{
    io::{AsyncRead, AsyncWrite},
    Future,
};
use std::{
    error::Error,
    fmt::Display,
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Debug)]
pub enum SinkError {
    Write(std::io::Error),
    Read(std::io::Error),
    LimitExceeded,
    Parse(ParseError),
    Closed,
}

impl Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SinkError::Write(e) => write!(f, "Write Error: {}", e),
            SinkError::Read(e) => write!(f, "Read Error: {}", e),
            SinkError::LimitExceeded => write!(f, "Limit Exceeded"),
            SinkError::Parse(e) => write!(f, "Parse Error: {}", e),
            SinkError::Closed => write!(f, "Stream Error: poll after closed"),
        }
    }
}

impl Error for SinkError {}

pub enum SinkStatus {
    Open,
    Closing,
    Closed,
}

pub struct MessageSink<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream: S,
    read_buffer: Vec<u8>,
    write_buffer: AsyncBuffer,
    scratch: [u8; 1024],
    status: SinkStatus,
    limit: usize,
}

impl<S> MessageSink<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(socket: S) -> Self {
        Self {
            stream: socket,
            read_buffer: Default::default(),
            write_buffer: Default::default(),
            scratch: [0; 1024],
            status: SinkStatus::Open,
            limit: usize::MAX,
        }
    }
    pub fn limit(&mut self, length: usize) {
        self.limit = length;
    }
    pub fn write(&mut self, message: Vec<u8>) -> Result<(), ParseError> {
        let message: Vec<u8> = Frame::new(message).try_into()?;
        self.write_buffer.extend(message);
        Ok(())
    }
    pub fn close(&mut self) {
        self.status = SinkStatus::Closing;
    }
}

impl<S> Future for MessageSink<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    type Output = Result<Vec<u8>, SinkError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let sink = self.get_mut();
        let buffer = sink.write_buffer.as_ref();
        match sink.status {
            SinkStatus::Open => {}
            SinkStatus::Closing => {
                let stream = Pin::new(&mut sink.stream);
                match stream.poll_close(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(_) => {
                        sink.status = SinkStatus::Closed;
                        return Poll::Ready(Err(SinkError::Closed));
                    }
                }
            }
            SinkStatus::Closed => {
                return Poll::Ready(Err(SinkError::Closed));
            }
        }
        let stream = Pin::new(&mut sink.stream);
        match stream.poll_write(cx, buffer) {
            Poll::Ready(Ok(length)) => {
                sink.write_buffer.drain(0..length);
            }
            Poll::Ready(Err(e)) => {
                sink.close();
                return Poll::Ready(Err(SinkError::Write(e)));
            }
            Poll::Pending => {}
        };
        sink.write_buffer.set_waker(cx);
        loop {
            let stream = Pin::new(&mut sink.stream);
            match stream.poll_read(cx, &mut sink.scratch) {
                Poll::Ready(Ok(length)) => {
                    if sink.read_buffer.len() + length > sink.limit {
                        sink.close();
                        return Poll::Ready(Err(SinkError::LimitExceeded));
                    }
                    sink.read_buffer.extend(&sink.scratch[0..length]);
                }
                Poll::Ready(Err(e)) => {
                    sink.close();
                    return Poll::Ready(Err(SinkError::Read(e)));
                }
                Poll::Pending => {
                    break;
                }
            };
            match Frame::try_from(&mut sink.read_buffer) {
                Ok(frame) => return Poll::Ready(Ok(frame.into_message())),
                Err(ParseError::NotReady) => {}
                Err(e) => {
                    sink.close();
                    return Poll::Ready(Err(SinkError::Parse(e)));
                }
            }
        }
        match Frame::try_from(&mut sink.read_buffer) {
            Ok(frame) => return Poll::Ready(Ok(frame.into_message())),
            Err(ParseError::NotReady) => {}
            Err(e) => {
                sink.close();
                return Poll::Ready(Err(SinkError::Parse(e)));
            }
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod message_sink {
    use super::*;
    use futures::{lock::Mutex, FutureExt};
    use futures_ringbuf::RingBuffer;
    use rand::RngCore;
    use std::sync::Arc;

    fn random(len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len];
        rand::thread_rng().fill_bytes(&mut bytes);
        bytes
    }

    #[tokio::test]
    async fn parse() {
        let stream = RingBuffer::new(1024);
        let mut sink = MessageSink::new(stream);
        let message = random(128);
        sink.write(message.clone()).unwrap();
        let received = sink.await.unwrap();
        assert_eq!(message, received);
    }

    #[tokio::test]
    async fn not_ready() {
        let stream = RingBuffer::new(1024);
        let sink = MessageSink::new(stream);
        if sink.now_or_never().is_some() {
            panic!("expected sink to not be ready");
        }
    }

    #[tokio::test]
    async fn parse_multiple() {
        let messages = [random(128), random(128), random(128)];
        let stream = RingBuffer::new(1024);
        let mut sink = MessageSink::new(stream);
        for message in messages.iter() {
            sink.write(message.clone()).unwrap();
        }
        let sink = Arc::new(Mutex::new(sink));
        for message in messages {
            let mut guard = sink.lock().await;
            let received = (&mut *guard).await.unwrap();
            assert_eq!(message, received);
        }
    }

    #[tokio::test]
    async fn limit() {
        let stream = RingBuffer::new(1024);
        let mut sink = MessageSink::new(stream);
        sink.limit(128);
        sink.write(random(256)).unwrap();
        match sink.await {
            Err(SinkError::LimitExceeded) => {}
            Err(e) => panic!("unexpected error {}", e),
            Ok(_) => panic!("unexpected success"),
        };
    }
}
