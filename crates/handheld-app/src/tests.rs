extern crate std;
mod render_batch;
mod shortwave_redraw;

use super::*;
use std::vec;
use std::vec::Vec;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Memory {
    bytes: Vec<u8>,
    address: Option<usize>,
    selected: bool,
    fail_at: Option<usize>,
    reads: usize,
    commands: Vec<u8>,
}
impl Memory {
    fn new() -> Self {
        let mut bytes = vec![0xff; 0x3c000];
        bytes[..4096].copy_from_slice(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ));
        let slot = &mut bytes[0x4000..0x4030];
        slot.fill(0);
        slot[2] = 1;
        slot[6..10].copy_from_slice(&14_550_000u32.to_le_bytes());
        slot[10..14].copy_from_slice(&14_550_000u32.to_le_bytes());
        bytes[0x20a4..0x20b3]
            .copy_from_slice(&[1, 0, 0, 0, 150, 0, 6, 11, 0, 31, 58, 15, 50, 12, 10]);
        Self {
            bytes,
            address: None,
            selected: false,
            fail_at: None,
            reads: 0,
            commands: Vec::new(),
        }
    }
}
impl Bus for Memory {
    type Error = &'static str;
    fn select(&mut self) -> Result<(), Self::Error> {
        assert!(!self.selected);
        self.selected = true;
        self.address = None;
        Ok(())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        assert!(self.selected && self.address.is_none());
        assert_eq!(bytes.len(), 4);
        assert_eq!(bytes[0], 3);
        self.commands.push(bytes[0]);
        self.address =
            Some(((bytes[1] as usize) << 16) | ((bytes[2] as usize) << 8) | bytes[3] as usize);
        Ok(())
    }
    fn read(&mut self, out: &mut [u8]) -> Result<(), Self::Error> {
        let a = self.address.expect("header");
        self.reads += 1;
        if self.fail_at == Some(a) {
            return Err("injected read fault");
        }
        out.copy_from_slice(&self.bytes[a..a + out.len()]);
        self.address = Some(a + out.len());
        Ok(())
    }
    fn deselect(&mut self) -> Result<(), Self::Error> {
        self.selected = false;
        self.address = None;
        Ok(())
    }
}
#[derive(Default)]
struct Panel {
    writes: usize,
    observed_writes: Rc<Cell<usize>>,
    last_command: u8,
    windows: Rc<RefCell<Vec<(u16, u16)>>>,
    capture: Option<Rc<RefCell<CapturedFrame>>>,
}

/// Optional host-only LCD RAM oracle. Ordinary App tests keep no framebuffer.
struct CapturedFrame {
    pixels: Vec<u16>,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
    x: usize,
    y: usize,
    high: Option<u8>,
}
impl CapturedFrame {
    fn new() -> Self {
        Self {
            pixels: vec![0xdead; render::WIDTH * usize::from(render::HEIGHT)],
            x0: 0,
            x1: render::WIDTH - 1,
            y0: 0,
            y1: usize::from(render::HEIGHT) - 1,
            x: 0,
            y: 0,
            high: None,
        }
    }
    fn data(&mut self, command: u8, bytes: &[u8]) {
        if (command == 0x2a || command == 0x2b) && bytes.len() == 4 {
            let start = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
            let end = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
            if command == 0x2a {
                self.x0 = start;
                self.x1 = end;
            } else {
                self.y0 = start;
                self.y1 = end;
            }
        } else if command == 0x2c {
            for &byte in bytes {
                if let Some(high) = self.high.take() {
                    assert!(self.x <= self.x1 && self.x < render::WIDTH);
                    assert!(self.y <= self.y1 && self.y < usize::from(render::HEIGHT));
                    self.pixels[self.y * render::WIDTH + self.x] = u16::from_be_bytes([high, byte]);
                    self.x += 1;
                    if self.x > self.x1 {
                        self.x = self.x0;
                        self.y += 1;
                    }
                } else {
                    self.high = Some(byte);
                }
            }
        }
    }
}
impl DisplayBus for Panel {
    type Error = &'static str;
    fn reset(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.last_command = command;
        if command == 0x2c {
            if let Some(capture) = &self.capture {
                let mut capture = capture.borrow_mut();
                capture.x = capture.x0;
                capture.y = capture.y0;
                capture.high = None;
            }
        }
        self.writes += 1;
        self.observed_writes.set(self.writes);
        Ok(())
    }
    fn data(&mut self, b: &[u8]) -> Result<(), Self::Error> {
        if let Some(capture) = &self.capture {
            capture.borrow_mut().data(self.last_command, b);
        }
        if self.last_command == 0x2b && b.len() == 4 {
            self.windows.borrow_mut().push((
                u16::from_be_bytes([b[0], b[1]]),
                u16::from_be_bytes([b[2], b[3]]) + 1,
            ));
        }
        self.writes += b.len();
        self.observed_writes.set(self.writes);
        Ok(())
    }
}
struct NoDelay;
impl Delay for NoDelay {
    fn delay_ms(&mut self, _: u32) {}
}
fn app(memory: Memory, hash: [u8; 32]) -> App<Memory, Panel, NoDelay> {
    App::new(
        memory,
        Panel::default(),
        NoDelay,
        Timing::new(30, 600).unwrap(),
        hash,
    )
}
#[test]
fn failed_catalog_read_invalidates_ui_and_never_retries_or_writes() {
    let mut memory = Memory::new();
    memory.fail_at = Some(0x4030);
    let before = memory.bytes.clone();
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    assert!(matches!(app.start(0), Err(Error::Catalog(_))));
    assert_eq!(app.status().state, State::Failed);
    assert!(app.snapshot().is_none());
    assert!(matches!(
        app.sample(
            0,
            InputSample::Stable {
                matrix: 0,
                ptt_pressed: false
            }
        ),
        Err(Error::State)
    ));
    let (memory, _) = app.into_parts();
    assert_eq!(memory.bytes, before);
    assert!(!memory.selected);
    assert_eq!(memory.reads, 3);
    assert_eq!(memory.commands, [3, 3, 3]);
}
#[test]
fn failed_calibration_read_does_not_scan_channels_or_claim_match() {
    let mut memory = Memory::new();
    memory.fail_at = Some(0);
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    assert!(matches!(app.start(0), Err(Error::Read(_))));
    assert_eq!(app.status().calibration_matches_backup, None);
    assert_eq!(app.status().state, State::Failed);
    let (memory, _) = app.into_parts();
    assert_eq!(memory.reads, 1);
    assert!(!memory.selected);
}
#[test]
fn mismatched_digest_remains_visible_without_replacing_storage() {
    let memory = Memory::new();
    let before = memory.bytes.clone();
    let mut app = app(memory, [0; 32]);
    app.start(0).unwrap();
    assert_eq!(app.status().calibration_matches_backup, Some(false));
    assert_eq!(app.status().state, State::Ready);
    let (memory, _) = app.into_parts();
    assert_eq!(memory.bytes, before);
    assert!(memory.commands.iter().all(|v| *v == 3));
}
#[test]
fn backwards_clock_is_terminal_and_hides_snapshot() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    app.sample(
        100,
        InputSample::Stable {
            matrix: 0,
            ptt_pressed: false,
        },
    )
    .unwrap();
    assert!(matches!(
        app.sample(
            99,
            InputSample::Stable {
                matrix: 0,
                ptt_pressed: false
            }
        ),
        Err(Error::Clock)
    ));
    assert_eq!(app.status().state, State::Failed);
    assert!(app.snapshot().is_none());
}

#[test]
fn key_page_refreshes_on_invalid_input_and_rearm_but_not_idle() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    event(&mut app, InputEvent::Hold(Key::Digit8));
    assert_eq!(app.status.view.screen, Screen::Keys);
    app.draw().unwrap();
    let release = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    assert!(!app.sample(0, release).unwrap().rendered);
    assert!(app.sample(30, release).unwrap().rendered);
    assert!(app.status().input_ready);
    let reads = app.nor.reads;
    assert!(!app.sample(40, release).unwrap().rendered);
    assert!(app.sample(50, InputSample::Unstable).unwrap().rendered);
    assert!(!app.status().input_ready);
    assert!(!app.sample(60, release).unwrap().rendered);
    assert!(app.sample(90, release).unwrap().rendered);
    assert!(app.status().input_ready);
    assert!(!app.sample(100, release).unwrap().rendered);
    assert_eq!(app.nor.reads, reads);
}

