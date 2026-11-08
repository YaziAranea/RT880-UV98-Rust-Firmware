use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use uv98_calibration::Calibration;
use uv98_receive::{
    executor::{Attempt, Executor, Failure, Io, StartError, Status},
    Chip, FrequencyPlan, Operation, Plan, Width,
};

const CAL: &[u8] = include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Guard,
    Read(Chip, u8),
    Write(Chip, u8, u16),
    Antenna(bool),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MockError {
    Injected(usize),
    Interlock,
}
#[derive(Default)]
struct State {
    events: Vec<Event>,
    reads: VecDeque<u16>,
    fail_at: Option<usize>,
    block_guard: bool,
    block_after_read: Option<usize>,
    read_count: usize,
}
type Shared = Rc<RefCell<State>>;
struct Mock(Shared);
impl Mock {
    fn emit(&mut self, event: Event) -> Result<(), MockError> {
        let mut s = self.0.borrow_mut();
        let index = s.events.len();
        s.events.push(event);
        if s.fail_at == Some(index) {
            Err(MockError::Injected(index))
        } else {
            Ok(())
        }
    }
}
impl Io for Mock {
    type Error = MockError;
    fn check_receive_preconditions(&mut self) -> Result<(), MockError> {
        self.emit(Event::Guard)?;
        if self.0.borrow().block_guard {
            Err(MockError::Interlock)
        } else {
            Ok(())
        }
    }
    fn read(&mut self, chip: Chip, register: u8) -> Result<u16, MockError> {
        self.emit(Event::Read(chip, register))?;
        let mut s = self.0.borrow_mut();
        s.read_count += 1;
        if s.block_after_read == Some(s.read_count) {
            s.block_guard = true;
        }
        Ok(s.reads.pop_front().expect("unexpected register read"))
    }
    fn write(&mut self, chip: Chip, register: u8, value: u16) -> Result<(), MockError> {
        self.emit(Event::Write(chip, register, value))
    }
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), MockError> {
        self.emit(Event::Antenna(high))
    }
}
fn make(reads: &[u16]) -> (Executor<Mock>, Shared) {
    let state = Rc::new(RefCell::new(State {
        reads: reads.iter().copied().collect(),
        ..State::default()
    }));
    (Executor::new(Mock(state.clone())), state)
}
fn plan() -> Plan {
    Plan::primary(
        &Calibration::parse(CAL).unwrap(),
        14_550_000,
        Width::Wide,
        5,
    )
    .unwrap()
}
fn finish(executor: &mut Executor<Mock>) {
    for _ in 0..10 {
        if executor.status() != Status::Running {
            return;
        }
        let _ = executor.step();
    }
    panic!("bounded executor did not terminate");
}

#[test]
fn inert_start_step_order_fresh_route_word_and_configured_is_not_retuned_by_polling() {
    let p = plan();
    let (mut e, shared) = make(&[0xa000, 0x50b0]);
    assert_eq!(e.step().unwrap().status, Status::Idle);
    e.start(p).unwrap();
    assert!(shared.borrow().events.is_empty());
    assert_eq!(e.step().unwrap().completed_operations, 0);
    assert_eq!(
        shared.borrow().events,
        [Event::Guard, Event::Read(Chip::A, 0x33)]
    );
    let mut expected = shared.borrow().events.clone();
    for (index, operation) in p.operations().iter().enumerate() {
        let progress = e.step().unwrap();
        assert_eq!(progress.completed_operations, index + 1);
        expected.push(Event::Guard);
        match *operation {
            Operation::Write {
                chip,
                register,
                value,
            } => expected.push(Event::Write(chip, register, value)),
            Operation::PrimaryRoute { .. } => {
                expected.extend([
                    Event::Read(Chip::A, 0x33),
                    Event::Guard,
                    Event::Write(Chip::A, 0x33, 0x5090),
                ]);
            }
            Operation::AntennaPf0 { high } => expected.push(Event::Antenna(high)),
        }
        assert_eq!(shared.borrow().events, expected);
    }
    assert_eq!(e.status(), Status::Configured);
    assert!(e.progress().initial_tx_check_completed);
    for _ in 0..3 {
        assert_eq!(e.step().unwrap().status, Status::Configured);
    }
    assert_eq!(shared.borrow().events, expected);
}

#[test]
fn every_io_failure_is_terminal_and_retains_error_attempt_and_completed_progress() {
    let (mut baseline, shared) = make(&[0, 0]);
    baseline.start(plan()).unwrap();
    finish(&mut baseline);
    let all = shared.borrow().events.clone();
    assert_eq!(all.len(), 18);
    for fail_at in 0..all.len() {
        let (mut e, shared) = make(&[0, 0]);
        shared.borrow_mut().fail_at = Some(fail_at);
        e.start(plan()).unwrap();
        finish(&mut e);
        let fault = e.fault().expect("failure must be retained");
        assert_eq!(fault.reason, Failure::Io(MockError::Injected(fail_at)));
        assert_eq!(fault.progress.status, Status::Faulted);
        let completed = all[..fail_at]
            .iter()
            .filter(|event| matches!(event, Event::Write(..) | Event::Antenna(..)))
            .count();
        assert_eq!(
            fault.progress.completed_operations, completed,
            "failure {fail_at}"
        );
        assert_eq!(fault.progress.total_operations, 7);
        assert_eq!(shared.borrow().events, all[..=fail_at]);
        let attempt = fault.attempt;
        for _ in 0..3 {
            assert_eq!(e.step().unwrap_err().attempt, attempt);
        }
        assert_eq!(e.start(plan()), Err(StartError::Terminal(Status::Faulted)));
        assert_eq!(e.cancel().status, Status::Faulted);
        assert_eq!(shared.borrow().events, all[..=fail_at]);
    }
}

