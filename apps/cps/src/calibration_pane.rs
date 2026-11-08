//! Human-readable local calibration workspace. All edits affect a separate copy.
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::path::PathBuf;
use uv98_calibration::Table;
use uv98_calibration_edit::{band_label, descriptor, fields, Document, Field, Group};

fn field_label(field: Field) -> String {
    let label = descriptor(field).expect("enumerated field").label;
    match field {
        Field::BatteryPoint { index } => format!("{label} / точка {}", index + 1),
        Field::TablePoint { point, .. } => format!("{label} / точка {:02}", point + 1),
        Field::WidthParameter { width, .. } => format!(
            "{} / {label}",
            match width {
                uv98_calibration_edit::ChannelWidth::Wide => "Wide",
                uv98_calibration_edit::ChannelWidth::Narrow => "Narrow",
            }
        ),
        _ => label.to_string(),
    }
}

#[derive(Default)]
pub struct CalibrationPane {
    document: Option<Box<Document>>,
    source: String,
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
    notice: String,
    error: bool,
    confirm_reset: bool,
}
impl CalibrationPane {
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
                "Примените значение или сбросьте форму перед сменой блока диапазона.",
                true,
            );
            return;
        }
        self.band = band;
        self.selected = None;
    }
    fn message(&mut self, message: impl Into<String>, error: bool) {
        self.notice = message.into();
        self.error = error;
    }
    pub fn load_bytes(
        &mut self,
        bytes: &[u8],
        source: String,
        source_sha: String,
    ) -> Result<(), String> {
        if self.pending_form() || self.unsaved() {
            return Err("Сначала сохраните пакет калибровок или верните исходные значения.".into());
        }
        let doc = Document::new(bytes).map_err(|e| format!("{e:?}"))?;
        self.last_export_sha = crate::radio::sha256(doc.bytes());
        self.document = Some(Box::new(doc));
        self.source = source;
        self.source_sha = source_sha;
        self.selected = None;
        self.history.clear();
        self.search.clear();
        self.band = None;
        self.message(
            "Калибровки открыты. Исходный блок сохранён в памяти отдельно от рабочей копии.",
            false,
        );
        Ok(())
    }
    fn open(&mut self) {
        if self.pending_form() || self.unsaved() {
            self.message(
                "Сохраните пакет правок или верните оригинал перед открытием другого блока.",
                true,
            );
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Калибровки 4096 байт или NOR-дамп 4 МиБ")
            .add_filter("Сохранённые данные", &["bin"])
            .pick_file()
        {
            let result = crate::calibration_io::load(&path).and_then(|loaded| {
                let name = format!(
                    "{}{}",
                    path.display(),
                    if loaded.extracted_from_nor {
                        " (извлечён первый блок NOR)"
                    } else {
                        ""
                    }
                );
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
                "Примените значение или сбросьте форму перед выбором другого поля.",
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
                self.message("Введите целое число в указанном диапазоне.", true);
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
                    "Изменена рабочая копия. Для сохранения нажмите «Сохранить пакет правок».",
                    false,
                );
            }
            Err(e) => self.message(format!("Значение отклонено: {e:?}"), true),
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
                    self.message("Поле возвращено к исходному значению.", false);
                }
                Err(e) => self.message(format!("{e:?}"), true),
            }
        }
    }
    fn undo(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.document = Some(previous);
            self.selected = None;
            self.message("Последняя правка калибровок отменена.", false);
        }
    }
    fn save_original(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Сохранить исходные 4096 байт новым файлом")
            .set_file_name("UV98-calibration-original.bin")
            .add_filter("Calibration", &["bin"])
            .save_file()
        else {
            return;
        };
        match crate::calibration_io::save_original(&path, doc.original()) {
            Ok(()) => self.message(
                format!("Оригинал сохранён и проверен: {}", path.display()),
                false,
            ),
            Err(e) => self.message(e, true),
        }
    }
    pub fn report(&self) -> Option<Value> {
        let doc = self.document.as_ref()?;
        let values: Vec<Value> = fields().map(|field| {
            let d=descriptor(field).expect("enumerated descriptor");
            json!({"field":format!("{field:?}"),"group":format!("{:?}",d.group),"label":field_label(field),"detail":d.detail,
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
            json!({"schema":1,"source":self.source,"source_sha256":self.source_sha,"original_sha256":crate::radio::sha256(doc.original()),
            "working_sha256":crate::radio::sha256(doc.bytes()),"values_are_codes":true,"rf_validated":false,"fields":values,"byte_changes":changes}),
        )
    }
    fn save_bundle(&mut self) {
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Папка для нового пакета калибровок")
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
            .set_title("Сохранить читаемый отчёт новым файлом")
            .set_file_name("UV98-calibration-report.txt")
            .add_filter("Отчёт UTF-8", &["txt"])
            .save_file()
        else {
            return;
        };
        match self.export_report_to(&path) {
            Ok(()) => self.message(format!("Отчёт сохранён и проверен: {}. Для бинарного резерва используйте пакет правок.", path.display()), false),
            Err(error) => self.message(error, true),
        }
    }
    pub fn export_report_to(&self, path: &std::path::Path) -> Result<(), String> {
        if self.pending_form() {
            return Err("Сначала примените значение или сбросьте форму".into());
        }
        let doc = self.document.as_ref().ok_or("Калибровки не открыты")?;
        crate::calibration_io::save_text_report(
            path,
            doc.original(),
            doc.bytes(),
            &self.source,
            &self.source_sha,
        )
        // A text report does not mark edited binary bytes as safely backed up.
    }
    pub fn export_to(&mut self, parent: &std::path::Path) -> Result<PathBuf, String> {
        if self.pending_form() {
            return Err("Сначала примените значение или сбросьте форму".into());
        }
        let report = self.report().ok_or("Калибровки не открыты")?;
        let doc = self.document.as_ref().unwrap();
        let path =
            crate::calibration_io::export_bundle(parent, doc.original(), doc.bytes(), &report)?;
        self.last_export_sha = crate::radio::sha256(doc.bytes());
        self.message(
            format!("Пакет сохранён и проверен: {}", path.display()),
            false,
        );
        Ok(path)
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, live: Option<(Vec<u8>, String)>) {
        ui.heading("Калибровки станции");
        ui.label(
            "Оригинал, рабочая копия и точная разница. Редактирование выполняется только в файлах.",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Открыть блок / NOR…").clicked() {
                self.open();
            }
            if ui
                .add_enabled(
                    live.is_some() && !self.unsaved() && !self.pending_form(),
                    egui::Button::new("Из проверенного снимка рации"),
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
                    egui::Button::new("Сохранить оригинал .bin…"),
                )
                .clicked()
            {
                self.save_original();
            }
            if ui
                .add_enabled(
                    self.document.is_some() && !self.pending_form(),
                    egui::Button::new("Отчёт .txt…"),
                )
                .on_hover_text("SHA256, известные поля, адреса и точная разница. Читается в Блокноте; без Excel.")
                .clicked()
            {
                self.save_report();
            }
            if ui
                .add_enabled(
                    self.document.is_some() && !self.pending_form(),
                    egui::Button::new("Сохранить пакет правок…"),
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
                &self.notice,
            );
        }
        let Some(doc) = &self.document else {
            ui.add_space(25.0);
            ui.label("Откройте calibration.bin из сохранённого снимка или весь 4-МиБ NOR-бэкап.");
            ui.label("Файл .880dmf содержит каналы и настройки, но не заводские калибровки.");
            return;
        };
        ui.small(&self.source);
        ui.horizontal(|ui| {
            ui.monospace(format!(
                "SHA256 оригинала: {}",
                crate::radio::sha256(doc.original())
            ));
        });
        ui.label(format!("Изменено байт: {}. Проверка формата: размер и маркер 0x9A; контрольная сумма блока неизвестна.",doc.dirty_count()));
        ui.separator();
        let mut requested_band = self.band;
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.band.is_none(), "Общие параметры")
                .clicked()
            {
                requested_band = None;
            }
            egui::ComboBox::from_id_salt("calibration_band")
                .selected_text(
                    self.band.map_or("Выбрать блок диапазона".into(), |i| {
                        format!("{}: {} МГц", i + 1, band_label(i).unwrap())
                    }),
                )
                .show_ui(ui, |ui| {
                    for band in 0..15 {
                        ui.selectable_value(
                            &mut requested_band,
                            Some(band),
                            format!("{}: {} МГц", band + 1, band_label(band).unwrap()),
                        );
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Поиск параметра")
                    .desired_width(180.0),
            );
            ui.checkbox(&mut self.changed_only, "Только изменения");
            ui.checkbox(&mut self.show_readonly, "Поля только для чтения");
        });
        self.select_band(requested_band);
        if let Some(band) = self.band {
            self.power_graph(ui, band);
        }
        if let Some(field) = self.selected {
            let d = descriptor(field).unwrap();
            egui::Frame::group(ui.style()).show(ui,|ui|{
                ui.small(match d.group { Group::General => "Общие параметры".into(), Group::Band { index } => format!("Блок {}: {} МГц — подпись штатной tuning-программы",index+1,band_label(index).unwrap()) });
                ui.label(RichText::new(field_label(field)).strong());ui.label(d.detail.replace('→', " -> "));
                ui.small(format!("Смещение 0x{:04X}, маска 0x{:04X}. Диапазон кода {}..{}. Это не диапазон безопасной RF-настройки.",d.offset,d.mask,d.min,d.max));
                ui.horizontal_wrapped(|ui|{
                    ui.add_enabled(d.editable,egui::TextEdit::singleline(&mut self.draft).desired_width(130.0));
                    if ui.add_enabled(d.editable&&self.pending_form(),egui::Button::new("Применить значение")).clicked(){self.apply();}
                    if ui.add_enabled(self.pending_form(),egui::Button::new("Сбросить форму")).clicked(){self.draft=self.initial_draft.clone();}
                    if ui.add_enabled(d.editable,egui::Button::new("Вернуть поле из оригинала")).clicked(){self.restore_field();}
                });
            });
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.history.is_empty() && !self.pending_form(),
                    egui::Button::new("Отменить правку калибровок"),
                )
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(
                    self.document.as_ref().is_some_and(|d| d.dirty_count() > 0)
                        || self.pending_form(),
                    egui::Button::new("Вернуть весь оригинал"),
                )
                .clicked()
            {
                self.confirm_reset = true;
            }
            if self.unsaved() {
                ui.colored_label(Color32::YELLOW, "Есть несохранённые правки калибровок");
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
                        ui.strong("Параметр");
                        ui.strong("Оригинал");
                        ui.strong("Рабочая копия");
                        ui.strong("Статус");
                        ui.end_row();
                        for field in fields() {
                            let d = descriptor(field).unwrap();
                            let group = match (self.band, d.group) {
                                (None, Group::General) => true,
                                (Some(a), Group::Band { index }) => a == index,
                                _ => false,
                            };
                            if !group
                                || (!d.editable && !self.show_readonly)
                                || (!needle.is_empty()
                                    && !format!("{} {}", d.label, d.detail)
                                        .to_lowercase()
                                        .contains(&needle))
                            {
                                continue;
                            }
                            let original = doc.original_value(field).unwrap();
                            let current = doc.value(field).unwrap();
                            if self.changed_only && original == current {
                                continue;
                            }
                            if ui
                                .selectable_label(self.selected == Some(field), field_label(field))
                                .on_hover_text(format!(
                                    "{:?}\n{}\n0x{:04X}",
                                    field, d.detail, d.offset
                                ))
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
                                "Только чтение"
                            } else if original != current {
                                "Изменено"
                            } else {
                                "Код"
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
                ui.heading("Вернуть исходные калибровки в рабочую копию?");
                ui.label("Файлы и радиостанция не изменяются. Правки в форме будут отменены.");
                ui.horizontal(|ui| {
                    if ui.button("Вернуть оригинал").clicked() {
                        self.remember();
                        if let Some(doc) = &mut self.document {
                            doc.reset_all();
                        }
                        self.selected = None;
                        self.confirm_reset = false;
                        self.message("Рабочая копия возвращена к оригиналу.", false);
                    }
                    if ui.button("Отмена").clicked() {
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
        ui.small("Кривые управляющих кодов мощности: High / Low. Горизонталь — номер точки таблицы; это не измеренные ватты.");
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
