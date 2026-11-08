//! Debounced foreground input. Invalid scans cancel pending gestures and require
//! a clean release. No event carries an RF or storage operation.
use uv98_keys::{ClockError, Key, Timing};

pub const REPEAT_MS: u32 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample {
    Released,
    Key(Key),
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Short(Key),
    Hold(Key),
    Repeat(Key),
    Ptt(bool),
    /// A normalized second side input. The board runtime owns physical mapping.
    PttB(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputState {
    WaitingForRelease,
    Ready,
    Unstable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Press {
    key: Key,
    since: u32,
    accepted: bool,
    held: bool,
    repeat_at: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputTracker {
    timing: Timing,
    state: InputState,
    last_ms: Option<u32>,
    release_since: Option<u32>,
    press: Option<Press>,
}

impl InputTracker {
    pub const fn new(timing: Timing) -> Self {
        Self {
            timing,
            state: InputState::WaitingForRelease,
            last_ms: None,
            release_since: None,
            press: None,
        }
    }

    pub const fn state(&self) -> InputState {
        self.state
    }

    pub const fn is_ready(&self) -> bool {
        matches!(self.state, InputState::Ready)
    }

    /// At most one event per observation. Delayed polling never produces a
    /// burst of repeats. Clock errors leave the complete tracker unchanged.
    pub fn sample(
        &mut self,
        now_ms: u32,
        sample: Sample,
    ) -> Result<Option<InputEvent>, ClockError> {
        if self
            .last_ms
            .is_some_and(|last| now_ms.wrapping_sub(last) >= (1 << 31))
        {
            return Err(ClockError);
        }
        self.last_ms = Some(now_ms);
        match sample {
            Sample::Invalid => Ok(self.invalidate()),
            Sample::Released => {
                let Some(since) = self.release_since else {
                    self.release_since = Some(now_ms);
                    return Ok(None);
                };
                if now_ms.wrapping_sub(since) < self.timing.debounce_ms() {
                    return Ok(None);
                }
                self.state = InputState::Ready;
                let finished = self.press.take();
                Ok(finished.and_then(|press| {
                    if !press.accepted {
                        None
                    } else if let Some(event) = side_event(press.key, false) {
                        Some(event)
                    } else if delayed_short(press.key) && !press.held {
                        Some(InputEvent::Short(press.key))
                    } else {
                        None
                    }
                }))
            }
            Sample::Key(key) => {
                self.release_since = None;
                if self.state != InputState::Ready {
                    return Ok(None);
                }
                let Some(mut press) = self.press else {
                    self.press = Some(Press {
                        key,
                        since: now_ms,
                        accepted: false,
                        held: false,
                        repeat_at: now_ms,
                    });
                    return Ok(None);
                };
                if press.key != key {
                    return Ok(self.invalidate());
                }
                if !press.accepted {
                    if now_ms.wrapping_sub(press.since) < self.timing.debounce_ms() {
                        return Ok(None);
                    }
                    press.accepted = true;
                    // Hold starts at the confirmed press, matching Timing.
                    press.since = now_ms;
                    press.repeat_at = now_ms;
                    self.press = Some(press);
                    return Ok(if let Some(event) = side_event(key, true) {
                        Some(event)
                    } else if delayed_short(key) {
                        None
                    } else {
                        Some(InputEvent::Short(key))
                    });
                }
                if now_ms.wrapping_sub(press.since) >= self.timing.hold_ms() {
                    if matches!(key, Key::Up | Key::Down) {
                        if !press.held || now_ms.wrapping_sub(press.repeat_at) >= REPEAT_MS {
                            press.held = true;
                            press.repeat_at = now_ms;
                            self.press = Some(press);
                            return Ok(Some(InputEvent::Repeat(key)));
                        }
                    } else if delayed_short(key) && !press.held {
                        press.held = true;
                        self.press = Some(press);
                        return Ok(Some(InputEvent::Hold(key)));
                    }
                }
                Ok(None)
            }
        }
    }

    fn invalidate(&mut self) -> Option<InputEvent> {
        let release = self
            .press
            .filter(|press| press.accepted)
            .and_then(|press| side_event(press.key, false));
        self.press = None;
        self.release_since = None;
        self.state = InputState::Unstable;
        release
    }
}

const fn side_event(key: Key, down: bool) -> Option<InputEvent> {
    match key {
        Key::Ptt => Some(InputEvent::Ptt(down)),
        Key::Aux16 => Some(InputEvent::PttB(down)),
        _ => None,
    }
}

const fn delayed_short(key: Key) -> bool {
    matches!(
        key,
        Key::Menu
            | Key::Star
            | Key::Hash
            | Key::Exit
            | Key::Digit0
            | Key::Digit1
            | Key::Digit2
            | Key::Digit3
            | Key::Digit4
            | Key::Digit5
            | Key::Digit6
            | Key::Digit7
            | Key::Digit8
            | Key::Digit9
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> InputTracker {
        InputTracker::new(Timing::new(30, 600).unwrap())
    }

    fn ready() -> InputTracker {
        let mut input = tracker();
        assert_eq!(input.sample(0, Sample::Released), Ok(None));
        assert_eq!(input.sample(30, Sample::Released), Ok(None));
        assert!(input.is_ready());
        input
    }

    #[test]
    fn boot_held_and_invalid_chords_require_clean_release() {
        let mut input = tracker();
        for now in [0, 100, 1000] {
            assert_eq!(input.sample(now, Sample::Key(Key::Menu)), Ok(None));
        }
        input.sample(1100, Sample::Released).unwrap();
        input.sample(1130, Sample::Released).unwrap();
        input.sample(1200, Sample::Key(Key::Menu)).unwrap();
        input.sample(1230, Sample::Key(Key::Menu)).unwrap();
        input.sample(1240, Sample::Invalid).unwrap();
        assert!(!input.is_ready());
        assert_eq!(input.sample(2000, Sample::Key(Key::Menu)), Ok(None));
        input.sample(2100, Sample::Released).unwrap();
        assert_eq!(input.sample(2130, Sample::Released), Ok(None));
        assert!(input.is_ready());
    }

    #[test]
    fn short_menu_waits_for_release_and_long_menu_never_adds_short() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Menu)).unwrap();
        assert_eq!(input.sample(130, Sample::Key(Key::Menu)), Ok(None));
        input.sample(200, Sample::Released).unwrap();
        assert_eq!(input.sample(229, Sample::Released), Ok(None));
        assert_eq!(
            input.sample(230, Sample::Released),
            Ok(Some(InputEvent::Short(Key::Menu)))
        );
        input.sample(300, Sample::Key(Key::Menu)).unwrap();
        input.sample(330, Sample::Key(Key::Menu)).unwrap();
        assert_eq!(
            input.sample(930, Sample::Key(Key::Menu)),
            Ok(Some(InputEvent::Hold(Key::Menu)))
        );
        assert_eq!(input.sample(3000, Sample::Key(Key::Menu)), Ok(None));
        input.sample(3010, Sample::Released).unwrap();
        assert_eq!(input.sample(3040, Sample::Released), Ok(None));
    }

    #[test]
    fn navigation_repeats_without_catching_up_after_slow_render() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Down)).unwrap();
        assert_eq!(
            input.sample(130, Sample::Key(Key::Down)),
            Ok(Some(InputEvent::Short(Key::Down)))
        );
        assert_eq!(input.sample(729, Sample::Key(Key::Down)), Ok(None));
        assert_eq!(
            input.sample(730, Sample::Key(Key::Down)),
            Ok(Some(InputEvent::Repeat(Key::Down)))
        );
        assert_eq!(
            input.sample(10_000, Sample::Key(Key::Down)),
            Ok(Some(InputEvent::Repeat(Key::Down)))
        );
        assert_eq!(input.sample(10_001, Sample::Key(Key::Down)), Ok(None));
        input.sample(10_010, Sample::Released).unwrap();
        input.sample(10_040, Sample::Released).unwrap();
        assert_eq!(input.sample(10_500, Sample::Released), Ok(None));
    }

    #[test]
    fn ptt_reports_both_states_and_invalid_sample_releases_feedback() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Ptt)).unwrap();
        assert_eq!(
            input.sample(130, Sample::Key(Key::Ptt)),
            Ok(Some(InputEvent::Ptt(true)))
        );
        assert_eq!(
            input.sample(140, Sample::Invalid),
            Ok(Some(InputEvent::Ptt(false)))
        );
        assert_eq!(input.sample(5000, Sample::Key(Key::Ptt)), Ok(None));
        input.sample(5010, Sample::Released).unwrap();
        input.sample(5040, Sample::Released).unwrap();
        input.sample(5100, Sample::Key(Key::Ptt)).unwrap();
        input.sample(5130, Sample::Key(Key::Ptt)).unwrap();
        input.sample(5200, Sample::Released).unwrap();
        assert_eq!(
            input.sample(5230, Sample::Released),
            Ok(Some(InputEvent::Ptt(false)))
        );
    }

    #[test]
    fn backward_time_is_atomic_and_wrap_is_supported() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Up)).unwrap();
        let old = input;
        assert_eq!(input.sample(99, Sample::Key(Key::Up)), Err(ClockError));
        assert_eq!(input, old);
        let mut input = tracker();
        input.sample(u32::MAX - 20, Sample::Released).unwrap();
        input.sample(10, Sample::Released).unwrap();
        assert!(input.is_ready());
    }

    #[test]
    fn different_key_without_release_cancels_gesture_and_release_bounce_is_ignored() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Menu)).unwrap();
        input.sample(130, Sample::Key(Key::Menu)).unwrap();
        input.sample(140, Sample::Released).unwrap();
        // A release shorter than debounce does not create a second gesture.
        input.sample(150, Sample::Key(Key::Menu)).unwrap();
        assert_eq!(input.sample(160, Sample::Key(Key::Exit)), Ok(None));
        assert_eq!(input.state(), InputState::Unstable);
        input.sample(170, Sample::Released).unwrap();
        assert_eq!(input.sample(200, Sample::Released), Ok(None));
        assert!(input.is_ready());
        input.sample(220, Sample::Key(Key::Menu)).unwrap();
        input.sample(250, Sample::Key(Key::Menu)).unwrap();
        input.sample(300, Sample::Released).unwrap();
        assert_eq!(
            input.sample(330, Sample::Released),
            Ok(Some(InputEvent::Short(Key::Menu)))
        );
    }

    #[test]
    fn all_assignable_digits_emit_short_on_release_or_one_hold_never_both() {
        for key in [
            Key::Digit0,
            Key::Digit1,
            Key::Digit2,
            Key::Digit3,
            Key::Digit4,
            Key::Digit5,
            Key::Digit6,
            Key::Digit7,
            Key::Digit8,
            Key::Digit9,
        ] {
            let mut input = ready();
            assert_eq!(input.sample(100, Sample::Key(key)), Ok(None));
            assert_eq!(input.sample(130, Sample::Key(key)), Ok(None));
            assert_eq!(input.sample(200, Sample::Released), Ok(None));
            assert_eq!(
                input.sample(230, Sample::Released),
                Ok(Some(InputEvent::Short(key)))
            );
            assert_eq!(input.sample(300, Sample::Key(key)), Ok(None));
            assert_eq!(input.sample(330, Sample::Key(key)), Ok(None));
            assert_eq!(input.sample(929, Sample::Key(key)), Ok(None));
            assert_eq!(
                input.sample(930, Sample::Key(key)),
                Ok(Some(InputEvent::Hold(key)))
            );
            assert_eq!(input.sample(5000, Sample::Key(key)), Ok(None));
            assert_eq!(input.sample(5010, Sample::Released), Ok(None));
            assert_eq!(input.sample(5040, Sample::Released), Ok(None));
        }
    }

    #[test]
    fn invalid_zero_or_other_digit_hold_never_commits_or_emits_a_short() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Digit0)).unwrap();
        assert_eq!(input.sample(130, Sample::Key(Key::Digit0)), Ok(None));
        assert_eq!(input.sample(200, Sample::Invalid), Ok(None));
        assert_eq!(input.sample(730, Sample::Key(Key::Digit0)), Ok(None));
        input.sample(800, Sample::Released).unwrap();
        assert_eq!(input.sample(830, Sample::Released), Ok(None));
        input.sample(900, Sample::Key(Key::Digit3)).unwrap();
        input.sample(930, Sample::Key(Key::Digit3)).unwrap();
        assert_eq!(input.sample(1000, Sample::Invalid), Ok(None));
        assert_eq!(input.sample(2000, Sample::Key(Key::Digit3)), Ok(None));
        input.sample(2100, Sample::Released).unwrap();
        assert_eq!(input.sample(2130, Sample::Released), Ok(None));
    }

    #[test]
    fn second_side_input_has_independent_edges_and_chord_releases_before_rearming() {
        let mut input = ready();
        input.sample(100, Sample::Key(Key::Aux16)).unwrap();
        assert_eq!(
            input.sample(130, Sample::Key(Key::Aux16)),
            Ok(Some(InputEvent::PttB(true)))
        );
        assert_eq!(
            input.sample(140, Sample::Key(Key::Ptt)),
            Ok(Some(InputEvent::PttB(false)))
        );
        assert_eq!(input.state(), InputState::Unstable);
        assert_eq!(input.sample(3000, Sample::Key(Key::Ptt)), Ok(None));
        input.sample(3100, Sample::Released).unwrap();
        assert_eq!(input.sample(3130, Sample::Released), Ok(None));
        input.sample(3200, Sample::Key(Key::Ptt)).unwrap();
        assert_eq!(
            input.sample(3230, Sample::Key(Key::Ptt)),
            Ok(Some(InputEvent::Ptt(true)))
        );
        assert_eq!(
            input.sample(3240, Sample::Invalid),
            Ok(Some(InputEvent::Ptt(false)))
        );
        input.sample(3300, Sample::Released).unwrap();
        input.sample(3330, Sample::Released).unwrap();
        input.sample(3400, Sample::Key(Key::Aux16)).unwrap();
        input.sample(3430, Sample::Key(Key::Aux16)).unwrap();
        input.sample(3500, Sample::Released).unwrap();
        assert_eq!(
            input.sample(3530, Sample::Released),
            Ok(Some(InputEvent::PttB(false)))
        );
    }
}
