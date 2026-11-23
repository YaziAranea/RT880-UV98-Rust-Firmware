//! Incremental LCD damage planning. Row composition and transfers are separate.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DamagePlan {
    pub controller: DirtyRanges,
    pub radio: DirtyRanges,
    pub spectrum: DirtyRanges,
}

/// Validate a snapshot once, then plan each source of incremental damage.
/// The application may defer radio telemetry independently of navigation.
pub fn plan_damage(
    model: &Model<'_>,
    previous: Option<&View>,
    previous_radio: RadioView,
    previous_spectrum: Option<SpectrumView<'_>>,
) -> Result<DamagePlan, Error> {
    validate(model)?;
    Ok(DamagePlan {
        controller: controller_damage(model, previous),
        radio: radio_damage(model, previous_radio),
        spectrum: spectrum_damage(model, previous_spectrum),
    })
}

/// At most four sorted, disjoint half-open row intervals. This is a render
/// plan, not a pixel buffer: the caller can merge it into a pending row bitmap
/// and render short slices between input samples using the latest model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirtyRanges {
    ranges: [(u16, u16); 4],
    len: usize,
    home_outline_edges: u8,
}

impl DirtyRanges {
    pub fn ranges(&self) -> &[(u16, u16)] {
        &self.ranges[..self.len]
    }

    pub fn home_outline_edges(&self) -> u8 {
        self.home_outline_edges
    }

    pub(super) fn new() -> Self {
        Self {
            ranges: [(0, 0); 4],
            len: 0,
            home_outline_edges: 0,
        }
    }

    pub(super) fn push(&mut self, mut start: u16, mut end: u16) {
        if start >= end {
            return;
        }
        let mut index = 0;
        while index < self.len {
            let (a, b) = self.ranges[index];
            if start <= b && a <= end {
                start = start.min(a);
                end = end.max(b);
                self.ranges.copy_within(index + 1..self.len, index);
                self.len -= 1;
            } else {
                index += 1;
            }
        }
        // Five independent changes need not invalidate the whole display.
        // Merge the nearest pair; extra rows are harmless, dropped dirty rows
        // are not. This matters when RSSI, squelch and power change together.
        if self.len == self.ranges.len() {
            let mut expanded = [(0, 0); 5];
            let insert = self.ranges.partition_point(|&(a, _)| a < start);
            expanded[..insert].copy_from_slice(&self.ranges[..insert]);
            expanded[insert] = (start, end);
            expanded[insert + 1..].copy_from_slice(&self.ranges[insert..]);
            let mut closest = 0;
            for index in 1..4 {
                if expanded[index + 1].0 - expanded[index].1
                    < expanded[closest + 1].0 - expanded[closest].1
                {
                    closest = index;
                }
            }
            expanded[closest].1 = expanded[closest + 1].1;
            expanded.copy_within(closest + 2..5, closest + 1);
            self.ranges.copy_from_slice(&expanded[..4]);
            return;
        }
        let index = self.ranges[..self.len].partition_point(|&(a, _)| a < start);
        self.ranges.copy_within(index..self.len, index + 1);
        self.ranges[index] = (start, end);
        self.len += 1;
    }
}

/// Plan the same updates as `render_changed`, without accessing a display.
/// Passing None invalidates the whole frame, including external model fields.
pub fn dirty_ranges(model: &Model<'_>, previous: Option<View>) -> Result<DirtyRanges, Error> {
    validate(model)?;
    Ok(controller_damage(model, previous.as_ref()))
}

