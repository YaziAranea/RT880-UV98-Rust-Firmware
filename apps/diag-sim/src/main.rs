//! Host-only protocol simulation. No serial, GPIO or RF backend is linked.
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
};
use uv98_board_diagnostic::{Session, State};
use uv98_diagnostic::Diagnostic;
use uv98_display::{Delay, DisplayBus, HEIGHT, WIDTH};
use uv98_keys::{matrix_key, ButtonEvent, Debouncer, Key, Timing};
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: uv98-diag-sim <4MiB-storage-backup> <output-directory>".into());
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let data = fs::read(input)?;
    if data.len() != uv98_nor_read::CAPACITY_BYTES as usize {
        return Err("expected exactly 4 MiB NOR main-array backup".into());
    }
    let input_hash = format!("{:x}", Sha256::digest(&data));
    // A fresh directory prevents accidental overwrite of prior previews or of
    // the backup itself, even if an input has one of the output filenames.
    fs::create_dir(output)?;
    let memory = Memory {
        data,
        address: None,
        selected: false,
        reads: Vec::new(),
    };
    let panel = Rc::new(RefCell::new(Panel::new()));
    let milliseconds = Rc::new(Cell::new(0));
    let mut session = Session::new(
        memory,
        ObservedPanel(panel.clone()),
        SimDelay {
            milliseconds: milliseconds.clone(),
        },
    );
    session
        .start(0)
        .map_err(|e| format!("session start: {e:?}"))?;
    let cal_hash = format!(
        "{:x}",
        Sha256::digest(session.snapshot().unwrap().calibration.raw())
    );
    let mut frames = Vec::new();
    for step in 0..2 {
        if step == 1 {
            // Synthetic stable Down matrix samples; policy 20/500 ms is chosen
            // for this demo and is not inferred stock timing or physical scan.
            let key = matrix_key(0x4000).ok_or("matrix decode")?;
            let mut debounce =
                Debouncer::new(Timing::new(20, 500).map_err(|e| format!("timing: {e:?}"))?);
            for time in [0, 19] {
                assert_eq!(
                    debounce
                        .update(time, true)
                        .map_err(|e| format!("clock: {e:?}"))?,
                    None
                );
            }
            let event = debounce
                .update(20, true)
                .map_err(|e| format!("clock: {e:?}"))?
                .ok_or("missing debounced event")?;
            if !session
                .navigate(key, event)
                .map_err(|e| format!("navigation: {e:?}"))?
            {
                return Err("navigation missing".into());
            }
        }
        let view = session.snapshot().ok_or("missing snapshot")?;
        let panel = panel.borrow();
        let name = format!("simulated-slot-{:04}.png", view.index() + 1);
        panel.save_png(&output.join(&name))?;
        frames.push(serde_json::json!({"image":name,"index":view.index(),"active":view.channel.is_active(),"rx_code":view.channel.rx_frequency_code(),"tx_code":view.channel.tx_frequency_code(),"name_raw_hex":view.channel.name_raw().iter().map(|b|format!("{b:02x}")).collect::<String>(),"lcd_commands":panel.commands,"pixel_words_including_init":panel.pixel_words,"modeled_delay_ms":milliseconds.get()}));
    }
    assert!(!session
        .navigate(Key::Ptt, ButtonEvent::Pressed)
        .map_err(|e| format!("PTT no-action: {e:?}"))?);
    assert_eq!(session.state(), State::Ready);
    let (memory, _display) = session.into_parts();
    if memory.selected {
        return Err("NOR CS left selected".into());
    }
    assert_eq!(memory.reads, [(0, 4096), (0x4000, 48), (0x4030, 48)]);
    let report = serde_json::json!({"kind":"host_protocol_simulation","composition":"uv98-board-diagnostic Session","lcd_initializations":1,"hardware_access":false,"flash_performed":false,"physical_display_verified":false,"input_sha256":input_hash,"calibration_sha256":cal_hash,"diagnostic_state_bytes_on_host":std::mem::size_of::<Diagnostic>(),"render_row_bytes":WIDTH as usize*2,"reads":memory.reads,"frames":frames,"limitations":["not a full MCU or panel emulator","not a hardware boot test","calibration layout check is not an integrity checksum","ASCII preview substitutes unsupported name bytes; original bytes preserved","all channel slots are raw storage indices, not zone navigation"]});
    let report_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("simulation-report.json"))?;
    serde_json::to_writer_pretty(report_file, &report)?;
    println!("Offline diagnostic simulation passed; two PNG frames and report saved.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_output_does_not_overwrite_existing_bytes() {
        let path = std::env::temp_dir().join(format!(
            "uv98-sim-no-overwrite-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, b"preserved input").unwrap();
        drop(file);
        assert!(Panel::new().save_png(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preserved input");
        fs::remove_file(path).unwrap();
    }
}
