//! Menu and setting pages, their shared list geometry, and cached labels.
use super::*;
use crate::controller::{
    binding_digit, vfo_tx_frequency_for, LightMode, MenuEntry, MenuGroup, MenuItem, ShortcutAction,
    SideAction, LIGHT_MODES,
};

const SETTING_Y: u16 = 112;
const SETTING_ROW_HEIGHT: u16 = 44;
const SETTING_PAINT_HEIGHT: u16 = 36;
pub(super) const SHIFT_TX_X: usize = 152;

pub(super) fn menu_start(index: u8) -> usize {
    usize::from(index) / LIST_ROWS * LIST_ROWS
}

/// The list only paints one item per LCD row.
fn list_row(row: u16) -> Option<(usize, u16)> {
    if !(LIST_Y..LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT).contains(&row) {
        return None;
    }
    let offset = (row - LIST_Y) / LIST_ROW_HEIGHT;
    Some((usize::from(offset), LIST_Y + offset * LIST_ROW_HEIGHT))
}

/// Physical rows of a filled menu item within its seven-item page.
pub fn menu_item_rows(index: u8) -> (u16, u16) {
    let start = LIST_Y + (usize::from(index) % LIST_ROWS) as u16 * LIST_ROW_HEIGHT;
    (start, start + ITEM_PAINT_HEIGHT)
}

pub(super) fn setting_title(setting: Setting) -> &'static str {
    match setting {
        Setting::BandPreset => "\x1f069",
        Setting::GpsEnable => "\x1f0E1",
        Setting::GpsBaud => "\x1f11C",
        Setting::Dtmf => "\x1f00F",
        Setting::NameRoger => "\x1f16E",
        Setting::MicrophoneGain => "\x1f174",
        Setting::TxTimeout => "\x1f182",
        Setting::TopKey => "\x1f091",
        Setting::BottomKey => "\x1f092",
        Setting::Shift => "\x1f101",
        Setting::TxTone => "\x1f133",
        Setting::Mode => "\x1f105",
        Setting::Power => "\x1f0A0",
        Setting::Monitor => "\x1f09F",
        Setting::Modulation => "\x1f104",
        Setting::Reverse => "\x1f102",
        Setting::ActiveSlot => "\x1f03F",
        Setting::Scan => "\x1f118",
        Setting::ScanPriority => "\x1f176",
        Setting::DisplayLight => "\x1f0E6",
        Setting::KeyLight => "\x1f0E5",
    }
}

pub(super) fn setting_options(setting: Setting) -> &'static [&'static str] {
    match setting {
        Setting::MicrophoneGain => &[
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
            "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29",
            "30", "31",
        ],
        Setting::BandPreset => &[
            "VHF / 145.500",
            "UHF / 433.500",
            "CB / 27.135 AM",
            "800 / 850.000 FM",
        ],
        Setting::TxTimeout => &["\x1f14E", "\x1f154", "\x1f156"],
        Setting::GpsEnable | Setting::Dtmf | Setting::NameRoger => &["\x1f05E", "\x1f04F"],
        Setting::GpsBaud => &[
            "4800", "9600", "14400", "19200", "38400", "56000", "57600", "115200", "128000",
            "256000",
        ],
        Setting::TopKey | Setting::BottomKey => &["\x1f05D", "\x1f09F", "\x1f118", "\x1f123"],
        Setting::TxTone => &[],
        Setting::Shift => &[
            "\x1f005", "\x1f14B", "\x1f148", "\x1f14C", "\x1f149", "\x1f14D", "\x1f14A",
        ],
        Setting::Mode => &["\x1f08A", "\x1f138"],
        Setting::Power => &["\x1f0BD", "\x1f12A", "\x1f060"],
        Setting::Monitor | Setting::Reverse | Setting::ScanPriority => &["\x1f05E", "\x1f04F"],
        Setting::Modulation => &["FM", "AM"],
        Setting::ActiveSlot => &["\x1f11D", "\x1f11E"],
        Setting::Scan => &["\x1f0D0", "\x1f07C"],
        Setting::DisplayLight | Setting::KeyLight => &[
            "\x1f05F", "\x1f050", "\x1f158", "\x1f14E", "\x1f154", "\x1f156", "\x1f159",
        ],
    }
}

