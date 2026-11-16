use super::*;

#[test]
fn menu_batch_can_drain_both_filled_rows_without_extra_foreground_passes() {
    fn navigate(budget: u16) -> (u16, usize, Vec<u16>) {
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
            let progress = app.render_batch(budget, || true).unwrap();
            assert!(progress.rows > 0);
            assert!(progress.rows <= budget);
            rows += progress.rows;
            polls += 1;
        }
        assert_eq!(app.nor.reads, reads);
        assert_matches_complete_frame(&mut app, &capture);
        let pixels = capture.borrow().pixels.clone();
        (rows, polls, pixels)
    }

    let (old_rows, old_polls, old_frame) = navigate(6);
    let (new_rows, new_polls, new_frame) = navigate(16);
    assert_eq!((old_rows, old_polls), (67, 12));
    assert_eq!((new_rows, new_polls), (67, 5));
    assert_eq!(old_frame, new_frame);
    let (menu_rows, menu_batches, menu_frame) = navigate(67);
    assert_eq!((menu_rows, menu_batches), (67, 1));
    assert_eq!(new_frame, menu_frame);
    // This counts admitted windows, not elapsed time on a physical LCD.
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

#[test]
fn incoming_name_interrupts_pending_paint_and_yields_to_tx_badge() {
    use uv98_handheld_ui::controller::RadioName;

    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    let idle = RadioView {
        state: render::RadioState::Receiving,
        ..RadioView::OFF
    };
    app.set_radio_view(idle).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }

    app.last_rendered = None;
    app.draw().unwrap();
    assert_eq!(app.render_step(4).unwrap().rows, 4);
    let name = RadioName::from_payload(*b"QRST UVWXYZ 0123").unwrap();
    let received = RadioView {
        radio_name: Some(render::NameReception::new(Slot::B, name)),
        ..idle
    };
    app.set_radio_view(received).unwrap();
    assert_eq!(app.render_radio, received);
    let (popup_first, _) = render::NAME_POPUP_PRIORITY_ROWS;
    assert_eq!(app.render_step(4).unwrap().rows, 4);
    assert_eq!(
        windows.borrow().last(),
        Some(&(popup_first, popup_first + 4))
    );
    assert!(app.pending_rows.has(300));
    assert_eq!(app.render_step(4).unwrap().rows, 4);
    assert_eq!(app.priority_run, PRIORITY_BURST_ROWS);

    app.set_radio_view(RadioView {
        state: render::RadioState::Transmitting,
        tx_slot: Some(Slot::B),
        ..received
    })
    .unwrap();
    assert!(!app.priority_rows.has(popup_first + 8));
    let tx_row = render::home_slot_rows(Slot::B).0;
    assert!(app.priority_rows.has(tx_row));
    assert_eq!(app.priority_run, PRIORITY_BURST_ROWS);
    assert_eq!(app.render_step(2).unwrap().rows, 2);
    assert_eq!(app.priority_run, 0); // One ordinary fairness slice.
    assert_eq!(app.render_step(2).unwrap().rows, 2);
    assert_eq!(windows.borrow().last(), Some(&(tx_row, tx_row + 2)));
    assert_matches_complete_frame(&mut app, &capture);
}

#[test]
fn interrupted_outline_and_tx_updates_converge_after_navigation() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let writes = panel.observed_writes.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    app.select_slot(Slot::A).unwrap();
    let idle = RadioView {
        state: render::RadioState::Receiving,
        ..RadioView::OFF
    };
    app.set_radio_view(idle).unwrap();
    while app.render_pending() {
        app.render_batch(16, || true).unwrap();
    }
    let header = capture.borrow().pixels[..28 * render::WIDTH].to_vec();
    let b_top = usize::from(render::home_slot_rows(Slot::B).0) * render::WIDTH;
    let receiving_b = RadioView {
        rx_slot: Some(Slot::B),
        squelch_open: true,
        ..idle
    };
    let reads = app.nor.reads;
    let before = writes.get();
    app.set_radio_view(receiving_b).unwrap();
    assert_eq!(app.pending_outlines.count_ones(), 4);
    assert_eq!(app.render_batch(1, || true).unwrap().rows, 1);
    assert_eq!(writes.get() - before, 11 + 232 * 2);
    assert_eq!(capture.borrow().pixels[b_top + 4], 0x07e0);

    app.set_radio_view(RadioView {
        state: render::RadioState::Transmitting,
        tx_slot: Some(Slot::B),
        ..receiving_b
    })
    .unwrap();
    assert_eq!(app.render_batch(2, || true).unwrap().rows, 2);
    assert_eq!(capture.borrow().pixels[b_top + 4], 0x001f);

    app.set_radio_view(receiving_b).unwrap();
    app.set_radio_view(RadioView {
        rx_slot: Some(Slot::A),
        ..receiving_b
    })
    .unwrap();
    app.render_batch(3, || true).unwrap();
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.pending_outlines, 0);
    app.render_batch(12, || true).unwrap();
    event(&mut app, InputEvent::Short(Key::Exit));
    app.set_radio_view(receiving_b).unwrap();
    assert_matches_complete_frame(&mut app, &capture);
    assert_eq!(app.status().view.active_slot, Slot::A);
    assert_eq!(app.nor.reads, reads);
    assert_eq!(capture.borrow().pixels[..28 * render::WIDTH], header);
    assert_eq!(capture.borrow().pixels[b_top + 4], 0x07e0);
    assert_ne!(capture.borrow().pixels[28 * render::WIDTH + 4], 0x07e0);
    for slot in [Slot::A, Slot::B] {
        let popup = RadioView {
            rx_slot: Some(slot),
            dtmf: render::DtmfReception::new(slot, "11AB*#"),
            ..receiving_b
        };
        app.set_radio_view(popup).unwrap();
        app.render_batch(7, || true).unwrap();
        let closed = RadioView {
            squelch_open: false,
            rx_slot: None,
            ..popup
        };
        app.set_radio_view(closed).unwrap();
        assert_matches_complete_frame(&mut app, &capture);
        app.set_radio_view(popup).unwrap();
        app.render_batch(1, || true).unwrap();
        app.set_radio_view(RadioView {
            state: render::RadioState::Transmitting,
            tx_slot: Some(slot),
            ..popup
        })
        .unwrap();
        assert_matches_complete_frame(&mut app, &capture);
        app.set_radio_view(closed).unwrap();
        app.render_batch(3, || true).unwrap();
        app.set_radio_view(idle).unwrap();
        assert_matches_complete_frame(&mut app, &capture);
    }
}
