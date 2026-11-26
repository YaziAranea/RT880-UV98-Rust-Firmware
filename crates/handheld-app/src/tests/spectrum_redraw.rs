use super::*;
use core::num::NonZeroU32;
use uv98_spectrum::compact::{HeldSample, Plot};

#[test]
fn spectrum_listen_moves_measured_history_and_returns_to_the_same_grid() {
    use render::color::{ACCENT, BACKGROUND};

    const WATERFALL_TOP: usize = 190;
    const WATERFALL_END: usize = 254;
    const GRAPH_START: usize = 12;
    const GRAPH_END: usize = 228;
    const HELD_START: usize = GRAPH_START + (GRAPH_END - GRAPH_START) * 4 / 5;
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let history = || -> [[u16; render::WIDTH]; 16] {
        core::array::from_fn(|age| spectrum_row(&capture, WATERFALL_TOP + age * 4))
    };
    let marker = || {
        (WATERFALL_TOP..WATERFALL_TOP + 16)
            .find(|row| capture.borrow().pixels[row * render::WIDTH + 5] == ACCENT)
    };
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
    menu(&mut app, controller::MenuItem::Spectrum);
    let mut snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_200_000,
        center_10hz: 43_202_500,
        step_10hz: 1250,
        len: 5,
        plot: Plot::EMPTY,
    };
    snapshot.plot.sweep = 1;
    snapshot.plot.display_floor = Some(128);
    snapshot.plot.latest[..5].copy_from_slice(&[134, 144, 154, 164, 174]);
    snapshot.plot.valid[0] = 0b11101;
    assert!(app.set_spectrum(snapshot).unwrap());
    drain_spectrum_rows(&mut app);
    let first_sweep = spectrum_row(&capture, WATERFALL_TOP);
    assert_eq!(first_sweep[60], BACKGROUND);
    assert_ne!(first_sweep[160], BACKGROUND);

    snapshot.plot.sweep += 1;
    snapshot.plot.latest[..5].copy_from_slice(&[174, 164, 154, 144, 134]);
    snapshot.plot.valid[0] = 0b10111;
    assert!(app.set_spectrum(snapshot).unwrap());
    drain_spectrum_rows(&mut app);
    let before_listen = history();
    let wide = before_listen[0];
    assert_ne!(wide[60], BACKGROUND);
    assert_eq!(wide[160], BACKGROUND);
    assert_ne!(wide[HELD_START], first_sweep[HELD_START]);

    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Down));
    drain_spectrum_rows(&mut app);
    let bin = app.status.view.spectrum_cursor;
    assert_eq!(bin, 4);
    assert_eq!(
        event(&mut app, InputEvent::Short(Key::Digit6)).request,
        Some(Request::ListenSpectrum { bin })
    );
    assert!(app.confirm_spectrum_listen(bin).unwrap());
    drain_spectrum_rows(&mut app);

    // A broad sweep and the first confirmed Listen sample may arrive together.
    // Non-held bins advance for the broad sweep only.
    snapshot.state = SpectrumState::Listening;
    snapshot.plot.sweep += 1;
    snapshot.plot.held_sample = Some(HeldSample {
        sequence: NonZeroU32::MIN,
        bin,
        raw: 180,
    });
    assert!(app.set_spectrum(snapshot).unwrap());
    let mut next = snapshot;
    next.plot.held_sample.as_mut().unwrap().sequence = NonZeroU32::new(2).unwrap();
    next.plot.held_sample.as_mut().unwrap().raw = 136;
    assert!(
        !app.set_spectrum(next).unwrap(),
        "queued rows use one sample"
    );
    drain_spectrum_rows(&mut app);
    let strong = spectrum_row(&capture, WATERFALL_TOP);
    let strong_reading: [_; 7] = core::array::from_fn(|row| spectrum_row(&capture, 255 + row));
    let assert_neighbors_unchanged = || {
        for y in WATERFALL_TOP..WATERFALL_END {
            let age = (y - WATERFALL_TOP) / 4;
            assert_eq!(
                spectrum_row(&capture, y)[GRAPH_START..HELD_START],
                before_listen[age.saturating_sub(1)][GRAPH_START..HELD_START],
                "non-held bins moved in LCD row {y}"
            );
        }
    };
    assert_neighbors_unchanged();
    assert_ne!(strong[HELD_START], wide[HELD_START]);
    assert!(strong[HELD_START..GRAPH_END]
        .iter()
        .all(|&ink| ink == strong[HELD_START]));
    assert_eq!(history()[1][HELD_START], wide[HELD_START]);
    assert_eq!(history()[3][HELD_START], first_sweep[HELD_START]);
    let first_history = history();
    assert!(app.set_spectrum(snapshot).unwrap());
    assert!(
        !app.render_pending(),
        "publishing the same sample is a no-op"
    );
    assert_eq!(history(), first_history);

    assert!(app.set_spectrum(next).unwrap());
    assert_eq!(
        drain_spectrum_rows(&mut app),
        64 + 7,
        "held-only changes update the waterfall and RSSI without repainting the graph"
    );
    let weak = spectrum_row(&capture, WATERFALL_TOP);
    assert_ne!(
        weak[HELD_START], strong[HELD_START],
        "held RSSI sets waterfall color"
    );
    let weak_reading: [_; 7] = core::array::from_fn(|row| spectrum_row(&capture, 255 + row));
    assert_ne!(
        weak_reading, strong_reading,
        "held RSSI updates the readout"
    );
    assert_eq!(history()[1][HELD_START], strong[HELD_START]);
    assert_neighbors_unchanged();
    assert_matches_complete_frame(&mut app, &capture);

    for sequence in 3..=20 {
        next.plot.held_sample.as_mut().unwrap().sequence = NonZeroU32::new(sequence).unwrap();
        let before = marker();
        assert!(app.set_spectrum(next).unwrap());
        let rows = drain_spectrum_rows(&mut app);
        assert_neighbors_unchanged();
        if sequence >= 18 {
            assert_eq!(rows, 16, "steady carrier updates only the marker band");
            assert_ne!(before, marker(), "marker advances at steady RSSI");
            assert!(marker().is_some());
        }
    }
    for row in history() {
        assert_eq!(row[HELD_START..GRAPH_END], weak[HELD_START..GRAPH_END]);
    }
    assert_matches_complete_frame(&mut app, &capture);

    event(&mut app, InputEvent::Short(Key::Menu));
    assert!(app.status.view.spectrum_menu);
    drain_spectrum_rows(&mut app);
    next.plot.held_sample.as_mut().unwrap().sequence = NonZeroU32::new(21).unwrap();
    next.plot.held_sample.as_mut().unwrap().raw = 180;
    assert!(app.set_spectrum(next).unwrap());
    assert!(
        !app.render_pending(),
        "overlay covers held RSSI and history"
    );
    event(&mut app, InputEvent::Short(Key::Exit));
    assert_matches_complete_frame(&mut app, &capture);
    assert_neighbors_unchanged();
    assert_eq!(history()[0][HELD_START], strong[HELD_START]);

    assert_eq!(
        event(&mut app, InputEvent::Short(Key::Exit)).request,
        Some(Request::StopListenSpectrum)
    );
    app.set_spectrum_listening(false).unwrap();
    drain_spectrum_rows(&mut app);
    assert_eq!(marker(), None);
    assert_matches_complete_frame(&mut app, &capture);
    let held = history();

    snapshot.state = SpectrumState::Sweeping;
    snapshot.window_id += 1;
    snapshot.plot.sweep = 1;
    snapshot.plot.held_sample = None;
    assert!(app.set_spectrum(snapshot).unwrap());
    drain_spectrum_rows(&mut app);
    assert_eq!(spectrum_row(&capture, WATERFALL_TOP), wide);
    for (row, previous) in history()[1..].iter().zip(&held) {
        assert_eq!(
            row[GRAPH_START..GRAPH_END],
            previous[GRAPH_START..GRAPH_END]
        );
    }

    snapshot.start_10hz += snapshot.step_10hz;
    snapshot.center_10hz += snapshot.step_10hz;
    snapshot.window_id += 1;
    snapshot.plot = Plot::EMPTY;
    assert!(app.set_spectrum(snapshot).unwrap());
    assert_matches_complete_frame(&mut app, &capture);
    for row in WATERFALL_TOP..WATERFALL_END {
        assert_eq!(spectrum_row(&capture, row), [BACKGROUND; render::WIDTH]);
    }
}

