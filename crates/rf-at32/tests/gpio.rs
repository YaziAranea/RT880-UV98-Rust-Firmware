use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use uv98_receive::transport::{AudioControl, BoardControl};
use uv98_rf_at32::{
    engine::{
        ConfigRegister as Reg, ControlEngine, GuardedControl, PinEngine, PinError, Port, Registers,
    },
    ControlLevels, Interlock,
};
use uv98_rf_bus::{Bus, Delay, Direction, Phase, Pins, Profile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Read(Port, Reg),
    Write(Port, Reg, u32),
    Set(Port, u16),
    Clear(Port, u16),
    Input(Port),
}
struct State {
    config: [[u32; 4]; 3],
    latch: [u16; 3],
    events: Vec<Event>,
    inputs: VecDeque<u32>,
}
type Shared = Rc<RefCell<State>>;
struct Mock(Shared);
fn port_index(p: Port) -> usize {
    match p {
        Port::A => 0,
        Port::C => 1,
        Port::F => 2,
    }
}
impl Registers for Mock {
    fn read_config(&mut self, p: Port, r: Reg) -> u32 {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Read(p, r));
        s.config[port_index(p)][r as usize / 4]
    }
    fn write_config(&mut self, p: Port, r: Reg, value: u32) {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Write(p, r, value));
        s.config[port_index(p)][r as usize / 4] = value;
    }
    fn set_bits(&mut self, p: Port, mask: u16) {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Set(p, mask));
        s.latch[port_index(p)] |= mask;
    }
    fn clear_bits(&mut self, p: Port, mask: u16) {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Clear(p, mask));
        s.latch[port_index(p)] &= !mask;
    }
    fn read_input(&mut self, p: Port) -> u32 {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Input(p));
        s.inputs.pop_front().expect("unexpected IDT read")
    }
}
fn make(seed: u32) -> (Mock, Shared) {
    let s = Rc::new(RefCell::new(State {
        config: [[seed; 4]; 3],
        latch: [seed as u16; 3],
        events: vec![],
        inputs: VecDeque::new(),
    }));
    (Mock(s.clone()), s)
}
fn masks(profile: Profile) -> [u16; 3] {
    match profile {
        Profile::A => [0x100, 0x200, 0x40],
        Profile::B => [0, 0x1c0, 0],
    }
}
fn expand(mask: u16) -> u32 {
    (0..16)
        .filter(|bit| mask & (1 << bit) != 0)
        .fold(0, |v, bit| v | (3 << (bit * 2)))
}
fn repeated(mask: u16, code: u32) -> u32 {
    (0..16)
        .filter(|bit| mask & (1 << bit) != 0)
        .fold(0, |v, bit| v | (code << (bit * 2)))
}
fn data_bit(profile: Profile) -> u16 {
    if profile == Profile::A {
        0x200
    } else {
        0x40
    }
}
fn sck(profile: Profile) -> (Port, u16) {
    if profile == Profile::A {
        (Port::F, 0x40)
    } else {
        (Port::C, 0x100)
    }
}

#[test]
fn prepare_preserves_every_unowned_config_bit_and_latch_for_both_profiles() {
    for seed in [0, u32::MAX, 0xa5a5_5a5a, 0x5a5a_a5a5, 0x8000_0001] {
        for profile in [Profile::A, Profile::B] {
            let (r, s) = make(seed);
            let _pins = PinEngine::prepare(r, profile);
            let state = s.borrow();
            for (port, mask) in masks(profile).into_iter().enumerate() {
                for reg in [Reg::Mode, Reg::OutputType, Reg::Drive, Reg::Pull] {
                    let allowed = if reg == Reg::OutputType {
                        u32::from(mask)
                    } else {
                        expand(mask)
                    };
                    let actual = state.config[port][reg as usize / 4];
                    assert_eq!((actual ^ seed) & !allowed, 0, "{profile:?} {reg:?}");
                    let expected = match reg {
                        Reg::Mode => repeated(mask, 1),
                        Reg::Drive => repeated(mask, 2),
                        _ => 0,
                    };
                    assert_eq!(actual & allowed, expected);
                }
                assert_eq!((state.latch[port] ^ seed as u16) & !mask, 0);
            }
            let cs = if profile == Profile::A {
                (0, 0x100)
            } else {
                (1, 0x80)
            };
            assert_eq!(state.latch[cs.0] & cs.1, cs.1);
            assert_eq!(state.latch[1] & data_bit(profile), 0);
            let (p, m) = sck(profile);
            assert_eq!(state.latch[port_index(p)] & m, 0);
        }
    }
}

