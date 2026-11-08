extern crate std;

use crate::{
    engine::{Engine, Registers},
    Config, Divider, Error, GpioRegisters, Mode, Wait,
};
use core::num::NonZeroU32;
use std::{collections::VecDeque, vec, vec::Vec};
use uv98_nor_read::{read, Bus, ReadError};

#[derive(Default)]
struct Fake {
    statuses: VecDeque<u32>,
    fallback: u32,
    polls: usize,
    transmitted: Vec<u8>,
    transmit_polls: Vec<usize>,
    received: u8,
    selections: Vec<bool>,
    disabled: usize,
}

impl Registers for Fake {
    fn status(&mut self) -> u32 {
        self.polls += 1;
        self.statuses.pop_front().unwrap_or(self.fallback)
    }
    fn transmit(&mut self, byte: u8) {
        self.transmitted.push(byte);
        self.transmit_polls.push(self.polls);
    }
    fn receive(&mut self) -> u8 {
        let result = self.received;
        self.received = self.received.wrapping_add(1);
        result
    }
    fn chip_select(&mut self, active: bool) {
        self.selections.push(active);
    }
    fn disable(&mut self) {
        self.disabled += 1;
    }
}

fn engine() -> Engine<Fake> {
    Engine::new(
        Fake {
            fallback: 3,
            ..Fake::default()
        },
        NonZeroU32::new(3).unwrap(),
    )
}

#[test]
fn gpio_plan_preserves_every_unowned_bit() {
    for old in [0, u32::MAX, 0xa5a5_a5a5, 0x1234_5678] {
        let actual = GpioRegisters {
            cfgr: old,
            omode: old,
            odrvr: old,
            pull: old,
            muxh: old,
        }
        .with_spi2_nor();
        assert_eq!(actual.cfgr & 0x00ff_ffff, old & 0x00ff_ffff);
        assert_eq!(actual.cfgr >> 24, 0xa9);
        assert_eq!(actual.omode & !0xf000, old & !0xf000);
        assert_eq!(actual.omode & 0xf000, 0);
        assert_eq!(actual.odrvr, (old & 0x00ff_ffff) | 0x5500_0000);
        assert_eq!(actual.pull, (old & 0x00ff_ffff) | 0x5500_0000);
        assert_eq!(actual.muxh, (old & 0x000f_ffff) | 0x5550_0000);
    }
}

#[test]
fn mode3_words_have_only_the_selected_clock_bits_changed() {
    let dividers = [
        Divider::Div2,
        Divider::Div4,
        Divider::Div8,
        Divider::Div16,
        Divider::Div32,
        Divider::Div64,
        Divider::Div128,
        Divider::Div256,
    ];
    for (index, divider) in dividers.into_iter().enumerate() {
        assert_eq!(divider.divisor(), 2 << index);
        assert_eq!(divider.ctrl1(), 0x347 | ((index as u32) << 3));
        let mode3 = Config {
            divider,
            mode: Mode::Mode3,
            poll_budget: NonZeroU32::new(3).unwrap(),
        };
        let mode0 = Config {
            mode: Mode::Mode0,
            ..mode3
        };
        assert_eq!(mode3.ctrl1(), divider.ctrl1());
        assert_eq!(mode0.ctrl1(), 0x344 | ((index as u32) << 3));
        assert_eq!(mode3.ctrl1() ^ mode0.ctrl1(), 3);
    }
}