fn add_channel(memory: &mut Memory, index: usize, frequency: u32) {
    let raw = &mut memory.bytes[0x4000 + index * CHANNEL_LEN..0x4000 + (index + 1) * CHANNEL_LEN];
    raw.fill(0);
    raw[2] = 1;
    raw[6..10].copy_from_slice(&frequency.to_le_bytes());
    raw[10..14].copy_from_slice(&frequency.to_le_bytes());
}

fn add_zone(memory: &mut Memory, index: usize, name: &[u8], members: &[u16]) {
    let raw = &mut memory.bytes[ZONES_ADDRESS as usize + index * RECORD_LEN
        ..ZONES_ADDRESS as usize + (index + 1) * RECORD_LEN];
    raw.fill(0xff);
    raw[0] = members.len() as u8;
    raw[1] = members.len() as u8;
    raw[4..4 + name.len()].copy_from_slice(name);
    for (slot, member) in members.iter().enumerate() {
        raw[20 + 2 * slot..22 + 2 * slot].copy_from_slice(&member.to_le_bytes());
    }
}

fn event(app: &mut App<Memory, Panel, NoDelay>, event: InputEvent) -> Update {
    let change = app.controller.handle(event, &app.bitmap);
    app.apply_change(change, None).unwrap()
}

fn menu(app: &mut App<Memory, Panel, NoDelay>, item: controller::MenuItem) -> Update {
    event(app, InputEvent::Hold(Key::Exit));
    event(app, InputEvent::Short(Key::Menu));
    let (group, index) = controller::menu_location(item);
    if let Some(group) = group {
        while controller::menu_entries(None)[app.status().view.menu_index as usize]
            != controller::MenuEntry::Group(group)
        {
            event(app, InputEvent::Short(Key::Down));
        }
        event(app, InputEvent::Short(Key::Menu));
    }
    for _ in 0..index {
        event(app, InputEvent::Short(Key::Down));
    }
    assert_eq!(
        controller::menu_entries(group)[app.status().view.menu_index as usize],
        controller::MenuEntry::Item(item)
    );
    event(app, InputEvent::Short(Key::Menu))
}

#[test]
fn cb_peer_selection_keeps_b_unchanged_and_restores_a_channel_from_nor_read_only() {
    let mut memory = Memory::new();
    add_channel(&mut memory, 1, 43_350_000);
    let original = memory.bytes.clone();
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    app.select_slot(Slot::B).unwrap();
    event(&mut app, InputEvent::Short(Key::Down));
    assert_eq!(app.status.view.channel, Some(1));

    assert!(app
        .controller
        .set_binding(6, controller::ShortcutAction::Cb));
    event(&mut app, InputEvent::Hold(Key::Digit6));
    assert_eq!(app.status.view.screen, Screen::Cb);
    assert_eq!(app.status.view.active_slot, Slot::B);
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.status.view.active_slot, Slot::A);
    assert_eq!(app.status.view.vfo_10hz, 2_713_500);
    let (b, b_channel) = app.peer_radio_selection();
    assert_eq!(b.active_slot, Slot::B);
    assert_eq!(b.cb_for_slot(b.active_slot), None);
    assert_eq!(b_channel.unwrap().rx_frequency_code(), 43_350_000);

    app.select_slot(Slot::B).unwrap();
    let (a, a_channel) = app.peer_radio_selection();
    assert_eq!(a.active_slot, Slot::A);
    assert_eq!(a.cb_for_slot(a.active_slot).unwrap().number(), 15);
    assert!(a_channel.is_none());
    let settings = uv98_radio_runtime::policy::Settings {
        squelch: 1,
        mic_gain: 31,
        fm_volume: [58, 15],
        am_volume: [50, 12],
        squelch_delay: 10,
    };
    let stale_fm = uv98_radio_runtime::policy::Overrides {
        modulation: Some(uv98_rf_sequence::Modulation::Fm),
        ..Default::default()
    };
    let am = uv98_radio_runtime::policy::profile(a, a_channel, settings, stale_fm).unwrap();
    assert!(am.transmit.is_none());
    // Choose FM explicitly in the CB grid, then request upper PTT while B
    // remains selected. This exercises app projection, policy and authority.
    event(&mut app, InputEvent::Hold(Key::Digit6));
    event(&mut app, InputEvent::Short(Key::Star));
    event(&mut app, InputEvent::Short(Key::Menu));
    app.select_slot(Slot::B).unwrap();
    event(&mut app, InputEvent::Ptt(true));
    assert_eq!(app.status.view.active_slot, Slot::B);
    let (a, a_channel) = app.peer_radio_selection();
    let fm = uv98_radio_runtime::policy::profile(
        a,
        a_channel,
        settings,
        uv98_radio_runtime::policy::Overrides {
            modulation: Some(uv98_rf_sequence::Modulation::Am),
            ..Default::default()
        },
    )
    .unwrap();
    let upper = uv98_radio_runtime::ptt::Input {
        upper: true,
        lower: false,
        clean: true,
    };
    let allowed =
        uv98_radio_runtime::tx_policy::authorize_target(app.status.view, a, fm, upper).unwrap();
    assert_eq!(allowed.slot, Slot::A);
    assert_eq!(allowed.transmit.frequency_10hz, 2_713_500);
    assert_eq!(allowed.transmit.tone, 0);
    assert!(allowed.still_held_target(app.status.view, a, fm, upper));
    event(&mut app, InputEvent::Ptt(false));
    event(&mut app, InputEvent::Hold(Key::Digit6));
    event(&mut app, InputEvent::Hold(Key::Exit));
    assert_eq!(app.status.view.active_slot, Slot::B);
    assert_eq!(
        app.selected_channel().unwrap().rx_frequency_code(),
        43_350_000
    );
    let (restored, a_channel) = app.peer_radio_selection();
    assert_eq!(restored.active_slot, Slot::A);
    assert_eq!(restored.cb_for_slot(restored.active_slot), None);
    assert_eq!(a_channel.unwrap().rx_frequency_code(), 14_550_000);
    assert_eq!(app.nor.bytes, original);
}

#[test]
fn stock_config_and_zones_are_read_only_zero_headers_are_empty_and_order_is_preserved() {
    let mut memory = Memory::new();
    add_channel(&mut memory, 2, 43_350_000);
    add_channel(&mut memory, 20, 27_185_00);
    add_zone(&mut memory, 0, b"Local", &[20, 0, 2, 20, 999, 1024]);
    add_zone(&mut memory, 7, b"Empty", &[]);
    // CPS empty slots can be zero-filled. They must never produce channel0.
    memory.bytes[ZONES_ADDRESS as usize + 8 * RECORD_LEN..ZONES_ADDRESS as usize + 9 * RECORD_LEN]
        .fill(0);
    let original = memory.bytes.clone();
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    assert_eq!(
        app.radio_config(),
        Some(RadioConfig {
            squelch: 1,
            mic_gain: 31,
            fm_volume: [58, 15],
            am_volume: [50, 12],
            squelch_delay: 10
        })
    );
    assert_eq!(app.zone_counts[0], 3);
    assert_eq!(app.zone_counts[7], 0);
    assert_eq!(app.zone_bitmap[0], 0x81);
    assert_eq!(app.zone_bitmap[1], 0);
    menu(&mut app, controller::MenuItem::Zones);
    assert_eq!(
        &app.zone_window[..app.zone_window_len],
        &[None, Some(0), Some(7)]
    );
    event(&mut app, InputEvent::Short(Key::Down));
    let selected = event(&mut app, InputEvent::Short(Key::Menu));
    assert!(selected.zone_changed);
    assert_eq!(app.status.view.channel, Some(20));
    assert_eq!(app.next_catalog_index(Some(20), true), Some(0));
    assert_eq!(app.next_catalog_index(Some(20), false), Some(2));
    menu(&mut app, controller::MenuItem::Channels);
    assert_eq!(
        app.list[..app.list_len]
            .iter()
            .map(|record| record.index)
            .collect::<Vec<_>>(),
        [20, 0, 2]
    );
    menu(&mut app, controller::MenuItem::Zones);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.status.view.zone, Some(7));
    assert_eq!(app.status.view.channel, None);
    assert!(app.selected_channel().is_none());
    assert!(app.snapshot().is_none());
    assert!(app.calibration_view().is_some());
    assert_eq!(app.next_catalog_index(None, true), None);
    let (memory, _) = app.into_parts();
    assert_eq!(memory.bytes, original);
    assert!(memory.commands.iter().all(|command| *command == 3));
}

