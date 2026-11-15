# Private inputs and original backups

Personal radio data belong to their owner. They are excluded from this repository and the public R36 kit. The [public build commands](BUILDING.md) do not require them; some historical golden tests and simulations do.

## Keep private

- Full 4 MiB NOR snapshots containing channels, settings and personal data.
- Original `.880dmf` / `.98pdmf` codeplugs, including the owner's `999.880dmf`.
- Individual 4096-byte factory calibration blocks.
- Raw USB logs and captures containing personal information or location.
- Local `source.zip` archives that include private fixtures.

Keep complete verified snapshot folders, including their manifests and two passes, in two protected locations. Do not attach them to a public GitHub issue. A useful public issue can contain the release/BIN hash, exact reproduction steps and a suitably checked screenshot instead.

## Private golden-test inputs

Some tests use `include_bytes!` with this intentionally ignored file:

```text
crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin
```

The full `build-diagnostic-image.ps1` workflow additionally requires the owner's original 4194304-byte NOR through `-NorBackup`. That script checks fixed SHA256 values for both original inputs before and after the run. The public checkout does not provide those files or a universal replacement.

**Do not replace a missing golden fixture with zeros, synthetic data or another radio's calibration.** Such changes would remove the meaning of the existing expected results. Use the public build and test subset, or add independently documented synthetic test cases as a separate development change.

## R36 per-radio preparation

R36 reads the real calibration from the radio and compares it with an expected SHA256. The raw calibration block is not embedded in the firmware.

CPS 0.6 can prepare a personal image from a recognized R36 template and the `manifest.json` of the owner's full, twice-read 4 MiB NOR snapshot. It validates the snapshot and changes only the expected calibration hash. It creates a new BIN and catalog, leaving the original template and backups unchanged. Raw calibration, channels and NOR are not copied into the new image folder. The catalog records hashes and provenance, not a replacement calibration block.

This process binds an image to an individual radio; it does **not** establish that another board revision has compatible hardware. Use it only for a confirmed compatible board. See the [complete preparation procedure](docs/GETTING-STARTED.en.md#4-check-compatibility-before-flashing).

Firmware settings, including language, remain in RAM. Writing calibration or silently replacing stock NOR regions is not part of R36. Original backups, codeplugs and immutable release files must be preserved.

## What the backup does and does not contain

CPS reads calibration and external NOR using the stock protocol with the radio running stock firmware normally. The known protocol does not read the MCU application or bootloader. A saved stock firmware BIN is a separately obtained recovery file, not a readback of the owner's MCU.

Our CPS currently does not restore a raw 4 MiB NOR or calibration block to the radio. Flashing a stock MCU application does not reconstruct damaged external data. Keep the originals even after a successful firmware recovery.

GPS/APRS file editing modifies a working codeplug copy, with byte differences and undo. Transferring that edited codeplug currently uses the stock CPS while on stock firmware; the Rust radio does not yet implement a general USB settings-write protocol.

## Recovery firmware and publication

The manufacturer's stock recovery BIN is excluded from the public R36 kit because redistribution permission has not been established. Its known filename, source and SHA256 are listed in the [recovery-file instructions](docs/GETTING-STARTED.en.md#where-to-get-the-stock-recovery-file). Obtaining that file does not replace an individual calibration or NOR backup.

The older R35 local kit and draft remain private historical artifacts; they include files that must not be assumed suitable for public redistribution. Preserve those originals separately from a new public package.

The legacy `tools/package_diagnostic_image.py` includes the private calibration fixture in `source.zip` unless `--exclude-private-fixture` is specified. Always use the exclusion for public source archives, then inspect the complete package for personal data and third-party licensing. `.gitignore` alone does not remove files already placed in an archive or release asset.
