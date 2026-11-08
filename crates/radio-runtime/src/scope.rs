//! Integration of the compact sweep protocol and the qualified stock RF
//! fragment. Preparation and restoration are complete normal RX plans.
//! This adapter has no transmit operation and never acknowledges work by time
//! alone. The caller continues Runtime::tick and supplies a fresh clock.

use crate::{Error, Fault, Io, Runtime, State};
use uv98_calibration::Calibration;
use uv98_rf_sequence::{self as sequence, Modulation, RxRequest, Width};
use uv98_spectrum::{compact, Range, Ticket, Timing};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    NotReceiving,
    Busy,
    Grid,
    Qualification(sequence::sweep::Error),
    Receive(sequence::Error),
    Protocol(compact::StartError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    None,
    Restored {
        selected_frequency: Option<u32>,
        modulation: Modulation,
        reason: compact::StopReason,
    },
}

#[derive(Clone, Copy)]
enum Waiting {
    Prepare(Ticket),
    Restore {
        ticket: Ticket,
        frequency: u32,
        reason: compact::StopReason,
    },
}

pub struct Scope {
    engine: compact::Engine,
    qualified: Option<sequence::sweep::QualifiedRange>,
    original: Option<RxRequest>,
    pending: Option<compact::Action>,
    waiting: Option<Waiting>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    pub const fn new() -> Self {
        Self {
            engine: compact::Engine::new(),
            qualified: None,
            original: None,
            pending: None,
            waiting: None,
        }
    }
    pub fn active(&self) -> bool {
        self.engine.running()
            || self.engine.restoring()
            || matches!(self.pending, Some(compact::Action::Terminal { .. }))
    }
    pub fn state(&self) -> compact::State {
        self.engine.state()
    }
    pub fn plot(&self) -> &compact::Plot {
        self.engine.plot()
    }
    pub fn range(&self) -> Option<Range> {
        self.engine.config().map(|c| c.range)
    }

    /// Only validation and protocol state change occur here; bad grids leave
    /// the working receiver alone. Exactly 81 points centered on the original
    /// frequency are used. Crossing a qualified band/route boundary is refused.
    pub fn start<I: Io>(
        &mut self,
        radio: &Runtime<I>,
        calibration: &Calibration<'_>,
        original: RxRequest,
        step_10hz: u32,
        now: u32,
    ) -> Result<(), StartError> {
        if self.active() {
            return Err(StartError::Busy);
        }
        if radio.state() != State::Receiving {
            return Err(StartError::NotReceiving);
        }
        let half = step_10hz.checked_mul(40).ok_or(StartError::Grid)?;
        let first = original
            .frequency_10hz
            .checked_sub(half)
            .ok_or(StartError::Grid)?;
        let last = original
            .frequency_10hz
            .checked_add(half)
            .ok_or(StartError::Grid)?;
        let range = Range::new(first, last, step_10hz).map_err(|_| StartError::Grid)?;
        let qualified = sequence::sweep::QualifiedRange::new(
            calibration,
            original.frequency_10hz,
            first,
            last,
            step_10hz,
        )
        .map_err(StartError::Qualification)?;
        sequence::receive(calibration, original).map_err(StartError::Receive)?;
        let timing = Timing::new(3000, 1000, 10, 1000).map_err(|_| StartError::Grid)?;
        let config = compact::Config::new(range, original.frequency_10hz, timing, 3000)
            .map_err(|_| StartError::Grid)?;
        let action = self
            .engine
            .start(now, config)
            .map_err(StartError::Protocol)?;
        self.qualified = Some(qualified);
        self.original = Some(original);
        self.waiting = None;
        self.pending = Some(action);
        Ok(())
    }

    /// Stop/PTT/accept override queued work before the next RF operation.
    /// A false PTT observation must also be forwarded after cancellation so a
    /// future sweep is not blocked by an obsolete held state.
    pub fn event(&mut self, now: u32, event: compact::Event) {
        if let compact::Event::Accept { index } = event {
            if !self.engine.plot().valid(usize::from(index)) {
                return;
            }
        }
        if let Some(action) = self.engine.handle(now, event) {
            if matches!(
                action,
                compact::Action::Restore { .. } | compact::Action::Terminal { .. }
            ) {
                self.waiting = None;
            }
            self.pending = Some(action);
        }
    }

