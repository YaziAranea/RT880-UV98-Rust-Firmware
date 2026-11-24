//! CPS patches this fixed-size flash slot before personalization. Take its
//! address from a volatile load so LTO cannot substitute the built-in clip.
pub use uv98_voice_clip::Samples;
use uv98_voice_clip::{VoiceConfig, CONFIG_BYTES, CONFIG_MAGIC, MAGIC, SLOT_BYTES};

#[no_mangle]
#[used]
static UV98_VOICE_CLIP: [u8; SLOT_BYTES] = *include_bytes!("../assets/test-voice.clip");
static SLOT_ADDRESS: &[u8; SLOT_BYTES] = &UV98_VOICE_CLIP;

#[no_mangle]
#[used]
static UV98_VOICE_CONFIG: [u8; CONFIG_BYTES] = VoiceConfig::UNASSIGNED.encode().unwrap();
static CONFIG_ADDRESS: &[u8; CONFIG_BYTES] = &UV98_VOICE_CONFIG;

#[inline(never)]
pub fn config() -> Option<VoiceConfig> {
    // Read the linked address exactly as for the CPS-patched clip below.
    let record = unsafe { core::ptr::read_volatile(&CONFIG_ADDRESS) };
    // CPS locates the record by its unique magic. Comparing against a second
    // verbatim string in .rodata would create a false record candidate.
    const REVERSED: [u8; CONFIG_MAGIC.len()] = {
        let mut bytes = *CONFIG_MAGIC;
        bytes.reverse();
        bytes
    };
    for (index, expected) in REVERSED.iter().enumerate() {
        if record[REVERSED.len() - 1 - index] != *expected {
            return None;
        }
    }
    VoiceConfig::from_body(&record[CONFIG_MAGIC.len()..])
}

#[inline(never)]
pub fn samples() -> Option<Samples<'static>> {
    // SAFETY: this aligned pointer in read-only firmware always designates
    // the linked slot. The desktop editor changes slot data only.
    let slot = unsafe { core::ptr::read_volatile(&SLOT_ADDRESS) };
    Samples::from_body(&slot[MAGIC.len()..])
}
