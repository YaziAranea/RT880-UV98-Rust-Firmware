//! Qualified same-path primary RX sweeps from original23C58..23CF8.
//! No GPIO/I/O, AGC, audio, timing or claims of physical PLL settling.

use crate::Step;
use uv98_band_plan::{select_routes, SelectorMode};
use uv98_calibration::Calibration;
use uv98_receive::{Chip, FrequencyPlan};
use uv98_stock_model::frequency;

pub const MAX_BINS: usize = 81;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    ZeroFrequency,
    ZeroStep,
    Reversed,
    OffGrid,
    TooManyBins,
    UnsupportedFrequency(u32),
    Calibration(uv98_receive::PlanError),
    DifferentPath,
    Index(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedRange {
    origin: u32,
    start: u32,
    end: u32,
    step: u32,
    raw_offset: u16,
    len: u8,
    band: u8,
    route: u8,
}

impl QualifiedRange {
    /// Validate every bin before any hardware request. Captures the immutable
    /// selected factory frequency offset; later tuning does not reread storage.
    /// Origin must be the full RX setup completed before entering the sweep.
    pub fn new(
        calibration: &Calibration<'_>,
        origin: u32,
        start: u32,
        end: u32,
        step: u32,
    ) -> Result<Self, Error> {
        if origin == 0 || start == 0 || end == 0 {
            return Err(Error::ZeroFrequency);
        }
        if step == 0 {
            return Err(Error::ZeroStep);
        }
        if start > end {
            return Err(Error::Reversed);
        }
        let distance = end - start;
        if distance % step != 0 {
            return Err(Error::OffGrid);
        }
        if distance / step >= MAX_BINS as u32 {
            return Err(Error::TooManyBins);
        }
        let first = FrequencyPlan::new(calibration, Chip::A, origin).map_err(Error::Calibration)?;
        let route = route(origin)?;
        let band = first.selection().band_index();
        let len = distance / step + 1;
        for index in 0..len {
            let frequency = start + index * step;
            let plan =
                FrequencyPlan::new(calibration, Chip::A, frequency).map_err(Error::Calibration)?;
            if plan.selection().band_index() != band
                || route != self::route(frequency)?
                || plan.raw_offset() != first.raw_offset()
            {
                return Err(Error::DifferentPath);
            }
        }
        Ok(Self {
            origin,
            start,
            end,
            step,
            raw_offset: first.raw_offset(),
            len: len as u8,
            band: band as u8,
            route,
        })
    }
    pub const fn len(self) -> usize {
        self.len as usize
    }
    pub const fn is_empty(self) -> bool {
        false
    }
    pub const fn origin(self) -> u32 {
        self.origin
    }
    pub const fn start(self) -> u32 {
        self.start
    }
    pub const fn end(self) -> u32 {
        self.end
    }
    pub const fn step(self) -> u32 {
        self.step
    }
    pub const fn band(self) -> u8 {
        self.band
    }
    pub const fn route(self) -> u8 {
        self.route
    }
    pub const fn antenna_high(self) -> bool {
        self.route != 3 && self.route != 4
    }
    pub const fn rx_mask(self) -> u16 {
        rx_mask(self.route)
    }
    pub fn origin_words(self) -> (u16, u16) {
        let words = frequency::correct_code(self.origin, self.raw_offset);
        (words.reg38, words.reg39)
    }
    pub fn tune(self, index: usize) -> Result<Tune, Error> {
        if index >= self.len() {
            return Err(Error::Index(index));
        }
        let frequency = self.start + index as u32 * self.step;
        let words = frequency::correct_code(frequency, self.raw_offset);
        Ok(Tune {
            frequency,
            reg38: words.reg38,
            reg39: words.reg39,
            rx_mask: self.rx_mask(),
        })
    }
}

fn route(frequency: u32) -> Result<u8, Error> {
    select_routes(frequency, SelectorMode::Primary)
        .and_then(|r| r.rx_code())
        .filter(|r| (1..=5).contains(r))
        .ok_or(Error::UnsupportedFrequency(frequency))
}

const fn rx_mask(route: u8) -> u16 {
    match route {
        1 | 2 => 1,
        3 => 0x10,
        4 => 0x20,
        5 => 2,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tune {
    frequency: u32,
    reg38: u16,
    reg39: u16,
    rx_mask: u16,
}
impl Tune {
    pub const fn frequency(self) -> u32 {
        self.frequency
    }
    /// Exact stock fragment. Executor MUST reject fresh A33 TX bits before
    /// this preserving RMW; this array is not a TX-off or PLL-lock proof.
    pub const fn operations(self) -> [Step; 6] {
        [
            Step::WriteA {
                register: 0x38,
                value: self.reg38,
            },
            Step::WriteA {
                register: 0x39,
                value: self.reg39,
            },
            Step::WriteA {
                register: 0x37,
                value: 0x1f0f,
            },
            Step::WriteA {
                register: 0x30,
                value: 0x0200,
            },
            Step::WriteA {
                register: 0x30,
                value: 0xbff1,
            },
            Step::UpdateA {
                register: 0x33,
                keep_mask: 0xffcc,
                value: self.rx_mask,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn cal() -> Calibration<'static> {
        Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap()
    }

    #[test]
    fn every_bin_uses_same_factory_band_and_route_with_compact_storage() {
        let c = cal();
        let range = QualifiedRange::new(&c, 43_297_500, 43_247_500, 43_347_500, 1250).unwrap();
        assert_eq!(range.len(), 81);
        assert_eq!(range.band(), 1);
        assert_eq!(range.route(), 4);
        assert!(!range.antenna_high());
        assert!(core::mem::size_of::<QualifiedRange>() <= 24);
        for index in 0..range.len() {
            let tune = range.tune(index).unwrap();
            let independent = FrequencyPlan::new(&c, Chip::A, tune.frequency()).unwrap();
            let words = independent.frequency_words();
            assert_eq!(tune.reg38, words.reg38);
            assert_eq!(tune.reg39, words.reg39);
        }
        assert_eq!(range.tune(81), Err(Error::Index(81)));
        let center = range.tune(40).unwrap();
        assert_eq!((center.reg38, center.reg39), (0xab1d, 0x0294));
    }

    #[test]
    fn malformed_grid_gaps_or_cross_band_paths_are_rejected() {
        let c = cal();
        assert_eq!(
            QualifiedRange::new(&c, 0, 1, 2, 1),
            Err(Error::ZeroFrequency)
        );
        assert_eq!(
            QualifiedRange::new(&c, 12_640_000, 12_640_000, 12_640_000, 0),
            Err(Error::ZeroStep)
        );
        assert_eq!(
            QualifiedRange::new(&c, 12_640_000, 12_640_001, 12_640_000, 1),
            Err(Error::Reversed)
        );
        assert_eq!(
            QualifiedRange::new(&c, 12_640_000, 12_640_000, 12_640_001, 2),
            Err(Error::OffGrid)
        );
        assert_eq!(
            QualifiedRange::new(&c, 12_640_000, 12_640_000, 12_640_081, 1),
            Err(Error::TooManyBins)
        );
        for (origin, start, end, step) in [
            (65_999_999, 65_999_999, 66_000_001, 1),
            (14_500_000, 14_500_000, 43_300_000, 28_800_000),
            (12_640_000, 1_799_999, 1_799_999, 1),
        ] {
            assert!(QualifiedRange::new(&c, origin, start, end, step).is_err());
        }
    }

    #[test]
    fn exact_six_requests_keep_rmw_visible_and_never_include_tx_or_audio_enable() {
        let range = QualifiedRange::new(&cal(), 12_640_000, 12_640_000, 12_640_000, 1).unwrap();
        assert_eq!(
            range.tune(0).unwrap().operations(),
            [
                Step::WriteA {
                    register: 0x38,
                    value: 0xdf00
                },
                Step::WriteA {
                    register: 0x39,
                    value: 0x00c0
                },
                Step::WriteA {
                    register: 0x37,
                    value: 0x1f0f
                },
                Step::WriteA {
                    register: 0x30,
                    value: 0x0200
                },
                Step::WriteA {
                    register: 0x30,
                    value: 0xbff1
                },
                Step::UpdateA {
                    register: 0x33,
                    keep_mask: 0xffcc,
                    value: 0x10
                },
            ]
        );
    }

    #[test]
    fn independent_original_thumb_oracle_matches_all_56_sweep_cases() {
        let hex = include_str!("../../../research/radio018/synthetic-calibration.hex").trim();
        let mut bytes = [0; 4096];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        let calibration = Calibration::parse(&bytes).unwrap();
        let rows: Vec<Vec<&str>> = include_str!("../../../research/radio018/fast-sweep.csv")
            .lines()
            .skip(1)
            .filter(|line| !line.is_empty())
            .map(|line| line.split(',').collect())
            .collect();
        assert_eq!(rows.len(), 56 * 7);
        for (case, expected) in rows.chunks_exact(7).enumerate() {
            let first = &expected[0];
            let number = |column: usize| first[column].parse::<u32>().unwrap();
            assert_eq!(number(0) as usize, case);
            let frequency = number(1);
            let mut word33 = number(2) as u16;
            let range =
                QualifiedRange::new(&calibration, frequency, frequency, frequency, 1).unwrap();
            assert_eq!(range.band(), number(3) as u8);
            assert_eq!(range.route(), number(4) as u8);
            let mut observed = Vec::new();
            for operation in range.tune(0).unwrap().operations() {
                match operation {
                    Step::WriteA { register, value } => {
                        observed.push(("write", "A", register, value))
                    }
                    Step::UpdateA {
                        register: 0x33,
                        keep_mask,
                        value,
                    } => {
                        observed.push(("read", "A", 0x33, word33));
                        word33 = (word33 & keep_mask) | value;
                        observed.push(("write", "A", 0x33, word33));
                    }
                    other => panic!("unexpected sweep operation: {other:?}"),
                }
            }
            for (index, (actual, row)) in observed.iter().zip(expected).enumerate() {
                assert_eq!(row[0].parse::<usize>().unwrap(), case);
                assert_eq!(row[5].parse::<usize>().unwrap(), index);
                assert_eq!(
                    *actual,
                    (
                        row[6],
                        row[7],
                        row[8].parse().unwrap(),
                        row[9].parse().unwrap()
                    ),
                    "stock case {case}, event {index}"
                );
            }
            assert_eq!(observed.len(), expected.len());
        }
    }
}
