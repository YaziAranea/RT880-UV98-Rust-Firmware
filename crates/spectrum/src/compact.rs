//! Compact 81-point receive sweep with acknowledged restoration. This module
//! never accesses hardware or assumes a supported frequency, gain or route.
//! The adapter must qualify every point before accepting Prepare and must
//! restore the complete normal receive configuration, not just its frequency.

use crate::{Range, Ticket, Timing, HALF_CLOCK};

pub const BINS: usize = 81;
pub const BITMAP_BYTES: usize = BINS.div_ceil(8);
pub const MAX_RAW: u16 = 511;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    TooManyBins,
    ZeroRestoreFrequency,
    InvalidRestoreTimeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub range: Range,
    pub restore_frequency_10hz: u32,
    pub timing: Timing,
    pub restore_timeout_ms: u32,
}

impl Config {
    pub const fn new(
        range: Range,
        restore_frequency_10hz: u32,
        timing: Timing,
        restore_timeout_ms: u32,
    ) -> Result<Self, ConfigError> {
        if range.len() > BINS {
            return Err(ConfigError::TooManyBins);
        }
        if restore_frequency_10hz == 0 {
            return Err(ConfigError::ZeroRestoreFrequency);
        }
        if restore_timeout_ms == 0 || restore_timeout_ms >= HALF_CLOCK {
            return Err(ConfigError::InvalidRestoreTimeout);
        }
        Ok(Self {
            range,
            restore_frequency_10hz,
            timing,
            restore_timeout_ms,
        })
    }

    fn valid(self) -> bool {
        Self::new(
            self.range,
            self.restore_frequency_10hz,
            self.timing,
            self.restore_timeout_ms,
        )
        .is_ok()
    }
}

/// Stable storage: unknown points have a clear valid bit, never a synthetic
/// zero reading. Fresh distinguishes this sweep from the previous sweep.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Plot {
    pub latest: [u16; BINS],
    pub peaks: [u16; BINS],
    pub valid: [u8; BITMAP_BYTES],
    pub fresh: [u8; BITMAP_BYTES],
    pub peak_valid: [u8; BITMAP_BYTES],
    pub sweep: u32,
}

impl Plot {
    pub const EMPTY: Self = Self {
        latest: [0; BINS],
        peaks: [0; BINS],
        valid: [0; BITMAP_BYTES],
        fresh: [0; BITMAP_BYTES],
        peak_valid: [0; BITMAP_BYTES],
        sweep: 0,
    };
    pub fn valid(&self, index: usize) -> bool {
        bit(&self.valid, index)
    }
    pub fn fresh(&self, index: usize) -> bool {
        bit(&self.fresh, index)
    }
    pub fn peak_valid(&self, index: usize) -> bool {
        bit(&self.peak_valid, index)
    }
}

pub fn bit(bitmap: &[u8; BITMAP_BYTES], index: usize) -> bool {
    index < BINS && bitmap[index / 8] & (1 << (index % 8)) != 0
}

