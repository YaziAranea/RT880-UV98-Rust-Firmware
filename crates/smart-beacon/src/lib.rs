#![no_std]
//! Schedule automatic APRS requests using a monotonic wrapping millisecond
//! clock and RMC motion samples supplied by the caller. The caller owns all
//! radio admission, channel checks and execution.

use uv98_gps_nmea::MotionFix;

const HALF_CLOCK: u32 = 1 << 31;
const FIX_AGE_MS: u32 = 5_000;
const RETRY_MS: u32 = 30_000;
const STOPPED_MS: u32 = 30 * 60_000;
const SLOW_MS: u32 = 5 * 60_000;
const FAST_MS: u32 = 60_000;
const TURN_MS: u32 = 30_000;
const MOVING_SPEED: u32 = 54; // 1 km/h, in hundredths of a knot
const TURN_SPEED: u32 = 432; // 8 km/h
const TURN_ANGLE: u16 = 3_000;
const FULL_CIRCLE: u16 = 36_000;

/// Automatic beacon session bound to a caller-defined channel identity.
pub struct SmartBeacon<Target> {
    armed_target: Option<Target>,
    pending: Option<MotionFix>,
    last_attempt: Option<u32>,
    last_sent: Option<(u32, Option<u16>)>,
}

impl<Target: Eq> SmartBeacon<Target> {
    pub const fn new() -> Self {
        Self {
            armed_target: None,
            pending: None,
            last_attempt: None,
            last_sent: None,
        }
    }

    /// Supply the operator-authorized target, or `None` when disarmed or
    /// unavailable. A changed target disarms the session. A revoked packet
    /// must finish before a new session can arm. The caller clears the
    /// operator's authorization when this returns false.
    pub fn update_arm(&mut self, target: Option<Target>) -> bool {
        self.armed_target = match (&self.armed_target, target) {
            (Some(previous), Some(target)) if *previous != target => None,
            (None, Some(_)) if self.pending() => None,
            (_, target) => target,
        };
        self.armed()
    }

    pub const fn armed(&self) -> bool {
        self.armed_target.is_some()
    }

    pub const fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Check authorization and freshness before each radio step for a pending
    /// automatic packet. For manual packets this check is always true.
    pub fn packet_valid(&self, now_ms: u32, current_fix: Option<MotionFix>) -> bool {
        self.pending.is_none_or(|fix| {
            self.armed()
                && fresh(now_ms, fix)
                && current_fix.is_some_and(|current| fresh(now_ms, current))
        })
    }

    /// Returns the exact position/motion sample to use for the next request.
    /// The caller still checks whether the radio can admit a request.
    pub fn due(&self, now_ms: u32, fix: Option<MotionFix>) -> Option<MotionFix> {
        let fix = fix?;
        if !self.armed()
            || self.pending()
            || !fresh(now_ms, fix)
            || self
                .last_attempt
                .is_some_and(|at| elapsed(now_ms, at).is_none_or(|age| age < RETRY_MS))
        {
            return None;
        }
        let Some((sent_at, sent_course)) = self.last_sent else {
            return Some(fix);
        };
        let age = elapsed(now_ms, sent_at)?;
        let speed = fix.speed_centi_knots.unwrap_or(0);
        let interval = if speed < MOVING_SPEED {
            STOPPED_MS
        } else if speed <= TURN_SPEED {
            SLOW_MS
        } else {
            (SLOW_MS * TURN_SPEED / speed).max(FAST_MS)
        };
        if age >= interval {
            return Some(fix);
        }
        if age < TURN_MS {
            return None;
        }
        let difference = sent_course?.abs_diff(turn_course(fix)?);
        (TURN_ANGLE..=FULL_CIRCLE - TURN_ANGLE)
            .contains(&difference)
            .then_some(fix)
    }

    /// Every radio request attempt, including a manual send or rejection,
    /// starts the retry delay. Only an admitted automatic request stores a fix.
    pub fn request_attempted(&mut self, now_ms: u32, auto_fix: Option<MotionFix>, accepted: bool) {
        self.last_attempt = Some(now_ms);
        if accepted {
            self.pending = auto_fix;
        }
    }

    /// Retire an admitted request. Only a successful automatic transmission
    /// updates the course baseline, using the captured fix.
    pub fn complete(&mut self, now_ms: u32, sent: bool) {
        if let Some(fix) = self.pending.take() {
            if sent {
                self.last_sent = Some((now_ms, turn_course(fix)));
            }
        }
    }
}

impl<Target: Eq> Default for SmartBeacon<Target> {
    fn default() -> Self {
        Self::new()
    }
}

fn fresh(now_ms: u32, fix: MotionFix) -> bool {
    now_ms.wrapping_sub(fix.received_at_ms) <= FIX_AGE_MS
}

fn turn_course(fix: MotionFix) -> Option<u16> {
    fix.course_centi_degrees
        .filter(|course| *course < FULL_CIRCLE && fix.speed_centi_knots.unwrap_or(0) >= TURN_SPEED)
}

fn elapsed(now_ms: u32, since_ms: u32) -> Option<u32> {
    let age = now_ms.wrapping_sub(since_ms);
    (age < HALF_CLOCK).then_some(age)
}
