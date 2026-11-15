//! Relative 10+2 bar scale. This is deliberately not an S-unit, dBm, RF power,
//! SWR or microphone measurement: those calibrations/telemetries do not exist
//! in the present board adapter. The final two cells are visual upper zones.

pub const SEGMENTS: u8 = 12;
pub const RAW_MAX: u16 = 0x01ff;

/// Relative scale. Leave four RAW counts for normal noise variation and
/// spread 56 counts above that over the twelve cells.
pub const fn receive_segments(raw: Option<u16>, floor: u16) -> Option<u8> {
    match raw {
        Some(value) if value <= RAW_MAX => {
            let excess = value.saturating_sub(floor).saturating_sub(4);
            let span = if excess < 56 { excess } else { 56 };
            Some((span * SEGMENTS as u16).div_ceil(56) as u8)
        }
        _ => None,
    }
}

#[derive(Default)]
pub struct NoiseFloor {
    quiet_q4: Option<u16>,
}

impl NoiseFloor {
    pub fn observe_quiet(&mut self, raw: u16) {
        if raw <= RAW_MAX {
            let sample = raw * 16;
            self.quiet_q4 = Some(match self.quiet_q4 {
                None => sample,
                Some(previous) => (7 * previous + sample) / 8,
            });
        }
    }

    pub fn raw(&self) -> u16 {
        self.quiet_q4.map_or(128, |value| value.div_ceil(16))
    }
}

/// Confirmed power *selection*, not measured transmitter output. L/M/H have
/// three fixed marks and never masquerade as a moving modulation envelope.
pub const fn transmit_segments(power: Option<u8>) -> Option<u8> {
    match power {
        Some(0) => Some(3),
        Some(1) => Some(6),
        Some(2) => Some(10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_floor_and_strong_signals_use_the_relative_scale() {
        let mut floor = NoiseFloor::default();
        for raw in [118, 119, 117, 120] {
            floor.observe_quiet(raw);
        }
        assert_eq!(receive_segments(Some(121), floor.raw()), Some(0));
        assert_eq!(receive_segments(Some(135), floor.raw()), Some(3));
        assert_eq!(receive_segments(Some(175), floor.raw()), Some(12));
    }

    #[test]
    fn relative_raw_scale_is_monotonic_and_covers_every_zone() {
        assert_eq!(receive_segments(Some(0), 100), Some(0));
        assert_eq!(receive_segments(Some(RAW_MAX), 100), Some(SEGMENTS));
        let mut seen = [false; 13];
        let mut previous = 0;
        for raw in 0..=RAW_MAX {
            let count = receive_segments(Some(raw), 100).unwrap();
            assert!(count >= previous && count <= SEGMENTS);
            seen[usize::from(count)] = true;
            previous = count;
        }
        assert!(seen.iter().all(|&zone| zone));
        assert_eq!(receive_segments(None, 100), None);
        assert_eq!(receive_segments(Some(RAW_MAX + 1), 100), None);
        assert_eq!(receive_segments(Some(u16::MAX), 100), None);
    }

    #[test]
    fn tx_scale_uses_only_confirmed_power_choices() {
        assert_eq!(transmit_segments(None), None);
        assert_eq!(transmit_segments(Some(0)), Some(3));
        assert_eq!(transmit_segments(Some(1)), Some(6));
        assert_eq!(transmit_segments(Some(2)), Some(10));
        for value in 3..=u8::MAX {
            assert_eq!(transmit_segments(Some(value)), None);
        }
    }
}
