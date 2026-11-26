//! Fixture-free inputs for the controller and portrait firmware renderer.
use std::{fs, path::Path};
use uv98_display::Display;
use uv98_handheld_ui::controller::{
    menu_entries, menu_location, Controller, MenuEntry, MenuItem, Mode, ScanRange, ScanRangeError,
    ScanRangeField, Screen, Slot, STEP_10HZ,
};
use uv98_handheld_ui::input::InputEvent;
use uv98_handheld_ui::locale::Language;
use uv98_handheld_ui::render::{
    render, Model, RadioState, RadioView, SpectrumError, HEIGHT, WIDTH,
};
use uv98_keys::Key::{self, *};

use super::{Panel, SimDelay};

const EMPTY_CHANNELS: [u8; 128] = [0; 128];

pub(super) fn run(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir(output)?;
    for (code, language) in [
        ("ru", Language::Russian),
        ("en", Language::English),
        ("de", Language::German),
    ] {
        let directory = output.join(code);
        fs::create_dir(&directory)?;
        let mut controller = Controller::new(&EMPTY_CHANNELS, 14_550_000);
        controller.set_preview_language(language);
        range(&mut controller, &directory)?;
        journal(&mut controller, &directory)?;
    }
    Ok(())
}

fn range(controller: &mut Controller, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    highlight(controller, MenuItem::ScanRange);
    capture(controller, output, "01-scanning-menu.png")?;
    press(controller, &[Menu]);
    assert_eq!(controller.view().screen, Screen::ScanRange);
    assert!(controller.scan_range().is_none());
    capture(controller, output, "02-range-auto.png")?;

    press(controller, &[Menu, Down]);
    enter_frequency(controller, b"145500");
    press(controller, &[Down]);
    enter_frequency(controller, b"145526");
    press(controller, &[Down]);
    assert_eq!(
        controller.view().scan_range_page.unwrap().field,
        ScanRangeField::Step
    );
    for _ in STEP_10HZ {
        if controller.view().scan_range_page.unwrap().step_10hz == 625 {
            break;
        }
        press(controller, &[Menu]);
    }
    assert_eq!(controller.view().scan_range_page.unwrap().step_10hz, 625);

    press(controller, &[Down, Menu]);
    assert_eq!(
        controller.view().scan_range_page.unwrap().error,
        Some(ScanRangeError::Grid)
    );
    assert!(controller.scan_range().is_none());
    capture(controller, output, "03-range-grid-error.png")?;

    press(controller, &[Up, Up]);
    enter_frequency(controller, b"145525");
    press(controller, &[Down, Down]);
    capture(controller, output, "04-range-custom.png")?;
    press(controller, &[Menu]);
    assert_eq!(controller.view().screen, Screen::Menu);
    assert_eq!(
        controller.scan_range(),
        Some(ScanRange {
            lower_10hz: 14_550_000,
            upper_10hz: 14_552_500,
            step_10hz: 625,
        })
    );
    Ok(())
}

