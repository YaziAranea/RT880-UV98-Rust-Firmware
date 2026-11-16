#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
//! Experimental executable for the related RT880/UV98 AT32F423 profile.
//! RX by default. Explicit features select the historical EXO trial or normal
//! FM channel/VFO TX. NOR remains read-only; the channel-TX profile supports
//! both physical PTTs using the investigated board's isolated key ladder.
//! A successful register check does not establish physical board/RF safety.

mod beacon;
#[path = "shortwave.rs"]
mod shortwave_bus;
mod signaling;
use at32f4xx_pac::at32f423::interrupt;

#[at32f4xx_pac::at32f423::interrupt]
fn USART2() {
    unsafe {
        uv98_gps_at32::on_interrupt();
    }
}

use uv98_radio_runtime::shortwave;

use core::{fmt::Debug, num::NonZeroU32, panic::PanicInfo};
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
#[cfg(feature = "verbose-faults")]
use uv98_boot_log::error_text::ErrorText;
use uv98_boot_log::hardware::{RawMmio, TxLogger};
use uv98_boot_log::{Frame, Logger, RegisterIo};
use uv98_clock_profile::hardware::CycleDelay;
use uv98_diag_startup::{hardware as startup, CycleMillis, CORE_NOMINAL_HZ, CORE_UPPER_HZ};
use uv98_display::DisplayBus;
use uv98_handheld_app::{App, InputSample, SpectrumSnapshot, PRESERVED_CALIBRATION_SHA256};
use uv98_handheld_ui::{
    controller::{
        CrossbandProbe, Request, ScanState, Screen, ShortwaveStatus, Slot, View,
        SPECTRUM_INTERVALS, STEP_10HZ,
    },
    render,
    render::{Modulation as UiModulation, RadioRoute, RadioState, RadioView, SpectrumState},
};
use uv98_nor_read::Bus;
use uv98_radio_runtime::{
    crossband, lighting, policy, ptt, scan, scope, spectrum_flow, tx_policy, tx_trial, Runtime,
    State as RfState,
};
use uv98_rf_sequence::Modulation;
use uv98_spectrum::compact as spectrum;

// Default remains RX-only; each published image declares one exact profile.
#[cfg(all(feature = "upper-exo-tx-trial", feature = "channel-tx"))]
compile_error!("Select exactly one TX profile");
const LIVE_TX: bool = cfg!(any(feature = "upper-exo-tx-trial", feature = "channel-tx"));
const LIVE_RELAY: bool = cfg!(feature = "channel-tx");
const LOWER_PTT_ENABLED: bool = cfg!(feature = "channel-tx");
// Keep the same first 32 UI events as the radio logger's per-boot quota.
const UI_LOG_LIMIT: usize = 32;
const LCD_BATCH_ADMISSION_CYCLES: u32 = CORE_NOMINAL_HZ / 500;

/// CPS may personalize only this digest, after verifying two identical stock
/// NOR reads. Calibration itself stays in the radio and is never overwritten.
/// Volatile access is intentional: a personalized BIN must use the bytes in
/// flash, not a compiler-folded copy of the original owner's expected digest.
#[used]
#[no_mangle]
pub static UV98_CALIBRATION_PROFILE: [u8; 48] = {
    let mut profile = [0; 48];
    let magic = *b"UV98CALPROFILE1\0";
    let mut index = 0;
    while index < 16 {
        profile[index] = magic[index];
        index += 1;
    }
    index = 0;
    while index < 32 {
        profile[16 + index] = PRESERVED_CALIBRATION_SHA256[index];
        index += 1;
    }
    profile
};

#[inline(never)]
fn expected_calibration_digest() -> [u8; 32] {
    // SAFETY: the immutable 48-byte object is allocated in readable application
    // flash; offset16..48 is exactly one byte-aligned [u8;32] array.
    unsafe {
        core::ptr::read_volatile(
            core::ptr::addr_of!(UV98_CALIBRATION_PROFILE)
                .cast::<u8>()
                .add(16)
                .cast::<[u8; 32]>(),
        )
    }
}

/// What the operator asked the spectrum session to become after the sweep
/// owner finishes its current restore. The allowed follow-up is decided by the
/// tested `spectrum_flow` policy, not by the restore reason alone.
use uv98_radio_runtime::spectrum_flow::{Action as SpectrumAction, Intent as SpectrumIntent};

fn spectrum_error(error: scope::StartError) -> render::SpectrumError {
    use render::SpectrumError as Ui;
    use scope::StartError as E;
    match error {
        E::NotReceiving | E::Receive(_) => Ui::Receiver,
        E::Busy | E::Protocol(spectrum::StartError::Busy) => Ui::Busy,
        E::Window(_) | E::Grid | E::CenterMismatch => Ui::Window,
        E::Qualification(_) => Ui::Route,
        E::Protocol(_) => Ui::Receiver,
    }
}

#[used]
#[no_mangle]
pub static UV98_RELEASE_POLICY: [u32; 4] = [
    0x108,
    LIVE_TX as u32,
    if LOWER_PTT_ENABLED { 0x16 } else { 0xffff_ffff },
    if cfg!(feature = "channel-tx") {
        0x464d_5431
    } else if LIVE_TX {
        0x4558_4f31
    } else {
        0x5258_4331
    },
];

// Optional debugger observation, NOT a requirement for ordinary cable updates.
// Written only after .bss initialization. Early ASM never touches this word.
#[no_mangle]
pub static mut UV98_DIAG_STATUS: u32 = 0;

fn status(code: u32) {
    // SAFETY: foreground has masked interrupts; terminal exception writes never
    // return to foreground. No mutable reference or concurrent logger exists.
    unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(UV98_DIAG_STATUS), code) };
}

fn stop(code: u32) -> ! {
    status(code);
    // SAFETY: all live owners are abandoned; this path never resumes them.
    // Low requests precede a bounded, best-effort error transmission.
    let _ = unsafe { startup::terminal_radio_off_requests() };
    let mut log = Logger::new(unsafe { RawMmio::new() });
    let _ = log.configure_for_current_clock();
    let _ = log.emit(Frame::new(code, [0; 3]));
    // SAFETY: terminal path abandons all owned drivers and never resumes them.
    unsafe { startup::terminal_fail_stop() }
}

trait Events {
    fn record(&mut self, frame: Frame);
}
impl<I: RegisterIo> Events for Logger<I> {
    fn record(&mut self, frame: Frame) {
        let _ = self.emit(frame);
    }
}
impl Events for TxLogger {
    fn record(&mut self, frame: Frame) {
        let _ = self.emit(frame);
    }
}
fn event(log: &mut impl Events, code: u32, details: [u32; 3]) {
    status(code);
    log.record(Frame::new(code, details));
}
fn checked<T, E: Debug>(log: &mut impl Events, result: Result<T, E>, fault: u32) -> T {
    match result {
        Ok(value) => value,
        Err(_error) => {
            // SAFETY: terminal takeover; owned peripherals never resume.
            let _ = unsafe { startup::terminal_radio_off_requests() };
            #[cfg(feature = "verbose-faults")]
            {
                let text = ErrorText::format(&_error);
                event(log, fault, text.metadata());
                for frame in text.frames() {
                    log.record(frame);
                }
            }
            #[cfg(not(feature = "verbose-faults"))]
            event(log, fault, [0; 3]);
            unsafe { startup::terminal_fail_stop() }
        }
    }
}

fn require<T, E>(result: Result<T, E>, fault: u32) -> T {
    match result {
        Ok(value) => value,
        Err(_) => stop(fault),
    }
}

fn feed() {
    // SAFETY: used only after successful startup in one masked-IRQ context;
    // no other owner can reconfigure either watchdog. Reload never starts it.
    require(unsafe { startup::feed_watchdog() }, 0xE001);
}

struct Timing(CycleDelay);
impl Timing {
    fn new() -> Self {
        // SAFETY: startup selected and checked nominal 48 MHz. The 50 MHz upper
        // bound remains an explicit, unmeasured reference-board assumption.
        Self(unsafe { CycleDelay::from_core_upper_bound(nz(CORE_UPPER_HZ)) })
    }
    fn us(&mut self, us: u32) {
        feed();
        self.0.wait_us(us);
    }
}
impl uv98_display::Delay for Timing {
    fn delay_ms(&mut self, ms: u32) {
        for _ in 0..ms {
            self.us(1000);
        }
    }
}
impl uv98_lcd_at32::BusTiming for Timing {
    fn wait(&mut self, _: uv98_lcd_at32::TimingPhase) {
        // Serviced feeds once before each bounded LCD transaction. Keep the
        // same minimum CS/DC timing without rereading watchdog state at each
        // of the three bus phases.
        self.0.wait_us(1);
    }
}
impl uv98_keyscan_at32::ScanTiming for Timing {
    fn settle(&mut self, _: uv98_keyscan_at32::SettlePhase) {
        self.us(100);
    }
}
impl uv98_rf_bus::Delay for Timing {
    // Experimental minimum 10 us; stock's delay(10) is NOT proven to be 10 us.
    fn wait(&mut self, _: uv98_rf_bus::Phase) {
        // Keep the same delay cycles, but do not perform watchdog register
        // checks at every bit edge. Transactions are bounded (at most 49
        // waits); the foreground feeds between bounded operations. A stuck
        // transaction must remain visible to the independent watchdog.
        self.0.wait_us(10);
    }
}

impl uv98_sidekeys_at32::PollTiming for Timing {
    fn wait_poll(&mut self) -> Result<(), uv98_sidekeys_at32::Error> {
        self.us(1);
        Ok(())
    }
}

fn tx_permitted<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &App<N, L, D>,
    target: View,
    profile: policy::Profile,
    input: ptt::Input,
) -> bool {
    if cfg!(feature = "channel-tx") {
        tx_policy::authorize_target(&app.status().view, &target, profile, input).is_ok()
    } else {
        tx_trial::permits(app.status().view, profile, input)
            && app
                .snapshot()
                .is_some_and(|s| s.index() == 1 && tx_trial::record_permitted(s.channel))
    }
}

fn tx_hold_within_limit(start: u32, now: u32) -> bool {
    if cfg!(feature = "channel-tx") {
        tx_policy::hold_within_limit(start, now)
    } else {
        tx_trial::hold_within_limit(start, now)
    }
}

fn radio_settings<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &App<N, L, D>,
) -> Option<policy::Settings> {
    app.radio_config().map(|cfg| policy::Settings {
        squelch: cfg.squelch,
        mic_gain: cfg.mic_gain,
        fm_volume: cfg.fm_volume,
        am_volume: cfg.am_volume,
        squelch_delay: cfg.squelch_delay,
    })
}

fn profile<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &App<N, L, D>,
    settings: &[policy::Overrides; 2],
) -> Result<policy::Profile, policy::Error> {
    let cfg = radio_settings(app).ok_or(policy::Error::Settings)?;
    let view = app.status().view;
    let index = if view.active_slot == Slot::A { 0 } else { 1 };
    policy::profile(view, app.selected_channel(), cfg, settings[index])
}

/// Project the requested slot without changing the editing/RX selection.
/// The peer's immutable record and RAM settings remain distinct from the
/// selected channel, including RX-only permission, duplex tone and power.
fn profile_for_slot<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &App<N, L, D>,
    settings: &[policy::Overrides; 2],
    slot: Slot,
) -> Result<(View, policy::Profile), policy::Error> {
    let operator = app.status().view;
    let (target, channel) = if operator.active_slot == slot {
        (operator, app.selected_channel())
    } else {
        app.peer_radio_selection()
    };
    let cfg = radio_settings(app).ok_or(policy::Error::Settings)?;
    let profile = policy::profile(target, channel, cfg, settings[usize::from(slot == Slot::B)])?;
    Ok((target, profile))
}

fn crossband_session<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &App<N, L, D>,
    settings: &[policy::Overrides; 2],
    incoming: policy::Profile,
) -> Option<crossband::Session> {
    let (view, channel) = app.peer_radio_selection();
    let index = if view.active_slot == Slot::A { 0 } else { 1 };
    let outgoing = policy::profile(view, channel, radio_settings(app)?, settings[index]).ok()?;
    if view.active_slot == Slot::B {
        crossband::Session::arm(incoming, outgoing, Slot::A)
    } else {
        crossband::Session::arm(outgoing, incoming, Slot::B)
    }
}

fn relay_carrier(signal: uv98_radio_runtime::Signal) -> crossband::Carrier {
    if signal.audio_open {
        crossband::Carrier::Open
    } else if signal.squelch_open {
        crossband::Carrier::PendingTone
    } else {
        crossband::Carrier::Closed
    }
}

