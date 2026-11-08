//! Independent stock traces plus adversarial, pure planner tests.
//! No register transport is instantiated here. Executor fault tests are separate.
use uv98_band_plan::{stock_receive_requests, Request};
use uv98_calibration::{Calibration, Table, BLOCK_LEN, MARKER};
use uv98_receive::{Chip, FrequencyPlan, Operation, Plan, PlanError, Width};

fn fixture(raw_offset: u16) -> [u8; BLOCK_LEN] {
    let text = match raw_offset {
        0 => include_str!("../../../research/receive-plan/synthetic-calibration-0000.hex"),
        0x8000 => include_str!("../../../research/receive-plan/synthetic-calibration-8000.hex"),
        0xffff => include_str!("../../../research/receive-plan/synthetic-calibration-ffff.hex"),
        _ => panic!("oracle fixture offset"),
    }
    .trim();
    assert_eq!(text.len(), BLOCK_LEN * 2);
    let mut bytes = [0; BLOCK_LEN];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * index..2 * index + 2], 16).unwrap();
    }
    assert_eq!(bytes[0], MARKER);
    bytes
}

#[derive(Debug, Eq, PartialEq)]
enum Trace {
    Write(Chip, u8, u16),
    Read(Chip, u8, u16),
    Pf0(bool),
}
fn chip(value: &str) -> Chip {
    match value {
        "A" => Chip::A,
        "B" => Chip::B,
        _ => panic!("unknown oracle chip"),
    }
}
fn trace(kind: &str, target: &str, address: &str, value: &str) -> Trace {
    let address: u8 = address.parse().unwrap();
    let value: u16 = value.parse().unwrap();
    match kind {
        "write" => Trace::Write(chip(target), address, value),
        "read" => Trace::Read(chip(target), address, value),
        "gpio" => {
            assert_eq!((target, address), ("F", 0));
            assert!(value <= 1);
            Trace::Pf0(value == 1)
        }
        _ => panic!("unknown oracle event"),
    }
}

/// Expand only the abstract route into its observed fresh read + write.
/// Unsafe prior TX bits are retained here to compare pure stock behavior:
/// the executor MUST reject those prior words, as its separate tests establish.
fn expand(operations: &[Operation], prior_reg33: u16) -> Vec<Trace> {
    let mut result = Vec::new();
    for operation in operations {
        match *operation {
            Operation::Write {
                chip,
                register,
                value,
            } => result.push(Trace::Write(chip, register, value)),
            Operation::PrimaryRoute { route } => {
                result.push(Trace::Read(Chip::A, 0x33, prior_reg33));
                let [Request::PrimaryGpioRegister { value }, _] =
                    stock_receive_requests(route, prior_reg33)
                else {
                    panic!("route shape")
                };
                result.push(Trace::Write(Chip::A, 0x33, value));
            }
            Operation::AntennaPf0 { high } => result.push(Trace::Pf0(high)),
        }
    }
    result
}