pub(super) fn setting_geometry(setting: Setting) -> (u16, u16, u16) {
    if matches!(
        setting,
        Setting::DisplayLight
            | Setting::KeyLight
            | Setting::Shift
            | Setting::TxTone
            | Setting::GpsBaud
            | Setting::MicrophoneGain
    ) {
        (100, 26, 22)
    } else {
        (SETTING_Y, SETTING_ROW_HEIGHT, SETTING_PAINT_HEIGHT)
    }
}

fn light_index(mode: LightMode) -> usize {
    LIGHT_MODES
        .iter()
        .position(|&candidate| candidate == mode)
        .unwrap()
}

pub(super) fn setting_uses_radio(setting: Option<Setting>) -> bool {
    matches!(
        setting,
        Some(Setting::Power | Setting::Monitor | Setting::Modulation)
    )
}

/// This is the committed value, never the highlighted draft. For RF settings
/// the backend must report an active state before the UI calls it confirmed.
pub(super) fn current_setting_label(model: &Model<'_>, setting: Setting) -> &'static str {
    const UNKNOWN: &str = "\x1f0B6";
    if setting_uses_radio(Some(setting))
        && !matches!(
            model.radio.state,
            RadioState::Receiving | RadioState::Transmitting
        )
    {
        return UNKNOWN;
    }
    let index = match setting {
        Setting::BandPreset => return "\x1f05C",
        Setting::GpsEnable => usize::from(model.view.gps_enabled),
        Setting::GpsBaud => usize::from(model.view.gps_baud),
        Setting::Dtmf => usize::from(model.view.dtmf_enabled),
        Setting::NameRoger => usize::from(model.view.name_roger_enabled),
        Setting::MicrophoneGain => usize::from(model.view.microphone_gain),
        Setting::TxTimeout => model.view.tx_timeout as usize,
        Setting::TopKey => usize::from(model.view.top_action),
        Setting::BottomKey => usize::from(model.view.bottom_action),
        Setting::Shift => usize::from(model.view.shift_index),
        Setting::TxTone => return UNKNOWN,
        Setting::Mode => usize::from(model.view.mode == Mode::Vfo),
        Setting::Power => match model.radio.power {
            Some(power) => usize::from(power),
            None => return UNKNOWN,
        },
        Setting::Monitor => usize::from(model.radio.monitor),
        Setting::Modulation => usize::from(model.radio.modulation == Modulation::Am),
        Setting::Reverse => usize::from(model.view.reverse),
        Setting::ActiveSlot => usize::from(model.view.active_slot == Slot::B),
        Setting::DisplayLight => light_index(model.view.display_light),
        Setting::KeyLight => light_index(model.view.key_light),
        Setting::ScanPriority => usize::from(model.view.scan_priority_enabled),
        Setting::Scan => {
            return match model.view.scan {
                ScanState::Stopped => "\x1f0D2",
                ScanState::Requested => "\x1f07B",
                ScanState::Running | ScanState::Paused => "\x1f11B",
                ScanState::Stopping => "\x1f0D1",
            }
        }
    };
    setting_options(setting)
        .get(index)
        .copied()
        .unwrap_or(UNKNOWN)
}

fn ctcss_label(index: usize) -> Text {
    let mut text = Text::new();
    match TX_CTCSS.get(index) {
        Some(0) => {
            let _ = text.write_str("\x1f05E");
        }
        Some(tone) => {
            let _ = write!(text, "{}.{} \x1f065", tone / 10, tone % 10);
        }
        None => {
            let _ = text.write_str("?");
        }
    }
    text
}

pub(super) fn shift_tx_preview(view: &View) -> Option<u32> {
    if view.mode != Mode::Vfo || view.cb_for_slot(view.active_slot).is_some() {
        return None;
    }
    let base = view.vfo_10hz;
    let shifted = vfo_tx_frequency_for(base, view.setting_index, None)?;
    for frequency in [base, shifted] {
        uv98_band_plan::operating_range(frequency)?;
        if uv98_band_plan::aviation_band(frequency) {
            return None;
        }
    }
    Some(if view.reverse { base } else { shifted })
}

pub(super) fn setting(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let Some(setting) = model.view.setting else {
        return;
    };
    if matches!(setting, Setting::TopKey | Setting::BottomKey) {
        let label = model
            .view
            .top_draft
            .map(key_label)
            .unwrap_or(if setting == Setting::TopKey {
                "\x1f0A5"
            } else {
                "\x1f0A6"
            });
        text(row, out, 16, 62, 1, label, color::ACCENT);
    } else {
        text(row, out, 16, 62, 1, "\x1f059", color::ACCENT);
    }
    if (84..91).contains(&row) {
        let mut current = Text::new();
        let tone;
        let label = if setting == Setting::TxTone {
            tone = ctcss_label(usize::from(model.view.tx_tone_index));
            tone.as_str()
        } else {
            current_setting_label(model, setting)
        };
        let _ = write!(current, "\x1f110 {}", label);
        text(row, out, 16, 84, 1, current.as_str(), color::MUTED);
        if setting == Setting::Shift {
            let tx = if model.radio.vfo_am_override {
                None
            } else {
                shift_tx_preview(&model.view)
            };
            let ink = if tx.is_some() {
                color::TEXT
            } else {
                color::MUTED
            };
            raw_text(row, out, SHIFT_TX_X, 84, 1, "TX", ink);
            raw_text(
                row,
                out,
                SHIFT_TX_X + 18,
                84,
                1,
                frequency(tx.unwrap_or(0)).as_str(),
                ink,
            );
        }
    }
    let (start, pitch, height) = setting_geometry(setting);
    let first = if setting.choice_count() > LIST_ROWS as u8 {
        menu_start(model.view.setting_index)
    } else {
        0
    };
    if let Some(relative) = row.checked_sub(start) {
        let offset = usize::from(relative / pitch);
        let index = first + offset;
        if offset < LIST_ROWS
            && index < usize::from(setting.choice_count())
            && relative % pitch < height
        {
            let y = start + offset as u16 * pitch;
            let selected = index == usize::from(model.view.setting_index);
            let tone;
            let label = if setting == Setting::TxTone {
                tone = ctcss_label(index);
                tone.as_str()
            } else {
                setting_options(setting)[index]
            };
            rect(
                row,
                out,
                12,
                y,
                216,
                height,
                if selected {
                    color::ACCENT
                } else {
                    color::PANEL
                },
            );
            text(
                row,
                out,
                24,
                y + (height - 14) / 2,
                2,
                label,
                item_color(selected),
            );
        }
    }
    if setting == Setting::NameRoger {
        text(row, out, 16, 261, 1, MDC_NAME_HINT, color::MUTED);
    }
    if setting.choice_count() > 3 {
        return;
    }
    text(row, out, 16, 252, 1, "\x1f0F3", color::MUTED);
}

pub fn menu_label(item: MenuItem) -> &'static str {
    match item {
        MenuItem::Cb => cb_label(),
        MenuItem::Mdc => "MDC1200",
        MenuItem::RadioName => "\x1f16D",
        MenuItem::NameRoger => "\x1f16E",
        MenuItem::MicrophoneGain => "\x1f174",
        MenuItem::TxTimeout => "\x1f182",
        MenuItem::BandPreset => "\x1f069",
        MenuItem::Gps => "\x1f01B",
        MenuItem::GpsEnable => "\x1f0E1",
        MenuItem::GpsBaud => "\x1f11C",
        MenuItem::Aprs => "\x1f00B",
        MenuItem::AprsRx => "\x1f183",
        MenuItem::Dtmf => "\x1f00E",
        MenuItem::Mode => "\x1f087",
        MenuItem::Channels => "\x1f126",
        MenuItem::ChannelDetails => "\x1f066",
        MenuItem::TopKey => "\x1f094",
        MenuItem::BottomKey => "\x1f0BC",
        MenuItem::Shift => "\x1f100",
        MenuItem::TxTone => "\x1f132",
        MenuItem::Step => "\x1f13F",
        MenuItem::About => "\x1f0C3",
        MenuItem::Reverse => "\x1f102",
        MenuItem::Scan => "\x1f118",
        MenuItem::ScanRange => super::scan_pages::RANGE_TITLE,
        MenuItem::ScanJournal => super::scan_pages::JOURNAL_TITLE,
        MenuItem::ScanPriority => "\x1f176",
        MenuItem::Shortcuts => "\x1f06D",
        MenuItem::Power => "\x1f0A1",
        MenuItem::Monitor => "\x1f09F",
        MenuItem::Modulation => "\x1f104",
        MenuItem::Zones => "\x1f080",
        MenuItem::ActiveSlot => "\x1f05B",
        MenuItem::SideKeys => "\x1f047",
        MenuItem::DisplayLight => "\x1f10D",
        MenuItem::KeyLight => "\x1f10C",
        MenuItem::Spectrum => "\x1f125",
        MenuItem::Shortwave => "\x1f08D",
        MenuItem::RxToneSearch => "\x1f17B",
        MenuItem::DtmfMemory => "\x1f17E",
    }
}

