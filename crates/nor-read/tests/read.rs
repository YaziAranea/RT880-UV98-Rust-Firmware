use uv98_calibration::BLOCK_LEN;
use uv98_nor_read::{
    range_is_valid, read, read_calibration, Bus, CalibrationReadError, Phase, ReadError,
    CAPACITY_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Select,
    Command,
    Data,
    Deselect,
}

#[derive(Debug, Eq, PartialEq)]
enum Event {
    Select,
    Write(Vec<u8>),
    Read(usize),
    Deselect,
}

struct MockBus<'a> {
    data: &'a [u8],
    events: Vec<Event>,
    fail: Option<Fault>,
    fail_deselect: bool,
}

impl<'a> MockBus<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            events: Vec::new(),
            fail: None,
            fail_deselect: false,
        }
    }
}

impl Bus for MockBus<'_> {
    type Error = Fault;

    fn select(&mut self) -> Result<(), Fault> {
        self.events.push(Event::Select);
        if self.fail == Some(Fault::Select) {
            Err(Fault::Select)
        } else {
            Ok(())
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Fault> {
        self.events.push(Event::Write(bytes.to_vec()));
        if self.fail == Some(Fault::Command) {
            Err(Fault::Command)
        } else {
            Ok(())
        }
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<(), Fault> {
        self.events.push(Event::Read(destination.len()));
        if self.fail == Some(Fault::Data) {
            // A real backend can fail after clocking part of the requested data.
            destination[0] = self.data[0];
            return Err(Fault::Data);
        }
        destination.copy_from_slice(&self.data[..destination.len()]);
        Ok(())
    }

    fn deselect(&mut self) -> Result<(), Fault> {
        self.events.push(Event::Deselect);
        if self.fail_deselect {
            Err(Fault::Deselect)
        } else {
            Ok(())
        }
    }
}

#[test]
fn read_emits_only_ordinary_read_and_exact_big_endian_address() {
    let mut bus = MockBus::new(&[0x9A, 0x34, 0x56]);
    let mut destination = [0xFF; 3];
    read(&mut bus, 0x123456, &mut destination).unwrap();
    assert_eq!(destination, [0x9A, 0x34, 0x56]);
    assert_eq!(
        bus.events,
        [
            Event::Select,
            Event::Write(vec![0x03, 0x12, 0x34, 0x56]),
            Event::Read(3),
            Event::Deselect,
        ]
    );
}

#[test]
fn empty_valid_ranges_make_no_bus_calls() {
    let mut bus = MockBus::new(&[]);
    for address in [0, 0x123456, CAPACITY_BYTES] {
        read(&mut bus, address, &mut []).unwrap();
    }
    assert!(bus.events.is_empty());
}

#[test]
fn boundaries_and_overflow_inputs_are_rejected_without_bus_activity() {
    assert!(range_is_valid(0, CAPACITY_BYTES as usize));
    assert!(range_is_valid(CAPACITY_BYTES, 0));
    assert!(!range_is_valid(0, usize::MAX));
    assert!(!range_is_valid(1, usize::MAX));
    assert!(!range_is_valid(u32::MAX, 0));
    assert!(!range_is_valid(u32::MAX, usize::MAX));
    assert!(!range_is_valid(CAPACITY_BYTES + 1, 0));
    let mut bus = MockBus::new(&[]);
    let mut destination = [0xA5; 2];
    for address in [CAPACITY_BYTES - 1, CAPACITY_BYTES, u32::MAX] {
        assert_eq!(
            read(&mut bus, address, &mut destination),
            Err(ReadError::OutOfBounds { address, length: 2 })
        );
    }
    assert!(read(&mut bus, CAPACITY_BYTES + 1, &mut []).is_err());
    assert_eq!(destination, [0xA5; 2]);
    assert!(bus.events.is_empty());
}

#[test]
fn final_main_array_byte_is_read_without_wrapping_address() {
    let mut bus = MockBus::new(&[0x71]);
    let mut destination = [0];
    read(&mut bus, CAPACITY_BYTES - 1, &mut destination).unwrap();
    assert_eq!(destination, [0x71]);
    assert_eq!(bus.events[1], Event::Write(vec![3, 0x3F, 0xFF, 0xFF]));
}

#[test]
fn every_transaction_error_attempts_deselect_and_preserves_both_errors() {
    for (fault, phase, expected_events) in [
        (Fault::Select, Phase::Select, 2),
        (Fault::Command, Phase::Command, 3),
        (Fault::Data, Phase::Data, 4),
    ] {
        for fail_deselect in [false, true] {
            let mut bus = MockBus::new(&[0x11, 0x22]);
            bus.fail = Some(fault);
            bus.fail_deselect = fail_deselect;
            let mut destination = [0xA5; 2];
            assert_eq!(
                read(&mut bus, 0, &mut destination),
                Err(ReadError::Bus {
                    phase,
                    source: fault,
                    deselect_error: fail_deselect.then_some(Fault::Deselect),
                })
            );
            assert_eq!(bus.events.len(), expected_events);
            assert_eq!(bus.events.last(), Some(&Event::Deselect));
            assert_eq!(
                bus.events.iter().filter(|e| **e == Event::Deselect).count(),
                1
            );
            if fault == Fault::Data {
                assert_eq!(destination, [0x11, 0xA5]);
            } else {
                assert_eq!(destination, [0xA5; 2]);
            }
        }
    }
}

#[test]
fn failed_deselect_is_not_reported_as_a_successful_read() {
    let mut bus = MockBus::new(&[0x12]);
    bus.fail_deselect = true;
    let mut destination = [0];
    assert_eq!(
        read(&mut bus, 0, &mut destination),
        Err(ReadError::Deselect(Fault::Deselect))
    );
    assert_eq!(destination, [0x12]);
    assert_eq!(bus.events.len(), 4);
}

#[test]
fn calibration_reads_exact_region_and_returns_original_borrowed_bytes() {
    let fixture = include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");
    let mut bus = MockBus::new(fixture);
    let mut buffer = [0xA5; BLOCK_LEN];
    let calibration = read_calibration(&mut bus, &mut buffer).unwrap();
    assert_eq!(calibration.raw(), fixture);
    assert_eq!(calibration.first_byte(), 0x9A);
    assert_eq!(
        bus.events,
        [
            Event::Select,
            Event::Write(vec![3, 0, 0, 0]),
            Event::Read(4096),
            Event::Deselect
        ]
    );
}

#[test]
fn calibration_never_returns_a_view_on_transport_or_layout_failure() {
    let invalid = [0xFF; BLOCK_LEN];
    let mut bus = MockBus::new(&invalid);
    let mut buffer = [0xA5; BLOCK_LEN];
    assert!(matches!(
        read_calibration(&mut bus, &mut buffer),
        Err(CalibrationReadError::Layout(
            uv98_calibration::Error::InvalidMarker { found: 0xFF }
        ))
    ));
    assert_eq!(buffer, invalid); // No fallback or default substitution.
    assert_eq!(bus.events.last(), Some(&Event::Deselect));

    let mut looks_valid = [0; BLOCK_LEN];
    looks_valid[0] = 0x9A;
    let mut bus = MockBus::new(&looks_valid);
    bus.fail = Some(Fault::Data);
    assert!(matches!(
        read_calibration(&mut bus, &mut buffer),
        Err(CalibrationReadError::Read(ReadError::Bus {
            phase: Phase::Data,
            ..
        }))
    ));
    assert_eq!(buffer[0], 0x9A); // A partial valid marker must not create a view.

    let mut bus = MockBus::new(&looks_valid);
    bus.fail_deselect = true;
    assert!(matches!(
        read_calibration(&mut bus, &mut buffer),
        Err(CalibrationReadError::Read(ReadError::Deselect(
            Fault::Deselect
        )))
    ));
}
