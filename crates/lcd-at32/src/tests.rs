extern crate std;
use crate::{
    engine::{Engine, Registers},
    BusTiming, Config, Divider, Error, GpioRegisters, Mode, TimingPhase, Wait,
};
use core::num::NonZeroU32;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, vec, vec::Vec};
use uv98_display::DisplayBus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Status(u32),
    Tx(u8),
    Cs(bool),
    Dc(bool),
    Reset(bool),
    Disable,
    Timing(TimingPhase),
}

struct Fake {
    statuses: VecDeque<u32>,
    fallback: u32,
    events: Rc<RefCell<Vec<Event>>>,
}
struct Timing(Rc<RefCell<Vec<Event>>>);
impl BusTiming for Timing {
    fn wait(&mut self, phase: TimingPhase) {
        self.0.borrow_mut().push(Event::Timing(phase));
    }
}
impl Registers for Fake {
    fn status(&mut self) -> u32 {
        let value = self.statuses.pop_front().unwrap_or(self.fallback);
        self.events.borrow_mut().push(Event::Status(value));
        value
    }
    fn transmit(&mut self, byte: u8) {
        self.events.borrow_mut().push(Event::Tx(byte));
    }
    fn chip_select(&mut self, active: bool) {
        self.events.borrow_mut().push(Event::Cs(active));
    }
    fn data_mode(&mut self, data: bool) {
        self.events.borrow_mut().push(Event::Dc(data));
    }
    fn reset_level(&mut self, high: bool) {
        self.events.borrow_mut().push(Event::Reset(high));
    }
    fn disable(&mut self) {
        self.events.borrow_mut().push(Event::Disable);
    }
}
fn engine() -> Engine<Fake, Timing> {
    engine_with_budget(3)
}
fn engine_with_budget(polls: u32) -> Engine<Fake, Timing> {
    let events = Rc::new(RefCell::new(Vec::new()));
    Engine::new(
        Fake {
            statuses: VecDeque::new(),
            fallback: 2,
            events: events.clone(),
        },
        Timing(events),
        NonZeroU32::new(polls).unwrap(),
    )
}
fn registers(seed: u32) -> GpioRegisters {
    GpioRegisters {
        cfgr: seed,
        omode: seed,
        odrvr: seed,
        pull: seed,
        muxl: seed,
        muxh: seed,
    }
}
fn words(gpio: GpioRegisters) -> [u32; 6] {
    [
        gpio.cfgr, gpio.omode, gpio.odrvr, gpio.pull, gpio.muxl, gpio.muxh,
    ]
}

#[test]
fn pin_words_match_machine_oracle_with_documented_manual_cs_difference() {
    for line in include_str!("../tests/fixtures/stock-lcd-init.csv")
        .lines()
        .skip(1)
    {
        let row: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let seed = row[0];
        assert_eq!(words(registers(seed).with_spi3()), row[7..13]);
        assert_eq!(words(registers(seed).with_control()), row[13..19]);
        let a = registers(seed).with_cs();
        assert_eq!(a.cfgr ^ row[1], 3 << 30); // output01 replaces AF10
        assert_eq!([a.omode, a.odrvr, a.pull, a.muxl], row[2..6]);
        assert_eq!(a.muxh, seed); // manual CS never alters unused PA15 AF field
        assert_eq!(row[6], (seed & 0x0FFF_FFFF) | 0x6000_0000);
        assert_eq!(row[19..], [0xC046, 4, 0]);
        assert_eq!(Divider::Div2.ctrl1(), row[19] | 0x300);
    }
}

#[test]
fn all_unowned_pin_bits_are_preserved() {
    for seed in [0, u32::MAX, 0xA5A5_A5A5, 0x1234_5678] {
        let a = registers(seed).with_cs();
        let b = registers(seed).with_spi3();
        let c = registers(seed).with_control();
        for (gpio, fields, pins) in [
            (a, 0xC000_0000, 0x8000),
            (b, 0xCC0, 0x28),
            (c, 0x3C00_0000, 0x6000),
        ] {
            assert_eq!(gpio.cfgr & !fields, seed & !fields);
            assert_eq!(gpio.odrvr & !fields, seed & !fields);
            assert_eq!(gpio.pull & !fields, seed & !fields);
            assert_eq!(gpio.omode & !pins, seed & !pins);
        }
        assert_eq!(b.muxl & !0x00F0_F000, seed & !0x00F0_F000);
        assert_eq!(
            (a.muxl, a.muxh, b.muxh, c.muxl, c.muxh),
            (seed, seed, seed, seed, seed)
        );
    }
}

