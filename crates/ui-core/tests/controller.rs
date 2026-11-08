use core::cell::Cell;
use uv98_codeplug::{channels::CHANNEL_LEN, CHANNELS_LEN};
use uv98_keys::{ButtonEvent, Key};
use uv98_ui_core::{
    action_for_key, Action, ChannelCatalog, CodeplugChannels, Controller, Direction, Effect,
    MenuItem, NavigationPolicy, NumberEntry, Screen, UiError,
};

struct Catalog<'a> {
    active: &'a [Option<bool>],
    lookups: Cell<usize>,
}

impl<'a> Catalog<'a> {
    fn new(active: &'a [Option<bool>]) -> Self {
        Self {
            active,
            lookups: Cell::new(0),
        }
    }
}

impl ChannelCatalog for Catalog<'_> {
    fn len(&self) -> usize {
        self.active.len()
    }
    fn is_active(&self, index: usize) -> Option<bool> {
        self.lookups.set(self.lookups.get() + 1);
        self.active.get(index).copied().flatten()
    }
}

fn policy(wrap: bool, include_inactive: bool) -> NavigationPolicy {
    NavigationPolicy {
        wrap,
        include_inactive,
    }
}

fn input_number(controller: &mut Controller, digits: &[u8], catalog: &impl ChannelCatalog) {
    for &digit in digits {
        assert_eq!(
            controller
                .dispatch(Action::Digit(digit), catalog)
                .unwrap()
                .effect,
            None
        );
    }
}

#[test]
fn constructor_rejects_empty_oversize_and_invalid_initial_selections() {
    let empty = Catalog::new(&[]);
    assert_eq!(
        Controller::new(0, policy(false, true), &empty),
        Err(UiError::InvalidCatalogLength { actual: 0 })
    );
    let large = [Some(true); 1025];
    assert_eq!(
        Controller::new(0, policy(false, true), &Catalog::new(&large)),
        Err(UiError::InvalidCatalogLength { actual: 1025 })
    );
    let catalog = Catalog::new(&[Some(true), Some(false), None]);
    assert_eq!(
        Controller::new(3, policy(false, true), &catalog),
        Err(UiError::ChannelOutOfRange { index: 3 })
    );
    assert_eq!(
        Controller::new(1, policy(false, false), &catalog),
        Err(UiError::InactiveChannel { index: 1 })
    );
    assert_eq!(
        Controller::new(2, policy(false, false), &catalog),
        Err(UiError::ChannelUnavailable { index: 2 })
    );
}

#[test]
fn navigation_is_explicitly_bounded_and_wrap_is_optional() {
    let catalog = Catalog::new(&[Some(true); 3]);
    let mut bounded = Controller::new(0, policy(false, true), &catalog).unwrap();
    assert_eq!(
        bounded.dispatch(Action::Previous, &catalog).unwrap().effect,
        None
    );
    assert_eq!(
        bounded.dispatch(Action::Next, &catalog).unwrap().effect,
        Some(Effect::SelectChannel { index: 1 })
    );
    bounded.dispatch(Action::Next, &catalog).unwrap();
    assert_eq!(
        bounded.dispatch(Action::Next, &catalog).unwrap().effect,
        None
    );
    assert_eq!(bounded.view().channel_index, 2);
    let mut wrapped = Controller::new(0, policy(true, true), &catalog).unwrap();
    assert_eq!(
        wrapped.dispatch(Action::Previous, &catalog).unwrap().effect,
        Some(Effect::SelectChannel { index: 2 })
    );
    assert_eq!(
        wrapped.dispatch(Action::Next, &catalog).unwrap().effect,
        Some(Effect::SelectChannel { index: 0 })
    );
    // Including inactive entries avoids unnecessary storage metadata reads.
    assert_eq!(catalog.lookups.get(), 0);
}

#[test]
fn inactive_entries_are_skipped_but_unavailable_metadata_is_an_error() {
    let catalog = Catalog::new(&[Some(true), Some(false), Some(true), None]);
    let mut controller = Controller::new(0, policy(true, false), &catalog).unwrap();
    assert_eq!(
        controller.dispatch(Action::Next, &catalog).unwrap().effect,
        Some(Effect::SelectChannel { index: 2 })
    );
    input_number(&mut controller, &[1], &catalog);
    let before = controller;
    assert_eq!(
        controller.dispatch(Action::Next, &catalog),
        Err(UiError::ChannelUnavailable { index: 3 })
    );
    assert_eq!(controller, before); // Entry also survives the failed transition.
    assert_eq!(
        controller
            .dispatch(Action::Previous, &catalog)
            .unwrap()
            .effect,
        Some(Effect::SelectChannel { index: 0 })
    );
    assert_eq!(controller.view().entry, None);
}

