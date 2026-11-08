use uv98_board_policy::{
    zero_pa_code, Change, Chip, McuLine, Operation, CLEAR_B_GPIO, DIAGNOSTIC_QUIESCENCE_REQUESTS,
    MCU_BOOT_LEVELS, SLEEP_A, SLEEP_B, TX_GATES_OFF_A,
};

const ZERO_A: [Operation; 1] = [zero_pa_code(Chip::A)];
const ZERO_B: [Operation; 1] = [zero_pa_code(Chip::B)];

fn plan(name: &str) -> &'static [Operation] {
    match name {
        "tx_gates_a" => &TX_GATES_OFF_A,
        "sleep_a" => &SLEEP_A,
        "clear_b_gpio" => &CLEAR_B_GPIO,
        "sleep_b" => &SLEEP_B,
        "zero_a" => &ZERO_A,
        "zero_b" => &ZERO_B,
        "boot_levels" => &MCU_BOOT_LEVELS,
        _ => panic!("unknown fixture plan"),
    }
}

#[test]
fn individual_plans_match_complete_ordered_stock_oracle_traces() {
    let mut prior_group = None;
    let mut group_steps = 0;
    let mut groups = 0;
    let mut rows = 0;
    for line in include_str!("fixtures/rf-requests.csv").lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        assert_eq!(fields.len(), 7);
        let group = (fields[0], fields[1]);
        if prior_group != Some(group) {
            if let Some((name, _)) = prior_group {
                assert_eq!(group_steps, plan(name).len());
            }
            prior_group = Some(group);
            group_steps = 0;
            groups += 1;
        }
        let input: u16 = fields[1].parse().unwrap();
        let step: usize = fields[2].parse().unwrap();
        assert_eq!(step, group_steps);
        let address: u8 = fields[5].parse().unwrap();
        let value: u16 = fields[6].parse().unwrap();
        match plan(fields[0])[step] {
            Operation::ClearMcu(line) => {
                let (port, pin) = match line {
                    McuLine::Pc15HfTxPower => ("C", 15),
                    McuLine::Pb11Tx29_50mSwitch => ("B", 11),
                    McuLine::Pc4RxTsSwitch => ("C", 4),
                    McuLine::Pc5FmPower => ("C", 5),
                };
                assert_eq!(
                    (fields[3], fields[4], address, value),
                    ("mcu", port, pin, 0)
                );
            }
            Operation::Register {
                chip,
                address: expected_address,
                change,
            } => {
                assert_eq!(fields[3], "rf");
                assert_eq!(fields[4], if chip == Chip::A { "A" } else { "B" });
                assert_eq!(address, expected_address);
                assert_eq!(value, change.apply(input));
            }
        }
        group_steps += 1;
        rows += 1;
    }
    assert_eq!(group_steps, plan(prior_group.unwrap().0).len());
    assert_eq!((groups, rows), (76, 175));
}

#[test]
fn sleep_alone_does_not_silently_claim_tx_gate_clear() {
    assert_eq!(Change::Keep(0xffcc).apply(0xffff), 0xffcc);
    assert_eq!(Change::Keep(0xffcc).apply(0x000c), 0x000c);
    assert_eq!(Change::Keep(0xfff3).apply(0x000c), 0);
    for prior in [0, 0xffff, 0xa5a5, 0xff00, 0x0040] {
        let combined = Change::Keep(0xffcc).apply(Change::Keep(0xfff3).apply(prior));
        assert_eq!(combined, prior & 0xffc0);
        assert_eq!(combined & 0xffc0, prior & 0xffc0);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RequestedState {
    // Register33,36,30,37 for A and B. No physical state is represented.
    registers: [[u16; 4]; 2],
    low_requests: u8,
}

impl RequestedState {
    fn apply(&mut self, operation: Operation) {
        match operation {
            Operation::ClearMcu(line) => {
                self.low_requests |= match line {
                    McuLine::Pc15HfTxPower => 1,
                    McuLine::Pb11Tx29_50mSwitch => 2,
                    McuLine::Pc4RxTsSwitch => 4,
                    McuLine::Pc5FmPower => 8,
                };
            }
            Operation::Register {
                chip,
                address,
                change,
            } => {
                let chip = if chip == Chip::A { 0 } else { 1 };
                let slot = match address {
                    0x33 => 0,
                    0x36 => 1,
                    0x30 => 2,
                    0x37 => 3,
                    _ => panic!("unexpected register in shutdown requests"),
                };
                self.registers[chip][slot] = change.apply(self.registers[chip][slot]);
            }
        }
    }
}

#[test]
fn composition_orders_external_gate_requests_first_and_remains_idempotent() {
    assert_eq!(&DIAGNOSTIC_QUIESCENCE_REQUESTS[..3], &TX_GATES_OFF_A);
    for seed in [0, 0xffff, 0xa5a5, 0x5a5a, 0xff00, 0x0040, 0x000c] {
        let mut model = RequestedState {
            registers: [[seed; 4]; 2],
            low_requests: 0,
        };
        for op in DIAGNOSTIC_QUIESCENCE_REQUESTS {
            model.apply(op);
        }
        assert_eq!(
            model.registers,
            [
                [seed & 0xffc0, 0x007f, 0, 0x1d00],
                [seed & 0xffe0, 0x007f, 0, 0x1d00]
            ]
        );
        assert_eq!(model.low_requests, 0x0b); // PC4 is boot-only, not RF policy
        let once = model.clone();
        for op in DIAGNOSTIC_QUIESCENCE_REQUESTS {
            model.apply(op);
        }
        assert_eq!(model, once);
    }
}

#[test]
fn gpio_masks_preserve_every_unowned_bit_including_high_control_bits() {
    for bit in 0..16 {
        let prior = 1u16 << bit;
        for mask in [0xfff3, 0xffcc, 0xffe0] {
            let actual = Change::Keep(mask).apply(prior);
            assert_eq!(actual & mask, prior & mask);
            assert_eq!(actual & !mask, 0);
        }
    }
}
