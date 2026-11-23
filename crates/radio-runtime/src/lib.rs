#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Bounded foreground radio sequencing. Timers are elapsed time, not PLL-lock
//! claims. A caller must provide fresh physical PTT, validated channel policy,
//! independent terminal shutdown, and a watchdog before enabling live TX.

use uv98_calibration::Calibration;
use uv98_rf_sequence::{self as sequence, Modulation, Plan, RxRequest, Step, TxRequest};

pub mod capabilities;
pub mod crossband;
mod dtmf_rx;
pub mod lighting;
pub mod mdc1200;
pub mod policy;
pub mod ptt;
mod radio_name;
pub mod scan;
pub mod scope;
pub mod shortwave;
pub mod shortwave_scope;
pub mod spectrum_flow;
pub mod sweep;
pub mod tone_search;
pub mod tx_policy;
pub mod tx_trial;
pub mod watch;

pub const CONFIG_TIMEOUT_MS: u32 = 3000;
pub const RECEIVE_SETTLE_MS: u32 = 10;
pub const TX_TIMEOUT_MS: u32 = 30_000;

/// TX configuration, voice, and end signaling share this deadline.
#[derive(Clone, Copy)]
pub struct TxTimer {
    start_ms: u32,
    limit_ms: u32,
}

impl TxTimer {
    fn new(start_ms: u32, requested_ms: u32) -> Self {
        Self {
            start_ms,
            limit_ms: requested_ms.min(TX_TIMEOUT_MS),
        }
    }

    pub const fn start_ms(self) -> u32 {
        self.start_ms
    }

    pub fn remaining_ms(self, now: u32) -> u32 {
        self.limit_ms
            .saturating_sub(now.wrapping_sub(self.start_ms))
    }

    pub fn remaining_seconds(self, now: u32) -> u8 {
        self.remaining_ms(now).div_ceil(1000) as u8
    }
}

// Beken BK4819 Application Note v1.0, §12. Bit 1 enables the TX DSP.
const MIC_ADC: u16 = 1 << 2;

#[cfg(any(feature = "hardware", test))]
mod gpio_control;
#[cfg(feature = "hardware")]
pub mod hardware;

