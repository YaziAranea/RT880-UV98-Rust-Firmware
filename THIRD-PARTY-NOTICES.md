# Third-party material

Original Rust implementation and project documentation are MIT licensed.
This does not relicense reference firmware, fonts or dependency code.

## Research reference: nicsure

`research/flash-protocol/webserial-flash880-pinned.html` is a preserved reference
from [nicsure/RT880-WebSerial-Firmware-Flasher](https://github.com/nicsure/RT880-WebSerial-Firmware-Flasher).
It retains the upstream GNU GPL v2 license, reproduced in
[`research/flash-protocol/LICENSE-GPL-2.0.txt`](research/flash-protocol/LICENSE-GPL-2.0.txt).
The file is an offline protocol reference and is not linked into the radio firmware
or CPS executable. Its pinned hash is used by the protocol verification scripts.

## Fonts

Chakra Petch, Rajdhani and Barlow Condensed font assets retain the SIL Open Font License 1.1.
Their copyright notices and complete licenses accompany the assets in
`crates/handheld-ui/fonts/`. The generated firmware glyph data are derived
from these fonts and remain covered by that license.

## External references

Ideas and hardware research are attributed in the research notes to fagci,
nicsure and other linked projects. The unlicensed upstream fagci build-file
copies and the externally supplied radio icon reference sheet are not
redistributed. See `reference/UPSTREAM.md` for build-file provenance.
Manufacturer schematic page renders and crops are also excluded because
redistribution permission has not been established. Their source and preserved
file hashes are documented in
[`research/rf-startup/UPSTREAM-DIAGRAMS.md`](research/rf-startup/UPSTREAM-DIAGRAMS.md).
Historical research notes still name those private images; the project MIT
license does not relicense manufacturer drawings or firmware excerpts.
Manufacturer firmware is not included in the public release kit; the recovery
guide links to a separately hosted copy with an exact SHA256.

## Rust dependencies

`Cargo.lock` fixes dependency versions. Each dependency retains the license
declared in its package and source. Public kits include `licenses/dependencies.json`
and copies of available dependency license and copyright files. These are
separate from the project's MIT license.
