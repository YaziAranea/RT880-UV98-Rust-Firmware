#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Experimental matrix/PTT scanner, never a transmit controller.
//! Columns use open-drain outputs instead of the stock push-pull profile.
//! A qualified passive matrix, caller-supplied settling time, and serialized
//! pin ownership are required. ADC/encoder are not sampled or fabricated.

use uv98_keys::Key;

pub const COLUMN_MASK: u16 = 0x0407;
pub const COLUMN_PINS: [u16; 4] = [1 << 10, 1 << 2, 1 << 1, 1];
pub const PTT_MASK: u16 = 1 << 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixState {
    Released,
    Single(Key),
    /// Multiple observed bits, including stock's 1111/8888 aliases. Must not
    /// be passed to a key debouncer as a normal Released sample.
    Ambiguous,
}

/// Conservative UI policy, deliberately stricter than the stock decoder.
pub const fn classify_matrix(raw: u16) -> MatrixState {
    if raw == 0 {
        return MatrixState::Released;
    }
    if raw.count_ones() != 1 {
        return MatrixState::Ambiguous;
    }
    match uv98_keys::matrix_key(raw) {
        Some(key) => MatrixState::Single(key),
        None => MatrixState::Ambiguous,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    matrix: u16,
    ptt_high: bool,
}
impl Scan {
    pub const fn matrix_raw(self) -> u16 {
        self.matrix
    }
    pub const fn matrix_state(self) -> MatrixState {
        classify_matrix(self.matrix)
    }
    /// Stable sampled PB6-low fact only; no TX action or permission.
    pub const fn ptt_pressed(self) -> bool {
        !self.ptt_high
    }
    pub const fn ptt_high(self) -> bool {
        self.ptt_high
    }
}

/// Fast rejection sample for an ALREADY ARMED upper PTT. It detects any matrix
/// activity, not a key identity, and must never arm TX or feed the UI debouncer.
/// The caller must separately sample auxiliary ADC inputs and retain all
/// release/deadline/profile guards. This is not continuous electrical proof.
/// `scan_idle_or_full` uses this internally only to produce a proven released
/// `Scan`; every pressed matrix/PTT observation still requires both full passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardScan {
    ptt_high: bool,
    matrix_clear: bool,
}
impl GuardScan {
    pub const fn ptt_pressed(self) -> bool {
        !self.ptt_high
    }
    pub const fn matrix_clear(self) -> bool {
        self.matrix_clear
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlePhase {
    /// All columns released, before testing idle rows or selecting another.
    Released,
    /// Owned column(s) pulled low, before sampling inputs. Full scans select
    /// one column; aggregate rejection/release checks select all four.
    Selected,
}

/// Must provide a board/clock-qualified settling interval. No default exists.
/// Must not panic, change these pins or re-enter scanning. Timing/debounce
/// values are application policy, not reconstructed stock milliseconds.
pub trait ScanTiming {
    fn settle(&mut self, phase: SettlePhase);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Low row bits while every column is released. Could be a wiring fault,
    /// insufficient settling or external active drive; never a key code.
    RowsNotReleased {
        low_rows: u8,
    },
    UnstableMatrix {
        forward: u16,
        reverse: u16,
    },
    UnstablePtt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpioRegisters {
    pub cfgr: u32,
    pub omode: u32,
    pub odrvr: u32,
    pub pull: u32,
}
impl GpioRegisters {
    /// PB0/1/2/10: open-drain output, no pull; PB6: pull-up input.
    /// Drive2 matches stock; open-drain differs to avoid driven-high columns.
    pub const fn with_columns_ptt(self) -> Self {
        Self {
            cfgr: (self.cfgr & !0x0030_303F) | 0x0010_0015,
            omode: (self.omode & !0x0447) | 0x0407,
            odrvr: (self.odrvr & !0x0030_303F) | 0x0020_202A,
            pull: (self.pull & !0x0030_303F) | 0x0000_1000,
        }
    }
    /// PC10/11/12: pull-up inputs. No LCD-owned PC13/14 changes.
    pub const fn with_rows_c(self) -> Self {
        Self {
            cfgr: self.cfgr & !0x03F0_0000,
            omode: self.omode & !0x1C00,
            odrvr: (self.odrvr & !0x03F0_0000) | 0x02A0_0000,
            pull: (self.pull & !0x03F0_0000) | 0x0150_0000,
        }
    }
    /// PD2: pull-up input. Other GPIOD fields are preserved.
    pub const fn with_row_d(self) -> Self {
        Self {
            cfgr: self.cfgr & !0x30,
            omode: self.omode & !4,
            odrvr: (self.odrvr & !0x30) | 0x20,
            pull: (self.pull & !0x30) | 0x10,
        }
    }
}

#[cfg(any(test, feature = "hardware"))]
mod engine;
#[cfg(feature = "hardware")]
pub mod hardware;
#[cfg(test)]
mod tests;
