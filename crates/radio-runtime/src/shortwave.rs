//! Foreground Si4732 AM/SSB protocol. Each transaction is bounded by its transport.
//! Power and the shared speaker gate belong to the radio coordinator.

use sha2::{Digest, Sha256};
use uv98_handheld_ui::controller::ShortwaveMode;

const PATCH_BYTES: u16 = 0x7bb * 8;
const PATCH_ID: [u8; 2] = [0x9d, 0x29];
// SHA-256 of the stock 1.16 NOR region read at 0x126000.
const PATCH_SHA256: [u8; 32] = [
    0x34, 0x30, 0x76, 0xb9, 0xe4, 0x72, 0x60, 0x56, 0xe8, 0xdb, 0x46, 0x98, 0x34, 0xf2, 0x61, 0x0e,
    0x56, 0x83, 0x09, 0x5b, 0xf1, 0x9b, 0x41, 0x67, 0x4e, 0xb5, 0xd5, 0xbf, 0x0f, 0xa3, 0xdc, 0x9f,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tuning {
    pub khz: u32,
    pub mode: ShortwaveMode,
    pub bfo: i16,
}

/// Si4732's documented AM DSP channel filters (AN332, property 0x3102).
/// These are implemented in the tuner, not a fictitious MCU audio pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmFilter {
    Wide6k,
    Voice3k,
    Narrow1k,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Processing {
    pub filter: AmFilter,
    /// Enables the tuner's 100 Hz high-pass line-noise filter and at most
    /// 8 dB of SNR-controlled soft mute. This is not broadband NR or an NB.
    pub soft_mute: bool,
}

impl Processing {
    fn filter_property(self) -> u16 {
        let bandwidth = match self.filter {
            AmFilter::Wide6k => 0,
            AmFilter::Voice3k => 2,
            AmFilter::Narrow1k => 4,
        };
        bandwidth | if self.soft_mute { 0x0100 } else { 0 }
    }

    fn attenuation_property(self) -> u16 {
        if self.soft_mute {
            8
        } else {
            0
        }
    }
}

pub const CTS_PENDING: u8 = 6;
const CTS_TIMEOUT_MS: u32 = 250;

impl Tuning {
    pub fn from_selection(khz: u32, fraction_hz: u16, mode: ShortwaveMode, bfo: i16) -> Self {
        let fraction = if mode.is_ssb() { fraction_hz as i16 } else { 0 };
        let (khz, offset) = if fraction >= 500 {
            (khz + 1, fraction - 1000)
        } else {
            (khz, fraction)
        };
        Self {
            khz,
            mode,
            bfo: bfo + offset,
        }
    }

    fn chip_frequency(self) -> u16 {
        let units = if self.mode == ShortwaveMode::Fm {
            self.khz / 10
        } else {
            self.khz
        };
        units as u16
    }
}

#[derive(Default)]
pub struct ModeSwitch {
    requested: Option<ShortwaveMode>,
    changed_at: u32,
    retried: bool,
}

impl ModeSwitch {
    pub fn settled(&mut self, mode: Option<ShortwaveMode>, now: u32) -> bool {
        if self.requested != mode {
            self.requested = mode;
            self.changed_at = now;
            self.retried = false;
        }
        now.wrapping_sub(self.changed_at) >= 160
    }

    pub fn reset_retry(&mut self) {
        self.retried = false;
    }

    pub fn retry(&mut self, code: u8) -> bool {
        if self.retried || matches!(code, 0x25 | 0x91..=0x93 | 0xb5) {
            return false;
        }
        self.retried = true;
        true
    }
}

pub trait Transport {
    fn write(&mut self, command: &[u8]) -> Result<(), u8>;
    fn read<const N: usize>(&mut self) -> Result<[u8; N], u8>;
}

#[derive(Clone, Copy)]
enum Phase {
    Probe,
    Identify,
    PowerAm,
    VerifyPatch,
    PatchPower,
    PatchFrame,
    PatchRevision,
    Volume,
    Tune,
    QueryComplete,
    ReadComplete,
    TuneStatus,
    ReadTune,
    Ready,
    ReadQuality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Update {
    Tuning,
    Identified([u8; 8]),
    PatchProgress(u8),
    Patched,
    Tuned { part: u8, rssi: u8, snr: u8 },
    Quality { part: u8, rssi: u8, snr: u8 },
}

pub struct Session {
    phase: Phase,
    at: u32,
    part: u8,
    tuned: Option<Tuning>,
    configured_mode: Option<ShortwaveMode>,
    configured_bfo: Option<i16>,
    processing: Option<Processing>,
    configured_filter: Option<u16>,
    configured_attenuation: Option<u16>,
    digest: Option<Sha256>,
    patch_offset: u16,
    patch_started: u32,
    tune_started: u32,
    cts_wait_started: Option<u32>,
}

impl Session {
    pub fn new(now: u32) -> Self {
        Self {
            phase: Phase::Probe,
            at: now.wrapping_add(100),
            part: 0,
            tuned: None,
            configured_mode: None,
            configured_bfo: None,
            processing: None,
            configured_filter: None,
            configured_attenuation: None,
            digest: None,
            patch_offset: 0,
            patch_started: 0,
            tune_started: 0,
            cts_wait_started: None,
        }
    }

    /// Settings are applied on the next foreground tick, one property command
    /// at a time with CTS handling. Existing callers retain the old defaults.
    pub fn set_processing(&mut self, processing: Processing) {
        self.processing = Some(processing);
    }

    fn processing_pending(&self, mode: ShortwaveMode) -> bool {
        mode == ShortwaveMode::Am
            && self.processing.is_some_and(|processing| {
                self.configured_filter != Some(processing.filter_property())
                    || self.configured_attenuation != Some(processing.attenuation_property())
            })
    }

    pub fn tick(
        &mut self,
        bus: &mut impl Transport,
        now: u32,
        tuning: Tuning,
        mut read_patch: impl FnMut(u16, &mut [u8]) -> bool,
    ) -> Result<Option<Update>, u8> {
        let needs_tune = matches!(self.phase, Phase::Ready)
            && (self.tuned != Some(tuning) || self.processing_pending(tuning.mode));
        if (!needs_tune || self.cts_wait_started.is_some())
            && now.wrapping_sub(self.at) >= 0x8000_0000
        {
            return Ok(None);
        }
        let phase = if needs_tune { Phase::Tune } else { self.phase };
        let result = self.tick_due(bus, now, tuning, &mut read_patch, phase);
        if let Err(code) = result {
            if code & 0x0f == CTS_PENDING {
                let started = *self.cts_wait_started.get_or_insert(now);
                if now.wrapping_sub(started) < CTS_TIMEOUT_MS {
                    self.at = now.wrapping_add(4);
                    return Ok(None);
                }
            }
        }
        self.cts_wait_started = None;
        result
    }

    fn tick_due(
        &mut self,
        bus: &mut impl Transport,
        now: u32,
        tuning: Tuning,
        mut read_patch: impl FnMut(u16, &mut [u8]) -> bool,
        phase: Phase,
    ) -> Result<Option<Update>, u8> {
        let mut update = None;
        let (phase, delay) = match phase {
            Phase::Probe => {
                bus.write(&[0x01, 0x1f, 0x05]).map_err(|e| 0x10 | e)?;
                (Phase::Identify, 500)
            }
            Phase::Identify => {
                let id = bus.read::<8>().map_err(|e| 0x20 | e)?;
                if matches!(id[1], 0 | 0xff) {
                    return Err(0x25);
                }
                self.part = id[1];
                update = Some(Update::Identified(id));
                if tuning.mode.is_ssb() {
                    // Device part-number bytes encountered for the Si4732/35
                    // family. The patch ID is verified again after download.
                    if !matches!(id[1], 0x20 | 0x23 | 0x32 | 0x35) || matches!(id[7], 0 | 0xff) {
                        return Err(0x91);
                    }
                    self.digest = Some(Sha256::new());
                    (Phase::VerifyPatch, 0)
                } else {
                    (Phase::PowerAm, 100)
                }
            }
            Phase::PowerAm => {
                bus.write(&[0x01, tuning.mode.tuner_function(), 0x05])
                    .map_err(|e| 0x30 | e)?;
                (Phase::Volume, 125)
            }
            Phase::VerifyPatch => {
                let mut block = [0; 256];
                let count = usize::from((PATCH_BYTES - self.patch_offset).min(256));
                if !read_patch(self.patch_offset, &mut block[..count]) {
                    return Err(0x92);
                }
                self.digest.as_mut().unwrap().update(&block[..count]);
                self.patch_offset += count as u16;
                if self.patch_offset == PATCH_BYTES {
                    let digest: [u8; 32] = self.digest.take().unwrap().finalize().into();
                    if digest != PATCH_SHA256 {
                        return Err(0x93);
                    }
                    self.patch_offset = 0;
                    update = Some(Update::PatchProgress(0));
                    (Phase::PatchPower, 100)
                } else {
                    (Phase::VerifyPatch, 0)
                }
            }
            Phase::PatchPower => {
                bus.write(&[0x01, 0x31, 0x05]).map_err(|e| 0xa0 | e)?;
                self.patch_started = now;
                (Phase::PatchFrame, 125)
            }
            Phase::PatchFrame => {
                if now.wrapping_sub(self.patch_started) >= 20_000 {
                    return Err(0xa5);
                }
                bus.read::<1>().map_err(|e| 0xa0 | e)?;
                if self.patch_offset == PATCH_BYTES {
                    bus.write(&[0x10]).map_err(|e| 0xb0 | e)?;
                    (Phase::PatchRevision, 3)
                } else {
                    let mut frame = [0; 8];
                    if !read_patch(self.patch_offset, &mut frame) {
                        return Err(0x92);
                    }
                    if !matches!(frame[0], 0x15 | 0x16) {
                        return Err(0x93);
                    }
                    bus.write(&frame).map_err(|e| 0xa0 | e)?;
                    self.patch_offset += 8;
                    if self.patch_offset % 2048 == 0 {
                        update = Some(Update::PatchProgress(
                            (u32::from(self.patch_offset) * 100 / u32::from(PATCH_BYTES)) as u8,
                        ));
                    }
                    (Phase::PatchFrame, 1)
                }
            }
            Phase::PatchRevision => {
                let reply = bus.read::<9>().map_err(|e| 0xb0 | e)?;
                if reply[1] != self.part || reply[4..6] != PATCH_ID {
                    return Err(0xb5);
                }
                update = Some(Update::Patched);
                (Phase::Volume, 0)
            }
            Phase::Volume => {
                bus.read::<1>().map_err(|e| 0x30 | e)?;
                // Stock uses RX_VOLUME=63. The radio's volume knob remains in circuit.
                bus.write(&[0x12, 0, 0x40, 0, 0, 63])
                    .map_err(|e| 0x40 | e)?;
                (Phase::Tune, 12)
            }
            Phase::Tune => {
                bus.read::<1>().map_err(|e| 0x40 | e)?;
                if self.processing_pending(tuning.mode) {
                    // The pending predicate guarantees Some and AM mode.
                    let processing = self.processing.unwrap();
                    let filter = processing.filter_property();
                    let attenuation = processing.attenuation_property();
                    let (property, value) = if self.configured_filter != Some(filter) {
                        (0x3102_u16, filter)
                    } else {
                        (0x3302_u16, attenuation)
                    };
                    let [ph, pl] = property.to_be_bytes();
                    let [vh, vl] = value.to_be_bytes();
                    bus.write(&[0x12, 0, ph, pl, vh, vl])
                        .map_err(|e| 0xc0 | e)?;
                    if property == 0x3102 {
                        self.configured_filter = Some(value);
                    } else {
                        self.configured_attenuation = Some(value);
                    }
                    update = Some(Update::Tuning);
                    (Phase::Tune, 12)
                } else if tuning.mode.is_ssb() && self.configured_mode != Some(tuning.mode) {
                    let bandwidth = if tuning.mode == ShortwaveMode::Cw {
                        [0x90, 0x04]
                    } else {
                        [0x90, 0x12]
                    };
                    bus.write(&[0x12, 0, 0x01, 0x01, bandwidth[0], bandwidth[1]])
                        .map_err(|e| 0xc0 | e)?;
                    self.configured_mode = Some(tuning.mode);
                    update = Some(Update::Tuning);
                    (Phase::Tune, 12)
                } else if tuning.mode.is_ssb() && self.configured_bfo != Some(tuning.bfo) {
                    let [high, low] = tuning.bfo.to_be_bytes();
                    bus.write(&[0x12, 0, 0x01, 0, high, low])
                        .map_err(|e| 0xc0 | e)?;
                    self.configured_bfo = Some(tuning.bfo);
                    update = Some(Update::Tuning);
                    (Phase::Tune, 12)
                } else {
                    let sideband = match tuning.mode {
                        ShortwaveMode::Am | ShortwaveMode::Fm => 0,
                        ShortwaveMode::Lsb => 0x40,
                        ShortwaveMode::Usb | ShortwaveMode::Cw => 0x80,
                    };
                    let [high, low] = tuning.chip_frequency().to_be_bytes();
                    if tuning.mode == ShortwaveMode::Fm {
                        bus.write(&[0x20, 0, high, low, 0]).map_err(|e| 0x50 | e)?;
                    } else {
                        bus.write(&[0x40, sideband, high, low, 0, 1])
                            .map_err(|e| 0x50 | e)?;
                    }
                    self.tuned = Some(tuning);
                    self.tune_started = now;
                    update = Some(Update::Tuning);
                    (Phase::QueryComplete, 85)
                }
            }
            Phase::QueryComplete => {
                bus.write(&[0x14]).map_err(|e| 0x50 | e)?;
                (Phase::ReadComplete, 3)
            }
            Phase::ReadComplete => {
                let [status] = bus.read::<1>().map_err(|e| 0x50 | e)?;
                if status & 1 != 0 {
                    (Phase::TuneStatus, 0)
                } else if now.wrapping_sub(self.tune_started) >= 300 {
                    return Err(0x55);
                } else {
                    (Phase::QueryComplete, 10)
                }
            }
            Phase::TuneStatus => {
                // A changed UI selection must not change the response format
                // of a command already issued to the tuner.
                let pending = self.tuned.unwrap_or(tuning);
                let opcode = if pending.mode == ShortwaveMode::Fm {
                    0x22
                } else {
                    0x42
                };
                bus.write(&[opcode, 1]).map_err(|e| 0x60 | e)?;
                (Phase::ReadTune, 3)
            }
            Phase::ReadTune => {
                let reply = bus.read::<8>().map_err(|e| 0x60 | e)?;
                if !self.tuned.is_some_and(|sent| {
                    sent.chip_frequency() == u16::from_be_bytes([reply[2], reply[3]])
                }) {
                    return Err(0x65);
                }
                if Some(tuning) == self.tuned {
                    update = Some(Update::Tuned {
                        part: self.part,
                        rssi: reply[4],
                        snr: reply[5],
                    });
                }
                (Phase::Ready, 750)
            }
            Phase::Ready => {
                let opcode = if tuning.mode == ShortwaveMode::Fm {
                    0x23
                } else {
                    0x43
                };
                bus.write(&[opcode, 0]).map_err(|e| 0x70 | e)?;
                (Phase::ReadQuality, 3)
            }
            Phase::ReadQuality => {
                let pending = self.tuned.unwrap_or(tuning);
                let (rssi, snr) = if pending.mode == ShortwaveMode::Fm {
                    let reply = bus.read::<8>().map_err(|e| 0x70 | e)?;
                    (reply[4], reply[5])
                } else {
                    let reply = bus.read::<6>().map_err(|e| 0x70 | e)?;
                    (reply[4], reply[5])
                };
                if Some(tuning) == self.tuned {
                    update = Some(Update::Quality {
                        part: self.part,
                        rssi,
                        snr,
                    });
                }
                (Phase::Ready, 750)
            }
        };
        self.phase = phase;
        self.at = now.wrapping_add(delay);
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn am(khz: u32) -> Tuning {
        Tuning {
            khz,
            mode: ShortwaveMode::Am,
            bfo: 0,
        }
    }

    #[derive(Default)]
    struct Mock {
        writes: Vec<Vec<u8>>,
        reads: Vec<usize>,
        reply: [u8; 9],
        error: Option<u8>,
        read_error: Option<u8>,
    }

    impl Transport for Mock {
        fn write(&mut self, command: &[u8]) -> Result<(), u8> {
            self.writes.push(command.to_vec());
            self.error.map_or(Ok(()), Err)
        }

        fn read<const N: usize>(&mut self) -> Result<[u8; N], u8> {
            self.reads.push(N);
            self.read_error
                .map_or_else(|| Ok(self.reply[..N].try_into().unwrap()), Err)
        }
    }

    #[test]
    fn am_processing_is_bounded_and_bypassed_for_ssb() {
        let tuning = am(6070);
        let mut session = Session::new(0);
        session.phase = Phase::Ready;
        session.tuned = Some(tuning);
        session.set_processing(Processing {
            filter: AmFilter::Voice3k,
            soft_mute: true,
        });
        let mut bus = Mock::default();
        assert_eq!(
            session.tick(&mut bus, 0, tuning, |_, _| false),
            Ok(Some(Update::Tuning))
        );
        assert_eq!(bus.writes, [std::vec![0x12, 0, 0x31, 0x02, 1, 2]]);
        assert_eq!(session.tick(&mut bus, 11, tuning, |_, _| false), Ok(None));
        assert_eq!(bus.writes.len(), 1);
        assert_eq!(
            session.tick(&mut bus, 12, tuning, |_, _| false),
            Ok(Some(Update::Tuning))
        );
        assert_eq!(bus.writes[1], [0x12, 0, 0x33, 0x02, 0, 8]);
        assert_eq!(
            session.tick(&mut bus, 24, tuning, |_, _| false),
            Ok(Some(Update::Tuning))
        );
        assert_eq!(bus.writes[2][0], 0x40);
        let mut ssb = Session::new(0);
        ssb.phase = Phase::Tune;
        ssb.at = 0;
        ssb.set_processing(Processing {
            filter: AmFilter::Narrow1k,
            soft_mute: true,
        });
        bus.writes.clear();
        assert_eq!(
            ssb.tick(
                &mut bus,
                0,
                Tuning {
                    mode: ShortwaveMode::Usb,
                    ..tuning
                },
                |_, _| false
            ),
            Ok(Some(Update::Tuning))
        );
        assert_eq!(&bus.writes[0][2..4], &[1, 1]); // SSB patch property, not AM 0x3102.
    }

    #[test]
    fn changed_selection_does_not_reinterpret_pending_tuner_reply() {
        let mut session = Session::new(0);
        let mut bus = Mock::default();
        session.phase = Phase::TuneStatus;
        session.tuned = Some(am(6070));
        session.at = 0;
        let next = Tuning {
            khz: 98100,
            mode: ShortwaveMode::Fm,
            bfo: 0,
        };
        assert_eq!(session.tick(&mut bus, 0, next, |_, _| false), Ok(None));
        assert_eq!(bus.writes, [std::vec![0x42, 1]]);
        session.phase = Phase::ReadQuality;
        session.at = 0;
        assert_eq!(session.tick(&mut bus, 0, next, |_, _| false), Ok(None));
        assert_eq!(bus.reads, [6]); // AM result remains six bytes despite FM UI.
    }

    #[test]
    fn library_id_and_confirmed_sw_frequency_precede_audio_request() {
        let mut bus = Mock::default();
        let mut session = Session::new(0);
        assert_eq!(session.tick(&mut bus, 99, am(6070), |_, _| false), Ok(None));
        assert!(bus.writes.is_empty());
        assert_eq!(
            session.tick(&mut bus, 100, am(6070), |_, _| false),
            Ok(None)
        );
        let id = [0x80, 0x20, 0x35, 0x30, 0, 0, 0x41, 6];
        bus.reply[..8].copy_from_slice(&id);
        assert_eq!(
            session.tick(&mut bus, 600, am(6070), |_, _| false),
            Ok(Some(Update::Identified(id)))
        );
        assert_eq!(
            session.tick(&mut bus, 700, am(6070), |_, _| false),
            Ok(None)
        );
        assert_eq!(
            session.tick(&mut bus, 825, am(6070), |_, _| false),
            Ok(None)
        );
        assert_eq!(
            session.tick(&mut bus, 837, am(6070), |_, _| false),
            Ok(Some(Update::Tuning))
        );
        assert_eq!(
            session.tick(&mut bus, 922, am(6070), |_, _| false),
            Ok(None)
        );
        bus.reply[0] = 0x81;
        assert_eq!(
            session.tick(&mut bus, 925, am(6070), |_, _| false),
            Ok(None)
        );
        assert_eq!(
            session.tick(&mut bus, 926, am(6070), |_, _| false),
            Ok(None)
        );
        bus.reply = [0x80, 0, 0x17, 0xb6, 41, 17, 0, 1, 0];
        assert_eq!(
            session.tick(&mut bus, 929, am(6070), |_, _| false),
            Ok(Some(Update::Tuned {
                part: 0x20,
                rssi: 41,
                snr: 17
            }))
        );
        assert_eq!(
            bus.writes,
            [
                &[1, 0x1f, 5][..],
                &[1, 0x11, 5],
                &[0x12, 0, 0x40, 0, 0, 63],
                &[0x40, 0, 0x17, 0xb6, 0, 1],
                &[0x14],
                &[0x42, 1],
            ]
        );
        assert_eq!(
            session.tick(&mut bus, 930, am(6075), |_, _| false),
            Ok(Some(Update::Tuning))
        );
        assert_eq!(bus.writes.last().unwrap(), &[0x40, 0, 0x17, 0xbb, 0, 1]);
    }

    #[test]
    fn rapid_changes_settle_only_on_the_final_mode_and_retry_is_bounded() {
        let mut switch = ModeSwitch::default();
        for (at, mode) in [
            (0, ShortwaveMode::Am),
            (80, ShortwaveMode::Lsb),
            (155, ShortwaveMode::Usb),
        ] {
            assert!(!switch.settled(Some(mode), at));
        }
        assert!(!switch.settled(Some(ShortwaveMode::Usb), 314));
        assert!(switch.settled(Some(ShortwaveMode::Usb), 315));
        assert!(switch.retry(0x56));
        assert!(!switch.retry(0x56));
        switch.reset_retry();
        assert!(!switch.retry(0x93));
        assert!(switch.retry(0x23));
    }

    #[test]
    fn busy_status_is_polled_and_fm_uses_ten_kilohertz_words() {
        let mut bus = Mock::default();
        let mut session = Session::new(0);
        let tune = Tuning::from_selection(98150, 0, ShortwaveMode::Fm, 0);
        let mut tick = |bus: &mut Mock, now| session.tick(bus, now, tune, |_, _| false);
        assert_eq!(tick(&mut bus, 100), Ok(None));
        bus.read_error = Some(CTS_PENDING);
        assert_eq!(tick(&mut bus, 600), Ok(None));
        assert_eq!(bus.writes.len(), 1);
        bus.read_error = None;
        bus.reply[1] = 0x20;
        assert_eq!(tick(&mut bus, 603), Ok(None));
        assert!(matches!(
            tick(&mut bus, 604),
            Ok(Some(Update::Identified(_)))
        ));
        assert_eq!(tick(&mut bus, 704), Ok(None));
        assert_eq!(bus.writes[1], [1, 0x10, 5]);
        assert_eq!(tick(&mut bus, 829), Ok(None));
        assert_eq!(tick(&mut bus, 841), Ok(Some(Update::Tuning)));
        assert_eq!(bus.writes.last().unwrap(), &[0x20, 0, 0x26, 0x57, 0]);
        assert_eq!(tick(&mut bus, 926), Ok(None));
        bus.reply[0] = 0x81;
        assert_eq!(tick(&mut bus, 929), Ok(None));
        assert_eq!(tick(&mut bus, 930), Ok(None));
        assert_eq!(bus.writes.last().unwrap(), &[0x22, 1]);
        bus.reply = [0x80, 0, 0x26, 0x57, 52, 26, 0, 0, 0];
        assert_eq!(
            tick(&mut bus, 933),
            Ok(Some(Update::Tuned {
                part: 0x20,
                rssi: 52,
                snr: 26
            }))
        );
        assert_eq!(tick(&mut bus, 1683), Ok(None));
        assert_eq!(bus.writes.last().unwrap(), &[0x23, 0]);
        assert_eq!(
            tick(&mut bus, 1686),
            Ok(Some(Update::Quality {
                part: 0x20,
                rssi: 52,
                snr: 26
            }))
        );
        assert_eq!(
            Tuning::from_selection(7100, 550, ShortwaveMode::Usb, 50),
            Tuning {
                khz: 7101,
                mode: ShortwaveMode::Usb,
                bfo: -400
            }
        );
    }

    #[test]
    fn foreign_patch_never_powers_up_patch_mode() {
        let mut session = Session::new(0);
        let mut bus = Mock::default();
        let tuning = Tuning {
            khz: 7100,
            mode: ShortwaveMode::Lsb,
            bfo: 0,
        };
        bus.reply = [0x80, 0x20, b'6', b'0', 0, 0, b'A', 6, 0];
        assert_eq!(session.tick(&mut bus, 100, tuning, |_, _| false), Ok(None));
        assert!(matches!(
            session.tick(&mut bus, 600, tuning, |_, _| false),
            Ok(Some(Update::Identified(_)))
        ));
        for tick in 601..700 {
            match session.tick(&mut bus, tick, tuning, |_, output| {
                output.fill(0xff);
                true
            }) {
                Ok(None) => {}
                Err(0x93) => {
                    assert_eq!(bus.writes, [&[0x01, 0x1f, 0x05][..]]);
                    return;
                }
                other => panic!("unexpected event {other:?}"),
            }
        }
        panic!("unrecognized NOR payload accepted");
    }

    #[test]
    fn stock_patch_transfers_and_confirms_id_when_nor_fixture_is_supplied() {
        let Some(path) = std::env::var_os("UV98_STOCK_NOR_FOR_TEST") else {
            return;
        };
        let nor = std::fs::read(path).unwrap();
        let patch = &nor[0x126000..0x126000 + usize::from(PATCH_BYTES)];
        let mut bus = Mock::default();
        let mut session = Session::new(0);
        let mut tuning = Tuning {
            khz: 7100,
            mode: ShortwaveMode::Lsb,
            bfo: 50,
        };
        let mut patched = false;
        for now in 0..10_000 {
            bus.reply = match session.phase {
                Phase::Identify => [0x80, 0x20, b'6', b'0', 0, 0, b'A', 6, 0],
                Phase::PatchRevision => [0x80, 0x20, 0, 0, 0x9d, 0x29, 0, 0, 0],
                Phase::ReadComplete => [0x81, 0, 0, 0, 0, 0, 0, 0, 0],
                Phase::ReadTune => [0x80, 0, 0x1b, 0xbc, 42, 15, 0, 1, 0],
                _ => [0x80, 0, 0, 0, 0, 0, 0, 0, 0],
            };
            let mut read = |offset: u16, output: &mut [u8]| {
                let start = usize::from(offset);
                output.copy_from_slice(&patch[start..start + output.len()]);
                true
            };
            match session.tick(&mut bus, now, tuning, &mut read).unwrap() {
                Some(Update::Patched) => patched = true,
                Some(Update::Tuned {
                    rssi: 42, snr: 15, ..
                }) => {
                    assert!(patched);
                    assert_eq!(bus.writes[1], [1, 0x31, 5]);
                    for (command, expected) in bus.writes[2..2 + patch.len() / 8]
                        .iter()
                        .zip(patch.chunks_exact(8))
                    {
                        assert_eq!(command, expected);
                    }
                    assert!(bus.writes.contains(&std::vec![0x12, 0, 1, 1, 0x90, 0x12]));
                    assert!(bus.writes.contains(&std::vec![0x12, 0, 1, 0, 0, 50]));
                    assert!(bus
                        .writes
                        .contains(&std::vec![0x40, 0x40, 0x1b, 0xbc, 0, 1]));
                    tuning.mode = ShortwaveMode::Cw;
                    tuning.bfo = -500;
                    for delta in [1, 13, 25] {
                        assert_eq!(
                            session.tick(&mut bus, now + delta, tuning, &mut read),
                            Ok(Some(Update::Tuning))
                        );
                    }
                    assert!(bus.writes.ends_with(&[
                        std::vec![0x12, 0, 1, 1, 0x90, 0x04],
                        std::vec![0x12, 0, 1, 0, 0xfe, 0x0c],
                        std::vec![0x40, 0x80, 0x1b, 0xbc, 0, 1],
                    ]));
                    return;
                }
                _ => {}
            }
        }
        panic!("no confirmed SSB tuning");
    }

    #[test]
    fn probe_nack_and_missing_tune_completion_are_reported() {
        let mut bus = Mock {
            error: Some(1),
            ..Mock::default()
        };
        let mut session = Session::new(0);
        assert_eq!(
            session.tick(&mut bus, 100, am(6070), |_, _| false),
            Err(0x11)
        );
        bus.error = None;
        session.phase = Phase::ReadComplete;
        session.tune_started = 100;
        assert_eq!(
            session.tick(&mut bus, 399, am(6070), |_, _| false),
            Ok(None)
        );
        assert_eq!(
            session.tick(&mut bus, 409, am(6070), |_, _| false),
            Ok(None)
        );
        assert_eq!(
            session.tick(&mut bus, 412, am(6070), |_, _| false),
            Err(0x55)
        );
    }
}