#[test]
fn divisors_only_change_baud_bits() {
    for (i, div) in [
        Divider::Div2,
        Divider::Div4,
        Divider::Div8,
        Divider::Div16,
        Divider::Div32,
        Divider::Div64,
        Divider::Div128,
        Divider::Div256,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(div.divisor(), 2 << i);
        assert_eq!(div.ctrl1(), 0xC346 | ((i as u32) << 3));
        let mode2 = Config {
            divider: div,
            mode: Mode::Mode2,
            poll_budget: NonZeroU32::new(3).unwrap(),
        };
        let mode0 = Config {
            mode: Mode::Mode0,
            ..mode2
        };
        assert_eq!(mode2.ctrl1(), div.ctrl1());
        assert_eq!(mode0.ctrl1(), 0xC344 | ((i as u32) << 3));
        assert_eq!(mode2.ctrl1() ^ mode0.ctrl1(), 2);
    }
}

#[test]
fn command_completes_before_dc_cs_change_and_calls_timing_in_order() {
    let mut bus = engine();
    bus.command(0x2A).unwrap();
    assert_eq!(
        *bus.registers.events.borrow(),
        vec![
            Event::Status(2),
            Event::Dc(false),
            Event::Cs(true),
            Event::Timing(TimingPhase::Setup),
            Event::Status(2),
            Event::Tx(0x2A),
            Event::Status(2),
            Event::Status(2),
            Event::Timing(TimingPhase::Hold),
            Event::Cs(false),
            Event::Dc(true),
            Event::Timing(TimingPhase::Inactive),
        ]
    );
}

#[test]
fn data_keeps_cs_selected_for_all_bytes_without_receive_wait() {
    let mut bus = engine();
    bus.data(&[0x12, 0x34, 0x56]).unwrap();
    let events = bus.registers.events.borrow();
    assert_eq!(
        events
            .iter()
            .filter_map(|e| if let Event::Tx(v) = e { Some(*v) } else { None })
            .collect::<Vec<_>>(),
        [0x12, 0x34, 0x56]
    );
    assert_eq!(
        events
            .iter()
            .filter_map(|e| if let Event::Cs(v) = e { Some(*v) } else { None })
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(!events.contains(&Event::Dc(false)));
    assert!(!events.contains(&Event::Disable));
    // RDBF never becomes1: transmit-only mode must not wait for reception.
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Status(_)))
            .count(),
        6
    );
}

#[test]
fn empty_data_is_inert_and_healthy_transactions_can_repeat() {
    let mut bus = engine();
    bus.data(&[]).unwrap();
    assert!(bus.registers.events.borrow().is_empty());
    bus.command(0x2A).unwrap();
    bus.data(&[0, 0, 1, 0x3F]).unwrap();
    bus.command(0x2B).unwrap();
    assert_eq!(
        bus.registers
            .events
            .borrow()
            .iter()
            .filter(|e| **e == Event::Cs(true))
            .count(),
        3
    );
}

#[test]
fn transmit_timeout_is_finite_releases_cs_and_terminally_poisoned() {
    let mut bus = engine();
    bus.registers.fallback = 0;
    assert_eq!(bus.command(0x11), Err(Error::Timeout(Wait::TransmitEmpty)));
    let log = bus.registers.events.borrow().clone();
    assert_eq!(
        log.iter().filter(|e| matches!(e, Event::Status(_))).count(),
        4
    );
    assert!(!log.iter().any(|e| matches!(e, Event::Tx(_))));
    assert_eq!(
        log[log.len() - 4..],
        [
            Event::Cs(false),
            Event::Disable,
            Event::Dc(true),
            Event::Timing(TimingPhase::Inactive)
        ]
    );
    assert_eq!(bus.command(0x11), Err(Error::Poisoned));
    assert_eq!(bus.data(&[]), Err(Error::Poisoned));
    assert_eq!(bus.reset(false), Err(Error::Poisoned));
    assert_eq!(*bus.registers.events.borrow(), log);
}

#[test]
fn initial_busy_timeout_does_not_assert_cs_or_transmit() {
    let mut bus = engine();
    bus.registers.fallback = 0x82;
    assert_eq!(bus.data(&[1]), Err(Error::Timeout(Wait::NotBusy)));
    let events = bus.registers.events.borrow();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Status(_)))
            .count(),
        3
    );
    assert!(!events.contains(&Event::Cs(true)));
    assert!(!events.iter().any(|e| matches!(e, Event::Tx(_))));
    assert!(events.contains(&Event::Disable));
}

