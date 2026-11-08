"""Mutation tests for backup-critical ELF boundaries, using no hardware."""

import contextlib
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest

import check_elf


def fixture() -> bytearray:
    """A minimal ARM executable with vectors, code and a RAM .data load."""
    data = bytearray(0x300)
    names = b"\0.vector_table\0.text\0.data\0.shstrtab\0"
    data[0x150:0x150 + len(names)] = names
    ident = b"\x7fELF\x01\x01\x01" + bytes(9)
    check_elf.ELF_HEADER.pack_into(data, 0, ident, 2, 40, 1, 0x08002809,
                                 52, 0x200, 0x05000000, 52, 32, 3, 40, 5, 4)
    check_elf.PROGRAM_HEADER.pack_into(data, 52, 1, 0x100, 0x08002800,
                                     0x08002800, 8, 8, 4, 4)
    check_elf.PROGRAM_HEADER.pack_into(data, 84, 1, 0x108, 0x08002808,
                                     0x08002808, 8, 8, 5, 4)
    check_elf.PROGRAM_HEADER.pack_into(data, 116, 1, 0x110, 0x20000000,
                                     0x08002810, 4, 8, 6, 4)
    struct.pack_into("<II", data, 0x100, 0x2000C000, 0x08002809)
    data[0x108:0x110] = b"\x00\xbf\xfe\xe7" * 2
    data[0x110:0x114] = b"DATA"
    for index, values in enumerate([
        (0,) * 10,
        (1, 1, 2, 0x08002800, 0x100, 8, 0, 0, 4, 0),
        (15, 1, 6, 0x08002808, 0x108, 8, 0, 0, 4, 0),
        (21, 1, 3, 0x20000000, 0x110, 4, 0, 0, 4, 0),
        (27, 3, 0, 0, 0x150, len(names), 0, 0, 1, 0),
    ]):
        check_elf.SECTION_HEADER.pack_into(data, 0x200 + index * 40, *values)
    return data


def diagnostic_fixture() -> bytearray:
    """Same three loads, with the observed stock MSP and Reset location."""
    data = fixture()
    struct.pack_into("<I", data, 24, check_elf.DIAGNOSTIC_RESET)
    struct.pack_into("<II", data, 0x100, check_elf.DIAGNOSTIC_MSP, check_elf.DIAGNOSTIC_RESET)
    struct.pack_into("<II", data, 84 + 8, 0x08002AC0, 0x08002AC0)
    struct.pack_into("<I", data, 116 + 12, 0x08002AC8)
    struct.pack_into("<I", data, 0x200 + 2 * 40 + 12, 0x08002AC0)
    return data