#[test]
fn data_is_high_z_first_and_all_latches_precede_output_enable() {
    for profile in [Profile::A, Profile::B] {
        let (r, s) = make(u32::MAX);
        let _pins = PinEngine::prepare(r, profile);
        let events = &s.borrow().events;
        let mask = data_bit(profile);
        let bit = mask.trailing_zeros();
        assert_eq!(events[0], Event::Read(Port::C, Reg::Mode));
        assert_eq!(
            events[1],
            Event::Write(Port::C, Reg::Mode, u32::MAX & !(3 << (bit * 2)))
        );
        let cs = if profile == Profile::A {
            (Port::A, 0x100)
        } else {
            (Port::C, 0x80)
        };
        let clock = sck(profile);
        let last_preload = events
            .iter()
            .position(|e| *e == Event::Clear(Port::C, mask))
            .unwrap();
        assert!(events[..last_preload].contains(&Event::Set(cs.0, cs.1)));
        assert!(events[..last_preload].contains(&Event::Clear(clock.0, clock.1)));
        for (port, pin) in [cs, clock, (Port::C, mask)] {
            let shift = pin.trailing_zeros() * 2;
            let output_index = events
                .iter()
                .position(|e| matches!(*e,Event::Write(p,Reg::Mode,v) if p==port&&(v>>shift)&3==1))
                .unwrap();
            assert!(output_index > last_preload);
        }
    }
}

#[test]
fn turnaround_changes_only_data_fields_releases_first_and_preloads_before_output() {
    for profile in [Profile::A, Profile::B] {
        let (r, s) = make(0xa55a_a55a);
        let mut p = PinEngine::prepare(r, profile);
        p.data_level(true).unwrap();
        let before = s.borrow().config;
        s.borrow_mut().events.clear();
        p.data_direction(Direction::Input).unwrap();
        let mask = data_bit(profile);
        let shift = mask.trailing_zeros() * 2;
        let mut expected = before;
        expected[1][0] &= !(3 << shift);
        expected[1][1] &= !u32::from(mask);
        expected[1][2] = (expected[1][2] & !(3 << shift)) | (1 << shift);
        expected[1][3] &= !(3 << shift);
        assert_eq!(s.borrow().config, expected);
        assert_eq!(s.borrow().events[0], Event::Read(Port::C, Reg::Mode));
        assert_eq!(
            s.borrow().events[1],
            Event::Write(Port::C, Reg::Mode, expected[1][0])
        );
        s.borrow_mut().events.clear();
        p.data_direction(Direction::Output).unwrap();
        assert_eq!(s.borrow().events[0], Event::Set(Port::C, mask));
        assert_eq!(s.borrow().config, before);
        assert!(matches!(
            s.borrow().events.last(),
            Some(Event::Write(Port::C, Reg::Mode, _))
        ));
    }
}

#[test]
fn gpio_edges_and_input_sampling_use_exact_ports_and_single_masks() {
    for profile in [Profile::A, Profile::B] {
        let (r, s) = make(0);
        let mut p = PinEngine::prepare(r, profile);
        s.borrow_mut().events.clear();
        p.chip_select(false).unwrap();
        p.clock(false).unwrap();
        p.data_level(true).unwrap();
        p.clock(true).unwrap();
        let cs = if profile == Profile::A {
            (Port::A, 0x100)
        } else {
            (Port::C, 0x80)
        };
        let clock = sck(profile);
        let data = data_bit(profile);
        assert_eq!(
            s.borrow().events,
            [
                Event::Clear(cs.0, cs.1),
                Event::Clear(clock.0, clock.1),
                Event::Set(Port::C, data),
                Event::Set(clock.0, clock.1)
            ]
        );
        p.data_direction(Direction::Input).unwrap();
        s.borrow_mut().events.clear();
        s.borrow_mut()
            .inputs
            .extend([u32::MAX & !u32::from(data), u32::from(data)]);
        assert!(!p.read_data().unwrap());
        assert!(p.read_data().unwrap());
        assert_eq!(s.borrow().events, [Event::Input(Port::C); 2]);
    }
}

