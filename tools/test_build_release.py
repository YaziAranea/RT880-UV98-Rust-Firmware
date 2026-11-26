"""A locale that exceeds the reserve must never leave a usable release folder."""
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import build_release


class ReleaseMatrixTests(unittest.TestCase):
    def run_matrix(self, destination, failing=None):
        def build(language, target_dir, stage, profile):
            folder = stage / language
            folder.mkdir()
            (folder / "test.bin").write_bytes(language.encode())
            return {"language": language, "status": "below_minimum" if language == failing else "passed"}

        with patch.object(build_release, "tool"), patch.object(build_release, "build_locale", side_effect=build) as calls:
            if failing:
                with self.assertRaisesRegex(ValueError, failing.upper()):
                    build_release.build_release(destination)
            else:
                build_release.build_release(destination)
            self.assertEqual([call.args[0] for call in calls.call_args_list], list(build_release.LANGUAGES))

    def test_any_low_language_hides_all_three_and_retry_succeeds(self):
        with TemporaryDirectory() as directory:
            destination = Path(directory) / "images"
            for failing in build_release.LANGUAGES:
                self.run_matrix(destination, failing)
                self.assertFalse(destination.exists())
            self.run_matrix(destination)
            summary = json.loads((destination / "flash-budget.json").read_text())
            self.assertEqual(set(summary["languages"]), set(build_release.LANGUAGES))
            with self.assertRaises(FileExistsError):
                build_release.build_release(destination)
