"""Synthetic ELF/BIN mutations for the internal settings journal boundary."""

import contextlib
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest

import check_elf
import check_settings_journal as journal
from export_diagnostic_image import export
from test_check_elf import diagnostic_fixture

SECTION_TABLE = 0x200
SYMBOL_TABLE = 0x400
DATA_PROGRAM_HEADER = 116
DATA_SECTION = 3
DATA_VMA = 0x20000000
DATA_LMA = 0x08002AC8


def fixture(*, without=()):
    elf = diagnostic_fixture()
    elf.extend(bytes(0x500 - len(elf)))
    struct.pack_into("<H", elf, 48, 7)  # e_shnum
    struct.pack_into("<I", elf, DATA_PROGRAM_HEADER + 24, 7)  # PT_LOAD RWX
    struct.pack_into("<I", elf, SECTION_TABLE + DATA_SECTION * 40 + 8, 7)  # .data WAX
    symbols = [
        (journal.SETTINGS_START, journal.JOURNAL_START, 0, 0x10, journal.SHN_ABS),
        (journal.SETTINGS_END, check_elf.FLASH_END, 0, 0x10, journal.SHN_ABS),
        (journal.COPY_START, DATA_VMA, 0, 0x10, DATA_SECTION),
        (journal.COPY_END, DATA_VMA + 4, 0, 0x10, DATA_SECTION),
        (journal.COPY_LMA, DATA_LMA, 0, 0x10, journal.SHN_ABS),
        (journal.OP_START, DATA_VMA | 1, 4, 0x12, DATA_SECTION),
        (journal.OP_END, DATA_VMA + 4, 0, 0x10, DATA_SECTION),
    ]
    strings = bytearray(b"\0")
    offsets = {}
    for name, value, size, info, section in symbols:
        if name in without:
            continue
        offset = SYMBOL_TABLE + (len(offsets) + 1) * journal.SYMBOL.size
        journal.SYMBOL.pack_into(elf, offset, len(strings), value, size, info, 0, section)
        offsets[name] = offset
        strings.extend(name + b"\0")
    elf[0x350:0x350 + len(strings)] = strings
    check_elf.SECTION_HEADER.pack_into(
        elf, SECTION_TABLE + 5 * 40, 0, check_elf.SHT_STRTAB, 0, 0, 0x350, len(strings), 0, 0, 1, 0)
    check_elf.SECTION_HEADER.pack_into(
        elf, SECTION_TABLE + 6 * 40, 0, journal.SHT_SYMTAB, 0, 0, SYMBOL_TABLE,
        (len(offsets) + 1) * journal.SYMBOL.size, 5, 1, 4, journal.SYMBOL.size)
    binary, _ = export(elf)
    return elf, binary, offsets


