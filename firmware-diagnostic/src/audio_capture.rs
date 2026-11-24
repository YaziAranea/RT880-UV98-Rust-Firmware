//! Manual, passive PA5 experiment. This module is excluded from TX images.
use cortex_m::peripheral::DWT;
use uv98_boot_log::{hardware::TxLogger, AudioExportBudget, Frame, PA5_CONTEXT};
use uv98_handheld_ui::render::{AudioCaptureStatus, AudioCaptureView};
use uv98_keys::Key;
use uv98_radio_runtime::policy::Profile;
use uv98_rf_sequence::Modulation;
use uv98_sidekeys_at32::{
    hardware::{RxAudioInput, SideAdc},
    CaptureStats, CaptureTiming, Error, PollTiming, RX_AUDIO_HZ, RX_AUDIO_SAMPLES,
};

#[path = "collect_tone.rs"]
mod collect_tone;
const _: () = assert!(RX_AUDIO_HZ == collect_tone::SAMPLE_RATE);

const POST_CAPTURE_CYCLES: u32 = 2_400_000;

#[derive(Default)]
struct Context {
    profile: Option<Profile>,
    before_open: bool,
    after_open: Option<bool>,
    completed_at: Option<u32>,
}

impl Context {
    fn frame(&self, sequence: u32, quiet: bool) -> Frame {
        // An A6 extension stays invisible to the legacy A5-only decoder.
        let (frequency, mode, squelch, monitor, tone_required, tone_mode, tone) = self
            .profile
            .and_then(|p| p.receive.map(|rx| (p.audio, rx)))
            .map_or((0, 0, 15, 0, 0, 0, 0), |(audio, rx)| {
                let mode = match rx.modulation {
                    Modulation::Fm => 1,
                    Modulation::Am => 2,
                };
                (
                    rx.frequency_10hz,
                    mode,
                    u32::from(audio.squelch),
                    flag(Some(audio.monitor)),
                    u32::from(audio.tone_required),
                    u32::from(rx.tone_mode),
                    u32::from(rx.tone),
                )
            });
        let flags = mode
            | (squelch << 2)
            | (monitor << 6)
            | (flag(Some(self.before_open)) << 8)
            | (flag(self.after_open) << 10)
            | (u32::from(quiet) << 12)
            | (tone_required << 13)
            | (tone_mode << 14)
            | (tone << 16);
        Frame::new(PA5_CONTEXT, [sequence, frequency, flags])
    }
}

fn flag(value: Option<bool>) -> u32 {
    match value {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    }
}

pub struct Capture {
    input: RxAudioInput,
    samples: [u16; RX_AUDIO_SAMPLES],
    timing: Option<CaptureTiming>,
    view: AudioCaptureView,
    export: Option<(u16, AudioExportBudget)>,
    checksum: u32,
    context: Context,
}

impl Capture {
    pub fn new(input: RxAudioInput) -> Self {
        Self {
            input,
            samples: [0; RX_AUDIO_SAMPLES],
            timing: None,
            view: AudioCaptureView::default(),
            export: None,
            checksum: 0,
            context: Context::default(),
        }
    }
    pub fn view(&self) -> AudioCaptureView {
        self.view
    }

