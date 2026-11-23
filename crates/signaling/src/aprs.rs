//! AX.25 UI frames and uncompressed APRS positions, with HDLC/NRZI wire data.
//! References: TAPR AX.25 v2.2 sections3.6-3.8; APRS101 chapter6.
//! No invented coordinates/callsign, auto-beacon or unattended RF authorization.

pub const FRAME_CAPACITY: usize = 192;
pub const WIRE_CAPACITY: usize = 288;

const ADDRESS_LEN: usize = 7;
const _: () = assert!(FRAME_CAPACITY >= 4 * ADDRESS_LEN + 22 + 32 + 2);
const MAX_DIGIPEATER_ADDRESSES: usize = 8;
const MAX_INFORMATION_LEN: usize = 256;
const RX_FRAME_CAPACITY: usize =
    (2 + MAX_DIGIPEATER_ADDRESSES) * ADDRESS_LEN + 2 + MAX_INFORMATION_LEN + 2;
// Worst case: one stuffed zero every five data bits, plus the closing flag.
const RX_WIRE_CAPACITY: usize = (RX_FRAME_CAPACITY * 8 + RX_FRAME_CAPACITY * 8 / 5 + 8).div_ceil(8);

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
    fn encode(&self, out: &mut [u8; ADDRESS_LEN], command: bool) {
        for (dest, c) in out.iter_mut().zip(self.call) {
            *dest = c << 1;
        }
        out[6] = 0x60 | (self.ssid << 1) | if command { 0x80 } else { 0 };
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
        const DESTINATION: Address = Address {
            call: *b"APRS  ",
            ssid: 0,
        };
        let (addresses, _) = frame.bytes.as_chunks_mut::<ADDRESS_LEN>();
        DESTINATION.encode(&mut addresses[0], true);
        self.source.encode(&mut addresses[1], false);
        let mut count = 2;
        for path in self.paths.into_iter().flatten() {
            path.encode(&mut addresses[count], false);
            count += 1;
        }
        addresses[count - 1][ADDRESS_LEN - 1] |= 1;
        frame.len = count * ADDRESS_LEN;
        // Header, position, comment and FCS are at most 84 bytes.
        frame.push(0x03);
        frame.push(0xf0);
        frame.push(b'!');
        coordinate(&mut frame, latitude_e7, false);
        frame.push(self.symbol_table);
        coordinate(&mut frame, longitude_e7, true);
        frame.push(self.symbol);
        for c in &self.comment[..usize::from(self.comment_len).min(32)] {
            frame.push(*c);
        }
        let crc = fcs(&frame.bytes[..frame.len]);
        frame.push(crc as u8);
        frame.push((crc >> 8) as u8);
        Ok(frame)
    }
}
fn rounded_hundredth_minutes(degrees_e7: u32) -> u32 {
    // Round degrees * 6000. Reducing the ratio to 3/5000 and splitting
    // whole units from the remainder avoids overflow and a wide division.
    3 * (degrees_e7 / 5000) + (3 * (degrees_e7 % 5000) + 2500) / 5000
}

fn coordinate(frame: &mut Frame, e7: i32, longitude: bool) {
    let hundredth_minutes = rounded_hundredth_minutes(e7.unsigned_abs());
    let degree = hundredth_minutes / 6000;
    let minute = hundredth_minutes % 6000;
    if longitude {
        frame.push(b'0' + (degree / 100) as u8);
    }
    frame.push(b'0' + ((degree / 10) % 10) as u8);
    frame.push(b'0' + (degree % 10) as u8);
    frame.push(b'0' + (minute / 1000) as u8);
    frame.push(b'0' + ((minute / 100) % 10) as u8);
    frame.push(b'.');
    frame.push(b'0' + ((minute / 10) % 10) as u8);
    frame.push(b'0' + (minute % 10) as u8);
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
    });
}

pub struct Frame {
    pub bytes: [u8; FRAME_CAPACITY],
    pub len: usize,
}
impl Frame {
    #[inline(never)]
    fn push(&mut self, byte: u8) {
        self.bytes[self.len] = byte;
        self.len += 1;
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

/// AX.25 UI packet with an APRS data type identifier and raw information bytes.
pub struct ReceivedPacket<'a> {
    source: [u8; 9],
    source_len: usize,
    information: &'a [u8],
}
impl ReceivedPacket<'_> {
    pub fn source(&self) -> &[u8] {
        &self.source[..self.source_len]
    }

