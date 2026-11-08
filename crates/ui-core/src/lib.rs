//! Small, deterministic channel/menu/spectrum controller.
//!
//! This is an explicit new UI policy, not a reconstruction of stock gestures.
//! It consumes already debounced input and emits intent only. There is no
//! allocation, hardware access, codeplug mutation, receiver tuning, or TX action.
//! Spectrum timing, frequency range, measurements, and cursor bounds belong to
//! the spectrum adapter. Opening a screen does not establish hardware readiness.

#![no_std]
#![forbid(unsafe_code)]

use uv98_codeplug::channels::{channel, ChannelError, CHANNEL_COUNT};
use uv98_codeplug::CHANNELS_LEN;
use uv98_keys::{ButtonEvent, Key};

/// Random-access metadata, without requiring all channel records in RAM.
///
/// `None` means lookup is unavailable, not an inactive record. Implementations
/// may fetch a single record from storage. Keep the count stable while a
/// controller uses this catalog. An active marker alone is not RF validation.
pub trait ChannelCatalog {
    fn len(&self) -> usize;
    fn is_active(&self, index: usize) -> Option<bool>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Optional host adapter. Borrows the existing 49,152 bytes and copies none.
/// Firmware can instead implement `ChannelCatalog` with one-record reads.
pub struct CodeplugChannels<'a> {
    bytes: &'a [u8],
}

impl<'a> CodeplugChannels<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, ChannelError> {
        if bytes.len() != CHANNELS_LEN {
            return Err(ChannelError::WrongSectionLength {
                actual: bytes.len(),
            });
        }
        Ok(Self { bytes })
    }
}

impl ChannelCatalog for CodeplugChannels<'_> {
    fn len(&self) -> usize {
        CHANNEL_COUNT
    }

    fn is_active(&self, index: usize) -> Option<bool> {
        channel(self.bytes, index)
            .ok()
            .map(|record| record.is_active())
    }
}

/// Caller-selected channel navigation policy. No implicit stock default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationPolicy {
    pub wrap: bool,
    pub include_inactive: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Channel,
    Menu,
    Spectrum,
}

/// Stable menu identifiers. Labels and layout belong to the renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuItem {
    Spectrum,
    ResetPeaks,
    Back,
}

const CHANNEL_MENU: [MenuItem; 2] = [MenuItem::Spectrum, MenuItem::Back];
const SPECTRUM_MENU: [MenuItem; 2] = [MenuItem::ResetPeaks, MenuItem::Back];

/// Logical actions are shared by physical buttons and host UI controls.
/// UI integrations should dispatch these rather than reproduce state changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Previous,
    Next,
    Confirm,
    Back,
    Digit(u8),
    OpenSpectrum,
    CloseSpectrum,
    ResetPeaks,
    /// Input state only. Assertion closes a spectrum session; release does not
    /// restart it. No action in this crate requests transmission.
    Ptt(bool),
}

/// Commands to an outer adapter. These contain no frequencies or GPIO writes.
/// A selected channel is a UI selection, not proof that a receiver has tuned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    SelectChannel { index: usize },
    OpenSpectrum,
    CloseSpectrum,
    MoveSpectrumCursor { direction: Direction },
    ResetPeaks,
}

