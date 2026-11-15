//! Firmware UI. Opening this page or validating a file never opens a COM port.
//! The one destructive entry point is the explicitly armed Start button.
use crate::firmware::{self, FlashReport, PreparedImage, Progress};
use eframe::egui::{self, Color32, RichText};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ACCENT: Color32 = Color32::from_rgb(100, 204, 202);
const WARNING: Color32 = Color32::from_rgb(245, 190, 100);
const ERROR: Color32 = Color32::from_rgb(255, 172, 133);

enum Event {
    Progress(Progress),
    Finished(Result<FlashReport, String>),
}

pub struct FirmwarePane {
    image: Option<PreparedImage>,
    selected_path: Option<PathBuf>,
    port: String,
    ports: Vec<String>,
    logs_parent: PathBuf,
    session_dir: Option<PathBuf>,
    receiver: Option<Receiver<Event>>,
    progress: Option<Progress>,
    armed: bool,
    preview: bool,
    message: String,
    error: bool,
    finished: bool,
    log: Vec<String>,
}

impl Default for FirmwarePane {
    fn default() -> Self {
        let base = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let mut pane = Self {
            image: None,
            selected_path: None,
            port: String::new(),
            ports: Vec::new(),
            logs_parent: base.join("logs"),
            session_dir: None,
            receiver: None,
            progress: None,
            armed: false,
            preview: false,
            message: "Выберите образ из комплекта и COM-порт кабеля.".into(),
            error: false,
            finished: false,
            log: Vec::new(),
        };
        if let Some(candidate) = firmware::default_image(&base) {
            pane.load_image(candidate);
        }
        pane
    }
}

impl FirmwarePane {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn set_preview(&mut self, preview: bool) {
        self.preview = preview;
        self.armed = false;
    }

    pub fn load_image(&mut self, path: PathBuf) {
        if self.busy() {
            return;
        }
        self.armed = false;
        self.finished = false;
        self.progress = None;
        self.session_dir = None;
        self.log.clear();
        self.selected_path = Some(path.clone());
        match firmware::prepare(&path) {
            Ok(image) => {
                self.image = Some(image);
                self.error = false;
                self.message =
                    "Образ проверен по каталогу комплекта. Запись ещё не начиналась.".into();
            }
            Err(error) => {
                self.image = None;
                self.error = true;
                self.message = format!("Образ не допущен к записи: {error}");
            }
        }
    }

    pub fn close_blocked(&mut self) {
        self.message = "Сначала дождитесь результата записи. Не отключайте питание и кабель. Окно можно будет закрыть после закрытия COM-порта.".into();
        self.error = false;
    }

    fn append_log(&mut self, text: String) {
        if self.log.last() == Some(&text) {
            return;
        }
        if self.log.len() == 256 {
            self.log.remove(0);
        }
        self.log.push(text);
    }