class JournalVerifierTests(unittest.TestCase):
    def assert_rejected(self, elf, binary, code, *, diagnostic=False):
        with self.assertRaises(check_elf.LayoutError) as context:
            journal.verify(elf, binary, diagnostic=diagnostic)
        self.assertEqual(context.exception.code, code)

    def test_accepts_linked_routine_at_exact_flash_boundary(self):
        elf, _, symbols = fixture()
        exact_end = journal.JOURNAL_START - 4
        struct.pack_into("<I", elf, DATA_PROGRAM_HEADER + 12, exact_end)
        struct.pack_into("<I", elf, symbols[journal.COPY_LMA] + 4, exact_end)
        binary, _ = export(elf)
        result = journal.verify(elf, binary)
        self.assertTrue(result["valid"])
        self.assertEqual(result["last_flash_load_end_exclusive"], "0x0803F000")
        self.assertEqual(result["ram_routine"]["flash_load_start"], "0x0803EFFC")
        self.assertEqual(result["ram_routine"]["sram_start"], "0x20000000")

    def test_rejects_ram_load_overlapping_reserved_flash_even_if_bytes_are_erased(self):
        elf, binary, _ = fixture()
        struct.pack_into("<I", elf, DATA_PROGRAM_HEADER + 12, journal.JOURNAL_START - 2)
        elf[0x110:0x114] = b"\xff" * 4
        self.assert_rejected(elf, binary, "journal_overlap")

    def test_rejects_wrong_missing_and_undefined_reservation_symbols(self):
        elf, binary, symbols = fixture()
        struct.pack_into("<I", elf, symbols[journal.SETTINGS_START] + 4, journal.JOURNAL_START - 4)
        self.assert_rejected(elf, binary, "journal_symbols")
        elf, binary, _ = fixture(without=(journal.SETTINGS_END,))
        self.assert_rejected(elf, binary, "missing_symbol", diagnostic=True)
        elf, binary, symbols = fixture()
        struct.pack_into("<H", elf, symbols[journal.SETTINGS_END] + 14, 0)
        self.assert_rejected(elf, binary, "symbol_table")

    def test_diagnostic_allows_no_writer_but_rejects_partial_symbols(self):
        elf, binary, _ = fixture(without=(journal.OP_START, journal.OP_END))
        result = journal.verify(elf, binary, diagnostic=True)
        self.assertIsNone(result["ram_routine"])
        self.assert_rejected(elf, binary, "missing_symbol")
        elf, binary, _ = fixture(without=(journal.OP_END,))
        self.assert_rejected(elf, binary, "missing_symbol", diagnostic=True)

    def test_rejects_wrong_instruction_state_section_and_copy_mapping(self):
        for symbol, field_offset, encoding, value, code in [
            (journal.OP_START, 4, "<I", DATA_VMA, "routine_symbols"),
            (journal.OP_START, 14, "<H", 2, "routine_section"),
            (journal.COPY_START, 4, "<I", DATA_VMA + 2, "routine_vma"),
            (journal.COPY_END, 4, "<I", DATA_VMA + 8, "data_copy"),
            (journal.COPY_LMA, 4, "<I", DATA_LMA + 4, "data_copy"),
        ]:
            with self.subTest(symbol=symbol, code=code):
                elf, binary, symbols = fixture()
                struct.pack_into(encoding, elf, symbols[symbol] + field_offset, value)
                self.assert_rejected(elf, binary, code)
        elf, binary, _ = fixture()
        struct.pack_into("<I", elf, DATA_PROGRAM_HEADER + 24, 6)
        self.assert_rejected(elf, binary, "data_copy")

    def test_rejects_corrupt_symbol_table_reference(self):
        elf, binary, _ = fixture()
        struct.pack_into("<I", elf, SECTION_TABLE + 6 * 40 + 24, 8)
        self.assert_rejected(elf, binary, "symbol_table")

    def test_rejects_wrong_bin_size_programmed_journal_and_mismatched_payload(self):
        elf, binary, _ = fixture()
        self.assert_rejected(elf, binary[:-1], "bin_size")
        for index, code in [
            (-journal.JOURNAL_BYTES, "bin_journal"),
            (-1, "bin_journal"),
            (DATA_LMA - check_elf.FLASH_START, "bin_elf_mismatch"),
        ]:
            with self.subTest(index=index):
                modified = bytearray(binary)
                modified[index] ^= 1
                self.assert_rejected(elf, modified, code)

    def test_cli_returns_json_and_fail_status(self):
        elf, binary, _ = fixture()
        with tempfile.TemporaryDirectory() as directory:
            image = Path(directory, "image.elf")
            exported = Path(directory, "image.bin")
            image.write_bytes(elf)
            exported.write_bytes(binary[:-1])
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                self.assertEqual(journal.main([str(image), str(exported)]), 1)
            report = json.loads(stdout.getvalue())
            self.assertFalse(report["valid"])
            self.assertEqual(report["code"], "bin_size")


if __name__ == "__main__":
    unittest.main()
