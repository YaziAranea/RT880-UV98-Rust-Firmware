#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod calibration_io;
mod calibration_pane;
mod calibration_report;
mod file_io;
mod radio;
mod text_codec;
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use text_codec::{
    decode_name, encode_name, format_frequency, format_tone, parse_frequency, parse_tone,
};
use uv98_codeplug::channels::{self, Bandwidth, Channel, ChannelPatch, OperationMode, Power};
use uv98_codeplug::settings::{Field, Settings, SettingsMut};
use uv98_codeplug::zones::{Zone, Zones, ZonesMut, EMPTY_MEMBER, MEMBER_COUNT, ZONE_COUNT};
use uv98_codeplug::{
    Section, CFG_LEN, CHANNELS_LEN, CHANNELS_OFFSET, FILE_LEN, ZONES_LEN, ZONES_OFFSET,
};
use uv98_cps_core::{zone_display_members, Document, ZoneDisplayError};

const TITLE: &str = concat!("UV98 CPS — ", env!("CARGO_PKG_VERSION"));
const FIELDS: [Field; 24] = Field::ALL;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([940.0, 640.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            let mut style = (*cc.egui_ctx.global_style()).clone();
            style.visuals = egui::Visuals::dark();
            style.visuals.panel_fill = Color32::from_rgb(20, 31, 37);
            style.visuals.window_fill = Color32::from_rgb(25, 39, 46);
            style.visuals.extreme_bg_color = Color32::from_rgb(13, 23, 29);
            style.visuals.selection.bg_fill = Color32::from_rgb(26, 105, 112);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(32, 119, 125);
            style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 72, 80);
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(23.0));
            cc.egui_ctx.set_global_style(style);
            let mut app = App::default();
            if let Some(path) = initial {
                app.load(path);
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Channels,
    Zones,
    Settings,
    Changes,
    Radio,
    Calibration,
}
#[derive(Clone, Copy)]
enum Action {
    Open,
    Close,
    Reset,
}

