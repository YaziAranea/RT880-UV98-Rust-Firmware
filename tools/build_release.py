"""Build all standard channel-TX images and publish the local folder after all checks pass."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory

from export_diagnostic_image import VERSION
from pack_ui_locales import LANGUAGES

ROOT = Path(__file__).resolve().parent.parent
TARGET = "thumbv7em-none-eabi"


def tool(name, *arguments, check=True):
    return subprocess.run([sys.executable, ROOT / "tools" / name, *arguments],
                          cwd=ROOT, check=check)


def build_locale(language, target_dir, stage):
    folder = stage / language
    folder.mkdir()
    subprocess.run(["cargo", "build", "--release", "--locked", "--offline",
                    "--target-dir", target_dir, "--target", TARGET, "-p", "uv98-diag-firmware",
                    "--features", f"channel-tx,locale-{language}"], cwd=ROOT, check=True)
    linked = target_dir / TARGET / "release/uv98-diag-firmware"
    report = folder / "flash-budget.json"
    check = tool("check_flash_budget.py", linked, "--language", language,
                 "--profile", "channel-tx", "--report", report, check=False)
    if not report.exists():
        check.check_returncode()
        raise ValueError(f"{language}: missing Flash budget report")
    budget = json.loads(report.read_text(encoding="utf-8"))
    if budget["status"] == "below_minimum" and check.returncode == 1:
        return budget
    check.check_returncode()
    if budget["status"] != "passed" or not budget["release_gate_applied"]:
        raise ValueError(f"{language}: release budget was not verified")
    image = folder / "image"
    tool("export_diagnostic_image.py", linked, image)
    layout = json.loads((image / "image-layout.json").read_text(encoding="utf-8"))
    if any(layout[key] != budget[key] for key in ("elf_sha256", "bin_sha256")):
        raise ValueError(f"{language}: budget and exported files differ")
    elf = image / f"UV98-diag-{VERSION}.elf"
    tool("check_diag_preinit.py", elf, "--report", image / "early-machine-code-check.json")
    tool("check_device_profile.py", elf, "--report", image / "device-profile-check.json")
    return budget


def build_release(destination):
    destination = destination.resolve()
    if destination.exists():
        raise FileExistsError(f"Output directory already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Each invocation controls its own linked ELF until all three exports finish.
    with TemporaryDirectory(prefix=".uv98-release-", dir=destination.parent) as temp:
        stage = Path(temp) / "images"
        stage.mkdir()
        tool("pack_ui_locales.py", "--check")
        budgets = {lang: build_locale(lang, Path(temp) / "target", stage) for lang in LANGUAGES}
        below = [lang.upper() for lang, report in budgets.items() if report["status"] != "passed"]
        if below:
            raise ValueError("Flash reserve below minimum for " + ", ".join(below))
        summary = {"profile": "channel-tx", "languages": budgets}
        (stage / "flash-budget.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
        stage.rename(destination)
    print(f"Checked RU/EN/DE images: {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_directory", type=Path, help="new directory for images and reports")
    try:
        build_release(parser.parse_args().output_directory)
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        sys.exit(str(error))