#[test]
fn zone_refresh_failure_hides_all_storage_views_and_cannot_leave_stale_selection() {
    let mut memory = Memory::new();
    add_zone(&mut memory, 0, b"Local", &[0]);
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Zones);
    event(&mut app, InputEvent::Short(Key::Down));
    app.nor.fail_at = Some(ZONES_ADDRESS as usize);
    let change = app
        .controller
        .handle(InputEvent::Short(Key::Menu), &app.bitmap);
    assert!(matches!(
        app.apply_change(change, None),
        Err(Error::Catalog(_))
    ));
    assert_eq!(app.status.state, State::Failed);
    assert!(app.snapshot().is_none());
    assert!(app.selected_channel().is_none());
    assert!(app.calibration_view().is_none());
    assert!(app.radio_config().is_none());
    assert!(matches!(app.render_step(4), Err(Error::State)));
    assert!(!app.nor.selected);
}

#[test]
fn a_b_switch_reload_zones_and_peer_frequency_without_mixing_contexts() {
    let mut memory = Memory::new();
    add_channel(&mut memory, 2, 43_350_000);
    add_zone(&mut memory, 0, b"UHF", &[2]);
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Zones);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.status.view.channel, Some(2));
    app.select_slot(Slot::B).unwrap();
    assert_eq!(app.status.view.zone, None);
    assert_eq!(app.status.view.channel, Some(0));
    assert_eq!(app.peer_view().frequency_10hz, Some(43_350_000));
    event(&mut app, InputEvent::Short(Key::Star));
    event(&mut app, InputEvent::Short(Key::Up));
    let b_frequency = app.status.view.vfo_10hz;
    app.select_slot(Slot::A).unwrap();
    assert_eq!(app.status.view.zone, Some(0));
    assert_eq!(app.status.view.channel, Some(2));
    assert_eq!(app.next_catalog_index(Some(2), true), Some(2));
    assert_eq!(
        app.selected_channel().unwrap().rx_frequency_code(),
        43_350_000
    );
    assert_eq!(app.peer_view().frequency_10hz, Some(b_frequency));
}

#[test]
fn pending_render_does_not_block_input_and_scan_ptt_requires_new_edge() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let release = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    app.sample(0, release).unwrap();
    app.sample(30, release).unwrap();
    let digit3 = InputSample::Stable {
        matrix: 0x100,
        ptt_pressed: false,
    };
    app.sample(100, digit3).unwrap();
    assert_eq!(app.sample(130, digit3).unwrap().request, None);
    assert_eq!(app.status.view.entry, None);
    assert_eq!(
        app.sample(730, digit3).unwrap().request,
        Some(Request::StartScan)
    );
    assert!(app.render_pending());
    assert_eq!(app.render_step(4).unwrap().rows, 4);
    assert!(app.acknowledge_scan_started().unwrap());
    app.sample(800, release).unwrap();
    assert_eq!(app.sample(830, release).unwrap().request, None);
    let lower = InputSample::WithAux {
        matrix: 0,
        ptt_pressed: false,
        aux: Some(Key::Aux16),
    };
    app.sample(900, lower).unwrap();
    let stopped = app.sample(930, lower).unwrap();
    assert_eq!(stopped.request, Some(Request::StopScan));
    assert!(!stopped.ptt_tx_requested);
    app.acknowledge_scan_stopped().unwrap();
    assert!(!app.sample(1500, lower).unwrap().ptt_tx_requested);
    app.sample(1600, release).unwrap();
    app.sample(1630, release).unwrap();
    app.sample(1700, lower).unwrap();
    assert_eq!(app.sample(1730, lower).unwrap().ptt_slot, Some(Slot::B));
    let chord = InputSample::WithAux {
        matrix: 0,
        ptt_pressed: true,
        aux: Some(Key::Aux16),
    };
    app.sample(1740, chord).unwrap();
    assert_eq!(app.status.view.ptt_slot, None);
    assert!(!app.status.input_ready);
    assert_eq!(app.radio_view(), RadioView::OFF);
}

#[test]
fn backend_metadata_damage_is_bounded_and_simulation_never_writes_nor() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let reads = app.nor.reads;
    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(80),
        meter_floor: 70,
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    radio.rssi = Some(90);
    app.set_radio_view(radio).unwrap();
    let mut rows = 0;
    while app.render_pending() {
        let progress = app.render_step(4).unwrap();
        assert!(progress.rows <= 4);
        rows += progress.rows;
    }
    assert_eq!(rows, 7);
    assert_eq!(app.nor.reads, reads);
    app.set_radio_view(radio).unwrap();
    assert!(!app.render_pending());
    assert!(app.nor.commands.iter().all(|command| *command == 3));
    assert!(core::mem::size_of::<App<(), (), ()>>() <= 16 * 1024);
}

#[test]
fn rejected_ptt_redraws_explicit_notice_without_fabricating_a_radio_state() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let receiving = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(80),
        ..RadioView::OFF
    };
    app.set_radio_view(receiving).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    assert!(event(&mut app, InputEvent::Ptt(true)).ptt_tx_requested);
    let reads = app.nor.reads;
    app.reject_tx().unwrap();
    assert_eq!(app.status.view.notice, controller::Notice::TxDisabled);
    assert_eq!(app.status.view.ptt_slot, None);
    assert!(app.status.view.ptt_down);
    assert!(app.render_pending());
    assert_eq!(app.radio_view(), receiving);
    assert_eq!(app.nor.reads, reads);
    assert!(!event(&mut app, InputEvent::Ptt(true)).ptt_tx_requested);
    event(&mut app, InputEvent::Ptt(false));
    assert_eq!(app.status.view.notice, controller::Notice::None);
    assert_eq!(app.radio_view(), receiving);
}

#[test]
fn row_cursor_is_fair_when_earlier_rows_are_continuously_invalidated() {
    let mut pending = PendingRows::default();
    pending.mark(0, render::HEIGHT);
    let mut cursor = 0;
    let mut seen = [false; render::HEIGHT as usize];
    for _ in 0..80 {
        // An adversarial reporter re-queues the first rows before every batch.
        pending.mark(0, 14);
        let start = pending.next(cursor).unwrap();
        let end = (start + 4).min(render::HEIGHT);
        for row in start..end {
            seen[row as usize] = true;
        }
        pending.clear(start, end);
        cursor = if end == render::HEIGHT { 0 } else { end };
    }
    assert!(seen.iter().all(|seen| *seen));
    assert_eq!(pending.next(cursor), Some(0));
    let mut sparse = PendingRows::default();
    sparse.mark(2, 3);
    sparse.mark(31, 33);
    sparse.mark(319, 320);
    assert_eq!(sparse.next(30), Some(31));
    assert_eq!(sparse.next(32), Some(32));
    assert_eq!(sparse.next(33), Some(319));
    sparse.clear(319, 320);
    assert_eq!(sparse.next(300), Some(2));
}

