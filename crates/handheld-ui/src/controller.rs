//! Storage-independent UI state. Frequencies and settings here are a volatile
//! preview; no effect tunes RF, transmits, or changes a saved channel record.
use crate::input::InputEvent;
use uv98_keys::Key;

pub const CHANNEL_COUNT: usize = 1024;
// This is an editor bound, not a claim that the radio supports every frequency.
pub const MIN_FREQUENCY_10HZ: u32 = 10_000;
pub const MAX_FREQUENCY_10HZ: u32 = 99_999_999;
pub const DEFAULT_FREQUENCY_10HZ: u32 = 14_550_000;
pub const STEP_10HZ: [u32; 6] = [250, 500, 625, 1000, 1250, 2500];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Channel,
    Vfo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
    pub const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Independent, volatile tuning context; this does not imply dual reception.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub mode: Mode,
    pub channel: Option<u16>,
    pub vfo_10hz: u32,
    pub zone: Option<u8>,
    pub step_index: u8,
    pub reverse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideAction {
    PttA,
    PttB,
    Active,
    Monitor,
    Scan,
    Power,
    None,
}

pub const SIDE_ACTIONS: [SideAction; 7] = [
    SideAction::PttA,
    SideAction::PttB,
    SideAction::Active,
    SideAction::Monitor,
    SideAction::Scan,
    SideAction::Power,
    SideAction::None,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Menu,
    Channels,
    ChannelDetails,
    Step,
    Calibration,
    Keys,
    Help,
    About,
    Shortcuts,
    ShortcutAction,
    Zones,
    SideKeys,
    SideAction,
    Setting,
    Spectrum,
}

/// Menu choice pages are drafts. Only MENU commits; opening, arrows and EXIT
/// never modify this setting or emit a setting request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Mode,
    Power,
    Monitor,
    Modulation,
    Reverse,
    ActiveSlot,
    Scan,
    DisplayLight,
    KeyLight,
}

impl Setting {
    pub const fn choice_count(self) -> u8 {
        if matches!(self, Self::DisplayLight | Self::KeyLight) {
            LIGHT_MODES.len() as u8
        } else if matches!(self, Self::Power) {
            3
        } else {
            2
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Mode,
    Channels,
    ChannelDetails,
    Step,
    Calibration,
    Keys,
    Help,
    About,
    Reverse,
    Scan,
    Shortcuts,
    Power,
    Monitor,
    Modulation,
    Zones,
    ActiveSlot,
    SideKeys,
    DisplayLight,
    KeyLight,
    Spectrum,
}

/// Full catalog for tooling. The visible menu uses `menu_entries`.
pub const MENU_ITEMS: [MenuItem; 20] = [
    MenuItem::Mode,
    MenuItem::Modulation,
    MenuItem::Channels,
    MenuItem::ChannelDetails,
    MenuItem::Step,
    MenuItem::Calibration,
    MenuItem::Keys,
    MenuItem::Help,
    MenuItem::About,
    MenuItem::Reverse,
    MenuItem::Scan,
    MenuItem::Shortcuts,
    MenuItem::Power,
    MenuItem::Monitor,
    MenuItem::Zones,
    MenuItem::ActiveSlot,
    MenuItem::SideKeys,
    MenuItem::DisplayLight,
    MenuItem::KeyLight,
    MenuItem::Spectrum,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuGroup {
    Mode,
    Receive,
    Scanning,
    Keys,
    System,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuItem),
    Group(MenuGroup),
}

pub const fn menu_entries(group: Option<MenuGroup>) -> &'static [MenuEntry] {
    use MenuEntry::{Group, Item};
    match group {
        None => &[
            Item(MenuItem::Modulation),
            Group(MenuGroup::Mode),
            Group(MenuGroup::Receive),
            Group(MenuGroup::Scanning),
            Group(MenuGroup::Keys),
            Group(MenuGroup::System),
        ],
        Some(MenuGroup::Mode) => &[
            Item(MenuItem::Mode),
            Item(MenuItem::ActiveSlot),
            Item(MenuItem::Channels),
            Item(MenuItem::Zones),
            Item(MenuItem::ChannelDetails),
        ],
        Some(MenuGroup::Receive) => &[
            Item(MenuItem::Modulation),
            Item(MenuItem::Monitor),
            Item(MenuItem::Power),
            Item(MenuItem::Reverse),
            Item(MenuItem::Step),
        ],
        Some(MenuGroup::Scanning) => &[Item(MenuItem::Scan), Item(MenuItem::Spectrum)],
        Some(MenuGroup::Keys) => &[
            Item(MenuItem::Shortcuts),
            Item(MenuItem::SideKeys),
            Item(MenuItem::Keys),
        ],
        Some(MenuGroup::System) => &[
            Item(MenuItem::DisplayLight),
            Item(MenuItem::KeyLight),
            Item(MenuItem::Calibration),
            Item(MenuItem::Help),
            Item(MenuItem::About),
        ],
    }
}

/// Stable path for simulations/CPS. FM/AM uses its direct root entry.
pub fn menu_location(item: MenuItem) -> (Option<MenuGroup>, u8) {
    for group in [
        None,
        Some(MenuGroup::Mode),
        Some(MenuGroup::Receive),
        Some(MenuGroup::Scanning),
        Some(MenuGroup::Keys),
        Some(MenuGroup::System),
    ] {
        if let Some(index) = menu_entries(group)
            .iter()
            .position(|entry| *entry == MenuEntry::Item(item))
        {
            return (group, index as u8);
        }
    }
    unreachable!("every MenuItem belongs to a menu group")
}

/// Global lighting policies. On/Off are not timers; the runtime owns actual GPIO
/// state and activity timing. Only these fixed choices are accepted by the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightMode {
    Off,
    On,
    Seconds5,
    Seconds10,
    Seconds20,
    Seconds30,
    Seconds60,
}
pub const LIGHT_MODES: [LightMode; 7] = [
    LightMode::Off,
    LightMode::On,
    LightMode::Seconds5,
    LightMode::Seconds10,
    LightMode::Seconds20,
    LightMode::Seconds30,
    LightMode::Seconds60,
];
impl LightMode {
    pub const fn timeout_ms(self) -> Option<u32> {
        match self {
            Self::Off | Self::On => None,
            Self::Seconds5 => Some(5_000),
            Self::Seconds10 => Some(10_000),
            Self::Seconds20 => Some(20_000),
            Self::Seconds30 => Some(30_000),
            Self::Seconds60 => Some(60_000),
        }
    }
}

/// Assignable long-digit actions. Their RF-related variants only emit requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutAction {
    None,
    Power,
    Monitor,
    Scan,
    Reverse,
    Mode,
    Step,
    Channels,
    Keys,
    Lock,
    Modulation,
    Zones,
    SwitchSlot,
    Spectrum,
}

pub const SHORTCUT_ACTIONS: [ShortcutAction; 14] = [
    ShortcutAction::None,
    ShortcutAction::Power,
    ShortcutAction::Monitor,
    ShortcutAction::Scan,
    ShortcutAction::Reverse,
    ShortcutAction::Mode,
    ShortcutAction::Step,
    ShortcutAction::Channels,
    ShortcutAction::Keys,
    ShortcutAction::Lock,
    ShortcutAction::Modulation,
    ShortcutAction::Zones,
    ShortcutAction::SwitchSlot,
    ShortcutAction::Spectrum,
];

/// Indices 0..8 retain digits 1..9; index 9 is digit 0.
pub const DEFAULT_BINDINGS: [ShortcutAction; 10] = [
    ShortcutAction::Power,
    ShortcutAction::Monitor,
    ShortcutAction::Scan,
    ShortcutAction::Reverse,
    ShortcutAction::Mode,
    ShortcutAction::Step,
    ShortcutAction::Channels,
    ShortcutAction::Keys,
    ShortcutAction::Lock,
    ShortcutAction::Spectrum,
];

/// Requested/Stopping describe pending work, never successful radio operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanState {
    Stopped,
    Requested,
    Running,
    Stopping,
}

/// An outer runtime must explicitly accept, execute and report these requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    CyclePower,
    ToggleMonitor,
    StartScan,
    StopScan,
    CycleModulation,
    SetPower(u8),
    SetMonitor(bool),
    SetModulation { am: bool },
    SetDisplayLight(LightMode),
    SetKeyLight(LightMode),
    StartSpectrum,
    StopSpectrum,
    AcceptSpectrum { bin: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Channel,
    Frequency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub value: u32,
    pub digits: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    None,
    InvalidChannel,
    InactiveChannel,
    InvalidFrequency,
    EntryFull,
    Locked,
    TxDisabled,
    NoChannels,
    ChannelOnly,
    RadioUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub screen: Screen,
    pub mode: Mode,
    pub channel: Option<u16>,
    pub list_channel: Option<u16>,
    pub vfo_10hz: u32,
    pub step_index: u8,
    pub menu_index: u8,
    pub menu_group: Option<MenuGroup>,
    pub spectrum_cursor: u8,
    pub entry: Option<Entry>,
    pub locked: bool,
    pub notice: Notice,
    pub last_key: Option<Key>,
    pub ptt_down: bool,
    /// Volatile RX/TX display reversal. The source channel and RF are untouched.
    pub reverse: bool,
    pub scan: ScanState,
    pub bindings: [ShortcutAction; 10],
    /// Long-digit binding being edited: 0 means digit1, 9 means digit0.
    pub binding_index: u8,
    /// Pending action in SHORTCUT_ACTIONS; only MENU commits this choice.
    pub binding_action_index: u8,
    pub active_slot: Slot,
    pub zone: Option<u8>,
    pub zone_cursor: Option<u8>,
    pub side_bindings: [SideAction; 2],
    pub side_index: u8,
    pub side_action_index: u8,
    pub side_down: [bool; 2],
    /// Eligible held TX input, never evidence that RF is transmitting.
    pub ptt_slot: Option<Slot>,
    pub setting: Option<Setting>,
    pub setting_index: u8,
    /// Confirmed global RAM policy, not proof that a physical lamp is lit.
    pub display_light: LightMode,
    pub key_light: LightMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub redraw: bool,
    pub channel_changed: bool,
    pub request: Option<Request>,
    /// An EXIT event asks the runtime to turn monitor off and stop its scanner.
    /// Separate from request so StopScan cannot discard monitor cancellation.
    pub cancel_temporary: bool,
    /// A new eligible PTT input edge, not authorization or proof of RF TX.
    pub ptt_tx_requested: bool,
    /// Present only for a new TX request; View::ptt_slot tracks a held intent.
    pub ptt_slot: Option<Slot>,
    pub zone_changed: bool,
    pub slot_changed: bool,
}

/// Catalog markers are supplied by the caller after read-only storage access.
/// A set bit means an active slot, not a physically validated RF frequency.
pub fn channel_active(bitmap: &[u8; 128], index: u16) -> bool {
    let index = index as usize;
    index < CHANNEL_COUNT && bitmap[index / 8] & (1 << (index % 8)) != 0
}

pub fn active_count(bitmap: &[u8; 128]) -> usize {
    bitmap.iter().map(|byte| byte.count_ones() as usize).sum()
}