    pub fn information(&self) -> &[u8] {
        self.information
    }
}

/// Parses an AX.25 frame (including FCS) already checked by `Decoder`.
/// The APRS data type is identified here. Its body is kept byte-for-byte.
pub fn received_packet(frame_with_fcs: &[u8]) -> Option<ReceivedPacket<'_>> {
    let body = frame_with_fcs.get(..frame_with_fcs.len().checked_sub(2)?)?;
    let destination = body.get(..ADDRESS_LEN)?;
    if destination[6] & 1 != 0 {
        return None;
    }

    let mut addresses = body[ADDRESS_LEN..].chunks_exact(ADDRESS_LEN);
    let source_address = addresses.next()?;
    let mut source_len = received_callsign_len(source_address)?;
    let mut last_address = source_address;
    let mut address_end = 2 * ADDRESS_LEN;
    for _ in 0..MAX_DIGIPEATER_ADDRESSES {
        if last_address[6] & 1 != 0 {
            break;
        }
        last_address = addresses.next()?;
        received_callsign_len(last_address)?;
        address_end += ADDRESS_LEN;
    }
    if last_address[6] & 1 == 0 {
        return None;
    }

    let [0x03, 0xf0, information @ ..] = body.get(address_end..)? else {
        return None;
    };
    if information.len() > MAX_INFORMATION_LEN {
        return None;
    }
    match *information.first()? {
        // Early Mic-E encoded latitude and flags as shifted printable
        // characters, including punctuation and lowercase.
        0x1c | 0x1d | b'\'' | b'`' => {
            if !destination[..6]
                .iter()
                .all(|&byte| byte & 1 == 0 && (b' '..=b'~').contains(&(byte >> 1)))
            {
                return None;
            }
        }
        // APRS 1.0.1, chapter 5, including the obsolete grid locator and
        // user-defined packets.
        kind if b"!#$%)*,/:;<=>?@T[_{}".contains(&kind) => {
            received_callsign_len(destination)?;
        }
        _ => return None,
    }

    let mut source = [0; 9];
    for (decoded, encoded) in source[..source_len].iter_mut().zip(source_address) {
        *decoded = encoded >> 1;
    }
    let ssid = (source_address[6] >> 1) & 0x0f;
    if ssid != 0 {
        source[source_len] = b'-';
        source_len += 1;
        if ssid >= 10 {
            source[source_len] = b'1';
            source_len += 1;
        }
        source[source_len] = b'0' + ssid % 10;
        source_len += 1;
    }
    Some(ReceivedPacket {
        source,
        source_len,
        information,
    })
}

fn received_callsign_len(address: &[u8]) -> Option<usize> {
    let mut len = 0;
    let mut padding = false;
    for &byte in &address[..6] {
        if byte & 1 != 0 {
            return None;
        }
        let character = byte >> 1;
        if character == b' ' {
            padding = true;
        } else {
            if padding || !(character.is_ascii_uppercase() || character.is_ascii_digit()) {
                return None;
            }
            len += 1;
        }
    }
    (len != 0).then_some(len)
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
    raw: [u8; RX_WIRE_CAPACITY],
    bits: usize,
    shift: u8,
    last_tone: bool,
    in_frame: bool,
    frame: [u8; RX_FRAME_CAPACITY],
}
impl Decoder {
    pub const fn new() -> Self {
        Self {
            raw: [0; RX_WIRE_CAPACITY],
            bits: 0,
            shift: 0xff,
            last_tone: true,
            in_frame: false,
            frame: [0; RX_FRAME_CAPACITY],
        }
    }
    /// Discards partial HDLC state after a gap in the NRZI stream.
    pub fn reset(&mut self) {
        self.bits = 0;
        self.shift = 0xff;
        self.last_tone = true;
        self.in_frame = false;
    }

