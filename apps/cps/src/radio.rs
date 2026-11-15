//! Read-only radio acquisition, durable local snapshots and explicit CPS merge.
use crate::i18n::{Language, LocalizedText};
use eframe::egui;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uv98_codeplug::Section;
use uv98_cps_core::Document;
use uv98_radio_read::{
    CancelToken, PassCapture, Phase, Progress, ReadError, ReadPlan, ReadSnapshot, Region,
};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    write_new(
        path,
        &serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
}
fn region_name(region: Region) -> &'static str {
    match region {
        Region::Calibration => "calibration",
        Region::Cfg => "cfg",
        Region::Channels => "channels",
        Region::Fm => "fm",
        Region::Zones => "zones",
        Region::Storage => "storage-address-space",
    }
}
fn region_label(region: Region, lang: Language) -> &'static str {
    match region {
        Region::Calibration => tr!(lang, "Калибровки", "Calibration", "Kalibrierung"),
        Region::Cfg => tr!(lang, "Настройки", "Settings", "Einstellungen"),
        Region::Channels => tr!(lang, "Каналы", "Channels", "Kanäle"),
        Region::Fm => "FM",
        Region::Zones => tr!(lang, "Зоны", "Zones", "Zonen"),
        Region::Storage => tr!(
            lang,
            "Память настроек",
            "Settings storage",
            "Einstellungsspeicher"
        ),
    }
}

fn phase_label(phase: Phase, lang: Language) -> &'static str {
    match phase {
        Phase::Opening => tr!(lang, "Открытие порта", "Opening port", "Port wird geöffnet"),
        Phase::Connecting => tr!(lang, "Подключение", "Connecting", "Verbindungsaufbau"),
        Phase::Reading => tr!(lang, "Чтение", "Reading", "Lesen"),
        Phase::Closing => tr!(
            lang,
            "Закрытие порта",
            "Closing port",
            "Port wird geschlossen"
        ),
        Phase::Complete => tr!(
            lang,
            "Чтение завершено",
            "Reading complete",
            "Lesen abgeschlossen"
        ),
    }
}