#[test]
fn primary_composition_matches_all_432_independent_machine_code_traces() {
    let csv = include_str!("../../../research/receive-plan/composition.csv");
    assert_eq!(
        csv.lines().next().unwrap(),
        "case,frequency_10hz,width,level,raw_offset,prior_reg33,step,kind,target,address,value"
    );
    let lines: Vec<_> = csv.lines().skip(1).collect();
    assert_eq!(lines.len(), 3456);
    for (case, rows) in lines.chunks_exact(8).enumerate() {
        let first: Vec<_> = rows[0].split(',').collect();
        assert_eq!(first.len(), 11);
        assert_eq!(first[0].parse::<usize>().unwrap(), case);
        let frequency: u32 = first[1].parse().unwrap();
        let width = Width::try_from(first[2].parse::<u8>().unwrap()).unwrap();
        let level: u8 = first[3].parse().unwrap();
        let raw_offset: u16 = first[4].parse().unwrap();
        let prior: u16 = first[5].parse().unwrap();
        let bytes = fixture(raw_offset);
        let calibration = Calibration::parse(&bytes).unwrap();
        let plan = Plan::primary(&calibration, frequency, width, level).unwrap();
        let expected: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(step, row)| {
                let values: Vec<_> = row.split(',').collect();
                assert_eq!(&values[..6], &first[..6]);
                assert_eq!(values[6].parse::<usize>().unwrap(), step);
                trace(values[7], values[8], values[9], values[10])
            })
            .collect();
        assert_eq!(
            expand(plan.operations(), prior),
            expected,
            "oracle case {case}"
        );
        assert_eq!(plan.frequency_10hz(), frequency);
        assert_eq!(plan.width(), width);
        assert_eq!(plan.level(), level);
        assert_eq!(plan.raw_offset(), raw_offset);
        let selection = plan.selection();
        let selected = calibration.band(selection.band_index()).unwrap();
        let (noise, squelch) = match width {
            Width::Wide => (Table::NoiseWide, Table::SquelchWide),
            Width::Narrow => (Table::NoiseNarrow, Table::SquelchNarrow),
        };
        assert_eq!(
            plan.noise_calibration(),
            selected.point_raw(noise, selection.table_index()).unwrap()
        );
        assert_eq!(
            plan.squelch_calibration(),
            selected
                .point_raw(squelch, selection.table_index())
                .unwrap()
        );
        assert_eq!(calibration.raw(), &bytes);
    }
}

#[test]
fn both_chip_frequency_plans_match_72_independent_machine_code_traces() {
    let csv = include_str!("../../../research/receive-plan/frequency.csv");
    assert_eq!(
        csv.lines().next().unwrap(),
        "case,chip,frequency_10hz,raw_offset,step,kind,target,address,value"
    );
    let lines: Vec<_> = csv.lines().skip(1).collect();
    assert_eq!(lines.len(), 144);
    for (case, rows) in lines.chunks_exact(2).enumerate() {
        let first: Vec<_> = rows[0].split(',').collect();
        assert_eq!(first.len(), 9);
        assert_eq!(first[0].parse::<usize>().unwrap(), case);
        let selected_chip = chip(first[1]);
        let frequency = first[2].parse().unwrap();
        let offset = first[3].parse().unwrap();
        let bytes = fixture(offset);
        let calibration = Calibration::parse(&bytes).unwrap();
        let plan = FrequencyPlan::new(&calibration, selected_chip, frequency).unwrap();
        let expected: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(step, row)| {
                let values: Vec<_> = row.split(',').collect();
                assert_eq!(&values[..4], &first[..4]);
                assert_eq!(values[4].parse::<usize>().unwrap(), step);
                trace(values[5], values[6], values[7], values[8])
            })
            .collect();
        assert_eq!(
            expand(plan.operations(), 0),
            expected,
            "frequency case {case}"
        );
        assert_eq!(plan.chip(), selected_chip);
        assert_eq!(plan.frequency_10hz(), frequency);
        assert_eq!(plan.raw_offset(), offset);
    }
}

#[test]
fn width_accepts_only_0_and_1_even_where_stock_bandwidth_falls_back_for_2() {
    let bytes = fixture(0x8000);
    let calibration = Calibration::parse(&bytes).unwrap();
    let csv = include_str!("../../../research/receive-plan/width.csv");
    let mut count = 0;
    for line in csv.lines().skip(1) {
        let row: Vec<_> = line.split(',').collect();
        assert_eq!(row.len(), 5);
        let raw = row[0].parse::<u8>().unwrap();
        assert_eq!(usize::from(raw), count);
        match Width::try_from(raw) {
            Ok(width) => {
                assert!(raw <= 1);
                let plan = Plan::primary(&calibration, 14_500_000, width, 5).unwrap();
                assert_eq!(
                    expand(&plan.operations()[..1], 0),
                    [trace(row[1], row[2], row[3], row[4])]
                );
            }
            Err(error) => assert_eq!(error, PlanError::UnsupportedWidth { raw }),
        }
        count += 1;
    }
    assert_eq!(count, 256);
    assert_eq!(
        Width::try_from(2),
        Err(PlanError::UnsupportedWidth { raw: 2 })
    );
}

