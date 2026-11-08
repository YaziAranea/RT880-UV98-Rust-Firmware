extern crate std;

use self::std::{cell::RefCell, rc::Rc, vec::Vec};
use super::*;

const FACTORY: &[u8; 4096] =
    include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");

#[derive(Default)]
struct MemoryState {
    selected: bool,
    address: u32,
    headers: Vec<[u8; 4]>,
    reads: Vec<(u32, usize)>,
    changed_byte: Option<(usize, u8)>,
    fail_read: Option<u32>,
    fail_cleanup: bool,
}
struct Memory(Rc<RefCell<MemoryState>>);
impl Bus for Memory {
    type Error = &'static str;
    fn select(&mut self) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        assert!(!state.selected);
        state.selected = true;
        Ok(())
    }
    fn write(&mut self, header: &[u8]) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        assert!(state.selected);
        assert_eq!(header.len(), 4);
        assert_eq!(header[0], 3, "the only allowed NOR opcode is READ03");
        state.headers.push(header.try_into().unwrap());
        state.address = u32::from_be_bytes([0, header[1], header[2], header[3]]);
        Ok(())
    }
    fn read(&mut self, output: &mut [u8]) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        assert!(state.selected);
        let address = state.address;
        state.reads.push((address, output.len()));
        if state.fail_read == Some(address) {
            output.fill(0);
            return Err("read fault");
        }
        if address == 0 {
            output.copy_from_slice(FACTORY);
            if let Some((offset, byte)) = state.changed_byte {
                output[offset] = byte;
            }
        } else {
            assert!((0x4000..0x10000).contains(&address));
            output.fill(0xff);
            output[2] = 1;
            output[6..10].copy_from_slice(&43_180_000u32.to_le_bytes());
            output[32..36].copy_from_slice(b"TEST");
        }
        Ok(())
    }
    fn deselect(&mut self) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        state.selected = false;
        if state.fail_cleanup {
            Err("cleanup fault")
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct SinkState {
    commands: Vec<u8>,
    pixel_windows: usize,
    fail_window: Option<usize>,
    fail_first_command: bool,
    data_bytes: usize,
}
struct Sink(Rc<RefCell<SinkState>>);
impl DisplayBus for Sink {
    type Error = &'static str;
    fn reset(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        if state.fail_first_command {
            return Err("LCD command fault");
        }
        state.commands.push(command);
        if command == 0x2c {
            state.pixel_windows += 1;
            if state.fail_window == Some(state.pixel_windows) {
                return Err("LCD frame fault");
            }
        }
        Ok(())
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.borrow_mut().data_bytes += bytes.len();
        Ok(())
    }
}
struct NoDelay;
impl Delay for NoDelay {
    fn delay_ms(&mut self, _: u32) {}
}
type TestApp = App<Memory, Sink, NoDelay>;
fn app() -> (TestApp, Rc<RefCell<MemoryState>>, Rc<RefCell<SinkState>>) {
    let memory = Rc::new(RefCell::new(MemoryState::default()));
    let sink = Rc::new(RefCell::new(SinkState::default()));
    (
        App::new(
            Memory(memory.clone()),
            Sink(sink.clone()),
            NoDelay,
            Timing::new(20, 500).unwrap(),
            PRESERVED_CALIBRATION_SHA256,
        ),
        memory,
        sink,
    )
}
fn sample(matrix: u16, ptt: bool) -> InputSample {
    InputSample::Stable {
        matrix,
        ptt_pressed: ptt,
    }
}
fn arm(app: &mut TestApp, time: u32) {
    app.sample(time, sample(0, false)).unwrap();
    app.sample(time.wrapping_add(20), sample(0, false)).unwrap();
    assert_eq!(app.status().input_state, InputState::Ready);
}
fn press(app: &mut TestApp, time: u32, matrix: u16, ptt: bool) -> Update {
    assert_eq!(app.sample(time, sample(matrix, ptt)).unwrap().pressed, None);
    app.sample(time.wrapping_add(20), sample(matrix, ptt))
        .unwrap()
}

#[test]
fn constructor_is_inert_start_reads_original_once_and_matches_its_full_sha() {
    let (mut app, memory, sink) = app();
    assert!(memory.borrow().headers.is_empty());
    assert!(sink.borrow().commands.is_empty());
    app.start(0).unwrap();
    assert_eq!(app.status().state, State::Ready);
    assert_eq!(
        app.status().calibration_sha256,
        Some(PRESERVED_CALIBRATION_SHA256)
    );
    assert_eq!(app.status().calibration_matches_backup, Some(true));
    assert_eq!(app.snapshot().unwrap().calibration.raw(), FACTORY);
    assert_eq!(memory.borrow().reads, [(0, 4096), (0x4000, 48)]);
    assert!(!memory.borrow().selected);
    assert_eq!(sink.borrow().pixel_windows, 2); // initial clear and first frame
    assert!(matches!(app.start(1), Err(Error::State)));
    assert_eq!(memory.borrow().reads.len(), 2);
}

#[test]
fn startup_observer_identifies_the_operation_that_failed() {
    for failure in 0..4 {
        let (mut app, memory, sink) = app();
        match failure {
            0 => sink.borrow_mut().fail_first_command = true,
            1 => memory.borrow_mut().fail_read = Some(0),
            2 => memory.borrow_mut().fail_read = Some(0x4000),
            _ => sink.borrow_mut().fail_window = Some(2),
        }
        let mut stages = Vec::new();
        assert!(app.start_observed(0, |stage| stages.push(stage)).is_err());
        assert_eq!(stages.len(), failure + 1);
        assert_eq!(
            *stages.last().unwrap(),
            [
                StartStage::DisplayBegin,
                StartStage::CalibrationBegin,
                StartStage::ChannelBegin,
                StartStage::RenderBegin
            ][failure]
        );
        assert!(!stages.contains(&StartStage::Complete));
    }
    let (mut app, _, _) = app();
    let mut stages = Vec::new();
    app.start_observed(0, |stage| stages.push(stage)).unwrap();
    assert_eq!(stages.last(), Some(&StartStage::Complete));
    let count = stages.len();
    assert!(app.start_observed(0, |stage| stages.push(stage)).is_err());
    assert_eq!(stages.len(), count);
}

#[test]
fn mismatch_in_uninterpreted_tail_is_visible_without_replacing_bytes() {
    let (mut app, memory, _) = app();
    let changed = FACTORY[4095] ^ 1;
    memory.borrow_mut().changed_byte = Some((4095, changed));
    app.start(0).unwrap();
    assert_eq!(app.status().calibration_matches_backup, Some(false));
    assert_eq!(app.snapshot().unwrap().calibration.raw()[4095], changed);
    assert_eq!(memory.borrow().headers, [[3, 0, 0, 0], [3, 0, 0x40, 0]]);
}

#[test]
fn invalid_marker_and_cleanup_failures_never_publish_calibration_or_channel() {
    for cleanup in [false, true] {
        let (mut app, memory, sink) = app();
        if cleanup {
            memory.borrow_mut().fail_cleanup = true;
        } else {
            memory.borrow_mut().changed_byte = Some((0, 0));
        }
        assert!(matches!(
            app.start(0),
            Err(Error::Read {
                fault_display_error: None,
                ..
            })
        ));
        assert_eq!(app.status().state, State::Failed);
        assert_eq!(app.status().calibration_sha256, None);
        assert!(app.snapshot().is_none());
        assert_eq!(memory.borrow().reads, [(0, 4096)]);
        assert_eq!(sink.borrow().pixel_windows, 2); // fault frame, no success frame
        assert!(matches!(app.sample(0, sample(0, false)), Err(Error::State)));
        assert_eq!(memory.borrow().reads.len(), 1);
    }
}

#[test]
fn display_init_failure_prevents_all_nor_access() {
    let (mut app, memory, sink) = app();
    sink.borrow_mut().fail_first_command = true;
    assert!(matches!(app.start(0), Err(Error::Display(_))));
    assert!(memory.borrow().headers.is_empty());
    assert_eq!(app.status().state, State::Failed);
}

#[test]
fn original_read_error_is_retained_if_drawing_the_fault_also_fails() {
    let (mut app, memory, sink) = app();
    memory.borrow_mut().fail_read = Some(0);
    sink.borrow_mut().fail_window = Some(2);
    assert!(matches!(
        app.start(0),
        Err(Error::Read {
            fault_display_error: Some(_),
            ..
        })
    ));
    assert!(app.snapshot().is_none());
    assert!(!memory.borrow().selected);
}

#[test]
fn navigation_read_failure_is_terminal_and_cannot_leave_a_current_snapshot() {
    let (mut app, memory, sink) = app();
    app.start(0).unwrap();
    arm(&mut app, 0);
    app.sample(21, sample(0x4000, false)).unwrap();
    memory.borrow_mut().fail_read = Some(0x4030);
    assert!(matches!(
        app.sample(41, sample(0x4000, false)),
        Err(Error::Read { .. })
    ));
    assert_eq!(app.status().state, State::Failed);
    assert!(app.snapshot().is_none());
    let windows = sink.borrow().pixel_windows;
    assert!(matches!(
        app.sample(42, sample(0, false)),
        Err(Error::State)
    ));
    assert_eq!(sink.borrow().pixel_windows, windows);
    assert_eq!(
        memory.borrow().reads,
        [(0, 4096), (0x4000, 48), (0x4030, 48)]
    );
}

#[test]
fn boot_held_key_is_ignored_until_clean_release_and_new_debounced_press() {
    let (mut app, memory, _) = app();
    app.start(0).unwrap();
    for time in [0, 20, 1000] {
        assert_eq!(
            app.sample(time, sample(0x4000, false)).unwrap().pressed,
            None
        );
    }
    arm(&mut app, 1001);
    let update = press(&mut app, 1022, 0x4000, false);
    assert_eq!(update.pressed, Some(Key::Down));
    assert!(update.channel_changed);
    assert_eq!(app.snapshot().unwrap().index(), 1);
    let reads = memory.borrow().reads.len();
    for time in [1043, 1500, 2500] {
        assert_eq!(
            app.sample(time, sample(0x4000, false)).unwrap().pressed,
            None
        );
    }
    assert_eq!(memory.borrow().reads.len(), reads);
}

#[test]
fn unchanged_samples_do_not_redraw_or_read_storage() {
    let (mut app, memory, sink) = app();
    app.start(0).unwrap();
    arm(&mut app, 0);
    let commands = sink.borrow().commands.len();
    for time in [21, 40, 1000, 10000] {
        assert!(!app.sample(time, sample(0, false)).unwrap().rendered);
    }
    assert_eq!(sink.borrow().commands.len(), commands);
    assert_eq!(memory.borrow().reads.len(), 2);
}

#[test]
fn ambiguous_or_failed_scan_cancels_pending_key_and_requires_release() {
    for invalid in [
        sample(0x6000, false),
        sample(0x1111, false),
        sample(0x4000, true),
        InputSample::Unstable,
    ] {
        let (mut app, memory, _) = app();
        app.start(0).unwrap();
        arm(&mut app, 0);
        app.sample(21, sample(0x4000, false)).unwrap();
        assert_eq!(app.sample(30, invalid).unwrap().pressed, None);
        assert_eq!(app.status().input_state, InputState::Unstable);
        assert_eq!(
            app.sample(100, sample(0x4000, false)).unwrap().pressed,
            None
        );
        assert_eq!(memory.borrow().reads.len(), 2);
        arm(&mut app, 101);
        assert_eq!(press(&mut app, 122, 0x4000, false).pressed, Some(Key::Down));
    }
}

#[test]
fn direct_key_transition_is_not_a_new_press_without_release() {
    let (mut app, memory, _) = app();
    app.start(0).unwrap();
    arm(&mut app, 0);
    press(&mut app, 21, 0x0001, false);
    app.sample(42, sample(0x4000, false)).unwrap();
    app.sample(100, sample(0x4000, false)).unwrap();
    assert_eq!(app.status().presses, 1);
    assert_eq!(memory.borrow().reads.len(), 2);
}

#[test]
fn ptt_is_visible_and_never_navigates_or_reads_more_nor() {
    let (mut app, memory, _) = app();
    app.start(0).unwrap();
    arm(&mut app, 0);
    let update = press(&mut app, 21, 0, true);
    assert_eq!(update.pressed, Some(Key::Ptt));
    assert!(!update.channel_changed);
    assert_eq!(app.status().raw_sample, Some((0, true)));
    assert_eq!(app.snapshot().unwrap().index(), 0);
    assert_eq!(memory.borrow().reads.len(), 2);
}

#[test]
fn menu_calibration_band_navigation_and_key_page_do_not_read_more_nor() {
    let (mut app, memory, _) = app();
    app.start(0).unwrap();
    arm(&mut app, 0);
    press(&mut app, 21, 0x1000, false);
    assert_eq!(app.status().page, Page::Calibration);
    arm(&mut app, 42);
    press(&mut app, 63, 0x4000, false);
    assert_eq!(app.status().band, 1);
    arm(&mut app, 84);
    press(&mut app, 105, 0x1000, false);
    assert_eq!(app.status().page, Page::Keys);
    arm(&mut app, 126);
    press(&mut app, 147, 0x4000, false);
    assert_eq!(app.status().band, 1);
    assert_eq!(app.snapshot().unwrap().index(), 0);
    assert_eq!(memory.borrow().reads.len(), 2);
}

#[test]
fn wrapping_clock_works_but_backward_or_half_range_jump_is_terminal() {
    let (mut app, memory, _) = app();
    app.start(0).unwrap();
    arm(&mut app, u32::MAX - 10);
    assert_eq!(press(&mut app, 10, 0, true).pressed, Some(Key::Ptt));
    assert!(matches!(
        app.sample(29, sample(0, false)),
        Err(Error::Clock { .. })
    ));
    assert!(app.snapshot().is_none());
    assert_eq!(memory.borrow().reads.len(), 2);
    assert_eq!(app.status().state, State::Failed);
}

#[test]
fn display_redraw_error_closes_app_and_no_followup_io_occurs() {
    let (mut app, memory, sink) = app();
    app.start(0).unwrap();
    sink.borrow_mut().fail_window = Some(3);
    assert!(matches!(
        app.sample(0, sample(0, false)),
        Err(Error::Display(_))
    ));
    assert!(app.snapshot().is_none());
    assert!(matches!(app.sample(1, sample(0, false)), Err(Error::State)));
    assert_eq!(memory.borrow().reads.len(), 2);
    assert_eq!(sink.borrow().pixel_windows, 3);
}
