/// Converts fresh battery samples to the RT880 1.16 displayed voltage scale.
/// Millivolt results have 100 mV resolution and use the stock conversion.
#[derive(Default)]
pub struct BatteryMonitor {
    previous_raw: Option<u16>,
}

impl BatteryMonitor {
    /// An invalid reading clears the filter and yields no voltage.
    pub fn observe(&mut self, raw: u16) -> Option<u16> {
        let voltage = stock_decivolts(raw);
        // Allow margin around the 7.4 V nominal pack and stock 5.8 V
        // shutdown threshold. These are validity limits, not charge levels.
        if raw > 4095 || !(40..=90).contains(&voltage) {
            self.previous_raw = None;
            return None;
        }

        let previous = self.previous_raw.replace(raw).unwrap_or(raw);
        let average = (previous + raw) / 2;
        Some(stock_decivolts(average) * 100)
    }
}

// Stock arithmetic at 0x080121D2..0x080121E4. UI consumer 0x0801C388
// renders the two decimal digits separated by a dot, followed by V.
const fn stock_decivolts(raw: u16) -> u16 {
    raw / 17 + 2
}