#[test]
fn no_other_active_channel_and_single_channel_do_not_loop() {
    let catalog = Catalog::new(&[Some(true), Some(false), Some(false)]);
    let mut controller = Controller::new(0, policy(true, false), &catalog).unwrap();
    catalog.lookups.set(0);
    assert_eq!(
        controller.dispatch(Action::Next, &catalog).unwrap().effect,
        None
    );
    assert_eq!(catalog.lookups.get(), 2);
    assert_eq!(controller.view().channel_index, 0);
    let single = Catalog::new(&[Some(true)]);
    let mut controller = Controller::new(0, policy(true, false), &single).unwrap();
    single.lookups.set(0);
    assert_eq!(
        controller
            .dispatch(Action::Previous, &single)
            .unwrap()
            .effect,
        None
    );
    assert_eq!(single.lookups.get(), 0);
}

#[test]
fn numeric_entry_is_one_based_and_never_commits_before_confirm() {
    let active = [Some(true); 1024];
    let catalog = Catalog::new(&active);
    let mut controller = Controller::new(0, policy(false, true), &catalog).unwrap();
    input_number(&mut controller, &[1, 0, 2, 4], &catalog);
    assert_eq!(controller.view().channel_index, 0);
    assert_eq!(
        controller.view().entry,
        Some(NumberEntry {
            value: 1024,
            digits: 4
        })
    );
    assert_eq!(
        controller
            .dispatch(Action::Confirm, &catalog)
            .unwrap()
            .effect,
        Some(Effect::SelectChannel { index: 1023 })
    );
    assert_eq!(controller.view().entry, None);
    input_number(&mut controller, &[0, 0, 0, 1], &catalog);
    assert_eq!(
        controller.view().entry,
        Some(NumberEntry {
            value: 1,
            digits: 4
        })
    );
    assert_eq!(
        controller
            .dispatch(Action::Confirm, &catalog)
            .unwrap()
            .effect,
        Some(Effect::SelectChannel { index: 0 })
    );
}

#[test]
fn invalid_numeric_entry_is_preserved_until_cancelled() {
    let catalog = Catalog::new(&[Some(true), Some(false)]);
    let mut controller = Controller::new(0, policy(false, false), &catalog).unwrap();
    for (digits, expected) in [
        (&[0][..], UiError::InvalidChannelNumber { number: 0 }),
        (&[3][..], UiError::InvalidChannelNumber { number: 3 }),
        (&[2][..], UiError::InactiveChannel { index: 1 }),
    ] {
        input_number(&mut controller, digits, &catalog);
        let before = controller;
        assert_eq!(
            controller.dispatch(Action::Confirm, &catalog),
            Err(expected)
        );
        assert_eq!(controller, before);
        controller.dispatch(Action::Back, &catalog).unwrap();
        assert_eq!(controller.view().entry, None);
        assert_eq!(controller.view().screen, Screen::Channel);
    }
    input_number(&mut controller, &[9, 9, 9, 9], &catalog);
    let before = controller;
    assert_eq!(
        controller.dispatch(Action::Digit(9), &catalog),
        Err(UiError::EntryTooLong)
    );
    assert_eq!(
        controller.dispatch(Action::Digit(10), &catalog),
        Err(UiError::InvalidDigit { value: 10 })
    );
    assert_eq!(controller, before);
}

#[test]
fn menu_round_trips_preserve_channel_and_use_stable_items() {
    let catalog = Catalog::new(&[Some(true); 3]);
    let mut controller = Controller::new(1, policy(false, true), &catalog).unwrap();
    assert_eq!(
        controller.menu_items(),
        &[MenuItem::Spectrum, MenuItem::Back]
    );
    controller.dispatch(Action::Confirm, &catalog).unwrap();
    assert_eq!(controller.view().screen, Screen::Menu);
    assert_eq!(controller.view().menu_item, MenuItem::Spectrum);
    controller.dispatch(Action::Previous, &catalog).unwrap();
    assert_eq!(controller.view().menu_item, MenuItem::Back);
    assert_eq!(
        controller
            .dispatch(Action::Confirm, &catalog)
            .unwrap()
            .effect,
        None
    );
    assert_eq!(controller.view().screen, Screen::Channel);
    assert_eq!(controller.view().channel_index, 1);
    controller.dispatch(Action::Confirm, &catalog).unwrap();
    assert_eq!(
        controller
            .dispatch(Action::Confirm, &catalog)
            .unwrap()
            .effect,
        Some(Effect::OpenSpectrum)
    );
    assert_eq!(controller.view().screen, Screen::Spectrum);
    assert!(controller.view().spectrum_open);
    assert_eq!(
        controller.menu_items(),
        &[MenuItem::ResetPeaks, MenuItem::Back]
    );
}

