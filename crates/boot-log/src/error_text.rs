//! Bounded Debug details, without allocation or private calibration bytes.
use crate::Frame;
use core::fmt::{self, Debug, Write};

pub const CAPACITY: usize = 96;
pub const DETAIL_EVENT: u32 = 0x1f00_0000;
pub struct ErrorText {
    bytes: [u8; CAPACITY],
    len: usize,
    truncated: bool,
}
impl ErrorText {
    pub fn format(error: &impl Debug) -> Self {
        let mut value = Self {
            bytes: [0; CAPACITY],
            len: 0,
            truncated: false,
        };
        if write!(&mut value, "{error:?}").is_err() {
            value.truncated = true;
        }
        value
    }
    pub fn metadata(&self) -> [u32; 3] {
        [self.len as u32, self.truncated as u32, 0]
    }
    pub fn frames(&self) -> impl Iterator<Item = Frame> + '_ {
        self.bytes[..self.len]
            .chunks(12)
            .enumerate()
            .map(|(index, chunk)| {
                let mut bytes = [0; 12];
                bytes[..chunk.len()].copy_from_slice(chunk);
                let words = core::array::from_fn(|word| {
                    u32::from_le_bytes(bytes[word * 4..word * 4 + 4].try_into().unwrap())
                });
                Frame::new(DETAIL_EVENT | index as u32, words)
            })
    }
}
impl Write for ErrorText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let count = (CAPACITY - self.len).min(text.len());
        self.bytes[self.len..self.len + count].copy_from_slice(&text.as_bytes()[..count]);
        self.len += count;
        if count < text.len() {
            self.truncated = true;
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_error_fields_and_stops_at_capacity() {
        let short = ErrorText::format(&1234u32);
        assert_eq!(short.metadata(), [4, 0, 0]);
        assert_eq!(short.frames().count(), 1);
        assert_eq!(
            short.frames().next().unwrap().encode(),
            Frame::new(DETAIL_EVENT, [u32::from_le_bytes(*b"1234"), 0, 0]).encode()
        );
        let long = ErrorText::format(&[u32::MAX; 32]);
        assert_eq!(long.metadata(), [96, 1, 0]);
        assert_eq!(long.frames().count(), 8);
    }
    #[test]
    fn failing_formatter_is_marked_incomplete() {
        struct Broken;
        impl Debug for Broken {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
                Err(fmt::Error)
            }
        }
        let text = ErrorText::format(&Broken);
        assert_eq!(text.metadata(), [0, 1, 0]);
        assert_eq!(text.frames().count(), 0);
    }
}