#[test]
fn invalid_direction_requests_cause_no_register_calls_and_cleanup_can_release_data() {
    let (r, s) = make(0);
    let mut p = PinEngine::prepare(r, Profile::A);
    s.borrow_mut().events.clear();
    assert_eq!(p.read_data(), Err(PinError::DataIsOutput));
    assert!(s.borrow().events.is_empty());
    p.chip_select(false).unwrap();
    p.data_direction(Direction::Input).unwrap();
    s.borrow_mut().events.clear();
    assert_eq!(
        p.data_direction(Direction::Output),
        Err(PinError::OutputWhileSelected)
    );
    assert_eq!(p.data_level(true), Err(PinError::DataIsInput));
    assert_eq!(p.chip_select(false), Err(PinError::DataIsInput));
    assert!(s.borrow().events.is_empty());
    p.chip_select(true).unwrap();
    p.data_direction(Direction::Input).unwrap();
    assert_eq!(s.borrow().events[0], Event::Set(Port::A, 0x100));
}

struct NoDelay;
impl Delay for NoDelay {
    fn wait(&mut self, _: Phase) {}
}
#[test]
fn real_bus_engine_uses_24_clock_requests_and_recovers_each_profile_direction() {
    for profile in [Profile::A, Profile::B] {
        let (r, s) = make(0);
        let p = PinEngine::prepare(r, profile);
        let mut bus = Bus::new(p, NoDelay, profile);
        s.borrow_mut().events.clear();
        bus.write(0x38, 0xa55a).unwrap();
        let clock = sck(profile);
        let data = data_bit(profile);
        assert_eq!(
            s.borrow()
                .events
                .iter()
                .filter(|e| **e == Event::Set(clock.0, clock.1))
                .count(),
            24
        );
        let data_bits: Vec<bool> = s
            .borrow()
            .events
            .iter()
            .filter_map(|e| match *e {
                Event::Set(Port::C, m) if m == data => Some(true),
                Event::Clear(Port::C, m) if m == data => Some(false),
                _ => None,
            })
            .collect();
        // B's output restoration preloads one initial latch before its 24bits.
        let bits = &data_bits[data_bits.len() - 24..];
        let bytes: Vec<u8> = bits
            .chunks(8)
            .map(|chunk| chunk.iter().fold(0, |v, &bit| (v << 1) | u8::from(bit)))
            .collect();
        assert_eq!(bytes, [0x38, 0xa5, 0x5a]);
        s.borrow_mut().events.clear();
        let word = 0x9a53u16;
        s.borrow_mut().inputs.extend((0..16).rev().map(|shift| {
            if word & (1 << shift) != 0 {
                u32::from(data)
            } else {
                0
            }
        }));
        assert_eq!(bus.read(0x33).unwrap(), word);
        assert_eq!(
            s.borrow()
                .events
                .iter()
                .filter(|e| **e == Event::Set(clock.0, clock.1))
                .count(),
            24
        );
        let mode = (s.borrow().config[1][0] >> (data.trailing_zeros() * 2)) & 3;
        assert_eq!(mode, if profile == Profile::A { 1 } else { 0 });
        bus.write(0x39, 0x1234).unwrap();
        assert!(!bus.is_poisoned());
    }
}

#[test]
fn control_initialization_preserves_other_gpiof_pins_and_preloads_both_levels() {
    for antenna in [false, true] {
        for audio in [false, true] {
            let (r, s) = make(0xdead_beef);
            let mut c = ControlEngine::prepare(
                r,
                ControlLevels {
                    antenna_pf0_high: antenna,
                    af_mute_pf8_high: audio,
                },
            );
            let events = s.borrow().events.clone();
            assert_eq!(
                events[0],
                if antenna {
                    Event::Set(Port::F, 1)
                } else {
                    Event::Clear(Port::F, 1)
                }
            );
            assert_eq!(
                events[1],
                if audio {
                    Event::Set(Port::F, 0x100)
                } else {
                    Event::Clear(Port::F, 0x100)
                }
            );
            let state = s.borrow();
            assert_eq!(state.config[0], [0xdead_beef; 4]);
            assert_eq!(state.config[1], [0xdead_beef; 4]);
            for reg in [Reg::Mode, Reg::OutputType, Reg::Drive, Reg::Pull] {
                let mask = if reg == Reg::OutputType {
                    0x101
                } else {
                    expand(0x101)
                };
                assert_eq!((state.config[2][reg as usize / 4] ^ 0xdead_beef) & !mask, 0);
            }
            assert_eq!((state.latch[2] ^ 0xbeef) & !0x101, 0);
            drop(state);
            s.borrow_mut().events.clear();
            c.set_antenna_pf0(!antenna);
            c.set_af_mute_pf8(!audio);
            assert_eq!(s.borrow().events.len(), 2);
            assert!(s.borrow().events.iter().all(|e| matches!(
                *e,
                Event::Set(Port::F, 1 | 0x100) | Event::Clear(Port::F, 1 | 0x100)
            )));
        }
    }
}