#[test]
fn initial_tx_flags_refuse_every_write_and_remain_terminal_even_if_state_changes() {
    for word in [4, 8, 12, 0xffff] {
        let (mut e, shared) = make(&[word]);
        e.start(plan()).unwrap();
        let fault = e.step().unwrap_err();
        assert_eq!(fault.attempt, Attempt::InitialTxRead);
        assert_eq!(
            fault.reason,
            Failure::TxGatesEnabled {
                observed_reg33: word
            }
        );
        assert_eq!(fault.progress.completed_operations, 0);
        assert!(!fault.progress.initial_tx_check_completed);
        shared.borrow_mut().reads.push_back(0);
        assert!(e.step().is_err());
        assert_eq!(
            shared.borrow().events,
            [Event::Guard, Event::Read(Chip::A, 0x33)]
        );
    }
}

#[test]
fn route_rechecks_tx_flags_and_prevents_later_relay_request() {
    let (mut e, shared) = make(&[0, 0x500c]);
    e.start(plan()).unwrap();
    finish(&mut e);
    let fault = e.fault().unwrap();
    assert_eq!(fault.attempt, Attempt::RouteRead { index: 5 });
    assert_eq!(
        fault.reason,
        Failure::TxGatesEnabled {
            observed_reg33: 0x500c
        }
    );
    assert_eq!(fault.progress.completed_operations, 5);
    assert!(!shared
        .borrow()
        .events
        .iter()
        .any(|event| matches!(event, Event::Write(_, 0x33, _) | Event::Antenna(_))));
}

#[test]
fn ptt_interlock_changing_during_route_read_is_checked_again_before_write() {
    let (mut e, shared) = make(&[0, 0]);
    shared.borrow_mut().block_after_read = Some(2);
    e.start(plan()).unwrap();
    finish(&mut e);
    let fault = e.fault().unwrap();
    assert_eq!(fault.attempt, Attempt::RouteWriteGuard { index: 5 });
    assert_eq!(fault.reason, Failure::Io(MockError::Interlock));
    assert_eq!(fault.progress.completed_operations, 5);
    assert!(!shared
        .borrow()
        .events
        .iter()
        .any(|event| matches!(event, Event::Write(_, 0x33, _) | Event::Antenna(_))));
}

#[test]
fn cancellation_at_every_step_and_after_configured_never_resumes_or_issues_cleanup() {
    for steps in 0..=8 {
        let (mut e, shared) = make(&[0, 0]);
        e.start(plan()).unwrap();
        for _ in 0..steps {
            e.step().unwrap();
        }
        let before = shared.borrow().events.clone();
        let progress = e.cancel();
        assert_eq!(progress.status, Status::Cancelled);
        for _ in 0..3 {
            assert_eq!(e.step().unwrap(), progress);
        }
        assert_eq!(
            e.start(plan()),
            Err(StartError::Terminal(Status::Cancelled))
        );
        assert_eq!(shared.borrow().events, before);
    }
    let (mut idle, shared) = make(&[]);
    idle.cancel();
    assert_eq!(
        idle.start(plan()),
        Err(StartError::Terminal(Status::Cancelled))
    );
    assert!(shared.borrow().events.is_empty());
}

#[test]
fn explicit_retune_requires_new_precheck_and_running_plan_cannot_be_replaced() {
    let (mut e, shared) = make(&[0, 0, 4]);
    e.start(plan()).unwrap();
    let cal = Calibration::parse(CAL).unwrap();
    let b = FrequencyPlan::new(&cal, Chip::B, 43_350_000).unwrap();
    assert_eq!(e.start_frequency(b), Err(StartError::AlreadyRunning));
    assert!(shared.borrow().events.is_empty());
    finish(&mut e);
    e.start_frequency(b).unwrap();
    assert_eq!(e.progress().completed_operations, 0);
    assert_eq!(e.progress().total_operations, 2);
    assert!(!e.progress().initial_tx_check_completed);
    let before = shared.borrow().events.len();
    let fault = e.step().unwrap_err();
    assert_eq!(fault.attempt, Attempt::InitialTxRead);
    assert_eq!(
        &shared.borrow().events[before..],
        [Event::Guard, Event::Read(Chip::A, 0x33)]
    );
}

#[test]
fn secondary_frequency_plan_writes_only_b38_b39_after_explicit_a_interlock() {
    let cal = Calibration::parse(CAL).unwrap();
    let p = FrequencyPlan::new(&cal, Chip::B, 43_350_000).unwrap();
    let (mut e, shared) = make(&[0]);
    e.start_frequency(p).unwrap();
    finish(&mut e);
    assert_eq!(e.status(), Status::Configured);
    assert_eq!(
        shared.borrow().events,
        [
            Event::Guard,
            Event::Read(Chip::A, 0x33),
            Event::Guard,
            Event::Write(Chip::B, 0x38, p.frequency_words().reg38),
            Event::Guard,
            Event::Write(Chip::B, 0x39, p.frequency_words().reg39),
        ]
    );
}
