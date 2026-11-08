"""Offline integration: linked-ARM emulator UART output -> host capture decoder.

No serial import/open, hardware access, image execution or firmware build here.
The input report comes from the separate linked-ELF/MMIO emulator audit.
"""
import argparse
import json
from pathlib import Path
import re

from capture_boot_log import decode_file, sha256, write_json_new


def decoded_words(directory):
    return [[item["code"], *item["details"]]
            for item in (json.loads(line) for line in (directory / "records.jsonl").read_text().splitlines())
            if item["kind"] == "record"]


def verify(report_path, output):
    report_bytes = report_path.read_bytes()
    audit = json.loads(report_bytes)
    if audit.get("passed") is not True or audit.get("physical_hardware_verified") is not False:
        raise ValueError("Expected successful synthetic linked-ARM/MMIO audit report")
    cases = [case for group in ("cases", "terminal_cases", "exception_cases") for case in audit[group]]
    if not 1 <= len(cases) <= 128:
        raise ValueError("Unexpected number of audit cases")
    for case in cases:
        if case.get("passed") is not True or not isinstance(case.get("wire_ascii"), str):
            raise ValueError("Case is not a successful audit with retained UART bytes")
    output.mkdir(parents=True, exist_ok=False)
    results = []
    for index, case in enumerate(cases):
        name = f"{index:02d}-" + re.sub(r"[^a-zA-Z0-9_-]", "_", case["name"])
        raw = case["wire_ascii"].encode("ascii")
        source = output / f"{name}.bin"
        source.write_bytes(raw)
        decoded = output / name
        result = decode_file(source, decoded)
        if (decoded / "rx.bin").read_bytes() != raw or source.read_bytes() != raw:
            raise AssertionError(f"{case['name']}: raw UART bytes changed")
        words = decoded_words(decoded)
        if words != case["frames"]:
            raise AssertionError(f"{case['name']}: host words differ from emulator frame evidence")
        results.append({"name": case["name"], "passed": True, "raw_bytes": len(raw),
                        "raw_sha256": sha256(raw), "raw_hex": raw.hex(),
                        "decoded_words": words, "decoder": result["decoder"]})

    successful = next(case for case in cases if [word[0] for word in case["frames"]] in ([1, 2, 3], [0x300, 0x301, 0x302, 1, 0x303, 0x306, 0x304, 0x305, 2, 3]))
    fault = next(case for case in cases if any(0xE1000001 <= word[0] <= 0xE1000007 for word in case["frames"]))
    exception = next(case for case in cases if any(word[0] == 0xE1FF0000 for word in case["frames"]))
    success_raw = successful["wire_ascii"].encode("ascii")
    # A partial record across simulated power loss, followed by a complete restart.
    combined = (b"\x06\xff" + success_raw + fault["wire_ascii"].encode("ascii")
                + exception["wire_ascii"].encode("ascii") + success_raw[:23] + success_raw)
    source = output / "combined-restarts.bin"
    source.write_bytes(combined)
    decoded = output / "combined-restarts"
    combined_result = decode_file(source, decoded)
    expected = successful["frames"] + fault["frames"] + exception["frames"] + successful["frames"]
    if decoded_words(decoded) != expected or (decoded / "rx.bin").read_bytes() != combined:
        raise AssertionError("Combined restart capture lost valid words or changed raw bytes")
    if combined_result["decoder"]["rejected_records"] != 1:
        raise AssertionError("Combined restart must retain one rejected partial record")
    summary = {"passed": True, "scope": "synthetic linked ARM instructions with mocked MMIO -> offline host decoder",
               "physical_hardware_verified": False, "device_access": False, "transmitted_bytes": 0,
               "source_report": str(report_path.resolve()), "source_report_sha256": sha256(report_bytes),
               "source_elf_sha256": audit["elf_sha256"], "verified_cases": len(cases), "cases": results,
               "combined_restarts": {"passed": True, "raw_hex": combined.hex(), "raw_sha256": sha256(combined),
                                     "expected_words": expected, "decoder": combined_result["decoder"]}}
    write_json_new(output / "integration.json", summary)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New output directory")
    args = parser.parse_args()
    result = verify(args.report, args.output)
    print(json.dumps({"passed": result["passed"], "verified_cases": result["verified_cases"],
                      "combined_restarts_passed": result["combined_restarts"]["passed"],
                      "physical_hardware_verified": False, "device_access": False}, indent=2))


if __name__ == "__main__":
    main()
