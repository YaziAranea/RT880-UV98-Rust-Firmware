"""Extract reproducible channel evidence from existing decompiled C# text only.

No CPS assembly loading/execution, firmware execution, serial access or device I/O.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent
METHODS = (
    "CopyArrayToString", "SaveCHName", "ClearSingleCH", "SaveSingle",
    "CheckFreq", "SaveSubaudio", "SetSubaudio", "SetChInfo",
)


def extract(text, name):
    pattern = (r"^\t(?:public|private|protected)[^\n]*\b" + re.escape(name)
               + r"\([^\n]*\)\n\t\{\n")
    match = re.search(pattern, text, re.M)
    if match is None:
        raise ValueError(f"method not found: {name}")
    end = text.index("\n\t}\n", match.end()) + len("\n\t}\n")
    return text[match.start():end], text[:match.start()].count("\n") + 1


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rt-source", type=Path, default=HERE.parent / "rt880-v110-decompiled/DM550_NOLCD/MainForm.cs")
    parser.add_argument("--uv-source", type=Path, default=HERE.parent.parent / "uv98-cps-decompiled/DM550_NOLCD/MainForm.cs")
    parser.add_argument("--output", type=Path, default=HERE)
    args = parser.parse_args()
    rt = args.rt_source.read_text(encoding="utf-8-sig")
    uv = args.uv_source.read_text(encoding="utf-8-sig")
    comparisons, excerpts = {}, []
    for name in METHODS:
        left, left_line = extract(rt, name)
        right, right_line = extract(uv, name)
        comparisons[name] = {
            "rt880_line": left_line,
            "uv98_line": right_line,
            "same_decompiled_body": left == right,
            "rt880_method_sha256": sha(left.encode("utf-8")),
            "uv98_method_sha256": sha(right.encode("utf-8")),
        }
        excerpts.append(f"// RT880 MainForm.cs:{left_line}; {name}\n{left}\n")
    fields = [
        {"name": "modulation_raw", "offset": 0, "bytes": 1, "patch_supported": False},
        {"name": "operation_mode", "offset": 1, "bytes": 1, "codes": {"0": "R+T", "1": "RX", "2": "TX"}},
        {"name": "active_marker", "offset": 2, "bytes": 1, "active_when": 1, "patch_supported": False},
        {"name": "bandwidth", "offset": 3, "bytes": 1, "codes": {"0": "Wide", "1": "Narrow"}},
        {"name": "rx_tone", "offset": 4, "bytes": 2, "endian": "little"},
        {"name": "rx_frequency_code", "offset": 6, "bytes": 4, "endian": "little", "ordinary_ui_mhz_divisor": 100000},
        {"name": "tx_frequency_code", "offset": 10, "bytes": 4, "endian": "little", "ordinary_ui_mhz_divisor": 100000},
        {"name": "tx_tone", "offset": 14, "bytes": 2, "endian": "little"},
        {"name": "power", "offset": 16, "bytes": 1, "codes": {"0": "Low", "1": "Medium", "2": "High"}},
        {"name": "tx_priority", "offset": 17, "bytes": 1, "patch_supported": False},
        {"name": "tot_ctdc", "offset": 18, "bytes": 1, "patch_supported": False},
        {"name": "scan_remove", "offset": 19, "mask": 128, "other_bits_preserved": True},
        {"name": "mute_codes", "offset": 20, "bytes": 12, "patch_supported": False},
        {"name": "name_raw", "offset": 32, "bytes": 16, "encoding": "GBK", "padding": 255},
    ]
    report = {
        "target_exe": "RT-880 CPS_V1_10_250806.exe",
        "target_exe_sha256": "b2533229c783c86150a4230a8a51c5ee274092e9f1b036bbb7a5b511c34dabd6",
        "rt880_source_sha256": sha(args.rt_source.read_bytes()),
        "uv98_source_sha256": sha(args.uv_source.read_bytes()),
        "target_executed": False,
        "device_accessed": False,
        "hardware_qualified": False,
        "method": "static comparison of ILSpy 8.2 decompiled source text",
        "channel_section_bytes": 49152,
        "channel_count": 1024,
        "record_bytes": 48,
        "fields": fields,
        "method_comparison": comparisons,
        "canonical_tone_words": {
            "None": "0x0000", "CTCSS": "0x1000 | tenths_hz (0..4095)",
            "DCS normal": "0x2000 | octal_numeric_code (0..511)",
            "DCS inverted": "0x3000 | octal_numeric_code (0..511)",
        },
        "limitations": [
            "Static source evidence, not CPS runtime or RF hardware validation.",
            "CPS SetChInfo uses signed Int32 arithmetic; codec retains the unsigned raw 32-bit frequency code.",
            "No TX-frequency disabled sentinel is established; CPS RX-only is separate byte1=1.",
            "Tone decoder recognizes canonical encodings only; malformed words remain raw.",
            "GBK text conversion belongs to host UI; core name helper copies bytes only.",
            "Stock SaveSingle can erase records on exceptions; lossless patch intentionally does not reproduce this.",
        ],
    }
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "channel-evidence.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (args.output / "source-excerpts.cs.txt").write_text("\n".join(excerpts), encoding="utf-8")
    print(json.dumps({"methods": len(comparisons), "identical": sum(item["same_decompiled_body"] for item in comparisons.values()), "target_executed": False, "device_accessed": False}))


if __name__ == "__main__":
    main()
