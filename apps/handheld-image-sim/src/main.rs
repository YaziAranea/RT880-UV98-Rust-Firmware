//! Host-only protocol simulation. No serial, GPIO or RF backend is linked.
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
};
use uv98_diag_app::PRESERVED_CALIBRATION_SHA256;
use uv98_display::{Delay, DisplayBus};
use uv98_handheld_app::{App, InputSample, SpectrumSnapshot, State, Update};
use uv98_handheld_ui::controller::{
    menu_entries, menu_location, LightMode, MenuEntry, MenuGroup, MenuItem, Mode, Request,
    ScanState, Screen, Setting, ShortcutAction, SideAction, Slot,
};
use uv98_handheld_ui::render::{
    Modulation, RadioRoute, RadioState, RadioView, SpectrumState, HEIGHT, WIDTH,
};
use uv98_keys::{Key, Timing};
use uv98_nor_read::Bus;

struct Memory {
    data: Vec<u8>,
    address: Option<usize>,
    selected: bool,
    reads: Vec<(usize, usize)>,
}
impl Bus for Memory {
    type Error = &'static str;
    fn select(&mut self) -> Result<(), Self::Error> {
        if self.selected {
            return Err("nested CS");
        }
        self.selected = true;
        self.address = None;
        Ok(())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        if !self.selected || self.address.is_some() || bytes.len() != 4 || bytes[0] != 3 {
            return Err("not a single READ03 header");
        }
        self.address =
            Some((bytes[1] as usize) << 16 | (bytes[2] as usize) << 8 | bytes[3] as usize);
        Ok(())
    }
    fn read(&mut self, out: &mut [u8]) -> Result<(), Self::Error> {
        if !self.selected {
            return Err("CS not selected");
        }
        let a = self.address.ok_or("missing command")?;
        let end = a.checked_add(out.len()).ok_or("overflow")?;
        out.copy_from_slice(self.data.get(a..end).ok_or("range")?);
        self.reads.push((a, out.len()));
        self.address = Some(end);
        Ok(())
    }
    fn deselect(&mut self) -> Result<(), Self::Error> {
        self.selected = false;
        self.address = None;
        Ok(())
    }
}

