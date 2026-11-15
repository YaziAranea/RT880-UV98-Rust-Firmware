//! Local CFG edits only. Offsets follow RT880 CPS1.10 MainForm.cs; unrelated
//! bytes, unedited unknown values and original comments are retained verbatim.
use eframe::egui::{self, Color32};
use uv98_codeplug::CFG_LEN;
use uv98_cps_core::Document;
use uv98_signaling::aprs::Address;

const GPS_ENABLE: usize = 214;
const GPS_BAUD: usize = 215;
const CALL: usize = 3120;
const COMMENT: usize = 3129;
const COMMENT_LEN: usize = 128;
const PATH: [usize; 2] = [3259, 3266];
const APRS_ENABLE: usize = 3309;
const TX_LEVEL: usize = 3313;
const LINK: usize = 3316;
const BAUDS: [&str; 10] = [
    "4800", "9600", "14400", "19200", "38400", "56000", "57600", "115200", "128000", "256000",
];
const LEVELS: [&str; 8] = [
    "0 · −10.5 dB",
    "1 · −9.0 dB",
    "2 · −7.5 dB",
    "3 · −6.0 dB",
    "4 · −4.5 dB",
    "5 · −3.0 dB",
    "6 · −1.5 dB",
    "7 · 0 dB",
];
const LINKS: [&str; 7] = [
    "CH-A",
    "CH-B",
    "CH-C",
    "CH-A + B",
    "CH-A + C",
    "CH-B + C",
    "CH-A + B + C",
];

#[derive(Clone, PartialEq, Eq)]
struct Form {
    gps: u8,
    baud: u8,
    aprs: u8,
    call: String,
    ssid: u8,
    paths: [String; 2],
    path_ssid: [u8; 2],
    comment: String,
    level: u8,
    link: u8,
}

pub struct Draft {
    initial: Form,
    current: Form,
    original_comment_needs_preservation: bool,
}

#[derive(Default)]
pub struct Action {
    pub apply: bool,
    pub reset: bool,
}

fn decode(raw: &[u8]) -> String {
    let end = raw
        .iter()
        .position(|b| matches!(*b, 0 | 0xff))
        .unwrap_or(raw.len());
    crate::text_codec::decode_name(&raw[..end]).0
}

impl Draft {
    pub fn new(cfg: &[u8]) -> Option<Self> {
        if cfg.len() != CFG_LEN {
            return None;
        }
        let comment = decode(&cfg[COMMENT..COMMENT + COMMENT_LEN]);
        let initial = Form {
            gps: cfg[GPS_ENABLE],
            baud: cfg[GPS_BAUD],
            aprs: cfg[APRS_ENABLE],
            call: decode(&cfg[CALL..CALL + 6])
                .trim_end_matches(' ')
                .to_owned(),
            ssid: cfg[CALL + 6],
            paths: PATH.map(|offset| {
                decode(&cfg[offset..offset + 6])
                    .trim_end_matches(' ')
                    .to_owned()
            }),
            path_ssid: PATH.map(|offset| cfg[offset + 6]),
            comment,
            level: cfg[TX_LEVEL],
            link: cfg[LINK],
        };
        let original_comment_needs_preservation = !valid_comment(&initial.comment);
        Some(Self {
            current: initial.clone(),
            initial,
            original_comment_needs_preservation,
        })
    }

    pub fn changed(&self) -> bool {
        self.initial != self.current
    }

    pub fn apply(&self, document: &mut Document) -> Result<usize, String> {
        document
            .apply(|bytes| self.patch(&mut bytes[..CFG_LEN]))
            .map(|summary| summary.changed_bytes)
    }

