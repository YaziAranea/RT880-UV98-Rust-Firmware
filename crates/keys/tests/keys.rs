use uv98_keys::{
    auxiliary_key, matrix_key, ptt_pressed, stock_auxiliary_code, stock_matrix_code, update_matrix,
    ButtonEvent, ClockError, Column, Debouncer, Key, RowLevels, Timing, TimingError,
};

fn debouncer() -> Debouncer {
    // Test/application policy only. These numbers do not claim stock timing.
    Debouncer::new(Timing::new(20, 500).unwrap())
}

#[test]
fn decoder_matches_bounded_stock_emulation_for_every_u16() {
    let mut count = 0;
    for line in include_str!("fixtures/decoder.csv").lines().skip(1) {
        let values: Vec<u16> = line.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(values.len(), 3);
        assert_eq!(usize::from(values[0]), count);
        assert_eq!(
            u16::from(stock_matrix_code(values[0])),
            values[1],
            "matrix {}",
            values[0]
        );
        assert_eq!(
            u16::from(stock_auxiliary_code(values[0])),
            values[2],
            "auxiliary {}",
            values[0]
        );
        count += 1;
    }
    assert_eq!(count, 65536);
}

#[test]
fn row_sampler_matches_simulated_stock_gpio_reads_and_preserves_other_columns() {
    let mut count = 0;
    for line in include_str!("fixtures/rows.csv").lines().skip(1) {
        let v: Vec<u16> = line.split(',').map(|s| s.parse().unwrap()).collect();
        assert_eq!(v.len(), 4);
        let column = [Column::C0, Column::C1, Column::C2, Column::C3][v[1] as usize];
        let rows = RowLevels {
            pc10_high: v[2] & 1 != 0,
            pc11_high: v[2] & 2 != 0,
            pc12_high: v[2] & 4 != 0,
            pd2_high: v[2] & 8 != 0,
        };
        assert_eq!(update_matrix(v[0], column, rows), v[3]);
        let retained = !(0xF_u16 << (v[1] * 4));
        assert_eq!(v[0] & retained, v[3] & retained);
        count += 1;
    }
    assert_eq!(count, 256);
}

#[test]
fn labels_special_patterns_and_unrecognized_combinations_are_explicit() {
    let layout = [
        Key::Digit1,
        Key::Digit4,
        Key::Digit7,
        Key::Star,
        Key::Digit2,
        Key::Digit5,
        Key::Digit8,
        Key::Digit0,
        Key::Digit3,
        Key::Digit6,
        Key::Digit9,
        Key::Hash,
        Key::Menu,
        Key::Up,
        Key::Down,
        Key::Exit,
    ];
    for (bit, key) in layout.into_iter().enumerate() {
        assert_eq!(matrix_key(1 << bit), Some(key));
    }
    assert_eq!(matrix_key(0x1111), Some(Key::Star));
    assert_eq!(matrix_key(0x8888), Some(Key::Hash));
    for raw in [0, 3, 0x6000, 0xFFFF] {
        assert_eq!(matrix_key(raw), None);
    }
    assert_eq!(auxiliary_key(1884), Some(Key::Aux17));
    assert_eq!(auxiliary_key(1885), Some(Key::Aux16));
    assert_eq!(auxiliary_key(2716), Some(Key::Aux16));
    assert_eq!(auxiliary_key(2717), Some(Key::Aux18));
    assert_eq!(auxiliary_key(3541), Some(Key::Aux18));
    assert_eq!(auxiliary_key(3542), None);
    assert!(ptt_pressed(false));
    assert!(!ptt_pressed(true));
}

#[test]
fn timing_must_be_explicit_nonzero_and_below_half_clock() {
    assert_eq!(Timing::new(0, 1), Err(TimingError::ZeroDuration));
    assert_eq!(Timing::new(1, 0), Err(TimingError::ZeroDuration));
    assert_eq!(Timing::new(1 << 31, 1), Err(TimingError::DurationTooLong));
    assert_eq!(Timing::new(1, 1 << 31), Err(TimingError::DurationTooLong));
    assert!(Timing::new((1 << 31) - 1, (1 << 31) - 1).is_ok());
}

