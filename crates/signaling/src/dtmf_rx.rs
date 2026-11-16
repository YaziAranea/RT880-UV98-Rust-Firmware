//! Temporary display of symbols from independently acknowledged DTMF events.

pub const CAPACITY: usize = 16;
pub const DISPLAY_MS: u32 = 3_000;

const SYMBOLS: &[u8; 16] = b"0123456789ABCD*#";

#[derive(Default)]
pub struct DtmfRx {
    symbols: [u8; CAPACITY],
    len: usize,
    last_symbol: u32,
}

impl DtmfRx {
    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn expire(&mut self, now: u32) {
        if now.wrapping_sub(self.last_symbol) >= DISPLAY_MS {
            self.clear();
        }
    }

    /// Record one fresh, acknowledged hardware symbol. The same symbol in a
    /// later IRQ is a new digit. The buffer retains the latest CAPACITY digits.
    pub fn push(&mut self, code: u8, now: u32) {
        let Some(&symbol) = SYMBOLS.get(usize::from(code)) else {
            return;
        };
        self.expire(now);
        if self.len == CAPACITY {
            self.symbols.copy_within(1.., 0);
            self.len -= 1;
        }
        self.symbols[self.len] = symbol;
        self.len += 1;
        self.last_symbol = now;
    }

    pub fn text(&self, now: u32) -> &str {
        if now.wrapping_sub(self.last_symbol) >= DISPLAY_MS {
            return "";
        }
        core::str::from_utf8(&self.symbols[..self.len]).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_symbols_and_repeats_keep_the_most_recent_digits() {
        let mut rx = DtmfRx::default();
        for code in 0..16 {
            rx.push(code, u32::from(code));
        }
        assert_eq!(rx.text(16), "0123456789ABCD*#");
        rx.push(15, 20);
        assert_eq!(rx.text(20), "123456789ABCD*##");
    }

    #[test]
    fn invalid_symbol_does_not_extend_display_and_timeout_wraps() {
        let mut rx = DtmfRx::default();
        let start = u32::MAX - 2;
        rx.push(2, start);
        rx.push(16, start.wrapping_add(1_000));
        assert_eq!(rx.text(start.wrapping_add(DISPLAY_MS - 1)), "2");
        assert_eq!(rx.text(start.wrapping_add(DISPLAY_MS)), "");
        rx.push(3, start.wrapping_add(DISPLAY_MS));
        assert_eq!(rx.text(start.wrapping_add(DISPLAY_MS)), "3");
    }
}