    fn patch(&self, cfg: &mut [u8]) -> Result<(), String> {
        if cfg.len() != CFG_LEN {
            return Err("Неверная длина CFG".into());
        }
        let a = &self.initial;
        let b = &self.current;
        for (offset, before, after, max, label) in [
            (GPS_ENABLE, a.gps, b.gps, 1, "GPS"),
            (GPS_BAUD, a.baud, b.baud, 9, "Скорость GPS"),
            (APRS_ENABLE, a.aprs, b.aprs, 1, "APRS"),
            (CALL + 6, a.ssid, b.ssid, 15, "SSID"),
            (TX_LEVEL, a.level, b.level, 7, "Уровень TX"),
            (LINK, a.link, b.link, 6, "Привязка APRS"),
            (
                PATH[0] + 6,
                a.path_ssid[0],
                b.path_ssid[0],
                15,
                "SSID пути 1",
            ),
            (
                PATH[1] + 6,
                a.path_ssid[1],
                b.path_ssid[1],
                15,
                "SSID пути 2",
            ),
        ] {
            if before != after {
                if after > max {
                    return Err(format!("{label}: выберите поддерживаемое значение"));
                }
                cfg[offset] = after;
            }
        }
        if b.call != a.call {
            encode_call(&b.call, b.ssid, false, &mut cfg[CALL..CALL + 6])?;
        }
        if b.call != a.call || b.ssid != a.ssid {
            Address::new(&cfg[CALL..CALL + 6], cfg[CALL + 6])
                .map_err(|_| "Позывной: 1–6 латинских букв/цифр и SSID 0–15")?;
        }
        for index in 0..2 {
            if b.paths[index] != a.paths[index] {
                encode_call(
                    &b.paths[index],
                    b.path_ssid[index],
                    true,
                    &mut cfg[PATH[index]..PATH[index] + 6],
                )?;
            }
            if (b.paths[index] != a.paths[index] || b.path_ssid[index] != a.path_ssid[index])
                && !matches!(cfg[PATH[index]], 0 | 0xff | b' ')
            {
                Address::new(&cfg[PATH[index]..PATH[index] + 6], cfg[PATH[index] + 6])
                    .map_err(|_| "Путь APRS: 1–6 латинских букв/цифр и SSID 0–15")?;
            }
        }
        if b.comment != a.comment {
            if !valid_comment(&b.comment) {
                return Err(
                    "Новый комментарий: до 32 печатных символов ASCII без переносов строк".into(),
                );
            }
            cfg[COMMENT..COMMENT + COMMENT_LEN].fill(0xff);
            cfg[COMMENT..COMMENT + b.comment.len()].copy_from_slice(b.comment.as_bytes());
        }
        // Enabling APRS is explicit: require a usable identity and paths. Do not
        // normalize any unchanged bytes or reject unrelated GPS-only edits.
        if b.aprs == 1 && b.aprs != a.aprs {
            Address::new(&cfg[CALL..CALL + 6], cfg[CALL + 6])
                .map_err(|_| "Для APRS задайте позывной 1–6 латинских букв/цифр и SSID 0–15")?;
            for offset in PATH {
                if !matches!(cfg[offset], 0 | 0xff | b' ') {
                    Address::new(&cfg[offset..offset + 6], cfg[offset + 6])
                        .map_err(|_| "Путь APRS: 1–6 латинских букв/цифр и SSID 0–15")?;
                }
            }
        }
        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Action {
        ui.heading("GPS / APRS");
        ui.label("Параметры открытого файла CPS. «Применить в файл» меняет только документ на компьютере.");
        ui.small("Сохраните отдельную .880dmf / .98pdmf. Перенос CFG в рацию выполняется штатной CPS; запись настроек R35 в NOR здесь не реализована.");
        ui.add_space(10.0);
        egui::ScrollArea::vertical().id_salt("navigation_scroll")
            .max_height((ui.available_height()-70.0).max(160.0)).show(ui, |ui| {
            egui::Grid::new("navigation_fields").num_columns(2).spacing([24.0,12.0]).show(ui, |ui| {
                ui.label("GPS / питание"); choice(ui,"nav_gps",&mut self.current.gps,&["Выключен","Включён"]); ui.end_row();
                ui.label("GPS / скорость UART"); choice(ui,"nav_baud",&mut self.current.baud,&BAUDS); ui.end_row();
                ui.label("APRS / разрешение"); choice(ui,"nav_aprs",&mut self.current.aprs,&["Выключен","Включён"]); ui.end_row();
                ui.label("Позывной (без суффикса SSID)");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.current.call).desired_width(160.0).hint_text("N0CALL"));
                    ui.label("SSID"); number(ui,"nav_ssid",&mut self.current.ssid,15);
                }); ui.end_row();
                for index in 0..2 {
                    ui.label(format!("Путь {} (пусто — не использовать)",index+1));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.current.paths[index]).desired_width(160.0).hint_text("WIDE1"));
                        ui.label("SSID"); number(ui,if index==0 {"path_ssid_0"}else{"path_ssid_1"},&mut self.current.path_ssid[index],15);
                    }); ui.end_row();
                }
                ui.label("Комментарий APRS / имя станции");
                ui.add(egui::TextEdit::singleline(&mut self.current.comment).desired_width(420.0).hint_text("До 32 символов ASCII")); ui.end_row();
                ui.label("Уровень модуляции CMX (не RF-мощность)"); choice(ui,"nav_level",&mut self.current.level,&LEVELS); ui.end_row();
                ui.label("Привязка APRS в стоковой прошивке"); choice(ui,"nav_link",&mut self.current.link,&LINKS); ui.end_row();
            });
            ui.add_space(12.0);
            if self.original_comment_needs_preservation && self.current.comment == self.initial.comment {
                ui.colored_label(Color32::from_rgb(245,190,100),"Исходный комментарий длиннее 32 символов или содержит не-ASCII. Он сохранится побайтно, пока вы его не измените.");
            }
            ui.small("Позывные при явном редактировании переводятся в верхний регистр. Неизвестные значения остаются неизменными, пока не выбран новый вариант.");
            ui.small("R35: ручной маяк передаётся на частоте TX активного A/B, показанной на экране APRS. Стоковая привязка A/B/C сохраняется для совместимости, но не управляет этим ручным маяком.");
            ui.small("Комментарий — текст в APRS. Select V передаёт числовой ID, а не текстовое имя. Автоматический маяк от одного появления GPS-позиции не запускается.");
        });
        let mut action = Action::default();
        ui.horizontal(|ui| {
            action.apply = ui
                .add_enabled(self.changed(), egui::Button::new("Применить в файл"))
                .clicked();
            action.reset = ui
                .add_enabled(self.changed(), egui::Button::new("Сбросить форму"))
                .clicked();
            if self.changed() {
                ui.label("Форма ещё не применена");
            }
        });
        action
    }
}

