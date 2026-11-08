use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use uv98_rf_bus::{
    Bus, Delay, Direction, Error, Phase, Pins, Profile, MAX_TRANSACTION_DELAYS, TRANSACTION_CLOCKS,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    kind: &'static str,
    value: u16,
    phase: Option<Phase>,
}
type Log = Rc<RefCell<Vec<Event>>>;

struct MockPins {
    log: Log,
    calls: Rc<Cell<usize>>,
    fail: Vec<usize>,
    direction: Direction,
    word: u16,
    sample: usize,
}
impl MockPins {
    fn emit(&mut self, kind: &'static str, value: u16) -> Result<(), usize> {
        self.log.borrow_mut().push(Event {
            kind,
            value,
            phase: None,
        });
        let call = self.calls.get();
        self.calls.set(call + 1);
        if self.fail.contains(&call) {
            Err(call)
        } else {
            Ok(())
        }
    }
}
impl Pins for MockPins {
    type Error = usize;
    fn chip_select(&mut self, high: bool) -> Result<(), usize> {
        self.emit("cs", u16::from(high))
    }
    fn clock(&mut self, high: bool) -> Result<(), usize> {
        self.emit("clock", u16::from(high))
    }
    fn data_level(&mut self, high: bool) -> Result<(), usize> {
        assert_eq!(self.direction, Direction::Output);
        self.emit("data", u16::from(high))
    }
    fn data_direction(&mut self, direction: Direction) -> Result<(), usize> {
        // Model a partially applied request even on failure.
        self.direction = direction;
        self.emit("direction", u16::from(direction == Direction::Input))
    }
    fn read_data(&mut self) -> Result<bool, usize> {
        assert_eq!(self.direction, Direction::Input);
        let bit = self.word & (0x8000 >> (self.sample % 16)) != 0;
        self.sample += 1;
        self.emit("sample", u16::from(bit))?;
        Ok(bit)
    }
}
struct MockDelay(Log);
impl Delay for MockDelay {
    fn wait(&mut self, phase: Phase) {
        self.0.borrow_mut().push(Event {
            kind: "delay",
            value: 10,
            phase: Some(phase),
        });
    }
}
fn make(
    profile: Profile,
    word: u16,
    fail: Vec<usize>,
) -> (Bus<MockPins, MockDelay>, Log, Rc<Cell<usize>>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let calls = Rc::new(Cell::new(0));
    let pins = MockPins {
        log: log.clone(),
        calls: calls.clone(),
        fail,
        direction: Direction::Output,
        word,
        sample: 0,
    };
    (Bus::new(pins, MockDelay(log.clone()), profile), log, calls)
}
fn transaction(bus: &mut Bus<MockPins, MockDelay>, read: bool) -> Result<u16, Error<usize>> {
    if read {
        bus.read(0x33)
    } else {
        bus.write(0x33, 0xa55a).map(|()| 0)
    }
}

#[test]
fn successful_transactions_match_original_thumb_pin_traces() {
    let mut cases = BTreeMap::<(String, String, u8, u16), Vec<(String, u16)>>::new();
    for line in include_str!("fixtures/transactions.csv").lines().skip(1) {
        let v: Vec<&str> = line.split(',').collect();
        let events = cases
            .entry((
                v[0].into(),
                v[1].into(),
                v[2].parse().unwrap(),
                v[3].parse().unwrap(),
            ))
            .or_default();
        assert_eq!(v[4].parse::<usize>().unwrap(), events.len());
        events.push((v[5].into(), v[6].parse().unwrap()));
    }
    assert_eq!(cases.len(), 80);
    let mut event_count = 0;
    for ((profile_name, operation, register, word), expected) in cases {
        let profile = if profile_name == "A" {
            Profile::A
        } else {
            Profile::B
        };
        let (mut bus, log, _) = make(profile, word, vec![]);
        if operation == "read" {
            assert_eq!(bus.read(register).unwrap(), word);
        } else {
            bus.write(register, word).unwrap();
        }
        let actual: Vec<(String, u16)> = log
            .borrow()
            .iter()
            .map(|e| (e.kind.into(), e.value))
            .collect();
        assert_eq!(
            actual, expected,
            "{profile_name} {operation} reg{register:02x} word{word:04x}"
        );
        event_count += actual.len();
        assert_eq!(
            actual
                .iter()
                .filter(|(kind, v)| kind == "clock" && *v == 1)
                .count(),
            TRANSACTION_CLOCKS
        );
        let delays = actual.iter().filter(|(kind, _)| kind == "delay").count();
        assert_eq!(delays, if operation == "read" { 49 } else { 48 });
        assert!(delays <= MAX_TRANSACTION_DELAYS);
        assert!(!bus.is_poisoned());
    }
    assert_eq!(event_count, 10020);
}

#[test]
fn inert_constructor_drop_and_invalid_addresses_do_not_touch_pins() {
    for profile in [Profile::A, Profile::B] {
        let (mut bus, log, _) = make(profile, 0, vec![]);
        assert_eq!(bus.profile(), profile);
        for register in 0x80..=0xff {
            assert_eq!(bus.read(register), Err(Error::InvalidRegister(register)));
            assert_eq!(
                bus.write(register, 0xffff),
                Err(Error::InvalidRegister(register))
            );
        }
        assert!(!bus.is_poisoned());
        drop(bus);
        assert!(log.borrow().is_empty());
    }
}

