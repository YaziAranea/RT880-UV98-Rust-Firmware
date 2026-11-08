//! Small Win32 transport. Device writes are reachable only with an opaque
//! allowlisted ReadPacket produced by the protocol engine.

use std::{io, mem, ptr, time::Duration};

use windows_sys::Win32::Devices::Communication::{
    GetCommState, SetCommState, SetCommTimeouts, SetupComm, COMMTIMEOUTS, DCB, NOPARITY, ONESTOPBIT,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, QueryDosDeviceW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING,
};

use crate::{is_allowed_packet, port_number, ReadPacket, Transport};

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
        // no security/template pointers. GENERIC_WRITE permits READ requests;
        // the higher-level protocol never sends a flash write/erase command.
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
        // fBinary bit0 + fDtrControl=DTR_CONTROL_ENABLE at bits4..5.
        // No CTS/DSR/XON/XOFF flow control; RTS_CONTROL_DISABLE, no error filtering.
        dcb._bitfield = 0x11;
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
        self.timeouts(Duration::from_millis(50), Duration::from_secs(1))
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
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.timeouts(timeout, Duration::from_secs(1))?;
        let size = u32::try_from(buffer.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Read buffer too large"))?;
        let mut count = 0;
        // SAFETY: buffer is writable for size bytes, count is writable, handle
        // is synchronous and valid; null OVERLAPPED is required for this handle.
        if unsafe {
            ReadFile(
                self.handle()?,
                buffer.as_mut_ptr(),
                size,
                &mut count,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(count as usize)
    }

    fn send(&mut self, packet: &ReadPacket, timeout: Duration) -> io::Result<usize> {
        let bytes = packet.as_bytes();
        if !is_allowed_packet(bytes) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Packet outside read-only allowlist",
            ));
        }
        self.timeouts(Duration::from_millis(50), timeout)?;
        let mut count = 0;
        // SAFETY: immutable packet buffer remains live, at most five bytes,
        // count is writable, and the synchronous handle uses null OVERLAPPED.
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

pub(super) fn list_ports() -> io::Result<Vec<String>> {
    let mut capacity = 16_384;
    loop {
        let mut buffer = vec![0u16; capacity];
        // SAFETY: null name requests the DOS device-name MULTI_SZ list; buffer
        // is writable for capacity UTF-16 words. This never opens a COM port.
        let length = unsafe { QueryDosDeviceW(ptr::null(), buffer.as_mut_ptr(), capacity as u32) };
        if length != 0 {
            let mut ports: Vec<(u16, String)> = buffer[..length as usize]
                .split(|value| *value == 0)
                .filter_map(|part| String::from_utf16(part).ok())
                .filter_map(|name| {
                    port_number(&name).map(|number| (number, format!("COM{number}")))
                })
                .collect();
            ports.sort_by_key(|(number, _)| *number);
            ports.dedup_by_key(|(number, _)| *number);
            return Ok(ports.into_iter().map(|(_, name)| name).collect());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INSUFFICIENT_BUFFER as i32) && capacity < 1_048_576 {
            capacity *= 2;
        } else {
            return Err(error);
        }
    }
}
