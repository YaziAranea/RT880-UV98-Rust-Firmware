"""Export this project's checked ARM ELF as an application-only diagnostic BIN.

The entire provisional 246 KiB application window is explicit, with 0xFF in
unoccupied gaps/tail. The internal loader prefix and external NOR are excluded.
This is an offline build step, not a flasher or hardware qualification tool.
"""

import argparse
import hashlib
import json
from pathlib import Path

from check_elf import FLASH_START, FLASH_END, RAM_END, RAM_START, check_layout

VERSION = "0.1.8"
PROFILE = "diagnostic-" + VERSION


def export(data: bytes) -> tuple[bytes, dict]:
    layout = check_layout(data, profile=PROFILE)
    initial_msp = int(layout["initial_msp"], 16)
    image = bytearray(b"\xff" * (FLASH_END - FLASH_START))
    populated = 0
    high_ram = RAM_START
    copied = []
    for segment in layout["load_segments"]:
        start = int(segment["physical_address"], 16)
        virtual = int(segment["virtual_address"], 16)
        count = segment["file_bytes"]
        if RAM_START <= virtual < RAM_END:
            high_ram = max(high_ram, virtual + segment["memory_bytes"])
        if not count:
            continue
        source = segment["file_offset"]
        offset = start - FLASH_START
        # check_layout already rejects overlaps, truncated files and any load
        # outside the application window, including a RAM .data load address.
        image[offset:offset + count] = data[source:source + count]
        populated += count
        copied.append({"physical_address": f"0x{start:08X}", "bytes": count})
    stack_margin = initial_msp - high_ram
    if stack_margin < 16384:
        raise ValueError("Less than 16 KiB remains between static RAM allocations and initial MSP")
    report = {
        "schema": 1,
        "kind": "diagnostic_application_build_export",
        "elf_sha256": hashlib.sha256(data).hexdigest(),
        "bin_sha256": hashlib.sha256(image).hexdigest(),
        "bin_bytes": len(image),
        "application_base": f"0x{FLASH_START:08X}",
        "application_end_exclusive": f"0x{FLASH_END:08X}",
        "file_backed_load_bytes": populated,
        "gap_and_tail_fill_byte": "0xFF",
        "gap_and_tail_fill_bytes": len(image) - populated,
        "copied_segments": copied,
        "static_ram_high_water": f"0x{high_ram:08X}",
        "initial_msp": f"0x{initial_msp:08X}",
        "bytes_above_static_ram": stack_margin,
        "stack_margin_boundary": "initial_msp",
        "minimum_space_above_static_ram": 16384,
        "maximum_runtime_stack_measured": False,
        "bootloader_prefix_included": False,
        "external_nor_included": False,
        "hardware_qualified": False,
        "device_accessed": False,
        "flash_performed": False,
        "layout": layout,
    }
    return bytes(image), report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("new_output_directory", type=Path)
    args = parser.parse_args()
    data = args.elf.read_bytes()
    image, report = export(data)
    output = args.new_output_directory
    output.mkdir(parents=True, exist_ok=False)
    for name, content in (
        (f"UV98-diag-{VERSION}.bin", image),
        (f"UV98-diag-{VERSION}.elf", data),
        ("image-layout.json", (json.dumps(report, indent=2) + "\n").encode()),
    ):
        with (output / name).open("xb") as stream:
            stream.write(content)
        if (output / name).read_bytes() != content:
            raise OSError(f"Artifact readback mismatch: {name}")
    print(json.dumps({key: report[key] for key in (
        "bin_bytes", "bin_sha256", "file_backed_load_bytes",
        "bytes_above_static_ram", "hardware_qualified")}, indent=2))


if __name__ == "__main__":
    main()