fn valid_comment(text: &str) -> bool {
    text.len() <= 32 && text.bytes().all(|b| (0x20..=0x7e).contains(&b))
}
fn encode_call(text: &str, ssid: u8, optional: bool, out: &mut [u8]) -> Result<(), String> {
    if text.is_empty() && optional {
        out.fill(0xff);
        return Ok(());
    }
    if text.is_empty()
        || text.len() > 6
        || !text.bytes().all(|b| b.is_ascii_alphanumeric())
        || ssid > 15
    {
        return Err("Позывной/путь: 1–6 латинских букв или цифр; SSID 0–15".into());
    }
    let upper = text.to_ascii_uppercase();
    Address::new(upper.as_bytes(), ssid).map_err(|_| "Недопустимый позывной")?;
    out.fill(0xff);
    out[..upper.len()].copy_from_slice(upper.as_bytes());
    Ok(())
}
fn choice(ui: &mut egui::Ui, id: &str, current: &mut u8, labels: &[&str]) {
    let selected = labels.get(usize::from(*current)).map_or_else(
        || format!("Неизвестное значение {} — сохранить", *current),
        |v| (*v).to_owned(),
    );
    egui::ComboBox::from_id_salt(id)
        .width(280.0)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (code, label) in labels.iter().enumerate() {
                ui.selectable_value(current, code as u8, *label);
            }
        });
}
fn number(ui: &mut egui::Ui, id: &str, current: &mut u8, max: u8) {
    egui::ComboBox::from_id_salt(id)
        .width(80.0)
        .selected_text(if *current <= max {
            current.to_string()
        } else {
            format!("? {}", *current)
        })
        .show_ui(ui, |ui| {
            for code in 0..=max {
                ui.selectable_value(current, code, code.to_string());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv98_codeplug::FILE_LEN;
    #[test]
    fn navigation_draft_participates_in_close_save_and_snapshot_guards() {
        let mut app = crate::App::default();
        app.document = Some(Document::from_bytes(&vec![0xff; FILE_LEN]).unwrap());
        app.last_export = app.document.as_ref().unwrap().export_bytes();
        app.refresh_forms();
        assert!(!app.pending_form());
        assert!(app.can_apply_snapshot());
        app.navigation_draft.as_mut().unwrap().current.gps = 1;
        assert!(app.pending_form());
        assert!(!app.can_apply_snapshot());
        assert!(!app.navigation_allowed());
        app.apply_navigation();
        assert!(!app.pending_form());
        assert!(app.unsaved());
        assert_eq!(app.document.as_ref().unwrap().undo_depth(), 1);
        app.document.as_mut().unwrap().undo();
        app.refresh_forms();
        assert!(!app.unsaved());
    }
    #[test]
    fn gps_edit_preserves_unknown_codes_comments_and_every_other_byte() {
        let mut bytes = vec![0xa5; FILE_LEN];
        bytes[GPS_ENABLE] = 0;
        bytes[COMMENT..COMMENT + COMMENT_LEN].fill(b'Q');
        let mut document = Document::from_bytes(&bytes).unwrap();
        let mut draft = Draft::new(document.view().cfg()).unwrap();
        assert_eq!(draft.apply(&mut document).unwrap(), 0);
        assert_eq!(document.undo_depth(), 0);
        draft.current.gps = 1;
        assert_eq!(draft.apply(&mut document).unwrap(), 1);
        bytes[GPS_ENABLE] = 1;
        assert_eq!(document.bytes(), bytes);
        assert!(document.undo());
        bytes[GPS_ENABLE] = 0;
        assert_eq!(document.bytes(), bytes);
    }
    #[test]
    fn explicit_identity_path_comment_changes_patch_only_documented_fields() {
        let bytes = vec![0xff; FILE_LEN];
        let mut document = Document::from_bytes(&bytes).unwrap();
        let mut draft = Draft::new(document.view().cfg()).unwrap();
        draft.current.call = "n0call".into();
        draft.current.ssid = 7;
        draft.current.paths[0] = "WIDE1".into();
        draft.current.path_ssid[0] = 1;
        draft.current.comment = "Portable".into();
        draft.current.aprs = 1;
        draft.apply(&mut document).unwrap();
        let cfg = document.view().cfg();
        assert_eq!(&cfg[CALL..CALL + 6], b"N0CALL");
        assert_eq!(cfg[CALL + 6], 7);
        assert_eq!(&cfg[PATH[0]..PATH[0] + 6], b"WIDE1\xff");
        assert_eq!(cfg[PATH[0] + 6], 1);
        assert_eq!(&cfg[COMMENT..COMMENT + 8], b"Portable");
        for (i, (&before, &after)) in bytes.iter().zip(document.bytes()).enumerate() {
            if before != after {
                assert!(
                    (CALL..CALL + 7).contains(&i)
                        || (COMMENT..COMMENT + COMMENT_LEN).contains(&i)
                        || (PATH[0]..PATH[0] + 7).contains(&i)
                        || i == APRS_ENABLE
                );
            }
        }
        assert_eq!(document.original_bytes(), bytes);
    }
    #[test]
    fn invalid_edited_field_rolls_back_other_valid_changes_atomically() {
        let bytes = vec![0xff; FILE_LEN];
        let mut document = Document::from_bytes(&bytes).unwrap();
        let mut draft = Draft::new(document.view().cfg()).unwrap();
        draft.current.gps = 1;
        draft.current.comment = "Недопустимо".into();
        assert!(draft.apply(&mut document).is_err());
        assert_eq!(document.bytes(), bytes);
        assert_eq!(document.undo_depth(), 0);
        draft.current.comment = draft.initial.comment.clone();
        draft.current.call = "ABC DEF".into();
        assert!(draft.apply(&mut document).is_err());
        assert_eq!(document.bytes(), bytes);
    }
}
