# Fixed UI languages

R40 firmware builds use one language for the handheld UI. Build with
`locale-ru`, `locale-en`, or `locale-de`. Russian is the default without a
locale feature. The build rejects conflicting features. For example:

```powershell
cargo build --locked --offline --release --target thumbv7em-none-eabi -p uv98-diag-firmware --features channel-tx,locale-de
```

`build-diagnostic-image.ps1 -Language ru|en|de` selects the language and checks
the packed catalog in the exported BIN. Each image includes only its catalog.
The System/TX menu has no language selector. The MDC local-label editor still
switches between its Russian and English alphabets with **#**. The fonts
support Latin, Cyrillic and German umlauts for channel names and other data.

The host image simulator uses `locale-all` for previews. This feature is
host-only and gives the simulator a programmatic language override. Its menus
match the fixed-language firmware.

## Editing translations

`messages.tsv` is UTF-8 with three tab-separated columns: Russian, English,
German. A message ID is its zero-based data row (excluding the header). **Append
new entries; keep existing row order.** Retire an entry by removing all three
translations and leaving a blank row in its place. IDs embedded in Rust string literals
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
The three `messages-{ru,en,de}.bin` files are checked in, so Cargo builds need
no Python. Each catalog contains a BMP character alphabet, a byte-pair
dictionary, offsets and packed message bytes. Decoding uses a 24-byte work stack and a 256-byte UTF-8 message
buffer; no heap, storage writes or language-specific RF behavior. Unmarked
numeric/technical labels bypass decompression.

Tests compare all packed strings with the reviewed TSV, check glyphs and
menu widths, and compare full and incremental rendering in each language.
The image simulator also produces gallery frames with synthetic channel and
RF data.
