//! Bound the quiet foreground wait for the first RX audio after voice TX.

const LISTEN_MS: u32 = 80;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReplyWindow {
    Idle,
    Tuning,
    Listening {
        ready_at: u32,
        sampled_at: Option<u32>,
    },
}

impl ReplyWindow {
    pub fn active(self) -> bool {
        self != Self::Idle
    }

    /// The deadline starts when the RX plan settles, with no assumed response
    /// or change to the carrier/tone decision. Permit one fresh sample per ms.
    pub fn sample_due(&mut self, now: u32, receiving: bool) -> bool {
        if *self == Self::Tuning && receiving {
            *self = Self::Listening {
                ready_at: now,
                sampled_at: None,
            };
        }
        let Self::Listening {
            ready_at,
            sampled_at,
        } = *self
        else {
            return false;
        };
        if !receiving || now.wrapping_sub(ready_at) >= LISTEN_MS {
            *self = Self::Idle;
            return false;
        }
        sampled_at.is_none_or(|at| now.wrapping_sub(at) >= 1)
    }

    pub fn observed(&mut self, now: u32, audio_open: bool) {
        if let Self::Listening { ready_at, .. } = *self {
            *self = if audio_open {
                Self::Idle
            } else {
                Self::Listening {
                    ready_at,
                    sampled_at: Some(now),
                }
            };
        }
    }
}