    /// At most one sweep command is performed per call. Full receive plans
    /// advance independently through Runtime::tick. `clock` must be sampled
    /// after synchronous writes, never a captured pre-transaction timestamp.
    pub fn step<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        calibration: &Calibration<'_>,
        mut clock: impl FnMut() -> u32,
    ) -> Result<Completion, Fault<I::Error>> {
        if !self.active() {
            return Ok(Completion::None);
        }
        let now = clock();
        if let Some(waiting) = self.waiting {
            if radio.state() == State::Receiving {
                self.waiting = None;
                match waiting {
                    Waiting::Prepare(ticket) => {
                        let qualified = self.qualified.ok_or_else(|| radio.fail(Error::State))?;
                        radio.sweep_enter(qualified, now)?;
                        self.pending = self
                            .engine
                            .handle(clock(), compact::Event::Prepared(ticket));
                    }
                    Waiting::Restore {
                        ticket,
                        frequency,
                        reason,
                    } => {
                        self.pending = self
                            .engine
                            .handle(clock(), compact::Event::Restored(ticket));
                        if !matches!(self.engine.state(), compact::State::Restored(_)) {
                            return Err(radio.fail(Error::State));
                        }
                        let modulation = self
                            .original
                            .ok_or_else(|| radio.fail(Error::State))?
                            .modulation;
                        return Ok(Completion::Restored {
                            selected_frequency: matches!(
                                reason,
                                compact::StopReason::Accepted { .. }
                            )
                            .then_some(frequency),
                            modulation,
                            reason,
                        });
                    }
                }
            } else {
                self.event(now, compact::Event::Poll);
            }
        } else {
            // Check deadlines even if the caller has not yet executed a queued
            // command. Poll emits no acknowledgement and cannot replace an
            // outstanding action except with a timeout restoration/terminal.
            self.event(now, compact::Event::Poll);
        }
        let Some(action) = self.pending.take() else {
            return Ok(Completion::None);
        };
        match action {
            compact::Action::Prepare { ticket } => {
                radio.stop()?;
                let mut request = self.original.ok_or_else(|| radio.fail(Error::State))?;
                request.tone = 0;
                request.tone_mode = 0;
                request.option_nibble = 0;
                request.level = 0;
                radio.receive(calibration, request, clock())?;
                self.waiting = Some(Waiting::Prepare(ticket));
            }
            compact::Action::Tune { ticket, index, .. } => {
                radio.sweep_tune(usize::from(index), clock())?;
                let completed_at = clock();
                // Establish the runtime's independent post-write settle anchor.
                if radio.sweep_read_rssi(completed_at)?.is_some() {
                    return Err(radio.fail(Error::State));
                }
                self.pending = self
                    .engine
                    .handle(completed_at, compact::Event::Tuned(ticket));
            }
            compact::Action::Read { ticket, index } => {
                if let Some(sample) = radio.sweep_read_rssi(clock())? {
                    if sample.index != usize::from(index) {
                        return Err(radio.fail(Error::State));
                    }
                    self.pending = self.engine.handle(
                        clock(),
                        compact::Event::Sample {
                            ticket,
                            raw: sample.raw,
                        },
                    );
                } else {
                    self.pending = Some(compact::Action::Read { ticket, index });
                }
            }
            compact::Action::Restore {
                ticket,
                frequency_10hz,
                reason,
            } => {
                radio.stop()?;
                let mut request = self.original.ok_or_else(|| radio.fail(Error::State))?;
                if matches!(reason, compact::StopReason::Accepted { .. }) {
                    // Explicit accept enters VFO, whose ordinary policy is
                    // wide/no tone/no channel-only option. Preserve AM/FM.
                    request.frequency_10hz = frequency_10hz;
                    request.width = Width::Wide;
                    request.tone = 0;
                    request.tone_mode = 0;
                    request.option_nibble = 0;
                }
                radio.receive(calibration, request, clock())?;
                self.waiting = Some(Waiting::Restore {
                    ticket,
                    frequency: frequency_10hz,
                    reason,
                });
            }
            compact::Action::SweepComplete { .. } => {}
            compact::Action::Terminal { .. } => return Err(radio.fail(Error::State)),
        }
        Ok(Completion::None)
    }
}

const _: () = assert!(core::mem::size_of::<Scope>() < 1200);
