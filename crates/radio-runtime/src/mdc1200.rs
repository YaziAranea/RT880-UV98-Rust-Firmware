//! MDC1200 scheduling and BK4819 FIFO driver for an authorized FM voice TX.
//! Register meanings: Beken BK4819 Application Note v1.0, §§7–8, 12 and 17.
//! https://github.com/amnemonic/Quansheng_UV-K5_Firmware/blob/main/hardware/BK4819%28V3%29%20Application%20Note%2020210428.pdf
//! FIFO framing: BK4819_send_MDC1200 in the Apache-2.0 reference.
//! https://github.com/OneOfEleven/uv-k5-firmware-custom/blob/main/driver/bk4819.c
//! Hardware timing and deviation require bench validation.

use crate::{radio_name, Error, Fault, Io, Runtime, State, MIC_ADC};
use uv98_signaling::mdc1200::{Edge, Packet, PttId, UnitId};
use uv98_signaling::radio_name::RadioName;
use uv98_signaling::tones::oscillator_word;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Begin,
    End,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub mode: Mode,
    pub unit: Option<UnitId>,
}

impl Config {
    pub const fn begin(self) -> Option<UnitId> {
        match self.mode {
            Mode::Begin | Mode::Both => self.unit,
            _ => None,
        }
    }