class LayoutTests(unittest.TestCase):
    def assert_invalid(self, data, code):
        report = check_elf.analyze_elf(bytes(data))
        self.assertFalse(report["valid"])
        self.assertFalse(report["hardware_qualified"])
        self.assertEqual(report["errors"][0]["code"], code)

    def test_valid_flash_and_ram_loads(self):
        report = check_elf.analyze_elf(bytes(fixture()))
        self.assertTrue(report["valid"], report["errors"])
        self.assertFalse(report["hardware_qualified"])
        self.assertEqual(report["initial_msp"], "0x2000C000")

    def test_diagnostic_profile_is_explicit_and_exact(self):
        self.assertIn("diagnostic-0.1.8", check_elf.DIAGNOSTIC_PROFILES)
        self.assertIn("diagnostic-0.1.7", check_elf.DIAGNOSTIC_PROFILES)
        data = diagnostic_fixture()
        self.assert_invalid(data, "stack_profile")
        for profile in check_elf.DIAGNOSTIC_PROFILES:
            with self.subTest(profile=profile):
                report = check_elf.analyze_elf(data, profile=profile)
                self.assertTrue(report["valid"], report["errors"])
                self.assertEqual(report["initial_msp"], "0x200099C8")
                self.assertEqual(report["reset_vector"], "0x08002AC1")
                for stack in (0x200099C0, 0x20009A68, 0x2000C000):
                    with self.subTest(stack=stack):
                        modified = diagnostic_fixture()
                        struct.pack_into("<I", modified, 0x100, stack)
                        result = check_elf.analyze_elf(modified, profile=profile)
                        self.assertEqual(result["errors"][0]["code"], "stack_profile")

    def test_diagnostic_reset_and_ram_above_stack_are_rejected(self):
        data = diagnostic_fixture()
        for offset in (24, 0x104):
            struct.pack_into("<I", data, offset, 0x08002AC3)
        for profile in check_elf.DIAGNOSTIC_PROFILES:
            with self.subTest(profile=profile):
                report = check_elf.analyze_elf(data, profile=profile)
                self.assertEqual(report["errors"][0]["code"], "reset_profile")
                ram_data = diagnostic_fixture()
                struct.pack_into("<I", ram_data, 116 + 20, 0x99D0)
                report = check_elf.analyze_elf(ram_data, profile=profile)
                self.assertEqual(report["errors"][0]["code"], "static_ram_above_stack")

    def test_unknown_profile_rejected(self):
        report = check_elf.analyze_elf(fixture(), profile="arbitrary")
        self.assertEqual(report["errors"][0]["code"], "unknown_profile")

    def test_bootloader_physical_write_is_rejected_even_for_ram_data(self):
        data = fixture()
        struct.pack_into("<I", data, 116 + 12, 0x08002000)
        self.assert_invalid(data, "physical_range")

    def test_flash_end_overrun(self):
        data = fixture()
        struct.pack_into("<I", data, 116 + 12, 0x0803FFFE)
        self.assert_invalid(data, "physical_range")

    def test_ram_end_overrun(self):
        data = fixture()
        struct.pack_into("<I", data, 116 + 8, 0x2000BFFC)
        self.assert_invalid(data, "virtual_range")

    def test_wrong_stack_values(self):
        for stack, code in [(0x2000C008, "stack_range"), (0x2000BFFF, "stack_range"),
                            (0x20009A68, "stack_profile")]:
            with self.subTest(stack=stack):
                data = fixture()
                struct.pack_into("<I", data, 0x100, stack)
                self.assert_invalid(data, code)

    def test_even_reset_or_reset_into_nonexecuting_vectors(self):
        for reset, code in [(0x08002808, "reset_thumb"), (0x08002801, "reset_range")]:
            with self.subTest(reset=reset):
                data = fixture()
                struct.pack_into("<I", data, 0x104, reset)
                self.assert_invalid(data, code)

    def test_entry_must_match_reset(self):
        data = fixture()
        struct.pack_into("<I", data, 24, 0x0800280B)
        self.assert_invalid(data, "entry_mismatch")

    def test_vector_section_cannot_be_relocated(self):
        data = fixture()
        struct.pack_into("<I", data, 0x200 + 40 + 12, 0x08002804)
        self.assert_invalid(data, "section_mapping")

    def test_segment_file_size_exceeds_memory(self):
        data = fixture()
        struct.pack_into("<I", data, 52 + 16, 12)
        self.assert_invalid(data, "segment_size")

    def test_flash_zero_fill_cannot_overlap_ram_initialization_load(self):
        data = fixture()
        # .text's virtual zero-fill tail would occupy 0x08002810..14,
        # exactly where RAM .data has its physical initialization bytes.
        # Physical file-backed overlap checks alone do not catch this.
        struct.pack_into("<I", data, 84 + 20, 12)
        self.assert_invalid(data, "flash_zero_fill")

    def test_flash_zero_fill_without_file_bytes_is_rejected(self):
        data = fixture()
        struct.pack_into("<I", data, 84 + 16, 0)
        self.assert_invalid(data, "flash_zero_fill")

    def test_malformed_or_truncated_tables_and_segments(self):
        for cut in [0, 20, 52, 100, 0x200, 0x2C7]:
            with self.subTest(cut=cut):
                self.assert_invalid(fixture()[:cut], "truncated")
        data = fixture()
        struct.pack_into("<I", data, 52 + 4, 0x1000)
        self.assert_invalid(data, "truncated")

    def test_not_arm_elf32_little_endian(self):
        for offset, value, code in [(4, 2, "elf_format"), (5, 2, "elf_format"),
                                    (18, 62, "elf_target")]:
            with self.subTest(offset=offset):
                data = fixture()
                data[offset] = value
                self.assert_invalid(data, code)

    def test_cli_writes_report_without_absolute_input_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "test.elf"
            output = root / "reports" / "layout.json"
            source.write_bytes(fixture())
            with contextlib.redirect_stdout(io.StringIO()):
                status = check_elf.main([str(source), "--output", str(output)])
            self.assertEqual(status, 0)
            report = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(report["artifact"], "test.elf")
            self.assertNotIn(directory, output.read_text(encoding="utf-8"))
            self.assertEqual(len(report["sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
