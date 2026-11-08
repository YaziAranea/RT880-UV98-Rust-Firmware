"""Read explicitly supplied saved CPS files; report hashes, never file contents.

This is a structural inventory, not a claim of device acquisition or semantic
validity. The Rust verify_document example checks the actual Document API.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

SECTIONS = (("Cfg", 0, 4096), ("Channels", 4096, 49152),
            ("Fm", 53248, 4096), ("Time", 57344, 32768),
            ("Zones", 90112, 131072))
FILE_LEN = 221184


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("files", type=Path, nargs="+")
    args = parser.parse_args()
    contents: list[bytes] = []
    entries = []
    for index, path in enumerate(args.files):
        data = path.read_bytes()
        if len(data) != FILE_LEN:
            raise ValueError(f"file index {index}: expected {FILE_LEN} bytes, got {len(data)}")
        contents.append(data)
        entries.append({
            "index": index,
            "source_path": str(path.resolve()),
            "length": len(data),
            "sha256": sha256(data),
            "expected_length_matches": True,
            "sections": [{"section": name, "file_offset": offset, "length": length,
                          "sha256": sha256(data[offset:offset + length]),
                          "all_zero": not any(data[offset:offset + length]),
                          "all_ff": data[offset:offset + length] == b"\xff" * length}
                         for name, offset, length in SECTIONS],
            "active_channel_markers_equal_one": sum(
                data[4096 + record * 48 + 2] == 1 for record in range(1024)),
        })
    comparisons = []
    for first in range(len(contents)):
        for second in range(first + 1, len(contents)):
            left, right = contents[first], contents[second]
            comparisons.append({
                "first_index": first, "second_index": second,
                "identical": left == right,
                "changed_bytes_by_section": {
                    name: sum(a != b for a, b in zip(left[offset:offset + length], right[offset:offset + length]))
                    for name, offset, length in SECTIONS},
            })
    report = {
        "schema": 1,
        "kind": "existing_local_saved_cps_file_structural_inventory",
        "acquisition_provenance": "unknown; file existence and length do not prove a radio read",
        "time_section_provenance": "unknown; standard examined CPS read does not fill this buffer",
        "source_files_written": False,
        "source_contents_embedded": False,
        "native_files_in_source_package": False,
        "rust_document_roundtrip_checked_by_this_script": False,
        "files": entries,
        "comparisons": comparisons,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({"file_count": len(entries), "all_exact_length": True,
                      "manifest": str(args.output)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
