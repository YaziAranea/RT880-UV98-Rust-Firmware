use uv98_calibration::Calibration;
use uv98_rf_sequence::{self as sequence, Modulation, RxRequest, Step, TxRequest, Width};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    kind: &'static str,
    target: &'static str,
    register: u16,
    value: u16,
}

fn event(kind: &'static str, target: &'static str, register: u16, value: u16) -> Event {
    Event {
        kind,
        target,
        register,
        value,
    }
}
fn rows(text: &'static str) -> Vec<Vec<&'static str>> {
    text.lines()
        .skip(1)
        .filter(|s| !s.is_empty())
        .map(|line| line.split(',').collect())
        .collect()
}
fn number(s: &str) -> u32 {
    s.parse().unwrap()
}
fn expected(row: &[&'static str], at: usize) -> Event {
    event(
        row[at],
        row[at + 1],
        number(row[at + 2]) as u16,
        number(row[at + 3]) as u16,
    )
}
fn fixture() -> [u8; 4096] {
    let hex = include_str!("../../../research/radio018/synthetic-calibration.hex").trim();
    let mut bytes = [0; 4096];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    bytes
}
fn run(steps: &[Step]) -> Vec<Event> {
    run_with_register31(steps, 0x55a0)
}
fn run_with_register31(steps: &[Step], initial31: u16) -> Vec<Event> {
    let mut regs = [0x55a0u16; 128];
    regs[0x33] = 0x5540;
    regs[0x31] = initial31;
    let mut out = Vec::new();
    for &step in steps {
        match step {
            Step::WriteA { register, value } => {
                regs[usize::from(register)] = value;
                out.push(event("write", "A", register.into(), value));
            }
            Step::UpdateA {
                register,
                keep_mask,
                value,
            } => {
                let previous = regs[usize::from(register)];
                out.push(event("read", "A", register.into(), previous));
                let next = (previous & keep_mask) | value;
                regs[usize::from(register)] = next;
                out.push(event("write", "A", register.into(), next));
            }
            Step::DelayMs(ms) => out.push(event("delay", "ms", 0, ms)),
            Step::AntennaPf0(high) => out.push(event("gpio", "F", 0, u16::from(high))),
            Step::Mute(muted) => out.push(event("gpio", "F", 8, u16::from(!muted))),
            Step::TxGates(route) => {
                out.push(event("gpio", "C", 15, 0));
                out.push(event("gpio", "B", 11, 0));
                let previous = regs[0x33];
                out.push(event("read", "A", 0x33, previous));
                let mut next = previous & 0xfff3;
                match route {
                    1 => {
                        out.push(event("gpio", "C", 15, 1));
                        out.push(event("gpio", "B", 11, 0));
                    }
                    2 => {
                        out.push(event("gpio", "C", 15, 1));
                        out.push(event("gpio", "B", 11, 1));
                    }
                    3 => next |= 8,
                    4 | 5 => next |= 4,
                    _ => {}
                }
                regs[0x33] = next;
                out.push(event("write", "A", 0x33, next));
            }
        }
    }
    out
}

#[test]
fn receive_resets_compander_after_width_before_tuning_with_fresh_register31() {
    let bytes = fixture();
    let cal = Calibration::parse(&bytes).unwrap();
    let plan = sequence::receive(
        &cal,
        RxRequest {
            frequency_10hz: 12_640_000,
            width: Width::Wide,
            modulation: Modulation::Am,
            level: 1,
            tone: 0,
            tone_mode: 0,
            option_nibble: 0,
            squelch_delay: 0,
        },
    )
    .unwrap();
    let ops = plan.operations();
    let width = ops
        .iter()
        .position(|op| matches!(op, Step::WriteA { register: 0x43, .. }))
        .unwrap();
    assert_eq!(
        ops[width + 1],
        Step::UpdateA {
            register: 0x31,
            keep_mask: 0xfff7,
            value: 0
        }
    );
    assert!(matches!(
        ops[width + 4],
        Step::WriteA { register: 0x38, .. }
    ));
    let rows = rows(include_str!(
        "../../../research/radio018/compander-reset.csv"
    ));
    let mut cases = 0;
    for chunk in rows.chunks_exact(4) {
        let initial = number(chunk[0][0]) as u16;
        let expected: Vec<_> = chunk.iter().map(|r| expected(r, 2)).collect();
        assert_eq!(
            run_with_register31(&ops[width + 1..width + 4], initial),
            expected,
            "initial31={initial:04x}"
        );
        cases += 1;
    }
    assert_eq!(cases, 20);
}

#[test]
fn all_cold_prefix_events_match_original_thumb() {
    let rows = rows(include_str!("../../../research/radio018/cold-prefix.csv"));
    let mut cases = 0;
    for index in [0, 1, 2, 3, 4, 7, 255] {
        let mut bytes = fixture();
        bytes[8..12].fill(index);
        let plan = sequence::cold_init(&Calibration::parse(&bytes).unwrap());
        let expected: Vec<_> = rows
            .iter()
            .filter(|r| number(r[0]) == u32::from(index))
            .map(|r| expected(r, 2))
            .collect();
        assert_eq!(
            run(&plan.operations()[5..5 + expected.len()]),
            expected,
            "index={index}"
        );
        assert!(plan.operations().len() < sequence::MAX_STEPS);
        assert_eq!(
            plan.operations().last(),
            Some(&Step::WriteA {
                register: 0x37,
                value: 0x1d00
            })
        );
        cases += 1;
    }
    assert_eq!(cases, 7);
}

#[test]
fn all_4116_tone_cases_match_original_thumb_including_dcs_encoder() {
    let rows = rows(include_str!("../../../research/radio018/tone.csv"));
    let mut start = 0;
    let mut cases = 0;
    while start < rows.len() {
        let raw = number(rows[start][0]) as u16;
        let mode = number(rows[start][1]) as u8;
        let mut end = start + 1;
        while end < rows.len() && rows[end][0] == rows[start][0] && rows[end][1] == rows[start][1] {
            end += 1;
        }
        let expected: Vec<_> = rows[start..end].iter().map(|r| expected(r, 3)).collect();
        let plan = sequence::tone(raw, mode).unwrap();
        assert_eq!(
            run(plan.operations()),
            expected,
            "raw={raw:04x}, mode={mode}"
        );
        cases += 1;
        start = end;
    }
    assert_eq!(cases, 4116);
}

#[test]
fn all_42_complete_tx_bodies_match_original_thumb() {
    let bytes = fixture();
    let cal = Calibration::parse(&bytes).unwrap();
    let rows = rows(include_str!("../../../research/radio018/tx-body.csv"));
    let mut start = 0;
    let mut cases = 0;
    while start < rows.len() {
        let r = &rows[start];
        let mut end = start + 1;
        while end < rows.len() && rows[end][0] == r[0] {
            end += 1;
        }
        let plan = sequence::transmit(
            &cal,
            TxRequest {
                frequency_10hz: number(r[1]),
                width: if r[2] == "0" {
                    Width::Wide
                } else {
                    Width::Narrow
                },
                tone: number(r[3]) as u16,
                tone_mode: number(r[4]) as u8,
                option_nibble: number(r[5]) as u8,
                power: number(r[6]) as u8,
                mic_gain: 16,
            },
        )
        .unwrap();
        let expected: Vec<_> = rows[start..end].iter().map(|r| expected(r, 8)).collect();
        // New off-prefix and explicit width are separate from the stock body.
        assert_eq!(run(&plan.operations()[6..]), expected, "case={}", r[0]);
        assert!(plan.operations().len() < sequence::MAX_STEPS);
        assert_eq!(plan.operations()[0], Step::Mute(true));
        assert_eq!(plan.operations()[1], Step::TxGates(0));
        assert_eq!(
            plan.operations()[2],
            Step::WriteA {
                register: 0x36,
                value: 0x007f
            }
        );
        assert!(matches!(
            plan.operations()[plan.operations().len() - 2],
            Step::TxGates(1..=5)
        ));
        assert!(matches!(
            plan.operations().last(),
            Some(Step::WriteA { register: 0x36, .. })
        ));
        start = end;
        cases += 1;
    }
    assert_eq!(cases, 42);
    assert_eq!(bytes, fixture());
}

#[test]
fn validates_before_any_executor_can_observe_plan() {
    let bytes = fixture();
    let cal = Calibration::parse(&bytes).unwrap();
    let good = TxRequest {
        frequency_10hz: 14_550_000,
        width: Width::Narrow,
        tone: 0x31ec,
        tone_mode: 3,
        option_nibble: 15,
        power: 2,
        mic_gain: 255,
    };
    for raw in [1, 0x0fff, 0x2200, 0x3200, 0xffff] {
        assert!(sequence::tone(raw, 0).is_err());
    }
    assert!(sequence::tone(0, 4).is_err());
    for frequency_10hz in [0, 1_799_999, 66_000_001, 83_999_999, 130_000_001, u32::MAX] {
        assert!(sequence::transmit(
            &cal,
            TxRequest {
                frequency_10hz,
                ..good
            }
        )
        .is_err());
    }
    assert!(sequence::transmit(&cal, TxRequest { power: 3, ..good }).is_err());
    assert!(sequence::transmit(
        &cal,
        TxRequest {
            option_nibble: 16,
            ..good
        }
    )
    .is_err());
    assert!(sequence::transmit(
        &cal,
        TxRequest {
            tone_mode: 4,
            ..good
        }
    )
    .is_err());
    assert!(sequence::dcs_word(512, false).is_err());
}

#[test]
fn rx_all_routes_and_maximal_tone_fit_and_never_request_tx() {
    let bytes = fixture();
    let cal = Calibration::parse(&bytes).unwrap();
    for frequency in [
        1_800_000,
        3_200_000,
        4_500_000,
        14_550_000,
        43_350_000,
        84_000_000,
        130_000_000,
    ] {
        for modulation in [Modulation::Fm, Modulation::Am] {
            let plan = sequence::receive(
                &cal,
                RxRequest {
                    frequency_10hz: frequency,
                    width: Width::Narrow,
                    modulation,
                    level: 10,
                    tone: 0x31ff,
                    tone_mode: 3,
                    option_nibble: 15,
                    squelch_delay: 255,
                },
            )
            .unwrap();
            assert!(plan.operations().len() < sequence::MAX_STEPS);
            assert!(!plan.operations().iter().any(|s| matches!(
                s,
                Step::TxGates(1..=255)
                    | Step::Mute(false)
                    | Step::WriteA {
                        register: 0x30,
                        value: 0xc1fe | 0xc3fa
                    }
            )));
        }
    }
}

#[test]
fn cold_non_enabling_tail_matches_full_stock_init() {
    let bytes = fixture();
    let cal = Calibration::parse(&bytes).unwrap();
    let plan = sequence::cold_init(&cal);
    let actual = run(&plan.operations()[30..plan.operations().len() - 3]);
    let expected: Vec<_> = rows(include_str!("../../../research/radio018/cold-full.csv"))
        .into_iter()
        .skip(25)
        .map(|r| expected(&r, 1))
        .filter(|e| !(e.kind == "delay" || e.kind == "write" && matches!(e.register, 0x30 | 0x37)))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn all_24_complete_rx_bodies_match_original_thumb_plus_named_prelude() {
    let bytes = fixture();
    let cal = Calibration::parse(&bytes).unwrap();
    let rows = rows(include_str!("../../../research/radio018/rx-body.csv"));
    let mut start = 0;
    let mut cases = 0;
    while start < rows.len() {
        let r = &rows[start];
        let mut end = start + 1;
        while end < rows.len() && rows[end][0] == r[0] {
            end += 1;
        }
        let plan = sequence::receive(
            &cal,
            RxRequest {
                frequency_10hz: number(r[1]),
                width: if r[2] == "0" {
                    Width::Wide
                } else {
                    Width::Narrow
                },
                tone: number(r[3]) as u16,
                tone_mode: number(r[4]) as u8,
                level: number(r[5]) as u8,
                squelch_delay: number(r[6]) as u8,
                modulation: if r[7] == "0" {
                    Modulation::Fm
                } else {
                    Modulation::Am
                },
                option_nibble: 0,
            },
        )
        .unwrap();
        // Explicit prefix is policy. RX option/filter configuration comes from
        // the separately proven050B8..50EA activation slice, not4BF0 itself.
        let source = &plan.operations()[6..];
        let afc = source
            .iter()
            .position(|s| {
                matches!(
                    s,
                    Step::UpdateA {
                        register: 0x73,
                        keep_mask: 0xffef,
                        value: 0
                    }
                )
            })
            .unwrap();
        // Remove only this contiguous, independently identified activation
        // fragment. In particular, keep the earlier compander A31 read/write.
        assert_eq!(
            source[afc + 1],
            Step::WriteA {
                register: 0x71,
                value: 0x68dc
            }
        );
        assert_eq!(
            source[afc + 2],
            Step::UpdateA {
                register: 0x31,
                keep_mask: 0xfffd,
                value: 0
            }
        );
        assert!(matches!(
            source[afc + 3],
            Step::WriteA { register: 0x75, .. }
        ));
        assert_eq!(
            source[afc + 4],
            Step::WriteA {
                register: 0x37,
                value: 0x1f0f
            }
        );
        let body: Vec<_> = source[..afc + 1]
            .iter()
            .chain(source[afc + 4..].iter())
            .copied()
            .collect();
        let expected: Vec<_> = rows[start..end].iter().map(|r| expected(r, 9)).collect();
        assert_eq!(run(&body), expected, "case={}", r[0]);
        start = end;
        cases += 1;
    }
    assert_eq!(cases, 24);
}

#[test]
fn fm_and_am_audio_words_match_48_stock_activation_cases() {
    let rows = rows(include_str!("../../../research/radio018/audio.csv"));
    assert_eq!(rows.len(), 48 * 3);
    for chunk in rows.chunks_exact(3) {
        let r = &chunk[0];
        let modulation = if r[1] == "0" {
            Modulation::Fm
        } else {
            Modulation::Am
        };
        assert_eq!(
            expected(&chunk[0], 6),
            event("write", "A", 0x47, sequence::audio_mode_word(modulation))
        );
        assert_eq!(
            expected(&chunk[1], 6),
            event("read", "A", 1, number(r[4]) as u16)
        );
        assert_eq!(
            expected(&chunk[2], 6),
            event(
                "write",
                "A",
                0x48,
                sequence::audio_volume_word(
                    number(r[2]) as u8,
                    number(r[3]) as u8,
                    number(r[4]) as u16
                )
            )
        );
    }
}

#[test]
fn am_agc_matches_1161_original_thumb_state_transitions_and_table() {
    use sequence::am_agc::{State, TABLE};
    let observations = rows(include_str!("../../../research/radio018/am-agc.csv"));
    assert_eq!(observations.len(), 1161);
    for r in observations {
        let state = State {
            index: number(r[1]) as u8,
            previous_rssi: number(r[2]) as u16,
            hold: number(r[3]) as u8,
        };
        let update = state.sample(number(r[0]) as u16).unwrap();
        assert_eq!(
            update.state,
            State {
                index: number(r[4]) as u8,
                previous_rssi: number(r[5]) as u16,
                hold: number(r[6]) as u8
            },
            "input={r:?}"
        );
        assert_eq!(update.register13, number(r[7]) as u16);
        assert_eq!(update.rssi_correction, r[8].parse::<i16>().unwrap());
    }
    for r in rows(include_str!("../../../research/radio018/am-gain-table.csv")) {
        assert_eq!(
            TABLE[number(r[0]) as usize],
            (number(r[1]) as u16, r[2].parse::<i8>().unwrap())
        );
    }
    assert_eq!(
        State::new(),
        State {
            index: 90,
            previous_rssi: 0,
            hold: 0
        }
    );
    assert!(State {
        index: 0,
        ..State::new()
    }
    .sample(100)
    .is_err());
    assert!(State {
        index: 97,
        ..State::new()
    }
    .sample(100)
    .is_err());
    assert!(State::new().sample(512).is_err());
}

#[test]
fn tone_status_1024_stock_cases_and_pending_irq_policy() {
    let observations = rows(include_str!("../../../research/radio018/tone-status.csv"));
    assert_eq!(observations.len(), 1024);
    for r in observations {
        assert_eq!(
            sequence::tone_status(number(r[1]) as u16, number(r[0]) as u16),
            number(r[2]) as u8
        );
    }
    for (raw, bit) in [(0x1302, 0x0400), (0x21ec, 0x4000), (0x31ec, 0x8000)] {
        assert!(sequence::tone_matches(bit, raw));
        assert!(!sequence::tone_matches(bit | 0x0800, raw));
        assert!(!sequence::tone_matches(bit | 1, raw));
        assert!(!sequence::tone_matches(0, raw));
    }
    assert!(!sequence::tone_matches(0x1400, 0x1302));
    assert!(!sequence::tone_matches(0x2400, 0x1302));
    assert!(sequence::tone_matches(0xffff, 0));
    assert!(!sequence::tone_matches(0xffff, 0xffff));
}
