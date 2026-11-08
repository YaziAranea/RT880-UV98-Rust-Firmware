//! Input decoding observed in a pinned related RT880 1.16 image.
//!
//! Exact UV98 MCU-image identity and the user's physical keypad wiring remain
//! unverified. This crate has no MMIO, pin writes, serial access, transmit
//! commands, allocation, or unsafe code. A `Ptt` event is an input fact only.
//! Debounce and hold timing are caller-selected UI policy, not stock timing.

#![no_std]
#![forbid(unsafe_code)]

mod debounce;
pub use debounce::{ButtonEvent, ClockError, Debouncer, Timing, TimingError};

/// Logical matrix labels agree with the related stock codes and fagci's map.
/// Auxiliary codes deliberately do not claim unverified physical labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Menu,
    Up,
    Down,
    Exit,
    Star,
    Hash,
    Aux16,
    Aux17,
    Aux18,
    Ptt,
}

/// Exact pure mapping at 0x08013084 in the related reference image.
///
/// Bits are `column * 4 + row`. Most multiple-key combinations return 0xFF;
/// the stock decoder explicitly accepts 0x1111 and 0x8888 as star/hash.
/// 0xFF means no recognized matrix key, not proof that all keys are released.
pub const fn stock_matrix_code(raw: u16) -> u8 {
    match raw {
        0x0001 => 1,
        0x0010 => 2,
        0x0100 => 3,
        0x0002 => 4,
        0x0020 => 5,
        0x0200 => 6,
        0x0004 => 7,
        0x0040 => 8,
        0x0400 => 9,
        0x0008 | 0x1111 => 14,
        0x0080 => 0,
        0x0800 | 0x8888 => 15,
        0x1000 => 10,
        0x8000 => 13,
        0x2000 => 11,
        0x4000 => 12,
        _ => 0xFF,
    }
}

/// Recognized matrix key. Callers should retain `raw` to distinguish an empty
/// matrix from an unrecognized combination; this function cannot deghost it.
pub const fn matrix_key(raw: u16) -> Option<Key> {
    match stock_matrix_code(raw) {
        0 => Some(Key::Digit0),
        1 => Some(Key::Digit1),
        2 => Some(Key::Digit2),
        3 => Some(Key::Digit3),
        4 => Some(Key::Digit4),
        5 => Some(Key::Digit5),
        6 => Some(Key::Digit6),
        7 => Some(Key::Digit7),
        8 => Some(Key::Digit8),
        9 => Some(Key::Digit9),
        10 => Some(Key::Menu),
        11 => Some(Key::Up),
        12 => Some(Key::Down),
        13 => Some(Key::Exit),
        14 => Some(Key::Star),
        15 => Some(Key::Hash),
        _ => None,
    }
}

/// Exact unsigned sample mapping at 0x08013D68. The source is a u16 loaded
/// from RAM 0x200010E6, populated from the ADC DMA sample at 0x200010CE.
/// This models the function for all u16 values, without claiming ADC scaling
/// or physical meanings for codes 0x16, 0x17, and 0x18.
pub const fn stock_auxiliary_code(sample: u16) -> u8 {
    if sample < 1885 {
        0x17
    } else if sample < 2717 {
        0x16
    } else if sample < 3542 {
        0x18
    } else {
        0xFF
    }
}

pub const fn auxiliary_key(sample: u16) -> Option<Key> {
    match stock_auxiliary_code(sample) {
        0x16 => Some(Key::Aux16),
        0x17 => Some(Key::Aux17),
        0x18 => Some(Key::Aux18),
        _ => None,
    }
}

/// GPIOB bit 6 is low when asserted in the traced stock PTT consumer.
pub const fn ptt_pressed(pb6_high: bool) -> bool {
    !pb6_high
}

/// Logical scan order, not permission or an instruction to drive a pin.
/// Related-stock columns are PB10, PB2, PB1, PB0 respectively.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Column {
    C0 = 0,
    C1 = 1,
    C2 = 2,
    C3 = 3,
}

/// Sampled input levels in stock row order PC10, PC11, PC12, PD2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RowLevels {
    pub pc10_high: bool,
    pub pc11_high: bool,
    pub pc12_high: bool,
    pub pd2_high: bool,
}

/// Pure equivalent of row sampling at 0x0801FA9C for columns 0..3.
/// Only the selected column's four bits change; other samples are retained.
/// A complete, stable matrix scan is the caller's responsibility.
pub const fn update_matrix(previous: u16, column: Column, rows: RowLevels) -> u16 {
    let pressed = (!rows.pc10_high as u16)
        | ((!rows.pc11_high as u16) << 1)
        | ((!rows.pc12_high as u16) << 2)
        | ((!rows.pd2_high as u16) << 3);
    let shift = column as u8 * 4;
    (previous & !(0xF_u16 << shift)) | (pressed << shift)
}

/// Schematic-only encoder input capture. Polarity, population, direction,
/// and stock quadrature handling are unverified; no decoder is provided.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncoderLevels {
    pub code1_high: bool,
    pub code2_high: bool,
}

/// Caller-supplied observations. Optional encoder levels are retained as raw
/// data and have no effect on keypad/PTT decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawInputs {
    pub matrix: u16,
    pub ptt_high: bool,
    pub auxiliary_adc: u16,
    pub encoder: Option<EncoderLevels>,
}

/// Integration boundary only. This crate never calls or implements GPIO I/O.
pub trait InputSource {
    type Error;
    fn sample(&mut self) -> Result<RawInputs, Self::Error>;
}
