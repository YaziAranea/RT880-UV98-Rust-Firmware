use std::{cell::RefCell, rc::Rc};
use uv98_display::{Delay, Display, DisplayBus, Error, HEIGHT, PIXEL_BUFFER_BYTES, WIDTH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Reset(bool),
    Delay(u32),
    Command(u8),
    Data(u8),
}

#[derive(Default)]
struct State {
    events: Vec<Event>,
    data_batches: Vec<usize>,
    attempts: usize,
    fail_at: Option<usize>,
    pixel_batches: Vec<usize>,
}

#[derive(Clone)]
struct Bus(Rc<RefCell<State>>);
struct Timer(Rc<RefCell<State>>);

impl Bus {
    fn attempt(&self) -> Result<(), &'static str> {
        let mut state = self.0.borrow_mut();
        let index = state.attempts;
        state.attempts += 1;
        if state.fail_at == Some(index) {
            Err("injected bus failure")
        } else {
            Ok(())
        }
    }
}

impl DisplayBus for Bus {
    type Error = &'static str;
    fn reset(&mut self, high: bool) -> Result<(), Self::Error> {
        self.attempt()?;
        self.0.borrow_mut().events.push(Event::Reset(high));
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.attempt()?;
        self.0.borrow_mut().events.push(Event::Command(command));
        Ok(())
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.attempt()?;
        let mut state = self.0.borrow_mut();
        state.data_batches.push(bytes.len());
        state.events.extend(bytes.iter().copied().map(Event::Data));
        Ok(())
    }
}

impl Delay for Timer {
    fn delay_ms(&mut self, ms: u32) {
        self.0.borrow_mut().events.push(Event::Delay(ms));
    }
}

fn new_display() -> (Display<Bus, Timer>, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State::default()));
    (
        Display::new(Bus(state.clone()), Timer(state.clone())),
        state,
    )
}

fn new_portrait_display() -> (Display<Bus, Timer>, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State::default()));
    (
        Display::new_portrait(Bus(state.clone()), Timer(state.clone())),
        state,
    )
}

fn ready() -> (Display<Bus, Timer>, Rc<RefCell<State>>) {
    let (mut display, state) = new_display();
    display.initialize().unwrap();
    *state.borrow_mut() = State::default();
    (display, state)
}

fn events_fixture(text: &str) -> Vec<Event> {
    text.lines()
        .map(|line| {
            let (kind, value) = line.split_once(' ').unwrap();
            match kind {
                "reset" => Event::Reset(value == "1"),
                "delay_ms" => Event::Delay(value.parse().unwrap()),
                "command" => Event::Command(u8::from_str_radix(value, 16).unwrap()),
                "data" => Event::Data(u8::from_str_radix(value, 16).unwrap()),
                _ => panic!("unknown oracle event {kind}"),
            }
        })
        .collect()
}

#[test]
fn construction_is_inert_and_drawing_requires_initialization() {
    let (mut display, state) = new_display();
    assert!(!display.is_initialized());
    assert_eq!(display.fill_rect(0, 0, 1, 1, 0), Err(Error::NotInitialized));
    assert_eq!(display.write_pixels(&[0]), Err(Error::NotInitialized));
    assert_eq!(display.finish_pixels(), Err(Error::NotInitialized));
    assert_eq!(state.borrow().attempts, 0);
    assert!(state.borrow().events.is_empty());
    assert_eq!(
        (display.width(), display.height(), display.madctl()),
        (320, 240, 0xA8)
    );
}

#[test]
fn portrait_initialization_changes_only_orientation_and_full_window_geometry() {
    let (mut display, state) = new_portrait_display();
    assert_eq!(
        (display.width(), display.height(), display.madctl()),
        (240, 320, 0x08)
    );
    assert_eq!(state.borrow().attempts, 0);
    display.initialize().unwrap();
    let mut prefix = events_fixture(include_str!("fixtures/stock-control-prefix.txt"));
    let madctl = prefix
        .iter()
        .position(|event| *event == Event::Command(0x36))
        .unwrap();
    assert_eq!(prefix[madctl + 1], Event::Data(0xA8));
    prefix[madctl + 1] = Event::Data(0x08);
    let state = state.borrow();
    assert_eq!(&state.events[..prefix.len()], prefix);
    let window = [
        Event::Command(0x2A),
        Event::Data(0),
        Event::Data(0),
        Event::Data(0),
        Event::Data(239),
        Event::Command(0x2B),
        Event::Data(0),
        Event::Data(0),
        Event::Data(1),
        Event::Data(63),
        Event::Command(0x2C),
    ];
    assert_eq!(
        &state.events[prefix.len()..prefix.len() + window.len()],
        window
    );
    let pixels_begin = prefix.len() + window.len();
    let pixel_bytes = 240 * 320 * 2;
    assert!(state.events[pixels_begin..pixels_begin + pixel_bytes]
        .iter()
        .all(|event| *event == Event::Data(0)));
    assert_eq!(
        state.events[pixels_begin + pixel_bytes..],
        [Event::Command(0x29)]
    );
    assert!(state
        .data_batches
        .iter()
        .all(|size| *size <= PIXEL_BUFFER_BYTES));
}

