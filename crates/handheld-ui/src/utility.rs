//! Compact pages and measured HF waterfall. No RF operations in rendering.
use super::*;
use crate::controller::{AprsStatus, ShortwaveMode};

const BINS: usize = 32;
const ROWS: usize = 16;
const GRAPH_TOP: u16 = 96;
const GRAPH_BOTTOM: u16 = 154;
const PROGRESS_TOP: u16 = 168;
const HISTORY_TOP: u16 = 172;
const HISTORY_ROW_HEIGHT: u16 = 4;
const HISTORY_LAST: u16 = HISTORY_TOP + ROWS as u16 * HISTORY_ROW_HEIGHT - 1;

/// LCD damage in full-width rows, after accounting for the displayed RSSI scale.
#[derive(Default)]
struct ShortwaveDamage {
    graph: u64,
    history: u16,
    progress: bool,
}

impl ShortwaveDamage {
    fn ranges(self) -> impl Iterator<Item = (u16, u16)> {
        (GRAPH_TOP..=GRAPH_BOTTOM)
            .filter(move |row| self.graph & (1 << (*row - GRAPH_TOP)) != 0)
            .map(|row| (row, row + 1))
            .chain(self.progress.then_some((PROGRESS_TOP, PROGRESS_TOP + 2)))
            .chain((0..ROWS as u16).filter_map(move |age| {
                let top = HISTORY_TOP + age * HISTORY_ROW_HEIGHT;
                (self.history & (1 << age) != 0).then_some((top, top + HISTORY_ROW_HEIGHT))
            }))
    }
}

/// Owned by the app, borrowed by Model. Never copied with each View/input event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortwaveScope {
    pub first_hz: u32,
    pub step_hz: u32,
    points: u8,
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
            points: 0,
            samples: [0; BINS],
            valid: 0,
            history: [[0; BINS]; ROWS],
            newest: 0,
            rows: 0,
        }
    }

    /// Clear history whenever the physical sampling grid changes.
    pub fn reset(&mut self, first_hz: u32, step_hz: u32, points: u8) {
        *self = Self::new();
        if matches!(points, 8 | 16 | 32) {
            self.first_hz = first_hz;
            self.step_hz = step_hz;
            self.points = points;
        }
    }

    pub fn point_count(&self) -> u8 {
        self.points
    }

    /// Stage measurements separately from the visible graph. Only a complete
    /// measured sweep publishes a new graph/history row; a partially acquired
    /// row must never erase the last complete sweep while the LCD is drawing.
    /// Returns half-open ranges of changed panel rows.
    pub fn sample(
        &mut self,
        bin: u8,
        rssi: u8,
        _snr: u8,
        complete: bool,
    ) -> impl Iterator<Item = (u16, u16)> {
        let bin = usize::from(bin);
        let mut damage = ShortwaveDamage::default();
        let count = usize::from(self.points);
        if bin >= count || self.step_hz == 0 {
            return damage.ranges();
        }
        let old_progress = self.valid.count_ones();
        if bin == 0 {
            self.valid = 0;
        }
        self.samples[bin] = rssi;
        self.valid |= 1 << bin;
        damage.progress = self.valid.count_ones() != old_progress;
        if complete && bin + 1 == count && self.valid == u32::MAX >> (BINS - count) {
            let previous = self.displayed_samples();
            for (bin, &sample) in self.samples[..count].iter().enumerate() {
                let old_bar = previous.map_or(0, |old| graph_mask(old[bin]));
                damage.graph |= old_bar ^ graph_mask(sample);
            }
            let rows = (self.rows + 1).min(ROWS);
            for age in 0..rows {
                // Compare at the visible age, before the ring overwrites the
                // oldest sample row. A changed RSSI in the same color is free.
                let incoming = if age == 0 {
                    &self.samples[..count]
                } else {
                    self.history_row(age - 1).unwrap()
                };
                if self.history_row(age).is_none_or(|old| {
                    old.iter()
                        .zip(incoming)
                        .any(|(&before, &after)| waterfall_ink(before) != waterfall_ink(after))
                }) {
                    damage.history |= 1 << age;
                }
            }
            self.newest = (self.newest + 1) % ROWS;
            self.history[self.newest] = self.samples;
            self.rows = rows;
        }
        damage.ranges()
    }

    pub fn measured_bins(&self) -> u32 {
        self.valid
    }
    pub fn history_rows(&self) -> usize {
        self.rows
    }

    pub fn displayed_samples(&self) -> Option<&[u8]> {
        self.history_row(0)
    }

    fn history_row(&self, age: usize) -> Option<&[u8]> {
        (age < self.rows)
            .then(|| &self.history[(self.newest + ROWS - age) % ROWS][..usize::from(self.points)])
    }
}

