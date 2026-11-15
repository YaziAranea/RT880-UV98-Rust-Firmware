//! Physical PTT ownership and release gating. No time, GPIO or RF operations.
//! An eligible edge is still only an input fact: caller must validate channel,
//! calibration, permissions and executor state before transmitting.

/// The UV98 Plus FP1KEY divider (15 kΩ to ground, 10 kΩ pull-up)
/// produces about 2457 on the 12-bit ADC. Top key plus FP1KEY
/// produces about 2010, also in the stock Aux16 range.
pub const fn lower_pressed(raw: u16) -> bool {
    raw >= 2300 && raw <= 2600
}

/// A released ladder reads approximately 4092 on this board. The stock
/// decoder's 0xff class starts much lower and includes ambiguous voltages;
/// only this isolated idle window may qualify a release or upper PTT hold.
pub const fn side_idle(raw: u16) -> bool {
    raw >= 4000 && raw <= 4095
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input {
    pub upper: bool,
    pub lower: bool,
    /// False for an unknown ADC code, matrix activity, unstable scan or read
    /// fault. A clean release requires true with both physical buttons false.
    pub clean: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Upper,
    Lower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guard {
    block_until_release: bool,
    source: Option<Source>,
}

impl Guard {
    /// Boot-held buttons are never eligible until a clean release is observed.
    pub const fn new() -> Self {
        Self {
            block_until_release: true,
            source: None,
        }
    }

    /// Call before cancelling a scan or doing UI/storage work, with the scan's
    /// state from before this input observation. Returns whether the SAME
    /// physical source that was armed is still held alone and cleanly.
    /// A handoff between buttons is a release, even when both map to one slot.
    pub fn observe(&mut self, input: Input, scan_active: bool) -> bool {
        if !input.clean || (input.upper && input.lower) {
            self.disarm();
            return false;
        }
        if !input.upper && !input.lower {
            self.block_until_release = false;
            self.source = None;
            return false;
        }
        if scan_active {
            self.disarm();
            return false;
        }
        if self.block_until_release {
            return false;
        }
        match self.source {
            Some(Source::Upper) if input.upper => true,
            Some(Source::Lower) if input.lower => true,
            Some(_) => {
                self.disarm();
                false
            }
            None => false,
        }
    }

    /// Call only for a fresh, debounced controller TX edge. Holding a button
    /// never arms by itself. Repeated calls while armed do not rearm the source.
    pub fn arm(&mut self, input: Input) -> bool {
        if !input.clean || input.upper == input.lower {
            self.disarm();
            return false;
        }
        if !self.allow_edge() {
            return false;
        }
        self.source = Some(if input.upper {
            Source::Upper
        } else {
            Source::Lower
        });
        true
    }

    /// Use on timeout, explicit cancellation or terminal TX stop. A held source
    /// cannot resume until release and a later explicit debounced edge.
    pub fn disarm(&mut self) {
        self.source = None;
        self.block_until_release = true;
    }

    pub const fn allow_edge(&self) -> bool {
        !self.block_until_release && self.source.is_none()
    }
}

impl Default for Guard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ambiguous_stock_idle_never_releases_or_sustains_a_ptt() {
        use super::*;
        for raw in [0, 3600, 3999, 4096, u16::MAX] {
            assert!(!side_idle(raw));
            let mut guard = Guard::new();
            guard.observe(
                Input {
                    upper: false,
                    lower: false,
                    clean: true,
                },
                false,
            );
            assert!(guard.arm(Input {
                upper: true,
                lower: false,
                clean: true
            }));
            assert!(!guard.observe(
                Input {
                    upper: true,
                    lower: false,
                    clean: side_idle(raw)
                },
                false
            ));
            guard.observe(
                Input {
                    upper: false,
                    lower: false,
                    clean: side_idle(raw),
                },
                false,
            );
            assert!(!guard.allow_edge());
        }
        assert!(side_idle(4000));
        assert!(side_idle(4092));
        assert!(side_idle(4095));
    }

    use super::*;

    const RELEASED: Input = Input {
        upper: false,
        lower: false,
        clean: true,
    };
    const UPPER: Input = Input {
        upper: true,
        lower: false,
        clean: true,
    };
    const LOWER: Input = Input {
        upper: false,
        lower: true,
        clean: true,
    };
    const CHORD: Input = Input {
        upper: true,
        lower: true,
        clean: true,
    };

    fn ready() -> Guard {
        let mut guard = Guard::new();
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.allow_edge());
        guard
    }

    #[test]
    fn lower_ptt_window_excludes_other_keys_and_all_analog_chords() {
        // RT880 V2 page 3: R543 pull-up, R579 top key, R578 FP1KEY
        // and R571 FP2KEY. Pressed key resistors are in parallel.
        let keys = [27_000.0, 15_000.0, 4_700.0];
        for pressed in 0u8..8 {
            let conductance: f64 = keys
                .iter()
                .enumerate()
                .filter(|&(index, _)| pressed & (1 << index) != 0)
                .map(|(_, &ohms)| 1.0 / ohms)
                .sum();
            let raw = (4095.0 / (1.0 + 10_000.0 * conductance)).round() as u16;
            assert_eq!(
                lower_pressed(raw),
                pressed == 0b010,
                "ADC {raw}, keys {pressed:03b}"
            );
        }
        assert!(lower_pressed(2458)); // Archived hardware sample.
        assert!(!lower_pressed(2299));
        assert!(lower_pressed(2300));
        assert!(lower_pressed(2600));
        assert!(!lower_pressed(2601));
    }

    #[test]
    fn lower_ptt_chord_and_timeout_require_a_clean_release_before_rearming() {
        let lower = Input {
            lower: lower_pressed(2458),
            ..RELEASED
        };
        let analog_chord = Input {
            lower: lower_pressed(2010),
            clean: false,
            ..RELEASED
        };
        let mut guard = Guard::new();
        assert!(!guard.arm(lower));
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.arm(lower));
        assert!(guard.observe(lower, false));
        assert!(!guard.observe(analog_chord, false));
        assert!(!guard.arm(lower));
        assert!(!guard.observe(lower, false));
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.arm(lower));
        guard.disarm(); // Executor reached TOT while lower PTT was still held.
        assert!(!guard.arm(lower));
        assert!(!guard.observe(lower, false));
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.allow_edge());
        assert!(!guard.observe(lower, false)); // A raw hold cannot arm TX.
        assert!(guard.arm(lower));
    }

    #[test]
    fn boot_held_and_plain_hold_never_arm_without_clean_release_and_new_edge() {
        let mut guard = Guard::new();
        assert!(!guard.allow_edge());
        for _ in 0..10 {
            assert!(!guard.observe(UPPER, false));
            assert!(!guard.arm(UPPER));
        }
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.allow_edge());
        assert!(!guard.observe(UPPER, false));
        assert!(guard.arm(UPPER));
        assert!(!guard.allow_edge());
        assert!(guard.observe(UPPER, false));
        assert!(!guard.arm(UPPER));
        assert!(guard.observe(UPPER, false));
    }

    #[test]
    fn scan_press_only_stops_even_if_scan_was_cancelled_before_debounce_finished() {
        for button in [UPPER, LOWER] {
            let mut guard = ready();
            assert!(!guard.observe(button, true)); // Raw press cancels scheduler.
            assert!(!guard.allow_edge());
            for _ in 0..10 {
                assert!(!guard.observe(button, false)); // Debounce may now finish.
                assert!(!guard.arm(button));
            }
            assert!(!guard.observe(RELEASED, false));
            assert!(!guard.observe(button, false));
            assert!(guard.arm(button));
            assert!(guard.observe(button, false));
        }
    }

    #[test]
    fn physical_source_handoff_unkeys_immediately_and_requires_both_released() {
        for (first, second) in [(UPPER, LOWER), (LOWER, UPPER)] {
            let mut guard = ready();
            assert!(guard.arm(first));
            assert!(guard.observe(first, false));
            assert!(!guard.observe(second, false));
            assert!(!guard.allow_edge());
            assert!(!guard.arm(second));
            assert!(!guard.observe(first, false)); // Switching back is no release.
            assert!(!guard.arm(first));
            guard.observe(RELEASED, false);
            assert!(guard.arm(second));
            assert!(guard.observe(second, false));
        }
    }

    #[test]
    fn chord_unknown_adc_matrix_or_unstable_input_cannot_be_treated_as_release() {
        for invalid in [
            CHORD,
            Input {
                clean: false,
                ..RELEASED
            },
            Input {
                clean: false,
                ..UPPER
            },
            Input {
                clean: false,
                ..LOWER
            },
        ] {
            let mut guard = ready();
            assert!(guard.arm(UPPER));
            assert!(!guard.observe(invalid, false));
            assert!(!guard.allow_edge());
            assert!(!guard.arm(UPPER));
            assert!(!guard.observe(
                Input {
                    clean: false,
                    ..RELEASED
                },
                false
            ));
            assert!(!guard.allow_edge());
            guard.observe(RELEASED, false);
            assert!(guard.arm(LOWER));
            assert!(guard.observe(LOWER, false));
        }
    }

    #[test]
    fn timeout_disarm_requires_release_and_never_resumes_from_held_input() {
        let mut guard = ready();
        assert!(guard.arm(UPPER));
        guard.disarm(); // Executor TOT has stopped TX, although PTT is held.
        for _ in 0..10 {
            assert!(!guard.observe(UPPER, false));
            assert!(!guard.arm(UPPER));
        }
        guard.observe(RELEASED, false);
        assert!(!guard.observe(UPPER, false)); // A fresh raw hold is not an edge.
        assert!(guard.allow_edge());
        assert!(guard.arm(UPPER));
        assert!(guard.observe(UPPER, false));
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.allow_edge());
    }

    #[test]
    fn arm_rejects_invalid_snapshot_and_scan_input_blocks_before_any_source_exists() {
        for invalid in [
            RELEASED,
            CHORD,
            Input {
                clean: false,
                ..UPPER
            },
        ] {
            let mut guard = ready();
            assert!(!guard.arm(invalid));
            assert!(!guard.allow_edge());
            guard.observe(RELEASED, false);
            assert!(guard.allow_edge());
        }
        let mut guard = ready();
        assert!(!guard.observe(
            Input {
                clean: false,
                ..RELEASED
            },
            true
        ));
        assert!(!guard.arm(UPPER));
        guard.observe(RELEASED, true);
        assert!(guard.allow_edge());
        assert!(!guard.observe(UPPER, true));
        assert!(!guard.arm(UPPER));
    }
}
