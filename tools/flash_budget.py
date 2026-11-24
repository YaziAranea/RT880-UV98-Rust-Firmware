"""Measure Flash headroom from a validated ELF export."""

import json
from pathlib import Path

from check_elf import FLASH_START
from check_settings_journal import JOURNAL_START, last_flash_load_end
from pack_ui_locales import LANGUAGES

BASELINE = Path(__file__).with_name("flash-budget-baseline.json")
RELEASE_PROFILE = "channel-tx"
MINIMUM = 4096


def read_baseline(path):
    baseline = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(baseline, dict):
        raise ValueError("Invalid Flash budget baseline")
    release = baseline.get("release")
    minimum = baseline.get("minimum_remaining_flash_bytes")
    remaining = baseline.get("remaining_flash_bytes")
    if (not isinstance(release, str) or not release.strip()
            or baseline.get("profile") != RELEASE_PROFILE
            or type(minimum) is not int or minimum != MINIMUM
            or not isinstance(remaining, dict) or set(remaining) != set(LANGUAGES)
            or any(type(value) is not int or not MINIMUM <= value < JOURNAL_START - FLASH_START
                   for value in remaining.values())):
        raise ValueError("Invalid Flash budget baseline")
    return baseline


def measure(exported, language, profile, *, baseline_path=BASELINE):
    """Use the last file-backed PT_LOAD physical end, including the RAM init image."""
    end = last_flash_load_end(exported["layout"]["load_segments"])
    remaining = JOURNAL_START - end
    gate_applied = profile in (RELEASE_PROFILE, "voice-keyer")
    report = {
        "schema": 1,
        "profile": profile,
        "language": language,
        "elf_sha256": exported["elf_sha256"],
        "bin_sha256": exported["bin_sha256"],
        "journal_start": f"0x{JOURNAL_START:08X}",
        "last_flash_load_end_exclusive": f"0x{end:08X}",
        "remaining_flash_bytes": remaining,
        "release_gate_applied": gate_applied,
        "status": "measured",
    }
    if gate_applied:
        report.update(
            minimum_remaining_flash_bytes=MINIMUM,
            status="passed" if remaining >= MINIMUM else "below_minimum",
        )
        # Pre-locale packages have no verified catalog to compare against.
        if language is not None:
            baseline = read_baseline(baseline_path)
            baseline_remaining = baseline["remaining_flash_bytes"][language]
            report.update(
                baseline_release=baseline["release"],
                baseline_remaining_flash_bytes=baseline_remaining,
                flash_cost_vs_baseline_bytes=baseline_remaining - remaining,
            )
    return report
