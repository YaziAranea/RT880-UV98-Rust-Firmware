#[path = "../../../firmware-diagnostic/src/position_save.rs"]
mod position_save;

use position_save::PositionSave;

#[test]
fn waits_for_a_changed_quiet_position_and_coalesces_transient_changes() {
    let a = [1, 2];
    let b = [3, 4];
    let mut save = PositionSave::new(a, 0);
    assert_eq!(save.observe(Some(a), 6_000), None);
    assert_eq!(save.observe(Some(b), 6_001), None);
    assert_eq!(save.observe(Some(a), 6_002), None);
    assert_eq!(save.observe(Some(a), 12_000), None);
    assert_eq!(save.observe(Some(b), 12_001), None);
    assert_eq!(save.observe(None, 16_000), None);
    assert_eq!(save.observe(Some(b), 20_999), None);
    assert_eq!(save.observe(Some(b), 21_000), Some(b));
    save.attempted(21_000, Some(b));
    assert_eq!(save.observe(Some(b), 60_000), None);
}

#[test]
fn throttles_retries_and_recovers_a_durable_readback_across_clock_wrap() {
    let a = [1, 2];
    let b = [3, 4];
    let mut save = PositionSave::new(a, u32::MAX - 5_001);
    assert_eq!(save.observe(Some(b), u32::MAX - 5_000), None);
    assert_eq!(save.observe(Some(b), 0), Some(b));
    save.attempted(0, None);
    assert_eq!(save.observe(Some(b), 29_999), None);
    assert_eq!(save.observe(Some(b), 30_000), Some(b));
    save.attempted(30_000, Some(b));
    assert_eq!(save.observe(Some(b), 100_000), None);
}