#[test]
fn selection_boundaries_shared_endpoints_and_last_slot_repeat_are_retained() {
    let bytes = fixture(0x8000);
    let calibration = Calibration::parse(&bytes).unwrap();
    let cases = [
        (1_800_000, 2, 0),
        (6_400_000, 2, 9),
        (6_400_001, 3, 0),
        (13_599_999, 3, 15),
        (13_600_000, 0, 0),
        (17_400_000, 0, 7),
        (17_400_001, 4, 0),
        (24_000_000, 4, 14),
        (24_000_001, 5, 0),
        (32_000_000, 5, 15),
        (32_000_001, 6, 0),
        (39_999_999, 6, 15),
        (40_000_000, 1, 0),
        (48_000_000, 1, 15),
        (48_000_001, 7, 0),
        (56_000_000, 7, 15),
        (56_000_001, 8, 0),
        (64_000_000, 8, 15),
        (66_000_000, 8, 15),
        (84_000_000, 9, 0),
        (92_000_000, 9, 15),
        (92_000_001, 10, 0),
        (100_000_000, 10, 15),
        (100_000_001, 11, 0),
        (108_000_000, 11, 15),
        (108_000_001, 12, 0),
        (116_000_000, 12, 15),
        (116_000_001, 13, 0),
        (124_000_000, 13, 15),
        (124_000_001, 13, 0),
        (130_000_000, 13, 12),
    ];
    for (frequency, band, point) in cases {
        let plan = Plan::primary(&calibration, frequency, Width::Wide, 0).unwrap();
        assert_eq!(
            (
                plan.selection().band_index(),
                plan.selection().table_index()
            ),
            (band, point),
            "frequency {frequency}"
        );
        assert_eq!(plan.selection().byte_offset(), 32 + band * 112);
    }
    // The UI tab says 560-640(660); its label cannot narrow the stock selector.
    assert_eq!(
        Plan::primary(&calibration, 65_000_000, Width::Wide, 0)
            .unwrap()
            .selection()
            .table_index(),
        15
    );
    assert_eq!(calibration.band(14).unwrap().valid_points(), 12);
    let last = Plan::primary(&calibration, 130_000_000, Width::Wide, 0).unwrap();
    assert_eq!(
        (
            last.selection().band_index(),
            last.selection().table_index()
        ),
        (13, 12)
    );
    // Validating against the final UI tab's 12 columns would incorrectly fail.
}

#[test]
fn unsupported_frequencies_and_levels_are_rejected_without_mutating_saved_data() {
    let bytes = fixture(0xffff);
    let before = bytes;
    let calibration = Calibration::parse(&bytes).unwrap();
    for code in [
        0,
        1,
        1_799_999,
        66_000_001,
        75_000_000,
        83_999_999,
        130_000_001,
        u32::MAX,
    ] {
        assert_eq!(
            Plan::primary(&calibration, code, Width::Wide, 0),
            Err(PlanError::UnsupportedFrequency { code })
        );
        for chip in [Chip::A, Chip::B] {
            assert_eq!(
                FrequencyPlan::new(&calibration, chip, code),
                Err(PlanError::UnsupportedFrequency { code })
            );
        }
    }
    for level in 11..=u8::MAX {
        assert_eq!(
            Plan::primary(&calibration, 43_350_000, Width::Narrow, level),
            Err(PlanError::InvalidLevel { level })
        );
    }
    assert_eq!(calibration.raw(), &before);
    // The constructors have no executor/transport argument or hidden I/O path.
}

#[test]
fn full_raw_offset_is_preserved_and_original_frequency_controls_routing() {
    let mut bytes = fixture(0x8000);
    bytes[32 + 5 * 112..34 + 5 * 112].copy_from_slice(&0xffffu16.to_le_bytes());
    let calibration = Calibration::parse(&bytes).unwrap();
    let plan = Plan::primary(&calibration, 25_999_999, Width::Wide, 5).unwrap();
    assert_eq!(plan.raw_offset(), 0xffff);
    assert_eq!(plan.frequency_words().code, 26_032_766);
    // The correction crosses the route threshold. Stock uses the original code.
    assert_eq!(plan.operations()[5], Operation::PrimaryRoute { route: 3 });
    assert_eq!(plan.operations()[6], Operation::AntennaPf0 { high: false });
    let over = Plan::primary(&calibration, 26_000_000, Width::Wide, 5).unwrap();
    assert_eq!(over.operations()[5], Operation::PrimaryRoute { route: 4 });
}

