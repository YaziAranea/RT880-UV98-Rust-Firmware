"""Regenerate bounded offline key oracles; never opens a device.

Requires the pinned, separately obtained reference image and Unicorn 2.1.4.
The stock helper maps only simulated flash/RAM; peripheral access is absent.
GPIO reads in the selected row sampler are replaced by documented test inputs.
No Rust, serial, boot flow, or host firmware utility is executed.
"""
from pathlib import Path
import argparse
import csv
import hashlib
import json
import os
import struct
import sys

HERE = Path(__file__).resolve().parent
PROJECT = Path(os.environ.get("UV98_RUST_PROJECT", HERE.parents[1]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", default=os.environ.get("UV98_STOCK_REFERENCE"))
    parser.add_argument("--deps", default=os.environ.get("UV98_RESEARCH_DEPS"))
    args = parser.parse_args()
    if not args.reference:
        parser.error("--reference or UV98_STOCK_REFERENCE is required")
    if args.deps:
        sys.path.insert(0, str(Path(args.deps).resolve()))
    sys.path.insert(0, str(HERE.parent))
    from stock_emu import StockArm, REFERENCE_SHA256
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1

    output = PROJECT / "crates/keys/tests/fixtures"
    output.mkdir(parents=True, exist_ok=True)
    machine = StockArm(args.reference)
    matrix_counts = {}
    auxiliary_counts = {}
    decoder_path = output / "decoder.csv"
    with decoder_path.open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(["input_u16", "matrix_code", "auxiliary_code"])
        for value in range(65536):
            machine.write(0x20001134, struct.pack("<H", value))
            matrix = machine.call(0x08013084, count=200, timeout_us=0)[0]
            machine.write(0x200010E6, struct.pack("<H", value))
            auxiliary = machine.call(0x08013D68, count=80, timeout_us=0)[0]
            if not (0 <= matrix <= 255 and 0 <= auxiliary <= 255):
                raise RuntimeError("Unexpected code outside byte range")
            writer.writerow([value, matrix, auxiliary])
            matrix_counts[str(matrix)] = matrix_counts.get(str(matrix), 0) + 1
            auxiliary_counts[str(auxiliary)] = auxiliary_counts.get(str(auxiliary), 0) + 1
            if value % 16384 == 0:
                print(f"Decoded {value}/65536 inputs", flush=True)

    sampler = StockArm(args.reference)
    expected_gpio = ((0x40020800, 0x400), (0x40020800, 0x800),
                     (0x40020800, 0x1000), (0x40020C00, 4))
    state = {"high_mask": 0, "calls": []}

    def gpio_read(arm):
        pair = (arm.uc.reg_read(UC_ARM_REG_R0), arm.uc.reg_read(UC_ARM_REG_R1))
        index = len(state["calls"])
        if index >= 4 or pair != expected_gpio[index]:
            raise RuntimeError(f"Unexpected simulated GPIO read {index}: {pair}")
        state["calls"].append(pair)
        return (state["high_mask"] >> index) & 1

    sampler.intercept(0x080254DE, gpio_read)
    sampler_path = output / "rows.csv"
    row_count = 0
    with sampler_path.open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(["previous", "column", "row_high_mask", "result"])
        for previous in (0x0000, 0xFFFF, 0x5555, 0xAAAA):
            for column in range(4):
                for high_mask in range(16):
                    sampler.write(0x20001134, struct.pack("<H", previous))
                    state.update(high_mask=high_mask, calls=[])
                    sampler.call(0x0801FA9C, column, count=300, timeout_us=0)
                    if len(state["calls"]) != 4:
                        raise RuntimeError("Expected exactly four simulated GPIO reads")
                    result = struct.unpack("<H", sampler.read(0x20001134, 2))[0]
                    writer.writerow([previous, column, high_mask, result])
                    row_count += 1

    report = {
        "reference_sha256": REFERENCE_SHA256,
        "related_reference_only": True,
        "exact_user_mcu_image_unverified": True,
        "peripheral_mappings": [],
        "decoder_inputs": 65536,
        "decoder_calls": 131072,
        "row_sampler_cases": row_count,
        "matrix_output_counts": matrix_counts,
        "auxiliary_output_counts": auxiliary_counts,
        "substitutions": [{
            "callee": "0x080254DE", "meaning": "GPIO input read",
            "scope": "row sampler cases only; supplied binary input levels",
            "expected_call_order": [[hex(a), hex(b)] for a, b in expected_gpio],
        }],
        "fixtures": [{
            "path": str(path.relative_to(PROJECT)).replace("\\", "/"),
            "size": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        } for path in (decoder_path, sampler_path)],
    }
    report_path = PROJECT / "research/keys/oracle-summary.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"decoder_inputs": 65536, "row_sampler_cases": row_count,
                      "report": str(report_path)}), flush=True)


if __name__ == "__main__":
    main()
