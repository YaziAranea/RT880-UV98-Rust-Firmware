//! Hardware-independent hold qualification and PCM limits for the keyer.

pub const MAX_MS: u32 = 1_500;
pub const PERIOD_CYCLES: u32 = 6_000; // nominal 8 kHz at 48 MHz
const MAX_SAMPLES: u16 = 12_000;
const MAX_CYCLES: u32 = (MAX_SAMPLES as u32) * PERIOD_CYCLES;
const CYCLES_PER_MS: u32 = 48_000; // confirmed run clock, also used by the 8 kHz DAC
const LEASE_CYCLES: u32 = 20 * CYCLES_PER_MS;
const START_MARGIN_MS: u32 = 25;
const MAX_GAP: u32 = PERIOD_CYCLES * 3 / 2;

pub const fn enough_time(remaining_tx_ms: u32) -> bool {
    remaining_tx_ms > MAX_MS + START_MARGIN_MS
}

pub fn lead_in_ready(
    confirmed_cycle: Option<u32>,
    delay_ms: u16,
    now_cycle: u32,
    held: bool,
    voice_path_ready: bool,
) -> bool {
    held && voice_path_ready
        && confirmed_cycle.is_some_and(|confirmed| {
            let elapsed = now_cycle.wrapping_sub(confirmed);
            elapsed < (1 << 31) && elapsed >= u32::from(delay_ms) * CYCLES_PER_MS
        })
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Hold {
    #[default]
    WaitRelease,
    Releasing(u32),
    Ready,
    Candidate(u8),
}

impl Hold {
    /// Qualify only from clean release on an eligible home screen. Starting
    /// in a blocked context, a chord or uncertainty invalidates this hold.
    pub fn observe(&mut self, now: u32, configured: Option<u8>, digit: Option<u8>, released: bool) {
        *self = match (configured, digit, released, *self) {
            (Some(_), _, true, Self::Ready) => Self::Ready,
            (Some(_), _, true, Self::Releasing(at)) if now.wrapping_sub(at) >= 12 => Self::Ready,
            (Some(_), _, true, Self::Releasing(at)) => Self::Releasing(at),
            (Some(_), _, true, _) => Self::Releasing(now),
            (Some(wanted), Some(held), false, Self::Ready | Self::Candidate(_))
                if wanted == held && (*self == Self::Ready || *self == Self::Candidate(held)) =>
            {
                Self::Candidate(held)
            }
            _ => Self::WaitRelease,
        };
    }

    /// Use only with the UI's debounced long-press event. It remains consumed
    /// through tuning, EOF, failed authorization and a sustained hold.
    pub fn take(&mut self, digit: u8) -> bool {
        core::mem::take(self) == Self::Candidate(digit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample {
    Value(u8),
    End,
    Interrupted,
}

#[derive(Default)]
pub struct ReplyHold(Option<u8>);

impl ReplyHold {
    pub const fn new(completed_held: Option<u8>) -> Self {
        Self(completed_held)
    }

    /// A continuous hold from the completed call may be ignored only during
    /// the fast RX window. Release, another key or uncertainty ends it.
    pub fn observe(&mut self, digit: Option<u8>) -> bool {
        self.0 = self.0.filter(|&previous| digit == Some(previous));
        self.0.is_some()
    }
}

pub struct Stream<S> {
    samples: S,
    began: u32,
    lease: u32,
    previous: u32,
    remaining: u16,
}

impl<S: Iterator<Item = u8>> Stream<S> {
    pub fn begin(mut samples: S, now: u32) -> Option<(Self, u8)> {
        let first = samples.next()?;
        Some((
            Self {
                samples,
                began: now,
                lease: now,
                previous: now,
                remaining: MAX_SAMPLES - 1,
            },
            first,
        ))
    }

    fn valid(&self, now: u32) -> bool {
        now.wrapping_sub(self.lease) < LEASE_CYCLES && now.wrapping_sub(self.began) < MAX_CYCLES
    }

    /// Only the freshly authorized foreground can renew a live stream.
    pub fn renew(&mut self, now: u32) -> bool {
        // At a maximum-length clip the foreground may see the time boundary
        // first. Keep the lease through the nominal EOF IRQ, without
        // allowing another sample or retaining an overdue final sample.
        let awaiting_end = self.remaining == 0
            && now.wrapping_sub(self.lease) < LEASE_CYCLES
            && now.wrapping_sub(self.previous) < MAX_GAP;
        if !self.valid(now) && !awaiting_end {
            return false;
        }
        self.lease = now;
        true
    }

    /// Do not stretch a clip after a missed timer interrupt.
    pub fn sample(&mut self, now: u32) -> Sample {
        if now.wrapping_sub(self.lease) >= LEASE_CYCLES
            || now.wrapping_sub(self.previous) >= MAX_GAP
        {
            return Sample::Interrupted;
        }
        if self.remaining == 0 {
            return Sample::End;
        }
        if !self.valid(now) {
            return Sample::Interrupted;
        }
        self.previous = now;
        self.remaining -= 1;
        self.samples.next().map_or(Sample::End, Sample::Value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_in_starts_only_after_confirmed_tx_at_zero_one_and_max_delay() {
        let confirmed = CYCLES_PER_MS - 1;
        assert!(!lead_in_ready(None, 0, confirmed, true, true));
        assert!(lead_in_ready(Some(confirmed), 0, confirmed, true, true));
        // Crossing a millisecond boundary by one cycle is not a full millisecond.
        assert!(!lead_in_ready(Some(confirmed), 1, confirmed + 1, true, true));
        for delay in [1u16, 437, 3_000] {
            let due = confirmed + u32::from(delay) * CYCLES_PER_MS;
            assert!(!lead_in_ready(Some(confirmed), delay, due - 1, true, true));
            assert!(lead_in_ready(Some(confirmed), delay, due, true, true));
        }
    }

    #[test]
    fn lead_in_waits_for_bot_and_cannot_start_after_release_or_at_invalid_time() {
        let confirmed = u32::MAX - 10;
        let due = confirmed.wrapping_add(3_000 * CYCLES_PER_MS);
        let ready =
            |at, held, bot_ready| lead_in_ready(Some(confirmed), 3_000, at, held, bot_ready);
        assert!(!ready(confirmed - 1, true, true));
        assert!(!ready(due - 1, true, true));
        assert!(!ready(due, false, true));
        assert!(!ready(due, true, false));
        assert!(ready(due + 100, true, true));
        assert!(!ready(due + 100, false, true));
        assert!(!lead_in_ready(None, 3_000, due + 100, true, true));
    }

    #[test]
    fn boot_hold_and_repeated_event_require_new_clean_release() {
        let mut hold = Hold::default();
        hold.observe(0, Some(5), Some(5), false);
        assert!(!hold.take(5));
        hold.observe(10, Some(5), None, true);
        hold.observe(22, Some(5), None, true);
        hold.observe(23, Some(5), Some(5), false);
        assert!(hold.take(5));
        hold.observe(24, Some(5), Some(5), false);
        assert!(!hold.take(5));
        hold.observe(100, Some(5), None, true);
        hold.observe(111, Some(5), None, true);
        hold.observe(112, Some(5), Some(5), false);
        assert!(!hold.take(5));
        hold.observe(200, Some(5), None, true);
        hold.observe(212, Some(5), None, true);
        hold.observe(213, Some(5), Some(5), false);
        assert!(hold.take(5));
    }

    #[test]
    fn uncertainty_other_key_and_blocked_context_disable_the_current_hold() {
        let mut hold = Hold::default();
        for (index, (configured, key)) in [(Some(5), None), (Some(5), Some(6)), (None, Some(5))]
            .into_iter()
            .enumerate()
        {
            let now = index as u32 * 20;
            hold.observe(now, Some(5), None, true);
            hold.observe(now + 12, Some(5), None, true);
            hold.observe(now + 13, Some(5), Some(5), false);
            hold.observe(now + 14, configured, key, false);
            hold.observe(now + 15, Some(5), Some(5), false);
            assert!(!hold.take(5));
        }
        assert!(!enough_time(MAX_MS + START_MARGIN_MS));
        assert!(enough_time(MAX_MS + START_MARGIN_MS + 1));
    }

    #[test]
    fn fast_reply_ignores_only_the_same_uninterrupted_completed_hold() {
        for interrupt in [None, Some(6)] {
            let mut reply = ReplyHold::new(Some(5));
            assert!(reply.observe(Some(5)));
            assert!(!reply.observe(interrupt));
            assert!(!reply.observe(Some(5)));
        }
        assert!(!ReplyHold::default().observe(Some(5)));
    }

    #[test]
    fn sample_limit_expired_lease_and_stalled_irq_cannot_emit_more_audio() {
        let start = u32::MAX - 10;
        let (mut finite, first) = Stream::begin([120, 130].into_iter(), start).unwrap();
        assert_eq!(first, 120);
        assert_eq!(
            finite.sample(start.wrapping_add(PERIOD_CYCLES)),
            Sample::Value(130)
        );
        assert_eq!(
            finite.sample(start.wrapping_add(2 * PERIOD_CYCLES)),
            Sample::End
        );

        let (mut stale, _) = Stream::begin(core::iter::repeat(128), start).unwrap();
        assert!(!stale.renew(start.wrapping_add(LEASE_CYCLES)));
        assert_eq!(
            stale.sample(start.wrapping_add(LEASE_CYCLES)),
            Sample::Interrupted
        );

        let (mut stalled, _) = Stream::begin(core::iter::repeat(128), start).unwrap();
        assert_eq!(
            stalled.sample(start.wrapping_add(PERIOD_CYCLES * 2)),
            Sample::Interrupted
        );

        let (mut full, _) = Stream::begin(core::iter::repeat(128), start).unwrap();
        for n in 1..u32::from(MAX_SAMPLES) {
            let now = start.wrapping_add(n * PERIOD_CYCLES);
            assert!(full.renew(now));
            assert_eq!(full.sample(now), Sample::Value(128));
        }
        let end = start.wrapping_add(MAX_CYCLES);
        assert!(full.renew(end));
        assert_eq!(full.sample(end), Sample::End);
        assert!(!full.renew(end.wrapping_add(MAX_GAP)));
    }
}
