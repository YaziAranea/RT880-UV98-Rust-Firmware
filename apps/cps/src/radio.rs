//! Read-only radio acquisition, durable local snapshots and explicit CPS merge.
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
use uv98_radio_read::{CancelToken, PassCapture, Progress, ReadPlan, ReadSnapshot, Region};

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
fn save_success(directory: &Path, port: &str, snapshot: &ReadSnapshot) -> Result<(), String> {
    let passes = save_passes(directory, snapshot.passes())?;
    let mut sections = Vec::new();
    for section in snapshot.sections() {
        let filename = format!("{}.bin", region_name(section.region));
        write_new(&directory.join(&filename), &section.bytes)?;
        sections.push(json!({"file":filename,"first_block":section.first_block,
            "bytes":section.bytes.len(),"sha256":sha256(&section.bytes)}));
    }
    if snapshot.plan() == ReadPlan::FullStorage {
        let bytes = snapshot
            .section(Region::Calibration)
            .ok_or("Нет калибровок в снимке")?;
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
    )
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
    Finished(Result<ReadSnapshot, String>),
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
    message: String,
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
            message: String::new(),
            error: false,
        }
    }
}
impl RadioPane {
    pub fn calibration_snapshot(&self) -> Option<(Vec<u8>, String)> {
        if self.busy() {
            return None;
        }
        let bytes = self.snapshot.as_ref()?.section(Region::Calibration)?;
        Some((
            bytes.to_vec(),
            format!("Два совпавших чтения: {}", self.bundle.as_ref()?.display()),
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
                    finished = Some(Err("Поток чтения остановлен. Снимок не подтверждён.".into()));
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
                        "Два чтения совпали. Снимок и журнал сохранены. Порт закрыт.".into();
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
    fn start(&mut self, ctx: &egui::Context, plan: ReadPlan) -> Result<(), String> {
        if self.busy() {
            return Err("Чтение уже выполняется".into());
        }
        let parent = self
            .directory
            .as_ref()
            .ok_or("Выберите папку для снимков")?;
        let directory = parent.join(format!("UV98-read-{}-{}", now_millis(), std::process::id()));
        fs::create_dir(&directory).map_err(|e| format!("Не удалось создать папку снимка: {e}"))?;
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
                            format!("Чтение прошло, но сохранение снимка не завершено: {e}")
                        }),
                    Err(failure) => {
                        let message = failure.error.to_string();
                        match save_failure(&output, &port, &failure) {
                            Ok(()) => {
                                Err(format!("Чтение не завершено: {message}. Журнал сохранён."))
                            }
                            Err(error) => Err(format!(
                                "Чтение не завершено: {message}. Ошибка журнала: {error}"
                            )),
                        }
                    }
                };
                let _ = tx.send(Event::Finished(finished));
                repaint.request_repaint();
            })
            .map_err(|e| format!("Не удалось запустить чтение: {e}"))?;
        self.job = Some(Job { rx, cancel });
        self.bundle = Some(directory);
        self.snapshot = None;
        self.progress = None;
        self.message = "Подключение к выбранному COM-порту…".into();
        self.error = false;
        Ok(())
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, can_apply: bool) -> Option<ReadSnapshot> {
        let mut apply = None;
        ui.heading("Резервные копии UV98");
        ui.label("Для чтения подключите рацию со стоком в обычном режиме и закройте другие CPS.");
        ui.label("Выберите COM и папку. Каждый снимок читается дважды и сверяется побайтово.");
        ui.add_space(12.0);
        ui.add_enabled_ui(!self.busy(), |ui| {
            ui.horizontal(|ui| {
                ui.label("COM-порт");
                ui.add(egui::TextEdit::singleline(&mut self.port).desired_width(100.0));
                egui::ComboBox::from_id_salt("radio_ports")
                    .selected_text("Доступные порты")
                    .show_ui(ui, |ui| {
                        for port in &self.ports {
                            ui.selectable_value(&mut self.port, port.clone(), port);
                        }
                    });
                if ui.button("Обновить список").clicked() {
                    match uv98_radio_read::list_ports() {
                        Ok(ports) => {
                            self.ports = ports;
                            self.message = format!(
                                "Найдено портов: {}. Порты не открывались.",
                                self.ports.len()
                            );
                            self.error = false;
                        }
                        Err(e) => {
                            self.message = e.to_string();
                            self.error = true;
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Папка для снимков…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Папка для новых снимков UV98")
                        .pick_folder()
                    {
                        self.directory = Some(path);
                    }
                }
                ui.label(
                    self.directory
                        .as_ref()
                        .map_or("Не выбрана".into(), |p| p.display().to_string()),
                );
            });
            ui.add_space(8.0);
            for (label, plan) in [
                ("Считать калибровки (4 КиБ)", ReadPlan::Calibration),
                ("Считать память настроек (4 МиБ)", ReadPlan::FullStorage),
                ("Считать каналы, настройки и калибровки", ReadPlan::CpsAndCalibration),
            ] {
                if ui.button(label).clicked() {
                    if self.directory.is_none() {
                        self.directory = rfd::FileDialog::new()
                            .set_title("Куда сохранить резервную копию UV98")
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
            ui.label("Прошивка MCU и загрузчик находятся вне памяти настроек. Известный протокол штатной CPS не содержит команды их чтения. Сохраняемый ниже заводской BIN — файл восстановления из комплекта, а не дамп вашей рации.");
            ui.label("Для отката можно сохранить заводской образ 1.16 из комплекта.");
            if ui.button("Сохранить заводской BIN 1.16…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("RT-880_Upgrade_V1_16_250915.bin")
                    .save_file()
                {
                    match crate::firmware::export_stock(&path) {
                        Ok(()) => {
                            self.message = format!("Заводской образ сохранён в {}", path.display());
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
                ui.add(
                    egui::ProgressBar::new(value)
                        .show_percentage()
                        .text(format!(
                            "Проход {}/2 · {} / {} блоков",
                            progress.pass, progress.completed_blocks, progress.total_blocks
                        )),
                );
            }
            if ui.button("Остановить чтение").clicked() {
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
                &self.message,
            );
        }
        if let Some(path) = &self.bundle {
            ui.label(format!("Снимок: {}", path.display()));
        }
        if let Some(snapshot) = &self.snapshot {
            ui.separator();
            for section in snapshot.sections() {
                ui.label(format!(
                    "{}: {} байт · SHA256 {}",
                    region_name(section.region),
                    section.bytes.len(),
                    sha256(&section.bytes)
                ));
            }
            ui.add_space(12.0);
            ui.label("Для редактирования откройте исходный .880dmf/.98pdmf и примените считанные секции.");
            ui.label("Секция Time не читается штатным CPS: её байты останутся из открытого файла. Калибровки хранятся отдельно.");
            if ui
                .add_enabled(
                    can_apply && snapshot.plan() != ReadPlan::Calibration,
                    egui::Button::new("Применить считанное к открытому файлу"),
                )
                .clicked()
            {
                apply = Some(snapshot.clone());
            }
            if !can_apply {
                ui.small("Нужен открытый файл без неприменённых или изменённых данных. Сохраните правки и откройте выбранную копию.");
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
) -> Result<usize, String> {
    let mapped = [
        (Region::Cfg, Section::Cfg),
        (Region::Channels, Section::Channels),
        (Region::Fm, Section::Fm),
        (Region::Zones, Section::Zones),
    ];
    let mut sections = Vec::new();
    for (region, section) in mapped {
        let bytes = snapshot
            .section(region)
            .ok_or_else(|| format!("Нет секции {}", region_name(region)))?;
        sections.push((section, bytes));
    }
    merge_sections(document, &sections)
}
fn merge_sections(document: &mut Document, sections: &[(Section, &[u8])]) -> Result<usize, String> {
    document
        .apply(|candidate| {
            for (section, bytes) in sections {
                if *section == Section::Time || bytes.len() != section.byte_len() {
                    return Err("Неверная секция снимка".to_string());
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
