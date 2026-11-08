extern crate std;

use super::*;
use std::{cell::RefCell, rc::Rc, vec::Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Off,
    Guard,
    Read(Chip, u8),
    Write(Chip, u8, u16),
    Light(bool),
}
struct Memory {
    a: [u16; 128],
    b: [u16; 128],
    events: Vec<Event>,
    io: usize,
    fail_io: Option<usize>,
    guard: usize,
    fail_guard: Option<usize>,
    off: usize,
    fail_off: Option<usize>,
    ignore_write: Option<(Chip, u8)>,
    constant_read: Option<u16>,
}
type Shared = Rc<RefCell<Memory>>;
struct MockBus {
    chip: Chip,
    shared: Shared,
}
struct MockControl {
    shared: Shared,
}
fn fixture() -> (MockBus, MockBus, MockControl, Shared) {
    let shared = Rc::new(RefCell::new(Memory {
        a: [0xa55a; 128],
        b: [0x5aa5; 128],
        events: Vec::new(),
        io: 0,
        fail_io: None,
        guard: 0,
        fail_guard: None,
        off: 0,
        fail_off: None,
        ignore_write: None,
        constant_read: None,
    }));
    (
        MockBus {
            chip: Chip::A,
            shared: shared.clone(),
        },
        MockBus {
            chip: Chip::B,
            shared: shared.clone(),
        },
        MockControl {
            shared: shared.clone(),
        },
        shared,
    )
}
impl RegisterIo for MockBus {
    type Error = u8;
    fn profile(&self) -> Profile {
        match self.chip {
            Chip::A => Profile::A,
            Chip::B => Profile::B,
        }
    }
    fn read(&mut self, address: u8) -> Result<u16, u8> {
        let mut s = self.shared.borrow_mut();
        s.events.push(Event::Read(self.chip, address));
        s.io += 1;
        if s.fail_io == Some(s.io) {
            return Err(7);
        }
        if let Some(value) = s.constant_read {
            return Ok(value);
        }
        Ok(match self.chip {
            Chip::A => s.a[address as usize],
            Chip::B => s.b[address as usize],
        })
    }
    fn write(&mut self, address: u8, value: u16) -> Result<(), u8> {
        let mut s = self.shared.borrow_mut();
        s.events.push(Event::Write(self.chip, address, value));
        s.io += 1;
        if s.fail_io == Some(s.io) {
            return Err(7);
        }
        if s.ignore_write != Some((self.chip, address)) {
            match self.chip {
                Chip::A => s.a[address as usize] = value,
                Chip::B => s.b[address as usize] = value,
            }
        }
        Ok(())
    }
}
impl ExternalOff for MockControl {
    type Error = u8;
    fn assert_off(&mut self) -> Result<(), u8> {
        let mut s = self.shared.borrow_mut();
        s.events.push(Event::Off);
        s.off += 1;
        if s.fail_off == Some(s.off) {
            Err(9)
        } else {
            Ok(())
        }
    }
    fn check_off(&mut self) -> Result<(), u8> {
        let mut s = self.shared.borrow_mut();
        s.events.push(Event::Guard);
        s.guard += 1;
        if s.fail_guard == Some(s.guard) {
            Err(8)
        } else {
            Ok(())
        }
    }
}
impl Backlight for MockControl {
    fn set_backlight(&mut self, on: bool) -> Result<(), u8> {
        if on {
            self.check_off()?;
        }
        self.shared.borrow_mut().events.push(Event::Light(on));
        Ok(())
    }
}

