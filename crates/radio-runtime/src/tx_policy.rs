//! General FM transmission eligibility for the current, committed A/B context.
//!
//! Unlike the historical `tx_trial`, this has no channel, frequency, tone or
//! power allowlist. `policy::profile` owns channel permissions and RAM settings;
//! `rf_sequence::transmit` must additionally accept the actual calibration
//! before arming. The physical edge/release guard, executor state, runtime TOT,
//! watchdog and fault shutdown remain separate mandatory checks.
//!
//! Routing is derived from the TX frequency, never `Profile::route` (which is
//! the receive route for a duplex channel). A traced software route is not a
//! measured RF coverage or output-power specification.

use crate::{policy, ptt::Input, TX_TIMEOUT_MS};
use uv98_band_plan::{select_routes, SelectorMode};
use uv98_codeplug::channels::Tone;
use uv98_handheld_ui::controller::{Context, ScanState, Screen, SideAction, Slot, View};
use uv98_rf_sequence::{Modulation, TxRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Upper,
    Lower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denied {
    PhysicalInput,
    Screen,
    Entry,
    Locked,
    Scan,
    Binding,
    Context,
    Modulation,
    NoTransmit,
    Request,
    Frequency,
    Route,
}

/// Retain the returned value from arming through the entire TX session.
/// Changes to its source, selected context, binding or RF request end that
/// session; they never retune a held transmission or hand it to another PTT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Authorization {
    pub slot: Slot,
    pub source: Source,
    pub transmit: TxRequest,
    pub tx_route: u8,
    context: Context,
    binding: SideAction,
}

impl Authorization {
    pub fn still_held(self, view: View, profile: policy::Profile, input: Input) -> bool {
        authorize(view, profile, input) == Ok(self)
    }
}

/// Call after loading the context selected by the debounced PTT event.
/// `profile` must be freshly derived from that context's immutable record (or
/// committed VFO) and its own RAM settings. This function does not generate an
/// edge and does not bypass a disabled/missing TX half of a channel.
pub fn authorize(
    view: View,
    profile: policy::Profile,
    input: Input,
) -> Result<Authorization, Denied> {
    if !input.clean || input.upper == input.lower {
        return Err(Denied::PhysicalInput);
    }
    if view.screen != Screen::Home {
        return Err(Denied::Screen);
    }
    if view.entry.is_some() {
        return Err(Denied::Entry);
    }
    if view.locked {
        return Err(Denied::Locked);
    }
    if view.scan != ScanState::Stopped {
        return Err(Denied::Scan);
    }
    let (source, index) = if input.upper {
        (Source::Upper, 0)
    } else {
        (Source::Lower, 1)
    };
    let binding = view.side_bindings[index];
    let slot = match binding {
        SideAction::PttA => Slot::A,
        SideAction::PttB => Slot::B,
        SideAction::Active => view.active_slot,
        _ => return Err(Denied::Binding),
    };
    if view.active_slot != slot || view.ptt_slot != Some(slot) {
        return Err(Denied::Context);
    }
    if profile.audio.modulation != Modulation::Fm {
        return Err(Denied::Modulation);
    }
    let transmit = profile.transmit.ok_or(Denied::NoTransmit)?;
    // Cheap repeatable checks; the full calibrated plan is validated at arming,
    // not regenerated on every guard poll. These match profile/sequence bounds.
    if transmit.power > 2
        || transmit.mic_gain > 31
        || transmit.tone_mode > 3
        || transmit.option_nibble > 15
        || Tone::decode(transmit.tone).is_none()
    {
        return Err(Denied::Request);
    }
    policy::range(transmit.frequency_10hz).ok_or(Denied::Frequency)?;
    let tx_route = select_routes(transmit.frequency_10hz, SelectorMode::Primary)
        .ok_or(Denied::Route)?
        .tx_code();
    // These are the primary selector's implemented stock TX paths. Route 2
    // exists in the GPIO helper but is never selected by this primary backend.
    if !matches!(tx_route, 1 | 3 | 4 | 5) {
        return Err(Denied::Route);
    }
    Ok(Authorization {
        slot,
        source,
        transmit,
        tx_route,
        context: Context {
            mode: view.mode,
            channel: view.channel,
            vfo_10hz: view.vfo_10hz,
            zone: view.zone,
            step_index: view.step_index,
            reverse: view.reverse,
        },
        binding,
    })
}

pub fn permits(view: View, profile: policy::Profile, input: Input) -> bool {
    authorize(view, profile, input).is_ok()
}