fn set_bit(bitmap: &mut [u8; BITMAP_BYTES], index: usize) {
    bitmap[index / 8] |= 1 << (index % 8);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Exit,
    Ptt,
    Accepted { index: u8 },
    PrepareTimeout,
    TuneTimeout,
    SampleTimeout,
    RestoreTimeout,
    Hardware { code: u32 },
    InvalidSample,
    Clock,
    CounterExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Idle,
    Preparing,
    Tuning,
    Settling,
    Reading,
    SweepComplete,
    Restoring(StopReason),
    Restored(StopReason),
    /// The adapter must enter its terminal safe-off path. A failed restoration
    /// is never reported as a working receiver and cannot restart this engine.
    Failed(StopReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Poll,
    Prepared(Ticket),
    Tuned(Ticket),
    Sample { ticket: Ticket, raw: u16 },
    Restored(Ticket),
    Failed { ticket: Ticket, code: u32 },
    Stop,
    Ptt(bool),
    Accept { index: u8 },
    ClearPeaks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Prepare {
        ticket: Ticket,
    },
    Tune {
        ticket: Ticket,
        index: u8,
        frequency_10hz: u32,
    },
    Read {
        ticket: Ticket,
        index: u8,
    },
    Restore {
        ticket: Ticket,
        frequency_10hz: u32,
        reason: StopReason,
    },
    SweepComplete {
        sweep: u32,
    },
    Terminal {
        reason: StopReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    Busy,
    PttHeld,
    Terminal,
    InvalidConfig,
    CounterExhausted,
}

/// Each transition emits at most one command. Tickets never repeat across
/// starts. A caller must retain this engine until all old adapter work is gone.
/// Elapsed time cannot acknowledge preparation, tuning, sampling or restore.
pub struct Engine {
    config: Option<Config>,
    plot: Plot,
    state: State,
    ticket: Option<Ticket>,
    next_ticket: u64,
    index: u8,
    remaining: u32,
    settle_remaining: u32,
    last_now: u32,
    ptt: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub const fn new() -> Self {
        Self {
            config: None,
            plot: Plot::EMPTY,
            state: State::Idle,
            ticket: None,
            next_ticket: 0,
            index: 0,
            remaining: 0,
            settle_remaining: 0,
            last_now: 0,
            ptt: false,
        }
    }
    pub const fn state(&self) -> State {
        self.state
    }
    pub const fn config(&self) -> Option<Config> {
        self.config
    }
    pub const fn plot(&self) -> &Plot {
        &self.plot
    }
    pub const fn active_index(&self) -> Option<u8> {
        match self.state {
            State::Tuning | State::Settling | State::Reading => Some(self.index),
            _ => None,
        }
    }
    pub const fn ptt_held(&self) -> bool {
        self.ptt
    }
    pub const fn running(&self) -> bool {
        matches!(
            self.state,
            State::Preparing
                | State::Tuning
                | State::Settling
                | State::Reading
                | State::SweepComplete
        )
    }
    pub const fn restoring(&self) -> bool {
        matches!(self.state, State::Restoring(_))
    }

    pub fn start(&mut self, now_ms: u32, config: Config) -> Result<Action, StartError> {
        if self.running() || self.restoring() {
            return Err(StartError::Busy);
        }
        if matches!(self.state, State::Failed(_)) {
            return Err(StartError::Terminal);
        }
        if self.ptt {
            return Err(StartError::PttHeld);
        }
        if !config.valid() {
            return Err(StartError::InvalidConfig);
        }
        // Always reserve one ticket for restoration before emitting RF work.
        if self.next_ticket >= u64::MAX - 1 {
            return Err(StartError::CounterExhausted);
        }
        let ticket = self.allocate().ok_or(StartError::CounterExhausted)?;
        self.config = Some(config);
        self.plot = Plot::EMPTY;
        self.plot.sweep = 1;
        self.state = State::Preparing;
        self.ticket = Some(ticket);
        self.remaining = config.timing.ready_timeout_ms;
        self.last_now = now_ms;
        Ok(Action::Prepare { ticket })
    }

    fn allocate(&mut self) -> Option<Ticket> {
        self.next_ticket = self.next_ticket.checked_add(1)?;
        Some(Ticket(self.next_ticket))
    }

    fn terminal(&mut self, reason: StopReason) -> Action {
        self.state = State::Failed(reason);
        self.ticket = None;
        Action::Terminal { reason }
    }

    fn restore(&mut self, now_ms: u32, reason: StopReason, selected: Option<u8>) -> Action {
        let Some(config) = self.config else {
            return self.terminal(reason);
        };
        let frequency_10hz = selected
            .and_then(|index| config.range.frequency_10hz(usize::from(index)))
            .unwrap_or(config.restore_frequency_10hz);
        let Some(ticket) = self.allocate() else {
            return self.terminal(StopReason::CounterExhausted);
        };
        self.state = State::Restoring(reason);
        self.ticket = Some(ticket);
        self.remaining = config.restore_timeout_ms;
        // Cancellation still works with a stale/backward clock. Give restoration
        // its own bounded deadline from this request; no RF work continues.
        self.last_now = now_ms;
        Action::Restore {
            ticket,
            frequency_10hz,
            reason,
        }
    }

    fn tune(&mut self, now_ms: u32, index: u8) -> Action {
        if self.next_ticket >= u64::MAX - 1 {
            return self.restore(now_ms, StopReason::CounterExhausted, None);
        }
        let ticket = self.allocate().expect("reserved work ticket");
        let config = self.config.expect("active configuration");
        self.index = index;
        self.state = State::Tuning;
        self.ticket = Some(ticket);
        self.remaining = config.timing.settled_timeout_ms;
        self.settle_remaining = config.timing.settle_ms;
        Action::Tune {
            ticket,
            index,
            frequency_10hz: config
                .range
                .frequency_10hz(usize::from(index))
                .expect("bounded sweep index"),
        }
    }

    fn read(&mut self, now_ms: u32) -> Action {
        if self.next_ticket >= u64::MAX - 1 {
            return self.restore(now_ms, StopReason::CounterExhausted, None);
        }
        let ticket = self.allocate().expect("reserved work ticket");
        self.state = State::Reading;
        self.ticket = Some(ticket);
        self.remaining = self
            .config
            .expect("active configuration")
            .timing
            .sample_timeout_ms;
        Action::Read {
            ticket,
            index: self.index,
        }
    }

    pub fn handle(&mut self, now_ms: u32, event: Event) -> Option<Action> {
        // Stop/PTT priority deliberately precedes clock and hardware ACKs.
        // Accept remains a draft until Restore ACK. A new cancellation must
        // supersede that work with an original-frequency restore ticket.
        let selection_pending = matches!(self.state, State::Restoring(StopReason::Accepted { .. }));
        match event {
            Event::Ptt(true) => {
                self.ptt = true;
                if self.running() || selection_pending {
                    return Some(self.restore(now_ms, StopReason::Ptt, None));
                }
            }
            Event::Ptt(false) => self.ptt = false,
            Event::Stop if self.running() || selection_pending => {
                return Some(self.restore(now_ms, StopReason::Exit, None));
            }
            Event::Accept { index } if self.running() && !self.ptt => {
                if self
                    .config?
                    .range
                    .frequency_10hz(usize::from(index))
                    .is_some()
                {
                    return Some(self.restore(now_ms, StopReason::Accepted { index }, Some(index)));
                }
            }
            Event::ClearPeaks => self.plot.peak_valid.fill(0),
            _ => {}
        }
        if !self.running() && !self.restoring() {
            return None;
        }
        let elapsed = now_ms.wrapping_sub(self.last_now);
        if elapsed >= HALF_CLOCK {
            return Some(if self.restoring() {
                self.terminal(StopReason::Clock)
            } else {
                self.restore(now_ms, StopReason::Clock, None)
            });
        }
        self.last_now = now_ms;
        self.remaining = self.remaining.saturating_sub(elapsed);
        self.settle_remaining = self.settle_remaining.saturating_sub(elapsed);
        if let Event::Failed { ticket, code } = event {
            if self.ticket == Some(ticket) {
                let reason = StopReason::Hardware { code };
                return Some(if self.restoring() {
                    self.terminal(reason)
                } else {
                    self.restore(now_ms, reason, None)
                });
            }
        }
        // ACKs at the timeout boundary are late. Settling is a minimum delay,
        // not an ACK deadline, and never triggers a fabricated measurement.
        if self.remaining == 0 {
            let reason = match self.state {
                State::Preparing => Some(StopReason::PrepareTimeout),
                State::Tuning => Some(StopReason::TuneTimeout),
                State::Reading => Some(StopReason::SampleTimeout),
                State::Restoring(_) => return Some(self.terminal(StopReason::RestoreTimeout)),
                _ => None,
            };
            if let Some(reason) = reason {
                return Some(self.restore(now_ms, reason, None));
            }
        }
        match (self.state, event) {
            (State::Preparing, Event::Prepared(ticket)) if self.ticket == Some(ticket) => {
                Some(self.tune(now_ms, 0))
            }
            (State::Tuning, Event::Tuned(ticket)) if self.ticket == Some(ticket) => {
                // Count the minimum delay from acknowledged completion of the
                // register burst, not from command issue or adapter queue time.
                self.settle_remaining = self.config?.timing.settle_ms;
                if self.settle_remaining == 0 {
                    Some(self.read(now_ms))
                } else {
                    self.state = State::Settling;
                    None
                }
            }
            (State::Settling, Event::Poll) if self.settle_remaining == 0 => Some(self.read(now_ms)),
            (State::Reading, Event::Sample { ticket, raw }) if self.ticket == Some(ticket) => {
                if raw > MAX_RAW {
                    return Some(self.restore(now_ms, StopReason::InvalidSample, None));
                }
                let index = usize::from(self.index);
                self.plot.latest[index] = raw;
                if !self.plot.peak_valid(index) || raw > self.plot.peaks[index] {
                    self.plot.peaks[index] = raw;
                }
                set_bit(&mut self.plot.valid, index);
                set_bit(&mut self.plot.fresh, index);
                set_bit(&mut self.plot.peak_valid, index);
                if index + 1 == self.config?.range.len() {
                    self.state = State::SweepComplete;
                    self.ticket = None;
                    Some(Action::SweepComplete {
                        sweep: self.plot.sweep,
                    })
                } else {
                    Some(self.tune(now_ms, self.index + 1))
                }
            }
            (State::SweepComplete, Event::Poll) => {
                let Some(sweep) = self.plot.sweep.checked_add(1) else {
                    return Some(self.restore(now_ms, StopReason::CounterExhausted, None));
                };
                self.plot.sweep = sweep;
                self.plot.fresh.fill(0);
                Some(self.tune(now_ms, 0))
            }
            (State::Restoring(reason), Event::Restored(ticket)) if self.ticket == Some(ticket) => {
                self.state = State::Restored(reason);
                self.ticket = None;
                None
            }
            _ => None,
        }
    }
}

// These are target-independent upper bounds, including a 64-bit host's padding.
const _: () = assert!(core::mem::size_of::<Plot>() <= 368);
const _: () = assert!(core::mem::size_of::<Engine>() < 1024);

#[cfg(test)]
mod tests {
    use super::*;

    fn config(len: usize) -> Config {
        Config::new(
            Range::new(43_000_000, 43_000_000 + (len as u32 - 1) * 1250, 1250).unwrap(),
            43_100_000,
            Timing::new(50, 50, 10, 30).unwrap(),
            100,
        )
        .unwrap()
    }
    fn prepared(engine: &mut Engine, now: u32, len: usize) -> Ticket {
        let Action::Prepare { ticket } = engine.start(now, config(len)).unwrap() else {
            panic!()
        };
        let Some(Action::Tune {
            ticket, index: 0, ..
        }) = engine.handle(now, Event::Prepared(ticket))
        else {
            panic!()
        };
        ticket
    }
    fn sample(engine: &mut Engine, now: u32, tuned: Ticket, raw: u16) -> Option<Action> {
        assert_eq!(engine.handle(now, Event::Tuned(tuned)), None);
        let Some(Action::Read { ticket, .. }) = engine.handle(now + 10, Event::Poll) else {
            panic!()
        };
        engine.handle(now + 10, Event::Sample { ticket, raw })
    }
    #[test]
    fn footprint_and_config_reject_more_than_eighty_one() {
        assert!(core::mem::size_of::<Engine>() < 1024);
        assert!(core::mem::size_of::<Plot>() <= 368);
        assert_eq!(
            Config::new(
                Range::new(1, 82, 1).unwrap(),
                1,
                Timing::new(10, 10, 10, 10).unwrap(),
                10
            ),
            Err(ConfigError::TooManyBins)
        );
        assert!(!Plot::EMPTY.valid(0));
        assert!(!Plot::EMPTY.valid(81));
    }
    #[test]
    fn needs_every_ack_and_ten_ms_before_read() {
        let mut e = Engine::new();
        let Action::Prepare { ticket } = e.start(0, config(81)).unwrap() else {
            panic!()
        };
        assert_eq!(e.handle(10, Event::Poll), None);
        let Some(Action::Tune { ticket, .. }) = e.handle(10, Event::Prepared(ticket)) else {
            panic!()
        };
        assert_eq!(e.handle(19, Event::Poll), None); // no Tuned ACK
        assert_eq!(e.handle(19, Event::Tuned(ticket)), None);
        assert_eq!(e.handle(28, Event::Poll), None);
        assert!(matches!(
            e.handle(29, Event::Poll),
            Some(Action::Read { .. })
        ));
        assert!(!e.plot().valid(0)); // elapsed time is not a sample
    }
    #[test]
    fn partial_sweeps_and_peaks_remain_honest() {
        let mut e = Engine::new();
        let t = prepared(&mut e, 0, 2);
        let Some(Action::Tune {
            ticket, index: 1, ..
        }) = sample(&mut e, 0, t, 200)
        else {
            panic!()
        };
        assert!(e.plot.valid(0));
        assert!(!e.plot.valid(1));
        assert_eq!(
            sample(&mut e, 10, ticket, 20),
            Some(Action::SweepComplete { sweep: 1 })
        );
        let Some(Action::Tune { ticket, .. }) = e.handle(20, Event::Poll) else {
            panic!()
        };
        assert!(e.plot.valid(0));
        assert!(!e.plot.fresh(0));
        sample(&mut e, 20, ticket, 100);
        assert_eq!(e.plot.latest[0], 100);
        assert_eq!(e.plot.peaks[0], 200);
        assert!(e.plot.fresh(0));
        assert!(!e.plot.fresh(1));
        e.handle(30, Event::ClearPeaks);
        assert!(!e.plot.peak_valid(0));
        assert!(e.plot.valid(0));
    }
    #[test]
    fn stop_requires_restore_ack_and_never_accepts_stale_sample() {
        let mut e = Engine::new();
        let t = prepared(&mut e, 0, 81);
        let Some(Action::Restore {
            ticket,
            frequency_10hz: 43_100_000,
            reason: StopReason::Exit,
        }) = e.handle(1, Event::Stop)
        else {
            panic!()
        };
        assert_eq!(e.start(2, config(81)), Err(StartError::Busy));
        assert_eq!(e.handle(2, Event::Tuned(t)), None);
        assert_eq!(e.handle(2, Event::Restored(t)), None);
        assert!(e.restoring());
        assert_eq!(e.handle(3, Event::Restored(ticket)), None);
        assert_eq!(e.state(), State::Restored(StopReason::Exit));
        let Action::Prepare { ticket: next } = e.start(4, config(81)).unwrap() else {
            panic!()
        };
        assert!(next.value() > ticket.value());
    }
    #[test]
    fn ptt_cancel_needs_release_then_explicit_restart() {
        let mut e = Engine::new();
        prepared(&mut e, 10, 81);
        let Some(Action::Restore {
            ticket,
            reason: StopReason::Ptt,
            ..
        }) = e.handle(0, Event::Ptt(true))
        else {
            panic!()
        }; // even clock moved backward
        e.handle(1, Event::Restored(ticket));
        assert_eq!(e.start(2, config(81)), Err(StartError::PttHeld));
        assert_eq!(e.handle(2, Event::Ptt(false)), None);
        assert_eq!(e.state(), State::Restored(StopReason::Ptt));
        assert!(e.start(3, config(81)).is_ok());
    }
    #[test]
    fn accept_uses_confirmed_grid_and_invalid_index_is_ignored() {
        let mut e = Engine::new();
        prepared(&mut e, 0, 3);
        assert_eq!(e.handle(0, Event::Accept { index: 80 }), None);
        assert!(matches!(
            e.handle(1, Event::Accept { index: 2 }),
            Some(Action::Restore {
                frequency_10hz: 43_002_500,
                reason: StopReason::Accepted { index: 2 },
                ..
            })
        ));
    }
    #[test]
    fn deadlines_restore_and_restore_failure_is_terminal() {
        let mut e = Engine::new();
        let Action::Prepare { ticket } = e.start(0, config(81)).unwrap() else {
            panic!()
        };
        let Some(Action::Restore {
            ticket: restore,
            reason: StopReason::PrepareTimeout,
            ..
        }) = e.handle(50, Event::Prepared(ticket))
        else {
            panic!()
        };
        assert_eq!(
            e.handle(150, Event::Restored(restore)),
            Some(Action::Terminal {
                reason: StopReason::RestoreTimeout
            })
        );
        assert_eq!(e.start(151, config(81)), Err(StartError::Terminal));
    }
    #[test]
    fn invalid_rssi_does_not_become_a_clamped_measurement() {
        let mut e = Engine::new();
        let t = prepared(&mut e, 0, 1);
        assert!(matches!(
            sample(&mut e, 0, t, 512),
            Some(Action::Restore {
                reason: StopReason::InvalidSample,
                ..
            })
        ));
        assert!(!e.plot.valid(0));
    }
    #[test]
    fn ptt_during_selected_restore_cancels_draft_even_after_release() {
        let mut e = Engine::new();
        prepared(&mut e, 0, 81);
        let Some(Action::Restore {
            ticket: selected, ..
        }) = e.handle(1, Event::Accept { index: 3 })
        else {
            panic!()
        };
        let Some(Action::Restore {
            ticket: original,
            frequency_10hz: 43_100_000,
            reason: StopReason::Ptt,
        }) = e.handle(2, Event::Ptt(true))
        else {
            panic!()
        };
        assert_ne!(selected, original);
        assert_eq!(e.handle(3, Event::Ptt(false)), None);
        assert_eq!(e.handle(4, Event::Restored(selected)), None);
        assert_eq!(e.state(), State::Restoring(StopReason::Ptt));
        assert_eq!(e.handle(5, Event::Restored(original)), None);
        assert_eq!(e.state(), State::Restored(StopReason::Ptt));
    }

    #[test]
    fn exit_replaces_uncommitted_selection_but_repeated_cancel_does_not_delay_timeout() {
        let mut e = Engine::new();
        prepared(&mut e, 0, 81);
        let Some(Action::Restore {
            ticket: selected, ..
        }) = e.handle(1, Event::Accept { index: 3 })
        else {
            panic!()
        };
        let Some(Action::Restore {
            ticket: original,
            frequency_10hz: 43_100_000,
            reason: StopReason::Exit,
        }) = e.handle(2, Event::Stop)
        else {
            panic!()
        };
        assert_ne!(selected, original);
        assert_eq!(e.handle(3, Event::Restored(selected)), None);
        assert_eq!(e.handle(40, Event::Stop), None);
        assert_eq!(e.handle(90, Event::Ptt(true)), None);
        assert_eq!(e.handle(101, Event::Ptt(true)), None);
        assert_eq!(
            e.handle(102, Event::Restored(original)),
            Some(Action::Terminal {
                reason: StopReason::RestoreTimeout
            })
        );
    }

    #[test]
    fn wrap_clock_and_terminal_counter_budget() {
        let mut e = Engine::new();
        let t = prepared(&mut e, u32::MAX - 4, 1);
        assert_eq!(e.handle(u32::MAX - 4, Event::Tuned(t)), None);
        assert!(matches!(
            e.handle(5, Event::Poll),
            Some(Action::Read { .. })
        ));
        let mut e = Engine::new();
        e.next_ticket = u64::MAX - 2;
        let Action::Prepare { ticket } = e.start(0, config(1)).unwrap() else {
            panic!()
        };
        assert!(matches!(
            e.handle(0, Event::Prepared(ticket)),
            Some(Action::Restore {
                ticket: Ticket(u64::MAX),
                reason: StopReason::CounterExhausted,
                ..
            })
        ));
    }
}
