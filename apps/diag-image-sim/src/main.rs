//! Host-only protocol simulation. No serial, GPIO or RF backend is linked.
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
};
use uv98_diag_app::{App, InputSample, Page, State, PRESERVED_CALIBRATION_SHA256};
use uv98_display::{Delay, DisplayBus, HEIGHT, WIDTH};
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
}
impl Panel {
    fn new() -> Self {
        Self {
            pixels: vec![0; WIDTH as usize * HEIGHT as usize],
            command: 0,
            x: [0, WIDTH - 1],
            y: [0, HEIGHT - 1],
            cursor: 0,
            commands: 0,
            pixel_words: 0,
            reset_high: false,
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
        }
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        if !self.reset_high {
            return Err("command during reset");
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
            0x2a | 0x2b => {
                if data.len() != 4 {
                    return Err("window needs four bytes");
                }
                let limits = [
                    u16::from_be_bytes([data[0], data[1]]),
                    u16::from_be_bytes([data[2], data[3]]),
                ];
                let maximum = if self.command == 0x2a { WIDTH } else { HEIGHT };
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

fn sample(app: &mut SimApp, time: u32, matrix: u16, ptt: bool) {
    app.sample(
        time,
        InputSample::Stable {
            matrix,
            ptt_pressed: ptt,
        },
    )
    .expect("synthetic input must be accepted");
}

fn release(app: &mut SimApp, time: u32) {
    sample(app, time, 0, false);
    sample(app, time + 20, 0, false);
}

fn press(app: &mut SimApp, time: u32, matrix: u16, ptt: bool, expected: Key) {
    sample(app, time, matrix, ptt);
    let update = app
        .sample(
            time + 20,
            InputSample::Stable {
                matrix,
                ptt_pressed: ptt,
            },
        )
        .expect("synthetic press must be accepted");
    assert_eq!(update.pressed, Some(expected));
}

fn capture(
    app: &SimApp,
    panel: &Rc<RefCell<Panel>>,
    output: &Path,
    name: &str,
    modeled_delay_ms: u64,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let status = app.status();
    let view = app.snapshot().ok_or("missing current snapshot")?;
    let general = view.calibration.general();
    let panel = panel.borrow();
    let path = output.join(name);
    panel.save_png(&path)?;
    Ok(serde_json::json!({
        "image": name,
        "image_sha256": format!("{:x}", Sha256::digest(fs::read(&path)?)),
        "page": format!("{:?}", status.page),
        "state": format!("{:?}", status.state),
        "calibration_matches_preserved_backup": status.calibration_matches_backup,
        "band_index": status.band,
        "slot_index": view.index(),
        "active": view.channel.is_active(),
        "rx_code": view.channel.rx_frequency_code(),
        "tx_code": view.channel.tx_frequency_code(),
        "name_raw_hex": hex(view.channel.name_raw()),
        "calibration_codes": {
            "adc": general.adc_correction_code(),
            "reg3e": general.reg_3e_raw(),
            "vox_on": general.vox_on_raw(),
            "vox_off": general.vox_off_raw(),
        },
        "raw_matrix_and_ptt": status.raw_sample,
        "last_pressed": status.last_pressed.map(uv98_diag_app::key_name),
        "presses": status.presses,
        "input_state": format!("{:?}", status.input_state),
        "lcd_commands": panel.commands,
        "pixel_words_including_init": panel.pixel_words,
        "modeled_lcd_delay_ms": modeled_delay_ms,
    }))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err(
            "usage: uv98-diag-image-sim <4MiB-storage-backup> <new-output-directory>".into(),
        );
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let data = fs::read(input)?;
    if data.len() != uv98_nor_read::CAPACITY_BYTES as usize {
        return Err("expected exactly 4 MiB NOR main-array backup".into());
    }
    let input_hash = format!("{:x}", Sha256::digest(&data));
    // The backup is opened only for reading. A new directory and create_new
    // output files protect both the source and existing previews.
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
        Timing::new(20, 500).map_err(|e| format!("timing: {e:?}"))?,
        PRESERVED_CALIBRATION_SHA256,
    );
    app.start(0)
        .map_err(|e| format!("diagnostic start: {e:?}"))?;
    let cal_hash = hex(&app
        .status()
        .calibration_sha256
        .ok_or("missing calibration digest")?);
    let mut frames = Vec::new();
    frames.push(capture(
        &app,
        &panel,
        output,
        "01-startup.png",
        milliseconds.get(),
    )?);

    // Explicitly synthetic host time and key observations. SimDelay only counts
    // the LCD driver's requested delays; it is not the input clock or hardware.
    release(&mut app, 0);
    press(&mut app, 21, 0x1000, false, Key::Menu);
    assert_eq!(app.status().page, Page::Calibration);
    frames.push(capture(
        &app,
        &panel,
        output,
        "02-calibration.png",
        milliseconds.get(),
    )?);
    release(&mut app, 42);
    press(&mut app, 63, 0x1000, false, Key::Menu);
    assert_eq!(app.status().page, Page::Keys);
    release(&mut app, 84);
    press(&mut app, 105, 0, true, Key::Ptt);
    frames.push(capture(
        &app,
        &panel,
        output,
        "03-keys-ptt.png",
        milliseconds.get(),
    )?);
    release(&mut app, 126);
    press(&mut app, 147, 0x8000, false, Key::Exit);
    release(&mut app, 168);
    press(&mut app, 189, 0x4000, false, Key::Down);
    assert_eq!(app.snapshot().ok_or("missing next slot")?.index(), 1);
    frames.push(capture(
        &app,
        &panel,
        output,
        "04-next-slot.png",
        milliseconds.get(),
    )?);
    assert_eq!(app.status().state, State::Ready);
    let (memory, _display) = app.into_parts();
    assert!(!memory.selected, "NOR CS left selected");
    assert_eq!(memory.reads, [(0, 4096), (0x4000, 48), (0x4030, 48)]);
    assert_eq!(format!("{:x}", Sha256::digest(&memory.data)), input_hash);
    let report = serde_json::json!({
        "kind": "host_protocol_simulation",
        "composition": "uv98-diag-app App: shared diagnostic image state machine",
        "lcd_initializations": 1,
        "hardware_access": false,
        "flash_performed": false,
        "physical_display_verified": false,
        "input_sha256": input_hash,
        "calibration_sha256": cal_hash,
        "expected_calibration_sha256": hex(&PRESERVED_CALIBRATION_SHA256),
        "expected_digest_only_embedded_in_firmware": true,
        "app_bytes_on_host_including_host_adapter_types": std::mem::size_of::<SimApp>(),
        "render_row_bytes": WIDTH as usize * 2,
        "nor_opcodes_accepted_by_mock": [3],
        "reads": memory.reads,
        "input_bytes_unchanged": true,
        "frames": frames,
        "limitations": [
            "synthetic host input and timestamps, not a GPIO or MCU execution test",
            "panel command interpreter does not verify physical orientation, SPI wiring or timing",
            "shown calibration codes are stored data; no RF measurements or calibration applied",
            "full-block digest checks equality with preserved backup, not physical calibration accuracy",
            "16 matrix keys and PTT supported; ADC side keys and encoder not sampled",
            "all channel slots are raw storage indices, not zone navigation",
            "ASCII substitutes unsupported name bytes while preserving original storage"
        ],
    });
    let report_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("simulation-report.json"))?;
    serde_json::to_writer_pretty(report_file, &report)?;
    println!("Diagnostic App simulation passed; four PNG frames and report saved.");
    Ok(())
}
