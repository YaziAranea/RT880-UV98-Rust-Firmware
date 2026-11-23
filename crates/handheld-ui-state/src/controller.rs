//! Storage-independent UI state. Frequencies and settings here are a volatile
//! preview; no effect tunes RF, transmits, or changes a saved channel record.
use crate::cb::CbChannel;
use crate::input::InputEvent;
use crate::text_entry::{Editor, InputMode, TextValue};
use crate::Language;
use uv98_codeplug::zones::ZONE_COUNT;
use uv98_keys::Key;
use uv98_signaling::mdc1200::UnitId;
pub use uv98_signaling::radio_name::RadioName;
use uv98_signaling::radio_name::NAME_BYTES;

pub const CHANNEL_COUNT: usize = 1024;
const NO_ZONE_PRIORITY: u16 = u16::MAX;
// This is an editor bound, not a claim that the radio supports every frequency.
pub const MIN_FREQUENCY_10HZ: u32 = 10_000;
pub const MAX_FREQUENCY_10HZ: u32 = 99_999_999;
pub const DEFAULT_FREQUENCY_10HZ: u32 = 14_550_000;
pub const SHIFT_10HZ: [i32; 7] = [0, -60_000, 60_000, -160_000, 160_000, -500_000, 500_000];
// Standard CTCSS frequencies in tenths of a hertz. Zero disables TX tone.
pub const TX_CTCSS: [u16; 51] = [
    0, 670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148,
    1188, 1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738, 1773,
    1799, 1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418, 2503,
    2541,
];
pub const STEP_10HZ: [u32; 6] = [250, 500, 625, 1000, 1250, 2500];
/// Spectrum window width in sweep intervals; points = intervals + 1.
pub const SPECTRUM_INTERVALS: [u8; 5] = [4, 10, 20, 40, 80];
/// Items in the spectrum context menu, in display order.
pub const SPECTRUM_MENU_LEN: u8 = 8;
pub const SPECTRUM_MENU_LISTEN: u8 = 0;
pub const SPECTRUM_MENU_ACCEPT_VFO: u8 = 1;
pub const SPECTRUM_MENU_SPAN_WIDER: u8 = 2;
pub const SPECTRUM_MENU_SPAN_NARROWER: u8 = 3;
pub const SPECTRUM_MENU_STEP_BIGGER: u8 = 4;
pub const SPECTRUM_MENU_STEP_SMALLER: u8 = 5;
pub const SPECTRUM_MENU_CLEAR_PEAKS: u8 = 6;
pub const SPECTRUM_MENU_CLOSE: u8 = 7;

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
    pub shift_index: u8,
    pub tx_tone_index: u8,
    pub zone: Option<u8>,
    pub step_index: u8,
    pub reverse: bool,
}

impl Context {
    const MEMORY_POSITION: u32 = 1 << 30;

    fn position(self) -> u32 {
        if self.mode == Mode::Vfo {
            self.vfo_10hz
        } else {
            Self::MEMORY_POSITION
                | (self.zone.map_or(0, |zone| u32::from(zone) + 1) << 11)
                | self.channel.map_or(0, |channel| u32::from(channel) + 1)
        }
    }
}

pub const POSITION_ACTIVE_B: u32 = 1 << 31;

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
    Shortwave,
    Crossband,
    Gps,
    Aprs,
    Cb,
    Mdc,
    RadioName,
    RxToneSearch,
    DtmfMemory,
    AprsRx,
}

pub const DTMF_MEMORY_SLOTS: usize = 3;
pub const DTMF_MEMORY_LENGTH: usize = 12;

/// Compact view of one selected memory slot. All three saved slots stay in
/// Controller RAM; View carries only the page's current preview or draft.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DtmfMemoryPage {
    pub slot: u8,
    pub bytes: [u8; DTMF_MEMORY_LENGTH],
    pub len: u8,
    pub active: bool,
    pub editing: bool,
}

