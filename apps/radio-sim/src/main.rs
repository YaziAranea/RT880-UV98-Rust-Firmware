#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use eframe::egui::{self, Color32, RichText};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use uv98_keys::{ButtonEvent, Key};
use uv98_radio_sim::{rgb, Simulator};
use uv98_ui_core::Action;

fn load(path: &Path) -> Result<(Simulator, String), String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() != uv98_codeplug::FILE_LEN as u64 {
        return Err("Файл должен содержать ровно 221184 байта".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let sim = Simulator::from_codeplug(
        &bytes,
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    )?;
    Ok((sim, hash))
}
fn png(path: &Path, pixels: &[u16]) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut encoder = png::Encoder::new(file, 320, 240);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgb(pixels))?;
    Ok(())
}
fn preview(dir: &Path, input: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let (mut sim, hash) = if let Some(path) = input {
        load(path)?
    } else {
        (Simulator::demo(), "demo".to_string())
    };
    // Existing output is rejected before any file is written.
    fs::create_dir(dir)?;
    png(&dir.join("01-channel.png"), &sim.frame())?;
    sim.key(Key::Menu, ButtonEvent::Pressed);
    png(&dir.join("02-menu.png"), &sim.frame())?;
    sim.key(Key::Menu, ButtonEvent::Pressed);
    for _ in 0..5000 {
        sim.step();
        if matches!(sim.spectrum.state(), uv98_spectrum::State::SweepComplete) {
            break;
        }
    }
    assert!(sim.spectrum.bins().iter().all(|b| b.latest.is_some()));
    png(&dir.join("03-spectrum.png"), &sim.frame())?;
    let sweep = sim.spectrum.sweep();
    sim.key(Key::Ptt, ButtonEvent::Pressed);
    sim.key(Key::Ptt, ButtonEvent::Released);
    for _ in 0..1000 {
        sim.step();
    }
    assert!(!sim.spectrum.is_running());
    assert_eq!(sim.spectrum.sweep(), sweep);
    png(&dir.join("04-ptt-return.png"), &sim.frame())?;
    if let Some(path) = input {
        assert_eq!(load(path)?.1, hash);
    }
    let report = serde_json::json!({
        "status":"offline_preview_passed", "input_sha256":hash,
        "input_unchanged":true, "width":320,"height":240,
        "spectrum_bins":sim.spectrum.range().len(),"complete_sweep":sweep,
        "spectrum_data":"synthetic", "hardware_access":false,"flash_performed":false,
        "shared_core":"uv98-ui-core + uv98-spectrum + uv98-radio-ui",
        "ptt_stops_and_release_does_not_restart":true,
        "renderer_row_bytes":640,
        "engine_bytes_host":std::mem::size_of::<uv98_spectrum::Engine>(),
        "controller_bytes_host":std::mem::size_of::<uv98_ui_core::Controller>()
    });
    fs::write(
        dir.join("preview-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

struct App {
    sim: Simulator,
    texture: Option<egui::TextureHandle>,
    last_tick: Instant,
    dirty: bool,
    message: Option<String>,
}

/// Reserve the radio keys before any egui widgets inspect the current input.
/// egui derives key_pressed from this event list, not keys_down. Removing both
/// presses and repeats therefore also prevents focused buttons from turning
/// Enter/Space into a second click. Modifier chords do not issue radio actions;
/// Space release always clears the PTT latch even when modifiers changed.
fn take_radio_keys(events: &mut Vec<egui::Event>) -> Vec<(Key, ButtonEvent)> {
    let mut actions = Vec::new();
    events.retain(|event| {
        let egui::Event::Key {
            key,
            pressed,
            repeat,
            modifiers,
            ..
        } = event
        else {
            return true;
        };
        let mapped = match key {
            egui::Key::ArrowUp => Key::Up,
            egui::Key::ArrowDown => Key::Down,
            egui::Key::Enter => Key::Menu,
            egui::Key::Escape => Key::Exit,
            egui::Key::Space => Key::Ptt,
            egui::Key::Num0 => Key::Digit0,
            egui::Key::Num1 => Key::Digit1,
            egui::Key::Num2 => Key::Digit2,
            egui::Key::Num3 => Key::Digit3,
            egui::Key::Num4 => Key::Digit4,
            egui::Key::Num5 => Key::Digit5,
            egui::Key::Num6 => Key::Digit6,
            egui::Key::Num7 => Key::Digit7,
            egui::Key::Num8 => Key::Digit8,
            egui::Key::Num9 => Key::Digit9,
            _ => return true,
        };
        if mapped == Key::Ptt && !pressed {
            actions.push((Key::Ptt, ButtonEvent::Released));
        } else if !repeat && !modifiers.ctrl && !modifiers.alt && !modifiers.command {
            actions.push((
                mapped,
                if *pressed {
                    ButtonEvent::Pressed
                } else {
                    ButtonEvent::Released
                },
            ));
        }
        // Consume even ignored repeats/modifier variants of reserved keys:
        // they must not activate a focused host control behind the radio UI.
        false
    });
    actions
}

impl App {
    fn new() -> Self {
        Self {
            sim: Simulator::demo(),
            texture: None,
            last_tick: Instant::now(),
            dirty: true,
            message: None,
        }
    }
    fn key(&mut self, key: Key, event: ButtonEvent) {
        self.sim.key(key, event);
        self.dirty = true;
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        let actions = ctx.input_mut(|input| take_radio_keys(&mut input.events));
        for (key, event) in actions {
            self.key(key, event);
        }
        if !ctx.input(|i| i.focused) && self.sim.ui.view().ptt_pressed {
            self.key(Key::Ptt, ButtonEvent::Released);
        }
    }
    fn button(&mut self, ui: &mut egui::Ui, text: &str, key: Key) {
        if ui
            .add_sized([70.0, 42.0], egui::Button::new(text))
            .clicked()
        {
            self.key(key, ButtonEvent::Pressed);
            self.key(key, ButtonEvent::Released);
        }
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;

    fn key_event(
        key: egui::Key,
        pressed: bool,
        repeat: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn modifier_change_on_space_release_clears_ptt_without_restarting_spectrum() {
        for modifiers in [
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
            egui::Modifiers {
                alt: true,
                ..Default::default()
            },
            egui::Modifiers {
                command: true,
                ..Default::default()
            },
        ] {
            let mut sim = Simulator::demo();
            sim.action(Action::OpenSpectrum);
            assert!(sim.spectrum.is_running());
            let mut press = vec![key_event(
                egui::Key::Space,
                true,
                false,
                egui::Modifiers::NONE,
            )];
            for (key, event) in take_radio_keys(&mut press) {
                sim.key(key, event);
            }
            assert!(press.is_empty());
            assert!(sim.ui.view().ptt_pressed);
            let mut release = vec![key_event(egui::Key::Space, false, false, modifiers)];
            for (key, event) in take_radio_keys(&mut release) {
                sim.key(key, event);
            }
            assert!(release.is_empty());
            assert!(!sim.ui.view().ptt_pressed);
            assert!(!sim.spectrum.ptt_held());
            for _ in 0..10 {
                sim.step();
            }
            assert!(!sim.spectrum.is_running());
            assert!(!sim.ui.view().spectrum_open);
        }
    }

    #[test]
    fn reserved_repeats_and_modifier_chords_are_consumed_without_extra_actions() {
        let mut events = vec![
            key_event(egui::Key::Enter, true, false, egui::Modifiers::NONE),
            key_event(egui::Key::Enter, true, true, egui::Modifiers::NONE),
            key_event(egui::Key::Space, true, false, egui::Modifiers::CTRL),
            key_event(egui::Key::Space, false, false, egui::Modifiers::CTRL),
            key_event(egui::Key::Num9, true, false, egui::Modifiers::NONE),
            key_event(egui::Key::F1, true, false, egui::Modifiers::NONE),
        ];
        assert_eq!(
            take_radio_keys(&mut events),
            vec![
                (Key::Menu, ButtonEvent::Pressed),
                (Key::Ptt, ButtonEvent::Released),
                (Key::Digit9, ButtonEvent::Pressed),
            ]
        );
        assert_eq!(
            events,
            vec![key_event(egui::Key::F1, true, false, egui::Modifiers::NONE)]
        );
    }

    #[test]
    fn consumed_enter_and_space_do_not_click_a_focused_egui_control() {
        for key in [egui::Key::Enter, egui::Key::Space] {
            for consume in [false, true] {
                let ctx = egui::Context::default();
                ctx.begin_pass(egui::RawInput {
                    events: vec![key_event(key, true, false, egui::Modifiers::NONE)],
                    focused: true,
                    ..Default::default()
                });
                if consume {
                    let actions = ctx.input_mut(|input| take_radio_keys(&mut input.events));
                    assert_eq!(actions.len(), 1);
                    assert!(!ctx.input(|input| input.key_pressed(key)));
                }
                let id = egui::Id::new("focused-control");
                ctx.memory_mut(|memory| memory.request_focus(id));
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 40.0));
                let ui = egui::Ui::new(
                    ctx.clone(),
                    egui::Id::new("test-root"),
                    egui::UiBuilder::new().max_rect(rect),
                );
                let response = ui.interact(rect, id, egui::Sense::click());
                assert_eq!(
                    response.clicked(),
                    !consume,
                    "key={key:?}, consume={consume}"
                );
                let mut output = ctx.end_pass();
                // Headless input regression: no painter consumes the font
                // atlas update, so explicitly discard its texture commands.
                output.textures_delta.clear();
            }
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.keyboard(&ctx);
        if self.last_tick.elapsed() >= Duration::from_millis(50) {
            if self.sim.spectrum.is_running() {
                for _ in 0..16 {
                    self.sim.step();
                }
                self.dirty = true;
            }
            self.last_tick = Instant::now();
        }
        if self.dirty || self.texture.is_none() {
            let image = egui::ColorImage::from_rgb([320, 240], &rgb(&self.sim.frame()));
            if let Some(texture) = &mut self.texture {
                texture.set(image, egui::TextureOptions::NEAREST);
            } else {
                self.texture =
                    Some(ctx.load_texture("radio-lcd", image, egui::TextureOptions::NEAREST));
            }
            self.dirty = false;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("UV98 · Radio UI");
                ui.label(RichText::new("ПРОТОТИП 0.1").color(Color32::from_rgb(77, 215, 203)));
            });
            ui.label("Экран 320 × 240. График демонстрационный — все сигналы созданы программно.");
            ui.add_space(10.0);
            ui.horizontal_top(|ui| {
                egui::Frame::new().fill(Color32::from_rgb(3, 9, 15)).inner_margin(10.0).show(ui, |ui| {
                    if let Some(texture) = &self.texture { ui.image((texture.id(), egui::vec2(640.0, 480.0))); }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(225.0);
                    ui.label(RichText::new("УПРАВЛЕНИЕ").strong());
                    ui.horizontal(|ui| { self.button(ui, "MENU", Key::Menu); self.button(ui, "UP", Key::Up); self.button(ui, "EXIT", Key::Exit); });
                    ui.horizontal(|ui| { self.button(ui, "1", Key::Digit1); self.button(ui, "2", Key::Digit2); self.button(ui, "3", Key::Digit3); });
                    ui.horizontal(|ui| { self.button(ui, "4", Key::Digit4); self.button(ui, "5", Key::Digit5); self.button(ui, "6", Key::Digit6); });
                    ui.horizontal(|ui| { self.button(ui, "7", Key::Digit7); self.button(ui, "8", Key::Digit8); self.button(ui, "9", Key::Digit9); });
                    ui.horizontal(|ui| { self.button(ui, "*", Key::Star); self.button(ui, "0", Key::Digit0); self.button(ui, "DOWN", Key::Down); });
                    ui.add_space(8.0);
                    if ui.button("Открыть спектр").clicked() { self.sim.action(Action::OpenSpectrum); self.dirty = true; }
                    if ui.add_enabled(self.sim.ui.view().spectrum_open, egui::Button::new(if self.sim.spectrum.is_running() { "Пауза обзора" } else { "Продолжить обзор" })).clicked() { self.sim.toggle_sweep(); self.dirty = true; }
                    if ui.button("PTT: остановить обзор").clicked() { self.key(Key::Ptt, ButtonEvent::Pressed); self.key(Key::Ptt, ButtonEvent::Released); }
                    if ui.add_enabled(self.sim.ui.view().spectrum_open, egui::Button::new("Сбросить пики")).clicked() { self.sim.action(Action::ResetPeaks); self.dirty = true; }
                    ui.add_space(8.0);
                    ui.small("Enter: Menu   Esc: Exit\nВверх/вниз: канал или курсор\nЦифры + Enter: номер канала\nПробел: PTT (передачи нет)");
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Открыть .880dmf / .98pdmf").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("CPS channels", &["880dmf", "98pdmf"]).pick_file() {
                        match load(&path) { Ok((sim, _)) => { self.sim = sim; self.dirty = true; self.message = None; }, Err(e) => self.message = Some(e) }
                    }
                }
                ui.label(format!("Каналы: {}", self.sim.source));
            });
            ui.small("Файл открывается только для чтения. Выбор канала меняет экран; COM-порт не используется.");
            if let Some(error) = self.sim.last_error.as_ref().or(self.message.as_ref()) { ui.colored_label(Color32::LIGHT_RED, error); }
        });
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--preview") {
        if !(2..=3).contains(&args.len()) {
            return Err("usage: --preview NEW_DIR [CODEPLUG]".into());
        }
        return preview(&PathBuf::from(&args[1]), args.get(2).map(Path::new));
    }
    if !args.is_empty() {
        return Err("usage: no arguments for GUI; --preview NEW_DIR [CODEPLUG]".into());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([970.0, 700.0])
            .with_min_inner_size([970.0, 700.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "UV98 Radio UI — offline spectrum demo",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.global_style()).clone();
            style.visuals = egui::Visuals::dark();
            style.visuals.panel_fill = Color32::from_rgb(17, 26, 36);
            style.spacing.item_spacing = egui::vec2(6.0, 6.0);
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            cc.egui_ctx.set_global_style(style);
            Ok(Box::new(App::new()))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
