"""Package a completed offline diagnostic build, preserving old outputs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import tomllib
import zipfile

import check_elf
from export_diagnostic_image import VERSION, export

ROOT = Path(__file__).resolve().parent.parent
RELEASE_SYMBOL = b"UV98_RELEASE_POLICY"
RX_CANDIDATE_POLICY = (0x108, 0, 0xFFFFFFFF, 0x52584331)
UPPER_EXO_TRIAL_POLICY = (0x108, 1, 0xFFFFFFFF, 0x45584F31)
CHANNEL_TX_POLICY = (0x108, 1, 0xFFFFFFFF, 0x464D5431)
TRIAL_FEATURE = "upper-exo-tx-trial"
CHANNEL_TX_FEATURE = "channel-tx"
PROFILES = {
    "rx": (RX_CANDIDATE_POLICY, [], "RX candidate"),
    "upper-exo-trial": (UPPER_EXO_TRIAL_POLICY, [TRIAL_FEATURE], "upper EXO trial"),
    "channel-tx": (CHANNEL_TX_POLICY, [CHANNEL_TX_FEATURE], "FM channel/VFO TX"),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def release_policy(data, layout, expected=RX_CANDIDATE_POLICY):
    """Read the actual allocated ELF object, not a coincidental byte pattern.

    This verifies the embedded release declaration. It is not a control-flow
    proof of every RF instruction or evidence of a physical receive/TX test.
    """
    header = check_elf.ELF_HEADER.unpack_from(data)
    sections = [check_elf.SECTION_HEADER.unpack_from(data, header[6] + i * header[11])
                for i in range(header[12])]
    found = []
    for section in sections:
        if section[1] != 2:  # SHT_SYMTAB
            continue
        if section[9] != 16 or section[5] % 16 or not 0 < section[6] < len(sections):
            raise ValueError("Invalid release policy symbol table")
        strings = sections[section[6]]
        if strings[1] != check_elf.SHT_STRTAB:
            raise ValueError("Invalid release policy string table")
        names = data[strings[4]:strings[4] + strings[5]]
        for offset in range(section[4], section[4] + section[5], 16):
            name, address, size, info, _, index = struct.unpack_from("<IIIBBH", data, offset)
            end = names.find(b"\0", name)
            if not 0 <= name <= end < len(names):
                raise ValueError("Invalid release policy symbol name")
            if names[name:end] != RELEASE_SYMBOL:
                continue
            if not 0 < index < len(sections) or size != 16 or info & 15 != 1 or address % 4:
                raise ValueError("Release policy must be one sized aligned ELF object")
            owner = sections[index]
            if (owner[1] != check_elf.SHT_PROGBITS or not owner[2] & check_elf.SHF_ALLOC
                    or not check_elf.contained(address, size, owner[3], owner[3] + owner[5])):
                raise ValueError("Release policy is not in an allocated file-backed section")
            mappings = [segment for segment in layout["load_segments"]
                        if check_elf.contained(address, size,
                            int(segment["virtual_address"], 16),
                            int(segment["virtual_address"], 16) + segment["file_bytes"])]
            if len(mappings) != 1:
                raise ValueError("Release policy does not have one file-backed load mapping")
            segment = mappings[0]
            source = segment["file_offset"] + address - int(segment["virtual_address"], 16)
            if source != owner[4] + address - owner[3]:
                raise ValueError("Release policy section/load mapping differs")
            raw = data[source:source + size]
            words = struct.unpack("<4I", raw)
            if words != expected:
                label = next((item[2] for item in PROFILES.values() if item[0] == expected),
                             "release profile")
                raise ValueError(f"Expected {label}: linked release policy differs from selected profile")
            found.append({"symbol": RELEASE_SYMBOL.decode(), "address": f"0x{address:08X}",
                          "words": [f"0x{word:08X}" for word in words],
                          "sha256": hashlib.sha256(raw).hexdigest()})
    if len(found) != 1:
        raise ValueError("Expected exactly one linked UV98_RELEASE_POLICY object")
    return found[0]


def validate_source(text, manifest, profile):
    """Check explicit source declarations, not Rust control-flow reachability."""
    # Do not satisfy a source assertion using an ordinary commented-out guard.
    # Preserve quoted strings while removing ordinary Rust comments.
    code = re.sub(r'("(?:\\.|[^"\\])*")|//[^\r\n]*|/\*.*?\*/',
                  lambda match: match[1] or " ", text, flags=re.S)
    declarations = re.findall(r'^\s*const\s+LIVE_TX\s*:\s*bool\s*=\s*([^;]+);', code, re.M)
    expression = re.sub(r"\s+", "", declarations[0]) if len(declarations) == 1 else None
    old_gate = 'cfg!(feature="upper-exo-tx-trial")'
    dual_gates = {
        'cfg!(any(feature="upper-exo-tx-trial",feature="channel-tx"))',
        'cfg!(any(feature="channel-tx",feature="upper-exo-tx-trial"))',
    }
    # rustfmt may retain trailing commas inside multiline cfg! predicates.
    expression = expression.replace(",)", ")") if expression else expression
    dual = expression in dual_gates
    if not (dual or expression == old_gate and profile != "channel-tx"
            or expression == "false" and profile == "rx"):
        raise ValueError("Packaged source must set LIVE_TX=false or use the selected explicit TX feature")
    features = manifest.get("features", {})
    expected = {"default": [], TRIAL_FEATURE: [], CHANNEL_TX_FEATURE: []}
    legacy = {"default": [], TRIAL_FEATURE: []}
    if features != expected and not (not dual and features in ({"default": []}, legacy)):
        raise ValueError("Firmware feature declarations must be exact, empty, and RX by default")
    if expression == old_gate and TRIAL_FEATURE not in features:
        raise ValueError("Trial feature declaration is missing")
    if dual:
        compact = re.sub(r"\s+", "", code).replace(",)", ")")
        exclusions = (
            '#[cfg(all(feature="upper-exo-tx-trial",feature="channel-tx"))]compile_error!(',
            '#[cfg(all(feature="channel-tx",feature="upper-exo-tx-trial"))]compile_error!(',
        )
        if not any(exclusion in compact for exclusion in exclusions):
            raise ValueError("Both TX features must be rejected by an explicit compile_error")
    # Guard tokens must be calls in source, not quoted descriptions. These
    # checks deliberately remain narrower than a full control-flow proof.
    calls = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)
    if profile == "upper-exo-trial" and any(not re.search(pattern, calls) for pattern in (
            r"\btx_trial\s*::\s*permits\s*\(",
            r"\btx_trial\s*::\s*record_permitted\s*\(",
            r"\btx_trial\s*::\s*hold_within_limit\s*\(")):
        raise ValueError("Trial source must retain the allowlist and hold deadline")
    if profile == "channel-tx" and any(not re.search(pattern, calls) for pattern in (
            r"\btx_policy\s*::\s*authorize\s*\(",
            r"\btx_policy\s*::\s*hold_within_limit\s*\(",
            r"\.\s*still_held\s*\(")):
        raise ValueError("Channel TX source must retain authorization, held-context checks, and hold deadline")


def validate_inputs(folder, root, profile="rx"):
    """Validate immutable build evidence before creating package files."""
    if profile not in PROFILES:
        raise ValueError("Unknown candidate profile")
    trial = profile == "upper-exo-trial"
    channel_tx = profile == "channel-tx"
    expected_policy, expected_features, _ = PROFILES[profile]
    verification = json.loads((folder / "verification.json").read_text(encoding="utf-8-sig"))
    layout = json.loads((folder / "image/image-layout.json").read_text())
    early = json.loads((folder / "image/early-machine-code-check.json").read_text())
    decoded = json.loads((folder / "boot-log-emulation-decode/integration.json").read_text())
    if verification["status"] != "passed" or early["passed"] is not True:
        raise ValueError("Expected successful build and linked-instruction checks")
    if any(verification.get(key) is not False for key in
           ("device_accessed", "flash_performed", "hardware_qualified")):
        raise ValueError("Expected explicit offline verification scope")
    if verification.get("preserved_inputs_unchanged") is not True:
        raise ValueError("Expected preserved input verification")
    binary, elf = folder / f"image/UV98-diag-{VERSION}.bin", folder / f"image/UV98-diag-{VERSION}.elf"
    elf_data, bin_data = elf.read_bytes(), binary.read_bytes()
    expected_image, expected_layout = export(elf_data)
    if bin_data != expected_image or layout != expected_layout:
        raise ValueError("BIN/layout differs from checked ELF re-export")
    if digest(binary) != layout["bin_sha256"] or digest(elf) != layout["elf_sha256"]:
        raise ValueError("Image changed after export")
    if early["elf_sha256"] != digest(elf):
        raise ValueError("Instruction audit does not belong to the packaged ELF")
    if decoded["passed"] is not True or decoded["source_elf_sha256"] != digest(elf) or decoded["source_report_sha256"] != digest(folder / "image/early-machine-code-check.json"):
        raise ValueError("Host decoder integration does not belong to the packaged audit/ELF")
    if verification.get("firmware_features", []) != expected_features:
        raise ValueError("Build feature evidence differs from the selected candidate profile")
    policy = release_policy(elf_data, layout["layout"], expected_policy)
    source = root / "firmware-diagnostic/src/main.rs"
    text = source.read_text(encoding="utf-8-sig")
    firmware_manifest = root / "firmware-diagnostic/Cargo.toml"
    validate_source(text, tomllib.loads(firmware_manifest.read_text(encoding="utf-8-sig")), profile)
    if re.findall(r"^const LOWER_PTT_CODE:\s*Option<u8>\s*=\s*(\w+)\s*;", text, re.M) != ["None"]:
        raise ValueError("Packaged source must leave physical side-key assignment pending")
    if not (root / "DIAGNOSTIC-IMAGE-RU.txt").read_text(encoding="utf-8-sig").startswith(f"UV98 Rust {VERSION}"):
        raise ValueError("Diagnostic README version is stale")
    calibration = root / "crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
    calibration_data = calibration.read_bytes()
    if len(calibration_data) != 4096 or digest(calibration) != verification.get("preserved_calibration_sha256"):
        raise ValueError("Preserved calibration fixture does not match build verification")
    if calibration_data in bin_data:
        raise ValueError("Full private calibration block unexpectedly embedded in BIN")
    return {"live_tx_enabled": trial or channel_tx,
            "physical_sidekey_mapping": ("upper_PB6_only_lower_unmapped" if trial or channel_tx else "pending_live_adc_observation"),
            "candidate_scope": ("upper PTT A / CH002 EXO Rep / LOW / 5s maximum; no live RF acceptance"
                                if trial else "explicit PTT / FM channel and VFO / A-B and reverse / power codes 0..2 / calibrated plans and traced stock routes / 30s maximum; no live RF acceptance"
                                if channel_tx else "offline RX/AM radio candidate; no live RF acceptance"),
            "profile": profile,
            "trial_restrictions": ({"channel_one_based": 2, "slot": "A", "frequency_hz": 432975000,
                                    "tx_ctcss_hz": 77.0, "power_code": 0, "max_hold_ms": 5000}
                                   if trial else None),
            "channel_tx_constraints": ({"modulation": "FM", "max_hold_ms": 30000,
                                        "power_codes": [0, 1, 2], "explicit_ptt_required": True,
                                        "calibrated_plan_required": True,
                                        "lower_physical_ptt_mapped": False}
                                       if channel_tx else None),
            "compiled_features": ["FM_RX", "AM_RX", "channel_and_vfo_scan", "A_B_selection",
                                  "zones", "key_shortcuts"] + (["FM_TX_UPPER_EXO_LIMITED"] if trial else
                                                              ["FM_TX_CHANNEL_VFO"] if channel_tx else []),
            "linked_release_policy": policy,
            "source_main_sha256": digest(source),
            "source_firmware_manifest_sha256": digest(firmware_manifest),
            "validation_scope": "linked release declaration, feature evidence/declarations and source assertions; not full TX reachability proof",
            "hardware_qualified": False, "live_test_performed": False}


def package(folder, root=ROOT, profile="rx"):
    folder, root = folder.resolve(), root.resolve()
    runtime = validate_inputs(folder, root, profile)
    source_files = [p for name in ("crates", "apps", "firmware", "firmware-diagnostic", "firmware-uart-probe",
                    "tools", "research", "reference", ".cargo")
                    for p in (root / name).rglob("*")
                    if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc"]
    source_files += [p for p in root.iterdir() if p.is_file() and
                     (p.suffix in (".toml", ".lock", ".ps1", ".txt") or p.name in (".gitignore", "AGENTS.md"))]
    capture_names = ("capture_boot_log.py", "test_capture_boot_log.py", "verify_boot_log_emulation.py", "BOOT-LOG-CAPTURE-RU.txt")
    capture_source = root / "tools/boot-log"
    if not capture_source.is_dir():
        capture_source = root.parent / "live-hardware"
    with zipfile.ZipFile(folder / "source.zip", "x", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(set(source_files)):
            archive.write(path, path.relative_to(root).as_posix())
        for name in capture_names:
            if capture_source / name not in source_files:
                archive.write(capture_source / name, "tools/boot-log/" + name)
    with zipfile.ZipFile(folder / "source.zip") as archive:
        if archive.testzip() is not None:
            raise ValueError("Source ZIP readback failed")
    for source, name in (("DIAGNOSTIC-IMAGE-RU.txt", "README-RU.txt"),
                         ("RECOVERY-PREPARATION-RU.txt", "RECOVERY-PREPARATION-RU.txt")):
        with (folder / name).open("xb") as stream:
            stream.write((root / source).read_bytes())
    capture = folder / "capture"
    capture.mkdir(exist_ok=False)
    for name in capture_names:
        source = capture_source / name
        with (capture / name).open("xb") as stream:
            stream.write(source.read_bytes())
    manifest = {"kind": "offline_diagnostic_image_package", "version": VERSION,
                "device_accessed": False, "flash_performed": False, "hardware_qualified": False,
                "runtime": runtime,
                "full_calibration_block_embedded_in_bin": False,
                "source_files": len(set(source_files)) + sum(capture_source / name not in source_files for name in capture_names),
                "files": [{"path": p.relative_to(folder).as_posix(), "bytes": p.stat().st_size,
                           "sha256": digest(p)} for p in sorted(folder.rglob("*")) if p.is_file()]}
    with (folder / "manifest.json").open("x", encoding="utf-8") as stream:
        json.dump(manifest, stream, indent=2)
        stream.write("\n")
    output = Path(str(folder) + ".zip")
    with zipfile.ZipFile(output, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(folder.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(folder).as_posix())
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None:
            raise ValueError("Package ZIP readback failed")
        for entry in manifest["files"]:
            if hashlib.sha256(archive.read(entry["path"])).hexdigest() != entry["sha256"]:
                raise ValueError(f"Packaged file changed: {entry['path']}")
    checksum = f"{digest(output)}  {output.name}\n"
    with output.with_suffix(".zip.sha256").open("x", encoding="ascii") as stream:
        stream.write(checksum)
    return {"zip": str(output), "bytes": output.stat().st_size,
                      "sha256": digest(output), "source_files": manifest["source_files"],
                      "device_accessed": False, "flash_performed": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build_directory", type=Path)
    parser.add_argument("--profile", choices=tuple(PROFILES), default="rx")
    args = parser.parse_args()
    print(json.dumps(package(args.build_directory, profile=args.profile), indent=2))


if __name__ == "__main__":
    main()
