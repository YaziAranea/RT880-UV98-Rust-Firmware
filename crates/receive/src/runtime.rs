//! Primary-chip RX activation, raw signal sampling and explicit FM audio.
//!
//! This is a NEW composition for an already initialized/qualified receiver,
//! not cold boot or a complete stock channel setup. It deliberately requires
//! external settling evidence; successful bus writes and elapsed time alone
//! never make Listening. No GPIO implementation, TX operation or calibrated
//! dBm conversion is hidden here. Stop/fault attempts external audio mute only,
//! not RF shutdown, rollback, or a claim that transmission is physically off.

use crate::{executor, Chip, Plan};
use uv98_stock_model::rf::volume_register_05768;

const HALF_CLOCK: u32 = 1 << 31;
/// Nominal stock SysTick delay; clock calibration is the adapter's contract.
/// This is NOT a PLL-lock test or a measured relay/receiver settling time.
pub const POWER_DELAY_MS: u32 = 10;

pub trait RuntimeIo: executor::Io {
    /// External audio gate only. On the reviewed related board muted=true
    /// requests PF8 LOW. Must be bounded and usable for cleanup even after a
    /// failed RF bus or interlock. An error may follow a partial GPIO change.
    fn set_audio_mute(&mut self, muted: bool) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    operation_timeout_ms: u32,
    settling_timeout_ms: u32,
    min_settle_ms: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingError {
    InvalidTimeout,
    InvalidSettle,
}
impl Timing {
    pub const fn new(
        operation_timeout_ms: u32,
        settling_timeout_ms: u32,
        min_settle_ms: u32,
    ) -> Result<Self, TimingError> {
        if operation_timeout_ms <= POWER_DELAY_MS
            || operation_timeout_ms >= HALF_CLOCK
            || settling_timeout_ms == 0
            || settling_timeout_ms >= HALF_CLOCK
        {
            return Err(TimingError::InvalidTimeout);
        }
        if min_settle_ms >= settling_timeout_ms {
            return Err(TimingError::InvalidSettle);
        }
        Ok(Self {
            operation_timeout_ms,
            settling_timeout_ms,
            min_settle_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Volume {
    gain: u8,
    dac: u8,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VolumeError;
impl Volume {
    pub const fn new(gain: u8, dac: u8) -> Result<Self, VolumeError> {
        if gain > 63 || dac > 15 {
            return Err(VolumeError);
        }
        Ok(Self { gain, dac })
    }
    pub const fn stored_word(self) -> u16 {
        0xb000 + (self.gain as u16) * 16 + self.dac as u16
    }
}

/// Generation within ONE Runtime lifetime. Never forward an acknowledgment
/// from a replaced runtime; an adapter must keep its own lifetime/epoch scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ticket(u64);
impl Ticket {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Idle,
    Preflight,
    MuteOutput,
    MuteInternal,
    Configuring,
    PowerOn,
    PowerDelay,
    StartLow,
    StartHigh,
    AwaitSettling,
    Listening,
    Stopped,
    Faulted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    pub state: State,
    pub ticket: Option<Ticket>,
    pub configuration_operations: usize,
    pub settled_acknowledged: bool,
    /// Software permission following a successful open request. False after
    /// fault/stop does NOT confirm a muted physical gate; inspect cleanup error.
    pub audio_open: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signal {
    pub ticket: Ticket,
    pub raw_rssi: u16,
    pub squelch_open: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Busy,
    Terminal,
    NotListening,
    Faulted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Guard,
    Write(u8),
    Read(u8),
    AudioMute(bool),
}
#[derive(Debug, Eq, PartialEq)]
pub enum Failure<E> {
    Configuration,
    Io { action: Action, error: E },
    Clock,
    OperationTimeout,
    SettlingTimeout,
    CounterExhausted,
    InternalState,
}
#[derive(Debug, Eq, PartialEq)]
pub struct Fault<E> {
    pub reason: Failure<E>,
    pub cleanup_error: Option<E>,
    pub state: State,
    pub ticket: Option<Ticket>,
}

pub struct Runtime<I: RuntimeIo> {
    executor: executor::Executor<I>,
    timing: Timing,
    state: State,
    counter: u64,
    ticket: Option<Ticket>,
    last_now: u32,
    operation_left: u32,
    settling_left: u32,
    minimum_left: u32,
    power_left: u32,
    power_armed: bool,
    settling_armed: bool,
    settled: bool,
    audio_open: bool,
    level: u8,
    fault: Option<Fault<I::Error>>,
}
impl<I: RuntimeIo> Runtime<I> {
    /// Inert. The caller must already qualify bus/pins, RF initialization,
    /// modulation/FM assumptions, PA inactivity, clock and exclusive ownership.
    /// Every `now` argument must be a fresh reading of the same monotonic
    /// millisecond clock. Observe active sessions less than 2^31 ms apart.
    pub const fn new(io: I, timing: Timing) -> Self {
        Self {
            executor: executor::Executor::new(io),
            timing,
            state: State::Idle,
            counter: 0,
            ticket: None,
            last_now: 0,
            operation_left: 0,
            settling_left: 0,
            minimum_left: 0,
            power_left: 0,
            power_armed: false,
            settling_armed: false,
            settled: false,
            audio_open: false,
            level: 0,
            fault: None,
        }
    }
    pub const fn state(&self) -> State {
        self.state
    }
    pub const fn ticket(&self) -> Option<Ticket> {
        self.ticket
    }
    pub fn progress(&self) -> Progress {
        Progress {
            state: self.state,
            ticket: self.ticket,
            configuration_operations: self.executor.progress().completed_operations,
            settled_acknowledged: self.settled,
            audio_open: self.audio_open,
        }
    }
    pub fn fault(&self) -> Option<&Fault<I::Error>> {
        self.fault.as_ref()
    }
    pub fn configuration_fault(&self) -> Option<&executor::Fault<I::Error>> {
        self.executor.fault()
    }
    fn terminal(&self) -> bool {
        matches!(self.state, State::Stopped | State::Faulted)
    }

    fn fail(&mut self, reason: Failure<I::Error>) -> Error {
        if self.terminal() {
            return Error::Faulted;
        }
        let state = self.state;
        self.state = State::Faulted;
        self.executor.cancel();
        // Only an external mute attempt: no retry through a broken RF bus,
        // no PA or frequency write and no invented quiescence success.
        let cleanup_error = self.executor.runtime_io().set_audio_mute(true).err();
        self.audio_open = false;
        self.fault = Some(Fault {
            reason,
            cleanup_error,
            state,
            ticket: self.ticket,
        });
        Error::Faulted
    }
    fn guard(&mut self) -> Result<(), Error> {
        match self.executor.runtime_io().check_receive_preconditions() {
            Ok(()) => Ok(()),
            Err(error) => Err(self.fail(Failure::Io {
                action: Action::Guard,
                error,
            })),
        }
    }
    fn write(&mut self, register: u8, value: u16) -> Result<(), Error> {
        self.guard()?;
        match self.executor.runtime_io().write(Chip::A, register, value) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.fail(Failure::Io {
                action: Action::Write(register),
                error,
            })),
        }
    }
    fn read(&mut self, register: u8) -> Result<u16, Error> {
        self.guard()?;
        match self.executor.runtime_io().read(Chip::A, register) {
            Ok(word) => Ok(word),
            Err(error) => Err(self.fail(Failure::Io {
                action: Action::Read(register),
                error,
            })),
        }
    }
    fn mute(&mut self, muted: bool) -> Result<(), Error> {
        self.guard()?;
        if let Err(error) = self.executor.runtime_io().set_audio_mute(muted) {
            return Err(self.fail(Failure::Io {
                action: Action::AudioMute(muted),
                error,
            }));
        }
        self.audio_open = !muted;
        Ok(())
    }

    /// Validated input only, no I/O. Retune is explicit after Listening;
    /// active work, terminal faults and stops cannot silently restart.
    pub fn start(&mut self, plan: Plan, now: u32) -> Result<Ticket, Error> {
        if self.terminal() {
            return Err(Error::Terminal);
        }
        if !matches!(self.state, State::Idle | State::Listening) {
            return Err(Error::Busy);
        }
        if self.state == State::Listening {
            self.advance_clock(now)?;
        }
        let Some(counter) = self.counter.checked_add(1) else {
            return Err(self.fail(Failure::CounterExhausted));
        };
        if self.executor.start(plan).is_err() {
            return Err(self.fail(Failure::InternalState));
        }
        self.counter = counter;
        let ticket = Ticket(counter);
        self.ticket = Some(ticket);
        self.state = State::Preflight;
        self.last_now = now;
        self.operation_left = self.timing.operation_timeout_ms;
        self.power_armed = false;
        self.settling_armed = false;
        self.settled = false;
        self.level = plan.level();
        // A previously open gate remains a physical possibility until MuteOutput.
        Ok(ticket)
    }

    fn advance_clock(&mut self, now: u32) -> Result<(), Error> {
        if self.terminal() {
            return Err(Error::Terminal);
        }
        if self.state == State::Idle {
            self.last_now = now;
            return Ok(());
        }
        let elapsed = now.wrapping_sub(self.last_now);
        if elapsed >= HALF_CLOCK {
            return Err(self.fail(Failure::Clock));
        }
        self.last_now = now;
        if self.state == State::AwaitSettling && !self.settling_armed {
            // `now` passed to StartHigh preceded its synchronous write. The
            // first later observation is a conservative POST-write baseline.
            if elapsed >= self.operation_left {
                return Err(self.fail(Failure::OperationTimeout));
            }
            self.operation_left -= elapsed;
            self.settling_armed = true;
            self.settling_left = self.timing.settling_timeout_ms;
            self.minimum_left = self.timing.min_settle_ms;
        } else if self.state == State::AwaitSettling {
            if elapsed >= self.settling_left {
                return Err(self.fail(Failure::SettlingTimeout));
            }
            self.settling_left -= elapsed;
            self.minimum_left = self.minimum_left.saturating_sub(elapsed);
        } else if self.state != State::Listening {
            if elapsed >= self.operation_left {
                return Err(self.fail(Failure::OperationTimeout));
            }
            self.operation_left -= elapsed;
            if self.power_armed {
                self.power_left = self.power_left.saturating_sub(elapsed);
            }
        }
        Ok(())
    }

    /// Each call is bounded. A configuration route step may read+write A33.
    /// Delays wait for caller time; no spin, sleep, retry or readiness synthesis.
    pub fn step(&mut self, now: u32) -> Result<Progress, Error> {
        self.advance_clock(now)?;
        match self.state {
            State::Idle => {}
            State::Preflight => {
                if self.executor.step().is_err() {
                    return Err(self.fail(Failure::Configuration));
                }
                self.state = State::MuteOutput;
            }
            State::MuteOutput => {
                self.mute(true)?;
                self.state = State::MuteInternal;
            }
            State::MuteInternal => {
                self.write(0x47, 0x6040)?;
                self.state = State::Configuring;
            }
            State::Configuring => {
                let configured = match self.executor.step() {
                    Ok(progress) => progress.status == executor::Status::Configured,
                    Err(_) => return Err(self.fail(Failure::Configuration)),
                };
                if configured {
                    self.state = State::PowerOn;
                }
            }
            State::PowerOn => {
                self.write(0x37, 0x1f0f)?;
                self.power_left = POWER_DELAY_MS;
                self.power_armed = false;
                self.state = State::PowerDelay;
            }
            State::PowerDelay => {
                self.guard()?;
                // As above, arm only at a later call after W37 completed.
                if !self.power_armed {
                    self.power_armed = true;
                    self.power_left = POWER_DELAY_MS;
                } else if self.power_left == 0 {
                    self.state = State::StartLow;
                }
            }
            State::StartLow => {
                self.write(0x30, 0x0200)?;
                self.state = State::StartHigh;
            }
            State::StartHigh => {
                self.write(0x30, 0xbff1)?;
                self.state = State::AwaitSettling;
                self.settling_armed = false;
                self.settling_left = self.timing.settling_timeout_ms;
                self.minimum_left = self.timing.min_settle_ms;
            }
            State::AwaitSettling => {
                self.guard()?;
                if self.settled && self.minimum_left == 0 {
                    self.state = State::Listening;
                }
            }
            State::Listening => self.guard()?,
            State::Stopped | State::Faulted => return Err(Error::Terminal),
        }
        Ok(self.progress())
    }

    /// Caller asserts independent settling evidence for the CURRENT generation.
    /// The configured minimum delay is also required. A timer alone never ACKs.
    /// Wrong-phase/stale acknowledgments do not alter clocks or deadlines.
    pub fn acknowledge_settled(&mut self, ticket: Ticket, now: u32) -> Result<bool, Error> {
        if self.terminal() {
            return Err(Error::Terminal);
        }
        if self.state != State::AwaitSettling || self.ticket != Some(ticket) {
            return Ok(false);
        }
        self.advance_clock(now)?;
        self.guard()?;
        self.settled = true;
        if self.minimum_left == 0 {
            self.state = State::Listening;
        }
        Ok(true)
    }

    fn listening(&mut self, now: u32) -> Result<(), Error> {
        if self.terminal() {
            return Err(Error::Terminal);
        }
        if self.state != State::Listening {
            return Err(Error::NotListening);
        }
        self.advance_clock(now)
    }
    /// Raw A67 RSSI and ordinary A0C bit1 squelch state; no dBm conversion,
    /// secondary-chip inference, monitor RAM override or automatic audio action.
    /// A final interlock rejects the sample if it detects PTT/fault after the
    /// reads; it cannot prove the absence of short pulses between observations.
    pub fn sample(&mut self, now: u32) -> Result<Signal, Error> {
        self.listening(now)?;
        let raw_rssi = self.read(0x67)? & 0x01ff;
        let squelch_open = self.level == 0 || (self.read(0x0c)? & 2 != 0);
        self.guard()?;
        let Some(ticket) = self.ticket else {
            return Err(self.fail(Failure::InternalState));
        };
        Ok(Signal {
            ticket,
            raw_rssi,
            squelch_open,
        })
    }
    /// Explicit FM audio request, for a caller-selected listening/monitor policy.
    /// External gate stays muted during internal mode and volume programming.
    /// This does not infer modulation from codeplug RX/TX operation-mode bits.
    pub fn open_fm_audio(&mut self, volume: Volume, now: u32) -> Result<(), Error> {
        self.listening(now)?;
        self.mute(true)?;
        self.write(0x47, 0x6140)?;
        let reg1 = self.read(0x01)?;
        self.write(0x48, volume_register_05768(volume.stored_word(), reg1))?;
        self.mute(false)?;
        self.guard()
    }
    pub fn mute_audio(&mut self, now: u32) -> Result<(), Error> {
        self.listening(now)?;
        self.mute(true)?;
        self.write(0x47, 0x6040)
    }
    /// Stop future requests even after a completed sweep. One external mute
    /// attempt; no RF bus access or claim of stopped RF. Never auto-restarts.
    pub fn stop(&mut self) -> Progress {
        if self.terminal() {
            return self.progress();
        }
        let state = self.state;
        self.executor.cancel();
        self.state = State::Stopped;
        self.audio_open = false;
        if let Err(error) = self.executor.runtime_io().set_audio_mute(true) {
            self.state = State::Faulted;
            self.fault = Some(Fault {
                reason: Failure::Io {
                    action: Action::AudioMute(true),
                    error,
                },
                cleanup_error: None,
                state,
                ticket: self.ticket,
            });
        }
        self.progress()
    }
}

impl<I: RuntimeIo> Drop for Runtime<I> {
    fn drop(&mut self) {
        // Explicit stop is required when cleanup errors must be inspected.
        // Forgetting it gets one bounded best-effort external mute request.
        // Inert/terminal lifetimes perform no additional I/O here.
        if self.state != State::Idle && !self.terminal() {
            let _ = self.stop();
        }
    }
}
