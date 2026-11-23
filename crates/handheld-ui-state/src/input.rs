//! Debounced foreground input. Invalid scans cancel pending gestures and require
//! a clean release. No event carries an RF or storage operation.
use uv98_keys::{ClockError, Key, Timing};

pub const REPEAT_MS: u32 = 180;
const PTT_DEBOUNCE_MS: u32 = 12;
const PTT_CONFIRMATION_SAMPLES: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample {
    Released,
    Key(Key),
    Invalid,
    UncertainMatrix,
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
    samples: u8,
    accepted: bool,
    held: bool,
    repeat_at: u32,
}

impl Press {
    const fn new(key: Key, now_ms: u32) -> Self {
        Self {
            key,
            since: now_ms,
            samples: 1,
            accepted: false,
            held: false,
            repeat_at: now_ms,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputTracker {
    timing: Timing,
    state: InputState,
    last_ms: Option<u32>,
    release_since: Option<u32>,
    press: Option<Press>,
    aux16_ptt: bool,
    fast_matrix: bool,
    fast_ptt: bool,
    uncertain_since: Option<u32>,
}

impl InputTracker {
    pub const fn new(timing: Timing) -> Self {
        Self {
            timing,
            state: InputState::WaitingForRelease,
            last_ms: None,
            release_since: None,
            press: None,
            aux16_ptt: true,
            fast_matrix: false,
            fast_ptt: false,
            uncertain_since: None,
        }
    }

    pub fn set_aux16_ptt(&mut self, mapped: bool) {
        self.aux16_ptt = mapped;
    }

    pub fn enable_fast_matrix(&mut self) {
        self.fast_matrix = true;
    }

    /// Use only when PTT input comes from fresh isolated physical samples.
    pub fn enable_fast_ptt(&mut self) {
        self.fast_ptt = true;
    }

    fn fast_ptt_key(&self, key: Key) -> bool {
        self.fast_ptt && side_event(key, true, self.aux16_ptt).is_some()
    }

    fn key_debounce(&self, key: Key, release: bool) -> u32 {
        if self.fast_ptt_key(key) {
            PTT_DEBOUNCE_MS
        } else if self.fast_matrix && is_matrix(key) {
            if release {
                8
            } else {
                12
            }
        } else if self.fast_matrix
            && matches!(key, Key::Aux16 | Key::Aux17 | Key::Aux18)
            && side_event(key, true, self.aux16_ptt).is_none()
        {
            12
        } else {
            self.timing.debounce_ms()
        }
    }

    pub const fn state(&self) -> InputState {
        self.state
    }

    pub const fn is_ready(&self) -> bool {
        matches!(self.state, InputState::Ready)
    }

    /// Includes press and release debounce. Hold or invalidation ends the
    /// pending short gesture, even if the physical key remains down.
    pub fn short_star_pending(&self) -> bool {
        self.press
            .is_some_and(|press| press.key == Key::Star && !press.held)
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
        if sample == Sample::UncertainMatrix {
            let since = *self.uncertain_since.get_or_insert(now_ms);
            // A transition between the forward and reverse matrix scans is
            // normal during a press or release. Never defer a PTT release.
            if self.fast_matrix
                && self.press.is_none_or(|press| is_matrix(press.key))
                && now_ms.wrapping_sub(since) < 8
            {
                return Ok(None);
            }
        } else {
            self.uncertain_since = None;
        }
        match sample {
            Sample::Invalid | Sample::UncertainMatrix => Ok(self.invalidate()),
            Sample::Released => {
                // Do not combine separate short taps into one qualified press.
                if self
                    .press
                    .is_some_and(|press| !press.accepted && self.fast_ptt_key(press.key))
                {
                    self.press = None;
                }
                let Some(since) = self.release_since else {
                    self.release_since = Some(now_ms);
                    return Ok(None);
                };
                let debounce = self.press.map_or(self.timing.debounce_ms(), |press| {
                    self.key_debounce(press.key, true)
                });
                if now_ms.wrapping_sub(since) < debounce {
                    return Ok(None);
                }
                self.state = InputState::Ready;
                let finished = self.press.take();
                Ok(finished.and_then(|press| {
                    if !press.accepted {
                        None
                    } else if let Some(event) = side_event(press.key, false, self.aux16_ptt) {
                        Some(event)
                    } else if delayed_short(press.key) && !press.held {
                        Some(InputEvent::Short(press.key))
                    } else {
                        None
                    }
                }))
            }
            Sample::Key(key) => {
                let released_at = self.release_since.take();
                if self.state != InputState::Ready {
                    return Ok(None);
                }
                let Some(mut press) = self.press else {
                    self.press = Some(Press::new(key, now_ms));
                    return Ok(None);
                };
                if press.key != key {
                    if self.fast_matrix
                        && is_matrix(press.key)
                        && is_matrix(key)
                        && released_at.is_some_and(|since| now_ms.wrapping_sub(since) >= 4)
                    {
                        self.press = Some(Press::new(key, now_ms));
                        return Ok((press.accepted && delayed_short(press.key) && !press.held)
                            .then_some(InputEvent::Short(press.key)));
                    }
                    return Ok(self.invalidate());
                }
                if !press.accepted {
                    if self.fast_ptt_key(key) && press.samples < PTT_CONFIRMATION_SAMPLES {
                        press.samples += 1;
                        self.press = Some(press);
                        if press.samples < PTT_CONFIRMATION_SAMPLES {
                            return Ok(None);
                        }
                    }
                    if now_ms.wrapping_sub(press.since) < self.key_debounce(key, false) {
                        return Ok(None);
                    }
                    press.accepted = true;
                    // Hold starts at the confirmed press, matching Timing.
                    press.since = now_ms;
                    press.repeat_at = now_ms;
                    self.press = Some(press);
                    return Ok(if let Some(event) = side_event(key, true, self.aux16_ptt) {
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
            .and_then(|press| side_event(press.key, false, self.aux16_ptt));
        self.press = None;
        self.release_since = None;
        self.state = InputState::Unstable;
        release
    }
}

/// Confirm an ADC-only key before letting it participate in UI chord logic.
/// The RF PTT guard must continue to use the unfiltered physical sample.
#[derive(Default)]
pub struct AuxFilter {
    candidate: Option<Key>,
    stable: Option<Key>,
    since: u32,
}

impl AuxFilter {
    pub fn sample(&mut self, now: u32, key: Option<Key>) -> Option<Key> {
        if key != self.candidate {
            self.candidate = key;
            self.since = now;
        }
        if now.wrapping_sub(self.since) >= 10 {
            self.stable = key;
        }
        self.stable
    }
}

const fn is_matrix(key: Key) -> bool {
    !matches!(key, Key::Ptt | Key::Aux16 | Key::Aux17 | Key::Aux18)
}

const fn side_event(key: Key, down: bool, aux16_ptt: bool) -> Option<InputEvent> {
    match key {
        Key::Ptt => Some(InputEvent::Ptt(down)),
        Key::Aux16 if aux16_ptt => Some(InputEvent::PttB(down)),
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
    fn pending_star_ends_on_hold_or_invalid_input() {
        let mut input = ready();
        let star = Sample::Key(Key::Star);
        input.sample(100, star).unwrap();
        assert!(input.short_star_pending());
        input.sample(130, star).unwrap();
        assert_eq!(
            input.sample(730, star),
            Ok(Some(InputEvent::Hold(Key::Star)))
        );
        assert!(!input.short_star_pending());
        input.sample(740, Sample::Released).unwrap();
        assert_eq!(input.sample(770, Sample::Released), Ok(None));
        input.sample(800, star).unwrap();
        assert!(input.short_star_pending());
        assert_eq!(input.sample(801, Sample::Invalid), Ok(None));
        assert!(!input.short_star_pending());
        input.sample(1000, star).unwrap();
        assert!(!input.short_star_pending());
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
    fn short_ptt_and_repress_after_a_short_clean_release() {
        for key in [Key::Ptt, Key::Aux16] {
            let mut input = ready();
            input.enable_fast_ptt();
            let pressed = Sample::Key(key);
            let down = side_event(key, true, true);
            let up = side_event(key, false, true);
            assert_eq!(input.sample(100, pressed), Ok(None));
            assert_eq!(input.sample(108, pressed), Ok(None));
            assert_eq!(input.sample(111, pressed), Ok(None));
            assert_eq!(input.sample(112, pressed), Ok(down));
            assert_eq!(input.sample(140, Sample::Released), Ok(None));
            assert_eq!(input.sample(151, Sample::Released), Ok(None));
            assert_eq!(input.sample(152, Sample::Released), Ok(up));
            assert_eq!(input.sample(155, pressed), Ok(None));
            assert_eq!(input.sample(167, pressed), Ok(None));
            assert_eq!(input.sample(168, pressed), Ok(down));
        }
    }

    #[test]
    fn short_ptt_does_not_combine_pulses_across_a_release() {
        let mut input = ready();
        input.enable_fast_ptt();
        let pressed = Sample::Key(Key::Ptt);
        assert_eq!(input.sample(100, pressed), Ok(None));
        assert_eq!(input.sample(107, pressed), Ok(None));
        assert_eq!(input.sample(108, Sample::Released), Ok(None));
        assert_eq!(input.sample(112, pressed), Ok(None));
        assert_eq!(input.sample(116, pressed), Ok(None));
        assert_eq!(input.sample(123, pressed), Ok(None));
        assert_eq!(input.sample(124, pressed), Ok(Some(InputEvent::Ptt(true))));
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

#[cfg(test)]
mod fast_input_tests {
    use super::*;

    #[test]
    fn rapid_digits_survive_short_press_and_release_gaps() {
        let mut input = InputTracker::new(Timing::new(30, 600).unwrap());
        input.enable_fast_matrix();
        input.sample(0, Sample::Released).unwrap();
        input.sample(30, Sample::Released).unwrap();
        for (start, key) in [(40, Key::Digit1), (68, Key::Digit2), (96, Key::Digit6)] {
            assert_eq!(input.sample(start, Sample::Key(key)).unwrap(), None);
            assert_eq!(input.sample(start + 12, Sample::Key(key)).unwrap(), None);
            assert_eq!(input.sample(start + 15, Sample::Released).unwrap(), None);
            assert_eq!(
                input.sample(start + 23, Sample::Released).unwrap(),
                Some(InputEvent::Short(key))
            );
        }
    }

    #[test]
    fn matrix_transition_and_quick_next_digit_preserve_both_short_events() {
        let mut input = InputTracker::new(Timing::new(30, 600).unwrap());
        input.enable_fast_matrix();
        input.sample(0, Sample::Released).unwrap();
        input.sample(30, Sample::Released).unwrap();
        let first = Sample::Key(Key::Digit1);
        let second = Sample::Key(Key::Digit2);
        input.sample(40, first).unwrap();
        input.sample(52, first).unwrap();
        input.sample(54, Sample::UncertainMatrix).unwrap();
        input.sample(55, Sample::Released).unwrap();
        assert_eq!(
            input.sample(60, second).unwrap(),
            Some(InputEvent::Short(Key::Digit1))
        );
        input.sample(72, second).unwrap();
        input.sample(76, Sample::Released).unwrap();
        assert_eq!(
            input.sample(84, Sample::Released).unwrap(),
            Some(InputEvent::Short(Key::Digit2))
        );
    }

    #[test]
    fn adc_glitch_does_not_enter_ui_chord_detection() {
        let mut filter = AuxFilter::default();
        assert_eq!(filter.sample(40, Some(Key::Aux17)), None);
        assert_eq!(filter.sample(45, None), None);
        assert_eq!(filter.sample(50, Some(Key::Aux17)), None);
        assert_eq!(filter.sample(70, Some(Key::Aux17)), Some(Key::Aux17));
        assert_eq!(filter.sample(71, None), Some(Key::Aux17));
        assert_eq!(filter.sample(81, None), None);
    }
}
