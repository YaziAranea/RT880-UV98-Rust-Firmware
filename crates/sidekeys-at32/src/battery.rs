/// Converts fresh battery samples to the RT880 1.16 displayed voltage scale.
/// Millivolt results have 100 mV resolution and use the stock conversion.
#[derive(Default)]
pub struct BatteryMonitor {
    previous_raw: Option<u16>,
}

impl BatteryMonitor {
    /// An invalid reading clears the filter and yields no voltage.
    pub fn observe(&mut self, raw: u16) -> Option<u16> {
        if valid_stock_decivolts(raw).is_none() {
            self.previous_raw = None;
            return None;
        }

        let previous = self.previous_raw.replace(raw).unwrap_or(raw);
        let average = (previous + raw) / 2;
        Some(stock_decivolts(average) * 100)
    }
}

/// Tracks displayed-scale voltage sag during a single transmission.
#[derive(Default)]
pub struct BatterySag {
    resting: Option<(u32, u16)>,
    baseline_decivolts: Option<u16>,
    sag_mv: Option<u16>,
}

impl BatterySag {
    /// Stores the latest idle RX sample and its wrapping millisecond timestamp.
    pub fn observe_resting(&mut self, now: u32, raw: u16) {
        self.resting = valid_stock_decivolts(raw).map(|voltage| (now, voltage));
    }

    /// Captures an RX baseline at most 1000 ms old, including the boundary.
    pub fn begin_tx(&mut self, now: u32) {
        self.baseline_decivolts = self
            .resting
            .take()
            .filter(|&(at, _)| now.wrapping_sub(at) <= 1000)
            .map(|(_, voltage)| voltage);
        self.sag_mv = None;
    }

    /// Replaces sag with the latest TX reading, or clears it if invalid.
    pub fn observe_tx(&mut self, raw: u16) {
        self.sag_mv = self
            .baseline_decivolts
            .zip(valid_stock_decivolts(raw))
            .map(|(baseline, loaded)| baseline.saturating_sub(loaded) * 100);
    }

    pub fn millivolts(&self) -> Option<u16> {
        self.sag_mv
    }

    /// Discards RX baseline and TX measurements from the finished session.
    pub fn end_tx(&mut self) {
        *self = Self::default();
    }
}

fn valid_stock_decivolts(raw: u16) -> Option<u16> {
    let voltage = stock_decivolts(raw);
    // Allow margin around the 7.4 V nominal pack and stock 5.8 V
    // shutdown threshold. These are validity limits, not charge levels.
    (raw <= 4095 && (40..=90).contains(&voltage)).then_some(voltage)
}

// Stock arithmetic at 0x080121D2..0x080121E4. UI consumer 0x0801C388
// renders the two decimal digits separated by a dot, followed by V.
const fn stock_decivolts(raw: u16) -> u16 {
    raw / 17 + 2
}
