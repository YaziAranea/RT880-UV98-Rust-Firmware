use uv98_gps_nmea::{MotionFix, Position};
use uv98_smart_beacon::SmartBeacon;

const STOPPED_MS: u32 = 1_800_000;
const SLOW_MS: u32 = 300_000;
const FAST_MS: u32 = 60_000;
const TURN_MS: u32 = 30_000;
const RETRY_MS: u32 = 30_000;
const TURN_SPEED: u32 = 432;
const MOVING_SPEED: u32 = 54;
const CHANNEL: u8 = 1;

fn fix(now_ms: u32, speed: Option<u32>, course: Option<u16>) -> MotionFix {
    MotionFix {
        position: Position {
            latitude_e7: 500_000_000,
            longitude_e7: 900_000_000,
        },
        speed_centi_knots: speed,
        course_centi_degrees: course,
        received_at_ms: now_ms,
    }
}

#[test]
fn first_request_requires_arm_and_fresh_fix_and_rejections_back_off() {
    let mut scheduler = SmartBeacon::new();
    let first = fix(100, None, None);
    assert!(!scheduler.armed());
    assert_eq!(scheduler.due(100, Some(first)), None);
    assert!(scheduler.update_arm(Some(CHANNEL)));
    assert!(scheduler.armed());
    assert_eq!(scheduler.due(99, Some(first)), None);
    assert_eq!(scheduler.due(100, None), None);
    assert_eq!(scheduler.due(5_101, Some(first)), None);
    assert_eq!(scheduler.due(5_100, Some(first)), Some(first));
    scheduler.request_attempted(5_100, Some(first), false);
    assert!(!scheduler.update_arm(None));
    assert!(scheduler.update_arm(Some(CHANNEL)));
    assert_eq!(scheduler.due(35_099, Some(fix(35_099, None, None))), None);
    let retry = fix(35_100, None, None);
    assert_eq!(scheduler.due(35_100, Some(retry)), Some(retry));
    scheduler.request_attempted(35_100, Some(retry), true);
    scheduler.complete(35_200, true);
    let early = 35_200 + STOPPED_MS - 1;
    assert_eq!(scheduler.due(early, Some(fix(early, None, None))), None);
    let due = early + 1;
    assert!(scheduler.due(due, Some(fix(due, None, None))).is_some());
}

#[test]
fn interval_varies_with_speed_between_conservative_limits() {
    let mut scheduler = SmartBeacon::new();
    scheduler.update_arm(Some(CHANNEL));
    scheduler.request_attempted(0, Some(fix(0, Some(0), Some(0))), true);
    scheduler.complete(10, true);
    for (speed, interval) in [
        (0, STOPPED_MS),
        (MOVING_SPEED - 1, STOPPED_MS),
        (MOVING_SPEED, SLOW_MS),
        (TURN_SPEED, SLOW_MS),
        (TURN_SPEED * 2, SLOW_MS / 2),
        (TURN_SPEED * 10, FAST_MS),
        (u32::MAX, FAST_MS),
    ] {
        let early = 10 + interval - 1;
        assert_eq!(
            scheduler.due(early, Some(fix(early, Some(speed), None))),
            None
        );
        let due = early + 1;
        assert!(scheduler
            .due(due, Some(fix(due, Some(speed), None)))
            .is_some());
    }
}

