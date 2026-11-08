//! Additional allowlist for the explicit, bounded CH002 upper-PTT trial.
//!
//! This predicate neither enables RF nor replaces the physical release guard,
//! immutable-channel policy, calibration hash check or executor fault handling.
//! Callers must derive `Profile` from the selected immutable channel and apply
//! this check both before configuration and while the trial remains active.

use crate::{policy::Profile, ptt::Input};
use uv98_codeplug::channels::Channel;
use uv98_handheld_ui::controller::{Mode, ScanState, Screen, SideAction, Slot, View};
use uv98_rf_sequence::{Modulation, TxRequest, Width};

/// Includes configuration time, measured from before the first TX operation.
pub const MAX_HELD_MS: u32 = 5_000;

const FREQUENCY_10HZ: u32 = 43_297_500;
const CHANNEL_INDEX: u16 = 1;
const UHF_ROUTE: u8 = 4;
const TRANSMIT: TxRequest = TxRequest {
    frequency_10hz: FREQUENCY_10HZ,
    width: Width::Wide,
    tone: 0x1302,
    tone_mode: 0,
    option_nibble: 0,
    power: 0,
    mic_gain: 31,
};

/// Check the original selected CH002 record before applying RAM overrides.
/// The caller must validate its catalog index separately and require BOTH
/// `record_permitted` and `permits`; `Profile` alone loses raw power/modulation.
/// Names and unrelated channel bytes are not part of the RF allowlist.
pub fn record_permitted(record: Channel<'_>) -> bool {
    let raw = record.raw();
    record.is_active()
        && raw[0] == 0
        && raw[1] == 0
        && raw[3] == 0
        && record.rx_frequency_code() == FREQUENCY_10HZ
        && record.tx_frequency_code() == FREQUENCY_10HZ
        && record.rx_tone_raw() == 0
        && record.tx_tone_raw() == 0x1302
        && raw[16] == 0
        && raw[18] >> 5 == 0
        && raw[19] & 15 == 0
}

/// Only CH002, slot A, Home and a clean physical upper PTT are eligible.
/// A matching frequency in VFO or another channel never inherits permission.
/// Power 0 is the raw LOW selector, not a measured output power in watts.
pub fn permits(view: View, profile: Profile, input: Input) -> bool {
    if !input.upper
        || input.lower
        || !input.clean
        || view.screen != Screen::Home
        || view.active_slot != Slot::A
        || view.mode != Mode::Channel
        || view.channel != Some(CHANNEL_INDEX)
        || view.entry.is_some()
        || view.locked
        || view.reverse
        || view.scan != ScanState::Stopped
        || view.side_bindings[0] != SideAction::PttA
        || view.ptt_slot != Some(Slot::A)
        || profile.frequency != FREQUENCY_10HZ
        || profile.route != UHF_ROUTE
        || profile.power != 0
        || profile.audio.modulation != Modulation::Fm
        || profile.transmit != Some(TRANSMIT)
    {
        return false;
    }
    let Some(receive) = profile.receive else {
        return false;
    };
    receive.frequency_10hz == FREQUENCY_10HZ
        && receive.modulation == Modulation::Fm
        && receive.width == Width::Wide
        && receive.tone == 0
        && receive.tone_mode == 0
        && receive.option_nibble == 0
}

