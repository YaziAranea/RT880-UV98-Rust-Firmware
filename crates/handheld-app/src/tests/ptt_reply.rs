#[path = "../../../../firmware-diagnostic/src/reply.rs"]
mod reply;

use super::*;

#[test]
fn confirmed_upper_edge_after_lower_release_rearms_during_reply() {
    use uv98_radio_runtime::ptt;

    let released = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    let release_guard = ptt::Input {
        upper: false,
        lower: false,
        clean: true,
    };
    let lower = InputSample::WithAux {
        matrix: 0,
        ptt_pressed: false,
        aux: Some(Key::Aux16),
    };
    let lower_guard = ptt::Input {
        upper: false,
        lower: true,
        clean: true,
    };
    let upper = InputSample::Stable {
        matrix: 0,
        ptt_pressed: true,
    };
    let upper_guard = ptt::Input {
        upper: true,
        lower: false,
        clean: true,
    };

    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    app.enable_fast_matrix_input();
    app.enable_fast_ptt_input();
    let mut guard = ptt::Guard::new();
    for at in [0, 30] {
        assert!(!guard.observe(release_guard, false));
        app.sample(at, released).unwrap();
    }
    for at in [100, 106] {
        assert!(!guard.observe(lower_guard, false));
        assert!(!app.sample(at, lower).unwrap().ptt_tx_requested);
    }
    assert!(!guard.observe(lower_guard, false));
    assert_eq!(app.sample(112, lower).unwrap().ptt_slot, Some(Slot::B));
    assert!(guard.arm(lower_guard));

    // Clean release is observed, then tx_ended disarms in the same pass.
    assert!(!guard.observe(release_guard, false));
    guard.disarm();
    let mut window = reply::ReplyWindow::Tuning;
    app.sample(120, released).unwrap();
    assert!(!guard.allow_edge());
    assert!(!guard.observe(release_guard, false));
    app.sample(132, released).unwrap();
    assert!(guard.allow_edge());
    assert!(!app.status().view.ptt_down);

    // RX became ready and the first sample found no repeater audio.
    assert!(window.sample_due(165, true));
    window.observed(165, false);
    assert!(window.active());
    // The scanner delivered a confirmed PB6 press following a fresh idle ADC.
    // Feeding the first observation during the listening window must still
    // allow ordinary debouncing and a new arm without another release.
    for at in [166, 172] {
        assert!(guard.allow_edge());
        assert!(!guard.observe(upper_guard, false));
        assert!(!app.sample(at, upper).unwrap().ptt_tx_requested);
    }
    assert!(!guard.observe(upper_guard, false));
    assert_eq!(app.sample(178, upper).unwrap().ptt_slot, Some(Slot::A));
    assert!(guard.arm(upper_guard));
}
