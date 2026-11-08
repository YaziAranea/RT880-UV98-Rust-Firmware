"""Compile pure crates in isolation; extract layout sizes without executing code."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent
HERE = PROJECT / "target/ram-analysis"
HERE.mkdir(parents=True, exist_ok=True)
RUSTC = Path.home() / ".cargo/bin/rustc.exe"
CRATES = {
    "codeplug": [], "keys": [], "display": [], "spectrum": [],
    "ui-core": ["codeplug", "keys"],
    "radio-ui": ["ui-core", "spectrum", "display"],
}
TYPES = {
    "ENGINE": "uv98_spectrum::Engine",
    "BIN": "uv98_spectrum::Bin",
    "BINS_240": "[uv98_spectrum::Bin; uv98_spectrum::MAX_BINS]",
    "MEASUREMENT": "uv98_spectrum::Measurement",
    "RANGE": "uv98_spectrum::Range",
    "TIMING": "uv98_spectrum::Timing",
    "CONTROLLER": "uv98_ui_core::Controller",
    "VIEW": "uv98_ui_core::View",
    "NUMBER_ENTRY": "uv98_ui_core::NumberEntry",
    "BORROWED_CHANNEL_CATALOG": "uv98_ui_core::CodeplugChannels<'static>",
    "MODEL": "uv98_radio_ui::Model<'static>",
    "CHANNEL_VIEW": "uv98_radio_ui::ChannelView<'static>",
    "SPECTRUM_VIEW": "uv98_radio_ui::SpectrumView<'static>",
    "ROW_320_RGB565": "[u16; uv98_radio_ui::WIDTH]",
}

def run(*args):
    return subprocess.run([str(RUSTC), *map(str, args)], check=True,
                          capture_output=True, text=True).stdout

def crate_name(name):
    return "uv98_" + name.replace("-", "_")

def main():
    source = "#![no_std]\nuse core::mem::size_of;\n"
    for symbol, ty in TYPES.items():
        source += (f"#[no_mangle] pub static {symbol}: [u8; size_of::<{ty}>()] = "
                   f"[0; size_of::<{ty}>()];\n")
    source += """
const _: () = assert!(size_of::<uv98_spectrum::Engine>() <= 8192);
const _: () = assert!(size_of::<uv98_ui_core::Controller>() <= 64);
const _: () = assert!(size_of::<uv98_radio_ui::Model<'static>>() <= 128);
const _: () = assert!(size_of::<[u16; uv98_radio_ui::WIDTH]>() == 640);
"""
    probe = HERE / "layout_probe.rs"
    probe.write_text(source, encoding="utf-8")
    report = {
        "scope": "pure type layout only; no executable or radio access",
        "rustc": run("-vV").splitlines(),
        "limitations": [
            "Rust default layout is compiler and target dependent.",
            "Borrowed data backing storage is excluded from view sizes.",
            "These sizes do not establish total firmware RAM or peak stack.",
            "Renderer row is 640 bytes; display staging and formatting add stack.",
        ],
        "source_sha256": {}, "targets": {},
    }
    for name in CRATES:
        for path in sorted((PROJECT / "crates" / name / "src").rglob("*.rs")):
            report["source_sha256"][path.relative_to(PROJECT).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    for target in ["x86_64-pc-windows-gnullvm", "thumbv7em-none-eabi"]:
        output = HERE / target
        output.mkdir(exist_ok=True)
        libs = {}
        for name, dependencies in CRATES.items():
            library = output / ("lib" + crate_name(name) + ".rlib")
            args = ["--crate-name", crate_name(name), "--edition=2021",
                    "--crate-type=rlib", "--target", target, "-C", "opt-level=3",
                    "-C", "panic=abort", "-L", f"dependency={output}"]
            for dependency in dependencies:
                args += ["--extern", f"{crate_name(dependency)}={libs[dependency]}"]
            args += [PROJECT / "crates" / name / "src/lib.rs", "-o", library]
            run(*args)
            libs[name] = library
        ir = output / "layout_probe.ll"
        args = ["--crate-name", "layout_probe", "--edition=2021", "--crate-type=rlib",
                "--target", target, "-C", "panic=abort", "--emit=llvm-ir",
                "-L", f"dependency={output}"]
        for name in ["spectrum", "ui-core", "radio-ui"]:
            args += ["--extern", f"{crate_name(name)}={libs[name]}"]
        run(*args, probe, "-o", ir)
        text = ir.read_text(encoding="utf-8")
        sizes = {}
        for symbol in TYPES:
            match = re.search(r"@" + symbol + r" = .*?constant \[(\d+) x i8\]", text)
            if not match:
                raise RuntimeError(f"Missing layout symbol {target}/{symbol}")
            sizes[symbol] = int(match.group(1))
        report["targets"][target] = {"compile_time_bounds_passed": True, "bytes": sizes}
    (PROJECT / "validation/ram-layout.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["targets"], indent=2))

if __name__ == "__main__":
    main()