fn read_operation(operation: &'static str, lang: Language) -> &'static str {
    match operation {
        "open port" => tr!(lang, "Открытие порта", "Opening port", "Port öffnen"),
        "serial send" => tr!(lang, "Отправка через COM", "Serial send", "Senden über COM"),
        "serial receive" => tr!(
            lang,
            "Приём через COM",
            "Serial receive",
            "Empfang über COM"
        ),
        "serial close" => tr!(
            lang,
            "Закрытие COM",
            "Closing COM port",
            "COM-Port schließen"
        ),
        "response boundary" => tr!(
            lang,
            "Проверка конца ответа",
            "Response boundary check",
            "Antwortende prüfen"
        ),
        "connection" => tr!(lang, "Подключение", "Connection", "Verbindung"),
        "frame" => tr!(lang, "Чтение кадра", "Frame read", "Frame lesen"),
        operation => operation,
    }
}

// radio-read flattens io::Error into text. Translate its fixed diagnostics and
// keep OS messages as supplied by the system.
fn read_io_detail(message: &str, lang: Language) -> &str {
    match message {
        "Expected COM1..COM65535" => tr!(
            lang,
            "Укажите COM1..COM65535",
            "Enter COM1..COM65535",
            "Geben Sie COM1..COM65535 ein",
        ),
        "Serial handle is closed" => tr!(
            lang,
            "COM-порт закрыт",
            "The COM port is closed",
            "Der COM-Port ist geschlossen",
        ),
        "Read buffer too large" => tr!(
            lang,
            "Слишком большой буфер чтения",
            "Read buffer is too large",
            "Lesepuffer ist zu groß",
        ),
        "Packet outside read-only allowlist" => tr!(
            lang,
            "Пакет не входит в список разрешённых команд чтения",
            "Packet is outside the read-only allowlist",
            "Paket ist für den Nur-Lese-Zugriff nicht zugelassen",
        ),
        "Windows serial transport is unavailable on this platform" => tr!(
            lang,
            "Чтение через COM поддерживается только в Windows",
            "Reading via COM is available only on Windows",
            "Lesen über COM ist nur unter Windows verfügbar",
        ),
        message => message,
    }
}

pub(crate) fn localize_read_error(error: &ReadError) -> LocalizedText {
    match error {
        ReadError::Cancelled => msg!(
            "Чтение остановлено",
            "Reading cancelled",
            "Lesen abgebrochen"
        ),
        ReadError::Io { operation, message } => LocalizedText::from_fn(|lang| {
            format!(
                "{}: {}",
                read_operation(operation, lang),
                read_io_detail(message, lang)
            )
        }),
        ReadError::Timeout {
            operation,
            received,
            expected,
        } => LocalizedText::from_fn(|lang| {
            let operation = read_operation(operation, lang);
            trf!(
                lang,
                "{operation}: таймаут, {received}/{expected} байт",
                "{operation}: timeout, {received}/{expected} bytes",
                "{operation}: Zeitüberschreitung, {received}/{expected} Byte",
            )
        }),
        ReadError::UnexpectedAck(byte) => msgf!(
            "Неожиданный ответ при подключении: {byte:02X}",
            "Unexpected connection response: {byte:02X}",
            "Unerwartete Antwort beim Verbindungsaufbau: {byte:02X}",
        ),
        ReadError::InvalidBlock(block) => msgf!(
            "Блок {block} вне разрешённых областей чтения",
            "Block {block} is outside the allowed read regions",
            "Block {block} liegt außerhalb der zulässigen Lesebereiche",
        ),
        ReadError::InvalidPortName => msg!(
            "Укажите имя порта COM1..COM65535",
            "Enter a port name from COM1 to COM65535",
            "Geben Sie einen Portnamen von COM1 bis COM65535 ein",
        ),
        ReadError::FrameLength { block, actual } => {
            let expected = uv98_radio_read::FRAME_SIZE;
            msgf!(
                "Блок {block}: ожидалось {expected} байт, получено {actual}",
                "Block {block}: expected {expected} bytes, got {actual}",
                "Block {block}: {expected} Byte erwartet, {actual} erhalten",
            )
        }
        ReadError::FrameHeader { block, actual } => msgf!(
            "Блок {block}: неожиданный заголовок или адрес {actual:02X?}",
            "Block {block}: unexpected header or address {actual:02X?}",
            "Block {block}: unerwarteter Header oder unerwartete Adresse {actual:02X?}",
        ),
        ReadError::Checksum {
            block,
            expected,
            actual,
        } => msgf!(
            "Блок {block}: контрольная сумма {actual:02X}, ожидалась {expected:02X}",
            "Block {block}: checksum {actual:02X}, expected {expected:02X}",
            "Block {block}: Prüfsumme {actual:02X}, erwartet wurde {expected:02X}",
        ),
        ReadError::UnexpectedTrailingByte { block, byte } => msgf!(
            "Блок {block}: лишний байт ответа {byte:02X}",
            "Block {block}: extra response byte {byte:02X}",
            "Block {block}: zusätzliches Antwortbyte {byte:02X}",
        ),
        ReadError::ShortSend { expected, actual } => msgf!(
            "Неполная отправка через COM: {actual}/{expected} байт",
            "Short serial send: {actual}/{expected} bytes",
            "Unvollständiger Versand über COM: {actual}/{expected} Byte",
        ),
        ReadError::TransportOverflow => msg!(
            "Порт сообщил о большем числе байт, чем запрошено",
            "The port reported more bytes than requested",
            "Der Port meldete mehr Byte als angefordert",
        ),
        ReadError::PacketNotAllowed => msg!(
            "Пакет не входит в список разрешённых команд чтения",
            "Packet is outside the read-only allowlist",
            "Paket ist für den Nur-Lese-Zugriff nicht zugelassen",
        ),
        ReadError::PassMismatch { block, byte_offset } => msgf!(
            "Чтения различаются: блок {block}, байт данных {byte_offset}",
            "Reads differ at block {block}, payload byte {byte_offset}",
            "Lesedurchläufe unterscheiden sich in Block {block}, Nutzdatenbyte {byte_offset}",
        ),
    }
}

pub(crate) fn localize_port_list_error(error: &std::io::Error) -> LocalizedText {
    if error.kind() == std::io::ErrorKind::Unsupported {
        return msg!(
            "Список COM-портов доступен только в Windows",
            "COM port listing is available only on Windows",
            "COM-Ports können nur unter Windows aufgelistet werden",
        );
    }
    msgf!(
        "Не удалось получить список COM-портов: {error}",
        "Could not list COM ports: {error}",
        "COM-Ports konnten nicht aufgelistet werden: {error}",
    )
}

fn save_passes(directory: &Path, passes: &[PassCapture]) -> Result<Vec<Value>, String> {
    let mut reports = Vec::new();
    for pass in passes {
        let prefix = format!("pass-{:02}", pass.pass);
        let tx: Vec<u8> = pass.tx_packets.iter().flatten().copied().collect();
        let frames: Vec<u8> = pass
            .frames
            .iter()
            .flat_map(|frame| frame.bytes.iter().copied())
            .collect();
        write_new(&directory.join(format!("{prefix}.tx.bin")), &tx)?;
        write_new(&directory.join(format!("{prefix}.rx.bin")), &pass.rx)?;
        write_new(&directory.join(format!("{prefix}.frames.bin")), &frames)?;
        let blocks: Vec<Value> = pass
            .frames
            .iter()
            .map(|frame| {
                json!({
                    "block":frame.block,"frame_bytes":frame.bytes.len(),"validated":frame.validated,
                    "sha256":sha256(&frame.bytes)
                })
            })
            .collect();
        reports.push(json!({"pass":pass.pass,"complete":pass.complete,"end_sent":pass.end_sent,
            "cleanup_error":pass.cleanup_error,"tx_sha256":sha256(&tx),"rx_sha256":sha256(&pass.rx),
            "tx_packets":pass.tx_packets.iter().map(|p| p.iter().map(|b| format!("{b:02X}")).collect::<String>()).collect::<Vec<_>>(),
            "frames_sha256":sha256(&frames),"frames":blocks}));
    }
    Ok(reports)
}
fn save_success(
    directory: &Path,
    port: &str,
    snapshot: &ReadSnapshot,
) -> Result<(), LocalizedText> {
    let passes = save_passes(directory, snapshot.passes())?;
    let mut sections = Vec::new();
    for section in snapshot.sections() {
        let filename = format!("{}.bin", region_name(section.region));
        write_new(&directory.join(&filename), &section.bytes)?;
        sections.push(json!({"file":filename,"first_block":section.first_block,
            "bytes":section.bytes.len(),"sha256":sha256(&section.bytes)}));
    }
    if snapshot.plan() == ReadPlan::FullStorage {
        let bytes = snapshot.section(Region::Calibration).ok_or_else(|| {
            msg!(
                "Нет калибровок в снимке",
                "Snapshot is missing calibration data",
                "Kalibrierdaten fehlen im Abbild"
            )
        })?;
        write_new(&directory.join("calibration.bin"), bytes)?;
        sections.push(json!({"file":"calibration.bin","first_block":0,
            "bytes":bytes.len(),"sha256":sha256(bytes)}));
    }
    write_json(
        &directory.join("manifest.json"),
        &json!({
            "schema":1,"status":"verified_two_reads","app_version":env!("CARGO_PKG_VERSION"),"port":port,
            "finished_unix_ms":now_millis(),"plan":snapshot.plan().label(),
            "two_reads_byte_identical":true,"sections":sections,"passes":passes,
            "time_section":"not_read_by_stock_cps; retained from a separately selected base file only",
            "mcu_flash_read":false,"bootloader_read":false,"full_radio_backup":false,
            "write_or_erase_commands_sent":false,
            "scope":"CONNECT, bounded stock READ and END; END behavior relies on stock session state"
        }),
    )?;
    Ok(())
}
fn save_failure(
    directory: &Path,
    port: &str,
    failure: &uv98_radio_read::ReadFailure,
) -> Result<(), String> {
    let passes = save_passes(directory, &failure.passes)?;
    write_json(
        &directory.join("manifest.json"),
        &json!({"schema":1,"status":"failed",
        "port":port,"error":failure.error.to_string(),"passes":passes,
        "two_reads_byte_identical":false,"write_or_erase_commands_sent":false,
        "finished_unix_ms":now_millis()}),
    )
}

enum Event {
    Progress(Progress),
    Finished(Result<ReadSnapshot, LocalizedText>),
}
struct Job {
    rx: Receiver<Event>,
    cancel: CancelToken,
}

pub struct RadioPane {
    port: String,
    ports: Vec<String>,
    directory: Option<PathBuf>,
    bundle: Option<PathBuf>,
    job: Option<Job>,
    progress: Option<Progress>,
    snapshot: Option<ReadSnapshot>,
    language: Language,
    message: LocalizedText,
    error: bool,
}
impl Default for RadioPane {
    fn default() -> Self {
        Self {
            port: "COM20".into(),
            ports: Vec::new(),
            directory: None,
            bundle: None,
            job: None,
            progress: None,
            snapshot: None,
            language: Language::default(),
            message: LocalizedText::default(),
            error: false,
        }
    }
}
impl RadioPane {
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    pub fn calibration_snapshot(&self) -> Option<(Vec<u8>, LocalizedText)> {
        if self.busy() {
            return None;
        }
        let bytes = self.snapshot.as_ref()?.section(Region::Calibration)?;
        let bundle = self.bundle.as_ref()?;
        Some((
            bytes.to_vec(),
            msgf!(
                "Два совпавших чтения: {}",
                "Two matching reads: {}",
                "Zwei übereinstimmende Lesedurchläufe: {}",
                bundle.display()
            ),
        ))
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn cancel(&self) {
        if let Some(job) = &self.job {
            job.cancel.cancel();
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else {
            return;
        };
        let mut finished = None;
        loop {
            match job.rx.try_recv() {
                Ok(Event::Progress(progress)) => self.progress = Some(progress),
                Ok(Event::Finished(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err(msg!(
                        "Поток чтения остановлен. Снимок не подтверждён.",
                        "The read worker stopped. The snapshot is unconfirmed.",
                        "Der Leseprozess wurde beendet. Das Abbild ist nicht bestätigt."
                    )));
                    break;
                }
            }
        }
        if let Some(result) = finished {
            self.job = None;
            match result {
                Ok(snapshot) => {
                    self.snapshot = Some(snapshot);
                    self.error = false;
                    self.message =
                        msg!(
                            "Два чтения совпали. Снимок и журнал сохранены. Порт закрыт.",
                            "Both reads matched. The snapshot and log were saved. The port is closed.",
                            "Beide Lesedurchläufe stimmen überein. Abbild und Protokoll wurden gespeichert. Der Port ist geschlossen.",
                        );
                }
                Err(error) => {
                    self.snapshot = None;
                    self.error = true;
                    self.message = error;
                }
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn start(&mut self, ctx: &egui::Context, plan: ReadPlan) -> Result<(), LocalizedText> {
        if self.busy() {
            return Err(msg!(
                "Чтение уже выполняется",
                "Reading is already in progress",
                "Ein Lesevorgang läuft bereits"
            ));
        }
        let parent = self.directory.as_ref().ok_or_else(|| {
            msg!(
                "Выберите папку для снимков",
                "Select a snapshot folder",
                "Wählen Sie einen Ordner für Abbilder"
            )
        })?;
        let directory = parent.join(format!("UV98-read-{}-{}", now_millis(), std::process::id()));
        fs::create_dir(&directory).map_err(|e| {
            msgf!(
                "Не удалось создать папку снимка: {e}",
                "Could not create the snapshot folder: {e}",
                "Ordner für das Abbild konnte nicht erstellt werden: {e}"
            )
        })?;
        write_json(
            &directory.join("started.json"),
            &json!({"schema":1,"status":"incomplete",
            "port":self.port,"started_unix_ms":now_millis(),"app_version":env!("CARGO_PKG_VERSION"),
            "plan":plan.label(),"full_radio_backup":false}),
        )?;
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::default();
        let worker_cancel = cancel.clone();
        let port = self.port.clone();
        let output = directory.clone();
        let repaint = ctx.clone();
        thread::Builder::new()
            .name("uv98-read-only".into())
            .spawn(move || {
                let result =
                    uv98_radio_read::read_windows(&port, plan, &worker_cancel, |progress| {
                        let _ = tx.send(Event::Progress(progress));
                        repaint.request_repaint();
                    });
                let finished = match result {
                    Ok(snapshot) => save_success(&output, &port, &snapshot)
                        .map(|()| snapshot)
                        .map_err(|e| {
                            LocalizedText::from_fn(|lang| {
                                trf!(
                                    lang,
                                    "Чтение прошло, но сохранение снимка не завершено: {detail}",
                                    "Reading finished, but the snapshot could not be saved: {detail}",
                                    "Lesen abgeschlossen, aber das Abbild konnte nicht gespeichert werden: {detail}",
                                    detail = e.as_str(lang),
                                )
                            })
                        }),
                    Err(failure) => {
                        let detail = localize_read_error(&failure.error);
                        let log = save_failure(&output, &port, &failure);
                        Err(LocalizedText::from_fn(|lang| {
                            let detail = detail.as_str(lang);
                            match &log {
                                Ok(()) => trf!(
                                    lang,
                                    "Чтение не завершено: {detail}. Журнал сохранён.",
                                    "Reading did not finish: {detail}. The log was saved.",
                                    "Lesen nicht abgeschlossen: {detail}. Das Protokoll wurde gespeichert.",
                                ),
                                Err(error) => trf!(
                                    lang,
                                    "Чтение не завершено: {detail}. Ошибка журнала: {error}",
                                    "Reading did not finish: {detail}. Log error: {error}",
                                    "Lesen nicht abgeschlossen: {detail}. Protokollfehler: {error}",
                                ),
                            }
                        }))
                    }
                };
                let _ = tx.send(Event::Finished(finished));
                repaint.request_repaint();
            })
            .map_err(|e| msgf!(
                "Не удалось запустить чтение: {e}",
                "Could not start reading: {e}",
                "Lesen konnte nicht gestartet werden: {e}",
            ))?;
        self.job = Some(Job { rx, cancel });
        self.bundle = Some(directory);
        self.snapshot = None;
        self.progress = None;
        self.message = msg!(
            "Подключение к выбранному COM-порту…",
            "Connecting to the selected COM port…",
            "Verbindung zum ausgewählten COM-Port wird hergestellt…"
        );
        self.error = false;
        Ok(())
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, can_apply: bool) -> Option<ReadSnapshot> {
        let lang = self.language;
        let mut apply = None;
        ui.heading(tr!(
            lang,
            "Резервные копии UV98",
            "UV98 backups",
            "UV98-Sicherungen"
        ));
        ui.label(tr!(
            lang,
            "Для чтения подключите рацию со стоком в обычном режиме и закройте другие CPS.",
            "To read, connect the radio running stock firmware in normal mode and close other CPS programs.",
            "Schließen Sie zum Lesen das Funkgerät mit Werksfirmware im Normalbetrieb an und schließen Sie andere CPS-Programme.",
        ));
        ui.label(tr!(
            lang,
            "Выберите COM и папку. Каждый снимок читается дважды и сверяется побайтово.",
            "Select a COM port and folder. CPS reads each snapshot twice and compares every byte.",
            "Wählen Sie COM-Port und Ordner. CPS liest jedes Abbild zweimal und vergleicht die Daten Byte für Byte.",
        ));
        ui.add_space(12.0);
        ui.add_enabled_ui(!self.busy(), |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!(lang, "COM-порт", "COM port", "COM-Port"));
                ui.add(egui::TextEdit::singleline(&mut self.port).desired_width(100.0));
                egui::ComboBox::from_id_salt("radio_ports")
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
                if ui.button(tr!(lang, "Обновить список", "Refresh list", "Liste aktualisieren")).clicked() {
                    match uv98_radio_read::list_ports() {
                        Ok(ports) => {
                            self.ports = ports;
                            self.message = msgf!(
                                "Найдено портов: {}. Порты не открывались.",
                                "Found {} ports. No ports were opened.",
                                "Gefundene Ports: {}. Es wurden keine Ports geöffnet.",
                                self.ports.len(),
                            );
                            self.error = false;
                        }
                        Err(e) => {
                            self.message = localize_port_list_error(&e);
                            self.error = true;
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(tr!(
                    lang,
                    "Папка для снимков…",
                    "Snapshot folder…",
                    "Ordner für Abbilder…",
                )).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(tr!(
                            lang,
                            "Папка для новых снимков UV98",
                            "Folder for new UV98 snapshots",
                            "Ordner für neue UV98-Abbilder",
                        ))
                        .pick_folder()
                    {
                        self.directory = Some(path);
                    }
                }
                ui.label(
                    self.directory
                        .as_ref()
                        .map_or(tr!(lang, "Не выбрана", "None selected", "Nicht ausgewählt").into(), |p| p.display().to_string()),
                );
            });
            ui.add_space(8.0);
            for (label, plan) in [
                (tr!(
                    lang,
                    "Считать калибровки (4 КиБ)",
                    "Read calibration (4 KiB)",
                    "Kalibrierung lesen (4 KiB)",
                ), ReadPlan::Calibration),
                (tr!(
                    lang,
                    "Считать память настроек (4 МиБ)",
                    "Read settings storage (4 MiB)",
                    "Einstellungsspeicher lesen (4 MiB)",
                ), ReadPlan::FullStorage),
                (tr!(
                    lang,
                    "Считать каналы, настройки и калибровки",
                    "Read channels, settings, and calibration",
                    "Kanäle, Einstellungen und Kalibrierung lesen",
                ), ReadPlan::CpsAndCalibration),
            ] {
                if ui.button(label).clicked() {
                    if self.directory.is_none() {
                        self.directory = rfd::FileDialog::new()
                            .set_title(tr!(
                                lang,
                                "Куда сохранить резервную копию UV98",
                                "Save UV98 backup to folder",
                                "Ordner für die UV98-Sicherung auswählen",
                            ))
                            .pick_folder();
                    }
                    if self.directory.is_some() {
                        if let Err(error) = self.start(ui.ctx(), plan) {
                            self.message = error;
                            self.error = true;
                        }
                    }
                }
            }
            ui.separator();
            ui.label(tr!(
                lang,
                "Прошивка MCU и загрузчик находятся вне памяти настроек. Известный протокол штатной CPS не содержит команды их чтения. Заводской BIN — отдельный файл восстановления, а не дамп вашей рации.",
                "The MCU firmware and bootloader reside outside settings storage. The known stock CPS protocol has no command to read them. The stock BIN is a separate recovery file and contains no dump of your radio.",
                "MCU-Firmware und Bootloader liegen außerhalb des Einstellungsspeichers. Das bekannte Protokoll der Werks-CPS bietet keinen Befehl zum Auslesen. Die Werks-BIN ist eine separate Wiederherstellungsdatei und enthält keinen Auszug Ihres Funkgeräts.",
            ));
            ui.label(tr!(
                lang,
                "Публичный комплект не содержит заводского BIN. Поместите свой RT-880_Upgrade_V1_16_250915.bin в recovery рядом с CPS; при сохранении проверяется SHA256.",
                "The public bundle omits the stock BIN. Place your RT-880_Upgrade_V1_16_250915.bin in the recovery folder beside CPS. CPS verifies the SHA256 when saving.",
                "Das öffentliche Paket enthält keine Werks-BIN. Legen Sie Ihre RT-880_Upgrade_V1_16_250915.bin in den Ordner recovery neben CPS. CPS prüft die SHA256-Prüfsumme beim Speichern.",
            ));
            if ui.button(tr!(
                lang,
                "Сохранить заводской BIN 1.16…",
                "Save stock BIN 1.16…",
                "Werks-BIN 1.16 speichern…",
            )).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(tr!(
                        lang,
                        "Сохранение заводского образа",
                        "Save stock image",
                        "Werks-Image speichern",
                    ))
                    .add_filter(tr!(lang, "Прошивка UV98", "UV98 firmware", "UV98-Firmware"), &["bin"])
                    .set_file_name("RT-880_Upgrade_V1_16_250915.bin")
                    .save_file()
                {
                    match crate::firmware::export_stock(&path) {
                        Ok(()) => {
                            self.message = msgf!(
                                "Заводской образ сохранён в {}",
                                "Stock image saved to {}",
                                "Werks-Image gespeichert in {}",
                                path.display(),
                            );
                            self.error = false;
                        }
                        Err(error) => {
                            self.message = error;
                            self.error = true;
                        }
                    }
                }
            }
        });
        if self.busy() {
            if let Some(progress) = &self.progress {
                let value = if progress.total_blocks > 0 {
                    progress.completed_blocks as f32 / progress.total_blocks as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(value).show_percentage().text(trf!(
                    lang,
                    "{} · проход {}/2 · {} / {} блоков",
                    "{} · pass {}/2 · {} / {} blocks",
                    "{} · Durchlauf {}/2 · {} / {} Blöcke",
                    phase_label(progress.phase, lang),
                    progress.pass,
                    progress.completed_blocks,
                    progress.total_blocks,
                )));
            }
            if ui
                .button(tr!(
                    lang,
                    "Остановить чтение",
                    "Cancel reading",
                    "Lesen abbrechen"
                ))
                .clicked()
            {
                self.cancel();
            }
        }
        ui.add_space(12.0);
        if !self.message.is_empty() {
            ui.colored_label(
                if self.error {
                    egui::Color32::LIGHT_RED
                } else {
                    egui::Color32::LIGHT_GREEN
                },
                self.message.as_str(lang),
            );
        }
        if let Some(path) = &self.bundle {
            ui.label(trf!(
                lang,
                "Снимок: {}",
                "Snapshot: {}",
                "Abbild: {}",
                path.display()
            ));
        }
        if let Some(snapshot) = &self.snapshot {
            ui.separator();
            for section in snapshot.sections() {
                ui.label(trf!(
                    lang,
                    "{}: {} байт · SHA256 {}",
                    "{}: {} bytes · SHA256 {}",
                    "{}: {} Byte · SHA256 {}",
                    region_label(section.region, lang),
                    section.bytes.len(),
                    sha256(&section.bytes)
                ));
            }
            ui.add_space(12.0);
            ui.label(tr!(
                lang,
                "Для редактирования откройте исходный .880dmf/.98pdmf и примените считанные секции.",
                "To edit, open the source .880dmf/.98pdmf and apply the sections you read.",
                "Öffnen Sie zum Bearbeiten die Quell-Datei .880dmf/.98pdmf und übernehmen Sie die gelesenen Abschnitte.",
            ));
            ui.label(tr!(
                lang,
                "Секция Time не читается штатным CPS: её байты останутся из открытого файла. Калибровки хранятся отдельно.",
                "Stock CPS cannot read the Time section. Its bytes come from the open file. Calibration data is stored separately.",
                "Die Werks-CPS kann den Abschnitt Time nicht lesen. Seine Bytes stammen aus der geöffneten Datei. Kalibrierdaten werden separat gespeichert.",
            ));
            if ui
                .add_enabled(
                    can_apply && snapshot.plan() != ReadPlan::Calibration,
                    egui::Button::new(tr!(
                        lang,
                        "Применить считанное к открытому файлу",
                        "Apply read data to open file",
                        "Gelesene Daten in geöffnete Datei übernehmen"
                    )),
                )
                .clicked()
            {
                apply = Some(snapshot.clone());
            }
            if !can_apply {
                ui.small(tr!(
                    lang,
                    "Нужен открытый файл без неприменённых или изменённых данных. Сохраните правки и откройте выбранную копию.",
                    "Open a file with no pending or unsaved changes. Save your edits and reopen the selected copy.",
                    "Öffnen Sie eine Datei ohne ausstehende oder ungespeicherte Änderungen. Speichern Sie Ihre Änderungen und öffnen Sie die ausgewählte Kopie erneut.",
                ));
            }
        }
        ui.add_space(18.0);
        apply
    }
}
impl Drop for RadioPane {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn apply_to_document(
    snapshot: &ReadSnapshot,
    document: &mut Document,
) -> Result<usize, LocalizedText> {
    let mapped = [
        (Region::Cfg, Section::Cfg),
        (Region::Channels, Section::Channels),
        (Region::Fm, Section::Fm),
        (Region::Zones, Section::Zones),
    ];
    let mut sections = Vec::new();
    for (region, section) in mapped {
        let bytes = snapshot.section(region).ok_or_else(|| {
            LocalizedText::from_fn(|lang| {
                trf!(
                    lang,
                    "Нет секции {}",
                    "Missing section {}",
                    "Abschnitt {} fehlt",
                    region_label(region, lang),
                )
            })
        })?;
        sections.push((section, bytes));
    }
    merge_sections(document, &sections)
}
fn merge_sections(
    document: &mut Document,
    sections: &[(Section, &[u8])],
) -> Result<usize, LocalizedText> {
    document
        .apply(|candidate| {
            for (section, bytes) in sections {
                if *section == Section::Time || bytes.len() != section.byte_len() {
                    return Err(msg!(
                        "Неверная секция снимка",
                        "Invalid snapshot section",
                        "Ungültiger Abschnitt im Abbild",
                    ));
                }
                let start = section.offset();
                candidate[start..start + bytes.len()].copy_from_slice(bytes);
            }
            Ok(())
        })
        .map(|edit| edit.changed_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radio_merge_preserves_time_and_is_undoable() {
        let bytes: Vec<u8> = (0..uv98_codeplug::FILE_LEN)
            .map(|i| (i % 251) as u8)
            .collect();
        let mut doc = Document::from_bytes(&bytes).unwrap();
        let cfg = vec![0x92; Section::Cfg.byte_len()];
        let channels = vec![0x37; Section::Channels.byte_len()];
        merge_sections(
            &mut doc,
            &[(Section::Cfg, &cfg), (Section::Channels, &channels)],
        )
        .unwrap();
        assert_eq!(doc.view().cfg().as_slice(), cfg);
        assert_eq!(doc.view().channels().as_slice(), channels);
        for section in [Section::Time, Section::Fm, Section::Zones] {
            assert_eq!(
                doc.view().section(section),
                &bytes[section.offset()..section.offset() + section.byte_len()]
            );
        }
        assert!(doc.undo());
        assert_eq!(doc.bytes(), bytes);
    }
    #[test]
    fn partial_or_time_merge_is_atomic_failure() {
        let bytes = vec![0xA5; uv98_codeplug::FILE_LEN];
        let mut doc = Document::from_bytes(&bytes).unwrap();
        let cfg = vec![0; Section::Cfg.byte_len()];
        assert!(
            merge_sections(&mut doc, &[(Section::Cfg, &cfg), (Section::Channels, &[0])]).is_err()
        );
        assert_eq!(doc.bytes(), bytes);
        assert!(merge_sections(
            &mut doc,
            &[(Section::Time, &vec![0; Section::Time.byte_len()])]
        )
        .is_err());
        assert_eq!(doc.bytes(), bytes);
    }
}
