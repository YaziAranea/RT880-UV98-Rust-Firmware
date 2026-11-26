//! Range editing and the session's scan observations on the native LCD.
use super::*;
use crate::controller::{ScanRangeError, ScanRangeField, ScanRangePage};

pub(super) const RANGE_TITLE: &str = "\x1f190";
pub(super) const JOURNAL_TITLE: &str = "\x1f191";
pub(super) const RANGE_ERROR_Y: u16 = 225;
pub(super) const RANGE_HINT_Y: u16 = 245;
pub(super) const JOURNAL_INDEX_Y: u16 = 82;
pub(super) const JOURNAL_FREQUENCY_Y: u16 = 112;
pub(super) const JOURNAL_METRICS_Y: u16 = 157;
pub(super) const JOURNAL_COUNT_Y: u16 = 176;
pub(super) const JOURNAL_LAST_OPENED_Y: u16 = 195;
pub(super) const JOURNAL_ACTION_Y: u16 = 224;
const JOURNAL_UNIT_Y: u16 = 135;
const RANGE_FIELDS: [ScanRangeField; 5] = [
    ScanRangeField::Mode,
    ScanRangeField::Lower,
    ScanRangeField::Upper,
    ScanRangeField::Step,
    ScanRangeField::Save,
];

pub(super) const fn range_field_top(field: ScanRangeField) -> u16 {
    56 + 32
        * match field {
            ScanRangeField::Mode => 0,
            ScanRangeField::Lower => 1,
            ScanRangeField::Upper => 2,
            ScanRangeField::Step => 3,
            ScanRangeField::Save => 4,
        }
}

pub(super) fn range(row: u16, out: &mut Row<'_>, view: &View) {
    let Some(page) = view.scan_range_page else {
        return;
    };
    for field in RANGE_FIELDS {
        let y = range_field_top(field);
        if (y..y + ITEM_PAINT_HEIGHT).contains(&row) {
            range_field(row, out, page, field, y);
            return;
        }
    }

    let (message, ink) = if let Some(error) = page.error {
        (
            match error {
                ScanRangeError::Entry => "\x1f198",
                ScanRangeError::Bounds => "\x1f199",
                ScanRangeError::Grid => "\x1f19A",
                ScanRangeError::TooMany => "\x1f19B",
                ScanRangeError::Band => "\x1f19C",
            },
            color::WARNING,
        )
    } else if page.manual {
        ("\x1f19B", color::MUTED)
    } else {
        ("\x1f19D", color::MUTED)
    };
    centered(row, out, RANGE_ERROR_Y, 1, message, ink);

    if page.entry.is_some() {
        centered(row, out, RANGE_HINT_Y + 15, 1, "\x1f169", color::MUTED);
    } else {
        centered(row, out, RANGE_HINT_Y, 1, "\x1f196", color::MUTED);
        if matches!(page.field, ScanRangeField::Lower | ScanRangeField::Upper) {
            centered(row, out, RANGE_HINT_Y + 15, 1, "\x1f197", color::MUTED);
        }
    }
}

#[inline(never)]
fn range_field(row: u16, out: &mut Row<'_>, page: ScanRangePage, field: ScanRangeField, y: u16) {
    let selected = page.field == field;
    item_background(row, out, y, selected);
    if !(y + 8..y + 22).contains(&row) {
        return;
    }
    let ink = if !page.manual
        && matches!(
            field,
            ScanRangeField::Lower | ScanRangeField::Upper | ScanRangeField::Step
        ) {
        color::MUTED
    } else {
        item_color(selected)
    };
    let label = match field {
        ScanRangeField::Mode => "\x1f192",
        ScanRangeField::Lower => "\x1f193",
        ScanRangeField::Upper => "\x1f194",
        ScanRangeField::Step => "\x1f1A5",
        ScanRangeField::Save => "\x1f195",
    };
    if field == ScanRangeField::Save {
        centered(row, out, y + 8, 2, label, ink);
        return;
    }
    text(row, out, 22, y + 12, 1, label, ink);
    if !page.manual && field != ScanRangeField::Mode {
        text(row, out, 109, y + 8, 2, "---", ink);
        return;
    }
    match field {
        ScanRangeField::Mode => text(
            row,
            out,
            109,
            y + 8,
            2,
            if page.manual { "\x1f1A4" } else { "\x1f1A8" },
            ink,
        ),
        ScanRangeField::Lower | ScanRangeField::Upper => {
            if let Some(entry) = page.entry.filter(|_| selected) {
                let value = entry_text(entry.value, entry.digits, EntryKind::Frequency);
                text(row, out, 109, y + 8, 2, value.as_str(), ink);
            } else {
                let value = frequency(if field == ScanRangeField::Lower {
                    page.lower_10hz
                } else {
                    page.upper_10hz
                });
                text(row, out, 109, y + 8, 2, value.as_str(), ink);
            }
        }
        ScanRangeField::Step => {
            let step = STEP_10HZ.iter().position(|&step| step == page.step_10hz);
            text(
                row,
                out,
                109,
                y + 8,
                2,
                step_label(step.map_or(u8::MAX, |i| i as u8)),
                ink,
            );
        }
        ScanRangeField::Save => {}
    }
}

