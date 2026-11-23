#[path = "../../../firmware-diagnostic/src/aprs_rx.rs"]
mod aprs_rx;

use aprs_rx::Receiver;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use uv98_signaling::{
    aprs::{Address, Config, Wire},
    cmx868::{Bus, Error, Modem},
};

#[derive(Default)]
struct Registers {
    bytes: VecDeque<u8>,
    overflow: bool,
    missing: bool,
    resets: usize,
}

#[derive(Clone, Default)]
struct Chip(Rc<RefCell<Registers>>);
impl Bus for Chip {
    fn reset(&mut self) -> Result<(), Error> {
        let mut r = self.0.borrow_mut();
        r.bytes.clear();
        r.resets += 1;
        Ok(())
    }
    fn write16(&mut self, _: u8, _: u16) -> Result<(), Error> {
        Ok(())
    }
    fn write8(&mut self, _: u8, _: u8) -> Result<(), Error> {
        panic!("APRS RX wrote the TX data buffer")
    }
    fn read16(&mut self, reg: u8) -> Result<u16, Error> {
        assert_eq!(reg, 0xe6);
        let r = self.0.borrow();
        if r.missing {
            return Ok(0xffff);
        }
        Ok(0x1400 | (u16::from(!r.bytes.is_empty()) << 6) | (u16::from(r.overflow) << 5))
    }
    fn read8(&mut self, reg: u8) -> Result<u8, Error> {
        assert_eq!(reg, 0xe5);
        let mut r = self.0.borrow_mut();
        r.overflow = false;
        Ok(r.bytes.pop_front().unwrap_or_default())
    }
    fn tx_audio(&mut self, enabled: bool) {
        assert!(!enabled, "APRS RX enabled TX audio");
    }
}

const FREQ: u32 = 14_480_000;
struct Station {
    chip: Chip,
    modem: Modem<Chip>,
    rx: Receiver,
    now: u32,
}
impl Station {
    fn new() -> Self {
        let chip = Chip::default();
        Self {
            modem: Modem::new(chip.clone()),
            chip,
            rx: Receiver::new(0),
            now: 0,
        }
    }
    fn step(&mut self, open: bool, freq: Option<u32>) -> Option<(Vec<u8>, Vec<u8>)> {
        self.now += 1;
        self.rx
            .service(Some(&mut self.modem), self.now, open, freq)
            .map(|p| (p.source().to_vec(), p.information().to_vec()))
    }
    fn listen(&mut self, freq: u32) {
        for _ in 0..500 {
            self.step(true, Some(freq));
            if self.rx.listening() {
                return;
            }
        }
        panic!("CMX receiver did not become ready");
    }
    fn feed(&mut self, freq: u32, bytes: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        bytes
            .iter()
            .filter_map(|b| {
                self.chip.0.borrow_mut().bytes.push_back(*b);
                self.step(true, Some(freq))
            })
            .collect()
    }
}

#[test]
fn modem_to_packet_recovers_from_overrun_retune_and_screen_exit() {
    let config = Config {
        source: Address::new(b"N0CALL", 7).unwrap(),
        paths: [None; 2],
        symbol_table: b'/',
        symbol: b'>',
        comment: [0; 32],
        comment_len: 0,
    };
    let frame = config.position(0, 0).unwrap();
    let wire = Wire::encode(&frame).unwrap();
    let bytes = wire.as_bytes();
    let split = bytes.len() - 10;
    let mut station = Station::new();
    assert!(station.rx.stats_due(1).is_none());
    assert!(!station.rx.needs_audio());
    station.step(true, Some(FREQ));
    assert!(station.rx.needs_audio());
    assert!(!station.rx.listening());
    station.listen(FREQ);
    assert!(station.rx.needs_audio());
    assert_eq!(station.feed(FREQ, bytes).len(), 1);
    assert!(station.feed(FREQ, &bytes[..split]).is_empty());
    station.now += 6; // Next step reaches the 7 ms receive deadline.
    assert!(station.feed(FREQ, &bytes[split..]).is_empty());
    assert!(station.feed(FREQ, &bytes[..split]).is_empty());
    station.chip.0.borrow_mut().overflow = true;
    assert!(station.feed(FREQ, &bytes[split..]).is_empty());
    assert!(station.feed(FREQ, &bytes[..split]).is_empty());
    station.listen(FREQ + 100);
    assert!(station.feed(FREQ + 100, &bytes[split..]).is_empty());
    assert!(station.feed(FREQ + 100, &bytes[..split]).is_empty());
    station.step(false, None);
    assert!(!station.rx.listening());
    assert!(!station.rx.needs_audio());
    station.listen(FREQ);
    assert!(station.feed(FREQ, &bytes[split..]).is_empty());
    let mut damaged = config.position(0, 0).unwrap();
    damaged.bytes[16] ^= 1; // Keep the original FCS, encode a clean HDLC envelope.
    assert!(station
        .feed(FREQ, Wire::encode(&damaged).unwrap().as_bytes())
        .is_empty());
    let packets = station.feed(FREQ, bytes);
    let stats = station.rx.stats_due(2000).unwrap();
    assert!(station.rx.stats_due(2999).is_none());
    assert_eq!(station.rx.stats_due(3000), Some(stats));
    assert!(stats.bytes >= 3 * bytes.len() as u32);
    assert_eq!(
        (
            stats.packets,
            stats.checksum_errors,
            stats.poll_gaps,
            stats.overruns
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(
        packets,
        vec![(
            b"N0CALL-7".to_vec(),
            frame.as_bytes()[16..frame.len - 2].to_vec()
        )]
    );
}

#[test]
fn failed_probe_waits_for_reentry_and_never_displays_random_data() {
    let mut station = Station::new();
    station.chip.0.borrow_mut().missing = true;
    for _ in 0..1000 {
        assert!(station.step(true, Some(FREQ)).is_none());
    }
    assert_eq!(station.chip.0.borrow().resets, 1);
    assert!(!station.rx.listening());
    assert!(!station.rx.needs_audio());
    station.chip.0.borrow_mut().missing = false;
    station.step(false, None);
    station.listen(FREQ);
    assert!(station.feed(FREQ, &[0xa5; 500]).is_empty());
}
