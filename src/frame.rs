use std::fmt::Display;

#[derive(Debug)]
pub struct Frame(Vec<u8>);

#[derive(Debug)]
pub enum ParseError {
    NotReady,
    Corrupt,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady => write!(f, "Not ready"),
            Self::Corrupt => write!(f, "Corrupt"),
        }
    }
}

impl Frame {
    pub fn new(message: Vec<u8>) -> Self {
        Self(message)
    }
    pub fn into_message(self) -> Vec<u8> {
        self.0
    }
    /// Attempt to parse a message from a buffer, removing the bytes read if we successfully
    /// parse the message (or if the buffer is corrupt)
    /// Note: We do not implement TryFrom for this because that trait takes ownership of the
    /// vector. We want to re-use the same vector across multiple invocations of this
    /// function.
    pub fn try_from(buffer: &mut Vec<u8>) -> std::result::Result<Frame, ParseError> {
        if buffer.len() < 4 {
            return Err(ParseError::NotReady);
        }
        let mut header: [u8; 4] = Default::default();
        header[0] = buffer[0];
        header[1] = buffer[1];
        header[2] = buffer[2];
        header[3] = buffer[3];
        let size: usize = u32::from_le_bytes(header)
            .try_into()
            .map_err(|_| ParseError::Corrupt)?;
        if size + 4 > buffer.len() {
            return Err(ParseError::NotReady);
        }
        buffer.drain(0..4);
        let mut message = Vec::new();
        message.extend(buffer.drain(0..size));
        Ok(Frame(message))
    }
}

/// Serialize a Frame into a framed vector of bytes
impl TryInto<Vec<u8>> for Frame {
    type Error = ParseError;
    fn try_into(self) -> std::result::Result<Vec<u8>, Self::Error> {
        let size: u32 = self.0.len().try_into().map_err(|_| ParseError::Corrupt)?;
        let header = size.to_le_bytes();
        let mut result = Vec::new();
        result.extend(header);
        result.extend(self.0);
        Ok(result)
    }
}

#[cfg(test)]
mod frame_test {
    use rand::RngCore;

    use super::*;

    fn random(len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len];
        rand::thread_rng().fill_bytes(&mut bytes);
        bytes
    }

    #[test]
    fn parse() {
        let message = random(128);
        let frame = Frame::new(message.clone());
        let mut buffer: Vec<u8> = frame.try_into().unwrap();
        assert_eq!(buffer.len(), message.len() + 4, "message wrapped in frame");
        let parsed_frame = Frame::try_from(&mut buffer).unwrap();
        assert_eq!(buffer.len(), 0, "consumed buffer");
        let parsed_message = parsed_frame.into_message();
        assert_eq!(message, parsed_message);
    }

    #[test]
    fn not_ready() {
        let message = random(128);
        let frame = Frame::new(message.clone());
        let mut buffer: Vec<u8> = frame.try_into().unwrap();
        buffer.truncate(128);
        let error = Frame::try_from(&mut buffer);
        match error {
            Err(ParseError::NotReady) => {}
            Err(e) => panic!("unexpected error: {}", e),
            Ok(_) => panic!("unexpected success"),
        }
        assert_eq!(buffer.len(), 128);
    }

    #[test]
    fn parse_multiple() {
        let messages = [random(128), random(128), random(128)];
        let frames = messages.iter().map(|message| Frame::new(message.clone()));
        let mut buffer: Vec<u8> = Vec::new();
        for frame in frames {
            let bytes: Vec<u8> = frame.try_into().unwrap();
            buffer.extend(bytes);
        }
        let mut i = 0;
        while let Ok(frame) = Frame::try_from(&mut buffer) {
            let message = frame.into_message();
            assert_eq!(messages[i], message);
            i += 1;
        }
        assert_eq!(i, 3);
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn parse_with_extra() {
        let message = random(128);
        let frame = Frame::new(message.clone());
        let mut buffer: Vec<u8> = frame.try_into().unwrap();
        buffer.extend(random(3));
        if let Err(e) = Frame::try_from(&mut buffer) {
            panic!("unexpected error: {}", e);
        }
        match Frame::try_from(&mut buffer) {
            Err(ParseError::NotReady) => {}
            Err(e) => panic!("unexpected error: {}", e),
            Ok(_) => panic!("unexpected success"),
        }
        assert_eq!(buffer.len(), 3);
    }
}
