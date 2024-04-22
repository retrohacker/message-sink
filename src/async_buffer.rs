use std::{
    ops::Range,
    task::{Context, Waker},
};

#[derive(Default)]
pub struct AsyncBuffer {
    buffer: Vec<u8>,
    waker: Option<Waker>,
}

impl AsyncBuffer {
    pub fn as_ref(&mut self) -> &Vec<u8> {
        &self.buffer
    }
    pub fn drain(&mut self, range: Range<usize>) {
        self.buffer.drain(range);
    }
    pub fn extend(&mut self, vec: Vec<u8>) {
        self.buffer.extend(vec);
        self.wake();
    }
    pub fn wake(&mut self) {
        if let Some(waker) = &self.waker {
            waker.wake_by_ref()
        }
        self.waker = None;
    }
    pub fn set_waker(&mut self, cx: &mut Context<'_>) {
        self.waker = Some(cx.waker().clone());
    }
}