struct Guard {
    blocked: Rc<Cell<bool>>,
    checks: Rc<Cell<usize>>,
}
impl Interlock for Guard {
    type Error = &'static str;
    fn check_receive_preconditions(&mut self) -> Result<(), Self::Error> {
        self.checks.set(self.checks.get() + 1);
        if self.blocked.get() {
            Err("PTT")
        } else {
            Ok(())
        }
    }
}
#[test]
fn failed_interlock_blocks_relay_and_audio_high_but_audio_low_remains_available() {
    let (r, s) = make(0);
    let blocked = Rc::new(Cell::new(true));
    let checks = Rc::new(Cell::new(0));
    let guard = Guard {
        blocked: blocked.clone(),
        checks: checks.clone(),
    };
    let mut c = GuardedControl::prepare(
        r,
        ControlLevels {
            antenna_pf0_high: false,
            af_mute_pf8_high: false,
        },
        guard,
    );
    s.borrow_mut().events.clear();
    assert_eq!(c.check_receive_preconditions(), Err("PTT"));
    assert_eq!(c.set_antenna_pf0(true), Err("PTT"));
    assert_eq!(c.set_audio_mute(false), Err("PTT"));
    assert!(s.borrow().events.is_empty());
    assert_eq!(checks.get(), 3);
    c.set_audio_mute(true).unwrap();
    assert_eq!(s.borrow().events, [Event::Clear(Port::F, 0x100)]);
    assert_eq!(checks.get(), 3);
    blocked.set(false);
    c.set_antenna_pf0(true).unwrap();
    c.set_audio_mute(false).unwrap();
    assert_eq!(
        s.borrow().events,
        [
            Event::Clear(Port::F, 0x100),
            Event::Set(Port::F, 1),
            Event::Set(Port::F, 0x100)
        ]
    );
}

/// Decode actual GPIO requests while A CS is asserted, sampling DATA only
/// when the MCU is output. Read frames therefore contain only their command.
fn primary_frames(events: &[Event]) -> Vec<Vec<u8>> {
    let mut selected = false;
    let mut output = true;
    let mut data = false;
    let mut bits = Vec::new();
    let mut frames = Vec::new();
    for &event in events {
        match event {
            Event::Clear(Port::A, 0x100) => {
                selected = true;
                bits.clear();
            }
            Event::Set(Port::A, 0x100) if selected => {
                selected = false;
                assert_eq!(bits.len() % 8, 0);
                frames.push(
                    bits.chunks(8)
                        .map(|chunk| chunk.iter().fold(0u8, |v, &bit| (v << 1) | u8::from(bit)))
                        .collect(),
                );
            }
            Event::Write(Port::C, Reg::Mode, value) => output = (value >> 18) & 3 == 1,
            Event::Set(Port::C, 0x200) => data = true,
            Event::Clear(Port::C, 0x200) => data = false,
            Event::Set(Port::F, 0x40) if selected && output => bits.push(data),
            _ => {}
        }
    }
    assert!(!selected);
    frames
}