fn bar_height(sample: u8) -> u16 {
    u16::from(sample.min(63)) * (GRAPH_BOTTOM - GRAPH_TOP) / 63
}

fn graph_mask(sample: u8) -> u64 {
    let height = bar_height(sample);
    // A zero-height bar is still one baseline pixel.
    ((1 << height.max(1)) - 1) << (GRAPH_BOTTOM - GRAPH_TOP - height)
}

fn waterfall_ink(rssi: u8) -> u16 {
    WATERFALL_COLORS[1 + usize::from(rssi / 10).min(6)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waterfall_contains_only_complete_measured_sweeps_and_resets_grid() {
        let mut scope = ShortwaveScope::new();
        scope.reset(7_020_000, 5_000, 32);
        let _ = scope.sample(31, 40, 8, true);
        assert_eq!(scope.history_rows(), 0);
        for bin in 0..32 {
            let _ = scope.sample(bin, bin + 10, 4, bin == 31);
        }
        assert_eq!(scope.history_rows(), 1);
        assert_eq!(scope.measured_bins(), u32::MAX);
        let _ = scope.sample(0, 30, 5, false);
        let _ = scope.sample(31, 40, 8, true);
        assert_eq!(scope.history_rows(), 1);
        scope.reset(14_100_000, 1_000, 32);
        assert_eq!(scope.history_rows(), 0);
        assert_eq!(scope.measured_bins(), 0);
    }

    #[test]
    fn unfinished_sweep_keeps_previous_graph_and_never_publishes_missing_bins() {
        let mut scope = ShortwaveScope::new();
        scope.reset(7_084_000, 1_000, 32);
        assert_eq!(scope.displayed_samples(), None);
        for bin in 0..32 {
            let _ = scope.sample(bin, bin + 10, 4, bin == 31);
            assert_eq!(scope.history_rows(), usize::from(bin == 31));
        }
        let mut complete = [0; 32];
        complete.copy_from_slice(scope.displayed_samples().unwrap());
        for bin in 0..31 {
            let _ = scope.sample(bin, 60, 4, false);
            assert_eq!(scope.displayed_samples(), Some(complete.as_slice()));
        }
        let _ = scope.sample(31, 60, 4, true);
        assert_eq!(scope.displayed_samples(), Some([60; 32].as_slice()));
        let _ = scope.sample(0, 4, 1, false);
        let _ = scope.sample(31, 4, 1, true);
        assert_eq!(scope.sample(32, 4, 1, true).count(), 0);
        assert_eq!(scope.displayed_samples(), Some([60; 32].as_slice()));
        assert_eq!(scope.history_rows(), 2);
    }

    #[test]
    fn each_resolution_publishes_only_its_measured_columns() {
        let mut scope = ShortwaveScope::new();
        for points in [8, 16, 32] {
            scope.reset(7_020_000, 5_000, points);
            assert_eq!(scope.history_rows(), 0);
            assert_eq!(scope.sample(points, 63, 0, true).count(), 0);
            let _ = scope.sample(points - 1, 40, 0, true);
            assert_eq!(scope.history_rows(), 0);
            for bin in 0..points {
                let _ = scope.sample(bin, bin + 20, 8, bin + 1 == points);
                assert_eq!(scope.history_rows(), usize::from(bin + 1 == points));
            }
            let row = scope.displayed_samples().unwrap();
            assert_eq!(row.len(), usize::from(points));
            assert!(row.iter().copied().eq(20..20 + points));
        }
    }
}

pub(super) fn shortwave_processing_label(row: u16, out: &mut Row<'_>, view: View, y: u16) {
    let mut label = Text::new();
    if view.shortwave_mode == ShortwaveMode::Am {
        let filter = ["6", "3", "1"][usize::from(view.shortwave_filter.min(2))];
        let _ = write!(
            label,
            "\x1f009 {filter} \x1f08F {}",
            if view.shortwave_soft_mute {
                "ON"
            } else {
                "OFF"
            }
        );
    } else {
        let _ = label.write_str("\x1f137");
    }
    centered(row, out, y, 1, label.as_str(), color::ACCENT);
}

pub(super) fn shortwave_scope_page(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let view = &model.view;
    if (55..69).contains(&row) {
        let mut title = Text::new();
        let _ = write!(
            title,
            "{}  {}.{:03} \x1f08E",
            view.shortwave_mode.label(),
            view.shortwave_khz,
            view.shortwave_fraction_hz
        );
        centered(row, out, 55, 2, title.as_str(), color::TEXT);
        return;
    }
    if (252..259).contains(&row) {
        centered(row, out, 252, 1, "# 8/16/32 \x1f134", color::ACCENT);
        return;
    }
    let Some(scope) = model
        .shortwave_scope
        .filter(|scope| scope.step_hz > 0 && scope.point_count() != 0)
    else {
        if (78..85).contains(&row) {
            let mut points = Text::new();
            let _ = write!(points, "{} \x1f134", view.shortwave_scope_points);
            centered(row, out, 78, 1, points.as_str(), color::ACCENT);
        }
        centered(row, out, 136, 2, "\x1f0AF", color::MUTED);
        centered(row, out, 266, 1, "\x1f017", color::ACCENT);
        return;
    };
    const X: usize = 12;
    const PLOT_WIDTH: usize = 224;
    let points = u32::from(scope.point_count());
    let pitch = PLOT_WIDTH / points as usize;
    match row {
        78..=84 => {
            let mut width = Text::new();
            let _ = write!(
                width,
                "{points} \x1f134  \x1f13A {}  \x1f090 {} \x1f08E",
                scope.step_hz / 1000,
                scope.step_hz * (points - 1) / 1000
            );
            centered(row, out, 78, 1, width.as_str(), color::ACCENT);
        }
        GRAPH_TOP..=GRAPH_BOTTOM => {
            // The fixed RSSI scale keeps levels comparable across sweeps.
            for (index, &sample) in scope.displayed_samples().into_iter().flatten().enumerate() {
                let height = bar_height(sample);
                rect(
                    row,
                    out,
                    X + index * pitch,
                    GRAPH_BOTTOM - height,
                    pitch - 1,
                    height.max(1),
                    color::ACCENT,
                );
            }
            let selected_hz = view.shortwave_khz * 1000 + u32::from(view.shortwave_fraction_hz);
            let cursor = selected_hz.saturating_sub(scope.first_hz) / scope.step_hz;
            if cursor < points {
                out[X + cursor as usize * pitch + pitch / 2] = color::WARNING;
            }
        }
        158..=164 => {
            for (index, x) in [(0, 12), (points / 2, 98), (points - 1, 194)] {
                let khz = (scope.first_hz + scope.step_hz * index) / 1000;
                let mut label = Text::new();
                let _ = write!(label, "{khz}");
                text(row, out, x, 158, 1, label.as_str(), color::MUTED);
            }
        }
        PROGRESS_TOP..=169 => {
            let progress = scope.measured_bins().count_ones() as usize * pitch;
            out[X..X + progress].fill(color::MUTED);
        }
        HISTORY_TOP..=HISTORY_LAST => {
            let age = usize::from((row - HISTORY_TOP) / HISTORY_ROW_HEIGHT);
            if let Some(samples) = scope.history_row(age) {
                for (bin, &rssi) in samples.iter().enumerate() {
                    let x = X + bin * pitch;
                    out[x..x + pitch].fill(waterfall_ink(rssi));
                }
            }
        }
        244..=250 => shortwave_processing_label(row, out, *view, 244),
        260..=266 => centered(row, out, 260, 1, "\x1f0C8", color::MUTED),
        276..=282 => centered(row, out, 276, 1, "\x1f144", color::ACCENT),
        _ => {}
    }
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

pub(super) fn utility_page(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let view = model.view;
    match view.screen {
        Screen::AprsRx => aprs_rx_page(row, out, &view),
        Screen::Gps => {
            centered(
                row,
                out,
                62,
                2,
                if !view.gps_enabled {
                    "\x1f01A"
                } else if view.gps_status.valid {
                    "\x1f0E7"
                } else {
                    "\x1f0EB"
                },
                if view.gps_status.valid {
                    color::GOOD
                } else {
                    color::MUTED
                },
            );
            let mut line = Text::new();
            let _ = write!(line, "\x1f128 {:02}", view.gps_status.satellites);
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
                let _ = write!(age, "\x1f053 {} \x1f10A", view.gps_status.age_seconds);
                centered(row, out, 213, 1, age.as_str(), color::MUTED);
            } else {
                centered(row, out, 151, 2, "--.-----", color::MUTED);
                centered(row, out, 180, 2, "--.-----", color::MUTED);
            }
            centered(row, out, 264, 1, "\x1f02A", color::ACCENT);
        }
        Screen::Aprs => {
            let (label, help) = match view.aprs_status {
                AprsStatus::Unconfigured => ("\x1f0B1", "\x1f0E9"),
                AprsStatus::Unsupported => ("\x1f09E", "\x1f078"),
                AprsStatus::NoFix => ("\x1f0B7", "\x1f06C"),
                AprsStatus::Ready => ("\x1f099", "\x1f029"),
                AprsStatus::Sending => ("\x1f0DE", "\x1f0D4"),
                AprsStatus::Sent => ("\x1f0DB", "\x1f077"),
                AprsStatus::Failed => ("\x1f0D8", "\x1f02B"),
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
                crate::controller::vfo_tx_frequency(&view)
            };
            let mut target = Text::new();
            let _ = write!(
                target,
                "{} TX {} \x1f09A",
                if view.active_slot == Slot::A {
                    "A"
                } else {
                    "B"
                },
                frequency(tx_frequency.unwrap_or(0)).as_str()
            );
            centered(row, out, 109, 1, "\x1f03E", color::MUTED);
            centered(row, out, 127, 1, target.as_str(), color::ACCENT);
            centered(row, out, 165, 2, label, color::TEXT);
            centered(row, out, 207, 1, help, color::MUTED);
            let (auto, ink) = if view.aprs_auto_armed {
                if view.aprs_auto_available {
                    ("\x1f189", color::GOOD)
                } else {
                    ("\x1f18A", color::WARNING)
                }
            } else if !view.aprs_auto_available {
                ("\x1f18B", color::MUTED)
            } else if view.can_arm_aprs_auto() {
                ("* \x1f188", color::ACCENT)
            } else {
                ("\x1f188", color::MUTED)
            };
            centered(row, out, 250, 1, auto, ink);
        }
        _ => {}
    }
}

fn aprs_rx_page(row: u16, out: &mut Row<'_>, view: &View) {
    let (label, ink) = if view.aprs_rx_available {
        ("\x1f184", color::GOOD)
    } else {
        ("\x1f185", color::MUTED)
    };
    centered(row, out, 69, 2, label, ink);

    if view.aprs_rx_count == 0 {
        centered(row, out, 162, 2, "\x1f186", color::MUTED);
        return;
    }
    rect(row, out, 12, 109, 216, 128, color::PANEL);
    if (124..131).contains(&row) {
        let mut position = Text::new();
        let _ = write!(
            position,
            "{} / {}",
            view.aprs_rx_index + 1,
            view.aprs_rx_count
        );
        centered(row, out, 124, 1, position.as_str(), color::MUTED);
    }
    if (145..159).contains(&row) {
        centered(
            row,
            out,
            145,
            2,
            view.aprs_rx_packet.source(),
            color::ACCENT,
        );
    }
    if (177..231).contains(&row) {
        let information = view.aprs_rx_packet.information();
        for (i, line) in information.as_bytes().chunks(16).enumerate() {
            text(
                row,
                out,
                20,
                177 + 20 * i as u16,
                2,
                core::str::from_utf8(line).unwrap_or(""),
                color::TEXT,
            );
        }
    }
    if view.aprs_rx_count > 1 {
        centered(row, out, 256, 1, "\x1f187", color::MUTED);
    }
}