#[inline(never)]
fn controller_damage(model: &Model<'_>, previous: Option<&View>) -> DirtyRanges {
    let mut dirty = DirtyRanges::new();
    let Some(old) = previous else {
        dirty.push(0, HEIGHT);
        return dirty;
    };
    let new = &model.view;
    #[cfg(any(test, feature = "locale-all"))]
    if old.language() != new.language() {
        dirty.push(0, HEIGHT);
        return dirty;
    }
    if old.same_view(new) {
        return dirty;
    }
    // Account for shared chrome before any navigation early return. Its
    // unchanged brand, panel fills and separators need not cross SPI again.
    if old.locked != new.locked {
        dirty.push(0, 27);
        if new.screen != Screen::Home {
            dirty.push(296, HEIGHT);
        }
    }
    if old.notice != new.notice {
        if new.screen == Screen::Home {
            dirty.push(0, 27);
        } else {
            dirty.push(269, 295);
        }
    }
    if old.ptt_down != new.ptt_down && model.radio.state == RadioState::Off {
        dirty.push(
            radio_banner_rows(new.screen).0,
            radio_banner_rows(new.screen).1,
        );
    }
    if old.screen != new.screen || new.screen == Screen::Setting && old.setting != new.setting {
        if old.screen == Screen::Home || new.screen == Screen::Home {
            dirty.push(0, HEIGHT);
            return dirty;
        } else {
            dirty.push(11, 18);
        }
        dirty.push(48, 296); // All old body content, padding and notice band.
        dirty.push(305, 312); // Footer labels; its fill and separator are fixed.
                              // A confirmed spectrum snapshot overrides the ordinary radio banner.
        if matches!(old.screen, Screen::Spectrum | Screen::Shortwave)
            || matches!(new.screen, Screen::Spectrum | Screen::Shortwave)
        {
            dirty.push(28, 48);
        }
        return dirty;
    }
    if new.screen == Screen::Menu && old.menu_group != new.menu_group {
        dirty.push(11, 18);
        // Include the old occupied rows when entering a shorter category.
        // Padding belongs to the list too; clearing only new items leaves
        // stale entries below Scanning and Keys.
        let visible_rows = menu_entries(old.menu_group)
            .len()
            .max(menu_entries(new.menu_group).len())
            .min(LIST_ROWS);
        dirty.push(LIST_Y, LIST_Y + visible_rows as u16 * LIST_ROW_HEIGHT);
        dirty.push(283, 290);
        return dirty;
    }
    if old.mode != new.mode || old.active_slot != new.active_slot || old.zone != new.zone {
        if new.screen != Screen::Home {
            dirty.push(0, HEIGHT);
            return dirty;
        }
        if old.active_slot != new.active_slot {
            dirty.push(home_slot_rows(Slot::A).0, home_slot_rows(Slot::B).1);
        } else {
            let (start, end) = home_slot_rows(new.active_slot);
            dirty.push(start, end);
        }
    }
    match new.screen {
        Screen::Menu if menu_start(old.menu_index) == menu_start(new.menu_index) => {
            if old.menu_index == new.menu_index {
                return dirty;
            }
            for index in [old.menu_index, new.menu_index] {
                let (start, end) = menu_item_rows(index);
                dirty.push(start, end);
            }
            dirty.push(283, 290);
        }
        Screen::Channels => {
            if old.list_channel == new.list_channel {
                return dirty;
            }
            let old_row = model
                .list
                .iter()
                .position(|channel| Some(channel.index as u16) == old.list_channel);
            let new_row = model
                .list
                .iter()
                .position(|channel| Some(channel.index as u16) == new.list_channel);
            if let (Some(a), Some(b)) = (old_row, new_row) {
                for index in [a, b] {
                    let y = LIST_Y + index as u16 * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            } else {
                dirty.push(LIST_Y, LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT);
            }
        }
        Screen::Zones => {
            if old.zone_cursor == new.zone_cursor {
                return dirty;
            }
            let a = model
                .zones
                .iter()
                .position(|zone| zone.index == old.zone_cursor);
            let b = model
                .zones
                .iter()
                .position(|zone| zone.index == new.zone_cursor);
            if let (Some(a), Some(b)) = (a, b) {
                for index in [a, b] {
                    let y = LIST_Y + index as u16 * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            } else {
                dirty.push(LIST_Y, LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT);
            }
        }
        Screen::SideKeys | Screen::SideAction => {
            let (a, b) = if new.screen == Screen::SideKeys {
                (old.side_index, new.side_index)
            } else {
                (old.side_action_index, new.side_action_index)
            };
            if old.side_bindings != new.side_bindings {
                dirty.push(48, HEIGHT);
            } else if a != b {
                for index in [a, b] {
                    let y = LIST_Y + u16::from(index) * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            }
            if new.screen == Screen::SideAction && old.side_index != new.side_index {
                dirty.push(283, 290);
            }
        }
        Screen::Shortcuts | Screen::ShortcutAction => {
            let (a, b) = if new.screen == Screen::Shortcuts {
                (old.binding_index, new.binding_index)
            } else {
                (old.binding_action_index, new.binding_action_index)
            };
            if menu_start(a) != menu_start(b) || old.bindings != new.bindings {
                dirty.push(48, HEIGHT);
            } else if a != b {
                for index in [a, b] {
                    let y = LIST_Y + (usize::from(index) - menu_start(b)) as u16 * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
                dirty.push(283, 290);
            }
        }
        Screen::Home => {
            let top = home_slot_rows(new.active_slot).0;
            match (old.cb, new.cb) {
                (Some(old_cb), Some(new_cb))
                    if old_cb.number() != new_cb.number() || old_cb.grid != new_cb.grid =>
                {
                    let (a, _) = home_slot_rows(Slot::A);
                    dirty.push(a + 7, a + 14);
                    let (a, b) = home_frequency_rows(Slot::A);
                    dirty.push(a, b);
                }
                (Some(_), None) | (None, Some(_)) => {
                    let (a, b) = home_slot_rows(Slot::A);
                    dirty.push(a, b);
                }
                _ => {}
            }
            if old.scan != new.scan || old.dual_watch != new.dual_watch {
                dirty.push(0, 27);
            }
            if (old.reverse != new.reverse
                || old.shift_index != new.shift_index
                || old.tx_tone_index != new.tx_tone_index)
                && new.entry.is_none()
            {
                let (a, b) = home_frequency_rows(new.active_slot);
                dirty.push(a, b + 9);
                let (a, b) = home_ham_rows(new.active_slot);
                dirty.push(a, b);
            }
            if old.entry != new.entry {
                if let (Some(a), Some(b)) = (old.entry, new.entry) {
                    if a.kind == b.kind {
                        let (a, b) = home_frequency_rows(new.active_slot);
                        dirty.push(a, b);
                        return dirty;
                    }
                }
                let (a, b) = home_slot_rows(new.active_slot);
                dirty.push(a, b);
                return dirty;
            }
            if old.channel != new.channel {
                dirty.push(top + 7, top + 14);
                dirty.push(top + HOME_NAME_Y, top + HOME_NAME_Y + 14);
                let (a, b) = home_frequency_rows(new.active_slot);
                dirty.push(a, b + 9);
                let (a, b) = home_ham_rows(new.active_slot);
                dirty.push(a, b);
            }
            if old.vfo_10hz != new.vfo_10hz && new.mode == Mode::Vfo && new.entry.is_none() {
                let (a, b) = home_frequency_rows(new.active_slot);
                dirty.push(
                    a,
                    if new.shift_index != 0 || new.tx_tone_index != 0 {
                        b + 9
                    } else {
                        b
                    },
                );
            }
            if old.step_index != new.step_index {
                if new.mode == Mode::Vfo && new.entry.is_none() {
                    dirty.push(top + HOME_META_Y, top + HOME_META_Y + 7);
                }
            }
        }
        Screen::Step => {
            if old.step_index != new.step_index {
                for index in [old.step_index, new.step_index] {
                    let y = 62 + u16::from(index) * 33;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            }
        }
        Screen::AprsRx => {
            if old.aprs_rx_available != new.aprs_rx_available {
                dirty.push(69, 83);
            }
            if old.aprs_rx_count != new.aprs_rx_count
                || old.aprs_rx_index != new.aprs_rx_index
                || old.aprs_rx_packet != new.aprs_rx_packet
            {
                dirty.push(109, 263);
            }
        }
        Screen::Gps | Screen::Aprs => dirty.push(48, 296),
        Screen::Cb => {
            if old.cb_draft != new.cb_draft || old.entry != new.entry {
                dirty.push(54, 70);
            }
            if old.cb_draft.map(|cb| cb.am) != new.cb_draft.map(|cb| cb.am) {
                dirty.push(84, 91);
            }
            if old.entry.is_some() != new.entry.is_some() {
                dirty.push(305, 312);
            }
            let old_number = old.cb_draft.map(|cb| cb.number());
            let new_number = new.cb_draft.map(|cb| cb.number());
            if old_number != new_number {
                for number in [old_number, new_number].into_iter().flatten() {
                    let y = cb_channel_rows(number).0;
                    dirty.push(y, y + CB_CHANNEL_HEIGHT);
                }
            }
            if old.cb.is_some() != new.cb.is_some() {
                dirty.push(94, 101);
            }
        }
        Screen::RadioName => {
            if old.radio_name_draft != new.radio_name_draft {
                dirty.push(96, 110);
            }
        }
        Screen::Mdc => {
            if mdc_end_replaced_by_name(old) != mdc_end_replaced_by_name(new) {
                dirty.push(MDC_NAME_HINT_ROW, MDC_NAME_HINT_ROW + 7);
            }
            if old.mdc_draft.is_some() != new.mdc_draft.is_some() {
                dirty.push(48, 269);
                dirty.push(305, 312);
            } else if old.mdc_draft != new.mdc_draft {
                dirty.push(75, 119);
            } else if old.mdc_option != new.mdc_option {
                for cursor in [old.mdc_option, new.mdc_option] {
                    let top = mdc_option_top(cursor);
                    dirty.push(top, top + 37);
                }
            }
        }
        Screen::Shortwave => {
            if old.shortwave_scope != new.shortwave_scope {
                dirty.push(48, 296);
            }
            if new.shortwave_scope
                && (old.shortwave_khz != new.shortwave_khz
                    || old.shortwave_fraction_hz != new.shortwave_fraction_hz
                    || old.shortwave_mode != new.shortwave_mode
                    || old.shortwave_bfo != new.shortwave_bfo
                    || old.shortwave_scope_points != new.shortwave_scope_points
                    || old.shortwave_band != new.shortwave_band)
            {
                dirty.push(48, 296);
            }
            if old.shortwave_filter != new.shortwave_filter
                || old.shortwave_soft_mute != new.shortwave_soft_mute
            {
                dirty.push(234, 296);
            }
            if old.shortwave_band_draft != new.shortwave_band_draft {
                dirty.push(48, 296);
                dirty.push(305, 312);
            }
            if old.shortwave_mode != new.shortwave_mode || old.shortwave_band != new.shortwave_band
            {
                dirty.push(63, 70);
                dirty.push(155, 175);
            }
            if old.shortwave_khz != new.shortwave_khz
                || old.shortwave_fraction_hz != new.shortwave_fraction_hz
                || old.shortwave_entry != new.shortwave_entry
            {
                dirty.push(78, 165);
            }
            if old.shortwave_step != new.shortwave_step || old.shortwave_scan != new.shortwave_scan
            {
                dirty.push(155, 165);
            }
            if old.shortwave_bfo != new.shortwave_bfo {
                dirty.push(167, 175);
            }
            if old.shortwave_status != new.shortwave_status
                || old.shortwave_scan != new.shortwave_scan
            {
                dirty.push(184, 224);
            }
        }
        Screen::Crossband => {
            if old.crossband_probe != new.crossband_probe {
                dirty.push(62, 226);
            }
        }
        Screen::RxToneSearch => {
            if old.rx_tone_search != new.rx_tone_search {
                dirty.push(92, 230);
                dirty.push(305, 313);
            }
        }
        Screen::DtmfMemory => {
            if old.dtmf_memory_page != new.dtmf_memory_page {
                dirty.push(70, 235);
                dirty.push(305, 313);
            }
        }
        Screen::Spectrum => {
            if old.spectrum_listening != new.spectrum_listening
                || old.spectrum_pending_listen != new.spectrum_pending_listen
            {
                dirty.push(28, 48);
                dirty.push(269, 320);
            }
            if old.spectrum_cursor != new.spectrum_cursor {
                dirty.push(48, 69);
                dirty.push(176, 183);
                dirty.push(255, 262);
            }
            if old.spectrum_trigger != new.spectrum_trigger
                || old.spectrum_auto_listen != new.spectrum_auto_listen
            {
                dirty.push(264, 271);
            }
            if old.spectrum_menu != new.spectrum_menu {
                // Opening/closing also hides or reveals the marker, the cursor
                // readout and the points/step line below the graph.
                dirty.push(28, 48);
                dirty.push(94, 320);
            } else if old.spectrum_option != new.spectrum_option {
                for option in [old.spectrum_option, new.spectrum_option] {
                    let y = GRAPH_TOP + u16::from(option) * 17;
                    dirty.push(y, y + 15);
                }
            }
            if old.spectrum_span_index != new.spectrum_span_index
                || old.spectrum_step_index != new.spectrum_step_index
            {
                dirty.push(28, 320);
            }
        }
        Screen::Setting => {
            if matches!(new.setting, Some(Setting::TopKey | Setting::BottomKey))
                && old.top_draft != new.top_draft
            {
                dirty.push(62, 69);
            }
            if old.setting_index != new.setting_index {
                let setting = new.setting.unwrap();
                let (start, pitch, height) = setting_geometry(setting);
                let first = if setting.choice_count() > LIST_ROWS as u8 {
                    menu_start(new.setting_index)
                } else {
                    0
                };
                if setting.choice_count() > LIST_ROWS as u8
                    && first != menu_start(old.setting_index)
                {
                    dirty.push(start, start + pitch * LIST_ROWS as u16);
                } else {
                    for index in [old.setting_index, new.setting_index] {
                        let y = start + (usize::from(index) - first) as u16 * pitch;
                        dirty.push(y, y + height);
                    }
                }
            }
            if old.reverse != new.reverse
                || old.scan != new.scan
                || old.display_light != new.display_light
                || old.key_light != new.key_light
                || new.setting == Some(Setting::Shift)
                    && (old.shift_index != new.shift_index
                        || shift_tx_preview(old) != shift_tx_preview(new))
            {
                dirty.push(84, 91);
            }
        }
        Screen::ChannelDetails => {
            if old.channel != new.channel || old.reverse != new.reverse {
                dirty.push(48, 295);
            }
        }
        Screen::Keys => dirty.push(52, 270),
        Screen::Calibration | Screen::Help | Screen::About => {}
        // Menu window changed. Its header and footer remain unchanged.
        Screen::Menu => {
            dirty.push(LIST_Y, LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT);
            dirty.push(283, 290);
        }
    }
    dirty
}

/// Metadata changes use much smaller regions than screen navigation. In
/// particular a live RSSI sample does not invalidate the frequency or footer.
pub fn radio_dirty_ranges(model: &Model<'_>, previous: RadioView) -> Result<DirtyRanges, Error> {
    validate(model)?;
    Ok(radio_damage(model, previous))
}

#[inline(never)]
fn radio_damage(model: &Model<'_>, previous: RadioView) -> DirtyRanges {
    let new = model.radio;
    let mut dirty = DirtyRanges::new();
    if let Some((start, end)) = new.signaling_popup_damage(previous) {
        dirty.push(start, end);
    }
    let (banner_start, banner_end) = radio_banner_rows(model.view.screen);
    let view = &model.view;
    if view.screen == Screen::Home {
        let marker_changed = (previous.tx_trial != new.tx_trial && !new.monitor)
            || (previous.monitor != new.monitor && !view.locked);
        if previous.state != new.state || marker_changed {
            dirty.push(banner_start, banner_end);
        }
        for slot in [Slot::A, Slot::B] {
            // The badge, frame and paired frequency all change together.
            if (previous.transmitting_slot() == Some(slot))
                != (new.transmitting_slot() == Some(slot))
            {
                let (start, end) = home_slot_rows(slot);
                dirty.push(start, end);
                continue;
            }
            if (previous.receiving_slot() == Some(slot)) != (new.receiving_slot() == Some(slot)) {
                dirty.home_outline_edges |= if slot == Slot::A { 0x0f } else { 0xf0 };
            }
            if view.active_slot == slot
                && view.entry.is_some()
                && new.transmitting_slot() != Some(slot)
            {
                continue;
            }
            let old_meter = home_meter(view, previous, slot);
            let new_meter = home_meter(view, new, slot);
            if old_meter != new_meter {
                let (start, end) = home_meter_rows(slot);
                let only_fill_changed = matches!(
                    (old_meter, new_meter),
                    (
                        MeterReading::Receive(Some(_)),
                        MeterReading::Receive(Some(_))
                    ) | (
                        MeterReading::Transmit(Some(_)),
                        MeterReading::Transmit(Some(_))
                    )
                );
                // A changed live sample needs only the seven bar rows.
                dirty.push(start, if only_fill_changed { start + 7 } else { end });
            }
            let old_live = home_radio_metadata(view, previous, slot);
            let new_live = home_radio_metadata(view, new, slot);
            if old_live != new_live
                || (new_live
                    && (previous.modulation != new.modulation || previous.power != new.power))
            {
                let (start, end) = home_ham_rows(slot);
                dirty.push(start, end);
            }
        }
        return dirty;
    }
    if previous.state != new.state {
        dirty.push(banner_start, banner_end);
        match view.screen {
            Screen::Keys => dirty.push(211, 218),
            Screen::Setting
                if setting_uses_radio(view.setting) || view.setting == Some(Setting::Shift) =>
            {
                dirty.push(84, 91);
            }
            _ => {}
        }
        return dirty;
    }
    if view.screen == Screen::Setting {
        let changed = match view.setting {
            Some(Setting::Power) => previous.power != new.power,
            Some(Setting::Monitor) => previous.monitor != new.monitor,
            Some(Setting::Modulation) => previous.modulation != new.modulation,
            Some(Setting::Shift) => previous.vfo_am_override != new.vfo_am_override,
            _ => false,
        };
        if changed {
            dirty.push(84, 91);
        }
    }
    dirty
}

/// Damage from replacing one stable spectrum snapshot. Cursor-only movement
/// is handled by dirty_ranges and does not repaint the 114-row graph.
pub fn spectrum_dirty_ranges(
    model: &Model<'_>,
    previous: Option<SpectrumView<'_>>,
) -> Result<DirtyRanges, Error> {
    validate(model)?;
    Ok(spectrum_damage(model, previous))
}

#[inline(never)]
fn spectrum_damage(model: &Model<'_>, previous: Option<SpectrumView<'_>>) -> DirtyRanges {
    let mut dirty = DirtyRanges::new();
    if model.view.screen != Screen::Spectrum {
        return dirty;
    }
    match (previous, model.spectrum) {
        (Some(old), Some(new))
            if old.start_10hz == new.start_10hz
                && old.step_10hz == new.step_10hz
                && old.len == new.len =>
        {
            if old.plot != new.plot {
                dirty.push(94, 176);
                dirty.push(WATERFALL_TOP, 262);
                if old.plot.held_bin != new.plot.held_bin {
                    dirty.push(274, 281);
                }
            }
            if old.state != new.state {
                dirty.push(28, 48);
                dirty.push(269, 320);
            }
        }
        (None, None) => {}
        _ => dirty.push(28, 296),
    }
    dirty
}
