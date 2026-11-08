use std::{cell::RefCell, rc::Rc};
use uv98_sidekeys_at32::{
    engine::{Engine, Register, Registers},
    Error, PollTiming, Sample,
};

#[derive(Default)]
struct Model {
    values: [u32; 11],
    writes: Vec<(Register, u32)>,
    reads: usize,
    stuck: Option<Error>,
    triggers: usize,
    raw: u32,
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
            return model.raw;
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
        model.values[register as usize] = value;
        if register == Register::Control2 && value == 1 && model.stuck != Some(Error::ReadyTimeout)
        {
            model.values[Register::Status as usize] |= 64;
        }
        if register == Register::Control2 && value & (1 << 30) != 0 {
            model.triggers += 1;
            if model.stuck != Some(Error::ConversionTimeout) {
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
        &m.borrow().writes[..8],
        &[
            (Register::Control2, 0),
            (Register::Control1, 0),
            (Register::SampleTime1, 448),
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