    const fn end(self) -> Option<UnitId> {
        match self.mode {
            Mode::End | Mode::Both => self.unit,
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The calibrated TX executor has not reached its keyed state.
    Inactive,
    /// The same authorized physical PTT is still held.
    Voice,
    /// Clean release with unchanged TX authorization and time left on TOT.
    Released,
    /// Any other event invalidating TX, including timeout or PTT handoff.
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Let the ordinary voice/DTMF path run.
    Idle,
    Start(PttId),
    StartName(RadioName),
    Keep,
    RestoreVoice,
    StopTx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Signal {
    Mdc(PttId),
    RadioName(RadioName),
}

impl Signal {
    fn begin(self) -> bool {
        matches!(
            self,
            Self::Mdc(PttId {
                edge: Edge::Begin,
                ..
            })
        )
    }

    fn timeout_ms(self) -> u32 {
        match self {
            Self::Mdc(id) => timeout_ms(id.edge),
            // 3 preamble + 4 sync + 16 payload bytes + hardware CRC.
            // Allow IRQ polling margin after the nominal FFSK duration.
            Self::RadioName(_) => 220,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Initial,
    Voice(Option<Signal>),
    Sending {
        signal: Signal,
        at: u32,
        end: Option<Signal>,
    },
    Restoring {
        at: u32,
        end: Option<Signal>,
    },
    Terminal,
}

pub struct Session {
    phase: Phase,
}

impl Session {
    pub const fn new() -> Self {
        Self {
            phase: Phase::Initial,
        }
    }

    pub fn step(
        &mut self,
        now: u32,
        input: Input,
        config: Config,
        progress: Option<Progress>,
    ) -> Action {
        self.step_with_name(now, input, config, None, progress)
    }

    /// Latch optional stock name at the first keyed voice step. It takes
    /// the single end-burst slot, while a configured MDC begin still runs.
    pub fn step_with_name(
        &mut self,
        now: u32,
        input: Input,
        config: Config,
        name: Option<RadioName>,
        progress: Option<Progress>,
    ) -> Action {
        match input {
            Input::Inactive => {
                self.phase = Phase::Initial;
                return Action::Idle;
            }
            Input::Abort => return self.stop(),
            _ => {}
        }
        match self.phase {
            Phase::Initial if input == Input::Voice => {
                let end = name.map(Signal::RadioName).or_else(|| {
                    config.end().map(|unit| {
                        Signal::Mdc(PttId {
                            unit,
                            edge: Edge::End,
                        })
                    })
                });
                if let Some(unit) = config.begin() {
                    self.start(
                        Signal::Mdc(PttId {
                            unit,
                            edge: Edge::Begin,
                        }),
                        now,
                        end,
                    )
                } else {
                    self.phase = Phase::Voice(end);
                    Action::Idle
                }
            }
            Phase::Initial | Phase::Terminal => self.stop(),
            Phase::Voice(Some(signal)) if input == Input::Released => self.start(signal, now, None),
            Phase::Voice(_) => Action::Idle,
            Phase::Sending { signal, at, end } => {
                let expected = if signal.begin() {
                    Input::Voice
                } else {
                    Input::Released
                };
                if input != expected {
                    return self.stop();
                }
                match progress {
                    Some(Progress::Preparing) if now.wrapping_sub(at) < SETUP_TIMEOUT_MS => {
                        Action::Keep
                    }
                    Some(Progress::OnAir { since, finished })
                        if now.wrapping_sub(since) < signal.timeout_ms() =>
                    {
                        if !finished {
                            Action::Keep
                        } else if signal.begin() {
                            self.phase = Phase::Restoring { at: now, end };
                            Action::RestoreVoice
                        } else {
                            self.stop()
                        }
                    }
                    _ => self.stop(),
                }
            }
            Phase::Restoring { at, end } => {
                if input != Input::Voice || now.wrapping_sub(at) >= SETUP_TIMEOUT_MS {
                    return self.stop();
                }
                match progress {
                    Some(Progress::Restoring) => Action::Keep,
                    None => {
                        self.phase = Phase::Voice(end);
                        Action::Idle
                    }
                    _ => self.stop(),
                }
            }
        }
    }

    fn start(&mut self, signal: Signal, at: u32, end: Option<Signal>) -> Action {
        self.phase = Phase::Sending { signal, at, end };
        match signal {
            Signal::Mdc(id) => Action::Start(id),
            Signal::RadioName(name) => Action::StartName(name),
        }
    }

    fn stop(&mut self) -> Action {
        self.phase = Phase::Terminal;
        Action::StopTx
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

const SETUP_TIMEOUT_MS: u32 = 300;

// 16/4 hardware preamble bytes + four hardware sync bytes + 22 FIFO bytes
// occupy 280/200 ms at 1200 bit/s. Allow 40 ms for the IRQ poll.
const fn timeout_ms(edge: Edge) -> u32 {
    match edge {
        Edge::Begin => 320,
        Edge::End => 240,
    }
}

const FSK_TX_FINISHED: u16 = 1 << 15;
const FIFO_CLEAR: u16 = (1 << 15) | (1 << 14);
const FSK_TX: u16 = 1 << 11;
const FSK_RX: u16 = 1 << 12;
const FSK_MODE: u16 = (1 << 13) | (7 << 10) | (1 << 1) | 1;
const SAVED_REGISTERS: [u8; 13] = [
    0x2b, 0x30, 0x3f, 0x50, 0x51, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x70, 0x72,
];
const RESTORE_WRITES: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Preparing,
    OnAir { since: u32, finished: bool },
    Restoring,
}

#[derive(Clone, Copy)]
enum ModemPhase {
    Capture(usize),
    Prepare(usize),
    Arm,
    OnAir { since: u32, acknowledge_irq: bool },
    Finished(u32),
    Restore(usize),
}

enum Burst {
    Mdc { packet: Packet, edge: Edge },
    RadioName(RadioName),
}

pub(super) struct Modem {
    words: [u16; SAVED_REGISTERS.len()],
    burst: Burst,
    phase: ModemPhase,
}

impl Modem {
    fn control(&self) -> u16 {
        match self.burst {
            Burst::Mdc {
                edge: Edge::Begin, ..
            } => 0xf8,
            Burst::Mdc {
                edge: Edge::End, ..
            } => 0x38,
            Burst::RadioName(_) => radio_name::CONTROL,
        }
    }

    fn arm_control(&self) -> u16 {
        FSK_TX
            | self.control()
            | match self.burst {
                Burst::Mdc { .. } => 0,
                Burst::RadioName(_) => radio_name::SCRAMBLE,
            }
    }

    fn preparation(&self, index: usize) -> Option<(u8, u16)> {
        let [filters, link, _, mute, ..] = self.words;
        let control = self.control();
        let common = [(0x30, link & !MIC_ADC), (0x50, mute & !0x8000), (0x51, 0)];
        let tail = [(0x02, 0), (0x3f, FSK_TX_FINISHED)];
        match self.burst {
            Burst::Mdc { packet, .. } => common
                .into_iter()
                .chain([
                    (0x2b, filters | 0x0005),
                    (0x58, FSK_MODE),
                    (0x72, oscillator_word(1200)),
                    (0x70, 0x00e0),
                    (0x5d, Packet::length_word()),
                    (0x5a, 0),
                    (0x5b, 0),
                    (0x5c, 0x5625),
                    (0x59, FIFO_CLEAR | control),
                    (0x59, control),
                ])
                .chain(packet.fifo_words().map(|word| (0x5f, word)))
                .chain(tail)
                .nth(index),
            Burst::RadioName(name) => common
                .into_iter()
                .chain(radio_name::PROFILE)
                .chain([(0x59, 0x8000 | control), (0x59, control)])
                .chain(name.fifo_words().map(|word| (0x5f, word)))
                .chain(tail)
                .nth(index),
        }
    }

    fn restoration(&self, terminal: bool) -> [(u8, u16); RESTORE_WRITES] {
        let [filters, _, irq, mute, css, mode, control, sync0, sync1, crc, length, tones, tone_frequency] =
            self.words;
        [
            (0x59, FIFO_CLEAR),
            (0x58, mode & !1),
            (0x59, control & !(FIFO_CLEAR | FSK_TX | FSK_RX)),
            (0x70, if terminal { tones & !0x8080 } else { tones }),
            (0x72, tone_frequency),
            (0x5a, sync0),
            (0x5b, sync1),
            (0x5c, crc),
            (0x5d, length),
            (0x2b, filters),
            (0x50, if terminal { mute & !0x8000 } else { mute }),
            (0x51, if terminal { css & !0x8000 } else { css }),
            (0x02, 0),
            (0x3f, if terminal { irq & !0xf002 } else { irq }),
        ]
    }
}

impl<I: Io> Runtime<I> {
    /// Stage an FFSK burst for the current authorized TX without touching
    /// hardware. The caller must requalify PTT and TOT before each step.
    pub fn start_mdc(&mut self, id: PttId) -> Result<(), Fault<I::Error>> {
        self.start_fsk(Burst::Mdc {
            packet: Packet::encode(id),
            edge: id.edge,
        })
    }

    /// A stock-name tail can only use the already-keyed calibrated TX route.
    pub fn start_radio_name(&mut self, name: RadioName) -> Result<(), Fault<I::Error>> {
        self.start_fsk(Burst::RadioName(name))
    }

    fn start_fsk(&mut self, burst: Burst) -> Result<(), Fault<I::Error>> {
        if self.state != State::Transmitting || self.mdc_modem.is_some() {
            return Err(self.reject(Error::State));
        }
        self.mdc_modem = Some(Modem {
            words: [0; SAVED_REGISTERS.len()],
            burst,
            phase: ModemPhase::Capture(0),
        });
        Ok(())
    }

    pub fn mdc_progress(&self) -> Option<Progress> {
        self.mdc_modem.as_ref().map(|modem| match modem.phase {
            ModemPhase::Capture(_) | ModemPhase::Prepare(_) | ModemPhase::Arm => {
                Progress::Preparing
            }
            ModemPhase::OnAir { since, .. } => Progress::OnAir {
                since,
                finished: false,
            },
            ModemPhase::Finished(since) => Progress::OnAir {
                since,
                finished: true,
            },
            ModemPhase::Restore(_) => Progress::Restoring,
        })
    }

    /// Stage voice restoration after the begin burst has completed.
    pub fn resume_after_mdc(&mut self) -> Result<(), Fault<I::Error>> {
        let Some(modem) = &mut self.mdc_modem else {
            return Err(self.reject(Error::State));
        };
        if self.state != State::Transmitting
            || !matches!(modem.phase, ModemPhase::Finished(_))
            || !matches!(
                modem.burst,
                Burst::Mdc {
                    edge: Edge::Begin,
                    ..
                }
            )
        {
            return Err(self.reject(Error::State));
        }
        modem.phase = ModemPhase::Restore(0);
        Ok(())
    }

    /// Advance after fresh authorization. Each call performs at most two
    /// register transactions. FSK enable and mic enable each have a dedicated
    /// step so earlier reads or writes cannot delay their PTT check.
    pub fn step_mdc(&mut self, now: u32) -> Result<(), Fault<I::Error>> {
        let Some(modem) = self.mdc_modem.as_ref() else {
            return Err(self.reject(Error::State));
        };
        if self.state != State::Transmitting {
            return Err(self.reject(Error::State));
        }
        let phase = modem.phase;
        let ownership = self.io.verify_control();
        self.io_result(ownership)?;
        let next = match phase {
            ModemPhase::Capture(index) => {
                let value = self.read(SAVED_REGISTERS[index])?;
                let modem = self.mdc_modem.as_mut().unwrap();
                modem.words[index] = value;
                if index + 1 < SAVED_REGISTERS.len() {
                    ModemPhase::Capture(index + 1)
                } else {
                    let [_, _, _, _, _, mode, control, ..] = modem.words;
                    if mode & 1 != 0 || control & (FSK_TX | FSK_RX) != 0 {
                        return Err(self.reject(Error::State));
                    }
                    ModemPhase::Prepare(0)
                }
            }
            ModemPhase::Prepare(index) => {
                let write = self.mdc_modem.as_ref().unwrap().preparation(index);
                if let Some((register, value)) = write {
                    self.write(register, value)?;
                    ModemPhase::Prepare(index + 1)
                } else {
                    ModemPhase::Arm
                }
            }
            ModemPhase::Arm => {
                let control = self.mdc_modem.as_ref().unwrap().arm_control();
                self.write(0x59, control)?;
                ModemPhase::OnAir {
                    since: now,
                    acknowledge_irq: false,
                }
            }
            ModemPhase::OnAir {
                since,
                acknowledge_irq: true,
            } => {
                // REG02 supplies the captured IRQ vector after acknowledgement.
                self.write(0x02, 0)?;
                if self.read(0x02)? & FSK_TX_FINISHED != 0 {
                    ModemPhase::Finished(since)
                } else {
                    ModemPhase::OnAir {
                        since,
                        acknowledge_irq: false,
                    }
                }
            }
            ModemPhase::OnAir {
                since,
                acknowledge_irq: false,
            } => ModemPhase::OnAir {
                since,
                acknowledge_irq: self.read(0x0c)? & 1 != 0,
            },
            ModemPhase::Finished(_) => return Err(self.reject(Error::State)),
            ModemPhase::Restore(index) => {
                let modem = self.mdc_modem.as_ref().unwrap();
                if index == RESTORE_WRITES {
                    let voice_link = modem.words[1] | MIC_ADC;
                    self.write(0x30, voice_link)?;
                    self.mdc_modem = None;
                    return Ok(());
                }
                let (register, value) = modem.restoration(false)[index];
                self.write(register, value)?;
                ModemPhase::Restore(index + 1)
            }
        };
        self.mdc_modem.as_mut().unwrap().phase = next;
        Ok(())
    }

    pub(super) fn cleanup_mdc(&mut self) -> bool {
        let Some(modem) = self.mdc_modem.take() else {
            return false;
        };
        if matches!(modem.phase, ModemPhase::Capture(_)) {
            return false;
        }
        let mut failed = false;
        for (register, value) in modem.restoration(true) {
            if self.io.write(register, value).is_err() {
                failed = true;
            }
        }
        failed
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use self::std::vec::Vec;
    use super::*;
    use crate::TX_TIMEOUT_MS;

    const ID: UnitId = match UnitId::new(0x1234) {
        Some(id) => id,
        None => panic!("invalid test ID"),
    };
    const BOTH: Config = Config {
        mode: Mode::Both,
        unit: Some(ID),
    };
    const BEGIN: PttId = PttId {
        unit: ID,
        edge: Edge::Begin,
    };
    const END: PttId = PttId {
        unit: ID,
        edge: Edge::End,
    };
    const PREPARING: Option<Progress> = Some(Progress::Preparing);
    const RESTORING: Option<Progress> = Some(Progress::Restoring);

    fn air(since: u32, finished: bool) -> Option<Progress> {
        Some(Progress::OnAir { since, finished })
    }

    #[test]
    fn begin_and_end_are_single_bursts_and_use_latched_id() {
        let mut tx = Session::new();
        let off = Config::default();
        assert_eq!(tx.step(5, Input::Inactive, BOTH, None), Action::Idle);
        assert_eq!(tx.step(10, Input::Voice, BOTH, None), Action::Start(BEGIN));
        assert_eq!(tx.step(11, Input::Voice, off, PREPARING), Action::Keep);
        assert_eq!(
            tx.step(340, Input::Voice, off, air(60, true)),
            Action::RestoreVoice
        );
        assert_eq!(tx.step(341, Input::Voice, off, RESTORING), Action::Keep);
        assert_eq!(tx.step(360, Input::Voice, BOTH, None), Action::Idle);
        assert_eq!(tx.step(400, Input::Released, off, None), Action::Start(END));
        assert_eq!(tx.step(401, Input::Released, off, PREPARING), Action::Keep);
        assert_eq!(
            tx.step(650, Input::Released, off, air(450, true)),
            Action::StopTx
        );
        assert_eq!(tx.step(651, Input::Released, BOTH, None), Action::StopTx);
    }

    #[test]
    fn abort_timeout_and_short_press_never_run_tail() {
        for unsafe_input in [Input::Abort, Input::Released] {
            let mut tx = Session::new();
            assert_eq!(tx.step(0, Input::Voice, BOTH, None), Action::Start(BEGIN));
            assert_eq!(tx.step(1, unsafe_input, BOTH, PREPARING), Action::StopTx);
            assert_eq!(tx.step(2, Input::Voice, BOTH, None), Action::StopTx);
            assert_eq!(tx.step(3, Input::Inactive, BOTH, None), Action::Idle);
            assert_eq!(tx.step(4, Input::Released, BOTH, None), Action::StopTx);
        }
        let mut tx = Session::new();
        let start = u32::MAX - 100;
        assert_eq!(
            tx.step(start, Input::Voice, BOTH, None),
            Action::Start(BEGIN)
        );
        assert_eq!(
            tx.step(
                start.wrapping_add(399),
                Input::Voice,
                BOTH,
                air(start.wrapping_add(80), false)
            ),
            Action::Keep
        );
        assert_eq!(
            tx.step(
                start.wrapping_add(400),
                Input::Voice,
                BOTH,
                air(start.wrapping_add(80), true)
            ),
            Action::StopTx
        );
    }

    #[test]
    fn end_only_interrupts_on_new_ptt_and_off_preserves_existing_path() {
        let mut tx = Session::default();
        let off = Config::default();
        let end = Config {
            mode: Mode::End,
            unit: Some(ID),
        };
        assert_eq!(tx.step(0, Input::Voice, off, None), Action::Idle);
        assert_eq!(tx.step(1, Input::Released, end, None), Action::Idle);
        assert_eq!(tx.step(2, Input::Inactive, end, None), Action::Idle);
        assert_eq!(tx.step(3, Input::Voice, end, None), Action::Idle);
        assert_eq!(tx.step(4, Input::Released, end, None), Action::Start(END));
        assert_eq!(tx.step(5, Input::Voice, end, PREPARING), Action::StopTx);
        assert_eq!(tx.step(6, Input::Inactive, end, None), Action::Idle);
        assert_eq!(tx.step(7, Input::Voice, end, None), Action::Idle);
        assert_eq!(tx.step(8, Input::Released, end, None), Action::Start(END));
        assert_eq!(
            tx.step(289, Input::Released, end, air(50, false)),
            Action::Keep
        );
        assert_eq!(
            tx.step(290, Input::Released, end, air(50, false)),
            Action::StopTx
        );
    }

    #[test]
    fn prep_and_restore_have_separate_deadlines() {
        let mut tx = Session::new();
        assert_eq!(tx.step(0, Input::Voice, BOTH, None), Action::Start(BEGIN));
        assert_eq!(tx.step(299, Input::Voice, BOTH, PREPARING), Action::Keep);
        assert_eq!(tx.step(300, Input::Voice, BOTH, PREPARING), Action::StopTx);
        assert_eq!(tx.step(301, Input::Inactive, BOTH, None), Action::Idle);
        assert_eq!(tx.step(302, Input::Voice, BOTH, None), Action::Start(BEGIN));
        assert_eq!(
            tx.step(630, Input::Voice, BOTH, air(350, true)),
            Action::RestoreVoice
        );
        assert_eq!(tx.step(929, Input::Voice, BOTH, RESTORING), Action::Keep);
        assert_eq!(tx.step(930, Input::Voice, BOTH, RESTORING), Action::StopTx);
    }

    struct Fake {
        registers: [u16; 128],
        writes: Vec<(u8, u16)>,
        irq: u16,
        transactions: usize,
        fail_next_read: Option<u8>,
        fail_next_write: Option<u8>,
        lost_control: bool,
    }

    impl Io for Fake {
        type Error = ();

        fn read(&mut self, register: u8) -> Result<u16, ()> {
            self.transactions += 1;
            if self.fail_next_read == Some(register) {
                self.fail_next_read = None;
                return Err(());
            }
            match register {
                0x0c => Ok(u16::from(self.irq != 0)),
                0x02 => Ok(core::mem::take(&mut self.irq)),
                _ => Ok(self.registers[usize::from(register)]),
            }
        }

        fn write(&mut self, register: u8, value: u16) -> Result<(), ()> {
            self.transactions += 1;
            self.writes.push((register, value));
            if self.fail_next_write == Some(register) {
                self.fail_next_write = None;
                return Err(());
            }
            self.registers[usize::from(register)] = value;
            Ok(())
        }

        fn mute(&mut self, _: bool) -> Result<(), ()> {
            Ok(())
        }

        fn antenna(&mut self, _: bool) -> Result<(), ()> {
            Ok(())
        }

        fn hf_gates(&mut self, _: bool, _: bool) -> Result<(), ()> {
            Ok(())
        }

        fn indicators(&mut self, _: bool, _: bool) -> Result<(), ()> {
            Ok(())
        }

        fn backlights(&mut self, _: bool, _: bool) -> Result<(), ()> {
            Ok(())
        }

        fn verify_control(&mut self) -> Result<(), ()> {
            if self.lost_control {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    fn transmitting() -> Runtime<Fake> {
        let mut radio = Runtime::new(Fake {
            registers: [0; 128],
            writes: Vec::new(),
            irq: 0,
            transactions: 0,
            fail_next_read: None,
            fail_next_write: None,
            lost_control: false,
        });
        radio.state = State::Transmitting;
        radio.tx_started = Some(0);
        radio.io.registers[0x30] = 0xc1fe;
        radio.io.registers[0x33] = 0xa518;
        radio.io.registers[0x36] = 0x5cff;
        radio.io.registers[0x50] = 0x3b20;
        radio.io.registers[0x51] = 0x8001;
        radio.io.registers[0x3f] = 0x0040;
        radio
    }

    fn assert_fsk_off(radio: &Runtime<Fake>) {
        assert_eq!(radio.io.registers[0x58] & 1, 0);
        assert_eq!(radio.io.registers[0x59] & (FSK_TX | FSK_RX), 0);
        assert_eq!(radio.io.registers[0x70] & 0x8080, 0);
    }

    fn step_mdc(radio: &mut Runtime<Fake>, now: u32) {
        let before = radio.io.transactions;
        radio.step_mdc(now).unwrap();
        assert!(radio.io.transactions - before <= 2);
    }

    fn advance_until_air(radio: &mut Runtime<Fake>) -> u32 {
        for now in 0..60 {
            step_mdc(radio, now);
            if let Some(Progress::OnAir { since, .. }) = radio.mdc_progress() {
                assert_eq!(since, now);
                return since;
            }
            assert_eq!(radio.io.registers[0x59] & FSK_TX, 0);
        }
        panic!("MDC preparation did not finish");
    }

    #[test]
    fn beginning_id_keeps_mic_off_before_gates_and_preserves_the_voice_plan() {
        use uv98_calibration::Calibration;
        use uv98_rf_sequence::{self as rf, Step, TxRequest, Width};

        let cal = Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        let request = TxRequest {
            frequency_10hz: 14_550_000,
            width: Width::Wide,
            tone: 0,
            tone_mode: 0,
            option_nibble: 0,
            power: 0,
            mic_gain: 31,
        };
        let normal = rf::transmit(&cal, request).unwrap();
        let gated = rf::transmit_with_microphone(&cal, request, false).unwrap();
        let normal = normal.operations();
        let gated = gated.operations();
        assert_eq!(normal.len(), gated.len());
        let index = normal
            .iter()
            .position(|step| {
                *step
                    == Step::WriteA {
                        register: 0x30,
                        value: 0xc1fe,
                    }
            })
            .unwrap();
        assert_eq!(&normal[..index], &gated[..index]);
        assert_eq!(
            gated[index],
            Step::WriteA {
                register: 0x30,
                value: 0xc1fa
            }
        );
        assert!(matches!(gated[index + 1], Step::TxGates(_)));
        assert_eq!(&normal[index + 1..], &gated[index + 1..]);
    }

    #[test]
    fn fifo_send_disables_only_mic_and_restores_after_done() {
        let mut radio = transmitting();
        radio.io.registers[0x30] &= !MIC_ADC;
        let mut before = radio.io.registers;
        radio.start_mdc(BEGIN).unwrap();
        assert!(radio.io.writes.is_empty());
        let since = advance_until_air(&mut radio);
        assert_eq!(radio.io.registers[0x30], 0xc1fa);
        assert_eq!(radio.io.registers[0x58], 0x3c03);
        assert_eq!(radio.io.registers[0x59], 0x08f8);
        assert_eq!(radio.io.registers[0x72], 0x3065);
        for address in [0x33, 0x36, 0x38, 0x39, 0x40] {
            assert_eq!(radio.io.registers[address], before[address]);
        }
        let fifo = radio
            .io
            .writes
            .iter()
            .filter_map(|&(reg, value)| (reg == 0x5f).then_some(value));
        assert!(fifo.eq(Packet::encode(BEGIN).fifo_words()));
        step_mdc(&mut radio, since + 1);
        assert_eq!(radio.mdc_progress(), air(since, false));
        radio.io.irq = 1 << 7;
        step_mdc(&mut radio, since + 2);
        step_mdc(&mut radio, since + 3);
        assert_eq!(radio.mdc_progress(), air(since, false));
        radio.io.irq = FSK_TX_FINISHED;
        step_mdc(&mut radio, since + 280);
        step_mdc(&mut radio, since + 281);
        assert_eq!(radio.mdc_progress(), air(since, true));
        radio.resume_after_mdc().unwrap();
        assert_eq!(radio.io.registers[0x30] & MIC_ADC, 0);
        for index in 0..RESTORE_WRITES {
            step_mdc(&mut radio, since + 282 + index as u32);
            assert_eq!(radio.io.registers[0x30] & MIC_ADC, 0);
        }
        step_mdc(&mut radio, since + 282 + RESTORE_WRITES as u32);
        assert_eq!(radio.mdc_progress(), None);
        before[0x30] |= MIC_ADC;
        before[0x5f] = radio.io.registers[0x5f]; // FIFO is not a readable configuration word.
        assert_eq!(radio.io.registers, before);
    }

    #[test]
    fn rx_entry_is_rejected_and_fault_or_tot_unkeys_partial_burst() {
        let mut radio = transmitting();
        radio.state = State::Receiving;
        assert_eq!(radio.start_mdc(BEGIN).unwrap_err().reason, Error::State);
        assert!(radio.io.writes.is_empty());
        for cut in 0..4 {
            let mut radio = transmitting();
            match cut {
                0 => {
                    radio.start_mdc(BEGIN).unwrap();
                    radio.io.fail_next_write = Some(0x5f);
                    let fault = (0..60).find_map(|now| radio.step_mdc(now).err()).unwrap();
                    assert!(fault.cleanup_attempted);
                }
                1 => {
                    radio.start_mdc(END).unwrap();
                    advance_until_air(&mut radio);
                    assert!(radio.tick(TX_TIMEOUT_MS, true).unwrap().tx_timed_out);
                }
                2 => {
                    radio.start_mdc(END).unwrap();
                    advance_until_air(&mut radio);
                    radio.io.lost_control = true;
                    assert!(radio.step_mdc(60).unwrap_err().cleanup_attempted);
                }
                _ => {
                    radio.start_mdc(END).unwrap();
                    advance_until_air(&mut radio);
                    radio.io.fail_next_write = Some(0x59);
                    assert_eq!(radio.stop().unwrap_err().cleanup_failures, 0x80);
                }
            }
            assert_fsk_off(&radio);
            assert_eq!(radio.io.registers[0x30], 0);
            assert_eq!(radio.io.registers[0x33], 0);
            assert_eq!(radio.io.registers[0x36], 0x007f);
            assert!(radio.io.writes.contains(&(0x59, 0xc000)));
            assert!(radio.io.writes.contains(&(0x3f, 0x0040)));
        }
    }

    #[test]
    fn partial_capture_fault_never_restores_unsaved_words() {
        let mut radio = transmitting();
        let before = radio.io.registers;
        radio.start_mdc(BEGIN).unwrap();
        step_mdc(&mut radio, 0);
        radio.io.fail_next_read = Some(0x30);
        assert!(radio.step_mdc(1).unwrap_err().cleanup_attempted);
        assert_eq!(radio.io.registers[0x3f], before[0x3f]);
        assert_eq!(radio.io.registers[0x51], before[0x51]);
        assert_eq!(radio.io.registers[0x30], 0);
        assert!(!radio
            .io
            .writes
            .iter()
            .any(|&(reg, _)| matches!(reg, 0x58 | 0x59 | 0x5f)));
        assert_eq!(radio.mdc_progress(), None);
    }
}
