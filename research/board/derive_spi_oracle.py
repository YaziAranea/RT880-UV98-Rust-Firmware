"""Bounded, offline SPI2 init oracle from the pinned related RT880 reference.

No peripheral addresses are mapped. Register helpers run against ordinary
emulated RAM. Their call sites are intercepted in 0x08010C74, and its one direct
CS-set store is recorded/skipped. This checks code/configuration, not hardware.
"""
from pathlib import Path
import csv
import hashlib
import json
import os
import struct
import sys

HERE = Path(__file__).resolve().parent
PROJECT = Path(os.environ.get("UV98_RUST_PROJECT", str(HERE.parents[1])))
WORK = PROJECT.parent
HELPER = PROJECT / "research"
if not (HELPER / "deps" / "unicorn").exists():
    HELPER = WORK / "calibration-analysis"
sys.path.insert(0, str(HELPER))
from stock_emu import StockArm, REFERENCE_SHA256, GPRS, UC_ARM_REG_PC, UC_HOOK_CODE

IMAGE = Path(os.environ.get("UV98_STOCK_REFERENCE", str(
    WORK / "reference-firmware" / "RT-880_Upgrade_V1_16_250915.nicsure.bin")))
OUT = PROJECT / "crates/board-at32/tests/fixtures/spi-init-oracle.csv"
RAM = 0x20004000
PARAM = 0x20004100


def trace(seed):
    gpio = bytearray(0x40)
    spi = bytearray(0x40)  # explicitly reset register state for SPI helper
    for offset in (0, 4, 8, 12, 0x24):
        struct.pack_into("<I", gpio, offset, seed)
    direct_stores = []
    helper_calls = []
    emu = StockArm(IMAGE)

    def helper(address, target, base, with_parameters):
        def run(e):
            assert e.uc.reg_read(GPRS[0]) == base
            machine = StockArm(IMAGE)
            machine.write(RAM, target)
            arg1 = e.uc.reg_read(GPRS[1])
            arg2 = e.uc.reg_read(GPRS[2])
            if with_parameters:
                params = e.read(arg1, 8)
                machine.write(PARAM, params)
                helper_calls.append([hex(address), params.hex()])
                machine.call(address, RAM, PARAM, count=20000)
            else:
                helper_calls.append([hex(address), arg1, arg2])
                machine.call(address, RAM, arg1, arg2, count=20000)
            target[:] = machine.read(RAM, len(target))
            return 0
        emu.intercept(address, run)

    helper(0x08025464, gpio, 0x40020400, True)
    helper(0x080254F0, gpio, 0x40020400, False)
    helper(0x08025B7E, spi, 0x40003800, True)
    helper(0x08025B3A, spi, 0x40003800, False)

    def cs_store(uc, pc, size, unused):
        address = uc.reg_read(GPRS[1])
        value = uc.reg_read(GPRS[0])
        assert (pc, size, address, value) == (0x08010CFE, 2, 0x40020418, 0x1000)
        direct_stores.append([hex(pc), hex(address), value])
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)

    emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, cs_store, begin=0x08010CFE, end=0x08010CFE))
    emu.call(0x08010C74, count=20000)
    assert len(direct_stores) == 1
    values = [seed] + [struct.unpack_from("<I", gpio, o)[0] for o in (0, 4, 8, 12, 0x24)]
    values += [struct.unpack_from("<I", spi, o)[0] for o in (0, 4, 0x1C)]
    values += [direct_stores[0][2]]
    assert values[1:6] == [
        (seed & 0x00ffffff) | 0xa9000000,
        seed & ~0xf000,
        (seed & 0x00ffffff) | 0x55000000,
        (seed & 0x00ffffff) | 0x55000000,
        (seed & 0x000fffff) | 0x55500000,
    ]
    assert values[6:] == [0x347, 0, 0, 0x1000]
    return values, helper_calls, direct_stores


def main():
    rows = []
    traces = []
    for seed in (0, 0xffffffff, 0xa5a5a5a5, 0x12345678):
        row, helpers, stores = trace(seed)
        rows.append(row)
        traces.append({"seed": seed, "helpers": helpers, "direct_stores": stores})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f, lineterminator="\n")
        writer.writerow(["seed", "cfgr", "omode", "odrvr", "pull", "muxh", "ctrl1", "ctrl2", "i2sctrl", "cs_set"])
        writer.writerows(rows)
    report = {
        "reference_sha256": REFERENCE_SHA256,
        "fixture_sha256": hashlib.sha256(OUT.read_bytes()).hexdigest(),
        "output": OUT.relative_to(PROJECT).as_posix(),
        "case_count": len(rows),
        "scope": "GPIO/SPI helper semantics in emulated RAM; no peripheral model, target boot or physical test",
        "traces": traces,
    }
    (HERE / "oracle-summary.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"SPI2 configuration oracle: {len(rows)} cases; no physical I/O")


if __name__ == "__main__":
    main()
