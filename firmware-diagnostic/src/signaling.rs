//! Foreground ownership of operator tones, MDC1200 PTT ID and stock radio names.
//! Does not start RF. Every tone is inside the existing calibrated TX session.
use uv98_handheld_ui::controller::{Slot, View};
use uv98_radio_runtime::mdc1200::{self, Action, Config, Input, Mode, Progress};
use uv98_radio_runtime::{Fault, Io, Runtime, State};
use uv98_signaling::radio_name::RadioName;
pub use uv98_signaling::tones::TxKey;
use uv98_signaling::tones::{BkToneSession, ToneRegisters, TxTones};

pub fn mdc_config(view: View, slot: Slot) -> Config {
    let settings = view.mdc_for_slot(slot);
    Config {
        mode: match settings.mode {
            1 => Mode::Begin,
            2 => Mode::End,
            3 => Mode::Both,
            _ => Mode::Off,
        },
        unit: settings.id.and_then(uv98_signaling::mdc1200::UnitId::new),
    }
}

/// Execute ready non-TX steps up to the caller's foreground admission
/// budget. Delay steps and control ownership are still owned by Runtime.
pub fn advance_receive<I: Io>(
    radio: &mut Runtime<I>,
    mut admit: impl FnMut() -> Option<u32>,
) -> Result<(), Fault<I::Error>> {
    for _ in 0..63 {
        if radio.tx_active_or_starting() {
            break;
        }
        let Some(now) = admit() else { break };
        if !radio.configuration_step_ready(now) {
            break;
        }
        radio.tick(now, false)?;
    }
    Ok(())
}

pub fn keypad(raw: u16) -> Option<TxKey> {
    if raw.count_ones() != 1 {
        return None;
    }
    match uv98_keys::stock_matrix_code(raw) {
        digit @ 0..=9 => Some(TxKey::Dtmf(b'0' + digit)),
        11 => Some(TxKey::Toneburst),
        12 => Some(TxKey::Memory),
        14 => Some(TxKey::Dtmf(b'*')),
        15 => Some(TxKey::Dtmf(b'#')),
        _ => None,
    }
}

struct Registers<'a, I: Io>(&'a mut Runtime<I>);
impl<I: Io> ToneRegisters for Registers<'_, I> {
    type Error = Fault<I::Error>;
    fn read(&mut self, register: u8) -> Result<u16, Self::Error> {
        self.0.signaling_read(register)
    }
    fn write(&mut self, register: u8, value: u16) -> Result<(), Self::Error> {
        self.0.signaling_write(register, value)
    }
}

const MODEM_STEPS_PER_PASS: usize = 8;
// A fresh keypad tone can need five reads, seven verified writes and MIC
// control. Reserve time for this bounded SPI batch before the runtime deadline.
const SIGNAL_START_MARGIN_MS: u32 = 20;

#[derive(Clone, Copy)]
pub struct Sample {
    pub now: u32,
    pub voice_held: bool,
    pub allow_tail: bool,
}

#[derive(Clone, Copy)]
pub struct ToneInput<'a> {
    pub key: Option<TxKey>,
    pub memory: &'a [u8],
    pub matrix_stable: bool,
}

pub struct TransmitTones {
    generator: BkToneSession,
    sequencer: TxTones,
    mic_muted: bool,
    timed_step_applied: bool,
    mdc: mdc1200::Session,
}
impl TransmitTones {
    pub const fn new() -> Self {
        Self {
            generator: BkToneSession::new(),
            sequencer: TxTones::new(),
            mic_muted: false,
            timed_step_applied: false,
            mdc: mdc1200::Session::new(),
        }
    }

    pub fn programmed_pending(&self, key: Option<TxKey>) -> bool {
        self.sequencer.mute_mic()
            || (self.mic_muted && !self.generator.active())
            || matches!(key, Some(TxKey::Toneburst | TxKey::Memory))
    }

