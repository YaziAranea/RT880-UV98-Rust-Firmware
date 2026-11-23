extern crate std;
use crate::{
    engine::{Engine, Registers},
    *,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, vec, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Release,
    Select(u16),
    Rows(u8),
    Ptt(bool),
    Settle(SettlePhase),
}
struct Fake {
    matrices: [u16; 2],
    selected: Option<usize>,
    selections: usize,
    idle_rows: u8,
    guard_rows: VecDeque<u8>,
    ptt: bool,
    ptt_values: VecDeque<bool>,
    events: Rc<RefCell<Vec<Event>>>,
}
struct Timer(Rc<RefCell<Vec<Event>>>);
impl ScanTiming for Timer {
    fn settle(&mut self, p: SettlePhase) {
        self.0.borrow_mut().push(Event::Settle(p));
    }
}
impl Registers for Fake {
    fn release(&mut self) {
        self.selected = None;
        self.events.borrow_mut().push(Event::Release);
    }
    fn select(&mut self, p: u16) {
        assert!(
            self.selected.is_none(),
            "must release before selecting another column"
        );
        assert_eq!(p.count_ones(), 1);
        self.selected = Some(COLUMN_PINS.iter().position(|x| *x == p).unwrap());
        self.selections += 1;
        self.events.borrow_mut().push(Event::Select(p));
    }
    fn select_all(&mut self) {
        assert!(self.selected.is_none());
        self.selected = Some(4);
        self.events.borrow_mut().push(Event::Select(COLUMN_MASK));
    }
    fn rows_high(&mut self) -> u8 {
        let rows = if self.selected == Some(4) {
            let matrix = self.matrices[0];
            self.guard_rows
                .pop_front()
                .unwrap_or((!(matrix | (matrix >> 4) | (matrix >> 8) | (matrix >> 12)) & 15) as u8)
        } else if let Some(col) = self.selected {
            (!(self.matrices[(self.selections - 1) / 4] >> (4 * col)) & 15) as u8
        } else {
            self.idle_rows
        };
        self.events.borrow_mut().push(Event::Rows(rows));
        rows
    }
    fn ptt_high(&mut self) -> bool {
        let value = self.ptt_values.pop_front().unwrap_or(self.ptt);
        self.events.borrow_mut().push(Event::Ptt(value));
        value
    }
}

#[test]
fn guard_scan_rejects_every_matrix_contact_and_chord_without_inventing_a_key() {
    for matrix in 0..=u16::MAX {
        let mut scan = scanner(matrix, matrix, false);
        let sample = scan.guard_scan().unwrap();
        assert!(sample.ptt_pressed());
        assert_eq!(sample.matrix_clear(), matrix == 0, "matrix {matrix:04X}");
        assert!(scan.registers.selected.is_none());
    }
    let mut scan = scanner(0, 0, true);
    let sample = scan.guard_scan().unwrap();
    assert!(sample.matrix_clear());
    assert!(!sample.ptt_pressed());
}

#[test]
fn guard_scan_keeps_two_settles_and_owned_open_drain_mask_then_releases() {
    let mut scan = scanner(0x8000, 0x8000, false);
    assert!(!scan.guard_scan().unwrap().matrix_clear());
    assert_eq!(
        *scan.registers.events.borrow(),
        vec![
            Event::Release,
            Event::Ptt(false),
            Event::Select(COLUMN_MASK),
            Event::Settle(SettlePhase::Selected),
            Event::Rows(7),
            Event::Ptt(false),
            Event::Release,
            Event::Settle(SettlePhase::Released),
            Event::Rows(15),
            Event::Ptt(false),
        ]
    );
    assert_eq!(
        COLUMN_MASK,
        COLUMN_PINS.iter().copied().fold(0, |a, b| a | b)
    );
    assert_eq!(COLUMN_MASK & PTT_MASK, 0);
}