pub(super) fn range_action(view: &View) -> &'static str {
    let Some(page) = view.scan_range_page else {
        return "";
    };
    match page.field {
        ScanRangeField::Mode | ScanRangeField::Step => "\x1f01F",
        ScanRangeField::Save => "\x1f026",
        _ if page.entry.is_some() => "\x1f027",
        _ => "",
    }
}

pub(super) fn journal(row: u16, out: &mut Row<'_>, view: &View) {
    let Some(page) = view.scan_journal_page else {
        return;
    };
    let Some(entry) = page.entry else {
        centered(row, out, 138, 2, "\x1f19E", color::MUTED);
        return;
    };

    rect(row, out, 12, 68, 216, 143, color::PANEL);
    rect(row, out, 24, 149, 192, 1, color::LINE);
    if (JOURNAL_INDEX_Y..JOURNAL_INDEX_Y + 7).contains(&row) {
        let mut value = Text::new();
        let _ = write!(value, "{} / {}", u16::from(page.index) + 1, page.count);
        centered(row, out, JOURNAL_INDEX_Y, 1, value.as_str(), color::MUTED);
    }
    if (JOURNAL_FREQUENCY_Y..JOURNAL_FREQUENCY_Y + 21).contains(&row) {
        let value = frequency(entry.frequency_10hz);
        centered(
            row,
            out,
            JOURNAL_FREQUENCY_Y,
            3,
            value.as_str(),
            color::ACCENT,
        );
    }
    centered(row, out, JOURNAL_UNIT_Y, 1, "\x1f09A", color::MUTED);

    for (y, label, number) in [
        (JOURNAL_METRICS_Y, "\x1f19F", entry.peak_raw_rssi),
        (JOURNAL_COUNT_Y, "\x1f1A0", entry.detections),
    ] {
        text(row, out, 24, y + 3, 1, label, color::TEXT);
        if (y..y + 14).contains(&row) {
            let mut value = Text::new();
            let _ = write!(value, "{number}");
            text(row, out, 156, y, 2, value.as_str(), color::TEXT);
        }
    }

    let y = JOURNAL_LAST_OPENED_Y;
    text(row, out, 24, y - 2, 1, "\x1f1A9", color::TEXT);
    text(row, out, 24, y + 8, 1, "\x1f1AA", color::MUTED);
    if (y..y + 14).contains(&row) {
        let mut value = Text::new();
        let seconds = entry.last_opened_s;
        let _ = write!(value, "{}:{:02}", seconds / 60, seconds % 60);
        let scale = fitting_scale(value.as_str(), 2, 228 - 156);
        text(row, out, 156, y, scale, value.as_str(), color::TEXT);
    }

    let prompt = if view.scan != ScanState::Stopped {
        "\x1f0D1"
    } else if page.target.is_some() {
        "\x1f1A3"
    } else {
        ""
    };
    centered(row, out, JOURNAL_ACTION_Y, 1, prompt, color::MUTED);
    if let Some(slot) = page.target {
        for (x, label, target) in [(12, "VFO A", Slot::A), (122, "VFO B", Slot::B)] {
            let selected = slot == target;
            let fill = if selected {
                color::ACCENT
            } else {
                color::PANEL
            };
            rect(row, out, x, 239, 106, 29, fill);
            text(row, out, x + 24, 247, 2, label, item_color(selected));
        }
    } else {
        centered(row, out, JOURNAL_ACTION_Y + 12, 1, "\x1f1A6", color::MUTED);
        let (label, ink) = if entry.ignored {
            ("\x1f1A2", color::WARNING)
        } else {
            ("\x1f1A1", color::MUTED)
        };
        centered(row, out, JOURNAL_ACTION_Y + 29, 1, label, ink);
    }
}

pub(super) fn journal_actions(view: &View) -> (&'static str, &'static str) {
    let Some(page) = view.scan_journal_page.filter(|page| page.entry.is_some()) else {
        return ("", "\x1f012");
    };
    let target = page.target.is_some();
    let left = if view.scan != ScanState::Stopped || view.ptt_down {
        ""
    } else if target {
        "\x1f024"
    } else {
        "\x1f1A7"
    };
    (left, if target { "\x1f013" } else { "\x1f012" })
}
