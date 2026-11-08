use std::{cell::RefCell, rc::Rc};
use uv98_board_diagnostic::{Error, Session, State};
use uv98_display::{Delay, DisplayBus};
use uv98_keys::{ButtonEvent, Key};
use uv98_nor_read::Bus;
#[derive(Default)]
struct Trace {
    reads: Vec<(u32, usize)>,
    lcd_commands: usize,
    selected: bool,
    fail_read: bool,
    fail_lcd: bool,
}
struct Memory {
    trace: Rc<RefCell<Trace>>,
    address: u32,
}
impl Bus for Memory {
    type Error = ();
    fn select(&mut self) -> Result<(), ()> {
        self.trace.borrow_mut().selected = true;
        Ok(())
    }
    fn write(&mut self, b: &[u8]) -> Result<(), ()> {
        assert_eq!(b[0], 3);
        self.address = u32::from_be_bytes([0, b[1], b[2], b[3]]);
        Ok(())
    }
    fn read(&mut self, b: &mut [u8]) -> Result<(), ()> {
        let mut t = self.trace.borrow_mut();
        t.reads.push((self.address, b.len()));
        if t.fail_read {
            return Err(());
        }
        b.fill(0);
        if self.address == 0 {
            b[0] = 0x9a;
        } else {
            b[2] = 1;
            b[32..].fill(0xff);
        }
        Ok(())
    }
    fn deselect(&mut self) -> Result<(), ()> {
        self.trace.borrow_mut().selected = false;
        Ok(())
    }
}
struct Lcd(Rc<RefCell<Trace>>);
impl DisplayBus for Lcd {
    type Error = ();
    fn reset(&mut self, _: bool) -> Result<(), ()> {
        Ok(())
    }
    fn command(&mut self, _: u8) -> Result<(), ()> {
        let mut t = self.0.borrow_mut();
        t.lcd_commands += 1;
        if t.fail_lcd {
            Err(())
        } else {
            Ok(())
        }
    }
    fn data(&mut self, _: &[u8]) -> Result<(), ()> {
        Ok(())
    }
}
struct FakeDelay;
impl Delay for FakeDelay {
    fn delay_ms(&mut self, _: u32) {}
}
fn setup() -> (Session<Memory, Lcd, FakeDelay>, Rc<RefCell<Trace>>) {
    let t = Rc::new(RefCell::new(Trace::default()));
    (
        Session::new(
            Memory {
                trace: t.clone(),
                address: 0,
            },
            Lcd(t.clone()),
            FakeDelay,
        ),
        t,
    )
}
#[test]
fn session_reads_cal_once_and_only_navigates_on_confirmed_keys() {
    let (mut s, t) = setup();
    assert_eq!(s.state(), State::New);
    assert!(t.borrow().reads.is_empty());
    s.start(0).unwrap();
    assert_eq!(s.state(), State::Ready);
    assert_eq!(s.snapshot().unwrap().index(), 0);
    assert!(!s.navigate(Key::Ptt, ButtonEvent::Pressed).unwrap());
    assert!(!s.navigate(Key::Down, ButtonEvent::Held).unwrap());
    assert!(s.navigate(Key::Down, ButtonEvent::Pressed).unwrap());
    assert_eq!(t.borrow().reads, [(0, 4096), (0x4000, 48), (0x4030, 48)]);
    assert_eq!(s.start(0), Err(Error::State));
    assert_eq!(s.snapshot().unwrap().index(), 1);
}
#[test]
fn calibration_failure_prevents_lcd_start_and_retry() {
    let (mut s, t) = setup();
    t.borrow_mut().fail_read = true;
    assert!(s.start(0).is_err());
    assert_eq!(s.state(), State::Failed);
    assert_eq!(t.borrow().lcd_commands, 0);
    assert!(!t.borrow().selected);
    assert_eq!(s.start(0), Err(Error::State));
    assert!(s.snapshot().is_none());
}
#[test]
fn failed_navigation_closes_session_and_hides_old_snapshot() {
    let (mut s, t) = setup();
    s.start(0).unwrap();
    t.borrow_mut().fail_read = true;
    assert!(s.navigate(Key::Down, ButtonEvent::Pressed).is_err());
    assert_eq!(s.state(), State::Failed);
    assert!(s.snapshot().is_none());
    assert_eq!(s.navigate(Key::Up, ButtonEvent::Pressed), Err(Error::State));
}
#[test]
fn lcd_error_never_reports_ready() {
    let (mut s, t) = setup();
    t.borrow_mut().fail_lcd = true;
    assert!(s.start(0).is_err());
    assert_eq!(s.state(), State::Failed);
    assert!(s.snapshot().is_none());
}

#[test]
fn nor_and_lcd_configuration_commute_on_shared_gpio_b() {
    type L = uv98_lcd_at32::GpioRegisters;
    fn nor(v: L) -> L {
        let n = uv98_board_at32::GpioRegisters {
            cfgr: v.cfgr,
            omode: v.omode,
            odrvr: v.odrvr,
            pull: v.pull,
            muxh: v.muxh,
        }
        .with_spi2_nor();
        L {
            cfgr: n.cfgr,
            omode: n.omode,
            odrvr: n.odrvr,
            pull: n.pull,
            muxh: n.muxh,
            muxl: v.muxl,
        }
    }
    for seed in [0, u32::MAX, 0xa5a5a5a5, 0x12345678] {
        let initial = L {
            cfgr: seed,
            omode: seed,
            odrvr: seed,
            pull: seed,
            muxh: seed,
            muxl: seed,
        };
        let both = nor(initial).with_spi3();
        assert_eq!(both, nor(initial.with_spi3()));
        assert_eq!(both.muxh & 0xfff00000, 0x55500000);
        assert_eq!(both.muxl & 0x00f0f000, 0x00606000);
    }
}
