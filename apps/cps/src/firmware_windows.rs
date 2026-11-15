//! Exclusive bounded Win32 loader transport. Kept separate from read-only CPS transport.

use std::{io, mem, ptr, time::Duration};

use windows_sys::Win32::Devices::Communication::{
    GetCommState, SetCommState, SetCommTimeouts, SetupComm, COMMTIMEOUTS, DCB, NOPARITY, ONESTOPBIT,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING,
};

use crate::firmware::Transport;
fn port_number(name: &str) -> Option<u16> {
    let digits = name.strip_prefix("COM")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u16>().ok().filter(|n| *n != 0)
}

fn milliseconds(duration: Duration) -> u32 {
    // Round up positive submillisecond intervals rather than accidentally
    // selecting an unbounded/instant Windows timeout combination.
    duration
        .as_millis()
        .saturating_add(u128::from(duration.subsec_nanos() % 1_000_000 != 0))
        .min(u128::from(u32::MAX - 1)) as u32
}

pub(super) struct WindowsPort {
    handle: Option<HANDLE>,
}

impl WindowsPort {
    pub(super) fn open(name: &str) -> io::Result<Self> {
        let number = port_number(name).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Expected COM1..COM65535")
        })?;
        let path: Vec<u16> = format!(r"\\.\COM{number}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: nul-terminated live UTF-16 path; synchronous exclusive handle,
        // no security/template pointers. Used exclusively by explicit Firmware Start.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut port = Self {
            handle: Some(handle),
        };
        port.configure()?;
        Ok(port)
    }

    fn handle(&self) -> io::Result<HANDLE> {
        self.handle
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Serial handle is closed"))
    }

    fn configure(&mut self) -> io::Result<()> {
        let handle = self.handle()?;
        let mut dcb = DCB {
            DCBlength: mem::size_of::<DCB>() as u32,
            ..DCB::default()
        };
        // SAFETY: valid open serial handle and valid writable DCB.
        if unsafe { GetCommState(handle, &mut dcb) } == 0 {
            return Err(io::Error::last_os_error());
        }
        dcb.DCBlength = mem::size_of::<DCB>() as u32;
        dcb.BaudRate = 115_200;
        dcb.ByteSize = 8;
        dcb.Parity = NOPARITY;
        dcb.StopBits = ONESTOPBIT;
        // fBinary bit0; DTR and RTS disabled, matching the proven flash writer.
        // No CTS/DSR/XON/XOFF flow control; RTS_CONTROL_DISABLE, no error filtering.
        dcb._bitfield = 0x01;
        dcb.XonChar = 0x11;
        dcb.XoffChar = 0x13;
        // SAFETY: DCB has the native size, supported scalar settings and live handle.
        if unsafe { SetCommState(handle, &dcb) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: native host-side serial queue sizes; no radio command is sent.
        if unsafe { SetupComm(handle, 4096, 4096) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.timeouts(Duration::from_millis(200), Duration::from_secs(5))
    }

    fn timeouts(&mut self, read: Duration, send: Duration) -> io::Result<()> {
        let timeouts = COMMTIMEOUTS {
            ReadIntervalTimeout: u32::MAX,
            ReadTotalTimeoutMultiplier: 0,
            ReadTotalTimeoutConstant: milliseconds(read),
            WriteTotalTimeoutMultiplier: 0,
            WriteTotalTimeoutConstant: milliseconds(send).max(1),
        };
        // SAFETY: native fixed struct and live synchronous handle.
        if unsafe { SetCommTimeouts(self.handle()?, &timeouts) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Transport for WindowsPort {
    fn read_byte(&mut self, timeout: Duration) -> io::Result<Option<u8>> {
        self.timeouts(timeout, Duration::from_secs(5))?;
        let mut byte = 0u8;
        let mut count = 0u32;
        // SAFETY: writable single byte and count, unique live synchronous handle.
        if unsafe { ReadFile(self.handle()?, &mut byte, 1, &mut count, ptr::null_mut()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((count == 1).then_some(byte))
    }

    fn write_frame(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // Engine emits only the fixed three erase frames and 1028-byte blocks.
        if bytes.len() != 5 && bytes.len() != 1028 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid loader frame size",
            ));
        }
        self.timeouts(Duration::from_secs(5), Duration::from_secs(5))?;
        let mut count = 0;
        // SAFETY: live immutable buffer, bounded native length, unique handle.
        if unsafe {
            WriteFile(
                self.handle()?,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut count,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(count as usize)
    }

    fn close(&mut self) -> io::Result<()> {
        if let Some(handle) = self.handle.take() {
            // SAFETY: this owner closes its unique native handle once.
            if unsafe { CloseHandle(handle) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

impl Drop for WindowsPort {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