fn drain_spectrum_rows(app: &mut App<Memory, Panel, NoDelay>) -> u16 {
    let mut rows = 0;
    while app.render_pending() {
        rows += app.render_step(6).unwrap().rows;
    }
    rows
}

fn spectrum_row(capture: &RefCell<CapturedFrame>, row: usize) -> [u16; render::WIDTH] {
    let row = row * render::WIDTH;
    capture.borrow().pixels[row..row + render::WIDTH]
        .try_into()
        .unwrap()
}

#[test]
fn automatic_capture_at_the_edge_preserves_the_manual_cursor_and_its_arrow() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let mut app = App::new(
        Memory::new(), panel, NoDelay, Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Spectrum);
    let mut grid = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 86_802_500,
        center_10hz: 86_807_500,
        step_10hz: 2_500,
        len: 5,
        plot: Plot::EMPTY,
    };
    grid.plot.valid[0] = 0b11111;
    grid.plot.sweep = 1;
    grid.plot.latest[..5].copy_from_slice(&[103, 54, 50, 51, 52]);
    assert!(app.set_spectrum(grid).unwrap());
    drain_spectrum_rows(&mut app);
    let arrow = spectrum_row(&capture, 176);
    assert_eq!(app.status.view.spectrum_cursor, 2);
    assert_eq!(arrow[120], render::color::ACCENT);

    grid.plot.sweep = 2;
    grid.plot.trigger_bin = Some(0);
    grid.plot.held_bin = Some(0);
    assert!(app.set_spectrum(grid).unwrap());
    drain_spectrum_rows(&mut app);
    assert!(app.begin_spectrum_peak_listen(0).unwrap());
    assert!(!app.confirm_spectrum_listen(2).unwrap());
    assert!(app.confirm_spectrum_listen(0).unwrap());
    assert_eq!(app.status.view.spectrum_rx_bin(), 0);
    drain_spectrum_rows(&mut app);
    grid.state = SpectrumState::Listening;
    grid.plot.held_sample = Some(HeldSample {
        bin: 0, raw: 57, sequence: NonZeroU32::MIN,
    });
    assert!(app.set_spectrum(grid).unwrap());
    drain_spectrum_rows(&mut app);
    assert_eq!(app.status.view.spectrum_cursor, 2);
    assert_eq!(spectrum_row(&capture, 176), arrow);
    assert_eq!(
        event(&mut app, InputEvent::Short(Key::Star)).request,
        Some(Request::AcceptSpectrum { bin: 0 })
    );
    app.set_spectrum_listening(false).unwrap();
    assert_eq!(app.status.view.spectrum_rx_bin(), 2);
}
