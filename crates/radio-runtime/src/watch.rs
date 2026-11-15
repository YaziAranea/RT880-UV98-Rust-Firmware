//! Time-multiplexed watch on the primary receiver.
use uv98_handheld_ui::controller::Slot;

use crate::policy::Profile;

const DWELL_MS: u32 = 100;
const TONE_DWELL_MS: u32 = 250;
const HANG_MS: u32 = 200;

pub fn eligible(profile: Profile) -> bool {
    matches!(profile.route, 3 | 4)
        && profile.receive.is_some_and(|rx| rx.level > 0)
        && !profile.audio.monitor
}

pub struct Watch {
    slot: Slot,
    dwell: Option<u32>,
    last_open: Option<u32>,
}

impl Watch {
    pub const fn new(slot: Slot) -> Self {
        Self {
            slot,
            dwell: None,
            last_open: None,
        }
    }

    pub const fn slot(&self) -> Slot {
        self.slot
    }

    pub fn observe(&mut self, now: u32, tone_required: bool, audio_open: bool) {
        let since = *self.dwell.get_or_insert(now);
        if audio_open {
            self.last_open = Some(now);
        } else {
            let dwell = if tone_required {
                TONE_DWELL_MS
            } else {
                DWELL_MS
            };
            if now.wrapping_sub(since) >= dwell
                && self
                    .last_open
                    .is_none_or(|open| now.wrapping_sub(open) >= HANG_MS)
            {
                *self = Self::new(self.slot.other());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_waits_for_tone_and_holds_an_open_channel() {
        let mut watch = Watch::new(Slot::A);
        watch.observe(1000, true, false);
        watch.observe(1249, true, false);
        assert_eq!(watch.slot(), Slot::A);
        watch.observe(1250, true, false);
        assert_eq!(watch.slot(), Slot::B);
        watch.observe(2000, false, true);
        watch.observe(3000, false, true);
        watch.observe(3199, false, false);
        assert_eq!(watch.slot(), Slot::B);
        watch.observe(3200, false, false);
        assert_eq!(watch.slot(), Slot::A);
        watch.observe(4000, false, false);
        assert_eq!(watch.slot(), Slot::A);
        watch.observe(4100, false, false);
        assert_eq!(watch.slot(), Slot::B);
    }
}
