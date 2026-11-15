#![no_std]
#![forbid(unsafe_code)]
//! Immutable requests from hash-pinned RT880 1.16 Thumb, not hardware proof.
//! The extra mute/gate-off prefixes are new policy. Every RMW needs a fresh read.
//! Executors must enforce ownership, PTT, deadlines, fault shutdown and settling.
//! A relay setup describes chip B without enabling its PA. No storage writes, allocation, MMIO or transport.

use uv98_band_plan::{select_routes, SelectorMode};
use uv98_calibration::{Calibration, Filter, Table};
pub use uv98_receive::Width;
use uv98_stock_model::rf::{self, FilterField};
pub mod am_agc;
pub mod sweep;

pub const MAX_STEPS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    WriteA {
        register: u8,
        value: u16,
    },
    /// Read current value, then write `(current & keep_mask) | value`.
    UpdateA {
        register: u8,
        keep_mask: u16,
        value: u16,
    },
    DelayMs(u16),
    /// true requests PF8 LOW; false requests audio gate HIGH.
    Mute(bool),
    AntennaPf0(bool),
    /// Exact05354 behavior: MCU PC15/PB11 LOW, fresh A33 &FFF3, then
    /// route1 PC15HIGH;2 PC15/PB11HIGH;3 A33bit3;4/5 A33bit2;0 none.
    /// Write A33 last. Does not set PF0, PA36, or claim RF inactivity.
    TxGates(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Plan {
    steps: [Step; MAX_STEPS],
    len: usize,
}

impl Plan {
    const fn new() -> Self {
        Self {
            steps: [Step::DelayMs(0); MAX_STEPS],
            len: 0,
        }
    }
    pub fn operations(&self) -> &[Step] {
        &self.steps[..self.len]
    }
    fn push(&mut self, step: Step) {
        // Bounded internal compositions only; checked in maximal-path tests.
        assert!(self.len < MAX_STEPS);
        self.steps[self.len] = step;
        self.len += 1;
    }
    fn write(&mut self, register: u8, value: u16) {
        self.push(Step::WriteA { register, value });
    }
    fn update(&mut self, register: u8, keep_mask: u16, value: u16) {
        self.push(Step::UpdateA {
            register,
            keep_mask,
            value,
        });
    }
    fn append(&mut self, other: &Self) {
        for &step in other.operations() {
            self.push(step);
        }
    }
    fn filter(&mut self, field: FilterField, index: u8) {
        let words = rf::filter_registers(field, index);
        self.write(words.first.address, words.first.value);
        if let Some(word) = words.second {
            self.write(word.address, word.value);
        }
    }
    fn off_prefix(&mut self) {
        self.push(Step::Mute(true));
        self.push(Step::TxGates(0));
        self.write(0x36, 0x007f);
        self.write(0x30, 0);
        self.write(0x47, 0x6040);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Receive(uv98_receive::PlanError),
    InvalidTone(u16),
    UnsupportedToneMode(u8),
    InvalidOption(u8),
    InvalidPower(u8),
    UnsupportedFrequency(u32),
    CalibrationPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Modulation {
    Fm,
    Am,
}

/// Stock050EA..518E: FM invokes038AC(1), AM invokes038AC(7).
/// AM must use the separate CFG+B0/B1 volume pair; FM usesAE/AF.
pub const fn audio_mode_word(modulation: Modulation) -> u16 {
    match modulation {
        Modulation::Fm => 0x6140,
        Modulation::Am => 0x6740,
    }
}

/// Caller supplies the appropriate raw CFG gain/DAC pair and fresh full A01.
/// Inputs are not silently clamped; stock uses full raw bytes and wraps u16.
pub const fn audio_volume_word(gain: u8, dac: u8, register01: u16) -> u16 {
    let input = 0xb000u16
        .wrapping_add((gain as u16) << 4)
        .wrapping_add(dac as u16);
    rf::volume_register_05768(input, register01)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RxRequest {
    pub frequency_10hz: u32,
    pub width: Width,
    pub modulation: Modulation,
    pub level: u8,
    pub tone: u16,
    /// channel[0x12] >> 5, ordinary analog modes0..3 only.
    pub tone_mode: u8,
    pub option_nibble: u8,
    /// Raw CFG+0xB2, reduced modulo11 exactly as stock04CB0.
    pub squelch_delay: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TxRequest {
    pub frequency_10hz: u32,
    pub width: Width,
    pub tone: u16,
    pub tone_mode: u8,
    pub option_nibble: u8,
    /// Channel+0x10 ordinary branch:0 low,1 average,2 high. No override.
    pub power: u8,
    /// Actual CFG+0xAD. Stock caller does not use band MIC calibration.
    pub mic_gain: u8,
}

const GAIN: [(u8, u16); 7] = [
    (0x13, 0x03be),
    (0x12, 0x037b),
    (0x11, 0x027b),
    (0x10, 0x007a),
    (0x14, 0x0019),
    (0x49, 0x2a38),
    (0x7b, 0x8420),
];

fn gain(plan: &mut Plan) {
    for (reg, value) in GAIN {
        plan.write(reg, value);
    }
}

/// Original prefix04DC0..04E62 and non-enabling parts of its tail.
/// Unlike full stock init, this leaves30=0/37=1D00 and PA/gates requested off.
/// Factory filter bytes are raw (no UI mask); unrelated general3E is preserved
/// in storage but stock init uses immediate A037, not that stored field.
pub fn cold_init(calibration: &Calibration<'_>) -> Plan {
    let mut plan = Plan::new();
    plan.off_prefix();
    plan.write(0x00, 0x8000);
    plan.push(Step::DelayMs(10));
    plan.write(0x00, 0);
    plan.write(0x37, 0x1d0f);
    gain(&mut plan);
    for (reg, value) in [
        (0x33, 0),
        (0x34, 0),
        (0x35, 0),
        (0x1e, 0x4c58),
        (0x1f, 0xa656),
        (0x3e, 0xa037),
        (0x7d, 0xe950),
        (0x26, 0x13a0),
    ] {
        plan.write(reg, value);
    }
    let general = calibration.general();
    for (field, stored) in [
        (FilterField::Rx3k, Filter::Rx3k),
        (FilterField::Rx300, Filter::Rx300),
        (FilterField::Tx3k, Filter::Tx3k),
        (FilterField::Tx300, Filter::Tx300),
    ] {
        plan.filter(field, general.filter_raw(stored));
    }
    // 05686(0); non-30 portion of05698(1,0);05218(0);038AC(0).
    plan.write(0x71, 0);
    plan.update(0x70, 0x00ff, 0x4000);
    option(&mut plan, 0);
    plan.write(0x47, 0x6040);
    plan.write(0x36, 0x007f);
    plan.write(0x30, 0);
    plan.write(0x37, 0x1d00);
    plan
}

fn validate_options(tone_mode: u8, option_nibble: u8) -> Result<(), Error> {
    if tone_mode > 3 {
        return Err(Error::UnsupportedToneMode(tone_mode));
    }
    if option_nibble > 15 {
        return Err(Error::InvalidOption(option_nibble));
    }
    Ok(())
}

/// Original09920/0994C: Golay polynomial0AE3 with exact stock bit reversals.
/// Alternate mode is selected by channel tone_mode1 or3, not DCS polarity.
pub fn dcs_word(code: u16, alternate: bool) -> Result<u32, Error> {
    if code > 0x1ff {
        return Err(Error::InvalidTone(code));
    }
    let input = if alternate {
        (u32::from(code) ^ 0x28) + 0xa00
    } else {
        u32::from(code) + 0x800
    };
    let reversed = input.reverse_bits() >> 20;
    let mut remainder = reversed << 11;
    for i in 0..12 {
        if remainder >> (22 - i) != 0 {
            remainder ^= 0xae3 << (11 - i);
        }
    }
    let full = remainder.wrapping_add(reversed << 11);
    let mut result = full.reverse_bits() >> 9;
    if alternate {
        result ^= 0x200;
    }
    Ok(result & 0x7fffff)
}

/// Exact4388 sequence including its called DCS encoder; rejects unknown raw types.
/// Tone deviation uses the stock immediate words; no unproved band-byte use.
pub fn tone(raw: u16, mode: u8) -> Result<Plan, Error> {
    validate_options(mode, 0)?;
    let mut plan = Plan::new();
    match raw {
        0 => plan.write(0x51, 0),
        0x1000..=0x1fff => {
            let word = ((u32::from(raw & 0xfff) * 413 / 200) & 0x1fff) as u16;
            plan.write(0x51, 0x9041);
            plan.write(0x07, word);
        }
        0x2000..=0x21ff | 0x3000..=0x31ff => {
            let word = dcs_word(raw & 0xfff, matches!(mode, 1 | 3))?;
            plan.write(
                0x51,
                if raw & 0xf000 == 0x3000 {
                    0xa02c
                } else {
                    0x802c
                },
            );
            plan.write(0x07, 0x0ad7);
            plan.write(0x08, (word & 0xfff) as u16);
            plan.write(0x08, 0x8000 + (word >> 12) as u16);
        }
        _ => return Err(Error::InvalidTone(raw)),
    }
    Ok(plan)
}

/// Original04444 status decision AFTER any pending IRQ service and fresh0C.
/// Type0/1 uses the CT loss bits12/13; every type respects loss bit11.
pub const fn tone_status(reg0c: u16, tone_raw: u16) -> u8 {
    if reg0c & 0x0800 != 0 || tone_raw >> 12 <= 1 && reg0c & 0x3000 != 0 {
        4
    } else if reg0c & 0x0400 != 0 {
        1
    } else if reg0c & 0x4000 != 0 {
        2
    } else if reg0c & 0x8000 != 0 {
        3
    } else {
        0
    }
}

/// FM acquisition policy076BA..76FA, without global monitor bypass.
/// Pending IRQbit0 is conservatively rejected until bounded ack+fresh read.
/// Analog-only IRQ ack prefix1D990: read0B,write02=0,then reread0C once.
/// This is not the omitted FSK FIFO service in the rest of1D990.
pub const fn tone_matches(reg0c: u16, tone_raw: u16) -> bool {
    if tone_raw == 0 {
        return true;
    }
    if reg0c & 1 != 0 {
        return false;
    }
    let kind = match tone_raw {
        0x1000..=0x1fff => 1,
        0x2000..=0x21ff => 2,
        0x3000..=0x31ff => 3,
        _ => return false,
    };
    tone_status(reg0c, tone_raw) == kind
}

fn option(plan: &mut Plan, option: u8) {
    // Stock05218: full16-bit truncation and fresh31 bit1 update.
    plan.write(0x71, (0x68dc_u32 + u32::from(option) * 1032) as u16);
    plan.update(0x31, 0xfffd, if option == 0 { 0 } else { 2 });
}

/// Ordinary FM/AM RX composition. Caller owns settling and AF volume/gate policy.
/// Both use the same tuning/AFC sequence; modulation selects AF47 when opening.
/// The stock AM software gain loop is not included in these immutable requests.
/// It accepts all stock selector routes; that is not a measured coverage claim.
pub fn receive(calibration: &Calibration<'_>, request: RxRequest) -> Result<Plan, Error> {
    validate_options(request.tone_mode, request.option_nibble)?;
    let tone = tone(request.tone, request.tone_mode)?;
    let rx = uv98_receive::Plan::primary(
        calibration,
        request.frequency_10hz,
        request.width,
        request.level,
    )
    .map_err(Error::Receive)?;
    let mut plan = Plan::new();
    plan.off_prefix();
    // Stock RX transition from TX waits20ms after gate/PA off.
    plan.push(Step::DelayMs(20));
    gain(&mut plan);
    for operation in &rx.operations()[..1] {
        if let uv98_receive::Operation::Write {
            register, value, ..
        } = *operation
        {
            plan.write(register, value);
        }
    }
    // Stock primary RX caller1F842..1F84A: width4228, reset04518,
    // then ordinary receive4BF0. Read A31 afresh; preserve unrelated bits.
    plan.update(0x31, 0xfff7, 0);
    let reset = rf::compander_reset_04518(0);
    plan.write(reset[1].address, reset[1].value);
    plan.write(reset[2].address, reset[2].value);
    for operation in &rx.operations()[1..3] {
        if let uv98_receive::Operation::Write {
            register, value, ..
        } = *operation
        {
            plan.write(register, value);
        }
    }
    plan.append(&tone);
    const DELAY_CODES: [u16; 11] = [20, 30, 40, 50, 60, 70, 100, 120, 160, 200, 255];
    let delay = DELAY_CODES[usize::from(request.squelch_delay % 11)];
    plan.write(0x4d, 0xa000 + delay);
    plan.write(0x4e, 0x6f00 + delay);
    for operation in &rx.operations()[3..5] {
        if let uv98_receive::Operation::Write {
            register, value, ..
        } = *operation
        {
            plan.write(register, value);
        }
    }
    plan.update(0x73, 0xffef, 0); // stock5274(1), channel modulation0/1
    option(&mut plan, request.option_nibble);
    let filter = if request.option_nibble != 0 {
        4
    } else {
        calibration.general().filter_raw(Filter::Rx3k)
    };
    plan.filter(FilterField::Rx3k, filter);
    plan.write(0x37, 0x1f0f);
    plan.push(Step::DelayMs(10));
    plan.write(0x30, 0x0200);
    plan.write(0x30, 0xbff1);
    for operation in &rx.operations()[5..] {
        match *operation {
            uv98_receive::Operation::PrimaryRoute { route } => {
                let mask = match route {
                    1 | 2 => 1,
                    3 => 0x10,
                    4 => 0x20,
                    5 => 2,
                    _ => 0,
                };
                plan.update(0x33, 0xffcc, mask);
            }
            uv98_receive::Operation::AntennaPf0 { high } => plan.push(Step::AntennaPf0(high)),
            _ => unreachable!(),
        }
    }
    Ok(plan)
}

/// Ordinary FM TX053F0..55C2 after new explicit gate/PA-off preparation.
/// Width is set before the stock body; this supports split-frequency channels.
/// RF-enable requests are final gates followed by selected calibration PA code.
/// Caller MUST enforce release-to-arm/PTT/TOT and independent fault shutdown.
pub fn transmit(calibration: &Calibration<'_>, request: TxRequest) -> Result<Plan, Error> {
    validate_options(request.tone_mode, request.option_nibble)?;
    if request.power > 2 {
        return Err(Error::InvalidPower(request.power));
    }
    let tone = tone(request.tone, request.tone_mode)?;
    let freq = uv98_receive::FrequencyPlan::new(
        calibration,
        uv98_receive::Chip::A,
        request.frequency_10hz,
    )
    .map_err(Error::Receive)?;
    let route = select_routes(request.frequency_10hz, SelectorMode::Primary)
        .ok_or(Error::UnsupportedFrequency(request.frequency_10hz))?
        .tx_code();
    let selected = freq.selection();
    let band = calibration
        .band(selected.band_index())
        .map_err(|_| Error::CalibrationPoint)?;
    let high = band
        .point_raw(Table::PowerHigh, selected.table_index())
        .ok_or(Error::CalibrationPoint)?;
    let low = band
        .point_raw(Table::PowerLow, selected.table_index())
        .ok_or(Error::CalibrationPoint)?;
    let power = rf::power_code_at_05552(request.power, high, low);
    let width_raw = u8::from(request.width == Width::Narrow);
    let deviation = rf::deviation_at_054cc(
        width_raw,
        request.option_nibble,
        band.wide().tx_deviation_raw(),
        band.narrow().tx_deviation_raw(),
    );
    let mut plan = Plan::new();
    plan.off_prefix();
    plan.write(
        0x43,
        if request.width == Width::Wide {
            0x3028
        } else {
            0x4048
        },
    );
    plan.update(0x33, 0xffcc, 0);
    plan.write(0x47, 0x6140); // ordinary stock38AC(1), external PF8 staysLOW
    for operation in freq.operations() {
        if let uv98_receive::Operation::Write {
            register, value, ..
        } = *operation
        {
            plan.write(register, value);
        }
    }
    plan.push(Step::AntennaPf0(route != 3 && route != 4));
    plan.append(&tone);
    option(&mut plan, request.option_nibble);
    let filter = if request.option_nibble != 0 {
        4
    } else {
        calibration.general().filter_raw(Filter::Tx3k)
    };
    plan.filter(FilterField::Tx3k, filter);
    plan.write(0x7d, rf::mic_register_04e90(u32::from(request.mic_gain)));
    plan.write(0x40, deviation);
    plan.write(0x37, 0x1d0f);
    plan.push(Step::DelayMs(10));
    plan.write(0x52, 0x028f);
    plan.write(0x30, 0x0200);
    plan.write(0x30, 0xc1fe);
    plan.push(Step::TxGates(route));
    plan.write(0x36, rf::power_register_04f0c(u32::from(power)));
    Ok(plan)
}

/// B frequency words while the chip remains asleep. No PLL or PA enable.
pub fn secondary_frequency(
    calibration: &Calibration<'_>,
    frequency_10hz: u32,
) -> Result<[u16; 2], Error> {
    let plan = uv98_receive::FrequencyPlan::new(calibration, uv98_receive::Chip::B, frequency_10hz)
        .map_err(Error::Receive)?;
    let words = plan.frequency_words();
    Ok([words.reg38, words.reg39])
}

/// Prepare the secondary RF chip for a diagnostic receive measurement.
pub fn secondary_receive(calibration: &Calibration<'_>, request: RxRequest) -> Result<Plan, Error> {
    if !matches!(request.frequency_10hz, 13_600_000..=17_400_000 | 40_000_000..=48_000_000) {
        return Err(Error::UnsupportedFrequency(request.frequency_10hz));
    }
    let [reg38, reg39] = secondary_frequency(calibration, request.frequency_10hz)?;
    let primary = receive(calibration, request)?;
    let mut plan = Plan::new();
    plan.write(0x00, 0x8000);
    plan.push(Step::DelayMs(10));
    plan.write(0x00, 0);
    plan.write(0x37, 0x1d0f);
    plan.write(0x33, 0);
    plan.write(0x36, 0x007f);
    plan.write(0x47, 0x6040);
    gain(&mut plan);
    plan.write(0x38, reg38);
    plan.write(0x39, reg39);
    // Reuse the validated bandwidth, squelch and tone settings. Do not
    // copy primary antenna, speaker, GPIO or PA operations.
    for &step in primary.operations() {
        if matches!(
            step,
            Step::WriteA {
                register: 0x07 | 0x08 | 0x43 | 0x4d | 0x4e | 0x4f | 0x51 | 0x78,
                ..
            }
        ) {
            plan.push(step);
        }
    }
    plan.write(0x37, 0x1f0f);
    plan.push(Step::DelayMs(10));
    plan.write(0x30, 0x0200);
    plan.write(0x30, 0xbff1);
    Ok(plan)
}

/// Prepared B transmit path. External V/U gating and B33/B36 enable are
/// deliberately separate so the owner can stop both PA paths on any error.
pub struct RelayTransmit {
    pub setup: Plan,
    pub route: u8,
    pub power: u16,
}

pub fn relay_transmit(
    calibration: &Calibration<'_>,
    request: TxRequest,
) -> Result<RelayTransmit, Error> {
    let route = match request.frequency_10hz {
        13_600_000..=17_400_000 => 3,
        40_000_000..=48_000_000 => 4,
        value => return Err(Error::UnsupportedFrequency(value)),
    };
    let freq = uv98_receive::FrequencyPlan::new(
        calibration,
        uv98_receive::Chip::B,
        request.frequency_10hz,
    )
    .map_err(Error::Receive)?;
    let selection = freq.selection();
    let band = calibration
        .band(selection.band_index())
        .map_err(|_| Error::CalibrationPoint)?;
    let low = band
        .point_raw(Table::PowerLow, selection.table_index())
        .filter(|value| (1..=63).contains(value))
        .ok_or(Error::CalibrationPoint)?;
    let mut setup = Plan::new();
    setup.write(0x00, 0x8000);
    setup.push(Step::DelayMs(10));
    setup.write(0x00, 0);
    setup.write(0x37, 0x1d0f);
    gain(&mut setup);
    for (reg, value) in [
        (0x33, 0),
        (0x34, 0),
        (0x35, 0),
        (0x36, 0x007f),
        (0x1e, 0x4c58),
        (0x1f, 0xa656),
        (0x3e, 0xa037),
        (0x26, 0x13a0),
        (0x47, 0x6140),
        (0x28, 0x6b38),
        (0x29, 0xab40),
        (0x38, freq.frequency_words().reg38),
        (0x39, freq.frequency_words().reg39),
    ] {
        setup.write(reg, value);
    }
    setup.update(0x31, 0xfff7, 0);
    option(&mut setup, 0);
    setup.filter(
        FilterField::Tx3k,
        calibration.general().filter_raw(Filter::Tx3k),
    );
    setup.filter(
        FilterField::Tx300,
        calibration.general().filter_raw(Filter::Tx300),
    );
    setup.append(&tone(request.tone, request.tone_mode)?);
    setup.write(0x7d, rf::mic_register_04e90(u32::from(request.mic_gain)));
    setup.write(
        0x40,
        rf::deviation_at_040a8(
            u8::from(request.width == Width::Narrow),
            band.wide().tx_deviation_raw(),
            band.narrow().tx_deviation_raw(),
        ),
    );
    setup.push(Step::DelayMs(10));
    setup.write(0x52, 0x028f);
    setup.write(0x30, 0x0200);
    setup.write(0x30, 0xc1fe);
    Ok(RelayTransmit {
        setup,
        route,
        power: rf::power_register_04f0c(u32::from(low)),
    })
}