#[test]
fn every_pin_failure_releases_then_poison_blocks_all_later_io() {
    for profile in [Profile::A, Profile::B] {
        for read in [false, true] {
            let (mut baseline, baseline_log, calls) = make(profile, 0xa55a, vec![]);
            transaction(&mut baseline, read).unwrap();
            let successful = baseline_log.borrow().clone();
            for failure in 0..calls.get() {
                let (mut bus, log, failed_calls) = make(profile, 0xa55a, vec![failure]);
                let Error::Pin(fault) = transaction(&mut bus, read).unwrap_err() else {
                    panic!()
                };
                assert_eq!(fault.cause, failure);
                assert_eq!(fault.chip_select_cleanup, None);
                assert_eq!(fault.data_release_cleanup, None);
                assert!(bus.is_poisoned());
                let failed_event = successful
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.kind != "delay")
                    .nth(failure)
                    .unwrap()
                    .0;
                let trace = log.borrow().clone();
                assert_eq!(trace[..=failed_event], successful[..=failed_event]);
                assert_eq!(
                    trace[failed_event + 1..],
                    [
                        Event {
                            kind: "cs",
                            value: 1,
                            phase: None
                        },
                        Event {
                            kind: "direction",
                            value: 1,
                            phase: None
                        },
                    ]
                );
                assert_eq!(failed_calls.get(), failure + 3);
                assert_eq!(bus.read(0x33), Err(Error::Poisoned));
                assert_eq!(bus.write(0, 0), Err(Error::Poisoned));
                assert_eq!(bus.read(0xff), Err(Error::Poisoned));
                assert_eq!(*log.borrow(), trace);
            }
        }
    }
}

#[test]
fn cleanup_failures_are_reported_and_do_not_skip_data_release() {
    for failures in [vec![7, 8], vec![7, 9], vec![7, 8, 9]] {
        let (mut bus, log, calls) = make(Profile::A, 0, failures.clone());
        let Error::Pin(fault) = bus.read(0x7f).unwrap_err() else {
            panic!()
        };
        assert_eq!(fault.cause, 7);
        assert_eq!(
            fault.chip_select_cleanup,
            failures.contains(&8).then_some(8)
        );
        assert_eq!(
            fault.data_release_cleanup,
            failures.contains(&9).then_some(9)
        );
        assert_eq!(calls.get(), 10);
        assert_eq!(log.borrow().last().unwrap().kind, "direction");
        assert!(bus.is_poisoned());
    }
}

#[test]
fn direction_turnaround_profiles_allow_consecutive_reads_and_writes() {
    for profile in [Profile::A, Profile::B] {
        let (mut bus, log, _) = make(profile, 0x8001, vec![]);
        assert_eq!(bus.read(0), Ok(0x8001));
        bus.write(0x7f, 0).unwrap();
        assert_eq!(bus.read(0x33), Ok(0x8001));
        let dirs: Vec<u16> = log
            .borrow()
            .iter()
            .filter(|e| e.kind == "direction")
            .map(|e| e.value)
            .collect();
        assert_eq!(
            dirs,
            if profile == Profile::A {
                vec![1, 0, 1, 0]
            } else {
                vec![0, 1, 0, 0, 1]
            }
        );
    }
}

#[test]
fn all_word_bit_positions_are_read_msb_first() {
    for profile in [Profile::A, Profile::B] {
        for bit in 0..16 {
            let (mut bus, _, _) = make(profile, 1 << bit, vec![]);
            assert_eq!(bus.read(0x33), Ok(1 << bit));
        }
    }
}

#[test]
fn timing_phases_do_not_invent_a_read_sample_setup_delay() {
    let (mut bus, log, _) = make(Profile::A, 0, vec![]);
    bus.read(0x33).unwrap();
    let events = log.borrow();
    let turn = events
        .iter()
        .position(|e| e.phase == Some(Phase::ReadTurnaround))
        .unwrap();
    assert_eq!(
        (events[turn + 1].kind, events[turn + 1].value),
        ("direction", 1)
    );
    assert_eq!(
        (events[turn + 2].kind, events[turn + 2].value),
        ("clock", 0)
    );
    assert_eq!(
        (events[turn + 3].kind, events[turn + 3].value),
        ("clock", 1)
    );
    assert_eq!(events[turn + 4].kind, "sample");
    assert_eq!(events[turn + 5].phase, Some(Phase::ReadHigh));
    assert_eq!(
        (events[turn + 6].kind, events[turn + 6].value),
        ("clock", 0)
    );
    assert_eq!(events[turn + 7].phase, Some(Phase::ReadLow));
}

#[test]
fn drop_after_use_attempts_deselect_and_input_without_clocking() {
    let (mut bus, log, _) = make(Profile::B, 0, vec![]);
    bus.write(0x33, 0x55aa).unwrap();
    log.borrow_mut().clear();
    drop(bus);
    assert_eq!(
        *log.borrow(),
        vec![
            Event {
                kind: "cs",
                value: 1,
                phase: None
            },
            Event {
                kind: "direction",
                value: 1,
                phase: None
            },
        ]
    );
}