#[test]
fn continuous_50ms_rssi_cannot_starve_a_full_home_pass_or_lose_the_latest_value() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(165),
        meter_floor: 150,
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    app.last_rendered = None;
    app.draw().unwrap();
    let reads = app.nor.reads;
    let mut seen = [false; render::HEIGHT as usize];
    for batch in 0..80 {
        if batch % 5 == 0 {
            radio.rssi = Some(if batch % 10 == 0 { 170 } else { 165 });
            app.set_radio_view(radio).unwrap();
        }
        let start = app.pending_rows.next(app.row_cursor).unwrap();
        let progress = app.render_step(4).unwrap();
        assert_eq!(progress.rows, 4);
        for row in start..start + progress.rows {
            seen[row as usize] = true;
        }
    }
    assert!(seen.iter().all(|seen| *seen));
    assert_eq!(app.radio_view(), radio);
    // Telemetry oscillated back to an earlier value; the union still repairs
    // rows painted with an intermediate value instead of silently dropping it.
    let mut repair_rows = 0;
    while app.render_pending() {
        repair_rows += app.render_step(4).unwrap().rows;
    }
    assert_eq!(repair_rows, 7);
    assert!(!app.deferred_radio_rows.any());
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn menu_and_up_are_not_lost_during_a_partial_home_frame_with_flickering_rssi() {
    let panel = Panel::default();
    let lcd_writes = panel.observed_writes.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(165),
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    app.last_rendered = None;
    app.draw().unwrap();
    app.render_step(4).unwrap();
    assert_eq!(app.row_cursor, 4);
    let before_input = lcd_writes.get();
    let release = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    app.sample(0, release).unwrap();
    app.sample(30, release).unwrap();
    let menu = InputSample::Stable {
        matrix: 0x1000,
        ptt_pressed: false,
    };
    app.sample(100, menu).unwrap();
    app.sample(130, menu).unwrap();
    app.sample(160, release).unwrap();
    assert_eq!(app.sample(190, release).unwrap().pressed, Some(Key::Menu));
    assert_eq!(app.status.view.screen, Screen::Menu);
    assert_eq!(app.row_cursor, 0); // New page paints retained damage from the top.
    let up = InputSample::Stable {
        matrix: 0x2000,
        ptt_pressed: false,
    };
    app.sample(200, up).unwrap();
    assert_eq!(app.sample(230, up).unwrap().pressed, Some(Key::Up));
    assert_eq!(
        app.status.view.menu_index as usize,
        controller::menu_entries(None).len() - 1
    );
    app.sample(260, release).unwrap();
    app.sample(290, release).unwrap();
    assert_eq!(lcd_writes.get(), before_input); // Input never flushes the LCD.
    let required = app.pending_rows;
    let mut seen = [false; render::HEIGHT as usize];
    for batch in 0..80 {
        if !app.render_pending() {
            break;
        }
        let now = 300 + batch * 10;
        if now % 50 == 0 {
            radio.rssi = Some(if now % 100 == 0 { 170 } else { 165 });
            app.set_radio_view(radio).unwrap();
        }
        assert_eq!(app.sample(now, release).unwrap().pressed, None);
        let start = app.pending_rows.next(app.row_cursor).unwrap();
        let progress = app.render_step(4).unwrap();
        for row in start..start + progress.rows {
            seen[row as usize] = true;
        }
    }
    for row in 0..render::HEIGHT {
        assert!(
            !required.has(row) || seen[row as usize],
            "lost pending row {row}"
        );
    }
    assert!(!app.render_pending()); // RSSI has no drawing on the menu.
    assert_eq!(app.status.view.screen, Screen::Menu);
    assert_eq!(app.radio_view(), radio);
    assert_eq!(app.status.presses, 2);
    event(&mut app, InputEvent::Short(Key::Exit));
    assert_eq!(app.status.view.screen, Screen::Home);
    assert_eq!(app.radio_view(), radio); // Latest metadata survives page changes.
    assert_eq!(app.pending_rows.next(app.row_cursor), Some(0));
}

#[test]
fn navigation_keeps_unpainted_unlock_banner_and_deferred_damage_pixel_exact() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();

    event(&mut app, InputEvent::Hold(Key::Hash));
    assert!(app.status.view.locked);
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    event(&mut app, InputEvent::Hold(Key::Hash));
    assert!(!app.status.view.locked);
    app.render_step(4).unwrap(); // Only part of the unlock repaint reached LCD.
    assert!(app.pending_rows.has(10));

    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(165),
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    radio.squelch_open = true;
    radio.rssi = Some(271);
    app.set_radio_view(radio).unwrap();
    assert!(app.deferred_radio_rows.has(6));
    let pending = app.pending_rows;
    let deferred = app.deferred_radio_rows;
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.status.view.screen, Screen::Menu);
    assert!(!app.deferred_radio_rows.any());
    for row in 0..render::HEIGHT {
        assert!(
            !(pending.has(row) || deferred.has(row)) || app.pending_rows.has(row),
            "navigation discarded unpainted row {row}"
        );
    }
    for _ in 0..84 {
        if !app.render_pending() {
            break;
        }
        app.render_step(4).unwrap();
    }
    assert!(!app.render_pending());
    let incremental = capture.borrow().pixels.clone();
    // Force a fresh complete render of the identical final model. This checks
    // actual RGB565 bytes/windows, not only the pending-bit implementation.
    app.last_rendered = None;
    app.draw().unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    assert_eq!(incremental, capture.borrow().pixels);
}

fn assert_matches_complete_frame(
    app: &mut App<Memory, Panel, NoDelay>,
    capture: &Rc<RefCell<CapturedFrame>>,
) {
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    let incremental = capture.borrow().pixels.clone();
    app.last_rendered = None;
    app.draw().unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    assert_eq!(incremental, capture.borrow().pixels);
}

#[test]
fn battery_refresh_is_limited_to_its_rows_and_coalesces_equivalent_readings() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    let reads = app.nor.reads;
    assert_eq!(app.battery_voltage(), None);
    let (start, end) = render::battery_rows();
    let mut previous = capture.borrow().pixels.clone();
    for mv in [Some(8000), Some(7999), Some(6800), None] {
        windows.borrow_mut().clear();
        app.set_battery_voltage(mv).unwrap();
        while app.render_pending() {
            app.render_step(4).unwrap();
        }
        assert_eq!(
            *windows.borrow(),
            [(start, start + 4), (start + 4, start + 8), (start + 8, end)]
        );
        assert_ne!(previous, capture.borrow().pixels);
        assert_matches_complete_frame(&mut app, &capture);
        previous = capture.borrow().pixels.clone();
    }
    app.set_battery_voltage(Some(8020)).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    app.set_battery_voltage(Some(8020)).unwrap();
    app.set_battery_voltage(Some(8049)).unwrap();
    assert_eq!(app.battery_voltage(), Some(8049));
    assert!(!app.render_pending());
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn rapid_menu_steps_with_battery_and_rf_updates_match_full_frame() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    event(&mut app, InputEvent::Short(Key::Menu));
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    event(&mut app, InputEvent::Short(Key::Down));
    let mut rows = 0;
    while app.render_pending() {
        rows += app.render_step(4).unwrap().rows;
    }
    assert_eq!(rows, 15);
    for (index, key) in [
        Key::Down,
        Key::Down,
        Key::Up,
        Key::Up,
        Key::Up,
        Key::Up,
        Key::Down,
    ]
    .into_iter()
    .enumerate()
    {
        event(&mut app, InputEvent::Short(key));
        app.set_battery_voltage(Some(6800 + index as u16 * 300))
            .unwrap();
        app.set_radio_view(RadioView {
            state: render::RadioState::Receiving,
            squelch_open: index % 2 == 0,
            rssi: Some(160 + index as u16 * 5),
            ..RadioView::OFF
        })
        .unwrap();
        app.render_step(3).unwrap();
    }
    assert_matches_complete_frame(&mut app, &capture);
}

#[test]
fn off_focus_tx_badges_preempt_page_work_and_clear_after_tail() {
    let capture = Rc::new(RefCell::new(CapturedFrame::new()));
    let panel = Panel {
        capture: Some(capture.clone()),
        ..Panel::default()
    };
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    let receiving = RadioView {
        state: render::RadioState::Receiving,
        ..RadioView::OFF
    };
    for focus in [Slot::A, Slot::B] {
        let tx_slot = focus.other();
        app.select_slot(focus).unwrap();
        app.set_radio_view(receiving).unwrap();
        while app.render_pending() {
            app.render_step(4).unwrap();
        }
        app.pending_rows.mark(302, 306);
        app.row_cursor = 302;
        windows.borrow_mut().clear();
        app.set_radio_view(RadioView {
            state: render::RadioState::Transmitting,
            tx_slot: Some(tx_slot),
            power: Some(1),
            ..receiving
        })
        .unwrap();
        app.render_step(4).unwrap();
        let start = render::home_slot_rows(tx_slot).0;
        let end = render::home_ptt_badge_rows(tx_slot).1;
        assert_eq!(windows.borrow()[0], (start, start + 4));
        assert!(app.priority_rows.has(end - 1));
        assert_eq!(app.status.view.active_slot, focus);
        assert!(!app.status.view.ptt_down);
        assert_matches_complete_frame(&mut app, &capture);
        app.set_radio_view(receiving).unwrap();
        assert!(app.priority_rows.has(start));
        assert_matches_complete_frame(&mut app, &capture);
    }
}

