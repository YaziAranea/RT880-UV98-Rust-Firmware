//! Read-only channel policy. Unsupported records fail explicitly; no saved
//! frequencies, tones, calibration or operation permissions are normalized.
use crate::{scan, Audio};
use uv98_codeplug::channels::{Bandwidth, Channel, OperationMode};
use uv98_handheld_ui::controller::{Mode, View, STEP_10HZ};
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

/// Monitor, displayed power and the inactive TX side do not retune a live
/// receiver. Actual receive parameters (including AM/FM) still require tuning.
pub fn receive_changed(previous: Option<Profile>, next: Option<Profile>) -> bool {
    previous.map(|p| p.receive) != next.map(|p| p.receive)
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
    let (mut rx, mut tx, mut rx_tone, mut tx_tone, width, modulation, mode, tone_mode, option) =
        match view.mode {
            Mode::Vfo => (
                view.vfo_10hz,
                view.vfo_10hz,
                0,
                0,
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
    let modulation = setting.modulation.unwrap_or(modulation);
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
    // AM transmit is deliberately unsupported. A malformed TX side does not
    // disable a valid RX side, but it can never inherit the RX frequency/tone.
    let transmit = if mode == OperationMode::ReceiveOnly
        || modulation != Modulation::Fm
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

/// Coverage of the traced route selector, not a sensitivity specification.
pub const fn range(f: u32) -> Option<(u32, u32)> {
    match f {
        1_800_000..=66_000_000 => Some((1_800_000, 66_000_000)),
        84_000_000..=99_999_999 => Some((84_000_000, 99_999_999)),
        _ => None,
    }
}
pub fn scan_target(view: View) -> Result<scan::Target, Error> {
    if view.mode == Mode::Channel {
        return Ok(scan::Target::Channels);
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
