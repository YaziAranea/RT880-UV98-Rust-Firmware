use super::*;

#[test]
fn navigation_lcd_traffic_and_complete_pixels() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let writes = panel.observed_writes.clone();
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    for (key, label, screen) in [
        (Key::Menu, "home-menu", Screen::Menu),
        (Key::Menu, "menu-setting", Screen::Setting),
        (Key::Down, "setting-cursor", Screen::Setting),
        (Key::Exit, "setting-menu", Screen::Menu),
        (Key::Down, "menu-cursor", Screen::Menu),
        (Key::Menu, "menu-group", Screen::Menu),
        (Key::Exit, "group-menu", Screen::Menu),
        (Key::Exit, "menu-home", Screen::Home),
    ] {
        let original = capture.borrow().pixels.clone();
        let before = writes.get();
        windows.borrow_mut().clear();
        event(&mut app, InputEvent::Short(key));
        assert_eq!(app.status.view.screen, screen);
        let mut first_change = None;
        while app.render_pending() {
            assert!(app.render_step(6).unwrap().rows > 0);
            if first_change.is_none() && capture.borrow().pixels != original {
                first_change = Some(writes.get() - before);
            }
        }
        let bytes = writes.get() - before;
        let windows = windows.borrow().len();
        std::println!(
            "PAGE {label} bytes={bytes} pixels={} windows={windows} first_changed_by_byte={}",
            (bytes - windows * 11) / 2,
            first_change.unwrap()
        );
        assert_matches_complete_frame(&mut app, &capture);
    }
}