#[test]
fn final_busy_timeout_does_not_report_a_successful_command() {
    let mut bus = engine();
    bus.registers.statuses = [2, 2, 2, 0x82, 0x82, 0x82].into();
    assert_eq!(bus.command(0x29), Err(Error::Timeout(Wait::NotBusy)));
    let events = bus.registers.events.borrow();
    assert!(events.contains(&Event::Tx(0x29)));
    assert!(!events.contains(&Event::Timing(TimingPhase::Hold)));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Status(_)))
            .count(),
        6
    );
    assert!(events.contains(&Event::Cs(false)));
}

#[test]
fn fault_bits_beat_ready_flags_and_cleanup_preserves_original_status() {
    for fault in [8, 16, 32, 64] {
        let mut bus = engine();
        bus.registers.fallback = 2 | fault;
        assert_eq!(bus.command(0), Err(Error::Status(2 | fault)));
        assert!(!bus
            .registers
            .events
            .borrow()
            .iter()
            .any(|e| matches!(e, Event::Tx(_))));
        assert_eq!(bus.command(1), Err(Error::Poisoned));
    }
}

#[test]
fn reset_only_sets_the_line_after_idle_and_preserves_spi_until_drop() {
    let mut bus = engine();
    bus.reset(false).unwrap();
    bus.reset(true).unwrap();
    let events = bus.registers.events.borrow();
    assert_eq!(
        *events,
        vec![
            Event::Status(2),
            Event::Cs(false),
            Event::Dc(true),
            Event::Reset(false),
            Event::Timing(TimingPhase::Inactive),
            Event::Status(2),
            Event::Cs(false),
            Event::Dc(true),
            Event::Reset(true),
            Event::Timing(TimingPhase::Inactive)
        ]
    );
}

#[test]
fn drop_releases_cs_and_disables_without_an_untimed_reset_pulse() {
    let bus = engine();
    let events = bus.registers.events.clone();
    drop(bus);
    assert_eq!(
        *events.borrow(),
        [Event::Cs(false), Event::Disable, Event::Dc(true)]
    );
}

#[test]
fn direct_rgb565_row_is_one_selected_msb_first_transaction() {
    for len in [1, 31, 32, 33, 239, 240, 320] {
        let mut bus = engine();
        let pixels: Vec<u16> = (0..len)
            .map(|index| (index as u16).wrapping_mul(0x91a7))
            .collect();
        bus.pixels_rgb565(&pixels).unwrap();
        let events = bus.registers.events.borrow();
        let bytes: Vec<u8> = events
            .iter()
            .filter_map(|event| match event {
                Event::Tx(byte) => Some(*byte),
                _ => None,
            })
            .collect();
        assert_eq!(
            bytes,
            pixels
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == Event::Cs(true))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == Event::Cs(false))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Timing(_)))
                .count(),
            3
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Status(_)))
                .count(),
            len * 2 + 3
        );
        assert!(!events.contains(&Event::Dc(false)));
        assert!(!events.contains(&Event::Disable));
    }
    let mut bus = engine();
    bus.pixels_rgb565(&[]).unwrap();
    assert!(bus.registers.events.borrow().is_empty());
}