/// One-based channel number under edit. `digits` preserves leading zeroes.
/// At most four digits are accepted, committed explicitly with Confirm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumberEntry {
    pub value: u16,
    pub digits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct View {
    pub screen: Screen,
    pub channel_index: usize,
    pub channel_count: usize,
    pub entry: Option<NumberEntry>,
    pub menu_item: MenuItem,
    /// True while a spectrum session is open, including its context menu.
    pub spectrum_open: bool,
    pub ptt_pressed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transition {
    pub view_changed: bool,
    pub effect: Option<Effect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiError {
    InvalidCatalogLength { actual: usize },
    CatalogSizeChanged { expected: usize, actual: usize },
    ChannelOutOfRange { index: usize },
    ChannelUnavailable { index: usize },
    InactiveChannel { index: usize },
    InvalidDigit { value: u8 },
    EntryTooLong,
    InvalidChannelNumber { number: u16 },
    ActionUnavailable,
    PttHeld,
}

/// Fixed-size state; it holds neither records nor samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Controller {
    screen: Screen,
    menu_origin: Screen,
    menu_position: usize,
    channel_index: usize,
    channel_count: usize,
    entry: Option<NumberEntry>,
    navigation: NavigationPolicy,
    ptt_pressed: bool,
}

impl Controller {
    /// Accept 1..=1024 catalog entries, matching the examined codeplug capacity.
    /// When inactive entries are excluded, the initial selection must be active.
    pub fn new<C: ChannelCatalog + ?Sized>(
        initial_index: usize,
        navigation: NavigationPolicy,
        catalog: &C,
    ) -> Result<Self, UiError> {
        let count = catalog.len();
        if count == 0 || count > CHANNEL_COUNT {
            return Err(UiError::InvalidCatalogLength { actual: count });
        }
        let state = Self {
            screen: Screen::Channel,
            menu_origin: Screen::Channel,
            menu_position: 0,
            channel_index: initial_index,
            channel_count: count,
            entry: None,
            navigation,
            ptt_pressed: false,
        };
        state.validate_selection(initial_index, catalog)?;
        Ok(state)
    }

    pub const fn view(&self) -> View {
        View {
            screen: self.screen,
            channel_index: self.channel_index,
            channel_count: self.channel_count,
            entry: self.entry,
            menu_item: self.menu_items()[self.menu_position],
            spectrum_open: self.spectrum_open(),
            ptt_pressed: self.ptt_pressed,
        }
    }

    /// Relevant menu contents. When no menu is open, these belong to that page.
    pub const fn menu_items(&self) -> &'static [MenuItem] {
        if matches!(self.screen, Screen::Spectrum)
            || (matches!(self.screen, Screen::Menu) && matches!(self.menu_origin, Screen::Spectrum))
        {
            &SPECTRUM_MENU
        } else {
            &CHANNEL_MENU
        }
    }

    /// Errors leave the complete state unchanged. Close/PTT handling never
    /// reads a catalog, so storage failure cannot prevent spectrum cancellation.
    pub fn dispatch<C: ChannelCatalog + ?Sized>(
        &mut self,
        action: Action,
        catalog: &C,
    ) -> Result<Transition, UiError> {
        let before = self.view();
        let mut next = *self;
        let effect = next.apply(action, catalog)?;
        let view_changed = next.view() != before;
        *self = next;
        Ok(Transition {
            view_changed,
            effect,
        })
    }

    /// Debouncing belongs upstream. Pressed events map once; Held/Released are
    /// ignored except PTT state. There is no implicit repeat or long-press menu.
    pub fn on_key<C: ChannelCatalog + ?Sized>(
        &mut self,
        key: Key,
        event: ButtonEvent,
        catalog: &C,
    ) -> Result<Transition, UiError> {
        match action_for_key(key, event) {
            Some(action) => self.dispatch(action, catalog),
            None => Ok(Transition {
                view_changed: false,
                effect: None,
            }),
        }
    }

    const fn spectrum_open(&self) -> bool {
        matches!(self.screen, Screen::Spectrum)
            || (matches!(self.screen, Screen::Menu) && matches!(self.menu_origin, Screen::Spectrum))
    }

    fn apply<C: ChannelCatalog + ?Sized>(
        &mut self,
        action: Action,
        catalog: &C,
    ) -> Result<Option<Effect>, UiError> {
        match action {
            Action::Previous => self.move_selection(Direction::Previous, catalog),
            Action::Next => self.move_selection(Direction::Next, catalog),
            Action::Confirm => self.confirm(catalog),
            Action::Back => Ok(self.back()),
            Action::Digit(value) => {
                if value > 9 {
                    return Err(UiError::InvalidDigit { value });
                }
                if self.screen != Screen::Channel {
                    return Err(UiError::ActionUnavailable);
                }
                let old = self.entry.unwrap_or(NumberEntry {
                    value: 0,
                    digits: 0,
                });
                if old.digits == 4 {
                    return Err(UiError::EntryTooLong);
                }
                self.entry = Some(NumberEntry {
                    value: old.value * 10 + u16::from(value),
                    digits: old.digits + 1,
                });
                Ok(None)
            }
            Action::OpenSpectrum => self.open_spectrum(),
            Action::CloseSpectrum => Ok(self.close_spectrum()),
            Action::ResetPeaks => {
                if !self.spectrum_open() {
                    return Err(UiError::ActionUnavailable);
                }
                Ok(Some(Effect::ResetPeaks))
            }
            Action::Ptt(pressed) => {
                self.ptt_pressed = pressed;
                Ok(if pressed { self.close_spectrum() } else { None })
            }
        }
    }

    fn check_catalog<C: ChannelCatalog + ?Sized>(&self, catalog: &C) -> Result<(), UiError> {
        let actual = catalog.len();
        if actual != self.channel_count {
            Err(UiError::CatalogSizeChanged {
                expected: self.channel_count,
                actual,
            })
        } else {
            Ok(())
        }
    }

    fn validate_selection<C: ChannelCatalog + ?Sized>(
        &self,
        index: usize,
        catalog: &C,
    ) -> Result<(), UiError> {
        self.check_catalog(catalog)?;
        if index >= self.channel_count {
            return Err(UiError::ChannelOutOfRange { index });
        }
        if !self.navigation.include_inactive {
            match catalog.is_active(index) {
                Some(true) => {}
                Some(false) => return Err(UiError::InactiveChannel { index }),
                None => return Err(UiError::ChannelUnavailable { index }),
            }
        }
        Ok(())
    }

    fn move_selection<C: ChannelCatalog + ?Sized>(
        &mut self,
        direction: Direction,
        catalog: &C,
    ) -> Result<Option<Effect>, UiError> {
        match self.screen {
            Screen::Menu => {
                // Menu wrap is explicit policy and independent of channel wrap.
                let count = self.menu_items().len();
                self.menu_position = match direction {
                    Direction::Previous if self.menu_position == 0 => count - 1,
                    Direction::Previous => self.menu_position - 1,
                    Direction::Next => (self.menu_position + 1) % count,
                };
                Ok(None)
            }
            Screen::Spectrum => Ok(Some(Effect::MoveSpectrumCursor { direction })),
            Screen::Channel => {
                self.check_catalog(catalog)?;
                self.entry = None;
                let mut candidate = self.channel_index;
                for _ in 0..self.channel_count - 1 {
                    candidate = match direction {
                        Direction::Previous if candidate > 0 => candidate - 1,
                        Direction::Previous if self.navigation.wrap => self.channel_count - 1,
                        Direction::Next if candidate + 1 < self.channel_count => candidate + 1,
                        Direction::Next if self.navigation.wrap => 0,
                        _ => return Ok(None),
                    };
                    if self.navigation.include_inactive {
                        self.channel_index = candidate;
                        return Ok(Some(Effect::SelectChannel { index: candidate }));
                    }
                    match catalog.is_active(candidate) {
                        Some(true) => {
                            self.channel_index = candidate;
                            return Ok(Some(Effect::SelectChannel { index: candidate }));
                        }
                        Some(false) => {}
                        None => return Err(UiError::ChannelUnavailable { index: candidate }),
                    }
                }
                Ok(None)
            }
        }
    }

    fn confirm<C: ChannelCatalog + ?Sized>(
        &mut self,
        catalog: &C,
    ) -> Result<Option<Effect>, UiError> {
        match self.screen {
            Screen::Channel => {
                if let Some(entry) = self.entry {
                    if entry.value == 0 || usize::from(entry.value) > self.channel_count {
                        return Err(UiError::InvalidChannelNumber {
                            number: entry.value,
                        });
                    }
                    let index = usize::from(entry.value) - 1;
                    self.validate_selection(index, catalog)?;
                    self.entry = None;
                    let effect =
                        (self.channel_index != index).then_some(Effect::SelectChannel { index });
                    self.channel_index = index;
                    Ok(effect)
                } else {
                    self.open_menu();
                    Ok(None)
                }
            }
            Screen::Spectrum => {
                self.open_menu();
                Ok(None)
            }
            Screen::Menu => match self.menu_items()[self.menu_position] {
                MenuItem::Spectrum => self.open_spectrum(),
                MenuItem::ResetPeaks => {
                    self.screen = Screen::Spectrum;
                    self.menu_position = 0;
                    Ok(Some(Effect::ResetPeaks))
                }
                MenuItem::Back => Ok(self.back()),
            },
        }
    }

    fn open_menu(&mut self) {
        self.menu_origin = self.screen;
        self.menu_position = 0;
        self.screen = Screen::Menu;
    }

    fn back(&mut self) -> Option<Effect> {
        match self.screen {
            Screen::Channel => {
                self.entry = None;
                None
            }
            Screen::Menu => {
                self.screen = self.menu_origin;
                self.menu_position = 0;
                None
            }
            Screen::Spectrum => self.close_spectrum(),
        }
    }

    fn open_spectrum(&mut self) -> Result<Option<Effect>, UiError> {
        if self.ptt_pressed {
            return Err(UiError::PttHeld);
        }
        let already_open = self.spectrum_open();
        self.screen = Screen::Spectrum;
        self.menu_origin = Screen::Spectrum;
        self.menu_position = 0;
        self.entry = None;
        Ok((!already_open).then_some(Effect::OpenSpectrum))
    }

    fn close_spectrum(&mut self) -> Option<Effect> {
        if !self.spectrum_open() {
            return None;
        }
        self.screen = Screen::Channel;
        self.menu_origin = Screen::Channel;
        self.menu_position = 0;
        self.entry = None;
        Some(Effect::CloseSpectrum)
    }
}

