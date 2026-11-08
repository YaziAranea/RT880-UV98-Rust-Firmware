#!/usr/bin/env python3
"""Offline ELF layout check for the provisional UV-98 Plus / RT-880 profile.

This checks file structure and linker placement, not hardware compatibility.
It never opens a serial port, executes the input, or accesses a device.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys


FLASH_START = 0x08002800
FLASH_END = 0x08040000
RAM_START = 0x20000000
RAM_END = 0x2000C000
DIAGNOSTIC_MSP = 0x200099C8
DIAGNOSTIC_RESET = 0x08002AC1
DIAGNOSTIC_PROFILES = ("diagnostic-0.1.3", "diagnostic-0.1.4", "diagnostic-0.1.5", "diagnostic-0.1.6", "diagnostic-0.1.7", "diagnostic-0.1.8")
PROFILES = ("stage0", *DIAGNOSTIC_PROFILES)
ELF_HEADER = struct.Struct("<16sHHIIIIIHHHHHH")
PROGRAM_HEADER = struct.Struct("<IIIIIIII")
SECTION_HEADER = struct.Struct("<IIIIIIIIII")
PT_LOAD = 1
SHT_PROGBITS = 1
SHT_STRTAB = 3
SHT_NOBITS = 8
SHF_ALLOC = 2
PF_X = 1


class LayoutError(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def require(condition: bool, code: str, message: str) -> None:
    if not condition:
        raise LayoutError(code, message)


def contained(start: int, size: int, lower: int, upper: int) -> bool:
    return lower <= start <= upper and 0 <= size <= upper - start


def check_file_range(data: bytes, offset: int, size: int, label: str) -> None:
    require(contained(offset, size, 0, len(data)), "truncated",
            f"{label} extends beyond the input file")


def check_address_range(address: int, size: int, label: str) -> None:
    require(contained(address, size, 0, 1 << 32), "address_overflow",
            f"{label} overflows the 32-bit address space")


def check_layout(data: bytes, *, profile: str = "stage0") -> dict:
    require(profile in PROFILES, "unknown_profile", f"Unknown layout profile: {profile}")
    require(len(data) >= ELF_HEADER.size, "truncated", "ELF header is truncated")
    (ident, elf_type, machine, version, entry, phoff, shoff, flags, ehsize,
     phentsize, phnum, shentsize, shnum, shstrndx) = ELF_HEADER.unpack_from(data)
    require(ident[:4] == b"\x7fELF", "elf_magic", "Input is not an ELF file")
    require(ident[4:7] == bytes((1, 1, 1)), "elf_format",
            "Expected ELF32, little endian, identification version 1")
    require(elf_type == 2 and machine == 40 and version == 1, "elf_target",
            "Expected ARM ET_EXEC, ELF version 1")
    require(ehsize == ELF_HEADER.size, "header_size", "Unexpected ELF header size")
    require(0 < phnum < 0xFFFF and phentsize == PROGRAM_HEADER.size,
            "program_table", "Missing, extended, or unsupported program header table")
    require(0 < shnum < 0xFF00 and shentsize == SECTION_HEADER.size
            and 0 < shstrndx < shnum, "section_table",
            "Missing, extended, or unsupported section header table")
    require(phoff >= ehsize and shoff >= ehsize, "table_offset",
            "Header tables must not overlap the ELF header")
    check_file_range(data, phoff, phnum * phentsize, "Program header table")
    check_file_range(data, shoff, shnum * shentsize, "Section header table")

    loads = []
    for index in range(phnum):
        (kind, offset, virtual, physical, file_size, memory_size,
         permissions, alignment) = PROGRAM_HEADER.unpack_from(data, phoff + index * phentsize)
        label = f"Program segment {index}"
        check_file_range(data, offset, file_size, label)
        if kind != PT_LOAD:
            continue
        require(file_size <= memory_size, "segment_size", f"{label}: filesz exceeds memsz")
        check_address_range(virtual, memory_size, label + " virtual range")
        check_address_range(physical, file_size, label + " physical range")
        require(alignment in (0, 1) or alignment & (alignment - 1) == 0,
                "segment_alignment", f"{label}: alignment is not a power of two")
        require(alignment <= 1 or virtual % alignment == offset % alignment,
                "segment_alignment", f"{label}: virtual address and file offset are incongruent")
        in_flash = contained(virtual, memory_size, FLASH_START, FLASH_END)
        in_ram = contained(virtual, memory_size, RAM_START, RAM_END)
        require(in_flash or in_ram, "virtual_range",
                f"{label}: virtual range is outside application Flash and RAM")
        # The exported BIN deliberately fills only unallocated gaps with FF.
        # A Flash PT_LOAD zero-fill tail would instead require zero bytes and
        # could overlap another segment's physical .data initialization image.
        # This firmware's linker emits zero-fill allocations in RAM only.
        require(not in_flash or file_size == memory_size, "flash_zero_fill",
                f"{label}: application Flash PT_LOAD must be entirely file-backed")
        if file_size:
            require(contained(physical, file_size, FLASH_START, FLASH_END),
                    "physical_range", f"{label}: file-backed load touches protected or out-of-profile Flash")
            if in_flash:
                require(physical == virtual, "flash_mapping",
                        f"{label}: Flash load and execution addresses differ")
        loads.append(dict(index=index, offset=offset, virtual=virtual, physical=physical,
                          file_size=file_size, memory_size=memory_size, flags=permissions))
    require(bool(loads), "no_load_segments", "ELF has no PT_LOAD segments")
    for index, left in enumerate(loads):
        for right in loads[index + 1:]:
            for start, size in (("virtual", "memory_size"), ("physical", "file_size")):
                if left[size] and right[size]:
                    overlap = max(left[start], right[start]) < min(
                        left[start] + left[size], right[start] + right[size])
                    require(not overlap, "overlapping_loads", f"PT_LOAD {start} ranges overlap")

    sections = [SECTION_HEADER.unpack_from(data, shoff + index * shentsize)
                for index in range(shnum)]
    require(sections[0] == (0,) * 10, "null_section", "Expected an empty section zero")
    strings = sections[shstrndx]
    require(strings[1] == SHT_STRTAB, "section_names", "Section names do not use SHT_STRTAB")
    check_file_range(data, strings[4], strings[5], "Section name table")
    names = data[strings[4]:strings[4] + strings[5]]
    require(bool(names) and names[0] == 0, "section_names", "Malformed section name table")
    vectors = []
    for index, section in enumerate(sections):
        name_offset, kind, attributes, address, offset, size = section[:6]
        require(name_offset < len(names), "section_names", "Section name offset is out of range")
        name_end = names.find(b"\0", name_offset)
        require(name_end >= 0, "section_names", "Unterminated section name")
        name = names[name_offset:name_end]
        if kind != SHT_NOBITS:
            check_file_range(data, offset, size, f"Section {index}")
        if attributes & SHF_ALLOC and size:
            check_address_range(address, size, f"Section {index}")
            owners = [segment for segment in loads if contained(
                address, size, segment["virtual"], segment["virtual"] + segment["memory_size"])]
            require(bool(owners), "section_mapping", f"Allocated section {index} is outside PT_LOAD")
            if kind != SHT_NOBITS:
                require(any(contained(address, size, segment["virtual"],
                                      segment["virtual"] + segment["file_size"])
                            and offset == segment["offset"] + address - segment["virtual"]
                            for segment in owners), "section_mapping",
                        f"Allocated section {index} does not match file-backed PT_LOAD mapping")
        if name == b".vector_table":
            vectors.append(section)
    require(len(vectors) == 1, "vector_section", "Expected exactly one .vector_table section")
    vector = vectors[0]
    require(vector[1] == SHT_PROGBITS and vector[2] & SHF_ALLOC
            and vector[3] == FLASH_START and vector[5] >= 8 and vector[5] % 4 == 0,
            "vector_layout", "Vector table must be allocated at 0x08002800 with complete words")
    stack, reset = struct.unpack_from("<II", data, vector[4])
    require(RAM_START < stack <= RAM_END and stack % 8 == 0, "stack_range",
            "Initial MSP must be 8-byte aligned and within the RAM stack boundary")
    expected_stack = DIAGNOSTIC_MSP if profile in DIAGNOSTIC_PROFILES else RAM_END
    require(stack == expected_stack, "stack_profile",
            f"{profile} initial MSP must be 0x{expected_stack:08X}")
    require(reset & 1 == 1, "reset_thumb", "Reset vector must have the Thumb bit set")
    code_address = reset & ~1
    require(contained(code_address, 2, FLASH_START, FLASH_END)
            and any(segment["flags"] & PF_X and contained(
                code_address, 2, segment["virtual"], segment["virtual"] + segment["file_size"])
                    for segment in loads), "reset_range",
            "Reset must point into file-backed executable application Flash")
    require(entry == reset, "entry_mismatch", "ELF entry must equal the Thumb Reset vector")
    if profile in DIAGNOSTIC_PROFILES:
        require(reset == DIAGNOSTIC_RESET, "reset_profile",
                f"{profile} Reset must match the observed stock vector 0x08002AC1")
        require(all(segment["virtual"] + segment["memory_size"] <= stack
                    for segment in loads if RAM_START <= segment["virtual"] < RAM_END),
                "static_ram_above_stack", "Static RAM allocation extends above diagnostic initial MSP")
    return {
        "layout_profile": profile,
        "entry": f"0x{entry:08X}", "initial_msp": f"0x{stack:08X}",
        "reset_vector": f"0x{reset:08X}", "vector_address": f"0x{vector[3]:08X}",
        "vector_bytes": vector[5], "elf_flags": f"0x{flags:08X}",
        "load_segments": [{"index": segment["index"], "file_offset": segment["offset"],
                           "virtual_address": f"0x{segment['virtual']:08X}",
                           "physical_address": f"0x{segment['physical']:08X}",
                           "file_bytes": segment["file_size"], "memory_bytes": segment["memory_size"],
                           "flags": segment["flags"]} for segment in loads],
    }


def analyze_elf(data: bytes, artifact_name: str = "input.elf", *, profile: str = "stage0") -> dict:
    report = {
        "schema_version": 1, "artifact": Path(artifact_name).name,
        "size_bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
        "validation_scope": "ELF layout only", "hardware_qualified": False,
        "valid": False, "errors": [],
        "profile": {"flash_start": "0x08002800", "flash_end_exclusive": "0x08040000",
                    "ram_start": "0x20000000", "ram_end_exclusive": "0x2000C000"},
    }
    try:
        report.update(check_layout(data, profile=profile))
        report["valid"] = True
    except LayoutError as error:
        report["errors"].append({"code": error.code, "message": str(error)})
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--output", type=Path, help="Write JSON report, creating parent directories")
    parser.add_argument("--profile", choices=PROFILES, default="stage0")
    args = parser.parse_args(argv)
    try:
        report = analyze_elf(args.elf.read_bytes(), args.elf.name, profile=args.profile)
    except OSError as error:
        report = {"schema_version": 1, "artifact": args.elf.name, "valid": False,
                  "validation_scope": "ELF layout only", "hardware_qualified": False,
                  "errors": [{"code": "input_io", "message": f"Cannot read input ({type(error).__name__})"}]}
    output = json.dumps(report, indent=2) + "\n"
    if args.output:
        try:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(output, encoding="utf-8")
        except OSError as error:
            print(f"Cannot write report ({type(error).__name__})", file=sys.stderr)
            return 2
        status = "PASS" if report["valid"] else "FAIL"
        print(f"{status}: ELF layout only; hardware qualification: false")
        for error in report["errors"]:
            print(f"{error['code']}: {error['message']}", file=sys.stderr)
    else:
        print(output, end="")
    return 0 if report["valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
