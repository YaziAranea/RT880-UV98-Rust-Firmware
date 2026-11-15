#![no_std]
#![forbid(unsafe_code)]
//! Deterministic receive-sweep protocol, not a radio driver or calibrated analyzer.
//!
//! Frequencies use the codeplug's integer 10 Hz units. Representable values are
//! not a promise that hardware can receive them. An adapter must independently
//! qualify the board, range, RX configuration, timing and signal measurement.
//! There are no defaults, TX actions, allocation, I/O or hardware acknowledgments
//! fabricated by time passing. See research/scanner/spec.txt.

pub mod compact;
pub mod window;

pub use window::{Window, WindowError};

/// Software/display capacity, not a supported RF bandwidth or frequency range.
pub const MAX_BINS: usize = 240;
const HALF_CLOCK: u32 = 1 << 31;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeError {
    ZeroFrequency,
    ZeroStep,
    Reversed,
    EndOffGrid,
    TooManyBins,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    start_10hz: u32,
    end_10hz: u32,
    step_10hz: u32,
    len: usize,
}
impl Range {
    /// Both endpoints are included and must lie on the same positive grid.
    /// One-bin ranges are valid. No actual receiver limits are assumed.
    pub const fn new(start_10hz: u32, end_10hz: u32, step_10hz: u32) -> Result<Self, RangeError> {
        if start_10hz == 0 || end_10hz == 0 {
            return Err(RangeError::ZeroFrequency);
        }
        if step_10hz == 0 {
            return Err(RangeError::ZeroStep);
        }
        if end_10hz < start_10hz {
            return Err(RangeError::Reversed);
        }
        let distance = end_10hz - start_10hz;
        if distance % step_10hz != 0 {
            return Err(RangeError::EndOffGrid);
        }
        // Compare before adding one; even the largest u32 distance is bounded.
        let intervals = distance / step_10hz;
        if intervals >= MAX_BINS as u32 {
            return Err(RangeError::TooManyBins);
        }
        Ok(Self {
            start_10hz,
            end_10hz,
            step_10hz,
            len: intervals as usize + 1,
        })
    }
    pub const fn start_10hz(self) -> u32 {
        self.start_10hz
    }
    pub const fn end_10hz(self) -> u32 {
        self.end_10hz
    }
    pub const fn step_10hz(self) -> u32 {
        self.step_10hz
    }
    pub const fn len(self) -> usize {
        self.len
    }
    pub const fn is_empty(self) -> bool {
        false
    }
    pub const fn frequency_10hz(self, index: usize) -> Option<u32> {
        if index >= self.len {
            return None;
        }
        // Validated grid: product <= end-start, so neither operation overflows.
        Some(self.start_10hz + index as u32 * self.step_10hz)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingError {
    ZeroTimeout,
    TooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    ready_timeout_ms: u32,
    settled_timeout_ms: u32,
    settle_ms: u32,
    sample_timeout_ms: u32,
}
impl Timing {
    /// Timeouts must be 1..2^31 ms. The minimum settle delay may be zero;
    /// even zero never substitutes for an explicit Settled acknowledgment.
    /// Settled timeout and minimum settle delay are independent: an early ACK
    /// can be followed by waiting for the rest of the minimum settle duration.
    pub const fn new(
        ready_timeout_ms: u32,
        settled_timeout_ms: u32,
        settle_ms: u32,
        sample_timeout_ms: u32,
    ) -> Result<Self, TimingError> {
        if ready_timeout_ms == 0 || settled_timeout_ms == 0 || sample_timeout_ms == 0 {
            return Err(TimingError::ZeroTimeout);
        }
        if ready_timeout_ms >= HALF_CLOCK
            || settled_timeout_ms >= HALF_CLOCK
            || settle_ms >= HALF_CLOCK
            || sample_timeout_ms >= HALF_CLOCK
        {
            return Err(TimingError::TooLong);
        }
        Ok(Self {
            ready_timeout_ms,
            settled_timeout_ms,
            settle_ms,
            sample_timeout_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Once,
    Continuous,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeakHold {
    Off,
    On,
}

/// Unique within one Engine's lifetime. Do not replace an Engine while an
/// adapter can still deliver its old commands/ACKs to the replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ticket(u64);
impl Ticket {
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Uninterpreted signal word: no dBm, S-unit, RF power or bandwidth conversion.
/// Numeric maximum is useful only if the adapter's code scale is monotonic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Measurement {
    pub raw: u16,
    pub sweep: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bin {
    pub latest: Option<Measurement>,
    pub peak: Option<u16>,
}
const EMPTY_BIN: Bin = Bin {
    latest: None,
    peak: None,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelReason {
    Stop,
    Ptt,
    Clock,
    ReadyTimeout,
    SettledTimeout,
    SampleTimeout,
    Hardware { code: u32 },
    CounterExhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Idle,
    AwaitReady,
    AwaitSettled,
    Settling,
    AwaitSample,
    SweepComplete,
    Stopped(CancelReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Start,
    Poll,
    Ready(Ticket),
    Settled(Ticket),
    Sample {
        ticket: Ticket,
        raw: u16,
    },
    Failed {
        ticket: Ticket,
        code: u32,
    },
    Stop,
    /// True represents pressed or held. Release never restarts the engine.
    Ptt(bool),
    ResetPeaks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    PrepareReceive {
        ticket: Ticket,
    },
    TuneReceive {
        ticket: Ticket,
        index: usize,
        frequency_10hz: u32,
    },
    ReadSignal {
        ticket: Ticket,
        index: usize,
    },
    /// Notification only. In Continuous mode a subsequent Poll starts the next
    /// sweep; in Once mode an explicit Start/Ready handshake is required.
    SweepComplete {
        sweep: u64,
    },
    /// Cancel pending receive work. Does not request TX, restore a frequency,
    /// claim physical RF-off or assume the adapter finished cancellation.
    Cancel {
        ticket: Option<Ticket>,
        reason: CancelReason,
    },
}

#[derive(Clone, Copy)]
enum Phase {
    Idle,
    Ready {
        ticket: Ticket,
        left: u32,
    },
    Settled {
        ticket: Ticket,
        index: usize,
        left: u32,
        settle_left: u32,
    },
    Settling {
        ticket: Ticket,
        index: usize,
        left: u32,
    },
    Sample {
        ticket: Ticket,
        index: usize,
        left: u32,
    },
    Complete,
    Stopped(CancelReason),
}

/// All state is local; callers execute and acknowledge Actions. Each call emits
/// at most one action. Stop/PTT take priority over clock checks and deadlines.
/// Old/wrong-phase ACKs are ignored; they do not extend a deadline or retry.
/// Active calls must be sampled less than 2^31 ms apart. u32 wrap is supported;
/// backward/ambiguous movement cancels. Timeouts fire at equality, so an ACK
/// arriving exactly at its deadline is late. No automatic retries occur.
pub struct Engine {
    range: Range,
    timing: Timing,
    mode: Mode,
    peak_hold: PeakHold,
    bins: [Bin; MAX_BINS],
    phase: Phase,
    counter: u64,
    sweep: u64,
    last_now: u32,
    ptt: bool,
}
impl Engine {
    pub const fn new(range: Range, timing: Timing, mode: Mode, peak_hold: PeakHold) -> Self {
        Self {
            range,
            timing,
            mode,
            peak_hold,
            bins: [EMPTY_BIN; MAX_BINS],
            phase: Phase::Idle,
            counter: 0,
            sweep: 0,
            last_now: 0,
            ptt: false,
        }
    }
    pub const fn range(&self) -> Range {
        self.range
    }
    pub fn bins(&self) -> &[Bin] {
        &self.bins[..self.range.len]
    }
    pub const fn sweep(&self) -> u64 {
        self.sweep
    }
    pub const fn ptt_held(&self) -> bool {
        self.ptt
    }
    pub const fn state(&self) -> State {
        match self.phase {
            Phase::Idle => State::Idle,
            Phase::Ready { .. } => State::AwaitReady,
            Phase::Settled { .. } => State::AwaitSettled,
            Phase::Settling { .. } => State::Settling,
            Phase::Sample { .. } => State::AwaitSample,
            Phase::Complete => State::SweepComplete,
            Phase::Stopped(reason) => State::Stopped(reason),
        }
    }
    pub const fn active_index(&self) -> Option<usize> {
        match self.phase {
            Phase::Settled { index, .. }
            | Phase::Settling { index, .. }
            | Phase::Sample { index, .. } => Some(index),
            _ => None,
        }
    }
    pub const fn is_running(&self) -> bool {
        match self.phase {
            Phase::Ready { .. }
            | Phase::Settled { .. }
            | Phase::Settling { .. }
            | Phase::Sample { .. } => true,
            Phase::Complete => matches!(self.mode, Mode::Continuous),
            _ => false,
        }
    }
    fn ticket(&self) -> Option<Ticket> {
        match self.phase {
            Phase::Ready { ticket, .. }
            | Phase::Settled { ticket, .. }
            | Phase::Settling { ticket, .. }
            | Phase::Sample { ticket, .. } => Some(ticket),
            _ => None,
        }
    }
    fn cancel(&mut self, reason: CancelReason) -> Action {
        let ticket = self.ticket();
        self.phase = Phase::Stopped(reason);
        Action::Cancel { ticket, reason }
    }
    fn next_ticket(&mut self) -> Option<Ticket> {
        self.counter = self.counter.checked_add(1)?;
        Some(Ticket(self.counter))
    }
    fn next_sweep(&mut self) -> bool {
        if let Some(next) = self.sweep.checked_add(1) {
            self.sweep = next;
            true
        } else {
            false
        }
    }
    fn tune(&mut self, index: usize) -> Action {
        let Some(ticket) = self.next_ticket() else {
            return self.cancel(CancelReason::CounterExhausted);
        };
        self.phase = Phase::Settled {
            ticket,
            index,
            left: self.timing.settled_timeout_ms,
            settle_left: self.timing.settle_ms,
        };
        Action::TuneReceive {
            ticket,
            index,
            frequency_10hz: self
                .range
                .frequency_10hz(index)
                .expect("bounded sweep index"),
        }
    }
    fn read_signal(&mut self, index: usize) -> Action {
        let Some(ticket) = self.next_ticket() else {
            return self.cancel(CancelReason::CounterExhausted);
        };
        self.phase = Phase::Sample {
            ticket,
            index,
            left: self.timing.sample_timeout_ms,
        };
        Action::ReadSignal { ticket, index }
    }
    fn elapsed(&mut self, now_ms: u32) -> Result<(), CancelReason> {
        let elapsed = now_ms.wrapping_sub(self.last_now);
        if elapsed >= HALF_CLOCK {
            return Err(CancelReason::Clock);
        }
        self.last_now = now_ms;
        match &mut self.phase {
            Phase::Ready { left, .. }
            | Phase::Settling { left, .. }
            | Phase::Sample { left, .. } => *left = left.saturating_sub(elapsed),
            Phase::Settled {
                left, settle_left, ..
            } => {
                *left = left.saturating_sub(elapsed);
                *settle_left = settle_left.saturating_sub(elapsed);
            }
            _ => {}
        }
        Ok(())
    }
    fn deadline(&self) -> Option<CancelReason> {
        match self.phase {
            Phase::Ready { left: 0, .. } => Some(CancelReason::ReadyTimeout),
            Phase::Settled { left: 0, .. } => Some(CancelReason::SettledTimeout),
            Phase::Sample { left: 0, .. } => Some(CancelReason::SampleTimeout),
            _ => None,
        }
    }
    /// A new Start rebases the caller clock, but never reuses tickets or resets
    /// peak history. Use ResetPeaks explicitly. Bin.latest remains tagged with
    /// its acquisition sweep: old/partial values must not be shown as fresh.
    pub fn handle(&mut self, now_ms: u32, event: Event) -> Option<Action> {
        // These events must cancel even with a stale/backward clock.
        match event {
            Event::Ptt(true) => {
                self.ptt = true;
                return if self.is_running() {
                    Some(self.cancel(CancelReason::Ptt))
                } else {
                    None
                };
            }
            Event::Stop => {
                return if self.is_running() {
                    Some(self.cancel(CancelReason::Stop))
                } else {
                    None
                };
            }
            Event::Ptt(false) => {
                self.ptt = false;
            }
            Event::ResetPeaks => {
                for bin in &mut self.bins {
                    bin.peak = None;
                }
            }
            _ => {}
        }
        if matches!(event, Event::Start) && !self.is_running() {
            if self.ptt {
                return Some(self.cancel(CancelReason::Ptt));
            }
            self.last_now = now_ms;
            if !self.next_sweep() {
                return Some(self.cancel(CancelReason::CounterExhausted));
            }
            let Some(ticket) = self.next_ticket() else {
                return Some(self.cancel(CancelReason::CounterExhausted));
            };
            self.phase = Phase::Ready {
                ticket,
                left: self.timing.ready_timeout_ms,
            };
            return Some(Action::PrepareReceive { ticket });
        }
        if !self.is_running() {
            return None;
        }
        if let Err(reason) = self.elapsed(now_ms) {
            return Some(self.cancel(reason));
        }
        if let Event::Failed { ticket, code } = event {
            if self.ticket() == Some(ticket) {
                return Some(self.cancel(CancelReason::Hardware { code }));
            }
        }
        if let Some(reason) = self.deadline() {
            return Some(self.cancel(reason));
        }
        match (self.phase, event) {
            (Phase::Ready { ticket, .. }, Event::Ready(ack)) if ticket == ack => Some(self.tune(0)),
            (
                Phase::Settled {
                    ticket,
                    index,
                    settle_left,
                    ..
                },
                Event::Settled(ack),
            ) if ticket == ack => {
                if settle_left == 0 {
                    Some(self.read_signal(index))
                } else {
                    self.phase = Phase::Settling {
                        ticket,
                        index,
                        left: settle_left,
                    };
                    None
                }
            }
            (Phase::Settling { index, left: 0, .. }, Event::Poll) => Some(self.read_signal(index)),
            (Phase::Sample { ticket, index, .. }, Event::Sample { ticket: ack, raw })
                if ticket == ack =>
            {
                let bin = &mut self.bins[index];
                bin.latest = Some(Measurement {
                    raw,
                    sweep: self.sweep,
                });
                if self.peak_hold == PeakHold::On {
                    bin.peak = Some(bin.peak.map_or(raw, |previous| previous.max(raw)));
                }
                if index + 1 == self.range.len {
                    self.phase = Phase::Complete;
                    Some(Action::SweepComplete { sweep: self.sweep })
                } else {
                    Some(self.tune(index + 1))
                }
            }
            (Phase::Complete, Event::Poll) if self.mode == Mode::Continuous => {
                if !self.next_sweep() {
                    Some(self.cancel(CancelReason::CounterExhausted))
                } else {
                    Some(self.tune(0))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod internal_tests {
    use super::*;
    #[test]
    fn exhausted_tickets_cancel_without_reuse() {
        let mut engine = Engine::new(
            Range::new(1, 1, 1).unwrap(),
            Timing::new(10, 10, 0, 10).unwrap(),
            Mode::Once,
            PeakHold::Off,
        );
        engine.counter = u64::MAX - 1;
        let Some(Action::PrepareReceive { ticket }) = engine.handle(0, Event::Start) else {
            panic!("prepare")
        };
        assert_eq!(ticket.value(), u64::MAX);
        assert_eq!(
            engine.handle(1, Event::Ready(ticket)),
            Some(Action::Cancel {
                ticket: Some(ticket),
                reason: CancelReason::CounterExhausted
            })
        );
        assert_eq!(
            engine.handle(2, Event::Start),
            Some(Action::Cancel {
                ticket: None,
                reason: CancelReason::CounterExhausted
            })
        );
    }
    #[test]
    fn exhausted_sweep_counter_never_labels_old_samples_as_new() {
        let mut engine = Engine::new(
            Range::new(1, 1, 1).unwrap(),
            Timing::new(10, 10, 0, 10).unwrap(),
            Mode::Once,
            PeakHold::Off,
        );
        engine.sweep = u64::MAX;
        assert!(matches!(
            engine.handle(0, Event::Start),
            Some(Action::Cancel {
                reason: CancelReason::CounterExhausted,
                ..
            })
        ));
        assert_eq!(engine.sweep(), u64::MAX);
    }
}
