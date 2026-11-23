#[path = "../../../firmware-diagnostic/src/beacon.rs"]
mod beacon;

use beacon::Beacon;
use std::{cell::RefCell, convert::Infallible, rc::Rc};
use uv98_calibration::Calibration;
use uv98_handheld_ui::controller::{AprsStatus, Controller};
use uv98_radio_runtime::{policy, Io, Runtime, State};
use uv98_signaling::{
    aprs::{Address, Config, Wire},
    cmx868::{Bus, Error, Modem},
};

const FREQ: u32 = 14_550_000;
const FIX: (i32, i32) = (490_583_333, -720_291_667);
const TONE: u16 = 0x1000 | 885;
const MENU: u16 = 0x1000;

#[derive(Clone)]
struct Rf(Rc<RefCell<[u16; 128]>>);

impl Io for Rf {
    type Error = Infallible;

    fn read(&mut self, reg: u8) -> Result<u16, Self::Error> {
        Ok(self.0.borrow()[usize::from(reg)])
    }

    fn write(&mut self, reg: u8, value: u16) -> Result<(), Self::Error> {
        self.0.borrow_mut()[usize::from(reg)] = value;
        Ok(())
    }

    fn mute(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn antenna(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn hf_gates(&mut self, _: bool, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn indicators(&mut self, _: bool, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn backlights(&mut self, _: bool, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn verify_control(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Default)]
struct ModemRegisters {
    wire: Vec<u8>,
    audio: bool,
    power: u16,
}

struct Chip {
    registers: Rc<RefCell<ModemRegisters>>,
    rf: Rf,
}

impl Bus for Chip {
    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn write16(&mut self, reg: u8, value: u16) -> Result<(), Error> {
        if reg == 0xe0 {
            self.registers.borrow_mut().power = value;
        }
        Ok(())
    }

    fn write8(&mut self, reg: u8, value: u8) -> Result<(), Error> {
        assert_eq!(reg, 0xe3);
        self.registers.borrow_mut().wire.push(value);
        Ok(())
    }

    fn read16(&mut self, reg: u8) -> Result<u16, Error> {
        assert_eq!(reg, 0xe6);
        Ok(0x1400) // TX buffer ready, no underflow.
    }

    fn read8(&mut self, reg: u8) -> Result<u8, Error> {
        panic!("APRS TX read unexpected byte register {reg:#04x}")
    }

    fn tx_audio(&mut self, enabled: bool) {
        if enabled {
            let rf = self.rf.0.borrow();
            assert_eq!(rf[0x30] & 6, 6, "MIC ADC and TX DSP must be enabled");
            assert_ne!(rf[0x36], 0x007f, "PA must be set before modem audio");
        }
        self.registers.borrow_mut().audio = enabled;
    }
}

struct Station {
    calibration: Calibration<'static>,
    profile: policy::Profile,
    config: Config,
    radio: Runtime<Rf>,
    rf: Rf,
    beacon: Beacon<Chip>,
    modem: Rc<RefCell<ModemRegisters>>,
    keys_still_clear: bool,
    now: u32,
}

impl Station {
    fn new() -> Self {
        let calibration = Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        let profile = policy::profile(
            Controller::new(&[0; 128], FREQ).view(),
            None,
            policy::Settings {
                squelch: 3,
                mic_gain: 16,
                fm_volume: [58, 15],
                am_volume: [50, 12],
                squelch_delay: 10,
            },
            policy::Overrides::default(),
        )
        .unwrap()
        .with_rx_tone_override(Some(TONE));
        assert_eq!(profile.receive.unwrap().frequency_10hz, FREQ);
        assert_eq!(profile.transmit.unwrap().frequency_10hz, FREQ);
        let config = Config {
            source: Address::new(b"N0CALL", 7).unwrap(),
            paths: [None; 2],
            symbol_table: b'/',
            symbol: b'>',
            comment: [0; 32],
            comment_len: 0,
        };
        let rf = Rf(Rc::new(RefCell::new([0; 128])));
        let modem = Rc::new(RefCell::new(ModemRegisters::default()));
        let chip = Chip {
            registers: modem.clone(),
            rf: rf.clone(),
        };
        let mut station = Self {
            calibration,
            profile,
            config,
            radio: Runtime::new(rf.clone()),
            rf,
            beacon: Beacon::from_modem(Some(Modem::new(chip)), Some(config), 1),
            modem,
            keys_still_clear: true,
            now: 0,
        };
        station.radio.cold_start(&station.calibration, 0).unwrap();
        station.complete_configuration(State::Off);
        station.receive();
        station
    }

    fn complete_configuration(&mut self, state: State) {
        for _ in 0..500 {
            if self.radio.state() == state {
                return;
            }
            self.now += 2;
            self.radio.tick(self.now, false).unwrap();
        }
        panic!("RF did not reach {state:?}");
    }

    fn receive(&mut self) {
        self.radio
            .receive(&self.calibration, self.profile.receive.unwrap(), self.now)
            .unwrap();
        self.complete_configuration(State::Receiving);
    }

    fn request(&mut self, automatic: bool) {
        self.beacon.refresh(true, self.now);
        assert!(self.beacon.request(
            &self.calibration,
            self.profile,
            Some(FIX),
            self.now,
            automatic,
        ));
    }

    fn step(&mut self, ms: u32, permitted: bool, matrix: Option<u16>, ptt: (bool, bool)) {
        self.now += ms;
        let keys_still_clear = self.keys_still_clear;
        let held = self
            .beacon
            .step(
                &mut self.radio,
                &self.calibration,
                self.now,
                Some(self.profile),
                permitted,
                ptt.0,
                ptt.1,
                true,
                matrix,
                || keys_still_clear,
            )
            .unwrap();
        self.radio.tick(self.now, held).unwrap();
    }

    fn wait_in_rx(&mut self, ms: u32) {
        for _ in 0..ms / 5 {
            self.step(5, true, Some(0), (false, false));
            assert_eq!(self.radio.state(), State::Receiving, "premature keying");
            assert!(self.modem.borrow().wire.is_empty());
        }
    }

    fn wait_for_audio(&mut self) {
        for _ in 0..200 {
            self.step(5, true, Some(0), (false, false));
            if self.modem.borrow().audio {
                assert_eq!(self.radio.state(), State::Transmitting);
                return;
            }
        }
        panic!("quiet simplex did not start transmitting");
    }

    fn assert_stopped(&mut self, status: AprsStatus) {
        assert!(!self.beacon.active());
        assert_eq!(self.beacon.status(), status);
        assert!(self.beacon.take_finished());
        assert!(!self.beacon.take_finished());
        assert_eq!(self.radio.state(), State::Off);
        let rf = self.rf.0.borrow();
        assert_eq!((rf[0x30], rf[0x33], rf[0x36]), (0, 0, 0x007f));
        let modem = self.modem.borrow();
        assert!(!modem.audio);
        assert_eq!(modem.power, 0x1000);
    }
}

#[test]
fn auto_beacon_qualifies_fresh_carrier_samples_sends_one_frame_and_restores_rx() {
    let mut h = Station::new();
    // A carrier without the configured CTCSS stays muted but occupies RF.
    h.rf.0.borrow_mut()[0x0c] = 2;
    let signal = h.radio.observe(h.profile.audio, TONE).unwrap();
    assert!(signal.squelch_open);
    assert!(!signal.audio_open);
    h.request(true);
    h.wait_in_rx(600);

    h.rf.0.borrow_mut()[0x0c] = 0;
    h.wait_in_rx(150);
    // This pause would satisfy elapsed-time-only logic without observations.
    h.step(250, true, Some(0), (false, false));
    assert_eq!(h.radio.state(), State::Receiving);
    h.wait_in_rx(180);
    h.wait_for_audio();
    for _ in 0..240 {
        if !h.beacon.active() {
            break;
        }
        h.step(5, true, Some(0), (false, false));
    }
    h.assert_stopped(AprsStatus::Sent);
    let packet = h.config.position(FIX.0, FIX.1).unwrap();
    let wire = Wire::encode(&packet).unwrap();
    assert_eq!(h.modem.borrow().wire, wire.as_bytes());

    // Model root's normal re-application after take_finished(). A later
    // fresh fix alone neither rekeys nor bypasses the next quiet window.
    h.receive();
    h.step(2_000, true, Some(0), (false, false));
    h.beacon.refresh(true, h.now);
    assert_eq!(h.beacon.status(), AprsStatus::Ready);
    assert_eq!(h.radio.state(), State::Receiving);
    assert_eq!(h.modem.borrow().wire, wire.as_bytes());
    h.modem.borrow_mut().wire.clear();
    h.request(true);
    h.wait_in_rx(400);
    h.wait_for_audio();
}

#[test]
fn automatic_requires_simplex_receive_and_transmit_with_carrier_sensing() {
    let h = Station::new();
    assert!(beacon::auto_channel(&h.profile));
    for case in 0..5 {
        let mut p = h.profile;
        match case {
            0 => p.transmit.as_mut().unwrap().frequency_10hz += 60_000,
            1 => p.transmit = None,
            2 => p.receive.as_mut().unwrap().modulation = uv98_rf_sequence::Modulation::Am,
            3 => p.audio.squelch = 0,
            _ => p.audio.monitor = true,
        }
        assert!(!beacon::auto_channel(&p));
    }
}

#[test]
fn auto_rejects_held_menu_and_unreliable_matrix_before_keying() {
    for matrix in [Some(MENU), None] {
        let mut h = Station::new();
        h.request(true);
        h.step(5, true, matrix, (false, false));
        assert!(!h.beacon.active());
        assert_eq!(h.beacon.status(), AprsStatus::Failed);
        h.wait_in_rx(600);
        assert!(!h.modem.borrow().audio);
    }

    let mut manual = Station::new();
    manual.request(false);
    manual.step(5, true, Some(MENU), (false, false));
    assert!(
        manual.beacon.active(),
        "manual MENU keeps its release guard"
    );
    manual.wait_in_rx(400);
    manual.wait_for_audio();
}

#[test]
fn auto_rechecks_physical_inputs_at_commit() {
    let mut h = Station::new();
    h.request(true);
    h.wait_in_rx(400);
    h.keys_still_clear = false;
    h.wait_in_rx(600);
    assert!(!h.beacon.active());
    assert_eq!(h.beacon.status(), AprsStatus::Failed);
}

#[test]
fn auto_cancellation_removes_rf_and_audio_during_a_packet() {
    // The caller revokes the same permission when the GPS fix ages out or
    // when the operator exits the page. Both physical PTTs cancel explicitly.
    for (permitted, matrix, ptt) in [
        (false, Some(0), (false, false)),
        (true, None, (false, false)),
        (true, Some(0), (true, false)),
        (true, Some(0), (false, true)),
    ] {
        let mut h = Station::new();
        h.request(true);
        h.wait_for_audio();
        let bytes = h.modem.borrow().wire.len();
        assert!(bytes > 0);
        h.step(5, permitted, matrix, ptt);
        h.assert_stopped(AprsStatus::Failed);
        assert_eq!(h.modem.borrow().wire.len(), bytes);
    }
}

#[test]
fn rmc_route_drives_one_position_then_waits_for_a_real_turn() {
    use uv98_gps_nmea::{Parser, Position};
    use uv98_smart_beacon::SmartBeacon;

    fn rmc(parser: &mut Parser, now: u32, course: &str) {
        let body = format!("GPRMC,123519,A,4807.038,N,01131.000,E,22.4,{course},230394,003.1,W");
        let sum = body.bytes().fold(0, |sum, b| sum ^ b);
        for byte in format!("${body}*{sum:02X}\r\n").bytes() {
            parser.push(byte, now);
        }
    }
    let mut h = Station::new();
    let mut gps = Parser::new();
    let mut smart = SmartBeacon::new();
    rmc(&mut gps, h.now, "359");
    let motion = gps.motion_snapshot(h.now, 5000);
    assert!(smart.due(h.now, motion).is_none());
    smart.update_arm(Some(h.profile.frequency));
    let fix = smart.due(h.now, motion).unwrap();
    assert_eq!(
        fix.position,
        Position {
            latitude_e7: 481_173_000,
            longitude_e7: 115_166_667
        }
    );
    let position = (fix.position.latitude_e7, fix.position.longitude_e7);
    let accepted = h
        .beacon
        .request(&h.calibration, h.profile, Some(position), h.now, true);
    assert!(accepted);
    smart.request_attempted(h.now, Some(fix), accepted);
    for _ in 0..450 {
        let next = h.now + 5;
        let permitted = smart.packet_valid(next, gps.motion_snapshot(next, 5000));
        h.step(5, permitted, Some(0), (false, false));
        if !h.beacon.active() {
            break;
        }
    }
    h.assert_stopped(AprsStatus::Sent);
    let frame = h.config.position(position.0, position.1).unwrap();
    assert!(frame
        .as_bytes()
        .windows(19)
        .any(|w| w == b"4807.04N/01131.00E>"));
    assert_eq!(
        h.modem.borrow().wire,
        Wire::encode(&frame).unwrap().as_bytes()
    );
    smart.complete(h.now, h.beacon.status() == AprsStatus::Sent);
    h.now += 30_000;
    rmc(&mut gps, h.now, "001");
    assert!(smart.due(h.now, gps.motion_snapshot(h.now, 5000)).is_none());
    rmc(&mut gps, h.now + 1, "031");
    assert!(smart
        .due(h.now + 1, gps.motion_snapshot(h.now + 1, 5000))
        .is_some());
}
