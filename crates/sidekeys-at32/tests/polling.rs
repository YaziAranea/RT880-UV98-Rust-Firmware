use std::{cell::RefCell, rc::Rc};
use uv98_sidekeys_at32::{
    engine::{Engine, Register, Registers},
    Error, PollTiming, Sample, RX_AUDIO_SAMPLES,
};

#[derive(Default)]
struct Model {
    values: [u32; 12],
    writes: Vec<(Register, u32)>,
    reads: usize,
    stuck: Option<Error>,
    triggers: usize,
    raw: u32,
    battery_raw: u32,
    audio_raw: u32,
    conversion_channels: Vec<u32>,
    data_channel: u32,
    mux_lag: bool,
    fail_conversion: Option<usize>,
    ignore_sequence: Option<u32>,
    ignore_sample_time: Option<u32>,
    waits: usize,
    fail_timing: bool,
}
#[derive(Clone)]
struct Bus(Rc<RefCell<Model>>);
#[derive(Clone)]
struct Time(Rc<RefCell<Model>>);
impl Registers for Bus {
    fn read(&mut self, register: Register) -> u32 {
        let mut model = self.0.borrow_mut();
        model.reads += 1;
        if register == Register::CalibrationValue {
            return 42;
        }
        if register == Register::Control2 {
            if model.stuck != Some(Error::CalibrationInitTimeout) {
                model.values[register as usize] &= !8;
            }
            if model.stuck != Some(Error::CalibrationTimeout) {
                model.values[register as usize] &= !4;
            }
        }
        if register == Register::Data {
            model.values[Register::Status as usize] &= !2;
            return match model.data_channel {
                5 => model.audio_raw,
                7 => model.battery_raw,
                _ => model.raw,
            };
        }
        if register == Register::Misc && model.stuck == Some(Error::Configuration) {
            return 0;
        }
        model.values[register as usize]
    }
    fn write(&mut self, register: Register, value: u32) {
        let mut model = self.0.borrow_mut();
        model.writes.push((register, value));
        if register == Register::Status {
            model.values[register as usize] &= value;
            return;
        }
        if register == Register::Sequence3 && model.ignore_sequence == Some(value) {
            return;
        }
        if register == Register::SampleTime2 && model.ignore_sample_time == Some(value) {
            return;
        }
        model.values[register as usize] = value;
        if register == Register::Control2 && value == 1 && model.stuck != Some(Error::ReadyTimeout)
        {
            model.values[Register::Status as usize] |= 64;
        }
        if register == Register::Control2 && value & (1 << 30) != 0 {
            model.triggers += 1;
            let channel = model.values[Register::Sequence3 as usize];
            model.data_channel = if model.mux_lag {
                model.conversion_channels.last().copied().unwrap_or(12)
            } else {
                channel
            };
            model.conversion_channels.push(channel);
            if model.stuck != Some(Error::ConversionTimeout)
                && model.fail_conversion != Some(model.triggers)
            {
                model.values[Register::Status as usize] |= 2;
                model.values[register as usize] &= !(1 << 30);
                if model.stuck == Some(Error::Overflow) {
                    model.values[Register::Status as usize] |= 32;
                }
            }
        }
    }
}
impl PollTiming for Time {
    fn wait_poll(&mut self) -> Result<(), Error> {
        let mut model = self.0.borrow_mut();
        model.waits += 1;
        if model.fail_timing {
            Err(Error::Timing)
        } else {
            Ok(())
        }
    }
}
fn model() -> (Rc<RefCell<Model>>, Bus, Time) {
    let m = Rc::new(RefCell::new(Model::default()));
    (m.clone(), Bus(m.clone()), Time(m))
}

#[test]
fn sdk_initialization_and_one_fresh_conversion_per_call() {
    let (m, bus, time) = model();
    let mut adc = Engine::prepare(bus, time, 8).unwrap();
    assert_eq!(adc.calibration_value(), 42);
    assert_eq!(
        &m.borrow().writes[..9],
        &[
            (Register::Control2, 0),
            (Register::Control1, 0),
            (Register::SampleTime1, 448),
            (Register::SampleTime2, 7 << 21),
            (Register::Sequence1, 0),
            (Register::Sequence2, 0),
            (Register::Sequence3, 12),
            (Register::Oversampling, 0),
            (Register::Misc, 9),
        ]
    );
    for raw in [4095, 3542, 3541, 2717, 2716, 1885, 1884, 0] {
        m.borrow_mut().raw = raw;
        let sample = adc.sample().unwrap();
        assert_eq!(sample, Sample::from_raw(raw as u16).unwrap());
    }
    assert_eq!(m.borrow().triggers, 8);
    assert_eq!(m.borrow().values[Register::Control2 as usize], 1);
    drop(adc);
    assert_eq!(m.borrow().values[Register::Control2 as usize], 0);
}