/// Shared, explicit input policy. Unknown auxiliary keys, star, and hash are
/// deliberately unassigned. Only the PTT input uses Held/Released events.
pub const fn action_for_key(key: Key, event: ButtonEvent) -> Option<Action> {
    if matches!(key, Key::Ptt) {
        return Some(Action::Ptt(!matches!(event, ButtonEvent::Released)));
    }
    if !matches!(event, ButtonEvent::Pressed) {
        return None;
    }
    match key {
        Key::Up => Some(Action::Previous),
        Key::Down => Some(Action::Next),
        Key::Menu => Some(Action::Confirm),
        Key::Exit => Some(Action::Back),
        Key::Digit0 => Some(Action::Digit(0)),
        Key::Digit1 => Some(Action::Digit(1)),
        Key::Digit2 => Some(Action::Digit(2)),
        Key::Digit3 => Some(Action::Digit(3)),
        Key::Digit4 => Some(Action::Digit(4)),
        Key::Digit5 => Some(Action::Digit(5)),
        Key::Digit6 => Some(Action::Digit(6)),
        Key::Digit7 => Some(Action::Digit(7)),
        Key::Digit8 => Some(Action::Digit(8)),
        Key::Digit9 => Some(Action::Digit(9)),
        _ => None,
    }
}
