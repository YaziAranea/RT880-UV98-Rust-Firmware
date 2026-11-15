#![no_std]
#![cfg_attr(not(feature = "hardware"), forbid(unsafe_code))]
//! USART2 RX only. No NMEA command transmission and no modem GPIO writes.
pub use uv98_gps_nmea as nmea;
#[cfg(feature = "hardware")]
mod hardware;
#[cfg(feature = "hardware")]
pub use hardware::{on_interrupt, Gps};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Counters {
    pub bytes: u32,
    pub uart_errors: u32,
    pub overflow: u32,
    pub accepted: u32,
    pub rejected: u32,
}

pub const fn baud_divider(apb1_hz: u32, baud: u32) -> Option<u16> {
    if apb1_hz == 0 || baud < 4_800 || baud > 256_000 {
        return None;
    }
    let divisor = (apb1_hz + baud / 2) / baud;
    if divisor < 16 || divisor > 65535 {
        None
    } else {
        Some(divisor as u16)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stock_baud_and_divider() {
        assert_eq!(super::baud_divider(24_000_000, 9_600), Some(2500));
        assert_eq!(super::baud_divider(24_000_000, 115_200), Some(208));
        assert_eq!(super::baud_divider(24_000_000, 0), None);
    }
}