#[test]
fn portrait_window_bounds_use_240_columns_and_320_rows_without_transposing_pixels() {
    let (mut display, state) = new_portrait_display();
    display.initialize().unwrap();
    *state.borrow_mut() = State::default();
    for (x, y, w, h) in [
        (240, 0, 1, 1),
        (0, 320, 1, 1),
        (239, 319, 2, 1),
        (239, 319, 1, 2),
        (0, 0, 320, 240),
    ] {
        assert_eq!(
            display.fill_rect(x, y, w, h, 0),
            Err(Error::InvalidRectangle)
        );
    }
    assert_eq!(state.borrow().attempts, 0);
    display.fill_rect(239, 319, 1, 1, 0x1234).unwrap();
    assert_eq!(
        state.borrow().events,
        [
            Event::Command(0x2A),
            Event::Data(0),
            Event::Data(239),
            Event::Data(0),
            Event::Data(239),
            Event::Command(0x2B),
            Event::Data(1),
            Event::Data(63),
            Event::Data(1),
            Event::Data(63),
            Event::Command(0x2C),
            Event::Data(0x12),
            Event::Data(0x34),
        ]
    );
    display.begin_window(0, 0, 240, 320).unwrap();
    assert_eq!(display.remaining_pixels(), 76_800);
}

#[test]
fn control_prefix_matches_stock_machine_oracle_and_clear_is_canonical() {
    let (mut display, state) = new_display();
    display.initialize().unwrap();
    assert!(display.is_initialized());
    let prefix = events_fixture(include_str!("fixtures/stock-control-prefix.txt"));
    let window = events_fixture(include_str!("fixtures/stock-full-window.txt"));
    let state = state.borrow();
    assert_eq!(&state.events[..prefix.len()], prefix);
    let start = prefix.len();
    assert_eq!(&state.events[start..start + window.len()], window);
    let start = start + window.len();
    assert_eq!(state.events[start], Event::Command(0x2C));
    let pixels = usize::from(WIDTH) * usize::from(HEIGHT);
    assert!(state.events[start + 1..start + 1 + pixels * 2]
        .iter()
        .all(|e| *e == Event::Data(0)));
    assert_eq!(
        state.events[start + 1 + pixels * 2..],
        [Event::Command(0x29)]
    );
    assert!(state
        .data_batches
        .iter()
        .all(|size| *size <= PIXEL_BUFFER_BYTES));
}

#[test]
fn words_match_stock_machine_oracle_without_color_reinterpretation() {
    let vectors: Vec<_> = include_str!("fixtures/stock-pixel-words.txt")
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (
                u16::from_str_radix(fields[0], 16).unwrap(),
                [
                    u8::from_str_radix(fields[1], 16).unwrap(),
                    u8::from_str_radix(fields[2], 16).unwrap(),
                ],
            )
        })
        .collect();
    let (mut display, state) = ready();
    display.begin_window(0, 0, vectors.len() as u16, 1).unwrap();
    state.borrow_mut().events.clear();
    display
        .write_pixels(&vectors.iter().map(|v| v.0).collect::<Vec<_>>())
        .unwrap();
    display.finish_pixels().unwrap();
    assert_eq!(
        state.borrow().events,
        vectors
            .iter()
            .flat_map(|v| v.1)
            .map(Event::Data)
            .collect::<Vec<_>>()
    );
}

#[test]
fn bottom_right_single_pixel_window_uses_inclusive_ends() {
    let (mut display, state) = ready();
    display.fill_rect(319, 239, 1, 1, 0x1234).unwrap();
    assert_eq!(
        state.borrow().events,
        vec![
            Event::Command(0x2A),
            Event::Data(1),
            Event::Data(0x3F),
            Event::Data(1),
            Event::Data(0x3F),
            Event::Command(0x2B),
            Event::Data(0),
            Event::Data(0xEF),
            Event::Data(0),
            Event::Data(0xEF),
            Event::Command(0x2C),
            Event::Data(0x12),
            Event::Data(0x34),
        ]
    );
    assert_eq!(display.remaining_pixels(), 0);
}