#[test]
fn guard_scan_ptt_changes_or_idle_row_faults_always_release_columns() {
    for initial in [false, true] {
        for changed_at in [1, 2] {
            let mut scan = scanner(0, 0, initial);
            let mut samples = [initial; 3];
            samples[changed_at] = !initial;
            scan.registers.ptt_values = samples.into_iter().collect();
            assert_eq!(scan.guard_scan(), Err(Error::UnstablePtt));
            assert!(scan.registers.selected.is_none());
            let events = scan.registers.events.borrow();
            assert_eq!(events.iter().filter(|e| **e == Event::Release).count(), 2);
            assert!(events.contains(&Event::Settle(SettlePhase::Released)));
        }
    }
    for idle_rows in 0..15 {
        let mut scan = scanner(0, 0, false);
        scan.registers.idle_rows = idle_rows;
        assert_eq!(
            scan.guard_scan(),
            Err(Error::RowsNotReleased {
                low_rows: !idle_rows & 15
            })
        );
        assert!(scan.registers.selected.is_none());
    }
}

#[test]
fn idle_fast_path_reports_only_release_with_two_unchanged_settles() {
    let mut scan = scanner(0, 0, true);
    let sample = scan.scan_idle_or_full().unwrap();
    assert_eq!(sample.matrix_state(), MatrixState::Released);
    assert_eq!(sample.matrix_raw(), 0);
    assert!(!sample.ptt_pressed());
    assert_eq!(scan.registers.selections, 0);
    assert!(scan.registers.selected.is_none());
    assert_eq!(
        scan.registers
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Settle(phase) => Some(*phase),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [SettlePhase::Selected, SettlePhase::Released]
    );
}

#[test]
fn idle_fast_path_uses_both_full_passes_for_every_contact_and_pair() {
    for first in 0..16 {
        for second in first..16 {
            let raw = (1 << first) | (1 << second);
            let mut scan = scanner(raw, raw, true);
            let sample = scan.scan_idle_or_full().unwrap();
            assert_eq!(sample.matrix_raw(), raw);
            assert_eq!(
                sample.matrix_state(),
                if first == second {
                    MatrixState::Single(uv98_keys::matrix_key(raw).unwrap())
                } else {
                    MatrixState::Ambiguous
                }
            );
            assert!(!sample.ptt_pressed());
            assert_eq!(scan.registers.selections, 8);
            assert!(scan.registers.selected.is_none());
            assert_eq!(
                scan.registers
                    .events
                    .borrow()
                    .iter()
                    .filter(|event| matches!(event, Event::Settle(_)))
                    .count(),
                19
            );
        }
    }
}

#[test]
fn idle_fast_path_never_qualifies_ptt_without_full_matrix_scan() {
    for matrix in [0, 1, 0x8000, 0x1111, 0x8888, 0xffff] {
        let mut scan = scanner(matrix, matrix, false);
        let sample = scan.scan_idle_or_full().unwrap();
        assert!(sample.ptt_pressed());
        assert_eq!(sample.matrix_raw(), matrix);
        assert_eq!(scan.registers.selections, 8);
        assert!(scan.registers.selected.is_none());
    }
    // A release after an accepted PTT may take the cheap path. It neither
    // emits an edge nor bypasses the caller's release/debounce guard.
    let mut scan = scanner(0, 0, false);
    assert!(scan.scan_idle_or_full().unwrap().ptt_pressed());
    scan.registers.ptt = true;
    let selections = scan.registers.selections;
    assert!(!scan.scan_idle_or_full().unwrap().ptt_pressed());
    assert_eq!(scan.registers.selections, selections);
}

#[test]
fn idle_fast_path_propagates_guard_errors_without_fallback_or_false_release() {
    for initial in [false, true] {
        for changed_at in [1, 2] {
            let mut scan = scanner(0, 0, initial);
            let mut values = [initial; 3];
            values[changed_at] = !initial;
            scan.registers.ptt_values = values.into_iter().collect();
            assert_eq!(scan.scan_idle_or_full(), Err(Error::UnstablePtt));
            assert_eq!(scan.registers.selections, 0);
            assert!(scan.registers.selected.is_none());
        }
    }
    for idle_rows in 0..15 {
        let mut scan = scanner(0, 0, true);
        scan.registers.idle_rows = idle_rows;
        assert_eq!(
            scan.scan_idle_or_full(),
            Err(Error::RowsNotReleased {
                low_rows: !idle_rows & 15
            })
        );
        assert_eq!(scan.registers.selections, 0);
        assert!(scan.registers.selected.is_none());
    }
}