pub trait Io {
    type Error;
    fn read(&mut self, register: u8) -> Result<u16, Self::Error>;
    fn write(&mut self, register: u8, value: u16) -> Result<(), Self::Error>;
    fn mute(&mut self, muted: bool) -> Result<(), Self::Error>;
    fn antenna(&mut self, high: bool) -> Result<(), Self::Error>;
    fn hf_gates(&mut self, hf: bool, band50: bool) -> Result<(), Self::Error>;
    fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), Self::Error>;
    fn backlights(&mut self, display: bool, keys: bool) -> Result<(), Self::Error>;
    fn verify_control(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off,
    Configuring,
    Receiving,
    Sweeping,
    Transmitting,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Cold,
    Receive,
    ToneSearch,
    Transmit,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    State,
    NotInitialized,
    PttReleased,
    Clock,
    ConfigurationTimeout,
    Request(sequence::Error),
    Io(E),
    Readback {
        register: u8,
        expected: u16,
        observed: u16,
    },
    InvalidAudio,
    Sweep(sequence::sweep::Error),
}
#[derive(Debug, PartialEq, Eq)]
pub struct Fault<E> {
    pub reason: Error<E>,
    pub cleanup_attempted: bool,
    /// All independent off requests are attempted, even if one fails.
    pub cleanup_failures: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Poll {
    pub state: State,
    pub changed: bool,
    pub tx_ended: bool,
    pub tx_timed_out: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signal {
    pub rssi: u16,
    pub squelch_open: bool,
    pub audio_open: bool,
    pub status_0c: u16,
    pub am_gain_index: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiveDiagnostics {
    pub gain_selection: u16,
    pub gain: u16,
    pub af_mode: u16,
    pub volume: u16,
    pub status01: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Audio {
    pub modulation: Modulation,
    pub gain: u8,
    pub dac: u8,
    pub squelch: u8,
    pub monitor: bool,
    pub tone_required: bool,
}

#[derive(Default)]
struct SignalingInterrupts {
    dtmf: Option<dtmf_rx::Detected>,
    name_crc: Option<bool>,
}

pub struct Runtime<I: Io> {
    io: I,
    state: State,
    initialized: bool,
    plan: Option<Plan>,
    target: Target,
    index: usize,
    started: u32,
    last_now: Option<u32>,
    wait: Option<(u32, u32)>,
    finishing: bool,
    tx_timer: Option<TxTimer>,
    audio_open: bool,
    am_agc: sequence::am_agc::State,
    am_gain_word: Option<u16>,
    am_gain_at: Option<u32>,
    am_carrier_at: Option<u32>,
    sweep_session: Option<sweep::Session>,
    indicator_state: Option<(bool, bool)>,
    backlight_state: Option<(bool, bool)>,
    signaling_baseline: Option<(u16, u16)>,
    dtmf_rx: dtmf_rx::Receiver,
    name_rx: radio_name::Receiver,
    mdc_modem: Option<mdc1200::Modem>,
}

impl<I: Io> Runtime<I> {
    pub fn new(io: I) -> Self {
        Self {
            io,
            state: State::Off,
            initialized: false,
            plan: None,
            target: Target::Cold,
            index: 0,
            started: 0,
            last_now: None,
            wait: None,
            finishing: false,
            tx_timer: None,
            audio_open: false,
            am_agc: sequence::am_agc::State::new(),
            am_gain_word: None,
            am_gain_at: None,
            am_carrier_at: None,
            sweep_session: None,
            indicator_state: None,
            backlight_state: None,
            signaling_baseline: None,
            dtmf_rx: dtmf_rx::Receiver::default(),
            name_rx: radio_name::Receiver::default(),
            mdc_modem: None,
        }
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn initialized(&self) -> bool {
        self.initialized
    }
    pub fn audio_open(&self) -> bool {
        self.audio_open
    }

    /// Sample the receiver's physical squelch flag without changing audio or IRQs.
    pub fn carrier_present(&mut self) -> Result<bool, Fault<I::Error>> {
        Ok(self.state == State::Receiving && self.read(0x0c)? & 2 != 0)
    }
    /// Foreground light policy changes only the already-owned PA4/PA9 pins.
    /// A GPIO fault follows the same RF shutdown path as other control faults.
    pub fn set_backlights(&mut self, display: bool, keys: bool) -> Result<(), Fault<I::Error>> {
        if self.state == State::Failed {
            return Err(self.reject(Error::State));
        }
        if self.backlight_state != Some((display, keys)) {
            let result = self.io.backlights(display, keys);
            self.io_result(result)?;
            self.backlight_state = Some((display, keys));
        }
        Ok(())
    }
    pub fn synchronize_indicators(&mut self) -> Result<(), Fault<I::Error>> {
        let desired = (
            self.state == State::Receiving && self.audio_open,
            self.state == State::Transmitting,
        );
        if self.indicator_state != Some(desired) {
            let result = self.io.indicators(desired.0, desired.1);
            self.io_result(result)?;
            self.indicator_state = Some(desired);
        }
        Ok(())
    }
    /// Readbacks for the RX trial only, never a claim of RF sensitivity.
    pub fn receive_diagnostics(&mut self) -> Result<ReceiveDiagnostics, Fault<I::Error>> {
        if self.state != State::Receiving {
            return Err(self.reject(Error::State));
        }
        Ok(ReceiveDiagnostics {
            gain_selection: self.read(0x7e)?,
            gain: self.read(0x13)?,
            af_mode: self.read(0x47)?,
            volume: self.read(0x48)?,
            status01: self.read(0x01)?,
        })
    }
    /// A new upper PTT can be rechecked while the voice receiver is tuning.
    pub fn rx_active_or_starting(&self) -> bool {
        self.state == State::Receiving
            || (self.state == State::Configuring && self.target == Target::Receive)
    }
    pub fn tx_active_or_starting(&self) -> bool {
        self.state == State::Transmitting
            || (self.state == State::Configuring && self.target == Target::Transmit)
    }
    /// A caller can execute another bounded configuration operation without
    /// spending an entire UI pass on a register write. Waiting never advances
    /// a delay; the next tick still checks the clock, GPIO ownership and PTT.
    pub fn configuration_step_ready(&self, now: u32) -> bool {
        self.state == State::Configuring
            && self
                .wait
                .is_none_or(|(since, delay)| now.wrapping_sub(since) >= delay)
    }
    pub fn into_io(self) -> I {
        self.io
    }

    fn advance_clock(&mut self, now: u32) -> Result<(), Fault<I::Error>> {
        if self
            .last_now
            .is_some_and(|last| now.wrapping_sub(last) >= 1 << 31)
        {
            return Err(self.fail(Error::Clock));
        }
        self.last_now = Some(now);
        Ok(())
    }
    fn cleanup(&mut self) -> u8 {
        let mut errors = 0;
        if self.io.mute(true).is_err() {
            errors |= 1;
        }
        if self.io.hf_gates(false, false).is_err() {
            errors |= 2;
        }
        // A33=0 is intentionally stronger than a read/modify/write: terminal
        // cleanup cannot retain an inherited or unreadable V/U TX gate.
        for (bit, reg, word) in [
            (2, 0x33, 0),
            (3, 0x36, 0x007f),
            (4, 0x30, 0),
            (5, 0x47, 0x6040),
        ] {
            if self.io.write(reg, word).is_err() {
                errors |= 1 << bit;
            }
        }
        if self.io.indicators(false, false).is_err() {
            errors |= 1 << 6;
            self.indicator_state = None;
        } else {
            self.indicator_state = Some((false, false));
        }
        if self.cleanup_mdc() {
            // Bit7 groups independent modulation cleanup requests.
            errors |= 1 << 7;
        }
        if self.restore_name_rx().is_err() {
            errors |= 1 << 7;
        }
        if self.restore_dtmf_rx().is_err() {
            errors |= 1 << 7;
        }
        if let Some((reg50, reg70)) = self.signaling_baseline.take() {
            // PA/gates/DSP are already requested off. A release, TOT or fault
            // may bypass the foreground tone-session restore; never carry an
            // enabled Tone2 into RX (the stock option helper preserves it).
            // Keep gain/filter bits but clear both generator enables and mute.
            // Bit7 groups these two independent modulation cleanup writes.
            if self.io.write(0x70, reg70 & !0x8080).is_err() {
                errors |= 1 << 7;
            }
            if self.io.write(0x50, reg50 & !0x8000).is_err() {
                errors |= 1 << 7;
            }
        }
        self.audio_open = false;
        self.am_gain_word = None;
        self.am_gain_at = None;
        self.am_carrier_at = None;
        self.sweep_session = None;
        self.plan = None;
        self.wait = None;
        self.finishing = false;
        self.tx_timer = None;
        errors
    }
    fn fail(&mut self, reason: Error<I::Error>) -> Fault<I::Error> {
        self.state = State::Failed;
        let cleanup_failures = self.cleanup();
        Fault {
            reason,
            cleanup_attempted: true,
            cleanup_failures,
        }
    }
    fn reject(&mut self, reason: Error<I::Error>) -> Fault<I::Error> {
        // An invalid/reentrant request must not leave an earlier TX running.
        if self.tx_active_or_starting()
            || matches!(self.state, State::Configuring | State::Sweeping)
        {
            self.fail(reason)
        } else {
            Fault {
                reason,
                cleanup_attempted: false,
                cleanup_failures: 0,
            }
        }
    }
    fn io_result<T>(&mut self, result: Result<T, I::Error>) -> Result<T, Fault<I::Error>> {
        result.map_err(|error| self.fail(Error::Io(error)))
    }
    fn read(&mut self, reg: u8) -> Result<u16, Fault<I::Error>> {
        let result = self.io.read(reg);
        self.io_result(result)
    }
    fn write(&mut self, reg: u8, word: u16) -> Result<(), Fault<I::Error>> {
        let result = self.io.write(reg, word);
        self.io_result(result)?;
        if matches!(reg, 0x30 | 0x33 | 0x36 | 0x38 | 0x39) {
            let actual = self.read(reg)?;
            if actual != word {
                return Err(self.fail(Error::Readback {
                    register: reg,
                    expected: word,
                    observed: actual,
                }));
            }
        }
        Ok(())
    }

    /// Restricted tone-generator access during an already-authorized TX.
    /// This cannot tune RF, select a route, alter power or start transmission.
    /// The caller restores saved audio registers before normal TX shutdown.
    pub fn signaling_read(&mut self, register: u8) -> Result<u16, Fault<I::Error>> {
        if self.state != State::Transmitting
            || !matches!(register, 0x47 | 0x50 | 0x70 | 0x71 | 0x72)
        {
            return Err(self.reject(Error::State));
        }
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        self.read(register)
    }

    pub fn signaling_write(&mut self, register: u8, value: u16) -> Result<(), Fault<I::Error>> {
        if self.state != State::Transmitting
            || !matches!(register, 0x47 | 0x50 | 0x70 | 0x71 | 0x72)
        {
            return Err(self.reject(Error::State));
        }
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        if self.signaling_baseline.is_none() {
            let reg50 = self.read(0x50)?;
            let reg70 = self.read(0x70)?;
            self.signaling_baseline = Some((reg50, reg70));
        }
        self.write(register, value)?;
        let observed = self.read(register)?;
        if observed != value {
            return Err(self.fail(Error::Readback {
                register,
                expected: value,
                observed,
            }));
        }
        Ok(())
    }

    /// REG30 bit 2 controls the microphone ADC. Keep RF/TX enable bits intact
    /// while replacing speech with a tone, then restore normal speech input.
    pub fn signaling_microphone(&mut self, enabled: bool) -> Result<(), Fault<I::Error>> {
        if self.state != State::Transmitting {
            return Err(self.reject(Error::State));
        }
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        let word = self.read(0x30)?;
        self.write(0x30, (word & !MIC_ADC) | if enabled { MIC_ADC } else { 0 })
    }
    fn gates(&mut self, route: u8) -> Result<(), Fault<I::Error>> {
        let result = self.io.hf_gates(false, false);
        self.io_result(result)?;
        let mut word = self.read(0x33)? & 0xfff3;
        match route {
            1 | 2 => {
                let result = self.io.hf_gates(true, route == 2);
                self.io_result(result)?;
            }
            3 => word |= 8,
            4 | 5 => word |= 4,
            _ => {}
        }
        self.write(0x33, word)
    }
    fn start(&mut self, plan: Plan, target: Target, now: u32) -> Result<(), Fault<I::Error>> {
        if matches!(
            self.state,
            State::Failed | State::Configuring | State::Transmitting | State::Sweeping
        ) {
            return Err(self.reject(Error::State));
        }
        self.advance_clock(now)?;
        self.stop_rx_signaling()?;
        self.state = State::Configuring;
        self.target = target;
        self.plan = Some(plan);
        self.index = 0;
        self.started = now;
        self.wait = None;
        self.finishing = false;
        Ok(())
    }
    pub fn cold_start(
        &mut self,
        calibration: &Calibration<'_>,
        now: u32,
    ) -> Result<(), Fault<I::Error>> {
        if self.initialized {
            return Err(self.reject(Error::State));
        }
        self.start(sequence::cold_init(calibration), Target::Cold, now)
    }
    pub fn receive(
        &mut self,
        calibration: &Calibration<'_>,
        request: RxRequest,
        now: u32,
    ) -> Result<(), Fault<I::Error>> {
        self.receive_inner(calibration, request, now, false)
    }

    pub fn receive_tone_search(
        &mut self,
        calibration: &Calibration<'_>,
        request: RxRequest,
        now: u32,
    ) -> Result<(), Fault<I::Error>> {
        self.receive_inner(calibration, request, now, true)
    }

    fn receive_inner(
        &mut self,
        calibration: &Calibration<'_>,
        request: RxRequest,
        now: u32,
        tone_search: bool,
    ) -> Result<(), Fault<I::Error>> {
        if !self.initialized {
            return Err(self.reject(Error::NotInitialized));
        }
        let plan = if tone_search {
            sequence::receive_tone_search(calibration, request)
        } else {
            sequence::receive(calibration, request)
        }
        .map_err(|e| self.reject(Error::Request(e)))?;
        self.am_agc = sequence::am_agc::State::new();
        self.am_gain_word = None;
        self.am_gain_at = None;
        self.am_carrier_at = None;
        self.start(
            plan,
            if tone_search {
                Target::ToneSearch
            } else {
                Target::Receive
            },
            now,
        )
    }

    /// Decode only the scan registers, between two actual carrier samples.
    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn read_rx_tone(&mut self) -> Result<tone_search::Observation, Fault<I::Error>> {
        use tone_search::Observation;
        if self.state != State::Receiving || self.target != Target::ToneSearch {
            return Err(self.reject(Error::State));
        }
        if self.read(0x0c)? & 2 == 0 {
            return Ok(Observation::NoCarrier);
        }
        let high = self.read(0x69)?;
        let dcs = if high & 0xc000 == 0 {
            let low = self.read(0x6a)?;
            (self.read(0x69)? == high)
                .then(|| tone_search::decode_dcs(high, low))
                .flatten()
        } else {
            None
        };
        let tone = if dcs.is_some() {
            dcs
        } else {
            tone_search::decode_ctcss(self.read(0x68)?)
        };
        Ok(if self.read(0x0c)? & 2 == 0 {
            Observation::NoCarrier
        } else {
            tone.map_or(Observation::NoTone, Observation::Tone)
        })
    }
    /// A fresh PTT edge, channel permission and calibration identity must be
    /// checked by the caller. tick additionally requires physical hold each time.
    pub fn transmit(
        &mut self,
        calibration: &Calibration<'_>,
        request: TxRequest,
        now: u32,
        held: bool,
    ) -> Result<(), Fault<I::Error>> {
        self.transmit_with_mdc(calibration, request, now, held, mdc1200::Config::default())
    }

    /// Keep MIC ADC off from the first carrier if a valid MDC beginning
    /// burst is configured. The foreground owns scheduling and PTT checks.
    pub fn transmit_with_mdc(
        &mut self,
        calibration: &Calibration<'_>,
        request: TxRequest,
        now: u32,
        held: bool,
        config: mdc1200::Config,
    ) -> Result<(), Fault<I::Error>> {
        self.transmit_with_mdc_limit(calibration, request, now, held, config, TX_TIMEOUT_MS)
    }

    pub fn transmit_with_mdc_limit(
        &mut self,
        calibration: &Calibration<'_>,
        request: TxRequest,
        now: u32,
        held: bool,
        config: mdc1200::Config,
        timeout_ms: u32,
    ) -> Result<(), Fault<I::Error>> {
        if !self.initialized {
            return Err(self.reject(Error::NotInitialized));
        }
        if !held {
            return Err(self.reject(Error::PttReleased));
        }
        let plan =
            sequence::transmit_with_microphone(calibration, request, config.begin().is_none())
                .map_err(|e| self.reject(Error::Request(e)))?;
        self.start(plan, Target::Transmit, now)?;
        self.tx_timer = Some(TxTimer::new(now, timeout_ms));
        Ok(())
    }

    pub fn tx_timer(&self) -> Option<TxTimer> {
        self.tx_timer
    }
    pub fn stop(&mut self) -> Result<(), Fault<I::Error>> {
        if self.state == State::Failed {
            return Err(self.reject(Error::State));
        }
        let cleanup_failures = self.cleanup();
        if cleanup_failures != 0 {
            self.state = State::Failed;
            return Err(Fault {
                reason: Error::State,
                cleanup_attempted: true,
                cleanup_failures,
            });
        }
        self.state = State::Off;
        // Verify the PA/gate/control words after all stop writes.
        for (reg, word) in [(0x33, 0), (0x36, 0x007f), (0x30, 0)] {
            let actual = self.read(reg)?;
            if actual != word {
                return Err(self.fail(Error::Readback {
                    register: reg,
                    expected: word,
                    observed: actual,
                }));
            }
        }
        Ok(())
    }
    pub fn tick(&mut self, now: u32, physical_tx_held: bool) -> Result<Poll, Fault<I::Error>> {
        if self.state == State::Failed {
            return Err(self.reject(Error::State));
        }
        self.advance_clock(now)?;
        let before = self.state;
        if self.tx_active_or_starting() {
            let expired = self
                .tx_timer
                .is_none_or(|timer| timer.remaining_ms(now) == 0);
            if !physical_tx_held || expired {
                self.stop()?;
                return Ok(Poll {
                    state: self.state,
                    changed: true,
                    tx_ended: true,
                    tx_timed_out: expired,
                });
            }
        }
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        if self.state == State::Configuring {
            if now.wrapping_sub(self.started) >= CONFIG_TIMEOUT_MS {
                return Err(self.fail(Error::ConfigurationTimeout));
            }
            if let Some((since, delay)) = self.wait {
                if now.wrapping_sub(since) < delay {
                    return Ok(Poll {
                        state: self.state,
                        changed: false,
                        tx_ended: false,
                        tx_timed_out: false,
                    });
                }
                self.wait = None;
            }
            if self.finishing {
                self.plan = None;
                self.finishing = false;
                self.state = match self.target {
                    Target::Cold => {
                        self.initialized = true;
                        State::Off
                    }
                    Target::Receive | Target::ToneSearch => State::Receiving,
                    Target::Transmit => State::Transmitting,
                };
            } else if let Some(step) = self
                .plan
                .as_ref()
                .and_then(|p| p.operations().get(self.index))
                .copied()
            {
                match step {
                    Step::WriteA { register, value } => self.write(register, value)?,
                    Step::UpdateA {
                        register,
                        keep_mask,
                        value,
                    } => {
                        let old = self.read(register)?;
                        self.write(register, (old & keep_mask) | value)?;
                    }
                    Step::DelayMs(delay) => self.wait = Some((now, u32::from(delay))),
                    Step::Mute(muted) => {
                        let result = self.io.mute(muted);
                        self.io_result(result)?;
                        self.audio_open = !muted;
                    }
                    Step::AntennaPf0(high) => {
                        let result = self.io.antenna(high);
                        self.io_result(result)?;
                    }
                    Step::TxGates(route) => self.gates(route)?,
                }
                self.index += 1;
            } else {
                // This fresh observation follows the final synchronous write.
                self.finishing = true;
                self.wait = Some((
                    now,
                    match self.target {
                        Target::Receive => RECEIVE_SETTLE_MS,
                        // An intentional restart briefly closes raw squelch.
                        // Ignore the settling interval before observing carrier.
                        Target::ToneSearch => 70,
                        _ => 0,
                    },
                ));
            }
        }
        Ok(Poll {
            state: self.state,
            changed: before != self.state,
            tx_ended: false,
            tx_timed_out: false,
        })
    }
    fn stop_rx_signaling(&mut self) -> Result<(), Fault<I::Error>> {
        // Name RX is armed after DTMF and preserves its live IRQ mask.
        self.stop_name_rx()?;
        self.stop_dtmf_rx()
    }

    /// tone_detected is an independently observed decoder state. Unknown never
    /// opens a tone-gated channel; explicit monitor bypasses both SQL and tones.
    pub fn signal(
        &mut self,
        audio: Audio,
        tone_detected: Option<bool>,
    ) -> Result<Signal, Fault<I::Error>> {
        if self.state == State::Receiving {
            self.stop_rx_signaling()?;
        }
        let (status, _) = self.receive_status(false, false)?;
        self.signal_at(audio, tone_detected, status)
    }

    fn receive_status(
        &mut self,
        capture_dtmf: bool,
        capture_name: bool,
    ) -> Result<(u16, SignalingInterrupts), Fault<I::Error>> {
        if self.state != State::Receiving {
            return Err(self.reject(Error::State));
        }
        let mut status = self.read(0x0c)?;
        let mut detected = SignalingInterrupts::default();
        if status & 1 != 0 {
            // Stock samples CRC before acknowledgement. This can be stale
            // if FSK finishes while an unrelated IRQ is already pending.
            let flags_before = if capture_name || !capture_dtmf {
                self.read(0x0b)?
            } else {
                0
            };
            self.write(0x02, 0)?;
            let causes = if capture_dtmf || capture_name {
                self.read(0x02)?
            } else {
                0
            };
            let dtmf_found = capture_dtmf && causes & dtmf_rx::DTMF_IRQ != 0;
            let fsk_finished = capture_name && causes & radio_name::RX_FINISHED_IRQ != 0;
            let flags_after = if dtmf_found || fsk_finished {
                self.read(0x0b)?
            } else {
                0
            };
            if dtmf_found {
                detected.dtmf = Some(dtmf_rx::Detected {
                    code: ((flags_after >> 8) & 0xf) as u8,
                    carrier_before_ack: status & 2 != 0,
                });
            }
            // A new pending IRQ can make the post-ACK flags belong to a
            // newer packet. Neither ambiguous sample qualifies a frame.
            status = self.read(0x0c)?;
            if fsk_finished {
                detected.name_crc =
                    Some(status & 1 == 0 && flags_before & flags_after & radio_name::CRC_PASS != 0);
            }
        }
        Ok((status, detected))
    }
    /// Hardware entry: fresh status and bounded IRQ acknowledgement, then the
    /// stock tone decision. Call at most once per foreground loop, no sooner
    /// than 1ms after the preceding observation. No catch-up burst. Stock's
    /// gain hold counts calls, not wall-clock milliseconds.
    pub fn observe(&mut self, audio: Audio, raw_tone: u16) -> Result<Signal, Fault<I::Error>> {
        self.observe_inner(audio, raw_tone, false, false)
    }

    /// Observe ordinary primary BK4819 FM receive with DTMF digit capture.
    /// Only one latched event is serviced per observation. The first FM RX
    /// after startup loads 16 decoder coefficients one at a time, then arms
    /// capture for future samples. Preparation runs after the audio update.
    /// Call `observe` for modes where the decoder should be inactive.
    pub fn observe_dtmf(&mut self, audio: Audio, raw_tone: u16) -> Result<Signal, Fault<I::Error>> {
        self.observe_inner(audio, raw_tone, audio.modulation == Modulation::Fm, false)
    }

    fn observe_inner(
        &mut self,
        mut audio: Audio,
        raw_tone: u16,
        dtmf: bool,
        name: bool,
    ) -> Result<Signal, Fault<I::Error>> {
        if self.state != State::Receiving {
            return Err(self.reject(Error::State));
        }
        if !name {
            self.stop_name_rx()?;
        }
        if !dtmf {
            self.stop_dtmf_rx()?;
        }
        let capture = dtmf && self.dtmf_rx.armed();
        let capture_name = name && self.name_rx.armed();
        let previously_open = self.audio_open;
        let (status, detected) = self.receive_status(capture, capture_name)?;
        audio.tone_required = raw_tone != 0;
        let mut result = self.signal_at(
            audio,
            Some(sequence::tone_matches(status, raw_tone)),
            status,
        )?;
        if dtmf {
            if !capture {
                self.prepare_dtmf_rx()?;
            } else if let Some(now) = self.last_now {
                self.dtmf_rx.text.expire(now);
                if let Some(symbol) = detected.dtmf {
                    // The last digit may arrive just before carrier loss.
                    // Require a fresh carrier sample even if audio was open.
                    let qualified = if status & 2 != 0 {
                        result.audio_open
                    } else {
                        previously_open && symbol.carrier_before_ack
                    };
                    if qualified {
                        self.dtmf_rx.text.push(symbol.code, now);
                    }
                }
            }
        }
        if let Some(crc_ok) = detected.name_crc {
            self.finish_name_rx(crc_ok)?;
        } else if name && !capture_name && self.dtmf_rx.armed() {
            self.prepare_name_rx()?;
        }
        if audio.modulation == Modulation::Am && result.audio_open {
            // Count elapsed time during the hold even if LCD or NOR work
            // delayed this sample. Still issue at most one gain update.
            if let (Some(now), Some(previous)) = (self.last_now, self.am_gain_at) {
                let skipped = now.wrapping_sub(previous).saturating_sub(1).min(250) as u8;
                self.am_agc.hold = self.am_agc.hold.saturating_sub(skipped);
            }
            self.am_gain_at = self.last_now;
            let update = self
                .am_agc
                .sample(result.rssi)
                .map_err(|_| self.fail(Error::InvalidAudio))?;
            // Update the stock AGC state on every fresh sample, but avoid a
            // redundant bus transaction when the selected gain is unchanged.
            // Reset/retune/cleanup invalidate this cache; no other ordinary
            // receiver operation writes A13 behind it.
            if self.am_gain_word != Some(update.register13) {
                self.write(0x13, update.register13)?;
                self.am_gain_word = Some(update.register13);
            }
            self.am_agc = update.state;
            result.am_gain_index = update.state.index;
        }
        Ok(result)
    }
    fn signal_at(
        &mut self,
        audio: Audio,
        tone_detected: Option<bool>,
        status: u16,
    ) -> Result<Signal, Fault<I::Error>> {
        if self.state != State::Receiving {
            return Err(self.reject(Error::State));
        }
        if audio.gain > 63 || audio.dac > 15 || audio.squelch > 10 {
            return Err(self.fail(Error::InvalidAudio));
        }
        let rssi = self.read(0x67)? & 0x01ff;
        let squelch_open = audio.squelch == 0 || status & 2 != 0;
        // AM envelope dips can briefly close the noise gate between
        // syllables. Extend only a real carrier on a tone-free AM channel.
        let am_hang = if audio.modulation == Modulation::Am && !audio.tone_required {
            if squelch_open {
                self.am_carrier_at = self.last_now;
            }
            self.last_now
                .zip(self.am_carrier_at)
                .is_some_and(|(now, seen)| now.wrapping_sub(seen) < 120)
        } else {
            self.am_carrier_at = None;
            false
        };
        let open = audio.monitor
            || am_hang
            || (squelch_open && (!audio.tone_required || tone_detected == Some(true)));
        if open != self.audio_open {
            let result = self.io.mute(true);
            self.io_result(result)?;
            if open {
                self.write(0x47, sequence::audio_mode_word(audio.modulation))?;
                let reg1 = self.read(1)?;
                self.write(
                    0x48,
                    sequence::audio_volume_word(audio.gain, audio.dac, reg1),
                )?;
                let result = self.io.mute(false);
                self.io_result(result)?;
            } else {
                self.write(0x47, 0x6040)?;
            }
            self.audio_open = open;
        }
        Ok(Signal {
            rssi,
            squelch_open,
            audio_open: open,
            status_0c: status,
            am_gain_index: 0,
        })
    }
}

#[cfg(test)]
mod tests;