#[test]
fn radio_control_and_ptt_changes_are_queued_without_waiting_for_telemetry_pass() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        rssi: Some(165),
        meter_floor: 150,
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    app.pending_rows.mark(300, 320);
    radio.rssi = Some(170);
    app.set_radio_view(radio).unwrap();
    assert!(!app
        .pending_rows
        .has(render::home_slot_rows(app.status.view.active_slot).0 + render::HOME_METER_Y));
    assert!(app
        .deferred_radio_rows
        .has(render::home_slot_rows(app.status.view.active_slot).0 + render::HOME_METER_Y));
    radio.state = render::RadioState::Transmitting; // Explicit simulated backend report.
    radio.tx_slot = Some(app.status.view.active_slot);
    app.set_radio_view(radio).unwrap();
    assert!(app.pending_rows.has(0));
    assert!(app
        .pending_rows
        .has(render::home_slot_rows(app.status.view.active_slot).0 + render::HOME_METER_Y));
    assert!(!app.deferred_radio_rows.any());
    event(&mut app, InputEvent::Ptt(true));
    app.reject_tx().unwrap();
    assert_eq!(app.status.view.notice, controller::Notice::TxDisabled);
    assert!(app.pending_rows.has(0));
}

#[test]
fn actual_lcd_banner_preempts_an_old_page_tail_without_a_fake_tx_state() {
    let panel = Panel::default();
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    app.last_rendered = None;
    app.draw().unwrap();
    app.row_cursor = 300; // A previous pass was painting the footer.
    let reads = app.nor.reads;
    windows.borrow_mut().clear();
    app.set_radio_view(RadioView {
        state: render::RadioState::Starting,
        ..RadioView::OFF
    })
    .unwrap();
    for _ in 0..18 {
        assert!(app.render_step(2).unwrap().rows <= 2);
    }
    let drawn = windows.borrow();
    assert_eq!(drawn[0], (0, 2));
    let mut banner = [false; 27];
    for &(start, end) in drawn.iter() {
        for row in start..end.min(27) {
            banner[row as usize] = true;
        }
    }
    assert!(banner.iter().all(|row| *row)); // <= 18 small batches, not 160.
    assert!(drawn.iter().any(|&(start, _)| start >= 300));
    assert_eq!(app.radio_view().state, render::RadioState::Starting);
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn continuously_replaced_banner_cannot_starve_footer_or_consume_rssi_priority() {
    let panel = Panel::default();
    let windows = panel.windows.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    app.pending_rows.mark(300, 320);
    app.row_cursor = 300;
    windows.borrow_mut().clear();
    let reads = app.nor.reads;
    for batch in 0..50 {
        app.set_radio_view(RadioView {
            state: if batch % 2 == 0 {
                render::RadioState::Starting
            } else {
                render::RadioState::Receiving
            },
            rssi: Some(165 + batch % 2),
            ..RadioView::OFF
        })
        .unwrap();
        assert!(app.render_step(2).unwrap().rows <= 2);
    }
    let mut footer = [false; 20];
    for &(start, end) in windows.borrow().iter() {
        for row in start.max(300)..end.min(320) {
            footer[(row - 300) as usize] = true;
        }
    }
    assert!(footer.iter().all(|row| *row));
    assert_eq!(app.nor.reads, reads);
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    app.pending_rows.mark(300, 320);
    let mut radio = app.radio_view();
    radio.rssi = Some(170);
    radio.squelch_open = !radio.squelch_open;
    app.set_radio_view(radio).unwrap();
    assert!(!app.priority_rows.any()); // Even SQL banner updates are coalesced.
    assert!(app.deferred_radio_rows.has(0));
}

#[test]
fn ptt_and_menu_input_do_not_flush_lcd_and_exit_discards_old_priority_work() {
    let panel = Panel::default();
    let writes = panel.observed_writes.clone();
    let mut app = App::new(
        Memory::new(),
        panel,
        NoDelay,
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).unwrap();
    let before = writes.get();
    let released = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    for now in [0, 30] {
        app.sample(now, released).unwrap();
    }
    let ptt = InputSample::Stable {
        matrix: 0,
        ptt_pressed: true,
    };
    app.sample(100, ptt).unwrap();
    assert!(app.sample(130, ptt).unwrap().ptt_tx_requested);
    assert_eq!(app.priority_rows.next(0), Some(0));
    assert!((0..27).all(|row| app.priority_rows.has(row)));
    assert!(!app.priority_rows.has(27));
    assert_eq!(app.radio_view(), RadioView::OFF); // Input is not TX acknowledgement.
    app.sample(160, released).unwrap();
    app.sample(190, released).unwrap();
    let menu = InputSample::Stable {
        matrix: 0x1000,
        ptt_pressed: false,
    };
    app.sample(200, menu).unwrap();
    app.sample(230, menu).unwrap();
    app.sample(260, released).unwrap();
    assert_eq!(app.sample(290, released).unwrap().pressed, Some(Key::Menu));
    assert_eq!(app.status.view.screen, Screen::Menu);
    assert!(!app.priority_rows.any());
    assert_eq!(app.row_cursor, 0);
    app.set_radio_view(RadioView {
        state: render::RadioState::Starting,
        ..RadioView::OFF
    })
    .unwrap();
    assert!(app.priority_rows.any());
    let exit = InputSample::Stable {
        matrix: 0x8000,
        ptt_pressed: false,
    };
    app.sample(300, exit).unwrap();
    app.sample(330, exit).unwrap();
    app.sample(360, released).unwrap();
    let update = app.sample(390, released).unwrap();
    assert_eq!(update.pressed, Some(Key::Exit));
    assert!(update.cancel_temporary);
    assert_eq!(app.status.view.screen, Screen::Home);
    assert!(!app.priority_rows.any());
    assert_eq!(writes.get(), before);
}

#[test]
fn confirmed_radio_choices_update_without_erasing_pending_setting_or_queued_page() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        power: Some(2),
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    assert_eq!(menu(&mut app, controller::MenuItem::Power).request, None);
    assert_eq!(app.status.view.screen, Screen::Setting);
    assert_eq!(app.status.view.setting_index, 2);
    assert_eq!(event(&mut app, InputEvent::Short(Key::Down)).request, None);
    assert_eq!(app.status.view.setting_index, 0);
    app.render_step(4).unwrap();
    let pending = app.pending_rows.0;
    let reads = app.nor.reads;
    radio.power = Some(1);
    app.set_radio_view(radio).unwrap();
    assert_eq!(app.status.view.setting_index, 0);
    for (old, new) in pending.iter().zip(app.pending_rows.0.iter()) {
        assert_eq!(old & new, *old);
    }
    assert_eq!(
        event(&mut app, InputEvent::Short(Key::Menu)).request,
        Some(Request::SetPower(0))
    );
    assert_eq!(app.radio_view().power, Some(1));
    assert_eq!(app.status.view.screen, Screen::Menu);
    assert_eq!(event(&mut app, InputEvent::Short(Key::Menu)).request, None);
    assert_eq!(app.status.view.setting_index, 1); // Still the confirmed baseline.
    assert_eq!(event(&mut app, InputEvent::Short(Key::Exit)).request, None);
    assert_eq!(app.radio_view().power, Some(1));
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn unavailable_backend_explicit_off_fm_and_power_commit_desired_state_without_claiming_success() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    // The runtime can retain overrides after tuning becomes unavailable. Its
    // unavailable report cannot prove that old monitor/AM overrides are off.
    app.set_radio_view(RadioView {
        state: render::RadioState::Receiving,
        monitor: true,
        modulation: render::Modulation::Am,
        power: Some(2),
        ..RadioView::OFF
    })
    .unwrap();
    let unavailable = RadioView {
        state: render::RadioState::Unavailable,
        ..RadioView::OFF
    };
    app.set_radio_view(unavailable).unwrap();
    let reads = app.nor.reads;
    for (item, request) in [
        (controller::MenuItem::Monitor, Request::SetMonitor(false)),
        (
            controller::MenuItem::Modulation,
            Request::SetModulation { am: false },
        ),
        (controller::MenuItem::Power, Request::SetPower(0)),
    ] {
        assert_eq!(menu(&mut app, item).request, None);
        assert_eq!(app.status.view.setting_index, 0);
        assert_eq!(
            event(&mut app, InputEvent::Short(Key::Menu)).request,
            Some(request)
        );
        assert_eq!(app.radio_view(), unavailable);
        assert_eq!(event(&mut app, InputEvent::Short(Key::Menu)).request, None);
        assert_eq!(
            event(&mut app, InputEvent::Short(Key::Menu)).request,
            Some(request)
        );
        assert_eq!(app.radio_view(), unavailable); // Repeated confirmation stays idempotent.
    }
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn exit_cleanup_survives_scan_request_and_backend_confirmation_is_explicit() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let mut radio = RadioView {
        state: render::RadioState::Receiving,
        monitor: true,
        power: Some(2),
        modulation: render::Modulation::Am,
        ..RadioView::OFF
    };
    app.set_radio_view(radio).unwrap();
    event(&mut app, InputEvent::Hold(Key::Digit3));
    assert!(app.acknowledge_scan_started().unwrap());
    let context_a = app.controller.context(Slot::A);
    let context_b = app.controller.context(Slot::B);
    let reads = app.nor.reads;
    let exit = event(&mut app, InputEvent::Short(Key::Exit));
    assert!(exit.cancel_temporary);
    assert_eq!(exit.request, Some(Request::StopScan));
    assert_eq!(app.status.view.scan, controller::ScanState::Stopping);
    assert_eq!(app.radio_view(), radio); // No optimistic MON-off claim.
    app.acknowledge_scan_stopped().unwrap();
    radio.monitor = false;
    app.set_radio_view(radio).unwrap();
    assert!(!app.radio_view().monitor);
    assert_eq!(app.controller.context(Slot::A), context_a);
    assert_eq!(app.controller.context(Slot::B), context_b);
    assert_eq!(
        (app.radio_view().power, app.radio_view().modulation),
        (Some(2), render::Modulation::Am)
    );
    let release = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    for now in [0, 30, 100] {
        assert!(!app.sample(now, release).unwrap().cancel_temporary);
    }
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn menu_and_list_cursors_reuse_ram_windows_and_page_changes_do_not_replan_the_frame() {
    let mut memory = Memory::new();
    for index in 1..14 {
        add_channel(&mut memory, index, 14_550_000 + index as u32 * 1250);
    }
    for index in 0..10 {
        add_zone(&mut memory, index, b"Zone", &[index as u16]);
    }
    let original = memory.bytes.clone();
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let reads = app.nor.reads;
    event(&mut app, InputEvent::Short(Key::Menu));
    for _ in 0..controller::MENU_ITEMS.len() * 2 {
        event(&mut app, InputEvent::Short(Key::Down));
    }
    assert_eq!(app.nor.reads, reads);
    menu(&mut app, controller::MenuItem::Channels);
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    let reads = app.nor.reads;
    for _ in 0..6 {
        event(&mut app, InputEvent::Short(Key::Down));
    }
    assert_eq!(app.nor.reads, reads);
    assert_eq!(app.status.view.list_channel, Some(6));
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    event(&mut app, InputEvent::Short(Key::Down));
    assert_eq!(app.status.view.list_channel, Some(7));
    assert_eq!(app.list_anchor, Some(7));
    assert_eq!(app.nor.reads - reads, 7);
    assert!(!app.pending_rows.has(0));
    assert!(!app.pending_rows.has(0));
    assert!(!app.pending_rows.has(300));
    assert!(app.pending_rows.has(54));
    menu(&mut app, controller::MenuItem::Zones);
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    let reads = app.nor.reads;
    for _ in 0..6 {
        event(&mut app, InputEvent::Short(Key::Down));
    }
    assert_eq!(app.nor.reads, reads);
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    event(&mut app, InputEvent::Short(Key::Down));
    assert_eq!(app.status.view.zone_cursor, Some(6));
    assert_eq!(app.nor.reads, reads);
    assert!(!app.pending_rows.has(0));
    assert!(!app.pending_rows.has(300));
    assert_eq!(app.nor.bytes, original);
    assert!(app.nor.commands.iter().all(|command| *command == 3));
}

#[test]
fn independent_slot_names_zones_and_modes_are_cached_and_lighting_is_global() {
    let mut memory = Memory::new();
    add_channel(&mut memory, 2, 43_350_000);
    let name = &mut memory.bytes[0x4000 + 2 * CHANNEL_LEN + 32..0x4000 + 3 * CHANNEL_LEN];
    name.fill(0xff);
    name[..5].copy_from_slice(b"LOCAL");
    add_zone(&mut memory, 0, b"UHF", &[2]);
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Zones);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    app.select_slot(Slot::B).unwrap();
    assert_eq!(app.peer_view().mode, Mode::Channel);
    assert_eq!(app.peer_view().channel, Some(2));
    assert_eq!(&app.peer_view().name[..5], b"LOCAL");
    assert_eq!(app.peer_view().zone, Some(0));
    assert_eq!(&app.peer_view().zone_name.unwrap()[..3], b"UHF");
    event(&mut app, InputEvent::Short(Key::Star));
    event(&mut app, InputEvent::Short(Key::Up));
    let b = app.controller.context(Slot::B);
    app.select_slot(Slot::A).unwrap();
    assert_eq!(app.status.view.mode, Mode::Channel);
    assert_eq!(app.status.view.zone, Some(0));
    assert_eq!(app.peer_view().mode, Mode::Vfo);
    assert_eq!(app.peer_view().frequency_10hz, Some(b.vfo_10hz));
    assert_eq!(app.peer_view().zone, None);
    assert!(app.peer_view().name.is_empty());
    let reads = app.nor.reads;
    menu(&mut app, controller::MenuItem::DisplayLight);
    event(&mut app, InputEvent::Short(Key::Down));
    let request = event(&mut app, InputEvent::Short(Key::Menu)).request;
    assert_eq!(request, Some(Request::SetDisplayLight(LightMode::Seconds5)));
    assert_eq!(app.status.view.display_light, LightMode::On);
    app.set_lighting_modes(LightMode::Seconds5, LightMode::Off)
        .unwrap();
    assert_eq!(
        (app.status.view.display_light, app.status.view.key_light),
        (LightMode::Seconds5, LightMode::Off)
    );
    assert_eq!(app.nor.reads, reads);
    app.select_slot(Slot::B).unwrap();
    assert_eq!(app.controller.context(Slot::B), b);
    assert_eq!(
        (app.status.view.display_light, app.status.view.key_light),
        (LightMode::Seconds5, LightMode::Off)
    );
}