    pub fn applied_at(&mut self, now: u32) {
        if core::mem::take(&mut self.timed_step_applied) {
            self.sequencer.applied_at(now);
        }
    }
    /// Consume a bounded slice of setup/restoration before other foreground
    /// work. The first sample has already been taken. Each extra modem
    /// operation needs its own fresh physical and TOT guard. A gate closing
    /// returns a negative sample, while None ends only the current slice.
    pub fn step_pass<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        first: Sample,
        tones: ToneInput<'_>,
        config: Config,
        name: Option<RadioName>,
        mut sample: impl FnMut() -> Option<Sample>,
    ) -> Result<bool, Fault<I::Error>> {
        let step = |session: &mut Self, radio: &mut Runtime<I>, at: Sample| {
            session.step(radio, at, tones, config, name)
        };
        let mut keep = step(self, radio, first)?;
        for _ in 1..MODEM_STEPS_PER_PASS {
            if !keep
                || !matches!(
                    radio.mdc_progress(),
                    Some(Progress::Preparing | Progress::Restoring)
                )
            {
                break;
            }
            let Some(at) = sample() else {
                break;
            };
            keep = step(self, radio, at)?;
        }
        Ok(keep)
    }

    /// allow_tail requires clean physical release, unchanged TX configuration,
    /// and remaining ordinary TOT. Any new key/PTT or context change cancels it.
    pub fn step<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        sample: Sample,
        tones: ToneInput<'_>,
        mdc_config: Config,
        name: Option<RadioName>,
    ) -> Result<bool, Fault<I::Error>> {
        let Sample {
            now,
            voice_held,
            allow_tail,
        } = sample;
        if radio.state() != State::Transmitting {
            self.generator = BkToneSession::new();
            self.sequencer = TxTones::new();
            self.mic_muted = false;
            self.timed_step_applied = false;
            self.mdc = mdc1200::Session::new();
            return Ok(voice_held);
        }
        let input = if voice_held {
            Input::Voice
        } else if allow_tail {
            Input::Released
        } else {
            Input::Abort
        };
        let can_start_signal = radio
            .tx_timer()
            .is_some_and(|timer| timer.remaining_ms(now) > SIGNAL_START_MARGIN_MS);
        match self
            .mdc
            .step_with_name(now, input, mdc_config, name, radio.mdc_progress())
        {
            Action::Start(_) | Action::StartName(_) if !can_start_signal => return Ok(false),
            action @ (Action::Start(_) | Action::StartName(_)) => {
                self.sequencer.cancel();
                // DTMF and FFSK share Tone2. Stop an active keypad tone in
                // one verified write before the end burst takes ownership.
                // Terminal TX cleanup restores the shared modulation state.
                if self.generator.active() {
                    radio.signaling_write(0x70, 0)?;
                }
                match action {
                    Action::Start(id) => radio.start_mdc(id)?,
                    Action::StartName(name) => radio.start_radio_name(name)?,
                    _ => unreachable!(),
                }
                return Ok(true);
            }
            Action::Keep
                if !can_start_signal && radio.mdc_progress() == Some(Progress::Preparing) =>
            {
                return Ok(false);
            }
            Action::Keep => {
                radio.step_mdc(now)?;
                return Ok(true);
            }
            Action::RestoreVoice => {
                radio.resume_after_mdc()?;
                self.sequencer = TxTones::new();
                self.mic_muted = false;
                return Ok(true);
            }
            Action::StopTx => return Ok(false),
            Action::Idle => {}
        }
        if !voice_held {
            // No EOT burst. Let Runtime::tick shut down RF before restoring
            // the modulation baseline, including an interrupted tone or gap.
            self.sequencer.cancel();
            return Ok(false);
        }
        let was_programmed = self.sequencer.mute_mic();
        if !tones.matrix_stable {
            self.sequencer.interrupt_key_debounce();
        }
        if !tones.matrix_stable && (was_programmed || (self.mic_muted && !self.generator.active()))
        {
            // The short ambiguous-matrix bridge may retain already qualified
            // PTT. End an expired mark, but do not start another or open MIC.
            if self.generator.active() && self.sequencer.mark_expired(now) {
                self.generator.stop(&mut Registers(radio))?;
            }
            return Ok(true);
        }
        let tone = if tones.matrix_stable {
            self.sequencer.sample(now, tones.key, tones.memory)
        } else {
            // A synthesized empty matrix can preserve voice briefly, but
            // cannot admit, rearm or continue a programmed tone sequence.
            None
        };
        if tone.is_some() && !can_start_signal {
            // Already keyed voice may continue to the deadline. Do not make
            // another generator write that could start a new tone beyond it.
            return Ok(true);
        }
        let programmed = self.sequencer.mute_mic();
        let mute_mic = tone.is_some() || programmed;
        let tone_changed = tone.is_some() != self.generator.active();
        if mute_mic && !self.mic_muted {
            radio.signaling_microphone(false)?;
            self.mic_muted = true;
        }
        if let Some(tone) = tone {
            self.generator.set(&mut Registers(radio), tone)?;
        } else if self.generator.active() {
            self.generator.stop(&mut Registers(radio))?;
            if was_programmed && !programmed {
                // A fresh foreground sample must precede opening MIC again.
                return Ok(true);
            }
        }
        self.timed_step_applied |= programmed && tone_changed;
        if !mute_mic && self.mic_muted {
            radio.signaling_microphone(true)?;
            self.mic_muted = false;
        }
        Ok(true)
    }
}
