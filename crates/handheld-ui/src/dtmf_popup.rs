//! A temporary, nonmodal report of symbols decoded by the radio backend.
use super::{color, fitting_scale, rect, text, Row, Slot};

pub(super) const COLUMNS: (u16, u16) = (17, 223);
pub(super) const ROWS: (u16, u16) = (132, 190);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DtmfReception {
    slot: Slot,
    symbols: [u8; 16],
    len: u8,
}

impl DtmfReception {
    pub fn new(slot: Slot, value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 16
            || !bytes.iter().all(|c| b"0123456789ABCD*#".contains(c))
        {
            return None;
        }
        let mut symbols = [0; 16];
        symbols[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            slot,
            symbols,
            len: bytes.len() as u8,
        })
    }

    fn text(&self) -> &str {
        core::str::from_utf8(&self.symbols[..usize::from(self.len)]).unwrap_or("")
    }
}

pub(super) fn paint(row: u16, out: &mut Row<'_>, report: DtmfReception) {
    if !(ROWS.0..ROWS.1).contains(&row) {
        return;
    }
    rect(row, out, 17, ROWS.0, 206, ROWS.1 - ROWS.0, color::ACCENT);
    rect(
        row,
        out,
        19,
        ROWS.0 + 2,
        202,
        ROWS.1 - ROWS.0 - 4,
        color::PANEL,
    );
    text(
        row,
        out,
        26,
        140,
        1,
        match report.slot {
            Slot::A => "DTMF A",
            Slot::B => "DTMF B",
        },
        color::ACCENT,
    );
    let value = report.text();
    let scale = fitting_scale(value, 2, 190);
    text(row, out, 26, 160, scale, value, color::TEXT);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_real_dtmf_symbols_can_reach_the_overlay() {
        assert_eq!(
            DtmfReception::new(Slot::B, "0011ABCD*#").unwrap().text(),
            "0011ABCD*#"
        );
        for invalid in ["", "12345678901234567", "CALL", "Привет", "12\x1f"] {
            assert!(DtmfReception::new(Slot::A, invalid).is_none());
        }
    }
}
