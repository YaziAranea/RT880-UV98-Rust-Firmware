//! Compact pages and measured HF waterfall. No RF operations in rendering.
use super::*;
use crate::controller::{AprsStatus, ShortwaveMode};

const BINS: usize = 32;
const ROWS: usize = 16;

/// Owned by the app, borrowed by Model. Never copied with each View/input event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortwaveScope {
    pub first_hz: u32,
    pub step_hz: u32,
    samples: [u8; BINS],
    valid: u32,
    history: [[u8; BINS]; ROWS],
    newest: usize,
    rows: usize,
}

impl Default for ShortwaveScope {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortwaveScope {
    pub const fn new() -> Self {
        Self {
            first_hz: 0,
            step_hz: 0,
            samples: [0; BINS],
            valid: 0,
            history: [[0; BINS]; ROWS],
            newest: 0,
            rows: 0,
        }
    }

    /// Clear history whenever the physical sampling grid changes.
    pub fn reset(&mut self, first_hz: u32, step_hz: u32) {
        *self = Self::new();
        self.first_hz = first_hz;
        self.step_hz = step_hz;
    }

    /// Only measured bins are drawn. A complete sweep advances time by one row.
    pub fn sample(&mut self, bin: u8, rssi: u8, _snr: u8, complete: bool) {
        let bin = usize::from(bin);
        if bin >= BINS || self.step_hz == 0 {
            return;
        }
        if bin == 0 {
            self.valid = 0;
        }
        self.samples[bin] = rssi;
        self.valid |= 1 << bin;
        if complete && self.valid == u32::MAX {
            self.newest = (self.newest + 1) % ROWS;
            self.history[self.newest] = self.samples;
            self.rows = (self.rows + 1).min(ROWS);
        }
    }