#[test]
fn corner_requires_qualified_course_and_uses_last_success_across_rollover() {
    let mut scheduler = SmartBeacon::new();
    let sent_at = u32::MAX - 10;
    let baseline = fix(sent_at, Some(TURN_SPEED), Some(35_900));
    scheduler.update_arm(Some(CHANNEL));
    assert_eq!(scheduler.due(sent_at, Some(baseline)), Some(baseline));
    scheduler.request_attempted(sent_at, Some(baseline), true);
    scheduler.complete(sent_at, true);

    let now = sent_at.wrapping_add(TURN_MS);
    assert_eq!(
        scheduler.due(now, Some(fix(now, Some(TURN_SPEED), Some(100)))),
        None
    );
    assert_eq!(
        scheduler.due(now, Some(fix(now, Some(TURN_SPEED), Some(2_899)))),
        None
    );
    assert_eq!(
        scheduler.due(now, Some(fix(now, Some(TURN_SPEED - 1), Some(2_900)))),
        None
    );
    assert_eq!(
        scheduler.due(now, Some(fix(now, Some(TURN_SPEED), None))),
        None
    );
    let early = now - 1;
    assert_eq!(
        scheduler.due(early, Some(fix(early, Some(TURN_SPEED), Some(2_900)))),
        None
    );

    let corner = fix(now, Some(TURN_SPEED), Some(2_900));
    assert_eq!(scheduler.due(now, Some(corner)), Some(corner));
    scheduler.request_attempted(now, Some(corner), true);
    scheduler.complete(now, false);
    assert_eq!(scheduler.due(now + 1, Some(corner)), None);
    let retry_at = now + RETRY_MS;
    let retry = fix(retry_at, Some(TURN_SPEED), Some(2_900));
    assert_eq!(scheduler.due(retry_at, Some(retry)), Some(retry));
    scheduler.request_attempted(retry_at, Some(retry), true);
    scheduler.complete(retry_at + 500, true);
    let next = retry_at + TURN_MS + 500;
    assert_eq!(
        scheduler.due(next, Some(fix(next, Some(TURN_SPEED), Some(2_900)))),
        None
    );
    assert!(scheduler
        .due(next, Some(fix(next, Some(TURN_SPEED), Some(35_900))))
        .is_some());
}

#[test]
fn pending_packet_tracks_its_captured_fix_and_armed_target() {
    let mut scheduler = SmartBeacon::new();
    let start = u32::MAX - 100;
    let captured = fix(start, Some(TURN_SPEED), Some(0));
    assert!(scheduler.packet_valid(start, None));
    scheduler.update_arm(Some(CHANNEL));
    scheduler.request_attempted(start, Some(captured), true);
    assert!(scheduler.pending());

    let last_fresh = start.wrapping_add(5_000);
    let latest = fix(last_fresh, Some(TURN_SPEED), Some(3_000));
    assert!(scheduler.packet_valid(last_fresh, Some(latest)));
    assert!(!scheduler.packet_valid(last_fresh, None));
    let stale = fix(start - 1, None, None);
    assert!(!scheduler.packet_valid(last_fresh, Some(stale)));
    assert!(!scheduler.packet_valid(last_fresh + 1, Some(latest)));

    let due = start.wrapping_add(RETRY_MS);
    let fresh = fix(due, None, None);
    assert_eq!(scheduler.due(due, Some(fresh)), None);
    assert!(!scheduler.update_arm(Some(CHANNEL + 1)));
    assert!(!scheduler.armed());
    assert!(scheduler.pending());
    assert!(!scheduler.update_arm(Some(CHANNEL + 1)));
    scheduler.complete(due, false);
    assert!(!scheduler.pending());
    assert!(scheduler.update_arm(Some(CHANNEL + 1)));
    assert_eq!(scheduler.due(due, Some(fresh)), Some(fresh));
}

#[test]
fn manual_send_backs_off_without_resetting_the_automatic_interval() {
    let mut scheduler = SmartBeacon::new();
    scheduler.update_arm(Some(CHANNEL));
    scheduler.request_attempted(0, Some(fix(0, None, None)), true);
    scheduler.complete(10, true);

    let automatic_due = STOPPED_MS + 10;
    let manual_at = automatic_due - 1;
    scheduler.request_attempted(manual_at, None, true);
    assert!(!scheduler.pending());
    scheduler.complete(manual_at, true);
    assert_eq!(
        scheduler.due(automatic_due, Some(fix(automatic_due, None, None))),
        None
    );
    let retry_at = manual_at + RETRY_MS;
    let retry = fix(retry_at, None, None);
    assert_eq!(scheduler.due(retry_at, Some(retry)), Some(retry));
}
