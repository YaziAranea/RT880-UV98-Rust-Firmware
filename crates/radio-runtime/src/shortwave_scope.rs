//! A measured Si4732 sweep, sharing the ordinary bounded tuning session.
//! This is a sequential RSSI waterfall, not an FFT or a second audio receiver.
//! The caller mutes audio while enabled, then restores the selected tuning.

use crate::shortwave::{Tuning, Update};
use uv98_handheld_ui::controller::{ShortwaveMode, SW_MAX_KHZ, SW_MIN_KHZ};

pub const BINS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sample {
    pub bin: u8,
    pub rssi: u8,
    pub snr: u8,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Window {
    pub first_khz: u32,
    pub step_khz: u16,
}

impl Window {
    pub fn around(tuning: Tuning) -> Self {
        let (low, high, step) = match tuning.mode {
            ShortwaveMode::Fm => (64_000_u32, 108_000_u32, 100_u16),
            ShortwaveMode::Am => (u32::from(SW_MIN_KHZ), u32::from(SW_MAX_KHZ), 5),
            _ => (u32::from(SW_MIN_KHZ), u32::from(SW_MAX_KHZ), 1),
        };
        let span = (BINS as u32 - 1) * u32::from(step);
        let first = tuning
            .khz
            .saturating_sub(BINS as u32 / 2 * u32::from(step))
            .clamp(low, high - span);
        // FM commands use 10 kHz words. Align all bins before presenting them.
        let first = if tuning.mode == ShortwaveMode::Fm {
            first / 10 * 10
        } else {
            first
        };
        Self {
            first_khz: first,
            step_khz: step,
        }
    }

    pub const fn frequency(self, bin: u8) -> u32 {
        self.first_khz + bin as u32 * self.step_khz as u32
    }
}

#[derive(Default)]
pub struct Sweep {
    selection: Option<Tuning>,
    requested: Option<Tuning>,
    bin: u8,
}

impl Sweep {
    /// Return one target until a matching confirmed measurement arrives.
    /// Changes of mode/centre restart the row; disabling restores the exact
    /// user selection, including the SSB BFO, without writing the saved VFO.
    pub fn request(&mut self, selected: Tuning, enabled: bool) -> Tuning {
        if !enabled {
            self.selection = None;
            self.requested = None;
            self.bin = 0;
            return selected;
        }
        if self.selection != Some(selected) {
            self.selection = Some(selected);
            self.bin = 0;
        }
        let target = Tuning {
            khz: Window::around(selected).frequency(self.bin),
            ..selected
        };
        self.requested = Some(target);
        target
    }

    /// Session suppresses results belonging to an earlier tune. Additionally
    /// require the exact target issued by this coordinator, so an old caller
    /// result cannot enter a newly centred waterfall row.
    pub fn observe(&mut self, requested: Tuning, update: Update) -> Option<Sample> {
        if self.requested != Some(requested) || self.selection.is_none() {
            return None;
        }
        let (rssi, snr) = match update {
            Update::Tuned { rssi, snr, .. } | Update::Quality { rssi, snr, .. } => (rssi, snr),
            _ => return None,
        };
        let complete = usize::from(self.bin) + 1 == BINS;
        let sample = Sample {
            bin: self.bin,
            rssi,
            snr,
            complete,
        };
        self.bin = if complete { 0 } else { self.bin + 1 };
        self.requested = None;
        Some(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected(mode: ShortwaveMode, khz: u32) -> Tuning {
        Tuning {
            khz,
            mode,
            bfo: -425,
        }
    }

    #[test]
    fn row_contains_only_confirmed_distinct_bins_and_stop_restores_user_tuning() {
        let tune = selected(ShortwaveMode::Usb, 7100);
        let mut sweep = Sweep::default();
        for bin in 0..BINS {
            let target = sweep.request(tune, true);
            assert_eq!(target.khz, 7084 + bin as u32);
            assert_eq!(target.bfo, tune.bfo);
            assert_eq!(sweep.request(tune, true), target);
            assert_eq!(sweep.observe(target, Update::Tuning), None);
            let update = Update::Tuned {
                part: 0x20,
                rssi: bin as u8,
                snr: 12,
            };
            assert_eq!(
                sweep.observe(target, update),
                Some(Sample {
                    bin: bin as u8,
                    rssi: bin as u8,
                    snr: 12,
                    complete: bin == BINS - 1,
                })
            );
            assert_eq!(sweep.observe(target, update), None);
        }
        assert_eq!(sweep.request(tune, true).khz, 7084);
        assert_eq!(sweep.request(tune, false), tune);
        assert_eq!(
            sweep.observe(
                tune,
                Update::Quality {
                    part: 0x20,
                    rssi: 9,
                    snr: 3
                }
            ),
            None
        );
    }

    #[test]
    fn retuning_restarts_row_and_never_accepts_old_measurement() {
        let mut sweep = Sweep::default();
        let old = sweep.request(selected(ShortwaveMode::Am, 6070), true);
        let next = sweep.request(selected(ShortwaveMode::Am, 7150), true);
        let update = Update::Tuned {
            part: 0x20,
            rssi: 40,
            snr: 5,
        };
        assert_eq!(sweep.observe(old, update), None);
        assert_eq!(sweep.observe(next, update).unwrap().bin, 0);
    }

    #[test]
    fn grid_stays_inside_tuner_limits_at_both_edges() {
        for mode in [
            ShortwaveMode::Am,
            ShortwaveMode::Usb,
            ShortwaveMode::Lsb,
            ShortwaveMode::Cw,
            ShortwaveMode::Fm,
        ] {
            let (low, high) = if mode == ShortwaveMode::Fm {
                (64_000, 108_000)
            } else {
                (u32::from(SW_MIN_KHZ), u32::from(SW_MAX_KHZ))
            };
            for khz in [0, low, high, u32::MAX] {
                let window = Window::around(selected(mode, khz));
                assert!(window.frequency(0) >= low);
                assert!(window.frequency(BINS as u8 - 1) <= high);
                if mode == ShortwaveMode::Fm {
                    assert_eq!(window.frequency(0) % 10, 0);
                }
            }
        }
    }
}