#[derive(Clone, PartialEq, Eq)]
struct ChannelForm {
    name: String,
    rx: String,
    tx: String,
    mode: u8,
    bandwidth: u8,
    power: u8,
    scan: bool,
    rx_tone: String,
    tx_tone: String,
}
struct ChannelDraft {
    index: usize,
    initial: ChannelForm,
    current: ChannelForm,
    name_errors: bool,
}
impl ChannelDraft {
    fn from_channel(index: usize, channel: Channel<'_>) -> Self {
        let (name, name_errors) = decode_name(channel.name_raw());
        let initial = ChannelForm {
            name,
            rx: format_frequency(channel.rx_frequency_code()),
            tx: format_frequency(channel.tx_frequency_code()),
            mode: channel.operation_mode_raw(),
            bandwidth: channel.bandwidth_raw(),
            power: channel.power_raw(),
            scan: channel.scan_add(),
            rx_tone: format_tone(channel.rx_tone_raw()),
            tx_tone: format_tone(channel.tx_tone_raw()),
        };
        Self {
            index,
            current: initial.clone(),
            initial,
            name_errors,
        }
    }
    fn changed(&self) -> bool {
        self.current != self.initial
    }
    fn patch(&self) -> Result<ChannelPatch, String> {
        let a = &self.initial;
        let b = &self.current;
        let mut patch = ChannelPatch::default();
        if b.name != a.name {
            patch.name_raw = Some(encode_name(&b.name)?);
        }
        if b.rx != a.rx {
            patch.rx_frequency_code = Some(edited_frequency(&b.rx)?);
        }
        if b.tx != a.tx {
            patch.tx_frequency_code = Some(edited_frequency(&b.tx)?);
        }
        if b.mode != a.mode {
            patch.operation_mode =
                Some(OperationMode::decode(b.mode).ok_or("Выберите режим RX/TX")?);
        }
        if b.bandwidth != a.bandwidth {
            patch.bandwidth = Some(Bandwidth::decode(b.bandwidth).ok_or("Выберите полосу")?);
        }
        if b.power != a.power {
            patch.power = Some(Power::decode(b.power).ok_or("Выберите мощность")?);
        }
        if b.scan != a.scan {
            patch.scan_add = Some(b.scan);
        }
        // Unchanged unknown tone text is never parsed or normalized.
        if b.rx_tone != a.rx_tone {
            patch.rx_tone = Some(parse_tone(&b.rx_tone)?);
        }
        if b.tx_tone != a.tx_tone {
            patch.tx_tone = Some(parse_tone(&b.tx_tone)?);
        }
        Ok(patch)
    }
}
fn edited_frequency(text: &str) -> Result<u32, String> {
    let code = parse_frequency(text)?;
    if (1_800_000..66_000_000).contains(&code) || (84_000_000..100_000_000).contains(&code) {
        Ok(code)
    } else {
        Err("Как в исходном CPS: 18 ≤ МГц < 660 или 840 ≤ МГц < 1000".into())
    }
}
#[derive(Clone, PartialEq, Eq)]
struct ZoneForm {
    name: String,
    members: String,
}
struct ZoneDraft {
    index: usize,
    initial: ZoneForm,
    current: ZoneForm,
    name_errors: bool,
    members_error: Option<ZoneDisplayError>,
}
impl ZoneDraft {
    fn from_zone(index: usize, zone: Zone<'_>) -> Self {
        let (name, name_errors) = decode_name(zone.name_raw());
        let (members, members_error) = match zone_display_members(zone) {
            Ok(members) => (
                members
                    .iter()
                    .map(|code| (code + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                None,
            ),
            Err(error) => (String::new(), Some(error)),
        };
        let initial = ZoneForm { name, members };
        Self {
            index,
            current: initial.clone(),
            initial,
            name_errors,
            members_error,
        }
    }
    fn changed(&self) -> bool {
        self.current != self.initial
    }
}
struct SettingsDraft {
    initial: [u8; FIELDS.len()],
    current: [u8; FIELDS.len()],
}
impl SettingsDraft {
    fn changed(&self) -> bool {
        self.current != self.initial
    }
}

struct App {
    document: Option<Document>,
    source: Option<PathBuf>,
    last_export: Vec<u8>,
    tab: Tab,
    search: String,
    active_only: bool,
    channel_index: Option<usize>,
    channel_draft: Option<ChannelDraft>,
    zone_index: usize,
    zone_draft: Option<ZoneDraft>,
    settings_draft: Option<SettingsDraft>,
    notice: String,
    notice_error: bool,
    pending_action: Option<Action>,
    close_allowed: bool,
    radio: radio::RadioPane,
    calibrations: calibration_pane::CalibrationPane,
}
impl Default for App {
    fn default() -> Self {
        Self {
            document: None,
            source: None,
            last_export: Vec::new(),
            tab: Tab::Channels,
            search: String::new(),
            active_only: true,
            channel_index: None,
            channel_draft: None,
            zone_index: 0,
            zone_draft: None,
            settings_draft: None,
            notice: "Откройте сохранённый файл .880dmf или .98pdmf".into(),
            notice_error: false,
            pending_action: None,
            close_allowed: false,
            radio: radio::RadioPane::default(),
            calibrations: calibration_pane::CalibrationPane::default(),
        }
    }
}
impl App {
    fn message(&mut self, text: impl Into<String>, error: bool) {
        self.notice = text.into();
        self.notice_error = error;
    }
    fn pending_form(&self) -> bool {
        self.calibrations.pending_form()
            || self
                .channel_draft
                .as_ref()
                .is_some_and(ChannelDraft::changed)
            || self.zone_draft.as_ref().is_some_and(ZoneDraft::changed)
            || self
                .settings_draft
                .as_ref()
                .is_some_and(SettingsDraft::changed)
    }
    fn unsaved(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.bytes() != self.last_export)
    }
    fn navigation_allowed(&mut self) -> bool {
        if self.radio.busy() {
            self.message(
                "Дождитесь завершения чтения или нажмите «Отменить» на вкладке «Радиостанция».",
                true,
            );
            false
        } else if self.pending_form() {
            self.message(
                "В форме есть изменения. Нажмите «Применить» или «Сбросить форму» перед переходом.",
                true,
            );
            false
        } else {
            true
        }
    }
    fn can_apply_snapshot(&self) -> bool {
        !self.radio.busy()
            && !self.pending_form()
            && !self.unsaved()
            && self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_dirty())
    }
    fn load(&mut self, path: PathBuf) {
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
        {
            let result = calibration_io::load(&path).and_then(|loaded| {
                self.calibrations.load_bytes(
                    &loaded.bytes,
                    path.display().to_string(),
                    loaded.source_sha256,
                )
            });
            match result {
                Ok(()) => self.tab = Tab::Calibration,
                Err(e) => self.message(e, true),
            }
            return;
        }
        let result = file_io::read_document(&path);
        match result {
            Ok(document) => {
                self.last_export = document.export_bytes();
                self.channel_index = (0..channels::CHANNEL_COUNT).find(|&index| {
                    channels::channel(document.view().channels(), index)
                        .is_ok_and(|channel| channel.is_active())
                });
                self.document = Some(document);
                self.source = Some(path);
                self.tab = Tab::Channels;
                self.zone_index = 0;
                self.search.clear();
                self.refresh_forms();
                self.message(
                    "Файл открыт. Изменения сохраняются отдельной копией.",
                    false,
                );
            }
            Err(error) => self.message(format!("Не удалось открыть файл: {error}"), true),
        }
    }
    fn refresh_forms(&mut self) {
        self.channel_draft = None;
        self.zone_draft = None;
        self.settings_draft = None;
        let Some(document) = &self.document else {
            return;
        };
        let view = document.view();
        if let Some(index) = self.channel_index {
            if let Ok(channel) = channels::channel(view.channels(), index) {
                if channel.is_active() {
                    self.channel_draft = Some(ChannelDraft::from_channel(index, channel));
                }
            }
        }
        if let Some(zone) = Zones::parse(view.zones())
            .ok()
            .and_then(|zones| zones.zone(self.zone_index))
        {
            self.zone_draft = Some(ZoneDraft::from_zone(self.zone_index, zone));
        }
        if let Ok(settings) = Settings::parse(view.cfg()) {
            let initial = FIELDS.map(|field| settings.code(field));
            self.settings_draft = Some(SettingsDraft {
                initial,
                current: initial,
            });
        }
    }
    fn request_action(&mut self, action: Action, ctx: &egui::Context) {
        if self.radio.busy() {
            self.message(
                "Чтение ещё выполняется. Сначала дождитесь завершения или отмените его.",
                true,
            );
            return;
        }
        if self.pending_form()
            || self.unsaved()
            || (matches!(action, Action::Close) && self.calibrations.unsaved())
            || matches!(action, Action::Reset)
        {
            self.pending_action = Some(action);
        } else {
            self.perform_action(action, ctx);
        }
    }
    fn perform_action(&mut self, action: Action, ctx: &egui::Context) {
        if self.radio.busy() {
            if matches!(action, Action::Close) {
                self.radio.cancel();
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            self.message("Ожидается завершение чтения и закрытие COM-порта.", true);
            return;
        }
        match action {
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Открыть файл CPS")
                    .add_filter("Файлы CPS", &["880dmf", "98pdmf"])
                    .pick_file()
                {
                    self.load(path);
                }
            }
            Action::Close => {
                self.close_allowed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Reset => {
                if let Some(document) = &mut self.document {
                    document.revert_all();
                }
                self.refresh_forms();
                self.message("Восстановлено содержимое исходного файла.", false);
            }
        }
    }
    fn save_copy(&mut self) {
        if !self.navigation_allowed() {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title("Сохранить новую копию CPS")
            .add_filter("Файлы CPS", &["880dmf", "98pdmf"]);
        if let Some(source) = &self.source {
            if let Some(parent) = source.parent() {
                dialog = dialog.set_directory(parent);
            }
            let stem = source.file_stem().unwrap_or_default().to_string_lossy();
            let extension = source.extension().unwrap_or_default().to_string_lossy();
            dialog = dialog.set_file_name(format!("{stem}-edited.{extension}"));
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        let bytes = document.export_bytes();
        let result = file_io::save_new_copy(&path, &bytes);
        match result {
            Ok(()) => {
                self.last_export = bytes;
                self.message(format!("Копия сохранена: {}", path.display()), false);
            }
            Err(error) => self.message(
                format!(
                    "Не удалось сохранить копию: {error}. Проверьте файл {} перед использованием.",
                    path.display()
                ),
                true,
            ),
        }
    }
    fn apply_channel(&mut self) {
        if self.radio.busy() {
            return;
        }
        let result = (|| -> Result<usize, String> {
            let draft = self.channel_draft.as_ref().ok_or("Канал не выбран")?;
            let patch = draft.patch()?;
            let index = draft.index;
            self.document
                .as_mut()
                .ok_or("Файл не открыт")?
                .apply(|bytes| {
                    channels::patch_channel(
                        &mut bytes[CHANNELS_OFFSET..CHANNELS_OFFSET + CHANNELS_LEN],
                        index,
                        &patch,
                    )
                })
                .map(|summary| summary.changed_bytes)
                .map_err(|error| error.to_string())
        })();
        self.finish_edit(result);
    }
    fn apply_zone(&mut self) {
        if self.radio.busy() {
            return;
        }
        let result = (|| -> Result<usize, String> {
            let draft = self.zone_draft.as_ref().ok_or("Зона не выбрана")?;
            let name = if draft.current.name != draft.initial.name {
                Some(encode_name(&draft.current.name)?)
            } else {
                None
            };
            let members = if draft.current.members != draft.initial.members {
                if draft.members_error.is_some() {
                    return Err(
                        "Состав зоны не изменён: исходный заголовок или ссылки противоречивы."
                            .into(),
                    );
                }
                Some(parse_members(&draft.current.members)?)
            } else {
                None
            };
            let index = draft.index;
            self.document
                .as_mut()
                .ok_or("Файл не открыт")?
                .apply(|bytes| -> Result<(), String> {
                    let mut zones =
                        ZonesMut::parse(&mut bytes[ZONES_OFFSET..ZONES_OFFSET + ZONES_LEN])
                            .map_err(|error| format!("{error:?}"))?;
                    let mut zone = zones.zone(index).ok_or("Неверный индекс зоны")?;
                    if let Some(name) = name {
                        zone.set_name_raw(&name);
                    }
                    if let Some(members) = &members {
                        for slot in 0..MEMBER_COUNT {
                            zone.set_member_code(
                                slot,
                                members.get(slot).copied().unwrap_or(EMPTY_MEMBER),
                            )
                            .map_err(|error| format!("{error:?}"))?;
                        }
                        zone.refresh_cps_count();
                    }
                    Ok(())
                })
                .map(|summary| summary.changed_bytes)
        })();
        self.finish_edit(result);
    }
    fn apply_settings(&mut self) {
        if self.radio.busy() {
            return;
        }
        let result = (|| -> Result<usize, String> {
            let draft = self.settings_draft.as_ref().ok_or("Настройки недоступны")?;
            let initial = draft.initial;
            let current = draft.current;
            self.document
                .as_mut()
                .ok_or("Файл не открыт")?
                .apply(|bytes| -> Result<(), String> {
                    let mut settings = SettingsMut::parse(&mut bytes[..CFG_LEN])
                        .map_err(|error| format!("{error:?}"))?;
                    for index in 0..FIELDS.len() {
                        if current[index] != initial[index] {
                            settings
                                .set_code(FIELDS[index], current[index])
                                .map_err(|error| format!("{error:?}"))?;
                        }
                    }
                    Ok(())
                })
                .map(|summary| summary.changed_bytes)
        })();
        self.finish_edit(result);
    }
    fn finish_edit(&mut self, result: Result<usize, String>) {
        match result {
            Ok(changed) => {
                self.refresh_forms();
                self.message(format!("Применено. Изменено байт: {changed}. Для записи файла нажмите «Сохранить копию»."), false);
            }
            Err(error) => self.message(error, true),
        }
    }
}

impl App {
    fn radio_ui(&mut self, ui: &mut egui::Ui) {
        let can_apply = self.can_apply_snapshot();
        let snapshot = egui::CentralPanel::default()
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("radio_pane_scroll")
                    .show(ui, |ui| self.radio.ui(ui, can_apply))
                    .inner
            })
            .inner;
        if let Some(snapshot) = snapshot {
            if !self.navigation_allowed() {
                return;
            }
            if !self.can_apply_snapshot() {
                self.message("Для применения снимка откройте файл CPS без правок. Непрочитанная секция Time будет сохранена из этого файла.", true);
                return;
            }
            let result = radio::apply_to_document(
                &snapshot,
                self.document.as_mut().expect("checked snapshot target"),
            );
            match result {
                Ok(changed) => {
                    self.refresh_forms();
                    self.message(format!("Снимок применён: {changed} изменённых байт. Секция Time сохранена из исходного файла. Сохраните отдельную копию."), false);
                }
                Err(error) => self.message(error, true),
            }
        }
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let busy = self.radio.busy();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("UV98 CPS").size(23.0).strong());
                ui.label(
                    RichText::new(concat!(env!("CARGO_PKG_VERSION"), " · КАЛИБРОВКИ"))
                        .color(Color32::from_rgb(100, 204, 202)),
                );
                ui.separator();
                if ui
                    .add_enabled(!busy, egui::Button::new("Открыть…"))
                    .clicked()
                {
                    self.request_action(Action::Open, &ctx);
                }
                if ui
                    .add_enabled(
                        self.document.is_some() && !busy,
                        egui::Button::new("Сохранить копию CPS…"),
                    )
                    .clicked()
                {
                    self.save_copy();
                }
                let undo = self.document.as_ref().map_or(0, Document::undo_depth);
                if ui
                    .add_enabled(undo > 0 && !busy, egui::Button::new("Отменить правку CPS"))
                    .clicked()
                    && self.navigation_allowed()
                {
                    if let Some(document) = &mut self.document {
                        document.undo();
                    }
                    self.refresh_forms();
                    self.message("Последняя правка отменена.", false);
                }
            });
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Channels, "Каналы"),
                    (Tab::Zones, "Зоны"),
                    (Tab::Settings, "Настройки"),
                    (Tab::Calibration, "Калибровки"),
                    (Tab::Changes, "Изменения"),
                    (Tab::Radio, "Радиостанция"),
                ] {
                    if ui
                        .add_enabled(
                            !busy || tab == Tab::Radio,
                            egui::Button::new(label).selected(self.tab == tab),
                        )
                        .clicked()
                        && self.tab != tab
                        && self.navigation_allowed()
                    {
                        self.tab = tab;
                    }
                }
                if let Some(path) = &self.source {
                    ui.separator();
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy())
                        .on_hover_text(path.display().to_string());
                    if self.unsaved() {
                        ui.colored_label(
                            Color32::from_rgb(245, 190, 100),
                            "● Есть несохранённые правки",
                        );
                    }
                }
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            if self.tab == Tab::Calibration {
                ui.small(
                    "Калибровки сохраняются отдельно от файла каналов. Запись в станцию отключена.",
                );
            } else if !self.notice.is_empty() {
                let color = if self.notice_error {
                    Color32::from_rgb(255, 172, 133)
                } else {
                    Color32::from_rgb(153, 214, 203)
                };
                ui.colored_label(color, &self.notice);
            }
            ui.horizontal(|ui| {
                ui.small("Редактор файлов CPS · Чтение рации; запись отключена");
                if let Some(document) = &self.document {
                    ui.separator();
                    ui.small(format!(
                        "{} байт · {} изменено от исходника",
                        FILE_LEN,
                        document.dirty_bytes()
                    ));
                }
            });
        });
    }

    fn channels_ui(&mut self, ui: &mut egui::Ui) {
        let mut rows = Vec::new();
        let mut active_count = 0;
        if let Some(document) = &self.document {
            let view = document.view();
            let needle = self.search.trim().to_lowercase();
            for index in 0..channels::CHANNEL_COUNT {
                let Ok(channel) = channels::channel(view.channels(), index) else {
                    continue;
                };
                let active = channel.is_active();
                if active {
                    active_count += 1;
                }
                if self.active_only && !active {
                    continue;
                }
                let (name, _) = decode_name(channel.name_raw());
                let name = if !active {
                    "(пусто)".into()
                } else if name.is_empty() {
                    "(без имени)".into()
                } else {
                    name
                };
                let rx = if active {
                    format_frequency(channel.rx_frequency_code())
                } else {
                    String::new()
                };
                let tx = if active {
                    format_frequency(channel.tx_frequency_code())
                } else {
                    String::new()
                };
                if !needle.is_empty()
                    && !format!("{} {name} {rx} {tx}", index + 1)
                        .to_lowercase()
                        .contains(&needle)
                {
                    continue;
                }
                rows.push((index, name, rx));
            }
        }
        let mut selected = None;
        egui::Panel::left("channel_list")
            .default_size(410.0)
            .size_range(300.0..=580.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.heading("Каналы");
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Поиск: имя, частота, номер")
                        .desired_width(f32::INFINITY),
                );
                ui.checkbox(&mut self.active_only, "Только заполненные");
                ui.small(format!(
                    "Найдено: {} · заполнено: {active_count} / 1024",
                    rows.len()
                ));
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("channels_scroll")
                    .show_rows(ui, 30.0, rows.len(), |ui, range| {
                        for row in &rows[range] {
                            let text = format!("{:04}   {:<12}  {}", row.0 + 1, row.1, row.2);
                            if ui
                                .add_sized(
                                    [ui.available_width(), 27.0],
                                    egui::Button::new(text)
                                        .selected(self.channel_index == Some(row.0)),
                                )
                                .clicked()
                            {
                                selected = Some(row.0);
                            }
                        }
                    });
            });
        if let Some(index) = selected {
            if self.channel_index != Some(index) && self.navigation_allowed() {
                self.channel_index = Some(index);
                self.refresh_forms();
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("channel_form_scroll").show(ui, |ui| {
                let mut apply = false; let mut reset = false;
                if let Some(draft) = &mut self.channel_draft {
                    ui.heading(format!("Канал {}", draft.index + 1));
                    ui.label("Измените нужные поля и нажмите «Применить»."); ui.add_space(8.0);
                    if draft.name_errors { ui.colored_label(Color32::YELLOW, "В исходном имени есть неизвестные GBK-байты. Без редактирования имя сохранится точно."); }
                    egui::Grid::new("channel_fields").num_columns(2).spacing([20.0, 12.0]).show(ui, |ui| {
                        text_row(ui, "Имя", &mut draft.current.name);
                        text_row(ui, "Приём, МГц", &mut draft.current.rx);
                        text_row(ui, "Передача, МГц", &mut draft.current.tx);
                        ui.label("Режим RX/TX"); choice(ui, "channel_mode", &mut draft.current.mode, &[(0, "Приём и передача"), (1, "Только приём"), (2, "Только передача")]); ui.end_row();
                        ui.label("Полоса"); choice(ui, "channel_band", &mut draft.current.bandwidth, &[(0, "Широкая"), (1, "Узкая")]); ui.end_row();
                        ui.label("Мощность"); choice(ui, "channel_power", &mut draft.current.power, &[(0, "Низкая"), (1, "Средняя"), (2, "Высокая")]); ui.end_row();
                        ui.label("Сканирование"); ui.checkbox(&mut draft.current.scan, "Включать канал"); ui.end_row();
                        text_row(ui, "Тон приёма", &mut draft.current.rx_tone);
                        text_row(ui, "Тон передачи", &mut draft.current.tx_tone);
                    });
                    ui.add_space(8.0);
                    ui.small("Тоны: Нет · CTCSS 67.0 · DCS D023N или D023I");
                    ui.small("Имя: до 16 байт GBK. Частоты: до пяти знаков после запятой.");
                    ui.small("Неизвестные значения сохраняются, пока вы не измените соответствующее поле."); ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        apply = ui.add_enabled(draft.changed(), egui::Button::new("Применить канал")).clicked();
                        reset = ui.add_enabled(draft.changed(), egui::Button::new("Сбросить форму")).clicked();
                        if draft.changed() { ui.label("Форма ещё не применена"); }
                    });
                } else { ui.heading("Выберите заполненный канал"); ui.label("Создание новых каналов появится в следующей версии."); }
                if reset { self.refresh_forms(); self.message("Форма сброшена; файл не изменён.", false); }
                if apply { self.apply_channel(); }
            });
        });
    }

    fn zones_ui(&mut self, ui: &mut egui::Ui) {
        let rows = self
            .document
            .as_ref()
            .and_then(|document| Zones::parse(document.view().zones()).ok())
            .map(|zones| {
                (0..ZONE_COUNT)
                    .filter_map(|index| {
                        zones.zone(index).map(|zone| {
                            let name = decode_name(zone.name_raw()).0;
                            (
                                index,
                                if name.is_empty() {
                                    "(без имени)".into()
                                } else {
                                    name
                                },
                                zone_display_members(zone).map_or_else(
                                    |_| "?".into(),
                                    |members| members.len().to_string(),
                                ),
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut selected = None;
        egui::Panel::left("zone_list")
            .default_size(340.0)
            .size_range(280.0..=500.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.heading("Зоны");
                ui.small("256 зон · до 200 каналов в каждой");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("zones_scroll")
                    .show_rows(ui, 30.0, rows.len(), |ui, range| {
                        for (index, name, count) in &rows[range] {
                            if ui
                                .add_sized(
                                    [ui.available_width(), 27.0],
                                    egui::Button::new(format!(
                                        "{:03}   {}   ({count})",
                                        index + 1,
                                        name
                                    ))
                                    .selected(self.zone_index == *index),
                                )
                                .clicked()
                            {
                                selected = Some(*index);
                            }
                        }
                    });
            });
        if let Some(index) = selected {
            if self.zone_index != index && self.navigation_allowed() {
                self.zone_index = index;
                self.refresh_forms();
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("zone_form_scroll").show(ui, |ui| {
                let mut apply = false; let mut reset = false;
                if let Some(draft) = &mut self.zone_draft {
                    ui.heading(format!("Зона {}", draft.index + 1));
                    if draft.name_errors { ui.colored_label(Color32::YELLOW, "В имени есть неизвестные GBK-байты. Без редактирования они сохранятся."); }
                    ui.label("Имя зоны"); ui.add(egui::TextEdit::singleline(&mut draft.current.name).desired_width(350.0)); ui.add_space(10.0);
                    ui.label("Номера каналов в нужном порядке");
                    if let Some(error) = draft.members_error {
                        ui.colored_label(Color32::YELLOW, zone_members_error(error));
                        ui.small("Редактор состава заблокирован. Исходные байты сохранены; имя можно изменить отдельно.");
                    }
                    ui.add_enabled(draft.members_error.is_none(), egui::TextEdit::multiline(&mut draft.current.members).desired_rows(10).desired_width(f32::INFINITY));
                    ui.small("Номера 1–1024 через запятую или пробел. Не более 200 записей.");
                    ui.small("Правка имени сохраняет состав зоны. Правка списка заменяет его целиком."); ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        apply = ui.add_enabled(draft.changed(), egui::Button::new("Применить зону")).clicked();
                        reset = ui.add_enabled(draft.changed(), egui::Button::new("Сбросить форму")).clicked();
                        if draft.changed() { ui.label("Форма ещё не применена"); }
                    });
                }
                if reset { self.refresh_forms(); self.message("Форма сброшена; файл не изменён.", false); }
                if apply { self.apply_zone(); }
            });
        });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Общие настройки");
            ui.label("24 настройки по логике штатного CPS: звук, подсветка, блокировка, сканирование и энергосбережение.");
            ui.add_space(16.0);
            let mut apply = false;
            let mut reset = false;
            if let Some(draft) = &mut self.settings_draft {
                egui::ScrollArea::vertical().id_salt("settings_scroll").max_height((ui.available_height()-85.0).max(120.0)).show(ui, |ui| {
                egui::Grid::new("settings_fields")
                    .num_columns(2)
                    .spacing([28.0, 16.0])
                    .show(ui, |ui| {
                        for index in 0..FIELDS.len() {
                            let field = FIELDS[index];
                            ui.label(format!("{} / {}", field.group_ru(), field.label_ru()));
                            egui::ComboBox::from_id_salt(("setting", index))
                                .width(230.0)
                                .selected_text(setting_label(field, draft.current[index]))
                                .show_ui(ui, |ui| {
                                    for code in 0..=field.max_code() {
                                        ui.selectable_value(
                                            &mut draft.current[index],
                                            code,
                                            setting_label(field, code),
                                        );
                                    }
                                });
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    apply = ui
                        .add_enabled(draft.changed(), egui::Button::new("Применить настройки"))
                        .clicked();
                    reset = ui
                        .add_enabled(draft.changed(), egui::Button::new("Сбросить форму"))
                        .clicked();
                    if draft.changed() {
                        ui.label("Форма ещё не применена");
                    }
                });
            }
            if reset {
                self.refresh_forms();
                self.message("Форма сброшена; файл не изменён.", false);
            }
            if apply {
                self.apply_settings();
            }
        });
    }

    fn changes_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(document) = &self.document else {
            return;
        };
        let changes = document.changes();
        let dirty_bytes = document.dirty_bytes();
        let mut reset = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Изменения относительно исходного файла");
            ui.horizontal(|ui| {
                ui.label(format!("{dirty_bytes} байт · {} секций", changes.len()));
                reset = ui
                    .add_enabled(dirty_bytes != 0, egui::Button::new("Вернуть исходный файл"))
                    .clicked();
            });
            ui.small("Адреса ниже — позиции байтов в сохранённом файле CPS.");
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("changes_scroll")
                .show(ui, |ui| {
                    if changes.is_empty() {
                        ui.label("Применённых изменений нет.");
                    }
                    for section in &changes {
                        egui::CollapsingHeader::new(format!(
                            "{} · {} байт · {} участков",
                            section_name(section.section),
                            section.changed_bytes,
                            section.ranges.len()
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            for range in &section.ranges {
                                for (chunk, (before, after)) in range
                                    .before
                                    .chunks(16)
                                    .zip(range.after.chunks(16))
                                    .enumerate()
                                {
                                    ui.monospace(format!(
                                        "{:06X}  {}  →  {}",
                                        range.file_offset + chunk * 16,
                                        hex(before),
                                        hex(after)
                                    ));
                                }
                            }
                        });
                    }
                });
        });
        if reset {
            self.request_action(Action::Reset, &ctx);
        }
    }

    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_action else {
            return;
        };
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Modal::new(egui::Id::unique("unsaved_confirmation")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.heading(if matches!(action, Action::Reset) { "Вернуть исходный файл?" } else { "Есть несохранённые изменения" }); ui.add_space(8.0);
            if matches!(action, Action::Reset) {
                ui.label("Все применённые правки будут отменены. История отмены очистится. Сохранённые копии останутся на диске.");
            } else {
                if self.pending_form() { ui.label("В форме есть неприменённые правки. Они будут потеряны."); }
                if self.unsaved() { ui.label("Последние применённые изменения ещё не сохранены в отдельную копию."); }
                if matches!(action, Action::Close) && self.calibrations.unsaved() { ui.label("Есть несохранённая рабочая копия калибровок. Сохраните её на вкладке «Калибровки» перед закрытием."); }
                ui.label(if matches!(action, Action::Close) { "Закрыть приложение без сохранения?" } else { "Продолжить открытие другого файла без сохранения текущих правок?" });
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                cancelled = ui.button("Вернуться").clicked();
                confirmed = ui.button(if matches!(action, Action::Reset) { "Вернуть исходный" } else { "Продолжить без сохранения" }).clicked();
            });
        });
        if cancelled {
            self.pending_action = None;
        }
        if confirmed {
            self.pending_action = None;
            self.perform_action(action, ctx);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.radio.poll(&ctx);
        if !self.close_allowed && ctx.input(|input| input.viewport().close_requested()) {
            if self.radio.busy() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.radio.cancel();
                self.pending_action = None;
                self.message("Чтение отменяется. Дождитесь завершения и закрытия COM-порта, затем закройте окно снова.", true);
            } else if self.pending_form() || self.unsaved() || self.calibrations.unsaved() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_action = Some(Action::Close);
            }
        }
        // The only interactive work area while the reader runs is its own pane.
        if self.radio.busy() {
            self.tab = Tab::Radio;
        }
        self.toolbar(ui);
        if self.tab == Tab::Radio {
            self.radio_ui(ui);
        } else if self.tab == Tab::Calibration {
            let live = self.radio.calibration_snapshot();
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("calibration_page")
                    .show(ui, |ui| {
                        self.calibrations.ui(ui, live);
                    });
            });
        } else if self.document.is_none() {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.add_space(70.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Ваши каналы — в отдельной рабочей копии");
                    ui.add_space(12.0);
                    ui.label(
                        "Откройте .880dmf или .98pdmf, отредактируйте каналы, зоны и настройки.",
                    );
                    ui.label("Исходный файл сохраняется. Результат записывается под новым именем.");
                    ui.add_space(24.0);
                    if ui.button("Открыть файл CPS…").clicked() {
                        self.request_action(Action::Open, &ctx);
                    }
                });
            });
        } else {
            match self.tab {
                Tab::Channels => self.channels_ui(ui),
                Tab::Zones => self.zones_ui(ui),
                Tab::Settings => self.settings_ui(ui),
                Tab::Changes => self.changes_ui(ui),
                Tab::Radio => unreachable!("radio pane is handled above"),
                Tab::Calibration => unreachable!("calibration pane is handled above"),
            }
        }
        self.confirmation(&ctx);
    }
}

fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(270.0));
    ui.end_row();
}
fn choice(ui: &mut egui::Ui, id: &str, raw: &mut u8, choices: &[(u8, &str)]) {
    let selected = choices
        .iter()
        .find(|(code, _)| code == raw)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| format!("Неизвестно (0x{raw:02X})"));
    egui::ComboBox::from_id_salt(id)
        .width(270.0)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (code, label) in choices {
                ui.selectable_value(raw, *code, *label);
            }
        });
}
fn zone_members_error(error: ZoneDisplayError) -> String {
    match error {
        ZoneDisplayError::HeaderMismatch { first, second } => {
            format!("Счётчики зоны не совпадают: {first} и {second}.")
        }
        ZoneDisplayError::HeaderCount { declared } => {
            format!("Недопустимый счётчик зоны: {declared}; максимум 200.")
        }
        ZoneDisplayError::MemberCode { slot, code } => {
            format!("Неизвестная ссылка в позиции {}: 0x{code:04X}.", slot + 1)
        }
        ZoneDisplayError::CountMismatch { declared, found } => {
            format!("Заголовок указывает {declared} каналов, сохранено ссылок: {found}.")
        }
    }
}

fn parse_members(text: &str) -> Result<Vec<u16>, String> {
    let mut members = Vec::new();
    for item in text
        .split(|character: char| character.is_whitespace() || character == ',' || character == ';')
        .filter(|item| !item.is_empty())
    {
        let number: u16 = item
            .parse()
            .map_err(|_| format!("Неверный номер канала: {item}"))?;
        if !(1..=1024).contains(&number) {
            return Err(format!("Номер канала {number} вне диапазона 1–1024"));
        }
        if members.len() == MEMBER_COUNT {
            return Err("В зоне может быть не более 200 записей".into());
        }
        members.push(number - 1);
    }
    Ok(members)
}
fn setting_label(field: Field, code: u8) -> String {
    if code > field.max_code() {
        return format!("Неизвестно (0x{code:02X})");
    }
    field
        .choice_label(code)
        .map(str::to_owned)
        .unwrap_or_else(|| code.to_string())
}

