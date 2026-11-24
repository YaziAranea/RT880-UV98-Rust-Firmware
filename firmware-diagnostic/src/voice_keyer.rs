//! Plays a stored clip on the local analog speaker path during manual FM TX.
//! Acoustic coupling or an independently verified accessory audio interface is
//! required to actually get that audio to MICI. This module cannot key RF.

use crate::voice_clip::Samples;
use crate::voice_keyer_logic::{enough_time, Sample, Stream, PERIOD_CYCLES};
use at32f4xx_pac::at32f423::{self as pac, Interrupt, CRM, DAC, GPIOA, TMR2};
use core::cell::RefCell;
use cortex_m::{
    interrupt::{self, Mutex},
    peripheral::{DWT, NVIC},
};
use uv98_board_pins::{RxAudioPins, RX_AUDIO_A_MASK};
use uv98_radio_runtime::{Fault, Io, Runtime, State};

const AUDIO_OFF: u16 = 0x6040;
// Only DAC2 fields. DAC1 and neighboring GPIO are owned elsewhere.
const DAC2_FIELDS: u32 = 0x3fff_0000;
struct Player {
    stream: Option<Stream<Samples<'static>>>,
    completed: bool,
}
static PLAYER: Mutex<RefCell<Player>> = Mutex::new(RefCell::new(Player {
    stream: None,
    completed: false,
}));

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Pending,
    Playing,
    Complete,
    Aborted,
}

pub struct Keyer {
    _pins: RxAudioPins,
    _dac: DAC,
    _timer: TMR2,
    clip: Option<Samples<'static>>,
    playback: Playback,
    saved_af: Option<u16>,
}

impl Keyer {
    pub fn new(
        pins: RxAudioPins,
        dac: DAC,
        timer: TMR2,
        crm: &mut CRM,
        nvic: &mut NVIC,
        clip: Option<Samples<'static>>,
    ) -> Option<Self> {
        NVIC::mask(Interrupt::TMR2);
        NVIC::unpend(Interrupt::TMR2);
        // SAFETY: TMR2 belongs only to this profile; GPS USART2 is separate.
        unsafe { nvic.set_priority(Interrupt::TMR2, 0x40) };
        crm.ahben1().modify(|_, w| w.gpioa().set_bit());
        crm.apb1en()
            .modify(|_, w| w.dac().set_bit().tmr2().set_bit());
        let clk = crm.apb1en().read();
        if clk.dac().bit_is_clear()
            || clk.tmr2().bit_is_clear()
            || crm.ahben1().read().gpioa().bit_is_clear()
        {
            return None;
        }
        crm.apb1rst().modify(|_, w| w.tmr2().set_bit());
        let _ = crm.apb1rst().read().bits();
        crm.apb1rst().modify(|_, w| w.tmr2().clear_bit());
        if crm.apb1rst().read().tmr2().bit_is_set() {
            return None;
        }
        // The checked 48/24/48 MHz profile yields nominal 48 MHz TMR2 on
        // divided APB1. Counter 0..5999 gives one update every 125 us.
        timer.div().write(|w| unsafe { w.bits(0) });
        timer.pr().write(|w| unsafe { w.bits(PERIOD_CYCLES - 1) });
        timer.swevt().write(|w| w.ovfswtr().set_bit());
        timer.ists().write(|w| unsafe { w.bits(0) });
        dac.ctrl()
            .modify(|r, w| unsafe { w.bits(r.bits() & !DAC2_FIELDS) });
        // SAFETY: RxAudioPins is the unique ownership token for PA5, and the
        // shared GPIOA update is serialized in foreground before IRQ enable.
        let gpio = unsafe { &*GPIOA::ptr() };
        let mode = 3 << (RX_AUDIO_A_MASK.trailing_zeros() * 2);
        gpio.pull()
            .modify(|r, w| unsafe { w.bits(r.bits() & !mode) });
        gpio.cfgr()
            .modify(|r, w| unsafe { w.bits(r.bits() | mode) });
        if gpio.cfgr().read().bits() & mode != mode
            || gpio.pull().read().bits() & mode != 0
            || dac.ctrl().read().bits() & DAC2_FIELDS != 0
            || timer.pr().read().bits() != PERIOD_CYCLES - 1
            || timer.div().read().bits() != 0
            || timer.ctrl1().read().tmren().bit_is_set()
        {
            return None;
        }
        Some(Self {
            _pins: pins,
            _dac: dac,
            _timer: timer,
            clip,
            playback: Playback::Pending,
            saved_af: None,
        })
    }

    pub fn reset(&mut self) {
        self.playback = Playback::Pending;
    }

    pub fn available(&self) -> bool {
        self.clip.is_some()
    }