#[test]
fn idle_fast_path_retains_full_scan_matrix_and_ptt_instability_rejection() {
    let mut scan = scanner(0x2000, 0x4000, true);
    assert_eq!(
        scan.scan_idle_or_full(),
        Err(Error::UnstableMatrix {
            forward: 0x2000,
            reverse: 0x4000
        })
    );
    assert!(scan.registers.selected.is_none());
    // Three aggregate PTT observations precede the full scanner's own initial
    // sample plus 24 checks. Exercise every latter check; no scan is retried.
    for changed_at in 4..28 {
        let mut scan = scanner(0, 0, false);
        scan.registers.ptt_values = (0..=changed_at).map(|i| i == changed_at).collect();
        assert_eq!(scan.scan_idle_or_full(), Err(Error::UnstablePtt));
        assert!(scan.registers.selected.is_none());
        assert_eq!(scan.registers.events.borrow().last(), Some(&Event::Release));
    }
}

#[test]
fn one_ptt_transition_can_be_resampled_without_fabricating_a_release() {
    let mut scan = scanner(0, 0, false);
    scan.registers.ptt_values = [true, false, false].into_iter().collect();
    let mut rechecks = 0;
    let sample = scan
        .scan_with_ptt_recheck(|| {
            rechecks += 1;
            true
        })
        .unwrap();
    assert!(sample.ptt_pressed());
    assert_eq!(sample.matrix_state(), MatrixState::Released);
    assert_eq!(scan.registers.selections, 8);
    assert_eq!(rechecks, 1);
    assert!(scan.registers.selected.is_none());
}

#[test]
fn upper_press_at_final_guard_read_still_requires_a_clean_second_scan() {
    let mut scan = scanner(0, 0, false);
    scan.registers.ptt_values = [true, true, false].into_iter().collect();
    assert!(scan.scan_with_ptt_recheck(|| true).unwrap().ptt_pressed());
    assert_eq!(scan.registers.selections, 8);

    let mut bounced = scanner(0, 0, true);
    bounced.registers.ptt_values = [true, false, true].into_iter().collect();
    assert_eq!(bounced.scan_with_ptt_recheck(|| panic!("bounce")), Err(Error::UnstablePtt));
}

#[test]
fn ptt_recheck_never_retries_twice_or_masks_a_matrix_error() {
    let mut scan = scanner(0, 0, false);
    scan.registers.ptt_values = [true, false, false, true, false, false]
        .into_iter()
        .collect();
    let mut rechecks = 0;
    assert_eq!(
        scan.scan_with_ptt_recheck(|| {
            rechecks += 1;
            true
        }),
        Err(Error::UnstablePtt)
    );
    assert_eq!(rechecks, 1);
    assert!(scan.registers.selected.is_none());

    let mut scan = scanner(0x2000, 0x4000, true);
    assert_eq!(
        scan.scan_with_ptt_recheck(|| panic!("matrix error is not a PTT transition")),
        Err(Error::UnstableMatrix {
            forward: 0x2000,
            reverse: 0x4000
        })
    );
    assert!(scan.registers.selected.is_none());
}

#[test]
fn ptt_recheck_preserves_observed_chords_and_rejects_analog_activity() {
    for (matrix, pb6) in [(1, [true, false, false]), (0, [false, true, true])] {
        let mut scan = scanner(matrix, matrix, false);
        scan.registers.ptt_values = pb6.into_iter().collect();
        assert_eq!(
            scan.scan_with_ptt_recheck(|| panic!("chord or release must not be retried")),
            Err(Error::UnstablePtt)
        );
        assert_eq!(scan.registers.selections, 0);
    }
    let mut scan = scanner(0, 0, false);
    scan.registers.ptt_values = [true, false, false].into_iter().collect();
    assert_eq!(
        scan.scan_with_ptt_recheck(|| false),
        Err(Error::UnstablePtt)
    );
    assert_eq!(scan.registers.selections, 0);

    let mut scan = scanner(0, 0, true);
    scan.registers.ptt_values = [true, false, false].into_iter().collect();
    assert_eq!(scan.scan_with_ptt_recheck(|| true), Err(Error::UnstablePtt));
    assert_eq!(scan.registers.selections, 0);

    // A matrix contact seen on the second guard cannot be erased by a
    // later clear full scan. The accepted gesture must be isolated.
    let mut scan = scanner(0, 0, false);
    scan.registers.ptt_values = [true, false, false].into_iter().collect();
    scan.registers.guard_rows = [15, 14].into_iter().collect();
    assert_eq!(scan.scan_with_ptt_recheck(|| true), Err(Error::UnstablePtt));
    assert_eq!(scan.registers.selections, 0);
    assert!(scan.registers.selected.is_none());
}