#[test]
fn full_gpio_bus_pair_runtime_chain_requires_ack_then_reads_signal_opens_audio_and_stops() {
    use uv98_calibration::Calibration;
    use uv98_receive::{
        runtime::{Runtime, State as RxState, Timing, Volume},
        transport::Pair,
        Operation, Plan, Width,
    };
    const CAL: &[u8] = include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");
    let plan = Plan::primary(
        &Calibration::parse(CAL).unwrap(),
        14_550_000,
        Width::Wide,
        5,
    )
    .unwrap();
    let (ar, a) = make(0xa5a5_5a5a);
    let (br, b) = make(0x5a5a_a5a5);
    let (cr, controls) = make(0xdead_beef);
    let mut a_bus = Bus::new(PinEngine::prepare(ar, Profile::A), NoDelay, Profile::A);
    let mut b_bus = Bus::new(PinEngine::prepare(br, Profile::B), NoDelay, Profile::B);
    let blocked = Rc::new(Cell::new(false));
    let guard = Guard {
        blocked: blocked.clone(),
        checks: Rc::new(Cell::new(0)),
    };
    let mut control = GuardedControl::prepare(
        cr,
        ControlLevels {
            antenna_pf0_high: false,
            af_mute_pf8_high: false,
        },
        guard,
    );
    for log in [&a, &b, &controls] {
        log.borrow_mut().events.clear();
    }
    // Initial A33, fresh route A33, raw RSSI A67, squelch A0C, volume selector A01.
    for word in [0u16, 0, 0x01d2, 2, 2] {
        a.borrow_mut().inputs.extend((0..16).rev().map(|shift| {
            if word & (1 << shift) != 0 {
                0x200
            } else {
                0
            }
        }));
    }
    let pair = Pair::new(&mut a_bus, &mut b_bus, &mut control).unwrap();
    let mut runtime = Runtime::new(pair, Timing::new(1000, 100, 5).unwrap());
    let mut now = 0;
    let ticket = runtime.start(plan, now).unwrap();
    assert!(a.borrow().events.is_empty());
    for _ in 0..100 {
        if runtime.state() == RxState::AwaitSettling {
            break;
        }
        now += 1;
        runtime.step(now).unwrap();
    }
    assert_eq!(runtime.state(), RxState::AwaitSettling);
    runtime.step(now).unwrap(); // First post-start write observation.
    now += 5;
    runtime.step(now).unwrap();
    assert_eq!(runtime.state(), RxState::AwaitSettling); // A timer cannot ACK.
    let before_sample = a.borrow().events.len();
    assert!(runtime.sample(now).is_err());
    assert_eq!(a.borrow().events.len(), before_sample);
    assert!(runtime.acknowledge_settled(ticket, now).unwrap());
    assert_eq!(runtime.state(), RxState::Listening);
    let signal = runtime.sample(now + 1).unwrap();
    assert_eq!(signal.ticket, ticket);
    assert_eq!(signal.raw_rssi, 0x01d2);
    assert!(signal.squelch_open);
    runtime
        .open_fm_audio(Volume::new(12, 8).unwrap(), now + 2)
        .unwrap();
    assert!(runtime.progress().audio_open);
    // Stop still reaches PF8 LOW if the interlock now detects PTT.
    blocked.set(true);
    let before_stop = a.borrow().events.len();
    assert_eq!(runtime.stop().state, RxState::Stopped);
    assert_eq!(a.borrow().events.len(), before_stop);
    assert_eq!(
        controls.borrow().events.last(),
        Some(&Event::Clear(Port::F, 0x100))
    );

    let mut expected = vec![vec![0xb3], vec![0x47, 0x60, 0x40]];
    for operation in &plan.operations()[..5] {
        let Operation::Write {
            register, value, ..
        } = *operation
        else {
            panic!()
        };
        expected.push(vec![register, (value >> 8) as u8, value as u8]);
    }
    expected.extend([
        vec![0xb3],
        vec![0x33, 0, 0x10],
        vec![0x37, 0x1f, 0x0f],
        vec![0x30, 0x02, 0],
        vec![0x30, 0xbf, 0xf1],
        vec![0xe7],
        vec![0x8c],
        vec![0x47, 0x61, 0x40],
        vec![0x81],
        vec![0x48, 0xb0, 0xc5],
    ]);
    assert_eq!(primary_frames(&a.borrow().events), expected);
    assert!(a.borrow().inputs.is_empty());
    assert!(b.borrow().events.is_empty());
    assert_eq!(
        controls.borrow().events,
        [
            Event::Clear(Port::F, 0x100),
            Event::Clear(Port::F, 1),
            Event::Clear(Port::F, 0x100),
            Event::Set(Port::F, 0x100),
            Event::Clear(Port::F, 0x100),
        ]
    );
    let before_drop = controls.borrow().events.len();
    drop(runtime); // Explicitly stopped runtime performs no extra mute request.
    assert_eq!(controls.borrow().events.len(), before_drop);
}