pub fn first_channel(bitmap: &[u8; 128]) -> Option<u16> {
    (0..CHANNEL_COUNT as u16).find(|&index| channel_active(bitmap, index))
}

/// Find the next active slot, wrapping through all1024 slots at most once.
/// An empty catalog returns None; one active slot returns itself.
pub fn adjacent_channel(bitmap: &[u8; 128], current: Option<u16>, forward: bool) -> Option<u16> {
    let Some(current) = current.filter(|&index| (index as usize) < CHANNEL_COUNT) else {
        return if forward {
            first_channel(bitmap)
        } else {
            (0..CHANNEL_COUNT as u16)
                .rev()
                .find(|&index| channel_active(bitmap, index))
        };
    };
    for distance in 1..=CHANNEL_COUNT {
        let index = if forward {
            (current as usize + distance) % CHANNEL_COUNT
        } else {
            (current as usize + CHANNEL_COUNT - distance) % CHANNEL_COUNT
        } as u16;
        if channel_active(bitmap, index) {
            return Some(index);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controller {
    view: View,
    request: Option<Request>,
    ptt_tx_requested: bool,
    ptt_suppressed_until_release: bool,
    contexts: [Context; 2],
    zones: [u8; 32],
    channel_order: [u16; 200],
    channel_order_len: u8,
    radio_power: Option<u8>,
    radio_monitor: bool,
    radio_am: bool,
    cancel_temporary: bool,
}

impl Controller {
    pub fn new(bitmap: &[u8; 128], initial_vfo_10hz: u32) -> Self {
        let channel = first_channel(bitmap);
        let mode = if channel.is_some() {
            Mode::Channel
        } else {
            Mode::Vfo
        };
        let vfo_10hz = if valid_frequency(initial_vfo_10hz) {
            initial_vfo_10hz
        } else {
            DEFAULT_FREQUENCY_10HZ
        };
        let context = Context {
            mode,
            channel,
            vfo_10hz,
            zone: None,
            step_index: 4,
            reverse: false,
        };
        Self {
            view: View {
                screen: Screen::Home,
                mode: if channel.is_some() {
                    Mode::Channel
                } else {
                    Mode::Vfo
                },
                channel,
                list_channel: channel,
                vfo_10hz: if valid_frequency(initial_vfo_10hz) {
                    initial_vfo_10hz
                } else {
                    DEFAULT_FREQUENCY_10HZ
                },
                step_index: 4,
                menu_index: 0,
                menu_group: None,
                spectrum_cursor: 40,
                entry: None,
                locked: false,
                notice: if channel.is_some() {
                    Notice::None
                } else {
                    Notice::NoChannels
                },
                last_key: None,
                ptt_down: false,
                reverse: false,
                scan: ScanState::Stopped,
                bindings: DEFAULT_BINDINGS,
                binding_index: 0,
                binding_action_index: 0,
                active_slot: Slot::A,
                zone: None,
                zone_cursor: None,
                side_bindings: [SideAction::PttA, SideAction::PttB],
                side_index: 0,
                side_action_index: 0,
                side_down: [false; 2],
                ptt_slot: None,
                setting: None,
                setting_index: 0,
                display_light: LightMode::On,
                key_light: LightMode::On,
            },
            request: None,
            ptt_tx_requested: false,
            ptt_suppressed_until_release: false,
            contexts: [context; 2],
            zones: [0; 32],
            channel_order: [0; 200],
            channel_order_len: 0,
            radio_power: None,
            radio_monitor: false,
            radio_am: false,
            cancel_temporary: false,
        }
    }

    pub const fn view(&self) -> View {
        self.view
    }

    /// Commit only after the spectrum backend restored normal RX successfully.
    /// A highlighted bin alone never changes a slot's saved frequency or mode.
    pub fn accept_spectrum_frequency(&mut self, frequency_10hz: u32) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        if self.view.screen == Screen::Spectrum
            && !self.view.locked
            && !self.view.ptt_down
            && valid_frequency(frequency_10hz)
        {
            self.view.mode = Mode::Vfo;
            self.view.vfo_10hz = frequency_10hz;
            self.view.reverse = false;
            self.view.entry = None;
            self.view.screen = Screen::Home;
            self.view.ptt_slot = None;
            if self.view.ptt_down {
                self.ptt_suppressed_until_release = true;
            }
        }
        self.change_since(before)
    }

    pub fn set_lighting_modes(&mut self, display: LightMode, keys: LightMode) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.view.display_light = display;
        self.view.key_light = keys;
        self.change_since(before)
    }

    /// Confirmed runtime selections seed newly opened pages only. Incoming
    /// telemetry must not move a pending choice while the operator is editing.
    pub fn set_radio_choices(&mut self, power: Option<u8>, monitor: bool, am: bool) {
        self.radio_power = power.filter(|power| *power < 3);
        self.radio_monitor = monitor;
        self.radio_am = am;
    }

    fn open_setting(&mut self, setting: Setting) {
        self.view.setting_index = match setting {
            Setting::Mode => u8::from(self.view.mode == Mode::Vfo),
            Setting::Power => self.radio_power.unwrap_or(0),
            Setting::Monitor => u8::from(self.radio_monitor),
            Setting::Modulation => u8::from(self.radio_am),
            Setting::Reverse => u8::from(self.view.reverse),
            Setting::ActiveSlot => u8::from(self.view.active_slot == Slot::B),
            Setting::Scan => u8::from(matches!(
                self.view.scan,
                ScanState::Requested | ScanState::Running
            )),
            Setting::DisplayLight => LIGHT_MODES
                .iter()
                .position(|mode| *mode == self.view.display_light)
                .unwrap_or(1) as u8,
            Setting::KeyLight => LIGHT_MODES
                .iter()
                .position(|mode| *mode == self.view.key_light)
                .unwrap_or(1) as u8,
        };
        self.view.setting = Some(setting);
        self.view.screen = Screen::Setting;
    }

    fn commit_setting(&mut self, bitmap: &[u8; 128]) {
        let Some(setting) = self.view.setting.take() else {
            self.view.screen = Screen::Menu;
            return;
        };
        let selected = self.view.setting_index;
        match setting {
            Setting::Mode => {
                let mode = if selected == 0 {
                    Mode::Channel
                } else {
                    Mode::Vfo
                };
                if self.view.mode != mode {
                    self.toggle_mode(bitmap);
                }
            }
            Setting::Power => {
                // Explicit confirmation sets desired state even when telemetry
                // is unavailable and cannot describe a retained RF override.
                self.queue_request(Request::SetPower(selected));
            }
            Setting::Monitor => {
                self.queue_request(Request::SetMonitor(selected != 0));
            }
            Setting::Modulation => {
                self.queue_request(Request::SetModulation { am: selected != 0 });
            }
            Setting::DisplayLight => {
                self.queue_request(Request::SetDisplayLight(LIGHT_MODES[selected as usize]))
            }
            Setting::KeyLight => {
                self.queue_request(Request::SetKeyLight(LIGHT_MODES[selected as usize]))
            }
            Setting::Reverse => {
                if self.view.reverse != (selected != 0) {
                    self.toggle_reverse();
                }
            }
            Setting::ActiveSlot => self.switch_slot(if selected == 0 { Slot::A } else { Slot::B }),
            Setting::Scan => {
                if selected == 0
                    && matches!(self.view.scan, ScanState::Requested | ScanState::Running)
                {
                    self.request_scan_stop();
                } else if selected != 0 && self.view.scan == ScanState::Stopped {
                    self.toggle_scan(bitmap);
                }
            }
        }
        self.view.screen = Screen::Menu;
    }

    pub fn context(&self, slot: Slot) -> Context {
        if slot == self.view.active_slot {
            self.current_context()
        } else {
            self.contexts[slot.index()]
        }
    }

    /// Preserve CPS zone membership order, dropping invalid or repeated slots.
    /// All uses an empty order; a selected empty zone never falls back to All.
    pub fn set_channel_order(&mut self, order: &[u16]) -> usize {
        self.channel_order_len = 0;
        for &channel in order {
            let len = self.channel_order_len as usize;
            if len == self.channel_order.len() {
                break;
            }
            if channel as usize >= CHANNEL_COUNT || self.channel_order[..len].contains(&channel) {
                continue;
            }
            self.channel_order[len] = channel;
            self.channel_order_len += 1;
        }
        self.channel_order_len as usize
    }

    pub fn channel_order(&self) -> &[u16] {
        &self.channel_order[..self.channel_order_len as usize]
    }

    pub fn first_ordered_channel(&self, bitmap: &[u8; 128]) -> Option<u16> {
        if self.channel_order_len == 0 && self.view.zone.is_none() {
            first_channel(bitmap)
        } else {
            self.channel_order()
                .iter()
                .copied()
                .find(|&channel| channel_active(bitmap, channel))
        }
    }

    pub fn ordered_channel(
        &self,
        bitmap: &[u8; 128],
        current: Option<u16>,
        forward: bool,
    ) -> Option<u16> {
        if self.channel_order_len == 0 {
            return if self.view.zone.is_none() {
                adjacent_channel(bitmap, current, forward)
            } else {
                None
            };
        }
        let order = self.channel_order();
        let current =
            current.and_then(|channel| order.iter().position(|candidate| *candidate == channel));
        let start = current.unwrap_or(if forward { order.len() - 1 } else { 0 });
        for distance in 1..=order.len() {
            let index = if forward {
                (start + distance) % order.len()
            } else {
                (start + order.len() - distance) % order.len()
            };
            if channel_active(bitmap, order[index]) {
                return Some(order[index]);
            }
        }
        None
    }

    fn current_context(&self) -> Context {
        Context {
            mode: self.view.mode,
            channel: self.view.channel,
            vfo_10hz: self.view.vfo_10hz,
            zone: self.view.zone,
            step_index: self.view.step_index,
            reverse: self.view.reverse,
        }
    }

    /// The caller must reload the chosen zone and reconcile its channel bitmap
    /// before using this context to build an RF plan.
    pub fn select_slot(&mut self, slot: Slot) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        if !self.view.locked {
            self.switch_slot(slot);
        }
        self.change_since(before)
    }

    fn switch_slot(&mut self, slot: Slot) {
        if slot == self.view.active_slot {
            return;
        }
        if self.view.ptt_down && self.view.ptt_slot != Some(slot) {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
        }
        self.contexts[self.view.active_slot.index()] = self.current_context();
        let context = self.contexts[slot.index()];
        self.view.active_slot = slot;
        self.view.mode = context.mode;
        self.view.channel = context.channel;
        self.view.list_channel = context.channel;
        self.view.vfo_10hz = context.vfo_10hz;
        self.view.zone = context.zone;
        self.view.zone_cursor = context.zone;
        self.view.step_index = context.step_index;
        self.view.reverse = context.reverse;
        self.channel_order_len = 0;
        self.view.entry = None;
        self.view.screen = Screen::Home;
        self.view.setting = None;
        self.request_scan_stop();
    }

    pub fn set_zone_catalog(&mut self, zones: &[u8; 32]) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.zones = *zones;
        for context in &mut self.contexts {
            if context
                .zone
                .is_some_and(|zone| zones[zone as usize / 8] & (1 << (zone % 8)) == 0)
            {
                context.zone = None;
            }
        }
        if self.view.zone.is_some_and(|zone| !self.zone_exists(zone)) {
            self.view.zone = None;
            self.view.reverse = false;
            self.channel_order_len = 0;
            self.view.ptt_slot = None;
            if self.view.ptt_down {
                self.ptt_suppressed_until_release = true;
            }
            self.request_scan_stop();
        }
        if self
            .view
            .zone_cursor
            .is_some_and(|zone| !self.zone_exists(zone))
        {
            self.view.zone_cursor = None;
        }
        self.change_since(before)
    }

    /// Reconcile after zone/slot changes without accepting a stale channel from
    /// a previous zone. An empty zone remains selected with channel=None.
    pub fn reconcile_channels(&mut self, bitmap: &[u8; 128]) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.view.channel = self
            .view
            .channel
            .filter(|&channel| {
                channel_active(bitmap, channel)
                    && (self.view.zone.is_none() || self.channel_order().contains(&channel))
            })
            .or_else(|| self.first_ordered_channel(bitmap));
        self.view.list_channel = self.view.channel;
        if self.view.channel != before.channel {
            self.view.reverse = false;
            self.request_scan_stop();
        }
        if self.view.mode == Mode::Channel && self.view.channel.is_none() {
            self.view.notice = Notice::NoChannels;
        }
        self.change_since(before)
    }

    /// A runtime scan result, not manual navigation. This preserves scan state;
    /// only a catalog-valid channel in Channel mode can become the scan cursor.
    pub fn select_scan_channel(&mut self, index: u16, bitmap: &[u8; 128]) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        if self.view.mode == Mode::Channel
            && matches!(self.view.scan, ScanState::Requested | ScanState::Running)
            && channel_active(bitmap, index)
            && (self.view.zone.is_none() || self.channel_order().contains(&index))
        {
            self.view.channel = Some(index);
            self.view.list_channel = Some(index);
            self.view.reverse = false;
        }
        self.change_since(before)
    }

    /// A backend-selected VFO scan cursor. Physical band/step validation remains
    /// the runtime's responsibility; this editor bound alone never permits RF.
    pub fn select_scan_frequency(&mut self, frequency_10hz: u32) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        if self.view.mode == Mode::Vfo
            && matches!(self.view.scan, ScanState::Requested | ScanState::Running)
            && valid_frequency(frequency_10hz)
        {
            self.view.vfo_10hz = frequency_10hz;
        }
        self.change_since(before)
    }

    fn zone_exists(&self, zone: u8) -> bool {
        self.zones[zone as usize / 8] & (1 << (zone % 8)) != 0
    }

    fn open_zones(&mut self) {
        self.view.zone_cursor = self.view.zone;
        self.view.screen = Screen::Zones;
    }

    fn navigate_zone(&mut self, forward: bool) {
        // 256 zones plus the permanent All entry; bounded even if none exist.
        let current = self.view.zone_cursor.map_or(0, |zone| u16::from(zone) + 1);
        for distance in 1..=257u16 {
            let candidate = if forward {
                (current + distance) % 257
            } else {
                (current + 257 - distance) % 257
            };
            if candidate == 0 || self.zone_exists((candidate - 1) as u8) {
                self.view.zone_cursor = if candidate == 0 {
                    None
                } else {
                    Some((candidate - 1) as u8)
                };
                break;
            }
        }
    }

    pub fn set_side_binding(&mut self, index: usize, action: SideAction) -> bool {
        let Some(binding) = self.view.side_bindings.get_mut(index) else {
            return false;
        };
        if *binding != action {
            *binding = action;
            // A remap never turns an already held function key into a TX key.
            self.view.ptt_slot = None;
            if self.view.ptt_down {
                self.ptt_suppressed_until_release = true;
            }
        }
        true
    }

    fn open_side_action(&mut self) {
        let action = self.view.side_bindings[self.view.side_index as usize];
        self.view.side_action_index = SIDE_ACTIONS
            .iter()
            .position(|candidate| *candidate == action)
            .unwrap_or(0) as u8;
        self.view.screen = Screen::SideAction;
    }

    pub fn handle(&mut self, event: InputEvent, bitmap: &[u8; 128]) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.apply(event, bitmap);
        if self.view.screen != Screen::Setting {
            self.view.setting = None;
        }
        // Reversal belongs to one current channel, never a later channel/VFO.
        // Centralize this invariant across arrows, direct entry and list commit.
        if self.view.active_slot == before.active_slot
            && (self.view.mode != before.mode || self.view.channel != before.channel)
        {
            self.view.reverse = false;
        }
        // Browsing/cancelling a draft is inert. Actual navigation and entry
        // cancel an existing scan; newly requested scans are excluded here.
        let browsing = matches!(before.screen, Screen::Home | Screen::Menu | Screen::Setting)
            && matches!(
                self.view.screen,
                Screen::Home | Screen::Menu | Screen::Setting
            );
        if before.scan != ScanState::Stopped
            && (self.view.mode != before.mode
                || self.view.channel != before.channel
                || self.view.vfo_10hz != before.vfo_10hz
                || (self.view.screen != before.screen && !browsing)
                || self.view.entry != before.entry
                || self.view.reverse != before.reverse
                || self.view.locked != before.locked
                || self.view.active_slot != before.active_slot
                || self.view.zone != before.zone)
        {
            if self.request == Some(Request::StartSpectrum) {
                // cancel_temporary lets the backend stop/ack the ordinary scan
                // before executing this separately requested spectrum start.
                self.view.scan = ScanState::Stopping;
            } else {
                self.request_scan_stop();
            }
        }
        if before.screen == Screen::Spectrum && self.view.screen != Screen::Spectrum {
            self.queue_request(Request::StopSpectrum);
        }
        if self.view.ptt_down
            && (self.view.mode != before.mode
                || self.view.channel != before.channel
                || self.view.vfo_10hz != before.vfo_10hz
                || self.view.zone != before.zone
                || self.view.active_slot != before.active_slot
                || self.view.reverse != before.reverse
                || self.view.screen != before.screen)
        {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
        }
        self.change_since(before)
    }

    /// Cancel from an outer frequency/menu change and return the stop request.
    /// Stopping is retained until the runtime explicitly acknowledges completion.
    pub fn cancel_scan(&mut self) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.request_scan_stop();
        self.change_since(before)
    }

    pub fn acknowledge_scan_started(&mut self) -> bool {
        if self.view.scan == ScanState::Requested
            && self.view.ptt_slot.is_none()
            && !self.view.locked
        {
            self.view.scan = ScanState::Running;
            true
        } else {
            false
        }
    }

    pub fn acknowledge_scan_stopped(&mut self) {
        self.view.scan = ScanState::Stopped;
    }

    /// A failed stop does not falsely claim that an active scan has stopped.
    pub fn reject_radio_request(&mut self, request: Request) {
        if request == Request::StartScan && self.view.scan == ScanState::Requested {
            self.view.scan = ScanState::Stopped;
        }
        if request == Request::StartSpectrum && self.view.screen == Screen::Spectrum {
            self.view.screen = Screen::Menu;
            let (group, index) = menu_location(MenuItem::Spectrum);
            self.view.menu_group = group;
            self.view.menu_index = index;
            self.view.ptt_slot = None;
            if self.view.ptt_down {
                self.ptt_suppressed_until_release = true;
            }
        }
        self.view.notice = Notice::RadioUnavailable;
    }

    /// Explicit backend refusal, including a compile-time TX gate or unsupported
    /// channel. Keep physical button feedback, but revoke the held TX intent.
    pub fn reject_tx(&mut self) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.view.ptt_slot = None;
        self.ptt_suppressed_until_release = true;
        self.view.notice = Notice::TxDisabled;
        self.change_since(before)
    }

    pub fn binding(&self, digit: u8) -> Option<ShortcutAction> {
        binding_index(digit).map(|index| self.view.bindings[index as usize])
    }

    /// Change a volatile UI assignment. No saved codeplug or NOR is modified.
    pub fn set_binding(&mut self, digit: u8, action: ShortcutAction) -> bool {
        let Some(index) = binding_index(digit) else {
            return false;
        };
        self.view.bindings[index as usize] = action;
        true
    }

    fn change_since(&mut self, before: View) -> Change {
        Change {
            redraw: self.view != before || self.view.screen == Screen::Keys,
            channel_changed: self.view.channel != before.channel,
            request: self.request.take(),
            cancel_temporary: core::mem::take(&mut self.cancel_temporary),
            ptt_tx_requested: self.ptt_tx_requested,
            ptt_slot: if self.ptt_tx_requested {
                self.view.ptt_slot
            } else {
                None
            },
            zone_changed: self.view.zone != before.zone,
            slot_changed: self.view.active_slot != before.active_slot,
        }
    }

    fn queue_request(&mut self, request: Request) {
        // Cancellation must not be overwritten by another action in this event.
        if request == Request::StopSpectrum {
            self.request = Some(request);
        } else if self.request != Some(Request::StopSpectrum)
            && (self.request != Some(Request::StopScan) || request == Request::StopScan)
        {
            self.request = Some(request);
        }
    }

    fn request_scan_stop(&mut self) {
        if self.view.scan != ScanState::Stopped {
            self.view.scan = ScanState::Stopping;
            self.queue_request(Request::StopScan);
        }
    }

    fn apply(&mut self, event: InputEvent, bitmap: &[u8; 128]) {
        match event {
            InputEvent::Ptt(down) => {
                self.side_event(0, down, bitmap);
                return;
            }
            InputEvent::PttB(down) => {
                self.side_event(1, down, bitmap);
                return;
            }
            _ => {}
        }
        let key = match event {
            InputEvent::Short(key) | InputEvent::Hold(key) | InputEvent::Repeat(key) => key,
            InputEvent::Ptt(_) | InputEvent::PttB(_) => return,
        };
        self.view.last_key = Some(key);
        if matches!(
            event,
            InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit)
        ) {
            self.cancel_temporary = true;
            self.request_scan_stop();
        }
        if event == InputEvent::Hold(Key::Hash) {
            self.toggle_lock();
            return;
        }
        if self.view.locked {
            if let InputEvent::Hold(key) = event {
                if digit(key).and_then(|digit| self.binding(digit)) == Some(ShortcutAction::Lock) {
                    self.toggle_lock();
                    return;
                }
            }
        }
        if self.view.locked {
            self.view.notice = Notice::Locked;
            return;
        }
        self.view.notice = Notice::None;
        match event {
            InputEvent::Hold(Key::Menu) => {
                self.view.entry = None;
                self.toggle_mode(bitmap);
            }
            InputEvent::Hold(Key::Exit) => {
                self.view.entry = None;
                self.view.screen = Screen::Home;
                self.view.menu_group = None;
                self.view.menu_index = 0;
            }
            InputEvent::Hold(Key::Star) if self.view.screen != Screen::Keys => {
                self.switch_slot(self.view.active_slot.other())
            }
            InputEvent::Hold(key) => {
                // An editing field or test page never accidentally invokes an
                // RF shortcut. In particular holding a digit cannot first type it.
                if self.view.screen == Screen::Home && self.view.entry.is_none() {
                    if let Some(action) = digit(key).and_then(|digit| self.binding(digit)) {
                        self.shortcut(action, bitmap);
                    }
                }
            }
            InputEvent::Repeat(Key::Up) => self.navigate(false, bitmap),
            InputEvent::Repeat(Key::Down) => self.navigate(true, bitmap),
            InputEvent::Repeat(_) => {}
            InputEvent::Short(Key::Up) => self.navigate(false, bitmap),
            InputEvent::Short(Key::Down) => self.navigate(true, bitmap),
            InputEvent::Short(Key::Menu) => self.confirm(bitmap),
            InputEvent::Short(Key::Exit) => self.back(),
            InputEvent::Short(Key::Star) if self.view.screen != Screen::Keys => {
                self.view.entry = None;
                self.toggle_mode(bitmap);
            }
            InputEvent::Short(Key::Hash)
                if self.view.screen == Screen::Home && self.view.entry.is_none() =>
            {
                self.view.screen = Screen::Step
            }
            InputEvent::Short(key) => {
                if let Some(digit) = digit(key) {
                    if self.view.screen == Screen::Shortcuts {
                        self.view.binding_index = binding_index(digit).expect("decoded digit");
                        self.open_binding_action();
                    } else {
                        self.enter_digit(digit);
                    }
                }
            }
            InputEvent::Ptt(_) | InputEvent::PttB(_) => {}
        }
    }

    fn side_event(&mut self, index: usize, down: bool, bitmap: &[u8; 128]) {
        let new_press = down && !self.view.side_down[index];
        self.view.side_down[index] = down;
        self.view.ptt_down = self.view.side_down.iter().any(|&pressed| pressed);
        self.view.last_key = Some(if index == 0 { Key::Ptt } else { Key::Aux16 });
        if !down {
            // Only one side key may own a TX intent. Any release also clears it.
            self.view.ptt_slot = None;
            if !self.view.ptt_down {
                self.ptt_suppressed_until_release = false;
            }
            if self.view.notice == Notice::TxDisabled {
                self.view.notice = Notice::None;
            }
            return;
        }
        if self.view.side_down[1 - index] || self.view.locked {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
            self.view.notice = if self.view.locked {
                Notice::Locked
            } else {
                Notice::TxDisabled
            };
            self.request_scan_stop();
            return;
        }
        if !new_press || self.ptt_suppressed_until_release {
            return;
        }
        if self.view.screen == Screen::Spectrum {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
            self.cancel_temporary = true;
            self.queue_request(Request::StopSpectrum);
            return;
        }
        self.view.notice = Notice::None;
        let action = self.view.side_bindings[index];
        let slot = match action {
            SideAction::PttA => Some(Slot::A),
            SideAction::PttB => Some(Slot::B),
            SideAction::Active => Some(self.view.active_slot),
            _ => None,
        };
        if let Some(slot) = slot {
            if self.view.scan != ScanState::Stopped {
                self.ptt_suppressed_until_release = true;
                self.view.ptt_slot = None;
                self.request_scan_stop();
                self.view.notice = Notice::TxDisabled;
            } else {
                self.view.ptt_slot = Some(slot);
                self.ptt_tx_requested = true;
            }
        } else {
            match action {
                SideAction::Monitor => self.queue_request(Request::ToggleMonitor),
                SideAction::Scan => self.toggle_scan(bitmap),
                SideAction::Power => self.queue_request(Request::CyclePower),
                _ => {}
            }
        }
    }

    fn toggle_lock(&mut self) {
        self.view.locked = !self.view.locked;
        self.view.entry = None;
        self.view.screen = Screen::Home;
        if self.view.locked && self.view.ptt_down {
            self.ptt_suppressed_until_release = true;
        }
        if self.view.locked {
            self.view.ptt_slot = None;
        }
        self.view.notice = if self.view.locked {
            Notice::Locked
        } else {
            Notice::None
        };
    }

    fn shortcut(&mut self, action: ShortcutAction, bitmap: &[u8; 128]) {
        match action {
            ShortcutAction::None => {}
            ShortcutAction::Power => self.queue_request(Request::CyclePower),
            ShortcutAction::Monitor => self.queue_request(Request::ToggleMonitor),
            ShortcutAction::Modulation => self.queue_request(Request::CycleModulation),
            ShortcutAction::Scan => self.toggle_scan(bitmap),
            ShortcutAction::Reverse => self.toggle_reverse(),
            ShortcutAction::Mode => self.toggle_mode(bitmap),
            ShortcutAction::Step => self.view.screen = Screen::Step,
            ShortcutAction::Channels => self.open_channels(bitmap),
            ShortcutAction::Keys => self.view.screen = Screen::Keys,
            ShortcutAction::Lock => self.toggle_lock(),
            ShortcutAction::Zones => self.open_zones(),
            ShortcutAction::SwitchSlot => self.switch_slot(self.view.active_slot.other()),
            ShortcutAction::Spectrum => self.open_spectrum(),
        }
    }

    fn open_spectrum(&mut self) {
        self.view.entry = None;
        self.view.screen = Screen::Spectrum;
        self.view.spectrum_cursor = 40;
        self.view.menu_group = Some(MenuGroup::Scanning);
        self.view.menu_index = menu_location(MenuItem::Spectrum).1;
        self.cancel_temporary = true;
        self.queue_request(Request::StartSpectrum);
    }

    fn toggle_scan(&mut self, bitmap: &[u8; 128]) {
        if self.view.scan != ScanState::Stopped {
            self.request_scan_stop();
        } else if self.view.ptt_slot.is_some() {
            self.view.notice = Notice::TxDisabled;
        } else if self.view.mode == Mode::Channel
            && self
                .view
                .channel
                .filter(|&slot| channel_active(bitmap, slot))
                .is_none()
        {
            self.view.notice = Notice::NoChannels;
        } else {
            self.view.entry = None;
            self.view.reverse = false;
            self.view.screen = Screen::Home;
            self.view.scan = ScanState::Requested;
            self.queue_request(Request::StartScan);
        }
    }

    fn toggle_reverse(&mut self) {
        if self.view.mode == Mode::Channel && self.view.channel.is_some() {
            self.view.reverse = !self.view.reverse;
            self.view.screen = Screen::Home;
        } else {
            self.view.notice = Notice::ChannelOnly;
        }
    }

    fn open_channels(&mut self, bitmap: &[u8; 128]) {
        self.view.list_channel = self
            .view
            .channel
            .filter(|&slot| channel_active(bitmap, slot))
            .or_else(|| self.first_ordered_channel(bitmap));
        self.view.screen = Screen::Channels;
        if self.view.list_channel.is_none() {
            self.view.notice = Notice::NoChannels;
        }
    }

    fn open_binding_action(&mut self) {
        let action = self.view.bindings[self.view.binding_index as usize];
        self.view.binding_action_index = SHORTCUT_ACTIONS
            .iter()
            .position(|candidate| *candidate == action)
            .unwrap_or(0) as u8;
        self.view.screen = Screen::ShortcutAction;
    }

    fn toggle_mode(&mut self, bitmap: &[u8; 128]) {
        self.view.screen = Screen::Home;
        match self.view.mode {
            Mode::Channel => self.view.mode = Mode::Vfo,
            Mode::Vfo => {
                let channel = self
                    .view
                    .channel
                    .filter(|&slot| channel_active(bitmap, slot))
                    .or_else(|| self.first_ordered_channel(bitmap));
                if channel.is_some() {
                    self.view.channel = channel;
                    self.view.mode = Mode::Channel;
                } else {
                    self.view.notice = Notice::NoChannels;
                }
            }
        }
    }

    fn navigate(&mut self, forward: bool, bitmap: &[u8; 128]) {
        // Editing never tunes or commits a half-entered value implicitly.
        if self.view.entry.is_some() {
            return;
        }
        match self.view.screen {
            Screen::Home => match self.view.mode {
                Mode::Channel => {
                    self.view.channel = self.ordered_channel(bitmap, self.view.channel, forward);
                    if self.view.channel.is_none() {
                        self.view.notice = Notice::NoChannels;
                    }
                }
                Mode::Vfo => {
                    let step = STEP_10HZ[self.view.step_index as usize];
                    // On VFO Up increases frequency; Down decreases it.
                    let next = if forward {
                        self.view.vfo_10hz.checked_sub(step)
                    } else {
                        self.view.vfo_10hz.checked_add(step)
                    };
                    if let Some(next) = next.filter(|&value| valid_frequency(value)) {
                        self.view.vfo_10hz = next;
                    } else {
                        self.view.notice = Notice::InvalidFrequency;
                    }
                }
            },
            Screen::Menu => {
                self.view.menu_index = adjacent_index(
                    self.view.menu_index,
                    menu_entries(self.view.menu_group).len() as u8,
                    forward,
                )
            }
            Screen::Spectrum => {
                self.view.spectrum_cursor = if forward {
                    self.view.spectrum_cursor.saturating_add(1).min(80)
                } else {
                    self.view.spectrum_cursor.saturating_sub(1)
                };
            }
            Screen::Channels => {
                self.view.list_channel =
                    self.ordered_channel(bitmap, self.view.list_channel, forward)
            }
            Screen::ChannelDetails => {
                self.view.channel = self.ordered_channel(bitmap, self.view.channel, forward)
            }
            Screen::Step => {
                self.view.step_index =
                    adjacent_index(self.view.step_index, STEP_10HZ.len() as u8, forward)
            }
            Screen::Shortcuts => {
                self.view.binding_index = adjacent_index(
                    self.view.binding_index,
                    DEFAULT_BINDINGS.len() as u8,
                    forward,
                )
            }
            Screen::ShortcutAction => {
                self.view.binding_action_index = adjacent_index(
                    self.view.binding_action_index,
                    SHORTCUT_ACTIONS.len() as u8,
                    forward,
                )
            }
            Screen::Zones => self.navigate_zone(forward),
            Screen::SideKeys => {
                self.view.side_index = adjacent_index(self.view.side_index, 2, forward)
            }
            Screen::SideAction => {
                self.view.side_action_index = adjacent_index(
                    self.view.side_action_index,
                    SIDE_ACTIONS.len() as u8,
                    forward,
                )
            }
            Screen::Setting => {
                if let Some(setting) = self.view.setting {
                    self.view.setting_index =
                        adjacent_index(self.view.setting_index, setting.choice_count(), forward);
                }
            }
            _ => {}
        }
    }

    fn confirm(&mut self, bitmap: &[u8; 128]) {
        if let Some(entry) = self.view.entry {
            match entry.kind {
                EntryKind::Channel => {
                    if entry.value == 0 || entry.value > CHANNEL_COUNT as u32 {
                        self.view.notice = Notice::InvalidChannel;
                        return;
                    }
                    let index = (entry.value - 1) as u16;
                    if !channel_active(bitmap, index) {
                        self.view.notice = Notice::InactiveChannel;
                        return;
                    }
                    self.view.channel = Some(index);
                    self.view.mode = Mode::Channel;
                }
                EntryKind::Frequency => {
                    // Six ordinary keypad digits name MHz with three decimals;
                    // seven/eight digits retain finer precision. Leading zeroes
                    // remain significant through Entry::digits.
                    let value = match entry.digits {
                        6 => entry.value * 100,
                        7 => entry.value * 10,
                        8 => entry.value,
                        _ => {
                            self.view.notice = Notice::InvalidFrequency;
                            return;
                        }
                    };
                    if !valid_frequency(value) {
                        self.view.notice = Notice::InvalidFrequency;
                        return;
                    }
                    self.view.vfo_10hz = value;
                    self.view.mode = Mode::Vfo;
                }
            }
            self.view.entry = None;
            self.view.screen = Screen::Home;
            return;
        }
        match self.view.screen {
            Screen::Home => {
                self.view.screen = Screen::Menu;
                self.view.menu_group = None;
                self.view.menu_index = 0;
            }
            Screen::Menu => {
                let entry = menu_entries(self.view.menu_group)[self.view.menu_index as usize];
                let MenuEntry::Item(item) = entry else {
                    if let MenuEntry::Group(group) = entry {
                        self.view.menu_group = Some(group);
                        self.view.menu_index = 0;
                    }
                    return;
                };
                match item {
                    MenuItem::Mode => self.open_setting(Setting::Mode),
                    MenuItem::Channels => self.open_channels(bitmap),
                    MenuItem::ChannelDetails => self.view.screen = Screen::ChannelDetails,
                    MenuItem::Step => self.view.screen = Screen::Step,
                    MenuItem::Calibration => self.view.screen = Screen::Calibration,
                    MenuItem::Keys => self.view.screen = Screen::Keys,
                    MenuItem::Help => self.view.screen = Screen::Help,
                    MenuItem::About => self.view.screen = Screen::About,
                    MenuItem::Reverse => self.open_setting(Setting::Reverse),
                    MenuItem::Scan => self.open_setting(Setting::Scan),
                    MenuItem::Shortcuts => self.view.screen = Screen::Shortcuts,
                    MenuItem::Power => self.open_setting(Setting::Power),
                    MenuItem::Monitor => self.open_setting(Setting::Monitor),
                    MenuItem::Modulation => self.open_setting(Setting::Modulation),
                    MenuItem::Zones => self.open_zones(),
                    MenuItem::ActiveSlot => self.open_setting(Setting::ActiveSlot),
                    MenuItem::SideKeys => self.view.screen = Screen::SideKeys,
                    MenuItem::DisplayLight => self.open_setting(Setting::DisplayLight),
                    MenuItem::KeyLight => self.open_setting(Setting::KeyLight),
                    MenuItem::Spectrum => self.open_spectrum(),
                }
            }
            Screen::Spectrum => self.queue_request(Request::AcceptSpectrum {
                bin: self.view.spectrum_cursor,
            }),
            Screen::Channels => {
                if let Some(slot) = self
                    .view
                    .list_channel
                    .filter(|&slot| channel_active(bitmap, slot))
                {
                    self.view.channel = Some(slot);
                    self.view.mode = Mode::Channel;
                    self.view.screen = Screen::Home;
                } else {
                    self.view.notice = Notice::NoChannels;
                }
            }
            Screen::Step => self.view.screen = Screen::Home,
            Screen::Shortcuts => self.open_binding_action(),
            Screen::ShortcutAction => {
                self.view.bindings[self.view.binding_index as usize] =
                    SHORTCUT_ACTIONS[self.view.binding_action_index as usize];
                self.view.screen = Screen::Shortcuts;
            }
            Screen::Zones => {
                if self.view.zone != self.view.zone_cursor {
                    self.view.zone = self.view.zone_cursor;
                    self.view.reverse = false;
                    self.view.channel = None;
                    self.view.list_channel = None;
                    self.channel_order_len = 0;
                }
                self.view.screen = Screen::Home;
            }
            Screen::SideKeys => self.open_side_action(),
            Screen::SideAction => {
                self.set_side_binding(
                    self.view.side_index as usize,
                    SIDE_ACTIONS[self.view.side_action_index as usize],
                );
                self.view.screen = Screen::SideKeys;
            }
            Screen::Setting => self.commit_setting(bitmap),
            // A test page must allow MENU/EXIT to leave; other keys stay visible.
            Screen::ChannelDetails
            | Screen::Calibration
            | Screen::Keys
            | Screen::Help
            | Screen::About => self.view.screen = Screen::Menu,
        }
    }

    fn back(&mut self) {
        if self.view.entry.is_some() {
            self.view.entry = None;
            return;
        }
        self.view.screen = match self.view.screen {
            Screen::Home => Screen::Home,
            Screen::Menu if self.view.menu_group.is_some() => {
                let group = self.view.menu_group.take().unwrap();
                self.view.menu_index = menu_entries(None)
                    .iter()
                    .position(|entry| *entry == MenuEntry::Group(group))
                    .unwrap_or(0) as u8;
                Screen::Menu
            }
            Screen::Menu => Screen::Home,
            Screen::ShortcutAction => Screen::Shortcuts,
            Screen::SideAction => Screen::SideKeys,
            _ => Screen::Menu,
        };
    }

    fn enter_digit(&mut self, digit: u8) {
        if self.view.screen != Screen::Home {
            return;
        }
        let kind = match self.view.mode {
            Mode::Channel => EntryKind::Channel,
            Mode::Vfo => EntryKind::Frequency,
        };
        let mut entry = self.view.entry.unwrap_or(Entry {
            kind,
            value: 0,
            digits: 0,
        });
        let max = match entry.kind {
            EntryKind::Channel => 4,
            EntryKind::Frequency => 8,
        };
        if entry.digits >= max {
            self.view.notice = Notice::EntryFull;
            return;
        }
        entry.value = entry.value * 10 + u32::from(digit);
        entry.digits += 1;
        self.view.entry = Some(entry);
    }
}

