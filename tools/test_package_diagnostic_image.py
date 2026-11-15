"""Offline packaging binds images, reports and mutually exclusive TX profiles."""

import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

import check_elf
from export_diagnostic_image import VERSION, export
import package_diagnostic_image as packaging
from test_check_elf import diagnostic_fixture


def policy_fixture(words=packaging.RX_CANDIDATE_POLICY):
    data = diagnostic_fixture()
    data.extend(bytes(0x600 - len(data)))
    names = b"\0.vector_table\0.text\0.data\0.shstrtab\0.release_policy\0.symtab\0.strtab\0"
    data[0x150:0x150 + len(names)] = names
    struct.pack_into("<H", data, 44, 4)  # phnum
    struct.pack_into("<H", data, 48, 8)  # shnum
    check_elf.PROGRAM_HEADER.pack_into(data, 148, 1, 0x120, 0x08002ACC,
                                     0x08002ACC, 16, 16, 4, 4)
    struct.pack_into("<4I", data, 0x120, *words)
    check_elf.SECTION_HEADER.pack_into(data, 0x200 + 4 * 40,
                                     27, 3, 0, 0, 0x150, len(names), 0, 0, 1, 0)
    symbols = b"\0" + packaging.RELEASE_SYMBOL + b"\0"
    data[0x3C0:0x3C0 + len(symbols)] = symbols
    struct.pack_into("<IIIBBH", data, 0x390, 1, 0x08002ACC, 16, 0x11, 0, 5)
    for index, fields in enumerate([
        (names.index(b".release_policy"), 1, 2, 0x08002ACC, 0x120, 16, 0, 0, 4, 0),
        (names.index(b".symtab"), 2, 0, 0, 0x380, 32, 7, 1, 4, 16),
        (names.index(b".strtab"), 3, 0, 0, 0x3C0, len(symbols), 0, 0, 1, 0),
    ], start=5):
        check_elf.SECTION_HEADER.pack_into(data, 0x200 + index * 40, *fields)
    return bytes(data)


