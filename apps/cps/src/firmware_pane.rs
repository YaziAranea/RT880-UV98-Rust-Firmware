//! Firmware UI. Opening this page or validating a file never opens a COM port.
//! The one destructive entry point is the explicitly armed Start button.
use crate::firmware::{self, FlashReport, PreparedImage, Progress};
use crate::i18n::{Language, LocalizedText};
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

mod voice;

enum Event {
    Progress(Progress),
    Finished(Result<FlashReport, LocalizedText>),
}

pub struct FirmwarePane {
    image: Option<PreparedImage>,
    voice: voice::VoiceImport,
    selected_path: Option<PathBuf>,
    port: String,
    ports: Vec<String>,
    logs_parent: PathBuf,
    session_dir: Option<PathBuf>,
    receiver: Option<Receiver<Event>>,
    progress: Option<Progress>,
    armed: bool,
    preview: bool,
    language: Language,
    message: LocalizedText,
    error: bool,
    finished: bool,
    log: Vec<LocalizedText>,
}

impl Default for FirmwarePane {
    fn default() -> Self {
        let base = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let mut pane = Self {
            image: None,
            voice: Default::default(),
            selected_path: None,
            port: String::new(),
            ports: Vec::new(),
            logs_parent: base.join("logs"),
            session_dir: None,
            receiver: None,
            progress: None,
            armed: false,
            preview: false,
            language: Language::default(),
            message: msg!(
                "Выберите BIN и COM-порт кабеля.",
                "Select a BIN and the cable's COM port.",
                "Wählen Sie eine BIN und den COM-Port des Kabels."
            ),
            error: false,
            finished: false,
            log: Vec::new(),
        };
        if let Some(image) = firmware::default_image(&base) {
            pane.set_prepared(image);
        }
        pane
    }
}

impl FirmwarePane {
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    fn has_voice_edits(&self) -> bool {
        self.voice.pending()
            || self
                .image
                .as_ref()
                .is_some_and(|image| self.voice.config_changed(image))
    }

    fn voice_draft_notice() -> LocalizedText {
        msg!(
            "Сохраните голосовые настройки и запись в BIN или отмените изменения.",
            "Save the voice settings and recording in a BIN or discard the changes.",
            "Spracheinstellungen und Aufnahme in einer BIN speichern oder Änderungen verwerfen."
        )
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
        if self.has_voice_edits() {
            self.error = true;
            self.message = Self::voice_draft_notice();
            return;
        }
        match firmware::prepare(&path) {
            Ok(image) => {
                self.finished = false;
                self.progress = None;
                self.session_dir = None;
                self.log.clear();
                self.set_prepared(image);
            }
            Err(error) => {
                self.error = true;
                self.message = LocalizedText::from_fn(|lang| {
                    trf!(
                        lang,
                        "Образ не допущен к записи: {error}",
                        "Image rejected for writing: {error}",
                        "Image wurde zum Schreiben abgelehnt: {error}",
                        error = error.as_str(lang)
                    )
                });
            }
        }
    }

    fn set_prepared(&mut self, image: PreparedImage) {
        self.voice = voice::VoiceImport::for_image(&image);
        self.selected_path = Some(image.path.clone());
        self.image = Some(image);
        self.error = false;
        self.message = msg!(
            "BIN выбран. Запись ещё не начиналась.",
            "BIN selected. Writing has not started.",
            "BIN ausgewählt. Der Schreibvorgang hat noch nicht begonnen.",
        );
    }

    pub fn close_blocked(&mut self) {
        self.message = msg!(
            "Сначала дождитесь результата записи. Не отключайте питание и кабель. Окно можно будет закрыть после закрытия COM-порта.",
            "Wait for the write result. Keep the radio powered and the cable connected. Close this window once the COM port closes.",
            "Warten Sie auf das Ergebnis des Schreibvorgangs. Lassen Sie das Funkgerät eingeschaltet und das Kabel angeschlossen. Das Fenster kann nach dem Schließen des COM-Ports geschlossen werden.",
        );
        self.error = false;
    }

