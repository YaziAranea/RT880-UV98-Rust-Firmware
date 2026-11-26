//! Compiled bitmap rows for the home-screen frequency. The source has seven
//! bytes per row. Its final byte is transparent for every glyph, so the LCD
//! only needs the first six. Glyph advances remain the renderer's choice.
use super::HOME_DIGIT_HEIGHT;

pub(super) const INK_BYTES: usize = 6;
const SOURCE_BYTES: usize = 7;
const GLYPHS: usize = 13;
const ROW_COUNT: usize = GLYPHS * HOME_DIGIT_HEIGHT as usize;
const UNIQUE_ROWS: usize = 235;
const HASH_SLOTS: usize = 256;
const SOURCE: &[u8; SOURCE_BYTES * ROW_COUNT] =
    include_bytes!("../fonts/chakra-frequency-2bpp.bin");
struct PackedRows {
    rows: [[u8; INK_BYTES]; UNIQUE_ROWS],
    indices: [u8; ROW_COUNT],
}

const fn pack(source: &[u8; SOURCE_BYTES * ROW_COUNT]) -> PackedRows {
    let mut packed = [[0; INK_BYTES]; UNIQUE_ROWS];
    let mut indices = [0; ROW_COUNT];
    let mut slots = [u8::MAX; HASH_SLOTS];
    let mut unique = 0;
    let mut row = 0;
    while row < ROW_COUNT {
        assert!(
            source[row * SOURCE_BYTES + INK_BYTES] == 0,
            "frequency font gained pixels in its right margin"
        );
        let mut hash = 0usize;
        let mut column = 0;
        while column < INK_BYTES {
            hash = (hash.wrapping_mul(31) + source[row * SOURCE_BYTES + column] as usize)
                & (HASH_SLOTS - 1);
            column += 1;
        }

        loop {
            let candidate = slots[hash];
            if candidate == u8::MAX {
                if unique >= UNIQUE_ROWS {
                    panic!("home digit font has more unique rows than expected");
                }
                column = 0;
                while column < INK_BYTES {
                    packed[unique][column] = source[row * SOURCE_BYTES + column];
                    column += 1;
                }
                slots[hash] = unique as u8;
                indices[row] = unique as u8;
                unique += 1;
                break;
            }

            let mut equal = true;
            column = 0;
            while column < INK_BYTES {
                if packed[candidate as usize][column] != source[row * SOURCE_BYTES + column] {
                    equal = false;
                    break;
                }
                column += 1;
            }
            if equal {
                indices[row] = candidate;
                break;
            }
            hash = (hash + 1) & (HASH_SLOTS - 1);
        }
        row += 1;
    }

    if unique != UNIQUE_ROWS {
        panic!("home digit font unique row count changed");
    }
    PackedRows {
        rows: packed,
        indices,
    }
}

const PACKED: PackedRows = pack(SOURCE);

pub(super) fn row(glyph: usize, y: usize) -> &'static [u8; INK_BYTES] {
    &PACKED.rows[PACKED.indices[glyph * HOME_DIGIT_HEIGHT as usize + y] as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_rows_reconstruct_every_original_pixel() {
        for (index, source) in SOURCE.chunks_exact(SOURCE_BYTES).enumerate() {
            // Home's narrow digit windows omit the two transparent left pixels.
            assert_eq!(source[0] & 0x0f, 0);
            let glyph = index / HOME_DIGIT_HEIGHT as usize;
            let y = index % HOME_DIGIT_HEIGHT as usize;
            assert_eq!(row(glyph, y), &source[..INK_BYTES]);
            assert_eq!(&source[INK_BYTES..], &[0]);
        }
    }
}