#[test]
fn bouncing_press_and_release_each_need_continuous_candidate_time() {
    let mut d = debouncer();
    for (time, raw) in [(0, false), (1, true), (10, false), (12, true), (31, true)] {
        assert_eq!(d.update(time, raw), Ok(None));
    }
    assert_eq!(d.update(32, true), Ok(Some(ButtonEvent::Pressed)));
    assert!(d.is_pressed());
    for (time, raw) in [(40, false), (51, true), (60, false), (79, false)] {
        assert_eq!(d.update(time, raw), Ok(None));
    }
    assert_eq!(d.update(80, false), Ok(Some(ButtonEvent::Released)));
    assert!(!d.is_pressed());
    assert_eq!(d.update(1000, false), Ok(None));
}

#[test]
fn hold_begins_at_confirmed_press_emits_once_and_still_releases() {
    let mut d = debouncer();
    assert_eq!(d.update(100, true), Ok(None));
    assert_eq!(d.update(119, true), Ok(None));
    assert_eq!(d.update(120, true), Ok(Some(ButtonEvent::Pressed)));
    assert_eq!(d.update(619, true), Ok(None));
    assert_eq!(d.update(620, true), Ok(Some(ButtonEvent::Held)));
    assert_eq!(d.update(620, true), Ok(None));
    assert_eq!(d.update(1000, true), Ok(None));
    assert_eq!(d.update(1001, false), Ok(None));
    assert_eq!(d.update(1020, false), Ok(None));
    assert_eq!(d.update(1021, false), Ok(Some(ButtonEvent::Released)));
    assert_eq!(d.update(2000, true), Ok(None));
    assert_eq!(d.update(2020, true), Ok(Some(ButtonEvent::Pressed)));
    assert_eq!(d.update(2520, true), Ok(Some(ButtonEvent::Held)));
}

#[test]
fn pending_release_suppresses_hold_without_hiding_final_release() {
    let mut d = debouncer();
    assert_eq!(d.update(0, true), Ok(None));
    assert_eq!(d.update(20, true), Ok(Some(ButtonEvent::Pressed)));
    assert_eq!(d.update(519, false), Ok(None));
    assert_eq!(d.update(520, false), Ok(None));
    assert_eq!(d.update(539, false), Ok(Some(ButtonEvent::Released)));
}

#[test]
fn release_bounce_can_resume_a_pending_hold_only_after_pressed_is_observed() {
    let mut d = debouncer();
    assert_eq!(d.update(0, true), Ok(None));
    assert_eq!(d.update(20, true), Ok(Some(ButtonEvent::Pressed)));
    assert_eq!(d.update(519, false), Ok(None));
    assert_eq!(d.update(520, false), Ok(None));
    assert_eq!(d.update(525, true), Ok(Some(ButtonEvent::Held)));
    assert!(d.is_pressed());
}

#[test]
fn debounce_and_hold_work_across_u32_wrap() {
    let mut d = debouncer();
    assert_eq!(d.update(u32::MAX - 10, true), Ok(None));
    assert_eq!(d.update(8, true), Ok(None));
    assert_eq!(d.update(9, true), Ok(Some(ButtonEvent::Pressed)));
    assert_eq!(d.update(508, true), Ok(None));
    assert_eq!(d.update(509, true), Ok(Some(ButtonEvent::Held)));
    assert_eq!(d.update(510, false), Ok(None));
    assert_eq!(d.update(530, false), Ok(Some(ButtonEvent::Released)));

    let mut d = debouncer();
    assert_eq!(d.update(u32::MAX - 30, true), Ok(None));
    assert_eq!(
        d.update(u32::MAX - 10, true),
        Ok(Some(ButtonEvent::Pressed))
    );
    assert_eq!(d.update(488, true), Ok(None));
    assert_eq!(d.update(489, true), Ok(Some(ButtonEvent::Held)));
}

#[test]
fn clock_regression_and_half_range_gap_are_rejected_without_state_changes() {
    let mut d = debouncer();
    assert_eq!(d.update(100, true), Ok(None));
    let before = d;
    assert_eq!(d.update(99, false), Err(ClockError));
    assert_eq!(d, before);
    assert_eq!(
        d.update(100_u32.wrapping_add(1 << 31), true),
        Err(ClockError)
    );
    assert_eq!(d, before);
    assert_eq!(d.update(120, true), Ok(Some(ButtonEvent::Pressed)));
}
