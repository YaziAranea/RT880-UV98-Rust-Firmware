//! Foreground ownership of manual DTMF, MDC1200 PTT ID and stock radio names.
//! Does not start RF. Every tone is inside the existing calibrated TX session.
use uv98_handheld_ui::controller::{Slot, View};
use uv98_radio_runtime::mdc1200::{self, Action, Config, Input, Mode};
use uv98_radio_runtime::{Fault, Io, Runtime, State};
use uv98_signaling::radio_name::RadioName;
use uv98_signaling::tones::{self, BkToneSession, ToneRegisters};

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

pub fn keypad(raw: u16) -> Option<u8> {
    if raw.count_ones() != 1 {
        return None;
    }
    match uv98_keys::stock_matrix_code(raw) {
        digit @ 0..=9 => Some(b'0' + digit),
        14 => Some(b'*'),
        15 => Some(b'#'),
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

pub struct TransmitTones {
    generator: BkToneSession,
    key: Option<u8>,
    key_since: u32,
    mdc: mdc1200::Session,
}
impl TransmitTones {
    pub const fn new() -> Self {
        Self {
            generator: BkToneSession::new(),
            key: None,
            key_since: 0,
            mdc: mdc1200::Session::new(),
        }
    }
    /// allow_tail requires clean physical release, unchanged TX configuration,
    /// and remaining ordinary TOT. Any new key/PTT or context change cancels it.
    pub fn step<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        now: u32,
        voice_held: bool,
        allow_tail: bool,
        key: Option<u8>,
        mdc_config: Config,
        name: Option<RadioName>,
    ) -> Result<bool, Fault<I::Error>> {
        if radio.state() != State::Transmitting {
            self.generator = BkToneSession::new();
            self.key = None;
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
        match self
            .mdc
            .step_with_name(now, input, mdc_config, name, radio.mdc_progress())
        {
            action @ (Action::Start(_) | Action::StartName(_)) => {
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
            Action::Keep => {
                radio.step_mdc(now)?;
                return Ok(true);
            }
            Action::RestoreVoice => {
                radio.resume_after_mdc()?;
                return Ok(true);
            }
            Action::StopTx => return Ok(false),
            Action::Idle => {}
        }
        if !voice_held {
            // No EOT burst. Let Runtime::tick shut down RF before restoring
            // the modulation baseline, including an interrupted DTMF tone.
            return Ok(false);
        }
        if self.key != key {
            self.key = key;
            self.key_since = now;
        }
        let tone = key
            .filter(|_| now.wrapping_sub(self.key_since) >= 12)
            .and_then(tones::dtmf);
        if let Some(tone) = tone {
            if !self.generator.active() {
                radio.signaling_microphone(false)?;
            }
            self.generator.set(&mut Registers(radio), tone)?;
        } else if self.generator.active() {
            self.generator.stop(&mut Registers(radio))?;
            radio.signaling_microphone(true)?;
        }
        Ok(true)
    }
}
