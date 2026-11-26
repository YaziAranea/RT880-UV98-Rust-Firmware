extern crate std;

use super::*;
use core::num::NonZeroU32;
use std::{collections::BTreeMap, vec::Vec};

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Mask,
    Barrier,
    DividerSettle,
    TerminalRfDelay,
    Write(u32, u32),
}

struct Mock {
    words: BTreeMap<u32, u32>,
    events: Vec<Event>,
    context: bool,
    clocks_respond: bool,
    ignore_gpio_clock: bool,
    stuck_high: Option<(u32, u32)>,
    wdt_config_latches: bool,
    wdt_update_stuck_after_write: bool,
}

impl Mock {
    fn new() -> Self {
        let mut words = BTreeMap::new();
        for (address, value) in [
            (PID, 0x700a_3243),
            (SERIES_REVISION, 0x1200),
            (FLASH_SIZE, 256),
            (ICTR, 2),
            (WDT_WINDOW, 0xfff),
            (WDT_RELOAD, 0xfff),
            // Deliberately inherited 150 MHz-like PLL configuration and masters.
            (CRM_CTRL, 0x0300_0003),
            (CRM_CFG, 0x4000_000a),
            (CRM_PLLCFG, 0x104b01),
            (CRM_AHBEN1, DMA_CLOCKS | (1 << 12)),
            (CRM_AHBEN2, USB_CLOCK | 1),
            (CRM_MISC1, 1 << 14),
            (CRM_APB1EN, SPI_CLOCKS),
            (SCB_SCR, 0x16),
            (PWC_LDOOV, 0x10),
        ] {
            words.insert(address, value);
        }
        // Nonzero unowned pin state exposes accidental whole-port operations.
        for port in [GPIOA, GPIOB, GPIOC, GPIOF] {
            for offset in [CFGR, OMODE, ODRVR, PULL, ODT] {
                words.insert(port + offset, 0xaaaa_aaaa);
            }
        }
        Self {
            words,
            events: Vec::new(),
            context: true,
            clocks_respond: true,
            ignore_gpio_clock: false,
            stuck_high: None,
            wdt_config_latches: true,
            wdt_update_stuck_after_write: false,
        }
    }
    fn value(&self, address: u32) -> u32 {
        *self.words.get(&address).unwrap_or(&0)
    }
    fn put(&mut self, address: u32, value: u32) {
        self.words.insert(address, value);
    }
    fn writes(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.events.iter().filter_map(|event| match event {
            Event::Write(address, value) => Some((*address, *value)),
            _ => None,
        })
    }
}

impl RegisterIo for Mock {
    fn mask_interrupts(&mut self) {
        self.events.push(Event::Mask);
    }
    fn entry_context_supported(&mut self) -> bool {
        self.context
    }
    fn read32(&mut self, address: u32) -> u32 {
        self.value(address)
    }
    fn read16(&mut self, address: u32) -> u16 {
        self.value(address) as u16
    }
    fn write32(&mut self, address: u32, value: u32) {
        self.events.push(Event::Write(address, value));
        for port in [GPIOA, GPIOB, GPIOC, GPIOF] {
            if address == port + CLR {
                self.put(port + ODT, self.value(port + ODT) & !value);
            }
            if address == port + SCR {
                self.put(port + ODT, self.value(port + ODT) | value);
            }
            if self
                .stuck_high
                .is_some_and(|(stuck_port, _)| stuck_port == port)
            {
                self.put(
                    port + ODT,
                    self.value(port + ODT) | self.stuck_high.unwrap().1,
                );
            }
        }
        if [WDT_DIV, WDT_RELOAD, WDT_WINDOW].contains(&address) {
            if self.wdt_update_stuck_after_write {
                self.put(WDT_STATUS, 7);
            }
            if !self.wdt_config_latches {
                return;
            }
        }
        let stored = if address == CRM_CTRLSTS {
            (value & !2)
                | if self.clocks_respond && value & 1 != 0 {
                    2
                } else {
                    self.value(address) & 2
                }
        } else if address == CRM_CTRL {
            // Stable flags are read-only; respond to source enable requests.
            let mut next =
                (value & !((1 << 1) | (1 << 25))) | (self.value(address) & ((1 << 1) | (1 << 25)));
            if self.clocks_respond {
                next = (next & !((1 << 1) | (1 << 25)))
                    | ((value & 1) << 1)
                    | ((value & (1 << 24)) << 1);
            }
            next
        } else if address == CRM_CFG {
            let old_status = self.value(address) & 12;
            (value & !12)
                | if self.clocks_respond {
                    (value & 3) << 2
                } else {
                    old_status
                }
        } else if address == CRM_AHBEN1 && self.ignore_gpio_clock {
            value & !GPIO_CLOCKS
        } else {
            value
        };
        self.put(address, stored);
    }
    fn barrier(&mut self) {
        self.events.push(Event::Barrier);
    }
    fn settle_hick_divider(&mut self) {
        self.events.push(Event::DividerSettle);
    }
}