#[test]
fn init_waits_have_finite_poll_budget_and_disable_on_failure() {
    for error in [
        Error::ReadyTimeout,
        Error::CalibrationInitTimeout,
        Error::CalibrationTimeout,
    ] {
        let (m, bus, time) = model();
        m.borrow_mut().stuck = Some(error);
        assert!(matches!(Engine::prepare(bus, time, 4), Err(e) if e == error));
        assert_eq!(m.borrow().waits, 3);
        assert_eq!(m.borrow().values[Register::Control2 as usize], 0);
        assert_eq!(m.borrow().triggers, 0);
        assert!(m.borrow().reads < 20);
    }
}

#[test]
fn conversion_timeout_does_not_retrigger_or_return_cached_release() {
    let (m, bus, time) = model();
    let mut adc = Engine::prepare(bus, time, 4).unwrap();
    m.borrow_mut().raw = 4095;
    assert_eq!(adc.sample().unwrap().key, None);
    m.borrow_mut().stuck = Some(Error::ConversionTimeout);
    assert_eq!(adc.sample(), Err(Error::ConversionTimeout));
    let writes = m.borrow().writes.len();
    let reads = m.borrow().reads;
    assert_eq!(adc.sample(), Err(Error::Faulted));
    assert_eq!((m.borrow().writes.len(), m.borrow().reads), (writes, reads));
    assert_eq!(m.borrow().triggers, 2);
    assert_eq!(m.borrow().values[Register::Control2 as usize], 0);
}

#[test]
fn overflow_bad_data_and_configuration_are_terminal() {
    for error in [Error::Overflow, Error::InvalidSample, Error::Configuration] {
        let (m, bus, time) = model();
        let mut adc = Engine::prepare(bus, time, 4).unwrap();
        match error {
            Error::Overflow => m.borrow_mut().stuck = Some(error),
            Error::InvalidSample => m.borrow_mut().raw = 4096,
            Error::Configuration => m.borrow_mut().values[Register::Control2 as usize] |= 1 << 8,
            _ => unreachable!(),
        }
        assert_eq!(adc.sample(), Err(error));
        assert_eq!(m.borrow().values[Register::Control2 as usize], 0);
        assert_eq!(adc.sample(), Err(Error::Faulted));
    }
}

#[test]
fn invalid_budget_does_no_io_and_timing_failure_disables_adc() {
    for budget in [0, 100_001, u32::MAX] {
        let (m, bus, time) = model();
        assert!(matches!(
            Engine::prepare(bus, time, budget),
            Err(Error::InvalidBudget)
        ));
        assert_eq!(m.borrow().reads, 0);
        assert!(m.borrow().writes.is_empty());
    }
    let (m, bus, time) = model();
    m.borrow_mut().stuck = Some(Error::ReadyTimeout);
    m.borrow_mut().fail_timing = true;
    assert!(matches!(Engine::prepare(bus, time, 8), Err(Error::Timing)));
    assert_eq!(m.borrow().values[Register::Control2 as usize], 0);
}

#[test]
fn no_physical_button_guess_and_out_of_range_is_not_release() {
    use uv98_keys::Key;
    assert_eq!(Sample::from_raw(1884).unwrap().key, Some(Key::Aux17));
    assert_eq!(Sample::from_raw(1885).unwrap().key, Some(Key::Aux16));
    assert_eq!(Sample::from_raw(2716).unwrap().key, Some(Key::Aux16));
    assert_eq!(Sample::from_raw(2717).unwrap().key, Some(Key::Aux18));
    assert_eq!(Sample::from_raw(3541).unwrap().key, Some(Key::Aux18));
    assert_eq!(Sample::from_raw(3542).unwrap().key, None);
    assert_eq!(Sample::from_raw(4096), Err(Error::InvalidSample));
}

