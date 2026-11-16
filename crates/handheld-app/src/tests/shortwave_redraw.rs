use super::*;

#[test]
fn hf_acquisition_redraws_progress_only_and_complete_sweeps_match_full_frame() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Shortwave);
    event(&mut app, InputEvent::Hold(Key::Digit0));
    assert!(app.status().view.shortwave_scope);
    let mut sampled_rows = 0;
    for sweep in 0..2 {
        for bin in 0..32 {
            while app.render_pending() {
                app.render_step(6).unwrap();
            }
            let before = capture.borrow().pixels.clone();
            let reads = app.nor.reads;
            app.shortwave_scope_sample(7_084_000, 1000, bin, 10 + bin + sweep, 8, bin == 31)
                .unwrap();
            let mut rows = 0;
            while app.render_pending() {
                rows += app.render_step(6).unwrap().rows;
            }
            assert_eq!(app.nor.reads, reads);
            if sweep == 0 && bin == 0 {
                assert_eq!(rows, 228);
            } else if bin == 31 {
                assert_eq!(rows, 125);
            } else {
                assert_eq!(rows, 2);
                let frame = capture.borrow();
                assert_eq!(
                    &before[96 * 240..155 * 240],
                    &frame.pixels[96 * 240..155 * 240]
                );
                assert_eq!(
                    &before[172 * 240..236 * 240],
                    &frame.pixels[172 * 240..236 * 240]
                );
            }
            if sweep == 1 {
                sampled_rows += rows;
            }
            assert_matches_complete_frame(&mut app, &capture);
        }
    }
    // Previously every sample invalidated 185 rows: 5920 rows per sweep.
    assert_eq!(sampled_rows, 31 * 2 + 125);
    assert_eq!(app.shortwave_scope.history_rows(), 2);
    // A changed measurement context clears old history before the next bin,
    // even when its centre and bin spacing are unchanged.
    for change in 0..3 {
        match change {
            0 => app.status.view.shortwave_filter = 2,
            1 => app.status.view.shortwave_bfo = 125,
            _ => app.status.view.shortwave_mode = controller::ShortwaveMode::Usb,
        }
        app.draw().unwrap();
        while app.render_pending() {
            app.render_step(6).unwrap();
        }
        assert_eq!(app.shortwave_scope.history_rows(), 0);
        assert_matches_complete_frame(&mut app, &capture);
        for bin in 0..32 {
            app.shortwave_scope_sample(7_084_000, 1000, bin, 20, 8, bin == 31)
                .unwrap();
        }
        while app.render_pending() {
            app.render_step(6).unwrap();
        }
        assert_eq!(app.shortwave_scope.history_rows(), 1);
    }
}
