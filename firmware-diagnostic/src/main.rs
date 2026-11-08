#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
//! Experimental executable for the related RT880/UV98 AT32F423 profile.
//! RX by default. Explicit features select the historical EXO trial or normal
//! FM channel/VFO TX. NOR remains read-only; lower physical PTT stays unmapped.
//! A successful register check does not establish physical board/RF safety.

use core::{fmt::Debug, num::NonZeroU32, panic::PanicInfo};
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use uv98_boot_log::error_text::ErrorText;
use uv98_boot_log::hardware::{RawMmio, TxLogger};
use uv98_boot_log::{Frame, Logger, RegisterIo};
use uv98_clock_profile::hardware::CycleDelay;
use uv98_diag_startup::{hardware as startup, CycleMillis, CORE_NOMINAL_HZ, CORE_UPPER_HZ};
use uv98_display::DisplayBus;
use uv98_handheld_app::{App, InputSample, SpectrumSnapshot, PRESERVED_CALIBRATION_SHA256};
use uv98_handheld_ui::{
    controller::{Request, ScanState, Slot, STEP_10HZ},
    render::{Modulation as UiModulation, RadioRoute, RadioState, RadioView, SpectrumState},
};
use uv98_nor_read::Bus;
use uv98_radio_runtime::{
    lighting, policy, ptt, scan, scope, tx_policy, tx_trial, Runtime, State as RfState,
};
use uv98_rf_sequence::Modulation;
use uv98_spectrum::compact as spectrum;

// Default remains RX-only; each published image declares one exact profile.
#[cfg(all(feature = "upper-exo-tx-trial", feature = "channel-tx"))]
compile_error!("Select exactly one TX profile");
const LIVE_TX: bool = cfg!(any(feature = "upper-exo-tx-trial", feature = "channel-tx"));
const LOWER_PTT_CODE: Option<u8> = None;