fn journal(controller: &mut Controller, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let first = 14_550_625;
    let second = 14_551_250;
    highlight(controller, MenuItem::ScanJournal);
    press(controller, &[Menu]);
    assert_eq!(controller.view().screen, Screen::ScanJournal);
    assert!(controller.view().scan_journal_page.unwrap().entry.is_none());
    capture(controller, output, "05-journal-empty.png")?;

    for (frequency, raw_rssi, opened, now_ms) in [
        (first, 190, true, 12_000),
        (second, 202, true, 43_000),
        (first, 211, true, 65_000),
        (first, 217, false, 70_000),
    ] {
        controller.record_scan_find(frequency, raw_rssi, opened, now_ms);
    }
    // Open again to start at the most recent entry, independent of selection
    // preservation while new observations arrive on the open journal page.
    highlight(controller, MenuItem::ScanJournal);
    press(controller, &[Menu]);
    let page = controller.view().scan_journal_page.unwrap();
    let entry = page.entry.unwrap();
    assert_eq!(
        (
            page.count,
            entry.frequency_10hz,
            entry.peak_raw_rssi,
            entry.detections,
            entry.last_opened_s,
        ),
        (2, first, 217, 2, 65)
    );
    capture(controller, output, "06-journal-hit.png")?;

    press(controller, &[Down]);
    assert_eq!(
        controller
            .view()
            .scan_journal_page
            .unwrap()
            .entry
            .unwrap()
            .frequency_10hz,
        second
    );
    press(controller, &[Up, Hash]);
    assert!(controller.scan_frequency_ignored(first));
    capture(controller, output, "07-journal-ignored.png")?;
    press(controller, &[Hash, Menu, Down]);
    assert!(!controller.scan_frequency_ignored(first));
    assert_eq!(
        controller.view().scan_journal_page.unwrap().target,
        Some(Slot::B)
    );
    capture(controller, output, "08-journal-vfo-b.png")?;

    let old_a = controller.context(Slot::A);
    press(controller, &[Menu]);
    let view = controller.view();
    assert_eq!(
        (view.screen, view.active_slot, view.mode, view.vfo_10hz),
        (Screen::Home, Slot::B, Mode::Vfo, first)
    );
    assert_eq!(controller.context(Slot::A), old_a);
    Ok(())
}

fn highlight(controller: &mut Controller, item: MenuItem) {
    controller.handle(InputEvent::Hold(Exit), &EMPTY_CHANNELS);
    assert_eq!(controller.view().screen, Screen::Home);
    press(controller, &[Menu]);
    let (group, index) = menu_location(item);
    let group = group.expect("scan item belongs to a menu group");
    for _ in menu_entries(None) {
        if menu_entries(None)[usize::from(controller.view().menu_index)] == MenuEntry::Group(group)
        {
            break;
        }
        press(controller, &[Down]);
    }
    press(controller, &[Menu]);
    assert_eq!(controller.view().menu_group, Some(group));
    for _ in menu_entries(Some(group)) {
        if controller.view().menu_index == index {
            break;
        }
        press(controller, &[Down]);
    }
    assert_eq!(
        menu_entries(Some(group))[usize::from(controller.view().menu_index)],
        MenuEntry::Item(item)
    );
}

fn press(controller: &mut Controller, keys: &[Key]) {
    for &key in keys {
        let change = controller.handle(InputEvent::Short(key), &EMPTY_CHANNELS);
        assert!(!change.ptt_tx_requested);
        assert!(
            change.request.is_none(),
            "unexpected RF request: {change:?}"
        );
    }
}

fn enter_frequency(controller: &mut Controller, digits: &[u8; 6]) {
    const DIGITS: [Key; 10] = [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ];
    for &digit in digits {
        press(controller, &[DIGITS[usize::from(digit - b'0')]]);
    }
    press(controller, &[Menu]);
}

fn capture(
    controller: &Controller,
    output: &Path,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut display = Display::new_portrait(
        Panel::new(),
        SimDelay {
            milliseconds: Default::default(),
        },
    );
    display
        .initialize()
        .expect("initialize simulated portrait LCD");
    let model = Model {
        view: controller.view(),
        channel: None,
        list: &[],
        calibration_matches: None,
        last_key: None,
        ptt_pressed: false,
        input_ready: true,
        radio: RadioView {
            state: RadioState::Receiving,
            ..RadioView::OFF
        },
        manual_tx: None,
        battery_mv: None,
        zone_name: None,
        zones: &[],
        peer: None,
        spectrum: None,
        shortwave_scope: None,
        spectrum_error: SpectrumError::None,
        lower_side_mapped: false,
    };
    render(&mut display, &model).expect("render fixture-free handheld screen");
    let (panel, _) = display.into_parts();
    assert_eq!((panel.madctl, panel.madctl_writes), (Some(0x08), 1));
    assert_eq!(panel.pixel_words, 2 * WIDTH * usize::from(HEIGHT));
    panel.save_png(&output.join(filename))
}
