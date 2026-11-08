use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use uv98_calibration::Calibration;
use uv98_receive::{
    executor::{Executor, Io, Status},
    transport::{BoardControl, Pair, PairError, ProfileMismatch},
    Chip, FrequencyPlan, Plan, Width,
};
use uv98_rf_bus::{Bus, Delay, Direction, Error, Phase, Pins, Profile};

const CAL: &[u8] = include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");

#[derive(Default)]
struct PinState {
    calls: usize,
    failures: Vec<usize>,
    active: bool,
    input: bool,
    bits: Vec<bool>,
    frames: Vec<Vec<u8>>,
    words: VecDeque<u16>,
    current_word: u16,
    samples: usize,
}
type PinLog = Rc<RefCell<PinState>>;
struct MockPins(PinLog);
impl MockPins {
    fn call(&mut self) -> Result<(), usize> {
        let mut state = self.0.borrow_mut();
        let index = state.calls;
        state.calls += 1;
        if state.failures.contains(&index) {
            Err(index)
        } else {
            Ok(())
        }
    }
}
impl Pins for MockPins {
    type Error = usize;
    fn chip_select(&mut self, high: bool) -> Result<(), usize> {
        self.call()?;
        let mut s = self.0.borrow_mut();
        if high && s.active {
            assert_eq!(s.bits.len() % 8, 0);
            let frame = s
                .bits
                .chunks(8)
                .map(|bits| {
                    bits.iter()
                        .fold(0u8, |word, &bit| (word << 1) | u8::from(bit))
                })
                .collect();
            s.frames.push(frame);
        }
        s.active = !high;
        if !high {
            s.bits.clear();
            s.samples = 0;
        }
        Ok(())
    }
    fn clock(&mut self, _: bool) -> Result<(), usize> {
        self.call()
    }
    fn data_level(&mut self, high: bool) -> Result<(), usize> {
        self.call()?;
        let mut s = self.0.borrow_mut();
        assert!(!s.input);
        assert!(s.active);
        s.bits.push(high);
        Ok(())
    }
    fn data_direction(&mut self, direction: Direction) -> Result<(), usize> {
        self.call()?;
        let mut s = self.0.borrow_mut();
        s.input = direction == Direction::Input;
        if s.input && s.active && s.bits.len() == 8 {
            s.current_word = s.words.pop_front().expect("unexpected RF read");
        }
        Ok(())
    }
    fn read_data(&mut self) -> Result<bool, usize> {
        self.call()?;
        let mut s = self.0.borrow_mut();
        assert!(s.input);
        assert!(s.samples < 16);
        let bit = s.current_word & (0x8000 >> s.samples) != 0;
        s.samples += 1;
        Ok(bit)
    }
}
struct NoDelay;
impl Delay for NoDelay {
    fn wait(&mut self, _: Phase) {}
}
fn bus(profile: Profile, words: &[u16]) -> (Bus<MockPins, NoDelay>, PinLog) {
    let state = Rc::new(RefCell::new(PinState {
        words: words.iter().copied().collect(),
        ..PinState::default()
    }));
    (Bus::new(MockPins(state.clone()), NoDelay, profile), state)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoardEvent {
    Guard,
    Pf0(bool),
}
#[derive(Default)]
struct BoardState {
    events: Vec<BoardEvent>,
    error: Option<usize>,
}
struct Board(Rc<RefCell<BoardState>>);
impl BoardControl for Board {
    type Error = usize;
    fn check_receive_preconditions(&mut self) -> Result<(), usize> {
        self.0.borrow_mut().events.push(BoardEvent::Guard);
        self.0.borrow().error.map_or(Ok(()), Err)
    }
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), usize> {
        self.0.borrow_mut().events.push(BoardEvent::Pf0(high));
        self.0.borrow().error.map_or(Ok(()), Err)
    }
}
fn board() -> (Board, Rc<RefCell<BoardState>>) {
    let state = Rc::new(RefCell::new(BoardState::default()));
    (Board(state.clone()), state)
}
fn bytes(register: u8, word: u16) -> Vec<u8> {
    vec![register, (word >> 8) as u8, word as u8]
}

#[test]
fn constructor_profile_checks_are_inert_even_for_previously_used_rejected_buses() {
    for (pa, pb) in [
        (Profile::A, Profile::A),
        (Profile::B, Profile::B),
        (Profile::B, Profile::A),
    ] {
        let (mut a, a_log) = bus(pa, &[]);
        let (mut b, b_log) = bus(pb, &[]);
        let (mut control, c_log) = board();
        a.write(0x38, 0x1234).unwrap();
        let a_before = a_log.borrow().calls;
        let error = Pair::new(&mut a, &mut b, &mut control)
            .err()
            .expect("profiles must be refused");
        assert_eq!(
            error,
            ProfileMismatch {
                primary: pa,
                secondary: pb
            }
        );
        assert_eq!(a_log.borrow().calls, a_before);
        assert_eq!(b_log.borrow().calls, 0);
        assert!(c_log.borrow().events.is_empty());
    }
    let (mut a, a_log) = bus(Profile::A, &[]);
    let (mut b, b_log) = bus(Profile::B, &[]);
    let (mut control, c_log) = board();
    let _pair = Pair::new(&mut a, &mut b, &mut control).unwrap();
    assert_eq!(a_log.borrow().calls + b_log.borrow().calls, 0);
    assert!(c_log.borrow().events.is_empty());
}