    fn append_log(&mut self, text: LocalizedText) {
        if self.log.last() == Some(&text) {
            return;
        }
        if self.log.len() == 256 {
            self.log.remove(0);
        }
        self.log.push(text);
    }

    fn personalize_image(&mut self) {
        if self.has_voice_edits() {
            self.error = true;
            self.message = Self::voice_draft_notice();
            return;
        }
        let lang = self.language;
        let Some(source) = self.image.clone() else {
            return;
        };
        self.armed = false;
        let Some(manifest) = rfd::FileDialog::new()
            .set_title(tr!(
                lang,
                "manifest.json полного снимка CPS (4 МиБ, два совпавших чтения)",
                "manifest.json from a full CPS snapshot (4 MiB, two matching reads)",
                "manifest.json eines vollständigen CPS-Abbilds (4 MiB, zwei übereinstimmende Lesedurchläufe)",
            ))
            .add_filter(tr!(
                lang,
                "Манифест резервной копии",
                "Backup manifest",
                "Sicherungsmanifest",
            ), &["json"])
            .pick_file()
        else {
            return;
        };
        let Some(parent) = rfd::FileDialog::new()
            .set_title(tr!(
                lang,
                "Папка для нового персонального образа (исходники не изменяются)",
                "Folder for the new personalized image (source files stay intact)",
                "Ordner für das neue personalisierte Image (Quelldateien bleiben erhalten)"
            ))
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
                self.set_prepared(image);
                self.finished = false;
                self.progress = None;
                self.session_dir = None;
                self.log.clear();
                self.message = msgf!(
                    "Персональный образ готов и выбран: {}. Изменён только SHA256 ваших калибровок. Запись ещё не начиналась.",
                    "Personalized image ready and selected: {}. Only the calibration SHA256 changed. Writing has not started.",
                    "Personalisiertes Image bereit und ausgewählt: {}. Nur die SHA256-Prüfsumme Ihrer Kalibrierdaten wurde geändert. Der Schreibvorgang hat noch nicht begonnen.",
                    output.display(),
                );
            }
            Err(error) => {
                self.error = true;
                self.message = LocalizedText::from_fn(|lang| {
                    trf!(
                        lang,
                        "Образ не подготовлен: {error}",
                        "Could not prepare the image: {error}",
                        "Image konnte nicht vorbereitet werden: {error}",
                        error = error.as_str(lang)
                    )
                });
            }
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.poll_voice(ctx);
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
                        msg!(
                            "Поток записи завершился без результата. Завершение прошивки не подтверждено.",
                            "The write worker stopped without a result. Completion of the flash is unconfirmed.",
                            "Der Schreibprozess endete ohne Ergebnis. Der Abschluss des Schreibvorgangs ist nicht bestätigt.",
                        ),
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
                        self.append_log(LocalizedText::from_fn(|lang| {
                            trf!(
                                lang,
                                "{} · подтверждено {} / {} блоков",
                                "{} · {} / {} blocks acknowledged",
                                "{} · {} / {} Blöcke bestätigt",
                                progress.phase.as_str(lang),
                                progress.completed_blocks,
                                progress.total_blocks,
                            )
                        }));
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
                            self.message = msgf!(
                                "Запись завершена: загрузчик подтвердил {} / {} блоков и окончание передачи. COM-порт закрыт.",
                                "Writing finished: the bootloader acknowledged {} / {} blocks and the end of the transfer. The COM port is closed.",
                                "Schreiben abgeschlossen: Der Bootloader hat {} / {} Blöcke und das Ende der Übertragung bestätigt. Der COM-Port ist geschlossen.",
                                report.confirmed_blocks,
                                report.total_blocks,
                            );
                        }
                        Err(error) => {
                            self.finished = false;
                            self.error = true;
                            self.message = LocalizedText::from_fn(|lang| {
                                trf!(
                                    lang,
                                    "Запись не завершена: {error}",
                                    "Writing did not finish: {error}",
                                    "Schreibvorgang nicht abgeschlossen: {error}",
                                    error = error.as_str(lang)
                                )
                            });
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

    fn start(&mut self, ctx: &egui::Context, other_busy: bool) -> Result<(), LocalizedText> {
        if self.preview {
            return Err(msg!(
                "Просмотр интерфейса: доступ к COM отключён.",
                "Interface preview: COM access is disabled.",
                "Oberflächenvorschau: COM-Zugriff ist deaktiviert."
            ));
        }
        if self.busy() || other_busy {
            return Err(msg!(
                "Другая операция с рацией ещё не завершена.",
                "Another radio operation is still in progress.",
                "Ein anderer Vorgang am Funkgerät läuft noch."
            ));
        }
        if self.has_voice_edits() {
            return Err(Self::voice_draft_notice());
        }
        if !self.armed {
            return Err(msg!(
                "Сначала подтвердите готовность загрузчика.",
                "Confirm that the bootloader is ready first.",
                "Bestätigen Sie zuerst, dass der Bootloader bereit ist."
            ));
        }
        let image = self
            .image
            .clone()
            .ok_or_else(|| msg!("BIN не выбран", "No BIN selected", "Keine BIN ausgewählt"))?;
        let port = self.port.trim().to_string();
        if port.is_empty() {
            return Err(msg!(
                "Выберите COM-порт кабеля.",
                "Select the cable's COM port.",
                "Wählen Sie den COM-Port des Kabels."
            ));
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
            .map_err(|error| {
                msgf!(
                    "Не удалось запустить запись: {error}",
                    "Could not start writing: {error}",
                    "Schreibvorgang konnte nicht gestartet werden: {error}"
                )
            })?;
        self.receiver = Some(receiver);
        self.session_dir = Some(directory);
        self.progress = None;
        self.finished = false;
        self.armed = false;
        self.error = false;
        self.log.clear();
        self.message = msg!(
            "Проверка файла и журнала, затем подключение к загрузчику…",
            "Checking file and log, then connecting to the bootloader…",
            "Datei und Protokoll werden geprüft, danach wird die Verbindung zum Bootloader hergestellt…",
        );
        self.append_log(self.message.clone());
        Ok(())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, other_busy: bool) {
        let lang = self.language;
        ui.heading(tr!(
            lang,
            "Прошивка UV98 Plus",
            "UV98 Plus firmware",
            "UV98-Plus-Firmware"
        ));
        ui.label(tr!(
            lang,
            "Образ Rust или восстановление стока — через штатный загрузчик и обычный кабель.",
            "Use the stock bootloader and standard cable to install a Rust image or restore stock firmware.",
            "Installieren Sie ein Rust-Image oder stellen Sie die Werksfirmware mit dem serienmäßigen Bootloader und Standardkabel wieder her.",
        ));
        if self.preview {
            ui.colored_label(
                WARNING,
                tr!(
                    lang,
                    "ПРОСМОТР ИНТЕРФЕЙСА · COM-порты не открываются, запись недоступна.",
                    "INTERFACE PREVIEW · COM ports stay closed. Writing is unavailable.",
                    "OBERFLÄCHENVORSCHAU · COM-Ports bleiben geschlossen. Schreiben ist deaktiviert.",
                ),
            );
        }
        ui.add_space(10.0);
        let busy = self.busy();
        ui.add_enabled_ui(!busy && !other_busy, |ui| {
            ui.group(|ui| {
                ui.label(
                    RichText::new(tr!(
                        lang,
                        "1  BIN приложения",
                        "1  Application BIN",
                        "1  Anwendungs-BIN",
                    ))
                        .strong()
                        .color(ACCENT),
                );
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.has_voice_edits(), egui::Button::new(tr!(lang, "Выбрать BIN…", "Select BIN…", "BIN auswählen…"))).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title(tr!(
                                lang,
                                "Выбор BIN для UV98 Plus",
                                "Select BIN for UV98 Plus",
                                "BIN für UV98 Plus auswählen",
                            ))
                            .add_filter(tr!(lang, "Прошивка UV98", "UV98 firmware", "UV98-Firmware"), &["bin"])
                            .pick_file()
                        {
                            self.load_image(path);
                        }
                    }
                    if let Some(image) = &self.image {
                        ui.colored_label(ACCENT, image.display_label());
                        ui.label(trf!(lang, "{} байт", "{} bytes", "{} Byte", image.bytes_len));
                    } else {
                        ui.label(tr!(
                            lang,
                            "BIN не выбран",
                            "No BIN selected",
                            "Keine BIN ausgewählt",
                        ));
                    }
                });
                if let Some(path) = &self.selected_path {
                    ui.small(path.display().to_string());
                }
                if let Some(image) = &self.image {
                    ui.monospace(format!("SHA256  {}", image.sha256));
                    ui.small(image.provenance_label(lang));
                }
                ui.add_space(5.0);
                if self.image.as_ref().is_some_and(|image| image.can_add_voice() || image.voice_config().is_some()) {
                    self.voice_controls(ui);
                }
                if ui.add_enabled(
                    !self.has_voice_edits() && self.image.as_ref().is_some_and(PreparedImage::is_personalization_source),
                    egui::Button::new(tr!(
                    lang,
                    "Подготовить образ для моей рации…",
                    "Prepare an image for my radio…",
                    "Image für mein Funkgerät vorbereiten…",
                ))).clicked() {
                    self.personalize_image();
                }
                ui.small(tr!(
                    lang,
                    "Для другой рации выберите исходный BIN и manifest.json полного снимка 4 МиБ с этой рации. CPS сохранит личный BIN, исходный BIN и каталог. Храните их вместе.",
                    "For another radio, select the original BIN and its full 4 MiB snapshot's manifest.json. CPS saves the personal BIN, the source BIN and a catalog. Keep them together.",
                    "Wählen Sie für ein anderes Funkgerät die ursprüngliche BIN und die manifest.json seines vollständigen 4-MiB-Abbilds. CPS speichert die persönliche BIN, die Quell-BIN und einen Katalog. Bewahren Sie diese Dateien zusammen auf.",
                ));
                ui.colored_label(WARNING, tr!(
                    lang,
                    "Только для той же подтверждённой аппаратной ревизии. Проверка снимка не подтверждает совместимость другой платы.",
                    "Use only with the same verified hardware revision. Snapshot verification cannot establish compatibility with another board.",
                    "Nur für dieselbe bestätigte Hardware-Revision verwenden. Die Abbildprüfung kann die Kompatibilität mit einer anderen Platine nicht bestätigen.",
                ));
            });
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(
                    RichText::new(tr!(
                        lang,
                        "2  Кабель и загрузчик",
                        "2  Cable and bootloader",
                        "2  Kabel und Bootloader",
                    ))
                        .strong()
                        .color(ACCENT),
                );
                ui.horizontal(|ui| {
                    ui.label(tr!(lang, "COM-порт", "COM port", "COM-Port"));
                    if ui
                        .add(egui::TextEdit::singleline(&mut self.port).desired_width(100.0))
                        .changed()
                    {
                        self.armed = false;
                    }
                    let old_port = self.port.clone();
                    egui::ComboBox::from_id_salt("firmware_ports")
                        .selected_text(tr!(
                            lang,
                            "Доступные порты",
                            "Available ports",
                            "Verfügbare Ports",
                        ))
                        .show_ui(ui, |ui| {
                            for port in &self.ports {
                                ui.selectable_value(&mut self.port, port.clone(), port);
                            }
                        });
                    if self.port != old_port {
                        self.armed = false;
                    }
                    if ui.button(tr!(lang, "Обновить список", "Refresh list", "Liste aktualisieren")).clicked() {
                        match uv98_radio_read::list_ports() {
                            Ok(ports) => {
                                self.ports = ports;
                                self.error = false;
                                self.message = msgf!(
                                    "Найдено портов: {}. Порты не открывались.",
                                    "Found {} ports. No ports were opened.",
                                    "Gefundene Ports: {}. Es wurden keine Ports geöffnet.",
                                    self.ports.len(),
                                );
                            }
                            Err(error) => {
                                self.error = true;
                                self.message = crate::radio::localize_port_list_error(&error);
                            }
                        }
                    }
                });
                ui.label(tr!(
                    lang,
                    "Закройте другие CPS и Radio Tuning Software. Выключите рацию ручкой.",
                    "Close other CPS programs and Radio Tuning Software. Turn off the radio with its power knob.",
                    "Schließen Sie andere CPS-Programme und Radio Tuning Software. Schalten Sie das Funkgerät am Drehknopf aus.",
                ));
                ui.label(tr!(
                    lang,
                    "Зажмите верхнюю PTT, включите рацию, через 2–3 секунды отпустите PTT.",
                    "Hold the upper PTT, turn on the radio, then release PTT after 2–3 seconds.",
                    "Halten Sie die obere PTT-Taste gedrückt und schalten Sie das Funkgerät ein. Lassen Sie die Taste nach 2–3 Sekunden los.",
                ));
                ui.label(
                    tr!(
                        lang,
                        "Загрузчик: чёрный экран и зелёный светодиод. Оставьте кабель подключённым.",
                        "Bootloader mode shows a black screen and green LED. Keep the cable connected.",
                        "Im Bootloader-Modus bleibt der Bildschirm schwarz und die LED leuchtet grün. Lassen Sie das Kabel angeschlossen.",
                    ),
                );
                if let Some(warning) = self.image.as_ref().and_then(|image| image.source_warning(lang)) {
                    ui.colored_label(WARNING, warning);
                }
                ui.add_enabled(
                    !self.has_voice_edits(),
                    egui::Checkbox::new(&mut self.armed,
                    tr!(
                        lang,
                        "Рация в загрузчике, выбран нужный COM-порт, питание устойчивое",
                        "The radio is in bootloader mode, the correct COM port is selected, and power is stable",
                        "Das Funkgerät ist im Bootloader-Modus, der richtige COM-Port ist ausgewählt und die Stromversorgung ist stabil",
                    )),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!(lang, "Папка журналов…", "Log folder…", "Protokollordner…")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(tr!(
                            lang,
                            "Папка журналов прошивки",
                            "Firmware log folder",
                            "Ordner für Firmware-Protokolle",
                        ))
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
                        && !self.has_voice_edits()
                        && self.armed
                        && !self.port.trim().is_empty()
                        && !self.preview,
                    egui::Button::new(RichText::new(tr!(
                        lang,
                        "Записать прошивку",
                        "Write firmware",
                        "Firmware schreiben",
                    )).strong())
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
                tr!(
                    lang,
                    "Дождитесь завершения чтения на вкладке «Резервные копии».",
                    "Wait for reading to finish in the Backups tab.",
                    "Warten Sie, bis der Lesevorgang im Tab „Sicherungen“ abgeschlossen ist."
                ),
            );
        }
        ui.add_space(10.0);
        if self.busy() {
            let fraction = self.progress.as_ref().map_or(0.0, |progress| {
                progress.completed_blocks as f32 / progress.total_blocks.max(1) as f32
            });
            let text = self.progress.as_ref().map_or_else(
                || tr!(lang, "Подготовка…", "Preparing…", "Vorbereitung…").into(),
                |progress| {
                    trf!(
                        lang,
                        "{} · {} / {} блоков",
                        "{} · {} / {} blocks",
                        "{} · {} / {} Blöcke",
                        progress.phase.as_str(lang),
                        progress.completed_blocks,
                        progress.total_blocks
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
                tr!(
                    lang,
                    "Запись выполняется. Не выключайте рацию, не вынимайте кабель и не закрывайте CPS.",
                    "Writing is in progress. Leave the radio powered on, the cable connected, and CPS open.",
                    "Der Schreibvorgang läuft. Lassen Sie das Funkgerät eingeschaltet, das Kabel angeschlossen und CPS geöffnet.",
                ),
            );
        }
        ui.colored_label(
            if self.error { ERROR } else { ACCENT },
            self.message.as_str(lang),
        );
        if self.finished {
            ui.label(
                RichText::new(tr!(
                    lang,
                    "Теперь выключите рацию на 3 секунды и включите без PTT.",
                    "Turn off the radio for 3 seconds, then turn it back on without pressing PTT.",
                    "Schalten Sie das Funkgerät für 3 Sekunden aus und danach ohne Betätigung der PTT-Taste wieder ein.",
                )).strong(),
            );
            ui.small(tr!(
                lang,
                "Подтверждения загрузчика означают завершение передачи файла. Запуск, экран и радиосвязь проверяются на самой рации.",
                "Bootloader acknowledgements confirm file transfer. Check boot, display, and radio operation on the device.",
                "Die Bootloader-Bestätigungen belegen die Dateiübertragung. Prüfen Sie Start, Display und Funkbetrieb am Gerät.",
            ));
        } else if self.error && self.session_dir.is_some() {
            if self
                .progress
                .as_ref()
                .is_some_and(|progress| progress.destructive_started)
            {
                ui.colored_label(WARNING, tr!(
                    lang,
                    "Стирание приложения уже начиналось. Не рассчитывайте на обычный запуск до успешной повторной записи.",
                    "Application erase already started. A normal boot may require a successful reflash.",
                    "Das Löschen der Anwendung hat bereits begonnen. Ein normaler Start kann ein erneutes erfolgreiches Schreiben erfordern.",
                ));
            }
            ui.label(tr!(
                lang,
                "Сохраните журнал. Для повторной записи заново включите загрузчик; автоматической повторной записи нет.",
                "Keep the log. To try again, restart the radio in bootloader mode and start the write manually.",
                "Bewahren Sie das Protokoll auf. Starten Sie das Funkgerät für einen weiteren Versuch im Bootloader-Modus und lösen Sie den Schreibvorgang manuell aus.",
            ));
            ui.small(tr!(
                lang,
                "Если COM-порт недоступен: закройте программы, переподключите USB кабеля и обновите список портов.",
                "If the COM port is unavailable, close other programs, reconnect the USB cable and refresh the port list.",
                "Falls der COM-Port nicht verfügbar ist, schließen Sie andere Programme, stecken Sie das USB-Kabel erneut ein und aktualisieren Sie die Portliste.",
            ));
        }
        if let Some(path) = &self.session_dir {
            ui.small(trf!(
                lang,
                "Журнал операции: {}",
                "Operation log: {}",
                "Vorgangsprotokoll: {}",
                path.display()
            ));
        }
        egui::CollapsingHeader::new(tr!(
            lang,
            "Подробности операции",
            "Operation details",
            "Vorgangsdetails"
        ))
        .id_salt("firmware_details")
        .default_open(self.busy() || self.error)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("firmware_log")
                .max_height(160.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if self.log.is_empty() {
                        ui.small(tr!(
                            lang,
                            "Запись ещё не запускалась.",
                            "Writing has not started.",
                            "Der Schreibvorgang hat noch nicht begonnen."
                        ));
                    }
                    for line in &self.log {
                        ui.monospace(line.as_str(lang));
                    }
                });
        });
        ui.add_space(8.0);
        ui.small(tr!(
            lang,
            "Записывается приложение MCU. Заводские калибровки и каналы во внешней памяти эта операция не записывает.",
            "This writes the MCU application. Factory calibration data and channels in external memory stay intact.",
            "Der Vorgang schreibt die MCU-Anwendung. Werkskalibrierdaten und Kanäle im externen Speicher bleiben erhalten.",
        ));
        ui.small(tr!(
            lang,
            "Для возврата стока выберите заводской RT-880_Upgrade_V1_16_250915.bin. CPS сверит его SHA256. В публичном комплекте этого файла нет.",
            "To restore stock firmware, select your RT-880_Upgrade_V1_16_250915.bin. CPS checks its SHA256. The public bundle omits this file.",
            "Wählen Sie zur Wiederherstellung der Werksfirmware Ihre RT-880_Upgrade_V1_16_250915.bin. CPS prüft ihre SHA256-Prüfsumme. Das öffentliche Paket enthält diese Datei nicht.",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_without_file_or_port() -> FirmwarePane {
        FirmwarePane {
            image: None,
            voice: Default::default(),
            selected_path: None,
            port: String::new(),
            ports: Vec::new(),
            logs_parent: PathBuf::from("unused-offline-test-directory"),
            session_dir: None,
            receiver: None,
            progress: None,
            armed: false,
            preview: false,
            language: Language::default(),
            message: LocalizedText::default(),
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
            .as_str(Language::Russian)
            .contains("COM отключён"));
        assert!(!pane.busy());
        pane.preview = false;
        assert!(pane
            .start(&ctx, true)
            .unwrap_err()
            .as_str(Language::Russian)
            .contains("Другая операция"));
        assert!(!pane.busy());
        pane.armed = false;
        assert!(pane
            .start(&ctx, false)
            .unwrap_err()
            .as_str(Language::Russian)
            .contains("готовность загрузчика"));
        assert!(!pane.busy());
    }

    #[test]
    fn full_block_progress_does_not_claim_success_without_terminal_result() {
        let mut pane = pane_without_file_or_port();
        let (tx, rx) = mpsc::channel();
        pane.receiver = Some(rx);
        tx.send(Event::Progress(Progress {
            phase: msg!(
                "Блоки подтверждены",
                "Blocks acknowledged",
                "Blöcke bestätigt"
            ),
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
        assert!(pane
            .message
            .as_str(Language::Russian)
            .contains("не подтверждено"));
    }

    #[test]
    fn terminal_failure_disarms_retry_and_keeps_destructive_progress() {
        let mut pane = pane_without_file_or_port();
        pane.armed = true;
        let (tx, rx) = mpsc::channel();
        pane.receiver = Some(rx);
        tx.send(Event::Progress(Progress {
            phase: msg!("Стирание", "Erasing", "Löschen"),
            completed_blocks: 0,
            total_blocks: 246,
            destructive_started: true,
        }))
        .unwrap();
        tx.send(Event::Finished(Err(msg!(
            "Таймаут ACK",
            "ACK timeout",
            "Zeitüberschreitung bei ACK",
        ))))
        .unwrap();
        pane.poll(&egui::Context::default());
        assert!(!pane.busy());
        assert!(!pane.armed);
        assert!(!pane.finished);
        assert!(pane.error);
        for (language, progress, error) in [
            (
                Language::Russian,
                "Стирание",
                "Запись не завершена: Таймаут ACK",
            ),
            (
                Language::English,
                "Erasing",
                "Writing did not finish: ACK timeout",
            ),
            (
                Language::German,
                "Löschen",
                "Schreibvorgang nicht abgeschlossen: Zeitüberschreitung bei ACK",
            ),
        ] {
            pane.set_language(language);
            assert!(pane.progress.as_ref().unwrap().destructive_started);
            assert!(pane.log[0].as_str(pane.language).starts_with(progress));
            assert_eq!(pane.message.as_str(pane.language), error);
        }
    }
}
