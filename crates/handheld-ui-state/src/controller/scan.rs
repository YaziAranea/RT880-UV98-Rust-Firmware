use super::{
    adjacent_index, digit, frequency_entry_value, Controller, Entry, EntryKind, Mode, ScanState,
    Screen, Slot, DEFAULT_FREQUENCY_10HZ, STEP_10HZ,
};
use crate::input::InputEvent;
use uv98_band_plan::operating_range;
use uv98_keys::Key;

pub const SCAN_RANGE_MAX_POINTS: u32 = 1024;
pub const SCAN_JOURNAL_CAPACITY: u8 = 16;

/// Inclusive custom VFO scan range. Frequencies and step use 10 Hz units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanRange {
    pub lower_10hz: u32,
    pub upper_10hz: u32,
    pub step_10hz: u32,
}

impl ScanRange {
    pub fn validate(self) -> Result<(), ScanRangeError> {
        if self.lower_10hz == 0 || self.lower_10hz > self.upper_10hz {
            return Err(ScanRangeError::Bounds);
        }
        let band = operating_range(self.lower_10hz).ok_or(ScanRangeError::Band)?;
        if Some(band) != operating_range(self.upper_10hz) {
            return Err(ScanRangeError::Band);
        }
        let distance = self.upper_10hz - self.lower_10hz;
        if !STEP_10HZ.contains(&self.step_10hz) || distance % self.step_10hz != 0 {
            return Err(ScanRangeError::Grid);
        }
        if distance / self.step_10hz >= SCAN_RANGE_MAX_POINTS {
            return Err(ScanRangeError::TooMany);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanRangeField {
    Mode,
    Lower,
    Upper,
    Step,
    Save,
}

impl ScanRangeField {
    fn adjacent(self, manual: bool, forward: bool) -> Self {
        const FIELDS: [ScanRangeField; 5] = [
            ScanRangeField::Mode,
            ScanRangeField::Lower,
            ScanRangeField::Upper,
            ScanRangeField::Step,
            ScanRangeField::Save,
        ];
        if manual {
            FIELDS[usize::from(adjacent_index(self as u8, FIELDS.len() as u8, forward))]
        } else if self == Self::Mode {
            Self::Save
        } else {
            Self::Mode
        }
    }

    fn is_endpoint(self) -> bool {
        matches!(self, Self::Lower | Self::Upper)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanRangeError {
    Entry,
    Bounds,
    Grid,
    TooMany,
    Band,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanRangePage {
    pub lower_10hz: u32,
    pub upper_10hz: u32,
    pub step_10hz: u32,
    pub manual: bool,
    pub field: ScanRangeField,
    pub entry: Option<Entry>,
    pub error: Option<ScanRangeError>,
}

impl ScanRangePage {
    fn range(self) -> ScanRange {
        ScanRange {
            lower_10hz: self.lower_10hz,
            upper_10hz: self.upper_10hz,
            step_10hz: self.step_10hz,
        }
    }

    fn confirm_entry(&mut self, entry: Entry) {
        let Some(frequency) = frequency_entry_value(entry) else {
            self.error = Some(ScanRangeError::Entry);
            return;
        };
        match self.field {
            ScanRangeField::Lower => self.lower_10hz = frequency,
            ScanRangeField::Upper => self.upper_10hz = frequency,
            _ => return,
        }
        self.entry = None;
        self.error = None;
    }

    fn push_digit(&mut self, digit: u8) {
        let mut entry = self.entry.unwrap_or(Entry {
            kind: EntryKind::Frequency,
            value: 0,
            digits: 0,
        });
        if entry.digits >= entry.kind.maximum_digits() {
            self.error = Some(ScanRangeError::Entry);
            return;
        }
        entry.value = entry.value * 10 + u32::from(digit);
        entry.digits += 1;
        self.entry = Some(entry);
        self.error = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanJournalEntry {
    pub frequency_10hz: u32,
    /// Whole seconds since boot at the most recent confirmed opening.
    pub last_opened_s: u32,
    pub peak_raw_rssi: u16,
    pub detections: u16,
    pub ignored: bool,
}

impl ScanJournalEntry {
    const EMPTY: Self = Self {
        frequency_10hz: 0,
        last_opened_s: 0,
        peak_raw_rssi: 0,
        detections: 0,
        ignored: false,
    };
}

/// Only the selected entry is copied with View. History stays in Controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanJournalPage {
    pub index: u8,
    pub count: u8,
    pub entry: Option<ScanJournalEntry>,
    pub target: Option<Slot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScanJournal {
    entries: [ScanJournalEntry; SCAN_JOURNAL_CAPACITY as usize],
    count: u8,
}

impl ScanJournal {
    pub(super) const fn new() -> Self {
        Self {
            entries: [ScanJournalEntry::EMPTY; SCAN_JOURNAL_CAPACITY as usize],
            count: 0,
        }
    }

    fn find(&self, frequency_10hz: u32) -> Option<usize> {
        self.entries[..usize::from(self.count)]
            .iter()
            .position(|entry| entry.frequency_10hz == frequency_10hz)
    }

    fn record(
        &mut self,
        frequency_10hz: u32,
        peak_raw_rssi: u16,
        opened: bool,
        opened_at_ms: u32,
    ) -> bool {
        let (index, mut entry) = if let Some(index) = self.find(frequency_10hz) {
            (index, self.entries[index])
        } else if !opened {
            return false;
        } else if self.count < SCAN_JOURNAL_CAPACITY {
            let index = usize::from(self.count);
            self.count += 1;
            (index, ScanJournalEntry::EMPTY)
        } else {
            // Ignored frequencies remain restorable for the whole session.
            let Some(index) = self.entries.iter().rposition(|entry| !entry.ignored) else {
                return false;
            };
            (index, ScanJournalEntry::EMPTY)
        };
        if entry.ignored {
            return false;
        }
        entry.frequency_10hz = frequency_10hz;
        entry.peak_raw_rssi = entry.peak_raw_rssi.max(peak_raw_rssi);
        if opened {
            entry.last_opened_s = opened_at_ms / 1000;
            entry.detections = entry.detections.saturating_add(1);
            self.entries.copy_within(0..index, 1);
            self.entries[0] = entry;
        } else if self.entries[index] != entry {
            self.entries[index] = entry;
        } else {
            return false;
        }
        true
    }
}

impl Controller {
    /// None leaves the runtime's existing automatic VFO scan in effect.
    pub const fn scan_range(&self) -> Option<ScanRange> {
        self.manual_scan_range
    }

    /// Record one confirmed scan opening or a subsequent peak sample.
    pub fn record_scan_find(
        &mut self,
        frequency_10hz: u32,
        peak_raw_rssi: u16,
        opened: bool,
        opened_at_ms: u32,
    ) {
        if operating_range(frequency_10hz).is_none()
            || !self
                .scan_journal
                .record(frequency_10hz, peak_raw_rssi, opened, opened_at_ms)
        {
            return;
        }
        if let Some(page) = self.view.scan_journal_page.as_mut() {
            let selection = page.entry.map(|entry| entry.frequency_10hz);
            let index = selection.and_then(|frequency| self.scan_journal.find(frequency));
            if index.is_none() {
                page.target = None;
            }
            page.index = index.unwrap_or(usize::from(page.index)) as u8;
            self.project_scan_journal();
        }
    }

    /// A temporary per-frequency filter scoped to the current power session.
    pub fn scan_frequency_ignored(&self, frequency_10hz: u32) -> bool {
        self.scan_journal
            .find(frequency_10hz)
            .is_some_and(|index| self.scan_journal.entries[index].ignored)
    }

    pub(super) fn open_scan_range(&mut self) {
        let range = self.manual_scan_range.unwrap_or_else(|| {
            let step = STEP_10HZ[usize::from(self.view.step_index)];
            let origin = self.view.vfo_10hz;
            let (origin, low, high) = operating_range(origin).map_or(
                (
                    DEFAULT_FREQUENCY_10HZ,
                    DEFAULT_FREQUENCY_10HZ - 50_000,
                    DEFAULT_FREQUENCY_10HZ + 50_000,
                ),
                |(low, high)| (origin, low, high),
            );
            ScanRange {
                lower_10hz: origin - ((origin - low).min(50_000) / step) * step,
                upper_10hz: origin + ((high - origin).min(50_000) / step) * step,
                step_10hz: step,
            }
        });
        self.view.scan_range_page = Some(ScanRangePage {
            lower_10hz: range.lower_10hz,
            upper_10hz: range.upper_10hz,
            step_10hz: range.step_10hz,
            manual: self.manual_scan_range.is_some(),
            field: ScanRangeField::Mode,
            entry: None,
            error: None,
        });
        self.view.screen = Screen::ScanRange;
    }

    pub(super) fn scan_range_input(&mut self, event: InputEvent) {
        let Some(mut page) = self.view.scan_range_page else {
            return;
        };
        match event {
            InputEvent::Short(Key::Exit) => self.view.screen = Screen::Menu,
            InputEvent::Hold(Key::Exit) => self.view.screen = Screen::Home,
            InputEvent::Short(Key::Menu) => {
                if let Some(entry) = page.entry {
                    page.confirm_entry(entry);
                } else {
                    match page.field {
                        ScanRangeField::Mode => {
                            page.manual = !page.manual;
                            page.error = None;
                        }
                        ScanRangeField::Step => {
                            let next = STEP_10HZ
                                .iter()
                                .position(|&step| step == page.step_10hz)
                                .map_or(0, |index| (index + 1) % STEP_10HZ.len());
                            page.step_10hz = STEP_10HZ[next];
                            page.error = None;
                        }
                        ScanRangeField::Save => {
                            let range = page.manual.then(|| page.range());
                            page.error = range.and_then(|range| range.validate().err());
                            if page.error.is_none() {
                                self.manual_scan_range = range;
                                self.view.screen = Screen::Menu;
                            }
                        }
                        ScanRangeField::Lower | ScanRangeField::Upper => {}
                    }
                }
            }
            InputEvent::Short(Key::Up | Key::Down) | InputEvent::Repeat(Key::Up | Key::Down)
                if page.entry.is_none() =>
            {
                let forward = matches!(
                    event,
                    InputEvent::Short(Key::Down) | InputEvent::Repeat(Key::Down)
                );
                page.field = page.field.adjacent(page.manual, forward);
                page.error = None;
            }
            InputEvent::Short(Key::Star) if page.entry.is_some() => {
                page.entry = page
                    .entry
                    .filter(|entry| entry.digits > 1)
                    .map(|mut entry| {
                        entry.value /= 10;
                        entry.digits -= 1;
                        entry
                    });
                page.error = None;
            }
            InputEvent::Short(key) if page.field.is_endpoint() => {
                if let Some(digit) = digit(key) {
                    page.push_digit(digit);
                }
            }
            _ => {}
        }
        self.view.scan_range_page = Some(page);
    }

    pub(super) fn open_scan_journal(&mut self) {
        self.view.screen = Screen::ScanJournal;
        self.view.scan_journal_page = Some(ScanJournalPage::default());
        self.project_scan_journal();
    }

    fn project_scan_journal(&mut self) {
        let Some(page) = self.view.scan_journal_page.as_mut() else {
            return;
        };
        page.count = self.scan_journal.count;
        page.index = page.index.min(page.count.saturating_sub(1));
        page.entry = (page.count != 0).then(|| self.scan_journal.entries[usize::from(page.index)]);
    }

    pub(super) fn scan_journal_input(&mut self, event: InputEvent) {
        let Some(mut page) = self.view.scan_journal_page else {
            return;
        };
        match event {
            InputEvent::Short(Key::Exit) if page.target.is_some() => page.target = None,
            InputEvent::Short(Key::Exit) => self.view.screen = Screen::Menu,
            InputEvent::Hold(Key::Exit) => self.view.screen = Screen::Home,
            InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up) => {
                if page.target.is_some() {
                    page.target = Some(Slot::A);
                } else {
                    page.index = page.index.saturating_sub(1);
                }
            }
            InputEvent::Short(Key::Down) | InputEvent::Repeat(Key::Down) => {
                if page.target.is_some() {
                    page.target = Some(Slot::B);
                } else {
                    page.index = page.index.saturating_add(1);
                }
            }
            InputEvent::Short(Key::Hash) if page.target.is_none() && page.entry.is_some() => {
                let entry = &mut self.scan_journal.entries[usize::from(page.index)];
                entry.ignored = !entry.ignored;
            }
            InputEvent::Short(Key::Menu)
                if self.view.scan == ScanState::Stopped && !self.view.ptt_down =>
            {
                if let Some(entry) = page.entry {
                    if let Some(target) = page.target {
                        self.switch_slot(target);
                        self.leave_selected_cb();
                        self.view.mode = Mode::Vfo;
                        self.view.vfo_10hz = entry.frequency_10hz;
                        self.view.reverse = false;
                        self.view.entry = None;
                        self.view.screen = Screen::Home;
                    } else {
                        page.target = Some(self.view.active_slot);
                    }
                }
            }
            _ => {}
        }
        self.view.scan_journal_page = Some(page);
        self.project_scan_journal();
    }
}