    /// Returns true after an acquisition attempt so main resamples time/keys.
    pub fn key<T: PollTiming>(
        &mut self,
        key: Option<Key>,
        allowed: bool,
        adc: &mut SideAdc<T>,
        profile: Option<Profile>,
        before_open: bool,
    ) -> Result<bool, Error> {
        if !allowed
            || self
                .context
                .completed_at
                .is_some_and(|at| DWT::cycle_count().wrapping_sub(at) > POST_CAPTURE_CYCLES)
        {
            self.context.completed_at = None;
        }
        if !allowed || self.export.is_some() {
            return Ok(false);
        }
        match key {
            Some(Key::Digit5 | Key::Digit0) => {
                let quiet = key == Some(Key::Digit0);
                self.timing = None;
                self.view = AudioCaptureView {
                    sequence: self.view.sequence.wrapping_add(1),
                    quiet,
                    ..AudioCaptureView::default()
                };
                self.context = Context {
                    profile,
                    before_open,
                    ..Context::default()
                };
                if quiet == before_open {
                    self.view.status = if quiet {
                        AudioCaptureStatus::QuietOpen
                    } else {
                        AudioCaptureStatus::RxClosed
                    };
                    return Ok(false);
                }
                match adc.capture_rx_audio(&mut self.input, &mut self.samples, DWT::cycle_count) {
                    Ok(timing) => {
                        self.context.completed_at = Some(DWT::cycle_count());
                        let stats = CaptureStats::from_samples(&self.samples)?;
                        self.checksum = pcm_crc32(&self.samples);
                        self.view = AudioCaptureView {
                            sequence: self.view.sequence,
                            samples: RX_AUDIO_SAMPLES as u16,
                            minimum: stats.min,
                            maximum: stats.max,
                            dc: (stats.sum / RX_AUDIO_SAMPLES as u32) as u16,
                            low_clips: stats.clip_low,
                            high_clips: stats.clip_high,
                            late_cycles: timing.max_late_cycles,
                            status: AudioCaptureStatus::Captured,
                            quiet,
                            export: 0,
                            collect_tone_candidate: stats.clip_low == 0
                                && stats.clip_high == 0
                                && collect_tone::candidate(&self.samples),
                        };
                        self.timing = Some(timing);
                    }
                    Err(Error::Timing) => self.view.status = AudioCaptureStatus::TimingRejected,
                    Err(error) => return Err(error),
                }
                Ok(true)
            }
            Some(Key::Hash) if self.timing.is_some() => {
                self.context.completed_at = None;
                self.export = Some((0, AudioExportBudget::default()));
                self.view.export = 1;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Only use a fresh observation from the same applied profile. Unknown is
    /// preserved if the receiver retuned, left the screen, or observed too late.
    pub fn observe_after(&mut self, profile: Profile, audio_open: bool) {
        if let Some(at) = self.context.completed_at.take() {
            if self.context.profile == Some(profile)
                && DWT::cycle_count().wrapping_sub(at) <= POST_CAPTURE_CYCLES
            {
                self.context.after_open = Some(audio_open);
            }
        }
    }

    /// One bounded UART frame per foreground pass. Never writes to the RF chip.
    pub fn export_step(&mut self, log: &mut TxLogger) {
        let Some((index, budget)) = self.export.as_mut() else {
            return;
        };
        let Some(timing) = self.timing else {
            self.export = None;
            return;
        };
        let frame = match *index {
            0 => Frame::new(
                0xA510_0000,
                [self.view.sequence, RX_AUDIO_HZ, RX_AUDIO_SAMPLES as u32],
            ),
            1 => Frame::new(
                0xA510_0001,
                [
                    timing.first_trigger_cycle,
                    timing.last_trigger_cycle,
                    timing.max_late_cycles,
                ],
            ),
            2 => Frame::new(
                0xA510_0002,
                [
                    timing.min_interval_cycles,
                    timing.max_interval_cycles,
                    48_000_000,
                ],
            ),
            3 => self.context.frame(self.view.sequence, self.view.quiet),
            4..=345 => {
                let chunk = usize::from(*index - 4);
                let mut words = [0u32; 3];
                for (word_index, word) in words.iter_mut().enumerate() {
                    let sample = chunk * 6 + word_index * 2;
                    *word = u32::from(self.samples.get(sample).copied().unwrap_or(0))
                        | (u32::from(self.samples.get(sample + 1).copied().unwrap_or(0)) << 16);
                }
                Frame::new(0xA520_0000 | chunk as u32, words)
            }
            _ => Frame::new(
                0xA51F_0000,
                [self.view.sequence, self.checksum, RX_AUDIO_SAMPLES as u32],
            ),
        };
        if log.emit_audio(frame, budget).is_err() {
            self.export = None;
            self.view.export = 3;
        } else if *index == 346 {
            self.export = None;
            self.view.export = 2;
        } else {
            *index += 1;
        }
    }
}

fn pcm_crc32(samples: &[u16; RX_AUDIO_SAMPLES]) -> u32 {
    let mut crc = !0u32;
    for byte in samples.iter().flat_map(|value| value.to_le_bytes()) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