#[test]
fn adapter_dispatches_reads_and_msb_first_words_to_the_selected_real_bus_engine() {
    let (mut a, a_log) = bus(Profile::A, &[0x1357]);
    let (mut b, b_log) = bus(Profile::B, &[0x2468]);
    let (mut control, _) = board();
    let mut pair = Pair::new(&mut a, &mut b, &mut control).unwrap();
    assert_eq!(pair.read(Chip::A, 0x33).unwrap(), 0x1357);
    assert_eq!(pair.read(Chip::B, 0x01).unwrap(), 0x2468);
    pair.write(Chip::A, 0x38, 0xa55a).unwrap();
    pair.write(Chip::B, 0x39, 0x5aa5).unwrap();
    assert_eq!(a_log.borrow().frames, [vec![0xb3], vec![0x38, 0xa5, 0x5a]]);
    assert_eq!(b_log.borrow().frames, [vec![0x81], vec![0x39, 0x5a, 0xa5]]);
    assert!(!a_log.borrow().input);
    assert!(!b_log.borrow().input); // B write explicitly restores output.
}

#[test]
fn executor_primary_plan_reaches_real_bus_frames_and_relay_request_in_order() {
    let cal = Calibration::parse(CAL).unwrap();
    let plan = Plan::primary(&cal, 14_550_000, Width::Narrow, 3).unwrap();
    let (mut a, a_log) = bus(Profile::A, &[0xa000, 0x50b0]);
    let (mut b, b_log) = bus(Profile::B, &[]);
    let (mut control, c_log) = board();
    let pair = Pair::new(&mut a, &mut b, &mut control).unwrap();
    let mut executor = Executor::new(pair);
    executor.start(plan).unwrap();
    for _ in 0..8 {
        executor.step().unwrap();
    }
    assert_eq!(executor.status(), Status::Configured);
    let mut expected = vec![vec![0xb3]];
    for operation in &plan.operations()[..5] {
        let uv98_receive::Operation::Write {
            register, value, ..
        } = *operation
        else {
            panic!()
        };
        expected.push(bytes(register, value));
    }
    expected.extend([vec![0xb3], bytes(0x33, 0x5090)]);
    assert_eq!(a_log.borrow().frames, expected);
    assert!(b_log.borrow().frames.is_empty());
    assert_eq!(b_log.borrow().calls, 0);
    assert_eq!(
        c_log
            .borrow()
            .events
            .iter()
            .filter(|e| **e == BoardEvent::Guard)
            .count(),
        9
    );
    assert_eq!(c_log.borrow().events.last(), Some(&BoardEvent::Pf0(false)));
    // No startup/PA or receive-enable register appeared in the command bytes.
    assert!(!a_log
        .borrow()
        .frames
        .iter()
        .any(|frame| matches!(frame[0], 0x00 | 0x30 | 0x36 | 0x37)));
}

#[test]
fn frequency_only_execution_selects_a_or_b_without_route_or_secondary_width_assumptions() {
    let cal = Calibration::parse(CAL).unwrap();
    for chip in [Chip::A, Chip::B] {
        let plan = FrequencyPlan::new(&cal, chip, 43_350_000).unwrap();
        let (mut a, a_log) = bus(Profile::A, &[0]);
        let (mut b, b_log) = bus(Profile::B, &[]);
        let (mut control, c_log) = board();
        let pair = Pair::new(&mut a, &mut b, &mut control).unwrap();
        let mut executor = Executor::new(pair);
        executor.start_frequency(plan).unwrap();
        for _ in 0..3 {
            executor.step().unwrap();
        }
        assert_eq!(executor.status(), Status::Configured);
        let writes = vec![
            bytes(0x38, plan.frequency_words().reg38),
            bytes(0x39, plan.frequency_words().reg39),
        ];
        if chip == Chip::A {
            let mut expected = vec![vec![0xb3]];
            expected.extend(writes);
            assert_eq!(a_log.borrow().frames, expected);
            assert!(b_log.borrow().frames.is_empty());
        } else {
            assert_eq!(a_log.borrow().frames, [vec![0xb3]]);
            assert_eq!(b_log.borrow().frames, writes);
        }
        assert_eq!(c_log.borrow().events, [BoardEvent::Guard; 3]);
    }
}

#[test]
fn transport_pin_error_preserves_cause_and_both_cleanup_errors_and_poison() {
    let (mut a, a_log) = bus(Profile::A, &[]);
    let (mut b, _) = bus(Profile::B, &[]);
    let (mut control, _) = board();
    a_log.borrow_mut().failures = vec![0, 1, 2];
    let mut pair = Pair::new(&mut a, &mut b, &mut control).unwrap();
    let error = pair.write(Chip::A, 0x38, 0xabcd).unwrap_err();
    let PairError::A(Error::Pin(fault)) = error else {
        panic!("transport fault lost")
    };
    assert_eq!(fault.cause, 0);
    assert_eq!(fault.chip_select_cleanup, Some(1));
    assert_eq!(fault.data_release_cleanup, Some(2));
    assert_eq!(
        pair.write(Chip::A, 0x39, 0),
        Err(PairError::A(Error::Poisoned))
    );
    assert_eq!(a_log.borrow().calls, 3);
}

#[test]
fn board_errors_keep_their_identity_without_implicit_rf_calls() {
    let (mut a, a_log) = bus(Profile::A, &[]);
    let (mut b, b_log) = bus(Profile::B, &[]);
    let (mut control, c_log) = board();
    c_log.borrow_mut().error = Some(71);
    let mut pair = Pair::new(&mut a, &mut b, &mut control).unwrap();
    assert_eq!(
        pair.check_receive_preconditions(),
        Err(PairError::Board(71))
    );
    assert_eq!(pair.set_antenna_pf0(false), Err(PairError::Board(71)));
    assert_eq!(a_log.borrow().calls + b_log.borrow().calls, 0);
}