    fn personalize_image(&mut self) {
        let Some(source) = self.image.clone() else {
            return;
        };
        self.armed = false;
        let Some(manifest) = rfd::FileDialog::new()
            .set_title("manifest.json полного снимка CPS (4 МиБ, два совпавших чтения)")
            .add_filter("Манифест резервной копии", &["json"])
            .pick_file()
        else {
            return;
        };
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Папка для нового персонального образа (исходники не изменяются)")
            .pick_folder()
        else {
            return;
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let output = parent.join(format!("UV98-personal-{stamp}"));
        match firmware::profile::personalize(&source, &manifest, &output) {
            Ok(image) => {
                self.selected_path = Some(image.path.clone());
                self.image = Some(image);
                self.finished = false;
                self.progress = None;
                self.session_dir = None;
                self.log.clear();
                self.error = false;
                self.message = format!("Персональный образ готов и выбран: {}. Изменён только SHA256 ваших калибровок. Запись ещё не начиналась.", output.display());
            }
            Err(error) => {
                self.error = true;
                self.message = format!("Образ не подготовлен: {error}");
            }
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    let finished = matches!(event, Event::Finished(_));
                    events.push(event);
                    if finished {
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    events.push(Event::Finished(Err(
                        "Поток записи завершился без результата. Завершение прошивки не подтверждено.".into(),
                    )));
                    break;
                }
            }
        }
        for event in events {
            match event {
                Event::Progress(progress) => {
                    let stage_changed = self
                        .progress
                        .as_ref()
                        .is_none_or(|old| old.phase != progress.phase);
                    if stage_changed
                        || progress.completed_blocks.is_multiple_of(16)
                        || progress.completed_blocks == progress.total_blocks
                    {
                        self.append_log(format!(
                            "{} · подтверждено {} / {} блоков",
                            progress.phase, progress.completed_blocks, progress.total_blocks
                        ));
                    }
                    self.progress = Some(progress);
                }
                Event::Finished(result) => {
                    self.receiver = None;
                    self.armed = false;
                    match result {
                        Ok(report) => {
                            self.finished = true;
                            self.error = false;
                            self.message = format!(
                                "Запись завершена: загрузчик подтвердил {} / {} блоков и окончание передачи. COM-порт закрыт.",
                                report.confirmed_blocks, report.total_blocks
                            );
                        }
                        Err(error) => {
                            self.finished = false;
                            self.error = true;
                            self.message = format!("Запись не завершена: {error}");
                        }
                    }
                    self.append_log(self.message.clone());
                }
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    fn start(&mut self, ctx: &egui::Context, other_busy: bool) -> Result<(), String> {
        if self.preview {
            return Err("Просмотр интерфейса: доступ к COM отключён.".into());
        }
        if self.busy() || other_busy {
            return Err("Другая операция с рацией ещё не завершена.".into());
        }
        if !self.armed {
            return Err("Сначала подтвердите готовность загрузчика.".into());
        }
        let image = self.image.clone().ok_or("Нет проверенного образа")?;
        let port = self.port.trim().to_string();
        if port.is_empty() {
            return Err("Выберите COM-порт кабеля.".into());
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let directory = self
            .logs_parent
            .join(format!("UV98-flash-{stamp}-{}", std::process::id()));
        let output = directory.clone();
        let (tx, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        thread::Builder::new()
            .name("uv98-firmware".into())
            .spawn(move || {
                let result = firmware::flash_windows(image, &port, &output, |progress| {
                    let _ = tx.send(Event::Progress(progress));
                    repaint.request_repaint();
                });
                let _ = tx.send(Event::Finished(result));
                repaint.request_repaint();
            })
            .map_err(|error| format!("Не удалось запустить запись: {error}"))?;
        self.receiver = Some(receiver);
        self.session_dir = Some(directory);
        self.progress = None;
        self.finished = false;
        self.armed = false;
        self.error = false;
        self.log.clear();
        self.message = "Проверка файла и журнала, затем подключение к загрузчику…".into();
        self.append_log(self.message.clone());
        Ok(())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, other_busy: bool) {
        ui.heading("Прошивка UV98 Plus");
        ui.label("Образ Rust или восстановление стока — через штатный загрузчик и обычный кабель.");
        if self.preview {
            ui.colored_label(
                WARNING,
                "ПРОСМОТР ИНТЕРФЕЙСА · COM-порты не открываются, запись недоступна.",
            );
        }
        ui.add_space(10.0);
        let busy = self.busy();
        ui.add_enabled_ui(!busy && !other_busy, |ui| {
            ui.group(|ui| {
                ui.label(
                    RichText::new("1  Образ из комплекта")
                        .strong()
                        .color(ACCENT),
                );
                ui.horizontal(|ui| {
                    if ui.button("Выбрать .bin…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Образ UV98 из проверенного комплекта")
                            .add_filter("Прошивка UV98", &["bin"])
                            .pick_file()
                        {
                            self.load_image(path);
                        }
                    }
                    if let Some(image) = &self.image {
                        ui.colored_label(ACCENT, &image.label);
                        ui.label(format!("{} байт", image.bytes_len));
                    } else {
                        ui.label("Проверенный образ не выбран");
                    }
                });
                if let Some(path) = &self.selected_path {
                    ui.small(path.display().to_string());
                }
                if let Some(image) = &self.image {
                    ui.monospace(format!("SHA256  {}", image.sha256));
                }
                ui.small(
                    "Принимаются образы каталога комплекта и проверенные персональные копии на их основе.",
                );
                ui.add_space(5.0);
                if ui.add_enabled(self.image.is_some(), egui::Button::new("Подготовить образ для моей рации…")).clicked() {
                    self.personalize_image();
                }
                ui.small("Для первого запуска на другой рации: шаблон текущего выпуска → manifest.json полного снимка 4 МиБ → новая папка с BIN и каталогом. COM не используется; дампы и исходный BIN сохраняются.");
                ui.colored_label(WARNING, "Только для той же подтверждённой аппаратной ревизии. Проверка снимка не подтверждает совместимость другой платы.");
            });
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(
                    RichText::new("2  Кабель и загрузчик")
                        .strong()
                        .color(ACCENT),
                );
                ui.horizontal(|ui| {
                    ui.label("COM-порт");
                    if ui
                        .add(egui::TextEdit::singleline(&mut self.port).desired_width(100.0))
                        .changed()
                    {
                        self.armed = false;
                    }
                    let old_port = self.port.clone();
                    egui::ComboBox::from_id_salt("firmware_ports")
                        .selected_text("Доступные порты")
                        .show_ui(ui, |ui| {
                            for port in &self.ports {
                                ui.selectable_value(&mut self.port, port.clone(), port);
                            }
                        });
                    if self.port != old_port {
                        self.armed = false;
                    }
                    if ui.button("Обновить список").clicked() {
                        match uv98_radio_read::list_ports() {
                            Ok(ports) => {
                                self.ports = ports;
                                self.error = false;
                                self.message = format!(
                                    "Найдено портов: {}. Порты не открывались.",
                                    self.ports.len()
                                );
                            }
                            Err(error) => {
                                self.error = true;
                                self.message = error.to_string();
                            }
                        }
                    }
                });
                ui.label("Закройте другие CPS и Radio Tuning Software. Выключите рацию ручкой.");
                ui.label("Зажмите верхнюю PTT, включите рацию, через 2–3 секунды отпустите PTT.");
                ui.label(
                    "Загрузчик: чёрный экран и зелёный светодиод. Оставьте кабель подключённым.",
                );
                ui.checkbox(
                    &mut self.armed,
                    "Рация в загрузчике, выбран нужный COM-порт, питание устойчивое",
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Папка журналов…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Папка журналов прошивки")
                        .pick_folder()
                    {
                        self.logs_parent = path;
                        self.armed = false;
                    }
                }
                ui.small(self.logs_parent.display().to_string());
            });
            ui.add_space(8.0);
            if ui
                .add_enabled(
                    self.image.is_some()
                        && self.armed
                        && !self.port.trim().is_empty()
                        && !self.preview,
                    egui::Button::new(RichText::new("Записать прошивку").strong())
                        .fill(Color32::from_rgb(26, 105, 112)),
                )
                .clicked()
            {
                if let Err(error) = self.start(ui.ctx(), other_busy) {
                    self.error = true;
                    self.message = error;
                }
            }
        });
        if other_busy {
            ui.colored_label(
                WARNING,
                "Дождитесь завершения чтения на вкладке «Резервные копии».",
            );
        }
        ui.add_space(10.0);
        if self.busy() {
            let fraction = self.progress.as_ref().map_or(0.0, |progress| {
                progress.completed_blocks as f32 / progress.total_blocks.max(1) as f32
            });
            let text = self.progress.as_ref().map_or_else(
                || "Подготовка…".into(),
                |progress| {
                    format!(
                        "{} · {} / {} блоков",
                        progress.phase, progress.completed_blocks, progress.total_blocks
                    )
                },
            );
            ui.add(
                egui::ProgressBar::new(fraction)
                    .show_percentage()
                    .text(text),
            );
            ui.colored_label(
                WARNING,
                "Запись выполняется. Не выключайте рацию, не вынимайте кабель и не закрывайте CPS.",
            );
        }
        ui.colored_label(if self.error { ERROR } else { ACCENT }, &self.message);
        if self.finished {
            ui.label(
                RichText::new("Теперь выключите рацию на 3 секунды и включите без PTT.").strong(),
            );
            ui.small("Подтверждения загрузчика означают завершение передачи файла. Запуск, экран и радиосвязь проверяются на самой рации.");
        } else if self.error && self.session_dir.is_some() {
            if self
                .progress
                .as_ref()
                .is_some_and(|progress| progress.destructive_started)
            {
                ui.colored_label(WARNING, "Стирание приложения уже начиналось. Не рассчитывайте на обычный запуск до успешной повторной записи.");
            }
            ui.label("Сохраните журнал. Для повторной записи заново включите загрузчик; автоматической повторной записи нет.");
            ui.small("Если COM-порт недоступен: закройте программы, переподключите USB кабеля и обновите список портов.");
        }
        if let Some(path) = &self.session_dir {
            ui.small(format!("Журнал операции: {}", path.display()));
        }
        egui::CollapsingHeader::new("Подробности операции")
            .default_open(self.busy() || self.error)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("firmware_log")
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.log.is_empty() {
                            ui.small("Запись ещё не запускалась.");
                        }
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });
        ui.add_space(8.0);
        ui.small("Записывается приложение MCU. Заводские калибровки и каналы во внешней памяти эта операция не записывает.");
        ui.small("Возврат стока: выберите собственный совместимый заводской BIN; CPS проверит его по каталогу. Публичный комплект не содержит заводского BIN.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_without_file_or_port() -> FirmwarePane {
        FirmwarePane {
            image: None,
            selected_path: None,
            port: String::new(),
            ports: Vec::new(),
            logs_parent: PathBuf::from("unused-offline-test-directory"),
            session_dir: None,
            receiver: None,
            progress: None,
            armed: false,
            preview: false,
            message: String::new(),
            error: false,
            finished: false,
            log: Vec::new(),
        }
    }

    #[test]
    fn preview_and_other_operation_cannot_start_worker() {
        let ctx = egui::Context::default();
        let mut pane = pane_without_file_or_port();
        pane.preview = true;
        pane.armed = true;
        assert!(pane
            .start(&ctx, false)
            .unwrap_err()
            .contains("COM отключён"));
        assert!(!pane.busy());
        pane.preview = false;
        assert!(pane
            .start(&ctx, true)
            .unwrap_err()
            .contains("Другая операция"));
        assert!(!pane.busy());
        pane.armed = false;
        assert!(pane
            .start(&ctx, false)
            .unwrap_err()
            .contains("готовность загрузчика"));
        assert!(!pane.busy());
    }

    #[test]
    fn full_block_progress_does_not_claim_success_without_terminal_result() {
        let mut pane = pane_without_file_or_port();
        let (tx, rx) = mpsc::channel();
        pane.receiver = Some(rx);
        tx.send(Event::Progress(Progress {
            phase: "Блоки подтверждены".into(),
            completed_blocks: 246,
            total_blocks: 246,
            destructive_started: true,
        }))
        .unwrap();
        let ctx = egui::Context::default();
        pane.poll(&ctx);
        assert!(pane.busy());
        assert!(!pane.finished);
        drop(tx);
        pane.poll(&ctx);
        assert!(!pane.busy());
        assert!(!pane.finished);
        assert!(pane.error);
        assert!(pane.message.contains("не подтверждено"));
    }

    #[test]
    fn terminal_failure_disarms_retry_and_keeps_destructive_progress() {
        let mut pane = pane_without_file_or_port();
        pane.armed = true;
        let (tx, rx) = mpsc::channel();
        pane.receiver = Some(rx);
        tx.send(Event::Progress(Progress {
            phase: "Стирание".into(),
            completed_blocks: 0,
            total_blocks: 246,
            destructive_started: true,
        }))
        .unwrap();
        tx.send(Event::Finished(Err("Таймаут ACK".into()))).unwrap();
        pane.poll(&egui::Context::default());
        assert!(!pane.busy());
        assert!(!pane.armed);
        assert!(!pane.finished);
        assert!(pane.error);
        assert!(pane.progress.unwrap().destructive_started);
    }
}
