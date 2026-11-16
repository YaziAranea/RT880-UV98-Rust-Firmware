#!/usr/bin/env python3
"""Verify the R41 internal-flash settings reservation in an ELF/BIN pair."""

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import struct

from check_elf import (
    ELF_HEADER,
    FLASH_END,
    FLASH_START,
    PF_X,
    RAM_END,
    RAM_START,
    SECTION_HEADER,
    SHF_ALLOC,
    SHT_PROGBITS,
    SHT_STRTAB,
    LayoutError,
    check_layout,
    contained,
    require,
)
from export_diagnostic_image import PROFILE

JOURNAL_START = 0x0803F000
JOURNAL_BYTES = FLASH_END - JOURNAL_START
BIN_BYTES = FLASH_END - FLASH_START
SHT_SYMTAB = 2
SHN_ABS = 0xFFF1
SHF_EXECINSTR = 4
STT_FUNC = 2
SYMBOL = struct.Struct("<IIIBBH")
OP_START = b"__uv98_flash_op"
OP_END = b"__uv98_flash_op_end"
SETTINGS_START = b"__settings_start"
SETTINGS_END = b"__settings_end"
COPY_START = b"__sdata"
COPY_END = b"__edata"
COPY_LMA = b"__sidata"
REQUIRED_NAMES = {SETTINGS_START, SETTINGS_END, OP_START, OP_END, COPY_START, COPY_END, COPY_LMA}


@dataclass(frozen=True)
class Symbol:
    value: int
    size: int
    type: int
    section: int


def section_headers(elf):
    header = ELF_HEADER.unpack_from(elf)
    table = header[6]
    sections = [SECTION_HEADER.unpack_from(elf, table + index * SECTION_HEADER.size)
                for index in range(header[12])]
    names = sections[header[13]]
    return sections, elf[names[4]:names[4] + names[5]]


def read_symbols(elf, sections):
    selected = {}
    for table in sections:
        if table[1] != SHT_SYMTAB:
            continue
        offset, count, link = table[4:7]
        require(table[9] == SYMBOL.size and count % SYMBOL.size == 0 and 0 < link < len(sections),
                "symbol_table", "Invalid ELF symbol table geometry")
        strings = sections[link]
        require(strings[1] == SHT_STRTAB, "symbol_table", "Symbol names do not use SHT_STRTAB")
        names = bytes(elf[strings[4]:strings[4] + strings[5]])
        for position in range(offset, offset + count, SYMBOL.size):
            name_offset, value, size, info, _, index = SYMBOL.unpack_from(elf, position)
            end = names.find(b"\0", name_offset)
            require(0 <= name_offset <= end, "symbol_table", "Invalid ELF symbol name")
            name = names[name_offset:end]
            if name in REQUIRED_NAMES:
                require(name not in selected and (0 < index < len(sections) or index == SHN_ABS),
                        "symbol_table", f"Duplicate or undefined {name.decode()}")
                selected[name] = Symbol(value, size, info & 15, index)
    return selected


def need_symbols(symbols, *names):
    missing = [name.decode() for name in names if name not in symbols]
    require(not missing, "missing_symbol", f"Missing ELF symbols {', '.join(missing)}")
    return [symbols[name] for name in names]