#[test]
fn level_zero_writes_exact_wrapping_threshold_codes_and_does_not_disable_rf() {
    let mut bytes = fixture(0x8000);
    let base = 32 + 112;
    bytes[base + 48 + 6] = 250;
    bytes[base + 64 + 6] = 1;
    bytes[base + 80 + 6] = 0;
    bytes[base + 96 + 6] = 255;
    let calibration = Calibration::parse(&bytes).unwrap();
    let wide = Plan::primary(&calibration, 43_350_000, Width::Wide, 0).unwrap();
    assert_eq!(
        (wide.noise_calibration(), wide.squelch_calibration()),
        (250, 1)
    );
    assert_eq!(
        wide.operations()[3],
        Operation::Write {
            chip: Chip::A,
            register: 0x4f,
            value: 0x0600
        }
    );
    assert_eq!(
        wide.operations()[4],
        Operation::Write {
            chip: Chip::A,
            register: 0x78,
            value: 0x03fd
        }
    );
    let narrow = Plan::primary(&calibration, 43_350_000, Width::Narrow, 0).unwrap();
    assert_eq!(
        (narrow.noise_calibration(), narrow.squelch_calibration()),
        (0, 255)
    );
    assert_eq!(
        narrow.operations()[3],
        Operation::Write {
            chip: Chip::A,
            register: 0x4f,
            value: 0x0c06
        }
    );
    assert_eq!(
        narrow.operations()[4],
        Operation::Write {
            chip: Chip::A,
            register: 0x78,
            value: 0x01fb
        }
    );
    let one = Plan::primary(&calibration, 43_350_000, Width::Wide, 1).unwrap();
    assert_eq!(wide.operations(), one.operations());
}

#[test]
fn plans_are_owned_snapshots_and_never_include_start_power_tx_or_storage_operations() {
    let mut bytes = fixture(0x8000);
    let plan = Plan::primary(
        &Calibration::parse(&bytes).unwrap(),
        43_350_000,
        Width::Narrow,
        10,
    )
    .unwrap();
    let snapshot = plan.operations().to_vec();
    bytes.fill(0); // The calculated plan cannot observe a later source replacement.
    assert_eq!(plan.operations(), snapshot);
    for operation in plan.operations() {
        match operation {
            Operation::Write { chip, register, .. } => {
                assert_eq!(*chip, Chip::A);
                assert!([0x43, 0x38, 0x39, 0x4f, 0x78].contains(register));
                assert!(![0x00, 0x30, 0x36, 0x37, 0x40, 0x51].contains(register));
            }
            Operation::PrimaryRoute { route } => assert!((1..=5).contains(route)),
            Operation::AntennaPf0 { .. } => {}
        }
    }
    assert_eq!(plan.operations().len(), 7);
    let bytes = fixture(0x8000);
    let calibration = Calibration::parse(&bytes).unwrap();
    for chip in [Chip::A, Chip::B] {
        let frequency = FrequencyPlan::new(&calibration, chip, 43_350_000).unwrap();
        assert_eq!(frequency.operations().len(), 2);
        assert!(frequency
            .operations()
            .iter()
            .all(|op| matches!(op, Operation::Write{chip:c,register:0x38|0x39,..} if *c==chip)));
    }
}

#[test]
fn rejected_calibration_cannot_create_a_plan_and_unknown_tail_is_not_normalized() {
    let mut bytes = fixture(0x8000);
    bytes[0] = 0;
    assert!(Calibration::parse(&bytes).is_err());
    bytes[0] = MARKER;
    bytes[1] = 0xfe;
    bytes[1712] = 0xa5;
    bytes[4095] = 0x5a;
    let before = bytes;
    let calibration = Calibration::parse(&bytes).unwrap();
    Plan::primary(&calibration, 14_500_000, Width::Wide, 5).unwrap();
    assert_eq!(calibration.raw(), &before);
}
