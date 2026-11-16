//! The central 40-channel CB grid in its 5 kHz and 0 kHz variants.

// EU/CEPT channel numbers from Ofcom IR 2027.2, Table 3.1. The zero-ending
// variant is 5 kHz lower (see OPTIM Truck, frequency standard 0/5).
const CEPT_KHZ: [u16; 40] = [
    26_965, 26_975, 26_985, 27_005, 27_015, 27_025, 27_035, 27_055, 27_065, 27_075, 27_085, 27_105,
    27_115, 27_125, 27_135, 27_155, 27_165, 27_175, 27_185, 27_205, 27_215, 27_225, 27_255, 27_235,
    27_245, 27_265, 27_275, 27_285, 27_295, 27_305, 27_315, 27_325, 27_335, 27_345, 27_355, 27_365,
    27_375, 27_385, 27_395, 27_405,
];

pub const CHANNEL_COUNT: u8 = CEPT_KHZ.len() as u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CbGrid {
    Zeros,
    Cept,
}

impl CbGrid {
    pub const fn other(self) -> Self {
        match self {
            Self::Zeros => Self::Cept,
            Self::Cept => Self::Zeros,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CbChannel {
    number: u8,
    pub grid: CbGrid,
    pub am: bool,
}

impl Default for CbChannel {
    fn default() -> Self {
        // OPTIM Truck's road channel shortcut is channel 15E, 27.135 MHz AM.
        Self {
            number: 15,
            grid: CbGrid::Cept,
            am: true,
        }
    }
}

impl CbChannel {
    pub const fn number(self) -> u8 {
        self.number
    }

    pub const fn with_number(self, number: u8) -> Option<Self> {
        if number == 0 || number > CHANNEL_COUNT {
            return None;
        }
        Some(Self { number, ..self })
    }

    pub const fn frequency_10hz(self) -> u32 {
        let khz = match self.grid {
            CbGrid::Zeros => CEPT_KHZ[self.number as usize - 1] - 5,
            CbGrid::Cept => CEPT_KHZ[self.number as usize - 1],
        };
        khz as u32 * 100
    }

    pub const fn is_am(self) -> bool {
        self.am
    }

    pub fn adjacent(self, up: bool) -> Self {
        let number = match (up, self.number) {
            (true, CHANNEL_COUNT) => 1,
            (true, number) => number + 1,
            (false, 1) => CHANNEL_COUNT,
            (false, number) => number - 1,
        };
        Self { number, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_keep_channel_numbers_and_exact_five_khz_offset() {
        let cept = CbChannel::default();
        assert_eq!(cept.frequency_10hz(), 2_713_500);
        assert_eq!(cept.number(), 15);
        assert!(cept.is_am());
        let zeros = CbChannel {
            grid: CbGrid::Zeros,
            ..cept
        };
        assert_eq!(zeros.frequency_10hz(), 2_713_000);
        for number in 1..=CHANNEL_COUNT {
            let zero_frequency = zeros.with_number(number).unwrap().frequency_10hz();
            let cept_frequency = cept.with_number(number).unwrap().frequency_10hz();
            assert_eq!(zero_frequency % 1_000, 0);
            assert_eq!(cept_frequency - zero_frequency, 500);
        }
        for (number, frequency) in [
            (1, 2_696_500),
            (23, 2_725_500),
            (24, 2_723_500),
            (25, 2_724_500),
            (40, 2_740_500),
        ] {
            assert_eq!(
                cept.with_number(number).unwrap().frequency_10hz(),
                frequency
            );
        }
        assert!(zeros.with_number(0).is_none());
        assert!(zeros.with_number(41).is_none());
        assert_eq!(
            zeros.with_number(1).unwrap().adjacent(false).number(),
            CHANNEL_COUNT
        );
        assert_eq!(
            zeros
                .with_number(CHANNEL_COUNT)
                .unwrap()
                .adjacent(true)
                .number(),
            1
        );
    }
}
