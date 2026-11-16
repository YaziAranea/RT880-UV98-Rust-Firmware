//! General FM transmission eligibility for the requested, committed A/B context.
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
use uv98_handheld_ui::controller::{Context, Mode, ScanState, Screen, SideAction, Slot, View};
use uv98_rf_sequence::{Modulation, TxRequest, Width};

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
    operator_slot: Slot,
    context: Context,
    binding: SideAction,
}

impl Authorization {
    pub fn still_held(self, view: View, profile: policy::Profile, input: Input) -> bool {
        self.still_held_target(view, view, profile, input)
    }

    pub fn still_held_target(
        self,
        operator: View,
        target: View,
        profile: policy::Profile,
        input: Input,
    ) -> bool {
        authorize_target(&operator, &target, profile, input) == Ok(self)
    }

    /// Configuration-only validation for a separately timed end-of-TX ID.
    /// This never authorizes a fresh transmission and never represents a
    /// physical PTT hold. The owner must enforce clean release, a short tail
    /// deadline, the original TOT and cancellation on every new input.
    pub fn tail_configuration_valid(self, view: View, profile: policy::Profile) -> bool {
        self.tail_configuration_valid_target(view, view, profile)
    }

    pub fn tail_configuration_valid_target(
        self,
        mut operator: View,
        target: View,
        profile: policy::Profile,
    ) -> bool {
        if operator.ptt_slot.is_some() || operator.ptt_down {
            return false;
        }
        operator.ptt_slot = Some(self.slot);
        let original_source = Input {
            upper: self.source == Source::Upper,
            lower: self.source == Source::Lower,
            clean: true,
        };
        authorize_target(&operator, &target, profile, original_source) == Ok(self)
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
    authorize_target(&view, &view, profile, input)
}

/// The operator view supplies the UI and binding state. The target view and
/// profile describe the requested slot's committed radio settings.
// Shared by arming, hold and tail checks. Avoid copying the whole policy into
// each firmware call site under whole-program LTO.
#[inline(never)]
pub fn authorize_target(
    operator: &View,
    target: &View,
    profile: policy::Profile,
    input: Input,
) -> Result<Authorization, Denied> {
    if !input.clean || input.upper == input.lower {
        return Err(Denied::PhysicalInput);
    }
    if operator.screen != Screen::Home {
        return Err(Denied::Screen);
    }
    if operator.entry.is_some() {
        return Err(Denied::Entry);
    }
    if operator.locked {
        return Err(Denied::Locked);
    }
    if operator.scan != ScanState::Stopped {
        return Err(Denied::Scan);
    }
    let (source, index) = if input.upper {
        (Source::Upper, 0)
    } else {
        (Source::Lower, 1)
    };
    let binding = operator.side_bindings[index];
    let slot = match binding {
        SideAction::PttA => Slot::A,
        SideAction::PttB => Slot::B,
        SideAction::Active => operator.active_slot,
        _ => return Err(Denied::Binding),
    };
    if target.active_slot != slot || operator.ptt_slot != Some(slot) {
        return Err(Denied::Context);
    }
    let cb = operator.cb_for_slot(slot);
    if target.cb_for_slot(slot) != cb {
        return Err(Denied::Context);
    }
    if let Some(cb) = cb {
        // The only AM-receive exception is committed CB A: the transmitter
        // itself is narrow FM, exactly +2 kHz, with no subaudio signalling.
        let modulation = if cb.is_am() {
            Modulation::Am
        } else {
            Modulation::Fm
        };
        if !(profile.audio.modulation == modulation
            && target.mode == Mode::Vfo
            && target.vfo_10hz == cb.frequency_10hz()
            && profile.receive.is_some_and(|rx| {
                rx.modulation == modulation && rx.frequency_10hz == cb.frequency_10hz()
            })
            && profile.transmit.is_some_and(|tx| {
                tx.frequency_10hz == cb.tx_frequency_10hz()
                    && tx.width == Width::Narrow
                    && tx.tone == 0
                    && tx.tone_mode == 0
                    && tx.option_nibble == 0
            }))
        {
            return Err(Denied::Modulation);
        }
    } else if profile.audio.modulation != Modulation::Fm {
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
        operator_slot: operator.active_slot,
        context: Context {
            mode: target.mode,
            channel: target.channel,
            vfo_10hz: target.vfo_10hz,
            shift_index: target.shift_index,
            tx_tone_index: target.tx_tone_index,
            zone: target.zone,
            step_index: target.step_index,
            reverse: target.reverse,
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
    fn cb_am_uses_shifted_fm_on_both_grids_with_normal_ptt_guards() {
        use uv98_handheld_ui::cb::{CbChannel, CbGrid, CHANNEL_COUNT};
        let calibration = Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        for grid in [CbGrid::Cept, CbGrid::Zeros] {
            for number in 1..=CHANNEL_COUNT {
                let mut cb = CbChannel::default().with_number(number).unwrap();
                cb.grid = grid;
                let nominal = cb.frequency_10hz();
                let mut v = view();
                v.mode = Mode::Vfo;
                v.cb = Some(cb);
                v.vfo_10hz = nominal;
                let p = profile(v, &record(nominal, nominal), 0);
                let permitted = authorize(v, p, UPPER).unwrap();
                let rx = p.receive.unwrap();
                assert_eq!(
                    (rx.frequency_10hz, rx.modulation),
                    (nominal, Modulation::Am)
                );
                assert_eq!(permitted.transmit.frequency_10hz, nominal + 200);
                assert_eq!(permitted.tx_route, 1);
                assert!(sequence::transmit(&calibration, permitted.transmit).is_ok());
                assert!(sequence::receive(&calibration, rx).is_ok());
                for input in [
                    RELEASED,
                    LOWER,
                    Input {
                        upper: true,
                        lower: true,
                        clean: true,
                    },
                    Input {
                        clean: false,
                        ..UPPER
                    },
                ] {
                    assert!(!permitted.still_held(v, p, input));
                }
                for changed in [
                    View { locked: true, ..v },
                    View {
                        screen: Screen::Cb,
                        ..v
                    },
                    View { cb: None, ..v },
                ] {
                    assert!(!permitted.still_held(changed, p, UPPER));
                }
            }
        }
    }

    #[test]
    fn cb_am_exception_rejects_forged_frequency_context_and_tx_parameters() {
        use uv98_handheld_ui::cb::CbChannel;
        let cb = CbChannel::default();
        let mut v = view();
        v.mode = Mode::Vfo;
        v.cb = Some(cb);
        v.vfo_10hz = cb.frequency_10hz();
        let p = profile(v, &record(v.vfo_10hz, v.vfo_10hz), 0);
        assert!(permits(v, p, UPPER));
        let mut fm_cb = cb;
        fm_cb.am = false;
        for changed in [
            View { cb: None, ..v },
            View {
                cb: Some(fm_cb),
                ..v
            },
            View {
                mode: Mode::Channel,
                ..v
            },
            View {
                vfo_10hz: v.vfo_10hz + 1,
                ..v
            },
            View {
                active_slot: Slot::B,
                ptt_slot: Some(Slot::B),
                side_bindings: [SideAction::PttB; 2],
                ..v
            },
        ] {
            assert!(!permits(changed, p, UPPER));
        }
        let tx = p.transmit.unwrap();
        for transmit in [
            TxRequest {
                frequency_10hz: v.vfo_10hz,
                ..tx
            },
            TxRequest {
                frequency_10hz: v.vfo_10hz + 199,
                ..tx
            },
            TxRequest {
                frequency_10hz: 14_550_000,
                ..tx
            },
            TxRequest {
                width: Width::Wide,
                ..tx
            },
            TxRequest { tone: 0x1375, ..tx },
            TxRequest { tone_mode: 1, ..tx },
            TxRequest {
                option_nibble: 1,
                ..tx
            },
        ] {
            assert!(!permits(
                v,
                policy::Profile {
                    transmit: Some(transmit),
                    ..p
                },
                UPPER
            ));
        }
        assert!(!permits(v, policy::Profile { receive: None, ..p }, UPPER));
        let mut changed = p;
        changed.receive.as_mut().unwrap().frequency_10hz += 200;
        assert!(!permits(v, changed, UPPER));
        changed = p;
        changed.receive.as_mut().unwrap().modulation = Modulation::Fm;
        assert!(!permits(v, changed, UPPER));
        // The committed operator CB context owns this exception, even if an
        // inconsistent projected target or profile claims ordinary FM.
        changed = p;
        changed.audio.modulation = Modulation::Fm;
        assert!(!permits(v, changed, UPPER));
        changed.transmit.as_mut().unwrap().frequency_10hz = 14_550_000;
        assert!(!permits(v, changed, UPPER));
        let erased = View { cb: None, ..v };
        assert_eq!(
            authorize_target(&v, &erased, changed, UPPER),
            Err(Denied::Context)
        );
        assert_eq!(
            authorize_target(&erased, &v, p, UPPER),
            Err(Denied::Context)
        );
    }

    #[test]
    fn id_tail_needs_same_committed_context_and_no_new_ptt() {
        let raw = record(14_550_000, 14_490_000);
        let held = view();
        let profile = profile(held, &raw, 1);
        let allowed = authorize(held, profile, UPPER).unwrap();
        assert!(!allowed.tail_configuration_valid(held, profile));
        let mut released = held;
        released.ptt_slot = None;
        released.ptt_down = false;
        released.side_down = [false, false];
        assert!(allowed.tail_configuration_valid(released, profile));
        released.locked = true;
        assert!(!allowed.tail_configuration_valid(released, profile));
        released.locked = false;
        released.active_slot = Slot::B;
        assert!(!allowed.tail_configuration_valid(released, profile));
        released.active_slot = Slot::A;
        released.side_bindings[0] = SideAction::PttB;
        assert!(!allowed.tail_configuration_valid(released, profile));
        released.side_bindings = held.side_bindings;
        let mut changed = profile;
        changed.transmit.as_mut().unwrap().frequency_10hz += 1250;
        assert!(!allowed.tail_configuration_valid(released, changed));
        changed = profile;
        changed.transmit.as_mut().unwrap().power = 2;
        assert!(!allowed.tail_configuration_valid(released, changed));
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
            (13_700_000, 3),
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
    fn each_ptt_uses_its_target_record_and_operator_focus_stays_independent() {
        let mut raw_b = record(43_897_500, 43_297_500);
        raw_b[14..16].copy_from_slice(&0x1375u16.to_le_bytes());
        let operator_a = View {
            ptt_slot: Some(Slot::B),
            side_down: [false, true],
            ..view()
        };
        let b = View {
            active_slot: Slot::B,
            channel: Some(12),
            ..operator_a
        };
        let profile_b = profile(b, &raw_b, 2);
        let allowed = authorize_target(&operator_a, &b, profile_b, LOWER).unwrap();
        assert_eq!((allowed.slot, allowed.source), (Slot::B, Source::Lower));
        assert_eq!(allowed.transmit.frequency_10hz, 43_297_500);
        assert_eq!(allowed.transmit.tone, 0x1375);
        assert_eq!(allowed.transmit.power, 2);
        assert!(allowed.still_held_target(operator_a, b, profile_b, LOWER));
        assert!(!allowed.still_held_target(operator_a, b, profile_b, RELEASED));
        let released = View {
            ptt_slot: None,
            ptt_down: false,
            side_down: [false; 2],
            ..operator_a
        };
        assert!(allowed.tail_configuration_valid_target(released, b, profile_b));
        assert!(!allowed.tail_configuration_valid_target(operator_a, b, profile_b));
        let selected_b = View {
            active_slot: Slot::B,
            ..operator_a
        };
        assert!(!allowed.still_held_target(selected_b, b, profile_b, LOWER));
        let reversed_b = View { reverse: true, ..b };
        assert!(!allowed.still_held_target(operator_a, reversed_b, profile_b, LOWER));

        let mut rx_only_b = raw_b;
        rx_only_b[1] = 1;
        assert_eq!(
            authorize_target(&operator_a, &b, profile(b, &rx_only_b, 2), LOWER),
            Err(Denied::NoTransmit)
        );

        let operator_b = View {
            ptt_slot: Some(Slot::A),
            side_down: [true, false],
            ..selected_b
        };
        let a = View {
            active_slot: Slot::A,
            channel: Some(4),
            ..operator_b
        };
        let mut raw_a = record(14_550_000, 14_490_000);
        raw_a[14..16].copy_from_slice(&0x2013u16.to_le_bytes());
        let allowed = authorize_target(&operator_b, &a, profile(a, &raw_a, 1), UPPER).unwrap();
        assert_eq!((allowed.slot, allowed.source), (Slot::A, Source::Upper));
        assert_eq!(allowed.transmit.frequency_10hz, 14_490_000);
        assert_eq!(allowed.transmit.tone, 0x2013);
        assert_eq!(allowed.transmit.power, 1);

        let mut follow_focus = operator_b;
        follow_focus.side_bindings[0] = SideAction::Active;
        assert_eq!(
            authorize_target(&follow_focus, &a, profile(a, &raw_a, 1), UPPER),
            Err(Denied::Context)
        );
        follow_focus.ptt_slot = Some(Slot::B);
        assert!(authorize_target(&follow_focus, &b, profile_b, UPPER).is_ok());
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
