# Building R53 and CPS 0.8.0 from source

These commands build files on the computer; they do not open a COM port or flash a radio. For installation without a compiler, use the [release package and CPS guide](docs/GETTING-STARTED.en.md).

The public checkout can build the ARM firmware, Windows CPS and the host-test subset below **without personal calibration or NOR files**. The complete historical verification pipeline still requires the owner's original private fixtures; it is a separate workflow.

For R53, build from the matching tag. The release reuses CPS 0.8.0 from R43.3. [CPS 0.8.0 source](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/tree/r41/apps/cps).

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

Build all three standard channel-TX images from a clean checkout. Fetch the locked dependencies once as shown above. Then run:

```powershell
python tools/build_release.py validation/r53-public
```

The command compiles RU, EN and DE offline, verifies the linked language and TX profile, checks startup and device calibration metadata, and writes a new directory when all three images pass. Each language has its own `image/` directory and `flash-budget.json`. The top-level `flash-budget.json` contains all three measurements and hashes. Use a new path for each build.

The minimum reserve is 4096 bytes before the settings journal. Each standard image is compared with the same language of the previous reference release. Positive cost means the change used Flash. The reserve uses the physical end of the last file-backed load in the ELF, including initializers for RAM. See the [module workflow and reference update](docs/FIRMWARE-MODULES.ru.md).

Each directory contains the ELF, BIN and `image-layout.json`. The exporter keeps the internal basenames `UV98-diag-0.1.8.elf` and `UV98-diag-0.1.8.bin`. The 251904-byte BIN covers the application window above the bootloader and excludes external NOR. Identify each image by its SHA256 from the layout report. The catalog check confirms the exact selected catalog in the BIN.

Use one locale feature per build. Rust selects Russian by default without an explicit locale feature. The host-only `locale-all` feature lets the simulator generate documentation in all three languages. The radio has no language selector.

`channel-tx` enables ordinary FM transmission with independent A/B PTT targets. Without a TX feature the default profile is RX-only. The separate historical `upper-exo-tx-trial` profile has its own feature selection. [PTT hardware evidence](research/r37/ptt.md)

Linked checks examine startup, exception code and the device profile in the actual ELF. RF behaviour, pin levels and maximum stack use require device measurements.

CPS 0.8.0 can open compatible application BINs with a calibration profile directly. Verify build provenance and board compatibility before preparing a personal image. The public firmware ZIP includes a matching catalog with SHA-256 checksums for the file and transfer stream.

## Host checks without private fixtures

```powershell
cargo test --locked -p uv98-smart-beacon
cargo test --locked --lib -p uv98-handheld-ui-state -p uv98-handheld-ui -p uv98-signaling -p uv98-gps-nmea -p uv98-spectrum -p uv98-keys -p uv98-display
```

This is a defined public subset, not a claim that all workspace tests are fixture-free. Do not run `cargo test --workspace` expecting the private golden tests to pass in a clean public checkout.

## Windows CPS 0.8.0 without private fixtures

For the tested Windows gnullvm host, first prepare the local OpenGL import library, then build the application:

```powershell
python tools/prepare_windows_link.py --sysroot $taskRustSysroot
cargo build --locked --release --target x86_64-pc-windows-gnullvm -p uv98-cps
python tools/pe_imports.py target/x86_64-pc-windows-gnullvm/release/uv98-cps.exe
```

The executable is `target/x86_64-pc-windows-gnullvm/release/uv98-cps.exe`. If the import report lists `libunwind.dll`, place the runtime from `lib/rustlib/x86_64-pc-windows-gnullvm/bin/` inside the Rust sysroot beside the executable. The import helper reads installed Windows export names and creates a local import library; it does not copy Windows DLL code.

`build-cps.ps1` documents the full packaging procedure and produces `target/cps-portable/UV98-CPS.exe`. That script also runs tests using the private calibration fixture; it is **not** the fixture-free build command above. Building the executable alone does not provide the firmware catalog, images or recovery files included in a separately assembled release kit. A successful build also does not replace a native Windows UI check.

## Full owner verification

`build-diagnostic-image.ps1` requires both the exact calibration fixture and preserved NOR listed in [PRIVATE-INPUTS.md](PRIVATE-INPUTS.md). It checks their fixed SHA values before and after running oracle checks, host tests, ARM checks and screen simulation. Another radio's backup is not a replacement for these golden inputs.

For a maintainer who already holds those originals:

```powershell
./build-diagnostic-image.ps1 -ChannelTx -Language ru -NorBackup 'D:/private-radio-backup/storage-address-space.bin' -OutputDirectory 'D:/radio-builds/uv98-r53-ru'
```

Use real local paths and a new output directory for each language. Select `-Language en` or `-Language de` for the other variants. This workflow additionally needs `pyserial` for boot-log test tools, but does not access the radio. Do not invent, zero-fill or substitute a fixture to make the tests pass. This workflow records host and linked-image verification. R53 hardware checks remain separate.

## Public packaging

Use `--exclude-private-fixture` with `tools/package_diagnostic_image.py` when preparing a shareable source archive. The legacy packager otherwise includes the owner's fixture in `source.zip`. That switch alone is not a public-release privacy or licensing audit: inspect archive entries, logs and reports as well.

Keep a compatible CPS catalog with each flashable BIN. The public kit excludes manufacturer recovery firmware and personal calibration, NOR and codeplug files. The [getting-started guide](docs/GETTING-STARTED.en.md#where-to-get-the-stock-recovery-file) links the separate stock recovery file and its hash.

### Optional runtime UART diagnostics

The firmware feature `verbose-runtime` enables key snapshots (0x132), extra
RX/register snapshots (0x174/0x175), and timing/profiling frames (0x172/0x176).
R53 builds in this guide leave it off. LCD meters,
normal RX observations, RF events, PTT/TOT guards and fault records remain active.
This flag is for diagnostic compositions; combining every feature with
`channel-tx` is not qualified to fit. Keep the linker memory bounds unchanged.