#[test]
fn executor_matches_off_fragments_and_preserves_unowned_high_bits() {
    let (mut a, mut b, mut c, s) = fixture();
    let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
    assert!(s.borrow().events.is_empty());
    let state = shutdown.run_once().unwrap();
    assert_eq!(shutdown.state(), State::RegisterVerified);
    assert_eq!(
        state.words,
        [0xa540, 0x7f, 0, 0x1d00, 0x5aa0, 0x7f, 0, 0x1d00]
    );
    assert_eq!(s.borrow().events[0], Event::Off);
    let writes: Vec<_> = s
        .borrow()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Write(chip, reg, value) => Some((*chip, *reg, *value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes,
        [
            (Chip::A, 0x33, 0xa540),
            (Chip::A, 0x36, 0x7f),
            (Chip::A, 0x30, 0),
            (Chip::A, 0x37, 0x1d00),
            (Chip::B, 0x33, 0x5aa0),
            (Chip::B, 0x36, 0x7f),
            (Chip::B, 0x30, 0),
            (Chip::B, 0x37, 0x1d00)
        ]
    );
    assert_eq!(s.borrow().io, 18);
}

#[test]
fn combined_primary_mask_is_exact_composition_for_every_word() {
    use uv98_board_policy::{Operation, SLEEP_A, TX_GATES_OFF_A};
    let Operation::Register { change: tx, .. } = TX_GATES_OFF_A[2] else {
        panic!("fixture")
    };
    let Operation::Register { change: rx, .. } = SLEEP_A[0] else {
        panic!("fixture")
    };
    for value in 0..=u16::MAX {
        assert_eq!(REQUESTS[0].change.apply(value), rx.apply(tx.apply(value)));
        assert_eq!(REQUESTS[0].change.apply(value) & 0x3f, 0);
    }
}

#[test]
fn every_bus_failure_prefix_is_terminal_and_never_retried() {
    for fail in 1..=18 {
        let (mut a, mut b, mut c, s) = fixture();
        s.borrow_mut().fail_io = Some(fail);
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        let failure = shutdown.run_once().unwrap_err();
        assert!(matches!(
            failure.cause,
            Cause::Primary(7) | Cause::Secondary(7)
        ));
        assert_eq!(failure.external_cleanup, None);
        assert_eq!(shutdown.state(), State::Failed);
        assert_eq!(s.borrow().io, fail);
        assert_eq!(s.borrow().events.last(), Some(&Event::Off));
        let count = s.borrow().events.len();
        assert_eq!(shutdown.run_once().unwrap_err().cause, Cause::State);
        assert_eq!(shutdown.verify_off().unwrap_err().cause, Cause::State);
        assert_eq!(
            shutdown.set_backlight(true).unwrap_err().cause,
            Cause::State
        );
        assert_eq!(s.borrow().events.len(), count);
    }
}

#[test]
fn external_failure_after_fresh_read_prevents_the_write() {
    let (mut a, mut b, mut c, s) = fixture();
    s.borrow_mut().fail_guard = Some(2);
    let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
    assert_eq!(shutdown.run_once().unwrap_err().cause, Cause::External(8));
    assert_eq!(s.borrow().io, 1);
    assert!(s
        .borrow()
        .events
        .iter()
        .all(|event| !matches!(event, Event::Write(..))));
    assert_eq!(s.borrow().off, 2);
}

#[test]
fn each_ignored_off_write_is_detected_by_its_readback() {
    for request in REQUESTS {
        let (mut a, mut b, mut c, s) = fixture();
        s.borrow_mut().ignore_write = Some((request.chip, request.address));
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        let error = shutdown.run_once().unwrap_err();
        assert!(
            matches!(error.cause,Cause::Readback {chip,address,..} if chip==request.chip && address==request.address)
        );
        assert_eq!(shutdown.state(), State::Failed);
        assert_eq!(s.borrow().off, 2);
    }
}

#[test]
fn stuck_low_and_stuck_high_reads_cannot_pass_initial_sequence() {
    for value in [0, 0xffff] {
        let (mut a, mut b, mut c, s) = fixture();
        s.borrow_mut().constant_read = Some(value);
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        assert!(matches!(
            shutdown.run_once().unwrap_err().cause,
            Cause::Readback { .. }
        ));
        assert_eq!(shutdown.state(), State::Failed);
    }
}

#[test]
fn cleanup_failure_preserves_original_cause_and_is_not_repeated_by_drop() {
    let (mut a, mut b, mut c, s) = fixture();
    s.borrow_mut().fail_io = Some(1);
    s.borrow_mut().fail_off = Some(2);
    {
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        let error = shutdown.run_once().unwrap_err();
        assert_eq!(error.cause, Cause::Primary(7));
        assert_eq!(error.external_cleanup, Some(9));
        assert_eq!(s.borrow().off, 2);
    }
    assert_eq!(s.borrow().off, 2);
}

#[test]
fn verify_is_read_only_and_later_tx_bit_drift_is_terminal() {
    let (mut a, mut b, mut c, s) = fixture();
    let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
    let first = shutdown.run_once().unwrap();
    s.borrow_mut().events.clear();
    assert_eq!(shutdown.verify_off().unwrap(), first);
    assert_eq!(
        s.borrow()
            .events
            .iter()
            .filter(|e| matches!(e, Event::Read(..)))
            .count(),
        8
    );
    assert!(s
        .borrow()
        .events
        .iter()
        .all(|e| !matches!(e, Event::Write(..))));
    s.borrow_mut().a[0x33] |= 4;
    assert!(matches!(
        shutdown.verify_off().unwrap_err().cause,
        Cause::Readback {
            chip: Chip::A,
            address: 0x33,
            ..
        }
    ));
    assert_eq!(shutdown.state(), State::Failed);
    assert_eq!(s.borrow().events.last(), Some(&Event::Off));
}

#[test]
fn active_drop_or_stop_blanks_once_new_drop_is_inert() {
    let (mut a, mut b, mut c, s) = fixture();
    {
        let _shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
    }
    assert!(s.borrow().events.is_empty());
    {
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        shutdown.run_once().unwrap();
        shutdown.set_backlight(true).unwrap();
        shutdown.stop().unwrap();
        shutdown.stop().unwrap();
        assert_eq!(s.borrow().off, 2);
    }
    assert_eq!(s.borrow().off, 2);
    let (mut a, mut b, mut c, s) = fixture();
    {
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        shutdown.run_once().unwrap();
    }
    assert_eq!(s.borrow().off, 2);
}

#[test]
fn swapped_profiles_are_rejected_without_any_bus_or_control_access() {
    let (mut a, mut b, mut c, s) = fixture();
    assert!(matches!(
        Shutdown::new(&mut b, &mut a, &mut c),
        Err(ProfileMismatch {
            primary: Profile::B,
            secondary: Profile::A
        })
    ));
    assert!(s.borrow().events.is_empty());
}

#[test]
fn initial_external_request_failure_never_reaches_rf_bus() {
    let (mut a, mut b, mut c, s) = fixture();
    s.borrow_mut().fail_off = Some(1);
    let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
    let error = shutdown.run_once().unwrap_err();
    assert_eq!(error.cause, Cause::External(9));
    assert_eq!(error.external_cleanup, None);
    assert_eq!(s.borrow().io, 0);
    assert_eq!(s.borrow().off, 2);
}

#[test]
fn invalid_order_poisoning_is_inert_before_start_and_blanks_after_start() {
    let (mut a, mut b, mut c, s) = fixture();
    {
        let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
        assert_eq!(shutdown.verify_off().unwrap_err().cause, Cause::State);
        assert_eq!(shutdown.run_once().unwrap_err().cause, Cause::State);
        assert_eq!(shutdown.state(), State::Failed);
    }
    assert!(s.borrow().events.is_empty());
    let mut shutdown = Shutdown::new(&mut a, &mut b, &mut c).unwrap();
    shutdown.run_once().unwrap();
    assert_eq!(shutdown.run_once().unwrap_err().cause, Cause::State);
    assert_eq!(shutdown.state(), State::Failed);
    assert_eq!(s.borrow().off, 2);
}