def check_ram_routine(loads, sections, names, symbols):
    op, end, copy_start, copy_end, copy_lma = need_symbols(
        symbols, OP_START, OP_END, COPY_START, COPY_END, COPY_LMA)
    start = op.value & ~1
    size = end.value - start
    require(op.type == STT_FUNC and op.value & 1 and end.value % 2 == 0
            and size > 0 and op.size == size,
            "routine_symbols", "Expected a sized Thumb function ending at __uv98_flash_op_end")
    require(0 < op.section < len(sections) and op.section == end.section,
            "routine_section", "Routine and end label must belong to the same section")
    owner = sections[op.section]
    name_end = names.index(b"\0", owner[0])
    name = names[owner[0]:name_end]
    data_flags = SHF_ALLOC | SHF_EXECINSTR
    require(name == b".data" and owner[1] == SHT_PROGBITS and owner[2] & data_flags == data_flags,
            "routine_section", "Routine must be in allocated executable .data")
    require(RAM_START <= copy_start.value <= start < end.value <= copy_end.value <= RAM_END,
            "routine_vma", "Routine must be inside the SRAM startup copy range")
    copy_size = copy_end.value - copy_start.value
    require(contained(copy_start.value, copy_size, owner[3], owner[3] + owner[5]),
            "data_copy", "Startup .data copy range exceeds its file-backed section")
    for segment in loads:
        vma = int(segment["virtual_address"], 16)
        if not segment["flags"] & PF_X or not contained(
                copy_start.value, copy_size, vma, vma + segment["file_bytes"]):
            continue
        lma = int(segment["physical_address"], 16) + copy_start.value - vma
        require(lma == copy_lma.value and contained(lma, copy_size, FLASH_START, JOURNAL_START),
                "data_copy", "__sidata differs from the file-backed Flash load address")
        return {"sram_start": f"0x{start:08X}", "sram_end_exclusive": f"0x{end.value:08X}",
                "flash_load_start": f"0x{lma + start - copy_start.value:08X}", "bytes": size}
    raise LayoutError("data_copy", "Startup .data is outside an executable file-backed load")


def verify(elf, binary, *, diagnostic=False):
    layout = check_layout(elf, profile=PROFILE)
    loads = layout["load_segments"]
    flash_end = max(int(segment["physical_address"], 16) + segment["file_bytes"]
                    for segment in loads if segment["file_bytes"])
    require(flash_end <= JOURNAL_START, "journal_overlap", "A file-backed Flash load enters the journal")
    sections, section_names = section_headers(elf)
    symbols = read_symbols(elf, sections)
    start, end = need_symbols(symbols, SETTINGS_START, SETTINGS_END)
    require((start.value, end.value) == (JOURNAL_START, FLASH_END),
            "journal_symbols", "Journal symbols must delimit 0x0803F000 to 0x08040000")
    routine = None
    if not diagnostic or OP_START in symbols or OP_END in symbols:
        routine = check_ram_routine(loads, sections, section_names, symbols)
    require(len(binary) == BIN_BYTES, "bin_size", f"Expected a {BIN_BYTES}-byte application BIN")
    require(binary[-JOURNAL_BYTES:] == b"\xff" * JOURNAL_BYTES,
            "bin_journal", "Final 4096 BIN bytes must all be 0xFF")
    for segment in loads:
        count = segment["file_bytes"]
        if count:
            source = segment["file_offset"]
            destination = int(segment["physical_address"], 16) - FLASH_START
            require(binary[destination:destination + count] == elf[source:source + count],
                    "bin_elf_mismatch", f"BIN differs from ELF load segment {segment['index']}")
    return {
        "valid": True,
        "diagnostic": diagnostic,
        "elf_sha256": hashlib.sha256(elf).hexdigest(),
        "bin_sha256": hashlib.sha256(binary).hexdigest(),
        "bin_bytes": len(binary),
        "journal_start": f"0x{JOURNAL_START:08X}",
        "journal_end_exclusive": f"0x{FLASH_END:08X}",
        "last_flash_load_end_exclusive": f"0x{flash_end:08X}",
        "erased_bin_tail_bytes": JOURNAL_BYTES,
        "ram_routine": routine,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--diagnostic", action="store_true",
                        help="Allow the PA5 diagnostic ELF to omit the journal writer")
    args = parser.parse_args(argv)
    try:
        report = verify(args.elf.read_bytes(), args.binary.read_bytes(), diagnostic=args.diagnostic)
    except OSError as error:
        report = {"valid": False, "code": "input_io", "message": f"Cannot read input ({type(error).__name__})"}
    except LayoutError as error:
        report = {"valid": False, "code": error.code, "message": str(error)}
    print(json.dumps(report, indent=2))
    return 0 if report["valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