    pub fn measured_bins(&self) -> u32 {
        self.valid
    }
    pub fn history_rows(&self) -> usize {
        self.rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waterfall_contains_only_complete_measured_sweeps_and_resets_grid() {
        let mut scope = ShortwaveScope::new();
        scope.reset(7_020_000, 5_000);
        scope.sample(31, 40, 8, true);
        assert_eq!(scope.history_rows(), 0);
        for bin in 0..32 {
            scope.sample(bin, bin + 10, 4, bin == 31);
        }
        assert_eq!(scope.history_rows(), 1);
        assert_eq!(scope.measured_bins(), u32::MAX);
        scope.sample(0, 30, 5, false);
        scope.sample(31, 40, 8, true);
        assert_eq!(scope.history_rows(), 1);
        scope.reset(14_100_000, 1_000);
        assert_eq!(scope.history_rows(), 0);
        assert_eq!(scope.measured_bins(), 0);
    }
}

pub(super) fn shortwave_processing_label(row: u16, out: &mut [u16; WIDTH], view: View, y: u16) {
    let mut label = Text::new();
    if view.shortwave_mode == ShortwaveMode::Am {
        let filter = ["6", "3", "1"][usize::from(view.shortwave_filter.min(2))];
        let _ = write!(
            label,
            "AM ФИЛЬТР {filter} КГЦ   SM {}",
            if view.shortwave_soft_mute {
                "ON"
            } else {
                "OFF"
            }
        );
    } else {
        let _ = label.write_str("ФИЛЬТР / SM: ДОСТУПНЫ В AM");
    }
    centered(row, out, y, 1, label.as_str(), color::ACCENT);
}

pub(super) fn shortwave_scope_page(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let view = model.view;
    let mut title = Text::new();
    let _ = write!(
        title,
        "{}  {}.{:03} КГЦ",
        view.shortwave_mode.label(),
        view.shortwave_khz,
        view.shortwave_fraction_hz
    );
    centered(row, out, 55, 2, title.as_str(), color::TEXT);
    let Some(scope) = model.shortwave_scope.filter(|scope| scope.step_hz > 0) else {
        centered(row, out, 136, 2, "НАЧАЛО ОБЗОРА", color::MUTED);
        centered(
            row,
            out,
            266,
            1,
            "EXIT: СЛУШАТЬ ВЫБРАННУЮ ЧАСТОТУ",
            color::ACCENT,
        );
        return;
    };
    let mut width = Text::new();
    let _ = write!(
        width,
        "ШАГ {} КГЦ  ПОЛОСА {} КГЦ",
        scope.step_hz / 1000,
        scope.step_hz * 31 / 1000
    );
    centered(row, out, 78, 1, width.as_str(), color::ACCENT);
    const X: usize = 12;
    const PITCH: usize = 7;
    const TOP: u16 = 96;
    const BOTTOM: u16 = 154;
    // Fixed RSSI scale keeps signals comparable across sweeps. A sliding floor
    // would make equal levels change colour as the receiver visits other bins.
    for index in 0..BINS {
        if scope.valid & (1 << index) == 0 {
            continue;
        }
        let height = u16::from(scope.samples[index].min(63)) * (BOTTOM - TOP) / 63;
        rect(
            row,
            out,
            X + index * PITCH,
            BOTTOM - height,
            6,
            height.max(1),
            color::ACCENT,
        );
    }
    for (index, x) in [(0, 12), (16, 98), (31, 194)] {
        let khz = (scope.first_hz + scope.step_hz * index) / 1000;
        let mut label = Text::new();
        let _ = write!(label, "{khz}");
        text(row, out, x, 158, 1, label.as_str(), color::MUTED);
    }
    let selected_hz = view.shortwave_khz * 1000 + u32::from(view.shortwave_fraction_hz);
    let cursor = selected_hz.saturating_sub(scope.first_hz) / scope.step_hz;
    if cursor < BINS as u32 {
        rect(
            row,
            out,
            X + cursor as usize * PITCH + 3,
            TOP,
            1,
            BOTTOM - TOP + 1,
            color::WARNING,
        );
    }
    for age in 0..scope.rows {
        let y = 172 + age as u16 * 4;
        if !(y..y + 4).contains(&row) {
            continue;
        }
        let samples = &scope.history[(scope.newest + ROWS - age) % ROWS];
        for (bin, &rssi) in samples.iter().enumerate() {
            let tint = WATERFALL_COLORS[1 + usize::from(rssi / 10).min(6)];
            rect(row, out, X + bin * PITCH, y, PITCH, 4, tint);
        }
    }
    shortwave_processing_label(row, out, view, 244);
    centered(row, out, 260, 1, "ОБЗОР: ЗВУК ПРЕРЫВАЕТСЯ", color::MUTED);
    centered(
        row,
        out,
        276,
        1,
        "↑↓ ЦЕНТР  ДОЛГО 0 / EXIT: ПРИЁМ",
        color::ACCENT,
    );
}

fn coordinate(value: i32, positive: &str, negative: &str) -> Text {
    let mut text = Text::new();
    let absolute = value.unsigned_abs();
    let _ = write!(
        text,
        "{} {}.{:05}",
        if value < 0 { negative } else { positive },
        absolute / 10_000_000,
        absolute % 10_000_000 / 100
    );
    text
}

pub(super) fn utility_page(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let view = model.view;
    match view.screen {
        Screen::Gps => {
            centered(
                row,
                out,
                62,
                2,
                if !view.gps_enabled {
                    "GPS ВЫКЛЮЧЕН"
                } else if view.gps_status.valid {
                    "ПОЗИЦИЯ НАЙДЕНА"
                } else {
                    "ПОИСК СПУТНИКОВ"
                },
                if view.gps_status.valid {
                    color::GOOD
                } else {
                    color::MUTED
                },
            );
            let mut line = Text::new();
            let _ = write!(line, "СПУТНИКИ {:02}", view.gps_status.satellites);
            centered(row, out, 99, 2, line.as_str(), color::ACCENT);
            if view.gps_enabled && view.gps_status.valid {
                centered(
                    row,
                    out,
                    143,
                    2,
                    coordinate(view.gps_status.latitude_e7, "N", "S").as_str(),
                    color::TEXT,
                );
                centered(
                    row,
                    out,
                    173,
                    2,
                    coordinate(view.gps_status.longitude_e7, "E", "W").as_str(),
                    color::TEXT,
                );
                let mut age = Text::new();
                let _ = write!(age, "ВОЗРАСТ ПОЗИЦИИ {} С", view.gps_status.age_seconds);
                centered(row, out, 213, 1, age.as_str(), color::MUTED);
            } else {
                centered(row, out, 151, 2, "--.-----", color::MUTED);
                centered(row, out, 180, 2, "--.-----", color::MUTED);
            }
            centered(row, out, 264, 1, "MENU: ПИТАНИЕ GPS", color::ACCENT);
        }
        Screen::Aprs => {
            let (label, help) = match view.aprs_status {
                AprsStatus::Unconfigured => ("НЕ НАСТРОЕН", "ПОЗЫВНОЙ: В CPS / ШТАТНОМ CFG"),
                AprsStatus::Unsupported => ("МОДЕМ НЕ ПОДДЕРЖАН", "ДРУГОЕ ИСПОЛНЕНИЕ ПЛАТЫ"),
                AprsStatus::NoFix => ("НЕТ ПОЗИЦИИ GPS", "ДОЖДИТЕСЬ ДОСТОВЕРНОЙ ПОЗИЦИИ"),
                AprsStatus::Ready => ("МАЯК ГОТОВ", "MENU: ОТПРАВИТЬ ОДИН РАЗ"),
                AprsStatus::Sending => ("ПЕРЕДАЧА ПАКЕТА", "ОТПУСТИТЕ КЛАВИШИ"),
                AprsStatus::Sent => ("ПАКЕТ ОТПРАВЛЕН", "ДОСТАВКА НЕ ПОДТВЕРЖДЕНА"),
                AprsStatus::Failed => ("ОШИБКА ПЕРЕДАЧИ", "MENU: ПОВТОР ПОСЛЕ ПРОВЕРКИ"),
            };
            let mut identity = Text::new();
            for &c in &view.aprs_callsign {
                if c != b' ' {
                    let _ = identity.write_char(char::from(c));
                }
            }
            if identity.as_str().is_empty() {
                let _ = identity.write_str("---");
            } else {
                let _ = write!(identity, "-{}", view.aprs_ssid);
            }
            centered(row, out, 65, 3, identity.as_str(), color::ACCENT);
            let tx_frequency = if view.mode == Mode::Channel {
                model
                    .channel
                    .filter(|c| c.active)
                    .map(|c| displayed_channel(c, view.reverse).1)
            } else if view.reverse {
                Some(view.vfo_10hz)
            } else {
                crate::controller::vfo_tx_frequency(view)
            };
            let mut target = Text::new();
            let _ = write!(
                target,
                "{} TX {} МГЦ",
                if view.active_slot == Slot::A {
                    "A"
                } else {
                    "B"
                },
                frequency(tx_frequency.unwrap_or(0)).as_str()
            );
            centered(row, out, 109, 1, "АКТИВНЫЙ КАНАЛ / VFO", color::MUTED);
            centered(row, out, 127, 1, target.as_str(), color::ACCENT);
            centered(row, out, 165, 2, label, color::TEXT);
            centered(row, out, 207, 1, help, color::MUTED);
            centered(row, out, 250, 1, "РУЧНОЙ МАЯК / AX.25", color::ACCENT);
        }
        Screen::TxId => {
            centered(row, out, 62, 2, "ПЯТЬ ЦИФР ID", color::ACCENT);
            let mut draft = Text::new();
            if view.tx_id_digits > 0 {
                let _ = write!(
                    draft,
                    "{:0width$}",
                    view.tx_id_draft,
                    width = usize::from(view.tx_id_digits)
                );
            }
            for _ in view.tx_id_digits..5 {
                let _ = draft.write_char('_');
            }
            centered(row, out, 112, 4, draft.as_str(), color::TEXT);
            let mut current = Text::new();
            if let Some(id) = view.tx_id {
                let _ = write!(current, "СЕЙЧАС: {id:05}");
            } else {
                let _ = current.write_str("ID НЕ ЗАДАН");
            }
            centered(row, out, 166, 2, current.as_str(), color::MUTED);
            centered(
                row,
                out,
                212,
                1,
                "MENU: СОХРАНИТЬ   #: ОЧИСТИТЬ",
                color::ACCENT,
            );
            centered(row, out, 238, 1, "EXIT: ОТМЕНА БЕЗ ИЗМЕНЕНИЙ", color::MUTED);
            centered(
                row,
                out,
                264,
                1,
                "SELECT V ПЕРЕДАЁТ ID, НЕ ИМЯ",
                color::MUTED,
            );
        }
        _ => {}
    }
}
