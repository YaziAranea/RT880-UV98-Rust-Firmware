use uv98_band_plan::{receive_plan, select_routes, stock_receive_requests, Request, SelectorMode};

#[test]
fn routing_matches_bounded_stock_machine_code() {
    let mut count = 0;
    for line in include_str!("fixtures/routing.csv").lines().skip(1) {
        let values: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let mode = if values[1] == 0 {
            SelectorMode::Primary
        } else {
            SelectorMode::Secondary
        };
        let result = select_routes(values[0], mode);
        assert_eq!(result.is_some(), values[2] != 0, "{line}");
        if let Some(update) = result {
            assert_eq!(u32::from(update.tx_code()), values[3], "{line}");
            assert_eq!(
                u32::from(update.rx_code().unwrap_or(0x5b)),
                values[4],
                "{line}"
            );
        } else {
            assert_eq!(
                (values[3], values[4]),
                (0xa6, 0x5b),
                "unrecognized input leaves stock RAM unchanged"
            );
        }
        count += 1;
    }
    assert_eq!(count, 640);
}

#[test]
fn receive_requests_match_complete_ordered_stock_trace() {
    let mut cases = 0;
    let mut step = 0;
    for line in include_str!("fixtures/receive.csv").lines().skip(1) {
        let values: Vec<&str> = line.split(',').collect();
        let code = values[0].parse::<u8>().unwrap();
        let prior = values[1].parse::<u16>().unwrap();
        let observed_step = values[2].parse::<usize>().unwrap();
        let value = values[6].parse::<u16>().unwrap();
        assert_eq!(observed_step, step, "{line}");
        let plan = stock_receive_requests(code, prior);
        match step {
            0 => {
                assert_eq!(&values[3..6], &["read", "A", "51"]);
                assert_eq!(value, prior);
            }
            1 => {
                assert_eq!(&values[3..6], &["write", "A", "51"]);
                assert_eq!(plan[0], Request::PrimaryGpioRegister { value }, "{line}");
            }
            2 => {
                assert_eq!(&values[3..6], &["gpio", "F", "0"]);
                assert!(value <= 1);
                assert_eq!(
                    plan[1],
                    Request::AntennaSwitchPf0 { high: value != 0 },
                    "{line}"
                );
                cases += 1;
            }
            _ => unreachable!(),
        }
        step = (step + 1) % 3;
    }
    assert_eq!((cases, step), (432, 0));
}

#[test]
fn preserves_every_unowned_bit_including_tx_enables() {
    for prior in 0..=u16::MAX {
        for route in [0, 1, 2, 3, 4, 5, 6, 255] {
            let Request::PrimaryGpioRegister { value } = stock_receive_requests(route, prior)[0]
            else {
                panic!()
            };
            assert_eq!(value & !0x0033, prior & !0x0033);
            assert!((value & 0x0033).count_ones() <= 1);
        }
    }
}

#[test]
fn unsupported_frequency_never_reuses_a_previous_route() {
    for frequency in [
        0,
        1_799_999,
        66_000_001,
        80_000_000,
        83_999_999,
        130_000_001,
        u32::MAX,
    ] {
        assert_eq!(receive_plan(frequency, 0xffff), None);
    }
}

#[test]
fn internal_route_changes_need_not_move_the_relay() {
    let low = select_routes(4_499_999, SelectorMode::Primary).unwrap();
    let high = select_routes(4_500_000, SelectorMode::Primary).unwrap();
    assert_eq!((low.rx_code(), high.rx_code()), (Some(1), Some(2)));
    assert_eq!(receive_plan(4_499_999, 0), receive_plan(4_500_000, 0));
    assert_ne!(receive_plan(6_400_000, 0), receive_plan(6_400_001, 0));
    assert_ne!(receive_plan(25_999_999, 0), receive_plan(26_000_000, 0));
}

#[test]
fn secondary_context_preserves_stock_uhf_exception() {
    for frequency in [39_999_999, 48_000_001] {
        assert_eq!(
            select_routes(frequency, SelectorMode::Secondary)
                .unwrap()
                .rx_code(),
            None
        );
    }
    for frequency in [40_000_000, 48_000_000] {
        assert_eq!(
            select_routes(frequency, SelectorMode::Secondary)
                .unwrap()
                .rx_code(),
            Some(4)
        );
    }
}

#[test]
fn tx_and_rx_selectors_use_different_low_band_thresholds() {
    for (frequency, tx, rx) in [
        (3_199_999, 1, 1),
        (3_200_000, 3, 1),
        (4_499_999, 3, 1),
        (4_500_000, 3, 2),
        (6_400_000, 3, 2),
        (6_400_001, 3, 3),
    ] {
        let update = select_routes(frequency, SelectorMode::Primary).unwrap();
        assert_eq!((update.tx_code(), update.rx_code()), (tx, Some(rx)));
    }
}
