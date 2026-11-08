"""Audit an already-decompiled RT880 updater HEX literal without executing it.

Accepts a plain C# text file from a static metadata/decompiler tool. It neither
opens nor loads an EXE/assembly. No extracted firmware file is created. A hash
of the observed stock host's zero-initialized array is calculated in memory
only to compare it with an independently published blob. It is NOT our
preflight padding policy and is not a recommendation to create such an image.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re
from image_preflight import APP_BASE, APP_BYTES, inspect, parse_hex


def audit(path: Path) -> dict:
    source = path.read_bytes()
    if len(source) > 8 * 1024 * 1024:
        raise ValueError("decompiled text exceeds bounded input size")
    text = source.decode("utf-8-sig")
    matches = re.findall(r'\bstring\s+allcode\s*=\s*"([:0-9A-Fa-f]+)"\s*;', text)
    if len(matches) != 1:
        raise ValueError("expected exactly one plain allcode HEX literal; no expression execution")
    records = [":" + part for part in matches[0].split(":") if part]
    encoded = ("\n".join(records) + "\n").encode("ascii")
    image = parse_hex(encoded)
    result = inspect(image)
    upper_values = []
    for line in records:
        record = bytes.fromhex(line[1:])
        if record[3] == 4:
            upper_values.append(int.from_bytes(record[4:-1], "big"))
    if upper_values != list(range(0x0800, 0x0800 + len(upper_values))):
        raise ValueError("type04 sequence does not support the reviewed legacy host mapping")

    # Observed UpDate_Load clears the entire array to zero before assigning
    # the decoded records. Model that behavior for a hash only; write no image.
    stock_host_array = bytearray(APP_BYTES)
    for segment in image.segments:
        offset = segment.address - APP_BASE
        stock_host_array[offset:offset + len(segment.data)] = segment.data
    git_blob = hashlib.sha1(b"blob " + str(len(stock_host_array)).encode() + b"\0" + stock_host_array).hexdigest()
    return {
        "kind": "static_decompiled_literal_audit",
        "decompiled_source_sha256": hashlib.sha256(source).hexdigest(),
        "updater_or_constructor_executed": False,
        "assembly_loaded": False,
        "firmware_image_written": False,
        "type04_values": [f"0x{v:04X}" for v in upper_values],
        "normalized_literal_hex_sha256": hashlib.sha256(encoded).hexdigest(),
        "stock_host_zero_initialized_array_bytes": len(stock_host_array),
        "stock_host_zero_initialized_array_sha256": hashlib.sha256(stock_host_array).hexdigest(),
        "stock_host_zero_initialized_array_git_blob_sha1": git_blob,
        "preflight_of_present_hex_bytes": result,
        "qualification": "related RT880 updater only; not exact UV98 recovery evidence",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("decompiled_source", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.decompiled_source), indent=2))