#[used]
#[no_mangle]
pub static UV98_RELEASE_POLICY: [u32; 4] = [
    0x108,
    LIVE_TX as u32,
    0xffff_ffff,
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
        Err(error) => {
            // SAFETY: terminal takeover; owned peripherals never resume.
            let _ = unsafe { startup::terminal_radio_off_requests() };
            let text = ErrorText::format(&error);
            event(log, fault, text.metadata());
            for frame in text.frames() {
                log.record(frame);
            }
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
        self.us(1);
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
    profile: policy::Profile,
    input: ptt::Input,
) -> bool {
    if cfg!(feature = "channel-tx") {
        tx_policy::authorize(app.status().view, profile, input).is_ok()
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

fn profile<N: Bus, L: DisplayBus, D: uv98_display::Delay>(
    app: &App<N, L, D>,
    settings: &[policy::Overrides; 2],
) -> Result<policy::Profile, policy::Error> {
    let cfg = app.radio_config().ok_or(policy::Error::Settings)?;
    let view = app.status().view;
    let index = if view.active_slot == Slot::A { 0 } else { 1 };
    policy::profile(
        view,
        app.selected_channel(),
        policy::Settings {
            squelch: cfg.squelch,
            mic_gain: cfg.mic_gain,
            fm_volume: cfg.fm_volume,
            am_volume: cfg.am_volume,
            squelch_delay: cfg.squelch_delay,
        },
        settings[index],
    )
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
        PRESERVED_CALIBRATION_SHA256,
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
    let _reserved = radio_pins.reserved;
    let mut side = checked(
        &mut log,
        unsafe {
            uv98_sidekeys_at32::hardware::SideAdc::from_prepared(
                radio_pins.side_adc,
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
    let mut spectrum_published_at = now;
    let mut tx_slot = None;
    let mut tx_started_at = None;
    let mut tx_authorization: Option<tx_policy::Authorization> = None;
    let mut ptt_guard = ptt::Guard::new();
    let mut last_signal = None;
    let mut signal_at = now;
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
    let mut max_loop_cycles = 0;
    let mut max_render_cycles = 0;
    let mut render_rows = 0;
    let mut loop_count = 0u32;
    let mut active_loop_us = 0u32;
    let mut redraw_since = None;
    let mut max_redraw_ms = 0u32;
    loop {
        let loop_started = DWT::cycle_count();
        feed();
        let adc = checked(&mut log, side.sample(), 0xE055);
        let adc_code = uv98_keys::stock_auxiliary_code(adc.raw);
        let key_sample = keys.scan_idle_or_full();
        let raw = key_sample
            .as_ref()
            .ok()
            .map(|s| (s.matrix_raw(), s.ptt_pressed()));
        let upper = raw.is_some_and(|(_, p)| p);
        let lower = LOWER_PTT_CODE == Some(adc_code);
        let raw_ptt = ptt::Input {
            upper,
            lower,
            clean: raw.is_some_and(|(m, _)| m == 0) && (adc_code == 0xff || lower),
        };
        let physical_source_held = ptt_guard.observe(raw_ptt, scanner.active() || scope.active());
        let view = app.status().view;
        let guarded_profile = if cfg!(feature = "channel-tx") {
            profile(&app, &settings).ok()
        } else {
            current
        };
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        // Stop on raw release/chord/source handoff before UI/NOR work.
        let physical_held = LIVE_TX
            && physical_source_held
            && tx_slot.is_some()
            && tx_slot == view.ptt_slot
            && tx_started_at.is_some_and(|start| tx_hold_within_limit(start, now))
            && !view.locked
            && guarded_profile.is_some_and(|p| {
                tx_permitted(&app, p, raw_ptt)
                    && (!cfg!(feature = "channel-tx")
                        || tx_authorization.is_some_and(|a| a.still_held(view, p, raw_ptt)))
            });
        let mut polled = checked(&mut log, radio.tick(now, physical_held), 0xE056);
        // Drain a small amount of configuration work without LCD/NOR/UART or
        // idle work between register operations. The 2ms admission budget is
        // checked before each operation; a single synchronous operation may
        // finish later. This is not a measured RF stop-latency claim.
        let burst_started = DWT::cycle_count();
        for _ in 0..7 {
            let now = require(time.sample(DWT::cycle_count()), 0xE016);
            if !radio.configuration_step_ready(now)
                || DWT::cycle_count().wrapping_sub(burst_started) >= 96_000
                || polled.tx_ended
            {
                break;
            }
            feed();
            let held = if radio.tx_active_or_starting() {
                // This faster aggregate matrix scan only sustains an already
                // armed upper PTT. It never creates UI events or arms TX.
                let fresh_adc = checked(&mut log, side.sample(), 0xE055);
                let aux = uv98_keys::stock_auxiliary_code(fresh_adc.raw);
                let guard = keys.guard_scan();
                let input = ptt::Input {
                    upper: guard.as_ref().is_ok_and(|s| s.ptt_pressed()),
                    lower: LOWER_PTT_CODE == Some(aux),
                    clean: guard.as_ref().is_ok_and(|s| s.matrix_clear()) && aux == 0xff,
                };
                let same_source = ptt_guard.observe(input, scanner.active() || scope.active());
                let now = require(time.sample(DWT::cycle_count()), 0xE016);
                let guarded_profile = if cfg!(feature = "channel-tx") {
                    profile(&app, &settings).ok()
                } else {
                    current
                };
                LIVE_TX
                    && same_source
                    && tx_slot.is_some()
                    && tx_slot == view.ptt_slot
                    && tx_started_at.is_some_and(|start| tx_hold_within_limit(start, now))
                    && !view.locked
                    && guarded_profile.is_some_and(|p| {
                        tx_permitted(&app, p, input)
                            && (!cfg!(feature = "channel-tx")
                                || tx_authorization.is_some_and(|a| a.still_held(view, p, input)))
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
        if !radio.tx_active_or_starting() && adc_previous != Some((adc_code, upper)) {
            event(
                &mut log,
                0x171,
                [u32::from(adc.raw), u32::from(adc_code), u32::from(upper)],
            );
            adc_previous = Some((adc_code, upper));
        }
        let sample = match raw {
            Some((matrix, ptt_pressed)) if adc_code == 0xff || lower => InputSample::WithAux {
                matrix,
                ptt_pressed,
                aux: if lower {
                    Some(uv98_keys::Key::Aux16)
                } else {
                    None
                },
            },
            _ => InputSample::Unstable,
        };
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        // Any physical side-key activity cancels a sweep. PTT cannot turn this
        // cancellation into transmission; the guard requires a later release
        // and a new eligible edge on the normal channel screen.
        scope.event(now, spectrum::Event::Ptt(upper || adc_code != 0xff));
        let update = checked(&mut log, app.sample(now, sample), 0xE032);
        if let Some(key) = update.pressed {
            let v = app.status().view;
            event(
                &mut log,
                0x132,
                [
                    uv98_handheld_app::key_code(key),
                    v.screen as u32,
                    v.channel.map(u32::from).unwrap_or(u32::MAX),
                ],
            );
        }
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
            scope.event(now, spectrum::Event::Stop);
            if scanner.active() {
                action = scanner.cancel();
                checked(&mut log, app.cancel_scan(), 0xE032);
                checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
            }
        }
        if let Some(request) = update.request {
            if matches!(
                request,
                Request::SetPower(_) | Request::SetMonitor(_) | Request::SetModulation { .. }
            ) && scanner.active()
            {
                scanner.cancel();
                checked(&mut log, app.cancel_scan(), 0xE032);
                checked(&mut log, app.acknowledge_scan_stopped(), 0xE032);
            }
            match request {
                Request::StartSpectrum => {
                    let calibration = match app.calibration_view() {
                        Some(c) => c,
                        None => stop(0xE050),
                    };
                    let rx = profile(&app, &settings).ok().and_then(|p| p.receive);
                    let step = STEP_10HZ
                        .get(usize::from(app.status().view.step_index))
                        .copied();
                    let started = match (rx, step) {
                        (Some(rx), Some(step)) => {
                            scope.start(&radio, &calibration, rx, step, now).is_ok()
                        }
                        _ => false,
                    };
                    if started {
                        ptt_guard.disarm();
                        last_signal = None;
                        checked(&mut log, app.clear_spectrum(), 0xE032);
                        spectrum_published_at = now.wrapping_sub(250);
                    } else {
                        checked(&mut log, app.reject_radio_request(request), 0xE032);
                    }
                }
                Request::StopSpectrum => scope.event(now, spectrum::Event::Stop),
                Request::AcceptSpectrum { bin } => {
                    // Unknown bins have no measurement and cannot be selected.
                    if scope.active() && scope.plot().valid(usize::from(bin)) {
                        scope.event(now, spectrum::Event::Accept { index: bin });
                    } else {
                        checked(&mut log, app.reject_radio_request(request), 0xE032);
                    }
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
                || adc_code != 0xff
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
            let completion = checked(
                &mut log,
                scope.step(&mut radio, &calibration, || {
                    require(time.sample(DWT::cycle_count()), 0xE016)
                }),
                0xE05E,
            );
            if let scope::Completion::Restored {
                selected_frequency,
                modulation,
                ..
            } = completion
            {
                if let Some(frequency) = selected_frequency {
                    settings[index].modulation = Some(modulation);
                    checked(&mut log, app.accept_spectrum_frequency(frequency), 0xE032);
                }
                last_signal = None;
                telemetry_samples = 4;
                // Reconcile the complete active-slot policy after restoration.
                current = None;
            }
        }
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        if now.wrapping_sub(spectrum_published_at) >= 250 {
            if let Some(range) = scope.range() {
                let state = match scope.state() {
                    spectrum::State::Preparing => SpectrumState::Preparing,
                    spectrum::State::Tuning
                    | spectrum::State::Settling
                    | spectrum::State::Reading
                    | spectrum::State::SweepComplete => SpectrumState::Sweeping,
                    spectrum::State::Restoring(_) => SpectrumState::Restoring,
                    spectrum::State::Failed(_) => SpectrumState::Failed,
                    spectrum::State::Idle | spectrum::State::Restored(_) => SpectrumState::Stopped,
                };
                let snapshot = SpectrumSnapshot {
                    state,
                    start_10hz: range.start_10hz(),
                    step_10hz: range.step_10hz(),
                    len: range.len() as u8,
                    plot: *scope.plot(),
                };
                if checked(&mut log, app.set_spectrum(snapshot), 0xE032) {
                    spectrum_published_at = now;
                }
            }
        }
        let wanted = profile(&app, &settings).ok();
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
        if radio.initialized() && !radio.tx_active_or_starting() && !scope.active() {
            if current != wanted {
                telemetry_samples = 4;
            }
            let retune =
                forced_retune || policy::receive_changed(current, wanted) || polled.tx_ended;
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
            if LIVE_TX
                && wanted.is_some_and(|p| tx_permitted(&app, p, raw_ptt))
                && ptt_guard.allow_edge()
                && update.ptt_tx_requested
                && (upper ^ lower)
                && (adc_code == 0xff || lower)
            {
                if let (Some(tx), Some(slot)) = (wanted.and_then(|p| p.transmit), update.ptt_slot) {
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
                            radio.transmit(&calibration, tx, now, true),
                            0xE059,
                        );
                        tx_slot = Some(slot);
                        tx_started_at = Some(now);
                        tx_authorization = wanted
                            .and_then(|p| tx_policy::authorize(app.status().view, p, raw_ptt).ok());
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
            && !scope.active()
            && now.wrapping_sub(signal_at) >= 1
        {
            if let Some(p) = current {
                last_signal = Some(checked(
                    &mut log,
                    radio.observe(p.audio, p.receive.map(|r| r.tone).unwrap_or(0)),
                    0xE05A,
                ));
                signal_at = now;
            }
        }
        if radio.state() == RfState::Receiving
            && !scope.active()
            && telemetry_samples > 0
            && now.wrapping_sub(telemetry_at) >= 500
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
        if scanner.active() {
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
        report.tx_trial = cfg!(feature = "upper-exo-tx-trial");
        report.state = match radio.state() {
            RfState::Off if radio.initialized() => RadioState::Unavailable,
            RfState::Off | RfState::Configuring | RfState::Sweeping => RadioState::Starting,
            RfState::Receiving => RadioState::Receiving,
            RfState::Transmitting => RadioState::Transmitting,
            RfState::Failed => RadioState::Failed,
        };
        if let Some(p) = current {
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
        if radio.state() == RfState::Receiving {
            if now.wrapping_sub(ui_rssi_at) >= 200 || app.radio_view().rssi.is_none() {
                report.rssi = last_signal.map(|s| s.rssi);
                ui_rssi_at = now;
            } else {
                report.rssi = app.radio_view().rssi;
            }
            report.squelch_open = last_signal.is_some_and(|s| s.audio_open);
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
        if app.render_pending() && redraw_since.is_none() {
            redraw_since = Some(now);
        }
        // Keep the foreground return to physical PTT polling short during the
        // TX trial. The bound is on row count; electrical latency is unmeasured.
        let row_budget = if radio.tx_active_or_starting() { 2 } else { 8 };
        let drawn = checked(&mut log, app.render_step(row_budget), 0xE032);
        max_render_cycles = max_render_cycles.max(DWT::cycle_count().wrapping_sub(render_start));
        render_rows += u32::from(drawn.rows);
        max_loop_cycles = max_loop_cycles.max(DWT::cycle_count().wrapping_sub(loop_started));
        let now = require(time.sample(DWT::cycle_count()), 0xE016);
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
        if now.wrapping_sub(heartbeat_at) >= 2000 && !radio.tx_active_or_starting() {
            event(
                &mut log,
                0x172,
                [max_loop_cycles / 48, max_render_cycles / 48, render_rows],
            );
            event(&mut log, 0x176, [loop_count, active_loop_us, max_redraw_ms]);
            max_loop_cycles = 0;
            max_render_cycles = 0;
            render_rows = 0;
            loop_count = 0;
            active_loop_us = 0;
            max_redraw_ms = 0;
            heartbeat_at = now;
        }
        // Debounce, AGC and scan deadlines use the monotonic clock, not loop
        // counts. A fixed 1ms sleep previously delayed every UI batch.
        idle.us(100);
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
