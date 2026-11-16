//! A measured Si4732 sweep, sharing the ordinary bounded tuning session.
//! This is a sequential RSSI waterfall, not an FFT or a second audio receiver.
//! The caller mutes audio while enabled, then restores the selected tuning.

use crate::shortwave::{Processing, Tuning, Update};
use uv98_handheld_ui::controller::{ShortwaveMode, SW_MAX_KHZ, SW_MIN_KHZ};

const DETAIL_POINTS: u8 = 32;

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
    points: u8,
}

impl Window {
    pub fn around(tuning: Tuning, points: u8) -> Self {
        let points = match points {
            16 | DETAIL_POINTS => points,
            _ => 8,
        };
        let (low, high, detail_step) = match tuning.mode {
            ShortwaveMode::Fm => (64_000_u32, 108_000_u32, 100_u16),
            ShortwaveMode::Am => (u32::from(SW_MIN_KHZ), u32::from(SW_MAX_KHZ), 5),
            _ => (u32::from(SW_MIN_KHZ), u32::from(SW_MAX_KHZ), 1),
        };
        // Keep approximately the same coverage as the detailed view. Each
        // column is one confirmed measurement at the coarser spacing.
        let step = detail_step * u16::from(DETAIL_POINTS / points);
        let span = u32::from(points - 1) * u32::from(step);
        let first = tuning
            .khz
            .saturating_sub(u32::from(points / 2) * u32::from(step))
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
            points,
        }
    }

    pub const fn points(self) -> u8 {
        self.points
    }

    pub const fn frequency(self, bin: u8) -> u32 {
        self.first_khz + bin as u32 * self.step_khz as u32
    }
}

#[derive(Default)]
pub struct Sweep {
    selection: Option<(Tuning, Window)>,
    requested: Option<Tuning>,
    bin: u8,
    processing: Option<Processing>,
}

impl Sweep {
    /// A filter change invalidates the partly measured row as well as its
    /// pending bin. The Session independently rejects pre-change replies.
    pub fn set_processing(&mut self, processing: Processing) {
        if self.processing != Some(processing) {
            self.processing = Some(processing);
            self.selection = None;
            self.requested = None;
            self.bin = 0;
        }
    }

    /// Return one target until a matching confirmed measurement arrives.
    /// Changes of mode, centre or sampling grid restart the row. Omitting
    /// the grid restores the exact user selection, including the SSB BFO.
    pub fn request(&mut self, selected: Tuning, window: Option<Window>) -> Tuning {
        let Some(window) = window else {
            self.selection = None;
            self.requested = None;
            self.bin = 0;
            return selected;
        };
        if self.selection != Some((selected, window)) {
            self.selection = Some((selected, window));
            self.bin = 0;
        }
        let target = Tuning {
            khz: window.frequency(self.bin),
            ..selected
        };
        self.requested = Some(target);
        target
    }

    /// Session suppresses results belonging to an earlier tune. Additionally
    /// require the exact target issued by this coordinator, so an old caller
    /// result cannot enter a newly centred waterfall row.
    pub fn observe(&mut self, requested: Tuning, update: Update) -> Option<Sample> {
        let (_, window) = self.selection?;
        if self.requested != Some(requested) {
            return None;
        }
        let (rssi, snr) = match update {
            Update::Tuned { rssi, snr, .. } | Update::Quality { rssi, snr, .. } => (rssi, snr),
            _ => return None,
        };
        let complete = self.bin + 1 == window.points();
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
        for points in [8, 16, 32] {
            let window = Some(Window::around(tune, points));
            let mut sweep = Sweep::default();
            for bin in 0..points {
                let target = sweep.request(tune, window);
                assert_eq!(target.khz, 7084 + u32::from(bin * (32 / points)));
                assert_eq!(target.bfo, tune.bfo);
                assert_eq!(sweep.request(tune, window), target);
                assert_eq!(sweep.observe(target, Update::Tuning), None);
                let update = Update::Tuned {
                    part: 0x20,
                    rssi: bin,
                    snr: 12,
                };
                assert_eq!(
                    sweep.observe(target, update),
                    Some(Sample {
                        bin,
                        rssi: bin,
                        snr: 12,
                        complete: bin == points - 1,
                    })
                );
                assert_eq!(sweep.observe(target, update), None);
            }
            assert_eq!(sweep.request(tune, window).khz, 7084);
            assert_eq!(sweep.request(tune, None), tune);
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
    }

    #[test]
    fn retuning_restarts_row_and_never_accepts_old_measurement() {
        let mut sweep = Sweep::default();
        let old_tune = selected(ShortwaveMode::Am, 6070);
        let next_tune = selected(ShortwaveMode::Am, 7150);
        let old = sweep.request(old_tune, Some(Window::around(old_tune, 8)));
        let next = sweep.request(next_tune, Some(Window::around(next_tune, 8)));
        let update = Update::Tuned {
            part: 0x20,
            rssi: 40,
            snr: 5,
        };
        assert_eq!(sweep.observe(old, update), None);
        assert_eq!(sweep.observe(next, update).unwrap().bin, 0);
    }

    #[test]
    fn processing_change_discards_partial_row_and_pending_measurement() {
        let mut sweep = Sweep::default();
        let tune = selected(ShortwaveMode::Am, 7100);
        let mut processing = Processing {
            filter: crate::shortwave::AmFilter::Wide6k,
            soft_mute: false,
        };
        sweep.set_processing(processing);
        let window = Some(Window::around(tune, 8));
        let first = sweep.request(tune, window);
        let measurement = Update::Tuned {
            part: 0x20,
            rssi: 40,
            snr: 9,
        };
        assert_eq!(sweep.observe(first, measurement).unwrap().bin, 0);
        let old = sweep.request(tune, window);
        processing.filter = crate::shortwave::AmFilter::Narrow1k;
        sweep.set_processing(processing);
        assert_eq!(sweep.observe(old, measurement), None);
        let restarted = sweep.request(tune, window);
        assert_eq!(restarted, first);
        assert_eq!(sweep.observe(restarted, measurement).unwrap().bin, 0);
    }

    #[test]
    fn changing_point_count_restarts_an_incomplete_row() {
        let mut sweep = Sweep::default();
        let tune = selected(ShortwaveMode::Am, 7100);
        let fast = Some(Window::around(tune, 8));
        let first = sweep.request(tune, fast);
        let update = Update::Tuned {
            part: 0x20,
            rssi: 40,
            snr: 5,
        };
        assert_eq!(sweep.observe(first, update).unwrap().bin, 0);
        let old = sweep.request(tune, fast);
        assert_ne!(old, first);
        let detailed = sweep.request(tune, Some(Window::around(tune, 32)));
        assert_eq!(detailed, first);
        assert_eq!(sweep.observe(old, update), None);
        assert_eq!(sweep.observe(detailed, update).unwrap().bin, 0);
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
            for points in [8, 16, 32] {
                for khz in [0, low, high, u32::MAX] {
                    let window = Window::around(selected(mode, khz), points);
                    assert_eq!(window.points(), points);
                    assert!(window.frequency(0) >= low);
                    assert!(window.frequency(points - 1) <= high);
                    if mode == ShortwaveMode::Fm {
                        assert_eq!(window.frequency(0) % 10, 0);
                    }
                }
            }
        }
    }
}