#[test]
fn app_confirms_pending_listen_on_the_restored_bin_not_a_moved_cursor() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    assert_eq!(
        menu(&mut app, controller::MenuItem::Spectrum).request,
        Some(Request::StartSpectrum)
    );
    event(&mut app, InputEvent::Short(Key::Digit2)); // 81-point test grid
    let mut snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_200_000,
        center_10hz: 43_250_000,
        step_10hz: 1_250,
        len: 81,
        plot: uv98_spectrum::compact::Plot::EMPTY,
    };
    snapshot.plot.valid[5] = 1;
    assert!(app.set_spectrum(snapshot).unwrap());
    let bin = app.status.view.spectrum_cursor;
    let opened = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(opened.request, None);
    let requested = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(requested.request, Some(Request::ListenSpectrum { bin }));
    assert_eq!(app.status.view.spectrum_pending_listen, Some(bin));
    assert!(!app.status.view.spectrum_listening);
    event(&mut app, InputEvent::Short(Key::Down));
    assert_eq!(app.status.view.spectrum_cursor, bin);
    // Real app callback used by main after the scope confirmed this bin.
    app.confirm_spectrum_listen(bin).unwrap();
    assert_eq!(app.status.view.spectrum_cursor, bin);
    assert!(app.status.view.spectrum_listening);
    assert_eq!(app.status.view.spectrum_pending_listen, None);
    let view = app.spectrum_view().unwrap();
    assert_eq!(
        view.frequency_10hz(bin),
        Some(43_250_000),
        "displayed Listen frequency must be the confirmed RF bin"
    );
}

