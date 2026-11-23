#!/usr/bin/env python3
"""Report linked Flash headroom and enforce the standard channel-tx release floor."""

import argparse
import json
from pathlib import Path
import sys

from check_settings_journal import verify
from export_diagnostic_image import export
from flash_budget import MINIMUM, RELEASE_PROFILE, measure
from pack_ui_locales import LANGUAGES, verify_image
from package_diagnostic_image import PROFILES, release_policy

DEV_PROFILE = "channel-tx-dev"


def check_budget(elf, language, profile):
    binary, exported = export(elf)
    verify_image(binary, language)
    linked_profile = RELEASE_PROFILE if profile == DEV_PROFILE else profile
    policy = release_policy(elf, exported["layout"], PROFILES[linked_profile][0])
    verify(elf, binary, diagnostic=profile == "pa5-audio")
    report = measure(exported, language, profile)
    report["linked_release_policy"] = policy
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--language", choices=LANGUAGES, required=True)
    parser.add_argument("--profile", choices=(*PROFILES, DEV_PROFILE), required=True,
                        help="Use channel-tx-dev for channel-tx builds with extra development features")
    parser.add_argument("--report", type=Path, required=True, help="path for a new JSON report")
    args = parser.parse_args(argv)
    try:
        report = check_budget(args.elf.read_bytes(), args.language, args.profile)
        with args.report.open("x", encoding="utf-8") as stream:
            json.dump(report, stream, indent=2)
            stream.write("\n")
    except (OSError, ValueError) as error:
        print(f"Flash budget check failed: {error}", file=sys.stderr)
        return 1

    summary = (f"Flash budget {args.language.upper()} ({args.profile}): "
               f"{report['remaining_flash_bytes']} bytes before settings journal")
    if not report["release_gate_applied"]:
        print(f"{summary}, diagnostic measurement only")
        return 0
    cost = report["flash_cost_vs_baseline_bytes"]
    result = "PASS" if report["status"] == "passed" else "FAIL"
    print(f"{summary}, cost vs {report['baseline_release']} {cost:+d} bytes, "
          f"minimum {MINIMUM} bytes: {result}")
    return 0 if result == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