impl TerminalRfTiming for Mock {
    fn terminal_rf_phase_delay(&mut self) {
        self.events.push(Event::TerminalRfDelay);
    }
}

#[test]
fn terminal_rf_a_emits_four_msb_frames_after_gates_without_touching_relay_or_watchdog() {
    let mut io = Mock::new();
    let original = io.words.clone();
    terminal_radio_off_requests(&mut io).unwrap();
    assert!(low_requests_present(&mut io));
    let mut frames = Vec::new();
    let mut bits: Option<(u32, usize)> = None;
    let mut data_high = false;
    let mut clock_high = false;
    let mut phase_seen = false;
    let mut gates = 0u8;
    for event in &io.events {
        match *event {
            Event::TerminalRfDelay => phase_seen = true,
            Event::Write(address, value) => {
                if address == GPIOC + CLR && value == 0x8030 {
                    gates |= 1;
                }
                if address == GPIOB + CLR && value == 0x0800 {
                    gates |= 2;
                }
                if address == GPIOF + CLR && value == 0x0100 {
                    gates |= 4;
                }
                if address == GPIOA + CLR && value == 0x0010 {
                    gates |= 8;
                }
                if address == GPIOA + CLR && value & 0x0100 != 0 {
                    assert_eq!(gates, 15, "RF-A selected before MCU-low requests");
                    assert!(bits.is_none());
                    bits = Some((0, 0));
                }
                if address == GPIOC + SCR && value & 0x0200 != 0 {
                    data_high = true;
                    phase_seen = false;
                }
                if address == GPIOC + CLR && value & 0x0200 != 0 {
                    data_high = false;
                    phase_seen = false;
                }
                if address == GPIOF + SCR && value & 0x0040 != 0 {
                    if let Some((frame, count)) = &mut bits {
                        assert!(!clock_high && phase_seen, "clock rise without setup phase");
                        *frame = (*frame << 1) | u32::from(data_high);
                        *count += 1;
                    }
                    clock_high = true;
                    phase_seen = false;
                }
                if address == GPIOF + CLR && value & 0x0040 != 0 {
                    if clock_high {
                        assert!(phase_seen, "clock fell without high phase");
                    }
                    clock_high = false;
                }
                if address == GPIOA + SCR && value & 0x0100 != 0 {
                    if let Some((frame, count)) = bits.take() {
                        assert_eq!(count, 24);
                        assert!(phase_seen, "CS rose without final high phase");
                        frames.push(frame);
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(frames, [0x330000, 0x36007f, 0x300000, 0x371d00]);
    assert!(bits.is_none());
    assert_eq!(
        io.events
            .iter()
            .filter(|event| **event == Event::TerminalRfDelay)
            .count(),
        392
    );
    let mut chip = BTreeMap::from([
        (0x33u8, 0x00ffu16),
        (0x36, 0xffff),
        (0x30, 0xffff),
        (0x37, 0xffff),
    ]);
    for frame in frames {
        chip.insert((frame >> 16) as u8, frame as u16);
    }
    assert_eq!(chip[&0x33] & 0x0c, 0, "modeled V/U PA bits not cleared");
    assert_eq!(chip[&0x36], 0x007f);
    assert_eq!(chip[&0x30], 0);
    assert_eq!(chip[&0x37], 0x1d00);
    for (port, owned) in [
        (GPIOA, 0x0110u16),
        (GPIOB, 0x0800),
        (GPIOC, 0x83f0),
        (GPIOF, 0x0140),
    ] {
        for offset in [CFGR, OMODE, ODRVR, PULL, ODT] {
            let mask = if offset == OMODE || offset == ODT {
                u32::from(owned)
            } else {
                two_bit_fields(owned, 3)
            };
            assert_eq!(
                io.value(port + offset) & !mask,
                original[&(port + offset)] & !mask
            );
        }
    }
    assert!(io
        .writes()
        .all(|(address, _)| !(WDT_CMD..=WDT_WINDOW).contains(&address)
            && address != WWDT_CTRL
            && address != CRM_CFG
            && address != CRM_PLLCFG));
}

#[test]
fn terminal_rf_rejects_identity_clocks_locks_and_readback_before_serial_select() {
    for (address, value) in [(PID, 0), (SERIES_REVISION, 0x1201), (FLASH_SIZE, 128)] {
        let mut io = Mock::new();
        io.put(address, value);
        assert!(terminal_radio_off_requests(&mut io).is_err());
        assert_eq!(io.events, [Event::Mask]);
    }
    for (port, mask) in [
        (GPIOA, 0x100),
        (GPIOF, 0x40),
        (GPIOC, 0x200),
        (GPIOC, 0x8000),
    ] {
        let mut io = Mock::new();
        io.put(port + LOCK, 0x10000 | mask);
        assert!(terminal_radio_off_requests(&mut io).is_err());
        assert!(!io
            .writes()
            .any(|(address, value)| address == GPIOA + CLR && value & 0x100 != 0));
        assert_eq!(io.events.contains(&Event::TerminalRfDelay), mask != 0x8000);
    }
    for (port, mask) in [(GPIOC, 0x8000), (GPIOC, 0x200), (GPIOF, 0x40)] {
        let mut io = Mock::new();
        io.stuck_high = Some((port, mask));
        assert_eq!(
            terminal_radio_off_requests(&mut io),
            Err(Error::LowRequestReadback)
        );
        assert!(!io
            .writes()
            .any(|(address, value)| address == GPIOA + CLR && value & 0x100 != 0));
        assert_eq!(io.events.contains(&Event::TerminalRfDelay), mask != 0x8000);
    }
    let mut io = Mock::new();
    io.ignore_gpio_clock = true;
    assert_eq!(
        terminal_radio_off_requests(&mut io),
        Err(Error::LowRequestReadback)
    );
    assert!(!io.writes().any(|(address, _)| [GPIOA, GPIOB, GPIOC, GPIOF]
        .iter()
        .any(|port| *port <= address && address < *port + 0x400)));
}

#[test]
fn independent_watchdog_is_explicitly_enabled_only_after_configuration_and_lick() {
    let mut io = Mock::new();
    io.put(CRM_CTRLSTS, 0xbd00_0000);
    arm_independent_watchdog(&mut io, 5).unwrap();
    assert_eq!(io.value(CRM_CTRLSTS) & 3, 3);
    assert_eq!(io.value(CRM_CTRLSTS) & 0xfc00_0000, 0xbc00_0000);
    assert_eq!(io.value(CRM_CTRLSTS) & (1 << 24), 0, "RSTFC command echoed");
    assert_eq!(io.value(WDT_DIV), 3);
    assert_eq!(io.value(WDT_RELOAD), 2499);
    assert_eq!(io.value(WDT_WINDOW), 0xfff);
    assert_eq!(
        32 * (WATCHDOG_RELOAD_VALUE + 1) * 1000 / 40_000,
        WATCHDOG_NOMINAL_TIMEOUT_MS
    );
    let commands: Vec<u32> = io
        .writes()
        .filter(|(address, _)| *address == WDT_CMD)
        .map(|(_, value)| value)
        .collect();
    assert_eq!(commands, [0xaaaa, 0x5555, 0xaaaa, 0xcccc, 0, 0xaaaa]);
    let enabled = io
        .events
        .iter()
        .position(|event| *event == Event::Write(WDT_CMD, 0xcccc))
        .unwrap();
    for event in [
        Event::Write(WDT_DIV, 3),
        Event::Write(WDT_RELOAD, 2499),
        Event::Write(WDT_WINDOW, 0xfff),
    ] {
        assert!(io.events[..enabled].contains(&event));
    }
    // Ordinary feed remains reload-only and never silently re-arms.
    io.events.clear();
    feed_watchdog(&mut io).unwrap();
    assert_eq!(io.events, [Event::Write(WDT_CMD, 0xaaaa)]);
}

#[test]
fn watchdog_arming_rejects_unsupported_context_and_failed_readiness_without_enable() {
    for (address, value) in [(PID, 0), (SERIES_REVISION, 0x1201), (FLASH_SIZE, 128)] {
        let mut io = Mock::new();
        io.put(address, value);
        assert!(arm_independent_watchdog(&mut io, 3).is_err());
        assert_eq!(io.events, [Event::Mask]);
    }
    let mut io = Mock::new();
    io.context = false;
    assert_eq!(
        arm_independent_watchdog(&mut io, 3),
        Err(Error::EntryContext)
    );
    assert_eq!(io.events, [Event::Mask]);
    let mut io = Mock::new();
    assert_eq!(arm_independent_watchdog(&mut io, 0), Err(Error::PollBudget));
    assert_eq!(io.events, [Event::Mask]);
    for case in 0..6 {
        let mut io = Mock::new();
        match case {
            0 => io.clocks_respond = false,
            1 => io.put(WDT_STATUS, 7),
            2 => io.put(WDT_WINDOW, 100),
            3 => {
                io.put(CRM_APB1EN, 1 << 11);
                io.put(WWDT_CTRL, 0x80);
            }
            4 => io.wdt_config_latches = false,
            _ => io.wdt_update_stuck_after_write = true,
        }
        assert!(arm_independent_watchdog(&mut io, 3).is_err(), "case {case}");
        assert!(
            !io.writes()
                .any(|(address, value)| address == WDT_CMD && value == 0xcccc),
            "enabled despite failed precondition case {case}"
        );
        if case >= 4 {
            assert_eq!(io.value(WDT_CMD), 0, "configuration was left unlocked");
        }
    }
}

#[test]
fn rejects_unknown_silicon_before_peripheral_mutation() {
    for (register, value, expected) in [
        (PID, 0x700a_3343, Error::ProductId(0x700a_3343)),
        (SERIES_REVISION, 0x1201, Error::SeriesRevision(0x1201)),
        (FLASH_SIZE, 128, Error::FlashSize(128)),
    ] {
        let mut io = Mock::new();
        io.put(register, value);
        assert_eq!(early_quiesce(&mut io), Err(expected));
        assert_eq!(io.events, [Event::Mask]);
    }
    let mut io = Mock::new();
    io.context = false;
    assert_eq!(early_quiesce(&mut io), Err(Error::EntryContext));
    assert_eq!(io.events, [Event::Mask]);
}

#[test]
fn preinit_disables_pending_irqs_and_holds_bus_masters_in_reset() {
    let mut io = Mock::new();
    early_quiesce(&mut io).unwrap();
    assert_eq!(io.events.first(), Some(&Event::Mask));
    for register in [SYSTICK_CTRL, SYSTICK_RELOAD, SYSTICK_CURRENT] {
        assert_eq!(io.value(register), 0);
    }
    for bank in 0..3 {
        assert_eq!(io.value(NVIC_ICER + bank * 4), u32::MAX);
        assert_eq!(io.value(NVIC_ICPR + bank * 4), u32::MAX);
    }
    assert_eq!(io.value(SCB_ICSR), (1 << 25) | (1 << 27));
    assert_eq!(io.value(SCB_VTOR), APPLICATION_BASE);
    assert_eq!(io.value(SCB_SCR), 0x10);
    assert_eq!(io.value(CRM_AHBEN1) & DMA_CLOCKS, 0);
    assert_eq!(io.value(CRM_AHBEN2), 1);
    assert_eq!(io.value(CRM_AHBRST1), DMA_CLOCKS);
    assert_eq!(io.value(CRM_AHBRST2), USB_CLOCK);
    assert_eq!(io.value(CRM_APB1EN) & SPI_CLOCKS, 0);
    assert_eq!(io.value(CRM_APB1RST) & SPI_CLOCKS, SPI_CLOCKS);
    let nor_cs = io
        .events
        .iter()
        .position(|event| {
            matches!(event,
        Event::Write(a, v) if *a == GPIOB + SCR && *v == (1 << 12))
        })
        .unwrap();
    let spi_stop = io
        .events
        .iter()
        .position(|event| {
            matches!(event,
        Event::Write(a, v) if *a == CRM_APB1EN && *v & SPI_CLOCKS == 0)
        })
        .unwrap();
    assert!(nor_cs < spi_stop);
    assert!(io
        .writes()
        .all(|(address, value)| address != CRM_AHBEN1 || value & DMA_CLOCKS == 0));
}

#[test]
fn pin_takeover_preloads_latches_and_preserves_every_unowned_bit() {
    let mut io = Mock::new();
    let before = io.words.clone();
    early_quiesce(&mut io).unwrap();
    assert!(low_requests_present(&mut io));
    for (port, mask, lows, highs) in [
        (GPIOA, 0x8110u16, 0x0010u32, 0x8100u32),
        (GPIOB, 0x1c07, 0x0800, 0x1407),
        (GPIOC, 0xc0b0, 0x8030, 0x4080),
        (GPIOF, 0x0100, 0x0100, 0),
    ] {
        assert_eq!(io.value(port + ODT) & lows, 0);
        assert_eq!(io.value(port + ODT) & highs, highs);
        for (offset, field_mask) in [
            (ODT, u32::from(mask)),
            (OMODE, u32::from(mask)),
            (CFGR, two_bit_fields(mask, 3)),
            (PULL, two_bit_fields(mask, 3)),
            (ODRVR, two_bit_fields(mask, 3)),
        ] {
            assert_eq!(
                io.value(port + offset) & !field_mask,
                before[&(port + offset)] & !field_mask
            );
        }
        let first_latch = io
            .events
            .iter()
            .position(|event| {
                matches!(event,
            Event::Write(a, _) if *a == port + CLR || *a == port + SCR)
            })
            .unwrap();
        let first_mode = io
            .events
            .iter()
            .position(|event| {
                matches!(event,
            Event::Write(a, _) if *a == port + CFGR)
            })
            .unwrap();
        assert!(first_latch < first_mode);
    }
    assert_eq!(io.value(GPIOB + OMODE) & 0x0407, 0x0407); // released keyboard
    assert_eq!(io.value(GPIOF + CFGR) & 3, before[&(GPIOF + CFGR)] & 3); // PF0 relay
}

#[test]
fn locked_gate_is_rejected_without_gpio_writes() {
    let mut io = Mock::new();
    io.put(GPIOC + LOCK, (1 << 16) | (1 << 15));
    assert_eq!(
        early_quiesce(&mut io),
        Err(Error::PinLocked {
            port: GPIOC,
            mask: 0x8030
        })
    );
    assert!(io
        .writes()
        .all(|(a, _)| !(GPIOA..GPIOF + 0x400).contains(&a)));
}

#[test]
fn inherited_lext_ownership_is_rejected_without_backup_or_gpio_writes() {
    let mut io = Mock::new();
    io.put(CRM_BPDC, 1);
    assert_eq!(
        early_quiesce(&mut io),
        Err(Error::LowSpeedExternalClockActive)
    );
    assert!(io
        .writes()
        .all(|(a, _)| a != CRM_BPDC && !(GPIOA..GPIOF + 0x400).contains(&a)));
    assert!(!low_requests_present(&mut io));
}

#[test]
fn watchdog_rejects_window_updates_short_budget_and_active_wwdt_without_reload() {
    for (address, value, expected) in [
        (WDT_WINDOW, 0xffe, Error::IndependentWatchdogWindow),
        (WDT_STATUS, 1, Error::IndependentWatchdogUpdating),
        (WDT_RELOAD, 1022, Error::IndependentWatchdogBudget),
        (WDT_DIV, 7, Error::IndependentWatchdogBudget),
    ] {
        let mut io = Mock::new();
        io.put(address, value);
        assert_eq!(feed_watchdog(&mut io), Err(expected));
        assert_eq!(io.events.len(), 0);
    }
    let mut io = Mock::new();
    io.put(CRM_APB1EN, 1 << 11);
    io.put(WWDT_CTRL, 0x80);
    assert_eq!(feed_watchdog(&mut io), Err(Error::WindowWatchdogActive));
    assert_eq!(io.events.len(), 0);
}

#[test]
fn watchdog_reload_only_never_starts_or_reconfigures() {
    let mut io = Mock::new();
    io.put(WDT_RELOAD, 1023); // Exactly 4096 input ticks with /4.
    feed_watchdog(&mut io).unwrap();
    assert_eq!(io.events, [Event::Write(WDT_CMD, 0xaaaa)]);
}

#[test]
fn clock_takeover_reaches_nominal_profile_only_after_hick_switch() {
    let mut io = Mock::new();
    early_quiesce(&mut io).unwrap();
    io.events.clear();
    initialize(&mut io, 10).unwrap();
    let profile = uv98_clock_profile::Snapshot {
        ctrl: io.value(CRM_CTRL),
        pllcfg: io.value(CRM_PLLCFG),
        cfg: io.value(CRM_CFG),
        misc1: io.value(CRM_MISC1),
    }
    .stock_profile()
    .unwrap();
    assert_eq!(
        (profile.core_hz(), profile.apb1_hz(), profile.apb2_hz()),
        (48_000_000, 24_000_000, 48_000_000)
    );
    let hick = io
        .events
        .iter()
        .position(|event| {
            matches!(event,
        Event::Write(a, v) if *a == CRM_CFG && *v & 3 == 0)
        })
        .unwrap();
    let pll_off = io
        .events
        .iter()
        .position(|event| {
            matches!(event,
        Event::Write(a, v) if *a == CRM_CTRL && *v & (1 << 24) == 0)
        })
        .unwrap();
    let pll_change = io
        .events
        .iter()
        .position(|event| {
            matches!(event,
        Event::Write(a, _) if *a == CRM_PLLCFG)
        })
        .unwrap();
    assert!(hick < pll_off && pll_off < pll_change);
    assert_eq!(io.value(FLASH_PSR) & 0x157, 0x154);
    assert_eq!(io.value(PWC_LDOOV) & 0x13, 3);
    assert_eq!(io.value(CRM_MISC2) & (0x30 | (7 << 16)), 0);
    assert_eq!(io.value(CRM_APB1RST) & SPI_CLOCKS, 0);
    assert_eq!(io.value(CRM_APB1EN) & SPI_CLOCKS, 0);
    let divide = io
        .events
        .iter()
        .position(|event| {
            matches!(event,
        Event::Write(a, v) if *a == CRM_MISC2 && *v & (7 << 16) == 4 << 16)
        })
        .unwrap();
    let settle = io
        .events
        .iter()
        .position(|event| *event == Event::DividerSettle)
        .unwrap();
    assert!(divide < settle && settle < hick);
    // No flash unlock, control/program/erase or option/protection registers.
    assert!(io
        .writes()
        .all(|(a, _)| !(FLASH_PSR..FLASH_PSR + 0x400).contains(&a) || a == FLASH_PSR));
    assert!(io.writes().all(|(a, v)| a != WDT_CMD || v == 0xaaaa));
}

#[test]
fn clock_wait_is_bounded_and_never_reconfigures_a_running_pll() {
    let mut io = Mock::new();
    early_quiesce(&mut io).unwrap();
    io.events.clear();
    io.clocks_respond = false;
    assert_eq!(
        initialize(&mut io, 3),
        Err(Error::ClockTimeout {
            register: CRM_CFG,
            mask: 12,
            expected: 0,
        })
    );
    assert!(io.writes().all(|(a, _)| a != CRM_PLLCFG));
    assert!(io
        .writes()
        .all(|(a, v)| a != CRM_CTRL || v & (1 << 24) != 0));
    let mut io = Mock::new();
    assert_eq!(initialize(&mut io, 0), Err(Error::PollBudget));
    assert!(io.events.is_empty());
}

#[test]
fn millis_retains_fractional_cycles_without_drift() {
    let mut clock = CycleMillis::new(0, NonZeroU32::new(48_000_000).unwrap()).unwrap();
    for cycles in [1, 21, 12_003, 47_999] {
        assert_eq!(clock.sample(cycles), Ok(0));
    }
    assert_eq!(clock.sample(48_000), Ok(1));
    assert_eq!(clock.sample(96_001), Ok(2));
    let mut odd = CycleMillis::new(0, NonZeroU32::new(1001).unwrap()).unwrap();
    for cycles in 0..1001 {
        assert_eq!(odd.sample(cycles), Ok(cycles * 1000 / 1001));
    }
    assert_eq!(odd.sample(1001), Ok(1000));
}

#[test]
fn millis_handles_cycle_wrap_and_zero_delta() {
    let start = u32::MAX - 23_999;
    let mut clock = CycleMillis::new(start, NonZeroU32::new(48_000_000).unwrap()).unwrap();
    assert_eq!(clock.sample(24_000), Ok(1));
    assert_eq!(clock.sample(24_000), Ok(1));
    assert_eq!(clock.sample(72_000), Ok(2));
}

#[test]
fn millis_rejects_ambiguous_jumps_without_consuming_state() {
    let mut clock = CycleMillis::new(100, NonZeroU32::new(1000).unwrap()).unwrap();
    assert_eq!(
        clock.sample(100 + (1 << 31)),
        Err(CycleTimeError::AmbiguousDelta)
    );
    assert_eq!(clock.sample(99), Err(CycleTimeError::AmbiguousDelta));
    assert_eq!(clock.sample(101), Ok(1));
    assert!(matches!(
        CycleMillis::new(0, NonZeroU32::new(999).unwrap()),
        Err(CycleTimeError::FrequencyTooLow)
    ));
}

#[test]
fn millis_wraps_output_like_embedded_deadline_counters() {
    let mut clock = CycleMillis::new(0, NonZeroU32::new(1000).unwrap()).unwrap();
    clock.milliseconds = u32::MAX - 1;
    assert_eq!(clock.sample(3), Ok(1));
}

#[test]
fn fixed_millis_matches_runtime_rate_at_fractional_and_wrap_boundaries() {
    fn check<const HZ: u32>() {
        let mut cycle = u32::MAX - 47_998;
        let mut reference = CycleMillis::new(cycle, NonZeroU32::new(HZ).unwrap()).unwrap();
        let mut fixed = FixedCycleMillis::<HZ>::new(cycle);
        reference.milliseconds = u32::MAX - 1;
        fixed.milliseconds = u32::MAX - 1;
        for delta in [
            0,
            1,
            HZ / 1000 - 1,
            1,
            HZ / 1000 + 1,
            (1 << 31) - 1,
            1 << 31,
            u32::MAX,
            48_000,
        ] {
            let next = cycle.wrapping_add(delta);
            let expected = reference.sample(next);
            assert_eq!(fixed.sample(next), expected, "at {HZ} Hz, delta {delta}");
            if expected.is_ok() {
                cycle = next;
            }
        }
    }

    check::<1_000>();
    check::<48_000_000>();
    check::<4_294_967_000>();
}