#[test]
fn app_scope_grid_identity_rejects_stale_bin_after_restart_and_backlog() {
    use uv98_radio_runtime::{scope::Scope, Io, Runtime, State as RadioState};
    use uv98_rf_sequence::RxRequest;
    use uv98_spectrum::Window;
    struct ScopeIo {
        words: [u16; 128],
    }
    impl Io for ScopeIo {
        type Error = &'static str;
        fn read(&mut self, register: u8) -> Result<u16, Self::Error> {
            Ok(self.words[register as usize])
        }
        fn write(&mut self, register: u8, value: u16) -> Result<(), Self::Error> {
            self.words[register as usize] = value;
            Ok(())
        }
        fn mute(&mut self, _muted: bool) -> Result<(), Self::Error> {
            Ok(())
        }
        fn antenna(&mut self, _high: bool) -> Result<(), Self::Error> {
            Ok(())
        }
        fn hf_gates(&mut self, _high: bool, _b: bool) -> Result<(), Self::Error> {
            Ok(())
        }
        fn indicators(&mut self, _rx: bool, _tx: bool) -> Result<(), Self::Error> {
            Ok(())
        }
        fn backlights(&mut self, _display: bool, _keys: bool) -> Result<(), Self::Error> {
            Ok(())
        }
        fn verify_control(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    let calibration = uv98_calibration::Calibration::parse(include_bytes!(
        "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
    ))
    .unwrap();
    let request = RxRequest {
        frequency_10hz: 14_550_000,
        width: uv98_rf_sequence::Width::Wide,
        level: 1,
        tone: 0,
        tone_mode: 0,
        option_nibble: 0,
        squelch_delay: 10,
        modulation: uv98_rf_sequence::Modulation::Fm,
    };
    let mut radio = Runtime::new(ScopeIo { words: [0; 128] });
    let mut now = 0;
    radio.cold_start(&calibration, now).unwrap();
    for _ in 0..400 {
        now += 2;
        radio.tick(now, false).unwrap();
        if radio.state() != RadioState::Configuring {
            break;
        }
    }
    radio.receive(&calibration, request, now).unwrap();
    for _ in 0..400 {
        now += 2;
        radio.tick(now, false).unwrap();
        if radio.state() == RadioState::Receiving {
            break;
        }
    }
    assert_eq!(radio.state(), RadioState::Receiving);

    let wide = Window::centered(14_550_000, 1_000_000, 12_500).unwrap();
    let mut scope = Scope::new();
    scope
        .start_window(&radio, &calibration, request, wide, now)
        .unwrap();
    for _ in 0..2000 {
        now += 2;
        radio.tick(now, false).unwrap();
        scope.step(&mut radio, &calibration, || now).unwrap();
        if scope.plot().valid(0) {
            break;
        }
    }
    assert!(scope.plot().valid(0), "wide grid bin0 must be measured");

    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    assert_eq!(
        menu(&mut app, controller::MenuItem::Spectrum).request,
        Some(Request::StartSpectrum)
    );
    let old = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: scope.generation(),
        start_10hz: wide.start_10hz(),
        center_10hz: wide.center_10hz(),
        step_10hz: wide.step_10hz(),
        len: wide.len() as u8,
        plot: *scope.plot(),
    };
    assert!(app.set_spectrum(old).unwrap());
    for _ in 0..40 {
        event(&mut app, InputEvent::Short(Key::Up));
    }
    assert_eq!(app.spectrum_view().unwrap().start_10hz, 14_050_000);
    assert_eq!(app.status.view.spectrum_cursor, 0);
    // Keep an old frame on screen: one row drained, many still pending.
    app.render_step(1).unwrap();

    // Real Scope restart to a narrower grid; old app snapshot stays published.
    scope.event(now, uv98_spectrum::compact::Event::Stop);
    let mut restored = false;
    for _ in 0..2000 {
        now += 2;
        radio.tick(now, false).unwrap();
        if let uv98_radio_runtime::scope::Completion::Restored { .. } =
            scope.step(&mut radio, &calibration, || now).unwrap()
        {
            restored = true;
            break;
        }
    }
    assert!(restored, "old sweep must restore before restart");
    let narrow = Window::centered(14_550_000, 500_000, 12_500).unwrap();
    if radio.state() != RadioState::Receiving {
        radio.receive(&calibration, request, now).unwrap();
        for _ in 0..400 {
            now += 2;
            radio.tick(now, false).unwrap();
            if radio.state() == RadioState::Receiving {
                break;
            }
        }
    }
    scope
        .start_window(&radio, &calibration, request, narrow, now)
        .unwrap();
    for _ in 0..2000 {
        now += 2;
        radio.tick(now, false).unwrap();
        scope.step(&mut radio, &calibration, || now).unwrap();
        if scope.plot().valid(0) {
            break;
        }
    }
    assert!(scope.plot().valid(0), "narrow grid bin0 must be measured");
    let newer = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: scope.generation(),
        start_10hz: narrow.start_10hz(),
        center_10hz: narrow.center_10hz(),
        step_10hz: narrow.step_10hz(),
        len: narrow.len() as u8,
        plot: *scope.plot(),
    };
    assert!(
        !app.set_spectrum(newer).unwrap(),
        "old frame must win backlog"
    );
    let displayed = app.spectrum_view().unwrap();
    assert_eq!(displayed.window_id, old.window_id);
    let published = uv98_radio_runtime::spectrum_flow::PublishedWindow {
        generation: displayed.window_id,
        start_10hz: displayed.start_10hz,
        step_10hz: displayed.step_10hz,
        len: displayed.len,
    };
    assert_eq!(
        uv98_radio_runtime::spectrum_flow::bind_bin(
            Some(published),
            scope.window(),
            scope.generation(),
            0,
        ),
        Err(uv98_radio_runtime::spectrum_flow::SelectionError::StaleWindow)
    );
    assert_ne!(narrow.start_10hz(), wide.start_10hz());
    assert_eq!(narrow.start_10hz(), 14_300_000);

    while app.render_pending() {
        app.render_step(32).unwrap();
    }
    assert!(app.set_spectrum(newer).unwrap());
    let displayed = app.spectrum_view().unwrap();
    let accepted = uv98_radio_runtime::spectrum_flow::PublishedWindow {
        generation: displayed.window_id,
        start_10hz: displayed.start_10hz,
        step_10hz: displayed.step_10hz,
        len: displayed.len,
    };
    assert_eq!(
        uv98_radio_runtime::spectrum_flow::bind_bin(
            Some(accepted),
            scope.window(),
            scope.generation(),
            0,
        ),
        Ok(14_300_000)
    );
}

#[test]
fn app_error_before_listen_ack_clears_pending_without_audio() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    assert_eq!(
        menu(&mut app, controller::MenuItem::Spectrum).request,
        Some(Request::StartSpectrum)
    );
    let snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_200_000,
        center_10hz: 43_250_000,
        step_10hz: 1_250,
        len: 81,
        plot: uv98_spectrum::compact::Plot::EMPTY,
    };
    assert!(app.set_spectrum(snapshot).unwrap());
    let bin = app.status.view.spectrum_cursor;
    event(&mut app, InputEvent::Short(Key::Menu));
    let requested = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(requested.request, Some(Request::ListenSpectrum { bin }));
    app.cancel_spectrum_pending().unwrap();
    app.note_spectrum_error(SpectrumError::Restore).unwrap();
    assert_eq!(app.status.view.spectrum_pending_listen, None);
    assert!(!app.status.view.spectrum_listening);
}

#[test]
fn codex_listen_confirmation_after_external_slot_change_is_refused() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    assert_eq!(
        menu(&mut app, controller::MenuItem::Spectrum).request,
        Some(Request::StartSpectrum)
    );
    let bin = app.status.view.spectrum_cursor;
    event(&mut app, InputEvent::Short(Key::Menu));
    let requested = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(requested.request, Some(Request::ListenSpectrum { bin }));
    assert_eq!(app.status.view.spectrum_pending_listen, Some(bin));

    // Public coordinator API can change context independently of the normal
    // keyboard event path. A late callback must not authorize audio on Home.
    app.select_slot(controller::Slot::B).unwrap();
    assert_eq!(app.status.view.screen, Screen::Home);
    assert!(!app.confirm_spectrum_listen(bin).unwrap());
    assert!(!app.status.view.spectrum_listening);
    assert_eq!(app.status.view.spectrum_pending_listen, None);
}

