use uv98_sidekeys_at32::BatteryMonitor;

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
