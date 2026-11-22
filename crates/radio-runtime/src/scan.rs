//! Bounded scan decisions only. The caller performs tuning and reports the actual
//! runtime state; channel catalog selection and RF policy remain outside here.
//! Cancel stops this session. Only a new explicit start can begin another one.

use uv98_handheld_ui::controller::CHANNEL_COUNT;

pub const DWELL_MS: u32 = 150;
pub const RESUME_MS: u32 = 500;
pub const TUNE_TIMEOUT_MS: u32 = super::CONFIG_TIMEOUT_MS + super::RECEIVE_SETTLE_MS + 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Channels,
    /// Units10Hz; the caller clamps to a supported RF routing window.
    Vfo {
        low: u32,
        high: u32,
        step: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    None,
    NextChannel,
    TuneFrequency(u32),
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Idle,
    AwaitingTune,
    Dwelling,
    Paused,
    Quiet,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidRange,
    Busy,
    Clock,
    TuneTimeout,
    TicketExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ticket(u64);
impl Ticket {
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Status {
    pub phase: Phase,
    pub target: Option<Target>,
    pub ticket: Option<Ticket>,
    pub frequency: Option<u32>,
    pub error: Option<Error>,
}

pub struct Scheduler {
    status: Status,
    counter: u64,
    last_now: Option<u32>,
    since: u32,
    vfo_point: usize,
    // Memory slot IDs or zero-based points in the current VFO grid.
    skipped: [u8; CHANNEL_COUNT / 8],
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            status: Status {
                phase: Phase::Idle,
                target: None,
                ticket: None,
                frequency: None,
                error: None,
            },
            counter: 0,
            last_now: None,
            since: 0,
            vfo_point: 0,
            skipped: [0; CHANNEL_COUNT / 8],
        }
    }
    pub const fn status(&self) -> Status {
        self.status
    }
    pub const fn active(&self) -> bool {
        matches!(
            self.status.phase,
            Phase::AwaitingTune | Phase::Dwelling | Phase::Paused | Phase::Quiet
        )
    }
    pub const fn paused(&self) -> bool {
        matches!(self.status.phase, Phase::Paused | Phase::Quiet)
    }

    /// Only used for a session scanning the memory catalog.
    pub const fn skipped_channels(&self) -> &[u8; CHANNEL_COUNT / 8] {
        &self.skipped
    }

    fn fail(&mut self, error: Error) -> Error {
        self.status.phase = Phase::Stopped;
        self.status.ticket = None;
        self.status.error = Some(error);
        error
    }
    fn advance(&mut self, now: u32) -> Result<(), Error> {
        if self
            .last_now
            .is_some_and(|last| now.wrapping_sub(last) >= 1 << 31)
        {
            return Err(self.fail(Error::Clock));
        }
        self.last_now = Some(now);
        Ok(())
    }

    /// Emits exactly one first tuning request. A stopped session never restarts
    /// from tick/PTT release; this method is an explicit new operator command.
    pub fn start(&mut self, target: Target, now: u32) -> Result<Action, Error> {
        self.advance(now)?;
        if self.active() {
            return Err(self.fail(Error::Busy));
        }
        if let Target::Vfo { low, high, step } = target {
            // The normal +/-500 kHz VFO window at the smallest step has
            // at most 401 points. Bound the grid by the shared skip bitmap.
            if low == 0 || low > high || (high - low) / CHANNEL_COUNT as u32 >= step {
                return Err(self.fail(Error::InvalidRange));
            }
        }
        self.status.target = Some(target);
        self.status.frequency = None;
        self.status.error = None;
        self.vfo_point = 0;
        self.skipped.fill(0);
        self.issue(now)
    }

    fn issue(&mut self, now: u32) -> Result<Action, Error> {
        let action = match self.status.target {
            Some(Target::Channels) => Action::NextChannel,
            Some(Target::Vfo { low, high, step }) => {
                let start = self.vfo_point;
                let mut point = start;
                let mut frequency = self.status.frequency.unwrap_or(high);
                loop {
                    if step <= high - frequency {
                        frequency += step;
                        point += 1;
                    } else {
                        frequency = low;
                        point = 0;
                    }
                    if self.skipped[point / 8] & (1 << (point % 8)) == 0 {
                        self.vfo_point = point;
                        break Action::TuneFrequency(frequency);
                    }
                    if point == start {
                        return Ok(self.cancel());
                    }
                }
            }
            None => return Err(self.fail(Error::InvalidRange)),
        };
        let Some(counter) = self.counter.checked_add(1) else {
            return Err(self.fail(Error::TicketExhausted));
        };
        self.counter = counter;
        self.status.ticket = Some(Ticket(counter));
        self.status.phase = Phase::AwaitingTune;
        self.since = now;
        if let Action::TuneFrequency(next) = action {
            self.status.frequency = Some(next);
        }
        Ok(action)
    }

    /// Exclude the signal's current point or selected memory slot and advance
    /// immediately, including during the quiet delay after the signal closes.
    pub fn skip_current(&mut self, now: u32, channel: Option<u16>) -> Result<Action, Error> {
        if !self.paused() {
            return Ok(Action::None);
        }
        let index = match self.status.target {
            Some(Target::Channels) => channel.map(usize::from),
            Some(Target::Vfo { .. }) => Some(self.vfo_point),
            None => None,
        };
        let Some(index) = index.filter(|&index| index < CHANNEL_COUNT) else {
            return Ok(Action::None);
        };
        self.advance(now)?;
        self.skipped[index / 8] |= 1 << (index % 8);
        self.issue(now)
    }

    /// Acknowledge only the current request and actual runtime Receiving state.
    /// Old/wrong-phase/non-Receiving ACKs neither advance time nor extend timeout.
    /// The accepting call starts a full new dwell interval after RF settling.
    pub fn tune_ack(
        &mut self,
        ticket: Ticket,
        now: u32,
        radio_state: super::State,
    ) -> Result<bool, Error> {
        if self.status.phase != Phase::AwaitingTune
            || self.status.ticket != Some(ticket)
            || radio_state != super::State::Receiving
        {
            return Ok(false);
        }
        self.advance(now)?;
        if now.wrapping_sub(self.since) >= TUNE_TIMEOUT_MS {
            return Err(self.fail(Error::TuneTimeout));
        }
        self.status.phase = Phase::Dwelling;
        self.since = now;
        Ok(true)
    }

    /// `signal_or_audio_open` is a fresh accepted signal/audio observation, not a
    /// placeholder RSSI threshold. At most one tune is emitted per call.
    pub fn tick(&mut self, now: u32, signal_or_audio_open: bool) -> Result<Action, Error> {
        self.tick_with_cursor_hold(now, signal_or_audio_open, false)
    }

    /// A pending short gesture can defer timed cursor moves. RF observations,
    /// tune deadlines and explicit cancel/skip still take effect.
    pub fn tick_with_cursor_hold(
        &mut self,
        now: u32,
        signal_or_audio_open: bool,
        cursor_held: bool,
    ) -> Result<Action, Error> {
        if !self.active() {
            return Ok(Action::None);
        }
        self.advance(now)?;
        match self.status.phase {
            Phase::AwaitingTune => {
                if now.wrapping_sub(self.since) >= TUNE_TIMEOUT_MS {
                    return Err(self.fail(Error::TuneTimeout));
                }
            }
            Phase::Dwelling => {
                if signal_or_audio_open {
                    self.status.phase = Phase::Paused;
                    self.since = now;
                } else if !cursor_held && now.wrapping_sub(self.since) >= DWELL_MS {
                    return self.issue(now);
                }
            }
            Phase::Paused => {
                if !signal_or_audio_open {
                    self.status.phase = Phase::Quiet;
                    self.since = now;
                }
            }
            Phase::Quiet => {
                if signal_or_audio_open {
                    self.status.phase = Phase::Paused;
                    self.since = now;
                } else if !cursor_held && now.wrapping_sub(self.since) >= RESUME_MS {
                    return self.issue(now);
                }
            }
            Phase::Idle | Phase::Stopped => {}
        }
        Ok(Action::None)
    }

    /// Manual navigation or either physical PTT must call this before RF action.
    /// Idempotent: stop is emitted once, no hold/release-triggered auto-resume.
    pub fn cancel(&mut self) -> Action {
        let active = self.active();
        self.status.phase = Phase::Stopped;
        self.status.ticket = None;
        if active {
            Action::Stop
        } else {
            Action::None
        }
    }
}
impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::State;
    fn ack(scan: &mut Scheduler, now: u32) {
        assert!(scan
            .tune_ack(scan.status().ticket.unwrap(), now, State::Receiving)
            .unwrap());
    }

    #[test]
    fn channels_wait_for_receiving_then_full_dwell() {
        let mut scan = Scheduler::new();
        assert_eq!(scan.start(Target::Channels, 0), Ok(Action::NextChannel));
        let ticket = scan.status().ticket.unwrap();
        for state in [
            State::Off,
            State::Configuring,
            State::Transmitting,
            State::Failed,
        ] {
            assert_eq!(scan.tune_ack(ticket, 50, state), Ok(false));
        }
        assert_eq!(scan.tick(1000, false), Ok(Action::None));
        ack(&mut scan, 1000);
        assert_eq!(scan.tick(1149, false), Ok(Action::None));
        assert_eq!(scan.tick(1150, false), Ok(Action::NextChannel));
        assert_ne!(scan.status().ticket, Some(ticket));
        assert_eq!(scan.tick(1151, false), Ok(Action::None));
    }

    #[test]
    fn signal_pause_restarts_quiet_timer_and_never_hops_during_audio() {
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        ack(&mut scan, 10);
        assert_eq!(scan.tick(20, true), Ok(Action::None));
        assert_eq!(scan.status().phase, Phase::Paused);
        assert_eq!(scan.tick(10_000, true), Ok(Action::None));
        assert_eq!(scan.tick(10_001, false), Ok(Action::None));
        assert_eq!(scan.tick(10_500, false), Ok(Action::None));
        assert_eq!(scan.tick(10_501, true), Ok(Action::None));
        assert_eq!(scan.tick(10_600, false), Ok(Action::None));
        assert_eq!(scan.tick(11_099, false), Ok(Action::None));
        assert_eq!(scan.tick(11_100, false), Ok(Action::NextChannel));
    }

    #[test]
    fn memory_skip_advances_only_from_signal_pause_and_expires_on_restart() {
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        let ticket = scan.status().ticket;
        assert_eq!(scan.skip_current(1, Some(1023)), Ok(Action::None));
        assert_eq!(scan.status().ticket, ticket);
        ack(&mut scan, 2);
        assert_eq!(scan.skip_current(3, Some(1023)), Ok(Action::None));
        scan.tick(4, true).unwrap();
        assert!(scan.paused());
        assert_eq!(scan.skip_current(5, None), Ok(Action::None));
        assert_eq!(scan.skip_current(5, Some(1024)), Ok(Action::None));
        assert_eq!(scan.skip_current(5, Some(1023)), Ok(Action::NextChannel));
        assert!(!scan.paused());
        assert_ne!(scan.status().ticket, ticket);
        assert_eq!(scan.skipped_channels()[127], 0x80);
        assert_eq!(scan.tick(6, true), Ok(Action::None));

        scan.cancel();
        assert_eq!(scan.skip_current(7, Some(0)), Ok(Action::None));
        scan.start(Target::Channels, 8).unwrap();
        assert_eq!(scan.skipped_channels(), &[0; 128]);
    }

    #[test]
    fn vfo_skip_advances_during_quiet_wraps_and_stops_when_exhausted() {
        let mut scan = Scheduler::new();
        let target = Target::Vfo {
            low: 10,
            high: 23,
            step: 6,
        };
        assert_eq!(scan.start(target, 0), Ok(Action::TuneFrequency(10)));
        ack(&mut scan, 1);
        scan.tick(2, true).unwrap();
        scan.tick(3, false).unwrap();
        assert!(scan.paused());
        assert_eq!(scan.skip_current(4, None), Ok(Action::TuneFrequency(16)));
        ack(&mut scan, 5);
        assert_eq!(
            scan.tick(5 + DWELL_MS, false),
            Ok(Action::TuneFrequency(22))
        );
        ack(&mut scan, 156);
        assert_eq!(scan.tick(306, false), Ok(Action::TuneFrequency(16)));
        ack(&mut scan, 307);
        scan.tick(308, true).unwrap();
        assert_eq!(scan.skip_current(309, None), Ok(Action::TuneFrequency(22)));
        ack(&mut scan, 310);
        scan.tick(311, true).unwrap();
        assert_eq!(scan.skip_current(312, None), Ok(Action::Stop));
        assert_eq!(scan.status().frequency, Some(22));
        assert!(!scan.active());
        assert_eq!(scan.tick(313, true), Ok(Action::None));
        assert_eq!(scan.start(target, 314), Ok(Action::TuneFrequency(10)));
    }

    #[test]
    fn vfo_bounds_nondivisible_span_and_no_catchup() {
        let mut scan = Scheduler::new();
        let low = u32::MAX - 10;
        assert_eq!(
            scan.start(
                Target::Vfo {
                    low,
                    high: u32::MAX,
                    step: 6
                },
                0
            ),
            Ok(Action::TuneFrequency(low))
        );
        ack(&mut scan, 0);
        assert_eq!(scan.tick(20_000, false), Ok(Action::TuneFrequency(low + 6)));
        ack(&mut scan, 20_001);
        assert_eq!(scan.tick(20_151, false), Ok(Action::TuneFrequency(low)));
        assert_eq!(scan.tick(20_152, false), Ok(Action::None));
    }

    #[test]
    fn one_point_and_oversized_step_remain_bounded() {
        for (low, high, step) in [(10, 10, 1), (10, 11, 100), (1, u32::MAX, u32::MAX)] {
            let mut scan = Scheduler::new();
            assert_eq!(
                scan.start(Target::Vfo { low, high, step }, 0),
                Ok(Action::TuneFrequency(low))
            );
            ack(&mut scan, 1);
            assert_eq!(scan.tick(151, false), Ok(Action::TuneFrequency(low)));
        }
    }

    #[test]
    fn wrong_generation_late_ack_and_deadline_equality() {
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        let old = scan.status().ticket.unwrap();
        ack(&mut scan, 0);
        scan.tick(DWELL_MS, false).unwrap();
        let current = scan.status().ticket.unwrap();
        assert_eq!(scan.tune_ack(old, 123_456, State::Receiving), Ok(false));
        assert_eq!(
            scan.tune_ack(current, DWELL_MS + TUNE_TIMEOUT_MS, State::Receiving),
            Err(Error::TuneTimeout)
        );
        assert_eq!(scan.status().phase, Phase::Stopped);
        assert_eq!(scan.tune_ack(current, 1, State::Receiving), Ok(false));
    }

    #[test]
    fn clock_wrap_works_backward_or_half_range_stops() {
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, u32::MAX - 50).unwrap();
        ack(&mut scan, u32::MAX - 20);
        assert_eq!(scan.tick(128, false), Ok(Action::None));
        assert_eq!(scan.tick(129, false), Ok(Action::NextChannel));
        assert_eq!(scan.tick(128, false), Err(Error::Clock));
        assert_eq!(scan.status().phase, Phase::Stopped);
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        assert_eq!(scan.tick(1 << 31, false), Err(Error::Clock));
    }

    #[test]
    fn cancel_is_terminal_until_explicit_new_start_and_rejects_old_ack() {
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        let old = scan.status().ticket.unwrap();
        assert_eq!(scan.cancel(), Action::Stop);
        assert_eq!(scan.cancel(), Action::None);
        assert_eq!(scan.tick(999_999, true), Ok(Action::None));
        assert_eq!(scan.tick(1_000_000, false), Ok(Action::None));
        assert_eq!(scan.tune_ack(old, 0, State::Receiving), Ok(false));
        assert_eq!(
            scan.start(Target::Channels, 1_000_001),
            Ok(Action::NextChannel)
        );
        assert_ne!(scan.status().ticket, Some(old));
        assert_eq!(scan.tune_ack(old, 1_000_002, State::Receiving), Ok(false));
    }

    #[test]
    fn invalid_ranges_timeout_and_busy_are_stopped() {
        for target in [
            Target::Vfo {
                low: 0,
                high: 1,
                step: 1,
            },
            Target::Vfo {
                low: 2,
                high: 1,
                step: 1,
            },
            Target::Vfo {
                low: 1,
                high: 2,
                step: 0,
            },
            Target::Vfo {
                low: 1,
                high: CHANNEL_COUNT as u32 + 1,
                step: 1,
            },
        ] {
            let mut scan = Scheduler::new();
            assert_eq!(scan.start(target, 0), Err(Error::InvalidRange));
            assert_eq!(scan.status().phase, Phase::Stopped);
        }
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        assert_eq!(scan.tick(TUNE_TIMEOUT_MS, false), Err(Error::TuneTimeout));
        let mut scan = Scheduler::new();
        scan.start(Target::Channels, 0).unwrap();
        assert_eq!(scan.start(Target::Channels, 1), Err(Error::Busy));
        assert_eq!(scan.status().phase, Phase::Stopped);
    }
}
