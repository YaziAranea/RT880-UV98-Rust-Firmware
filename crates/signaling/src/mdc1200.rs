//! Single-codeword MDC1200 PTT ID for the BK4819 FFSK modem.
//!
//! Wire format and known-answer reference:
//! https://github.com/OneOfEleven/uv-k5-firmware-custom/blob/main/mdc1200.c
//! (Apache-2.0 reference, independent implementation).
//! Individual ID range: Motorola EP350 MX User Guide, 68012000071-C, p. 13.

const CODEWORD_BYTES: usize = 14;
const CODEWORD_BITS: usize = CODEWORD_BYTES * 8;
const INFO_BYTES: usize = 7;
const PTT_ID_OPCODE: u8 = 0x01;
const SYNC: [u8; 5] = [0x07, 0x09, 0x2a, 0x44, 0x6f];
const PREFIX_BYTES: usize = 3 + SYNC.len();
const FIFO_BYTES: usize = PREFIX_BYTES + CODEWORD_BYTES;

/// The modem can match the last four bytes of the five-byte MDC sync.
/// REG0B reports whether it matched this pattern or its complement.
pub const RX_SYNC_WORDS: [u16; 2] = [0x7240, 0x99a7];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitId(u16);

impl UnitId {
    pub const fn new(value: u16) -> Option<Self> {
        match value {
            0x0001..=0xdeee => Some(Self(value)),
            _ => None,
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Begin,
    End,
}

impl Edge {
    const fn argument(self) -> u8 {
        match self {
            Self::Begin => 0x80,
            Self::End => 0x00,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PttId {
    pub unit: UnitId,
    pub edge: Edge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packet([u8; FIFO_BYTES]);

impl Packet {
    /// The FIFO gets an MDC suffix of three preamble bytes, five sync bytes,
    /// and one codeword. The hardware adds a separate preamble and sync.
    pub fn encode(id: PttId) -> Self {
        let [id_high, id_low] = id.unit.get().to_be_bytes();
        let header = [PTT_ID_OPCODE, id.edge.argument(), id_high, id_low];
        let [crc_low, crc_high] = crc(&header).to_le_bytes();
        let info = [
            header[0], header[1], header[2], header[3], crc_low, crc_high, 0,
        ];
        let mut codeword = [0; CODEWORD_BYTES];
        codeword[..INFO_BYTES].copy_from_slice(&info);
        let mut history = 0u8;
        for (byte, parity) in info.into_iter().zip(&mut codeword[INFO_BYTES..]) {
            for bit in 0..8 {
                history = (history << 1) | (byte >> bit & 1);
                *parity |= ((history & 0x65).count_ones() as u8 & 1) << bit;
            }
        }

        let mut frame = [0; FIFO_BYTES];
        frame[3..PREFIX_BYTES].copy_from_slice(&SYNC);
        for output in 0..CODEWORD_BITS {
            let input = deinterleaved_bit(output);
            let bit = codeword[input / 8] >> (input % 8) & 1;
            frame[PREFIX_BYTES + output / 8] |= bit << (7 - output % 8);
        }
        let mut previous = 0;
        for byte in &mut frame {
            let shifted = (*byte >> 1) | previous;
            previous = (*byte & 1) << 7;
            *byte = !(*byte ^ shifted);
        }
        Self(frame)
    }

    pub const fn fifo_bytes(&self) -> &[u8; FIFO_BYTES] {
        &self.0
    }

    /// REG5F consumes each word's low byte before its high byte.
    pub fn fifo_words(&self) -> impl Iterator<Item = u16> + '_ {
        self.0
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
    }

    /// REG5D encodes the FIFO payload length minus one.
    pub const fn length_word() -> u16 {
        ((FIFO_BYTES as u16) - 1) << 8
    }
}

/// Decode the data bytes after a confirmed hardware sync. Set `inverted_sync`
/// when the modem matched the complement of `RX_SYNC_WORDS`.
///
/// CRC detects damage to the information bits. This decoder accepts intact
/// information when the parity bits were damaged and has no error correction.
pub fn decode_payload(bytes: &[u8; CODEWORD_BYTES], inverted_sync: bool) -> Option<PttId> {
    let mut info = [0; INFO_BYTES];
    // The last bit of the original 40-bit sync is one.
    let mut previous = SYNC[SYNC.len() - 1] & 1;
    for output in 0..CODEWORD_BITS {
        let bit = bytes[output / 8] >> (7 - output % 8) & 1;
        previous ^= bit ^ u8::from(!inverted_sync);
        let input = deinterleaved_bit(output);
        if input < INFO_BYTES * 8 {
            info[input / 8] |= previous << (input % 8);
        }
    }
    let [opcode, argument, id_high, id_low, crc_low, crc_high, _status] = info;
    if opcode != PTT_ID_OPCODE
        || crc(&[opcode, argument, id_high, id_low]) != u16::from_le_bytes([crc_low, crc_high])
    {
        return None;
    }
    let edge = match argument {
        0x80 => Edge::Begin,
        0x00 => Edge::End,
        _ => return None,
    };
    let unit = UnitId::new(u16::from_be_bytes([id_high, id_low]))?;
    Some(PttId { unit, edge })
}

const fn deinterleaved_bit(interleaved: usize) -> usize {
    (interleaved % 16) * 7 + interleaved / 16
}

fn crc(header: &[u8; 4]) -> u16 {
    let mut register = 0u16;
    for byte in header {
        register ^= u16::from(*byte);
        for _ in 0..8 {
            register = (register >> 1) ^ (0x8408 * (register & 1));
        }
    }
    !register
}

#[cfg(test)]
mod tests {
    use super::*;

    // Independently derived from the reference's systematic/parity example
    // for ID 1234 (BOT) and checked with the reference interleave and XOR.
    const BEGIN: [u8; FIFO_BYTES] = [
        0xff, 0xff, 0xff, 0xfb, 0x72, 0x40, 0x99, 0xa7, 0xb6, 0x21, 0xc0, 0xc6, 0xfa, 0x2a, 0x4c,
        0xf9, 0xff, 0xed, 0xeb, 0x0c, 0xc5, 0x33,
    ];
    const END: [u8; FIFO_BYTES] = [
        0xff, 0xff, 0xff, 0xfb, 0x72, 0x40, 0x99, 0xa7, 0xb3, 0x3e, 0x73, 0xf3, 0xfc, 0x29, 0x4a,
        0xcf, 0xf9, 0xeb, 0xeb, 0x09, 0xc9, 0x0f,
    ];

    fn id(edge: Edge) -> PttId {
        PttId {
            unit: UnitId::new(0x1234).unwrap(),
            edge,
        }
    }

    #[test]
    fn expected_begin_end_wire_bytes_and_fifo_order() {
        assert_eq!(Packet::encode(id(Edge::Begin)).fifo_bytes(), &BEGIN);
        assert_eq!(Packet::encode(id(Edge::End)).fifo_bytes(), &END);
        assert_eq!(Packet::length_word(), 0x1500);
        assert!(Packet::encode(id(Edge::Begin)).fifo_words().eq([
            0xffff, 0xfbff, 0x4072, 0xa799, 0x21b6, 0xc6c0, 0x2afa, 0xf94c, 0xedff, 0x0ceb, 0x33c5,
        ]));
    }

    #[test]
    fn decode_known_sync_polarities_and_reject_corrupt_information() {
        for (frame, edge) in [(BEGIN, Edge::Begin), (END, Edge::End)] {
            let payload: [u8; CODEWORD_BYTES] = frame[PREFIX_BYTES..].try_into().unwrap();
            assert_eq!(decode_payload(&payload, false), Some(id(edge)));
            assert_eq!(
                decode_payload(&payload.map(|byte| !byte), true),
                Some(id(edge))
            );
            let mut damaged = payload;
            damaged[0] ^= 0x80;
            assert_eq!(decode_payload(&damaged, false), None);
        }
    }

    #[test]
    fn individual_unit_id_limits() {
        assert_eq!(UnitId::new(0), None);
        assert!(UnitId::new(1).is_some());
        assert!(UnitId::new(0xdeee).is_some());
        assert_eq!(UnitId::new(0xdeef), None);
        assert_eq!(UnitId::new(u16::MAX), None);
    }
}
