//! Run the foreground reply-window tests without a private calibration fixture.

#[path = "../../../firmware-diagnostic/src/reply.rs"]
mod reply;

use reply::ReplyWindow;

#[test]
fn silent_first_sample_does_not_defer_a_short_reply() {
    let mut window = ReplyWindow::Tuning;
    assert!(!window.sample_due(39, false));
    assert!(window.sample_due(40, true));
    window.observed(40, false);
    assert!(!window.sample_due(40, true));
    // A full repaint admitted after the first observation would miss
    // a brief reply from 42 through 46. Keep polling the same RX path.
    assert!(window.sample_due(41, true));
    window.observed(41, false);
    assert!(window.sample_due(42, true));
    window.observed(42, true);
    assert!(!window.active());
}

#[test]
fn quiet_window_expires_from_rx_ready_across_counter_wrap() {
    let mut window = ReplyWindow::Tuning;
    let ready_at = u32::MAX - 40;
    assert!(!window.sample_due(ready_at - 30, false));
    assert!(window.sample_due(ready_at, true));
    window.observed(ready_at, false);
    assert!(window.sample_due(ready_at.wrapping_add(79), true));
    assert!(!window.sample_due(ready_at.wrapping_add(80), true));
    assert!(!window.active());
}

#[test]
fn rx_lost_during_listen_drops_the_window() {
    let mut window = ReplyWindow::Tuning;
    assert!(window.sample_due(40, true));
    window.observed(40, false);
    assert!(!window.sample_due(41, false));
    assert!(!window.active());
}