pub fn menu_group_label(group: MenuGroup) -> &'static str {
    match group {
        MenuGroup::Mode => "\x1f08B",
        MenuGroup::Receive => "\x1f0F1",
        MenuGroup::Scanning => "\x1f0EA",
        MenuGroup::Keys => "\x1f095",
        MenuGroup::Navigation => "\x1f162",
        MenuGroup::System => "\x1f117",
    }
}

pub(super) const MENU_LABEL_X: usize = 44;
const MENU_GLYPHS: usize = (WIDTH - MENU_LABEL_X).div_ceil(6);

/// The label depends only on an item's semantic ID and current language.
/// Store glyph rows once, independently of the selection and LCD window.
pub struct MenuCache {
    pub(super) key: Option<(MenuEntry, Language)>,
    glyphs: [[u8; 7]; MENU_GLYPHS],
    len: usize,
    scale: u16,
    #[cfg(test)]
    pub(super) resolutions: usize,
}

impl MenuCache {
    pub const fn new() -> Self {
        Self {
            key: None,
            glyphs: [[0; 7]; MENU_GLYPHS],
            len: 0,
            scale: 1,
            #[cfg(test)]
            resolutions: 0,
        }
    }

    pub(super) fn paint(
        &mut self,
        entry: MenuEntry,
        row: u16,
        out: &mut Row<'_>,
        y: u16,
        ink: u16,
    ) {
        let key = (entry, out.language);
        if self.key != Some(key) {
            let template = match entry {
                MenuEntry::Item(item) => menu_label(item),
                MenuEntry::Group(group) => menu_group_label(group),
            };
            let message = locale::resolve(template, out.language);
            let text = message.as_str();
            self.scale = fitting_scale(text, 2, WIDTH - MENU_LABEL_X);
            self.len = 0;
            let limit = (WIDTH - MENU_LABEL_X).div_ceil(6 * usize::from(self.scale));
            for (glyph, ch) in self.glyphs[..limit].iter_mut().zip(text.chars()) {
                *glyph = glyph_rows(ch);
                self.len += 1;
            }
            self.key = Some(key);
            #[cfg(test)]
            {
                self.resolutions += 1;
            }
        }
        if row < y || row >= y + 7 * self.scale {
            return;
        }
        let glyph_row = usize::from((row - y) / self.scale);
        let scale = usize::from(self.scale);
        for (index, glyph) in self.glyphs[..self.len].iter().enumerate() {
            draw_glyph_row(
                out,
                MENU_LABEL_X + index * 6 * scale,
                glyph[glyph_row],
                scale,
                ink,
            );
        }
    }
}

