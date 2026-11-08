use uv98_diagnostic::{Diagnostic, LoadError};
use uv98_keys::{ButtonEvent, Key};
use uv98_nor_read::Bus;

#[derive(Default)]
struct Memory {
    address: u32,
    commands: Vec<[u8; 4]>,
    active: bool,
    fail_data: bool,
    fail_cleanup: bool,
    bad_marker: bool,
    reads: Vec<usize>,
}
impl Bus for Memory {
    type Error = &'static str;
    fn select(&mut self) -> Result<(), Self::Error> {
        assert!(!self.active);
        self.active = true;
        Ok(())
    }
    fn write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        assert!(self.active);
        assert_eq!(data.len(), 4);
        assert_eq!(data[0], 3);
        self.commands.push(data.try_into().unwrap());
        self.address = (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32;
        Ok(())
    }
    fn read(&mut self, out: &mut [u8]) -> Result<(), Self::Error> {
        assert!(self.active);
        self.reads.push(out.len());
        out.fill(0xff);
        if self.fail_data {
            out[0] = 0;
            return Err("partial");
        }
        if self.address == 0 {
            out.fill(0);
            out[0] = if self.bad_marker { 0 } else { 0x9a };
        } else {
            out[2] = 1;
            out[6..10].copy_from_slice(&43_180_000u32.to_le_bytes());
            out[32..48].fill(0xff);
            out[32..36].copy_from_slice(b"TEST");
        }
        Ok(())
    }
    fn deselect(&mut self) -> Result<(), Self::Error> {
        self.active = false;
        if self.fail_cleanup {
            Err("cleanup")
        } else {
            Ok(())
        }
    }
}

#[test]
fn physical_addresses_and_minimal_reads() {
    let mut d = Diagnostic::new();
    let mut m = Memory::default();
    d.load_calibration(&mut m).unwrap();
    d.load_channel(&mut m, 0).unwrap();
    assert_eq!(m.commands, [[3, 0, 0, 0], [3, 0, 0x40, 0]]);
    assert_eq!(m.reads, [4096, 48]);
    let s = d.snapshot().unwrap();
    assert_eq!(s.channel.rx_frequency_code(), 43_180_000);
    assert_eq!(s.channel.name_bytes(), b"TEST");
    d.load_channel(&mut m, 1023).unwrap();
    assert_eq!(m.commands.last(), Some(&[3, 0, 0xff, 0xd0]));
    assert!(core::mem::size_of::<Diagnostic>() < 4200);
}

#[test]
fn rejects_uninitialized_and_out_of_bounds_without_io() {
    let mut d = Diagnostic::new();
    let mut m = Memory::default();
    assert_eq!(
        d.load_channel(&mut m, 0),
        Err(LoadError::CalibrationNotLoaded)
    );
    assert!(m.commands.is_empty());
    d.load_calibration(&mut m).unwrap();
    for index in [1024, usize::MAX] {
        assert_eq!(
            d.load_channel(&mut m, index),
            Err(LoadError::IndexOutOfRange(index))
        );
    }
    assert_eq!(m.commands.len(), 1);
    assert!(d.snapshot().is_none());
}

#[test]
fn channel_partial_or_cleanup_error_never_exposes_old_or_partial_data() {
    let mut d = Diagnostic::new();
    let mut m = Memory::default();
    d.load_calibration(&mut m).unwrap();
    d.load_channel(&mut m, 0).unwrap();
    m.fail_data = true;
    assert!(d.load_channel(&mut m, 1).is_err());
    assert!(d.snapshot().is_none());
    assert!(!m.active);
    m.fail_data = false;
    d.load_channel(&mut m, 1).unwrap();
    m.fail_cleanup = true;
    assert!(d.load_channel(&mut m, 2).is_err());
    assert!(d.snapshot().is_none());
}

#[test]
fn bad_calibration_refresh_invalidates_session_until_success() {
    let mut d = Diagnostic::new();
    let mut m = Memory::default();
    d.load_calibration(&mut m).unwrap();
    d.load_channel(&mut m, 0).unwrap();
    m.bad_marker = true;
    assert!(d.load_calibration(&mut m).is_err());
    assert!(d.snapshot().is_none());
    assert_eq!(
        d.load_channel(&mut m, 0),
        Err(LoadError::CalibrationNotLoaded)
    );
    m.bad_marker = false;
    d.load_calibration(&mut m).unwrap();
    assert!(d.snapshot().is_none());
    d.load_channel(&mut m, 0).unwrap();
    m.fail_cleanup = true;
    assert!(d.load_calibration(&mut m).is_err());
    assert!(d.snapshot().is_none());
}

#[test]
fn navigation_uses_only_up_down_press_and_never_wraps() {
    let mut d = Diagnostic::new();
    let mut m = Memory::default();
    d.load_calibration(&mut m).unwrap();
    d.load_channel(&mut m, 0).unwrap();
    assert_eq!(d.key_target(Key::Up, ButtonEvent::Pressed), None);
    assert_eq!(d.key_target(Key::Down, ButtonEvent::Pressed), Some(1));
    for key in [Key::Ptt, Key::Digit1, Key::Menu] {
        assert_eq!(d.key_target(key, ButtonEvent::Pressed), None);
    }
    for event in [ButtonEvent::Released, ButtonEvent::Held] {
        assert_eq!(d.key_target(Key::Down, event), None);
    }
    d.load_channel(&mut m, 1023).unwrap();
    assert_eq!(d.key_target(Key::Down, ButtonEvent::Pressed), None);
    assert_eq!(d.key_target(Key::Up, ButtonEvent::Pressed), Some(1022));
}