const fn valid_frequency(value: u32) -> bool {
    value >= MIN_FREQUENCY_10HZ && value <= MAX_FREQUENCY_10HZ
}

pub const fn binding_index(digit: u8) -> Option<u8> {
    match digit {
        0 => Some(9),
        1..=9 => Some(digit - 1),
        _ => None,
    }
}
pub const fn binding_digit(index: u8) -> Option<u8> {
    match index {
        0..=8 => Some(index + 1),
        9 => Some(0),
        _ => None,
    }
}

const fn adjacent_index(index: u8, len: u8, forward: bool) -> u8 {
    if forward {
        (index + 1) % len
    } else {
        (index + len - 1) % len
    }
}

pub const fn digit(key: Key) -> Option<u8> {
    match key {
        Key::Digit0 => Some(0),
        Key::Digit1 => Some(1),
        Key::Digit2 => Some(2),
        Key::Digit3 => Some(3),
        Key::Digit4 => Some(4),
        Key::Digit5 => Some(5),
        Key::Digit6 => Some(6),
        Key::Digit7 => Some(7),
        Key::Digit8 => Some(8),
        Key::Digit9 => Some(9),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(slots: &[u16]) -> [u8; 128] {
        let mut bitmap = [0; 128];
        for &slot in slots {
            bitmap[slot as usize / 8] |= 1 << (slot % 8);
        }
        bitmap
    }
    fn press(controller: &mut Controller, key: Key, bitmap: &[u8; 128]) -> Change {
        controller.handle(InputEvent::Short(key), bitmap)
    }
    fn digits(controller: &mut Controller, keys: &[Key], bitmap: &[u8; 128]) {
        for &key in keys {
            press(controller, key, bitmap);
        }
    }

    #[test]
    fn empty_sparse_and_single_channel_navigation_are_bounded() {
        let empty = [0; 128];
        let mut ui = Controller::new(&empty, 0);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.channel, None);
        assert_eq!(ui.view.vfo_10hz, DEFAULT_FREQUENCY_10HZ);
        press(&mut ui, Key::Star, &empty);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.notice, Notice::NoChannels);
        let bitmap = catalog(&[3, 900, 1023]);
        assert_eq!(active_count(&bitmap), 3);
        assert_eq!(first_channel(&bitmap), Some(3));
        assert_eq!(adjacent_channel(&bitmap, Some(3), false), Some(1023));
        assert_eq!(adjacent_channel(&bitmap, Some(1023), true), Some(3));
        assert_eq!(adjacent_channel(&catalog(&[7]), Some(7), true), Some(7));
        assert!(!channel_active(&bitmap, u16::MAX));
    }

    #[test]
    fn channel_entry_is_explicit_and_inactive_slots_do_not_replace_selection() {
        let bitmap = catalog(&[0, 2, 1023]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        press(&mut ui, Key::Digit2, &bitmap);
        assert_eq!(ui.view.channel, Some(0));
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.notice, Notice::InactiveChannel);
        assert_eq!(ui.view.channel, Some(0));
        press(&mut ui, Key::Exit, &bitmap);
        press(&mut ui, Key::Digit3, &bitmap);
        let change = press(&mut ui, Key::Menu, &bitmap);
        assert!(change.channel_changed);
        assert_eq!(ui.view.channel, Some(2));
        digits(
            &mut ui,
            &[Key::Digit1, Key::Digit0, Key::Digit2, Key::Digit4],
            &bitmap,
        );
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.channel, Some(1023));
    }

    #[test]
    fn frequency_requires_six_to_eight_digits_bounds_and_explicit_commit() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        press(&mut ui, Key::Star, &bitmap);
        digits(&mut ui, &[Key::Digit4, Key::Digit3, Key::Digit3], &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.notice, Notice::InvalidFrequency);
        assert_eq!(ui.view.vfo_10hz, DEFAULT_FREQUENCY_10HZ);
        digits(
            &mut ui,
            &[
                Key::Digit5,
                Key::Digit0,
                Key::Digit0,
                Key::Digit0,
                Key::Digit0,
            ],
            &bitmap,
        );
        assert_eq!(ui.view.vfo_10hz, DEFAULT_FREQUENCY_10HZ);
        press(&mut ui, Key::Digit9, &bitmap);
        assert_eq!(ui.view.notice, Notice::EntryFull);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.vfo_10hz, 43_350_000);
        assert_eq!(ui.view.entry, None);
        press(&mut ui, Key::Up, &bitmap);
        assert_eq!(ui.view.vfo_10hz, 43_351_250);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.vfo_10hz, 43_350_000);
    }

    #[test]
    fn ordinary_six_digit_and_fine_seven_digit_entries_pad_on_commit() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        press(&mut ui, Key::Star, &bitmap);
        digits(
            &mut ui,
            &[
                Key::Digit1,
                Key::Digit4,
                Key::Digit5,
                Key::Digit5,
                Key::Digit0,
                Key::Digit0,
            ],
            &bitmap,
        );
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.vfo_10hz, 14_550_000);
        digits(
            &mut ui,
            &[
                Key::Digit1,
                Key::Digit4,
                Key::Digit5,
                Key::Digit5,
                Key::Digit1,
                Key::Digit2,
                Key::Digit5,
            ],
            &bitmap,
        );
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.vfo_10hz, 14_551_250);
        digits(
            &mut ui,
            &[
                Key::Digit0,
                Key::Digit0,
                Key::Digit0,
                Key::Digit1,
                Key::Digit0,
                Key::Digit0,
            ],
            &bitmap,
        );
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.vfo_10hz, MIN_FREQUENCY_10HZ);
        digits(
            &mut ui,
            &[
                Key::Digit0,
                Key::Digit0,
                Key::Digit0,
                Key::Digit0,
                Key::Digit9,
                Key::Digit9,
            ],
            &bitmap,
        );
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.notice, Notice::InvalidFrequency);
        assert_eq!(ui.view.vfo_10hz, MIN_FREQUENCY_10HZ);
    }

    #[test]
    fn locking_blocks_navigation_entry_and_mode_but_preserves_ptt_feedback() {
        let bitmap = catalog(&[0, 1]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        for event in [
            InputEvent::Short(Key::Down),
            InputEvent::Short(Key::Digit1),
            InputEvent::Hold(Key::Menu),
            InputEvent::Hold(Key::Exit),
        ] {
            ui.handle(event, &bitmap);
        }
        assert!(ui.view.locked);
        assert_eq!(ui.view.channel, Some(0));
        assert_eq!(ui.view.mode, Mode::Channel);
        assert_eq!(ui.view.entry, None);
        ui.handle(InputEvent::Ptt(true), &bitmap);
        assert!(ui.view.ptt_down);
        assert_eq!(ui.view.notice, Notice::Locked);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        assert!(!ui.view.ptt_down);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert!(!ui.view.locked);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.channel, Some(1));
    }

    #[test]
    fn channel_list_selection_is_preview_until_confirmed_and_exit_cancels() {
        let bitmap = catalog(&[0, 10, 99]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Channels, &bitmap);
        assert_eq!(ui.view.screen, Screen::Channels);
        assert!(!press(&mut ui, Key::Down, &bitmap).channel_changed);
        assert_eq!(ui.view.list_channel, Some(10));
        assert_eq!(ui.view.channel, Some(0));
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert!(press(&mut ui, Key::Menu, &bitmap).channel_changed);
        assert_eq!(ui.view.channel, Some(10));
        assert_eq!(ui.view.screen, Screen::Home);
    }

    #[test]
    fn hold_exit_discards_partial_entry_and_long_menu_toggles_only_once() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        press(&mut ui, Key::Digit1, &bitmap);
        press(&mut ui, Key::Digit0, &bitmap);
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        assert_eq!(ui.view.channel, Some(0));
        assert_eq!(ui.view.entry, None);
        assert_eq!(ui.view.screen, Screen::Home);
        ui.handle(InputEvent::Hold(Key::Menu), &bitmap);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.screen, Screen::Home);
    }

    fn select_reverse(ui: &mut Controller, bitmap: &[u8; 128]) {
        choose_menu(ui, MenuItem::Reverse, bitmap);
        assert_eq!(ui.view.setting, Some(Setting::Reverse));
        press(ui, Key::Down, bitmap);
        press(ui, Key::Menu, bitmap);
        // Existing reversal assertions operate on the Home preview.
        ui.view.screen = Screen::Home;
    }

    #[test]
    fn reverse_toggles_only_the_current_channel_preview_and_resets_on_navigation() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert!(!ui.view.reverse);
        select_reverse(&mut ui, &bitmap);
        assert!(ui.view.reverse);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.view.channel, Some(0));
        assert_eq!(ui.view.mode, Mode::Channel);
        assert_eq!(ui.view.vfo_10hz, DEFAULT_FREQUENCY_10HZ);
        select_reverse(&mut ui, &bitmap);
        assert!(!ui.view.reverse);
        select_reverse(&mut ui, &bitmap);
        assert!(ui.view.reverse);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.channel, Some(2));
        assert!(!ui.view.reverse);
        select_reverse(&mut ui, &bitmap);
        press(&mut ui, Key::Star, &bitmap);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert!(!ui.view.reverse);
    }

    #[test]
    fn reverse_resets_on_direct_channel_commit_and_vfo_rejects_it() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        select_reverse(&mut ui, &bitmap);
        press(&mut ui, Key::Digit3, &bitmap);
        assert!(ui.view.reverse);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.channel, Some(2));
        assert!(!ui.view.reverse);
        press(&mut ui, Key::Star, &bitmap);
        select_reverse(&mut ui, &bitmap);
        assert_eq!(ui.view.notice, Notice::ChannelOnly);
        assert!(!ui.view.reverse);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.vfo_10hz, DEFAULT_FREQUENCY_10HZ);
        let empty = [0; 128];
        let mut empty_ui = Controller::new(&empty, DEFAULT_FREQUENCY_10HZ);
        select_reverse(&mut empty_ui, &empty);
        assert_eq!(empty_ui.view.notice, Notice::ChannelOnly);
        assert!(!empty_ui.view.reverse);
    }

    #[test]
    fn lock_ignores_reverse_navigation_and_ptt_does_not_change_reversal() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        select_reverse(&mut ui, &bitmap);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert!(ui.view.locked);
        for key in [Key::Menu, Key::Down, Key::Menu, Key::Star] {
            press(&mut ui, key, &bitmap);
        }
        assert!(ui.view.reverse);
        assert_eq!(ui.view.channel, Some(0));
        assert_eq!(ui.view.mode, Mode::Channel);
        assert_eq!(ui.view.screen, Screen::Home);
        ui.handle(InputEvent::Ptt(true), &bitmap);
        assert_eq!(ui.view.notice, Notice::Locked);
        assert!(ui.view.reverse);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert!(!ui.view.locked);
        assert!(ui.view.reverse);
        select_reverse(&mut ui, &bitmap);
        assert!(!ui.view.reverse);
    }

    fn choose_menu(ui: &mut Controller, item: MenuItem, bitmap: &[u8; 128]) -> Change {
        ui.handle(InputEvent::Hold(Key::Exit), bitmap);
        press(ui, Key::Menu, bitmap);
        let (group, index) = menu_location(item);
        if let Some(group) = group {
            while menu_entries(None)[ui.view.menu_index as usize] != MenuEntry::Group(group) {
                press(ui, Key::Down, bitmap);
            }
            press(ui, Key::Menu, bitmap);
        }
        for _ in 0..index {
            press(ui, Key::Down, bitmap);
        }
        assert_eq!(
            menu_entries(group)[ui.view.menu_index as usize],
            MenuEntry::Item(item)
        );
        press(ui, Key::Menu, bitmap)
    }

    #[test]
    fn categories_retain_every_item_and_exit_returns_through_the_selected_parent() {
        let bitmap = catalog(&[0]);
        assert_eq!(menu_entries(None)[0], MenuEntry::Item(MenuItem::Modulation));
        for &item in &MENU_ITEMS {
            let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
            let original = ui.context(Slot::A);
            let (group, index) = menu_location(item);
            assert!(menu_entries(group).len() <= 7);
            choose_menu(&mut ui, item, &bitmap);
            assert_eq!(ui.context(Slot::A), original);
            assert_ne!(ui.view.screen, Screen::Menu);
            press(&mut ui, Key::Exit, &bitmap);
            assert_eq!(ui.view.screen, Screen::Menu);
            assert_eq!((ui.view.menu_group, ui.view.menu_index), (group, index));
            press(&mut ui, Key::Exit, &bitmap);
            if let Some(group) = group {
                assert_eq!(ui.view.screen, Screen::Menu);
                assert_eq!(ui.view.menu_group, None);
                assert_eq!(
                    menu_entries(None)[ui.view.menu_index as usize],
                    MenuEntry::Group(group)
                );
                press(&mut ui, Key::Exit, &bitmap);
            }
            assert_eq!(ui.view.screen, Screen::Home);
            assert_eq!(ui.context(Slot::A), original);
        }
    }

    #[test]
    fn spectrum_cursor_requests_are_bounded_and_only_restore_ack_commits_frequency() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let old_a = ui.context(Slot::A);
        let old_b = ui.context(Slot::B);
        let start = choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        assert!(start.cancel_temporary);
        assert_eq!(start.request, Some(Request::StartSpectrum));
        assert_eq!(ui.view.screen, Screen::Spectrum);
        assert_eq!(ui.view.spectrum_cursor, 40);
        for _ in 0..100 {
            press(&mut ui, Key::Up, &bitmap);
        }
        assert_eq!(ui.view.spectrum_cursor, 0);
        for _ in 0..100 {
            press(&mut ui, Key::Down, &bitmap);
        }
        assert_eq!(ui.view.spectrum_cursor, 80);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::AcceptSpectrum { bin: 80 })
        );
        assert_eq!(ui.view.screen, Screen::Spectrum);
        assert_eq!(ui.context(Slot::A), old_a);
        ui.accept_spectrum_frequency(0);
        assert_eq!(ui.context(Slot::A), old_a);
        ui.accept_spectrum_frequency(43_297_500);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.vfo_10hz, 43_297_500);
        assert_eq!(ui.context(Slot::B), old_b);
    }

    #[test]
    fn spectrum_ptt_only_cancels_and_does_not_inherit_permission_after_page_exit() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        let old = ui.context(Slot::A);
        let ptt = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert_eq!(ptt.request, Some(Request::StopSpectrum));
        assert!(ptt.cancel_temporary);
        assert!(!ptt.ptt_tx_requested);
        assert_eq!(ui.view.ptt_slot, None);
        ui.accept_spectrum_frequency(43_297_500);
        assert_eq!(ui.context(Slot::A), old); // PTT cancellation wins over stale ACK.
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Exit), &bitmap).request,
            Some(Request::StopSpectrum)
        );
        assert_eq!(ui.view.screen, Screen::Home);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        assert!(ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
    }

    #[test]
    fn spectrum_start_keeps_scan_cleanup_and_backend_request_together() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let (group, index) = menu_location(MenuItem::Spectrum);
        ui.view.screen = Screen::Menu;
        ui.view.menu_group = group;
        ui.view.menu_index = index;
        ui.view.scan = ScanState::Running;
        let start = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(start.request, Some(Request::StartSpectrum));
        assert!(start.cancel_temporary);
        assert_eq!(ui.view.scan, ScanState::Stopping);
        ui.acknowledge_scan_stopped();
        ui.reject_radio_request(Request::StartSpectrum);
        assert_eq!(ui.view.notice, Notice::RadioUnavailable);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.menu_group, Some(MenuGroup::Scanning));
        assert_eq!(
            menu_entries(ui.view.menu_group)[ui.view.menu_index as usize],
            MenuEntry::Item(MenuItem::Spectrum)
        );
        assert_eq!(press(&mut ui, Key::Exit, &bitmap).request, None);
        assert_eq!(ui.view.mode, Mode::Channel);
    }

    #[test]
    fn long_digit_bindings_are_volatile_configurable_and_do_not_edit_fields() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert_eq!(ui.view.bindings, DEFAULT_BINDINGS);
        assert_eq!(ui.binding(3), Some(ShortcutAction::Scan));
        assert!(!ui.set_binding(10, ShortcutAction::Power));
        assert_eq!(ui.binding(0), Some(ShortcutAction::Spectrum));
        assert!(ui.set_binding(1, ShortcutAction::Monitor));
        let change = ui.handle(InputEvent::Hold(Key::Digit1), &bitmap);
        assert_eq!(change.request, Some(Request::ToggleMonitor));
        assert_eq!(ui.view.entry, None);
        press(&mut ui, Key::Digit2, &bitmap);
        let before = ui.view.entry;
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit1), &bitmap).request,
            None
        );
        assert_eq!(ui.view.entry, before);
        choose_menu(&mut ui, MenuItem::Keys, &bitmap);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit3), &bitmap).request,
            None
        );
        assert_eq!(ui.view.scan, ScanState::Stopped);
    }

    #[test]
    fn zero_binding_defaults_to_spectrum_is_editable_and_respects_lock_and_draft() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert_eq!(binding_index(0), Some(9));
        assert_eq!(binding_digit(9), Some(0));
        for digit in 0..=9 {
            assert_eq!(binding_digit(binding_index(digit).unwrap()), Some(digit));
        }
        assert_eq!(binding_index(10), None);
        assert_eq!(binding_digit(10), None);
        assert_eq!(ui.binding(0), Some(ShortcutAction::Spectrum));
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit0), &bitmap).request,
            None
        );
        assert_eq!(ui.view.screen, Screen::Home);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        press(&mut ui, Key::Digit1, &bitmap);
        let entry = ui.view.entry;
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit0), &bitmap).request,
            None
        );
        assert_eq!(ui.view.entry, entry);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit0), &bitmap).request,
            Some(Request::StartSpectrum)
        );
        assert_eq!(ui.view.entry, None);
        assert_eq!(ui.view.screen, Screen::Spectrum);
        choose_menu(&mut ui, MenuItem::Shortcuts, &bitmap);
        press(&mut ui, Key::Digit0, &bitmap);
        assert_eq!(ui.view.binding_index, 9);
        assert_eq!(ui.view.screen, Screen::ShortcutAction);
        press(&mut ui, Key::Down, &bitmap); // Spectrum is last; wrap to None.
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.binding(0), Some(ShortcutAction::None));
        assert_eq!(&ui.view.bindings[..9], &DEFAULT_BINDINGS[..9]);
        assert!(ui.set_binding(4, ShortcutAction::Spectrum));
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit0), &bitmap).request,
            None
        );
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit4), &bitmap).request,
            Some(Request::StartSpectrum)
        );
    }

    #[test]
    fn binding_menu_commits_only_on_confirm_and_long_lock_can_unlock() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Shortcuts, &bitmap);
        press(&mut ui, Key::Digit3, &bitmap);
        assert_eq!(ui.view.binding_index, 2);
        assert_eq!(ui.view.screen, Screen::ShortcutAction);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.binding(3), Some(ShortcutAction::Scan));
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.binding(3), Some(ShortcutAction::Reverse));
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        ui.handle(InputEvent::Hold(Key::Digit9), &bitmap);
        assert!(ui.view.locked);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit1), &bitmap).request,
            None
        );
        ui.handle(InputEvent::Hold(Key::Digit9), &bitmap);
        assert!(!ui.view.locked);
    }

    #[test]
    fn scan_needs_ack_and_cancel_rejects_late_start_without_claiming_stop() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit3), &bitmap).request,
            Some(Request::StartScan)
        );
        assert_eq!(ui.view.scan, ScanState::Requested);
        assert_eq!(ui.cancel_scan().request, Some(Request::StopScan));
        assert!(!ui.acknowledge_scan_started());
        ui.reject_radio_request(Request::StopScan);
        assert_eq!(ui.view.scan, ScanState::Stopping);
        assert_eq!(ui.view.notice, Notice::RadioUnavailable);
        ui.acknowledge_scan_stopped();
        assert_eq!(ui.view.scan, ScanState::Stopped);
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        ui.reject_radio_request(Request::StartScan);
        assert_eq!(ui.view.scan, ScanState::Stopped);
        assert_eq!(ui.view.notice, Notice::RadioUnavailable);
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        assert!(ui.acknowledge_scan_started());
        assert_eq!(ui.view.scan, ScanState::Running);
        assert!(!ui.acknowledge_scan_started());
    }

    #[test]
    fn scan_ptt_press_only_stops_and_requires_release_before_a_new_tx_intent() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        ui.acknowledge_scan_started();
        let change = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert_eq!(change.request, Some(Request::StopScan));
        assert!(!change.ptt_tx_requested);
        assert_eq!(ui.view.ptt_slot, None);
        ui.acknowledge_scan_stopped();
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        let change = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert!(change.ptt_tx_requested);
        assert_eq!(change.ptt_slot, Some(Slot::A));
        assert_eq!(ui.view.ptt_slot, Some(Slot::A));
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        assert_eq!(ui.view.ptt_slot, None);
    }

    #[test]
    fn manual_changes_cancel_scan_and_stop_request_has_priority() {
        let bitmap = catalog(&[0, 2]);
        for event in [
            InputEvent::Short(Key::Down),
            InputEvent::Short(Key::Star),
            InputEvent::Short(Key::Digit1),
            InputEvent::Hold(Key::Digit4),
            InputEvent::Hold(Key::Hash),
            InputEvent::Short(Key::Exit),
            InputEvent::Hold(Key::Star),
        ] {
            let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
            ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
            ui.acknowledge_scan_started();
            let change = ui.handle(event, &bitmap);
            assert_eq!(change.request, Some(Request::StopScan), "{event:?}");
            assert_eq!(ui.view.scan, ScanState::Stopping);
            assert!(!change.ptt_tx_requested);
        }
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.queue_request(Request::StopScan);
        ui.queue_request(Request::CyclePower);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Digit2), &bitmap).request,
            Some(Request::StopScan)
        );
    }

    #[test]
    fn power_monitor_and_modulation_requests_never_claim_backend_success() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        for (item, request) in [
            (MenuItem::Power, Request::SetPower(1)),
            (MenuItem::Monitor, Request::SetMonitor(true)),
            (MenuItem::Modulation, Request::SetModulation { am: true }),
        ] {
            assert_eq!(choose_menu(&mut ui, item, &bitmap).request, None);
            assert_eq!(press(&mut ui, Key::Down, &bitmap).request, None);
            assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, Some(request));
            ui.reject_radio_request(request);
            assert_eq!(ui.view.notice, Notice::RadioUnavailable);
            assert_eq!(ui.view.scan, ScanState::Stopped);
        }
    }

    #[test]
    fn two_side_keys_and_lock_cannot_retrigger_tx_until_both_are_released() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert_eq!(
            ui.handle(InputEvent::PttB(true), &bitmap).ptt_slot,
            Some(Slot::B)
        );
        assert!(ui.view.ptt_down);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        assert_eq!(ui.view.ptt_slot, None);
        ui.handle(InputEvent::PttB(false), &bitmap);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        assert!(ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert_eq!(ui.view.ptt_slot, None);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        assert!(ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        press(&mut ui, Key::Star, &bitmap);
        assert_eq!(ui.view.ptt_slot, None);
    }

    #[test]
    fn second_side_can_be_a_function_and_remapping_held_input_never_starts_tx() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert!(!ui.set_side_binding(2, SideAction::Active));
        assert!(ui.set_side_binding(1, SideAction::Monitor));
        let change = ui.handle(InputEvent::PttB(true), &bitmap);
        assert_eq!(change.request, Some(Request::ToggleMonitor));
        assert!(!change.ptt_tx_requested);
        assert_eq!(ui.view.ptt_slot, None);
        ui.set_side_binding(1, SideAction::PttB);
        assert!(!ui.handle(InputEvent::PttB(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::PttB(false), &bitmap);
        assert_eq!(
            ui.handle(InputEvent::PttB(true), &bitmap).ptt_slot,
            Some(Slot::B)
        );
        ui.handle(InputEvent::PttB(false), &bitmap);
        ui.set_side_binding(1, SideAction::Scan);
        assert_eq!(
            ui.handle(InputEvent::PttB(true), &bitmap).request,
            Some(Request::StartScan)
        );
        assert!(ui.acknowledge_scan_started());
        ui.handle(InputEvent::PttB(false), &bitmap);
        assert_eq!(
            ui.handle(InputEvent::PttB(true), &bitmap).request,
            Some(Request::StopScan)
        );
    }

    #[test]
    fn side_binding_menu_cancel_and_commit_are_distinct() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::SideKeys, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.screen, Screen::SideAction);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.side_bindings[1], SideAction::PttB);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.side_bindings[1], SideAction::Active);
    }

    #[test]
    fn sparse_zones_preview_commit_and_reconcile_preserve_stock_order() {
        let bitmap = catalog(&[0, 2, 20, 999]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let mut zones = [0; 32];
        zones[0] = 1 << 2;
        zones[31] = 1 << 7;
        ui.set_zone_catalog(&zones);
        choose_menu(&mut ui, MenuItem::Zones, &bitmap);
        press(&mut ui, Key::Up, &bitmap);
        assert_eq!(ui.view.zone_cursor, Some(255));
        assert_eq!(ui.view.zone, None);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.zone, None);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.zone_cursor, Some(2));
        let change = press(&mut ui, Key::Menu, &bitmap);
        assert!(change.zone_changed);
        assert_eq!(ui.view.channel, None);
        assert_eq!(ui.set_channel_order(&[20, 0, 999, 20, 1024]), 3);
        ui.reconcile_channels(&bitmap);
        assert_eq!(ui.view.channel, Some(20));
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.channel, Some(0));
        press(&mut ui, Key::Up, &bitmap);
        assert_eq!(ui.view.channel, Some(20));
        press(&mut ui, Key::Up, &bitmap);
        assert_eq!(ui.view.channel, Some(999));
        ui.set_channel_order(&[]);
        ui.reconcile_channels(&bitmap);
        assert_eq!(ui.view.channel, None); // Empty selected zone cannot become All.
        assert_eq!(ui.ordered_channel(&bitmap, None, true), None);
        ui.set_zone_catalog(&[0; 32]);
        assert_eq!(ui.view.zone, None);
        ui.reconcile_channels(&bitmap);
        assert_eq!(ui.view.channel, Some(0));
    }

    #[test]
    fn contexts_a_b_restore_independent_mode_channel_frequency_zone_step_and_reverse() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.zone = Some(9);
        ui.set_channel_order(&[2, 0]);
        press(&mut ui, Key::Down, &bitmap);
        ui.handle(InputEvent::Hold(Key::Digit4), &bitmap);
        ui.view.step_index = 2;
        let a = ui.context(Slot::A);
        let change = ui.handle(InputEvent::Hold(Key::Star), &bitmap);
        assert!(change.slot_changed && change.zone_changed);
        assert_eq!(ui.view.active_slot, Slot::B);
        assert_eq!(ui.channel_order(), &[]);
        press(&mut ui, Key::Star, &bitmap);
        press(&mut ui, Key::Up, &bitmap);
        let b = ui.context(Slot::B);
        assert_eq!(b.mode, Mode::Vfo);
        assert_ne!(a.vfo_10hz, b.vfo_10hz);
        ui.select_slot(Slot::A);
        assert_eq!(ui.context(Slot::A), a);
        assert_eq!(ui.context(Slot::B), b);
        assert!(ui.view.reverse);
        ui.select_slot(Slot::B);
        assert_eq!(ui.context(Slot::B), b);
        assert_eq!(ui.view.zone, None);
    }

    #[test]
    fn automatic_scan_cursors_preserve_scan_but_manual_navigation_cancels() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert!(!ui.select_scan_frequency(43_350_000).redraw);
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        assert!(!ui.select_scan_frequency(43_350_000).redraw); // Channel scan.
        let step = ui.select_scan_channel(2, &bitmap);
        assert!(step.channel_changed);
        assert_eq!(step.request, None);
        assert_eq!(ui.view.scan, ScanState::Requested);
        assert!(!ui.select_scan_channel(1, &bitmap).channel_changed);
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Star), &bitmap).request,
            Some(Request::StopScan)
        );
        ui.acknowledge_scan_stopped();
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        ui.acknowledge_scan_started();
        let step = ui.select_scan_frequency(27_185_00);
        assert!(step.redraw);
        assert_eq!(step.request, None);
        assert_eq!(ui.view.vfo_10hz, 27_185_00);
        assert_eq!(ui.view.scan, ScanState::Running);
        for invalid in [0, MIN_FREQUENCY_10HZ - 1, MAX_FREQUENCY_10HZ + 1, u32::MAX] {
            assert!(!ui.select_scan_frequency(invalid).redraw);
            assert_eq!(ui.view.vfo_10hz, 27_185_00);
        }
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Up), &bitmap).request,
            Some(Request::StopScan)
        );
        assert_eq!(ui.view.scan, ScanState::Stopping);
        assert!(!ui.select_scan_frequency(43_350_000).redraw);
    }

    #[test]
    fn rejected_tx_keeps_physical_feedback_but_revokes_intent_until_release() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert!(ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        let rejected = ui.reject_tx();
        assert!(rejected.redraw);
        assert!(!rejected.ptt_tx_requested);
        assert_eq!(rejected.request, None);
        assert_eq!(rejected.ptt_slot, None);
        assert_eq!(ui.view.ptt_slot, None);
        assert!(ui.view.ptt_down);
        assert_eq!(ui.view.side_down, [true, false]);
        assert_eq!(ui.view.notice, Notice::TxDisabled);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        assert_eq!(ui.view.notice, Notice::TxDisabled);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        assert_eq!(ui.view.notice, Notice::None);
        assert!(ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
    }

    #[test]
    fn setting_entry_arrows_and_exit_never_apply_drafts_or_emit_radio_requests() {
        let bitmap = catalog(&[0, 2]);
        for (item, setting) in [
            (MenuItem::Mode, Setting::Mode),
            (MenuItem::Power, Setting::Power),
            (MenuItem::Monitor, Setting::Monitor),
            (MenuItem::Modulation, Setting::Modulation),
            (MenuItem::Reverse, Setting::Reverse),
            (MenuItem::ActiveSlot, Setting::ActiveSlot),
            (MenuItem::Scan, Setting::Scan),
        ] {
            let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
            ui.set_radio_choices(Some(2), true, true);
            let original = ui.context(Slot::A);
            assert_eq!(choose_menu(&mut ui, item, &bitmap).request, None);
            assert_eq!(ui.view.screen, Screen::Setting);
            assert_eq!(ui.view.setting, Some(setting));
            assert_eq!(press(&mut ui, Key::Down, &bitmap).request, None);
            assert_eq!(press(&mut ui, Key::Up, &bitmap).request, None);
            assert_eq!(press(&mut ui, Key::Down, &bitmap).request, None);
            assert_eq!(press(&mut ui, Key::Exit, &bitmap).request, None);
            assert_eq!(ui.view.screen, Screen::Menu);
            assert_eq!(ui.view.setting, None);
            assert_eq!(ui.context(Slot::A), original);
            assert_eq!(ui.view.active_slot, Slot::A);
            assert_eq!(ui.view.scan, ScanState::Stopped);
            assert_eq!(
                (ui.radio_power, ui.radio_monitor, ui.radio_am),
                (Some(2), true, true)
            );
        }
    }

    #[test]
    fn confirmed_radio_updates_seed_future_choices_without_overwriting_a_pending_draft() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.set_radio_choices(Some(2), false, false);
        choose_menu(&mut ui, MenuItem::Power, &bitmap);
        assert_eq!(ui.view.setting_index, 2);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.setting_index, 0);
        ui.set_radio_choices(Some(1), true, true);
        assert_eq!(ui.view.setting_index, 0);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::SetPower(0))
        );
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.radio_power, Some(1)); // Request is not backend acceptance.
        ui.reject_radio_request(Request::SetPower(0));
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.setting_index, 1);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::SetPower(1))
        );
        assert_eq!(ui.radio_power, Some(1));
    }

    #[test]
    fn local_setting_confirmation_applies_the_chosen_value_then_returns_to_menu() {
        let bitmap = catalog(&[0, 2]);
        for (item, setting) in [
            (MenuItem::Mode, Setting::Mode),
            (MenuItem::Reverse, Setting::Reverse),
            (MenuItem::ActiveSlot, Setting::ActiveSlot),
        ] {
            let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
            choose_menu(&mut ui, item, &bitmap);
            press(&mut ui, Key::Down, &bitmap);
            let change = press(&mut ui, Key::Menu, &bitmap);
            assert_eq!(change.request, None);
            assert_eq!(ui.view.screen, Screen::Menu);
            assert_eq!(ui.view.setting, None);
            match setting {
                Setting::Mode => assert_eq!(ui.view.mode, Mode::Vfo),
                Setting::Reverse => assert!(ui.view.reverse),
                Setting::ActiveSlot => {
                    assert_eq!(ui.view.active_slot, Slot::B);
                    assert!(change.slot_changed);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn scan_setting_browse_preserves_scan_but_exit_cancels_and_confirm_avoids_duplicate_start() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Scan, &bitmap);
        assert_eq!(ui.view.setting_index, 0);
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::StartScan)
        );
        assert!(ui.acknowledge_scan_started());
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(press(&mut ui, Key::Down, &bitmap).request, None); // Browse Shortcuts.
        assert_eq!(press(&mut ui, Key::Up, &bitmap).request, None);
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
        assert_eq!(ui.view.setting_index, 1);
        assert_eq!(press(&mut ui, Key::Up, &bitmap).request, None);
        let cancel = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(cancel.request, Some(Request::StopScan));
        assert!(cancel.cancel_temporary);
        assert_eq!(ui.view.scan, ScanState::Stopping);
        assert_eq!(ui.view.screen, Screen::Menu);
        ui.acknowledge_scan_stopped();
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::StartScan)
        );
        assert!(ui.acknowledge_scan_started());
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.setting_index, 1);
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None); // Already running.
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Up, &bitmap);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::StopScan)
        );
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None); // Stop already pending.
        ui.acknowledge_scan_stopped();
        assert_eq!(ui.view.scan, ScanState::Stopped);
    }

    #[test]
    fn exit_cancels_temporary_state_stepwise_without_reverting_confirmed_context_or_settings() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.set_radio_choices(Some(2), true, true);
        let original_a = ui.context(Slot::A);
        let original_b = ui.context(Slot::B);
        for item in [
            MenuItem::Power,
            MenuItem::Monitor,
            MenuItem::Modulation,
            MenuItem::Reverse,
        ] {
            choose_menu(&mut ui, item, &bitmap);
            press(&mut ui, Key::Down, &bitmap);
            let exit = press(&mut ui, Key::Exit, &bitmap);
            assert!(exit.cancel_temporary);
            assert_eq!(exit.request, None);
            assert_eq!(ui.view.screen, Screen::Menu);
            assert_eq!(ui.view.setting, None);
            assert!(press(&mut ui, Key::Exit, &bitmap).cancel_temporary);
            if ui.view.screen == Screen::Menu {
                assert_eq!(ui.view.menu_group, None);
                assert!(press(&mut ui, Key::Exit, &bitmap).cancel_temporary);
            }
            assert_eq!(ui.view.screen, Screen::Home);
            assert_eq!(ui.context(Slot::A), original_a);
            assert_eq!(ui.context(Slot::B), original_b);
            assert_eq!(
                (ui.radio_power, ui.radio_monitor, ui.radio_am),
                (Some(2), true, true)
            );
        }
        choose_menu(&mut ui, MenuItem::Shortcuts, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        for expected in [Screen::Shortcuts, Screen::Menu, Screen::Menu, Screen::Home] {
            assert!(press(&mut ui, Key::Exit, &bitmap).cancel_temporary);
            assert_eq!(ui.view.screen, expected);
        }
        assert_eq!(ui.view.bindings, DEFAULT_BINDINGS);
        press(&mut ui, Key::Star, &bitmap);
        let frequency = ui.view.vfo_10hz;
        digits(&mut ui, &[Key::Digit4, Key::Digit3, Key::Digit3], &bitmap);
        assert!(press(&mut ui, Key::Exit, &bitmap).cancel_temporary);
        assert_eq!(ui.view.entry, None);
        assert_eq!(ui.view.vfo_10hz, frequency);
        assert!(!press(&mut ui, Key::Menu, &bitmap).cancel_temporary);
    }

    #[test]
    fn locked_exit_still_requests_temporary_cleanup_without_unlocking_or_navigating() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.screen = Screen::Menu;
        ui.view.locked = true;
        ui.view.scan = ScanState::Running;
        let exit = press(&mut ui, Key::Exit, &bitmap);
        assert!(exit.cancel_temporary);
        assert_eq!(exit.request, Some(Request::StopScan));
        assert!(ui.view.locked);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.scan, ScanState::Stopping);
        assert!(!press(&mut ui, Key::Up, &bitmap).cancel_temporary);
    }

    #[test]
    fn lighting_choices_are_global_drafts_and_only_runtime_ack_changes_confirmed_modes() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert_eq!(menu_entries(None)[0], MenuEntry::Item(MenuItem::Modulation));
        assert_eq!(ui.binding(8), Some(ShortcutAction::Keys));
        assert_eq!(
            LIGHT_MODES.map(LightMode::timeout_ms),
            [
                None,
                None,
                Some(5_000),
                Some(10_000),
                Some(20_000),
                Some(30_000),
                Some(60_000)
            ]
        );
        for item in [MenuItem::DisplayLight, MenuItem::KeyLight] {
            choose_menu(&mut ui, item, &bitmap);
            assert_eq!(ui.view.setting_index, 1);
            press(&mut ui, Key::Up, &bitmap);
            assert_eq!(press(&mut ui, Key::Exit, &bitmap).request, None);
            assert_eq!(
                (ui.view.display_light, ui.view.key_light),
                (LightMode::On, LightMode::On)
            );
            press(&mut ui, Key::Menu, &bitmap);
            for _ in 0..3 {
                press(&mut ui, Key::Down, &bitmap);
            }
            assert_eq!(ui.view.setting_index, 4);
            let request = if item == MenuItem::DisplayLight {
                Request::SetDisplayLight(LightMode::Seconds20)
            } else {
                Request::SetKeyLight(LightMode::Seconds20)
            };
            assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, Some(request));
            assert_eq!(
                (ui.view.display_light, ui.view.key_light),
                (LightMode::On, LightMode::On)
            );
            ui.reject_radio_request(request);
            assert_eq!(
                (ui.view.display_light, ui.view.key_light),
                (LightMode::On, LightMode::On)
            );
        }
        ui.set_lighting_modes(LightMode::Seconds20, LightMode::Off);
        ui.select_slot(Slot::B);
        assert_eq!(
            (ui.view.display_light, ui.view.key_light),
            (LightMode::Seconds20, LightMode::Off)
        );
        choose_menu(&mut ui, MenuItem::DisplayLight, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        ui.set_lighting_modes(LightMode::Seconds5, LightMode::On);
        assert_eq!(ui.view.setting_index, 5); // Incoming ack never moves the draft.
    }
}
