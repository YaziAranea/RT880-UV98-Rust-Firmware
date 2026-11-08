"""Bounded related-stock RF shutdown requests. Offline; all MMIO unmapped.

RF register I/O is captured, not executed. Three reviewed MCU CLR stores in
the boot slice and two in TX-gates helper are recorded/skipped at exact PCs.
No complete boot, target I/O, firmware installation or host EXE is run.
"""
from pathlib import Path
import csv
import hashlib
import json
import os
import sys

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
OUT = PROJECT / "crates/board-policy/tests/fixtures/rf-requests.csv"


def capture(entry, value, boot_slice=False):
    emu = StockArm(IMAGE)
    events = []
    reads = []
    for address, chip in ((0x0801E620, "A"), (0x0801E698, "B")):
        def read(e, chip=chip):
            register = e.uc.reg_read(GPRS[0])
            assert register == 0x33
            reads.append([chip, register, value])
            return value
        emu.intercept(address, read)
    for address, chip in ((0x0801E660, "A"), (0x0801E6D4, "B")):
        def write(e, chip=chip):
            events.append(["rf", chip, e.uc.reg_read(GPRS[0]), e.uc.reg_read(GPRS[1]) & 0xffff])
            return 0
        emu.intercept(address, write)

    stores = {
        0x08005320: (0x40020828, 0x8000, "C", 15),
        0x08005326: (0x40020428, 0x0800, "B", 11),
        0x08011B74: (0x40020828, 0x8000, "C", 15),
        0x08011B78: (0x40020828, 0x0010, "C", 4),
        0x08011B7C: (0x40020828, 0x0020, "C", 5),
    }
    def skip_store(uc, pc, size, unused):
        address, word, port, pin = stores[pc]
        assert size == 2
        assert uc.reg_read(GPRS[0]) == word
        assert uc.reg_read(GPRS[1]) == address
        events.append(["mcu", port, pin, 0])
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
    for pc in stores:
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, skip_store, begin=pc, end=pc))
    if boot_slice:
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE,
            lambda uc, pc, size, unused: uc.reg_write(UC_ARM_REG_PC, emu.STOP | 1),
            begin=0x08011B7E, end=0x08011B7E))
    emu.call(entry, 0, count=20000)
    return events, reads


def main():
    rows = []
    cases = []
    seeds = sorted(set([0, 0xffff, 0xff00, 0x00ff, 0xa5a5, 0x5a5a, 0x000c, 0x0033] + [1 << i for i in range(16)]))
    plans = [
        ("tx_gates_a", 0x08005318, seeds, False),
        ("sleep_a", 0x080052B0, seeds, False),
        ("clear_b_gpio", 0x08003F08, seeds, False),
        ("sleep_b", 0x08003EC0, [0], False),
        ("zero_a", 0x08004F0C, [0], False),
        ("zero_b", 0x08003E08, [0], False),
        ("boot_levels", 0x08011B6C, [0], True),
    ]
    for name, entry, inputs, boot_slice in plans:
        for prior in inputs:
            events, reads = capture(entry, prior, boot_slice)
            for index, event in enumerate(events):
                rows.append([name, prior, index, *event])
            cases.append({"plan": name, "entry": hex(entry), "input": prior,
                          "read_registers": reads, "events": len(events)})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f, lineterminator="\n")
        writer.writerow(["plan", "input", "step", "kind", "target", "address", "value"])
        writer.writerows(rows)
    summary = {
        "reference_sha256": REFERENCE_SHA256,
        "fixture_sha256": hashlib.sha256(OUT.read_bytes()).hexdigest(),
        "output": OUT.relative_to(PROJECT).as_posix(),
        "case_count": len(cases), "vector_count": len(rows),
        "scope": "Register/GPIO request traces only. No physical RF-off assertion. Composite plan is new policy, not a captured stock function.",
        "cases": cases,
    }
    (HERE / "oracle-summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(f"RF request oracle: {len(cases)} cases, {len(rows)} events; no target I/O")


if __name__ == "__main__":
    main()
