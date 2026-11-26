use super::*;

// Same scan entry point, row budget and channels on both versions. Count bytes
// sent to a portrait LCD, including its address commands. Keep radio-state
// transitions in the workload, as they also invalidate parts of the screen.
#[test]
fn home_scan_lcd_traffic_and_complete_pixels() {
    let mut memory = Memory::new();
    let channels: [(u32, &[u8]); 4] = [
        (43_417_500, b"TEST1"),
        (43_420_000, b"TEST2"),
        (43_280_000, b"ZONE A"),
        (43_407_500, b"RELAY B"),
    ];
    for (index, (frequency, name)) in channels.iter().enumerate() {
        add_channel(&mut memory, index, *frequency);
        let end = 0x4000 + (index + 1) * CHANNEL_LEN;
        memory.bytes[end - 16..end - 16 + name.len()].copy_from_slice(name);
    }
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let windows = panel.windows.clone();
    let writes = panel.observed_writes.clone();
    let mut app = App::new(
        memory,
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    let rx = RadioView {
        state: render::RadioState::Receiving,
        rx_slot: None,
        rssi: Some(168),
        meter_floor: 168,
        power: Some(0),
        route: Some(render::RadioRoute::Uhf),
        ..RadioView::OFF
    };
    app.set_radio_view(rx).unwrap();
    assert_eq!(
        event(&mut app, InputEvent::Hold(Key::Digit3)).request,
        Some(Request::StartScan)
    );
    assert!(app.acknowledge_scan_started().unwrap());
    assert_matches_complete_frame(&mut app, &capture);
    for index in 1..channels.len() {
        let before = writes.get();
        windows.borrow_mut().clear();
        app.select_scan_channel(index as u16).unwrap();
        assert_eq!(app.status().view.channel, Some(index as u16));
        app.set_scan_radio_view(
            RadioView {
                state: render::RadioState::Starting,
                ..RadioView::OFF
            },
            true,
        )
        .unwrap();
        app.set_scan_radio_view(rx, false).unwrap();
        let mut batches = 0;
        while app.render_pending() {
            assert!(app.render_batch(32, || true).unwrap().rows > 0);
            batches += 1;
        }
        let bytes = writes.get() - before;
        let windows = windows.borrow().len();
        assert!(bytes < 20_000, "scan hop {index} sent {bytes} LCD bytes");
        std::println!(
            "LCD hop={} bytes={bytes} pixels={} windows={windows} batches={batches}",
            index,
            (bytes - windows * 11) / 2
        );
        assert_matches_complete_frame(&mut app, &capture);
    }
}
