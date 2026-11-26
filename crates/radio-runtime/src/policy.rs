//! Read-only channel policy. Unsupported records fail explicitly; no saved
//! frequencies, tones, calibration or operation permissions are normalized.
use crate::{scan, Audio};
pub use uv98_band_plan::{aviation_band, operating_range as range};
use uv98_codeplug::channels::{Bandwidth, Channel, OperationMode};
use uv98_handheld_ui::controller::{Mode, ScanRange, View, STEP_10HZ, TX_CTCSS};
use uv98_rf_sequence::{Modulation, RxRequest, TxRequest, Width};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub squelch: u8,
    pub mic_gain: u8,
    pub fm_volume: [u8; 2],
    pub am_volume: [u8; 2],
    pub squelch_delay: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overrides {
    pub power: u8,
    pub modulation: Option<Modulation>,
    pub monitor: bool,
}
impl Default for Overrides {
    fn default() -> Self {
        Self {
            power: 0,
            modulation: None,
            monitor: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub receive: Option<RxRequest>,
    pub transmit: Option<TxRequest>,
    pub audio: Audio,
    pub frequency: u32,
    pub route: u8,
    pub power: u8,
}

impl Profile {
    /// A discovered CTCSS/DCS gate applies only to the FM receive half.
    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn with_rx_tone_override(mut self, tone: Option<u16>) -> Self {
        if let (Some(rx), Some(raw)) = (&mut self.receive, tone) {
            if rx.modulation == Modulation::Fm {
                rx.tone = raw;
                // Search recognizes standard DCS, independent of a memory
                // record's alternate codec setting on the TX half.
                rx.tone_mode = 0;
                self.audio.tone_required = true;
            }
        }
        self
    }
}

/// Monitor, displayed power and the inactive TX side do not retune a live
/// receiver. Actual receive parameters (including AM/FM) still require tuning.
pub fn receive_changed(previous: Option<Profile>, next: Option<Profile>) -> bool {
    previous.map(|p| p.receive) != next.map(|p| p.receive)
}

/// A confirmed spectrum restoration leaves the receiver on `confirmed`. Reuse
/// the freshly computed profile as the applied cache only when ordinary policy
/// still asks for exactly that receive plan; otherwise the caller must retune
/// once instead of trusting a partial match.
pub fn restored_current(wanted: Option<Profile>, confirmed: RxRequest) -> Option<Profile> {
    wanted.filter(|profile| profile.receive == Some(confirmed))
}

/// Temporary listen on a confirmed RX plan: audio is unconditionally open and
/// the original channel/VFO context is never replaced. The receive fields are
/// exactly the plan the adapter reported restored, so no hidden retune occurs.
pub fn listen_profile(request: RxRequest, cfg: Settings) -> Profile {
    let [gain, dac] = match request.modulation {
        Modulation::Fm => cfg.fm_volume,
        Modulation::Am => cfg.am_volume,
    };
    let route = uv98_band_plan::select_routes(
        request.frequency_10hz,
        uv98_band_plan::SelectorMode::Primary,
    )
    .and_then(|routes| routes.rx_code())
    .unwrap_or(0);
    Profile {
        receive: Some(request),
        transmit: None,
        audio: Audio {
            modulation: request.modulation,
            gain,
            dac,
            squelch: 0,
            monitor: true,
            tone_required: false,
        },
        frequency: request.frequency_10hz,
        route,
        power: 0,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    MissingChannel,
    InactiveChannel,
    OperationMode,
    Modulation,
    Bandwidth,
    Frequency,
    Tone,
    Options,
    Settings,
    Power,
}

pub fn profile(
    view: View,
    channel: Option<Channel<'_>>,
    cfg: Settings,
    setting: Overrides,
) -> Result<Profile, Error> {
    if cfg.squelch > 10
        || cfg.mic_gain > 31
        || cfg.fm_volume[0] > 63
        || cfg.fm_volume[1] > 15
        || cfg.am_volume[0] > 63
        || cfg.am_volume[1] > 15
    {
        return Err(Error::Settings);
    }
    if setting.power > 2 {
        return Err(Error::Power);
    }
    let cb = view.cb_for_slot(view.active_slot);
    if let Some(cb) = cb {
        if view.mode != Mode::Vfo || view.vfo_10hz != cb.frequency_10hz() {
            return Err(Error::Frequency);
        }
    }
    let (mut rx, mut tx, mut rx_tone, mut tx_tone, mut width, modulation, mode, tone_mode, option) =
        match view.mode {
            Mode::Vfo => (
                view.vfo_10hz,
                uv98_handheld_ui::controller::vfo_tx_frequency(&view).unwrap_or(0),
                0,
                match TX_CTCSS.get(usize::from(view.tx_tone_index)) {
                    Some(0) => 0,
                    Some(tone) => 0x1000 | tone,
                    None => return Err(Error::Tone),
                },
                Width::Wide,
                Modulation::Fm,
                OperationMode::ReceiveTransmit,
                0,
                0,
            ),
            Mode::Channel => {
                let ch = channel.ok_or(Error::MissingChannel)?;
                if !ch.is_active() {
                    return Err(Error::InactiveChannel);
                }
                let mode = ch.operation_mode().ok_or(Error::OperationMode)?;
                let modulation = match ch.raw()[0] {
                    0 => Modulation::Fm,
                    1 => Modulation::Am,
                    _ => return Err(Error::Modulation),
                };
                let width = match ch.bandwidth().ok_or(Error::Bandwidth)? {
                    Bandwidth::Wide => Width::Wide,
                    Bandwidth::Narrow => Width::Narrow,
                };
                (
                    ch.rx_frequency_code(),
                    ch.tx_frequency_code(),
                    ch.rx_tone_raw(),
                    ch.tx_tone_raw(),
                    width,
                    modulation,
                    mode,
                    ch.raw()[18] >> 5,
                    ch.raw()[19] & 15,
                )
            }
        };
    if view.reverse {
        core::mem::swap(&mut rx, &mut tx);
        core::mem::swap(&mut rx_tone, &mut tx_tone);
    }
    let modulation = if let Some(cb) = cb {
        // CB is an explicit per-A mode. AM reception stays on the channel;
        // experimental pseudo-AM uses narrow FM TX exactly 2 kHz above it.
        // Generic shift, reverse and modulation overrides do not alter this.
        rx = cb.frequency_10hz();
        tx = cb.tx_frequency_10hz();
        rx_tone = 0;
        tx_tone = 0;
        // Use the existing narrow calibration branch for CB FM. This is not
        // a claim of measured CB deviation or regulatory RF qualification.
        width = Width::Narrow;
        if cb.is_am() {
            Modulation::Am
        } else {
            Modulation::Fm
        }
    } else if aviation_band(rx) {
        Modulation::Am
    } else {
        setting.modulation.unwrap_or(modulation)
    };
    if tone_mode > 3 {
        return Err(Error::Options);
    }
    let receive = if mode == OperationMode::TransmitOnly {
        None
    } else {
        uv98_rf_sequence::tone(rx_tone, tone_mode).map_err(|_| Error::Tone)?;
        range(rx).ok_or(Error::Frequency)?;
        Some(RxRequest {
            frequency_10hz: rx,
            width,
            modulation,
            level: cfg.squelch,
            tone: rx_tone,
            tone_mode,
            option_nibble: option,
            squelch_delay: cfg.squelch_delay,
        })
    };
    // The transmitter remains FM. Only the explicit CB AM context permits
    // shifted FM alongside AM reception; all other AM stays receive-only.
    let transmit = if mode == OperationMode::ReceiveOnly
        || (modulation != Modulation::Fm && cb.is_none())
        || aviation_band(tx)
        || (view.mode == Mode::Channel && channel.is_some_and(|ch| ch.power().is_none()))
        || range(tx).is_none()
        || uv98_rf_sequence::tone(tx_tone, tone_mode).is_err()
    {
        None
    } else {
        Some(TxRequest {
            frequency_10hz: tx,
            width,
            tone: tx_tone,
            tone_mode,
            option_nibble: option,
            power: setting.power,
            mic_gain: cfg.mic_gain,
        })
    };
    let frequency = if receive.is_some() { rx } else { tx };
    let route = uv98_band_plan::select_routes(frequency, uv98_band_plan::SelectorMode::Primary)
        .and_then(|r| r.rx_code())
        .ok_or(Error::Frequency)?;
    let [gain, dac] = match modulation {
        Modulation::Fm => cfg.fm_volume,
        Modulation::Am => cfg.am_volume,
    };
    Ok(Profile {
        receive,
        transmit,
        frequency,
        route,
        power: setting.power,
        audio: Audio {
            modulation,
            gain,
            dac,
            squelch: cfg.squelch,
            monitor: setting.monitor,
            tone_required: rx_tone != 0,
        },
    })
}

pub fn scan_target(view: View) -> Result<scan::Target, Error> {
    scan_target_with_range(&view, None)
}

pub fn scan_target_with_range(
    view: &View,
    custom: Option<ScanRange>,
) -> Result<scan::Target, Error> {
    if view.cb_for_slot(view.active_slot).is_some() {
        // A general +/-500 kHz VFO sweep is not the irregular CB channel grid.
        return Err(Error::Options);
    }
    if view.mode == Mode::Channel {
        return Ok(scan::Target::Channels);
    }
    if let Some(range) = custom {
        range.validate().map_err(|_| Error::Frequency)?;
        return Ok(scan::Target::Vfo {
            low: range.lower_10hz,
            high: range.upper_10hz,
            step: range.step_10hz,
        });
    }
    let (low, high) = range(view.vfo_10hz).ok_or(Error::Frequency)?;
    let step = *STEP_10HZ
        .get(usize::from(view.step_index))
        .ok_or(Error::Settings)?;
    Ok(scan::Target::Vfo {
        low: view.vfo_10hz.saturating_sub(50_000).max(low),
        high: view.vfo_10hz.saturating_add(50_000).min(high),
        step,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv98_handheld_ui::controller::Controller;
    fn cfg() -> Settings {
        Settings {
            squelch: 1,
            mic_gain: 31,
            fm_volume: [58, 15],
            am_volume: [50, 12],
            squelch_delay: 10,
        }
    }

    #[test]
    fn cb_a_keeps_explicit_rx_and_tx_modes_and_does_not_leak_into_b() {
        use uv98_handheld_ui::{cb::CbChannel, controller::Slot};
        let mut v = view();
        v.mode = Mode::Vfo;
        v.active_slot = Slot::A;
        v.cb = Some(CbChannel::default());
        v.vfo_10hz = v.cb.unwrap().frequency_10hz();
        // Conflicting ordinary VFO settings never escape the CB grid.
        v.shift_index = 5;
        v.tx_tone_index = 1;
        v.reverse = true;
        let am = profile(
            v,
            None,
            cfg(),
            Overrides {
                modulation: Some(Modulation::Fm),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(am.receive.unwrap().frequency_10hz, 2_713_500);
        assert_eq!(am.receive.unwrap().modulation, Modulation::Am);
        let tx = am.transmit.unwrap();
        assert_eq!(tx.frequency_10hz, 2_713_700);
        assert_eq!(tx.width, Width::Narrow);
        assert_eq!((tx.tone, tx.tone_mode, tx.option_nibble), (0, 0, 0));
        assert_eq!(scan_target(v), Err(Error::Options));
        v.cb.as_mut().unwrap().am = false;
        let fm = profile(
            v,
            None,
            cfg(),
            Overrides {
                modulation: Some(Modulation::Am),
                ..Default::default()
            },
        )
        .unwrap();
        let tx = fm.transmit.unwrap();
        assert_eq!(tx.frequency_10hz, 2_713_500);
        assert_eq!(tx.tone, 0);
        assert_eq!(tx.width, Width::Narrow);
        assert_eq!(fm.receive.unwrap().tone, 0);
        // A's CB metadata is global in View, but B keeps its own normal VFO.
        v.active_slot = Slot::B;
        v.vfo_10hz = 43_500_000;
        v.reverse = false;
        v.shift_index = 0;
        let b = profile(v, None, cfg(), Overrides::default()).unwrap();
        assert_eq!(b.transmit.unwrap().frequency_10hz, 43_500_000);
        assert_eq!(b.transmit.unwrap().width, Width::Wide);
        assert_ne!(b.transmit.unwrap().tone, 0);
        assert!(scan_target(v).is_ok());
        v.active_slot = Slot::A;
        assert_eq!(
            profile(v, None, cfg(), Overrides::default()),
            Err(Error::Frequency)
        );
    }

    #[test]
    fn airband_selects_am_and_disallows_tx_even_with_fm_override() {
        let mut v = view();
        v.mode = Mode::Vfo;
        for f in [11_800_000, 12_640_000, 13_699_999] {
            v.vfo_10hz = f;
            let p = profile(
                v,
                None,
                cfg(),
                Overrides {
                    modulation: Some(Modulation::Fm),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(p.receive.unwrap().modulation, Modulation::Am);
            assert!(p.transmit.is_none());
        }
        v.vfo_10hz = 13_700_000;
        assert_eq!(
            profile(v, None, cfg(), Overrides::default())
                .unwrap()
                .audio
                .modulation,
            Modulation::Fm
        );
    }

    #[test]
    fn vfo_shift_and_reverse_swap_the_effective_rx_and_tx() {
        let mut v = view();
        v.mode = Mode::Vfo;
        v.vfo_10hz = 43_500_000;
        v.shift_index = 5;
        for reverse in [false, true] {
            v.reverse = reverse;
            let p = profile(v, None, cfg(), Overrides::default()).unwrap();
            assert_eq!(
                p.receive.unwrap().frequency_10hz,
                if reverse { 43_000_000 } else { 43_500_000 }
            );
            assert_eq!(
                p.transmit.unwrap().frequency_10hz,
                if reverse { 43_500_000 } else { 43_000_000 }
            );
        }
    }

    #[test]
    fn shifted_vfo_uses_the_configured_tone_only_on_the_tx_half() {
        let mut v = view();
        v.mode = Mode::Vfo;
        v.vfo_10hz = 43_180_000;
        v.shift_index = 3;
        v.tx_tone_index = TX_CTCSS.iter().position(|&tone| tone == 770).unwrap() as u8;

        let profile_at = |view| profile(view, None, cfg(), Overrides::default()).unwrap();
        let normal = profile_at(v);
        let rx = normal.receive.unwrap();
        let tx = normal.transmit.unwrap();
        assert_eq!((rx.frequency_10hz, rx.tone), (43_180_000, 0));
        assert_eq!((tx.frequency_10hz, tx.tone), (43_020_000, 0x1302));

        v.reverse = true;
        let reverse = profile_at(v);
        let rx = reverse.receive.unwrap();
        let tx = reverse.transmit.unwrap();
        assert_eq!((rx.frequency_10hz, rx.tone), (43_020_000, 0x1302));
        assert_eq!((tx.frequency_10hz, tx.tone), (43_180_000, 0));
    }

    #[test]
    fn listen_profile_opens_audio_without_changing_receive_plan() {
        let request = RxRequest {
            frequency_10hz: 14_550_000,
            width: Width::Wide,
            level: 1,
            tone: 0,
            tone_mode: 0,
            option_nibble: 0,
            squelch_delay: 10,
            modulation: Modulation::Fm,
        };
        let profile = listen_profile(request, cfg());
        assert_eq!(profile.receive, Some(request));
        assert_eq!(profile.frequency, request.frequency_10hz);
        assert_eq!(profile.audio.modulation, request.modulation);
        assert_eq!(profile.audio.squelch, 0);
        assert!(profile.audio.monitor);
        assert!(!profile.audio.tone_required);
        assert_ne!(profile.route, 0, "qualified frequency keeps a route");
        assert!(profile.transmit.is_none());
    }

    #[test]
    fn restored_cache_requires_the_complete_confirmed_receive_plan() {
        let mut v = view();
        v.mode = Mode::Vfo;
        let wanted = profile(v, None, cfg(), Overrides::default()).unwrap();
        let confirmed = wanted.receive.unwrap();
        assert_eq!(restored_current(Some(wanted), confirmed), Some(wanted));

        let mut other = confirmed;
        other.tone = 0x1302;
        assert_eq!(restored_current(Some(wanted), other), None);
        let mut other = confirmed;
        other.width = Width::Narrow;
        assert_eq!(restored_current(Some(wanted), other), None);
        let mut other = confirmed;
        other.frequency_10hz += 250;
        assert_eq!(restored_current(Some(wanted), other), None);
        let mut other = confirmed;
        other.option_nibble = 1;
        assert_eq!(restored_current(Some(wanted), other), None);
        let mut other = confirmed;
        other.level = confirmed.level.wrapping_add(1);
        assert_eq!(restored_current(Some(wanted), other), None);
        assert_eq!(restored_current(None, confirmed), None);
    }

    #[test]
    fn monitor_and_power_do_not_interrupt_reception_but_modulation_does() {
        let mut v = view();
        v.mode = Mode::Vfo;
        let original = profile(v, None, cfg(), Overrides::default()).unwrap();
        let mut settings = Overrides::default();
        settings.monitor = true;
        settings.power = 2;
        let next = profile(v, None, cfg(), settings).unwrap();
        assert!(!receive_changed(Some(original), Some(next)));
        settings.modulation = Some(Modulation::Am);
        assert!(receive_changed(
            Some(next),
            Some(profile(v, None, cfg(), settings).unwrap())
        ));
        assert!(receive_changed(Some(original), None));
    }
    fn view() -> View {
        Controller::new(&[255; 128], 14_550_000).view()
    }
    fn record() -> [u8; 48] {
        let mut r = [0u8; 48];
        r[2] = 1;
        r[6..10].copy_from_slice(&14_550_000u32.to_le_bytes());
        r[10..14].copy_from_slice(&14_490_000u32.to_le_bytes());
        r[4..6].copy_from_slice(&0x1375u16.to_le_bytes());
        r[14..16].copy_from_slice(&0x2013u16.to_le_bytes());
        r
    }
    #[test]
    fn reverse_swaps_frequencies_and_tones_without_escalating_permissions() {
        let mut r = record();
        r[1] = 1;
        let mut v = view();
        v.reverse = true;
        let p = profile(
            v,
            Some(Channel::parse(&r).unwrap()),
            cfg(),
            Overrides::default(),
        )
        .unwrap();
        assert_eq!(p.receive.unwrap().frequency_10hz, 14_490_000);
        assert_eq!(p.receive.unwrap().tone, 0x2013);
        assert!(p.transmit.is_none());
    }
    #[test]
    fn am_uses_separate_volume_never_fm_transmit() {
        let mut r = record();
        r[0] = 1;
        let p = profile(
            view(),
            Some(Channel::parse(&r).unwrap()),
            cfg(),
            Overrides::default(),
        )
        .unwrap();
        assert_eq!((p.audio.gain, p.audio.dac), (50, 12));
        assert!(p.transmit.is_none());
    }
    #[test]
    fn malformed_tx_never_substitutes_rx_but_preserves_valid_reception() {
        let mut r = record();
        r[10..14].copy_from_slice(&0u32.to_le_bytes());
        let p = profile(
            view(),
            Some(Channel::parse(&r).unwrap()),
            cfg(),
            Overrides::default(),
        )
        .unwrap();
        assert!(p.receive.is_some());
        assert!(p.transmit.is_none());
        r[0] = 255;
        assert_eq!(
            profile(
                view(),
                Some(Channel::parse(&r).unwrap()),
                cfg(),
                Overrides::default()
            ),
            Err(Error::Modulation)
        );
    }
    #[test]
    fn custom_window_stays_out_of_channel_and_cb_scan() {
        let requested = ScanRange {
            lower_10hz: 14_400_000,
            upper_10hz: 14_600_000,
            step_10hz: 2_500,
        };
        let mut v = view();
        v.mode = Mode::Vfo;
        assert_eq!(
            scan_target_with_range(&v, Some(requested)),
            Ok(scan::Target::Vfo {
                low: requested.lower_10hz,
                high: requested.upper_10hz,
                step: requested.step_10hz
            })
        );
        assert!(scan_target_with_range(
            &v,
            Some(ScanRange {
                upper_10hz: requested.upper_10hz + 1,
                ..requested
            })
        )
        .is_err());
        v.mode = Mode::Channel;
        assert_eq!(
            scan_target_with_range(&v, Some(requested)),
            Ok(scan::Target::Channels)
        );
        v.mode = Mode::Vfo;
        v.active_slot = uv98_handheld_ui::controller::Slot::A;
        v.cb = Some(uv98_handheld_ui::cb::CbChannel::default());
        assert_eq!(
            scan_target_with_range(&v, Some(requested)),
            Err(Error::Options)
        );
    }

    #[test]
    fn scan_window_clamped_and_hole_rejected() {
        let mut v = view();
        v.mode = Mode::Vfo;
        v.vfo_10hz = 84_000_000;
        assert!(matches!(
            scan_target(v),
            Ok(scan::Target::Vfo {
                low: 84_000_000,
                high: 84_050_000,
                ..
            })
        ));
        v.vfo_10hz = 70_000_000;
        assert_eq!(scan_target(v), Err(Error::Frequency));
        v.vfo_10hz = 1_000_000;
        assert_eq!(
            profile(v, None, cfg(), Overrides::default()),
            Err(Error::Frequency)
        );
    }
}