fn section_name(section: Section) -> &'static str {
    match section {
        Section::Cfg => "Настройки (CFG)",
        Section::Channels => "Каналы",
        Section::Fm => "FM",
        Section::Time => "Time",
        Section::Zones => "Зоны",
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_application_requires_a_clean_document_and_no_draft() {
        let mut app = App::default();
        assert!(!app.can_apply_snapshot());
        let document = Document::from_bytes(&vec![0xff; FILE_LEN]).unwrap();
        app.last_export = document.export_bytes();
        app.document = Some(document);
        assert!(app.can_apply_snapshot());
        let mut raw = [0xff; channels::CHANNEL_LEN];
        raw[2] = 1;
        let mut draft = ChannelDraft::from_channel(0, Channel::parse(&raw).unwrap());
        draft.current.name = "Pending".into();
        app.channel_draft = Some(draft);
        assert!(!app.can_apply_snapshot());
        app.channel_draft = None;
        assert!(app.can_apply_snapshot());
        app.document
            .as_mut()
            .unwrap()
            .apply(|bytes| -> Result<(), ()> {
                bytes[99] = 0;
                Ok(())
            })
            .unwrap();
        app.last_export = app.document.as_ref().unwrap().export_bytes();
        assert!(!app.unsaved());
        assert!(!app.can_apply_snapshot());
    }
    #[test]
    fn unchanged_unknown_fields_do_not_enter_the_patch() {
        let mut raw = [0xff; channels::CHANNEL_LEN];
        raw[2] = 1;
        let mut draft = ChannelDraft::from_channel(0, Channel::parse(&raw).unwrap());
        assert_eq!(draft.patch().unwrap(), ChannelPatch::default());
        draft.current.name = "Test".into();
        let patch = draft.patch().unwrap();
        assert_eq!(patch.name_raw, Some(encode_name("Test").unwrap()));
        assert_eq!(patch.rx_tone, None);
        assert_eq!(patch.tx_tone, None);
        assert_eq!(patch.rx_frequency_code, None);
        assert_eq!(patch.tx_frequency_code, None);
        assert_eq!(patch.operation_mode, None);
    }
    #[test]
    fn edits_enforce_cps_frequency_boundaries_without_rounding() {
        for value in ["18", "659.99999", "840", "999.99999"] {
            assert!(edited_frequency(value).is_ok(), "{value}");
        }
        for value in ["17.99999", "660", "839.99999", "1000", "NaN"] {
            assert!(edited_frequency(value).is_err(), "{value}");
        }
    }
    #[test]
    fn zone_members_keep_order_duplicates_and_limits() {
        assert_eq!(parse_members("1, 1024; 2\n1").unwrap(), [0, 1023, 1, 0]);
        assert!(parse_members("").unwrap().is_empty());
        for bad in ["0", "1025", "0xFFFF", "-1"] {
            assert!(parse_members(bad).is_err());
        }
        assert_eq!(parse_members(&vec!["1"; 200].join(",")).unwrap().len(), 200);
        assert!(parse_members(&vec!["1"; 201].join(",")).is_err());
    }
    #[test]
    fn zone_form_uses_header_count_and_name_edits_do_not_repair_raw_members() {
        for counts in [[0, 0], [1, 2]] {
            let mut bytes = vec![0xa5; FILE_LEN];
            bytes[ZONES_OFFSET..ZONES_OFFSET + 512].fill(0);
            bytes[ZONES_OFFSET..ZONES_OFFSET + 2].copy_from_slice(&counts);
            let mut app = App {
                document: Some(Document::from_bytes(&bytes).unwrap()),
                ..Default::default()
            };
            app.refresh_forms();
            let draft = app.zone_draft.as_ref().unwrap();
            assert!(draft.current.members.is_empty());
            assert_eq!(draft.members_error.is_some(), counts[0] != counts[1]);
            assert_eq!(app.document.as_ref().unwrap().export_bytes(), bytes);
            assert!(!app.document.as_ref().unwrap().is_dirty());
            app.zone_draft.as_mut().unwrap().current.name = "Local".into();
            app.apply_zone();
            assert!(!app.notice_error);
            let mut expected = bytes.clone();
            expected[ZONES_OFFSET + 4..ZONES_OFFSET + 20]
                .copy_from_slice(&encode_name("Local").unwrap());
            let document = app.document.as_ref().unwrap();
            assert_eq!(document.bytes(), expected);
            assert_eq!(document.original_bytes(), bytes);
        }
    }
    #[test]
    fn invalid_zone_members_cannot_be_overwritten_by_bypassing_the_disabled_form() {
        let mut bytes = vec![0xa5; FILE_LEN];
        bytes[ZONES_OFFSET..ZONES_OFFSET + 512].fill(0);
        bytes[ZONES_OFFSET] = 1;
        let mut app = App {
            document: Some(Document::from_bytes(&bytes).unwrap()),
            ..Default::default()
        };
        app.refresh_forms();
        app.zone_draft.as_mut().unwrap().current.members = "1, 2".into();
        app.apply_zone();
        assert!(app.notice_error);
        assert_eq!(app.document.as_ref().unwrap().bytes(), bytes);
        assert_eq!(app.document.as_ref().unwrap().undo_depth(), 0);
    }
    #[test]
    fn unsaved_state_tracks_export_and_pending_forms_separately() {
        let document = Document::from_bytes(&vec![0xff; FILE_LEN]).unwrap();
        let mut app = App {
            last_export: document.export_bytes(),
            document: Some(document),
            ..Default::default()
        };
        assert!(!app.unsaved());
        app.document
            .as_mut()
            .unwrap()
            .apply(|bytes| -> Result<(), ()> {
                bytes[99] = 0;
                Ok(())
            })
            .unwrap();
        assert!(app.unsaved());
        app.last_export = app.document.as_ref().unwrap().export_bytes();
        assert!(!app.unsaved());
        assert!(app.document.as_ref().unwrap().is_dirty());
        app.document.as_mut().unwrap().undo();
        assert!(app.unsaved());
    }
}
