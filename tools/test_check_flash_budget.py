"""Exercise the Flash budget using synthetic linked ELF files."""

from contextlib import redirect_stderr, redirect_stdout
from hashlib import sha256
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest

import check_elf
import check_flash_budget as cli
import check_settings_journal as journal
from flash_budget import BASELINE, read_baseline
from pack_ui_locales import DIRECTORY
from package_diagnostic_image import CHANNEL_TX_POLICY, RX_CANDIDATE_POLICY, UPPER_EXO_TRIAL_POLICY
from test_package_diagnostic_image import policy_fixture


def linked_elf(remaining, *languages, policy=CHANNEL_TX_POLICY, writer=True):
    elf = bytearray(policy_fixture(policy))
    catalog = b"".join((DIRECTORY / f"messages-{language}.bin").read_bytes()
                       for language in languages)
    elf.extend(catalog)
    struct.pack_into("<H", elf, 44, 6)
    check_elf.PROGRAM_HEADER.pack_into(elf, 180, 1, 0x600, 0x08002AE0, 0x08002AE0,
                                      len(catalog), len(catalog), 4, 4)
    # This address has no file bytes and must not contribute to Flash use.
    check_elf.PROGRAM_HEADER.pack_into(elf, 212, 1, 0, 0x20000010, 0xFFFFFFFF,
                                      0, 4, 6, 4)
    data_vma = 0x20000000
    data_lma = journal.JOURNAL_START - remaining - 4
    struct.pack_into("<I", elf, 116 + 12, data_lma)
    struct.pack_into("<I", elf, 116 + 24, 7)
    struct.pack_into("<I", elf, 0x200 + 3 * 40 + 8, 7)
    elf[0x110:0x114] = b"\xff" * 4
    symbols = [(journal.SETTINGS_START, journal.JOURNAL_START, 0, 0x10, journal.SHN_ABS),
               (journal.SETTINGS_END, check_elf.FLASH_END, 0, 0x10, journal.SHN_ABS)]
    if writer:
        symbols += [(journal.COPY_START, data_vma, 0, 0x10, 3),
                    (journal.COPY_END, data_vma + 4, 0, 0x10, 3),
                    (journal.COPY_LMA, data_lma, 0, 0x10, journal.SHN_ABS),
                    (journal.OP_START, data_vma | 1, 4, 0x12, 3),
                    (journal.OP_END, data_vma + 4, 0, 0x10, 3)]
    names = bytearray(b"\0UV98_RELEASE_POLICY\0")
    for i, (name, value, size, info, section) in enumerate(symbols, start=2):
        journal.SYMBOL.pack_into(elf, 0x380 + i * journal.SYMBOL.size,
                                 len(names), value, size, info, 0, section)
        names.extend(name + b"\0")
    elf[0x500:0x500 + len(names)] = names
    struct.pack_into("<I", elf, 0x200 + 6 * 40 + 20, (2 + len(symbols)) * journal.SYMBOL.size)
    struct.pack_into("<II", elf, 0x200 + 7 * 40 + 16, 0x500, len(names))
    return elf


class FlashBudgetTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.elf = Path(directory.name, "candidate.elf")
        self.report = Path(directory.name, "flash-budget.json")

    def run_cli(self, *, language="ru", profile="channel-tx"):
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = cli.main([str(self.elf), "--language", language, "--profile", profile,
                               "--report", str(self.report)])
        return status, stdout.getvalue(), stderr.getvalue()

    def test_cli_gate_at_exact_floor_rejects_one_byte_less(self):
        for remaining, expected_code, expected_status, expected_summary in (
            (4097, 0, "passed", "PASS"), (4096, 0, "passed", "PASS"),
            (4095, 1, "below_minimum", "FAIL"),
        ):
            with self.subTest(remaining=remaining):
                elf = linked_elf(remaining, "ru")
                self.elf.write_bytes(elf)
                code, output, stderr = self.run_cli()
                report = json.loads(self.report.read_text())
                self.report.unlink()
                self.assertEqual(code, expected_code, stderr)
                self.assertIn(f"{remaining} bytes before settings journal", output)
                self.assertIn(expected_summary, output)
                self.assertEqual(report["status"], expected_status)
                self.assertEqual(report["elf_sha256"], sha256(elf).hexdigest())
                self.assertEqual(report["remaining_flash_bytes"], remaining)
                self.assertEqual(report["minimum_remaining_flash_bytes"], 4096)
                self.assertEqual(int(report["last_flash_load_end_exclusive"], 16),
                                 journal.JOURNAL_START - remaining)

    def test_cost_uses_selected_locale_and_sign(self):
        baseline = read_baseline(BASELINE)
        for language, cost in (("ru", -56), ("en", 0), ("de", 56)):
            previous = baseline["remaining_flash_bytes"][language]
            remaining = previous - cost
            with self.subTest(language=language):
                self.elf.write_bytes(linked_elf(remaining, language))
                code, output, stderr = self.run_cli(language=language)
                self.assertEqual(code, 0, stderr)
                self.assertIn(f"cost vs {baseline['release']} {cost:+d} bytes", output)
                report = json.loads(self.report.read_text())
                self.report.unlink()
                self.assertEqual(report["baseline_remaining_flash_bytes"], previous)
                self.assertEqual(report["flash_cost_vs_baseline_bytes"], cost)

    def test_diagnostic_profiles_report_measurement_without_release_gate_or_cost(self):
        for profile, policy in (("rx", RX_CANDIDATE_POLICY), ("pa5-audio", RX_CANDIDATE_POLICY),
                                ("upper-exo-trial", UPPER_EXO_TRIAL_POLICY),
                                ("channel-tx-dev", CHANNEL_TX_POLICY)):
            with self.subTest(profile=profile):
                self.elf.write_bytes(linked_elf(4095, "de", policy=policy, writer=profile != "pa5-audio"))
                code, output, stderr = self.run_cli(language="de", profile=profile)
                self.assertEqual(code, 0, stderr)
                self.assertIn("diagnostic measurement only", output)
                report = json.loads(self.report.read_text())
                self.report.unlink()
                self.assertEqual(report["remaining_flash_bytes"], 4095)
                self.assertEqual(report["status"], "measured")
                self.assertFalse(report["release_gate_applied"])
                self.assertNotIn("baseline_release", report)
                self.assertNotIn("flash_cost_vs_baseline_bytes", report)

    def test_missing_wrong_and_diagnostic_elf_cannot_pass_as_release(self):
        code, _, stderr = self.run_cli()
        self.assertEqual(code, 1)
        self.assertIn("Flash budget check failed", stderr)
        self.assertFalse(self.report.exists())
        for elf, message in (
            (b"invalid", "ELF header is truncated"),
            (linked_elf(5000, "de"), "only the ru UI catalog"),
            (linked_elf(5000, "ru", "de"), "only the ru UI catalog"),
            (linked_elf(5000, "ru", policy=RX_CANDIDATE_POLICY), "linked release policy differs"),
            (linked_elf(-1, "ru"), "file-backed Flash load enters the journal"),
        ):
            with self.subTest(message=message):
                self.elf.write_bytes(elf)
                code, _, stderr = self.run_cli()
                self.assertEqual(code, 1)
                self.assertIn(message, stderr)
                self.assertFalse(self.report.exists())

    def test_existing_report_is_immutable_and_cannot_return_success(self):
        self.elf.write_bytes(linked_elf(4096, "ru"))
        self.assertEqual(self.run_cli()[0], 0)
        prior = self.report.read_bytes()
        self.elf.write_bytes(linked_elf(4095, "ru"))
        self.assertEqual(self.run_cli()[0], 1)
        self.assertEqual(self.report.read_bytes(), prior)

    def test_baseline_requires_all_locales_standard_profile_and_fixed_floor(self):
        baseline = read_baseline(BASELINE)
        with self.assertRaises(FileNotFoundError):
            read_baseline(self.report)
        for mutation in ({"release": " "}, {"profile": "rx"}, {"minimum_remaining_flash_bytes": 0},
                         {"remaining_flash_bytes": {"ru": 7644}}, {"remaining_flash_bytes": None}):
            with self.subTest(mutation=mutation):
                self.report.write_text(json.dumps(baseline | mutation))
                with self.assertRaisesRegex(ValueError, "Invalid Flash budget baseline"):
                    read_baseline(self.report)


if __name__ == "__main__":
    unittest.main()
