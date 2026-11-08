"""Verify that export never changes placement or leaks a loader/RAM load."""

import struct
import unittest

import check_elf
from export_diagnostic_image import export
from test_check_elf import diagnostic_fixture as fixture


class DiagnosticExportTests(unittest.TestCase):
    def test_load_data_and_tail_are_explicit_and_input_unchanged(self):
        original = bytes(fixture())
        image, report = export(original)
        self.assertEqual(original, bytes(fixture()))
        self.assertEqual(len(image), 251904)
        self.assertEqual(image[:8], original[0x100:0x108])
        self.assertEqual(image[8:0x2C0], b"\xff" * (0x2C0 - 8))
        self.assertEqual(image[0x2C0:0x2C8], original[0x108:0x110])
        self.assertEqual(image[0x2C8:0x2CC], b"DATA")
        self.assertEqual(image[0x2CC:], b"\xff" * (len(image) - 0x2CC))
        self.assertEqual(report["file_backed_load_bytes"], 20)
        self.assertFalse(report["hardware_qualified"])
        self.assertFalse(report["bootloader_prefix_included"])
        self.assertEqual(report["bytes_above_static_ram"], 0x99C8 - 8)
        self.assertEqual(report["stack_margin_boundary"], "initial_msp")
        self.assertEqual(report["layout"]["layout_profile"], "diagnostic-0.1.8")

    def test_ram_data_cannot_be_mapped_to_loader(self):
        data = fixture()
        struct.pack_into("<I", data, 116 + 12, 0x08002000)
        with self.assertRaises(check_elf.LayoutError):
            export(data)

    def test_rejects_static_ram_leaving_no_stack_margin(self):
        data = fixture()
        # This left >=16 KiB when incorrectly calculated from RAM_END.
        struct.pack_into("<I", data, 116 + 20, 0x6000)
        with self.assertRaisesRegex(ValueError, "16 KiB"):
            export(data)

    def test_rejects_overlapping_physical_loads(self):
        data = fixture()
        struct.pack_into("<I", data, 116 + 12, 0x08002AC0)
        with self.assertRaises(check_elf.LayoutError):
            export(data)


if __name__ == "__main__":
    unittest.main()