/// Same ordinary TOT as the runtime, including configuration time. This
/// wrapper is not a second, shorter trial timeout. Backwards clock movement
/// fails closed; normal u32 wrap is supported for intervals below 2^31 ms.
pub fn hold_within_limit(start: u32, now: u32) -> bool {
    let elapsed = now.wrapping_sub(start);
    elapsed < 0x8000_0000 && elapsed < TX_TIMEOUT_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Overrides, Settings};
    use crate::ptt::Guard;
    use uv98_calibration::Calibration;
    use uv98_codeplug::channels::Channel;
    use uv98_handheld_ui::controller::{Controller, Entry, EntryKind, Mode, SIDE_ACTIONS};
    use uv98_rf_sequence::{self as sequence, Step, Width};

    const UPPER: Input = Input {
        upper: true,
        lower: false,
        clean: true,
    };
    const LOWER: Input = Input {
        upper: false,
        lower: true,
        clean: true,
    };
    const RELEASED: Input = Input {
        upper: false,
        lower: false,
        clean: true,
    };

    fn view() -> View {
        let mut view = Controller::new(&[255; 128], 14_550_000).view();
        view.ptt_slot = Some(Slot::A);
        view.ptt_down = true;
        view.side_down = [true, false];
        view
    }

    fn record(rx: u32, tx: u32) -> [u8; 48] {
        let mut raw = [0; 48];
        raw[2] = 1;
        raw[6..10].copy_from_slice(&rx.to_le_bytes());
        raw[10..14].copy_from_slice(&tx.to_le_bytes());
        raw
    }

    fn profile(view: View, raw: &[u8; 48], power: u8) -> policy::Profile {
        policy::profile(
            view,
            Some(Channel::parse(raw).unwrap()),
            Settings {
                squelch: 1,
                mic_gain: 31,
                fm_volume: [58, 15],
                am_volume: [50, 12],
                squelch_delay: 10,
            },
            Overrides {
                power,
                ..Overrides::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn all_current_band_boundaries_powers_and_routes_have_calibrated_plans() {
        let calibration = Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        // Independent expected selector transitions, including unlike RX/TX
        // routes in the HF/50 MHz window. These are not hardware measurements.
        for (frequency, route) in [
            (1_800_000, 1),
            (3_199_999, 1),
            (3_200_000, 3),
            (4_500_000, 3),
            (6_400_000, 3),
            (6_400_001, 3),
            (13_600_000, 3),
            (17_400_000, 3),
            (25_999_999, 3),
            (26_000_000, 4),
            (40_000_000, 4),
            (48_000_000, 4),
            (66_000_000, 4),
            (84_000_000, 5),
            (92_000_000, 5),
            (99_999_999, 5),
        ] {
            for power in 0..=2 {
                for (width, tone, tone_mode, option) in [
                    (0, 0, 0, 0),
                    (1, 0x1375, 0, 0),
                    (0, 0x2013, 2, 0),
                    (1, 0x31ff, 3, 1),
                ] {
                    let mut raw = record(frequency, frequency);
                    raw[3] = width;
                    raw[14..16].copy_from_slice(&u16::to_le_bytes(tone));
                    raw[18] = tone_mode << 5;
                    raw[19] = option;
                    let p = profile(view(), &raw, power);
                    let allowed = authorize(view(), p, UPPER).unwrap();
                    assert_eq!(allowed.tx_route, route);
                    assert_eq!(allowed.transmit.power, power);
                    let plan = sequence::transmit(&calibration, allowed.transmit).unwrap();
                    let steps = plan.operations();
                    assert_eq!(steps[steps.len() - 2], Step::TxGates(route));
                    assert!(steps.contains(&Step::AntennaPf0(route != 3 && route != 4)));
                }
            }
        }
    }

    #[test]
    fn split_rx_tx_and_reverse_use_tx_route_and_tx_tone() {
        let mut raw = record(5_000_000, 43_297_500);
        raw[4..6].copy_from_slice(&0x2013u16.to_le_bytes());
        raw[14..16].copy_from_slice(&0x1375u16.to_le_bytes());
        let before = raw;
        let p = profile(view(), &raw, 2);
        assert_eq!(p.route, 2);
        let allowed = authorize(view(), p, UPPER).unwrap();
        assert_eq!(allowed.tx_route, 4);
        assert_eq!(allowed.transmit.frequency_10hz, 43_297_500);
        assert_eq!(allowed.transmit.tone, 0x1375);
        let reversed = View {
            reverse: true,
            ..view()
        };
        let p = profile(reversed, &raw, 1);
        assert_eq!(p.route, 4);
        let allowed = authorize(reversed, p, UPPER).unwrap();
        assert_eq!(allowed.tx_route, 3); // 50 MHz TX != its RX route 2.
        assert_eq!(allowed.transmit.frequency_10hz, 5_000_000);
        assert_eq!(allowed.transmit.tone, 0x2013);
        assert_eq!(raw, before);
    }

    #[test]
    fn upper_and_lower_follow_each_configured_ptt_target_after_slot_loading() {
        let raw = record(14_550_000, 14_490_000);
        for input in [UPPER, LOWER] {
            let index = usize::from(input.lower);
            for active in [Slot::A, Slot::B] {
                for binding in SIDE_ACTIONS {
                    let mut v = view();
                    v.side_bindings[index] = binding;
                    v.active_slot = active;
                    v.ptt_slot = Some(active);
                    let result = authorize(v, profile(v, &raw, 2), input);
                    let expected = match binding {
                        SideAction::PttA => active == Slot::A,
                        SideAction::PttB => active == Slot::B,
                        SideAction::Active => true,
                        _ => false,
                    };
                    assert_eq!(result.is_ok(), expected, "{input:?} {active:?} {binding:?}");
                    if let Ok(allowed) = result {
                        assert_eq!(allowed.slot, active);
                        assert_eq!(
                            allowed.source,
                            if input.upper {
                                Source::Upper
                            } else {
                                Source::Lower
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn arbitrary_channel_vfo_and_transmit_only_are_not_trial_limited() {
        let raw = record(14_550_000, 14_490_000);
        for channel in [0, 1, 127, 1023] {
            let v = View {
                channel: Some(channel),
                ..view()
            };
            assert!(permits(v, profile(v, &raw, 2), UPPER));
        }
        let v = View {
            mode: Mode::Vfo,
            vfo_10hz: 43_300_000,
            ..view()
        };
        let p = profile(v, &raw, 1);
        assert!(permits(v, p, UPPER));
        assert_eq!(p.transmit.unwrap().frequency_10hz, 43_300_000);
        let mut tx_only = raw;
        tx_only[1] = 2;
        let p = profile(view(), &tx_only, 2);
        assert!(p.receive.is_none());
        assert!(permits(view(), p, UPPER));
    }

    #[test]
    fn permissions_am_and_malformed_tx_are_not_escalated() {
        for offset in [0, 1] {
            let mut raw = record(14_550_000, 14_490_000);
            raw[offset] = 1; // AM, or receive-only.
            assert!(!permits(view(), profile(view(), &raw, 0), UPPER));
        }
        let p = profile(view(), &record(14_550_000, 14_490_000), 0);
        let tx = p.transmit.unwrap();
        for transmit in [
            TxRequest { power: 3, ..tx },
            TxRequest { mic_gain: 32, ..tx },
            TxRequest { tone: 0x2200, ..tx },
            TxRequest { tone: 0x4000, ..tx },
            TxRequest { tone_mode: 4, ..tx },
            TxRequest {
                option_nibble: 16,
                ..tx
            },
        ] {
            assert_eq!(
                authorize(
                    view(),
                    policy::Profile {
                        transmit: Some(transmit),
                        ..p
                    },
                    UPPER
                ),
                Err(Denied::Request)
            );
        }
        for frequency_10hz in [0, 1_799_999, 66_000_001, 83_999_999, 100_000_000] {
            let transmit = TxRequest {
                frequency_10hz,
                ..tx
            };
            assert_eq!(
                authorize(
                    view(),
                    policy::Profile {
                        transmit: Some(transmit),
                        ..p
                    },
                    UPPER
                ),
                Err(Denied::Frequency)
            );
        }
    }

    #[test]
    fn authorization_does_not_create_an_edge_or_relax_physical_ui_gates() {
        let original = view();
        let p = profile(original, &record(14_550_000, 14_490_000), 0);
        for upper in [false, true] {
            for lower in [false, true] {
                for clean in [false, true] {
                    let input = Input {
                        upper,
                        lower,
                        clean,
                    };
                    let mut v = original;
                    v.side_bindings[1] = SideAction::PttA;
                    assert_eq!(permits(v, p, input), clean && upper != lower);
                }
            }
        }
        for screen in [
            Screen::Menu,
            Screen::Channels,
            Screen::Setting,
            Screen::Spectrum,
        ] {
            assert_eq!(
                authorize(View { screen, ..original }, p, UPPER),
                Err(Denied::Screen)
            );
        }
        assert_eq!(
            authorize(
                View {
                    locked: true,
                    ..original
                },
                p,
                UPPER
            ),
            Err(Denied::Locked)
        );
        let entry = Some(Entry {
            kind: EntryKind::Frequency,
            value: 145,
            digits: 3,
        });
        assert_eq!(
            authorize(View { entry, ..original }, p, UPPER),
            Err(Denied::Entry)
        );
        for scan in [
            ScanState::Requested,
            ScanState::Running,
            ScanState::Stopping,
        ] {
            assert_eq!(
                authorize(View { scan, ..original }, p, UPPER),
                Err(Denied::Scan)
            );
        }
        assert_eq!(
            authorize(
                View {
                    ptt_slot: None,
                    ..original
                },
                p,
                UPPER
            ),
            Err(Denied::Context)
        );
        let mut guard = Guard::new();
        assert!(permits(original, p, UPPER));
        assert!(!guard.arm(UPPER)); // Boot-held remains rejected independently.
        guard.observe(RELEASED, false);
        assert!(guard.arm(UPPER));
    }

    #[test]
    fn held_session_cannot_handoff_change_binding_or_change_context() {
        let v = view();
        let p = profile(v, &record(14_550_000, 14_490_000), 0);
        let allowed = authorize(v, p, UPPER).unwrap();
        assert!(allowed.still_held(v, p, UPPER));
        assert!(!allowed.still_held(v, p, RELEASED));
        let mut changed = v;
        changed.side_bindings[1] = SideAction::PttA;
        assert!(permits(changed, p, LOWER));
        assert!(!allowed.still_held(changed, p, LOWER));
        changed = v;
        changed.side_bindings[0] = SideAction::Active;
        assert!(permits(changed, p, UPPER));
        assert!(!allowed.still_held(changed, p, UPPER));
        for changed in [
            View {
                channel: Some(999),
                ..v
            },
            View { reverse: true, ..v },
            View { zone: Some(1), ..v },
            View {
                mode: Mode::Vfo,
                ..v
            },
            View {
                vfo_10hz: v.vfo_10hz + 1,
                ..v
            },
        ] {
            assert!(permits(changed, p, UPPER));
            assert!(!allowed.still_held(changed, p, UPPER));
        }
        let changed = View {
            active_slot: Slot::B,
            ptt_slot: Some(Slot::B),
            side_bindings: [SideAction::PttB; 2],
            ..v
        };
        assert!(permits(changed, p, UPPER));
        assert!(!allowed.still_held(changed, p, UPPER));
    }

    #[test]
    fn every_tx_parameter_is_frozen_for_an_active_session() {
        let v = view();
        let p = profile(v, &record(14_550_000, 14_490_000), 0);
        let allowed = authorize(v, p, UPPER).unwrap();
        let tx = allowed.transmit;
        for transmit in [
            TxRequest {
                frequency_10hz: tx.frequency_10hz + 1,
                ..tx
            },
            TxRequest { power: 1, ..tx },
            TxRequest { mic_gain: 30, ..tx },
            TxRequest { tone: 0x2013, ..tx },
            TxRequest { tone_mode: 1, ..tx },
            TxRequest {
                option_nibble: 1,
                ..tx
            },
            TxRequest {
                width: Width::Narrow,
                ..tx
            },
        ] {
            let changed = policy::Profile {
                transmit: Some(transmit),
                ..p
            };
            assert!(permits(v, changed, UPPER));
            assert!(!allowed.still_held(v, changed, UPPER));
        }
        // Receive metadata is not the transmitter route or identity.
        let changed = policy::Profile {
            route: 5,
            frequency: 84_000_000,
            ..p
        };
        assert!(allowed.still_held(v, changed, UPPER));
    }

    #[test]
    fn ordinary_tot_replaces_five_seconds_and_keeps_wrap_and_rearm_rules() {
        assert_eq!(TX_TIMEOUT_MS, 30_000);
        for start in [0u32, 123, u32::MAX - 100] {
            assert!(hold_within_limit(start, start.wrapping_add(5_000)));
            assert!(hold_within_limit(start, start.wrapping_add(29_999)));
            assert!(!hold_within_limit(start, start.wrapping_add(30_000)));
            assert!(!hold_within_limit(start, start.wrapping_sub(1)));
            assert!(!hold_within_limit(start, start.wrapping_add(0x8000_0000)));
        }
        let mut guard = Guard::new();
        guard.observe(RELEASED, false);
        assert!(guard.arm(UPPER));
        guard.disarm(); // Caller does this on timeout/release/fault.
        assert!(!guard.arm(UPPER));
        assert!(!guard.observe(UPPER, false));
        guard.observe(RELEASED, false);
        assert!(guard.arm(UPPER));
    }
}