#[test]
fn invalid_bounds_never_touch_bus_or_invalidate_driver() {
    let (mut display, state) = ready();
    for (x, y, w, h) in [
        (0, 0, 0, 1),
        (0, 0, 1, 0),
        (320, 0, 1, 1),
        (0, 240, 1, 1),
        (319, 239, 2, 1),
        (319, 239, 1, 2),
        (65535, 0, 1, 1),
        (1, 1, 65535, 1),
        (0, 65535, 1, 65535),
        (0, 0, 321, 240),
    ] {
        assert_eq!(
            display.fill_rect(x, y, w, h, 0),
            Err(Error::InvalidRectangle)
        );
    }
    assert!(display.is_initialized());
    assert_eq!(state.borrow().attempts, 0);
}

#[test]
fn streams_require_exact_count_and_preserve_window_after_validation_failure() {
    let (mut display, state) = ready();
    assert_eq!(display.write_pixels(&[]), Err(Error::NoWindow));
    assert_eq!(display.finish_pixels(), Err(Error::NoWindow));
    display.begin_window(7, 9, 3, 1).unwrap();
    let attempts = state.borrow().attempts;
    assert_eq!(
        display.begin_window(0, 0, 1, 1),
        Err(Error::WindowInProgress)
    );
    assert_eq!(
        display.write_pixels(&[0, 0, 0, 0]),
        Err(Error::TooManyPixels)
    );
    assert_eq!(
        display.finish_pixels(),
        Err(Error::IncompletePixels { remaining: 3 })
    );
    display.write_pixels(&[]).unwrap();
    assert_eq!(state.borrow().attempts, attempts);
    display.write_pixels(&[0x1234]).unwrap();
    assert_eq!(display.remaining_pixels(), 2);
    assert_eq!(
        display.finish_pixels(),
        Err(Error::IncompletePixels { remaining: 2 })
    );
    display.write_pixels(&[1, 2]).unwrap();
    assert_eq!(display.write_pixels(&[3]), Err(Error::TooManyPixels));
    display.finish_pixels().unwrap();
    assert_eq!(display.finish_pixels(), Err(Error::NoWindow));
    display.fill_rect(0, 0, 1, 1, 0).unwrap();
}

#[test]
fn pixel_streams_and_repeated_fills_have_bounded_batches() {
    let (mut display, state) = ready();
    let words: Vec<u16> = (0..73).map(|x| 0xAB00 | x).collect();
    display.begin_window(0, 0, 73, 1).unwrap();
    *state.borrow_mut() = State::default();
    display.write_pixels(&words).unwrap();
    display.finish_pixels().unwrap();
    assert_eq!(state.borrow().data_batches, [64, 64, 18]);
    let expected: Vec<_> = words
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .map(Event::Data)
        .collect();
    assert_eq!(state.borrow().events, expected);
    *state.borrow_mut() = State::default();
    display.fill_rect(0, 0, 73, 1, 0xABCD).unwrap();
    assert_eq!(state.borrow().data_batches, [4, 4, 64, 64, 18]);
    assert_eq!(
        state.borrow().events[11..],
        [Event::Data(0xAB), Event::Data(0xCD)].repeat(73)
    );
}

#[test]
fn initialization_bus_failures_stop_immediately_and_require_retry() {
    let (mut baseline, state) = new_display();
    baseline.initialize().unwrap();
    let operation_count = state.borrow().attempts;
    // Every reset/register/window operation plus representative first/middle/
    // last pixel chunks and final DisplayOn. No register path is skipped.
    let positions: Vec<_> = (0..42)
        .chain([
            operation_count / 2,
            operation_count - 2,
            operation_count - 1,
        ])
        .collect();
    for position in positions {
        let (mut display, state) = new_display();
        state.borrow_mut().fail_at = Some(position);
        assert_eq!(
            display.initialize(),
            Err(Error::Bus("injected bus failure"))
        );
        assert!(!display.is_initialized());
        assert_eq!(state.borrow().attempts, position + 1);
        assert_eq!(display.fill_rect(0, 0, 1, 1, 0), Err(Error::NotInitialized));
        assert_eq!(state.borrow().attempts, position + 1);
    }
}

