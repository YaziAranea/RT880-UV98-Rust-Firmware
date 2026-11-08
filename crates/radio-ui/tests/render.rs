use std::{cell::RefCell, rc::Rc};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_radio_ui::{
    color, render, render_row, ChannelView, Error, Model, RenderError, SpectrumView, CHART_BOTTOM,
    CHART_TOP, CHART_WIDTH, CHART_X, HEIGHT, WIDTH,
};
use uv98_spectrum::{Action, Engine, Event, Mode, PeakHold, Range, Ticket, Timing};
use uv98_ui_core::{MenuItem, NumberEntry, Screen, View};

fn view(screen: Screen) -> View {
    View {
        screen,
        channel_index: 0,
        channel_count: 1024,
        entry: None,
        menu_item: MenuItem::Spectrum,
        spectrum_open: screen == Screen::Spectrum,
        ptt_pressed: false,
    }
}
fn model(screen: Screen) -> Model<'static> {
    Model {
        view: view(screen),
        channel: Some(ChannelView {
            name: b"LOCAL CHANNEL",
            rx_10hz: 14_550_000,
            tx_10hz: 14_550_000,
        }),
        spectrum: None,
        demo: true,
    }
}
fn engine(bins: usize, peak: PeakHold) -> Engine {
    Engine::new(
        Range::new(14_400_000, 14_400_000 + bins as u32 - 1, 1).unwrap(),
        Timing::new(100, 100, 0, 100).unwrap(),
        Mode::Continuous,
        peak,
    )
}
fn with_spectrum<'a>(engine: &'a Engine, cursor: usize, scale_max: u16) -> Model<'a> {
    Model {
        view: view(Screen::Spectrum),
        channel: None,
        spectrum: Some(SpectrumView {
            engine,
            cursor,
            scale_max,
        }),
        demo: true,
    }
}
fn prepare(engine: &mut Engine) -> Ticket {
    let Some(Action::PrepareReceive { ticket }) = engine.handle(0, Event::Start) else {
        panic!("prepare")
    };
    let Some(Action::TuneReceive { ticket, .. }) = engine.handle(0, Event::Ready(ticket)) else {
        panic!("tune")
    };
    ticket
}
fn sample(engine: &mut Engine, ticket: Ticket, raw: u16) -> Option<Action> {
    let Some(Action::ReadSignal { ticket, .. }) = engine.handle(0, Event::Settled(ticket)) else {
        panic!("read")
    };
    engine.handle(0, Event::Sample { ticket, raw })
}

#[test]
fn invalid_rows_and_models_leave_output_untouched() {
    let mut row = [0xdead; WIDTH];
    let mut m = model(Screen::Channel);
    assert_eq!(
        render_row(240, &mut row, &m),
        Err(Error::RowOutOfRange { row: 240 })
    );
    assert_eq!(row, [0xdead; WIDTH]);
    for bad in [
        View {
            channel_count: 0,
            ..m.view
        },
        View {
            channel_index: 1024,
            ..m.view
        },
        View {
            channel_count: 1025,
            ..m.view
        },
        View {
            spectrum_open: true,
            ..m.view
        },
        View {
            entry: Some(NumberEntry {
                value: 10,
                digits: 1,
            }),
            ..m.view
        },
        View {
            entry: Some(NumberEntry {
                value: 0,
                digits: 0,
            }),
            ..m.view
        },
        View {
            entry: Some(NumberEntry {
                value: 1,
                digits: 5,
            }),
            ..m.view
        },
    ] {
        m.view = bad;
        assert_eq!(render_row(0, &mut row, &m), Err(Error::InvalidView));
        assert_eq!(row, [0xdead; WIDTH]);
    }
    m = model(Screen::Menu);
    m.view.menu_item = MenuItem::ResetPeaks;
    assert_eq!(render_row(0, &mut row, &m), Err(Error::InvalidView));
    assert_eq!(row, [0xdead; WIDTH]);
}

#[test]
fn zero_scale_and_unbounded_cursor_are_rejected_before_pixels_change() {
    let e = engine(1, PeakHold::Off);
    let mut row = [0xdead; WIDTH];
    assert_eq!(
        render_row(0, &mut row, &with_spectrum(&e, 0, 0)),
        Err(Error::ZeroScale)
    );
    assert_eq!(
        render_row(0, &mut row, &with_spectrum(&e, usize::MAX, 1)),
        Err(Error::CursorOutOfRange {
            cursor: usize::MAX,
            bins: 1
        })
    );
    assert_eq!(row, [0xdead; WIDTH]);
}