    pub fn push_byte(&mut self, byte: u8) -> Option<Result<&[u8], Error>> {
        let mut event = None;
        for shift in 0..8 {
            let tone = byte & (1 << shift) != 0;
            let bit = tone == self.last_tone;
            self.last_tone = tone;
            self.shift = (self.shift >> 1) | if bit { 0x80 } else { 0 };
            if self.in_frame {
                if self.shift & 0xfe == 0xfe {
                    self.in_frame = false;
                    self.bits = 0;
                    event.get_or_insert(Err(Error::Frame));
                } else if self.bits >= RX_WIRE_CAPACITY * 8 {
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
        event.map(|result| result.map(|len| &self.frame[..len]))
    }
    fn finish(&mut self, bits: usize) -> Result<usize, Error> {
        let mut len = 0;
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
                *self.frame.get_mut(len).ok_or(Error::Space)? = octet;
                len += 1;
                count = 0;
                octet = 0;
            }
        }
        if count != 0 || ones == 5 || len < 18 {
            return Err(Error::Frame);
        }
        let expected = u16::from_le_bytes([self.frame[len - 2], self.frame[len - 1]]);
        if fcs(&self.frame[..len - 2]) != expected {
            return Err(Error::Checksum);
        }
        Ok(len)
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
    extern crate std;
    use self::std::vec::Vec;

    // Fixed NRZI vector generated by an independent bit serializer using the
    // non-reflected 0x1021 CRC polynomial and explicit bit reversal.
    const MIC_E_FRAME: &[u8] = &[
        0xc8, 0x52, 0x4a, 0x42, 0xc8, 0x44, 0xe0, 0xae, 0x84, 0x68, 0x82, 0xa0, 0xa4, 0x7a, 0xae,
        0x92, 0x88, 0x8a, 0x62, 0x40, 0xe3, 0x03, 0xf0, 0x1d, 0x1c, 0x53, 0x1f, 0x7e, 0x1c, 0x1c,
        0x5b, 0x1d, 0x00, 0xff, 0x81, 0x20, 0x1e, 0x9e, 0x7d,
    ];
    const MIC_E_NRZI: &[u8] = &[
        0x80, 0x12, 0x9b, 0x6c, 0x6b, 0xed, 0x96, 0x0a, 0xcf, 0xd6, 0x72, 0x2b, 0x35, 0xc9, 0x7c,
        0xcf, 0x24, 0x2d, 0xd3, 0x74, 0x95, 0x0b, 0x54, 0xf5, 0x7b, 0x85, 0x6e, 0x7e, 0x05, 0x16,
        0xea, 0xc5, 0x19, 0xaa, 0xfa, 0x41, 0xa5, 0x16, 0x14, 0xc4, 0x2f, 0x20, 0xe0,
    ];

    fn received_frame(paths: usize, information: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.extend(b"APRS  ".iter().map(|&c| c << 1));
        frame.push(0xe0);
        frame.extend(b"N0ABC ".iter().map(|&c| c << 1));
        frame.push(0x60 | u8::from(paths == 0));
        for i in 0..paths {
            frame.extend(b"WIDE1 ".iter().map(|&c| c << 1));
            frame.push(0xe2 | u8::from(i + 1 == paths));
        }
        frame.extend([0x03, 0xf0]);
        frame.extend(information);
        frame.extend(fcs(&frame).to_le_bytes());
        frame
    }

    fn flag(bits: &mut Vec<bool>) {
        bits.extend([false, true, true, true, true, true, true, false]);
    }

    fn stuffed_bits(bits: &mut Vec<bool>, frame: &[u8]) {
        let mut ones = 0;
        for bit in frame
            .iter()
            .flat_map(|octet| (0..8).map(move |offset| octet & (1 << offset) != 0))
        {
            bits.push(bit);
            if bit {
                ones += 1;
                if ones == 5 {
                    bits.push(false);
                    ones = 0;
                }
            } else {
                ones = 0;
            }
        }
    }

    fn pack_nrzi(bits: &[bool]) -> Vec<u8> {
        let mut tone = true;
        bits.chunks(8)
            .map(|chunk| {
                chunk.iter().enumerate().fold(0, |byte, (i, &bit)| {
                    tone ^= !bit;
                    byte | (u8::from(tone) << i)
                })
            })
            .collect()
    }

    #[test]
    fn independent_mic_e_vector_preserves_binary_and_source_ssid() {
        let mut decoder = Decoder::new();
        let mut frames = 0;
        for &byte in MIC_E_NRZI {
            if let Some(event) = decoder.push_byte(byte) {
                let frame = event.unwrap();
                assert_eq!(frame, MIC_E_FRAME);
                let packet = received_packet(frame).unwrap();
                assert_eq!(packet.source(), b"WB4APR-13");
                assert_eq!(packet.information(), &MIC_E_FRAME[23..37]);
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn validates_aprs_envelope_addresses_and_information_bounds() {
        let info = b":N0CALL   :test";
        let frame = received_frame(1, info);
        assert_eq!(received_packet(&frame).unwrap().information(), info);
        assert_eq!(received_packet(&frame).unwrap().source(), b"N0ABC");
        let mut single_digit = frame.clone();
        single_digit[13] |= 7 << 1;
        assert_eq!(received_packet(&single_digit).unwrap().source(), b"N0ABC-7");

        for (index, invalid) in [
            (0, b'A' << 1 | 1), // Address characters have no extension bit.
            (6, 0xe1),          // Destination cannot end the address field.
            (7, b' ' << 1),     // No embedded or leading spaces in a callsign.
            (13, 0x61),         // Source cannot end the path before its digipeater.
            (14, b'?' << 1),    // Digipeater address must be a shifted callsign.
            (20, 0xe2),         // Missing end of address field.
            (21, 0x00),         // AX.25 I-frame instead of UI.
            (22, 0xcf),         // NET/ROM instead of APRS.
            (23, b'H'),         // Plain AX.25 text begins with an unknown DTI.
        ] {
            let mut bad = frame.clone();
            bad[index] = invalid;
            assert!(received_packet(&bad).is_none(), "field at byte {index}");
        }
        assert!(received_packet(&received_frame(0, b"")).is_none());
        assert!(received_packet(&received_frame(9, info)).is_none());
        assert!(received_packet(&received_frame(0, &[b'>'; 257])).is_none());
        for n in 0..18 {
            assert!(received_packet(&frame[..n]).is_none());
        }
    }

    #[test]
    fn bit_stuffing_at_max_lengths_and_resync_after_invalid_frames() {
        let mut information = [0xff; 256];
        information[0] = b'{';
        let max_frame = received_frame(8, &information);
        assert_eq!(max_frame.len(), 330);
        let mut oversize = max_frame.clone();
        oversize.push(0xff);
        let mut bad_fcs = MIC_E_FRAME.to_vec();
        bad_fcs[30] ^= 1;

        let mut bits = Vec::from([true, false, true]);
        flag(&mut bits);
        stuffed_bits(&mut bits, &bad_fcs);
        flag(&mut bits);
        stuffed_bits(&mut bits, &oversize);
        flag(&mut bits);
        bits.extend([true; 15]);
        flag(&mut bits);
        bits.extend((0..RX_WIRE_CAPACITY * 8 + 1).map(|i| i % 2 == 0));
        flag(&mut bits);
        stuffed_bits(&mut bits, &max_frame);
        flag(&mut bits);
        stuffed_bits(&mut bits, MIC_E_FRAME);
        flag(&mut bits);

        let mut decoder = Decoder::new();
        let mut errors = Vec::new();
        let mut frames = Vec::new();
        for byte in pack_nrzi(&bits) {
            match decoder.push_byte(byte) {
                Some(Ok(frame)) => frames.push(frame.to_vec()),
                Some(Err(error)) => errors.push(error),
                None => {}
            }
        }
        assert_eq!(
            errors,
            [Error::Checksum, Error::Space, Error::Frame, Error::Space]
        );
        assert_eq!(frames, [max_frame, MIC_E_FRAME.to_vec()]);
        assert_eq!(
            received_packet(&frames[0]).unwrap().information(),
            information
        );
    }

    #[test]
    fn gap_reset_discards_old_partial_frame() {
        let mut decoder = Decoder::new();
        for &byte in &MIC_E_NRZI[..15] {
            assert!(decoder.push_byte(byte).is_none());
        }
        decoder.reset();
        for &byte in &MIC_E_NRZI[15..] {
            assert!(decoder.push_byte(byte).is_none());
        }
        decoder.reset();
        let frames = MIC_E_NRZI
            .iter()
            .filter_map(|&byte| {
                decoder
                    .push_byte(byte)
                    .map(|frame| frame.map(<[u8]>::to_vec))
            })
            .collect::<Vec<_>>();
        assert_eq!(frames, [Ok(MIC_E_FRAME.to_vec())]);
    }

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
#[test]
fn coordinate_rounding_matches_the_wide_formula() {
    for base in [0, 1, 9_999_999, 899_995_000, 1_799_995_000, 2_147_480_000] {
        for offset in 0..5000 {
            let degrees_e7 = base + offset;
            assert_eq!(
                u64::from(rounded_hundredth_minutes(degrees_e7)),
                (u64::from(degrees_e7) * 6000 + 5_000_000) / 10_000_000
            );
        }
    }
}