#[test]
fn drawing_failure_invalidates_uncertain_progress_then_reset_recovers() {
    for position in 0..8 {
        let (mut display, state) = ready();
        state.borrow_mut().fail_at = Some(position);
        assert_eq!(
            display.fill_rect(0, 0, 73, 1, 0),
            Err(Error::Bus("injected bus failure"))
        );
        assert_eq!(state.borrow().attempts, position + 1);
        assert!(!display.is_initialized());
        assert_eq!(display.remaining_pixels(), 0);
        assert_eq!(display.write_pixels(&[0]), Err(Error::NotInitialized));
        *state.borrow_mut() = State::default();
        display.initialize().unwrap();
        assert!(display.is_initialized());
        display.fill_rect(0, 0, 1, 1, 0).unwrap();
    }
}

#[test]
fn streamed_transfer_failure_invalidates_even_if_previous_chunks_succeeded() {
    let (mut display, state) = ready();
    display.begin_window(0, 0, 73, 1).unwrap();
    *state.borrow_mut() = State {
        fail_at: Some(1),
        ..State::default()
    };
    assert_eq!(
        display.write_pixels(&[0xFFFF; 73]),
        Err(Error::Bus("injected bus failure"))
    );
    assert_eq!(state.borrow().attempts, 2);
    assert_eq!(state.borrow().events.len(), 64);
    assert!(!display.is_initialized());
    assert_eq!(display.finish_pixels(), Err(Error::NotInitialized));
}

#[test]
fn explicit_reinitialization_abandons_open_stream() {
    let (mut display, _) = ready();
    display.begin_window(0, 0, 3, 1).unwrap();
    display.write_pixels(&[1]).unwrap();
    display.initialize().unwrap();
    assert_eq!(display.remaining_pixels(), 0);
    assert_eq!(display.finish_pixels(), Err(Error::NoWindow));
    display.fill_rect(0, 0, 1, 1, 0).unwrap();
}

struct PixelBus(Bus);
impl DisplayBus for PixelBus {
    type Error = &'static str;
    fn reset(&mut self, high: bool) -> Result<(), Self::Error> {
        self.0.reset(high)
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.0.command(command)
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.data(bytes)
    }
    fn pixels_rgb565(&mut self, pixels: &[u16]) -> Result<(), Self::Error> {
        self.0.attempt()?;
        let mut state = self.0 .0.borrow_mut();
        state.pixel_batches.push(pixels.len());
        state.events.extend(
            pixels
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .map(Event::Data),
        );
        Ok(())
    }
}

#[test]
fn optimized_pixel_transport_receives_existing_row_once_after_all_bounds_checks() {
    let state = Rc::new(RefCell::new(State::default()));
    let mut display = Display::new_portrait(PixelBus(Bus(state.clone())), Timer(state.clone()));
    display.initialize().unwrap();
    display.begin_window(0, 31, 240, 1).unwrap();
    *state.borrow_mut() = State::default();
    assert_eq!(display.write_pixels(&[0; 241]), Err(Error::TooManyPixels));
    display.write_pixels(&[]).unwrap();
    assert_eq!(state.borrow().attempts, 0);
    let pixels: Vec<u16> = (0..240).map(|i| (i as u16).wrapping_mul(0x1279)).collect();
    display.write_pixels(&pixels).unwrap();
    assert_eq!(display.remaining_pixels(), 0);
    display.finish_pixels().unwrap();
    let state = state.borrow();
    assert_eq!(state.pixel_batches, [240]);
    assert!(
        state.data_batches.is_empty(),
        "direct pixels must not be restaged as 64-byte data"
    );
    assert_eq!(
        state.events,
        pixels
            .iter()
            .flat_map(|word| word.to_be_bytes())
            .map(Event::Data)
            .collect::<Vec<_>>()
    );
}

#[test]
fn optimized_pixel_failure_invalidates_all_uncertain_window_progress() {
    let state = Rc::new(RefCell::new(State::default()));
    let mut display = Display::new_portrait(PixelBus(Bus(state.clone())), Timer(state.clone()));
    display.initialize().unwrap();
    display.begin_window(0, 0, 240, 2).unwrap();
    display.write_pixels(&[0x1234; 240]).unwrap();
    assert_eq!(display.remaining_pixels(), 240);
    *state.borrow_mut() = State {
        fail_at: Some(0),
        ..State::default()
    };
    assert_eq!(
        display.write_pixels(&[0xabcd; 240]),
        Err(Error::Bus("injected bus failure"))
    );
    assert!(!display.is_initialized());
    assert_eq!(display.remaining_pixels(), 0);
    assert_eq!(display.finish_pixels(), Err(Error::NotInitialized));
    assert_eq!(display.write_pixels(&[0]), Err(Error::NotInitialized));
    assert_eq!(state.borrow().attempts, 1);
}
