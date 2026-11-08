#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Experimental related-RT880 SPI3 LCD transport. Hardware feature is opt-in.
//! No clock-tree, panel power, backlight or RF initialization is performed.
//! Stock uses SPI3/AF6, mode2 and half-duplex TX. We use manual GPIO CS and
//! software NSS, plus bounded completion waits and an explicit clock mode.
//! Physical board/IM strap and
//! timing qualification are explicit caller preconditions, not inferred from
//! the reference firmware or its E7=10 command. See research/lcd-hardware.

use core::num::NonZeroU32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Divider {
    Div2 = 0,
    Div4,
    Div8,
    Div16,
    Div32,
    Div64,
    Div128,
    Div256,
}
impl Divider {
    pub const fn divisor(self) -> u16 {
        2 << (self as u8)
    }
    /// Enabled half-duplex TX, master, 8-bit, MSB-first, CPOL1/CPHA0;
    /// software NSS high. Difference from stock C046 is SSMEN/SSIN=1.
    pub const fn ctrl1(self) -> u32 {
        0xC346 | ((self as u32) << 3)
    }
}

/// Mode2 is the related stock word; Mode0 selects rising-edge sampling for the
/// diagnostic panel experiment without changing the byte stream or GPIO pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Mode0 = 0,
    Mode2 = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub divider: Divider,
    pub mode: Mode,
    /// Maximum STS reads per wait, not elapsed time. Must be nonzero.
    pub poll_budget: NonZeroU32,
}

impl Config {
    /// Keep half-duplex TX/master/NSS/frame settings; change only CPOL/CPHA.
    pub const fn ctrl1(self) -> u32 {
        (self.divider.ctrl1() & !3) | self.mode as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingPhase {
    /// DC is stable and CS has just been asserted; before the first clock.
    Setup,
    /// Last byte completed and SPI is not busy; before releasing CS.
    Hold,
    /// CS is high; before this bus method returns and another may begin.
    Inactive,
}

/// Provides board/clock-specific minimum bus timing. There is no default.
/// Implementations must not panic or access these SPI/GPIO resources. Delays
/// must satisfy the actual panel specification, including DC setup/hold and
/// CS inactive timing, at the maximum CPU/peripheral speeds in use.
pub trait BusTiming {
    fn wait(&mut self, phase: TimingPhase);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    TransmitEmpty,
    NotBusy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Timeout(Wait),
    Status(u32),
    /// A previous fault makes this backend permanently unavailable.
    Poisoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpioRegisters {
    pub cfgr: u32,
    pub omode: u32,
    pub odrvr: u32,
    pub pull: u32,
    pub muxl: u32,
    pub muxh: u32,
}
impl GpioRegisters {
    /// PA15 output, push-pull, drive1, no pull. Mux nibble unchanged because
    /// manual CS deliberately replaces the stock PA15 AF6 hardware NSS.
    pub const fn with_cs(self) -> Self {
        Self {
            cfgr: (self.cfgr & !0xC000_0000) | 0x4000_0000,
            omode: self.omode & !0x8000,
            odrvr: (self.odrvr & !0xC000_0000) | 0x4000_0000,
            pull: self.pull & !0xC000_0000,
            ..self
        }
    }
    /// PB3/PB5 AF6, push-pull, drive1, no pull. PB4 is untouched.
    pub const fn with_spi3(self) -> Self {
        Self {
            cfgr: (self.cfgr & !0xCC0) | 0x880,
            omode: self.omode & !0x28,
            odrvr: (self.odrvr & !0xCC0) | 0x440,
            pull: self.pull & !0xCC0,
            muxl: (self.muxl & !0x00F0_F000) | 0x0060_6000,
            ..self
        }
    }
    /// PC13/PC14 output, push-pull, drive1, no pull.
    pub const fn with_control(self) -> Self {
        Self {
            cfgr: (self.cfgr & !0x3C00_0000) | 0x1400_0000,
            omode: self.omode & !0x6000,
            odrvr: (self.odrvr & !0x3C00_0000) | 0x1400_0000,
            pull: self.pull & !0x3C00_0000,
            ..self
        }
    }
}

#[cfg(any(feature = "hardware", test))]
mod engine;
#[cfg(feature = "hardware")]
pub mod hardware;
#[cfg(test)]
mod tests;
