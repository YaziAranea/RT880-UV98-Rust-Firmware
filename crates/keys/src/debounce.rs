const HALF_CLOCK: u32 = 1 << 31;

/// Explicit application policy. There is deliberately no default duration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    debounce_ms: u32,
    hold_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingError {
    ZeroDuration,
    DurationTooLong,
}

impl Timing {
    /// Both durations must be within 1..2^31 milliseconds.
    /// Hold starts at the confirmed press, not at the first raw transition.
    pub const fn new(debounce_ms: u32, hold_ms: u32) -> Result<Self, TimingError> {
        if debounce_ms == 0 || hold_ms == 0 {
            Err(TimingError::ZeroDuration)
        } else if debounce_ms >= HALF_CLOCK || hold_ms >= HALF_CLOCK {
            Err(TimingError::DurationTooLong)
        } else {
            Ok(Self {
                debounce_ms,
                hold_ms,
            })
        }
    }

    pub const fn debounce_ms(self) -> u32 {
        self.debounce_ms
    }

    pub const fn hold_ms(self) -> u32 {
        self.hold_ms
    }
}

/// Input events only; no event authorizes a radio transmission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonEvent {
    Pressed,
    Released,
    Held,
}

/// Backward or ambiguous clock movement. The debouncer remains unchanged.
/// Consecutive timestamps must advance by less than 2^31 milliseconds;
/// wrapping from u32::MAX to zero is supported within that bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockError;

/// Debounces one boolean input. Use separate instances for separate buttons.
///
/// Starts released. Press and release require a continuously sampled stable
/// candidate for `debounce_ms`. Emits one Held event per confirmed press and
/// always emits Released, even after Held. A pending raw release suppresses
/// Held until the input is observed pressed again. No autorepeat is inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Debouncer {
    timing: Timing,
    stable: bool,
    candidate: bool,
    candidate_since: Option<u32>,
    pressed_since: Option<u32>,
    held: bool,
    last_sample: Option<u32>,
}

impl Debouncer {
    pub const fn new(timing: Timing) -> Self {
        Self {
            timing,
            stable: false,
            candidate: false,
            candidate_since: None,
            pressed_since: None,
            held: false,
            last_sample: None,
        }
    }

    pub const fn is_pressed(&self) -> bool {
        self.stable
    }

    pub fn update(
        &mut self,
        now_ms: u32,
        raw_pressed: bool,
    ) -> Result<Option<ButtonEvent>, ClockError> {
        if let Some(previous) = self.last_sample {
            if now_ms.wrapping_sub(previous) >= HALF_CLOCK {
                return Err(ClockError);
            }
        }
        self.last_sample = Some(now_ms);

        if raw_pressed != self.candidate {
            self.candidate = raw_pressed;
            self.candidate_since = Some(now_ms);
        }
        if raw_pressed == self.stable {
            self.candidate_since = None;
        } else if let Some(since) = self.candidate_since {
            if now_ms.wrapping_sub(since) >= self.timing.debounce_ms {
                self.stable = raw_pressed;
                self.candidate_since = None;
                self.held = false;
                self.pressed_since = if raw_pressed { Some(now_ms) } else { None };
                return Ok(Some(if raw_pressed {
                    ButtonEvent::Pressed
                } else {
                    ButtonEvent::Released
                }));
            }
        }

        if self.stable && raw_pressed && !self.held {
            if let Some(since) = self.pressed_since {
                if now_ms.wrapping_sub(since) >= self.timing.hold_ms {
                    self.held = true;
                    return Ok(Some(ButtonEvent::Held));
                }
            }
        }
        Ok(None)
    }
}