#[test]
fn spectrum_cursor_effects_do_not_select_channels_or_invent_bounds() {
    let catalog = Catalog::new(&[Some(true); 2]);
    let mut controller = Controller::new(1, policy(false, true), &catalog).unwrap();
    controller.dispatch(Action::OpenSpectrum, &catalog).unwrap();
    for (action, direction) in [
        (Action::Previous, Direction::Previous),
        (Action::Next, Direction::Next),
    ] {
        let result = controller.dispatch(action, &catalog).unwrap();
        assert_eq!(
            result.effect,
            Some(Effect::MoveSpectrumCursor { direction })
        );
        assert!(!result.view_changed); // Cursor belongs to the spectrum adapter.
        assert_eq!(controller.view().channel_index, 1);
    }
    assert_eq!(
        controller
            .dispatch(Action::OpenSpectrum, &catalog)
            .unwrap()
            .effect,
        None
    );
    let before = controller;
    assert_eq!(
        controller.dispatch(Action::Digit(4), &catalog),
        Err(UiError::ActionUnavailable)
    );
    assert_eq!(controller, before);
}

#[test]
fn spectrum_context_menu_resets_peaks_or_returns_without_restarting() {
    let catalog = Catalog::new(&[Some(true)]);
    let mut controller = Controller::new(0, policy(false, true), &catalog).unwrap();
    controller.dispatch(Action::OpenSpectrum, &catalog).unwrap();
    controller.dispatch(Action::Confirm, &catalog).unwrap();
    assert!(controller.view().spectrum_open);
    assert_eq!(controller.view().menu_item, MenuItem::ResetPeaks);
    assert_eq!(
        controller
            .dispatch(Action::Confirm, &catalog)
            .unwrap()
            .effect,
        Some(Effect::ResetPeaks)
    );
    assert_eq!(controller.view().screen, Screen::Spectrum);
    controller.dispatch(Action::Confirm, &catalog).unwrap();
    controller.dispatch(Action::Next, &catalog).unwrap();
    assert_eq!(controller.view().menu_item, MenuItem::Back);
    assert_eq!(
        controller
            .dispatch(Action::Confirm, &catalog)
            .unwrap()
            .effect,
        None
    );
    assert_eq!(controller.view().screen, Screen::Spectrum);
    assert_eq!(
        controller.dispatch(Action::Back, &catalog).unwrap().effect,
        Some(Effect::CloseSpectrum)
    );
    assert_eq!(controller.view().screen, Screen::Channel);
    assert!(!controller.view().spectrum_open);
    assert_eq!(
        controller
            .dispatch(Action::CloseSpectrum, &catalog)
            .unwrap()
            .effect,
        None
    );
    assert_eq!(
        controller.dispatch(Action::ResetPeaks, &catalog),
        Err(UiError::ActionUnavailable)
    );
}

#[test]
fn ptt_cancels_spectrum_and_blocks_reopen_until_release() {
    let catalog = Catalog::new(&[Some(true)]);
    let mut controller = Controller::new(0, policy(false, true), &catalog).unwrap();
    controller.dispatch(Action::OpenSpectrum, &catalog).unwrap();
    controller.dispatch(Action::Confirm, &catalog).unwrap(); // Cancel from context menu too.
    assert_eq!(
        controller
            .on_key(Key::Ptt, ButtonEvent::Pressed, &catalog)
            .unwrap()
            .effect,
        Some(Effect::CloseSpectrum)
    );
    assert_eq!(controller.view().screen, Screen::Channel);
    assert!(controller.view().ptt_pressed);
    assert_eq!(
        controller.dispatch(Action::OpenSpectrum, &catalog),
        Err(UiError::PttHeld)
    );
    assert_eq!(
        controller
            .on_key(Key::Ptt, ButtonEvent::Held, &catalog)
            .unwrap()
            .effect,
        None
    );
    assert_eq!(
        controller
            .on_key(Key::Ptt, ButtonEvent::Released, &catalog)
            .unwrap()
            .effect,
        None
    );
    assert!(!controller.view().spectrum_open);
    assert!(!controller.view().ptt_pressed);
    assert_eq!(
        controller
            .dispatch(Action::OpenSpectrum, &catalog)
            .unwrap()
            .effect,
        Some(Effect::OpenSpectrum)
    );
}

