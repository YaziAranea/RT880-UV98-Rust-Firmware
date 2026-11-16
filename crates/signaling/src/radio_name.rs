//! RT880 stock Radio Name FFSK payload. The BK4819 handles sync, scrambling
//! and CRC; this module encodes only the 16 bytes carried in the FIFO.

pub const NAME_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RadioName {
    payload: [u8; NAME_BYTES],
}

impl RadioName {
    /// Parse the Latin portion of stock's GBK field, including digits and
    /// spaces. Stock CPS pads with FF. Accept NUL padding on input too,
    /// and normalize to the stock representation for transmission.
    pub fn from_payload(mut payload: [u8; NAME_BYTES]) -> Option<Self> {
        let end = payload
            .iter()
            .position(|&byte| matches!(byte, 0 | 0xff))
            .unwrap_or(NAME_BYTES);
        let text = &payload[..end];
        if !text.iter().all(|byte| (b' '..=b'~').contains(byte))
            || !text.iter().any(|&byte| byte != b' ')
            || !payload[end..].iter().all(|&byte| matches!(byte, 0 | 0xff))
        {
            return None;
        }
        payload[end..].fill(0xff);
        Some(Self { payload })
    }

    pub const fn payload(self) -> [u8; NAME_BYTES] {
        self.payload
    }

    pub fn as_str(&self) -> &str {
        let end = self
            .payload
            .iter()
            .position(|&byte| byte == 0xff)
            .unwrap_or(NAME_BYTES);
        core::str::from_utf8(&self.payload[..end]).unwrap_or("")
    }

    pub fn fifo_words(self) -> impl Iterator<Item = u16> {
        (0..NAME_BYTES)
            .step_by(2)
            .map(move |i| u16::from_be_bytes([self.payload[i], self.payload[i + 1]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_name_words_preserve_text_and_padding() {
        let payload = *b"AZ QX R0ABC\xFF\xFF\xFF\xFF\xFF";
        let name = RadioName::from_payload(payload).unwrap();
        assert_eq!(
            RadioName::from_payload(*b"AZ QX R0ABC\0\0\0\0\0"),
            Some(name)
        );
        assert_eq!(name.as_str(), "AZ QX R0ABC");
        assert_eq!(name.payload(), payload);
        assert!(name
            .fifo_words()
            .eq([0x415a, 0x2051, 0x5820, 0x5230, 0x4142, 0x43ff, 0xffff, 0xffff]));
        assert_eq!(
            RadioName::from_payload(*b"QRST UVWXYZ 0123")
                .unwrap()
                .as_str(),
            "QRST UVWXYZ 0123"
        );
    }

    #[test]
    fn invalid_or_blank_names_do_not_become_text_packets() {
        for invalid in [
            [0; NAME_BYTES],
            [b' '; NAME_BYTES],
            [0xff; NAME_BYTES],
            [0x1f; NAME_BYTES],
        ] {
            assert_eq!(RadioName::from_payload(invalid), None);
        }
    }
}
