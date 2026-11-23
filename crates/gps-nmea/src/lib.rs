#![no_std]
#![forbid(unsafe_code)]
//! Bounded NMEA observations, not proof of installed GPS hardware or RF support.
//! No allocation, IO, default coordinates, century inference or cross-sentence
//! metadata merging. Framing requires `$body*HH\r\n`, unlike the stock IRQ which
//! dispatches at `*HH`. Callers supply monotonic wrapping milliseconds.

pub const LINE_CAPACITY: usize = 128;
const HALF_CLOCK: u32 = 1 << 31;
const FIELD_CAPACITY: usize = 20;

/// CFG +0xD7 selector, independently traced in the stock firmware. Invalid
/// selectors fall back to 9600 there. This function does not configure a UART.
pub const fn stock_baud(selector: u8) -> u32 {
    match selector {
        0 => 4_800,
        1 => 9_600,
        2 => 14_400,
        3 => 19_200,
        4 => 38_400,
        5 => 56_000,
        6 => 57_600,
        7 => 115_200,
        8 => 128_000,
        9 => 256_000,
        _ => 9_600,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Talker {
    Gp,
    Gn,
    Bd,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gga,
    Rmc,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utc {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub day: u8,
    pub month: u8,
    pub year_2digit: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub talker: Talker,
    pub kind: Kind,
    /// Receiver validity, not a claim about accuracy or satellite-only position.
    /// Inspect quality/mode to distinguish estimated, manual or simulated data.
    pub valid: bool,
    pub position: Option<Position>,
    pub utc: Option<Utc>,
    pub date: Option<Date>,
    pub satellites: Option<u8>,
    /// HDOP times100, rounded to nearest hundredth; not metres or accuracy.
    pub hdop_centi: Option<u16>,
    pub quality: Option<u8>,
    /// RMC optional position mode, retained as its ASCII code.
    pub mode: Option<u8>,
    pub received_at_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fix {
    pub position: Position,
    pub report: Report,
}

/// Position and motion from one RMC sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionFix {
    pub position: Position,
    pub speed_centi_knots: Option<u32>,
    /// True course in hundredths of a degree, in 0..36000.
    pub course_centi_degrees: Option<u16>,
    pub received_at_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Framing,
    TooLong,
    Checksum,
    Fields,
    Coordinate,
    Time,
    Date,
    Clock,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Accepted(Report),
    Ignored,
    Rejected(Error),
}

pub struct Parser {
    line: [u8; LINE_CAPACITY],
    len: usize,
    collecting: bool,
    awaiting_lf: bool,
    last: Option<Report>,
    motion: Option<MotionFix>,
}

impl Parser {
    pub const fn new() -> Self {
        Self {
            line: [0; LINE_CAPACITY],
            len: 0,
            collecting: false,
            awaiting_lf: false,
            last: None,
            motion: None,
        }
    }
    pub const fn buffered_len(&self) -> usize {
        self.len
    }
    /// Last accepted sentence, possibly invalid or stale. Use snapshot for a
    /// usable fresh fix; do not treat this diagnostic accessor as freshness.
    pub const fn last_report(&self) -> Option<Report> {
        self.last
    }

    /// Freshness is inclusive at max_age_ms. Intervals >=2^31 and backwards
    /// timestamps are ambiguous and rejected; ordinary u32 wrap is supported.
    pub fn snapshot(&self, now_ms: u32, max_age_ms: u32) -> Option<Fix> {
        if max_age_ms >= HALF_CLOCK {
            return None;
        }
        let report = self.last?;
        let age = now_ms.wrapping_sub(report.received_at_ms);
        if !report.valid || age >= HALF_CLOCK || age > max_age_ms {
            return None;
        }
        Some(Fix {
            position: report.position?,
            report,
        })
    }

    /// Latest fresh RMC motion fix. GGA never refreshes its age or supplies
    /// its coordinates. An accepted no-fix sentence clears the motion fix.
    pub fn motion_snapshot(&self, now_ms: u32, max_age_ms: u32) -> Option<MotionFix> {
        let fix = self.motion?;
        let age = now_ms.wrapping_sub(fix.received_at_ms);
        (max_age_ms < HALF_CLOCK && age <= max_age_ms).then_some(fix)
    }

    /// Each byte performs bounded work. A new '$' resynchronizes immediately;
    /// overflow discards the partial sentence until another '$'. No partial,
    /// malformed or unsupported sentence overwrites the last accepted report.
    pub fn push(&mut self, byte: u8, now_ms: u32) -> Option<Event> {
        if byte == b'$' {
            let interrupted = self.collecting;
            self.len = 0;
            self.collecting = true;
            self.awaiting_lf = false;
            return interrupted.then_some(Event::Rejected(Error::Framing));
        }
        if !self.collecting {
            return None;
        }
        if self.awaiting_lf {
            self.collecting = false;
            self.awaiting_lf = false;
            if byte != b'\n' {
                self.len = 0;
                return Some(Event::Rejected(Error::Framing));
            }
            let parsed = parse_line(&self.line[..self.len], now_ms);
            self.len = 0;
            return Some(match parsed {
                Ok(Some((report, motion))) => {
                    if self
                        .last
                        .is_some_and(|last| now_ms.wrapping_sub(last.received_at_ms) >= HALF_CLOCK)
                    {
                        Event::Rejected(Error::Clock)
                    } else {
                        if report.kind == Kind::Rmc || !report.valid {
                            self.motion = motion;
                        }
                        self.last = Some(report);
                        Event::Accepted(report)
                    }
                }
                Ok(None) => Event::Ignored,
                Err(error) => Event::Rejected(error),
            });
        }
        if byte == b'\r' {
            self.awaiting_lf = true;
            return None;
        }
        if !(0x20..=0x7e).contains(&byte) {
            self.collecting = false;
            self.len = 0;
            return Some(Event::Rejected(Error::Framing));
        }
        if self.len == LINE_CAPACITY {
            self.collecting = false;
            self.len = 0;
            return Some(Event::Rejected(Error::TooLong));
        }
        self.line[self.len] = byte;
        self.len += 1;
        None
    }
}
impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_line(line: &[u8], now: u32) -> Result<Option<(Report, Option<MotionFix>)>, Error> {
    let star = line
        .iter()
        .position(|byte| *byte == b'*')
        .ok_or(Error::Checksum)?;
    if star + 3 != line.len() {
        return Err(Error::Checksum);
    }
    let expected = hex(line[star + 1]).ok_or(Error::Checksum)? * 16
        + hex(line[star + 2]).ok_or(Error::Checksum)?;
    if line[..star].iter().fold(0, |sum, byte| sum ^ byte) != expected {
        return Err(Error::Checksum);
    }
    let mut fields: [&[u8]; FIELD_CAPACITY] = [&[]; FIELD_CAPACITY];
    let mut count = 0;
    for field in line[..star].split(|byte| *byte == b',') {
        if count == FIELD_CAPACITY {
            return Err(Error::Fields);
        }
        fields[count] = field;
        count += 1;
    }
    let head = fields[0];
    if head.len() != 5 {
        return Err(Error::Fields);
    }
    let talker = match &head[..2] {
        b"GP" => Talker::Gp,
        b"GN" => Talker::Gn,
        b"BD" => Talker::Bd,
        _ => return Ok(None),
    };
    let kind = match &head[2..] {
        b"GGA" => Kind::Gga,
        b"RMC" => Kind::Rmc,
        _ => return Ok(None),
    };
    let f = &fields[..count];
    let mut report = Report {
        talker,
        kind,
        valid: false,
        position: None,
        utc: None,
        date: None,
        satellites: None,
        hdop_centi: None,
        quality: None,
        mode: None,
        received_at_ms: now,
    };
    let mut motion = None;
    match kind {
        Kind::Gga => {
            if f.len() != 15 {
                return Err(Error::Fields);
            }
            report.utc = time(f[1])?;
            let quality = one_digit(f[6]).filter(|q| *q <= 8).ok_or(Error::Fields)?;
            report.quality = Some(quality);
            report.valid = quality != 0;
            report.position = position(f[2], f[3], f[4], f[5], report.valid)?;
            report.satellites = optional_integer(f[7], 255)?.map(|value| value as u8);
            report.hdop_centi = optional_centi(f[8], u16::MAX as u32)?.map(|value| value as u16);
            signed_decimal(f[9])?;
            signed_decimal(f[11])?;
            if !matches!(f[10], b"" | b"M") || !matches!(f[12], b"" | b"M") {
                return Err(Error::Fields);
            }
            optional_centi(f[13], u32::MAX)?;
            optional_integer(f[14], 4095)?;
        }
        Kind::Rmc => {
            if !(12..=14).contains(&f.len()) {
                return Err(Error::Fields);
            }
            report.utc = time(f[1])?;
            report.valid = match f[2] {
                b"A" => true,
                b"V" => false,
                _ => return Err(Error::Fields),
            };
            if f.len() >= 13 && !f[12].is_empty() {
                if f[12].len() != 1 || !b"ADEFMNPRS".contains(&f[12][0]) {
                    return Err(Error::Fields);
                }
                let mode = f[12][0];
                report.mode = Some(mode);
                if mode == b'N' {
                    report.valid = false;
                }
            }
            if f.len() == 14 && !matches!(f[13], b"" | b"S" | b"C" | b"U" | b"V") {
                return Err(Error::Fields);
            }
            report.position = position(f[3], f[4], f[5], f[6], report.valid)?;
            let speed_centi_knots = optional_centi(f[7], u32::MAX)?;
            let course_centi_degrees =
                optional_centi(f[8], 36_000)?
                    .map(|course| if course == 36_000 { 0 } else { course as u16 });
            report.date = date(f[9])?;
            optional_centi(f[10], 18_000)?;
            if !matches!(f[11], b"" | b"E" | b"W") || (!f[10].is_empty() && f[11].is_empty()) {
                return Err(Error::Fields);
            }
            // Dead reckoning, manual and simulated positions remain available
            // as ordinary reports, but cannot drive automatic transmission.
            if !matches!(report.mode, Some(b'E' | b'M' | b'S')) {
                motion = report.position.map(|position| MotionFix {
                    position,
                    speed_centi_knots,
                    course_centi_degrees,
                    received_at_ms: now,
                });
            }
        }
    }
    Ok(Some((report, motion)))
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
fn one_digit(bytes: &[u8]) -> Option<u8> {
    (bytes.len() == 1 && bytes[0].is_ascii_digit()).then(|| bytes[0] - b'0')
}
fn integer(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}
fn optional_integer(bytes: &[u8], maximum: u32) -> Result<Option<u32>, Error> {
    if bytes.is_empty() {
        return Ok(None);
    }
    integer(bytes)
        .filter(|value| *value <= maximum)
        .map(Some)
        .ok_or(Error::Fields)
}
/// Decimal whole part and fraction in billionths. NMEA allows up to nine
/// fractional digits. Keep them all for time, position limits and rounding.
fn decimal(bytes: &[u8]) -> Option<(u32, u32)> {
    let dot = bytes.iter().position(|&b| b == b'.').unwrap_or(bytes.len());
    let whole = integer(&bytes[..dot])?;
    let Some(fraction) = bytes.get(dot + 1..) else {
        return Some((whole, 0));
    };
    if fraction.len() > 9 {
        return None;
    }
    let mut billionths = integer(fraction)?;
    for _ in fraction.len()..9 {
        billionths *= 10;
    }
    Some((whole, billionths))
}

fn optional_centi(bytes: &[u8], maximum: u32) -> Result<Option<u32>, Error> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let (whole, fraction) = decimal(bytes).ok_or(Error::Fields)?;
    whole
        .checked_mul(100)
        .and_then(|base| base.checked_add((fraction + 5_000_000) / 10_000_000))
        .filter(|&scaled| scaled <= maximum)
        .map(Some)
        .ok_or(Error::Fields)
}
fn signed_decimal(bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() {
        return Ok(());
    }
    let bytes = bytes.strip_prefix(b"-").unwrap_or(bytes);
    decimal(bytes).map(|_| ()).ok_or(Error::Fields)
}
fn coordinate(bytes: &[u8], hemisphere: &[u8], latitude: bool) -> Result<i32, Error> {
    let degrees_digits = if latitude { 2 } else { 3 };
    let whole_len = bytes
        .iter()
        .position(|byte| *byte == b'.')
        .unwrap_or(bytes.len());
    if whole_len != degrees_digits + 2 {
        return Err(Error::Coordinate);
    }
    let degrees = integer(&bytes[..degrees_digits]).ok_or(Error::Coordinate)?;
    let (minutes, fraction) = decimal(&bytes[degrees_digits..]).ok_or(Error::Coordinate)?;
    let maximum = if latitude { 90 } else { 180 };
    if degrees > maximum || minutes >= 60 || (degrees == maximum && (minutes != 0 || fraction != 0))
    {
        return Err(Error::Coordinate);
    }
    let negative = match (latitude, hemisphere) {
        (true, b"N") | (false, b"E") => false,
        (true, b"S") | (false, b"W") => true,
        _ => return Err(Error::Coordinate),
    };
    // Convert minutes to units of 1e-7 minutes, then round once to 1e-7
    // degrees. The discarded fraction is <1, so it cannot cross the next
    // integer threshold when dividing by 60.
    let minute_e7 = minutes * 10_000_000 + fraction / 100;
    let value = (degrees * 10_000_000 + (minute_e7 + 30) / 60) as i32;
    Ok(if negative { -value } else { value })
}
fn position(
    lat: &[u8],
    ns: &[u8],
    lon: &[u8],
    ew: &[u8],
    valid: bool,
) -> Result<Option<Position>, Error> {
    if !valid && lat.is_empty() && ns.is_empty() && lon.is_empty() && ew.is_empty() {
        return Ok(None);
    }
    let value = Position {
        latitude_e7: coordinate(lat, ns, true)?,
        longitude_e7: coordinate(lon, ew, false)?,
    };
    Ok(valid.then_some(value))
}
fn time(bytes: &[u8]) -> Result<Option<Utc>, Error> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let whole_len = bytes
        .iter()
        .position(|byte| *byte == b'.')
        .unwrap_or(bytes.len());
    if whole_len != 6 {
        return Err(Error::Time);
    }
    let hour = integer(&bytes[..2]).ok_or(Error::Time)? as u8;
    let minute = integer(&bytes[2..4]).ok_or(Error::Time)? as u8;
    let (second, nanosecond) = decimal(&bytes[4..]).ok_or(Error::Time)?;
    if hour > 23 || minute > 59 || second > 60 || (second == 60 && (hour != 23 || minute != 59)) {
        return Err(Error::Time);
    }
    Ok(Some(Utc {
        hour,
        minute,
        second: second as u8,
        nanosecond,
    }))
}
fn date(bytes: &[u8]) -> Result<Option<Date>, Error> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != 6 {
        return Err(Error::Date);
    }
    let day = integer(&bytes[..2]).ok_or(Error::Date)? as u8;
    let month = integer(&bytes[2..4]).ok_or(Error::Date)? as u8;
    let year_2digit = integer(&bytes[4..]).ok_or(Error::Date)? as u8;
    let maximum = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year_2digit % 4 == 0 => 29,
        2 => 28,
        _ => return Err(Error::Date),
    };
    if day == 0 || day > maximum {
        return Err(Error::Date);
    }
    Ok(Some(Date {
        day,
        month,
        year_2digit,
    }))
}

#[cfg(test)]
mod tests;