/// Snapshot identifies the receive source for one temporary tone search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxToneSource {
    Channel { slot: Slot, channel: u16 },
    Vfo { slot: Slot, frequency_10hz: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxToneSearch {
    pub source: RxToneSource,
    /// Raw `uv98_codeplug::channels::Tone` encoding; None means still waiting.
    pub result_raw: Option<u16>,
}

impl RxToneSource {
    /// Canonical source snapshot for a plain RX profile. Channel activity and
    /// any hardware-specific RX guard remain the caller's responsibility.
    pub fn for_view(view: View) -> Option<Self> {
        if view.reverse || view.cb_for_slot(view.active_slot).is_some() {
            return None;
        }
        match view.mode {
            Mode::Channel => Some(Self::Channel {
                slot: view.active_slot,
                channel: view.channel?,
            }),
            Mode::Vfo if valid_frequency(view.vfo_10hz) => Some(Self::Vfo {
                slot: view.active_slot,
                frequency_10hz: view.vfo_10hz,
            }),
            Mode::Vfo => None,
        }
    }
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
    ScanPriority,
    DisplayLight,
    KeyLight,
    Shift,
    TxTone,
    TopKey,
    BottomKey,
    BandPreset,
    GpsEnable,
    GpsBaud,
    Dtmf,
    NameRoger,
    MicrophoneGain,
    TxTimeout,
}

impl Setting {
    const fn is_tx_tone(self) -> bool {
        matches!(self, Self::TxTone)
    }

    pub const fn choice_count(self) -> u8 {
        if matches!(self, Self::MicrophoneGain) {
            32
        } else if matches!(self, Self::GpsBaud) {
            10
        } else if matches!(self, Self::BandPreset) {
            4
        } else if self.is_tx_tone() {
            TX_CTCSS.len() as u8
        } else if matches!(self, Self::Shift) {
            SHIFT_10HZ.len() as u8
        } else if matches!(self, Self::DisplayLight | Self::KeyLight) {
            LIGHT_MODES.len() as u8
        } else if matches!(self, Self::TopKey | Self::BottomKey) {
            4
        } else if matches!(self, Self::Power | Self::TxTimeout) {
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
    Shift,
    TxTone,
    TopKey,
    BottomKey,
    Shortwave,
    BandPreset,
    Gps,
    GpsEnable,
    GpsBaud,
    Aprs,
    Dtmf,
    Cb,
    Mdc,
    RadioName,
    NameRoger,
    MicrophoneGain,
    ScanPriority,
    RxToneSearch,
    DtmfMemory,
    TxTimeout,
    AprsRx,
}

/// Full catalog for tooling. The visible menu uses `menu_entries`.
pub const MENU_ITEMS: [MenuItem; 38] = [
    MenuItem::Mode,
    MenuItem::Modulation,
    MenuItem::Channels,
    MenuItem::ChannelDetails,
    MenuItem::Step,
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
    MenuItem::Shift,
    MenuItem::TxTone,
    MenuItem::TopKey,
    MenuItem::BottomKey,
    MenuItem::Shortwave,
    MenuItem::BandPreset,
    MenuItem::Gps,
    MenuItem::GpsEnable,
    MenuItem::GpsBaud,
    MenuItem::Aprs,
    MenuItem::Dtmf,
    MenuItem::Cb,
    MenuItem::Mdc,
    MenuItem::RadioName,
    MenuItem::NameRoger,
    MenuItem::MicrophoneGain,
    MenuItem::ScanPriority,
    MenuItem::RxToneSearch,
    MenuItem::DtmfMemory,
    MenuItem::TxTimeout,
    MenuItem::AprsRx,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuGroup {
    Mode,
    Receive,
    Scanning,
    Keys,
    System,
    Navigation,
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
            Group(MenuGroup::Navigation),
        ],
        Some(MenuGroup::Mode) => &[
            Item(MenuItem::BandPreset),
            Item(MenuItem::Cb),
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
            Item(MenuItem::Shift),
            Item(MenuItem::TxTone),
        ],
        Some(MenuGroup::Scanning) => &[
            Item(MenuItem::Scan),
            Item(MenuItem::Spectrum),
            Item(MenuItem::Shortwave),
            Item(MenuItem::ScanPriority),
            Item(MenuItem::RxToneSearch),
        ],
        Some(MenuGroup::Keys) => &[
            Item(MenuItem::Shortcuts),
            Item(MenuItem::SideKeys),
            Item(MenuItem::TopKey),
            Item(MenuItem::BottomKey),
            Item(MenuItem::DtmfMemory),
            Item(MenuItem::TxTimeout),
        ],
        Some(MenuGroup::Navigation) => &[
            Item(MenuItem::Gps),
            Item(MenuItem::GpsEnable),
            Item(MenuItem::GpsBaud),
            Item(MenuItem::Aprs),
            Item(MenuItem::AprsRx),
            Item(MenuItem::Mdc),
        ],
        Some(MenuGroup::System) => &[
            Item(MenuItem::DisplayLight),
            Item(MenuItem::KeyLight),
            Item(MenuItem::About),
            Item(MenuItem::Dtmf),
            Item(MenuItem::RadioName),
            Item(MenuItem::NameRoger),
            Item(MenuItem::MicrophoneGain),
        ],
    }
}

const MENU_LOCATIONS: [(Option<MenuGroup>, u8); MENU_ITEMS.len()] = {
    let groups = [
        None,
        Some(MenuGroup::Mode),
        Some(MenuGroup::Receive),
        Some(MenuGroup::Scanning),
        Some(MenuGroup::Keys),
        Some(MenuGroup::System),
        Some(MenuGroup::Navigation),
    ];
    let mut locations = [(None, u8::MAX); MENU_ITEMS.len()];
    let mut group_index = 0;
    while group_index < groups.len() {
        let group = groups[group_index];
        let entries = menu_entries(group);
        let mut index = 0;
        while index < entries.len() {
            if let MenuEntry::Item(item) = entries[index] {
                let slot = item as usize;
                if locations[slot].1 == u8::MAX {
                    locations[slot] = (group, index as u8);
                }
            }
            index += 1;
        }
        group_index += 1;
    }
    let mut index = 0;
    while index < locations.len() {
        assert!(
            locations[index].1 != u8::MAX,
            "menu item missing from catalog"
        );
        index += 1;
    }
    locations
};

/// Stable path for simulations/CPS. FM/AM uses its direct root entry.
pub fn menu_location(item: MenuItem) -> (Option<MenuGroup>, u8) {
    MENU_LOCATIONS[item as usize]
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

/// RAM-only limit for a manual PTT session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TxTimeout {
    Seconds10,
    Seconds20,
    #[default]
    Seconds30,
}

impl TxTimeout {
    const CHOICES: [Self; 3] = [Self::Seconds10, Self::Seconds20, Self::Seconds30];

    pub const fn milliseconds(self) -> u32 {
        (self as u32 + 1) * 10_000
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
    Crossband,
    DualWatch,
    Cb,
}

pub const SHORTCUT_ACTIONS: [ShortcutAction; 17] = [
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
    ShortcutAction::DualWatch,
    ShortcutAction::Crossband,
    ShortcutAction::Spectrum,
    ShortcutAction::Cb,
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
    Paused,
    Stopping,
}

impl ScanState {
    fn accepts_cursor(self) -> bool {
        matches!(self, Self::Requested | Self::Running | Self::Paused)
    }
}

/// An outer runtime must explicitly accept, execute and report these requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    SendAprs,
    RetryShortwave,
    CyclePower,
    ToggleMonitor,
    StartScan,
    StopScan,
    SkipScanTarget,
    CycleModulation,
    SetPower(u8),
    SetMonitor(bool),
    SetModulation {
        am: bool,
    },
    SetDisplayLight(LightMode),
    SetKeyLight(LightMode),
    StartSpectrum,
    StartRxToneSearch {
        source: RxToneSource,
    },
    ApplyRxToneOverride {
        source: RxToneSource,
        tone_raw: u16,
    },
    StopSpectrum,
    /// Apply the volatile span/step selection: stop the current sweep and start
    /// a new one around the same confirmed receive frequency.
    RestartSpectrum,
    PanSpectrum {
        forward: bool,
        fine: bool,
    },
    ResetSpectrumPeaks,
    ResumeSpectrum,
    /// Temporary audio on a measured bin; returns to the same window.
    ListenSpectrum {
        bin: u8,
    },
    /// End temporary listening and resume the same sweep window.
    StopListenSpectrum,
    /// Explicit operator action: accept the current bin into VFO/Home.
    AcceptSpectrum {
        bin: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Channel,
    Frequency,
    CbChannel,
}

impl EntryKind {
    pub const fn maximum_digits(self) -> u8 {
        match self {
            Self::Channel => 4,
            Self::Frequency => 8,
            Self::CbChannel => 2,
        }
    }
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
    AuxBinding,
    InactiveChannel,
    InvalidFrequency,
    EntryFull,
    Locked,
    TxDisabled,
    NoChannels,
    ChannelOnly,
    RadioUnavailable,
    SpectrumUnavailable,
    InvalidCbChannel,
    InvalidMdcId,
    NameSaved,
    NameSaveUnconfirmed,
    ScanPrioritySet,
    ScanPriorityCleared,
    ScanPriorityUnavailable,
    ScanPriorityStopScan,
}

pub const SW_MIN_KHZ: u16 = 1_700;
pub const SW_MAX_KHZ: u16 = 26_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortwaveMode {
    Am,
    Lsb,
    Usb,
    Cw,
    Fm,
}

impl ShortwaveMode {
    pub const fn is_ssb(self) -> bool {
        matches!(self, Self::Lsb | Self::Usb | Self::Cw)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Am => "AM",
            Self::Lsb => "LSB",
            Self::Usb => "USB",
            Self::Cw => "CW",
            Self::Fm => "WFM",
        }
    }

    pub const fn steps_hz(self) -> &'static [u32] {
        match self {
            Self::Fm => &[10_000, 50_000, 100_000, 200_000],
            Self::Am => &[1000, 2000, 2500, 5000, 9000, 10000],
            _ => &[50, 100, 500, 1000, 2500, 5000, 10000],
        }
    }

    pub const fn default_step_index(self) -> u8 {
        match self {
            Self::Fm => 2,
            Self::Am => 3,
            Self::Cw => 0,
            _ => 1,
        }
    }

    pub const fn tuner_function(self) -> u8 {
        match self {
            Self::Fm => 0x10,
            Self::Am => 0x11,
            _ => 0x31,
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Am => Self::Lsb,
            Self::Lsb => Self::Usb,
            Self::Usb => Self::Cw,
            Self::Cw => Self::Am,
            Self::Fm => Self::Fm,
        }
    }

    const fn default_bfo(self) -> i16 {
        if matches!(self, Self::Cw) {
            -500
        } else {
            0
        }
    }
}

// Presets are operator tuning limits and starting frequencies, not relay routes.
pub const SW_BANDS: [(&str, u32, u32, u32); 10] = [
    ("\x1f04E", SW_MIN_KHZ as u32, SW_MAX_KHZ as u32, 6070),
    ("\x1f151", 1800, 2000, 1900),
    ("\x1f15A", 3500, 3800, 3650),
    ("\x1f157", 7000, 7200, 7100),
    ("\x1f155", 10100, 10150, 10120),
    ("\x1f153", 14000, 14350, 14200),
    ("\x1f152", 18068, 18168, 18130),
    ("\x1f150", 21000, 21450, 21200),
    ("\x1f14F", 24890, 24990, 24950),
    ("\x1f018", 64000, 108000, 98100),
];

pub const SW_FM_BAND: u8 = SW_BANDS.len() as u8 - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortwaveStatus {
    Starting,
    Patching(u8),
    Recovering(u8),
    Listening { part: u8, rssi: u8, snr: u8 },
    Fault(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossbandProbe {
    Starting,
    InvalidPair,
    InvalidProbeChannels,
    BusError,
    Stopped,
    Measuring {
        a_rx: u32,
        b_rx: u32,
        rssi: u16,
        status: u16,
        tone_match: Option<bool>,
    },
    Ready {
        a_rx: u32,
        b_rx: u32,
        traffic: Option<(Slot, u32, u32)>,
        live: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpsStatus {
    pub valid: bool,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub satellites: u8,
    pub age_seconds: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AprsStatus {
    #[default]
    Unconfigured,
    Unsupported,
    NoFix,
    Ready,
    Sending,
    Sent,
    Failed,
}

impl AprsStatus {
    pub(crate) const fn can_send(self) -> bool {
        matches!(self, Self::Ready | Self::Sent | Self::Failed)
    }
}

const APRS_RX_CAPACITY: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AprsPacket {
    source: [u8; 9],
    information: [u8; 40],
    source_len: u8,
    information_len: u8,
}

impl AprsPacket {
    #[doc(hidden)]
    pub const EMPTY: Self = Self {
        source: [0; 9],
        information: [0; 40],
        source_len: 0,
        information_len: 0,
    };

    fn new(source: &[u8], information: &[u8]) -> Self {
        fn copy_printable(destination: &mut [u8], bytes: &[u8]) -> u8 {
            for (out, &byte) in destination.iter_mut().zip(bytes) {
                *out = if byte.is_ascii_graphic() || byte == b' ' {
                    byte
                } else {
                    b'?'
                };
            }
            bytes.len().min(destination.len()) as u8
        }

        let mut packet = Self::EMPTY;
        packet.source_len = copy_printable(&mut packet.source, source);
        packet.information_len = copy_printable(&mut packet.information, information);
        packet
    }

    pub fn source(&self) -> &str {
        core::str::from_utf8(&self.source[..usize::from(self.source_len)]).unwrap_or("")
    }

    pub fn information(&self) -> &str {
        core::str::from_utf8(&self.information[..usize::from(self.information_len)]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MdcSettings {
    /// Off, begin, end, both. The runtime validates every command.
    pub mode: u8,
    pub id: Option<u16>,
}

const NAME_MULTITAP_MS: u32 = 650;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdcDraft {
    Timing(u8),
    UnitId(Editor<4>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AprsRxStats {
    pub bytes: u32,
    pub packets: u16,
    pub checksum_errors: u16,
    pub poll_gaps: u16,
    pub overruns: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    #[cfg(any(test, feature = "locale-all", feature = "renderer-tests"))]
    pub language: Language,
    /// Committed channel grid context for slot A, independent of edit focus.
    pub cb: Option<CbChannel>,
    pub cb_draft: Option<CbChannel>,
    pub mdc: [MdcSettings; 2],
    pub radio_name: Option<RadioName>,
    pub name_roger_enabled: bool,
    /// BK4819 microphone sensitivity code, 0..31. Zero is minimum, not mute.
    pub microphone_gain: u8,
    pub tx_timeout: TxTimeout,
    pub radio_name_draft: Option<Editor<NAME_BYTES>>,
    pub mdc_option: u8,
    pub mdc_draft: Option<MdcDraft>,
    pub gps_enabled: bool,
    pub gps_baud: u8,
    pub gps_status: GpsStatus,
    pub aprs_status: AprsStatus,
    /// The runtime confirms a supported simplex source and fresh RMC fix.
    pub aprs_auto_available: bool,
    /// Explicit operator opt-in, scoped to the APRS beacon page.
    pub aprs_auto_armed: bool,
    pub aprs_rx_available: bool,
    pub aprs_rx_stats: AprsRxStats,
    pub aprs_rx_packet: AprsPacket,
    pub aprs_rx_count: u8,
    pub aprs_rx_index: u8,
    pub aprs_callsign: [u8; 6],
    pub aprs_ssid: u8,
    pub dtmf_enabled: bool,
    pub shortwave_scope: bool,
    pub shortwave_scope_points: u8,
    pub shortwave_filter: u8,
    pub shortwave_soft_mute: bool,
    pub shortwave_high_pass: bool,
    pub shortwave_khz: u32,
    pub shortwave_fraction_hz: u16,
    pub shortwave_step: u8,
    pub shortwave_band_draft: Option<u8>,
    pub shortwave_mode: ShortwaveMode,
    pub shortwave_band: u8,
    pub shortwave_bfo: i16,
    pub shortwave_scan: bool,
    pub shortwave_entry: Option<u32>,
    pub shortwave_status: ShortwaveStatus,
    pub crossband_probe: CrossbandProbe,
    pub dual_watch: bool,
    pub watch_rx: Option<Slot>,
    pub screen: Screen,
    pub mode: Mode,
    pub channel: Option<u16>,
    pub list_channel: Option<u16>,
    pub vfo_10hz: u32,
    pub shift_index: u8,
    pub tx_tone_index: u8,
    pub top_key: Option<Key>,
    pub top_draft: Option<Key>,
    pub top_action: u8,
    pub bottom_key: Option<Key>,
    pub bottom_action: u8,
    pub step_index: u8,
    pub menu_index: u8,
    pub menu_group: Option<MenuGroup>,
    pub spectrum_cursor: u8,
    /// Volatile spectrum window: independent of `step_index` (the VFO step).
    pub spectrum_span_index: u8,
    pub spectrum_trigger: u8,
    pub spectrum_auto_listen: bool,
    pub spectrum_step_index: u8,
    /// The context menu overlays the graph; it owns MENU/UP/DOWN until EXIT.
    pub spectrum_menu: bool,
    pub spectrum_option: u8,
    /// A temporary Listen session owns RF; short EXIT stops it without leaving
    /// the spectrum page or committing the original MR/VFO context.
    pub spectrum_listening: bool,
    /// Bin submitted for Listen while the RF restore is still pending. Input
    /// cannot move the cursor until a confirmed callback clears this.
    pub spectrum_pending_listen: Option<u8>,
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
    /// Committed status for the channel being edited in the priority page.
    pub scan_priority_enabled: bool,
    /// Confirmed global RAM policy, not proof that a physical lamp is lit.
    pub display_light: LightMode,
    pub key_light: LightMode,
    pub rx_tone_search: Option<RxToneSearch>,
    pub dtmf_memory_page: Option<DtmfMemoryPage>,
}

impl View {
    #[doc(hidden)]
    pub fn can_arm_aprs_auto(&self) -> bool {
        self.aprs_auto_available && self.aprs_status.can_send() && !self.ptt_down && !self.locked
    }

    /// Keep one copy of this large comparison at the MCU rendering boundaries.
    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn same_view(&self, other: &Self) -> bool {
        self == other
    }

    pub const fn language(&self) -> Language {
        #[cfg(any(test, feature = "locale-all", feature = "renderer-tests"))]
        {
            self.language
        }
        #[cfg(not(any(test, feature = "locale-all", feature = "renderer-tests")))]
        {
            Language::CURRENT
        }
    }

    pub const fn cb_for_slot(&self, slot: Slot) -> Option<CbChannel> {
        match slot {
            Slot::A => self.cb,
            Slot::B => None,
        }
    }

    pub const fn mdc_for_slot(&self, slot: Slot) -> MdcSettings {
        self.mdc[slot.index()]
    }

    pub const fn roger_name(&self) -> Option<RadioName> {
        if self.name_roger_enabled {
            self.radio_name
        } else {
            None
        }
    }

    pub const fn radio_name_settings(&self) -> (Option<RadioName>, bool) {
        (self.radio_name, self.name_roger_enabled)
    }

    pub fn shortwave_step_hz(&self, scanning: bool) -> u32 {
        let step = self.shortwave_mode.steps_hz()[usize::from(self.shortwave_step)];
        if scanning {
            step.max(1000)
        } else {
            step
        }
    }
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
    spectrum_last_bin: u8,
    view: View,
    request: Option<Request>,
    ptt_tx_requested: bool,
    ptt_suppressed_until_release: bool,
    /// Step chosen before the Step page opened. MENU commits the draft,
    /// EXIT restores this value; no other screen changes it.
    step_before: Option<u8>,
    /// True when the Step page was opened from Home rather than from MENU.
    step_home: bool,
    contexts: [Context; 2],
    cb_before: Option<Context>,
    cb_last: CbChannel,
    cb_from_home: bool,
    zones: [u8; 32],
    channel_order: [u16; 200],
    channel_order_len: u8,
    // Scan restrictions stay separate from the manually selectable catalog.
    scan_excluded: [u8; 128],
    /// One volatile scan priority per stored zone ID, shared by A and B.
    zone_priorities: [u16; ZONE_COUNT],
    radio_power: Option<u8>,
    name_last_digit_at: Option<u32>,
    radio_monitor: bool,
    radio_am: bool,
    cancel_temporary: bool,
    dtmf_memories: [[u8; DTMF_MEMORY_LENGTH]; DTMF_MEMORY_SLOTS],
    dtmf_memory_lengths: [u8; DTMF_MEMORY_SLOTS],
    active_dtmf_memory: u8,
    aprs_rx_history: [AprsPacket; APRS_RX_CAPACITY as usize],
    aprs_rx_head: u8,
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
            shift_index: 0,
            tx_tone_index: 0,
            zone: None,
            step_index: 4,
            reverse: false,
        };
        Self {
            spectrum_last_bin: SPECTRUM_INTERVALS[0],
            view: View {
                #[cfg(any(test, feature = "locale-all", feature = "renderer-tests"))]
                language: Language::CURRENT,
                cb: None,
                cb_draft: None,
                mdc: [MdcSettings { mode: 0, id: None }; 2],
                radio_name: None,
                name_roger_enabled: false,
                microphone_gain: 16,
                tx_timeout: TxTimeout::default(),
                radio_name_draft: None,
                mdc_option: 0,
                mdc_draft: None,
                gps_enabled: false,
                gps_baud: 1,
                gps_status: GpsStatus::default(),
                aprs_status: AprsStatus::Unconfigured,
                aprs_auto_available: false,
                aprs_auto_armed: false,
                aprs_rx_available: false,
                aprs_rx_stats: AprsRxStats::default(),
                aprs_rx_packet: AprsPacket::EMPTY,
                aprs_rx_count: 0,
                aprs_rx_index: 0,
                aprs_callsign: [b' '; 6],
                aprs_ssid: 0,
                dtmf_enabled: true,
                shortwave_scope: false,
                shortwave_scope_points: 8,
                shortwave_filter: 1,
                shortwave_soft_mute: false,
                shortwave_high_pass: false,
                screen: Screen::Home,
                mode: if channel.is_some() {
                    Mode::Channel
                } else {
                    Mode::Vfo
                },
                channel,
                list_channel: channel,
                shift_index: 0,
                tx_tone_index: 0,
                top_key: Some(Key::Aux18),
                top_draft: None,
                top_action: 0,
                bottom_key: Some(Key::Aux17),
                bottom_action: 0,
                vfo_10hz: if valid_frequency(initial_vfo_10hz) {
                    initial_vfo_10hz
                } else {
                    DEFAULT_FREQUENCY_10HZ
                },
                step_index: 4,
                menu_index: 0,
                menu_group: None,
                spectrum_cursor: 20,
                spectrum_span_index: 0,
                spectrum_trigger: 24,
                spectrum_auto_listen: true,
                shortwave_khz: 6_070,
                shortwave_fraction_hz: 0,
                shortwave_step: ShortwaveMode::Am.default_step_index(),
                shortwave_band_draft: None,
                shortwave_mode: ShortwaveMode::Am,
                shortwave_band: 0,
                shortwave_bfo: 0,
                shortwave_scan: false,
                shortwave_entry: None,
                shortwave_status: ShortwaveStatus::Starting,
                crossband_probe: CrossbandProbe::Starting,
                dual_watch: false,
                watch_rx: None,
                spectrum_step_index: 5,
                spectrum_menu: false,
                spectrum_option: 0,
                spectrum_listening: false,
                spectrum_pending_listen: None,
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
                scan_priority_enabled: false,
                display_light: LightMode::On,
                key_light: LightMode::On,
                rx_tone_search: None,
                dtmf_memory_page: None,
            },
            request: None,
            ptt_tx_requested: false,
            ptt_suppressed_until_release: false,
            step_before: None,
            step_home: false,
            contexts: [context; 2],
            cb_before: None,
            cb_last: CbChannel::default(),
            cb_from_home: false,
            zones: [0; 32],
            channel_order: [0; 200],
            channel_order_len: 0,
            scan_excluded: [0; 128],
            zone_priorities: [NO_ZONE_PRIORITY; ZONE_COUNT],
            radio_power: None,
            name_last_digit_at: None,
            radio_monitor: false,
            radio_am: false,
            cancel_temporary: false,
            dtmf_memories: [[0; DTMF_MEMORY_LENGTH]; DTMF_MEMORY_SLOTS],
            dtmf_memory_lengths: [0; DTMF_MEMORY_SLOTS],
            active_dtmf_memory: 0,
            aprs_rx_history: [AprsPacket::EMPTY; APRS_RX_CAPACITY as usize],
            aprs_rx_head: 0,
        }
    }

    /// Change the locale for synthetic host previews.
    #[cfg(feature = "locale-all")]
    pub fn set_preview_language(&mut self, language: Language) {
        self.view.language = language;
    }

    pub const fn view(&self) -> View {
        self.view
    }

    pub const fn view_ref(&self) -> &View {
        &self.view
    }

    /// Currently active, committed DTMF sequence. Empty until a slot is saved.
    pub fn dtmf_memory(&self) -> &[u8] {
        let slot = usize::from(self.active_dtmf_memory);
        &self.dtmf_memories[slot][..usize::from(self.dtmf_memory_lengths[slot])]
    }

    fn dtmf_memory_page(&self, slot: u8) -> DtmfMemoryPage {
        let slot = slot.min((DTMF_MEMORY_SLOTS - 1) as u8);
        let index = usize::from(slot);
        DtmfMemoryPage {
            slot,
            bytes: self.dtmf_memories[index],
            len: self.dtmf_memory_lengths[index],
            active: slot == self.active_dtmf_memory,
            editing: false,
        }
    }

    fn open_dtmf_memory(&mut self) {
        self.view.dtmf_memory_page = Some(self.dtmf_memory_page(self.active_dtmf_memory));
        self.view.screen = Screen::DtmfMemory;
    }

    fn dtmf_memory_input(&mut self, event: InputEvent) {
        let Some(mut page) = self.view.dtmf_memory_page else {
            self.view.screen = Screen::Menu;
            return;
        };
        let index = usize::from(page.slot);
        if page.editing {
            match event {
                InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up) => {
                    page.len = page.len.saturating_sub(1);
                    self.view.notice = Notice::None;
                }
                InputEvent::Short(Key::Menu) => {
                    self.dtmf_memories[index] = page.bytes;
                    self.dtmf_memory_lengths[index] = page.len;
                    self.active_dtmf_memory = page.slot;
                    page.active = true;
                    page.editing = false;
                    self.view.notice = Notice::None;
                }
                InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit) => {
                    page = self.dtmf_memory_page(page.slot);
                    self.view.notice = Notice::None;
                }
                InputEvent::Short(Key::Star) => self.append_dtmf_memory_symbol(&mut page, b'*'),
                InputEvent::Short(Key::Hash) => self.append_dtmf_memory_symbol(&mut page, b'#'),
                InputEvent::Short(key) => {
                    if let Some(digit) = digit(key) {
                        self.append_dtmf_memory_symbol(&mut page, b'0' + digit);
                    }
                }
                _ => {}
            }
        } else {
            match event {
                InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up) => {
                    page = self.dtmf_memory_page(adjacent_index(
                        page.slot,
                        DTMF_MEMORY_SLOTS as u8,
                        false,
                    ));
                }
                InputEvent::Short(Key::Down) | InputEvent::Repeat(Key::Down) => {
                    page = self.dtmf_memory_page(adjacent_index(
                        page.slot,
                        DTMF_MEMORY_SLOTS as u8,
                        true,
                    ));
                }
                InputEvent::Short(Key::Menu) => {
                    page.editing = true;
                    self.view.notice = Notice::None;
                }
                InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit) => {
                    self.view.dtmf_memory_page = None;
                    self.view.screen = Screen::Menu;
                    return;
                }
                _ => {}
            }
        }
        self.view.dtmf_memory_page = Some(page);
    }

    fn append_dtmf_memory_symbol(&mut self, page: &mut DtmfMemoryPage, symbol: u8) {
        let len = usize::from(page.len);
        if len == DTMF_MEMORY_LENGTH {
            self.view.notice = Notice::EntryFull;
            return;
        }
        page.bytes[len] = symbol;
        page.len += 1;
        self.view.notice = Notice::None;
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
            self.leave_selected_cb();
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

    /// Accept only a decoded, non-empty tone for the same source that started
    /// the active search. `None` returns the display to its waiting state.
    pub fn set_rx_tone_search_result(&mut self, result_raw: Option<u16>) -> bool {
        let Some(search) = self.view.rx_tone_search else {
            return false;
        };
        if self.view.screen != Screen::RxToneSearch
            || self.rx_tone_source() != Some(search.source)
            || search.result_raw == result_raw
            || result_raw.is_some_and(|raw| {
                !matches!(
                    uv98_codeplug::channels::Tone::decode(raw),
                    Some(
                        uv98_codeplug::channels::Tone::Ctcss { .. }
                            | uv98_codeplug::channels::Tone::Dcs { .. }
                    )
                )
            })
        {
            return false;
        }
        self.view.rx_tone_search = Some(RxToneSearch {
            result_raw,
            ..search
        });
        true
    }

    /// Runtime guard rejected the request. Return to the menu without applying
    /// any tone or changing the receive source.
    pub fn reject_rx_tone_search(&mut self) {
        if self.view.screen == Screen::RxToneSearch {
            self.view.rx_tone_search = None;
            self.view.screen = Screen::Menu;
            self.view.notice = Notice::RadioUnavailable;
        }
    }

    fn rx_tone_source(&self) -> Option<RxToneSource> {
        RxToneSource::for_view(self.view)
    }

    fn start_rx_tone_search(&mut self, bitmap: &[u8; 128]) {
        let source = self.rx_tone_source().filter(|source| {
            !self.view.locked
                && !self.view.ptt_down
                && self.view.scan == ScanState::Stopped
                && !self.radio_am
                && match source {
                    RxToneSource::Channel { channel, .. } => {
                        channel_active(bitmap, *channel)
                            && (self.view.zone.is_none()
                                || self.zone_exists(self.view.zone.unwrap())
                                    && self.channel_order().contains(channel))
                    }
                    RxToneSource::Vfo { .. } => true,
                }
        });
        let Some(source) = source else {
            self.view.notice = Notice::RadioUnavailable;
            return;
        };
        self.view.rx_tone_search = Some(RxToneSearch {
            source,
            result_raw: None,
        });
        self.view.screen = Screen::RxToneSearch;
        self.queue_request(Request::StartRxToneSearch { source });
    }

    fn rx_tone_search_ptt(&mut self, index: usize, down: bool) {
        self.view.side_down[index] = down;
        self.view.ptt_down = self.view.side_down.iter().any(|&pressed| pressed);
        self.view.last_key = Some(if index == 0 { Key::Ptt } else { Key::Aux16 });
        self.view.ptt_slot = None;
        self.ptt_tx_requested = false;
        if down {
            self.ptt_suppressed_until_release = true;
            self.view.rx_tone_search = None;
            self.view.screen = Screen::Menu;
        } else if !self.view.ptt_down {
            self.ptt_suppressed_until_release = false;
        }
    }

    pub fn apply_radio_name_settings(&mut self, name: Option<RadioName>, enabled: bool) {
        self.view.radio_name = name;
        self.view.name_roger_enabled = enabled;
    }
    pub fn apply_microphone_gain(&mut self, gain: u8) {
        if gain <= 31 {
            self.view.microphone_gain = gain;
        }
    }
    /// Firmware calls this only after the persistent store has returned.
    pub fn report_name_save(&mut self, confirmed: bool) {
        self.view.notice = if confirmed {
            Notice::NameSaved
        } else {
            Notice::NameSaveUnconfirmed
        };
    }

    fn open_setting(&mut self, setting: Setting) {
        self.view.setting_index = match setting {
            Setting::BandPreset => 0,
            Setting::GpsEnable => u8::from(self.view.gps_enabled),
            Setting::GpsBaud => self.view.gps_baud,
            Setting::Dtmf => u8::from(self.view.dtmf_enabled),
            Setting::NameRoger => u8::from(self.view.name_roger_enabled),
            Setting::MicrophoneGain => self.view.microphone_gain,
            Setting::TxTimeout => self.view.tx_timeout as u8,
            Setting::TopKey => {
                self.view.top_draft = self.view.top_key;
                self.view.top_action
            }
            Setting::BottomKey => {
                self.view.top_draft = self.view.bottom_key;
                self.view.bottom_action
            }
            Setting::Shift => self.view.shift_index,
            Setting::TxTone => self.view.tx_tone_index,
            Setting::Mode => u8::from(self.view.mode == Mode::Vfo),
            Setting::Power => self.radio_power.unwrap_or(0),
            Setting::Monitor => u8::from(self.radio_monitor),
            Setting::Modulation => {
                u8::from(self.selected_cb().map_or(self.radio_am, CbChannel::is_am))
            }
            Setting::Reverse => u8::from(self.view.reverse),
            Setting::ActiveSlot => u8::from(self.view.active_slot == Slot::B),
            Setting::Scan => u8::from(self.view.scan.accepts_cursor()),
            Setting::ScanPriority => u8::from(self.view.scan_priority_enabled),
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
            Setting::GpsEnable => self.view.gps_enabled = selected != 0,
            Setting::GpsBaud => self.view.gps_baud = selected,
            Setting::Dtmf => self.view.dtmf_enabled = selected != 0,
            Setting::NameRoger => self.view.name_roger_enabled = selected != 0,
            Setting::MicrophoneGain => self.apply_microphone_gain(selected),
            Setting::TxTimeout => self.view.tx_timeout = TxTimeout::CHOICES[selected as usize],
            Setting::BandPreset => {
                // Presets select frequency, modulation and step together. The
                // RF runtime still derives its relay route from frequency.
                let (frequency, am, step) = match selected {
                    0 => (14_550_000, false, 4),
                    1 => (43_350_000, false, 4),
                    2 => (2_713_500, true, 3),
                    _ => (85_000_000, false, 5),
                };
                self.leave_selected_cb();
                self.view.mode = Mode::Vfo;
                self.view.vfo_10hz = frequency;
                self.view.step_index = step;
                self.view.shift_index = 0;
                self.view.tx_tone_index = 0;
                self.view.reverse = false;
                self.queue_request(Request::SetModulation { am });
                self.view.screen = Screen::Home;
                return;
            }
            Setting::TopKey | Setting::BottomKey => {
                let key = self.view.top_draft;
                let (binding, action, other) = if setting == Setting::TopKey {
                    (
                        &mut self.view.top_key,
                        &mut self.view.top_action,
                        self.view.bottom_key,
                    )
                } else {
                    (
                        &mut self.view.bottom_key,
                        &mut self.view.bottom_action,
                        self.view.top_key,
                    )
                };
                if selected != 0 && (key.is_none() || key == other) {
                    self.view.notice = Notice::AuxBinding;
                    self.view.setting = Some(setting);
                    return;
                }
                if selected != 0 {
                    *binding = key;
                }
                *action = selected;
            }
            Setting::Shift => self.view.shift_index = selected,
            Setting::TxTone => self.view.tx_tone_index = selected,
            Setting::Mode => {
                let mode = if selected == 0 {
                    Mode::Channel
                } else {
                    Mode::Vfo
                };
                self.leave_selected_cb();
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
                if let Some(mut cb) = self.selected_cb() {
                    cb.am = selected != 0;
                    self.apply_cb_channel(cb);
                } else {
                    self.queue_request(Request::SetModulation { am: selected != 0 });
                }
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
                if selected == 0 && self.view.scan.accepts_cursor() {
                    self.request_scan_stop();
                } else if selected != 0 && self.view.scan == ScanState::Stopped {
                    self.toggle_scan(bitmap);
                }
            }
            Setting::ScanPriority => {
                self.set_scan_zone_priority(bitmap, selected != 0);
                return;
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

    pub fn positions(&self) -> [u32; 2] {
        [
            self.context(Slot::A).position()
                | if self.view.active_slot == Slot::B {
                    POSITION_ACTIVE_B
                } else {
                    0
                },
            self.context(Slot::B).position(),
        ]
    }

    /// Restore only the boot position. The App checks current zone membership
    /// and rebuilds both channel records before either slot is used for RF.
    pub fn restore_position(&mut self, slot: Slot, word: u32) -> Change {
        let before = self.view;
        self.switch_slot(slot);
        self.view.notice = Notice::None;
        let mut context = self.current_context();
        if valid_frequency(word) {
            context.mode = Mode::Vfo;
            context.vfo_10hz = word;
            context.zone = None;
        } else if word & !0x000f_ffff == Context::MEMORY_POSITION {
            let zone = (word >> 11) & 0x1ff;
            let channel = word & 0x7ff;
            context.mode = Mode::Channel;
            context.zone = (1..=256)
                .contains(&zone)
                .then(|| (zone - 1) as u8)
                .filter(|&zone| self.zone_exists(zone));
            context.channel = (1..=CHANNEL_COUNT as u32)
                .contains(&channel)
                .then(|| (channel - 1) as u16);
        }
        self.load_context(context);
        self.change_since(before)
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

    pub fn set_scan_exclusions(&mut self, excluded: [u8; 128]) {
        self.scan_excluded = excluded;
    }

    pub fn scan_exclusions(&self) -> &[u8; 128] {
        &self.scan_excluded
    }

    /// Current named-zone scan priority, if it is still eligible for scan.
    /// A and B share the designation for a zone with the same stored ID.
    pub fn scan_zone_priority(&self, bitmap: &[u8; 128]) -> Option<u16> {
        if self.view.mode != Mode::Channel || self.selected_cb().is_some() {
            return None;
        }
        let zone = self.view.zone?;
        if !self.zone_exists(zone) {
            return None;
        }
        let channel = self.zone_priorities[zone as usize];
        (channel != NO_ZONE_PRIORITY
            && channel_active(bitmap, channel)
            && !channel_active(&self.scan_excluded, channel)
            && self.channel_order().contains(&channel))
        .then_some(channel)
    }

    fn set_scan_zone_priority(&mut self, bitmap: &[u8; 128], enabled: bool) {
        self.view.screen = Screen::Home;
        let Some(zone) = self.view.zone else {
            self.view.notice = Notice::ScanPriorityUnavailable;
            return;
        };
        let Some(channel) = self.view.channel.filter(|&channel| {
            self.view.scan == ScanState::Stopped
                && !self.view.locked
                && self.view.mode == Mode::Channel
                && self.selected_cb().is_none()
                && self.zone_exists(zone)
                && channel_active(bitmap, channel)
                && !channel_active(&self.scan_excluded, channel)
                && self.channel_order().contains(&channel)
        }) else {
            self.view.notice = Notice::ScanPriorityUnavailable;
            return;
        };

        let priority = &mut self.zone_priorities[zone as usize];
        if enabled {
            *priority = channel;
            self.view.notice = Notice::ScanPrioritySet;
        } else if *priority == channel {
            *priority = NO_ZONE_PRIORITY;
            self.view.notice = Notice::ScanPriorityCleared;
        }
    }

    fn current_context(&self) -> Context {
        Context {
            mode: self.view.mode,
            channel: self.view.channel,
            vfo_10hz: self.view.vfo_10hz,
            shift_index: self.view.shift_index,
            tx_tone_index: self.view.tx_tone_index,
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
        // Slot selection ends a Step transaction even when selecting the
        // current slot. Close the editor after rolling back its draft.
        if self.view.screen == Screen::Step {
            self.finish_step_cancel();
            self.view.screen = Screen::Home;
        }
        if !self.view.locked {
            self.switch_slot(slot);
        }
        self.change_since(before)
    }

    fn switch_slot(&mut self, slot: Slot) {
        if slot == self.view.active_slot {
            return;
        }
        if self.view.screen == Screen::Step {
            self.finish_step_cancel();
            self.view.screen = Screen::Home;
        }
        if self.view.ptt_down && self.view.ptt_slot != Some(slot) {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
        }
        self.contexts[self.view.active_slot.index()] = self.current_context();
        self.view.active_slot = slot;
        self.load_context(self.contexts[slot.index()]);
        self.channel_order_len = 0;
        self.view.entry = None;
        self.view.cb_draft = None;
        self.view.mdc_draft = None;
        self.view.radio_name_draft = None;
        self.name_last_digit_at = None;
        self.view.screen = Screen::Home;
        self.view.setting = None;
        self.request_scan_stop();
    }

    fn load_context(&mut self, context: Context) {
        self.view.mode = context.mode;
        self.view.channel = context.channel;
        self.view.list_channel = context.channel;
        self.view.vfo_10hz = context.vfo_10hz;
        self.view.shift_index = context.shift_index;
        self.view.tx_tone_index = context.tx_tone_index;
        self.view.zone = context.zone;
        self.view.zone_cursor = context.zone;
        self.view.step_index = context.step_index;
        self.view.reverse = context.reverse;
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
            && self.view.scan.accepts_cursor()
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
            && self.selected_cb().is_none()
            && self.view.scan.accepts_cursor()
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

    pub fn handle_at(&mut self, event: InputEvent, bitmap: &[u8; 128], now: u32) -> Change {
        if let InputEvent::Short(key) | InputEvent::Hold(key) = event {
            if let Some(editor) = self
                .view
                .radio_name_draft
                .as_mut()
                .filter(|_| digit(key).is_some())
            {
                if self
                    .name_last_digit_at
                    .is_some_and(|last| now.wrapping_sub(last) >= NAME_MULTITAP_MS)
                {
                    editor.commit_character();
                }
                self.name_last_digit_at = Some(now);
            }
        }
        self.handle(event, bitmap)
    }

    pub fn handle(&mut self, event: InputEvent, bitmap: &[u8; 128]) -> Change {
        let before = self.view;
        self.ptt_tx_requested = false;
        self.apply(event, bitmap);
        if self.view.screen != Screen::Cb {
            self.view.cb_draft = None;
        }
        if self.view.screen != Screen::Mdc {
            self.view.mdc_draft = None;
        }
        if self.view.screen != Screen::RadioName {
            self.view.radio_name_draft = None;
            self.name_last_digit_at = None;
        }
        if self.view.screen != Screen::Setting {
            self.view.setting = None;
        }
        // Reversal belongs to one current channel, never a later channel/VFO.
        // Centralize this invariant across arrows, direct entry and list commit.
        if self.view.active_slot == before.active_slot
            && self.view.cb == before.cb
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
                || self.view.cb != before.cb
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
                || self.view.cb != before.cb
                || self.view.zone != before.zone
                || self.view.active_slot != before.active_slot
                || self.view.reverse != before.reverse
                || self.view.screen != before.screen)
        {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
        }
        // Leave the Step editor only after validating the new TX intent.
        // Its own rollback must not look like navigation during a PTT hold.
        if self.ptt_tx_requested && self.view.ptt_slot.is_some() && self.view.screen == Screen::Step
        {
            self.finish_step_cancel();
            self.view.screen = Screen::Home;
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

    /// Reflect an acknowledged RF pause without reviving a cancelled scan.
    pub fn set_scan_paused(&mut self, paused: bool) -> bool {
        self.view.scan = match (self.view.scan, paused) {
            (ScanState::Running, true) => ScanState::Paused,
            (ScanState::Paused, false) => ScanState::Running,
            _ => return false,
        };
        true
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

    /// Refuse a spectrum start without pretending the operator left the page.
    /// The graph keeps showing only what was measured; no fake sweep appears.
    pub fn reject_spectrum_start(&mut self) {
        if self.view.screen == Screen::Spectrum {
            self.view.spectrum_menu = false;
            self.view.notice = Notice::SpectrumUnavailable;
            self.view.ptt_slot = None;
        } else {
            self.reject_radio_request(Request::StartSpectrum);
        }
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
        if self.view.rx_tone_search.is_some()
            && (self.view.screen != Screen::RxToneSearch
                || self.rx_tone_source() != self.view.rx_tone_search.map(|search| search.source))
        {
            self.view.rx_tone_search = None;
        }
        if self.view.screen != Screen::DtmfMemory {
            self.view.dtmf_memory_page = None;
        }
        if self.view.aprs_auto_armed && (self.view.screen != Screen::Aprs || self.view.ptt_down) {
            self.disarm_aprs_auto();
        }
        Change {
            redraw: !self.view.same_view(&before) || self.view.screen == Screen::Keys,
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

    pub fn set_gps_status(&mut self, status: GpsStatus) {
        self.view.gps_status = status;
    }

    pub fn set_aprs_status(&mut self, status: AprsStatus) {
        self.view.aprs_status = status;
        if matches!(status, AprsStatus::Unconfigured | AprsStatus::Unsupported) {
            self.disarm_aprs_auto();
        }
    }

    pub fn disarm_aprs_auto(&mut self) {
        self.view.aprs_auto_armed = false;
    }

    pub fn set_aprs_auto_available(&mut self, available: bool) {
        self.view.aprs_auto_available = available;
    }

    pub fn set_aprs_rx_stats(&mut self, stats: AprsRxStats) {
        self.view.aprs_rx_stats = stats;
    }

    pub fn set_aprs_rx_available(&mut self, available: bool) {
        self.view.aprs_rx_available = available;
    }

    pub fn push_aprs_packet(&mut self, source: &[u8], information: &[u8]) {
        self.aprs_rx_head = (self.aprs_rx_head + APRS_RX_CAPACITY - 1) % APRS_RX_CAPACITY;
        self.aprs_rx_history[usize::from(self.aprs_rx_head)] = AprsPacket::new(source, information);
        self.view.aprs_rx_count = (self.view.aprs_rx_count + 1).min(APRS_RX_CAPACITY);
        let index = self.view.aprs_rx_index;
        // Follow live at the newest packet. While browsing, keep the selected
        // packet until it ages out of the four-entry history.
        self.select_aprs_packet(if index == 0 { 0 } else { index + 1 });
    }

    fn select_aprs_packet(&mut self, index: u8) {
        let index = index.min(self.view.aprs_rx_count.saturating_sub(1));
        self.view.aprs_rx_index = index;
        let slot = (self.aprs_rx_head + index) % APRS_RX_CAPACITY;
        self.view.aprs_rx_packet = self.aprs_rx_history[usize::from(slot)];
    }

    pub fn set_aprs_identity(&mut self, callsign: [u8; 6], ssid: u8) {
        if ssid <= 15
            && callsign
                .iter()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b' ')
        {
            self.view.aprs_callsign = callsign;
            self.view.aprs_ssid = ssid;
        }
    }

    pub fn set_shortwave_status(&mut self, status: ShortwaveStatus) {
        self.view.shortwave_status = status;
        if matches!(status, ShortwaveStatus::Fault(_)) {
            self.view.shortwave_scan = false;
        }
    }

    pub fn set_watch_rx(&mut self, slot: Option<Slot>) {
        self.view.watch_rx = slot;
    }

    pub fn set_crossband_probe(&mut self, status: CrossbandProbe) {
        self.view.crossband_probe = status;
    }

    fn step_shortwave(&mut self, up: bool, scanning: bool) {
        let (_, low, high, _) = SW_BANDS[usize::from(self.view.shortwave_band)];
        let (low, high) = (low * 1000, high * 1000);
        let hz = self.view.shortwave_khz * 1000 + u32::from(self.view.shortwave_fraction_hz);
        let step = self.view.shortwave_step_hz(scanning);
        let next = if up {
            if scanning && hz >= high {
                low
            } else {
                (hz + step).min(high)
            }
        } else {
            hz.saturating_sub(step).max(low)
        };
        self.view.shortwave_khz = next / 1000;
        self.view.shortwave_fraction_hz = (next % 1000) as u16;
    }

    /// True when the scan wants the next frequency. A peak or operator stop
    /// leaves the tuner on the currently confirmed frequency.
    pub fn continue_shortwave_scan(&mut self, rssi: u8, snr: u8) -> bool {
        if !self.view.shortwave_scan {
            return false;
        }
        let hit = if self.view.shortwave_mode.is_ssb() {
            rssi >= 28
        } else {
            rssi >= 22 && snr >= 8
        };
        if hit {
            self.view.shortwave_scan = false;
            false
        } else {
            self.step_shortwave(true, true);
            true
        }
    }

    fn shortwave_input(&mut self, event: InputEvent) {
        if let Some(draft) = self.view.shortwave_band_draft {
            match event {
                InputEvent::Short(Key::Menu) => {
                    self.view.shortwave_scope = false;
                    let mode = if draft == SW_FM_BAND {
                        ShortwaveMode::Fm
                    } else if self.view.shortwave_mode == ShortwaveMode::Fm {
                        ShortwaveMode::Am
                    } else {
                        self.view.shortwave_mode
                    };
                    if mode != self.view.shortwave_mode {
                        self.view.shortwave_mode = mode;
                        self.view.shortwave_step = mode.default_step_index();
                        self.view.shortwave_bfo = 0;
                        self.view.shortwave_status = ShortwaveStatus::Starting;
                    }
                    self.view.shortwave_band = draft;
                    self.view.shortwave_khz = SW_BANDS[usize::from(draft)].3;
                    self.view.shortwave_fraction_hz = 0;
                    self.view.shortwave_band_draft = None;
                }
                InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up) => {
                    self.view.shortwave_band_draft =
                        Some(adjacent_index(draft, SW_BANDS.len() as u8, false));
                }
                InputEvent::Short(Key::Down) | InputEvent::Repeat(Key::Down) => {
                    self.view.shortwave_band_draft =
                        Some(adjacent_index(draft, SW_BANDS.len() as u8, true));
                }
                InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit) => {
                    self.view.shortwave_band_draft = None;
                }
                _ => {}
            }
            return;
        }
        match event {
            InputEvent::Hold(Key::Digit0) if self.view.shortwave_entry.is_none() => {
                self.view.shortwave_scope = !self.view.shortwave_scope;
                self.view.shortwave_scan = false;
            }
            InputEvent::Short(Key::Hash)
                if self.view.shortwave_scope && self.view.shortwave_entry.is_none() =>
            {
                self.view.shortwave_scope_points = match self.view.shortwave_scope_points {
                    8 => 16,
                    16 => 32,
                    _ => 8,
                };
            }
            InputEvent::Hold(Key::Digit1)
                if self.view.shortwave_entry.is_none()
                    && self.view.shortwave_mode == ShortwaveMode::Am =>
            {
                self.view.shortwave_filter = (self.view.shortwave_filter + 1) % 3;
            }
            InputEvent::Hold(Key::Digit2)
                if self.view.shortwave_entry.is_none()
                    && self.view.shortwave_mode == ShortwaveMode::Am =>
            {
                self.view.shortwave_soft_mute = !self.view.shortwave_soft_mute;
            }
            InputEvent::Hold(Key::Digit3)
                if self.view.shortwave_entry.is_none()
                    && self.view.shortwave_mode == ShortwaveMode::Am =>
            {
                self.view.shortwave_high_pass = !self.view.shortwave_high_pass;
            }
            InputEvent::Short(Key::Exit) if self.view.shortwave_entry.is_some() => {
                self.view.shortwave_entry = None;
            }
            InputEvent::Short(Key::Exit) if self.view.shortwave_scope => {
                self.view.shortwave_scope = false;
            }
            InputEvent::Short(Key::Exit) if self.view.shortwave_scan => {
                self.view.shortwave_scan = false;
            }
            InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit) => {
                self.view.shortwave_entry = None;
                self.view.shortwave_scan = false;
                self.view.shortwave_scope = false;
                self.view.screen = Screen::Home;
                self.cancel_temporary = true;
            }
            InputEvent::Short(Key::Menu)
                if self.view.shortwave_entry.is_none()
                    && matches!(self.view.shortwave_status, ShortwaveStatus::Fault(_)) =>
            {
                self.queue_request(Request::RetryShortwave);
            }
            InputEvent::Short(Key::Menu) => {
                if let Some(khz) = self.view.shortwave_entry.take() {
                    let fm = self.view.shortwave_mode == ShortwaveMode::Fm;
                    let (_, fm_low, fm_high, _) = SW_BANDS[usize::from(SW_FM_BAND)];
                    if fm && (fm_low..=fm_high).contains(&khz) && khz % 10 == 0 {
                        self.view.shortwave_khz = khz;
                    } else if !fm && (u32::from(SW_MIN_KHZ)..=u32::from(SW_MAX_KHZ)).contains(&khz)
                    {
                        self.view.shortwave_khz = khz;
                        self.view.shortwave_fraction_hz = 0;
                        self.view.shortwave_band = SW_BANDS
                            .iter()
                            .enumerate()
                            .skip(1)
                            .take(usize::from(SW_FM_BAND - 1))
                            .find(|(_, (_, low, high, _))| (*low..=*high).contains(&khz))
                            .map_or(0, |(index, _)| index as u8);
                    } else {
                        self.view.shortwave_entry = Some(khz);
                    }
                }
            }
            InputEvent::Short(Key::Star)
                if self.view.shortwave_entry.is_none()
                    && self.view.shortwave_mode != ShortwaveMode::Fm =>
            {
                self.view.shortwave_scan = false;
                let mode = self.view.shortwave_mode.next();
                if self.view.shortwave_mode.is_ssb() != mode.is_ssb() || mode == ShortwaveMode::Cw {
                    self.view.shortwave_step = mode.default_step_index();
                }
                if !mode.is_ssb() {
                    self.view.shortwave_fraction_hz = 0;
                }
                self.view.shortwave_mode = mode;
                self.view.shortwave_bfo = mode.default_bfo();
                self.view.shortwave_status = ShortwaveStatus::Starting;
            }
            InputEvent::Hold(Key::Hash) if self.view.shortwave_entry.is_none() => {
                self.view.shortwave_scan = false;
                self.view.shortwave_scope = false;
                self.view.shortwave_band_draft = Some(self.view.shortwave_band);
            }
            InputEvent::Short(Key::Hash)
                if self.view.shortwave_entry.is_none()
                    && !matches!(self.view.shortwave_status, ShortwaveStatus::Fault(_)) =>
            {
                self.view.shortwave_scope = false;
                self.view.shortwave_scan = !self.view.shortwave_scan;
                if self.view.shortwave_scan {
                    self.step_shortwave(true, true);
                }
            }
            InputEvent::Short(Key::Aux17) if self.view.shortwave_entry.is_none() => {
                self.view.shortwave_scan = false;
                self.view.shortwave_step = adjacent_index(
                    self.view.shortwave_step,
                    self.view.shortwave_mode.steps_hz().len() as u8,
                    true,
                );
            }
            InputEvent::Hold(Key::Digit4 | Key::Digit6) if self.view.shortwave_entry.is_none() => {
                self.view.shortwave_scan = false;
                let step = self.view.shortwave_step;
                let last = self.view.shortwave_mode.steps_hz().len() as u8 - 1;
                self.view.shortwave_step = if event == InputEvent::Hold(Key::Digit6) {
                    step.saturating_add(1).min(last)
                } else {
                    step.saturating_sub(1)
                };
            }
            InputEvent::Hold(Key::Digit1 | Key::Digit2 | Key::Digit3)
                if self.view.shortwave_entry.is_none() && self.view.shortwave_mode.is_ssb() =>
            {
                self.view.shortwave_scan = false;
                self.view.shortwave_bfo = match event {
                    InputEvent::Hold(Key::Digit1) => (self.view.shortwave_bfo - 50).max(-2000),
                    InputEvent::Hold(Key::Digit3) => (self.view.shortwave_bfo + 50).min(2000),
                    _ => self.view.shortwave_mode.default_bfo(),
                };
            }
            InputEvent::Short(Key::Up | Key::Down) | InputEvent::Repeat(Key::Up | Key::Down) => {
                self.view.shortwave_entry = None;
                self.view.shortwave_scan = false;
                self.step_shortwave(
                    matches!(
                        event,
                        InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up)
                    ),
                    false,
                );
            }
            InputEvent::Short(key) => {
                if let Some(digit) = digit(key) {
                    self.view.shortwave_scan = false;
                    let khz = self.view.shortwave_entry.unwrap_or(0) * 10 + u32::from(digit);
                    if khz <= 999_999 {
                        self.view.shortwave_entry = Some(khz);
                    }
                }
            }
            _ => {}
        }
    }

    fn apply(&mut self, event: InputEvent, bitmap: &[u8; 128]) {
        match event {
            InputEvent::Ptt(down) => {
                if self.view.screen == Screen::RxToneSearch {
                    self.rx_tone_search_ptt(0, down);
                    return;
                }
                self.side_event(0, down, bitmap);
                return;
            }
            InputEvent::PttB(down) => {
                if self.view.screen == Screen::RxToneSearch {
                    self.rx_tone_search_ptt(1, down);
                    return;
                }
                self.side_event(1, down, bitmap);
                return;
            }
            _ => {}
        }
        if self.view.screen == Screen::RxToneSearch {
            match event {
                InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit) => {
                    self.view.rx_tone_search = None;
                    self.view.screen = Screen::Menu;
                }
                InputEvent::Short(Key::Menu) => self.apply_rx_tone_search(),
                _ => {}
            }
            return;
        }
        if self.view.screen == Screen::DtmfMemory {
            self.dtmf_memory_input(event);
            return;
        }
        if self.view.screen == Screen::AprsRx {
            match event {
                InputEvent::Hold(Key::Exit) => {
                    self.view.screen = Screen::Home;
                    self.cancel_temporary = true;
                }
                InputEvent::Short(Key::Exit) => self.view.screen = Screen::Menu,
                InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up) => {
                    self.select_aprs_packet(self.view.aprs_rx_index.saturating_sub(1));
                }
                InputEvent::Short(Key::Down) | InputEvent::Repeat(Key::Down) => {
                    self.select_aprs_packet(self.view.aprs_rx_index + 1);
                }
                _ => {}
            }
            return;
        }
        if self.view.screen == Screen::Gps || self.view.screen == Screen::Aprs {
            match event {
                InputEvent::Hold(Key::Exit) => {
                    self.view.screen = Screen::Home;
                    self.cancel_temporary = true;
                }
                InputEvent::Short(Key::Exit) => self.view.screen = Screen::Menu,
                InputEvent::Short(Key::Menu) if self.view.screen == Screen::Gps => {
                    self.open_setting(Setting::GpsEnable)
                }
                InputEvent::Short(Key::Menu) if self.view.aprs_status.can_send() => {
                    self.queue_request(Request::SendAprs)
                }
                InputEvent::Short(Key::Star) if self.view.screen == Screen::Aprs => {
                    self.view.aprs_auto_armed =
                        !self.view.aprs_auto_armed && self.view.can_arm_aprs_auto();
                }
                _ => {}
            }
            return;
        }
        if self.view.screen == Screen::Shortwave {
            self.shortwave_input(event);
            return;
        }
        let key = match event {
            InputEvent::Short(key) | InputEvent::Hold(key) | InputEvent::Repeat(key) => key,
            InputEvent::Ptt(_) | InputEvent::PttB(_) => return,
        };
        self.view.last_key = Some(key);
        if self.view.screen == Screen::Crossband {
            let toggle = event == InputEvent::Hold(key)
                && digit(key).and_then(|digit| self.binding(digit))
                    == Some(ShortcutAction::Crossband);
            if toggle
                || matches!(
                    event,
                    InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit)
                )
            {
                self.view.screen = Screen::Home;
                self.cancel_temporary = true;
            }
            return;
        }
        if matches!(key, Key::Aux16 | Key::Aux17 | Key::Aux18) && event == InputEvent::Short(key) {
            if self.view.screen == Screen::Setting
                && matches!(
                    self.view.setting,
                    Some(Setting::TopKey | Setting::BottomKey)
                )
            {
                self.view.top_draft = Some(key);
            } else if !self.view.locked && self.view.screen == Screen::Home {
                let selected = if self.view.top_key == Some(key) {
                    self.view.top_action
                } else if self.view.bottom_key == Some(key) {
                    self.view.bottom_action
                } else {
                    0
                };
                let action = match selected {
                    1 => ShortcutAction::Monitor,
                    2 => ShortcutAction::Scan,
                    3 => ShortcutAction::Spectrum,
                    _ => ShortcutAction::None,
                };
                self.shortcut(action, bitmap);
            }
            return;
        }
        // Short EXIT on an open spectrum overlay only closes the overlay, and
        // short EXIT during temporary listening only ends the listen. Neither
        // is the runtime cancel that stops the sweep session; long EXIT, PTT
        // and EXIT from the graph itself still cancel.
        let closes_overlay_only = event == InputEvent::Short(Key::Exit)
            && self.view.screen == Screen::Spectrum
            && self.view.spectrum_menu;
        let stops_listen_only = event == InputEvent::Short(Key::Exit)
            && self.view.screen == Screen::Spectrum
            && !self.view.spectrum_menu
            && self.view.spectrum_listening;
        let modal_exit = matches!(
            self.view.screen,
            Screen::Cb | Screen::Mdc | Screen::RadioName
        ) || (self.view.screen == Screen::Home
            && self.selected_cb().is_some()
            && event == InputEvent::Hold(Key::Exit));
        if matches!(
            event,
            InputEvent::Short(Key::Exit) | InputEvent::Hold(Key::Exit)
        ) && !closes_overlay_only
            && !stops_listen_only
            && !modal_exit
        {
            self.cancel_spectrum_pending();
            self.cancel_temporary = true;
            self.request_scan_stop();
        }
        if event == InputEvent::Hold(Key::Hash) {
            if self.view.screen == Screen::Spectrum {
                self.cancel_spectrum_pending();
                self.cancel_temporary = true;
                self.queue_request(Request::StopSpectrum);
            }
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
        if self.view.screen == Screen::Cb {
            self.cb_input(event);
            return;
        }
        if self.view.screen == Screen::Mdc {
            self.mdc_input(event);
            return;
        }
        if self.view.screen == Screen::RadioName {
            self.radio_name_input(event);
            return;
        }
        if self.view.screen == Screen::Home
            && self.selected_cb().is_some()
            && event == InputEvent::Hold(Key::Exit)
        {
            self.leave_cb();
            return;
        }
        if self.view.screen == Screen::Spectrum {
            if let InputEvent::Short(key) = event {
                if !self.view.spectrum_menu && self.view.spectrum_pending_listen.is_none() {
                    match key {
                        Key::Hash => {
                            self.view.spectrum_auto_listen = !self.view.spectrum_auto_listen;
                            if self.view.spectrum_listening && !self.view.spectrum_auto_listen {
                                self.queue_request(Request::StopListenSpectrum);
                            }
                            return;
                        }
                        Key::Digit4 | Key::Digit5 => {
                            let level = self.view.spectrum_trigger;
                            self.view.spectrum_trigger = if key == Key::Digit4 {
                                level.saturating_add(4).min(96)
                            } else {
                                level.saturating_sub(4).max(4)
                            };
                            return;
                        }
                        Key::Digit3 | Key::Digit9 => {
                            self.queue_request(Request::PanSpectrum {
                                forward: key == Key::Digit3,
                                fine: false,
                            });
                            return;
                        }
                        Key::Digit0 => {
                            self.queue_request(if self.view.spectrum_listening {
                                Request::StopListenSpectrum
                            } else {
                                Request::ResumeSpectrum
                            });
                            return;
                        }
                        Key::Star => {
                            self.apply_spectrum_option(SPECTRUM_MENU_ACCEPT_VFO);
                            return;
                        }
                        _ => {}
                    }
                }
                if !self.view.spectrum_menu
                    && !self.view.spectrum_listening
                    && self.view.spectrum_pending_listen.is_none()
                {
                    let option = match key {
                        Key::Digit1 => Some(SPECTRUM_MENU_STEP_BIGGER),
                        Key::Digit7 => Some(SPECTRUM_MENU_STEP_SMALLER),
                        Key::Digit2 => Some(SPECTRUM_MENU_SPAN_WIDER),
                        Key::Digit8 => Some(SPECTRUM_MENU_SPAN_NARROWER),
                        Key::Digit6 => Some(SPECTRUM_MENU_LISTEN),
                        _ => None,
                    };
                    if let Some(option) = option {
                        self.apply_spectrum_option(option);
                        return;
                    }
                }
            }
            // Mode/slot shortcuts belong to the home screen. Leaving the
            // graph without a Stop request would strand a muted sweep.
            if matches!(
                event,
                InputEvent::Hold(Key::Menu | Key::Star) | InputEvent::Short(Key::Star)
            ) {
                return;
            }
        }
        match event {
            InputEvent::Short(Key::Exit) if stops_listen_only => {
                self.view.spectrum_menu = false;
                self.queue_request(Request::StopListenSpectrum);
            }
            InputEvent::Hold(Key::Menu) => {
                self.view.entry = None;
                self.toggle_mode(bitmap);
            }
            InputEvent::Hold(Key::Exit) => {
                self.view.entry = None;
                self.finish_step_cancel();
                self.view.screen = Screen::Home;
                self.view.menu_group = None;
                self.view.menu_index = 0;
            }
            InputEvent::Hold(Key::Star) if self.view.screen != Screen::Keys => {
                // The Step page owns a draft; switching slots would save it.
                if self.view.screen != Screen::Step {
                    self.switch_slot(self.view.active_slot.other());
                }
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
            InputEvent::Short(Key::Star)
                if self.view.screen == Screen::Home && self.view.scan == ScanState::Paused =>
            {
                self.queue_request(Request::SkipScanTarget);
            }
            InputEvent::Short(Key::Star) if self.view.screen != Screen::Keys => {
                self.view.entry = None;
                self.toggle_mode(bitmap);
            }
            InputEvent::Short(Key::Hash)
                if self.view.screen == Screen::Home && self.view.entry.is_none() =>
            {
                self.open_step(true)
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
        if matches!(
            self.view.screen,
            Screen::Shortwave
                | Screen::Crossband
                | Screen::Gps
                | Screen::Aprs
                | Screen::AprsRx
                | Screen::Cb
                | Screen::Mdc
                | Screen::RadioName
                | Screen::DtmfMemory
        ) {
            self.view.ptt_slot = None;
            self.ptt_suppressed_until_release = true;
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
            self.cancel_spectrum_pending();
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
        self.finish_step_cancel();
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
            ShortcutAction::Modulation => {
                if let Some(mut cb) = self.selected_cb() {
                    cb.am = !cb.am;
                    self.apply_cb_channel(cb);
                } else {
                    self.queue_request(Request::CycleModulation);
                }
            }
            ShortcutAction::Scan => self.toggle_scan(bitmap),
            ShortcutAction::Reverse => self.toggle_reverse(),
            ShortcutAction::Mode => self.toggle_mode(bitmap),
            ShortcutAction::Step => self.open_step(true),
            ShortcutAction::Channels => self.open_channels(bitmap),
            ShortcutAction::Keys => self.view.screen = Screen::Keys,
            ShortcutAction::Lock => self.toggle_lock(),
            ShortcutAction::Zones => self.open_zones(),
            ShortcutAction::SwitchSlot => self.switch_slot(self.view.active_slot.other()),
            ShortcutAction::Spectrum => self.open_spectrum(),
            ShortcutAction::Cb => self.open_cb(),
            ShortcutAction::DualWatch => {
                self.view.dual_watch = !self.view.dual_watch;
                self.view.watch_rx = None;
                if self.view.dual_watch {
                    self.request_scan_stop();
                }
            }
            ShortcutAction::Crossband => {
                self.request_scan_stop();
                self.cancel_temporary = true;
                self.view.crossband_probe = CrossbandProbe::Starting;
                self.view.screen = Screen::Crossband;
            }
        }
    }

    /// The Step page edits a volatile draft. MENU commits it, EXIT restores the
    /// value that was active when the page opened.
    fn selected_cb(&self) -> Option<CbChannel> {
        self.view.cb_for_slot(self.view.active_slot)
    }

    fn open_cb(&mut self) {
        self.cb_from_home = self.view.screen == Screen::Home;
        self.view.cb_draft = Some(self.cb_last);
        self.view.entry = None;
        self.view.screen = Screen::Cb;
    }

    fn apply_cb_channel(&mut self, cb: CbChannel) {
        self.switch_slot(Slot::A);
        if self.cb_before.is_none() {
            self.cb_before = Some(self.current_context());
        }
        self.cb_last = cb;
        self.view.cb = Some(cb);
        self.view.mode = Mode::Vfo;
        self.view.vfo_10hz = cb.frequency_10hz();
        self.view.shift_index = 0;
        self.view.tx_tone_index = 0;
        self.view.reverse = false;
    }

    fn leave_cb(&mut self) {
        if let Some(context) = self.cb_before.take() {
            if self.view.active_slot == Slot::A {
                self.load_context(context);
            } else {
                self.contexts[Slot::A.index()] = context;
            }
            self.view.cb = None;
        }
        self.view.entry = None;
    }

    fn leave_selected_cb(&mut self) {
        if self.selected_cb().is_some() {
            self.leave_cb();
        }
    }

    fn cb_input(&mut self, event: InputEvent) {
        let Some(mut cb) = self.view.cb_draft else {
            return;
        };
        match event {
            InputEvent::Hold(Key::Exit) => {
                self.leave_cb();
                self.view.screen = Screen::Home;
            }
            InputEvent::Short(Key::Exit) => {
                if self.view.entry.take().is_none() {
                    self.view.screen = if self.cb_from_home {
                        Screen::Home
                    } else {
                        Screen::Menu
                    };
                }
            }
            InputEvent::Short(Key::Menu) => {
                if let Some(cb) = self.cb_with_entry(cb) {
                    self.apply_cb_channel(cb);
                    self.view.entry = None;
                    self.view.screen = Screen::Home;
                }
            }
            InputEvent::Short(Key::Up | Key::Down) | InputEvent::Repeat(Key::Up | Key::Down)
                if self.view.entry.is_none() =>
            {
                cb = cb.adjacent(matches!(
                    event,
                    InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up)
                ));
            }
            InputEvent::Short(Key::Hash) if self.view.entry.is_none() => cb.grid = cb.grid.other(),
            InputEvent::Short(Key::Star) if self.view.entry.is_none() => cb.am = !cb.am,
            InputEvent::Short(key) => {
                if let Some(number) = digit(key) {
                    self.enter_digit(number);
                }
            }
            _ => {}
        }
        if self.view.screen == Screen::Cb {
            self.view.cb_draft = Some(cb);
        }
    }

    fn cb_with_entry(&mut self, cb: CbChannel) -> Option<CbChannel> {
        if let Some(entry) = self.view.entry {
            let numbered = cb.with_number(entry.value as u8);
            if numbered.is_none() {
                self.view.notice = Notice::InvalidCbChannel;
            }
            numbered
        } else {
            Some(cb)
        }
    }

    fn mdc_input(&mut self, event: InputEvent) {
        if event == InputEvent::Hold(Key::Exit) {
            self.view.screen = Screen::Home;
            return;
        }
        let slot = self.view.active_slot.index();
        let up = matches!(
            event,
            InputEvent::Short(Key::Up) | InputEvent::Repeat(Key::Up)
        );
        let down = matches!(
            event,
            InputEvent::Short(Key::Down) | InputEvent::Repeat(Key::Down)
        );
        let Some(mut draft) = self.view.mdc_draft.take() else {
            match event {
                InputEvent::Short(Key::Exit) => self.view.screen = Screen::Menu,
                InputEvent::Short(Key::Menu) => {
                    let settings = self.view.mdc[slot];
                    self.view.mdc_draft = Some(match self.view.mdc_option {
                        0 => MdcDraft::Timing(settings.mode),
                        _ => MdcDraft::UnitId(Editor::new(
                            InputMode::Hex,
                            TextValue::from_hex(settings.id),
                        )),
                    });
                }
                _ if up || down => {
                    self.view.mdc_option = adjacent_index(self.view.mdc_option, 2, down)
                }
                _ => {}
            }
            return;
        };
        match event {
            InputEvent::Short(Key::Exit) => return,
            InputEvent::Short(Key::Menu) => {
                match draft {
                    MdcDraft::Timing(mode) => self.view.mdc[slot].mode = mode,
                    MdcDraft::UnitId(editor) => {
                        let value = editor.value().to_hex().and_then(UnitId::new);
                        if !editor.value().is_empty() && value.is_none() {
                            self.view.notice = Notice::InvalidMdcId;
                            self.view.mdc_draft = Some(draft);
                            return;
                        }
                        self.view.mdc[slot].id = value.map(UnitId::get);
                    }
                }
                return;
            }
            _ => {}
        }
        match &mut draft {
            MdcDraft::Timing(mode) if up || down => *mode = adjacent_index(*mode, 4, down),
            MdcDraft::Timing(_) => {}
            MdcDraft::UnitId(editor) => self.text_input(event, editor),
        }
        self.view.mdc_draft = Some(draft);
    }

    fn radio_name_input(&mut self, event: InputEvent) {
        let Some(mut editor) = self.view.radio_name_draft.take() else {
            self.view.screen = Screen::Menu;
            return;
        };
        match event {
            InputEvent::Short(Key::Menu) => {
                self.view.radio_name = editor
                    .value()
                    .ascii_bytes()
                    .and_then(RadioName::from_payload);
                self.view.screen = Screen::Menu;
            }
            InputEvent::Short(Key::Exit) => self.view.screen = Screen::Menu,
            InputEvent::Hold(Key::Exit) => self.view.screen = Screen::Home,
            _ => {
                self.text_input(event, &mut editor);
                self.view.radio_name_draft = Some(editor);
            }
        }
    }

    fn text_input<const N: usize>(&mut self, event: InputEvent, editor: &mut Editor<N>) {
        match event {
            InputEvent::Short(Key::Star) => editor.delete(),
            InputEvent::Short(Key::Hash) => editor.cycle_language(),
            InputEvent::Short(Key::Up) => editor.commit_character(),
            InputEvent::Short(key) | InputEvent::Hold(key) => {
                if let Some(digit) = digit(key) {
                    if !editor.digit(digit, matches!(event, InputEvent::Hold(_))) {
                        self.view.notice = Notice::EntryFull;
                    }
                }
            }
            _ => {}
        }
    }

    fn open_step(&mut self, from_home: bool) {
        if self.selected_cb().is_some() {
            self.open_cb();
            return;
        }
        if self.view.screen != Screen::Step {
            self.step_before = Some(self.view.step_index);
            self.step_home = from_home;
        }
        self.view.screen = Screen::Step;
    }

    fn revert_step(&mut self) {
        if let Some(previous) = self.step_before.take() {
            self.view.step_index = previous;
        }
    }

    /// End a Step draft without committing it and leave no transaction state
    /// behind for a later slot switch.
    fn finish_step_cancel(&mut self) {
        self.revert_step();
        self.step_before = None;
        self.step_home = false;
    }

    fn finish_step_commit(&mut self) {
        self.step_before = None;
        self.step_home = false;
    }

    fn open_spectrum(&mut self) {
        self.view.entry = None;
        self.view.screen = Screen::Spectrum;
        self.spectrum_last_bin = self.spectrum_intervals();
        self.view.spectrum_cursor = self.spectrum_last_bin / 2;
        self.view.spectrum_menu = false;
        self.view.spectrum_option = 0;
        self.view.spectrum_listening = false;
        self.view.spectrum_pending_listen = None;
        self.view.menu_group = Some(MenuGroup::Scanning);
        self.view.menu_index = menu_location(MenuItem::Spectrum).1;
        self.cancel_temporary = true;
        self.queue_request(Request::StartSpectrum);
    }

    /// Current spectrum window width in intervals (points = intervals + 1).
    pub const fn spectrum_intervals(&self) -> u8 {
        SPECTRUM_INTERVALS[self.view.spectrum_span_index as usize]
    }
    /// Spectrum grid step, deliberately independent of the ordinary VFO step.
    pub const fn spectrum_step_10hz(&self) -> u32 {
        STEP_10HZ[self.view.spectrum_step_index as usize]
    }
    /// Keep the UI's short-EXIT contract in step with the runtime listening
    /// phase; the runtime is the owner of the actual RF state.
    pub fn set_spectrum_listening(&mut self, listening: bool) -> bool {
        if !listening {
            self.view.spectrum_pending_listen = None;
        }
        if self.view.spectrum_listening == listening {
            return false;
        }
        self.view.spectrum_listening = listening;
        true
    }

    /// Main calls this only after the RF restore completed for this exact bin.
    /// It atomically clears the pending freeze, pins the displayed cursor to
    /// the confirmed bin and only then marks audio as listening.
    pub fn begin_peak_listen(&mut self, bin: u8) -> bool {
        if self.view.screen != Screen::Spectrum
            || self.view.spectrum_menu
            || !self.view.spectrum_auto_listen
            || self.view.spectrum_listening
            || self.view.spectrum_pending_listen.is_some()
        {
            return false;
        }
        self.view.spectrum_cursor = bin;
        self.view.spectrum_pending_listen = Some(bin);
        true
    }

    pub fn confirm_spectrum_listen(&mut self, bin: u8) -> bool {
        let bin = bin.min(80);
        let Some(pending) = self.view.spectrum_pending_listen else {
            return false;
        };
        if pending != bin {
            return false;
        }
        self.view.spectrum_pending_listen = None;
        let mut changed = true;
        if self.view.screen != Screen::Spectrum {
            return changed;
        }
        if self.view.spectrum_cursor != bin {
            self.view.spectrum_cursor = bin;
            changed = true;
        }
        if !self.view.spectrum_listening {
            self.view.spectrum_listening = true;
            changed = true;
        }
        changed
    }

    /// Cancel a submitted Listen that has not reached its ACK yet.
    pub fn cancel_spectrum_pending(&mut self) -> bool {
        self.view.spectrum_pending_listen.take().is_some()
    }

    /// Place the cursor on an explicit bin after a window change. Frequency
    /// preservation is computed by the caller that owns both snapshots.
    pub fn place_spectrum_cursor(&mut self, index: u8) -> bool {
        let index = index.min(80);
        if self.view.screen == Screen::Spectrum && self.view.spectrum_cursor != index {
            self.view.spectrum_cursor = index;
            return true;
        }
        false
    }

    /// Keep the cursor inside a published window after a span change.
    pub fn clamp_spectrum_cursor(&mut self, last: u8) -> bool {
        self.spectrum_last_bin = last.min(80);
        if self.view.screen == Screen::Spectrum && self.view.spectrum_cursor > last {
            self.view.spectrum_cursor = last;
            if self.view.spectrum_option > 0 {
                self.view.spectrum_option = self.view.spectrum_option.min(SPECTRUM_MENU_LEN - 1);
            }
            return true;
        }
        false
    }

    /// MENU confirms the highlighted spectrum option. Span and step changes are
    /// applied by an explicit restart; EXIT only leaves the menu.
    fn confirm_spectrum_option(&mut self) {
        self.apply_spectrum_option(self.view.spectrum_option);
    }

    fn apply_spectrum_option(&mut self, option: u8) {
        match option {
            SPECTRUM_MENU_LISTEN => {
                self.view.spectrum_menu = false;
                let bin = self.view.spectrum_cursor;
                if !self.view.spectrum_listening && self.view.spectrum_pending_listen.is_none() {
                    self.view.spectrum_pending_listen = Some(bin);
                }
                self.queue_request(Request::ListenSpectrum { bin });
            }
            SPECTRUM_MENU_ACCEPT_VFO => {
                self.view.spectrum_menu = false;
                self.queue_request(Request::AcceptSpectrum {
                    bin: self.view.spectrum_cursor,
                });
            }
            SPECTRUM_MENU_SPAN_WIDER | SPECTRUM_MENU_SPAN_NARROWER => {
                let wider = option == SPECTRUM_MENU_SPAN_WIDER;
                let last = SPECTRUM_INTERVALS.len() as u8 - 1;
                let next = if wider {
                    self.view.spectrum_span_index.saturating_add(1).min(last)
                } else {
                    self.view.spectrum_span_index.saturating_sub(1)
                };
                if next != self.view.spectrum_span_index {
                    self.view.spectrum_span_index = next;
                    self.view.spectrum_menu = false;
                    self.queue_request(Request::RestartSpectrum);
                }
            }
            SPECTRUM_MENU_STEP_BIGGER | SPECTRUM_MENU_STEP_SMALLER => {
                let bigger = option == SPECTRUM_MENU_STEP_BIGGER;
                let last = STEP_10HZ.len() as u8 - 1;
                let next = if bigger {
                    self.view.spectrum_step_index.saturating_add(1).min(last)
                } else {
                    self.view.spectrum_step_index.saturating_sub(1)
                };
                if next != self.view.spectrum_step_index {
                    self.view.spectrum_step_index = next;
                    self.view.spectrum_menu = false;
                    self.queue_request(Request::RestartSpectrum);
                }
            }
            SPECTRUM_MENU_CLEAR_PEAKS => {
                self.view.spectrum_menu = false;
                self.queue_request(Request::ResetSpectrumPeaks);
            }
            _ => {
                self.view.spectrum_menu = false;
                self.queue_request(Request::StopSpectrum);
                self.view.screen = Screen::Home;
            }
        }
    }

    fn toggle_scan(&mut self, bitmap: &[u8; 128]) {
        if self.view.scan != ScanState::Stopped {
            self.request_scan_stop();
        } else if self.view.ptt_slot.is_some() {
            self.view.notice = Notice::TxDisabled;
        } else if self.selected_cb().is_some() {
            self.view.notice = Notice::RadioUnavailable;
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
        if (self.view.mode == Mode::Channel && self.view.channel.is_some())
            || (self.view.mode == Mode::Vfo
                && self.view.shift_index != 0
                && self.selected_cb().is_none())
        {
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
        self.finish_step_cancel();
        self.view.screen = Screen::Home;
        if self.selected_cb().is_some() {
            self.leave_cb();
            return;
        }
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
            Screen::Home if self.selected_cb().is_some() => {
                if let Some(cb) = self.selected_cb() {
                    self.apply_cb_channel(cb.adjacent(!forward));
                }
            }
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
                if self.view.spectrum_menu {
                    self.view.spectrum_option =
                        adjacent_index(self.view.spectrum_option, SPECTRUM_MENU_LEN, forward);
                } else if self.view.spectrum_pending_listen.is_none() {
                    let cursor = self.view.spectrum_cursor;
                    if (forward && cursor == self.spectrum_last_bin) || (!forward && cursor == 0) {
                        self.queue_request(Request::PanSpectrum {
                            forward,
                            fine: true,
                        });
                    } else if !self.view.spectrum_listening {
                        self.view.spectrum_cursor = if forward { cursor + 1 } else { cursor - 1 };
                    }
                }
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

    fn apply_rx_tone_search(&mut self) {
        let Some(search) = self.view.rx_tone_search else {
            return;
        };
        let Some(tone_raw) = search.result_raw.filter(|raw| {
            matches!(
                uv98_codeplug::channels::Tone::decode(*raw),
                Some(
                    uv98_codeplug::channels::Tone::Ctcss { .. }
                        | uv98_codeplug::channels::Tone::Dcs { .. }
                )
            )
        }) else {
            return;
        };
        if self.rx_tone_source() != Some(search.source) {
            self.view.rx_tone_search = None;
            self.view.screen = Screen::Menu;
            self.view.notice = Notice::RadioUnavailable;
            return;
        }
        self.view.rx_tone_search = None;
        self.view.screen = Screen::Menu;
        self.queue_request(Request::ApplyRxToneOverride {
            source: search.source,
            tone_raw,
        });
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
                    self.view.reverse = false;
                    self.view.mode = Mode::Vfo;
                }
                EntryKind::CbChannel => {
                    let Some(cb) = self.selected_cb() else {
                        return;
                    };
                    let Some(cb) = self.cb_with_entry(cb) else {
                        return;
                    };
                    self.apply_cb_channel(cb);
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
                    MenuItem::Cb => self.open_cb(),
                    MenuItem::Mdc => {
                        self.view.mdc_option = 0;
                        self.view.screen = Screen::Mdc;
                    }
                    MenuItem::BandPreset => self.open_setting(Setting::BandPreset),
                    MenuItem::GpsEnable => self.open_setting(Setting::GpsEnable),
                    MenuItem::GpsBaud => self.open_setting(Setting::GpsBaud),
                    MenuItem::Dtmf => self.open_setting(Setting::Dtmf),
                    MenuItem::NameRoger => self.open_setting(Setting::NameRoger),
                    MenuItem::MicrophoneGain => self.open_setting(Setting::MicrophoneGain),
                    MenuItem::TxTimeout => self.open_setting(Setting::TxTimeout),
                    MenuItem::RadioName => {
                        let text = self.view.radio_name.as_ref().map_or("", RadioName::as_str);
                        self.view.radio_name_draft = Some(Editor::new(
                            InputMode::RadioName,
                            TextValue::from_ascii(text),
                        ));
                        self.name_last_digit_at = None;
                        self.view.screen = Screen::RadioName;
                    }
                    MenuItem::Gps => self.view.screen = Screen::Gps,
                    MenuItem::Aprs => self.view.screen = Screen::Aprs,
                    MenuItem::AprsRx => {
                        self.select_aprs_packet(0);
                        self.view.screen = Screen::AprsRx;
                    }
                    MenuItem::Mode => self.open_setting(Setting::Mode),
                    MenuItem::Channels => self.open_channels(bitmap),
                    MenuItem::ChannelDetails => self.view.screen = Screen::ChannelDetails,
                    MenuItem::Shift
                        if self.view.mode == Mode::Vfo && self.selected_cb().is_none() =>
                    {
                        self.open_setting(Setting::Shift)
                    }
                    MenuItem::TxTone
                        if self.view.mode == Mode::Vfo && self.selected_cb().is_none() =>
                    {
                        self.open_setting(Setting::TxTone)
                    }
                    MenuItem::Shift | MenuItem::TxTone => self.view.screen = Screen::ChannelDetails,
                    MenuItem::Step => self.open_step(false),
                    MenuItem::About => self.view.screen = Screen::About,
                    MenuItem::Reverse => self.open_setting(Setting::Reverse),
                    MenuItem::Scan => self.open_setting(Setting::Scan),
                    MenuItem::ScanPriority => {
                        if self.view.scan != ScanState::Stopped {
                            self.view.notice = Notice::ScanPriorityStopScan;
                            return;
                        }
                        self.view.scan_priority_enabled = self
                            .scan_zone_priority(bitmap)
                            .is_some_and(|priority| Some(priority) == self.view.channel);
                        self.open_setting(Setting::ScanPriority);
                    }
                    MenuItem::RxToneSearch => self.start_rx_tone_search(bitmap),
                    MenuItem::DtmfMemory => self.open_dtmf_memory(),
                    MenuItem::Shortcuts => self.view.screen = Screen::Shortcuts,
                    MenuItem::Power => self.open_setting(Setting::Power),
                    MenuItem::Monitor => self.open_setting(Setting::Monitor),
                    MenuItem::Modulation => self.open_setting(Setting::Modulation),
                    MenuItem::Zones => self.open_zones(),
                    MenuItem::ActiveSlot => self.open_setting(Setting::ActiveSlot),
                    MenuItem::TopKey => self.open_setting(Setting::TopKey),
                    MenuItem::BottomKey => self.open_setting(Setting::BottomKey),
                    MenuItem::SideKeys => self.view.screen = Screen::SideKeys,
                    MenuItem::DisplayLight => self.open_setting(Setting::DisplayLight),
                    MenuItem::KeyLight => self.open_setting(Setting::KeyLight),
                    MenuItem::Spectrum => self.open_spectrum(),
                    MenuItem::Shortwave => {
                        self.view.shortwave_scope = false;
                        self.view.shortwave_entry = None;
                        self.view.shortwave_band_draft = None;
                        self.view.shortwave_status = ShortwaveStatus::Starting;
                        self.view.screen = Screen::Shortwave;
                    }
                }
            }
            Screen::Spectrum => {
                if self.view.spectrum_menu {
                    self.confirm_spectrum_option();
                } else {
                    self.view.spectrum_menu = true;
                    self.view.spectrum_option = 0;
                }
            }
            Screen::Channels => {
                if let Some(slot) = self
                    .view
                    .list_channel
                    .filter(|&slot| channel_active(bitmap, slot))
                {
                    self.leave_selected_cb();
                    self.view.channel = Some(slot);
                    self.view.mode = Mode::Channel;
                    self.view.screen = Screen::Home;
                } else {
                    self.view.notice = Notice::NoChannels;
                }
            }
            Screen::Step => {
                // MENU commits the draft; it is the only confirmation path.
                self.finish_step_commit();
                self.view.screen = Screen::Home;
            }
            Screen::Shortcuts => self.open_binding_action(),
            Screen::ShortcutAction => {
                self.view.bindings[self.view.binding_index as usize] =
                    SHORTCUT_ACTIONS[self.view.binding_action_index as usize];
                self.view.screen = Screen::Shortcuts;
            }
            Screen::Zones => {
                let selected = self.view.zone_cursor;
                if self.view.zone != selected {
                    self.leave_selected_cb();
                    self.view.zone = selected;
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
            Screen::RxToneSearch => self.apply_rx_tone_search(),
            Screen::DtmfMemory => {}
            // A test page must allow MENU/EXIT to leave; other keys stay visible.
            Screen::Shortwave
            | Screen::Crossband
            | Screen::Gps
            | Screen::Aprs
            | Screen::AprsRx
            | Screen::Cb
            | Screen::Mdc
            | Screen::RadioName => {}
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
        if self.view.screen == Screen::Spectrum {
            if self.view.spectrum_menu {
                self.view.spectrum_menu = false;
            } else {
                self.view.screen = Screen::Home;
            }
            return;
        }
        if self.view.screen == Screen::RxToneSearch {
            self.view.rx_tone_search = None;
            self.view.screen = Screen::Menu;
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
            Screen::Step => {
                let from_home = self.step_home;
                self.finish_step_cancel();
                if from_home {
                    Screen::Home
                } else {
                    Screen::Menu
                }
            }
            _ => Screen::Menu,
        };
    }

    fn enter_digit(&mut self, digit: u8) {
        let kind = match self.view.screen {
            Screen::Cb => EntryKind::CbChannel,
            Screen::Home if self.selected_cb().is_some() => EntryKind::CbChannel,
            Screen::Home if self.view.mode == Mode::Channel => EntryKind::Channel,
            Screen::Home => EntryKind::Frequency,
            _ => return,
        };
        let mut entry = self.view.entry.unwrap_or(Entry {
            kind,
            value: 0,
            digits: 0,
        });
        if entry.digits >= entry.kind.maximum_digits() {
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
    fn learned_top_key_runs_only_committed_receive_action() {
        let bitmap = [0; 128];
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.open_setting(Setting::TopKey);
        assert!(
            !ui.handle(InputEvent::Short(Key::Aux16), &bitmap)
                .ptt_tx_requested
        );
        ui.view.setting_index = 1;
        ui.commit_setting(&bitmap);
        ui.view.screen = Screen::Home;
        let matched = ui.handle(InputEvent::Short(Key::Aux16), &bitmap);
        assert_eq!(matched.request, Some(Request::ToggleMonitor));
        assert!(!matched.ptt_tx_requested);
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Aux17), &bitmap).request,
            None
        );
    }

    #[test]
    fn shortwave_am_soft_mute_and_high_pass_are_independent() {
        let bitmap = [0; 128];
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.screen = Screen::Shortwave;
        ui.view.shortwave_mode = ShortwaveMode::Am;
        for (key, soft_mute, high_pass) in [
            (Key::Digit3, false, true), (Key::Digit2, true, true),
            (Key::Digit3, true, false), (Key::Digit2, false, false),
        ] {
            ui.handle(InputEvent::Hold(key), &bitmap);
            assert_eq!((ui.view.shortwave_soft_mute, ui.view.shortwave_high_pass), (soft_mute, high_pass));
        }
    }

    #[test]
    fn bottom_key_cycles_step_only_on_shortwave_receiver_screen() {
        let bitmap = [0; 128];
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.bottom_action = 1;
        ui.view.screen = Screen::Shortwave;
        ui.view.shortwave_step = ShortwaveMode::Am.steps_hz().len() as u8 - 1;
        ui.view.shortwave_scan = true;
        let change = press(&mut ui, Key::Aux17, &bitmap);
        assert_eq!(ui.view.shortwave_step_hz(false), 1000);
        assert!(!ui.view.shortwave_scan);
        assert_eq!(change.request, None);
        assert!(!change.ptt_tx_requested);

        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        press(&mut ui, Key::Aux17, &bitmap);
        assert_eq!(ui.view.shortwave_step_hz(false), 1000);
        press(&mut ui, Key::Exit, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        let change = press(&mut ui, Key::Aux17, &bitmap);
        assert_eq!(ui.view.shortwave_step_hz(false), 1000);
        assert_eq!(change.request, Some(Request::ToggleMonitor));
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
    fn aprs_auto_requires_opt_in_and_disarms_on_exit_or_ptt() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Aprs, &bitmap);
        assert!(!ui.view.aprs_auto_available);
        assert!(!ui.view.aprs_auto_armed);
        press(&mut ui, Key::Star, &bitmap);
        assert!(!ui.view.aprs_auto_armed);

        ui.set_aprs_status(AprsStatus::Ready);
        assert_eq!(press(&mut ui, Key::Star, &bitmap).request, None);
        assert!(!ui.view.aprs_auto_armed);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::SendAprs)
        );
        ui.set_aprs_auto_available(true);
        assert_eq!(press(&mut ui, Key::Star, &bitmap).request, None);
        assert!(ui.view.aprs_auto_armed);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::SendAprs)
        );
        ui.set_aprs_auto_available(false);
        assert!(ui.view.aprs_auto_armed);
        press(&mut ui, Key::Star, &bitmap);
        assert!(!ui.view.aprs_auto_armed);
        ui.set_aprs_auto_available(true);
        ui.set_aprs_status(AprsStatus::NoFix);
        press(&mut ui, Key::Star, &bitmap);
        assert!(!ui.view.aprs_auto_armed);

        ui.set_aprs_status(AprsStatus::Ready);
        for (down, up) in [
            (InputEvent::Ptt(true), InputEvent::Ptt(false)),
            (InputEvent::PttB(true), InputEvent::PttB(false)),
        ] {
            press(&mut ui, Key::Star, &bitmap);
            assert!(ui.view.aprs_auto_armed);
            let change = ui.handle(down, &bitmap);
            assert!(!ui.view.aprs_auto_armed);
            assert_eq!(change.request, None);
            assert!(!change.ptt_tx_requested);
            assert_eq!(ui.view.screen, Screen::Aprs);
            press(&mut ui, Key::Star, &bitmap);
            assert!(!ui.view.aprs_auto_armed);
            ui.handle(up, &bitmap);
        }

        press(&mut ui, Key::Star, &bitmap);
        ui.set_aprs_status(AprsStatus::Unsupported);
        assert!(!ui.view.aprs_auto_armed);
        ui.set_aprs_status(AprsStatus::Ready);
        assert!(!ui.view.aprs_auto_armed);

        press(&mut ui, Key::Star, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert!(!ui.view.aprs_auto_armed);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.screen, Screen::Aprs);
        assert!(!ui.view.aprs_auto_armed);
        press(&mut ui, Key::Star, &bitmap);
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        assert_eq!(ui.view.screen, Screen::Home);
        assert!(!ui.view.aprs_auto_armed);
    }

    #[test]
    fn aprs_rx_browses_four_packets_and_remains_receive_only() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.set_aprs_status(AprsStatus::Ready);
        let opened = choose_menu(&mut ui, MenuItem::AprsRx, &bitmap);
        assert_eq!(opened.request, None);
        assert_eq!(ui.view.screen, Screen::AprsRx);
        assert!(!ui.view.aprs_rx_available);
        assert_eq!(ui.view.aprs_rx_count, 0);
        for event in [
            InputEvent::Short(Key::Menu),
            InputEvent::Short(Key::Down),
            InputEvent::Short(Key::Up),
            InputEvent::Short(Key::Aux16),
            InputEvent::Short(Key::Aux17),
            InputEvent::Ptt(true),
            InputEvent::Ptt(false),
            InputEvent::PttB(true),
            InputEvent::PttB(false),
        ] {
            let change = ui.handle(event, &bitmap);
            assert_eq!(change.request, None);
            assert!(!change.ptt_tx_requested);
            assert_eq!(ui.view.ptt_slot, None);
        }
        assert_eq!(ui.view.aprs_rx_index, 0);

        ui.set_aprs_rx_available(true);
        ui.push_aprs_packet(b"N0CALL-10 extra", &[0x1f; 64]);
        assert_eq!(ui.view.aprs_rx_packet.source(), "N0CALL-10");
        assert_eq!(ui.view.aprs_rx_packet.information().len(), 40);
        assert!(ui
            .view
            .aprs_rx_packet
            .information()
            .bytes()
            .all(|b| b == b'?'));
        ui.push_aprs_packet(b"A", b"Two");
        press(&mut ui, Key::Down, &bitmap);
        ui.push_aprs_packet(b"B", b"Three");
        assert_eq!(ui.view.aprs_rx_index, 2);
        assert_eq!(ui.view.aprs_rx_packet.source(), "N0CALL-10");
        ui.push_aprs_packet(b"C", b"Four");
        assert_eq!(ui.view.aprs_rx_packet.source(), "N0CALL-10");
        ui.push_aprs_packet(b"D", b"Five");
        assert_eq!(ui.view.aprs_rx_count, 4);
        assert_eq!(ui.view.aprs_rx_index, 3);
        assert_eq!(ui.view.aprs_rx_packet.source(), "A");
        ui.set_aprs_rx_available(false);
        ui.handle(InputEvent::Repeat(Key::Up), &bitmap);
        assert_eq!(ui.view.aprs_rx_packet.source(), "B");

        ui.handle(InputEvent::Ptt(true), &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.screen, Screen::AprsRx);
        assert_eq!(ui.view.aprs_rx_packet.information(), "Five");

        choose_menu(&mut ui, MenuItem::Aprs, &bitmap);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::SendAprs)
        );
    }

    #[test]
    fn menu_location_matches_every_visible_catalog_item() {
        for item in MENU_ITEMS {
            let (group, index) = menu_location(item);
            assert_eq!(
                menu_entries(group)[index as usize],
                MenuEntry::Item(item),
                "{item:?}"
            );
        }
    }

    #[test]
    fn tx_timeout_defaults_to_thirty_and_commits_only_on_menu() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert_eq!(ui.view.tx_timeout, TxTimeout::Seconds30);
        assert_eq!(
            menu_location(MenuItem::TxTimeout),
            (Some(MenuGroup::Keys), 5)
        );
        assert_eq!(
            choose_menu(&mut ui, MenuItem::TxTimeout, &bitmap).request,
            None
        );
        assert_eq!(ui.view.setting, Some(Setting::TxTimeout));
        assert_eq!(ui.view.setting_index, 2);
        assert_eq!(press(&mut ui, Key::Down, &bitmap).request, None);
        assert_eq!(ui.view.setting_index, 0);
        assert_eq!(ui.view.tx_timeout, TxTimeout::Seconds30);
        assert_eq!(press(&mut ui, Key::Exit, &bitmap).request, None);
        assert_eq!(ui.view.tx_timeout, TxTimeout::Seconds30);

        for (index, (choice, milliseconds)) in [
            (TxTimeout::Seconds10, 10_000),
            (TxTimeout::Seconds20, 20_000),
            (TxTimeout::Seconds30, 30_000),
        ]
        .into_iter()
        .enumerate()
        {
            choose_menu(&mut ui, MenuItem::TxTimeout, &bitmap);
            assert_eq!(press(&mut ui, Key::Down, &bitmap).request, None);
            assert_eq!(usize::from(ui.view.setting_index), index);
            assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
            assert_eq!(ui.view.tx_timeout, choice);
            assert_eq!(ui.view.tx_timeout.milliseconds(), milliseconds);
        }
    }

    #[test]
    fn microphone_gain_commits_only_on_menu_and_rejects_invalid_restore() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.apply_microphone_gain(12);
        assert_eq!(
            choose_menu(&mut ui, MenuItem::MicrophoneGain, &bitmap).request,
            None
        );
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.microphone_gain, 12);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.microphone_gain, 12);
        choose_menu(&mut ui, MenuItem::MicrophoneGain, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
        assert_eq!(ui.view.microphone_gain, 13);
        ui.select_slot(Slot::B);
        assert_eq!(ui.view.microphone_gain, 13);
        ui.apply_microphone_gain(31);
        choose_menu(&mut ui, MenuItem::MicrophoneGain, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.microphone_gain, 0);
        ui.apply_microphone_gain(255);
        assert_eq!(ui.view.microphone_gain, 0);
    }

    #[test]
    fn rx_tone_search_waits_then_applies_only_accepted_detected_tone() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let started = choose_menu(&mut ui, MenuItem::RxToneSearch, &bitmap);
        let source = RxToneSource::Channel {
            slot: Slot::A,
            channel: 0,
        };
        assert_eq!(started.request, Some(Request::StartRxToneSearch { source }));
        assert_eq!(ui.view.screen, Screen::RxToneSearch);
        assert_eq!(ui.view.rx_tone_search.unwrap().result_raw, None);
        assert!(!started.cancel_temporary);

        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
        assert_eq!(ui.view.screen, Screen::RxToneSearch);
        ui.set_rx_tone_search_result(Some(0x1302));
        let applied = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(
            applied.request,
            Some(Request::ApplyRxToneOverride {
                source,
                tone_raw: 0x1302,
            })
        );
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.rx_tone_search, None);
        assert!(!applied.cancel_temporary);
        assert!(!applied.ptt_tx_requested);
    }

    #[test]
    fn rx_tone_search_exit_and_ptt_cancel_without_tx_or_temporary_stop() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::RxToneSearch, &bitmap);
        let exited = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.rx_tone_search, None);
        assert!(!exited.cancel_temporary);
        assert_eq!(exited.request, None);

        choose_menu(&mut ui, MenuItem::RxToneSearch, &bitmap);
        let ptt = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.rx_tone_search, None);
        assert!(ui.view.ptt_down);
        assert!(!ptt.ptt_tx_requested);
        assert!(!ptt.cancel_temporary);
        assert_eq!(ptt.request, None);
        let release = ui.handle(InputEvent::Ptt(false), &bitmap);
        assert!(!release.ptt_tx_requested);
    }

    #[test]
    fn dtmf_memory_slots_edit_save_replace_and_cancel_in_ram() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        assert!(ui.dtmf_memory().is_empty());
        choose_menu(&mut ui, MenuItem::DtmfMemory, &bitmap);
        assert_eq!(ui.view.screen, Screen::DtmfMemory);
        assert_eq!(ui.view.dtmf_memory_page.unwrap().slot, 0);

        press(&mut ui, Key::Menu, &bitmap);
        for key in [Key::Digit1, Key::Digit2, Key::Star, Key::Digit9, Key::Hash] {
            press(&mut ui, key, &bitmap);
        }
        press(&mut ui, Key::Up, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.dtmf_memory(), b"12*9");
        assert_eq!(ui.view.screen, Screen::DtmfMemory);

        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.dtmf_memory_page.unwrap().slot, 1);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Digit7, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.dtmf_memory(), b"12*9");
        assert_eq!(ui.view.dtmf_memory_page.unwrap().len, 0);

        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Digit3, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.dtmf_memory(), b"3");
        assert_eq!(ui.view.dtmf_memory_page.unwrap().slot, 1);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.dtmf_memory_page, None);
    }

    #[test]
    fn dtmf_memory_limits_length_and_blocks_tx_and_side_actions() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::DtmfMemory, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        for _ in 0..DTMF_MEMORY_LENGTH {
            press(&mut ui, Key::Digit1, &bitmap);
        }
        press(&mut ui, Key::Digit2, &bitmap);
        assert_eq!(ui.view.notice, Notice::EntryFull);
        assert_eq!(
            ui.view.dtmf_memory_page.unwrap().len as usize,
            DTMF_MEMORY_LENGTH
        );
        assert!(ui.dtmf_memory().is_empty());

        ui.view.scan = ScanState::Running;
        ui.view.side_bindings = [SideAction::Monitor, SideAction::Power];
        let ptt = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert!(ui.view.ptt_down);
        assert!(!ptt.ptt_tx_requested);
        assert_eq!(ptt.request, None);
        assert_eq!(ui.view.scan, ScanState::Running);
        let ptt_release = ui.handle(InputEvent::Ptt(false), &bitmap);
        assert!(!ptt_release.ptt_tx_requested);

        let ptt_b = ui.handle(InputEvent::PttB(true), &bitmap);
        assert!(ui.view.ptt_down);
        assert!(!ptt_b.ptt_tx_requested);
        assert_eq!(ptt_b.request, None);
        assert_eq!(ui.view.scan, ScanState::Running);
        ui.handle(InputEvent::PttB(false), &bitmap);
        assert_eq!(
            ui.view.dtmf_memory_page.unwrap().len as usize,
            DTMF_MEMORY_LENGTH
        );
    }

    #[test]
    fn rx_tone_search_rejects_unstable_or_non_fm_context() {
        let bitmap = catalog(&[0]);
        let (group, index) = menu_location(MenuItem::RxToneSearch);
        for (am, scanning, reverse) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
            ui.radio_am = am;
            ui.view.scan = if scanning {
                ScanState::Running
            } else {
                ScanState::Stopped
            };
            ui.view.reverse = reverse;
            ui.view.screen = Screen::Menu;
            ui.view.menu_group = group;
            ui.view.menu_index = index;
            let denied = press(&mut ui, Key::Menu, &bitmap);
            assert_eq!(denied.request, None);
            assert_eq!(ui.view.screen, Screen::Menu);
            assert_eq!(ui.view.notice, Notice::RadioUnavailable);
            assert!(!denied.cancel_temporary);
        }
    }

    #[test]
    fn shortwave_scope_resolution_cycles_without_retuning_center() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.screen = Screen::Shortwave;
        let tuning = (ui.view.shortwave_khz, ui.view.shortwave_mode);
        ui.handle(InputEvent::Hold(Key::Digit0), &bitmap);
        assert!(ui.view.shortwave_scope);
        for points in [16, 32, 8] {
            assert_eq!(press(&mut ui, Key::Hash, &bitmap).request, None);
            assert_eq!(ui.view.shortwave_scope_points, points);
            assert_eq!((ui.view.shortwave_khz, ui.view.shortwave_mode), tuning);
        }
    }

    #[test]
    fn cb_draft_keeps_b_unchanged_and_commits_a_simplex_until_explicit_exit() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.shift_index = 1;
        ui.view.tx_tone_index = 9;
        ui.view.reverse = true;
        ui.view.step_index = 5;
        let original_a = ui.context(Slot::A);
        ui.select_slot(Slot::B);
        ui.view.mode = Mode::Vfo;
        ui.view.vfo_10hz = 43_350_000;
        let original_b = ui.context(Slot::B);

        assert_eq!(choose_menu(&mut ui, MenuItem::Cb, &bitmap).request, None);
        assert_eq!(ui.view.cb_draft, Some(CbChannel::default()));
        assert_eq!(ui.view.active_slot, Slot::B);
        assert_eq!(ui.view.cb, None);
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        press(&mut ui, Key::Hash, &bitmap);
        assert_eq!(ui.view.cb_draft.unwrap().frequency_10hz(), 2_713_000);
        press(&mut ui, Key::Star, &bitmap);
        digits(&mut ui, &[Key::Digit9, Key::Digit9], &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.notice, Notice::InvalidCbChannel);
        assert_eq!(ui.view.screen, Screen::Cb);
        press(&mut ui, Key::Exit, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.context(Slot::A), original_a);
        assert_eq!(ui.context(Slot::B), original_b);
        assert_eq!(ui.view.active_slot, Slot::B);

        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.cb_draft, Some(CbChannel::default()));
        press(&mut ui, Key::Star, &bitmap);
        digits(&mut ui, &[Key::Digit2, Key::Digit3], &bitmap);
        let committed = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(committed.request, None);
        assert!(committed.slot_changed);
        assert_eq!(ui.view.active_slot, Slot::A);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.view.cb.unwrap().number(), 23);
        assert!(!ui.view.cb.unwrap().is_am());
        assert_eq!(ui.view.vfo_10hz, 2_725_500);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.shift_index, 0);
        assert_eq!(ui.view.tx_tone_index, 0);
        assert!(!ui.view.reverse);
        assert_eq!(ui.context(Slot::B), original_b);
        assert_eq!(ui.view.cb_for_slot(Slot::B), None);
        ui.view.shift_index = 1;
        ui.handle(InputEvent::Hold(Key::Digit4), &bitmap);
        assert!(!ui.view.reverse);
        ui.view.shift_index = 0;

        press(&mut ui, Key::Up, &bitmap);
        assert_eq!(ui.view.vfo_10hz, 2_723_500); // Channel 24 after 23.
        digits(&mut ui, &[Key::Digit0, Key::Digit1], &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.cb.unwrap().number(), 40);
        assert_eq!(ui.view.vfo_10hz, 2_740_500);
        assert!(ui.set_binding(6, ShortcutAction::Cb));
        ui.select_slot(Slot::B);
        ui.handle(InputEvent::Hold(Key::Digit6), &bitmap);
        let last = ui.view.cb_draft;
        assert_eq!(last.unwrap().number(), 40);
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        assert_eq!(ui.view.cb, None);
        assert_eq!(ui.view.cb_draft, None);
        assert_eq!(ui.view.active_slot, Slot::B);
        assert_eq!(ui.context(Slot::A), original_a);
        assert_eq!(ui.context(Slot::B), original_b);
        ui.handle(InputEvent::Hold(Key::Digit6), &bitmap);
        assert_eq!(ui.view.cb_draft, last);
    }

    #[test]
    fn mdc_modes_and_ids_commit_per_slot_and_drafts_block_ptt() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let disabled = MdcSettings { mode: 0, id: None };
        assert_eq!(ui.view.mdc_for_slot(Slot::A), disabled);
        assert_eq!(ui.view.mdc_for_slot(Slot::B), disabled);
        choose_menu(&mut ui, MenuItem::Mdc, &bitmap);
        assert_eq!(ui.view.screen, Screen::Mdc);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.mdc_draft, Some(MdcDraft::Timing(1)));
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.mdc_for_slot(Slot::A), disabled);
        assert_eq!(ui.view.mdc_draft, None);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Up, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.mdc_for_slot(Slot::A).mode, 3);

        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        for _ in 0..4 {
            press(&mut ui, Key::Digit0, &bitmap);
        }
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.notice, Notice::InvalidMdcId);
        assert_eq!(ui.view.mdc_for_slot(Slot::A).id, None);
        press(&mut ui, Key::Exit, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Digit0, &bitmap);
        ui.handle(InputEvent::Hold(Key::Digit1), &bitmap);
        ui.handle(InputEvent::Hold(Key::Digit6), &bitmap);
        press(&mut ui, Key::Digit1, &bitmap);
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
        let a = MdcSettings {
            mode: 3,
            id: Some(0x0af1),
        };
        assert_eq!(ui.view.mdc_for_slot(Slot::A), a);

        press(&mut ui, Key::Menu, &bitmap);
        let ptt = ui.handle(InputEvent::PttB(true), &bitmap);
        assert_eq!(ptt.ptt_slot, None);
        assert!(!ptt.ptt_tx_requested);
        ui.handle(InputEvent::PttB(false), &bitmap);
        press(&mut ui, Key::Exit, &bitmap);

        ui.select_slot(Slot::B);
        assert_eq!(ui.view.mdc_for_slot(Slot::A), a);
        assert_eq!(ui.view.mdc_for_slot(Slot::B), disabled);
        choose_menu(&mut ui, MenuItem::Mdc, &bitmap);
        assert_eq!(ui.view.active_slot, Slot::B);
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(
            ui.view.mdc_for_slot(Slot::B),
            MdcSettings { mode: 1, id: None }
        );
        assert_eq!(ui.view.mdc_for_slot(Slot::A), a);
    }

    #[test]
    fn radio_name_multitap_commits_global_text_and_uses_separate_roger_toggle() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::RadioName, &bitmap);
        for (at, key) in [
            (0, Key::Digit2),
            (100, Key::Digit2),
            (800, Key::Digit2),
            (900, Key::Digit0),
            (1000, Key::Digit9),
            (1100, Key::Digit9),
        ] {
            ui.handle_at(InputEvent::Short(key), &bitmap, at);
        }
        assert!(!ui.handle(InputEvent::Ptt(true), &bitmap).ptt_tx_requested);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        let (name, enabled) = ui.view.radio_name_settings();
        assert_eq!(name.unwrap().as_str(), "BA X");
        assert!(!enabled);
        assert_eq!(ui.view.roger_name(), None);
        ui.select_slot(Slot::B);
        choose_menu(&mut ui, MenuItem::RadioName, &bitmap);
        press(&mut ui, Key::Star, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.radio_name, name);
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        choose_menu(&mut ui, MenuItem::NameRoger, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.roger_name(), None);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.roger_name(), name);
        assert_eq!(ui.view.radio_name_settings(), (name, true));
    }

    #[test]
    fn review_spectrum_overlay_exit_does_not_cancel_runtime_sweep() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert!(ui.view().spectrum_menu);
        let update = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view().screen, Screen::Spectrum);
        assert!(!ui.view().spectrum_menu);
        assert_eq!(update.request, None);
        // Firmware main maps this flag directly to scope Event::Stop.
        assert!(
            !update.cancel_temporary,
            "closing only the overlay must not stop the sweep"
        );
    }

    #[test]
    fn ptt_routing_does_not_commit_or_leak_step_draft() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.step_index = 1;
        let old_a = ui.context(Slot::A).step_index;
        let old_b = ui.context(Slot::B).step_index;
        assert_ne!(old_a, old_b);
        assert!(ui.set_side_binding(0, SideAction::PttB));
        press(&mut ui, Key::Hash, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_ne!(ui.view().step_index, old_a);
        let update = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert_eq!(update.ptt_slot, Some(Slot::B));
        assert!(!update.slot_changed);
        assert_eq!(ui.view().active_slot, Slot::A);
        assert_eq!(ui.view().screen, Screen::Home);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        ui.handle(InputEvent::Hold(Key::Hash), &bitmap);
        assert_eq!(
            (
                ui.context(Slot::A).step_index,
                ui.context(Slot::B).step_index
            ),
            (old_a, old_b),
            "PTT routing must not commit A's draft or leak its rollback into B"
        );
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
            if matches!(item, MenuItem::Spectrum | MenuItem::Shortwave) {
                // EXIT on a receive tool returns straight to the same A/B
                // view after the backend stops and restores reception.
                assert_eq!(ui.view.screen, Screen::Home);
                assert_eq!(ui.context(Slot::A), original);
                continue;
            }
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
        assert_eq!(ui.view.spectrum_cursor, 2);
        ui.clamp_spectrum_cursor(80); // Backend publishes a wider grid.
        for _ in 0..100 {
            press(&mut ui, Key::Up, &bitmap);
        }
        assert_eq!(ui.view.spectrum_cursor, 0);
        for _ in 0..100 {
            press(&mut ui, Key::Down, &bitmap);
        }
        assert_eq!(ui.view.spectrum_cursor, 80);
        let opened = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(opened.request, None);
        assert!(ui.view.spectrum_menu);
        assert_eq!(ui.view.spectrum_option, SPECTRUM_MENU_LISTEN);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::ListenSpectrum { bin: 80 })
        );
        assert_eq!(ui.view.screen, Screen::Spectrum);
        assert_eq!(ui.context(Slot::A), old_a);
        // Listen never commits context; explicit ACCEPT VFO is a second action.
        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.spectrum_option, SPECTRUM_MENU_ACCEPT_VFO);
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::AcceptSpectrum { bin: 80 })
        );
        ui.accept_spectrum_frequency(0);
        assert_eq!(ui.context(Slot::A), old_a);
        ui.accept_spectrum_frequency(43_297_500);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.view.mode, Mode::Vfo);
        assert_eq!(ui.view.vfo_10hz, 43_297_500);
        assert_eq!(ui.context(Slot::B), old_b);
    }

    #[test]
    fn graph_shortcuts_use_existing_requests_and_respect_modal_listen() {
        let bitmap = [0; 128];
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.open_spectrum();
        assert_eq!(ui.spectrum_intervals(), 4);
        assert_eq!(ui.spectrum_step_10hz(), 2_500);
        ui.view.spectrum_span_index = 4;
        ui.view.spectrum_step_index = 4;
        let initial_step = ui.view.spectrum_step_index;
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Digit8), &bitmap).request,
            Some(Request::RestartSpectrum)
        );
        assert_eq!(ui.view.spectrum_span_index, 3);
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Digit1), &bitmap).request,
            Some(Request::RestartSpectrum)
        );
        assert_eq!(ui.view.spectrum_step_index, initial_step + 1);
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Digit6), &bitmap).request,
            Some(Request::ListenSpectrum { bin: 2 })
        );
        assert_eq!(
            ui.handle(InputEvent::Short(Key::Digit7), &bitmap).request,
            None
        );
        assert_eq!(ui.view.spectrum_step_index, initial_step + 1);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Star), &bitmap).request,
            None
        );
        assert_eq!(ui.view.screen, Screen::Spectrum);
        assert_eq!(
            ui.handle(InputEvent::Hold(Key::Hash), &bitmap).request,
            Some(Request::StopSpectrum)
        );
    }

    #[test]
    fn spectrum_menu_applies_span_and_step_only_by_explicit_choice() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        assert_eq!(ui.spectrum_intervals(), 4);
        ui.view.spectrum_span_index = 4;
        ui.view.spectrum_step_index = 4;
        // MENU opens the overlay; EXIT closes it without stopping the sweep.
        press(&mut ui, Key::Menu, &bitmap);
        assert!(ui.view.spectrum_menu);
        let closed = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(closed.request, None);
        assert!(!ui.view.spectrum_menu);
        assert_eq!(ui.view.screen, Screen::Spectrum);

        press(&mut ui, Key::Menu, &bitmap);
        press(&mut ui, Key::Down, &bitmap); // ACCEPT VFO
        press(&mut ui, Key::Down, &bitmap); // span wider, already maximum
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            None,
            "a saturated choice does not restart the sweep"
        );
        assert!(ui.view.spectrum_menu);
        press(&mut ui, Key::Down, &bitmap); // narrower
        let narrower = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(narrower.request, Some(Request::RestartSpectrum));
        assert!(!ui.view.spectrum_menu);
        assert_eq!(ui.spectrum_intervals(), 40);
        assert_eq!(ui.spectrum_step_10hz(), STEP_10HZ[4]);

        press(&mut ui, Key::Menu, &bitmap);
        for _ in 0..SPECTRUM_MENU_STEP_SMALLER {
            press(&mut ui, Key::Down, &bitmap);
        }
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::RestartSpectrum)
        );
        assert_eq!(ui.spectrum_step_10hz(), STEP_10HZ[3]);
        // The VFO step is a separate context and must not follow the sweep.
        assert_eq!(ui.view.step_index, 4);
        assert_eq!(ui.view.screen, Screen::Spectrum);
    }

    #[test]
    fn spectrum_cursor_is_clamped_when_a_shorter_window_is_published() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.spectrum_cursor, 3);
        assert!(ui.clamp_spectrum_cursor(1));
        assert_eq!(ui.view.spectrum_cursor, 1);
        assert!(!ui.clamp_spectrum_cursor(1));
    }

    #[test]
    fn caught_peak_keeps_exact_bin_and_quick_keys_work_during_listen() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        assert!(ui.begin_peak_listen(3));
        assert_eq!(ui.view.spectrum_cursor, 3);
        assert_eq!(ui.view.spectrum_pending_listen, Some(3));
        assert!(!ui.confirm_spectrum_listen(2));
        assert!(ui.confirm_spectrum_listen(3));
        press(&mut ui, Key::Digit5, &bitmap);
        assert_eq!(ui.view.spectrum_trigger, 20);
        assert_eq!(
            press(&mut ui, Key::Star, &bitmap).request,
            Some(Request::AcceptSpectrum { bin: 3 })
        );
        assert_eq!(
            press(&mut ui, Key::Digit0, &bitmap).request,
            Some(Request::StopListenSpectrum)
        );
        assert_eq!(
            press(&mut ui, Key::Hash, &bitmap).request,
            Some(Request::StopListenSpectrum)
        );
        assert!(!ui.view.spectrum_auto_listen);
        assert_eq!(ui.view.spectrum_trigger, 20);
    }

    #[test]
    fn listen_freezes_cursor_until_exit_and_overlay_does_not_stop_it() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        let cursor = ui.view.spectrum_cursor;
        ui.set_spectrum_listening(true);
        press(&mut ui, Key::Up, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        ui.handle(InputEvent::Repeat(Key::Down), &bitmap);
        assert_eq!(ui.view.spectrum_cursor, cursor);

        // Overlay navigation is allowed, closing only the overlay is soft.
        let opened = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(opened.request, None);
        let closed = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(closed.request, None);
        assert!(!closed.cancel_temporary);
        assert_eq!(ui.view.screen, Screen::Spectrum);
        // The next short EXIT stops Listen, not the whole session.
        let stop = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(stop.request, Some(Request::StopListenSpectrum));
        assert!(!stop.cancel_temporary);
        ui.set_spectrum_listening(false);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.view.spectrum_cursor, cursor + 1);
    }

    #[test]
    fn spectrum_listen_short_exit_resumes_and_long_exit_cancels() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        let old = ui.context(Slot::A);
        ui.set_spectrum_listening(true);
        let stop = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(stop.request, Some(Request::StopListenSpectrum));
        assert!(!stop.cancel_temporary);
        assert_eq!(ui.view.screen, Screen::Spectrum);
        assert_eq!(ui.context(Slot::A), old);

        // Long EXIT during listening ends the whole temporary session.
        let cancel = ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        assert!(cancel.cancel_temporary);
        assert_eq!(cancel.request, Some(Request::StopSpectrum));
        assert_eq!(ui.view.screen, Screen::Home);
    }

    #[test]
    fn spectrum_clear_peaks_and_close_are_explicit_menu_requests() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        for _ in 0..SPECTRUM_MENU_CLEAR_PEAKS {
            press(&mut ui, Key::Down, &bitmap);
        }
        assert_eq!(
            press(&mut ui, Key::Menu, &bitmap).request,
            Some(Request::ResetSpectrumPeaks)
        );
        assert!(!ui.view.spectrum_menu);
        press(&mut ui, Key::Menu, &bitmap);
        for _ in 0..SPECTRUM_MENU_CLOSE {
            press(&mut ui, Key::Down, &bitmap);
        }
        let close = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(close.request, Some(Request::StopSpectrum));
        assert_eq!(ui.view.screen, Screen::Home);
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
        ui.handle(InputEvent::Hold(Key::Exit), &bitmap);
        ui.handle(InputEvent::Hold(Key::Digit8), &bitmap);
        assert_eq!(ui.view.screen, Screen::Keys);
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
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(
            SHORTCUT_ACTIONS[ui.view.binding_action_index as usize],
            ShortcutAction::Cb,
        );
        press(&mut ui, Key::Down, &bitmap); // Wrap from CB to None.
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
    fn zone_scan_priority_commits_replaces_shares_and_guards_context() {
        fn set_priority(ui: &mut Controller, bitmap: &[u8; 128], enabled: bool) {
            assert_eq!(
                choose_menu(ui, MenuItem::ScanPriority, bitmap).request,
                None
            );
            assert_eq!(ui.view.setting, Some(Setting::ScanPriority));
            if ui.view.setting_index != u8::from(enabled) {
                press(ui, Key::Down, bitmap);
            }
            assert_eq!(press(ui, Key::Menu, bitmap).request, None);
        }

        let bitmap = catalog(&[2, 4, 8]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let mut zones = [0; 32];
        zones[9 / 8] |= 1 << (9 % 8);
        zones[10 / 8] |= 1 << (10 % 8);
        ui.set_zone_catalog(&zones);
        ui.view.mode = Mode::Channel;
        ui.view.zone = Some(9);
        ui.view.zone_cursor = Some(9);
        ui.view.channel = Some(2);
        ui.set_channel_order(&[2, 4]);

        choose_menu(&mut ui, MenuItem::ScanPriority, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        assert_eq!(ui.scan_zone_priority(&bitmap), None);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.scan_zone_priority(&bitmap), None);

        set_priority(&mut ui, &bitmap, true);
        assert_eq!(ui.view.notice, Notice::ScanPrioritySet);
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(2));
        ui.view.scan = ScanState::Requested;
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(2));
        ui.view.scan = ScanState::Stopped;

        ui.contexts[Slot::B.index()].mode = Mode::Channel;
        ui.contexts[Slot::B.index()].channel = Some(4);
        ui.contexts[Slot::B.index()].zone = Some(9);
        ui.select_slot(Slot::B);
        ui.set_channel_order(&[2, 4]);
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(2));
        set_priority(&mut ui, &bitmap, false);
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(2));
        set_priority(&mut ui, &bitmap, true);
        assert_eq!(ui.view.notice, Notice::ScanPrioritySet);
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(4));

        ui.select_slot(Slot::A);
        ui.set_channel_order(&[2, 4]);
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(4));
        ui.select_slot(Slot::B);
        ui.set_channel_order(&[2, 4]);
        set_priority(&mut ui, &bitmap, false);
        assert_eq!(ui.view.notice, Notice::ScanPriorityCleared);
        assert_eq!(ui.scan_zone_priority(&bitmap), None);

        ui.select_slot(Slot::A);
        ui.contexts[Slot::B.index()].zone = Some(10);
        ui.contexts[Slot::B.index()].channel = Some(2);
        ui.select_slot(Slot::B);
        assert_eq!(ui.view.zone, Some(10));
        ui.set_channel_order(&[2, 4]);
        set_priority(&mut ui, &bitmap, true);
        assert_eq!(ui.scan_zone_priority(&bitmap), Some(2));
        ui.select_slot(Slot::A);
        ui.set_channel_order(&[2, 4]);
        assert_eq!(ui.scan_zone_priority(&bitmap), None);

        for (zone, mode, channel, scan, order) in [
            (
                None,
                Mode::Channel,
                Some(2),
                ScanState::Stopped,
                &[2, 4][..],
            ),
            (
                Some(11),
                Mode::Channel,
                Some(2),
                ScanState::Stopped,
                &[2, 4][..],
            ),
            (Some(9), Mode::Vfo, Some(2), ScanState::Stopped, &[2, 4][..]),
            (
                Some(9),
                Mode::Channel,
                Some(8),
                ScanState::Stopped,
                &[2, 4][..],
            ),
        ] {
            ui.view.zone = zone;
            ui.view.mode = mode;
            ui.view.channel = channel;
            ui.view.scan = scan;
            ui.set_channel_order(order);
            set_priority(&mut ui, &bitmap, true);
            assert_eq!(ui.view.notice, Notice::ScanPriorityUnavailable);
            assert_eq!(ui.scan_zone_priority(&bitmap), None);
        }

        for scan in [
            ScanState::Requested,
            ScanState::Running,
            ScanState::Paused,
            ScanState::Stopping,
        ] {
            ui.view.zone = Some(9);
            ui.view.mode = Mode::Channel;
            ui.view.channel = Some(2);
            ui.view.scan = scan;
            ui.set_channel_order(&[2, 4]);
            let (group, index) = menu_location(MenuItem::ScanPriority);
            ui.view.screen = Screen::Menu;
            ui.view.menu_group = group;
            ui.view.menu_index = index;
            assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
            assert_eq!(ui.view.screen, Screen::Menu);
            assert_eq!(ui.view.setting, None);
            assert_eq!(ui.view.notice, Notice::ScanPriorityStopScan);
            assert_eq!(ui.view.scan, scan);
            ui.select_scan_channel(4, &bitmap);
            ui.acknowledge_scan_stopped();
            assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
            assert_eq!(ui.view.setting, Some(Setting::ScanPriority));
            assert_eq!(ui.view.setting_index, 0);
            assert_eq!(ui.scan_zone_priority(&bitmap), None);
        }
    }

    #[test]
    fn scan_ptt_press_only_stops_and_requires_release_before_a_new_tx_intent() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        ui.acknowledge_scan_started();
        assert!(ui.set_scan_paused(true));
        let change = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert_eq!(change.request, Some(Request::StopScan));
        assert!(!change.ptt_tx_requested);
        assert_eq!(ui.view.ptt_slot, None);
        assert!(!ui.set_scan_paused(true));
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
    fn short_star_requests_scan_skip_only_on_paused_home_screen() {
        let bitmap = catalog(&[0, 2]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.handle(InputEvent::Hold(Key::Digit3), &bitmap);
        assert!(!ui.set_scan_paused(true));
        assert!(ui.acknowledge_scan_started());
        assert!(ui.set_scan_paused(true));
        let a = ui.context(Slot::A);
        let b = ui.context(Slot::B);
        let change = press(&mut ui, Key::Star, &bitmap);
        assert_eq!(change.request, Some(Request::SkipScanTarget));
        assert!(!change.ptt_tx_requested);
        assert!(!change.slot_changed);
        assert_eq!(ui.view.scan, ScanState::Paused);
        assert_eq!(ui.context(Slot::A), a);
        assert_eq!(ui.context(Slot::B), b);
        assert!(ui.set_scan_paused(false));
        assert_eq!(
            press(&mut ui, Key::Star, &bitmap).request,
            Some(Request::StopScan)
        );
        assert_ne!(ui.view.mode, a.mode);
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
    fn default_ptts_request_their_slots_without_changing_the_selected_context() {
        let bitmap = catalog(&[0]);
        for (selected, down, up) in [
            (Slot::A, InputEvent::PttB(true), InputEvent::PttB(false)),
            (Slot::B, InputEvent::Ptt(true), InputEvent::Ptt(false)),
        ] {
            let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
            ui.select_slot(selected);
            let contexts = [ui.context(Slot::A), ui.context(Slot::B)];
            let pressed = ui.handle(down, &bitmap);
            assert_eq!(pressed.ptt_slot, Some(selected.other()));
            assert!(!pressed.slot_changed);
            assert!(!ui.handle(down, &bitmap).ptt_tx_requested);
            assert!(!ui.handle(up, &bitmap).ptt_tx_requested);
            assert_eq!(ui.view().ptt_slot, None);
            assert_eq!(ui.view().active_slot, selected);
            assert_eq!([ui.context(Slot::A), ui.context(Slot::B)], contexts);
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
    fn language_starts_in_the_build_locale_and_system_menu_has_no_selector() {
        let ui = Controller::new(&catalog(&[0, 2]), DEFAULT_FREQUENCY_10HZ);
        assert_eq!(ui.view().language(), Language::CURRENT);
        use MenuEntry::Item;
        assert_eq!(
            menu_entries(Some(MenuGroup::System)),
            &[
                Item(MenuItem::DisplayLight),
                Item(MenuItem::KeyLight),
                Item(MenuItem::About),
                Item(MenuItem::Dtmf),
                Item(MenuItem::RadioName),
                Item(MenuItem::NameRoger),
                Item(MenuItem::MicrophoneGain),
            ]
        );
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
            (MenuItem::ScanPriority, Setting::ScanPriority),
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
    fn step_page_commits_only_on_menu_and_exit_restores_the_previous_value() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        let original = ui.view.step_index;
        choose_menu(&mut ui, MenuItem::Step, &bitmap);
        assert_eq!(ui.view.screen, Screen::Step);
        press(&mut ui, Key::Down, &bitmap);
        assert_ne!(ui.view.step_index, original);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Menu);
        assert_eq!(ui.view.step_index, original, "EXIT cancels the draft");

        choose_menu(&mut ui, MenuItem::Step, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        let committed = ui.view.step_index;
        assert_ne!(committed, original);
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.view.step_index, committed);

        // The physical # shortcut also edits a draft; EXIT goes Home.
        press(&mut ui, Key::Hash, &bitmap);
        assert_eq!(ui.view.screen, Screen::Step);
        press(&mut ui, Key::Up, &bitmap);
        assert_ne!(ui.view.step_index, committed);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.view.step_index, committed);
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

    #[test]
    fn codex_same_slot_ptt_keeps_step_cancel_transaction_valid() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.step_index = 1;
        let original = ui.context(Slot::A).step_index;
        assert!(ui.set_side_binding(0, SideAction::PttA));
        press(&mut ui, Key::Hash, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        let update = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert!(update.ptt_tx_requested);
        assert_eq!(update.ptt_slot, Some(Slot::A));
        assert_eq!(ui.view.screen, Screen::Home);
        ui.handle(InputEvent::Ptt(false), &bitmap);
        press(&mut ui, Key::Hash, &bitmap);
        assert_eq!(ui.view.screen, Screen::Step);
        press(&mut ui, Key::Down, &bitmap);
        press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(
            ui.context(Slot::A).step_index,
            original,
            "same-slot PTT must not leave a Step editor without rollback"
        );
    }

    #[test]
    fn step_route_cancellation_covers_same_and_cross_slot_paths() {
        for active in [Slot::A, Slot::B] {
            for target in [Slot::A, Slot::B] {
                let bitmap = catalog(&[0]);
                let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
                ui.select_slot(active);
                ui.view.step_index = if active == Slot::A { 1 } else { 4 };
                let before = ui.context(active).step_index;
                let other = ui.context(active.other()).step_index;
                press(&mut ui, Key::Hash, &bitmap);
                press(&mut ui, Key::Down, &bitmap);
                ui.select_slot(target);
                assert_eq!(ui.view.screen, Screen::Home);
                assert_eq!(ui.context(active).step_index, before);
                assert_eq!(ui.context(active.other()).step_index, other);
                // Re-entry must start a fresh transaction with rollback.
                press(&mut ui, Key::Hash, &bitmap);
                press(&mut ui, Key::Down, &bitmap);
                press(&mut ui, Key::Exit, &bitmap);
                assert_eq!(ui.context(active).step_index, before);
            }
        }
    }

    #[test]
    fn codex_pending_listen_keeps_confirmed_rf_bin_visible() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        assert!(ui.view.spectrum_menu);
        assert_eq!(ui.view.spectrum_option, SPECTRUM_MENU_LISTEN);
        let update = press(&mut ui, Key::Menu, &bitmap);
        let Some(Request::ListenSpectrum { bin }) = update.request else {
            panic!("Listen must request the current bin");
        };
        assert_eq!(ui.view.spectrum_pending_listen, Some(bin));
        assert!(!ui.view.spectrum_listening);
        // RF restore pending; foreground arrows/repeat must not move the bin.
        press(&mut ui, Key::Down, &bitmap);
        ui.handle(InputEvent::Repeat(Key::Up), &bitmap);
        assert_eq!(ui.view.spectrum_cursor, bin);
        // Main's confirmation callback after the exact restore ACK.
        ui.confirm_spectrum_listen(bin);
        assert_eq!(ui.view.spectrum_cursor, bin);
        assert!(ui.view.spectrum_listening);
        assert_eq!(ui.view.spectrum_pending_listen, None);
    }

    #[test]
    fn confirmed_listen_rebinds_a_cursor_that_moved_during_pending_restore() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        let update = press(&mut ui, Key::Menu, &bitmap);
        let Some(Request::ListenSpectrum { bin }) = update.request else {
            panic!("Listen");
        };
        // A stale frame/other caller could still place the cursor elsewhere.
        ui.place_spectrum_cursor(bin + 1);
        ui.confirm_spectrum_listen(bin);
        assert_eq!(ui.view.spectrum_cursor, bin);
        assert!(ui.view.spectrum_listening);
    }

    #[test]
    fn pending_listen_duplicate_request_keeps_the_original_confirmation_bin() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        let first = press(&mut ui, Key::Menu, &bitmap);
        let Some(Request::ListenSpectrum { bin }) = first.request else {
            panic!("Listen");
        };
        press(&mut ui, Key::Menu, &bitmap);
        let second = press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(second.request, Some(Request::ListenSpectrum { bin }));
        assert_eq!(ui.view.spectrum_pending_listen, Some(bin));
        // The later request is refused in main; the original ACK still binds.
        ui.confirm_spectrum_listen(bin);
        assert!(ui.view.spectrum_listening);
        assert_eq!(ui.view.spectrum_cursor, bin);
    }

    #[test]
    fn pending_listen_cancel_clears_freeze_without_listening() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        let update = press(&mut ui, Key::Menu, &bitmap);
        assert!(matches!(
            update.request,
            Some(Request::ListenSpectrum { .. })
        ));
        assert!(ui.view.spectrum_pending_listen.is_some());
        // Overlay is allowed while pending; EXIT closes only the overlay.
        assert_eq!(press(&mut ui, Key::Menu, &bitmap).request, None);
        assert_eq!(press(&mut ui, Key::Exit, &bitmap).request, None);
        assert!(ui.view.spectrum_pending_listen.is_some());
        let exit = press(&mut ui, Key::Exit, &bitmap);
        assert_eq!(exit.request, Some(Request::StopSpectrum));
        assert_eq!(ui.view.spectrum_pending_listen, None);
        assert!(!ui.view.spectrum_listening);
        // Late ACK must not start Listen or move the confirmed VFO.
        ui.confirm_spectrum_listen(40);
        assert!(!ui.view.spectrum_listening);
    }

    #[test]
    fn pending_listen_ptt_cancels_and_late_ack_is_ignored() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        choose_menu(&mut ui, MenuItem::Spectrum, &bitmap);
        press(&mut ui, Key::Menu, &bitmap);
        let update = press(&mut ui, Key::Menu, &bitmap);
        assert!(matches!(
            update.request,
            Some(Request::ListenSpectrum { .. })
        ));
        let ptt = ui.handle(InputEvent::Ptt(true), &bitmap);
        assert!(ptt.cancel_temporary);
        assert_eq!(ptt.request, Some(Request::StopSpectrum));
        assert_eq!(ui.view.spectrum_pending_listen, None);
        assert!(!ui.view.spectrum_listening);
        ui.confirm_spectrum_listen(40);
        assert!(!ui.view.spectrum_listening);
    }

    #[test]
    fn step_menu_is_the_only_commit_after_slot_route() {
        let bitmap = catalog(&[0]);
        let mut ui = Controller::new(&bitmap, DEFAULT_FREQUENCY_10HZ);
        ui.view.step_index = 1;
        let original = ui.context(Slot::A).step_index;
        press(&mut ui, Key::Hash, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        ui.select_slot(Slot::A);
        press(&mut ui, Key::Hash, &bitmap);
        press(&mut ui, Key::Down, &bitmap);
        let committed = ui.view.step_index;
        press(&mut ui, Key::Menu, &bitmap);
        assert_eq!(ui.view.screen, Screen::Home);
        assert_eq!(ui.context(Slot::A).step_index, committed);
        assert_ne!(committed, original);
    }
}

pub fn vfo_tx_frequency(view: &View) -> Option<u32> {
    vfo_tx_frequency_for(
        view.vfo_10hz,
        view.shift_index,
        view.cb_for_slot(view.active_slot),
    )
}

#[inline(never)]
pub fn vfo_tx_frequency_for(
    frequency_10hz: u32,
    shift_index: u8,
    cb: Option<CbChannel>,
) -> Option<u32> {
    if let Some(cb) = cb {
        return Some(cb.tx_frequency_10hz());
    }
    frequency_10hz.checked_add_signed(*SHIFT_10HZ.get(usize::from(shift_index))?)
}
