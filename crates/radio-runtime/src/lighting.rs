//! Independent, volatile screen/keypad timers. Activity comes from physical
//! keys, never from RSSI changes, received audio or repainting the display.
use uv98_handheld_ui::controller::LightMode;

#[derive(Clone, Copy, Debug)]
struct Timer {
    mode: LightMode,
    active_at: u32,
    expired: bool,
}
impl Timer {
    fn new(now: u32) -> Self {
        Self {
            mode: LightMode::On,
            active_at: now,
            expired: false,
        }
    }
    fn configure(&mut self, mode: LightMode, now: u32) {
        self.mode = mode;
        self.activity(now);
    }
    fn activity(&mut self, now: u32) {
        self.active_at = now;
        self.expired = false;
    }
    fn level(&mut self, now: u32) -> bool {
        match self.mode {
            LightMode::Off => false,
            LightMode::On => true,
            _ => {
                if let Some(duration) = self.mode.timeout_ms() {
                    self.expired |= now.wrapping_sub(self.active_at) >= duration;
                } else {
                    self.expired = true;
                }
                !self.expired
            }
        }
    }
}

pub struct Lights {
    display: Timer,
    keys: Timer,
}
impl Lights {
    pub fn new(now: u32) -> Self {
        Self {
            display: Timer::new(now),
            keys: Timer::new(now),
        }
    }
    pub fn set_display(&mut self, mode: LightMode, now: u32) {
        self.display.configure(mode, now);
    }
    pub fn set_keys(&mut self, mode: LightMode, now: u32) {
        self.keys.configure(mode, now);
    }
    pub fn modes(&self) -> (LightMode, LightMode) {
        (self.display.mode, self.keys.mode)
    }
    pub fn activity(&mut self, now: u32) {
        self.display.activity(now);
        self.keys.activity(now);
    }
    /// Called every foreground pass, including after both lamps have expired.
    pub fn levels(&mut self, now: u32) -> (bool, bool) {
        (self.display.level(now), self.keys.level(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independently_expire_and_physical_activity_restarts_both() {
        let mut lights = Lights::new(0);
        lights.set_display(LightMode::Seconds20, 0);
        lights.set_keys(LightMode::Seconds5, 0);
        assert_eq!(lights.levels(4999), (true, true));
        assert_eq!(lights.levels(5000), (true, false));
        assert_eq!(lights.levels(20_000), (false, false));
        lights.activity(20_001);
        assert_eq!(lights.levels(20_001), (true, true));
        assert_eq!(lights.levels(25_001), (true, false));
    }

    #[test]
    fn off_remains_off_and_reconfiguring_one_does_not_extend_the_other() {
        let mut lights = Lights::new(0);
        lights.set_keys(LightMode::Seconds5, 0);
        lights.set_display(LightMode::Off, 4900);
        assert_eq!(lights.levels(5000), (false, false));
        lights.activity(5001);
        assert_eq!(lights.levels(5001), (false, true));
        lights.set_keys(LightMode::On, 5002);
        assert_eq!(lights.levels(99_000), (false, true));
    }

    #[test]
    fn wrap_is_supported_and_expired_lamps_never_relight_without_activity() {
        let start = u32::MAX - 2000;
        let mut lights = Lights::new(start);
        lights.set_display(LightMode::Seconds5, start);
        assert_eq!(lights.levels(start.wrapping_add(4999)), (true, true));
        assert_eq!(lights.levels(start.wrapping_add(5000)), (false, true));
        assert_eq!(lights.levels(start), (false, true));
        lights.activity(start);
        assert_eq!(lights.levels(start), (true, true));
    }
}