#[test]
fn storage_changes_fail_navigation_but_cannot_prevent_cancel() {
    let catalog = Catalog::new(&[Some(true); 2]);
    let invalid = Catalog::new(&[]);
    let mut controller = Controller::new(0, policy(false, true), &catalog).unwrap();
    input_number(&mut controller, &[2], &catalog);
    let before = controller;
    assert_eq!(
        controller.dispatch(Action::Next, &invalid),
        Err(UiError::CatalogSizeChanged {
            expected: 2,
            actual: 0
        })
    );
    assert_eq!(controller, before);
    assert_eq!(
        controller.dispatch(Action::Confirm, &invalid),
        Err(UiError::CatalogSizeChanged {
            expected: 2,
            actual: 0
        })
    );
    assert_eq!(controller, before);
    controller.dispatch(Action::OpenSpectrum, &invalid).unwrap();
    assert_eq!(
        controller
            .on_key(Key::Ptt, ButtonEvent::Pressed, &invalid)
            .unwrap()
            .effect,
        Some(Effect::CloseSpectrum)
    );
    assert_eq!(invalid.lookups.get(), 0);
}

#[test]
fn key_mapping_has_no_implicit_repeat_or_auxiliary_gestures() {
    let keys = [
        Key::Digit0,
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
    ];
    for (digit, key) in keys.into_iter().enumerate() {
        assert_eq!(
            action_for_key(key, ButtonEvent::Pressed),
            Some(Action::Digit(digit as u8))
        );
        assert_eq!(action_for_key(key, ButtonEvent::Held), None);
        assert_eq!(action_for_key(key, ButtonEvent::Released), None);
    }
    for (key, action) in [
        (Key::Up, Action::Previous),
        (Key::Down, Action::Next),
        (Key::Menu, Action::Confirm),
        (Key::Exit, Action::Back),
    ] {
        assert_eq!(action_for_key(key, ButtonEvent::Pressed), Some(action));
        assert_eq!(action_for_key(key, ButtonEvent::Held), None);
        assert_eq!(action_for_key(key, ButtonEvent::Released), None);
    }
    for key in [Key::Star, Key::Hash, Key::Aux16, Key::Aux17, Key::Aux18] {
        for event in [
            ButtonEvent::Pressed,
            ButtonEvent::Held,
            ButtonEvent::Released,
        ] {
            assert_eq!(action_for_key(key, event), None);
        }
    }
}

#[test]
fn codeplug_adapter_uses_exact_marker_and_preserves_bytes() {
    let mut bytes = vec![0xa5; CHANNELS_LEN];
    bytes[2] = 1;
    bytes[CHANNEL_LEN + 2] = 2; // Unknown marker must not become active.
    bytes[2 * CHANNEL_LEN + 2] = 0;
    let before = bytes.clone();
    let catalog = CodeplugChannels::new(&bytes).unwrap();
    assert_eq!(catalog.len(), 1024);
    assert_eq!(catalog.is_active(0), Some(true));
    assert_eq!(catalog.is_active(1), Some(false));
    assert_eq!(catalog.is_active(2), Some(false));
    assert_eq!(catalog.is_active(1024), None);
    let mut controller = Controller::new(0, policy(true, false), &catalog).unwrap();
    assert_eq!(
        controller.dispatch(Action::Next, &catalog).unwrap().effect,
        None
    );
    assert_eq!(bytes, before);
    assert!(CodeplugChannels::new(&bytes[..CHANNELS_LEN - 1]).is_err());
    bytes.push(0);
    assert!(CodeplugChannels::new(&bytes).is_err());
}

#[test]
fn ignored_events_leave_every_state_field_unchanged() {
    let catalog = Catalog::new(&[Some(true); 2]);
    let mut controller = Controller::new(0, policy(false, true), &catalog).unwrap();
    input_number(&mut controller, &[1], &catalog);
    let before = controller;
    for (key, event) in [
        (Key::Down, ButtonEvent::Held),
        (Key::Menu, ButtonEvent::Released),
        (Key::Aux16, ButtonEvent::Pressed),
    ] {
        let result = controller.on_key(key, event, &catalog).unwrap();
        assert!(!result.view_changed);
        assert_eq!(result.effect, None);
        assert_eq!(controller, before);
    }
}