fn crossband_status(session: &crossband::Session, relaying: bool) -> CrossbandProbe {
    CrossbandProbe::Ready {
        a_rx: session.pair(Slot::A).receive().frequency_10hz,
        b_rx: session.pair(Slot::B).receive().frequency_10hz,
        traffic: relaying.then(|| {
            let slot = session.slot();
            let pair = session.pair(slot);
            (
                slot,
                pair.receive().frequency_10hz,
                pair.transmit().frequency_10hz,
            )
        }),
        live: LIVE_RELAY,
    }
}

fn probe_secondary_frequency<B: uv98_diag_quiesce::RegisterIo>(
    bus: &mut B,
    words: [u16; 2],
) -> Result<bool, B::Error> {
    if bus.profile() != uv98_rf_bus::Profile::B {
        return Ok(false);
    }
    for (address, mask, expected) in [(0x33, 0x001f, 0), (0x36, 0xffff, 0x007f), (0x30, 0xffff, 0)]
    {
        if bus.read(address)? & mask != expected {
            return Ok(false);
        }
    }
    bus.write(0x38, words[0])?;
    bus.write(0x39, words[1])?;
    Ok(bus.read(0x38)? == words[0] && bus.read(0x39)? == words[1])
}

fn write_relay_register<B: uv98_diag_quiesce::RegisterIo>(
    bus: &mut B,
    address: u8,
    value: u16,
) -> Result<bool, B::Error> {
    bus.write(address, value)?;
    if matches!(address, 0x30 | 0x33 | 0x36 | 0x38 | 0x39) {
        return Ok(bus.read(address)? == value);
    }
    Ok(true)
}

fn apply_secondary_plan<B: uv98_diag_quiesce::RegisterIo>(
    bus: &mut B,
    plan: &uv98_rf_sequence::Plan,
) -> Result<bool, B::Error> {
    use uv98_rf_sequence::Step;
    for &step in plan.operations() {
        let (register, value) = match step {
            Step::WriteA { register, value } => (register, value),
            Step::UpdateA {
                register,
                keep_mask,
                value,
            } => (register, (bus.read(register)? & keep_mask) | value),
            Step::DelayMs(ms) => {
                uv98_display::Delay::delay_ms(&mut Timing::new(), u32::from(ms));
                continue;
            }
            _ => return Ok(false),
        };
        if !write_relay_register(bus, register, value)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn stop_relay<A: uv98_diag_quiesce::RegisterIo, B: uv98_diag_quiesce::RegisterIo>(
    radio: &mut Runtime<uv98_radio_runtime::hardware::RadioIo<A>>,
    secondary: &mut B,
) {
    // Drop the external V/U gate before touching any B register. If either
    // bus fails, the independent terminal writer still attempts both chips.
    if radio.relay_route(None).is_err() {
        stop(0xE065);
    }
    stop_secondary(secondary);
}

fn stop_secondary<B: uv98_diag_quiesce::RegisterIo>(secondary: &mut B) {
    for (reg, value) in [(0x33, 0), (0x36, 0x007f), (0x30, 0), (0x37, 0x1d00)] {
        if !matches!(write_relay_register(secondary, reg, value), Ok(true)) {
            stop(0xE065);
        }
    }
}

/// Build the volatile spectrum window from the UI's independent span/step.
/// The window is always centred on the confirmed receive frequency; refused
/// combinations never reach the protocol engine.
fn spectrum_window(
    view: uv98_handheld_ui::controller::View,
    center_10hz: u32,
) -> Option<uv98_spectrum::Window> {
    let step = STEP_10HZ
        .get(usize::from(view.spectrum_step_index))
        .copied()?;
    let intervals = u32::from(*SPECTRUM_INTERVALS.get(usize::from(view.spectrum_span_index))?);
    let span = step.checked_mul(intervals)?;
    uv98_spectrum::Window::centered(center_10hz, span, step).ok()
}

fn scan_action<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &mut App<N, L, D>,
    action: scan::Action,
) -> Result<bool, uv98_handheld_app::Error<N::Error, L::Error>> {
    match action {
        scan::Action::NextChannel => {
            if let Some(index) = app.next_catalog_index(app.status().view.channel, true) {
                app.select_scan_channel(index)?;
                Ok(true)
            } else {
                app.reject_radio_request(Request::StartScan)?;
                Ok(false)
            }
        }
        scan::Action::TuneFrequency(frequency) => {
            app.select_scan_frequency(frequency)?;
            Ok(true)
        }
        scan::Action::Stop => {
            app.acknowledge_scan_stopped()?;
            Ok(false)
        }
        scan::Action::None => Ok(false),
    }
}

// Keep inherited supported watchdogs fed during long foreground screen/read
// operations. These wrappers add no protocol commands and retain bus errors.
struct Serviced<T>(T);
impl<T: Bus> Bus for Serviced<T> {
    type Error = T::Error;
    fn select(&mut self) -> Result<(), Self::Error> {
        feed();
        self.0.select()
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        feed();
        self.0.write(bytes)
    }
    fn read(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        for chunk in destination.chunks_mut(64) {
            feed();
            self.0.read(chunk)?;
        }
        Ok(())
    }
    fn deselect(&mut self) -> Result<(), Self::Error> {
        feed();
        self.0.deselect()
    }
}
impl<T: DisplayBus> DisplayBus for Serviced<T> {
    type Error = T::Error;
    fn reset(&mut self, high: bool) -> Result<(), Self::Error> {
        feed();
        self.0.reset(high)
    }
    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        feed();
        self.0.command(command)
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        feed();
        self.0.data(bytes)
    }
    fn pixels_rgb565(&mut self, pixels: &[u16]) -> Result<(), Self::Error> {
        feed();
        self.0.pixels_rgb565(pixels)
    }
}

fn nz(value: u32) -> NonZeroU32 {
    match NonZeroU32::new(value) {
        Some(v) => v,
        None => stop(0xE002),
    }
}

