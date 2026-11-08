//! Physical PTT ownership and release gating. No time, GPIO or RF operations.
//! An eligible edge is still only an input fact: caller must validate channel,
//! calibration, permissions and executor state before transmitting.

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
