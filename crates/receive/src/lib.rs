//! Bounded configuration of an already prepared receiver, without RF startup.
//!
//! Primary composition: bandwidth, corrected frequency, ordinary calibrated
//! noise/squelch thresholds, and the antenna route. Each fragment comes from
//! related RT880 stock 1.16. The seven-operation composition is NEW: it omits
//! stock tone setup, AFC, gain, audio, power/start registers and override modes.
//! It is not a complete stock receive transition, a settling acknowledgment,
//! an RF-off proof, or a hardware-qualified UV98 driver.
//!
//! Secondary chip support is deliberately limited to its two frequency words.
//! All input validation and calibration reads precede device operations. No
//! calibration/default/storage write, allocation, GPIO/MMIO backend or COM I/O.
//! Exact UV98 MCU-image identity and electrical timing remain unverified.

#![no_std]
#![forbid(unsafe_code)]

use uv98_band_plan::{select_routes, SelectorMode};
use uv98_calibration::{Calibration, Table};
use uv98_stock_model::{frequency, general};

pub mod executor;
pub mod runtime;
pub mod transport;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Chip {
    A,
    B,
}

/// Only the two channel encodings whose bandwidth and calibration agree.
/// Stock raw values other than 0/1 have asymmetric behavior and are rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Width {
    Wide,
    Narrow,
}

impl TryFrom<u8> for Width {
    type Error = PlanError;
    fn try_from(raw: u8) -> Result<Self, Self::Error> {
        match raw {
            0 => Ok(Self::Wide),
            1 => Ok(Self::Narrow),
            _ => Err(PlanError::UnsupportedWidth { raw }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    UnsupportedFrequency { code: u32 },
    UnsupportedWidth { raw: u8 },
    InvalidLevel { level: u8 },
    Calibration(uv98_calibration::Error),
    CalibrationPointUnavailable,
}

/// Observable immutable requests. Executors accept validated Plan types, not
/// user-created Operation slices. A route operation MUST read register A33
/// afresh, reject enabled TX bits, and preserve its other bits at that time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Write {
        chip: Chip,
        register: u8,
        value: u16,
    },
    PrimaryRoute {
        route: u8,
    },
    AntennaPf0 {
        high: bool,
    },
}

/// Owned, fully calculated primary plan. Stores codes, never calibration data
/// by mutable reference. Ordinary squelch path only (stock override disabled).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Plan {
    operations: [Operation; 7],
    frequency_10hz: u32,
    selection: frequency::Selection,
    frequency_words: frequency::FrequencyWords,
    raw_offset: u16,
    noise: u8,
    squelch: u8,
    width: Width,
    level: u8,
}

impl Plan {
    pub fn primary(
        calibration: &Calibration<'_>,
        frequency_10hz: u32,
        width: Width,
        level: u8,
    ) -> Result<Self, PlanError> {
        if level > 10 {
            return Err(PlanError::InvalidLevel { level });
        }
        let tuning = FrequencyPlan::new(calibration, Chip::A, frequency_10hz)?;
        let band = calibration
            .band(tuning.selection.band_index())
            .map_err(PlanError::Calibration)?;
        let point = tuning.selection.table_index();
        let (noise_table, squelch_table, bandwidth) = match width {
            Width::Wide => (Table::NoiseWide, Table::SquelchWide, 0x3028),
            Width::Narrow => (Table::NoiseNarrow, Table::SquelchNarrow, 0x4048),
        };
        // valid_points() describes the tuning UI, not the stock selector.
        let noise = band
            .point_raw(noise_table, point)
            .ok_or(PlanError::CalibrationPointUnavailable)?;
        let squelch = band
            .point_raw(squelch_table, point)
            .ok_or(PlanError::CalibrationPointUnavailable)?;
        let thresholds = general::squelch_words(level, noise, squelch)
            .ok_or(PlanError::InvalidLevel { level })?;
        // Route selection also uses the ORIGINAL frequency, before correction.
        let route = select_routes(frequency_10hz, SelectorMode::Primary)
            .and_then(|r| r.rx_code())
            .ok_or(PlanError::UnsupportedFrequency {
                code: frequency_10hz,
            })?;
        Ok(Self {
            operations: [
                Operation::Write {
                    chip: Chip::A,
                    register: 0x43,
                    value: bandwidth,
                },
                tuning.operations[0],
                tuning.operations[1],
                Operation::Write {
                    chip: Chip::A,
                    register: 0x4f,
                    value: thresholds.noise_4f,
                },
                Operation::Write {
                    chip: Chip::A,
                    register: 0x78,
                    value: thresholds.squelch_78,
                },
                Operation::PrimaryRoute { route },
                Operation::AntennaPf0 {
                    high: route != 3 && route != 4,
                },
            ],
            frequency_10hz,
            selection: tuning.selection,
            frequency_words: tuning.words,
            raw_offset: tuning.raw_offset,
            noise,
            squelch,
            width,
            level,
        })
    }
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }
    pub const fn frequency_10hz(&self) -> u32 {
        self.frequency_10hz
    }
    pub const fn selection(&self) -> frequency::Selection {
        self.selection
    }
    pub const fn frequency_words(&self) -> frequency::FrequencyWords {
        self.frequency_words
    }
    pub const fn raw_offset(&self) -> u16 {
        self.raw_offset
    }
    pub const fn noise_calibration(&self) -> u8 {
        self.noise
    }
    pub const fn squelch_calibration(&self) -> u8 {
        self.squelch
    }
    pub const fn width(&self) -> Width {
        self.width
    }
    /// Zero still writes threshold words. Software squelch-open policy is
    /// outside this plan; level zero is not an RF shutdown/disable operation.
    pub const fn level(&self) -> u8 {
        self.level
    }
}

/// The two observed frequency writes on one chip; no shared route changes,
/// secondary bandwidth/squelch assumptions, PLL latch, enable or lock check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrequencyPlan {
    operations: [Operation; 2],
    chip: Chip,
    frequency_10hz: u32,
    selection: frequency::Selection,
    words: frequency::FrequencyWords,
    raw_offset: u16,
}

impl FrequencyPlan {
    pub fn new(
        calibration: &Calibration<'_>,
        chip: Chip,
        frequency_10hz: u32,
    ) -> Result<Self, PlanError> {
        let selection =
            frequency::select(frequency_10hz).ok_or(PlanError::UnsupportedFrequency {
                code: frequency_10hz,
            })?;
        let band = calibration
            .band(selection.band_index())
            .map_err(PlanError::Calibration)?;
        let raw_offset = band.frequency_offset_raw();
        let words = frequency::correct_code(frequency_10hz, raw_offset);
        Ok(Self {
            operations: [
                Operation::Write {
                    chip,
                    register: 0x38,
                    value: words.reg38,
                },
                Operation::Write {
                    chip,
                    register: 0x39,
                    value: words.reg39,
                },
            ],
            chip,
            frequency_10hz,
            selection,
            words,
            raw_offset,
        })
    }
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }
    pub const fn chip(&self) -> Chip {
        self.chip
    }
    pub const fn frequency_10hz(&self) -> u32 {
        self.frequency_10hz
    }
    pub const fn selection(&self) -> frequency::Selection {
        self.selection
    }
    pub const fn frequency_words(&self) -> frequency::FrequencyWords {
        self.words
    }
    pub const fn raw_offset(&self) -> u16 {
        self.raw_offset
    }
}