#[test]
fn battery_sampling_discards_mux_residue_and_restores_fresh_pc2() {
    let (m, bus, time) = model();
    let mut adc = Engine::prepare(bus, time, 8).unwrap();
    {
        let mut model = m.borrow_mut();
        model.mux_lag = true;
        model.raw = 4095;
        model.battery_raw = 1224;
    }
    assert_eq!(adc.sample().unwrap().key, None);
    assert_eq!(adc.sample_battery(), Ok(1224));
    assert_eq!(m.borrow().values[Register::Sequence3 as usize], 12);
    assert_eq!(adc.sample().unwrap(), Sample::from_raw(4095).unwrap());
    m.borrow_mut().raw = 2200;
    assert_eq!(adc.sample().unwrap().key, Some(uv98_keys::Key::Aux16));
    m.borrow_mut().battery_raw = 0;
    assert_eq!(adc.sample_battery(), Ok(0));
    assert_eq!(adc.sample().unwrap().key, Some(uv98_keys::Key::Aux16));
    assert_eq!(
        m.borrow().conversion_channels,
        [12, 7, 7, 12, 12, 12, 7, 7, 12, 12]
    );
}

#[test]
fn timeout_in_any_battery_mux_phase_disables_adc_before_restore() {
    for phase in 1..=3 {
        let (m, bus, time) = model();
        let mut adc = Engine::prepare(bus, time, 4).unwrap();
        m.borrow_mut().fail_conversion = Some(phase);
        assert_eq!(adc.sample_battery(), Err(Error::ConversionTimeout));
        {
            let model = m.borrow();
            assert_eq!(model.waits, 3);
            assert_eq!(model.triggers, phase);
            assert!(model
                .writes
                .ends_with(&[(Register::Control2, 0), (Register::Sequence3, 12)]));
        }
        assert_eq!(adc.sample(), Err(Error::Faulted));
        assert_eq!(adc.sample_battery(), Err(Error::Faulted));
        assert_eq!(m.borrow().triggers, phase);
    }
}

#[test]
fn mux_selection_readback_mismatch_prevents_wrong_channel_conversion() {
    for (ignored_channel, expected_triggers) in [(7, 0), (12, 2)] {
        let (m, bus, time) = model();
        let mut adc = Engine::prepare(bus, time, 4).unwrap();
        m.borrow_mut().ignore_sequence = Some(ignored_channel);
        assert_eq!(adc.sample_battery(), Err(Error::Configuration));
        assert_eq!(adc.sample(), Err(Error::Faulted));
        assert_eq!(m.borrow().triggers, expected_triggers);
        assert_eq!(m.borrow().values[Register::Control2 as usize], 0);
    }
}

fn advancing_counter(start: u32) -> impl FnMut() -> u32 {
    let mut cycle = start;
    move || {
        cycle = cycle.wrapping_add(17);
        cycle
    }
}

#[test]
fn audio_has_wrapping_timestamps_and_restores_the_exact_sample_time_and_mux() {
    let (m, bus, time) = model();
    let mut adc = Engine::prepare(bus, time, 8).unwrap();
    let original_sample_time = (7 << 21) | (1 << 15);
    {
        let mut model = m.borrow_mut();
        model.mux_lag = true;
        model.raw = 4095;
        model.battery_raw = 1224;
        model.audio_raw = 902;
        model.values[Register::SampleTime2 as usize] = original_sample_time;
    }
    let mut samples = [u16::MAX; RX_AUDIO_SAMPLES];
    let timing = adc
        .capture_rx_audio(&mut samples, advancing_counter(u32::MAX - 20_000))
        .unwrap();
    assert_eq!(samples, [902; RX_AUDIO_SAMPLES]);
    assert!(timing.first_trigger_cycle > timing.last_trigger_cycle);
    let nominal_span = (RX_AUDIO_SAMPLES as u32 - 1) * 6_000;
    let span = timing
        .last_trigger_cycle
        .wrapping_sub(timing.first_trigger_cycle);
    assert!(span.abs_diff(nominal_span) < 40);
    assert!(timing.min_interval_cycles.abs_diff(6_000) < 40);
    assert!(timing.max_interval_cycles.abs_diff(6_000) < 40);
    assert!(timing.max_late_cycles < 40);
    {
        let model = m.borrow();
        assert_eq!(
            model.values[Register::SampleTime2 as usize],
            original_sample_time
        );
        assert_eq!(model.values[Register::Sequence3 as usize], 12);
        assert_eq!(model.values[Register::Control2 as usize], 1);
        assert!(model
            .writes
            .contains(&(Register::SampleTime2, (7 << 21) | (7 << 15))));
        assert_eq!(
            &model.conversion_channels[..RX_AUDIO_SAMPLES + 1],
            &[5; RX_AUDIO_SAMPLES + 1]
        );
        assert_eq!(model.conversion_channels[RX_AUDIO_SAMPLES + 1], 12);
        assert_eq!(model.conversion_channels.len(), RX_AUDIO_SAMPLES + 2);
    }
    assert_eq!(adc.sample().unwrap(), Sample::from_raw(4095).unwrap());
    assert_eq!(adc.sample_battery(), Ok(1224));
    assert_eq!(adc.calibration_value(), 42);
}