def json_write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value) + "\n", encoding="utf-8")


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "source"
        self.folder = Path(self.temp.name) / "build"
        self.source = self.root / "firmware-diagnostic/src/main.rs"
        self.source.parent.mkdir(parents=True)
        self.source.write_text("const LIVE_TX: bool = false;\nconst LOWER_PTT_CODE: Option<u8> = None;\n")
        self.manifest = self.root / "firmware-diagnostic/Cargo.toml"
        self.manifest.write_text('[features]\ndefault = []\nupper-exo-tx-trial = []\nchannel-tx = []\n')
        (self.root / "DIAGNOSTIC-IMAGE-RU.txt").write_text(f"UV98 Rust {VERSION} RX candidate\n")
        (self.root / "RECOVERY-PREPARATION-RU.txt").write_text("fixture only\n")
        capture = self.root / "tools/boot-log"
        capture.mkdir(parents=True)
        for name in ("capture_boot_log.py", "test_capture_boot_log.py", "verify_boot_log_emulation.py", "BOOT-LOG-CAPTURE-RU.txt"):
            (capture / name).write_text("fixture only\n")
        self.calibration = self.root / "crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        self.calibration.parent.mkdir(parents=True)
        self.calibration.write_bytes(bytes(range(256)) * 16)
        json_write(self.folder / "verification.json", {
            "status": "passed", "device_accessed": False, "flash_performed": False,
            "hardware_qualified": False, "preserved_inputs_unchanged": True,
            "preserved_calibration_sha256": packaging.digest(self.calibration)})
        self.build(policy_fixture())

    def build(self, elf):
        image, report = export(elf)
        directory = self.folder / "image"
        directory.mkdir(exist_ok=True)
        (directory / f"UV98-diag-{VERSION}.elf").write_bytes(elf)
        (directory / f"UV98-diag-{VERSION}.bin").write_bytes(image)
        json_write(directory / "image-layout.json", report)
        json_write(directory / "early-machine-code-check.json", {"passed": True, "elf_sha256": report["elf_sha256"]})
        json_write(self.folder / "boot-log-emulation-decode/integration.json", {
            "passed": True, "source_elf_sha256": report["elf_sha256"],
            "source_report_sha256": packaging.digest(directory / "early-machine-code-check.json")})

    def mutate_json(self, relative, **changes):
        path = self.folder / relative
        value = json.loads(path.read_text())
        value.update(changes)
        json_write(path, value)

    def test_policy_uses_allocated_symbol_not_any_matching_bytes(self):
        runtime = packaging.validate_inputs(self.folder, self.root)
        self.assertEqual(runtime["linked_release_policy"]["address"], "0x08002ACC")
        self.assertFalse(runtime["live_tx_enabled"])
        self.assertEqual(runtime["physical_sidekey_mapping"], "pending_live_adc_observation")
        self.assertFalse(runtime["live_test_performed"])
        elf = bytearray(policy_fixture())
        # Correct declaration bytes remain in the ELF/BIN; object name is gone.
        elf[0x3C1] = ord("X")
        self.build(bytes(elf))
        with self.assertRaisesRegex(ValueError, "exactly one linked"):
            packaging.validate_inputs(self.folder, self.root)

    def test_rejects_tx_enabled_or_assigned_ptt_or_wrong_version_in_linked_marker(self):
        for words in ((0x108, 1, 0xFFFFFFFF, 0x52584331),
                      (0x108, 0, 0x16, 0x52584331),
                      (0x107, 0, 0xFFFFFFFF, 0x52584331)):
            with self.subTest(words=words):
                self.build(policy_fixture(words))
                with self.assertRaisesRegex(ValueError, "RX candidate"):
                    packaging.validate_inputs(self.folder, self.root)

    def test_rejects_nonobject_or_wrong_size_or_unallocated_policy(self):
        for offset, fmt, value in ((0x390 + 12, "<B", 0x12), (0x390 + 8, "<I", 12),
                                   (0x200 + 5 * 40 + 8, "<I", 0)):
            with self.subTest(offset=offset):
                elf = bytearray(policy_fixture())
                struct.pack_into(fmt, elf, offset, value)
                self.build(bytes(elf))
                with self.assertRaisesRegex(ValueError, "Release policy"):
                    packaging.validate_inputs(self.folder, self.root)

    def test_image_and_reports_must_belong_to_exact_export(self):
        for mutation in ("bin", "layout", "audit", "decoder"):
            with self.subTest(mutation=mutation):
                self.build(policy_fixture())
                if mutation == "bin":
                    binary = self.folder / f"image/UV98-diag-{VERSION}.bin"
                    data = bytearray(binary.read_bytes()); data[-1] = 0
                    binary.write_bytes(data)
                elif mutation == "layout":
                    self.mutate_json("image/image-layout.json", bytes_above_static_ram=12345)
                elif mutation == "audit":
                    self.mutate_json("image/early-machine-code-check.json", elf_sha256="0" * 64)
                else:
                    self.mutate_json("boot-log-emulation-decode/integration.json", source_report_sha256="0" * 64)
                with self.assertRaises(ValueError):
                    packaging.validate_inputs(self.folder, self.root)

    def test_source_and_readme_cannot_claim_other_candidate(self):
        original = self.source.read_text()
        for source in (original.replace("false", "true"), original.replace("None", "Some(0x16)")):
            self.source.write_text(source)
            with self.assertRaises(ValueError):
                packaging.validate_inputs(self.folder, self.root)
        self.source.write_text(original)
        (self.root / "DIAGNOSTIC-IMAGE-RU.txt").write_text("UV98 Rust 0.1.7\n")
        with self.assertRaisesRegex(ValueError, "README version"):
            packaging.validate_inputs(self.folder, self.root)

    def test_verification_must_be_offline_with_unchanged_preserved_inputs(self):
        original = (self.folder / "verification.json").read_text()
        for key in ("device_accessed", "flash_performed", "hardware_qualified", "preserved_inputs_unchanged"):
            (self.folder / "verification.json").write_text(original)
            self.mutate_json("verification.json", **{key: key != "preserved_inputs_unchanged"})
            with self.assertRaises(ValueError):
                packaging.validate_inputs(self.folder, self.root)
        (self.folder / "verification.json").write_text(original)
        self.calibration.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "calibration fixture"):
            packaging.validate_inputs(self.folder, self.root)

    def test_package_manifest_zip_readback_and_no_overwrite(self):
        result = packaging.package(self.folder, self.root)
        manifest = json.loads((self.folder / "manifest.json").read_text())
        self.assertEqual(manifest["version"], "0.1.8")
        self.assertFalse(manifest["hardware_qualified"])
        self.assertFalse(manifest["runtime"]["live_tx_enabled"])
        self.assertEqual(len(manifest["runtime"]["compiled_features"]), 6)
        archive_path = Path(result["zip"])
        original_hash = packaging.digest(archive_path)
        with zipfile.ZipFile(archive_path) as archive:
            for entry in manifest["files"]:
                self.assertEqual(hashlib.sha256(archive.read(entry["path"])).hexdigest(), entry["sha256"])
        with self.assertRaises(FileExistsError):
            packaging.package(self.folder, self.root)
        self.assertEqual(packaging.digest(archive_path), original_hash)

    def test_rejected_package_creates_no_outputs(self):
        self.build(policy_fixture((0x108, 1, 0xFFFFFFFF, 0x52584331)))
        with self.assertRaises(ValueError):
            packaging.package(self.folder, self.root)
        for relative in ("manifest.json", "source.zip", "README-RU.txt", "capture"):
            self.assertFalse((self.folder / relative).exists())
        self.assertFalse(Path(str(self.folder) + ".zip").exists())

    def trial_source(self):
        self.source.write_text(
            'const LIVE_TX: bool = cfg!(feature = "upper-exo-tx-trial");\n'
            'const LOWER_PTT_CODE: Option<u8> = None;\n'
            'fn fixture_only() { tx_trial::permits(); tx_trial::record_permitted(); tx_trial::hold_within_limit(); }\n')

    def test_tx_trial_needs_explicit_profile_feature_evidence_and_linked_tag(self):
        self.trial_source()
        self.build(policy_fixture(packaging.UPPER_EXO_TRIAL_POLICY))
        with self.assertRaisesRegex(ValueError, "feature evidence"):
            packaging.validate_inputs(self.folder, self.root, "upper-exo-trial")
        self.mutate_json("verification.json", firmware_features=[packaging.TRIAL_FEATURE])
        trial = packaging.validate_inputs(self.folder, self.root, "upper-exo-trial")
        self.assertTrue(trial["live_tx_enabled"])
        self.assertEqual(trial["trial_restrictions"]["max_hold_ms"], 5000)
        with self.assertRaises(ValueError):
            packaging.validate_inputs(self.folder, self.root)  # Default never packages TX.
        self.build(policy_fixture())
        with self.assertRaisesRegex(ValueError, "upper EXO trial"):
            packaging.validate_inputs(self.folder, self.root, "upper-exo-trial")

    def test_missing_trial_guards_or_plain_true_source_cannot_be_packaged(self):
        self.build(policy_fixture(packaging.UPPER_EXO_TRIAL_POLICY))
        self.mutate_json("verification.json", firmware_features=[packaging.TRIAL_FEATURE])
        self.trial_source()
        original = self.source.read_text()
        for source in [original.replace('tx_trial::permits(', 'missing('),
                       original.replace('tx_trial::record_permitted(', 'missing('),
                       original.replace('tx_trial::hold_within_limit(', 'missing('),
                       original.replace('cfg!(feature = "upper-exo-tx-trial")', 'true')]:
            self.source.write_text(source)
            with self.assertRaises(ValueError):
                packaging.validate_inputs(self.folder, self.root, "upper-exo-trial")

    def test_feature_gated_source_still_packages_the_rx_default(self):
        self.trial_source()
        result = packaging.validate_inputs(self.folder, self.root)
        self.assertFalse(result["live_tx_enabled"])
        self.assertIsNone(result["trial_restrictions"])
        with self.assertRaisesRegex(ValueError, "Unknown candidate"):
            packaging.validate_inputs(self.folder, self.root, "unrestricted-tx")

    def channel_source(self):
        self.source.write_text(
            '#[cfg(all(\n feature = "upper-exo-tx-trial",\n feature = "channel-tx",\n))]\n'
            'compile_error!("Select exactly one TX profile");\n'
            'const LIVE_TX: bool = cfg!(any(\n feature = "upper-exo-tx-trial",\n'
            ' feature = "channel-tx",\n));\n'
            'const LOWER_PTT_ENABLED: bool = cfg!(feature = "channel-tx");\n'
            'fn fixture_only() { ptt::lower_pressed(); ptt::side_idle(); tx_policy::authorize_target(); tx_policy::hold_within_limit(); '
            'auth.still_held_target(); tx_trial::permits(); tx_trial::record_permitted(); '
            'tx_trial::hold_within_limit(); }\n')

    def prepare_channel(self):
        self.channel_source()
        self.build(policy_fixture(packaging.CHANNEL_TX_POLICY))
        self.mutate_json("verification.json", firmware_features=[packaging.CHANNEL_TX_FEATURE])

    def test_channel_profile_has_distinct_linked_tag_and_no_historical_trial_allowlist(self):
        self.prepare_channel()
        runtime = packaging.validate_inputs(self.folder, self.root, "channel-tx")
        self.assertTrue(runtime["live_tx_enabled"])
        self.assertIsNone(runtime["trial_restrictions"])
        self.assertEqual(runtime["linked_release_policy"]["words"],
                         ["0x00000108", "0x00000001", "0x00000016", "0x464D5431"])
        self.assertEqual(runtime["channel_tx_constraints"]["max_hold_ms"], 30000)
        self.assertEqual(runtime["channel_tx_constraints"]["power_codes"], [0, 1, 2])
        self.assertEqual(runtime["compiled_features"][-1], "FM_TX_CHANNEL_VFO")
        self.assertTrue(runtime["channel_tx_constraints"]["lower_physical_ptt_mapped"])
        self.assertFalse(runtime["hardware_qualified"])
        self.assertFalse(runtime["live_test_performed"])
        self.assertIn("not full TX reachability proof", runtime["validation_scope"])
        self.assertNotIn("CH002", runtime["candidate_scope"])

    def test_each_profile_rejects_other_compiled_tags_and_feature_sets(self):
        self.channel_source()
        for profile, (words, features, _) in packaging.PROFILES.items():
            with self.subTest(profile=profile):
                self.build(policy_fixture(words))
                self.mutate_json("verification.json", firmware_features=features)
                packaging.validate_inputs(self.folder, self.root, profile)
                for wrong in [[], [packaging.TRIAL_FEATURE], [packaging.CHANNEL_TX_FEATURE],
                              [packaging.TRIAL_FEATURE, packaging.CHANNEL_TX_FEATURE],
                              [packaging.CHANNEL_TX_FEATURE, packaging.CHANNEL_TX_FEATURE]]:
                    if wrong == features:
                        continue
                    self.mutate_json("verification.json", firmware_features=wrong)
                    with self.assertRaisesRegex(ValueError, "feature evidence"):
                        packaging.validate_inputs(self.folder, self.root, profile)
                self.mutate_json("verification.json", firmware_features=features)
                for other_words, _, _ in packaging.PROFILES.values():
                    if other_words == words:
                        continue
                    self.build(policy_fixture(other_words))
                    with self.assertRaisesRegex(ValueError, "linked release policy differs"):
                        packaging.validate_inputs(self.folder, self.root, profile)

    def test_channel_source_requires_live_feature_gate_mutual_exclusion_and_real_guard_tokens(self):
        self.prepare_channel()
        original = self.source.read_text()
        gate = 'cfg!(any(\n feature = "upper-exo-tx-trial",\n feature = "channel-tx",\n))'
        sources = [original.replace(gate, "true"),
                   original.replace(gate, gate + " || true"),
                   original.replace('compile_error!("Select exactly one TX profile");', ""),
                   original + "\nconst LIVE_TX: bool = true;\n"]
        for guard in ("tx_policy::authorize_target()", "tx_policy::hold_within_limit()", "auth.still_held_target()", "ptt::lower_pressed()", "ptt::side_idle()"):
            sources += [original.replace(guard, "missing()"),
                        original.replace(guard, "/* " + guard + " */"),
                        original.replace(guard, '"' + guard + '"')]
        for source in sources:
            with self.subTest(source=source):
                self.source.write_text(source)
                with self.assertRaises(ValueError):
                    packaging.validate_inputs(self.folder, self.root, "channel-tx")

    def test_tx_feature_declarations_cannot_enable_defaults_aliases_or_dependencies(self):
        self.prepare_channel()
        original = self.manifest.read_text()
        for invalid in (original.replace('default = []', 'default = ["channel-tx"]'),
                        original.replace('channel-tx = []', 'channel-tx = ["upper-exo-tx-trial"]'),
                        original.replace('channel-tx = []\n', ''),
                        original + 'tx-alias = ["channel-tx"]\n'):
            with self.subTest(manifest=invalid):
                self.manifest.write_text(invalid)
                with self.assertRaisesRegex(ValueError, "feature declarations"):
                    packaging.validate_inputs(self.folder, self.root, "channel-tx")

    def test_disabled_verbose_faults_does_not_change_tx_policy(self):
        self.prepare_channel()
        original = self.manifest.read_text()
        self.manifest.write_text(original + 'verbose-faults = []\n')
        packaging.validate_inputs(self.folder, self.root, "channel-tx")
        self.manifest.write_text(original + 'verbose-faults = ["channel-tx"]\n')
        with self.assertRaisesRegex(ValueError, "feature declarations"):
            packaging.validate_inputs(self.folder, self.root, "channel-tx")

    def test_channel_package_retains_no_overwrite_and_private_calibration_exclusion(self):
        self.prepare_channel()
        elf = bytearray(policy_fixture(packaging.CHANNEL_TX_POLICY))
        private = self.calibration.read_bytes()
        elf.extend(private)
        struct.pack_into("<H", elf, 44, 5)
        struct.pack_into("<H", elf, 48, 9)
        check_elf.PROGRAM_HEADER.pack_into(elf, 180, 1, 0x600, 0x08003000,
                                         0x08003000, len(private), len(private), 4, 4)
        check_elf.SECTION_HEADER.pack_into(elf, 0x200 + 8 * 40,
                                         15, 1, 2, 0x08003000, 0x600, len(private), 0, 0, 4, 0)
        self.build(bytes(elf))
        with self.assertRaisesRegex(ValueError, "private calibration block"):
            packaging.package(self.folder, self.root, "channel-tx")
        self.assertFalse((self.folder / "source.zip").exists())
        self.assertEqual(self.calibration.read_bytes(), private)
        self.build(policy_fixture(packaging.CHANNEL_TX_POLICY))
        result = packaging.package(self.folder, self.root, "channel-tx")
        archive = Path(result["zip"])
        before = packaging.digest(archive)
        with self.assertRaises(FileExistsError):
            packaging.package(self.folder, self.root, "channel-tx")
        self.assertEqual(packaging.digest(archive), before)
        self.assertEqual(self.calibration.read_bytes(), private)


if __name__ == "__main__":
    unittest.main()
