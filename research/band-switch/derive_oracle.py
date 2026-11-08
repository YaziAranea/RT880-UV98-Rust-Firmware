"""Bounded ARM oracle for routing bytes and receive-path request order.

No device, serial, MMIO mapping or firmware boot. The selector is stopped at
0x08012CE0, after routing writes and before calibration I/O. RF register helpers
are intercepted; exactly two PF0 store instructions are recorded and skipped.
TX helper traces are evidence only, not an implemented TX executor.
"""
import csv
import hashlib
import json
import os
import random
import struct
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
FIXTURES = PROJECT / "crates/band-plan/tests/fixtures"

def routes(frequency, mode):
    emu = StockArm(IMAGE)
    matched = []
    emu.write(0x20000838, bytes([0xa6, 0x5b]))
    def stop(uc, pc, size, unused):
        matched.append(True)
        uc.reg_write(UC_ARM_REG_PC, emu.STOP | 1)
    emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, stop, begin=0x08012CE0, end=0x08012CE0))
    emu.call(0x08012A30, frequency, mode, count=1000)
    tx, rx = emu.read(0x20000838, 2)
    return [frequency, mode, int(bool(matched)), tx, rx]

def requests(code, previous, tx=False):
    emu = StockArm(IMAGE)
    emu.write(0x20000838 if tx else 0x20000839, bytes([code]))
    events = []
    def read(e):
        assert e.uc.reg_read(GPRS[0]) == 0x33
        events.append(["read", "A", 0x33, previous])
        return previous
    def write(e):
        assert e.uc.reg_read(GPRS[0]) == 0x33
        events.append(["write", "A", 0x33, e.uc.reg_read(GPRS[1]) & 0xffff])
        return 0
    emu.intercept(0x0801E620, read)
    emu.intercept(0x0801E660, write)
    stores = {
        0x0800507C: (0x40021428, 1, "F", 0, 0),
        0x08005086: (0x40021418, 1, "F", 0, 1),
        0x0800535C: (0x40020828, 0x8000, "C", 15, 0),
        0x08005362: (0x40020428, 0x0800, "B", 11, 0),
        0x08005392: (0x40020818, 0x8000, "C", 15, 1),
        0x08005398: (0x40020428, 0x0800, "B", 11, 0),
        0x080053A4: (0x40020818, 0x8000, "C", 15, 1),
        0x080053AC: (0x40020418, 0x0800, "B", 11, 1),
    }
    def skip(uc, pc, size, unused):
        address, word, port, pin, high = stores[pc]
        assert size == 2 and uc.reg_read(GPRS[0]) == word and uc.reg_read(GPRS[1]) == address
        events.append(["gpio", port, pin, high])
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
    for pc in stores:
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, skip, begin=pc, end=pc))
    emu.call(0x08005354 if tx else 0x08004FF0, count=1000)
    return events

def save_csv(path, header, rows):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(header)
        writer.writerows(rows)
    return hashlib.sha256(path.read_bytes()).hexdigest()

def disassembly():
    # Capstone is optional for regenerating test vectors.
    for dep in [HELPER / "deps", WORK / "hardware_readback/deps"]:
        sys.path.insert(0, str(dep))
    try:
        from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_LITTLE_ENDIAN
        from capstone.arm import ARM_OP_MEM, ARM_REG_PC
    except ImportError:
        return False
    data = IMAGE.read_bytes()
    md = Cs(CS_ARCH_ARM, CS_MODE_THUMB | CS_MODE_LITTLE_ENDIAN)
    md.detail = True
    md.skipdata = True
    out = ["SHA256 " + REFERENCE_SHA256, "Literal pools and TBB tables are data, not instructions."]
    for start, end in [(0x08004FF0, 0x08005098), (0x08005354, 0x080053F0),
                       (0x08012A30, 0x08012CE0), (0x08011B7E, 0x08011BB2)]:
        out.append(f"\nRANGE {start:08X}..{end:08X} exclusive")
        for ins in md.disasm(data[start-0x08002800:end-0x08002800], start):
            comment = ""
            if ins.id and ins.mnemonic.startswith("ldr"):
                for op in ins.operands:
                    if op.type == ARM_OP_MEM and op.mem.base == ARM_REG_PC:
                        addr = ((ins.address + 4) & ~3) + op.mem.disp
                        if 0x08002800 <= addr <= 0x08002800 + len(data) - 4:
                            value = struct.unpack_from("<I", data, addr-0x08002800)[0]
                            comment = f" ; literal[{addr:08X}]={value:08X}"
            out.append(f"{ins.address:08X}: {ins.bytes.hex():<10} {ins.mnemonic:<9} {ins.op_str}{comment}")
    (HERE / "bounded-disassembly.txt").write_text("\n".join(out)+"\n", encoding="utf-8")
    return True

def main():
    boundaries = [0, 1_800_000, 3_200_000, 4_500_000, 6_400_000, 13_600_000,
                  17_400_000, 24_000_000, 26_000_000, 32_000_000, 40_000_000,
                  48_000_000, 56_000_000, 66_000_000, 84_000_000, 92_000_000,
                  100_000_000, 108_000_000, 116_000_000, 124_000_000, 130_000_000, 0xffffffff]
    frequencies = {value+d for value in boundaries for d in [-1, 0, 1] if 0 <= value+d <= 0xffffffff}
    rng = random.Random(0x880116)
    frequencies.update(rng.randrange(0, 140_000_000) for _ in range(96))
    selection = [routes(f, mode) for f in sorted(frequencies) for mode in [0, 1, 2, 255]]
    seeds = sorted({0, 0xffff, 0xa5a5, 0x5a5a, 0xff00, 0x00ff, 0x000c} | {1 << b for b in range(16)})
    inputs = sorted({(code, 0xa5a5) for code in range(256)} |
                    {(code, prior) for code in [0, 1, 2, 3, 4, 5, 6, 255] for prior in seeds})
    rx = []
    tx = []
    for code, prior in inputs:
        for dest, is_tx in [(rx, False), (tx, True)]:
            for step, event in enumerate(requests(code, prior, is_tx)):
                dest.append([code, prior, step, *event])
    hashes = {}
    for name, header, rows in [
        ("routing.csv", ["frequency_10hz", "mode", "matched", "tx_code", "rx_code"], selection),
        ("receive.csv", ["route_code", "prior_reg33", "step", "kind", "target", "address", "value"], rx),
        ("transmit-evidence.csv", ["route_code", "prior_reg33", "step", "kind", "target", "address", "value"], tx),
    ]:
        path = FIXTURES / name if name != "transmit-evidence.csv" else HERE / name
        hashes[path.relative_to(PROJECT).as_posix()] = save_csv(path, header, rows)
    report = {
        "reference_sha256": REFERENCE_SHA256,
        "selector_cases": len(selection), "receive_cases": len(inputs), "tx_evidence_cases": len(inputs),
        "receive_events": len(rx), "tx_evidence_events": len(tx),
        "mmio_mapped": False, "hardware_accessed": False,
        "selector_stop": "0x08012CE0 before calibration I/O",
        "substitutions": ["A read/write 0x0801E620/0x0801E660", "exact reviewed MCU store instructions"],
        "disassembly_generated": disassembly(), "sha256": hashes,
    }
    (HERE / "oracle-summary.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(report, indent=2))

if __name__ == "__main__":
    main()
