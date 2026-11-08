"""Capture logical three-wire transactions from bounded original Thumb code.

All MMIO remains unmapped. Only the explicit GPIO store PCs below are skipped;
DATA direction, DATA read and delay callees are intercepted at reviewed entries.
No boot flow, host firmware execution, serial or hardware access.
"""
import csv
import hashlib
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROJECT = Path(os.environ.get("UV98_RUST_PROJECT", str(HERE.parents[1])))
WORK = PROJECT.parent
HELPER = PROJECT / "research"
if not (HELPER / "deps/unicorn").exists():
    HELPER = WORK / "calibration-analysis"
sys.path.insert(0, str(HELPER))
from stock_emu import StockArm, REFERENCE_SHA256, GPRS, UC_ARM_REG_PC, UC_HOOK_CODE

IMAGE = Path(os.environ.get("UV98_STOCK_REFERENCE", str(
    WORK / "reference-firmware/RT-880_Upgrade_V1_16_250915.nicsure.bin")))
FIXTURE = PROJECT / "crates/rf-bus/tests/fixtures/transactions.csv"

# (PC, r1-relative displacement). Actual GPIO address and mask are checked.
STORES = {
    0x0801E628: 0, 0x0801E62E: 0x28, 0x0801E64A: 0x18,
    0x0801E66A: 0, 0x0801E670: 0x28, 0x0801E68A: 0x18,
    0x0801D8F6: 0, 0x0801D902: 0, 0x0801D90E: 0, 0x0801D91C: 0,
    0x0801D84A: 0, 0x0801D85E: 0, 0x0801D878: 0,
    0x0801E6A8: 0, 0x0801E6AC: 0, 0x0801E6C8: 0,
    0x0801E6E6: 0, 0x0801E6EA: 0, 0x0801E704: 0,
    0x0801D7FC: 0, 0x0801D80A: 0, 0x0801D812: 0, 0x0801D822: 0,
    0x0801D750: 0, 0x0801D766: 0, 0x0801D784: 0,
}

def capture(profile, operation, register, word):
    emu = StockArm(IMAGE)
    events = []
    sampled = []
    pins = ({(0x40020000, 0x100): "cs", (0x40021400, 0x40): "clock", (0x40020800, 0x200): "data"}
            if profile == "A" else
            {(0x40020800, 0x80): "cs", (0x40020800, 0x100): "clock", (0x40020800, 0x40): "data"})
    def store(uc, pc, size, unused):
        assert size == 2
        address = uc.reg_read(GPRS[1]) + STORES[pc]
        high = address & 0xff == 0x18
        assert address & 0xff in [0x18, 0x28]
        pin = pins[(address & ~0xff, uc.reg_read(GPRS[0]))]
        events.append([pin, int(high)])
        uc.reg_write(UC_ARM_REG_PC, (pc+size) | 1)
    for pc in STORES:
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, store, begin=pc, end=pc))
    def direction(e):
        value = e.uc.reg_read(GPRS[0])
        assert value in [0, 1]
        events.append(["direction", value]) # 0 output, 1 input
        return 0
    emu.intercept(0x0801D894 if profile == "A" else 0x0801D79C, direction)
    def delay(e):
        assert e.uc.reg_read(GPRS[0]) == 10
        events.append(["delay", 10])
        return 0
    emu.intercept(0x080251DC, delay)
    def sample(e):
        assert e.uc.reg_read(GPRS[0]) == 0x40020800
        assert e.uc.reg_read(GPRS[1]) == (0x200 if profile == "A" else 0x40)
        assert len(sampled) < 16
        bit = (word >> (15-len(sampled))) & 1
        sampled.append(bit)
        events.append(["sample", bit])
        return bit
    emu.intercept(0x080254DE, sample)
    entry = {("A", "read"): 0x0801E620, ("A", "write"): 0x0801E660,
             ("B", "read"): 0x0801E698, ("B", "write"): 0x0801E6D4}[profile, operation]
    result = emu.call(entry, register, word, count=5000)[0]
    if operation == "read":
        assert len(sampled) == 16 and result == word
    else:
        assert not sampled
    assert sum(event == ["clock", 1] for event in events) == 24
    return events

def main():
    rows = []
    cases = 0
    for profile in ["A", "B"]:
        for operation in ["read", "write"]:
            for register in [0, 1, 0x33, 0x7f]:
                for word in [0, 0xffff, 0xa55a, 0x1234, 0x8001]:
                    events = capture(profile, operation, register, word)
                    rows.extend([profile, operation, register, word, index, *event] for index, event in enumerate(events))
                    cases += 1
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    with FIXTURE.open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f, lineterminator="\n")
        writer.writerow(["profile", "operation", "register", "word", "step", "event", "value"])
        writer.writerows(rows)
    report = {
        "reference_sha256": REFERENCE_SHA256, "cases": cases, "events": len(rows),
        "fixture": FIXTURE.relative_to(PROJECT).as_posix(),
        "fixture_sha256": hashlib.sha256(FIXTURE.read_bytes()).hexdigest(),
        "mmio_mapped": False, "hardware_accessed": False,
        "skipped_gpio_store_pcs": [f"0x{pc:08X}" for pc in STORES],
        "intercepts": ["A/B direction helpers", "GPIO data sample 0x080254DE", "delay(10) 0x080251DC"],
        "limits": "logical pin call order only, not electrical timing, target response, or physical RF state",
    }
    (HERE / "oracle-summary.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(report, indent=2))

if __name__ == "__main__":
    main()