#[test]
fn spectrum_publications_stay_immutable_until_rows_drain_and_never_read_nor() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    assert_eq!(
        menu(&mut app, controller::MenuItem::Spectrum).request,
        Some(Request::StartSpectrum)
    );
    event(&mut app, InputEvent::Short(Key::Digit2)); // 81-point test grid
    let reads = app.nor.reads;
    let old_context = app.controller.context(Slot::A);
    let mut snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_200_000,
        center_10hz: 43_250_000,
        step_10hz: 1_250,
        len: 81,
        plot: uv98_spectrum::compact::Plot::EMPTY,
    };
    snapshot.plot.latest[40] = 165;
    snapshot.plot.valid[5] = 1;
    assert!(app.set_spectrum(snapshot).unwrap());
    app.render_step(2).unwrap();
    let mut latest = snapshot;
    latest.plot.latest[40] = 170;
    assert!(!app.set_spectrum(latest).unwrap());
    assert_eq!(app.spectrum_view().unwrap().plot.latest[40], 165);
    event(&mut app, InputEvent::Short(Key::Down));
    assert_eq!(app.status.view.spectrum_cursor, 41);
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    assert!(app.set_spectrum(latest).unwrap());
    assert_eq!(app.spectrum_view().unwrap().plot.latest[40], 170);
    let opened = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(opened.request, None);
    assert!(app.status.view.spectrum_menu);
    let requested = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(requested.request, Some(Request::ListenSpectrum { bin: 41 }));
    assert_eq!(app.controller.context(Slot::A), old_context);
    assert_eq!(app.status.view.screen, Screen::Spectrum);
    // Listen leaves the MR/VFO context alone; explicit accept is separate.
    event(&mut app, InputEvent::Short(Key::Menu));
    event(&mut app, InputEvent::Short(Key::Down));
    let accepted = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(accepted.request, Some(Request::AcceptSpectrum { bin: 41 }));
    app.accept_spectrum_frequency(43_251_250).unwrap();
    assert_eq!(app.status.view.mode, Mode::Vfo);
    assert_eq!(app.status.view.screen, Screen::Home);
    assert_eq!(app.status.view.vfo_10hz, 43_251_250);
    app.clear_spectrum().unwrap();
    assert!(app.spectrum_view().is_none());
    assert_eq!(app.nor.reads, reads);
}

#[test]
fn nudging_from_the_edge_keeps_the_cursor_at_the_new_edge() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Spectrum);
    let mut snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_200_000,
        center_10hz: 43_205_000,
        step_10hz: 2_500,
        len: 5,
        plot: uv98_spectrum::compact::Plot::EMPTY,
    };
    assert!(app.set_spectrum(snapshot).unwrap());
    for expected in [3, 4] {
        assert_eq!(event(&mut app, InputEvent::Short(Key::Down)).request, None);
        assert_eq!(app.status.view.spectrum_cursor, expected);
    }
    let right = Some(Request::PanSpectrum {
        forward: true,
        fine: true,
    });
    assert_eq!(event(&mut app, InputEvent::Short(Key::Down)).request, right);
    assert_eq!(
        event(&mut app, InputEvent::Repeat(Key::Down)).request,
        right
    );
    while app.render_pending() {
        app.render_step(8).unwrap();
    }
    snapshot.window_id += 1;
    snapshot.start_10hz += 5_000;
    snapshot.center_10hz += 5_000;
    assert!(app.set_spectrum_with_cursor(snapshot, Some(4)).unwrap());
    assert_eq!(
        app.spectrum_view().unwrap().frequency_10hz(4),
        Some(43_215_000)
    );
    assert_eq!(app.status.view.spectrum_cursor, 4);
    assert_eq!(event(&mut app, InputEvent::Short(Key::Down)).request, right);
    for _ in 0..4 {
        assert_eq!(event(&mut app, InputEvent::Short(Key::Up)).request, None);
    }
    assert_eq!(
        event(&mut app, InputEvent::Short(Key::Up)).request,
        Some(Request::PanSpectrum {
            forward: false,
            fine: true
        }),
    );
}

#[test]
fn spectrum_window_change_keeps_cursor_frequency() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Spectrum);
    event(&mut app, InputEvent::Short(Key::Digit2)); // 81-point test grid
    let first = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_247_500,
        center_10hz: 43_297_500,
        step_10hz: 1_250,
        len: 81,
        plot: uv98_spectrum::compact::Plot::EMPTY,
    };
    assert!(app.set_spectrum(first).unwrap());
    for _ in 0..20 {
        event(&mut app, InputEvent::Short(Key::Down));
    }
    assert_eq!(app.status.view.spectrum_cursor, 60);
    let frequency = 43_247_500 + 60 * 1_250;
    while app.render_pending() {
        app.render_step(4).unwrap();
    }
    // Same centre, wider step: 81 -> 41 points. Keep the same frequency.
    let second = SpectrumSnapshot {
        step_10hz: 2_500,
        len: 41,
        ..first
    };
    assert!(app.set_spectrum(second).unwrap());
    let index = u32::from(app.status.view.spectrum_cursor);
    assert_eq!(
        second.start_10hz + index * second.step_10hz,
        frequency,
        "cursor keeps its frequency when the window grid changes"
    );
}

#[test]
fn rejected_spectrum_start_keeps_reason_and_window_menu_without_stale_plot() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Spectrum);
    let mut snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        window_id: 1,
        start_10hz: 43_200_000,
        center_10hz: 43_250_000,
        step_10hz: 1_250,
        len: 81,
        plot: uv98_spectrum::compact::Plot::EMPTY,
    };
    snapshot.plot.valid[5] = 1;
    snapshot.plot.latest[40] = 200;
    assert!(app.set_spectrum(snapshot).unwrap());
    app.clear_spectrum().unwrap();
    app.reject_spectrum_start(render::SpectrumError::Window)
        .unwrap();
    assert!(app.spectrum_view().is_none(), "old plot must not return");
    assert_eq!(app.status.view.screen, Screen::Spectrum);
    assert_eq!(
        app.status.view.notice,
        controller::Notice::SpectrumUnavailable
    );

    // The overlay is usable before any measurement: widen the default narrow window.
    let opened = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(opened.request, None);
    assert!(app.status.view.spectrum_menu);
    event(&mut app, InputEvent::Short(Key::Down)); // ACCEPT VFO
    event(&mut app, InputEvent::Short(Key::Down)); // SPAN WIDER
    let restart = event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(restart.request, Some(Request::RestartSpectrum));
    assert!(!app.status.view.spectrum_menu);
}

#[test]
fn physical_long_zero_opens_spectrum_without_digit_and_short_zero_still_edits_vfo() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let reads = app.nor.reads;
    let released = InputSample::Stable {
        matrix: 0,
        ptt_pressed: false,
    };
    let zero = InputSample::Stable {
        matrix: 0x0080,
        ptt_pressed: false,
    };
    app.sample(0, released).unwrap();
    app.sample(30, released).unwrap();
    assert_eq!(app.sample(100, zero).unwrap().request, None);
    assert_eq!(app.sample(130, zero).unwrap().request, None);
    assert_eq!(app.status.view.entry, None);
    assert_eq!(
        app.sample(730, zero).unwrap().request,
        Some(Request::StartSpectrum)
    );
    assert_eq!(app.status.view.screen, Screen::Spectrum);
    assert_eq!(app.status.view.entry, None);
    app.sample(800, released).unwrap();
    assert_eq!(app.sample(830, released).unwrap().pressed, None);
    event(&mut app, InputEvent::Hold(Key::Exit));
    event(&mut app, InputEvent::Short(Key::Star));
    event(&mut app, InputEvent::Short(Key::Digit1));
    app.sample(900, zero).unwrap();
    app.sample(930, zero).unwrap();
    assert_eq!(app.status.view.entry.unwrap().value, 1);
    app.sample(960, released).unwrap();
    let short = app.sample(990, released).unwrap();
    assert_eq!(short.pressed, Some(Key::Digit0));
    assert_eq!(short.request, None);
    assert_eq!(app.status.view.entry.unwrap().value, 10);
    assert_eq!(app.status.view.entry.unwrap().digits, 2);
    assert_eq!(app.nor.reads, reads);
}
