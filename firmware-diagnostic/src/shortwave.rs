use at32f4xx_pac::at32f423::{CRM, GPIOC, I2C1};
use cortex_m::peripheral::DWT;
use uv98_board_pins::ShortwavePins;

const ACK_FAIL: u32 = 1 << 4;
const STOP: u32 = 1 << 5;
const BUS_ERRORS: u32 = 0x1f00;
const BUSY: u32 = 1 << 15;
const CTS: u8 = 0x80;
const COMMAND_ERROR: u8 = 0x40;

pub struct Bus {
    i2c: I2C1,
    _pins: ShortwavePins,
}

impl Bus {
    pub fn new(i2c: I2C1, pins: ShortwavePins, crm: &mut CRM) -> Self {
        crm.piclks().modify(|_, w| unsafe { w.i2c1sel().bits(0) });
        crm.apb1en().modify(|_, w| w.i2c1().set_bit());
        let _ = crm.apb1en().read().bits();
        // SAFETY: the token grants PC0/PC1. GPIOC clock is already enabled.
        // RMW leaves the keyboard, RF bus, mux and output latches alone.
        unsafe {
            let gpio = GPIOC::ptr().cast::<u32>().cast_mut();
            for (offset, mask, bits) in [(4, 3, 3), (12, 15, 5), (0x20, 0xff, 0x77), (0, 15, 10)] {
                let reg = gpio.add(offset / 4);
                reg.write_volatile((reg.read_volatile() & !mask) | bits);
            }
        }
        Self { i2c, _pins: pins }
    }

    pub fn start(&mut self, crm: &mut CRM) {
        crm.apb1rst().modify(|_, w| w.i2c1().set_bit());
        crm.apb1rst().modify(|_, w| w.i2c1().clear_bit());
        // PCLK1 = 24 MHz. PRESC=1, SCLH=SCLL=60 gives about 98 kHz.
        self.i2c.clkctrl().write(|w| unsafe { w.bits(0x1053_3c3c) });
        self.i2c.ctrl1().write(|w| unsafe { w.bits(1) });
    }

    pub fn stop(&mut self) {
        self.i2c.ctrl1().write(|w| unsafe { w.bits(0) });
    }

    fn wait(&self, mask: u32, set: bool, started: u32) -> Result<(), u8> {
        loop {
            let status = self.i2c.sts().read().bits();
            if status & ACK_FAIL != 0 {
                return Err(1);
            }
            if status & BUS_ERRORS != 0 {
                return Err(2);
            }
            if (status & mask != 0) == set {
                return Ok(());
            }
            if DWT::cycle_count().wrapping_sub(started) >= 240_000 {
                return Err(3); // 5 ms per whole transaction at 48 MHz
            }
        }
    }

    fn clear_flags(&self) {
        self.i2c
            .clr()
            .write(|w| unsafe { w.bits(ACK_FAIL | STOP | BUS_ERRORS) });
    }

    fn begin(&self, len: usize, read: bool, started: u32) -> Result<(), u8> {
        self.clear_flags();
        self.wait(BUSY, false, started)?;
        // 7-bit address 0x11, NBYTES, hardware STOP and START.
        let control = 0x22 | (u32::from(read) << 10) | (1 << 13) | ((len as u32) << 16) | (1 << 25);
        self.i2c.ctrl2().write(|w| unsafe { w.bits(control) });
        Ok(())
    }
}

impl uv98_radio_runtime::shortwave::Transport for Bus {
    fn write(&mut self, command: &[u8]) -> Result<(), u8> {
        let started = DWT::cycle_count();
        self.begin(command.len(), false, started)?;
        for &byte in command {
            self.wait(1 << 1, true, started)?;
            self.i2c
                .txdt()
                .write(|w| unsafe { w.bits(u32::from(byte)) });
        }
        self.wait(STOP, true, started)?;
        self.clear_flags();
        Ok(())
    }

    fn read<const N: usize>(&mut self) -> Result<[u8; N], u8> {
        let started = DWT::cycle_count();
        self.begin(N, true, started)?;
        let mut reply = [0; N];
        for byte in &mut reply {
            self.wait(1 << 2, true, started)?;
            *byte = self.i2c.rxdt().read().bits() as u8;
        }
        self.wait(STOP, true, started)?;
        self.clear_flags();
        if reply[0] & COMMAND_ERROR != 0 {
            return Err(4);
        }
        if reply[0] & CTS == 0 {
            return Err(uv98_radio_runtime::shortwave::CTS_PENDING);
        }
        Ok(reply)
    }
}