#[test]
fn gpio_and_spi_words_match_bounded_reference_machine_code() {
    let fixture = include_str!("../tests/fixtures/spi-init-oracle.csv");
    let mut count = 0;
    for line in fixture.lines().skip(1) {
        let row: Vec<u32> = line
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 10);
        let seed = row[0];
        let actual = GpioRegisters {
            cfgr: seed,
            omode: seed,
            odrvr: seed,
            pull: seed,
            muxh: seed,
        }
        .with_spi2_nor();
        assert_eq!(
            [
                actual.cfgr,
                actual.omode,
                actual.odrvr,
                actual.pull,
                actual.muxh
            ],
            row[1..6]
        );
        assert_eq!(Divider::Div2.ctrl1(), row[6]);
        assert_eq!(&row[7..], &[0, 0, 0x1000]);
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn nor_read_integrates_header_dummy_receive_and_release() {
    let mut bus = engine();
    let mut destination = [0; 3];
    read(&mut bus, 0x012345, &mut destination).unwrap();
    assert_eq!(
        bus.registers.transmitted,
        [3, 1, 0x23, 0x45, 0xa5, 0xa5, 0xa5]
    );
    assert_eq!(destination, [4, 5, 6]); // first four received bytes belong to header
    assert_eq!(bus.registers.selections, [true, false]);
    assert_eq!(bus.registers.disabled, 0);
    read(&mut bus, 0, &mut [0]).unwrap(); // completed transactions are reusable
}

#[test]
fn header_bytes_wait_for_busy_but_data_pipeline_remains_unchanged() {
    let mut bus = engine();
    bus.registers.statuses.push_back(2); // Initial idle check.
    for _ in 0..4 {
        // TDBE, RDBF, still BUSY, then command/address byte completed.
        bus.registers.statuses.extend([2, 1, 0x80, 0]);
    }
    bus.registers.statuses.extend([2, 1, 2, 1, 0]); // Two data bytes + deselect.
    let mut data = [0; 2];
    read(&mut bus, 0, &mut data).unwrap();
    assert_eq!(bus.registers.transmitted, [3, 0, 0, 0, 0xa5, 0xa5]);
    assert_eq!(bus.registers.transmit_polls, [2, 6, 10, 14, 18, 20]);
    assert_eq!(bus.registers.polls, 22);
    assert!(bus.registers.statuses.is_empty());
    assert_eq!(data, [4, 5]);
    assert_eq!(bus.registers.selections, [true, false]);
}

#[test]
fn header_busy_timeout_reports_command_error_and_releases_cs() {
    let mut bus = engine();
    bus.registers.statuses = [2, 2, 1, 0x80, 0x80, 0x80, 0].into();
    let mut data = [0xa6];
    assert_eq!(
        read(&mut bus, 0, &mut data),
        Err(ReadError::Bus {
            phase: uv98_nor_read::Phase::Command,
            source: Error::Timeout(Wait::NotBusy),
            deselect_error: None,
        })
    );
    assert_eq!(data, [0xa6]);
    assert_eq!(bus.registers.transmitted, [3]);
    assert_eq!(bus.registers.polls, 7);
    assert_eq!(bus.registers.selections, [true, false]);
    assert_eq!(bus.registers.disabled, 1);
    assert_eq!(bus.select(), Err(Error::Poisoned));
}

#[test]
fn refuses_mutating_and_malformed_headers_before_any_byte() {
    for command in [
        vec![0x06],
        vec![0x02, 0, 0, 0],
        vec![0x20, 0, 0, 0],
        vec![0x03],
        vec![0x03, 0, 0, 0, 0],
        vec![],
    ] {
        let mut bus = engine();
        bus.select().unwrap();
        assert_eq!(bus.write(&command), Err(Error::InvalidReadHeader));
        assert!(bus.registers.transmitted.is_empty());
        bus.deselect().unwrap();
        assert_eq!(bus.registers.selections, [true, false]);
    }
}

#[test]
fn rejects_read_order_and_capacity_wrap_without_io() {
    let mut bus = engine();
    assert_eq!(bus.read(&mut [0]), Err(Error::InvalidState));
    assert_eq!(bus.write(&[3, 0, 0, 0]), Err(Error::InvalidState));
    bus.select().unwrap();
    assert_eq!(bus.read(&mut [0]), Err(Error::InvalidState));
    assert_eq!(bus.write(&[3, 0x40, 0, 0]), Err(Error::OutOfBounds));
    assert!(bus.registers.transmitted.is_empty());
    bus.write(&[3, 0x3f, 0xff, 0xff]).unwrap();
    let before = bus.registers.transmitted.len();
    assert_eq!(bus.read(&mut [0; 2]), Err(Error::OutOfBounds));
    assert_eq!(bus.registers.transmitted.len(), before);
    bus.read(&mut [0]).unwrap();
    bus.read(&mut []).unwrap();
    assert_eq!(bus.read(&mut [0]), Err(Error::OutOfBounds));
    assert_eq!(bus.write(&[3, 0, 0, 0]), Err(Error::InvalidState));
    bus.deselect().unwrap();
}

#[test]
fn receive_timeout_preserves_primary_error_and_permanently_stops_transport() {
    let mut bus = engine();
    // Idle, transmit-ready, then three receive-not-ready polls; cleanup idle.
    bus.registers.statuses = [2, 2, 2, 2, 2, 2].into();
    let result = read(&mut bus, 0, &mut [0]);
    assert_eq!(
        result,
        Err(ReadError::Bus {
            phase: uv98_nor_read::Phase::Command,
            source: Error::Timeout(Wait::ReceiveFull),
            deselect_error: None,
        })
    );
    assert_eq!(bus.registers.transmitted, [3]);
    assert_eq!(bus.registers.selections, [true, false]);
    assert_eq!(bus.registers.polls, 6);
    assert_eq!(bus.registers.disabled, 1);
    assert_eq!(bus.select(), Err(Error::Poisoned));
    assert_eq!(bus.registers.transmitted, [3]);
}

#[test]
fn busy_timeout_on_deselect_still_raises_cs_and_disables_spi() {
    let mut bus = engine();
    bus.select().unwrap();
    bus.registers.fallback = 0x80;
    assert_eq!(bus.deselect(), Err(Error::Timeout(Wait::NotBusy)));
    assert_eq!(bus.registers.polls, 4);
    assert_eq!(bus.registers.selections, [true, false]);
    assert_eq!(bus.registers.disabled, 1);
    assert_eq!(bus.select(), Err(Error::Poisoned));
}

#[test]
fn initial_fault_never_asserts_cs_and_cleanup_preserves_second_error() {
    let mut bus = engine();
    bus.registers.fallback = 0x42; // overrun + transmit-empty
    assert_eq!(
        read(&mut bus, 0, &mut [0]),
        Err(ReadError::Bus {
            phase: uv98_nor_read::Phase::Select,
            source: Error::Status(0x42),
            deselect_error: Some(Error::Status(0x42)),
        })
    );
    assert_eq!(bus.registers.selections, [false]);
    assert!(bus.registers.transmitted.is_empty());
    assert_eq!(bus.registers.disabled, 1);
}
