const QUIET_MS: u32 = 5_000;
const ATTEMPT_INTERVAL_MS: u32 = 30_000;

pub struct PositionSave {
    saved: [u32; 2],
    pending: [u32; 2],
    quiet_since: u32,
    last_attempt: Option<u32>,
}

impl PositionSave {
    pub fn new(boot: [u32; 2], now: u32) -> Self {
        Self {
            saved: boot,
            pending: boot,
            quiet_since: now,
            last_attempt: None,
        }
    }

    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn observe(&mut self, eligible: Option<[u32; 2]>, now: u32) -> Option<[u32; 2]> {
        let Some(position) = eligible else {
            self.quiet_since = now;
            return None;
        };
        if position != self.pending {
            self.pending = position;
            self.quiet_since = now;
        }
        (position != self.saved
            && now.wrapping_sub(self.quiet_since) >= QUIET_MS
            && self
                .last_attempt
                .is_none_or(|last| now.wrapping_sub(last) >= ATTEMPT_INTERVAL_MS))
        .then_some(position)
    }

    pub fn attempted(&mut self, now: u32, durable: Option<[u32; 2]>) {
        self.last_attempt = Some(now);
        if let Some(position) = durable {
            self.saved = position;
        }
    }
}
