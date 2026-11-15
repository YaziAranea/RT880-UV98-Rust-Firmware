# Runtime UI languages

R36 provides Russian, English, and German for the complete handheld UI: home,
menu/settings, channel details, spectrum, HF, GPS/APRS, signaling, notices and
help. The compact frequency font is unchanged. The small label font includes
actual German umlauts, in addition to Latin and Cyrillic letters.

Select **MENU → SYSTEM / TX → LANGUAGE**, choose **РУССКИЙ / ENGLISH / DEUTSCH** with
the arrow keys and confirm with **MENU**. **EXIT** discards the pending choice.
The choice is in RAM: the default after power-on remains Russian. No NOR,
calibration, channel or frequency data is written when changing language.

The language travels in `controller::View` and in the renderer's private row
context. There is no mutable global language or duplicated per-language
renderer. Changing language invalidates all 320 display rows. Live measurements,
channel/zone names, frequencies and protocol identifiers remain data; no
dictionary lookup guesses whether an arbitrary saved name is translatable.

## Editing translations

`messages.tsv` is UTF-8 with three tab-separated columns: Russian, English,
German. A message ID is its zero-based data row (excluding the header). **Append
new entries; keep existing row order.** IDs embedded in Rust string literals
use `\x1fNNN`, where `NNN` is exactly three hexadecimal digits. The renderer
expands IDs after numeric formatting, so templates can combine localized text
and runtime values without allocations. Use the same ID wherever the meaning
is the same. A saved name cannot introduce an ID: its control bytes are filtered
by the existing codeplug-name decoder.

Regenerate and verify from the repository root:

```powershell
python tools/pack_ui_locales.py
python tools/pack_ui_locales.py --check
```

The generator verifies row structure, ID bounds, remaining untranslated Russian
literals in production screen code, and the maximum bounded decoder depth.
`messages.bin` is checked in; Python is not required to build the firmware. Its
format is a BMP character alphabet, a byte-pair dictionary, offsets and packed
message bytes. Decoding uses a 24-byte work stack and a 256-byte UTF-8 message
buffer; no heap, storage writes or language-specific RF behavior. Unmarked
numeric/technical labels bypass decompression.

Tests compare all packed strings with the reviewed TSV, check every glyph,
check translated menu widths, verify confirm/cancel semantics and preservation
of both VFO contexts, and compare full/incremental rendering after language
changes. The image simulator saves twelve public gallery files:
`{ru,en,de}-{home,menu,spectrum,hf}.png`. These are renderer output with synthetic
channel data and simulated RF values, not photos or measurements from a radio.
