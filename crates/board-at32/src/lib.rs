#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Experimental related-RT880 AT32F423 SPI2/NOR profile, not a verified UV98 BSP.
//!
//! The optional `hardware` feature supplies a PAC-backed implementation of
//! `uv98_nor_read::Bus`. It sends only ordinary READ (03h), never write-enable,
//! program, erase, status writes, chip discovery or recovery commands. This does
//! not initialize the radio, establish RF-off levels, change the clock tree or
//! run from Stage 0. See research/board/SPI2-EVIDENCE.txt for provenance/gaps.

use core::num::NonZeroU32;

/// Power-of-two SPI divisors supported by this bounded backend.
/// Stock uses Div2. The caller must establish the actual APB1 clock and allowed
/// NOR SCK frequency; selecting a divisor is not a clock-frequency measurement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Divider {
    Div2 = 0,
    Div4 = 1,
    Div8 = 2,
    Div16 = 3,
    Div32 = 4,
    Div64 = 5,
    Div128 = 6,
    Div256 = 7,
}

impl Divider {
    pub const fn divisor(self) -> u16 {
        2u16 << (self as u8)
    }

    /// Full-duplex, master, MSB-first, 8-bit, mode 3, software NSS high,
    /// SPI enabled. No DMA, interrupts or CRC are enabled by this word.
    pub const fn ctrl1(self) -> u32 {
        0x0347 | ((self as u32) << 3)
    }
}

/// SPI polarity/phase combinations used for ordinary single-lane NOR READ.
/// Mode3 matches the related stock; Mode0 is an explicit diagnostic alternative.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Mode {
    Mode0 = 0,
    Mode3 = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub divider: Divider,
    pub mode: Mode,
    /// Maximum status-register reads per wait, not microseconds or CPU cycles.
    /// Each transferred byte has separate TDBE and RDBF waits.
    pub poll_budget: NonZeroU32,
}

impl Config {
    /// Change only CPOL/CPHA from the reviewed stock-compatible control word.
    pub const fn ctrl1(self) -> u32 {
        (self.divider.ctrl1() & !3) | self.mode as u32
    }
}

/// Register values for PB12 output/CS and PB13..15 AF5. Other pins preserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpioRegisters {
    pub cfgr: u32,
    pub omode: u32,
    pub odrvr: u32,
    pub pull: u32,
    pub muxh: u32,
}

impl GpioRegisters {
    pub const fn with_spi2_nor(self) -> Self {
        Self {
            cfgr: (self.cfgr & 0x00ff_ffff) | 0xa900_0000,
            omode: self.omode & !0x0000_f000,
            odrvr: (self.odrvr & 0x00ff_ffff) | 0x5500_0000,
            pull: (self.pull & 0x00ff_ffff) | 0x5500_0000,
            muxh: (self.muxh & 0x000f_ffff) | 0x5550_0000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Wait {
    TransmitEmpty,
    ReceiveFull,
    NotBusy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Timeout(Wait),
    /// Entire STS snapshot, including fault bits 3..6.
    Status(u32),
    InvalidState,
    InvalidReadHeader,
    OutOfBounds,
    /// A previous transport failure makes further transactions unavailable.
    Poisoned,
}

#[cfg(any(feature = "hardware", test))]
mod engine;
#[cfg(feature = "hardware")]
pub mod hardware;
#[cfg(test)]
mod tests;
