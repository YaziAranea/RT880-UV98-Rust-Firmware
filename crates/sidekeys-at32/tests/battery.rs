use uv98_sidekeys_at32::{BatteryMonitor, BatterySag};

#[test]
fn voltage_matches_stock_decivolts_at_and_between_boundaries() {
    for (raw, millivolts) in [(646, 4000), (1223, 7300), (1224, 7400), (1512, 9000)] {
        assert_eq!(BatteryMonitor::default().observe(raw), Some(millivolts));
    }
}

#[test]
fn rail_and_implausible_readings_clear_previous_voltage() {
    for raw in [0, 645, 1513, 4095, 4096, u16::MAX] {
        let mut battery = BatteryMonitor::default();
        assert_eq!(battery.observe(1224), Some(7400));
        assert_eq!(battery.observe(raw), None);
        assert_eq!(battery.observe(1360), Some(8200));
    }
}

#[test]
fn two_sample_average_smooths_raw_values_before_quantization() {
    let mut battery = BatteryMonitor::default();
    assert_eq!(battery.observe(1224), Some(7400));
    assert_eq!(battery.observe(1292), Some(7600));
    assert_eq!(battery.observe(1292), Some(7800));
    // Truncating the average of the quantized codes would yield 7.4 V.
    let mut battery = BatteryMonitor::default();
    assert_eq!(battery.observe(1240), Some(7400));
    assert_eq!(battery.observe(1242), Some(7500));
}

#[test]
fn tx_sag_uses_latest_loaded_voltage_on_stock_scale() {
    let mut sag = BatterySag::default();
    sag.observe_resting(100, 1224);
    sag.begin_tx(350);
    assert_eq!(sag.millivolts(), None);

    for (raw, millivolts) in [(1156, 400), (1223, 100), (1224, 0), (1230, 0), (1241, 0)] {
        sag.observe_tx(raw);
        assert_eq!(sag.millivolts(), Some(millivolts));
    }
}

#[test]
fn invalid_rx_discards_baseline_and_invalid_tx_clears_result() {
    for raw in [0, 645, 1513, 4095, 4096, u16::MAX] {
        let mut sag = BatterySag::default();
        sag.observe_resting(100, 1224);
        sag.observe_resting(200, raw);
        sag.begin_tx(300);
        sag.observe_tx(1156);
        assert_eq!(sag.millivolts(), None);

        sag.end_tx();
        sag.observe_resting(400, 1224);
        sag.begin_tx(500);
        sag.observe_tx(1156);
        assert_eq!(sag.millivolts(), Some(400));
        sag.observe_tx(raw);
        assert_eq!(sag.millivolts(), None);
        sag.observe_tx(1156);
        assert_eq!(sag.millivolts(), Some(400));
    }
}

#[test]
fn baseline_age_is_inclusive_across_millisecond_wrap() {
    for at in [100_u32, u32::MAX - 100] {
        for (age, expected) in [(1000, Some(400)), (1001, None)] {
            let mut sag = BatterySag::default();
            sag.observe_resting(at, 1224);
            sag.begin_tx(at.wrapping_add(age));
            sag.observe_tx(1156);
            assert_eq!(sag.millivolts(), expected);
        }
    }
}

#[test]
fn subsequent_tx_requires_its_own_rx_sample() {
    let mut sag = BatterySag::default();
    sag.begin_tx(0);
    sag.observe_tx(1156);
    assert_eq!(sag.millivolts(), None);
    sag.end_tx();

    sag.observe_resting(100, 1224);
    sag.begin_tx(200);
    sag.observe_tx(1156);
    assert_eq!(sag.millivolts(), Some(400));
    sag.end_tx();
    assert_eq!(sag.millivolts(), None);

    sag.begin_tx(300);
    sag.observe_tx(1156);
    assert_eq!(sag.millivolts(), None);
    sag.end_tx();

    sag.observe_resting(400, 1292);
    sag.begin_tx(500);
    sag.observe_tx(1156);
    assert_eq!(sag.millivolts(), Some(800));
}
