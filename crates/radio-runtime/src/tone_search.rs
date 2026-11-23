//! BK4819 CSS scan results on a single FM receive frequency.

use uv98_handheld_ui::controller::TX_CTCSS;

const DCS_MASK: u32 = 0x7f_ffff;
// Bitmap of the 104 standard DCS codes, indexed by the nine-bit code.
const DCS_CODES: [u32; 16] = [
    0x4668_0000,
    0x1e20_1a88,
    0x1624_7000,
    0x1424_6428,
    0x0068_0420,
    0x126a_2678,
    0x0620_2240,
    0x0230_4248,
    0x0608_0e00,
    0x0074_3460,
    0x0448_4048,
    0x0020_0040,
    0x0690_0440,
    0x0014_1000,
    0x1608_0408,
    0x0000_1008,
];

pub(crate) fn decode_ctcss(word: u16) -> Option<u16> {
    if word & 0x8000 != 0 {
        return None;
    }
    // The live stock CTCSS encoder uses the 13/26 MHz coefficient.
    let measured = ((u32::from(word & 0x1fff) * 4843 + 5000) / 10_000) as u16;
    TX_CTCSS[1..]
        .iter()
        .copied()
        .find(|tone| tone.abs_diff(measured) <= 8)
        .map(|tone| 0x1000 | tone)
}

pub(crate) fn decode_dcs(high: u16, low: u16) -> Option<u16> {
    let raw = (u32::from(high & 0x0fff) << 12) | u32::from(low & 0x0fff);
    if high & 0xc000 != 0 || raw > DCS_MASK {
        return None;
    }
    let mut phase = raw;
    let mut result = None;
    for _ in 0..23 {
        let inverted = (phase >> 9) & 7 == 3;
        let word = if inverted { phase ^ DCS_MASK } else { phase };
        let code = (word & 0x1ff) as u16;
        if (word >> 9) & 7 == 4
            && DCS_CODES[usize::from(code >> 5)] & (1 << (code & 31)) != 0
            && uv98_rf_sequence::dcs_word(code, false) == Ok(word)
        {
            // A positive and a negative label can represent the same repeated
            // sequence. Choose the smaller code, independent of clock phase.
            if result.is_none_or(|previous| code < previous & 0x1ff) {
                result = Some(code | if inverted { 0x3000 } else { 0x2000 });
            }
        }
        phase = (phase >> 1) | ((phase & 1) << 22);
    }
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Continue,
    Rearm,
    Found(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observation {
    NoCarrier,
    NoTone,
    Tone(u16),
}

#[derive(Default)]
pub struct Search {
    armed_at: u32,
    sampled_at: u32,
    candidate: Option<u16>,
    needs_fresh_arm: bool,
}

impl Search {
    /// Rearm the scan engine before confirming a candidate. A repeated read
    /// of one latched hardware result does not count as a second detection.
    pub fn armed(&mut self, now: u32) {
        self.armed_at = now;
        self.sampled_at = now;
        self.needs_fresh_arm = false;
    }

    pub fn ready(&self, now: u32) -> bool {
        now.wrapping_sub(self.armed_at) >= 120 && now.wrapping_sub(self.sampled_at) >= 30
    }

    pub fn carrier_lost(&mut self) {
        self.candidate = None;
        self.needs_fresh_arm = true;
    }

    pub fn observe(&mut self, now: u32, observation: Observation) -> Action {
        self.sampled_at = now;
        if observation == Observation::NoCarrier {
            self.carrier_lost();
            return Action::Continue;
        }
        if self.needs_fresh_arm {
            return Action::Rearm;
        }
        let expired = now.wrapping_sub(self.armed_at) >= 1000;
        if expired {
            self.candidate = None;
        }
        if let Observation::Tone(tone) = observation {
            if self.candidate == Some(tone) {
                return Action::Found(tone);
            }
            self.candidate = Some(tone);
            return Action::Rearm;
        }
        if expired {
            return Action::Rearm;
        }
        Action::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctcss_matches_standard_tones_and_rejects_noise_busy_and_gaps() {
        for &tone in &TX_CTCSS[1..] {
            let register = (u32::from(tone) * 413 / 200) as u16;
            assert_eq!(decode_ctcss(register), Some(0x1000 | tone));
            assert_eq!(decode_ctcss(0x8000 | register), None);
        }
        for register in [0, 1136, 1407, 8191] {
            assert_eq!(decode_ctcss(register), None);
        }
    }

    #[test]
    fn dcs_validates_parity_and_handles_every_phase_and_polarity() {
        let mut count = 0;
        for code in 0..512_u16 {
            if DCS_CODES[usize::from(code >> 5)] & (1 << (code & 31)) == 0 {
                continue;
            }
            count += 1;
            let normal = uv98_rf_sequence::dcs_word(code, false).unwrap();
            for bits in [normal, normal ^ DCS_MASK] {
                let mut phase = bits;
                let mut expected = None;
                for _ in 0..23 {
                    let result = decode_dcs((phase >> 12) as u16, phase as u16 & 0xfff).unwrap();
                    assert_eq!(*expected.get_or_insert(result), result);
                    let result_word = uv98_rf_sequence::dcs_word(result & 0x1ff, false).unwrap()
                        ^ if result & 0x1000 != 0 { DCS_MASK } else { 0 };
                    let double = u64::from(phase) | (u64::from(phase) << 23);
                    assert!((0..23)
                        .any(|rotation| (double >> rotation) as u32 & DCS_MASK == result_word));
                    phase = (phase >> 1) | ((phase & 1) << 22);
                }
            }
        }
        assert_eq!(count, 104);
        assert_eq!(decode_dcs(0x0763, 0x0813), Some(0x2013));
        assert_eq!(decode_dcs(0x009c, 0x07ec), Some(0x3013));
        assert_eq!(decode_dcs(0x0763, 0x0812), None);
        assert_eq!(decode_dcs(0x8763, 0x0813), None);
        assert_eq!(decode_dcs(0x4763, 0x0813), None);
    }

    #[test]
    fn confirmation_rearms_and_drops_a_candidate_after_carrier_loss() {
        let mut search = Search::default();
        search.armed(u32::MAX - 59);
        assert!(!search.ready(59));
        assert!(search.ready(60));
        assert_eq!(search.observe(60, Observation::Tone(0x1302)), Action::Rearm);
        search.armed(60);
        assert_eq!(
            search.observe(180, Observation::NoCarrier),
            Action::Continue
        );
        assert!(!search.ready(209));
        assert_eq!(
            search.observe(210, Observation::Tone(0x1302)),
            Action::Rearm
        );
        search.armed(210);
        assert_eq!(
            search.observe(330, Observation::Tone(0x1302)),
            Action::Rearm
        );
        search.armed(330);
        assert_eq!(
            search.observe(450, Observation::Tone(0x1302)),
            Action::Found(0x1302)
        );
        assert_eq!(
            search.observe(5000, Observation::NoCarrier),
            Action::Continue
        );
    }
}