pub(super) fn menu(row: u16, out: &mut Row<'_>, model: &Model<'_>, cache: &mut MenuCache) {
    let entries = menu_entries(model.view.menu_group);
    if let Some((offset, y)) = list_row(row) {
        let index = menu_start(model.view.menu_index) + offset;
        let Some(&entry) = entries.get(index) else {
            return;
        };
        let selected = index == usize::from(model.view.menu_index);
        item_background(row, out, y, selected);
        if !(y + 7..y + 23).contains(&row) {
            return;
        }
        let item = match entry {
            MenuEntry::Item(item) => Some(item),
            MenuEntry::Group(_) => None,
        };
        let glyph = match item {
            Some(MenuItem::Mode | MenuItem::Modulation) => Symbol::Wave,
            Some(MenuItem::Cb | MenuItem::Channels | MenuItem::ChannelDetails) => Symbol::Channels,
            Some(MenuItem::Zones) => Symbol::Bank,
            Some(MenuItem::Scan | MenuItem::ScanRange | MenuItem::ScanJournal | MenuItem::Spectrum) => {
                Symbol::Scan
            }
            Some(MenuItem::Monitor) => Symbol::Monitor,
            Some(MenuItem::Power) => Symbol::Power,
            Some(MenuItem::Reverse) => Symbol::Reverse,
            Some(MenuItem::ActiveSlot) => {
                if model.view.active_slot == Slot::A {
                    Symbol::SlotA
                } else {
                    Symbol::SlotB
                }
            }
            None => match entry {
                MenuEntry::Group(MenuGroup::Mode) => Symbol::Channels,
                MenuEntry::Group(MenuGroup::Receive) => Symbol::Monitor,
                MenuEntry::Group(MenuGroup::Scanning) => Symbol::Scan,
                _ => Symbol::Settings,
            },
            _ => Symbol::Settings,
        };
        let ink = item_color(selected);
        let icon_ink = if selected { ink } else { color::ACCENT };
        symbol(row, out, 20, y + 7, glyph, icon_ink);
        if row >= y + 8 && row < y + 22 {
            cache.paint(entry, row, out, y + 8, ink);
        }
        if matches!(entry, MenuEntry::Group(_)) {
            text(row, out, 216, y + 8, 2, ">", ink);
        }
    } else if (283..290).contains(&row) {
        let mut position = Text::new();
        let _ = write!(
            position,
            "{} / {}",
            model.view.menu_index + 1,
            entries.len()
        );
        text(row, out, 192, 283, 1, position.as_str(), color::MUTED);
    }
}