fn scanner(first: u16, second: u16, ptt: bool) -> Engine<Fake, Timer> {
    let log = Rc::new(RefCell::new(Vec::new()));
    Engine::new(
        Fake {
            matrices: [first, second],
            selected: None,
            selections: 0,
            idle_rows: 15,
            guard_rows: VecDeque::new(),
            ptt,
            ptt_values: VecDeque::new(),
            events: log.clone(),
        },
        Timer(log),
    )
}
fn regs(seed: u32) -> GpioRegisters {
    GpioRegisters {
        cfgr: seed,
        omode: seed,
        odrvr: seed,
        pull: seed,
    }
}
fn words(r: GpioRegisters) -> [u32; 4] {
    [r.cfgr, r.omode, r.odrvr, r.pull]
}

#[test]
fn gpio_fields_match_stock_oracle_except_explicit_open_drain_and_unowned_pb4() {
    for line in include_str!("../tests/fixtures/stock-gpio.csv")
        .lines()
        .skip(1)
    {
        let row: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let seed = row[0];
        let b = regs(seed).with_columns_ptt();
        assert_eq!(b.cfgr & 0x0030_303F, row[1] & 0x0030_303F);
        assert_eq!(b.omode & 0x447, (row[2] | 0x407) & 0x447);
        assert_eq!(b.odrvr & 0x0030_303F, row[3] & 0x0030_303F);
        assert_eq!(b.pull & 0x0030_303F, row[4] & 0x0030_303F);
        assert_eq!(words(regs(seed).with_rows_c()), row[5..9]);
        assert_eq!(words(regs(seed).with_row_d()), row[9..13]);
    }
}

#[test]
fn all_unowned_gpio_fields_and_lcd_nor_pins_are_preserved() {
    for seed in [0, u32::MAX, 0xA5A5_A5A5, 0x1234_5678] {
        for (r, mask, pins) in [
            (regs(seed).with_columns_ptt(), 0x0030_303F, 0x447),
            (regs(seed).with_rows_c(), 0x03F0_0000, 0x1C00),
            (regs(seed).with_row_d(), 0x30, 4),
        ] {
            assert_eq!(r.cfgr & !mask, seed & !mask);
            assert_eq!(r.odrvr & !mask, seed & !mask);
            assert_eq!(r.pull & !mask, seed & !mask);
            assert_eq!(r.omode & !pins, seed & !pins);
        }
    }
    assert_eq!(COLUMN_MASK & PTT_MASK, 0);
    assert_eq!((COLUMN_MASK | PTT_MASK) & (0xF000 | (1 << 3) | (1 << 5)), 0);
    assert_eq!(0x1C00 & ((1 << 13) | (1 << 14)), 0);
}

#[test]
fn column_pin_map_matches_stock_transition_oracle() {
    for line in include_str!("../tests/fixtures/stock-columns.csv")
        .lines()
        .skip(1)
    {
        let r: Vec<usize> = line.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(r[1], (r[0] + 1) % 4);
        assert_eq!(COLUMN_PINS[r[1]] as usize, r[2]);
        assert_eq!(COLUMN_PINS[r[0]] as usize, r[3]);
    }
}

#[test]
fn all_sixteen_keys_have_correct_column_row_mapping_on_both_passes() {
    for bit in 0..16 {
        let raw = 1 << bit;
        let mut s = scanner(raw, raw, true);
        let got = s.scan().unwrap();
        assert_eq!(got.matrix_raw(), raw);
        assert_eq!(
            got.matrix_state(),
            MatrixState::Single(uv98_keys::matrix_key(raw).unwrap())
        );
        assert!(!got.ptt_pressed());
        assert!(got.ptt_high());
        assert!(s.registers.selected.is_none());
        assert_eq!(s.registers.selections, 8);
    }
}

