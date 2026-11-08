"""Synthetic fixtures only: no device, EXE, or user's firmware is executed."""
import hashlib
import struct
import unittest
from image_preflight import (
    APP_BASE, APP_END, APP_BYTES, RAM_END, Image, PreflightError, Segment,
    inspect, parse_bin, parse_hex,
)


def binary(size=1024, sp=RAM_END, reset=APP_BASE + 0x21):
    result = bytearray(size)
    result[:8] = struct.pack("<II", sp, reset)
    return bytes(result)


def record(kind, offset=0, data=b""):
    fields = bytes([len(data), offset >> 8, offset & 255, kind]) + data
    return ":" + (fields + bytes([-sum(fields) & 255])).hex().upper()


def ihex(data=binary(64), *, start=None):
    lines = [record(4, data=b"\x08\x00"), record(0, 0x2800, data)]
    if start is not None:
        lines.append(record(5, data=start.to_bytes(4, "big")))
    lines.append(record(1))
    return ("\n".join(lines) + "\n").encode("ascii")


class PreflightTests(unittest.TestCase):
    def test_binary_preserves_bytes_and_does_not_fill_a_short_image(self):
        source = binary(73)
        image = parse_bin(source)
        report = inspect(image)
        self.assertEqual(image.segments, (Segment(APP_BASE, source),))
        self.assertEqual(report["source_sha256"], hashlib.sha256(source).hexdigest())
        self.assertEqual(report["missing_bytes"], APP_BYTES - 73)
        self.assertFalse(report["span_is_1024_byte_aligned"])
        self.assertFalse(report["padding_applied"])
        self.assertFalse(report["image_output_created"])
        self.assertFalse(report["hardware_qualified"])
        self.assertFalse(report["recovery_verified"])

    def test_full_window_is_246_blocks_and_end_is_exclusive(self):
        report = inspect(parse_bin(binary(APP_BYTES)))
        self.assertEqual(report["blocks_covering_span"], 246)
        self.assertEqual(report["missing_intervals"], [])
        self.assertTrue(report["source_covers_full_application_window"])
        self.assertEqual(report["application_end_exclusive"], "0x08040000")

    def test_oversize_truncated_and_container_inputs_rejected(self):
        for data in [b"", b"1234567", binary(APP_BYTES + 1), b"MZ" + bytes(100), b"\x7fELF" + bytes(100)]:
            with self.subTest(size=len(data)), self.assertRaises(PreflightError):
                parse_bin(data)

    def test_invalid_stack_thumb_and_reset_targets_rejected(self):
        for sp, reset in [(0x20000000, APP_BASE + 0x21), (RAM_END + 8, APP_BASE + 0x21),
                          (RAM_END - 4, APP_BASE + 0x21), (RAM_END, APP_BASE + 0x20),
                          (RAM_END, 0x08000001), (RAM_END, APP_END + 1),
                          (RAM_END, APP_BASE + 1), (RAM_END, APP_BASE + 1025)]:
            with self.subTest(sp=sp, reset=reset), self.assertRaises(PreflightError):
                inspect(parse_bin(binary(sp=sp, reset=reset)))

    def test_equivalent_bin_and_hex_have_identical_segments(self):
        source = binary(64)
        self.assertEqual(parse_bin(source).segments, parse_hex(ihex(source)).segments)
        self.assertEqual(inspect(parse_hex(ihex(source)))["reset_vector"], "0x08002821")

    def test_hex_checksum_length_and_nonhex_syntax_rejected(self):
        good = ihex()
        cases = [good.replace(b"F2", b"F3", 1), good.replace(b":402800", b":3F2800", 1),
                 good.replace(b"F2", b"G2", 1), good.replace(b"F2", b"F 2", 1),
                 b"\xef\xbb\xbf" + good, good.replace(b"F2", b"F", 1)]
        for case in cases:
            with self.subTest(prefix=case[:25]), self.assertRaises(PreflightError):
                parse_hex(case)

    def test_hex_requires_single_final_eof(self):
        good = ihex()
        for case in [good.replace(b":00000001FF\n", b""),
                     good + (record(1) + "\n").encode(),
                     good + (record(0, 0x2900, b"x") + "\n").encode()]:
            with self.assertRaises(PreflightError):
                parse_hex(case)

    def test_no_silent_skipping_of_bootloader_system_or_outside_data(self):
        for high, low in [(0x0800, 0), (0x0804, 0), (0x1FFF, 0xF800)]:
            source = (record(4, data=high.to_bytes(2, "big")) + "\n" +
                      record(0, low, b"a") + "\n" + record(1)).encode()
            with self.subTest(high=high), self.assertRaises(PreflightError):
                parse_hex(source)

    def test_overlap_rejected_even_for_identical_bytes(self):
        good = ihex().decode().splitlines()
        for duplicate in [record(0, 0x2800, binary(64)), record(0, 0x2810, b"\x00")]:
            with self.assertRaises(PreflightError):
                parse_hex(("\n".join(good[:-1] + [duplicate, good[-1]])).encode())

    def test_sparse_hex_retains_holes_and_rejects_a_reset_into_a_hole(self):
        source = (record(4, data=b"\x08\x00") + "\n" +
                  record(0, 0x2800, binary(64)[:8]) + "\n" +
                  record(0, 0x2820, b"\x00\xBF") + "\n" + record(1)).encode()
        image = parse_hex(source)
        self.assertEqual([len(s.data) for s in image.segments], [8, 2])
        self.assertEqual(inspect(image)["missing_bytes"], APP_BYTES - 10)
        self.assertEqual(inspect(image)["missing_intervals"][0]["bytes"], 24)
        broken = source.replace(record(0, 0x2820, b"\x00\xBF").encode(),
                                record(0, 0x2822, b"\x00\xBF").encode())
        with self.assertRaises(PreflightError):
            inspect(parse_hex(broken))

    def test_type05_is_preserved_as_metadata_not_used_as_reset_vector(self):
        report = inspect(parse_hex(ihex(start=APP_BASE + 0x31)))
        self.assertEqual(report["reset_vector"], "0x08002821")
        self.assertEqual(report["start_linear_address"], "0x08002831")
        self.assertFalse(report["start_linear_equals_reset_vector"])
        with self.assertRaises(PreflightError):
            inspect(parse_hex(ihex(start=0x08000001)))

    def test_unsupported_and_malformed_records_fail_closed(self):
        for bad in [record(2, data=b"\x00\x00"), record(3, data=bytes(4)),
                    record(4, offset=1, data=b"\x08\x00"),
                    record(5, data=b"\x08\x00"), record(1, offset=1),
                    record(0, 0x2800), record(0, 0xFFFF, b"\x00\x00")]:
            source = (record(4, data=b"\x08\x00") + "\n" + bad + "\n" + record(1)).encode()
            with self.subTest(record=bad), self.assertRaises(PreflightError):
                parse_hex(source)

    def test_manual_image_cannot_bypass_address_and_order_checks(self):
        for segments in [(), (Segment(APP_BASE - 1, binary()),),
                         (Segment(APP_BASE, binary()), Segment(APP_BASE + 2, b"x")),
                         (Segment(APP_BASE, binary()), Segment(APP_END, b"x"))]:
            with self.assertRaises(PreflightError):
                inspect(Image("bin", "test", segments))


if __name__ == "__main__":
    unittest.main()