pub fn shortcut_label(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::None => "\x1f0B0",
        ShortcutAction::Power => "\x1f0A1",
        ShortcutAction::Monitor => "\x1f09F",
        ShortcutAction::Scan => "\x1f118",
        ShortcutAction::Reverse => "\x1f102",
        ShortcutAction::Mode => "\x1f086",
        ShortcutAction::Step => "\x1f13F",
        ShortcutAction::Channels => "\x1f126",
        ShortcutAction::Keys => "\x1f0F6",
        ShortcutAction::Lock => "\x1f042",
        ShortcutAction::Modulation => "\x1f104",
        ShortcutAction::Zones => "\x1f080",
        ShortcutAction::SwitchSlot => "\x1f05B",
        ShortcutAction::Spectrum => "\x1f125",
        ShortcutAction::Crossband => "\x1f097",
        ShortcutAction::DualWatch => "DUAL WATCH A/B",
        ShortcutAction::Cb => cb_label(),
        #[cfg(feature = "voice-keyer")]
        ShortcutAction::VoiceCall => "\x1f18F",
    }
}

pub(super) fn shortcuts(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let index = model.view.binding_index;
    let start = menu_start(index);
    if let Some((offset, y)) = list_row(row) {
        let item = start + offset;
        if let Some(action) = model.view.shortcut_at(item) {
            let selected = item == usize::from(index);
            item_background(row, out, y, selected);
            let mut number = Text::new();
            let _ = write!(number, "{}", binding_digit(item as u8).unwrap_or(0));
            text(
                row,
                out,
                18,
                y + 8,
                2,
                number.as_str(),
                item_color(selected),
            );
            text(
                row,
                out,
                48,
                y + 8,
                2,
                shortcut_label(action),
                item_color(selected),
            );
        }
    }
    if model.view.notice == Notice::None && (283..290).contains(&row) {
        text(row, out, 16, 283, 1, "\x1f06B", color::MUTED);
        let mut count = Text::new();
        let _ = write!(count, "{} / {}", index + 1, model.view.bindings.len());
        text(row, out, 182, 283, 1, count.as_str(), color::MUTED);
    }
}

pub(super) fn actions(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let shortcut = model.view.screen == Screen::ShortcutAction;
    let index = if shortcut {
        model.view.binding_action_index
    } else {
        model.view.side_action_index
    };
    if let Some((offset, y)) = list_row(row) {
        let (item, label) = if shortcut {
            let item = menu_start(index) + offset;
            (
                item,
                SHORTCUT_ACTIONS.get(item).copied().map(shortcut_label),
            )
        } else {
            (
                offset,
                SIDE_ACTIONS.get(offset).copied().map(side_action_label),
            )
        };
        if let Some(label) = label {
            let selected = item == usize::from(index);
            item_background(row, out, y, selected);
            text(row, out, 21, y + 8, 2, label, item_color(selected));
        }
    }
    if model.view.notice != Notice::None || !(283..290).contains(&row) {
        return;
    }
    if shortcut {
        let mut key = Text::new();
        let _ = write!(
            key,
            "\x1f06E {}\x1f008",
            binding_digit(model.view.binding_index).unwrap_or(0)
        );
        text(row, out, 16, 283, 1, key.as_str(), color::MUTED);
    } else {
        text(
            row,
            out,
            16,
            283,
            1,
            if model.view.side_index == 0 {
                "\x1f0A9"
            } else {
                "\x1f0AA"
            },
            color::MUTED,
        );
    }
}