#[entry]
fn main() -> ! {
    // SAFETY: startup crate provides the assembly-only __pre_init hook before
    // RAM initialization. Repeat and check takeover now that Rust RAM is valid;
    // no PAC owner/driver exists yet, and interrupts remain masked.
    {
        // SAFETY: no PAC owners exist; IRQs masked. No retained MMIO reference.
        // Each frame drains TDC; no transmission spans the clock transition.
        let mut log = Logger::new(unsafe { RawMmio::new() });
        let _ = log.configure_for_current_clock();
        event(&mut log, 0x100, [0x0000_0108, 0, 0]);
        checked(&mut log, unsafe { startup::early_quiesce() }, 0xE00F);
        event(&mut log, 0x101, [0; 3]);
        event(&mut log, 0x102, [0; 3]);
        let clocks = unsafe { startup::initialize() };
        // Even a partial transition can change APB2. Re-derive before logging.
        let _ = log.configure_for_current_clock();
        checked(&mut log, clocks, 0xE010);
        event(&mut log, 0x103, [CORE_NOMINAL_HZ, 24_000_000, 48_000_000]);
    }
    let mut p = match at32f4xx_pac::at32f423::Peripherals::take() {
        Some(p) => p,
        None => stop(0xE011),
    };
    let mut core = match cortex_m::Peripherals::take() {
        Some(p) => p,
        None => stop(0xE012),
    };
    core.DCB.enable_trace();
    if !DWT::has_cycle_counter() {
        stop(0xE013);
    }
    core.DWT.set_cycle_count(0);
    core.DWT.enable_cycle_counter();
    let start = DWT::cycle_count();
    cortex_m::asm::delay(1000);
    if !DWT::cycle_counter_enabled() || DWT::cycle_count() == start {
        stop(0xE014);
    }
    let mut time = require(
        CycleMillis::new(DWT::cycle_count(), nz(CORE_NOMINAL_HZ)),
        0xE015,
    );

    let pins = uv98_board_pins::BoardPins::split(p.GPIOA, p.GPIOB, p.GPIOC, p.GPIOD);
    let rf = pins.reserved.with_rf(p.GPIOF);
    let diagnostic = rf.reserved.with_diagnostic(rf.control);
    let cable = diagnostic.reserved.with_boot_log();
    // SAFETY: pre-init qualified the family; clocks now stable, IRQs masked,
    // PA11/USART6 exclusively transferred from the temporary raw logger.
    let mut log = require(TxLogger::new_radio(p.USART6, cable.tx, 48_000_000), 0xE040);
    event(&mut log, 0x104, [DWT::cycle_count(), 0, 0]);

    // SAFETY: this experimental image explicitly selects the related reference
    // pin map. Startup checked MCU identity, stopped DMA/IRQs and applied low
    // requests. PCB/electrical compatibility is NOT inferred from those checks.
    // Ownership is disjoint and shared register RMW is serialized throughout.
    let mut control = unsafe {
        uv98_diag_quiesce::hardware::Control::from_early_low_requests(
            diagnostic.control,
            &mut p.CRM,
        )
    };
    let mut primary = unsafe {
        uv98_rf_at32::hardware::PrimaryPins::from_diagnostic_takeover(rf.primary, &mut p.CRM)
    }
    .into_bus(Timing::new());
    let mut secondary = unsafe {
        uv98_rf_at32::hardware::SecondaryPins::from_diagnostic_takeover(rf.secondary, &mut p.CRM)
    }
    .into_bus(Timing::new());
    event(&mut log, 0x110, [0; 3]);
    let mut shutdown = checked(
        &mut log,
        uv98_diag_quiesce::Shutdown::new(&mut primary, &mut secondary, &mut control),
        0xE020,
    );
    checked(&mut log, shutdown.run_once(), 0xE021);
    event(&mut log, 0x111, [0; 3]);

    // SAFETY: startup deasserted CS/released matrix, stopped inherited masters,
    // and selected the checked 48/24/48 MHz profile. Reference wiring/mode/strap
    // and timing remain experimental hardware assumptions, stated in the README.
    let nor = unsafe {
        uv98_board_at32::hardware::Spi2Nor::from_quiescent(
            p.SPI2,
            pins.nor,
            &mut p.CRM,
            uv98_board_at32::Config {
                // Diagnostic timing experiment: nominal APB1 24 MHz / 8.
                divider: uv98_board_at32::Divider::Div8,
                mode: uv98_board_at32::Mode::Mode0,
                poll_budget: nz(10_000),
            },
        )
    };
    let lcd = unsafe {
        uv98_lcd_at32::hardware::Spi3Lcd::from_quiescent(
            p.SPI3,
            pins.lcd,
            &mut p.CRM,
            uv98_lcd_at32::Config {
                // Stock SPI3 uses /2 on the nominal 24MHz APB1: 12MHz SCK.
                // Keep the on-radio working Mode0 and NOR's verified3MHz.
                // Physical LCD timing at this restored divider needs testing.
                divider: uv98_lcd_at32::Divider::Div2,
                mode: uv98_lcd_at32::Mode::Mode0,
                poll_budget: nz(10_000),
            },
            Timing::new(),
        )
    };
    let mut keys = unsafe {
        uv98_keyscan_at32::hardware::KeyScanner::from_quiescent(
            pins.keyscan,
            &mut p.CRM,
            Timing::new(),
        )
    };
    // Read-only observations after all pin constructors, while every driver is
    // idle and IRQs/DMA remain disabled. No raw register reference is retained.
    // The STS read is diagnostic only; no DT/receive register is touched here.
    let (spi2_registers, gpiob_registers, gpiob_levels) = unsafe {
        (
            [
                core::ptr::read_volatile(0x4000_3800 as *const u32),
                core::ptr::read_volatile(0x4000_3804 as *const u32),
                core::ptr::read_volatile(0x4000_3808 as *const u32),
            ],
            [
                core::ptr::read_volatile(0x4002_0400 as *const u32),
                core::ptr::read_volatile(0x4002_0408 as *const u32),
                core::ptr::read_volatile(0x4002_0424 as *const u32),
            ],
            [
                core::ptr::read_volatile(0x4002_0410 as *const u32),
                core::ptr::read_volatile(0x4002_0414 as *const u32),
                core::ptr::read_volatile(0x4002_040c as *const u32),
            ],
        )
    };
    event(&mut log, 0x152, spi2_registers);
    event(&mut log, 0x153, gpiob_registers);
    event(&mut log, 0x154, gpiob_levels);
    // SAFETY: fixed read-only SPI3/GPIO samples after construction. Drivers
    // are idle in this foreground context; neither RX data nor latches change.
    let (spi3_registers, lcd_pin_levels) = unsafe {
        (
            [
                core::ptr::read_volatile(0x4000_3c00 as *const u32),
                core::ptr::read_volatile(0x4000_3c04 as *const u32),
                core::ptr::read_volatile(0x4000_3c08 as *const u32),
            ],
            [
                core::ptr::read_volatile(0x4002_0410 as *const u32),
                core::ptr::read_volatile(0x4002_0010 as *const u32),
                core::ptr::read_volatile(0x4002_0810 as *const u32),
            ],
        )
    };
    event(&mut log, 0x160, spi3_registers);
    event(&mut log, 0x161, lcd_pin_levels);
    let mut nor = Serviced(nor);
    let mut prefix = [0u8; 24];
    checked(
        &mut log,
        uv98_nor_read::read(&mut nor, 0, &mut prefix),
        0xE033,
    );
    for (index, bytes) in prefix.chunks_exact(12).enumerate() {
        let words = [
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        ];
        event(&mut log, 0x150 + index as u32, words);
    }
    let mut app = App::new(
        nor,
        Serviced(lcd),
        Timing::new(),
        require(uv98_keys::Timing::new(30, 600), 0xE030),
        expected_calibration_digest(),
    );
    event(&mut log, 0x120, [0; 3]);
    event(&mut log, 0x121, [0; 3]);
    let started = app.start_observed(0, |stage| {
        use uv98_handheld_app::StartStage::*;
        let code = match stage {
            DisplayBegin => 0x140,
            CalibrationBegin => 0x141,
            ChannelBegin => 0x142,
            RenderBegin => 0x143,
            Complete => 0x144,
        };
        event(&mut log, code, [0; 3]);
    });
    checked(&mut log, started, 0xE031);
    // UI must distinguish an editable lower-key assignment from a physically
    // identified input source. The constant stays None until the board trace
    // proves which ADC code belongs to that key.
    checked(
        &mut log,
        app.set_lower_side_mapped(LOWER_PTT_ENABLED),
        0xE032,
    );
    event(
        &mut log,
        0x122,
        [
            app.status()
                .calibration_matches_backup
                .map(u32::from)
                .unwrap_or(2),
            0,
            0,
        ],
    );
    event(&mut log, 0x123, [0; 3]);
    checked(&mut log, shutdown.verify_off(), 0xE022);
    // Drop the diagnostic owner while all paths are off, then transfer every
    // GPIO token into the operational driver. Chip B remains asleep throughout.
    drop(shutdown);
    if app.status().calibration_matches_backup != Some(true) {
        stop(0xE050);
    }
    let radio_pins = cable.reserved.with_radio(control.into_pins());
    let radio_control = checked(
        &mut log,
        unsafe {
            uv98_radio_runtime::hardware::Control::from_shutdown(radio_pins.control, &mut p.CRM)
        },
        0xE051,
    );
    let (tuner_pins, reserved) = radio_pins.reserved.with_shortwave();
    let (gps_pins, modem_pins, _reserved) = reserved.with_auxiliary();
    let mut gps = unsafe { uv98_gps_at32::Gps::new(gps_pins) };
    let mut aprs_raw = [0xffu8; 198];
    let aprs_config = if app.read_auxiliary_config(3120, &mut aprs_raw) {
        uv98_signaling::aprs::Config::from_stock_tail(&aprs_raw).ok()
    } else {
        None
    };
    if let Some(config) = aprs_config {
        app.set_aprs_identity(*config.source.callsign(), config.source.ssid());
    }
    let mut beacon = unsafe { beacon::Beacon::new(modem_pins, aprs_config, aprs_raw[193].min(7)) };
    let mut gps_configuration = (false, 1u8);
    let mut gps_ui_at = 0u32;
    // Preinit disabled and cleared every NVIC line. Only the installed USART2
    // handler may be enabled by Gps::configure; it owns a separate IRQ ring.
    unsafe {
        cortex_m::interrupt::enable();
    }
    let mut tuner_bus = shortwave_bus::Bus::new(p.I2C1, tuner_pins, &mut p.CRM);
    let mut tuner_session: Option<shortwave::Session> = None;
    let mut tuner_mode_switch = shortwave::ModeSwitch::default();
    let mut tuner_sweep = uv98_radio_runtime::shortwave_scope::Sweep::default();
    let mut tuner_failed = None;
    let mut tuner_function = 0;
    let mut tuner_repower_at: Option<u32> = None;
    let mut side = checked(
        &mut log,
        unsafe {
            uv98_sidekeys_at32::hardware::SideAdc::from_prepared(
                radio_pins.side_adc,
                radio_pins.battery_adc,
                p.ADC1,
                p.ADCCOM,
                &mut p.CRM,
                Timing::new(),
                10_000,
            )
        },
        0xE052,
    );
    checked(
        &mut log,
        unsafe { startup::arm_independent_watchdog() },
        0xE053,
    );
    let mut radio = Runtime::new(uv98_radio_runtime::hardware::RadioIo::new(
        primary,
        radio_control,
    ));
    let now = require(time.sample(DWT::cycle_count()), 0xE016);
    let calibration = match app.calibration_view() {
        Some(c) => c,
        None => stop(0xE050),
    };
    checked(&mut log, radio.cold_start(&calibration, now), 0xE054);
    // Volatile load retains this linked, inspectable release-policy record.
    let release_policy =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(UV98_RELEASE_POLICY)) };
    event(
        &mut log,
        0x124,
        [
            release_policy[1],
            side.calibration_value() as u32,
            release_policy[2],
        ],
    );
    let mut settings = [policy::Overrides::default(); 2];
    let mut lights = lighting::Lights::new(now);
    let mut current = None;
    let mut scanner = scan::Scheduler::new();
    let mut scope = scope::Scope::new();
    // Set only by an explicit span/step change while a sweep is active. The
    // restart waits for the acknowledged RX restoration instead of retuning an
    // owned sweep; PTT and EXIT never inherit it.
    let mut restart_spectrum = false;
    let mut spectrum_center = None;
    let mut spectrum_edge: Option<(u32, bool)> = None;
    // Pending action after the sweep's selected/original restore completes.
    let mut spectrum_intent: Option<SpectrumIntent> = None;
    // End temporary listening: Some(true) resumes the same window, Some(false)
    // exits the spectrum session. None means no stop was requested.
    let mut listen_after: Option<bool> = None;
    // Publish one final stopped snapshot after the sweep ends, then stop
    // rewriting an identical snapshot every 250 ms while the scope is idle.
    let mut publish_spectrum_final = false;
    let mut spectrum_published_at = now;
    let mut tx_slot = None;
    let mut tx_started_at = None;
    let mut tx_authorization: Option<tx_policy::Authorization> = None;
    let mut ptt_guard = ptt::Guard::new();
    let mut relay_session = None;
    let mut second_rx = None;
    let mut watch_config = None;
    let mut dual_watch = None;
    let mut relay_active = false;
    let mut last_signal = None;
    let mut signal_at = now;
    let mut meter_floor = uv98_handheld_ui::meter::NoiseFloor::default();
    let mut meter_receive = None;
    let mut ui_rssi_at = now;
    let mut telemetry_at = now;
    let mut telemetry_samples = 4u8;
    // Reserve some of the finite logger quota for AM even after the operator
    // spends time browsing FM channels. UI/key events cannot consume it.
    let mut fm_signal_records = 0u8;
    let mut am_signal_records = 0u8;
    let mut fm_register_records = 0u8;
    let mut am_register_records = 0u8;
    let mut heartbeat_at = now;
    let mut idle = Timing::new();
    let mut adc_previous = None;
    let mut battery = uv98_sidekeys_at32::BatteryMonitor::default();
    let mut battery_at = now.wrapping_sub(250);
    let mut ui_aux = uv98_handheld_ui::input::AuxFilter::default();
    app.enable_fast_matrix_input();
    let mut tx_tones = signaling::TransmitTones::new();
    let mut tx_key_latched = false;
    let mut uncertain_tx_since: Option<u32> = None;
    let mut max_loop_cycles = 0;
    let mut max_render_cycles = 0;
    let mut render_rows = 0;
    let mut loop_count = 0u32;
    let mut active_loop_us = 0u32;
    let mut redraw_since = None;
    let mut max_redraw_ms = 0u32;
    let mut ui_log = [[0; 3]; UI_LOG_LIMIT];
    let mut ui_log_len = 0;
    let mut ui_log_sent = 0;
    let mut throughput_log = None;
    loop {
        let loop_started = DWT::cycle_count();
        feed();
        let adc = checked(&mut log, side.sample(), 0xE055);
        let adc_code = uv98_keys::stock_auxiliary_code(adc.raw);
        let key_sample = keys.scan_idle_or_full();
        let mut raw = key_sample
            .as_ref()
            .ok()
            .map(|s| (s.matrix_raw(), s.ptt_pressed()));
        let input_at = require(time.sample(DWT::cycle_count()), 0xE016);
        // A keypad contact can change between forward/reverse scans. During
        // established voice TX, retain only fresh physical PTT for <=32 ms.
        // Never arm from this fallback or turn uncertain data into a DTMF key.
        if radio.state() == RfState::Transmitting
            && matches!(
                key_sample,
                Err(uv98_keyscan_at32::Error::UnstableMatrix { .. })
            )
        {
            let since = *uncertain_tx_since.get_or_insert(input_at);
            if input_at.wrapping_sub(since) < 32 {
                if let Ok(guard) = keys.guard_scan() {
                    raw = Some((0, guard.ptt_pressed()));
                }
            }
        } else {
            uncertain_tx_since = None;
        }
        let matrix = raw.map(|(matrix, _)| matrix);
        let dtmf_key = matrix
            .and_then(signaling::keypad)
            .filter(|_| app.status().view.dtmf_enabled && radio.state() == RfState::Transmitting);
        if dtmf_key.is_some() {
            tx_key_latched = true;
        }
        if matrix == Some(0) && uncertain_tx_since.is_none() {
            tx_key_latched = false;
        }
        let upper = raw.is_some_and(|(_, p)| p);
        let lower = LOWER_PTT_ENABLED && ptt::lower_pressed(adc.raw);
        let raw_ptt = ptt::Input {
            upper,
            lower,
            clean: raw.is_some_and(|(m, _)| m == 0 || dtmf_key.is_some())
                && (ptt::side_idle(adc.raw) || lower),
        };
        if relay_active && (upper || lower) {
            stop_relay(&mut radio, &mut secondary);
            relay_active = false;
        }
        let receive_tool_blocks_tx = matches!(
            app.status().view.screen,
            Screen::Shortwave | Screen::Crossband
        ) || tuner_repower_at.is_some();
        let physical_source_held = ptt_guard.observe(
            raw_ptt,
            scanner.active() || scope.active() || receive_tool_blocks_tx,
        );
        let view = app.status().view;
        if gps_configuration != (view.gps_enabled, view.gps_baud) {
            gps.configure(view.gps_enabled, view.gps_baud);
            gps_configuration = (view.gps_enabled, view.gps_baud);
        }
        gps.poll(input_at, 96);
        if input_at.wrapping_sub(gps_ui_at) >= 500 {
            gps_ui_at = input_at;
            let fix = gps.fix(input_at);
            let report = gps.last_report();
            let position = fix.map(|fix| fix.position);
            checked(
                &mut log,
                app.set_gps_status(uv98_handheld_ui::controller::GpsStatus {
                    valid: position.is_some(),
                    latitude_e7: position.map_or(0, |p| p.latitude_e7),
                    longitude_e7: position.map_or(0, |p| p.longitude_e7),
                    satellites: report.and_then(|r| r.satellites).unwrap_or(0),
                    age_seconds: report
                        .map_or(0, |r| input_at.wrapping_sub(r.received_at_ms) / 1000),
                }),
                0xE032,
            );
        }
        let guarded_target = if cfg!(feature = "channel-tx") {
            profile_for_slot(&app, &settings, tx_slot.unwrap_or(view.active_slot)).ok()
        } else {
            current.map(|p| (view, p))
        };
        let guarded_profile = guarded_target.map(|(_, profile)| profile);
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        // Stop on raw release/chord/source handoff before UI/NOR work.
        let physical_held = LIVE_TX
            && physical_source_held
            && tx_slot.is_some()
            && tx_slot == view.ptt_slot
            && tx_started_at.is_some_and(|start| tx_hold_within_limit(start, now))
            && !view.locked
            && guarded_target.is_some_and(|(target, p)| {
                tx_permitted(&app, target, p, raw_ptt)
                    && (!cfg!(feature = "channel-tx")
                        || tx_authorization
                            .is_some_and(|a| a.still_held_target(view, target, p, raw_ptt)))
            });
        let mut tail_view = view;
        tail_view.ptt_down = false;
        tail_view.ptt_slot = None;
        let allow_tail = !upper
            && !lower
            && matrix == Some(0)
            && uncertain_tx_since.is_none()
            && ptt::side_idle(adc.raw)
            && tx_started_at.is_some_and(|start| tx_hold_within_limit(start, now))
            && guarded_target.is_some_and(|(target, profile)| {
                tx_authorization
                    .is_some_and(|a| a.tail_configuration_valid_target(tail_view, target, profile))
            });
        let calibration = app.calibration_view().unwrap_or_else(|| stop(0xE050));
        let beacon_held = checked(
            &mut log,
            beacon.step(
                &mut radio,
                &calibration,
                now,
                guarded_profile,
                view.screen == Screen::Aprs,
                upper,
                lower,
                ptt::side_idle(adc.raw),
                matrix,
            ),
            0xE066,
        );
        if beacon.take_finished() {
            current = None;
            last_signal = None;
        }
        beacon.refresh(gps.fix(now).is_some(), now);
        checked(&mut log, app.set_aprs_status(beacon.status()), 0xE032);
        let tx_held = if beacon.active() {
            beacon_held
        } else {
            let mut signal_voice = physical_held;
            let mut signal_tail = allow_tail;
            if radio.mdc_progress().is_some() {
                // Modem preparation/arming/voice restoration each consume a
                // short guarded step. Never commit against a physical input
                // sampled before GPS/UI/other foreground work in this pass.
                let fresh_adc = checked(&mut log, side.sample(), 0xE055);
                let guard = keys.guard_scan();
                let input = ptt::Input {
                    upper: guard.as_ref().is_ok_and(|s| s.ptt_pressed()),
                    lower: LOWER_PTT_ENABLED && ptt::lower_pressed(fresh_adc.raw),
                    clean: guard.as_ref().is_ok_and(|s| s.matrix_clear())
                        && (ptt::side_idle(fresh_adc.raw)
                            || (LOWER_PTT_ENABLED && ptt::lower_pressed(fresh_adc.raw))),
                };
                let same_source = ptt_guard.observe(
                    input,
                    scanner.active() || scope.active() || receive_tool_blocks_tx,
                );
                signal_voice &= same_source && input.clean;
                signal_tail &=
                    input.clean && !input.upper && !input.lower && ptt::side_idle(fresh_adc.raw);
            }
            let signal_now = require(time.sample(DWT::cycle_count()), 0xE016);
            let within_limit =
                tx_started_at.is_some_and(|start| tx_hold_within_limit(start, signal_now));
            checked(
                &mut log,
                tx_tones.step(
                    &mut radio,
                    signal_now,
                    signal_voice && within_limit,
                    signal_tail && within_limit,
                    dtmf_key,
                    signaling::mdc_config(view, tx_slot.unwrap_or(view.active_slot)),
                ),
                0xE065,
            )
        };
        // Foreground signaling may perform bounded SPI work. Re-evaluate TOT
        // against elapsed time rather than the timestamp before that work.
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        let mut polled = checked(&mut log, radio.tick(now, tx_held), 0xE056);
        // RX restoration must not spend a full UI pass between a few register
        // writes: a repeater may answer immediately after PTT release. Allow
        // a larger RX-only batch, preserving every explicit settle delay.
        // An active/starting transmitter retains the short batch and a fresh
        // physical PTT check before each operation. Admission is checked before
        // each transaction, not an interruption of synchronous bus work.
        let restoring_rx = radio.state() == RfState::Configuring
            && !radio.tx_active_or_starting();
        let burst_cycles = if restoring_rx { 384_000 } else { 96_000 };
        let burst_steps = if restoring_rx { 63 } else { 7 };
        let burst_started = DWT::cycle_count();
        for _ in 0..burst_steps {
            let now = require(time.sample(DWT::cycle_count()), 0xE016);
            if beacon.active()
                || !radio.configuration_step_ready(now)
                || DWT::cycle_count().wrapping_sub(burst_started) >= burst_cycles
                || polled.tx_ended
            {
                break;
            }
            feed();
            let held = if radio.tx_active_or_starting() {
                // This faster aggregate matrix scan only sustains an already
                // armed PTT source. It never creates UI events or arms TX.
                let fresh_adc = checked(&mut log, side.sample(), 0xE055);
                let guard = keys.guard_scan();
                let input = ptt::Input {
                    upper: guard.as_ref().is_ok_and(|s| s.ptt_pressed()),
                    lower: LOWER_PTT_ENABLED && ptt::lower_pressed(fresh_adc.raw),
                    clean: guard.as_ref().is_ok_and(|s| s.matrix_clear())
                        && (ptt::side_idle(fresh_adc.raw)
                            || (LOWER_PTT_ENABLED && ptt::lower_pressed(fresh_adc.raw))),
                };
                let same_source = ptt_guard.observe(
                    input,
                    scanner.active() || scope.active() || receive_tool_blocks_tx,
                );
                let now = require(time.sample(DWT::cycle_count()), 0xE016);
                let guarded_target = if cfg!(feature = "channel-tx") {
                    profile_for_slot(&app, &settings, tx_slot.unwrap_or(view.active_slot)).ok()
                } else {
                    current.map(|p| (view, p))
                };
                LIVE_TX
                    && same_source
                    && tx_slot.is_some()
                    && tx_slot == view.ptt_slot
                    && tx_started_at.is_some_and(|start| tx_hold_within_limit(start, now))
                    && !view.locked
                    && guarded_target.is_some_and(|(target, p)| {
                        tx_permitted(&app, target, p, input)
                            && (!cfg!(feature = "channel-tx")
                                || tx_authorization
                                    .is_some_and(|a| a.still_held_target(view, target, p, input)))
                    })
            } else {
                false
            };
            let now = require(time.sample(DWT::cycle_count()), 0xE016);
            let next = checked(&mut log, radio.tick(now, held), 0xE056);
            polled.changed |= next.changed;
            polled.tx_ended |= next.tx_ended;
            polled.tx_timed_out |= next.tx_timed_out;
            polled.state = next.state;
        }
        if polled.tx_ended {
            ptt_guard.disarm();
            tx_slot = None;
            tx_started_at = None;
            tx_authorization = None;
            current = None;
        }
        // UART logging can block for a frame; physical TX release is handled
        // first. Side-button identities are still telemetry, not inferred PTTs.
        if !radio.tx_active_or_starting()
            && !beacon.active()
            && adc_previous != Some((adc_code, upper))
        {
            event(
                &mut log,
                0x171,
                [u32::from(adc.raw), u32::from(adc_code), u32::from(upper)],
            );
            adc_previous = Some((adc_code, upper));
        }
        let aux_now = require(time.sample(DWT::cycle_count()), 0xE016);
        // Broad stock Aux16 includes a top-key + lower-key chord. Only the
        // isolated lower-key ADC window may enter the PTT event stream.
        let auxiliary = uv98_keys::auxiliary_key(adc.raw)
            .filter(|key| !LOWER_PTT_ENABLED || *key != uv98_keys::Key::Aux16 || lower);
        let stable_aux = ui_aux.sample(aux_now, auxiliary);
        let sample = match raw {
            Some((matrix, ptt_pressed)) => InputSample::WithAux {
                matrix: if tx_key_latched { 0 } else { matrix },
                ptt_pressed,
                aux: if lower {
                    Some(uv98_keys::Key::Aux16)
                } else if matrix == 0 {
                    stable_aux
                } else {
                    None
                },
            },
            None if matches!(
                key_sample,
                Err(uv98_keyscan_at32::Error::UnstableMatrix { .. })
            ) =>
            {
                InputSample::MatrixTransition
            }
            None => InputSample::Unstable,
        };
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        // Any physical side-key activity cancels a sweep. PTT cannot turn this
        // cancellation into transmission; the guard requires a later release
        // and a new eligible edge on the normal channel screen. Side activity
        // also cancels a queued span/step restart: the pending restore still
        // ends with StopReason::Exit, which must not be read as restart intent.
        let side_activity = upper || lower;
        if side_activity {
            restart_spectrum = false;
        }
        scope.event(now, spectrum::Event::Ptt(side_activity));
        let update = checked(&mut log, app.sample(now, sample), 0xE032);
        if let Some(key) = update.pressed.filter(|_| {
            cfg!(feature = "verbose-runtime") && !beacon.active() && ui_log_len < ui_log.len()
        }) {
            let v = app.status().view;
            // USART6 waits for all 47 bytes to finish. Queue the snapshot at
            // the event so key repeats do not stall the LCD pass or RF work.
            ui_log[ui_log_len] = [
                uv98_handheld_app::key_code(key),
                v.screen as u32,
                v.channel.map(u32::from).unwrap_or(u32::MAX),
            ];
            ui_log_len += 1;
        }
        // Preserve the explicitly historical trial's selected-A allowlist.
        // The ordinary channel-TX build never changes focus for a PTT press.
        #[cfg(feature = "upper-exo-tx-trial")]
        if update.ptt_tx_requested && ptt_guard.allow_edge() {
            if let Some(slot) = update.ptt_slot {
                checked(&mut log, app.select_slot(slot), 0xE032);
            }
        }
        let index = if app.status().view.active_slot == Slot::A {
            0
        } else {
            1
        };
        let mut forced_retune = false;
        let mut action = scan::Action::None;
        if update.cancel_temporary {
            settings[index].monitor = false;
            // EXIT/PTT cancels any queued restart even if the in-flight restore
            // still completes with StopReason::Exit.
            restart_spectrum = false;
            checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
            if scope.listening() || scope.listen_restoring() {
                checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                listen_after = Some(false);
            } else {
                spectrum_intent = Some(SpectrumIntent::Exit);
                scope.event(now, spectrum::Event::Stop);
            }
            if scanner.active() {
                action = scanner.cancel();
                checked(&mut log, app.cancel_scan(), 0xE032);
                checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
            }
        }
        if let Some(request) = update.request {
            if matches!(
                request,
                Request::SetPower(_)
                    | Request::SetMonitor(_)
                    | Request::SetModulation { .. }
                    | Request::StartSpectrum
                    | Request::RestartSpectrum
            ) && scanner.active()
            {
                scanner.cancel();
                checked(&mut log, app.cancel_scan(), 0xE032);
                checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
            }
            match request {
                Request::StartSpectrum => {
                    spectrum_center = None;
                    spectrum_edge = None;
                    let calibration = match app.calibration_view() {
                        Some(c) => c,
                        None => stop(0xE050),
                    };
                    let started = match profile(&app, &settings).ok().and_then(|p| p.receive) {
                        Some(rx) => match spectrum_window(app.status().view, rx.frequency_10hz) {
                            Some(window) => {
                                scope.start_window(&radio, &calibration, rx, window, now)
                            }
                            None => Err(scope::StartError::CenterMismatch),
                        },
                        None => Err(scope::StartError::NotReceiving),
                    };
                    match started {
                        Ok(()) => {
                            restart_spectrum = false;
                            spectrum_intent = None;
                            listen_after = None;
                            ptt_guard.disarm();
                            last_signal = None;
                            checked(&mut log, app.clear_spectrum(), 0xE032);
                            spectrum_published_at = now.wrapping_sub(250);
                        }
                        Err(error) => {
                            scope.discard();
                            publish_spectrum_final = false;
                            checked(&mut log, app.clear_spectrum(), 0xE032);
                            checked(
                                &mut log,
                                app.reject_spectrum_start(spectrum_error(error)),
                                0xE032,
                            );
                        }
                    }
                }
                Request::StopSpectrum => {
                    restart_spectrum = false;
                    checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                    if scope.listening() || scope.listen_restoring() {
                        checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                        listen_after = Some(false);
                    } else {
                        spectrum_intent = Some(SpectrumIntent::Exit);
                        scope.event(now, spectrum::Event::Stop);
                    }
                }
                Request::RestartSpectrum => {
                    spectrum_edge = None;
                    if scope.listening() || scope.listen_restoring() {
                        checked(
                            &mut log,
                            app.note_spectrum_error(render::SpectrumError::Busy),
                            0xE032,
                        );
                    } else if scope.can_restart() {
                        restart_spectrum = true;
                        spectrum_intent = Some(SpectrumIntent::Restart);
                        scope.event(now, spectrum::Event::Stop);
                    } else if scope.active() {
                        checked(
                            &mut log,
                            app.note_spectrum_error(render::SpectrumError::Busy),
                            0xE032,
                        );
                    } else {
                        let calibration = match app.calibration_view() {
                            Some(c) => c,
                            None => stop(0xE050),
                        };
                        let started = match profile(&app, &settings).ok().and_then(|p| p.receive) {
                            Some(rx) => {
                                match spectrum_window(
                                    app.status().view,
                                    spectrum_center.unwrap_or(rx.frequency_10hz),
                                ) {
                                    Some(window) => scope.start_panned_window(
                                        &radio,
                                        &calibration,
                                        rx,
                                        window,
                                        now,
                                    ),
                                    None => Err(scope::StartError::CenterMismatch),
                                }
                            }
                            None => Err(scope::StartError::NotReceiving),
                        };
                        match started {
                            Ok(()) => {
                                listen_after = None;
                                last_signal = None;
                                spectrum_published_at = now.wrapping_sub(250);
                            }
                            Err(error) => {
                                scope.discard();
                                publish_spectrum_final = false;
                                checked(&mut log, app.clear_spectrum(), 0xE032);
                                checked(
                                    &mut log,
                                    app.reject_spectrum_start(spectrum_error(error)),
                                    0xE032,
                                );
                            }
                        }
                    }
                }
                Request::PanSpectrum { forward, fine } => {
                    let from_listen = (scope.listening() || scope.listen_restoring())
                        && listen_after != Some(false);
                    if scope.can_restart() || restart_spectrum || from_listen {
                        let next = (|| {
                            let current = scope.window()?;
                            let delta = if fine {
                                current.step_10hz()
                            } else {
                                current.span_10hz() / 2
                            };
                            let base = spectrum_center.unwrap_or(current.center_10hz());
                            let center = if forward {
                                base.checked_add(delta)?
                            } else {
                                base.checked_sub(delta)?
                            };
                            let window = spectrum_window(app.status().view, center)?;
                            let original = profile(&app, &settings).ok()?.receive?;
                            let calibration = app.calibration_view()?;
                            scope::Scope::qualify_window(&calibration, original, window).ok()?;
                            Some(center)
                        })();
                        if let Some(center) = next {
                            spectrum_center = Some(center);
                            spectrum_edge = fine.then_some((center, forward));
                            if from_listen {
                                checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                                listen_after = Some(true);
                            } else if !restart_spectrum {
                                restart_spectrum = true;
                                spectrum_intent = Some(SpectrumIntent::Restart);
                                scope.event(now, spectrum::Event::Stop);
                            }
                        } else {
                            checked(
                                &mut log,
                                app.note_spectrum_error(render::SpectrumError::Route),
                                0xE032,
                            );
                        }
                    } else {
                        checked(
                            &mut log,
                            app.note_spectrum_error(render::SpectrumError::Busy),
                            0xE032,
                        );
                    }
                }
                Request::ResumeSpectrum => {
                    scope.resume();
                    spectrum_published_at = now.wrapping_sub(250);
                }
                Request::ResetSpectrumPeaks => {
                    if scope.active() {
                        scope.event(now, spectrum::Event::ClearPeaks);
                        spectrum_published_at = now.wrapping_sub(250);
                    }
                }
                Request::ListenSpectrum { bin } => {
                    let published =
                        app.spectrum_view()
                            .map(|view| spectrum_flow::PublishedWindow {
                                generation: view.window_id,
                                start_10hz: view.start_10hz,
                                step_10hz: view.step_10hz,
                                len: view.len,
                            });
                    let bound =
                        spectrum_flow::bind_bin(published, scope.window(), scope.generation(), bin);
                    if bound.is_ok()
                        && scope.can_accept_bin_action()
                        && scope.plot().valid(usize::from(bin))
                    {
                        spectrum_intent = Some(SpectrumIntent::Listen(bin));
                        listen_after = None;
                        scope.event(now, spectrum::Event::Accept { index: bin });
                    } else {
                        let reason = if bound.is_err() {
                            render::SpectrumError::Stale
                        } else if scope.plot().valid(usize::from(bin)) {
                            render::SpectrumError::Busy
                        } else {
                            render::SpectrumError::Window
                        };
                        if !matches!(spectrum_intent, Some(SpectrumIntent::Listen(_))) {
                            checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                        }
                        checked(&mut log, app.note_spectrum_error(reason), 0xE032);
                    }
                }
                Request::StopListenSpectrum => {
                    checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                    if scope.listening() || scope.listen_restoring() {
                        // Resume the same sweep window after the original RX
                        // plan is confirmed again. The UI stops claiming audio
                        // before the restore completes.
                        checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                        listen_after = Some(true);
                        spectrum_published_at = now.wrapping_sub(250);
                    }
                }
                Request::AcceptSpectrum { bin } => {
                    // Explicit operator action: only a measured bin on the
                    // published grid may become the new VFO frequency, and
                    // only after full RF restore.
                    let published =
                        app.spectrum_view()
                            .map(|view| spectrum_flow::PublishedWindow {
                                generation: view.window_id,
                                start_10hz: view.start_10hz,
                                step_10hz: view.step_10hz,
                                len: view.len,
                            });
                    let bound =
                        spectrum_flow::bind_bin(published, scope.window(), scope.generation(), bin);
                    let listened = current.and_then(|profile| profile.receive).filter(|rx| {
                        scope.listening()
                            && listen_after.is_none()
                            && app.status().view.spectrum_listening
                            && bound
                                .as_ref()
                                .is_ok_and(|frequency| *frequency == rx.frequency_10hz)
                    });
                    if let Some(rx) = listened {
                        if !policy::aviation_band(rx.frequency_10hz) {
                            settings[index].modulation = Some(rx.modulation);
                        }
                        checked(
                            &mut log,
                            app.accept_spectrum_frequency(rx.frequency_10hz),
                            0xE032,
                        );
                        current = policy::restored_current(profile(&app, &settings).ok(), rx);
                        checked(&mut log, app.clear_spectrum(), 0xE032);
                        scope.discard();
                        publish_spectrum_final = false;
                        last_signal = None;
                    } else if bound.is_ok()
                        && scope.can_accept_bin_action()
                        && scope.plot().valid(usize::from(bin))
                    {
                        spectrum_intent = Some(SpectrumIntent::AcceptVfo(bin));
                        scope.event(now, spectrum::Event::Accept { index: bin });
                    } else if bound.is_err() {
                        checked(
                            &mut log,
                            app.note_spectrum_error(render::SpectrumError::Stale),
                            0xE032,
                        );
                    } else {
                        checked(&mut log, app.reject_radio_request(request), 0xE032);
                    }
                }
                Request::SendAprs => {
                    if LIVE_TX
                        && !radio.tx_active_or_starting()
                        && !scope.active()
                        && !scanner.active()
                        && !relay_active
                        && tuner_session.is_none()
                    {
                        if let Some(profile) = profile(&app, &settings).ok() {
                            let calibration =
                                app.calibration_view().unwrap_or_else(|| stop(0xE050));
                            let fix = gps
                                .fix(now)
                                .map(|fix| (fix.position.latitude_e7, fix.position.longitude_e7));
                            if radio.state() == RfState::Receiving && current == Some(profile) {
                                beacon.request(&calibration, profile, fix, now);
                            }
                            checked(&mut log, app.set_aprs_status(beacon.status()), 0xE032);
                            ptt_guard.disarm();
                        }
                    }
                }
                Request::RetryShortwave => {
                    tuner_failed = None;
                    tuner_mode_switch.reset_retry();
                    checked(
                        &mut log,
                        app.set_shortwave_status(ShortwaveStatus::Starting),
                        0xE032,
                    );
                }
                Request::SetDisplayLight(mode) => {
                    lights.set_display(mode, now);
                    let (display, keys) = lights.modes();
                    checked(&mut log, app.set_lighting_modes(display, keys), 0xE032);
                }
                Request::SetKeyLight(mode) => {
                    lights.set_keys(mode, now);
                    let (display, keys) = lights.modes();
                    checked(&mut log, app.set_lighting_modes(display, keys), 0xE032);
                }
                Request::SetPower(power) => settings[index].power = power,
                Request::SetMonitor(enabled) => settings[index].monitor = enabled,
                Request::SetModulation { am } => {
                    settings[index].modulation =
                        Some(if am { Modulation::Am } else { Modulation::Fm })
                }
                Request::CyclePower => settings[index].power = (settings[index].power + 1) % 3,
                Request::ToggleMonitor => settings[index].monitor = !settings[index].monitor,
                Request::CycleModulation => {
                    let previous = profile(&app, &settings)
                        .ok()
                        .map(|p| p.audio.modulation)
                        .unwrap_or(Modulation::Fm);
                    settings[index].modulation = Some(if previous == Modulation::Fm {
                        Modulation::Am
                    } else {
                        Modulation::Fm
                    });
                }
                Request::StartScan => {
                    let now = require(time.sample(DWT::cycle_count()), 0xE016);
                    action = match policy::scan_target(app.status().view)
                        .ok()
                        .and_then(|target| scanner.start(target, now).ok())
                    {
                        Some(action) => action,
                        None => {
                            checked(&mut log, app.reject_radio_request(request), 0xE032);
                            scan::Action::None
                        }
                    };
                }
                Request::StopScan => {
                    action = scanner.cancel();
                    checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
                }
            }
        }
        if scanner.active()
            && (upper
                || !ptt::side_idle(adc.raw)
                || matches!(
                    app.status().view.scan,
                    ScanState::Stopped | ScanState::Stopping
                ))
        {
            action = scanner.cancel();
            checked(&mut log, app.cancel_scan(), 0xE032);
            checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
        }
        if action != scan::Action::None {
            forced_retune = checked(&mut log, scan_action(&mut app, action), 0xE032);
            if !forced_retune && action != scan::Action::Stop {
                scanner.cancel();
            }
        }
        if scope.active() {
            let calibration = match app.calibration_view() {
                Some(c) => c,
                None => stop(0xE050),
            };
            let view = app.status().view;
            scope.set_trigger(
                Some(u16::from(view.spectrum_trigger)),
                view.spectrum_auto_listen,
            );
            let completion = checked(
                &mut log,
                scope.step(&mut radio, &calibration, || {
                    require(time.sample(DWT::cycle_count()), 0xE016)
                }),
                0xE05E,
            );
            if let scope::Completion::Restored { request, reason } = completion {
                let intent = spectrum_intent.take();
                last_signal = None;
                telemetry_samples = 4;
                let action = spectrum_flow::completion(intent, reason);
                if action != SpectrumAction::BeginListen {
                    checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                }
                match action {
                    SpectrumAction::BeginListen => {
                        let at = require(time.sample(DWT::cycle_count()), 0xE016);
                        if scope.begin_listen(at) {
                            // A cancel may already have arrived while the bin
                            // restore was in flight; the listen-end pass below
                            // then restores the original plan immediately.
                            let listening = listen_after.is_none();
                            if listening {
                                // Bind the UI to the exact bin/request the RF
                                // just restored; never to a cursor moved during
                                // the pending restore.
                                if let spectrum::StopReason::Accepted { index } = reason {
                                    let accepted =
                                        matches!(app.confirm_spectrum_listen(index), Ok(true));
                                    if accepted {
                                        current = radio_settings(&app)
                                            .map(|cfg| policy::listen_profile(request, cfg));
                                        spectrum_published_at = at.wrapping_sub(250);
                                    } else {
                                        // UI rejected this late ACK. Restore the
                                        // saved receiver now, not after Listen timeout.
                                        listen_after = Some(false);
                                        checked(
                                            &mut log,
                                            app.set_spectrum_listening(false),
                                            0xE032,
                                        );
                                        checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                                        checked(
                                            &mut log,
                                            app.note_spectrum_error(render::SpectrumError::Restore),
                                            0xE032,
                                        );
                                    }
                                } else {
                                    listen_after = Some(false);
                                    checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                                    checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                                    checked(
                                        &mut log,
                                        app.note_spectrum_error(render::SpectrumError::Restore),
                                        0xE032,
                                    );
                                }
                            } else {
                                checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                                checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                            }
                        } else {
                            checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                            checked(&mut log, app.cancel_spectrum_pending(), 0xE032);
                            checked(
                                &mut log,
                                app.note_spectrum_error(render::SpectrumError::Restore),
                                0xE032,
                            );
                        }
                    }
                    SpectrumAction::AcceptVfo => {
                        if !policy::aviation_band(request.frequency_10hz) {
                            settings[index].modulation = Some(request.modulation);
                        }
                        checked(
                            &mut log,
                            app.accept_spectrum_frequency(request.frequency_10hz),
                            0xE032,
                        );
                        current = policy::restored_current(profile(&app, &settings).ok(), request);
                        checked(&mut log, app.clear_spectrum(), 0xE032);
                        scope.discard();
                        publish_spectrum_final = false;
                        spectrum_published_at = now.wrapping_sub(250);
                    }
                    SpectrumAction::Restart => {
                        let mut restarted = false;
                        let mut discarded = false;
                        if restart_spectrum {
                            restart_spectrum = false;
                            if matches!(reason, spectrum::StopReason::Exit) {
                                let started = {
                                    let calibration = match app.calibration_view() {
                                        Some(c) => c,
                                        None => stop(0xE050),
                                    };
                                    match spectrum_window(
                                        app.status().view,
                                        spectrum_center.unwrap_or(request.frequency_10hz),
                                    ) {
                                        Some(window) => scope
                                            .start_panned_window(
                                                &radio,
                                                &calibration,
                                                request,
                                                window,
                                                require(time.sample(DWT::cycle_count()), 0xE016),
                                            )
                                            .is_ok(),
                                        None => false,
                                    }
                                };
                                if started {
                                    restarted = true;
                                    last_signal = None;
                                    telemetry_samples = 4;
                                    spectrum_published_at = now.wrapping_sub(250);
                                } else {
                                    scope.discard();
                                    discarded = true;
                                    publish_spectrum_final = false;
                                    checked(&mut log, app.clear_spectrum(), 0xE032);
                                    checked(
                                        &mut log,
                                        app.reject_spectrum_start(render::SpectrumError::Route),
                                        0xE032,
                                    );
                                }
                            }
                        }
                        if !restarted && !discarded {
                            current =
                                policy::restored_current(profile(&app, &settings).ok(), request);
                            publish_spectrum_final = true;
                            spectrum_published_at = now.wrapping_sub(250);
                        }
                    }
                    SpectrumAction::Finish => {
                        current = policy::restored_current(profile(&app, &settings).ok(), request);
                        publish_spectrum_final = true;
                        spectrum_published_at = now.wrapping_sub(250);
                    }
                }
            }
        }
        if scope.listening() || scope.listen_restoring() {
            let at = require(time.sample(DWT::cycle_count()), 0xE016);
            if scope.listen_expired(at) && listen_after.is_none() {
                checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                listen_after = Some(true);
            }
            if let Some(resume) = listen_after {
                let window = scope.window();
                let held_bin = scope.completed_plot().held_bin;
                let restored = {
                    let calibration = match app.calibration_view() {
                        Some(c) => c,
                        None => stop(0xE050),
                    };
                    checked(
                        &mut log,
                        scope.end_listen(&mut radio, &calibration, at),
                        0xE05F,
                    )
                };
                if let Some(original) = restored {
                    checked(&mut log, app.set_spectrum_listening(false), 0xE032);
                    current = policy::restored_current(profile(&app, &settings).ok(), original);
                    listen_after = None;
                    if resume {
                        let target = spectrum_center
                            .or_else(|| window.map(|window| window.center_10hz()))
                            .and_then(|center| spectrum_window(app.status().view, center));
                        let started = {
                            let calibration = match app.calibration_view() {
                                Some(c) => c,
                                None => stop(0xE050),
                            };
                            target.is_some_and(|window| {
                                scope
                                    .start_panned_window(&radio, &calibration, original, window, at)
                                    .is_ok()
                            })
                        };
                        if started {
                            if let Some(bin) = held_bin.filter(|_| target == window) {
                                scope.suppress_bin_until_below(bin);
                            }
                            spectrum_published_at = at.wrapping_sub(250);
                        } else {
                            scope.discard();
                            publish_spectrum_final = false;
                            checked(&mut log, app.clear_spectrum(), 0xE032);
                            checked(
                                &mut log,
                                app.reject_spectrum_start(render::SpectrumError::Route),
                                0xE032,
                            );
                        }
                    } else {
                        scope.discard();
                        publish_spectrum_final = false;
                        checked(&mut log, app.clear_spectrum(), 0xE032);
                    }
                }
            }
        }
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        if (scope.active() || publish_spectrum_final)
            && now.wrapping_sub(spectrum_published_at) >= 125
        {
            if let Some(range) = scope.range() {
                let state = if scope.listening() {
                    SpectrumState::Listening
                } else if scope.listen_restoring() {
                    SpectrumState::Restoring
                } else {
                    match scope.state() {
                        spectrum::State::Preparing => SpectrumState::Preparing,
                        spectrum::State::Tuning
                        | spectrum::State::Settling
                        | spectrum::State::Reading
                        | spectrum::State::SweepComplete => SpectrumState::Sweeping,
                        spectrum::State::Restoring(_) => SpectrumState::Restoring,
                        spectrum::State::Failed(_) => SpectrumState::Failed,
                        spectrum::State::Idle | spectrum::State::Restored(_) => {
                            SpectrumState::Stopped
                        }
                    }
                };
                let snapshot = SpectrumSnapshot {
                    state,
                    window_id: scope.generation(),
                    start_10hz: range.start_10hz(),
                    center_10hz: scope
                        .window()
                        .map(|window| window.center_10hz())
                        .or_else(|| range.frequency_10hz(range.len() / 2))
                        .unwrap_or(range.start_10hz()),
                    step_10hz: range.step_10hz(),
                    len: range.len() as u8,
                    plot: *scope.completed_plot(),
                };
                let edge_cursor = spectrum_edge
                    .filter(|&(center, _)| {
                        center == snapshot.center_10hz && scope.can_accept_bin_action()
                    })
                    .map(|(_, forward)| if forward { snapshot.len - 1 } else { 0 });
                if checked(
                    &mut log,
                    app.set_spectrum_with_cursor(snapshot, edge_cursor),
                    0xE032,
                ) {
                    if edge_cursor.is_some() {
                        spectrum_edge = None;
                    }
                    spectrum_published_at = now;
                    publish_spectrum_final = false;
                    if let Some(bin) = snapshot.plot.held_bin {
                        if spectrum_intent.is_none()
                            && !side_activity
                            && scope.can_accept_bin_action()
                            && scope.plot().valid(usize::from(bin))
                            && checked(&mut log, app.begin_spectrum_peak_listen(bin), 0xE032)
                        {
                            spectrum_intent = Some(SpectrumIntent::Listen(bin));
                            listen_after = None;
                            scope.event(now, spectrum::Event::Accept { index: bin });
                        }
                    }
                }
            }
        }
        let crossband_open = app.status().view.screen == Screen::Crossband;
        if !LIVE_TX && (!crossband_open || upper || lower) && second_rx.take().is_some() {
            stop_secondary(&mut secondary);
        }
        if !crossband_open {
            if relay_active {
                stop_relay(&mut radio, &mut secondary);
                relay_active = false;
            }
            relay_session = None;
        }
        let operator_profile = profile(&app, &settings).ok();
        let view = app.status().view;
        let watch_pair = if view.dual_watch
            && view.screen == Screen::Home
            && view.entry.is_none()
            && view.scan == ScanState::Stopped
            && !scanner.active()
            && !scope.active()
            && tuner_repower_at.is_none()
            && !(upper || lower || update.ptt_tx_requested || radio.tx_active_or_starting())
        {
            operator_profile
                .filter(|p| uv98_radio_runtime::watch::eligible(*p))
                .and_then(|primary| {
                    let (peer, channel) = app.peer_radio_selection();
                    let i = usize::from(peer.active_slot == Slot::B);
                    let other =
                        policy::profile(peer, channel, radio_settings(&app)?, settings[i]).ok()?;
                    (uv98_radio_runtime::watch::eligible(other)
                        && primary.frequency != other.frequency)
                        .then_some((view.active_slot, primary, other))
                })
        } else {
            None
        };
        if watch_config != watch_pair {
            watch_config = watch_pair;
            dual_watch = watch_pair.map(|(slot, _, _)| uv98_radio_runtime::watch::Watch::new(slot));
        }
        let watch_receive =
            watch_pair
                .zip(dual_watch.as_ref())
                .map(|((selected, primary, other), watch)| {
                    let slot = watch.slot();
                    (slot, if slot == selected { primary } else { other })
                });
        let mut wanted = relay_session
            .as_ref()
            .map(crossband::Session::listening_profile)
            .or(watch_receive.map(|(_, rx)| rx))
            .or(operator_profile);
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        let tuner_view = app.status().view;
        let tuner_open = tuner_view.screen == Screen::Shortwave;
        let tuner_mode = tuner_view.shortwave_mode;
        let frequency = tuner_view.shortwave_khz;
        let tuning = shortwave::Tuning::from_selection(
            frequency,
            tuner_view.shortwave_fraction_hz,
            tuner_mode,
            tuner_view.shortwave_bfo,
        );
        let scope_enabled = tuner_open && tuner_view.shortwave_scope;
        let tuner_processing = shortwave::Processing {
            filter: match tuner_view.shortwave_filter {
                1 => shortwave::AmFilter::Voice3k,
                2 => shortwave::AmFilter::Narrow1k,
                _ => shortwave::AmFilter::Wide6k,
            },
            soft_mute: tuner_view.shortwave_soft_mute,
        };
        tuner_sweep.set_processing(tuner_processing);
        let requested_tuning = tuner_sweep.request(tuning, scope_enabled);
        if scope_enabled {
            checked(&mut log, radio.shortwave_audio(false), 0xE05D);
        }
        if tuner_failed.is_some_and(|failed| failed != tuning) {
            tuner_failed = None;
            tuner_mode_switch.reset_retry();
        }
        let tuner_settled = tuner_mode_switch.settled(tuner_open.then_some(tuner_mode), now);
        if tuner_repower_at.is_some_and(|at| now.wrapping_sub(at) < 0x8000_0000) {
            tuner_repower_at = None;
        }
        if !tuner_open {
            tuner_failed = None;
        }
        if (!tuner_open || (tuner_settled && tuner_function != tuner_mode.tuner_function()))
            && tuner_session.take().is_some()
        {
            ptt_guard.disarm();
            checked(&mut log, radio.shortwave_audio(false), 0xE05D);
            tuner_bus.stop();
            checked(&mut log, radio.shortwave_power(false), 0xE05D);
            tuner_repower_at = Some(now.wrapping_add(200));
        }
        if tuner_open
            && tuner_settled
            && radio.initialized()
            && !scanner.active()
            && !scope.active()
            && !radio.tx_active_or_starting()
            && tuner_repower_at.is_none()
            && tuner_failed != Some(tuning)
        {
            if tuner_session.is_none() {
                ptt_guard.disarm();
                checked(&mut log, radio.stop(), 0xE057);
                current = None;
                last_signal = None;
                checked(&mut log, radio.shortwave_power(true), 0xE05D);
                tuner_bus.start(&mut p.CRM);
                tuner_session = Some(shortwave::Session::new(now));
                tuner_function = tuner_mode.tuner_function();
            }
            let session = tuner_session.as_mut().unwrap();
            session.set_sweep_mode(scope_enabled);
            session.set_processing(tuner_processing);
            let tuner_result =
                session.tick(&mut tuner_bus, now, requested_tuning, |offset, bytes| {
                    app.read_shortwave_patch(offset, bytes)
                });
            if let Ok(Some(update)) = tuner_result {
                if let Some(sample) = tuner_sweep.observe(requested_tuning, update) {
                    let window = uv98_radio_runtime::shortwave_scope::Window::around(tuning);
                    checked(
                        &mut log,
                        app.shortwave_scope_sample(
                            window.first_khz * 1000,
                            u32::from(window.step_khz) * 1000,
                            sample.bin,
                            sample.rssi,
                            sample.snr,
                            sample.complete,
                        ),
                        0xE032,
                    );
                }
            }
            match tuner_result {
                Ok(Some(shortwave::Update::Identified(id))) => {
                    event(
                        &mut log,
                        0x183,
                        [
                            u32::from_le_bytes([id[0], id[1], id[2], id[3]]),
                            u32::from_le_bytes([id[4], id[5], id[6], id[7]]),
                            0,
                        ],
                    );
                }
                Ok(Some(shortwave::Update::PatchProgress(percent))) => {
                    checked(
                        &mut log,
                        app.set_shortwave_status(ShortwaveStatus::Patching(percent)),
                        0xE032,
                    );
                }
                Ok(Some(shortwave::Update::Patched)) => {
                    event(&mut log, 0x186, [0x9d29, 0, 0]);
                }
                Ok(Some(shortwave::Update::Tuning)) => {
                    checked(&mut log, radio.shortwave_audio(false), 0xE05D);
                    checked(
                        &mut log,
                        app.set_shortwave_status(ShortwaveStatus::Starting),
                        0xE032,
                    );
                }
                Ok(Some(shortwave::Update::Tuned { part, rssi, snr })) => {
                    tuner_mode_switch.reset_retry();
                    let scanning = !scope_enabled
                        && checked(&mut log, app.continue_shortwave_scan(rssi, snr), 0xE032);
                    if !scanning && !scope_enabled {
                        checked(&mut log, radio.shortwave_audio(true), 0xE05D);
                        checked(
                            &mut log,
                            app.set_shortwave_status(ShortwaveStatus::Listening {
                                part,
                                rssi,
                                snr,
                            }),
                            0xE032,
                        );
                        event(
                            &mut log,
                            0x184,
                            [frequency, u32::from(rssi), u32::from(snr)],
                        );
                    }
                }
                Ok(Some(shortwave::Update::Quality { part, rssi, snr })) if !scope_enabled => {
                    if !tuner_view.shortwave_scan {
                        checked(&mut log, radio.shortwave_audio(true), 0xE05D);
                    }
                    checked(
                        &mut log,
                        app.set_shortwave_status(ShortwaveStatus::Listening { part, rssi, snr }),
                        0xE032,
                    );
                }
                Ok(None) | Ok(Some(shortwave::Update::Quality { .. })) => {}
                Err(code) => {
                    checked(&mut log, radio.shortwave_audio(false), 0xE05D);
                    tuner_bus.stop();
                    checked(&mut log, radio.shortwave_power(false), 0xE05D);
                    tuner_session = None;
                    tuner_repower_at = Some(now.wrapping_add(200));
                    let retry = tuner_mode_switch.retry(code);
                    if !retry {
                        tuner_failed = Some(tuning);
                    }
                    let state = if retry {
                        ShortwaveStatus::Recovering(code)
                    } else {
                        ShortwaveStatus::Fault(code)
                    };
                    checked(&mut log, app.set_shortwave_status(state), 0xE032);
                    event(
                        &mut log,
                        0x185,
                        [u32::from(code), frequency, u32::from(retry)],
                    );
                }
            }
        }

        if radio.initialized()
            && !radio.tx_active_or_starting()
            && !scope.active()
            && !tuner_open
            && !beacon.active()
        {
            if current != wanted {
                telemetry_samples = 4;
            }
            let retune =
                forced_retune || policy::receive_changed(current, wanted) || polled.tx_ended;
            if retune && relay_active {
                stop_relay(&mut radio, &mut secondary);
                relay_active = false;
                relay_session = None;
                wanted = operator_profile;
                checked(
                    &mut log,
                    app.set_crossband_probe(CrossbandProbe::Stopped),
                    0xE032,
                );
            }
            current = wanted;
            if retune {
                if radio.state() != RfState::Off {
                    checked(&mut log, radio.stop(), 0xE057);
                }
                current = wanted;
                last_signal = None;
                if let Some(rx) = current.and_then(|p| p.receive) {
                    let calibration = match app.calibration_view() {
                        Some(c) => c,
                        None => stop(0xE050),
                    };
                    // Validate before entering the executor; unsupported numeric
                    // records show unavailable, never continue the previous RF.
                    if uv98_rf_sequence::receive(&calibration, rx).is_ok() {
                        checked(&mut log, radio.receive(&calibration, rx, now), 0xE058);
                    }
                }
            }
            let requested_tx = update
                .ptt_slot
                .and_then(|slot| profile_for_slot(&app, &settings, slot).ok());
            if LIVE_TX
                && app.status().view.screen != Screen::Crossband
                && requested_tx.is_some_and(|(target, p)| tx_permitted(&app, target, p, raw_ptt))
                && ptt_guard.allow_edge()
                && update.ptt_tx_requested
                && (upper ^ lower)
                && (ptt::side_idle(adc.raw) || lower)
            {
                if let (Some((target, tx_profile)), Some(slot)) = (requested_tx, update.ptt_slot) {
                    let tx = tx_profile.transmit.unwrap_or_else(|| stop(0xE059));
                    let calibration = match app.calibration_view() {
                        Some(c) => c,
                        None => stop(0xE050),
                    };
                    if uv98_rf_sequence::transmit(&calibration, tx).is_ok()
                        && ptt_guard.arm(raw_ptt)
                    {
                        checked(&mut log, radio.stop(), 0xE057);
                        checked(
                            &mut log,
                            radio.transmit_with_mdc(
                                &calibration,
                                tx,
                                now,
                                true,
                                signaling::mdc_config(target, slot),
                            ),
                            0xE059,
                        );
                        tx_slot = Some(slot);
                        tx_started_at = Some(now);
                        tx_authorization = tx_policy::authorize_target(
                            &app.status().view,
                            &target,
                            tx_profile,
                            raw_ptt,
                        )
                        .ok();
                        current = Some(tx_profile);
                        last_signal = None;
                    }
                }
            }
        }
        if update.ptt_tx_requested && !radio.tx_active_or_starting() {
            checked(&mut log, app.reject_tx(), 0xE032);
        }
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        if radio.state() == RfState::Receiving
            && (!scope.active() || scope.listening())
            && now.wrapping_sub(signal_at) >= 1
        {
            if let Some(p) = current {
                last_signal = Some(checked(
                    &mut log,
                    if p.audio.modulation == Modulation::Fm
                        && !scope.active() && !tuner_open && !crossband_open
                    {
                        radio.observe_dtmf(p.audio, p.receive.map(|r| r.tone).unwrap_or(0))
                    } else {
                        radio.observe(p.audio, p.receive.map(|r| r.tone).unwrap_or(0))
                    },
                    0xE05A,
                ));
                signal_at = now;
            }
        }
        let mut watch_rx = None;
        if let (Some((slot, receive)), Some(watch), Some(signal)) =
            (watch_receive, dual_watch.as_mut(), last_signal)
        {
            if current == Some(receive)
                && radio.state() == RfState::Receiving
                && now.wrapping_sub(signal_at) < 100
            {
                watch.observe(
                    now,
                    receive.receive.is_some_and(|rx| rx.tone != 0),
                    signal.audio_open,
                );
                watch_rx = signal.audio_open.then_some(slot);
            }
        }
        let view = app.status().view;
        if view.screen == Screen::Crossband {
            if upper || lower {
                relay_session = None;
                checked(
                    &mut log,
                    app.set_crossband_probe(CrossbandProbe::Stopped),
                    0xE032,
                );
            } else if !LIVE_TX {
                if view.crossband_probe == CrossbandProbe::Starting
                    && radio.state() == RfState::Receiving
                    && !scanner.active()
                    && !scope.active()
                {
                    let (peer, channel) = app.peer_radio_selection();
                    let i = usize::from(peer.active_slot == Slot::B);
                    let receive = radio_settings(&app)
                        .and_then(|cfg| policy::profile(peer, channel, cfg, settings[i]).ok())
                        .and_then(|p| p.receive)
                        .filter(|r| r.modulation == Modulation::Fm && r.level > 0);
                    let status = if let (Some(primary), Some(rx)) =
                        (current.and_then(|p| p.receive), receive)
                    {
                        let calibration = app.calibration_view().unwrap_or_else(|| stop(0xE050));
                        match uv98_rf_sequence::secondary_receive(&calibration, rx) {
                            Ok(plan)
                                if view.active_slot == Slot::A
                                    && primary.frequency_10hz != rx.frequency_10hz =>
                            {
                                let words = checked(
                                    &mut log,
                                    uv98_rf_sequence::secondary_frequency(
                                        &calibration,
                                        rx.frequency_10hz,
                                    ),
                                    0xE065,
                                );
                                if matches!(
                                    probe_secondary_frequency(&mut secondary, words),
                                    Ok(true)
                                ) && matches!(
                                    apply_secondary_plan(&mut secondary, &plan),
                                    Ok(true)
                                ) {
                                    second_rx = Some((
                                        rx,
                                        require(time.sample(DWT::cycle_count()), 0xE016),
                                    ));
                                    CrossbandProbe::Measuring {
                                        a_rx: primary.frequency_10hz,
                                        b_rx: rx.frequency_10hz,
                                        rssi: 0,
                                        status: 0,
                                        tone_match: (rx.tone != 0).then_some(false),
                                    }
                                } else {
                                    stop_secondary(&mut secondary);
                                    CrossbandProbe::BusError
                                }
                            }
                            _ => CrossbandProbe::InvalidProbeChannels,
                        }
                    } else {
                        CrossbandProbe::InvalidProbeChannels
                    };
                    checked(&mut log, app.set_crossband_probe(status), 0xE032);
                } else if let Some((rx, sampled_at)) = second_rx.as_mut() {
                    if radio.state() != RfState::Receiving {
                        stop_secondary(&mut secondary);
                        second_rx = None;
                        checked(
                            &mut log,
                            app.set_crossband_probe(CrossbandProbe::Stopped),
                            0xE032,
                        );
                    } else if now.wrapping_sub(*sampled_at) >= 250 {
                        let mut status = checked(&mut log, secondary.read(0x0c), 0xE065);
                        if status & 1 != 0 {
                            checked(&mut log, secondary.read(0x0b), 0xE065);
                            checked(&mut log, secondary.write(0x02, 0), 0xE065);
                            status = checked(&mut log, secondary.read(0x0c), 0xE065);
                        }
                        let rssi = checked(&mut log, secondary.read(0x67), 0xE065) & 0x01ff;
                        let a_rx = current
                            .and_then(|p| p.receive)
                            .unwrap_or_else(|| stop(0xE065))
                            .frequency_10hz;
                        let tone_match =
                            (rx.tone != 0).then(|| uv98_rf_sequence::tone_matches(status, rx.tone));
                        event(
                            &mut log,
                            0x189,
                            [
                                a_rx,
                                rx.frequency_10hz,
                                u32::from(rssi) << 16 | u32::from(status),
                            ],
                        );
                        checked(
                            &mut log,
                            app.set_crossband_probe(CrossbandProbe::Measuring {
                                a_rx,
                                b_rx: rx.frequency_10hz,
                                rssi,
                                status,
                                tone_match,
                            }),
                            0xE032,
                        );
                        *sampled_at = now;
                    }
                }
            } else if view.crossband_probe == CrossbandProbe::Starting
                && !scanner.active()
                && !scope.active()
                && radio.initialized()
            {
                let status = if let Some(session) =
                    operator_profile.and_then(|p| crossband_session(&app, &settings, p))
                {
                    let calibration = app.calibration_view().unwrap_or_else(|| stop(0xE050));
                    let mut status = crossband_status(&session, false);
                    for slot in [Slot::A, Slot::B] {
                        let pair = session.pair(slot);
                        let rx = pair.receive().frequency_10hz;
                        let tx = pair.transmit().frequency_10hz;
                        if LIVE_RELAY
                            && uv98_rf_sequence::relay_transmit(&calibration, pair.transmit())
                                .is_err()
                        {
                            status = CrossbandProbe::InvalidPair;
                            break;
                        }
                        let Ok(words) = uv98_rf_sequence::secondary_frequency(&calibration, tx)
                        else {
                            status = CrossbandProbe::InvalidPair;
                            break;
                        };
                        let ok =
                            matches!(probe_secondary_frequency(&mut secondary, words), Ok(true));
                        event(&mut log, 0x187, [rx, tx, u32::from(ok)]);
                        if !ok {
                            status = CrossbandProbe::BusError;
                            break;
                        }
                    }
                    if matches!(status, CrossbandProbe::Ready { .. }) {
                        relay_session = Some(session);
                    }
                    status
                } else {
                    CrossbandProbe::InvalidPair
                };
                checked(&mut log, app.set_crossband_probe(status), 0xE032);
            } else if let Some(session) = relay_session.as_mut() {
                let sample = last_signal
                    .filter(|_| {
                        radio.state() == RfState::Receiving && now.wrapping_sub(signal_at) < 100
                    })
                    .map(|signal| relay_carrier(signal));
                let action = session.tick(now, sample);
                if LIVE_RELAY {
                    match action {
                        crossband::Action::Start(request) => {
                            let calibration =
                                app.calibration_view().unwrap_or_else(|| stop(0xE050));
                            let plan = checked(
                                &mut log,
                                uv98_rf_sequence::relay_transmit(&calibration, request),
                                0xE065,
                            );
                            if !matches!(
                                apply_secondary_plan(&mut secondary, &plan.setup),
                                Ok(true)
                            ) {
                                stop(0xE065);
                            }
                            let rx_profile = wanted.unwrap_or_else(|| stop(0xE065));
                            let fresh = checked(
                                &mut log,
                                radio.observe(
                                    rx_profile.audio,
                                    rx_profile.receive.map(|r| r.tone).unwrap_or(0),
                                ),
                                0xE065,
                            );
                            let rechecked_at = require(time.sample(DWT::cycle_count()), 0xE016);
                            last_signal = Some(fresh);
                            signal_at = rechecked_at;
                            let carrier = relay_carrier(fresh);
                            if session.tick(rechecked_at, Some(carrier)) == crossband::Action::Stop
                            {
                                stop_relay(&mut radio, &mut secondary);
                            } else {
                                checked(&mut log, radio.relay_route(Some(plan.route)), 0xE065);
                                if !matches!(
                                    write_relay_register(&mut secondary, 0x33, 0x0010),
                                    Ok(true)
                                ) || !matches!(
                                    write_relay_register(&mut secondary, 0x36, plan.power),
                                    Ok(true)
                                ) {
                                    stop(0xE065);
                                }
                                relay_active = true;
                            }
                        }
                        crossband::Action::Stop if relay_active => {
                            stop_relay(&mut radio, &mut secondary);
                            relay_active = false;
                        }
                        _ => {}
                    }
                }
                if action != crossband::Action::None {
                    let carrier = if LIVE_RELAY {
                        relay_active
                    } else {
                        matches!(action, crossband::Action::Start(_))
                    };
                    let pair = session.pair(session.slot());
                    event(
                        &mut log,
                        0x188,
                        [
                            pair.receive().frequency_10hz,
                            pair.transmit().frequency_10hz,
                            u32::from(carrier),
                        ],
                    );
                    checked(
                        &mut log,
                        app.set_crossband_probe(crossband_status(session, carrier)),
                        0xE032,
                    );
                }
                if session.disarmed() {
                    relay_session = None;
                    checked(
                        &mut log,
                        app.set_crossband_probe(CrossbandProbe::Stopped),
                        0xE032,
                    );
                }
            }
        }
        // Optional developer snapshots. Normal RX observation, LCD meters,
        // PTT/TOT guards and fault records remain unconditional.
        if cfg!(feature = "verbose-runtime")
            && radio.state() == RfState::Receiving
            && (!scope.active() || scope.listening())
            && telemetry_samples > 0
            && now.wrapping_sub(telemetry_at) >= 500
            && !app.render_pending()
        {
            if let (Some(p), Some(signal)) = (current, last_signal) {
                let is_am = p.audio.modulation == Modulation::Am;
                let flags = u32::from(p.audio.modulation == Modulation::Am)
                    | (u32::from(p.audio.monitor) << 1)
                    | (u32::from(signal.audio_open) << 2)
                    | (u32::from(signal.am_gain_index) << 8);
                let (signal_records, signal_limit, register_records, register_limit) = if is_am {
                    (&mut am_signal_records, 16, &mut am_register_records, 4)
                } else {
                    (&mut fm_signal_records, 8, &mut fm_register_records, 2)
                };
                if *signal_records < signal_limit {
                    event(
                        &mut log,
                        0x174,
                        [
                            p.frequency,
                            (u32::from(signal.rssi) << 16) | u32::from(signal.status_0c),
                            flags,
                        ],
                    );
                    *signal_records += 1;
                }
                if telemetry_samples == 4 && *register_records < register_limit {
                    let registers = checked(&mut log, radio.receive_diagnostics(), 0xE05B);
                    event(
                        &mut log,
                        0x175,
                        [
                            (u32::from(registers.gain_selection) << 16) | u32::from(registers.gain),
                            (u32::from(registers.af_mode) << 16) | u32::from(registers.volume),
                            u32::from(registers.status01),
                        ],
                    );
                    *register_records += 1;
                }
                telemetry_samples -= 1;
                telemetry_at = now;
            }
        }
        if scanner.active() && !scope.active() {
            let now = require(time.sample(DWT::cycle_count()), 0xE016);
            if let Some(ticket) = scanner.status().ticket {
                match scanner.tune_ack(ticket, now, radio.state()) {
                    Ok(true) => {
                        checked(&mut log, app.acknowledge_scan_started(), 0xE032);
                    }
                    Ok(false) => {}
                    Err(_) => {
                        scanner.cancel();
                        checked(&mut log, app.cancel_scan(), 0xE032);
                        checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
                        checked(
                            &mut log,
                            app.reject_radio_request(Request::StartScan),
                            0xE032,
                        );
                    }
                }
            }
            match scanner.tick(now, last_signal.is_some_and(|s| s.audio_open)) {
                Ok(scan::Action::None) => {}
                Ok(action) => {
                    if checked(&mut log, scan_action(&mut app, action), 0xE032) {
                        checked(&mut log, radio.stop(), 0xE057);
                        current = None;
                        last_signal = None;
                    } else {
                        scanner.cancel();
                    }
                }
                Err(_) => {
                    scanner.cancel();
                    checked(&mut log, app.cancel_scan(), 0xE032);
                    checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
                    checked(
                        &mut log,
                        app.reject_radio_request(Request::StartScan),
                        0xE032,
                    );
                }
            }
        }
        checked(&mut log, radio.synchronize_indicators(), 0xE05C);
        let active_tx = if radio.tx_active_or_starting() {
            tx_authorization
        } else {
            None
        };
        let mut report = RadioView::OFF;
        report.tx_slot = if radio.state() == RfState::Transmitting {
            tx_slot
        } else {
            None
        };
        report.tx_trial = cfg!(feature = "upper-exo-tx-trial");
        report.state = match radio.state() {
            RfState::Off if radio.initialized() => RadioState::Unavailable,
            RfState::Off | RfState::Configuring | RfState::Sweeping => RadioState::Starting,
            RfState::Receiving => RadioState::Receiving,
            RfState::Transmitting => RadioState::Transmitting,
            RfState::Failed => RadioState::Failed,
        };
        let displayed_profile = if watch_receive.is_some() {
            operator_profile
        } else {
            current
        };
        if let Some(p) = displayed_profile {
            report.power = Some(p.power);
            report.monitor = p.audio.monitor;
            report.modulation = if p.audio.modulation == Modulation::Fm {
                UiModulation::Fm
            } else {
                UiModulation::Am
            };
            report.route = match active_tx.map(|a| a.tx_route).unwrap_or(p.route) {
                1 | 2 => Some(RadioRoute::Hf),
                3 => Some(RadioRoute::Vhf),
                4 => Some(RadioRoute::Uhf),
                5 => Some(RadioRoute::Uhf800),
                _ => None,
            };
        }
        let receive = current.and_then(|profile| profile.receive);
        if receive != meter_receive || radio.state() != RfState::Receiving {
            meter_floor = uv98_handheld_ui::meter::NoiseFloor::default();
            meter_receive = receive;
        }
        if radio.state() == RfState::Receiving {
            let previous = app.radio_view();
            let quiet = receive.is_some_and(|rx| rx.level != 0)
                && last_signal.is_some_and(|signal| !signal.squelch_open);
            if now.wrapping_sub(ui_rssi_at) >= 100 || previous.rssi.is_none() {
                report.rssi = last_signal.map(|s| s.rssi);
                if let Some(raw) = report.rssi.filter(|_| quiet) {
                    meter_floor.observe_quiet(raw);
                }
                ui_rssi_at = now;
            } else {
                report.rssi = previous.rssi;
            }
            report.meter_floor = meter_floor.raw();
            if quiet && !report.monitor {
                // With the physical squelch closed, avoid showing noise as
                // a station. Monitor still shows relative RSSI above floor.
                report.meter_floor = report.meter_floor.max(report.rssi.unwrap_or(0));
            }
            report.squelch_open = last_signal.is_some_and(|s| s.audio_open);
            let carrier = now.wrapping_sub(signal_at) < 100
                && last_signal.is_some_and(|s| s.audio_open && s.status_0c & 2 != 0);
            report.rx_slot = if !carrier {
                None
            } else if watch_receive.is_some() {
                watch_rx
            } else if !scope.active() && !tuner_open && !crossband_open
                && current.is_some() && current == operator_profile
            {
                Some(view.active_slot)
            } else {
                None
            };
            if !scope.active() && !tuner_open && !crossband_open {
                let slot = watch_receive.map(|(slot, _)| slot).unwrap_or(view.active_slot);
                report.dtmf = uv98_handheld_ui::render::DtmfReception::new(
                    slot, radio.received_dtmf(now));
            }
            if watch_receive.is_some() && watch_rx.is_none() {
                report.rssi = None;
                report.meter_floor = 0;
            }
        }
        if report.state != app.radio_view().state {
            event(
                &mut log,
                0x170,
                [
                    report.state as u32,
                    active_tx
                        .map(|a| a.transmit.frequency_10hz)
                        .or(current.map(|p| p.frequency))
                        .unwrap_or(0),
                    current.map(|p| p.power as u32).unwrap_or(u32::MAX),
                ],
            );
        }
        checked(&mut log, app.set_radio_view(report), 0xE032);
        // Three bounded ADC conversions; the side-key channel is restored
        // before returning. Physical release/TOT work has already run this loop.
        if now.wrapping_sub(battery_at) >= 250
            && radio.state() != RfState::Configuring
            && !beacon.active()
        {
            let raw_battery = checked(&mut log, side.sample_battery(), 0xE055);
            checked(
                &mut log,
                app.set_battery_voltage(battery.observe(raw_battery)),
                0xE032,
            );
            battery_at = now;
        }
        if raw.is_some_and(|(matrix, ptt)| matrix != 0 || ptt) || adc_code != 0xff {
            lights.activity(now);
        }
        let (display_light, key_light) = lights.levels(now);
        checked(
            &mut log,
            radio.set_backlights(display_light, key_light),
            0xE05D,
        );
        let render_start = DWT::cycle_count();
        if cfg!(feature = "verbose-runtime") && app.render_pending() && redraw_since.is_none() {
            redraw_since = Some(now);
        }
        // Keep physical PTT/TOT polling short during TX and while a PTT is
        // down. The cap covers every window, including narrow outline edges.
        // Cycle admission does not interrupt an in-flight transfer.
        let (row_budget, admission_cycles) = if beacon.active() {
            (1, LCD_BATCH_ADMISSION_CYCLES)
        } else if radio.tx_active_or_starting() || relay_active || upper || lower {
            (2, LCD_BATCH_ADMISSION_CYCLES)
        } else if radio.state() == RfState::Configuring {
            // Finish RX restoration before spending more foreground time on LCD.
            (0, LCD_BATCH_ADMISSION_CYCLES)
        } else if scope.active()
            || current.is_some_and(|profile| profile.audio.modulation == Modulation::Am)
        {
            (4, LCD_BATCH_ADMISSION_CYCLES)
        } else if raw.is_some() && ptt::side_idle(adc.raw) {
            // A longer RX-only batch amortizes the keypad and RF work. Each
            // LCD window is still bounded. Either physical PTT source keeps
            // its short budget as soon as it is observed.
            (16, LCD_BATCH_ADMISSION_CYCLES * 3)
        } else {
            (6, LCD_BATCH_ADMISSION_CYCLES)
        };
        let drawn = checked(
            &mut log,
            app.render_batch(row_budget, || {
                DWT::cycle_count().wrapping_sub(render_start) < admission_cycles
            }),
            0xE032,
        );
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        if cfg!(feature = "verbose-runtime") {
            max_render_cycles =
                max_render_cycles.max(DWT::cycle_count().wrapping_sub(render_start));
            render_rows += u32::from(drawn.rows);
            max_loop_cycles = max_loop_cycles.max(DWT::cycle_count().wrapping_sub(loop_started));
            loop_count = loop_count.saturating_add(1);
            active_loop_us =
                active_loop_us.saturating_add(DWT::cycle_count().wrapping_sub(loop_started) / 48);
            if let Some(start) = redraw_since {
                // Oldest outstanding dirty pass, possibly including coalesced
                // telemetry. This is backlog duration, not key-to-photon latency.
                max_redraw_ms = max_redraw_ms.max(now.wrapping_sub(start));
                if !drawn.pending {
                    redraw_since = None;
                }
            }
        }
        // At most one optional developer frame in a quiet loop. Fault frames still
        // take the direct checked/stop path irrespective of these conditions.
        if cfg!(feature = "verbose-runtime")
            && drawn.rows == 0
            && !drawn.pending
            && raw == Some((0, false))
            && ptt::side_idle(adc.raw)
            && !radio.tx_active_or_starting()
            && !relay_active
            && !beacon.active()
        {
            if let Some(details) = throughput_log.take() {
                event(&mut log, 0x176, details);
            } else if cfg!(feature = "verbose-runtime") && ui_log_sent < ui_log_len {
                event(&mut log, 0x132, ui_log[ui_log_sent]);
                ui_log_sent += 1;
            } else if now.wrapping_sub(heartbeat_at) >= 2000 {
                throughput_log = Some([loop_count, active_loop_us, max_redraw_ms]);
                event(
                    &mut log,
                    0x172,
                    [max_loop_cycles / 48, max_render_cycles / 48, render_rows],
                );
                max_loop_cycles = 0;
                max_render_cycles = 0;
                render_rows = 0;
                loop_count = 0;
                active_loop_us = 0;
                max_redraw_ms = 0;
                heartbeat_at = now;
            }
        }
        // Debounce, AGC and scan deadlines use the monotonic clock, not loop
        // counts. A fixed 1ms sleep previously delayed every UI batch.
        if !drawn.pending {
            idle.us(100);
        }
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    stop(0xE0FF)
}

// Faults/NMI can arrive before .bss initialization as well as at runtime.
// Both entry points stay entirely in assembly. The runtime linker aliases
// unimplemented exceptions (including NonMaskableInt) to DefaultHandler.
core::arch::global_asm!(
    ".syntax unified",
    ".thumb",
    ".section .text.DiagnosticExceptions, \"ax\", %progbits",
    ".global HardFault",
    ".type HardFault, %function",
    ".thumb_func",
    "HardFault:",
    "b __diag_terminal_fail_stop",
    ".size HardFault, . - HardFault",
    ".global DefaultHandler",
    ".type DefaultHandler, %function",
    ".thumb_func",
    "DefaultHandler:",
    "b __diag_terminal_fail_stop",
    ".size DefaultHandler, . - DefaultHandler",
);
