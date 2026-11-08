//! Bounded stock024140..0242AC AM gain update; no timing or I/O.
//! Call once per active-RX foreground tick; never catch up with burst calls.
//! Stock24966 schedules on the nominal1ms TMR1 countdown (211D4/21204).
//! Actual foreground latency is not measured:250 counts calls, not elapsed ms.
//! Original24140 also requires AF mode flag00810!=0 and RX state1125==2.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct State {
    pub index: u8,
    pub previous_rssi: u16,
    pub hold: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Update {
    pub state: State,
    pub register13: u16,
    pub rssi_correction: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidIndex,
    InvalidRssi,
}

impl State {
    /// Exact reset RAM requests02E50/02E5C: gain90, previous0, hold0.
    pub const fn new() -> Self {
        Self {
            index: 90,
            previous_rssi: 0,
            hold: 0,
        }
    }

    pub fn sample(self, raw_rssi: u16) -> Result<Update, Error> {
        if !(1..=96).contains(&self.index) {
            return Err(Error::InvalidIndex);
        }
        if raw_rssi > 511 || self.previous_rssi > 511 {
            return Err(Error::InvalidRssi);
        }
        let average = if self.previous_rssi > 0 {
            (raw_rssi + self.previous_rssi) / 2
        } else {
            raw_rssi
        };
        let delta = (average as i16 - 142) / 2; // C signed truncation toward zero
        let mut index = self.index;
        let mut hold = self.hold.saturating_sub(1);
        if delta > 0 {
            if delta >= 10 {
                let target = i16::from(TABLE[usize::from(index)].1) - delta + 8;
                while index > 1 {
                    index -= 1;
                    if i16::from(TABLE[usize::from(index)].1) <= target {
                        break;
                    }
                }
            } else if index > 1 {
                index -= 1;
            }
            if index != self.index {
                hold = 250;
            }
        }
        if delta >= -6 {
            hold = 250;
        }
        if hold == 0 {
            index = index.saturating_add(1).min(96);
        }
        let (register13, gain) = TABLE[usize::from(index)];
        Ok(Update {
            state: State {
                index,
                previous_rssi: raw_rssi,
                hold,
            },
            register13,
            rssi_correction: (i16::from(gain) - i16::from(TABLE[90].1)) * 2,
        })
    }
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

// Original reference data0802EAD8,97 entries of u16 word + i8 gain.
pub const TABLE: [(u16, i8); 97] = [
    (0x03be, -7),
    (0x0000, -98),
    (0x0008, -96),
    (0x0100, -95),
    (0x0020, -93),
    (0x0001, -92),
    (0x0028, -91),
    (0x0009, -90),
    (0x0101, -89),
    (0x0030, -88),
    (0x0118, -87),
    (0x0002, -86),
    (0x0130, -85),
    (0x0019, -84),
    (0x0060, -83),
    (0x0138, -82),
    (0x0119, -81),
    (0x0058, -80),
    (0x0141, -79),
    (0x0070, -78),
    (0x0180, -77),
    (0x0139, -76),
    (0x0013, -75),
    (0x0161, -74),
    (0x01c0, -73),
    (0x00e8, -72),
    (0x00d0, -71),
    (0x0239, -70),
    (0x006a, -69),
    (0x0006, -68),
    (0x00b1, -67),
    (0x000e, -66),
    (0x015a, -65),
    (0x022b, -64),
    (0x01f8, -63),
    (0x0163, -62),
    (0x0035, -61),
    (0x0214, -60),
    (0x01d9, -59),
    (0x0145, -58),
    (0x02a2, -57),
    (0x02d1, -56),
    (0x00b3, -55),
    (0x0216, -54),
    (0x0066, -53),
    (0x00c4, -52),
    (0x006e, -51),
    (0x015d, -50),
    (0x00ad, -49),
    (0x007d, -48),
    (0x00d4, -47),
    (0x01b4, -46),
    (0x030b, -45),
    (0x00ce, -44),
    (0x01b5, -43),
    (0x0097, -42),
    (0x0257, -41),
    (0x02b4, -40),
    (0x027d, -39),
    (0x01dd, -38),
    (0x02ae, -37),
    (0x0379, -36),
    (0x035a, -35),
    (0x02b6, -34),
    (0x030e, -33),
    (0x0307, -32),
    (0x02be, -31),
    (0x037a, -30),
    (0x02de, -29),
    (0x0345, -28),
    (0x03a3, -27),
    (0x0364, -26),
    (0x032f, -25),
    (0x0393, -24),
    (0x0384, -23),
    (0x0347, -22),
    (0x03eb, -21),
    (0x03d3, -20),
    (0x03bb, -19),
    (0x037c, -18),
    (0x03cc, -17),
    (0x03c5, -16),
    (0x03ec, -15),
    (0x035f, -14),
    (0x03bc, -13),
    (0x038f, -12),
    (0x03e6, -11),
    (0x03af, -10),
    (0x03f5, -9),
    (0x03d6, -8),
    (0x03be, -7),
    (0x03f6, -6),
    (0x03de, -5),
    (0x03bf, -4),
    (0x03f7, -3),
    (0x03df, -2),
    (0x03ff, 0),
];
