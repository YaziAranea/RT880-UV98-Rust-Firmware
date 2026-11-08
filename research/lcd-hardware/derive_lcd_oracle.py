"""Bounded offline SPI3/LCD configuration oracle, no MMIO mapped."""
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
sys.path.insert(0, str(PROJECT / "research"))
sys.path.insert(0, str(WORK / "calibration-analysis" / "deps"))
from stock_emu import StockArm, REFERENCE_SHA256, GPRS, UC_ARM_REG_PC, UC_HOOK_CODE

IMAGE = Path(os.environ.get("UV98_STOCK_REFERENCE", str(
    WORK / "reference-firmware" / "RT-880_Upgrade_V1_16_250915.nicsure.bin")))
RAM, PARAM = 0x20004000, 0x20004100
OFFSETS = (0, 4, 8, 12, 0x20, 0x24)


def trace(seed):
    ports = {base: bytearray(0x40) for base in (0x40020000, 0x40020400, 0x40020800)}
    for memory in ports.values():
        for offset in OFFSETS:
            struct.pack_into("<I", memory, offset, seed)
    spi = bytearray(0x40)
    calls, stores = [], []
    emu = StockArm(IMAGE)

    def gpio_helper(address, parameters):
        def callback(e):
            base = e.uc.reg_read(GPRS[0])
            assert base in ports
            machine = StockArm(IMAGE)
            machine.write(RAM, ports[base])
            if parameters:
                raw = e.read(e.uc.reg_read(GPRS[1]), 8)
                machine.write(PARAM, raw)
                calls.append([hex(address), hex(base), raw.hex()])
                machine.call(address, RAM, PARAM)
            else:
                args = [e.uc.reg_read(r) for r in GPRS[1:3]]
                calls.append([hex(address), hex(base), *args])
                machine.call(address, RAM, *args)
            ports[base][:] = machine.read(RAM, 0x40)
            return 0
        emu.intercept(address, callback)

    def spi_helper(address, parameters):
        def callback(e):
            assert e.uc.reg_read(GPRS[0]) == 0x40003C00
            machine = StockArm(IMAGE)
            machine.write(RAM, spi)
            if parameters:
                raw = e.read(e.uc.reg_read(GPRS[1]), 8)
                machine.write(PARAM, raw)
                calls.append([hex(address), "0x40003c00", raw.hex()])
                machine.call(address, RAM, PARAM)
            else:
                value = e.uc.reg_read(GPRS[1])
                calls.append([hex(address), "0x40003c00", value])
                machine.call(address, RAM, value)
            spi[:] = machine.read(RAM, 0x40)
            return 0
        emu.intercept(address, callback)

    gpio_helper(0x08025464, True)
    gpio_helper(0x080254F0, False)
    spi_helper(0x08025B7E, True)
    spi_helper(0x08025B44, False)
    spi_helper(0x08025B3A, False)

    def store(uc, pc, size, unused):
        address, value = uc.reg_read(GPRS[1]), uc.reg_read(GPRS[0])
        assert (pc, size, address, value) in (
            (0x080180C4, 2, 0x40020828, 0x4000),
            (0x080180C8, 2, 0x40020828, 0x2000),
        )
        stores.append([hex(pc), hex(address), value])
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
    for pc in (0x080180C4, 0x080180C8):
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, store, begin=pc, end=pc))
    emu.call(0x08018064, count=20000)
    assert len(stores) == 2
    values = [seed]
    for base in ports:
        values.extend(struct.unpack_from("<I", ports[base], off)[0] for off in OFFSETS)
    values.extend(struct.unpack_from("<I", spi, off)[0] for off in (0, 4, 0x1C))
    assert values[-3:] == [0xC046, 4, 0]
    return values, {"seed": seed, "helper_calls": calls, "skipped_stores": stores}


def main():
    traces, rows = [], []
    for seed in (0, 0xFFFFFFFF, 0xA5A5A5A5, 0x12345678):
        values, evidence = trace(seed)
        rows.append(values)
        traces.append(evidence)
    out = PROJECT / "crates/lcd-at32/tests/fixtures/stock-lcd-init.csv"
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="ascii", newline="") as file:
        writer = csv.writer(file, lineterminator="\n")
        writer.writerow(["seed"] + [port + "_" + reg for port in ("a", "b", "c")
                         for reg in ("cfgr", "omode", "odrvr", "pull", "muxl", "muxh")]
                        + ["ctrl1", "ctrl2", "i2sctrl"])
        writer.writerows(rows)
    report = {
        "reference_sha256": REFERENCE_SHA256,
        "init_address": "0x08018064",
        "peripheral": "SPI3 at 0x40003C00, not SPI1",
        "fixture_sha256": hashlib.sha256(out.read_bytes()).hexdigest(),
        "hardware_qualified": False,
        "scope": "Register helpers executed against RAM; LCD GPIO/SPI calls intercepted; two direct PC stores verified/skipped; no peripheral mappings",
        "traces": traces,
    }
    (HERE / "oracle.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("LCD SPI3 configuration oracle: 4 cases; CTRL1=C046 CTRL2=4; no physical I/O")


if __name__ == "__main__":
    main()
