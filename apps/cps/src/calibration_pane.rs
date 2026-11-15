//! Human-readable local calibration workspace. All edits affect a separate copy.
use crate::calibration_report::{
    band_caption, edit_error, field_detail, field_label, general_label, group_label,
};
use crate::i18n::{Language, LocalizedText};
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::path::PathBuf;
use uv98_calibration::Table;
use uv98_calibration_edit::{descriptor, fields, Document, Field, Group};

const FINISH_FORM: LocalizedText = msg!(
    "Сначала примените значение или сбросьте форму",
    "Apply the value or reset the form first",
    "Wenden Sie zuerst den Wert an oder setzen Sie das Formular zurück",
);
const NOT_LOADED: LocalizedText = msg!(
    "Калибровки не открыты",
    "No calibration data is open",
    "Keine Kalibrierdaten geöffnet",
);

#[derive(Default)]
pub struct CalibrationPane {
    language: Language,
    document: Option<Box<Document>>,
    source: LocalizedText,
    source_sha: String,
    last_export_sha: String,
    band: Option<usize>,
    search: String,
    changed_only: bool,
    show_readonly: bool,
    selected: Option<Field>,
    draft: String,
    initial_draft: String,
    history: Vec<Box<Document>>,
    notice: LocalizedText,
    error: bool,
    confirm_reset: bool,
}
impl CalibrationPane {
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }
    pub fn pending_form(&self) -> bool {
        self.selected.is_some() && self.draft != self.initial_draft
    }
    pub fn unsaved(&self) -> bool {
        self.document.as_ref().is_some_and(|d| {
            d.is_dirty() && crate::radio::sha256(d.bytes()) != self.last_export_sha
        })
    }
    fn select_band(&mut self, band: Option<usize>) {
        if self.band == band {
            return;
        }
        if self.pending_form() {
            self.message(
                msg!(
                    "Примените значение или сбросьте форму перед сменой блока диапазона.",
                    "Apply the value or reset the form before changing the band block.",
                    "Wenden Sie den Wert an oder setzen Sie das Formular zurück, bevor Sie den Bandblock wechseln.",
                ),
                true,
            );
            return;
        }
        self.band = band;
        self.selected = None;
    }
    fn message(&mut self, message: LocalizedText, error: bool) {
        self.notice = message;
        self.error = error;
    }
    pub fn load_bytes(
        &mut self,
        bytes: &[u8],
        source: LocalizedText,
        source_sha: String,
    ) -> Result<(), LocalizedText> {
        if self.pending_form() || self.unsaved() {
            return Err(msg!(
                "Сначала сохраните пакет калибровок или верните исходные значения.",
                "Save the calibration bundle or restore the original values first.",
                "Speichern Sie zuerst das Kalibrierpaket oder stellen Sie die Originalwerte wieder her.",
            ));
        }
        let doc = Document::new(bytes).map_err(edit_error)?;
        self.last_export_sha = crate::radio::sha256(doc.bytes());
        self.document = Some(Box::new(doc));
        self.source = source;
        self.source_sha = source_sha;
        self.selected = None;
        self.history.clear();
        self.search.clear();
        self.band = None;
        self.message(
            msg!(
                "Калибровки открыты. Исходный блок сохранён в памяти отдельно от рабочей копии.",
                "Calibration data opened. The original block is kept in memory separately from the working copy.",
                "Kalibrierdaten geöffnet. Der Originalblock wird getrennt von der Arbeitskopie im Speicher gehalten.",
            ),
            false,
        );
        Ok(())
    }
    fn open(&mut self) {
        if self.pending_form() || self.unsaved() {
            self.message(
                msg!(
                    "Сохраните пакет правок или верните оригинал перед открытием другого блока.",
                    "Save the edit bundle or restore the original before opening another block.",
                    "Speichern Sie das Änderungspaket oder stellen Sie das Original wieder her, bevor Sie einen anderen Block öffnen.",
                ),
                true,
            );
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr!(
                self.language,
                "Калибровки 4096 байт или NOR-дамп 4 МиБ",
                "4096-byte calibration block or 4 MiB NOR dump",
                "4096-Byte-Kalibrierblock oder 4-MiB-NOR-Dump"
            ))
            .add_filter(
                tr!(
                    self.language,
                    "Сохранённые данные",
                    "Saved data",
                    "Gespeicherte Daten"
                ),
                &["bin"],
            )
            .pick_file()
        {
            let result = crate::calibration_io::load(&path).and_then(|loaded| {
                let name = crate::calibration_io::source_label(&path, loaded.extracted_from_nor);
                self.load_bytes(&loaded.bytes, name, loaded.source_sha256)
            });
            if let Err(error) = result {
                self.message(error, true);
            }
        }
    }
    fn remember(&mut self) {
        if let Some(doc) = &self.document {
            if self.history.len() == 32 {
                self.history.remove(0);
            }
            self.history.push(doc.clone());
        }
    }
    fn select(&mut self, field: Field) {
        if self.pending_form() {
            self.message(
                msg!(
                    "Примените значение или сбросьте форму перед выбором другого поля.",
                    "Apply the value or reset the form before selecting another field.",
                    "Wenden Sie den Wert an oder setzen Sie das Formular zurück, bevor Sie ein anderes Feld auswählen.",
                ),
                true,
            );
            return;
        }
        if let Some(doc) = &self.document {
            self.draft = doc.value(field).expect("enumerated field").to_string();
            self.initial_draft = self.draft.clone();
            self.selected = Some(field);
        }
    }
    fn apply(&mut self) {
        let Some(field) = self.selected else {
            return;
        };
        let value = match self.draft.trim().parse::<i32>() {
            Ok(v) => v,
            Err(_) => {
                self.message(
                    msg!(
                        "Введите целое число в указанном диапазоне.",
                        "Enter an integer within the displayed range.",
                        "Geben Sie eine ganze Zahl im angezeigten Bereich ein.",
                    ),
                    true,
                );
                return;
            }
        };
        let Some(doc) = &self.document else {
            return;
        };
        let mut candidate = doc.clone();
        match candidate.set(field, value) {
            Ok(_) => {
                self.remember();
                self.document = Some(candidate);
                self.initial_draft = value.to_string();
                self.draft = self.initial_draft.clone();
                self.message(
                    msg!(
                        "Изменена рабочая копия. Для сохранения нажмите «Сохранить пакет правок».",
                        "Working copy changed. Use “Save edit bundle” to export it.",
                        "Arbeitskopie geändert. Exportieren Sie sie mit „Änderungspaket speichern“.",
                    ),
                    false,
                );
            }
            Err(error) => self.message(edit_error(error), true),
        }
    }
    fn restore_field(&mut self) {
        let Some(field) = self.selected else {
            return;
        };
        self.remember();
        if let Some(doc) = &mut self.document {
            match doc.reset_field(field) {
                Ok(_) => {
                    self.draft = doc.value(field).unwrap().to_string();
                    self.initial_draft = self.draft.clone();
                    self.message(
                        msg!(
                            "Поле возвращено к исходному значению.",
                            "Original field value restored.",
                            "Ursprünglicher Feldwert wiederhergestellt.",
                        ),
                        false,
                    );
                }
                Err(error) => self.message(edit_error(error), true),
            }
        }
    }
    fn undo(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.document = Some(previous);
            self.selected = None;
            self.message(
                msg!(
                    "Последняя правка калибровок отменена.",
                    "Last calibration edit undone.",
                    "Letzte Kalibrieränderung rückgängig gemacht.",
                ),
                false,
            );
        }
    }
    fn save_original(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!(
                self.language,
                "Сохранить исходные 4096 байт новым файлом",
                "Save original 4096 bytes to a new file",
                "Ursprüngliche 4096 Bytes in einer neuen Datei speichern"
            ))
            .set_file_name("UV98-calibration-original.bin")
            .add_filter(
                tr!(self.language, "Калибровки", "Calibration", "Kalibrierung"),
                &["bin"],
            )
            .save_file()
        else {
            return;
        };
        match crate::calibration_io::save_original(&path, doc.original()) {
            Ok(()) => self.message(
                msgf!(
                    "Оригинал сохранён и проверен: {}",
                    "Original saved and verified: {}",
                    "Original gespeichert und überprüft: {}",
                    path.display()
                ),
                false,
            ),
            Err(e) => self.message(e, true),
        }
    }
    pub fn report(&self) -> Option<Value> {
        let doc = self.document.as_ref()?;
        let values: Vec<Value> = fields().map(|field| {
            let d=descriptor(field).expect("enumerated descriptor");
            json!({"field":format!("{field:?}"),"group":format!("{:?}",d.group),"label":field_label(d, Language::Russian),"detail":d.detail,
                "offset":d.offset,"width_bytes":d.width,"mask":d.mask,"original":doc.original_value(field).unwrap(),
                "working":doc.value(field).unwrap(),"editable":d.editable,"min_code":d.min,"max_code":d.max,
                "evidence":format!("{:?}",d.consumption)})
        }).collect();
        let changes: Vec<Value> = doc
            .original()
            .iter()
            .zip(doc.bytes())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(offset, (a, b))| json!({"offset":offset,"original":a,"working":b}))
            .collect();
        Some(
            json!({"schema":1,"source":self.source.as_str(Language::Russian),"source_sha256":self.source_sha,"original_sha256":crate::radio::sha256(doc.original()),
            "working_sha256":crate::radio::sha256(doc.bytes()),"values_are_codes":true,"rf_validated":false,"fields":values,"byte_changes":changes}),
        )
    }
    fn save_bundle(&mut self) {
        let Some(parent) = rfd::FileDialog::new()
            .set_title(tr!(
                self.language,
                "Папка для нового пакета калибровок",
                "Folder for new calibration bundle",
                "Ordner für neues Kalibrierpaket"
            ))
            .pick_folder()
        else {
            return;
        };
        if let Err(e) = self.export_to(&parent) {
            self.message(e, true);
        }
    }
    fn save_report(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!(
                self.language,
                "Сохранить читаемый отчёт новым файлом",
                "Save readable report to a new file",
                "Lesbaren Bericht in einer neuen Datei speichern"
            ))
            .set_file_name("UV98-calibration-report.txt")
            .add_filter(
                tr!(
                    self.language,
                    "Отчёт UTF-8",
                    "UTF-8 report",
                    "UTF-8-Bericht"
                ),
                &["txt"],
            )
            .save_file()
        else {
            return;
        };
        match self.export_report_to(&path) {
            Ok(()) => self.message(msgf!(
                "Отчёт сохранён и проверен: {}. Для бинарного резерва используйте пакет правок.",
                "Report saved and verified: {}. Use the edit bundle for a binary backup.",
                "Bericht gespeichert und überprüft: {}. Verwenden Sie das Änderungspaket zur Binärsicherung.",
                path.display(),
            ), false),
            Err(error) => self.message(error, true),
        }
    }
    pub fn export_report_to(&self, path: &std::path::Path) -> Result<(), LocalizedText> {
        if self.pending_form() {
            return Err(FINISH_FORM);
        }
        let doc = self.document.as_ref().ok_or(NOT_LOADED)?;
        crate::calibration_io::save_text_report(
            path,
            doc.original(),
            doc.bytes(),
            self.source.as_str(self.language),
            &self.source_sha,
            self.language,
        )
        // A text report does not mark edited binary bytes as safely backed up.
    }
    pub fn export_to(&mut self, parent: &std::path::Path) -> Result<PathBuf, LocalizedText> {
        if self.pending_form() {
            return Err(FINISH_FORM);
        }
        let report = self.report().ok_or(NOT_LOADED)?;
        let doc = self.document.as_ref().unwrap();
        let path = crate::calibration_io::export_bundle(
            parent,
            doc.original(),
            doc.bytes(),
            &report,
            Some(self.source.as_str(self.language)),
            self.language,
        )?;
        self.last_export_sha = crate::radio::sha256(doc.bytes());
        self.message(
            msgf!(
                "Пакет сохранён и проверен: {}",
                "Bundle saved and verified: {}",
                "Paket gespeichert und überprüft: {}",
                path.display()
            ),
            false,
        );
        Ok(path)
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, live: Option<(Vec<u8>, LocalizedText)>) {
        let language = self.language;
        ui.heading(tr!(
            language,
            "Калибровки станции",
            "Radio calibration",
            "Funkgeräte-Kalibrierung"
        ));
        ui.label(tr!(language,
            "Оригинал, рабочая копия и точная разница. Редактирование выполняется только в файлах.",
            "Compare the original block and working copy. This editor works on local files only.",
            "Vergleichen Sie Originalblock und Arbeitskopie. Der Editor bearbeitet nur lokale Dateien.",
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr!(language, "Открыть блок / NOR…", "Open block / NOR…", "Block / NOR öffnen…")).clicked() {
                self.open();
            }
            if ui
                .add_enabled(
                    live.is_some() && !self.unsaved() && !self.pending_form(),
                    egui::Button::new(tr!(language, "Из проверенного снимка рации", "From verified radio snapshot", "Aus verifiziertem Funkgeräteabbild")),
                )
                .clicked()
            {
                if let Some((bytes, source)) = live {
                    let sha = crate::radio::sha256(&bytes);
                    if let Err(e) = self.load_bytes(&bytes, source, sha) {
                        self.message(e, true);
                    }
                }
            }
            if ui
                .add_enabled(
                    self.document.is_some(),
                    egui::Button::new(tr!(language, "Сохранить оригинал .bin…", "Save original .bin…", "Original als .bin speichern…")),
                )
                .clicked()
            {
                self.save_original();
            }
            if ui
                .add_enabled(
                    self.document.is_some() && !self.pending_form(),
                    egui::Button::new(tr!(language, "Отчёт .txt…", "Report .txt…", "Bericht als .txt…")),
                )
                .on_hover_text(tr!(language,
                    "SHA256, известные поля, адреса и точная разница. Отчёт открывается в Блокноте.",
                    "SHA256, known fields, addresses and exact differences. Opens in Notepad.",
                    "SHA256, bekannte Felder, Adressen und exakte Unterschiede. Lässt sich im Texteditor öffnen.",
                ))
                .clicked()
            {
                self.save_report();
            }
            if ui
                .add_enabled(
                    self.document.is_some() && !self.pending_form(),
                    egui::Button::new(tr!(language, "Сохранить пакет правок…", "Save edit bundle…", "Änderungspaket speichern…")),
                )
                .clicked()
            {
                self.save_bundle();
            }
        });
        if !self.notice.is_empty() {
            ui.colored_label(
                if self.error {
                    Color32::LIGHT_RED
                } else {
                    Color32::from_rgb(130, 214, 201)
                },
                self.notice.as_str(language),
            );
        }
        let Some(doc) = &self.document else {
            ui.add_space(25.0);
            ui.label(tr!(language,
                "Откройте calibration.bin из сохранённого снимка или весь 4-МиБ NOR-бэкап.",
                "Open calibration.bin from a saved snapshot or the full 4 MiB NOR backup.",
                "Öffnen Sie calibration.bin aus einem gespeicherten Abbild oder die vollständige 4-MiB-NOR-Sicherung.",
            ));
            ui.label(tr!(language,
                "Файл .880dmf содержит каналы и настройки, но не заводские калибровки.",
                "An .880dmf file contains channels and settings. Factory calibration data is stored separately.",
                "Eine .880dmf-Datei enthält Kanäle und Einstellungen. Die Werkskalibrierung ist separat gespeichert.",
            ));
            return;
        };
        ui.small(self.source.as_str(language));
        ui.horizontal(|ui| {
            ui.monospace(trf!(
                language,
                "SHA256 оригинала: {}",
                "Original SHA256: {}",
                "SHA256 des Originals: {}",
                crate::radio::sha256(doc.original()),
            ));
        });
        ui.label(trf!(language,
            "Изменено байт: {}. Проверка формата учитывает размер и маркер 0x9A. Контрольная сумма блока неизвестна.",
            "Bytes changed: {}. The format check covers size and marker 0x9A. The block checksum is unknown.",
            "Geänderte Bytes: {}. Die Formatprüfung umfasst die Größe und die Kennung 0x9A. Die Blockprüfsumme ist unbekannt.",
            doc.dirty_count(),
        ));
        ui.separator();
        let mut requested_band = self.band;
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.band.is_none(), general_label(language))
                .clicked()
            {
                requested_band = None;
            }
            egui::ComboBox::from_id_salt("calibration_band")
                .selected_text(self.band.map_or_else(
                    || {
                        tr!(
                            language,
                            "Выбрать блок диапазона",
                            "Choose a band block",
                            "Bandblock auswählen"
                        )
                        .into()
                    },
                    |index| band_caption(index, language),
                ))
                .show_ui(ui, |ui| {
                    for band in 0..15 {
                        ui.selectable_value(
                            &mut requested_band,
                            Some(band),
                            band_caption(band, language),
                        );
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(tr!(
                        language,
                        "Поиск параметра",
                        "Find a parameter",
                        "Parameter suchen"
                    ))
                    .desired_width(180.0),
            );
            ui.checkbox(
                &mut self.changed_only,
                tr!(
                    language,
                    "Только изменения",
                    "Changed only",
                    "Nur geänderte"
                ),
            );
            ui.checkbox(
                &mut self.show_readonly,
                tr!(
                    language,
                    "Поля только для чтения",
                    "Show read-only fields",
                    "Schreibgeschützte Felder anzeigen"
                ),
            );
        });
        self.select_band(requested_band);
        if let Some(band) = self.band {
            self.power_graph(ui, band);
        }
        if let Some(field) = self.selected {
            let d = descriptor(field).unwrap();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let group = group_label(d.group, language);
                ui.small(match d.group {
                    Group::General => group,
                    Group::Band { .. } => trf!(language,
                        "{group} — подпись штатной tuning-программы",
                        "{group} — label from the stock tuning tool",
                        "{group} — Beschriftung der Original-Tuningsoftware",
                    ),
                });
                ui.label(RichText::new(field_label(d, language)).strong());
                ui.label(field_detail(d, language).replace('→', " -> "));
                ui.small(trf!(language,
                    "Смещение 0x{:04X}, маска 0x{:04X}. Диапазон кода {}..{}. Безопасный диапазон RF-настройки неизвестен.",
                    "Offset 0x{:04X}, mask 0x{:04X}. Code range {}..{}. Safe RF settings remain unverified.",
                    "Position 0x{:04X}, Maske 0x{:04X}. Codebereich {}..{}. Sichere HF-Einstellungen sind unbestätigt.",
                    d.offset, d.mask, d.min, d.max,
                ));
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled(d.editable, egui::TextEdit::singleline(&mut self.draft).desired_width(130.0));
                    if ui.add_enabled(d.editable && self.pending_form(), egui::Button::new(tr!(language, "Применить значение", "Apply value", "Wert anwenden"))).clicked() {
                        self.apply();
                    }
                    if ui.add_enabled(self.pending_form(), egui::Button::new(tr!(language, "Сбросить форму", "Reset form", "Formular zurücksetzen"))).clicked() {
                        self.draft = self.initial_draft.clone();
                    }
                    if ui.add_enabled(d.editable, egui::Button::new(tr!(language, "Вернуть поле из оригинала", "Restore original field", "Originalfeld wiederherstellen"))).clicked() {
                        self.restore_field();
                    }
                });
            });
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.history.is_empty() && !self.pending_form(),
                    egui::Button::new(tr!(
                        language,
                        "Отменить правку калибровок",
                        "Undo calibration edit",
                        "Kalibrieränderung rückgängig machen"
                    )),
                )
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(
                    self.document.as_ref().is_some_and(|d| d.dirty_count() > 0)
                        || self.pending_form(),
                    egui::Button::new(tr!(
                        language,
                        "Вернуть весь оригинал",
                        "Restore entire original",
                        "Gesamtes Original wiederherstellen"
                    )),
                )
                .clicked()
            {
                self.confirm_reset = true;
            }
            if self.unsaved() {
                ui.colored_label(
                    Color32::YELLOW,
                    tr!(
                        language,
                        "Есть несохранённые правки калибровок",
                        "Unsaved calibration edits",
                        "Ungespeicherte Kalibrieränderungen",
                    ),
                );
            }
        });
        let doc = self.document.as_ref().unwrap();
        let needle = self.search.to_lowercase();
        let mut select = None;
        egui::ScrollArea::both()
            .id_salt("calibration_fields_scroll")
            .max_height(360.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("calibration_fields")
                    .striped(true)
                    .num_columns(4)
                    .spacing([18.0, 7.0])
                    .show(ui, |ui| {
                        ui.strong(tr!(language, "Параметр", "Parameter", "Parameter"));
                        ui.strong(tr!(language, "Оригинал", "Original", "Original"));
                        ui.strong(tr!(
                            language,
                            "Рабочая копия",
                            "Working copy",
                            "Arbeitskopie"
                        ));
                        ui.strong(tr!(language, "Статус", "Status", "Status"));
                        ui.end_row();
                        for field in fields() {
                            let d = descriptor(field).unwrap();
                            let group = match (self.band, d.group) {
                                (None, Group::General) => true,
                                (Some(a), Group::Band { index }) => a == index,
                                _ => false,
                            };
                            if !group || (!d.editable && !self.show_readonly) {
                                continue;
                            }
                            let original = doc.original_value(field).unwrap();
                            let current = doc.value(field).unwrap();
                            if self.changed_only && original == current {
                                continue;
                            }
                            let label = field_label(d, language);
                            let detail = field_detail(d, language);
                            if !needle.is_empty()
                                && !format!("{label} {detail}").to_lowercase().contains(&needle)
                            {
                                continue;
                            }
                            if ui
                                .selectable_label(self.selected == Some(field), label)
                                .on_hover_text(format!("{field:?}\n{detail}\n0x{:04X}", d.offset,))
                                .clicked()
                            {
                                select = Some(field);
                            }
                            ui.monospace(original.to_string());
                            ui.colored_label(
                                if current == original {
                                    Color32::LIGHT_GRAY
                                } else {
                                    Color32::YELLOW
                                },
                                current.to_string(),
                            );
                            ui.label(if !d.editable {
                                tr!(language, "Только чтение", "Read-only", "Schreibgeschützt")
                            } else if original != current {
                                tr!(language, "Изменено", "Changed", "Geändert")
                            } else {
                                tr!(language, "Код", "Code", "Code")
                            });
                            ui.end_row();
                        }
                    });
            });
        if let Some(field) = select {
            self.select(field);
        }
        if self.confirm_reset {
            egui::Modal::new(egui::Id::unique("calibration-reset")).show(ui.ctx(), |ui| {
                ui.heading(tr!(language,
                    "Вернуть исходные калибровки в рабочую копию?", "Restore the original calibration in the working copy?", "Ursprüngliche Kalibrierung in der Arbeitskopie wiederherstellen?",
                ));
                ui.label(tr!(language,
                    "Файлы и радиостанция не изменяются. Правки в форме будут отменены.",
                    "This discards form changes. Files and the radio are unaffected.",
                    "Formularänderungen werden verworfen. Dateien und das Funkgerät bleiben unverändert.",
                ));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr!(language, "Вернуть оригинал", "Restore original", "Original wiederherstellen")).clicked() {
                        self.remember();
                        if let Some(doc) = &mut self.document {
                            doc.reset_all();
                        }
                        self.selected = None;
                        self.confirm_reset = false;
                        self.message(msg!("Рабочая копия возвращена к оригиналу.", "Working copy restored from the original.", "Arbeitskopie aus dem Original wiederhergestellt."), false);
                    }
                    if ui.button(tr!(language, "Отмена", "Cancel", "Abbrechen")).clicked() {
                        self.confirm_reset = false;
                    }
                });
            });
        }
    }
    fn power_graph(&self, ui: &mut egui::Ui, band: usize) {
        let Some(doc) = &self.document else {
            return;
        };
        ui.small(tr!(self.language,
            "Управляющие коды мощности High / Low. Горизонталь показывает номер точки таблицы. Пересчёт в ватты не подтверждён.",
            "High / Low power control codes. The horizontal axis shows table point numbers. Conversion to watts is unverified.",
            "Steuercodes für hohe und niedrige Sendeleistung. Die horizontale Achse zeigt die Tabellenpunktnummern. Die Umrechnung in Watt ist unbestätigt.",
        ));
        let (response, painter) = ui.allocate_painter(
            egui::vec2(ui.available_width().min(820.0), 78.0),
            egui::Sense::hover(),
        );
        let rect = response.rect.shrink(5.0);
        painter.rect_filled(rect, 3.0, Color32::from_rgb(12, 24, 31));
        for (table, color) in [
            (Table::PowerHigh, Color32::from_rgb(74, 206, 200)),
            (Table::PowerLow, Color32::from_rgb(245, 190, 90)),
        ] {
            let points: Vec<egui::Pos2> = (0..16)
                .filter_map(|point| {
                    let field = Field::TablePoint { band, table, point };
                    if !descriptor(field).ok()?.editable {
                        return None;
                    }
                    let value = doc.value(field).ok()? as f32;
                    Some(egui::pos2(
                        rect.left() + rect.width() * point as f32 / 15.0,
                        rect.bottom() - rect.height() * value / 255.0,
                    ))
                })
                .collect();
            for pair in points.windows(2) {
                painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1.5, color));
            }
            for p in points {
                painter.circle_filled(p, 2.0, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv98_calibration_edit::GeneralField;
    const ORIGINAL: &[u8] =
        include_bytes!("../../../crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin");
    #[test]
    fn pending_edits_survive_navigation_and_invalid_edits_and_undo_restore_exact_original() {
        let mut pane = CalibrationPane::default();
        pane.load_bytes(ORIGINAL, "fixture".into(), crate::radio::sha256(ORIGINAL))
            .unwrap();
        let field = Field::General(GeneralField::AdcCorrectionCode);
        pane.select(field);
        pane.draft = "not integer".into();
        pane.apply();
        assert!(pane.pending_form());
        assert!(!pane.unsaved());
        pane.select_band(Some(1));
        assert_eq!(pane.band, None);
        assert_eq!(pane.selected, Some(field));
        assert!(pane
            .load_bytes(ORIGINAL, "replacement".into(), String::new())
            .is_err());
        let old = pane.document.as_ref().unwrap().value(field).unwrap();
        pane.draft = (if old < 127 { old + 1 } else { old - 1 }).to_string();
        pane.apply();
        assert!(pane.unsaved());
        assert!(!pane.pending_form());
        assert_eq!(
            pane.document.as_ref().unwrap().original().as_slice(),
            ORIGINAL
        );
        pane.undo();
        assert!(!pane.unsaved());
        assert_eq!(pane.document.as_ref().unwrap().bytes().as_slice(), ORIGINAL);
        pane.select_band(Some(1));
        assert_eq!(pane.band, Some(1));
        assert!(pane.selected.is_none());
    }

    #[test]
    fn text_report_excludes_unapplied_draft_and_does_not_replace_binary_backup_status() {
        let mut pane = CalibrationPane::default();
        pane.load_bytes(ORIGINAL, "fixture".into(), crate::radio::sha256(ORIGINAL))
            .unwrap();
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/calibration-report-pane-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("report.txt");
        let field = Field::General(GeneralField::AdcCorrectionCode);
        pane.select(field);
        let old = pane.document.as_ref().unwrap().value(field).unwrap();
        pane.draft = (if old < 127 { old + 1 } else { old - 1 }).to_string();
        assert!(pane.export_report_to(&path).is_err());
        assert!(!path.exists());
        pane.apply();
        assert!(pane.unsaved());
        let before = pane.document.as_ref().unwrap().clone();
        let export_sha = pane.last_export_sha.clone();
        pane.export_report_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(&crate::radio::sha256(before.bytes())));
        assert!(pane.unsaved());
        assert_eq!(pane.last_export_sha, export_sha);
        assert_eq!(
            pane.document.as_ref().unwrap().original(),
            before.original()
        );
        assert_eq!(pane.document.as_ref().unwrap().bytes(), before.bytes());
        assert!(pane.export_report_to(&path).is_err());
    }
}
