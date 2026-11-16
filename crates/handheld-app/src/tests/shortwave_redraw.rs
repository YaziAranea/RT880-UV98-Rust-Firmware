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
    for _ in 0..2 {
        event(&mut app, InputEvent::Short(Key::Hash));
    }
    assert_eq!(app.status().view.shortwave_scope_points, 32);
    let mut sampled_rows = 0;
    for sweep in 0..2 {
        for bin in 0..32 {
            while app.render_pending() {
                app.render_step(6).unwrap();
            }
            let before = capture.borrow().pixels.clone();
            let reads = app.nor.reads;
            app.shortwave_scope_sample(7_084_000, 1000, 32, bin, 10 + bin + sweep, 8, bin == 31)
                .unwrap();
            let mut rows = 0;
            while app.render_pending() {
                rows += app.render_step(6).unwrap().rows;
            }
            assert_eq!(app.nor.reads, reads);
            if sweep == 0 && bin == 0 {
                assert_eq!(rows, 228);
            } else if bin == 31 {
                // First plot: 37 graph rows and one four-row band. Following
                // +1 RSSI sweep: 29 graph rows, two bands. Each adds progress.
                assert_eq!(rows, if sweep == 0 { 43 } else { 39 });
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
    // This fixture needs 101 rows instead of the previous 187.
    assert_eq!(sampled_rows, 101);
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
            app.shortwave_scope_sample(7_084_000, 1000, 32, bin, 20, 8, bin == 31)
                .unwrap();
        }
        while app.render_pending() {
            app.render_step(6).unwrap();
        }
        assert_eq!(app.shortwave_scope.history_rows(), 1);
    }
}

#[test]
fn hf_stable_full_waterfall_preserves_history_and_skips_unchanged_colors() {
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
    for _ in 0..2 {
        event(&mut app, InputEvent::Short(Key::Hash));
    }
    assert_eq!(app.status().view.shortwave_scope_points, 32);
    while app.render_pending() {
        app.render_step(6).unwrap();
    }
    for sweep in 0..18 {
        // 63..=79 differ in raw RSSI but saturate to the same plot and tint.
        let rssi = if sweep == 17 { 0 } else { 63 + sweep };
        let mut total_rows = 0;
        let before = capture.borrow().pixels.clone();
        for bin in 0..32 {
            app.shortwave_scope_sample(7_084_000, 1000, 32, bin, rssi, 8, bin == 31)
                .unwrap();
            while app.render_pending() {
                total_rows += app.render_step(6).unwrap().rows;
            }
        }
        if sweep == 16 {
            assert_eq!(app.shortwave_scope.history_rows(), 16);
            assert_eq!(total_rows, 64);
            assert_eq!(capture.borrow().pixels, before);
        }
        if sweep == 17 {
            let actual = capture.borrow();
            // The changed sweep paints 59 graph rows and just the newest band.
            assert_eq!(total_rows, 64 + 59 + 4);
            assert_ne!(
                &actual.pixels[172 * 240..176 * 240],
                &before[172 * 240..176 * 240]
            );
            assert_eq!(
                &actual.pixels[176 * 240..236 * 240],
                &before[172 * 240..232 * 240]
            );
        }
        assert_matches_complete_frame(&mut app, &capture);
    }
}

#[test]
fn hf_resolution_change_drops_old_history_and_late_points() {
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
    assert_eq!(app.status().view.shortwave_scope_points, 8);
    for bin in 0..8 {
        app.shortwave_scope_sample(7_020_000, 20_000, 8, bin, bin + 24, 8, bin == 7)
            .unwrap();
    }
    while app.render_pending() {
        app.render_step(6).unwrap();
    }
    assert_eq!(app.shortwave_scope.history_rows(), 1);
    assert_matches_complete_frame(&mut app, &capture);

    event(&mut app, InputEvent::Short(Key::Hash));
    assert_eq!(app.status().view.shortwave_scope_points, 16);
    assert_eq!(app.shortwave_scope.history_rows(), 0);
    app.shortwave_scope_sample(7_020_000, 20_000, 8, 7, 60, 8, true)
        .unwrap();
    assert_eq!(app.shortwave_scope.point_count(), 0);
    for bin in 0..16 {
        app.shortwave_scope_sample(7_020_000, 10_000, 16, bin, bin + 10, 8, bin == 15)
            .unwrap();
    }
    while app.render_pending() {
        app.render_step(6).unwrap();
    }
    assert_eq!(app.shortwave_scope.history_rows(), 1);
    assert_eq!(app.shortwave_scope.displayed_samples().unwrap().len(), 16);
    assert_matches_complete_frame(&mut app, &capture);
}