#[test]
fn eight_rows_preserve_wire_bytes_with_thirteen_instead_of_sixty_nine_transactions() {
    fn window(bus: &mut Engine<Fake, Timing>) {
        bus.command(0x2a).unwrap();
        bus.data(&[0, 0, 0, 239]).unwrap();
        bus.command(0x2b).unwrap();
        bus.data(&[0, 54, 0, 61]).unwrap();
        bus.command(0x2c).unwrap();
    }
    let pixels: Vec<u16> = (0..240).map(|i| (i as u16).wrapping_mul(0x1023)).collect();
    let mut old = engine();
    let mut new = engine();
    window(&mut old);
    window(&mut new);
    for _ in 0..8 {
        for chunk in pixels.chunks(32) {
            let bytes: Vec<u8> = chunk.iter().flat_map(|word| word.to_be_bytes()).collect();
            old.data(&bytes).unwrap();
        }
        new.pixels_rgb565(&pixels).unwrap();
    }
    let old = old.registers.events.borrow();
    let new = new.registers.events.borrow();
    let bytes = |events: &[Event]| {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Tx(byte) => Some(*byte),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(bytes(&old), bytes(&new));
    assert_eq!(bytes(&new).len(), 8 * 240 * 2 + 11);
    assert_eq!(
        old.iter()
            .filter(|event| **event == Event::Cs(true))
            .count(),
        69
    );
    assert_eq!(
        new.iter()
            .filter(|event| **event == Event::Cs(true))
            .count(),
        13
    );
    assert_eq!(
        old.iter()
            .filter(|event| matches!(event, Event::Timing(_)))
            .count(),
        207
    );
    assert_eq!(
        new.iter()
            .filter(|event| matches!(event, Event::Timing(_)))
            .count(),
        39
    );
}

#[test]
fn faults_before_every_row_byte_and_final_completion_abort_and_poison() {
    let pixels = [0x12ab; 240];
    // Initial idle, each byte's TDBE, final TDBE and final BUSY read.
    for fault_at in 0..483 {
        let mut bus = engine();
        bus.registers.statuses = core::iter::repeat_n(2, fault_at)
            .chain(core::iter::once(10))
            .collect();
        assert_eq!(bus.pixels_rgb565(&pixels), Err(Error::Status(10)));
        let log = bus.registers.events.borrow().clone();
        assert_eq!(
            log.iter()
                .filter(|event| matches!(event, Event::Tx(_)))
                .count(),
            fault_at.saturating_sub(1).min(480)
        );
        assert_eq!(
            &log[log.len() - 4..],
            &[
                Event::Cs(false),
                Event::Disable,
                Event::Dc(true),
                Event::Timing(TimingPhase::Inactive)
            ]
        );
        assert_eq!(bus.pixels_rgb565(&[]), Err(Error::Poisoned));
        assert_eq!(bus.data(&[0]), Err(Error::Poisoned));
        assert_eq!(*bus.registers.events.borrow(), log);
    }
}

#[test]
fn pixel_byte_retries_spend_only_the_remaining_budget() {
    let mut single_poll = engine_with_budget(1);
    single_poll.registers.statuses = [2, 2, 0, 2].into();
    assert_eq!(
        single_poll.pixels_rgb565(&[0x12ab]),
        Err(Error::Timeout(Wait::TransmitEmpty))
    );
    assert_eq!(single_poll.registers.statuses, VecDeque::from([2]));
    assert!(single_poll
        .registers
        .events
        .borrow()
        .contains(&Event::Tx(0x12)));
    assert!(!single_poll
        .registers
        .events
        .borrow()
        .contains(&Event::Tx(0xab)));
    assert_eq!(single_poll.pixels_rgb565(&[0x12ab]), Err(Error::Poisoned));

    let mut ready_on_last_poll = engine();
    ready_on_last_poll.registers.statuses = [2, 0, 0, 2, 2, 2, 2].into();
    ready_on_last_poll.pixels_rgb565(&[0x12ab]).unwrap();
    assert!(ready_on_last_poll.registers.statuses.is_empty());

    let mut fault_on_retry = engine();
    fault_on_retry.registers.statuses = [2, 0, 10, 2].into();
    assert_eq!(
        fault_on_retry.pixels_rgb565(&[0x12ab]),
        Err(Error::Status(10))
    );
    assert_eq!(fault_on_retry.registers.statuses, VecDeque::from([2]));
    assert!(!fault_on_retry
        .registers
        .events
        .borrow()
        .iter()
        .any(|event| matches!(event, Event::Tx(_))));
}

#[test]
fn row_timeouts_stay_bounded_and_never_skip_final_busy_wait() {
    for written in [0, 1, 63, 64, 239, 479] {
        let mut bus = engine();
        bus.registers.statuses = core::iter::repeat_n(2, written + 1).collect();
        bus.registers.fallback = 0;
        assert_eq!(
            bus.pixels_rgb565(&[0xabcd; 240]),
            Err(Error::Timeout(Wait::TransmitEmpty))
        );
        let events = bus.registers.events.borrow();
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Tx(_))).count(),
            written
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Status(_)))
                .count(),
            written + 4
        );
    }
    let mut bus = engine();
    bus.registers.statuses = core::iter::repeat_n(2, 482).collect();
    bus.registers.fallback = 0x82;
    assert_eq!(
        bus.pixels_rgb565(&[0xabcd; 240]),
        Err(Error::Timeout(Wait::NotBusy))
    );
    let events = bus.registers.events.borrow();
    assert_eq!(
        events.iter().filter(|e| matches!(e, Event::Tx(_))).count(),
        480
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Status(_)))
            .count(),
        485
    );
    assert!(!events.contains(&Event::Timing(TimingPhase::Hold)));
}