pub(super) fn zones(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    if model.zones.is_empty() {
        centered(row, out, 128, 2, "\x1f0B8", color::TEXT);
        return;
    }
    if let Some((offset, y)) = list_row(row) {
        if let Some(zone) = model.zones.get(offset) {
            let selected = zone.index == model.view.zone_cursor;
            item_background(row, out, y, selected);
            let name = channel_name(zone.name);
            text(
                row,
                out,
                16,
                y + 4,
                2,
                if zone.index.is_none() {
                    "\x1f056"
                } else {
                    name.as_str()
                },
                item_color(selected),
            );
            let mut count = Text::new();
            let _ = write!(count, "\x1f089 {}", zone.channel_count);
            text(
                row,
                out,
                16,
                y + 22,
                1,
                count.as_str(),
                item_color(selected),
            );
        }
    }
    if model.view.notice == Notice::None {
        text(row, out, 16, 283, 1, "\x1f07D", color::MUTED);
    }
}

pub fn side_action_label(action: SideAction) -> &'static str {
    match action {
        SideAction::PttA => "PTT A",
        SideAction::PttB => "PTT B",
        SideAction::Active => "\x1f032",
        SideAction::Monitor => "\x1f09F",
        SideAction::Scan => "\x1f118",
        SideAction::Power => "\x1f0A1",
        SideAction::None => "\x1f0B0",
    }
}

pub(super) fn side_keys(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    if let Some((index, y)) = list_row(row) {
        if let Some(&action) = model.view.side_bindings.get(index) {
            let selected = usize::from(model.view.side_index) == index;
            item_background(row, out, y, selected);
            text(
                row,
                out,
                16,
                y + 4,
                2,
                if index == 0 { "PTT 1" } else { "PTT 2" },
                item_color(selected),
            );
            text(
                row,
                out,
                16,
                y + 22,
                1,
                side_action_label(action),
                item_color(selected),
            );
        }
    }
    for (y, label) in [
        (154, "\x1f031"),
        (178, "\x1f033"),
        (210, "\x1f06F"),
        (248, "\x1f081"),
    ] {
        text(row, out, 16, y, 1, label, color::MUTED);
    }
    if model.lower_side_mapped {
        text(row, out, 16, 124, 1, "\x1f0BA", color::GOOD);
    } else {
        text(row, out, 16, 124, 1, "\x1f0B9", color::WARNING);
    }
}

pub(super) fn channels(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    if model.list.is_empty() {
        centered(row, out, 128, 2, "\x1f127", color::TEXT);
        centered(row, out, 163, 1, "\x1f122", color::MUTED);
        return;
    }
    if let Some((offset, y)) = list_row(row) {
        if let Some(channel) = model.list.get(offset) {
            let selected = model.view.list_channel == Some(channel.index as u16);
            item_background(row, out, y, selected);
            if (y + 7..y + 14).contains(&row) {
                let mut number = Text::new();
                let _ = write!(number, "{:04}", channel.index + 1);
                text(
                    row,
                    out,
                    15,
                    y + 7,
                    1,
                    number.as_str(),
                    item_color(selected),
                );
            }
            if (y + 3..y + 17).contains(&row) {
                let name = channel_name(channel.name);
                let scale = if text_width(name.as_str(), 2) <= 184 {
                    2
                } else {
                    1
                };
                text(
                    row,
                    out,
                    44,
                    y + 3,
                    scale,
                    name.as_str(),
                    item_color(selected),
                );
            }
            if (y + 22..y + 29).contains(&row) {
                let value = frequency(channel.rx_10hz);
                text(
                    row,
                    out,
                    44,
                    y + 22,
                    1,
                    value.as_str(),
                    item_color(selected),
                );
                text(row, out, 108, y + 22, 1, "\x1f09A", item_color(selected));
            }
        }
    }
}