#[test]
fn every_screen_renders_all_rows_deterministically_without_stale_pixels() {
    let e = engine(240, PeakHold::Off);
    let mut numeric = model(Screen::Channel);
    numeric.view.entry = Some(NumberEntry {
        value: 1024,
        digits: 4,
    });
    let mut context = model(Screen::Menu);
    context.view.spectrum_open = true;
    context.view.menu_item = MenuItem::ResetPeaks;
    let mut max_channel = model(Screen::Channel);
    max_channel.channel = Some(ChannelView {
        name: b"0123456789ABCDEFEXCESS",
        rx_10hz: u32::MAX,
        tx_10hz: 0,
    });
    for m in [
        model(Screen::Channel),
        model(Screen::Menu),
        model(Screen::Spectrum),
        numeric,
        context,
        max_channel,
        with_spectrum(&e, 239, u16::MAX),
    ] {
        for y in 0..HEIGHT {
            let mut first = [0xdead; WIDTH];
            let mut second = [0xbeef; WIDTH];
            render_row(y, &mut first, &m).unwrap();
            render_row(y, &mut second, &m).unwrap();
            assert_eq!(first, second);
            assert!(!first.contains(&0xdead));
        }
    }
    assert_eq!(core::mem::size_of::<[u16; WIDTH]>(), 640);
}

#[test]
fn demo_badge_is_explicit_and_does_not_change_body_data() {
    let demo = model(Screen::Channel);
    let mut live = demo;
    live.demo = false;
    let mut left = [0; WIDTH];
    let mut right = [0; WIDTH];
    render_row(10, &mut left, &demo).unwrap();
    render_row(10, &mut right, &live).unwrap();
    assert_ne!(left[260..308], right[260..308]);
    assert_eq!(left[..260], right[..260]);
    for y in 29..HEIGHT {
        render_row(y, &mut left, &demo).unwrap();
        render_row(y, &mut right, &live).unwrap();
        assert_eq!(left, right);
    }
}

#[test]
fn raw_name_preview_has_explicit_ascii_padding_and_length_policy() {
    let mut first = model(Screen::Channel);
    first.channel.as_mut().unwrap().name = b"ab\x81cd\xffignored";
    let mut expected = first;
    expected.channel.as_mut().unwrap().name = b"AB?CD";
    let mut left = [0; WIDTH];
    let mut right = [0; WIDTH];
    for y in 61..75 {
        render_row(y, &mut left, &first).unwrap();
        render_row(y, &mut right, &expected).unwrap();
        assert_eq!(left, right);
    }
    first.channel.as_mut().unwrap().name = b"ABCDEFGHIJKLMNOPQQQ";
    expected.channel.as_mut().unwrap().name = b"ABCDEFGHIJKLMNOP";
    for y in 61..75 {
        render_row(y, &mut left, &first).unwrap();
        render_row(y, &mut right, &expected).unwrap();
        assert_eq!(left, right);
    }
}

#[test]
fn current_old_and_peak_pixels_follow_actual_measurement_tags() {
    let mut e = engine(2, PeakHold::On);
    let first = prepare(&mut e);
    let Some(Action::TuneReceive { ticket, .. }) = sample(&mut e, first, 100) else {
        panic!("second bin")
    };
    assert!(matches!(
        sample(&mut e, ticket, 50),
        Some(Action::SweepComplete { .. })
    ));
    let mut row = [0; WIDTH];
    render_row(100, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::CURRENT);
    assert_eq!(row[200], color::CHART);
    render_row(CHART_TOP, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::PEAK);
    render_row(130, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[200], color::PEAK);
    let Some(Action::TuneReceive { ticket, .. }) = e.handle(0, Event::Poll) else {
        panic!("new sweep")
    };
    render_row(100, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::PREVIOUS);
    sample(&mut e, ticket, 20);
    render_row(170, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::CURRENT);
    assert_eq!(row[200], color::PREVIOUS);
    render_row(CHART_TOP, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::PEAK); // Peak is not silently reset by a new sweep.
}

#[test]
fn missing_samples_are_not_drawn_as_zero_and_scale_clips_only_geometry() {
    let mut e = engine(2, PeakHold::Off);
    let mut row = [0; WIDTH];
    render_row(CHART_BOTTOM, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::GRID);
    let first = prepare(&mut e);
    let Some(Action::TuneReceive { ticket, .. }) = sample(&mut e, first, 0) else {
        panic!("second bin")
    };
    render_row(CHART_BOTTOM, &mut row, &with_spectrum(&e, 1, 100)).unwrap();
    assert_eq!(row[50], color::CURRENT);
    sample(&mut e, ticket, u16::MAX);
    render_row(CHART_TOP, &mut row, &with_spectrum(&e, 0, 100)).unwrap();
    assert_eq!(row[200], color::CURRENT);
    assert_eq!(e.bins()[1].latest.unwrap().raw, u16::MAX);
}