    /// Run before other TX-signaling operations. A stale or ambiguous sample
    /// never extends the independent DAC lease. EOT signaling sees the
    /// ordinary speaker/AF state after this function.
    pub fn maintain<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        authorized: bool,
    ) -> Result<Playback, Fault<I::Error>> {
        let Some(saved) = self.saved_af else {
            return Ok(self.playback);
        };
        self.playback = interrupt::free(|cs| {
            let mut player = PLAYER.borrow(cs).borrow_mut();
            if authorized
                && player
                    .stream
                    .as_mut()
                    .is_some_and(|s| s.renew(DWT::cycle_count()))
            {
                return Playback::Playing;
            }
            if player.stream.take().is_some() {
                park_output();
            }
            if authorized && player.completed {
                Playback::Complete
            } else {
                Playback::Aborted
            }
        });
        if self.playback != Playback::Playing {
            self.saved_af = None;
            if radio.state() == State::Transmitting {
                radio.signaling_keyer_speaker(false)?;
                radio.signaling_write(0x47, saved)?;
            }
            // PF8 is closed. Remove the DAC bias only after muting the amplifier.
            dac().ctrl().modify(|_, w| w.d2en().clear_bit());
        }
        Ok(self.playback)
    }

    /// Called only after established voice TX and ordinary signaling have
    /// been rechecked. This changes AF OUT and its external amplifier, never
    /// the transmit synthesizer, microphone ADC, PA gate or TX timer.
    pub fn start<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        remaining_tx_ms: u32,
    ) -> Result<bool, Fault<I::Error>> {
        if self.playback != Playback::Pending || !enough_time(remaining_tx_ms) {
            return Ok(false);
        }
        let Some(samples) = self.clip.clone() else {
            return Ok(false);
        };
        let saved = radio.signaling_read(0x47)?;
        radio.signaling_write(0x47, AUDIO_OFF)?;
        let dac = dac();
        // No playback IRQ is active yet. Establish the DAC bias before
        // switching on the speaker, under the ordinary verified AF mute.
        write_sample(dac, 128);
        dac.ctrl().modify(|_, w| w.d2en().set_bit());
        // DAC wake-up is at most 4 us under datasheet test load. Board audio
        // settling and the actual click level still need measurement.
        cortex_m::asm::delay(384);
        radio.signaling_keyer_speaker(true)?;
        self.saved_af = Some(saved);
        let started = interrupt::free(|cs| {
            let Some((stream, first)) = Stream::begin(samples, DWT::cycle_count()) else {
                return false;
            };
            let timer = timer();
            *PLAYER.borrow(cs).borrow_mut() = Player {
                stream: Some(stream),
                completed: false,
            };
            // No peripheral trigger, DMA, or waveform generator on DAC2.
            write_sample(dac, first);
            timer.cval().write(|w| unsafe { w.bits(0) });
            timer.ists().write(|w| unsafe { w.bits(0) });
            NVIC::unpend(Interrupt::TMR2);
            timer.iden().write(|w| w.ovfien().set_bit());
            // SAFETY: the stream, update source, and register ownership are
            // initialized before enabling the sole playback IRQ.
            unsafe { NVIC::unmask(Interrupt::TMR2) };
            timer.ctrl1().write(|w| w.tmren().set_bit());
            true
        });
        self.playback = if started {
            Playback::Playing
        } else {
            Playback::Aborted
        };
        Ok(started)
    }
}

fn timer() -> &'static pac::tmr2::RegisterBlock {
    // SAFETY: Keyer owns TMR2. All foreground and ISR updates are serialized
    // with interrupts masked and do not move the peripheral register block.
    unsafe { &*TMR2::ptr() }
}

fn dac() -> &'static pac::dac::RegisterBlock {
    // SAFETY: Keyer owns DAC. This module changes only DAC2.
    unsafe { &*DAC::ptr() }
}

fn write_sample(dac: &pac::dac::RegisterBlock, sample: u8) {
    dac.d2dth8r()
        .write(|w| unsafe { w.bits(u32::from(sample)) });
}

/// Silence the output without removing its bias while PF8 may still be open.
/// An IRQ deadline can do this on a stalled foreground. If interrupts are
/// masked, DAC2 holds the last code until servicing resumes or reset.
fn park_output() {
    let timer = timer();
    NVIC::mask(Interrupt::TMR2);
    timer.ctrl1().reset();
    timer.iden().reset();
    timer.ists().write(|w| unsafe { w.bits(0) });
    NVIC::unpend(Interrupt::TMR2);
    write_sample(dac(), 128);
}

pub fn on_interrupt() {
    interrupt::free(|cs| {
        let timer = timer();
        if timer.ists().read().ovfif().bit_is_clear() {
            return;
        }
        timer.ists().write(|w| unsafe { w.bits(0) });
        let mut player = PLAYER.borrow(cs).borrow_mut();
        let sample = player
            .stream
            .as_mut()
            .map_or(Sample::Interrupted, |s| s.sample(DWT::cycle_count()));
        match sample {
            Sample::Value(sample) => write_sample(dac(), sample),
            Sample::End | Sample::Interrupted => {
                player.stream = None;
                player.completed = sample == Sample::End;
                park_output();
            }
        }
    });
}
