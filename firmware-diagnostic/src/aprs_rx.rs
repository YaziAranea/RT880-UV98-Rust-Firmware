use uv98_signaling::aprs::{received_packet, Decoder, ReceivedPacket};
use uv98_signaling::cmx868::{Bus, Error, Modem, Progress};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Starting,
    Listening,
    Failed,
}

pub struct Receiver {
    gain: u8,
    phase: Phase,
    frequency: Option<u32>,
    last_poll_ms: u32,
    decoder: Decoder,
}

impl Receiver {
    pub const fn new(gain: u8) -> Self {
        Self {
            gain,
            phase: Phase::Idle,
            frequency: None,
            last_poll_ms: 0,
            decoder: Decoder::new(),
        }
    }

    pub fn listening(&self) -> bool {
        self.phase == Phase::Listening
    }

    /// Prepare FM audio during modem power-up, before its one-byte RX buffer is live.
    pub fn needs_audio(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Listening)
    }

    /// Release the shared modem on exit or retune. A failed probe is retried
    /// only on re-entry. Partial frames belong to exactly one tuned session.
    pub fn service<B: Bus>(
        &mut self,
        modem: Option<&mut Modem<B>>,
        now: u32,
        open: bool,
        frequency: Option<u32>,
    ) -> Option<ReceivedPacket<'_>> {
        if !open && self.phase == Phase::Idle {
            return None;
        }
        let modem = modem?;
        let frequency = frequency.filter(|_| open);
        if !open || self.frequency != frequency {
            if matches!(self.phase, Phase::Starting | Phase::Listening) {
                if modem.off().is_err() {
                    self.phase = Phase::Failed;
                    self.frequency = None;
                    self.decoder.reset();
                    return None;
                }
            }
            if !open || self.phase != Phase::Failed {
                self.phase = Phase::Idle;
            }
            self.frequency = frequency;
            self.decoder.reset();
        }
        frequency?;
        // At 1200 bit/s a byte takes 6.67 ms. After a late poll E5 may
        // acknowledge a replacement byte after E6 was sampled; discard the
        // partial frame even if that status snapshot did not show overrun.
        if self.listening() && now.wrapping_sub(self.last_poll_ms) >= 7 {
            self.decoder.reset();
        }
        self.last_poll_ms = now;
        match self.phase {
            Phase::Idle => match modem.begin_rx(now, self.gain) {
                Ok(()) => self.phase = Phase::Starting,
                Err(_) => self.fail(modem),
            },
            Phase::Starting => match modem.poll(now) {
                Ok(Progress::Ready) => self.phase = Phase::Listening,
                Ok(Progress::Starting) => {}
                _ => self.fail(modem),
            },
            Phase::Listening => match modem.receive() {
                Ok(Some(byte)) => {
                    return self.decoder.push_byte(byte)?.ok().and_then(received_packet)
                }
                Ok(None) => {}
                Err(Error::Overflow) => self.decoder.reset(),
                Err(_) => self.fail(modem),
            },
            Phase::Failed => {}
        }
        None
    }

    fn fail<B: Bus>(&mut self, modem: &mut Modem<B>) {
        let _ = modem.off();
        self.phase = Phase::Failed;
        self.decoder.reset();
    }
}
