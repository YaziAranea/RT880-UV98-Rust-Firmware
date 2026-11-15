//! AX.25 UI frames and uncompressed APRS positions, with HDLC/NRZI wire data.
//! References: TAPR AX.25 v2.2 sections3.6-3.8; APRS101 chapter6.
//! No invented coordinates/callsign, auto-beacon or unattended RF authorization.

pub const FRAME_CAPACITY: usize = 192;
pub const WIRE_CAPACITY: usize = 288;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Address,
    Coordinate,
    Space,
    Frame,
    Checksum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address {
    call: [u8; 6],
    ssid: u8,
}
impl Address {
    pub fn new(call: &[u8], ssid: u8) -> Result<Self, Error> {
        let end = call
            .iter()
            .position(|c| matches!(*c, 0 | 0xff | b' '))
            .unwrap_or(call.len());
        if end == 0 || end > 6 || ssid > 15 {
            return Err(Error::Address);
        }
        if call[end..].iter().any(|c| !matches!(*c, 0 | 0xff | b' ')) {
            return Err(Error::Address);
        }
        let mut result = Self {
            call: [b' '; 6],
            ssid,
        };
        for (i, c) in call[..end].iter().enumerate() {
            let c = c.to_ascii_uppercase();
            if !c.is_ascii_uppercase() && !c.is_ascii_digit() {
                return Err(Error::Address);
            }
            result.call[i] = c;
        }
        Ok(result)
    }
    pub const fn callsign(&self) -> &[u8; 6] {
        &self.call
    }
    pub const fn ssid(&self) -> u8 {
        self.ssid
    }
    fn encode(&self, out: &mut [u8], last: bool, command: bool) {
        for (dest, c) in out[..6].iter_mut().zip(self.call) {
            *dest = c << 1;
        }
        out[6] = 0x60 | (self.ssid << 1) | u8::from(last) | if command { 0x80 } else { 0 };
    }
}

#[derive(Clone, Copy)]
pub struct Config {
    pub source: Address,
    pub paths: [Option<Address>; 2],
    pub symbol_table: u8,
    pub symbol: u8,
    pub comment: [u8; 32],
    pub comment_len: u8,
}
impl Config {
    /// CPS RT8801.10 CFG layout; strictly reject invalid identity/path. A bad
    /// configured path is not silently turned into a different RF route.
    pub fn from_stock(cfg: &[u8]) -> Result<Self, Error> {
        if cfg.len() < 3318 {
            return Err(Error::Frame);
        }
        Self::from_stock_tail(&cfg[3120..3318])
    }
    /// The198-byte CFG3120..3318 region, so firmware need not put a4KiB
    /// codeplug buffer on its stack just to read an APRS configuration.
    pub fn from_stock_tail(cfg: &[u8]) -> Result<Self, Error> {
        if cfg.len() < 198 {
            return Err(Error::Frame);
        }
        let source = Address::new(&cfg[0..6], cfg[6])?;
        let mut paths = [None; 2];
        for (i, start) in [139, 146].into_iter().enumerate() {
            if !matches!(cfg[start], 0 | 0xff | b' ') {
                paths[i] = Some(Address::new(&cfg[start..start + 6], cfg[start + 6])?);
            }
        }
        let mut comment = [0; 32];
        let mut n = 0;
        for c in &cfg[9..137] {
            if *c == 0 || *c == 0xff || n == 32 {
                break;
            }
            // Stock uses GBK; APRS comment here intentionally only printable ASCII.
            if !(0x20..=0x7e).contains(c) {
                continue;
            }
            comment[n] = *c;
            n += 1;
        }
        // CPS symbol index0 maps ASCII'!'; out-of-range selects portable radio.
        let symbol = if cfg[8] < 94 { cfg[8] + 33 } else { b'[' };
        Ok(Self {
            source,
            paths,
            symbol_table: if cfg[7] == 1 { b'\\' } else { b'/' },
            symbol,
            comment,
            comment_len: n as u8,
        })
    }
    pub fn position(&self, latitude_e7: i32, longitude_e7: i32) -> Result<Frame, Error> {
        if latitude_e7.unsigned_abs() > 900_000_000 || longitude_e7.unsigned_abs() > 1_800_000_000 {
            return Err(Error::Coordinate);
        }
        let mut frame = Frame {
            bytes: [0; FRAME_CAPACITY],
            len: 0,
        };
        let destination = Address::new(b"APRS", 0)?;
        let mut addresses = [None; 4];
        addresses[0] = Some(destination);
        addresses[1] = Some(self.source);
        let mut count = 2;
        for path in self.paths.into_iter().flatten() {
            addresses[count] = Some(path);
            count += 1;
        }
        for (i, address) in addresses[..count].iter().enumerate() {
            address.unwrap().encode(
                &mut frame.bytes[frame.len..frame.len + 7],
                i + 1 == count,
                i == 0,
            );
            frame.len += 7;
        }
        frame.push(0x03)?;
        frame.push(0xf0)?;
        frame.push(b'!')?;
        coordinate(&mut frame, latitude_e7, false)?;
        frame.push(self.symbol_table)?;
        coordinate(&mut frame, longitude_e7, true)?;
        frame.push(self.symbol)?;
        for c in &self.comment[..usize::from(self.comment_len).min(32)] {
            frame.push(*c)?;
        }
        let crc = fcs(&frame.bytes[..frame.len]);
        frame.push(crc as u8)?;
        frame.push((crc >> 8) as u8)?;
        Ok(frame)
    }
}
fn coordinate(frame: &mut Frame, e7: i32, longitude: bool) -> Result<(), Error> {
    let hundredth_minutes = (u64::from(e7.unsigned_abs()) * 6000 + 5_000_000) / 10_000_000;
    let degree = hundredth_minutes / 6000;
    let minute = hundredth_minutes % 6000;
    if longitude {
        frame.push(b'0' + (degree / 100) as u8)?;
    }
    frame.push(b'0' + ((degree / 10) % 10) as u8)?;
    frame.push(b'0' + (degree % 10) as u8)?;
    frame.push(b'0' + (minute / 1000) as u8)?;
    frame.push(b'0' + ((minute / 100) % 10) as u8)?;
    frame.push(b'.')?;
    frame.push(b'0' + ((minute / 10) % 10) as u8)?;
    frame.push(b'0' + (minute % 10) as u8)?;
    frame.push(if longitude {
        if e7 < 0 {
            b'W'
        } else {
            b'E'
        }
    } else if e7 < 0 {
        b'S'
    } else {
        b'N'
    })
}