/// Strictly less than five seconds; expiry also requires the caller to disarm
/// the PTT guard so a held key cannot restart. Supports a monotonic u32 clock
/// wrap. As with other modular clock checks, observations must be less than
/// 2^31 ms apart; a backwards clock change in that range fails closed.
pub fn hold_within_limit(start: u32, now: u32) -> bool {
    let elapsed = now.wrapping_sub(start);
    elapsed < 0x8000_0000 && elapsed < MAX_HELD_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{self, Overrides, Settings};
    use crate::ptt::Guard;
    use uv98_handheld_ui::controller::{Controller, Entry, EntryKind, SIDE_ACTIONS};

    const UPPER: Input = Input {
        upper: true,
        lower: false,
        clean: true,
    };
    const RELEASED: Input = Input {
        upper: false,
        lower: false,
        clean: true,
    };

    fn view() -> View {
        let mut bitmap = [0; 128];
        bitmap[0] = 1 << 1;
        let mut view = Controller::new(&bitmap, 14_550_000).view();
        view.ptt_slot = Some(Slot::A);
        view.ptt_down = true;
        view.side_down = [true, false];
        view
    }

    fn record() -> [u8; 48] {
        // Decoded CH002 shape; not a substitute for the user's hash-pinned NOR.
        let mut raw = [0; 48];
        raw[2] = 1;
        raw[6..10].copy_from_slice(&43_297_500u32.to_le_bytes());
        raw[10..14].copy_from_slice(&43_297_500u32.to_le_bytes());
        raw[14..16].copy_from_slice(&0x1302u16.to_le_bytes());
        raw
    }

    fn profile(raw: &[u8; 48]) -> Result<Profile, policy::Error> {
        policy::profile(
            view(),
            Some(Channel::parse(raw).unwrap()),
            Settings {
                squelch: 1,
                mic_gain: 31,
                fm_volume: [58, 15],
                am_volume: [50, 12],
                squelch_delay: 10,
            },
            Overrides::default(),
        )
    }

    #[test]
    fn exact_immutable_channel_profile_and_traced_uhf_route_are_eligible() {
        let raw = record();
        let before = raw;
        let p = profile(&raw).unwrap();
        let route =
            uv98_band_plan::select_routes(43_297_500, uv98_band_plan::SelectorMode::Primary)
                .unwrap();
        assert_eq!(route.rx_code(), Some(4));
        assert_eq!(route.tx_code(), 4);
        assert_eq!(view().channel, Some(1));
        assert!(record_permitted(Channel::parse(&raw).unwrap()));
        assert!(permits(view(), p, UPPER));
        assert_eq!(raw, before);
    }

    #[test]
    fn allowed_profile_with_factory_calibration_matches_independent_thumb_words() {
        use uv98_calibration::Calibration;
        use uv98_rf_sequence::{self as sequence, Step};

        // Independent original Thumb reference: research/radio018/
        // exo-tx-stock.csv and exo-tx-evidence.json, function 080053F0..080055C2.
        // The build validates the factory fixture hash. These values do not
        // establish physical RF output or safe shutdown on the actual radio.
        let calibration = Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        let p = profile(&record()).unwrap();
        assert!(permits(view(), p, UPPER));
        let plan = sequence::transmit(&calibration, p.transmit.unwrap()).unwrap();
        let steps = plan.operations();
        for (register, value) in [
            (0x38, 0xab1d), // 43297500 + factory frequency correction 65.
            (0x39, 0x0294),
            (0x51, 0x9041), // CTCSS 77.0 Hz, not the RX tone.
            (0x07, 0x0636),
            (0x7d, 0xe95f), // CFG microphone gain 31.
            (0x40, 0x34e2), // Factory wide TX deviation.
        ] {
            assert!(
                steps.contains(&Step::WriteA { register, value }),
                "missing independent oracle word A{register:02X}={value:04X}"
            );
        }
        assert!(steps.contains(&Step::AntennaPf0(false)));
        assert!(!steps.contains(&Step::AntennaPf0(true)));
        assert!(steps
            .iter()
            .all(|step| !matches!(step, Step::TxGates(route) if *route != 0 && *route != 4)));
        assert_eq!(steps[steps.len() - 2], Step::TxGates(4));
        assert_eq!(
            steps.last(),
            Some(&Step::WriteA {
                register: 0x36,
                value: 0x5cff
            }) // LOW PA code 23.
        );
    }

    #[test]
    fn physical_input_must_be_upper_alone_and_clean() {
        let p = profile(&record()).unwrap();
        for upper in [false, true] {
            for lower in [false, true] {
                for clean in [false, true] {
                    let input = Input {
                        upper,
                        lower,
                        clean,
                    };
                    assert_eq!(permits(view(), p, input), input == UPPER, "{input:?}");
                }
            }
        }
    }

    #[test]
    fn every_ui_gate_rejects_without_using_frequency_as_identity() {
        let original = view();
        let p = profile(&record()).unwrap();
        for screen in [
            Screen::Menu,
            Screen::Channels,
            Screen::ChannelDetails,
            Screen::Step,
            Screen::Calibration,
            Screen::Keys,
            Screen::Help,
            Screen::About,
            Screen::Shortcuts,
            Screen::ShortcutAction,
            Screen::Zones,
            Screen::SideKeys,
            Screen::SideAction,
            Screen::Setting,
        ] {
            assert!(!permits(View { screen, ..original }, p, UPPER));
        }
        assert!(!permits(
            View {
                active_slot: Slot::B,
                ..original
            },
            p,
            UPPER
        ));
        assert!(!permits(
            View {
                mode: Mode::Vfo,
                vfo_10hz: 43_297_500,
                ..original
            },
            p,
            UPPER
        ));
        for channel in [None, Some(0), Some(2), Some(1023)] {
            assert!(!permits(
                View {
                    channel,
                    ..original
                },
                p,
                UPPER
            ));
        }
        for kind in [EntryKind::Channel, EntryKind::Frequency] {
            let entry = Some(Entry {
                kind,
                value: 2,
                digits: 1,
            });
            assert!(!permits(View { entry, ..original }, p, UPPER));
        }
        assert!(!permits(
            View {
                locked: true,
                ..original
            },
            p,
            UPPER
        ));
        assert!(!permits(
            View {
                reverse: true,
                ..original
            },
            p,
            UPPER
        ));
        for scan in [
            ScanState::Requested,
            ScanState::Running,
            ScanState::Stopping,
        ] {
            assert!(!permits(View { scan, ..original }, p, UPPER));
        }
        for binding in SIDE_ACTIONS {
            let mut v = original;
            v.side_bindings[0] = binding;
            assert_eq!(permits(v, p, UPPER), binding == SideAction::PttA);
        }
        for ptt_slot in [None, Some(Slot::B)] {
            assert!(!permits(
                View {
                    ptt_slot,
                    ..original
                },
                p,
                UPPER
            ));
        }
    }

    #[test]
    fn every_transmit_parameter_is_pinned_and_missing_tx_stays_denied() {
        let p = profile(&record()).unwrap();
        let t = p.transmit.unwrap();
        let variants = [
            TxRequest {
                frequency_10hz: t.frequency_10hz - 1,
                ..t
            },
            TxRequest {
                frequency_10hz: t.frequency_10hz + 1,
                ..t
            },
            TxRequest {
                width: Width::Narrow,
                ..t
            },
            TxRequest { tone: 0, ..t },
            TxRequest { tone: 0x1303, ..t },
            TxRequest { tone_mode: 1, ..t },
            TxRequest {
                option_nibble: 1,
                ..t
            },
            TxRequest { power: 1, ..t },
            TxRequest { power: 2, ..t },
            TxRequest { mic_gain: 30, ..t },
            TxRequest { mic_gain: 32, ..t },
        ];
        for transmit in variants {
            assert!(!permits(
                view(),
                Profile {
                    transmit: Some(transmit),
                    ..p
                },
                UPPER
            ));
        }
        assert!(!permits(
            view(),
            Profile {
                transmit: None,
                ..p
            },
            UPPER
        ));
    }

    #[test]
    fn receive_and_metadata_cannot_hide_a_different_rf_context() {
        let p = profile(&record()).unwrap();
        let r = p.receive.unwrap();
        let variants = [
            uv98_rf_sequence::RxRequest {
                frequency_10hz: r.frequency_10hz - 1,
                ..r
            },
            uv98_rf_sequence::RxRequest {
                frequency_10hz: r.frequency_10hz + 1,
                ..r
            },
            uv98_rf_sequence::RxRequest {
                width: Width::Narrow,
                ..r
            },
            uv98_rf_sequence::RxRequest {
                modulation: Modulation::Am,
                ..r
            },
            uv98_rf_sequence::RxRequest { tone: 0x1302, ..r },
            uv98_rf_sequence::RxRequest { tone_mode: 1, ..r },
            uv98_rf_sequence::RxRequest {
                option_nibble: 1,
                ..r
            },
        ];
        for receive in variants {
            assert!(!permits(
                view(),
                Profile {
                    receive: Some(receive),
                    ..p
                },
                UPPER
            ));
        }
        assert!(!permits(view(), Profile { receive: None, ..p }, UPPER));
        for frequency in [0, 43_297_499, 43_297_501] {
            assert!(!permits(view(), Profile { frequency, ..p }, UPPER));
        }
        for route in [0, 1, 2, 3, 5, 255] {
            assert!(!permits(view(), Profile { route, ..p }, UPPER));
        }
        for power in [1, 2, 255] {
            assert!(!permits(view(), Profile { power, ..p }, UPPER));
        }
        let mut changed = p;
        changed.audio.modulation = Modulation::Am;
        assert!(!permits(view(), changed, UPPER));
    }

    #[test]
    fn immutable_record_permissions_and_invalid_fields_are_not_escalated() {
        for operation in [1, 2] {
            let mut raw = record();
            raw[1] = operation;
            let before = raw;
            let p = profile(&raw).unwrap();
            assert_eq!(p.transmit.is_none(), operation == 1);
            assert_eq!(p.receive.is_none(), operation == 2);
            assert!(!permits(view(), p, UPPER));
            assert_eq!(raw, before);
        }
        let mut raw = record();
        raw[1] = 255;
        assert_eq!(profile(&raw), Err(policy::Error::OperationMode));
        raw = record();
        raw[2] = 0;
        assert_eq!(profile(&raw), Err(policy::Error::InactiveChannel));
        raw = record();
        raw[16] = 255;
        let p = profile(&raw).unwrap();
        assert!(p.transmit.is_none());
        assert!(!permits(view(), p, UPPER));
        raw = record();
        raw[0] = 1;
        assert!(!permits(view(), profile(&raw).unwrap(), UPPER));
    }

    #[test]
    fn raw_record_gate_checks_critical_fields_but_preserves_unrelated_bytes() {
        for offset in [0, 1, 2, 3, 16, 18, 19] {
            for value in 0..=255u8 {
                let mut raw = record();
                raw[offset] = value;
                let expected = match offset {
                    2 => value == 1,
                    18 => value >> 5 == 0,
                    19 => value & 15 == 0,
                    _ => value == 0,
                };
                assert_eq!(
                    record_permitted(Channel::parse(&raw).unwrap()),
                    expected,
                    "raw[{offset}]={value:02X}"
                );
            }
        }
        for offset in 4..16 {
            let mut raw = record();
            raw[offset] ^= 1;
            assert!(
                !record_permitted(Channel::parse(&raw).unwrap()),
                "raw[{offset}]"
            );
        }
        let mut raw = record();
        raw[17] = 255;
        raw[20..].fill(255); // Name/unknown bytes must not be normalized.
        let before = raw;
        assert!(record_permitted(Channel::parse(&raw).unwrap()));
        assert_eq!(raw, before);
    }

    #[test]
    fn overrides_cannot_launder_a_different_original_channel_into_the_trial() {
        for (offset, value) in [(0, 1), (16, 1), (16, 2)] {
            let mut raw = record();
            raw[offset] = value;
            let channel = Channel::parse(&raw).unwrap();
            let p = policy::profile(
                view(),
                Some(channel),
                Settings {
                    squelch: 1,
                    mic_gain: 31,
                    fm_volume: [58, 15],
                    am_volume: [50, 12],
                    squelch_delay: 10,
                },
                Overrides {
                    power: 0,
                    modulation: Some(Modulation::Fm),
                    monitor: false,
                },
            )
            .unwrap();
            // An effective profile has lost these raw distinctions. Requiring
            // the separate immutable-record check is intentional, not redundant.
            assert!(permits(view(), p, UPPER));
            assert!(!record_permitted(channel));
        }
        let mut raw = record();
        raw[6..10].copy_from_slice(&43_297_501u32.to_le_bytes());
        assert!(!record_permitted(Channel::parse(&raw).unwrap()));
        assert!(!permits(view(), profile(&raw).unwrap(), UPPER));
    }

    #[test]
    fn exact_deadline_wrap_and_backwards_time_fail_closed() {
        assert_eq!(MAX_HELD_MS, 5_000);
        for start in [0u32, 123, u32::MAX - 99, u32::MAX] {
            assert!(hold_within_limit(start, start));
            assert!(hold_within_limit(start, start.wrapping_add(4_999)));
            assert!(!hold_within_limit(start, start.wrapping_add(5_000)));
            assert!(!hold_within_limit(start, start.wrapping_add(5_001)));
            assert!(!hold_within_limit(start, start.wrapping_sub(1)));
            assert!(!hold_within_limit(start, start.wrapping_sub(5_000)));
            assert!(!hold_within_limit(start, start.wrapping_add(0x8000_0000)));
        }
    }

    #[test]
    fn allowlist_does_not_override_boot_release_guard_or_timeout_rearm() {
        let p = profile(&record()).unwrap();
        assert!(permits(view(), p, UPPER));
        let mut guard = Guard::new();
        assert!(!guard.arm(UPPER));
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.arm(UPPER));
        assert!(guard.observe(UPPER, false));
        assert!(!hold_within_limit(100, 5_100));
        guard.disarm();
        assert!(!guard.observe(UPPER, false));
        assert!(!guard.arm(UPPER));
        assert!(!guard.observe(RELEASED, false));
        assert!(guard.arm(UPPER));
    }
}
