# Building R37 and CPS 0.7.0 from source

These commands build files on the computer; they do not open a COM port or flash a radio. For installation without a compiler, use the [release package and CPS guide](docs/GETTING-STARTED.en.md).

The public checkout can build the ARM firmware, Windows CPS and the host-test subset below **without personal calibration or NOR files**. The complete historical verification pipeline still requires the owner's original private fixtures; it is a separate workflow.

The [`r37` tag](https://github.com/YaziAranea/uv98-rust/tree/r37) identifies the original firmware release. The [`cps-v0.7.0` tag](https://github.com/YaziAranea/uv98-rust/tree/cps-v0.7.0) identifies the CPS 0.7.0 source used for the updated kit. The kit carries the same `UV98-r37.bin`, so updating CPS requires no reflash.

## Toolchain

The development environment uses Rust 1.99.1, Windows host `x86_64-pc-windows-gnullvm` and ARM target `thumbv7em-none-eabi`. There is no `rust-toolchain.toml`: `Cargo.lock` fixes dependencies, not the compiler. Other host/toolchain combinations require their own verification.

Use Rust/Cargo, Python 3.11+ and PowerShell 7 for the Windows commands below. Run them from the repository root:

```powershell
rustup target add thumbv7em-none-eabi
cargo fetch --locked
python -m pip install capstone unicorn pyelftools
```

On the Windows gnullvm host, select the installed linker for host programs and build scripts:

```powershell
$taskRustSysroot = rustc --print sysroot
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $taskRustSysroot 'lib/rustlib/x86_64-pc-windows-gnullvm/bin/rust-lld.exe'
```

## Public ARM build

```powershell
cargo build --locked --release --target thumbv7em-none-eabi -p uv98-diag-firmware --features channel-tx
python tools/export_diagnostic_image.py target/thumbv7em-none-eabi/release/uv98-diag-firmware validation/r37-public-build/image
python tools/check_diag_preinit.py validation/r37-public-build/image/UV98-diag-0.1.8.elf --report validation/r37-public-build/early-machine-code-check.json
python tools/check_device_profile.py validation/r37-public-build/image/UV98-diag-0.1.8.elf --report validation/r37-public-build/device-profile-check.json
```

The export directory must be new. Use another directory name for a subsequent build. The exporter retains its historical filenames:

```text
validation/r37-public-build/image/UV98-diag-0.1.8.bin
validation/r37-public-build/image/UV98-diag-0.1.8.elf
validation/r37-public-build/image/image-layout.json
```

The BIN covers the 251904-byte application window. It excludes the bootloader prefix and external NOR. A release name such as `UV98-r37.bin` is assigned during packaging; identify the exact artifact by its SHA256, not its filename alone.

`channel-tx` enables the R37 release profile, including the independent A/B PTT targets. With neither TX feature, the default profile is RX-only. Build the historical `upper-exo-tx-trial` with its own feature selection. [PTT hardware evidence](research/r37/ptt.md)

The linked checks examine startup/exception code and the device profile in the actual ELF. They do not measure RF behaviour, electrical pin states or maximum stack use on physical hardware.

**A self-built BIN is not automatically accepted by the released CPS.** CPS validates recognized images against its release catalog. The exporter creates a layout report, not a trusted CPS catalog. Development builds need a separately reviewed catalog and packaging pipeline; do not rename an unknown BIN or disable validation to make it look like a release. The ordinary user path prepares a personal image from the recognized R37 template in CPS.

## Host checks without private fixtures

```powershell
cargo test --locked --lib -p uv98-handheld-ui -p uv98-signaling -p uv98-gps-nmea -p uv98-spectrum -p uv98-keys -p uv98-display
```

This is a defined public subset, not a claim that all workspace tests are fixture-free. Do not run `cargo test --workspace` expecting the private golden tests to pass in a clean public checkout.

## Windows CPS 0.7.0 without private fixtures

For the tested Windows gnullvm host, first prepare the local OpenGL import library, then build the application:

```powershell
python tools/prepare_windows_link.py --sysroot $taskRustSysroot
cargo build --locked --release --target x86_64-pc-windows-gnullvm -p uv98-cps
python tools/pe_imports.py target/x86_64-pc-windows-gnullvm/release/uv98-cps.exe
```

The executable is `target/x86_64-pc-windows-gnullvm/release/uv98-cps.exe`. If the import report lists `libunwind.dll`, place the runtime from `lib/rustlib/x86_64-pc-windows-gnullvm/bin/` inside the Rust sysroot beside the executable. The import helper reads installed Windows export names and creates a local import library; it does not copy Windows DLL code.

`build-cps.ps1` documents the full packaging procedure and produces `target/cps-portable/UV98-CPS.exe`. That script also runs tests using the private calibration fixture; it is **not** the fixture-free build command above. Building the executable alone does not provide the firmware catalog, images or recovery files included in a separately assembled release kit. A successful build also does not replace a native Windows UI check.

## Full owner verification

The historical `build-diagnostic-image.ps1` requires both the exact calibration fixture and preserved NOR listed in [PRIVATE-INPUTS.md](PRIVATE-INPUTS.md). It checks their fixed SHA values before and after running oracle checks, host tests, ARM checks and screen simulation. Another radio's backup is not a replacement for these golden inputs.

For a maintainer who already holds those originals:

```powershell
./build-diagnostic-image.ps1 -ChannelTx -NorBackup 'D:/private-radio-backup/storage-address-space.bin' -OutputDirectory 'D:/radio-builds/uv98-next-build'
```

Use real local paths and a new output directory. This workflow additionally needs `pyserial` for boot-log test tools, but does not access the radio. Do not invent, zero-fill or substitute a fixture to make the tests pass. Historical R35 results remain in [R35-REPORT-RU.txt](R35-REPORT-RU.txt); they are not R37 hardware acceptance evidence.

## Public packaging

Use `--exclude-private-fixture` with `tools/package_diagnostic_image.py` when preparing a shareable source archive. The legacy packager otherwise includes the owner's fixture in `source.zip`. That switch alone is not a public-release privacy or licensing audit: inspect archive entries, logs and reports as well.

The [public R37 kit with CPS 0.7.0](https://github.com/YaziAranea/uv98-rust/releases/download/r37/UV98-r37-CPS-0.7.0-2026-11-16.zip) excludes manufacturer recovery firmware and personal calibration/NOR/codeplug files. The [getting-started guide](docs/GETTING-STARTED.en.md#where-to-get-the-stock-recovery-file) provides the separately obtained matching stock file and its hash. The older R35 kit remains a private historical artifact; its bundled recovery file must not be copied automatically into a public release.