/// Minimal sink for canonical CASET/PASET/RAMWR. This does not simulate panel
/// timing, analog behavior, physical orientation or ST7789 SPI2EN wiring.
struct Panel {
    pixels: Vec<u16>,
    command: u8,
    x: [u16; 2],
    y: [u16; 2],
    cursor: usize,
    commands: usize,
    pixel_words: usize,
    reset_high: bool,
    madctl: Option<u8>,
    madctl_writes: usize,
}
impl Panel {
    fn new() -> Self {
        Self {
            pixels: vec![0; WIDTH as usize * HEIGHT as usize],
            command: 0,
            x: [0, WIDTH as u16 - 1],
            y: [0, HEIGHT - 1],
            cursor: 0,
            commands: 0,
            pixel_words: 0,
            reset_high: false,
            madctl: None,
            madctl_writes: 0,
        }
    }
    fn save_png(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        let mut rgb = Vec::with_capacity(self.pixels.len() * 3);
        for &v in &self.pixels {
            rgb.extend_from_slice(&[
                ((v >> 11) as u32 * 255 / 31) as u8,
                (((v >> 5) & 63) as u32 * 255 / 63) as u8,
                ((v & 31) as u32 * 255 / 31) as u8,
            ]);
        }
        writer.write_image_data(&rgb)?;
        Ok(())
    }
}
impl DisplayBus for Panel {
    type Error = &'static str;
    fn reset(&mut self, high: bool) -> Result<(), Self::Error> {
        self.reset_high = high;
        if !high {
            self.command = 0;
            self.cursor = 0;
            self.madctl = None;
        }
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        if !self.reset_high {
            return Err("command during reset");
        }
        if matches!(command, 0x2a | 0x2b | 0x2c) && self.madctl != Some(0x08) {
            return Err("portrait MADCTL08 must precede window/pixel commands");
        }
        self.command = command;
        self.commands += 1;
        if command == 0x2c {
            self.cursor = 0;
        }
        Ok(())
    }
    fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        match self.command {
            0x36 => {
                if data != [0x08] {
                    return Err("expected actual portrait MADCTL08 command data");
                }
                self.madctl = Some(data[0]);
                self.madctl_writes += 1;
            }
            0x2a | 0x2b => {
                if data.len() != 4 {
                    return Err("window needs four bytes");
                }
                let limits = [
                    u16::from_be_bytes([data[0], data[1]]),
                    u16::from_be_bytes([data[2], data[3]]),
                ];
                let maximum = if self.command == 0x2a {
                    WIDTH as u16
                } else {
                    HEIGHT
                };
                if limits[0] > limits[1] || limits[1] >= maximum {
                    return Err("window range");
                }
                if self.command == 0x2a {
                    self.x = limits;
                } else {
                    self.y = limits;
                }
            }
            0x2c => {
                if data.len() % 2 != 0 {
                    return Err("partial pixel");
                }
                let width = (self.x[1] - self.x[0] + 1) as usize;
                let height = (self.y[1] - self.y[0] + 1) as usize;
                if self.cursor + data.len() / 2 > width * height {
                    return Err("pixel overrun");
                }
                for p in data.chunks_exact(2) {
                    let x = self.x[0] as usize + self.cursor % width;
                    let y = self.y[0] as usize + self.cursor / width;
                    self.pixels[y * WIDTH as usize + x] = u16::from_be_bytes([p[0], p[1]]);
                    self.cursor += 1;
                    self.pixel_words += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
struct ObservedPanel(Rc<RefCell<Panel>>);
impl DisplayBus for ObservedPanel {
    type Error = &'static str;
    fn reset(&mut self, high: bool) -> Result<(), Self::Error> {
        self.0.borrow_mut().reset(high)
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.0.borrow_mut().command(command)
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.borrow_mut().data(bytes)
    }
}
struct SimDelay {
    milliseconds: Rc<Cell<u64>>,
}
impl Delay for SimDelay {
    fn delay_ms(&mut self, ms: u32) {
        self.milliseconds.set(self.milliseconds.get() + ms as u64);
    }
}

type SimApp = App<Memory, ObservedPanel, SimDelay>;

fn sample(app: &mut SimApp, now: &mut u32, matrix: u16, ptt: bool) -> Update {
    let update = app
        .sample(
            *now,
            InputSample::Stable {
                matrix,
                ptt_pressed: ptt,
            },
        )
        .expect("synthetic input must be accepted");
    // Match the cooperative runtime: input first, then at most four LCD rows.
    let progress = app.render_step(4).expect("bounded render slice");
    assert!(progress.rows <= 4);
    *now += 40;
    update
}

fn drain(app: &mut SimApp) -> usize {
    let mut steps = 0;
    while app.render_pending() {
        let progress = app.render_step(4).expect("drain queued LCD rows");
        assert!(progress.rows > 0 && progress.rows <= 4);
        steps += 1;
        assert!(steps <= 320, "renderer failed to make bounded progress");
    }
    steps
}

fn side_sample(app: &mut SimApp, now: &mut u32, lower: bool) -> Update {
    let update = app
        .sample(
            *now,
            InputSample::WithAux {
                matrix: 0,
                ptt_pressed: false,
                aux: if lower { Some(Key::Aux16) } else { None },
            },
        )
        .expect("synthetic normalized side input");
    let progress = app.render_step(4).expect("bounded side-input render");
    assert!(progress.rows <= 4);
    *now += 40;
    update
}

fn release(app: &mut SimApp, now: &mut u32) {
    sample(app, now, 0, false);
    sample(app, now, 0, false);
}

fn raw(key: Key) -> (u16, bool) {
    (
        match key {
            Key::Digit0 => 0x0080,
            Key::Digit1 => 0x0001,
            Key::Digit2 => 0x0010,
            Key::Digit3 => 0x0100,
            Key::Digit4 => 0x0002,
            Key::Digit5 => 0x0020,
            Key::Digit6 => 0x0200,
            Key::Digit7 => 0x0004,
            Key::Digit8 => 0x0040,
            Key::Digit9 => 0x0400,
            Key::Menu => 0x1000,
            Key::Up => 0x2000,
            Key::Down => 0x4000,
            Key::Exit => 0x8000,
            Key::Star => 0x0008,
            Key::Hash => 0x0800,
            Key::Ptt => 0,
            _ => panic!("not a matrix/PTT key"),
        },
        key == Key::Ptt,
    )
}

fn tap(app: &mut SimApp, now: &mut u32, key: Key) {
    let _ = tap_request(app, now, key);
}

fn tap_request(app: &mut SimApp, now: &mut u32, key: Key) -> Option<Request> {
    let (matrix, ptt) = raw(key);
    let mut request = None;
    let mut cancelled_temporary = false;
    // Capture requests on press AND release. In particular MENU confirms only
    // on its debounced release; a helper must not silently discard that event.
    for (bits, down) in [
        (0, false),
        (0, false),
        (matrix, ptt),
        (matrix, ptt),
        (0, false),
        (0, false),
    ] {
        let update = sample(app, now, bits, down);
        cancelled_temporary |= update.cancel_temporary;
        if let Some(next) = update.request {
            assert!(
                request.replace(next).is_none(),
                "one tap emitted multiple requests"
            );
        }
    }
    if key == Key::Exit {
        assert!(
            cancelled_temporary,
            "EXIT did not cancel temporary operations"
        );
    }
    request
}

fn hold(app: &mut SimApp, now: &mut u32, key: Key) {
    let _ = hold_request(app, now, key);
}

fn hold_request(app: &mut SimApp, now: &mut u32, key: Key) -> Option<Request> {
    release(app, now);
    let (matrix, ptt) = raw(key);
    sample(app, now, matrix, ptt);
    sample(app, now, matrix, ptt);
    *now += 650;
    let update = sample(app, now, matrix, ptt);
    release(app, now);
    update.request
}

fn home(app: &mut SimApp, now: &mut u32) {
    hold(app, now, Key::Exit);
    assert_eq!(app.status().view.screen, Screen::Home);
    assert!(app.status().view.entry.is_none());
}

fn menu(app: &mut SimApp, now: &mut u32, item: MenuItem) {
    let (group, index) = menu_location(item);
    home(app, now);
    tap(app, now, Key::Menu);
    assert_eq!(app.status().view.screen, Screen::Menu);
    assert_eq!(app.status().view.menu_group, None);
    if let Some(group) = group {
        let group_index = menu_entries(None)
            .iter()
            .position(|entry| *entry == MenuEntry::Group(group))
            .expect("root group") as u8;
        for _ in 0..menu_entries(None).len() {
            if app.status().view.menu_index == group_index {
                break;
            }
            tap(app, now, Key::Down);
        }
        assert_eq!(tap_request(app, now, Key::Menu), None);
        assert_eq!(app.status().view.menu_group, Some(group));
    }
    for _ in 0..menu_entries(group).len() {
        if app.status().view.menu_index == index {
            break;
        }
        tap(app, now, Key::Down);
    }
    assert_eq!(app.status().view.menu_index, index);
    assert_eq!(
        tap_request(app, now, Key::Menu),
        if item == MenuItem::Spectrum {
            Some(Request::StartSpectrum)
        } else {
            None
        },
        "opening a menu entry emitted an RF request"
    );
}

fn open_setting(app: &mut SimApp, now: &mut u32, item: MenuItem, expected: Setting) {
    menu(app, now, item);
    assert_eq!(app.status().view.screen, Screen::Setting);
    assert_eq!(app.status().view.setting, Some(expected));
}

fn choose_setting(app: &mut SimApp, now: &mut u32, index: u8) {
    let setting = app.status().view.setting.expect("setting editor");
    assert!(index < setting.choice_count());
    for _ in 0..setting.choice_count() {
        if app.status().view.setting_index == index {
            break;
        }
        assert_eq!(
            tap_request(app, now, Key::Down),
            None,
            "draft emitted RF request"
        );
    }
    assert_eq!(app.status().view.setting_index, index);
}

fn confirm_setting(app: &mut SimApp, now: &mut u32) -> Option<Request> {
    let request = tap_request(app, now, Key::Menu);
    assert_eq!(app.status().view.screen, Screen::Menu);
    assert_eq!(app.status().view.setting, None);
    request
}

fn digits(app: &mut SimApp, now: &mut u32, text: &str) {
    for digit in text.bytes() {
        tap(
            app,
            now,
            match digit {
                b'0' => Key::Digit0,
                b'1' => Key::Digit1,
                b'2' => Key::Digit2,
                b'3' => Key::Digit3,
                b'4' => Key::Digit4,
                b'5' => Key::Digit5,
                b'6' => Key::Digit6,
                b'7' => Key::Digit7,
                b'8' => Key::Digit8,
                b'9' => Key::Digit9,
                _ => panic!("non-digit test input"),
            },
        );
    }
}

fn capture(
    app: &mut SimApp,
    panel: &Rc<RefCell<Panel>>,
    output: &Path,
    name: &str,
    modeled_delay_ms: u64,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let drain_steps = drain(app);
    let status = app.status();
    let view = app.snapshot();
    let panel = panel.borrow();
    assert_eq!(panel.madctl, Some(0x08));
    assert_eq!(panel.madctl_writes, 1);
    let path = output.join(name);
    panel.save_png(&path)?;
    Ok(serde_json::json!({
        "image": name,
        "png_width": WIDTH, "png_height": HEIGHT,
        "lcd_madctl_observed": panel.madctl, "lcd_madctl_writes": panel.madctl_writes,
        "image_sha256": format!("{:x}", Sha256::digest(fs::read(&path)?)),
        "view": format!("{:?}", status.view),
        "screen": format!("{:?}", status.view.screen),
        "radio_status_explicitly_simulated":format!("{:?}", app.radio_view()),
        "state": format!("{:?}", status.state),
        "calibration_matches_preserved_backup": status.calibration_matches_backup,
        "slot_index": view.as_ref().map(|s| s.index()),
        "active": view.as_ref().map(|s| s.channel.is_active()),
        "rx_code": view.as_ref().map(|s| s.channel.rx_frequency_code()),
        "tx_code": view.as_ref().map(|s| s.channel.tx_frequency_code()),
        "rx_tone_raw": view.as_ref().map(|s| s.channel.rx_tone_raw()),
        "tx_tone_raw": view.as_ref().map(|s| s.channel.tx_tone_raw()),
        "bandwidth_raw": view.as_ref().map(|s| s.channel.bandwidth_raw()),
        "power_raw": view.as_ref().map(|s| s.channel.power_raw()),
        "reverse_display_only": status.view.reverse,
        "name_raw_hex": view.as_ref().map(|s| hex(s.channel.name_raw())),
        "raw_matrix_and_ptt": status.raw_sample,
        "last_pressed": status.last_pressed.map(uv98_diag_app::key_name),
        "presses": status.presses,
        "input_ready": status.input_ready,
        "lcd_commands": panel.commands,
        "pixel_words_including_init": panel.pixel_words,
        "modeled_lcd_delay_ms": modeled_delay_ms,
        "capture_drain_steps_max4rows": drain_steps,
        "render_pending_after_capture": app.render_pending(),
        "rf_metadata_provenance":"host-only simulated status; no RF hardware backend linked",
    }))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        (WIDTH, HEIGHT),
        (240, 320),
        "handheld renderer must use portrait geometry"
    );
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err(
            "usage: uv98-handheld-image-sim <4MiB-storage-backup> <new-output-directory>".into(),
        );
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let data = fs::read(input)?;
    if data.len() != uv98_nor_read::CAPACITY_BYTES as usize {
        return Err("expected exactly 4 MiB NOR main-array backup".into());
    }
    let input_hash = format!("{:x}", Sha256::digest(&data));
    let active: Vec<usize> = data[0x4000..0x4000 + 1024 * 48]
        .chunks_exact(48)
        .enumerate()
        .filter_map(|(index, bytes)| {
            uv98_codeplug::channels::Channel::parse(bytes)
                .ok()
                .filter(|record| record.is_active())
                .map(|_| index)
        })
        .collect();
    let fixture_zones: Vec<(u8, Vec<u16>)> = data[0x1c000..0x3c000]
        .chunks_exact(512)
        .enumerate()
        .filter_map(|(index, record)| {
            let count = usize::from(record[0]);
            if record[0] != record[1] || count == 0 || count > 200 {
                return None;
            }
            let mut members: Vec<u16> = record[20..20 + count * 2]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .filter(|channel| active.contains(&usize::from(*channel)))
                .collect();
            members.sort_unstable();
            members.dedup();
            if members.is_empty() {
                None
            } else {
                Some((index as u8, members))
            }
        })
        .collect();
    if active.is_empty() {
        return Err("this fixture needs at least one active channel".into());
    }
    let gbk_record_start = 0x4000 + 127 * 48;
    let gbk_name =
        uv98_codeplug::channels::Channel::parse(&data[gbk_record_start..gbk_record_start + 48])
            .expect("fixed channel128 record")
            .name_raw()
            .to_vec();
    assert!(active.contains(&127), "fixture channel128 must be active");
    assert!(
        gbk_name.iter().any(|byte| *byte >= 0x80 && *byte != 0xff),
        "fixture channel128 needs non-ASCII bytes"
    );
    fs::create_dir(output)?;
    let memory = Memory {
        data,
        address: None,
        selected: false,
        reads: Vec::new(),
    };
    let panel = Rc::new(RefCell::new(Panel::new()));
    let milliseconds = Rc::new(Cell::new(0));
    let mut app = App::new(
        memory,
        ObservedPanel(panel.clone()),
        SimDelay {
            milliseconds: milliseconds.clone(),
        },
        Timing::new(30, 600).map_err(|e| format!("timing: {e:?}"))?,
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0).map_err(|e| format!("handheld start: {e:?}"))?;
    assert_eq!(app.status().state, State::Ready);
    assert_eq!(app.status().calibration_matches_backup, Some(true));
    assert_eq!(app.status().view.channel, Some(active[0] as u16));
    let cal_hash = hex(&app
        .status()
        .calibration_sha256
        .ok_or("missing calibration digest")?);
    let mut frames = Vec::new();
    let mut performance = Vec::new();
    let mut now = 0;
    release(&mut app, &mut now);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "01-home.png",
        milliseconds.get(),
    )?);

    // Same App/NOR/display composition as firmware. Time and key observations
    // are synthetic; no physical timing or real UI responsiveness is claimed.
    let initial_pixels = panel.borrow().pixel_words;
    for _ in 0..12 {
        sample(&mut app, &mut now, 0, false);
    }
    assert_eq!(
        panel.borrow().pixel_words,
        initial_pixels,
        "idle scans redraw screen"
    );
    performance.push(serde_json::json!({"action":"12 idle scans", "pixel_bytes":0}));

    let old = app.status().view.channel;
    let before = panel.borrow().pixel_words;
    tap(&mut app, &mut now, Key::Down);
    assert_eq!(
        app.status().view.channel,
        Some(active[if active.len() > 1 { 1 } else { 0 }] as u16)
    );
    drain(&mut app);
    performance.push(serde_json::json!({"action":"next active channel completed", "pixel_bytes":(panel.borrow().pixel_words-before)*2}));
    tap(&mut app, &mut now, Key::Up);
    assert_eq!(app.status().view.channel, old);

    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.screen, Screen::Menu);
    assert!(
        app.render_pending(),
        "menu transition should still have queued rows"
    );
    let queued_index = app.status().view.menu_index;
    let pixels_before_pending_input = panel.borrow().pixel_words;
    for _ in 0..2 {
        app.sample(
            now,
            InputSample::Stable {
                matrix: raw(Key::Down).0,
                ptt_pressed: false,
            },
        )
        .expect("input accepted while a frame is pending");
        now += 40;
    }
    assert_eq!(
        panel.borrow().pixel_words,
        pixels_before_pending_input,
        "input processing must not synchronously draw a frame"
    );
    assert_ne!(
        app.status().view.menu_index,
        queued_index,
        "pending LCD work blocked navigation"
    );
    let progress = app
        .render_step(4)
        .expect("one bounded step after pending input");
    assert!(progress.rows <= 4);
    assert!(panel.borrow().pixel_words - pixels_before_pending_input <= WIDTH * 4);
    performance.push(
        serde_json::json!({"action":"input during pending menu frame",
        "input_accepted_without_lcd_io":true,"maximum_rows_per_step":4,
        "rows_this_step":progress.rows,"physical_latency_measured":false}),
    );
    release(&mut app, &mut now);
    tap(&mut app, &mut now, Key::Up);
    assert_eq!(app.status().view.menu_index, queued_index);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "02-menu.png",
        milliseconds.get(),
    )?);
    menu(&mut app, &mut now, MenuItem::Channels);
    assert_eq!(app.status().view.screen, Screen::Channels);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "03-channels.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Down);
    let list_choice = app.status().view.list_channel;
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, list_choice);
    assert_eq!(app.status().view.screen, Screen::Home);

    let jump = *active.last().unwrap();
    digits(&mut app, &mut now, &(jump + 1).to_string());
    let selected_before_confirm = app.status().view.channel;
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "04-channel-entry.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, Some(jump as u16));
    assert!(app.status().view.entry.is_none());
    let invalid_before = app.status().view.channel;
    digits(&mut app, &mut now, "0");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(
        app.status().view.channel,
        invalid_before,
        "invalid channel changed selection"
    );
    home(&mut app, &mut now);
    assert_eq!(
        app.status().view.channel,
        invalid_before,
        "cancel changed selection"
    );

    tap(&mut app, &mut now, Key::Star);
    assert_eq!(app.status().view.mode, Mode::Vfo);
    let old_frequency = app.status().view.vfo_10hz;
    digits(&mut app, &mut now, "14550000");
    assert_eq!(
        app.status().view.vfo_10hz,
        old_frequency,
        "entry committed without confirm"
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "05-frequency-entry.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.vfo_10hz, 14_550_000);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "06-vfo.png",
        milliseconds.get(),
    )?);
    digits(&mut app, &mut now, "145625");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(
        app.status().view.vfo_10hz,
        14_562_500,
        "six-digit frequency padding failed"
    );
    assert!(app.status().view.entry.is_none());
    digits(&mut app, &mut now, "00000000");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(
        app.status().view.vfo_10hz,
        14_562_500,
        "invalid frequency committed"
    );
    home(&mut app, &mut now);

    menu(&mut app, &mut now, MenuItem::Step);
    assert_eq!(app.status().view.screen, Screen::Step);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "07-step.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Down);
    tap(&mut app, &mut now, Key::Menu);
    home(&mut app, &mut now);
    let step = uv98_handheld_ui::controller::STEP_10HZ[app.status().view.step_index as usize];
    let before_frequency = app.status().view.vfo_10hz;
    tap(&mut app, &mut now, Key::Up);
    assert_eq!(app.status().view.vfo_10hz, before_frequency + step);

    for (index, expected, name) in [
        (
            MenuItem::ChannelDetails,
            Screen::ChannelDetails,
            "08-channel-details.png",
        ),
        (
            MenuItem::Calibration,
            Screen::Calibration,
            "09-calibration.png",
        ),
        (MenuItem::Help, Screen::Help, "10-help.png"),
        (MenuItem::About, Screen::About, "11-about.png"),
    ] {
        menu(&mut app, &mut now, index);
        assert_eq!(app.status().view.screen, expected);
        frames.push(capture(&mut app, &panel, output, name, milliseconds.get())?);
    }
    menu(&mut app, &mut now, MenuItem::Keys);
    assert_eq!(app.status().view.screen, Screen::Keys);
    tap(&mut app, &mut now, Key::Digit1);
    assert_eq!(app.status().view.screen, Screen::Keys);
    assert_eq!(app.status().last_pressed, Some(Key::Digit1));
    release(&mut app, &mut now);
    sample(&mut app, &mut now, 0, true);
    sample(&mut app, &mut now, 0, true);
    assert!(app.status().view.ptt_down);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "12-keys-ptt.png",
        milliseconds.get(),
    )?);
    release(&mut app, &mut now);
    home(&mut app, &mut now);

    hold(&mut app, &mut now, Key::Hash);
    assert!(app.status().view.locked);
    let locked_selection = (app.status().view.channel, app.status().view.vfo_10hz);
    tap(&mut app, &mut now, Key::Up);
    assert_eq!(
        (app.status().view.channel, app.status().view.vfo_10hz),
        locked_selection
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "13-locked.png",
        milliseconds.get(),
    )?);
    hold(&mut app, &mut now, Key::Hash);
    assert!(!app.status().view.locked);

    home(&mut app, &mut now);
    // Ambiguous matrix combination is not a valid Menu or Up press.
    sample(&mut app, &mut now, 0x3000, false);
    sample(&mut app, &mut now, 0x3000, false);
    assert_eq!(app.status().view.screen, Screen::Home);
    release(&mut app, &mut now);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.screen, Screen::Menu);
    home(&mut app, &mut now);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "14-final-home.png",
        milliseconds.get(),
    )?);
    assert_eq!(app.status().state, State::Ready);
    // The user's channel128 name is GBK-encoded Cyrillic. Exercise the same
    // read-only App path, preserving all original bytes rather than a mock label.
    if app.status().view.mode == Mode::Vfo {
        tap(&mut app, &mut now, Key::Star);
    }
    digits(&mut app, &mut now, "128");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, Some(127));
    assert_eq!(
        app.snapshot()
            .expect("selected channel128")
            .channel
            .name_raw(),
        gbk_name.as_slice()
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "16-cyrillic-channel.png",
        milliseconds.get(),
    )?);
    // Channel1 is a real saved repeater with CTCSS. Reverse changes only
    // presentation; the borrowed raw record and all source bytes stay intact.
    home(&mut app, &mut now);
    digits(&mut app, &mut now, "1");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, Some(0));
    let offset_rx = app.snapshot().unwrap().channel.rx_frequency_code();
    let offset_tx = app.snapshot().unwrap().channel.tx_frequency_code();
    assert_ne!(offset_rx, offset_tx);
    assert!(matches!(
        app.snapshot().unwrap().channel.rx_tone(),
        Some(uv98_codeplug::channels::Tone::Ctcss { .. })
    ));
    open_setting(&mut app, &mut now, MenuItem::Reverse, Setting::Reverse);
    assert!(
        !app.status().view.reverse,
        "opening reverse must not apply it"
    );
    choose_setting(&mut app, &mut now, 1);
    assert!(
        !app.status().view.reverse,
        "draft reverse must not apply it"
    );
    assert_eq!(confirm_setting(&mut app, &mut now), None);
    assert!(app.status().view.reverse);
    home(&mut app, &mut now);
    assert_eq!(
        app.snapshot().unwrap().channel.rx_frequency_code(),
        offset_rx
    );
    assert_eq!(
        app.snapshot().unwrap().channel.tx_frequency_code(),
        offset_tx
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "17-reverse-repeater.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Down);
    assert!(
        !app.status().view.reverse,
        "actual channel change must reset reverse"
    );
    home(&mut app, &mut now);
    digits(&mut app, &mut now, "127");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, Some(126));
    assert!(matches!(
        app.snapshot().unwrap().channel.rx_tone(),
        Some(uv98_codeplug::channels::Tone::Dcs { .. })
    ));
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "18-dcs-channel.png",
        milliseconds.get(),
    )?);

    // 018: use actual preserved zones and channels. Source membership is never
    // edited; only the application's volatile view/filter changes.
    menu(&mut app, &mut now, MenuItem::Zones);
    assert_eq!(app.status().view.screen, Screen::Zones);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "19-zones.png",
        milliseconds.get(),
    )?);
    if let Some((zone_index, members)) = fixture_zones.first() {
        for _ in 0..257 {
            if app.status().view.zone_cursor == Some(*zone_index) {
                break;
            }
            tap(&mut app, &mut now, Key::Down);
        }
        assert_eq!(app.status().view.zone_cursor, Some(*zone_index));
        tap(&mut app, &mut now, Key::Menu);
        assert_eq!(app.status().view.zone, Some(*zone_index));
        assert!(app
            .status()
            .view
            .channel
            .is_some_and(|channel| members.contains(&channel)));
        frames.push(capture(
            &mut app,
            &panel,
            output,
            "20-zone-home.png",
            milliseconds.get(),
        )?);
        menu(&mut app, &mut now, MenuItem::Channels);
        frames.push(capture(
            &mut app,
            &panel,
            output,
            "21-zone-channel-list.png",
            milliseconds.get(),
        )?);
        menu(&mut app, &mut now, MenuItem::Zones);
        for _ in 0..257 {
            if app.status().view.zone_cursor.is_none() {
                break;
            }
            tap(&mut app, &mut now, Key::Down);
        }
        assert_eq!(app.status().view.zone_cursor, None);
        tap(&mut app, &mut now, Key::Menu);
        assert_eq!(app.status().view.zone, None);
    }
    home(&mut app, &mut now);
    if app.status().view.active_slot != Slot::A {
        app.select_slot(Slot::A).expect("select A in simulator");
    }
    let slot_a = app.status().view;
    hold(&mut app, &mut now, Key::Star);
    assert_eq!(app.status().view.active_slot, Slot::B);
    if app.status().view.mode == Mode::Channel {
        tap(&mut app, &mut now, Key::Star);
    }
    digits(&mut app, &mut now, "145525");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.vfo_10hz, 14_552_500);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "22-slot-b-vfo.png",
        milliseconds.get(),
    )?);
    hold(&mut app, &mut now, Key::Star);
    assert_eq!(app.status().view.active_slot, Slot::A);
    assert_eq!(
        (
            app.status().view.mode,
            app.status().view.channel,
            app.status().view.vfo_10hz
        ),
        (slot_a.mode, slot_a.channel, slot_a.vfo_10hz)
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "23-slot-a-with-b-preview.png",
        milliseconds.get(),
    )?);

    menu(&mut app, &mut now, MenuItem::Shortcuts);
    assert_eq!(app.status().view.screen, Screen::Shortcuts);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "24-long-key-bindings.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.screen, Screen::ShortcutAction);
    for _ in 0..uv98_handheld_ui::controller::SHORTCUT_ACTIONS.len() {
        let action = uv98_handheld_ui::controller::SHORTCUT_ACTIONS
            [app.status().view.binding_action_index as usize];
        if action == ShortcutAction::Modulation {
            break;
        }
        tap(&mut app, &mut now, Key::Down);
    }
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "25-assign-long-1.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.bindings[0], ShortcutAction::Modulation);
    home(&mut app, &mut now);
    assert_eq!(
        hold_request(&mut app, &mut now, Key::Digit1),
        Some(Request::CycleModulation)
    );
    assert!(
        app.status().view.entry.is_none(),
        "long digit must not leave a short numeric entry"
    );
    app.reject_radio_request(Request::CycleModulation)
        .expect("simulate unavailable backend");
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "26-unavailable-radio-action.png",
        milliseconds.get(),
    )?);
    home(&mut app, &mut now);

    if app.status().view.mode == Mode::Vfo {
        tap(&mut app, &mut now, Key::Star);
    }
    digits(&mut app, &mut now, "1");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, Some(0));
    assert_eq!(
        hold_request(&mut app, &mut now, Key::Digit3),
        Some(Request::StartScan)
    );
    assert_eq!(app.status().view.scan, ScanState::Requested);
    assert!(app.status().view.entry.is_none());
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "27-scan-request-awaiting-backend.png",
        milliseconds.get(),
    )?);
    assert!(app
        .acknowledge_scan_started()
        .expect("synthetic scan backend acknowledgement"));
    let simulated_rx = RadioView {
        state: RadioState::Receiving,
        rssi: Some(245),
        squelch_open: true,
        monitor: false,
        power: Some(0),
        modulation: Modulation::Fm,
        route: Some(RadioRoute::Uhf),
        tx_trial: false,
    };
    app.set_radio_view(simulated_rx)
        .expect("inject explicitly simulated RX metadata");
    assert_eq!(app.status().view.scan, ScanState::Running);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "28-SIMULATED-scan-rx.png",
        milliseconds.get(),
    )?);
    release(&mut app, &mut now);
    sample(&mut app, &mut now, 0, true);
    let stop = sample(&mut app, &mut now, 0, true);
    assert_eq!(stop.request, Some(Request::StopScan));
    assert!(!stop.ptt_tx_requested);
    assert_eq!(app.status().view.scan, ScanState::Stopping);
    app.acknowledge_scan_stopped()
        .expect("synthetic scan-stop acknowledgement");
    let still_held = sample(&mut app, &mut now, 0, true);
    assert!(
        !still_held.ptt_tx_requested,
        "held scan-stop PTT must not become TX"
    );
    release(&mut app, &mut now);
    sample(&mut app, &mut now, 0, true);
    let tx_intent = sample(&mut app, &mut now, 0, true);
    assert!(tx_intent.ptt_tx_requested);
    assert_eq!(tx_intent.ptt_slot, Some(Slot::A));
    // Explicit fake backend metadata only. No transmit function/device exists.
    app.set_radio_view(RadioView {
        state: RadioState::Transmitting,
        rssi: None,
        ..simulated_rx
    })
    .expect("inject explicitly simulated TX metadata");
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "29-SIMULATED-tx-paired-frequency.png",
        milliseconds.get(),
    )?);
    release(&mut app, &mut now);
    app.set_radio_view(simulated_rx)
        .expect("return simulated RX metadata");
    home(&mut app, &mut now);
    if app.status().view.mode == Mode::Channel {
        tap(&mut app, &mut now, Key::Star);
    }
    digits(&mut app, &mut now, "007100");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.vfo_10hz, 710_000);
    app.set_radio_view(RadioView {
        modulation: Modulation::Am,
        route: Some(RadioRoute::Hf),
        rssi: Some(982),
        monitor: true,
        ..simulated_rx
    })
    .expect("simulated AM/HF metadata");
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "30-SIMULATED-am-hf-rssi.png",
        milliseconds.get(),
    )?);
    app.set_radio_view(RadioView::OFF)
        .expect("clear simulated radio metadata");

    menu(&mut app, &mut now, MenuItem::SideKeys);
    assert_eq!(app.status().view.screen, Screen::SideKeys);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "31-side-key-bindings.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.screen, Screen::SideAction);
    for _ in 0..7 {
        if uv98_handheld_ui::controller::SIDE_ACTIONS[app.status().view.side_action_index as usize]
            == SideAction::Monitor
        {
            break;
        }
        tap(&mut app, &mut now, Key::Down);
    }
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "32-assign-upper-side.png",
        milliseconds.get(),
    )?);
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.side_bindings[0], SideAction::Monitor);
    home(&mut app, &mut now);
    release(&mut app, &mut now);
    sample(&mut app, &mut now, 0, true);
    let upper = sample(&mut app, &mut now, 0, true);
    assert_eq!(upper.request, Some(Request::ToggleMonitor));
    assert!(!upper.ptt_tx_requested);
    release(&mut app, &mut now);
    menu(&mut app, &mut now, MenuItem::Keys);
    release(&mut app, &mut now);
    side_sample(&mut app, &mut now, true);
    let lower = side_sample(&mut app, &mut now, true);
    assert_eq!(lower.ptt_slot, Some(Slot::B));
    assert!(app.status().view.side_down[1]);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "33-normalized-lower-side-input.png",
        milliseconds.get(),
    )?);
    side_sample(&mut app, &mut now, false);
    side_sample(&mut app, &mut now, false);

    // Nested settings are drafts. Cancellation and RF acknowledgement are
    // separate from confirmation; the mock never executes an RF operation.
    home(&mut app, &mut now);
    let original_mode = app.status().view.mode;
    let mode_choice = u8::from(original_mode == Mode::Channel);
    open_setting(&mut app, &mut now, MenuItem::Mode, Setting::Mode);
    choose_setting(&mut app, &mut now, mode_choice);
    assert_eq!(app.status().view.mode, original_mode);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "34-mode-draft.png",
        milliseconds.get(),
    )?);
    assert_eq!(tap_request(&mut app, &mut now, Key::Exit), None);
    assert_eq!(app.status().view.screen, Screen::Menu);
    assert_eq!(app.status().view.mode, original_mode);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "35-setting-cancelled-menu.png",
        milliseconds.get(),
    )?);
    open_setting(&mut app, &mut now, MenuItem::Mode, Setting::Mode);
    choose_setting(&mut app, &mut now, mode_choice);
    assert_eq!(confirm_setting(&mut app, &mut now), None);
    assert_ne!(app.status().view.mode, original_mode);
    home(&mut app, &mut now);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "36-mode-confirmed-home.png",
        milliseconds.get(),
    )?);
    // Ensure the following scan example has a real stored channel context.
    if app.status().view.mode != Mode::Channel {
        open_setting(&mut app, &mut now, MenuItem::Mode, Setting::Mode);
        choose_setting(&mut app, &mut now, 0);
        assert_eq!(confirm_setting(&mut app, &mut now), None);
    }
    open_setting(&mut app, &mut now, MenuItem::Reverse, Setting::Reverse);
    let before_reverse = app.status().view.reverse;
    choose_setting(&mut app, &mut now, u8::from(!before_reverse));
    assert_eq!(app.status().view.reverse, before_reverse);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "37-reverse-setting-draft.png",
        milliseconds.get(),
    )?);
    assert_eq!(confirm_setting(&mut app, &mut now), None);
    assert_ne!(app.status().view.reverse, before_reverse);

    open_setting(
        &mut app,
        &mut now,
        MenuItem::ActiveSlot,
        Setting::ActiveSlot,
    );
    let before_slot = app.status().view.active_slot;
    choose_setting(&mut app, &mut now, u8::from(before_slot == Slot::A));
    assert_eq!(app.status().view.active_slot, before_slot);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "38-slot-setting-draft.png",
        milliseconds.get(),
    )?);
    assert_eq!(confirm_setting(&mut app, &mut now), None);
    assert_ne!(app.status().view.active_slot, before_slot);
    open_setting(
        &mut app,
        &mut now,
        MenuItem::ActiveSlot,
        Setting::ActiveSlot,
    );
    choose_setting(&mut app, &mut now, u8::from(before_slot == Slot::B));
    assert_eq!(confirm_setting(&mut app, &mut now), None);
    assert_eq!(app.status().view.active_slot, before_slot);

    app.set_radio_view(simulated_rx)
        .expect("explicit simulated RF setting baseline");
    let mut setting_requests = Vec::new();
    for (index, setting, choice, expected, name) in [
        (
            MenuItem::Power,
            Setting::Power,
            2,
            Request::SetPower(2),
            "39-SIMULATED-power-draft.png",
        ),
        (
            MenuItem::Monitor,
            Setting::Monitor,
            1,
            Request::SetMonitor(true),
            "40-SIMULATED-monitor-draft.png",
        ),
        (
            MenuItem::Modulation,
            Setting::Modulation,
            1,
            Request::SetModulation { am: true },
            "41-SIMULATED-modulation-draft.png",
        ),
    ] {
        let before = app.radio_view();
        open_setting(&mut app, &mut now, index, setting);
        choose_setting(&mut app, &mut now, choice);
        assert_eq!(
            app.radio_view(),
            before,
            "draft altered confirmed RF metadata"
        );
        frames.push(capture(&mut app, &panel, output, name, milliseconds.get())?);
        // Each RF setting must also support leaving without emitting a request.
        assert_eq!(tap_request(&mut app, &mut now, Key::Exit), None);
        assert_eq!(app.radio_view(), before);
        open_setting(&mut app, &mut now, index, setting);
        choose_setting(&mut app, &mut now, choice);
        assert_eq!(confirm_setting(&mut app, &mut now), Some(expected));
        assert_eq!(
            app.radio_view(),
            before,
            "a request was mistaken for backend acknowledgement"
        );
        setting_requests.push(format!("{expected:?}"));
        let mut acknowledged = before;
        match expected {
            Request::SetPower(power) => acknowledged.power = Some(power),
            Request::SetMonitor(monitor) => acknowledged.monitor = monitor,
            Request::SetModulation { am } => {
                acknowledged.modulation = if am { Modulation::Am } else { Modulation::Fm }
            }
            _ => unreachable!(),
        }
        app.set_radio_view(acknowledged)
            .expect("explicit fake setting acknowledgement");
        open_setting(&mut app, &mut now, index, setting);
        assert_eq!(
            app.status().view.setting_index,
            choice,
            "editor did not reopen at acknowledged value"
        );
        if setting == Setting::Power {
            frames.push(capture(
                &mut app,
                &panel,
                output,
                "42-SIMULATED-power-confirmed.png",
                milliseconds.get(),
            )?);
        }
        assert_eq!(tap_request(&mut app, &mut now, Key::Exit), None);
    }

    open_setting(&mut app, &mut now, MenuItem::Scan, Setting::Scan);
    choose_setting(&mut app, &mut now, 1);
    assert_eq!(app.status().view.scan, ScanState::Stopped);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "43-SIMULATED-scan-setting-draft.png",
        milliseconds.get(),
    )?);
    assert_eq!(
        confirm_setting(&mut app, &mut now),
        Some(Request::StartScan)
    );
    assert_eq!(app.status().view.scan, ScanState::Requested);
    assert!(app
        .acknowledge_scan_started()
        .expect("explicit fake nested-scan start acknowledgement"));
    assert_eq!(tap_request(&mut app, &mut now, Key::Menu), None);
    assert_eq!(app.status().view.screen, Screen::Setting);
    assert_eq!(app.status().view.setting, Some(Setting::Scan));
    choose_setting(&mut app, &mut now, 0);
    assert_eq!(
        app.status().view.scan,
        ScanState::Running,
        "draft stopped a running scan"
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "44-SIMULATED-running-scan-stop-draft.png",
        milliseconds.get(),
    )?);
    assert_eq!(
        tap_request(&mut app, &mut now, Key::Exit),
        Some(Request::StopScan)
    );
    assert_eq!(
        app.status().view.scan,
        ScanState::Stopping,
        "EXIT must cancel temporary scan"
    );
    app.acknowledge_scan_stopped()
        .expect("synthetic universal EXIT stop acknowledgement");
    assert_eq!(tap_request(&mut app, &mut now, Key::Menu), None);
    choose_setting(&mut app, &mut now, 1);
    assert_eq!(
        confirm_setting(&mut app, &mut now),
        Some(Request::StartScan)
    );
    assert!(app
        .acknowledge_scan_started()
        .expect("synthetic restart for explicit stop choice"));
    assert_eq!(tap_request(&mut app, &mut now, Key::Menu), None);
    choose_setting(&mut app, &mut now, 0);
    assert_eq!(confirm_setting(&mut app, &mut now), Some(Request::StopScan));
    assert_eq!(app.status().view.scan, ScanState::Stopping);
    app.acknowledge_scan_stopped()
        .expect("explicit fake nested-scan stop acknowledgement");
    app.set_radio_view(RadioView::OFF)
        .expect("clear fake RF status after settings cases");
    home(&mut app, &mut now);
    app.select_slot(Slot::A).expect("saved A context");
    if let Some((zone_index, _)) = fixture_zones.first() {
        menu(&mut app, &mut now, MenuItem::Zones);
        for _ in 0..257 {
            if app.status().view.zone_cursor == Some(*zone_index) {
                break;
            }
            tap(&mut app, &mut now, Key::Down);
        }
        assert_eq!(app.status().view.zone_cursor, Some(*zone_index));
        tap(&mut app, &mut now, Key::Menu);
    }
    home(&mut app, &mut now);
    let saved_a = app.status().view;
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "45-two-slots-a-active.png",
        milliseconds.get(),
    )?);
    hold(&mut app, &mut now, Key::Star);
    assert_eq!(app.status().view.active_slot, Slot::B);
    assert_eq!(app.status().view.mode, Mode::Vfo);
    assert_eq!(app.status().view.vfo_10hz, 14_552_500);
    assert_eq!(app.status().view.zone, None);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "46-two-slots-b-active.png",
        milliseconds.get(),
    )?);
    hold(&mut app, &mut now, Key::Star);
    assert_eq!(app.status().view.channel, saved_a.channel);
    assert_eq!(app.status().view.zone, saved_a.zone);

    for (item, setting, choice, expected, name) in [
        (
            MenuItem::DisplayLight,
            Setting::DisplayLight,
            3,
            Request::SetDisplayLight(LightMode::Seconds10),
            "47-display-light-draft.png",
        ),
        (
            MenuItem::KeyLight,
            Setting::KeyLight,
            5,
            Request::SetKeyLight(LightMode::Seconds30),
            "48-key-light-draft.png",
        ),
    ] {
        let before = (app.status().view.display_light, app.status().view.key_light);
        open_setting(&mut app, &mut now, item, setting);
        choose_setting(&mut app, &mut now, choice);
        assert_eq!(
            (app.status().view.display_light, app.status().view.key_light),
            before
        );
        frames.push(capture(&mut app, &panel, output, name, milliseconds.get())?);
        assert_eq!(tap_request(&mut app, &mut now, Key::Exit), None);
        assert_eq!(
            (app.status().view.display_light, app.status().view.key_light),
            before
        );
        open_setting(&mut app, &mut now, item, setting);
        choose_setting(&mut app, &mut now, choice);
        assert_eq!(confirm_setting(&mut app, &mut now), Some(expected));
        assert_eq!(
            (app.status().view.display_light, app.status().view.key_light),
            before,
            "lighting request is not backend acknowledgement"
        );
        match expected {
            Request::SetDisplayLight(mode) => app
                .set_lighting_modes(mode, before.1)
                .expect("synthetic display light ack"),
            Request::SetKeyLight(mode) => app
                .set_lighting_modes(before.0, mode)
                .expect("synthetic key light ack"),
            _ => unreachable!(),
        }
    }
    assert_eq!(app.status().view.display_light, LightMode::Seconds10);
    assert_eq!(app.status().view.key_light, LightMode::Seconds30);
    open_setting(&mut app, &mut now, MenuItem::KeyLight, Setting::KeyLight);
    assert_eq!(app.status().view.setting_index, 5);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "49-independent-lighting-confirmed.png",
        milliseconds.get(),
    )?);
    performance.push(
        serde_json::json!({"action":"dual-slot layout and independent lighting policies",
        "both_frequencies_full_size_fixed_A_above_B":true,"slot_contexts_preserved":true,
        "menu_helper_uses_semantic_item_not_hardcoded_index":true,
        "display_policy":"Seconds10","keys_policy":"Seconds30",
        "lighting_ack_explicitly_simulated":true,"physical_light_outputs_verified":false}),
    );
    performance.push(serde_json::json!({"action":"nested settings explicit commit",
        "all_seven_setting_editors":true, "arrows_and_exit_emit_no_setting_requests":true,
        "rf_requests_on_confirm":setting_requests,"rf_metadata_changes_only_after_explicit_fake_ack":true,
        "scan_browsing_preserves_running_state_but_exit_stops":true,"physical_rf_operations":false}));
    performance.push(serde_json::json!({"action":"018 host-only behavior",
        "zone_source":"preserved NOR fixture", "valid_nonempty_zones":fixture_zones.len(),
        "slot_b_preserved_independently":true,"long_digit_no_phantom_entry":true,
        "scan_running_requires_backend_ack":true,"scan_stop_ptt_not_tx":true,
        "radio_metadata":"explicitly simulated; no physical RF evidence",
        "normalized_lower_side_input":"Aux16 synthetic, not ADC threshold validation"}));
    home(&mut app, &mut now);
    app.select_slot(Slot::A).expect("upper trial context A");
    menu(&mut app, &mut now, MenuItem::Zones);
    for _ in 0..257 {
        if app.status().view.zone_cursor.is_none() {
            break;
        }
        tap(&mut app, &mut now, Key::Down);
    }
    assert_eq!(app.status().view.zone_cursor, None);
    tap(&mut app, &mut now, Key::Menu);
    if app.status().view.mode != Mode::Channel {
        tap(&mut app, &mut now, Key::Star);
    }
    digits(&mut app, &mut now, "2");
    tap(&mut app, &mut now, Key::Menu);
    assert_eq!(app.status().view.channel, Some(1));
    app.set_radio_view(RadioView {
        tx_trial: true,
        ..simulated_rx
    })
    .expect("explicit synthetic RX state with TX trial build warning");
    assert_eq!(app.radio_view().state, RadioState::Receiving);
    assert!(!app.status().view.ptt_down);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "50-SIMULATED-upper-exo-trial.png",
        milliseconds.get(),
    )?);
    performance.push(
        serde_json::json!({"action":"upper EXO trial build indicator",
        "state":"Receiving","tx_trial":true,"ptt_held":false,
        "label":"TX TEST: CH002 / A / 5S","actual_transmission_performed":false}),
    );
    // Explicitly synthetic spectrum input exercises the same compact protocol
    // and App publication queue. These PNGs are not evidence of RF measurements.
    home(&mut app, &mut now);
    menu(&mut app, &mut now, MenuItem::Power);
    assert_eq!(tap_request(&mut app, &mut now, Key::Exit), None);
    assert_eq!(app.status().view.menu_group, Some(MenuGroup::Receive));
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "51-receive-menu-group.png",
        milliseconds.get(),
    )?);
    app.set_radio_view(RadioView {
        tx_trial: false,
        ..simulated_rx
    })
    .expect("synthetic spectrum RX metadata");
    menu(&mut app, &mut now, MenuItem::Spectrum);
    assert_eq!(app.status().view.screen, Screen::Spectrum);
    assert!(app.spectrum_view().is_none());
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "52-SIMULATED-spectrum-no-data.png",
        milliseconds.get(),
    )?);
    use uv98_spectrum::compact::{
        Action as SweepAction, Config as SweepConfig, Engine as SweepEngine, Event as SweepEvent,
        State as SweepState,
    };
    let mut spectrum = SweepEngine::new();
    let range = uv98_spectrum::Range::new(43_247_500, 43_347_500, 1250).unwrap();
    let config = SweepConfig::new(
        range,
        43_297_500,
        uv98_spectrum::Timing::new(100, 100, 10, 100).unwrap(),
        500,
    )
    .unwrap();
    let SweepAction::Prepare { ticket } = spectrum.start(now, config).unwrap() else {
        panic!()
    };
    let mut action = spectrum
        .handle(now, SweepEvent::Prepared(ticket))
        .expect("synthetic prepare ACK");
    for index in 0..81 {
        let SweepAction::Tune {
            ticket,
            index: actual,
            frequency_10hz,
        } = action
        else {
            panic!()
        };
        assert_eq!(actual as usize, index);
        assert_eq!(frequency_10hz, range.frequency_10hz(index).unwrap());
        assert_eq!(spectrum.handle(now, SweepEvent::Tuned(ticket)), None);
        now += 10;
        let Some(SweepAction::Read { ticket, .. }) = spectrum.handle(now, SweepEvent::Poll) else {
            panic!()
        };
        // Deterministic demo peaks. Never fed to the physical backend.
        let distance = [15usize, 42, 66]
            .iter()
            .map(|&p| p.abs_diff(index))
            .min()
            .unwrap();
        let raw = (35 + index * 13 % 19 + 420usize.saturating_sub(distance * 80)) as u16;
        action = spectrum
            .handle(now, SweepEvent::Sample { ticket, raw })
            .expect("synthetic RSSI ACK");
    }
    assert!(matches!(action, SweepAction::SweepComplete { sweep: 1 }));
    let mut snapshot = SpectrumSnapshot {
        state: SpectrumState::Sweeping,
        start_10hz: range.start_10hz(),
        step_10hz: range.step_10hz(),
        len: range.len() as u8,
        plot: *spectrum.plot(),
    };
    assert!(app
        .set_spectrum(snapshot)
        .expect("publish first stable plot"));
    let mut later = snapshot;
    later.plot.latest[40] = 1;
    assert!(!app.set_spectrum(later).expect("busy immutable snapshot"));
    assert_eq!(
        app.spectrum_view().unwrap().plot.latest[40],
        snapshot.plot.latest[40]
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "53-SIMULATED-spectrum-full-sweep.png",
        milliseconds.get(),
    )?);
    action = spectrum
        .handle(now, SweepEvent::Poll)
        .expect("next sweep starts explicitly");
    for index in 0..24 {
        let SweepAction::Tune {
            ticket,
            index: actual,
            ..
        } = action
        else {
            panic!()
        };
        assert_eq!(actual as usize, index);
        assert_eq!(spectrum.handle(now, SweepEvent::Tuned(ticket)), None);
        now += 10;
        let Some(SweepAction::Read { ticket, .. }) = spectrum.handle(now, SweepEvent::Poll) else {
            panic!()
        };
        action = spectrum
            .handle(
                now,
                SweepEvent::Sample {
                    ticket,
                    raw: (20 + index * 5) as u16,
                },
            )
            .unwrap();
    }
    snapshot.plot = *spectrum.plot();
    assert!(app.set_spectrum(snapshot).unwrap());
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "54-SIMULATED-spectrum-partial-and-peaks.png",
        milliseconds.get(),
    )?);
    let before_cursor = panel.borrow().pixel_words;
    tap(&mut app, &mut now, Key::Up);
    drain(&mut app);
    let cursor_words = panel.borrow().pixel_words - before_cursor;
    assert!(
        cursor_words <= WIDTH * 35,
        "spectrum cursor repainted graph"
    );
    let cursor = app.status().view.spectrum_cursor;
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "55-SIMULATED-spectrum-cursor.png",
        milliseconds.get(),
    )?);
    let old_frequency = app.status().view.vfo_10hz;
    assert_eq!(
        tap_request(&mut app, &mut now, Key::Menu),
        Some(Request::AcceptSpectrum { bin: cursor })
    );
    assert_eq!(app.status().view.screen, Screen::Spectrum);
    assert_eq!(app.status().view.vfo_10hz, old_frequency);
    let Some(SweepAction::Restore {
        ticket,
        frequency_10hz,
        ..
    }) = spectrum.handle(now, SweepEvent::Accept { index: cursor })
    else {
        panic!()
    };
    snapshot.state = SpectrumState::Restoring;
    assert!(app.set_spectrum(snapshot).unwrap());
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "56-SIMULATED-spectrum-restoring.png",
        milliseconds.get(),
    )?);
    spectrum.handle(now + 1, SweepEvent::Restored(ticket));
    assert!(matches!(spectrum.state(), SweepState::Restored(_)));
    app.accept_spectrum_frequency(frequency_10hz)
        .expect("only after synthetic full RX restore ACK");
    app.clear_spectrum().unwrap();
    assert_eq!(app.status().view.screen, Screen::Home);
    assert_eq!(app.status().view.mode, Mode::Vfo);
    assert_eq!(app.status().view.vfo_10hz, frequency_10hz);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "57-SIMULATED-spectrum-frequency-accepted.png",
        milliseconds.get(),
    )?);
    menu(&mut app, &mut now, MenuItem::Spectrum);
    let request = tap_request(&mut app, &mut now, Key::Ptt);
    assert_eq!(request, Some(Request::StopSpectrum));
    assert_ne!(app.radio_view().state, RadioState::Transmitting);
    performance.push(serde_json::json!({"action":"compact spectrum protocol and stable publication",
        "engine_bytes_host":std::mem::size_of::<SweepEngine>(),
        "plot_bytes_host":std::mem::size_of::<uv98_spectrum::compact::Plot>(),
        "bins":81,"synthetic_rssi_only":true,"scale":"raw0..511",
        "cursor_pixel_bytes":cursor_words*2,"snapshot_replacement_blocked_during_pending_frame":true,
        "unknown_data_has_no_synthetic_noise":true,"current_vs_previous_sweep_distinguished":true,
        "menu_commit_waits_for_full_receive_restore_ack":true,"ptt_stops_spectrum_without_tx":true,
        "frequency_after_restore_10hz":frequency_10hz,"physical_rf_operations":false}));
    home(&mut app, &mut now);
    let before_long_zero = app.status().view.vfo_10hz;
    assert_eq!(
        hold_request(&mut app, &mut now, Key::Digit0),
        Some(Request::StartSpectrum)
    );
    assert_eq!(app.status().view.screen, Screen::Spectrum);
    assert!(
        app.status().view.entry.is_none(),
        "long zero emitted a phantom frequency digit"
    );
    assert_eq!(app.status().view.vfo_10hz, before_long_zero);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "58-SIMULATED-long-zero-spectrum-request.png",
        milliseconds.get(),
    )?);
    menu(&mut app, &mut now, MenuItem::Shortcuts);
    for _ in 0..10 {
        if app.status().view.binding_index == 9 {
            break;
        }
        tap(&mut app, &mut now, Key::Down);
    }
    assert_eq!(app.status().view.binding_index, 9);
    assert_eq!(app.status().view.bindings[9], ShortcutAction::Spectrum);
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "59-ten-digit-bindings-zero.png",
        milliseconds.get(),
    )?);
    assert_eq!(tap_request(&mut app, &mut now, Key::Menu), None);
    assert_eq!(app.status().view.screen, Screen::ShortcutAction);
    assert_eq!(
        uv98_handheld_ui::controller::SHORTCUT_ACTIONS
            [app.status().view.binding_action_index as usize],
        ShortcutAction::Spectrum
    );
    frames.push(capture(
        &mut app,
        &panel,
        output,
        "60-zero-spectrum-assignment.png",
        milliseconds.get(),
    )?);
    performance.push(serde_json::json!({"action":"long0 spectrum shortcut",
        "default_digit0":"Spectrum","digits1to9_defaults_unchanged":true,
        "all_ten_digits_assignable":true,"long_zero_no_numeric_entry":true,
        "long_zero_requests_backend_without_fabricated_measurements":true,
        "labels_use_digit0_not_binding10":true}));
    let (memory, _display) = app.into_parts();
    assert!(!memory.selected, "NOR CS left selected");
    assert!(memory.reads.iter().any(|&(a, n)| a == 0 && n == 4096));
    assert!(memory
        .reads
        .iter()
        .all(|&(a, n)| a.checked_add(n).is_some_and(|end| end <= memory.data.len())));
    assert_eq!(format!("{:x}", Sha256::digest(&memory.data)), input_hash);
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(input)?)),
        input_hash
    );
    let app_bytes = std::mem::size_of::<SimApp>();
    assert!(
        app_bytes <= 16 * 1024,
        "unexpected App storage growth: {app_bytes}"
    );
    // A separate synthetic copy exercises an empty catalog. The source file
    // and the real-backup application above remain unchanged.
    let mut empty_bytes = memory.data.clone();
    empty_bytes[0x4000..0x4000 + 1024 * 48].fill(0xff);
    let empty_hash = format!("{:x}", Sha256::digest(&empty_bytes));
    let empty_panel = Rc::new(RefCell::new(Panel::new()));
    let empty_ms = Rc::new(Cell::new(0));
    let mut empty_app = App::new(
        Memory {
            data: empty_bytes,
            address: None,
            selected: false,
            reads: Vec::new(),
        },
        ObservedPanel(empty_panel.clone()),
        SimDelay {
            milliseconds: empty_ms.clone(),
        },
        Timing::new(30, 600).unwrap(),
        PRESERVED_CALIBRATION_SHA256,
    );
    empty_app
        .start(0)
        .map_err(|e| format!("empty-catalog start: {e:?}"))?;
    assert_eq!(empty_app.status().state, State::Ready);
    assert_eq!(empty_app.status().view.mode, Mode::Vfo);
    assert_eq!(empty_app.status().view.channel, None);
    assert_eq!(empty_app.status().calibration_matches_backup, Some(true));
    assert!(empty_app.snapshot().is_none());
    frames.push(capture(
        &mut empty_app,
        &empty_panel,
        output,
        "15-empty-catalog.png",
        empty_ms.get(),
    )?);
    let mut empty_now = 0;
    release(&mut empty_app, &mut empty_now);
    menu(&mut empty_app, &mut empty_now, MenuItem::Channels);
    assert_eq!(empty_app.status().view.screen, Screen::Channels);
    assert_eq!(empty_app.status().view.list_channel, None);
    tap(&mut empty_app, &mut empty_now, Key::Menu);
    assert_eq!(empty_app.status().view.channel, None);
    let (empty_memory, _) = empty_app.into_parts();
    assert!(!empty_memory.selected);
    assert_eq!(
        format!("{:x}", Sha256::digest(&empty_memory.data)),
        empty_hash
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(input)?)),
        input_hash
    );
    let frame_count = frames.len();
    let report = serde_json::json!({
        "kind":"host_protocol_simulation",
        "composition":"uv98-handheld-app App: same application as live image",
        "hardware_access":false,"flash_performed":false,"physical_display_verified":false,
        "input_sha256":input_hash,"calibration_sha256":cal_hash,
        "expected_calibration_sha256":hex(&PRESERVED_CALIBRATION_SHA256),
        "active_channel_count":active.len(),"first_active_index":active[0],
        "jump_test_index":jump,"selection_before_jump_confirm":selected_before_confirm,
        "app_bytes_on_host_including_host_adapters":app_bytes,"app_storage_limit_checked":16*1024,
        "maximum_runtime_stack_measured":false,"render_row_bytes":WIDTH as usize*2,
        "display_geometry":{"width":WIDTH,"height":HEIGHT,"portrait":true,"actual_bus_madctl":8,"image_only_rotation":false,"actual_bus_window_bounds_checked":true,"physical_orientation_verified":false},
        "full_frame_pixel_bytes":WIDTH as usize*HEIGHT as usize*2,
        "full_frame_minimum_wire_time_at_3mhz_ms":409.6,
        "input_bytes_unchanged":true,"nor_opcodes_accepted_by_mock":[3],
        "cooperative_render":{"rows_per_step":4,"row_pixel_bytes":WIDTH*2,
            "maximum_slice_pixel_bytes":WIDTH*4*2,"minimum_slice_wire_time_at_3mhz_ms":5.12,
            "input_tested_while_render_pending":true,"physical_latency_measured":false},
        "radio_simulation":{"metadata_injected":true,"hardware_backend_present":false,
            "mock_values_not_measurements":true,"rf_image_filename_marker":"SIMULATED"},
        "empty_catalog":{"synthetic_only":true,"input_sha256":empty_hash,"ready_in_vfo":true,"calibration_match":true,"selection_remains_none":true,"bytes_unchanged":true},
        "reads":memory.reads,"render_work":performance,"frame_count":frame_count,"frames":frames,
        "acceptance":["first/adjacent active channels", "channel list choice", "explicit numeric channel confirm",
            "invalid channel does not navigate", "cancel does not commit", "VFO entry commits only on MENU", "six-digit MHz entry pads right",
            "out-of-range frequency rejected", "step changes VFO by exact10Hz units", "all menu destinations", "reverse display flag resets on actual channel change and never rewrites record", "real saved CTCSS/repeater and DCS icon examples", "actual MADCTL08 command before portrait windows, not image-only rotation",
            "digit1/PTT human key page", "hold# lock and unlock", "locked navigation ignored",
            "chord rejected and clean release rearms", "empty catalog starts in VFO and cannot select invented channel", "idle scans do not redraw", "READ03-only source unchanged",
            "input accepts navigation while previous frame pending, sample itself performs no LCD I/O",
            "queued frames drain in slices of at most four rows", "actual fixture zones filter channels without writes",
            "A/B contexts preserve independent channel/frequency selections", "long-digit assignment emits action without numeric entry",
            "scan requested and running remain distinct until explicit synthetic acknowledgement",
            "scan-stop PTT cannot transmit until release and new press", "explicitly simulated RF metadata renders RX/TX, paired TX frequency, AM/HF and raw RSSI",
            "upper side assignment and normalized lower Aux16 input exercise distinct actions",
            "seven nested setting editors keep draft separate from current state and commit only on MENU",
            "setting EXIT cancels without RF request; SetPower/SetMonitor/SetModulation wait for explicit synthetic acknowledgement",
            "running scan survives setting browsing; EXIT universally cancels scan and other temporary operations"],
        "limitations":["synthetic host input/timestamps, not GPIO or MCU execution",
            "panel interpreter checks protocol/window bounds, not physical SPI timing/orientation/colors",
            "radio statuses and RSSI are explicitly injected mock data, not physical tuning/reception/transmission evidence",
            "full calibration hash equality is preservation evidence, not RF measurement accuracy",
            "Aux16 side input is normalized synthetic input; physical ADC thresholds and encoder remain outside this adapter",
            "App size bound is not maximum ARM stack measurement"]
    });
    let report_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("simulation-report.json"))?;
    serde_json::to_writer_pretty(report_file, &report)?;
    println!("Handheld App simulation passed; {frame_count} PNG frames and report saved.");
    Ok(())
}
