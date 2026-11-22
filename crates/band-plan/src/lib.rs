//! Related RT880 1.16 band routing, reconstructed from bounded machine code.
//!
//! This crate does not access registers, move the relay, tune a receiver, or
//! enable transmission. The returned requests need a separately qualified
//! executor and transition policy. RX requests PRESERVE prior TX-enable bits.
//! In particular, this is not an RF-off proof or a complete RX transition.
//! Frequencies use integer 10 Hz codeplug units, without implying supported
//! physical coverage. See research/band-switch/EVIDENCE-RU.txt.

#![no_std]
#![forbid(unsafe_code)]

/// Civil aviation frequencies, where the ordinary FM TX path is unavailable.
pub const fn aviation_band(f: u32) -> bool {
    f >= 11_800_000 && f < 13_700_000
}

/// Firmware's operating bounds within traced routing coverage, in 10 Hz units.
pub const fn operating_range(f: u32) -> Option<(u32, u32)> {
    match f {
        1_800_000..=66_000_000 => Some((1_800_000, 66_000_000)),
        84_000_000..=99_999_999 => Some((84_000_000, 99_999_999)),
        _ => None,
    }
}

/// Call context of stock frequency selector 0x08012A30.
/// This is a chip/context argument, not modulation or a TX authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectorMode {
    /// Stock argument zero; primary frequency setter 0x08004B88.
    Primary,
    /// Stock argument nonzero; secondary setter 0x08003CCC passes one.
    Secondary,
}

/// Changes to two stock software routing bytes, not electrical pin levels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutingUpdate {
    tx_code: u8,
    rx_code: Option<u8>,
}
impl RoutingUpdate {
    /// New value for RAM 0x20000838. Its downstream consumer selects TX gates.
    pub const fn tx_code(self) -> u8 {
        self.tx_code
    }
    /// New value for RAM 0x20000839. None means stock leaves that byte unchanged.
    pub const fn rx_code(self) -> Option<u8> {
        self.rx_code
    }
}

/// Reconstruct routing writes of 0x08012A30, without its calibration cache.
///
/// Unknown frequencies return None; no previous route is silently reused.
/// The 400..480 MHz branch updates RX even in Secondary mode, unlike the other
/// recognized windows. This observed asymmetry is preserved, not corrected.
pub const fn select_routes(frequency_10hz: u32, mode: SelectorMode) -> Option<RoutingUpdate> {
    let f = frequency_10hz;
    let (tx, rx) = if f >= 1_800_000 && f <= 6_400_000 {
        (
            if f < 3_200_000 { 1 } else { 3 },
            if f < 4_500_000 { 1 } else { 2 },
        )
    } else if f > 6_400_000 && f < 26_000_000 {
        (3, 3)
    } else if f >= 26_000_000 && f <= 66_000_000 {
        (4, 4)
    } else if f >= 84_000_000 && f <= 130_000_000 {
        (5, 5)
    } else {
        return None;
    };
    Some(RoutingUpdate {
        tx_code: tx,
        rx_code: if matches!(mode, SelectorMode::Primary) || (f >= 40_000_000 && f <= 48_000_000) {
            Some(rx)
        } else {
            None
        },
    })
}

/// Exact targets used by the observed receive-path helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    /// RF chip A/U201, reg 0x33. Caller must supply a fresh prior register word.
    PrimaryGpioRegister { value: u16 },
    /// MCU PF0, schematic net UV/HF ANT-SW, driving Q119 and relay K200.
    /// High/low is the requested GPIO level, not a verified contact position.
    AntennaSwitchPf0 { high: bool },
}

/// Stock helper 0x08004FF0 for an explicit raw RAM routing byte.
///
/// Order is register update, then PF0. Bits outside 0x0033, including the
/// external V/U TX gates in bits 3/2, are preserved. Codes 0 and 6..255 clear
/// the four RX gates and request PF0 high, exactly as the stock switch does.
/// No break-before-make delay, settling time, rollback or PA shutdown is added.
pub const fn stock_receive_requests(route_code: u8, previous_reg33: u16) -> [Request; 2] {
    let mask = match route_code {
        1 | 2 => 0x0001, // HF-RXPOWER-EN, chip A GPIO6
        3 => 0x0010,     // V-RXPOWER-EN, chip A GPIO2
        4 => 0x0020,     // U-RXPOWER-EN, chip A GPIO1
        5 => 0x0002,     // U8-RXPOWER-EN, chip A GPIO5
        _ => 0,
    };
    [
        Request::PrimaryGpioRegister {
            value: (previous_reg33 & 0xffcc) | mask,
        },
        Request::AntennaSwitchPf0 {
            high: route_code != 3 && route_code != 4,
        },
    ]
}

/// Primary-context RX plan for a recognized frequency, in integer 10 Hz units.
/// This does not imply that the chosen frequency is physically usable.
pub const fn receive_plan(frequency_10hz: u32, previous_reg33: u16) -> Option<[Request; 2]> {
    match select_routes(frequency_10hz, SelectorMode::Primary) {
        Some(update) => match update.rx_code {
            Some(code) => Some(stock_receive_requests(code, previous_reg33)),
            None => None,
        },
        None => None,
    }
}