#[test]
fn sampling_has_settle_and_break_before_make_and_reverse_order() {
    let mut s = scanner(0, 0, true);
    s.scan().unwrap();
    let events = s.registers.events.borrow();
    let order: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Select(p) = e {
                Some(*p)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(order, [1024, 4, 2, 1, 1, 2, 4, 1024]);
    for (i, e) in events.iter().enumerate() {
        if matches!(e, Event::Select(_)) {
            assert_eq!(events[i + 1], Event::Settle(SettlePhase::Selected));
        }
        if *e == Event::Settle(SettlePhase::Released) {
            assert_eq!(events[i - 1], Event::Release);
            assert_eq!(events[i + 1], Event::Rows(15));
        }
    }
    assert_eq!(events.last(), Some(&Event::Release));
    assert_eq!(
        events.iter().filter(|e| matches!(e, Event::Ptt(_))).count(),
        25
    );
}

#[test]
fn aliases_and_multikey_masks_never_become_single_keys_or_ptt() {
    for raw in [0x1111, 0x8888, 0x6000, 0x0003, 0xFFFF] {
        let mut s = scanner(raw, raw, true);
        let got = s.scan().unwrap();
        assert_eq!(got.matrix_state(), MatrixState::Ambiguous);
        assert!(!got.ptt_pressed());
    }
    let mut s = scanner(0, 0, false);
    let got = s.scan().unwrap();
    assert_eq!(got.matrix_state(), MatrixState::Released);
    assert!(got.ptt_pressed());
}

#[test]
fn changed_matrix_returns_error_and_releases_all_columns() {
    let mut s = scanner(0x2000, 0x4000, true);
    assert_eq!(
        s.scan(),
        Err(Error::UnstableMatrix {
            forward: 0x2000,
            reverse: 0x4000
        })
    );
    assert!(s.registers.selected.is_none());
    assert_eq!(s.registers.events.borrow().last(), Some(&Event::Release));
}

#[test]
fn ptt_change_at_every_sample_position_cancels_scan_and_releases_columns() {
    for change_at in 1..25 {
        let mut s = scanner(0x2000, 0x2000, true);
        s.registers.ptt_values = (0..=change_at).map(|i| i != change_at).collect();
        assert_eq!(s.scan(), Err(Error::UnstablePtt));
        assert!(s.registers.selected.is_none());
        assert_eq!(s.registers.events.borrow().last(), Some(&Event::Release));
    }
}

#[test]
fn low_idle_row_is_not_a_key_and_no_column_is_selected() {
    for row in 0..4 {
        let mut s = scanner(0, 0, true);
        s.registers.idle_rows = 15 ^ (1 << row);
        assert_eq!(s.scan(), Err(Error::RowsNotReleased { low_rows: 1 << row }));
        assert_eq!(s.registers.selections, 0);
        assert!(s.registers.selected.is_none());
    }
}

#[test]
fn drop_releases_columns_without_driving_or_sampling_ptt() {
    let s = scanner(0, 0, true);
    let log = s.registers.events.clone();
    drop(s);
    assert_eq!(*log.borrow(), vec![Event::Release]);
}

// Ideal passive no-diode switch network: selecting one open-drain column
// pulls every connected row/column low. Released columns never drive high.
// This model checks ghost rejection, not actual PCB wiring or edge timing.
fn electrical_observation(switches: u16) -> u16 {
    let mut observed = 0u16;
    for selected in 0..4 {
        let mut columns = 1u8 << selected;
        let mut rows = 0u8;
        loop {
            let before = (columns, rows);
            for c in 0..4 {
                let connected = ((switches >> (4 * c)) & 15) as u8;
                if columns & (1 << c) != 0 {
                    rows |= connected;
                }
                if rows & connected != 0 {
                    columns |= 1 << c;
                }
            }
            if before == (columns, rows) {
                break;
            }
        }
        observed |= u16::from(rows) << (4 * selected);
    }
    observed
}

#[test]
fn exhaustive_passive_switch_network_rejects_every_multikey_and_ghost_pattern() {
    for physical in 0..=u16::MAX {
        let observed = electrical_observation(physical);
        assert_eq!(observed & physical, physical);
        let expected = match physical.count_ones() {
            0 => MatrixState::Released,
            1 => MatrixState::Single(uv98_keys::matrix_key(physical).unwrap()),
            _ => MatrixState::Ambiguous,
        };
        assert_eq!(
            classify_matrix(observed),
            expected,
            "physical={physical:04X} observed={observed:04X}"
        );
        if let MatrixState::Single(key) = expected {
            assert_ne!(key, uv98_keys::Key::Ptt);
        }
    }
    assert_eq!(electrical_observation(0x0013), 0x0033); // three switches create a fourth observed corner
}