pub struct Frame {
    pub bytes: [u8; FRAME_CAPACITY],
    pub len: usize,
}
impl Frame {
    fn push(&mut self, byte: u8) -> Result<(), Error> {
        if self.len >= FRAME_CAPACITY {
            return Err(Error::Space);
        }
        self.bytes[self.len] = byte;
        self.len += 1;
        Ok(())
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
/// CRC-16/X-25, reflected0x8408, initFFFF, xoroutFFFF; wire low byte first.
pub fn fcs(data: &[u8]) -> u16 {
    let mut crc = 0xffff;
    for byte in data {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub struct Wire {
    pub bytes: [u8; WIRE_CAPACITY],
    pub len: usize,
}
impl Wire {
    /// 300ms preamble at1200bps, three tail flags, bit stuffing before NRZI.
    /// CMX868 transmits buffer bytes bit0 first. One wire bit selects one tone.
    pub fn encode(frame: &Frame) -> Result<Self, Error> {
        if frame.len < 18 || frame.len > FRAME_CAPACITY {
            return Err(Error::Frame);
        }
        let mut encoder = Encoder {
            wire: Self {
                bytes: [0; WIRE_CAPACITY],
                len: 0,
            },
            bits: 0,
            tone: true,
            ones: 0,
        };
        for _ in 0..45 {
            encoder.flag()?;
        }
        for byte in frame.as_bytes() {
            for shift in 0..8 {
                let bit = byte & (1 << shift) != 0;
                encoder.bit(bit)?;
                if bit {
                    encoder.ones += 1;
                    if encoder.ones == 5 {
                        encoder.bit(false)?;
                        encoder.ones = 0;
                    }
                } else {
                    encoder.ones = 0;
                }
            }
        }
        for _ in 0..3 {
            encoder.flag()?;
        }
        // Pad with the BEGINNING of another flag, not all-one abort bits.
        let mut shift = 0;
        while encoder.bits % 8 != 0 {
            encoder.bit(0x7e & (1 << shift) != 0)?;
            shift += 1;
        }
        encoder.wire.len = encoder.bits / 8;
        Ok(encoder.wire)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
struct Encoder {
    wire: Wire,
    bits: usize,
    tone: bool,
    ones: u8,
}
impl Encoder {
    fn bit(&mut self, bit: bool) -> Result<(), Error> {
        if self.bits >= WIRE_CAPACITY * 8 {
            return Err(Error::Space);
        }
        if !bit {
            self.tone = !self.tone;
        }
        if self.tone {
            self.wire.bytes[self.bits / 8] |= 1 << (self.bits % 8);
        }
        self.bits += 1;
        Ok(())
    }
    fn flag(&mut self) -> Result<(), Error> {
        for shift in 0..8 {
            self.bit(0x7e & (1 << shift) != 0)?;
        }
        self.ones = 0;
        Ok(())
    }
}

/// Bounded streaming HDLC receiver for CMX868 synchronous NRZI bits. Returned
/// frame includes FCS and is overwritten by the next completed frame.
pub struct Decoder {
    raw: [u8; WIRE_CAPACITY],
    bits: usize,
    shift: u8,
    last_tone: bool,
    in_frame: bool,
    frame: Frame,
}
impl Decoder {
    pub const fn new() -> Self {
        Self {
            raw: [0; WIRE_CAPACITY],
            bits: 0,
            shift: 0,
            last_tone: true,
            in_frame: false,
            frame: Frame {
                bytes: [0; FRAME_CAPACITY],
                len: 0,
            },
        }
    }
    pub fn push_byte(&mut self, byte: u8) -> Option<Result<&[u8], Error>> {
        let mut event = None;
        for shift in 0..8 {
            let tone = byte & (1 << shift) != 0;
            let bit = tone == self.last_tone;
            self.last_tone = tone;
            self.shift = (self.shift >> 1) | if bit { 0x80 } else { 0 };
            if self.in_frame {
                if self.bits >= WIRE_CAPACITY * 8 {
                    self.in_frame = false;
                    self.bits = 0;
                    event = Some(Err(Error::Space));
                } else {
                    let target = &mut self.raw[self.bits / 8];
                    if self.bits % 8 == 0 {
                        *target = 0;
                    }
                    if bit {
                        *target |= 1 << (self.bits % 8);
                    }
                    self.bits += 1;
                }
            }
            if self.shift == 0x7e {
                if self.in_frame && self.bits > 8 {
                    event = Some(self.finish(self.bits - 8));
                }
                self.in_frame = true;
                self.bits = 0;
            }
        }
        event.map(|result| result.map(|()| self.frame.as_bytes()))
    }
    fn finish(&mut self, bits: usize) -> Result<(), Error> {
        self.frame.len = 0;
        let mut ones = 0;
        let mut octet = 0;
        let mut count = 0;
        for bit_index in 0..bits {
            let bit = self.raw[bit_index / 8] & (1 << (bit_index % 8)) != 0;
            if ones == 5 {
                if bit {
                    return Err(Error::Frame);
                }
                ones = 0;
                continue;
            }
            if bit {
                ones += 1;
                octet |= 1 << count;
            } else {
                ones = 0;
            }
            count += 1;
            if count == 8 {
                self.frame.push(octet)?;
                count = 0;
                octet = 0;
            }
        }
        if count != 0 || self.frame.len < 18 {
            return Err(Error::Frame);
        }
        let n = self.frame.len;
        let expected = u16::from_le_bytes([self.frame.bytes[n - 2], self.frame.bytes[n - 1]]);
        if fcs(&self.frame.bytes[..n - 2]) != expected {
            return Err(Error::Checksum);
        }
        Ok(())
    }
}
impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            source: Address::new(b"N0CALL", 7).unwrap(),
            paths: [Some(Address::new(b"WIDE1", 1).unwrap()), None],
            symbol_table: b'/',
            symbol: b'[',
            comment: [0; 32],
            comment_len: 0,
        }
    }
    #[test]
    fn crc_known_check_and_position_roundtrip() {
        assert_eq!(fcs(b"123456789"), 0x906e);
        let frame = config().position(490_583_333, -720_291_667).unwrap();
        assert_eq!(&frame.as_bytes()[23..43], b"!4903.50N/07201.75W[");
        let wire = Wire::encode(&frame).unwrap();
        let mut decoder = Decoder::new();
        let mut found = false;
        for byte in wire.as_bytes() {
            if let Some(Ok(decoded)) = decoder.push_byte(*byte) {
                assert_eq!(decoded, frame.as_bytes());
                found = true;
            }
        }
        assert!(found);
    }
    #[test]
    fn coordinate_rounding_carries_and_bounds_reject() {
        let frame = config().position(899_999_999, 1_799_999_999).unwrap();
        assert_eq!(&frame.as_bytes()[23..43], b"!9000.00N/18000.00E[");
        assert!(config().position(i32::MIN, 0).is_err());
        assert!(Address::new(b"N0CALL", 16).is_err());
        assert!(Address::new(b"A B", 0).is_err());
    }
    #[test]
    fn stuffed_ff_payload_decodes_without_abort() {
        let mut frame = config().position(0, 0).unwrap();
        for byte in &mut frame.bytes[23..40] {
            *byte = 0xff;
        }
        let n = frame.len;
        let crc = fcs(&frame.bytes[..n - 2]);
        frame.bytes[n - 2..n].copy_from_slice(&crc.to_le_bytes());
        let wire = Wire::encode(&frame).unwrap();
        let mut decoder = Decoder::new();
        let mut count = 0;
        for byte in wire.as_bytes() {
            if let Some(Ok(decoded)) = decoder.push_byte(*byte) {
                assert_eq!(decoded, frame.as_bytes());
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }
    #[test]
    fn small_stock_slice_matches_full_layout_and_rejects_bad_frame_length() {
        let mut cfg = [0u8; 3318];
        cfg[3120..3126].copy_from_slice(b"N0CALL");
        cfg[3126] = 7;
        cfg[3259..3264].copy_from_slice(b"WIDE1");
        cfg[3265] = 1;
        let full = Config::from_stock(&cfg).unwrap();
        let tail = Config::from_stock_tail(&cfg[3120..3318]).unwrap();
        assert_eq!(full.source, tail.source);
        assert_eq!(tail.paths[0], Some(Address::new(b"WIDE1", 1).unwrap()));
        assert!(Config::from_stock_tail(&cfg[3120..3317]).is_err());
        let mut frame = full.position(0, 0).unwrap();
        frame.len = FRAME_CAPACITY + 1;
        assert!(matches!(Wire::encode(&frame), Err(Error::Frame)));
    }
}