#[test]
fn audio_deadline_and_stalled_counter_discard_the_block_without_faulting_adc() {
    for stalled_counter in [false, true] {
        let (m, bus, time) = model();
        let mut adc = Engine::prepare(bus, time, 8).unwrap();
        m.borrow_mut().raw = 4095;
        let mut cycle = 0u32;
        let mut samples = [u16::MAX; RX_AUDIO_SAMPLES];
        assert_eq!(
            adc.capture_rx_audio(&mut samples, || {
                // Inject a missed deadline just after the first retained trigger.
                if !stalled_counter {
                    cycle += if m.borrow().triggers == 2 { 6_301 } else { 17 };
                }
                cycle
            }),
            Err(Error::Timing)
        );
        {
            let model = m.borrow();
            let expected = if stalled_counter { 2 } else { 3 };
            assert_eq!(model.triggers, expected);
            assert_eq!(model.values[Register::SampleTime2 as usize], 7 << 21);
            assert_eq!(model.values[Register::Sequence3 as usize], 12);
            assert_eq!(model.values[Register::Control2 as usize], 1);
        }
        assert_eq!(adc.sample().unwrap(), Sample::from_raw(4095).unwrap());
    }
}

#[test]
fn audio_timeouts_disable_adc_and_restore_configuration_before_returning() {
    for phase in [1, 2, RX_AUDIO_SAMPLES + 1, RX_AUDIO_SAMPLES + 2] {
        let (m, bus, time) = model();
        let mut adc = Engine::prepare(bus, time, 8).unwrap();
        m.borrow_mut().fail_conversion = Some(phase);
        let mut samples = [0; RX_AUDIO_SAMPLES];
        assert_eq!(
            adc.capture_rx_audio(&mut samples, advancing_counter(0)),
            Err(Error::ConversionTimeout)
        );
        {
            let model = m.borrow();
            assert_eq!(model.triggers, phase);
            assert_eq!(model.values[Register::SampleTime2 as usize], 7 << 21);
            assert_eq!(model.values[Register::Sequence3 as usize], 12);
            assert_eq!(model.values[Register::Control2 as usize], 0);
            if phase != RX_AUDIO_SAMPLES + 2 {
                assert!(model.writes.ends_with(&[
                    (Register::Control2, 0),
                    (Register::SampleTime2, 7 << 21),
                    (Register::Sequence3, 12),
                ]));
            }
        }
        assert_eq!(adc.sample(), Err(Error::Faulted));
        assert_eq!(adc.sample_battery(), Err(Error::Faulted));
        assert_eq!(
            adc.capture_rx_audio(&mut samples, advancing_counter(0)),
            Err(Error::Faulted)
        );
    }
}

#[test]
fn audio_readback_failures_disable_adc_and_prevent_mislabeled_samples() {
    for (ignored_sample_time, expected_triggers) in
        [(7 << 21 | 7 << 15, 0), (7 << 21, RX_AUDIO_SAMPLES + 1)]
    {
        let (m, bus, time) = model();
        let mut adc = Engine::prepare(bus, time, 8).unwrap();
        m.borrow_mut().ignore_sample_time = Some(ignored_sample_time);
        let mut samples = [0; RX_AUDIO_SAMPLES];
        assert_eq!(
            adc.capture_rx_audio(&mut samples, advancing_counter(0)),
            Err(Error::Configuration)
        );
        assert_eq!(adc.sample(), Err(Error::Faulted));
        assert_eq!(adc.sample_battery(), Err(Error::Faulted));
        let model = m.borrow();
        assert_eq!(model.triggers, expected_triggers);
        assert_eq!(model.values[Register::Control2 as usize], 0);
    }
}