#[test]
fn cursor_matches_selected_bin_for_every_supported_width() {
    let mut row = [0; WIDTH];
    for count in 1..=240 {
        let e = engine(count, PeakHold::Off);
        for cursor in 0..count {
            render_row(CHART_TOP, &mut row, &with_spectrum(&e, cursor, 100)).unwrap();
            let marked: Vec<usize> = row[CHART_X..CHART_X + CHART_WIDTH]
                .iter()
                .enumerate()
                .filter_map(|(index, &pixel)| (pixel == color::CURSOR).then_some(index))
                .collect();
            assert_eq!(marked.len(), 1);
            assert_eq!(
                marked[0] * count / CHART_WIDTH,
                cursor,
                "N={count} i={cursor}"
            );
            if count == 161 && cursor == 49 {
                assert_eq!(marked[0], 74);
            }
        }
    }
}

#[test]
fn rendering_does_not_mutate_the_sweep_engine() {
    let mut e = engine(2, PeakHold::On);
    let ticket = prepare(&mut e);
    sample(&mut e, ticket, 200);
    let before = e.bins().to_vec();
    let state = e.state();
    let sweep = e.sweep();
    for y in 0..HEIGHT {
        render_row(y, &mut [0; WIDTH], &with_spectrum(&e, 0, 100)).unwrap();
    }
    assert_eq!(e.bins(), before);
    assert_eq!(e.state(), state);
    assert_eq!(e.sweep(), sweep);
}

#[derive(Default)]
struct Trace {
    commands: Vec<u8>,
    pixels: Vec<u8>,
    last_command: u8,
    fail: bool,
    calls: usize,
}
#[derive(Clone)]
struct Bus(Rc<RefCell<Trace>>);
impl DisplayBus for Bus {
    type Error = &'static str;
    fn reset(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        let mut trace = self.0.borrow_mut();
        trace.commands.push(command);
        trace.last_command = command;
        Ok(())
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let mut trace = self.0.borrow_mut();
        trace.calls += 1;
        if trace.fail {
            return Err("synthetic bus failure");
        }
        if trace.last_command == 0x2c {
            trace.pixels.extend_from_slice(bytes);
        }
        Ok(())
    }
}
struct NoDelay;
impl Delay for NoDelay {
    fn delay_ms(&mut self, _: u32) {}
}

#[test]
fn stream_wrapper_sends_exact_frame_and_matches_render_row_byte_order() {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut display = Display::new(Bus(trace.clone()), NoDelay);
    display.initialize().unwrap();
    *trace.borrow_mut() = Trace::default();
    let m = model(Screen::Channel);
    render(&mut display, &m).unwrap();
    assert_eq!(display.remaining_pixels(), 0);
    let trace = trace.borrow();
    assert_eq!(trace.commands, [0x2a, 0x2b, 0x2c]);
    assert_eq!(trace.pixels.len(), WIDTH * usize::from(HEIGHT) * 2);
    let mut row = [0; WIDTH];
    for y in 0..HEIGHT {
        render_row(y, &mut row, &m).unwrap();
        for (x, &word) in row.iter().enumerate() {
            let offset = (usize::from(y) * WIDTH + x) * 2;
            assert_eq!(&trace.pixels[offset..offset + 2], &word.to_be_bytes());
        }
    }
}

#[test]
fn stream_wrapper_rejects_invalid_model_before_bus_and_stops_on_bus_error() {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut display = Display::new(Bus(trace.clone()), NoDelay);
    let mut invalid = model(Screen::Channel);
    invalid.view.channel_count = 0;
    assert_eq!(
        render(&mut display, &invalid),
        Err(RenderError::Model(Error::InvalidView))
    );
    assert_eq!(trace.borrow().calls, 0);
    assert!(trace.borrow().commands.is_empty());
    let valid = model(Screen::Channel);
    assert_eq!(
        render(&mut display, &valid),
        Err(RenderError::Display(uv98_display::Error::NotInitialized))
    );
    display.initialize().unwrap();
    *trace.borrow_mut() = Trace {
        fail: true,
        ..Trace::default()
    };
    assert_eq!(
        render(&mut display, &valid),
        Err(RenderError::Display(uv98_display::Error::Bus(
            "synthetic bus failure"
        )))
    );
    assert_eq!(trace.borrow().calls, 1);
    assert!(!display.is_initialized());
}
