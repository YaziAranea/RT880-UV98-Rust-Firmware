use super::*;

#[test]
fn held_menu_key_drains_three_islands_in_three_polls_instead_of_seven() {
    fn navigate(batched: bool) -> (u16, usize, Vec<u16>) {
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
        event(&mut app, InputEvent::Short(Key::Menu));
        while app.render_pending() {
            app.render_step(4).unwrap();
        }
        event(&mut app, InputEvent::Short(Key::Down));
        let reads = app.nor.reads;
        let mut rows = 0;
        let mut polls = 0;
        while app.render_pending() {
            let progress = if batched {
                app.render_batch(6, || true)
            } else {
                app.render_step(3)
            }
            .unwrap();
            assert!(progress.rows > 0);
            assert!(progress.rows <= if batched { 6 } else { 3 });
            rows += progress.rows;
            polls += 1;
        }
        assert_eq!(app.nor.reads, reads);
        assert_matches_complete_frame(&mut app, &capture);
        let pixels = capture.borrow().pixels.clone();
        (rows, polls, pixels)
    }

    let (old_rows, old_polls, old_frame) = navigate(false);
    let (new_rows, new_polls, new_frame) = navigate(true);
    assert_eq!((old_rows, old_polls), (15, 7));
    assert_eq!((new_rows, new_polls), (15, 3));
    assert_eq!(old_frame, new_frame);
}

#[test]
fn admission_retains_unpainted_rows_and_tx_priority_across_batches() {
    let panel = Panel::default();
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    windows.borrow_mut().clear();
    app.pending_rows.mark(300, 302);
    app.row_cursor = 300;
    app.mark_priority((30, 31));
    assert_eq!(
        app.render_batch(2, || false).unwrap(),
        RenderProgress {
            rows: 0,
            pending: true
        }
    );
    assert!(windows.borrow().is_empty());
    let mut admissions = 0;
    assert_eq!(
        app.render_batch(2, || {
            admissions += 1;
            admissions == 1
        })
        .unwrap(),
        RenderProgress {
            rows: 1,
            pending: true
        }
    );
    assert_eq!(admissions, 2);
    assert_eq!(*windows.borrow(), [(30, 31)]);
    assert_eq!(
        app.render_batch(2, || true).unwrap(),
        RenderProgress {
            rows: 2,
            pending: false
        }
    );
    assert_eq!(*windows.borrow(), [(30, 31), (300, 302)]);
}

#[test]
fn rapid_navigation_with_telemetry_and_interrupted_batches_matches_full_frame() {
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
    let reads = app.nor.reads;
    for (index, key) in [
        Key::Menu,
        Key::Down,
        Key::Down,
        Key::Menu,
        Key::Down,
        Key::Up,
        Key::Exit,
        Key::Exit,
        Key::Menu,
        Key::Up,
        Key::Down,
    ]
    .into_iter()
    .enumerate()
    {
        event(&mut app, InputEvent::Short(key));
        app.set_battery_voltage(Some(7100 + index as u16 * 100))
            .unwrap();
        app.set_radio_view(RadioView {
            state: render::RadioState::Receiving,
            squelch_open: index % 2 == 0,
            rssi: Some(160 + index as u16 * 5),
            ..RadioView::OFF
        })
        .unwrap();
        let mut admissions = 0;
        let progress = app
            .render_batch(6, || {
                admissions += 1;
                admissions <= 1 + index % 2
            })
            .unwrap();
        assert!(progress.rows <= 6);
    }
    while app.render_pending() {
        assert!(app.render_batch(6, || true).unwrap().rows > 0);
    }
    assert_eq!(app.nor.reads, reads);
    assert_matches_complete_frame(&mut app, &capture);
}

#[test]
fn pending_home_pass_paints_one_radio_snapshot_until_telemetry_repair() {
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
    let old = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(165),
        meter_floor: 150,
        ..RadioView::OFF
    };
    app.set_radio_view(old).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    let baseline = capture.borrow().pixels.clone();

    app.last_rendered = None;
    app.draw().unwrap();
    let fresh = RadioView {
        squelch_open: true,
        rssi: Some(271),
        ..old
    };
    for step in 0..render::HEIGHT / 4 {
        if step == 3 {
            app.set_radio_view(fresh).unwrap();
            assert_eq!(app.radio_view(), fresh);
            assert_eq!(app.render_radio, old);
        }
        assert_eq!(app.render_step(4).unwrap().rows, 4);
    }
    // The completed pass contains the old meter and banner. Only the next
    // pass may combine the new values, even though it was queued earlier.
    assert_eq!(capture.borrow().pixels, baseline);
    assert!(app.render_pending());
    assert_eq!(app.render_radio, fresh);
    while app.render_pending() {
        app.render_batch(6, || true).unwrap();
    }
    assert_ne!(capture.borrow().pixels, baseline);
    assert_matches_complete_frame(&mut app, &capture);
}
