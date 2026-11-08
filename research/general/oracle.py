"""Generate reference results by executing bounded RT880 Thumb functions offline.

No serial/device/network access. Only Flash and RAM are mapped. Peripheral
callees are intercepted; every substitution is recorded in evidence.json.
"""
from pathlib import Path
import csv
import hashlib
import json
import os
import struct
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stock_emu import StockArm, REFERENCE_SHA256
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2

ROOT = HERE.parents[2]
PROJECT = Path(os.environ.get("UV98_RUST_PROJECT", str(ROOT / "work/uv98-rust-stage")))
OUT = PROJECT / "crates/stock-model/tests/fixtures"
OUT.mkdir(parents=True, exist_ok=True)

def save(name, header, rows):
    path = OUT / name
    with path.open("w", newline="", encoding="ascii") as file:
        writer = csv.writer(file)
        writer.writerow(header)
        writer.writerows(rows)
    return {"rows": len(rows), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

def reg(cpu, number):
    return cpu.uc.reg_read((UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2)[number])

artifacts = {}

# Load entry with ordinary argument zero; SPI READ gets bytes from RAM fixtures.
cpu = StockArm()
primary = bytearray(range(32))
backup = bytearray(range(128, 160))
reads = []
def spi_read(cpu):
    dest, addr, length = (reg(cpu, x) for x in range(3))
    assert addr in (0, 0x1000) and length == 32
    reads.append(addr)
    cpu.write(dest, primary if addr == 0 else backup)
    return 0
def unexpected(cpu):
    raise AssertionError("ordinary load unexpectedly reached persistence helper")
cpu.intercept(0x08025D30, spi_read)
for address in (0x080119B0, 0x08025DE4, 0x08025E40, 0x08025F28):
    cpu.intercept(address, unexpected)
rows = []
for first in (0, 0x9A, 0xFF):
    for second in range(256):
        primary[0], backup[0] = first, second
        reads.clear()
        cpu.write(0x20003044, bytes([0xCC]) * 32)
        cpu.write(0x200010E9, b"\xff")
        cpu.call(0x08013E3C, 0, count=1000, timeout_us=0)
        source = cpu.read(0x200010E9, 1)[0]
        expected_data = (primary if source == 0 else backup if source == 1
                         else cpu.read(0x08028344, 32))
        assert cpu.read(0x20003044, 32) == expected_data
        rows.append((first, second, source, len(reads), reads[-1]))
artifacts["general_selection.csv"] = save("general_selection.csv",
    ("primary_marker", "backup_marker", "stock_source", "spi_read_count", "last_spi_address"), rows)

# ADC acquisition callee only: no hardware mapping, DMA completion supplied.
cpu = StockArm()
cpu.intercept(0x08024CC4, lambda c: 0)
rows = []
for sample in list(range(4096)) + [32767, 32768, 65534, 65535]:
    cpu.write(0x200010CC, struct.pack("<HH", sample, 0xA55A))
    cpu.word(0x200010D8, 1)
    cpu.word(0x200010D4, 0)
    cpu.call(0x080121A8, count=1000, timeout_us=0)
    result, second = struct.unpack("<HH", cpu.read(0x200010E4, 4))
    assert second == 0xA55A
    assert cpu.read(0x200010D4, 8) == bytes(8)
    rows.append((sample, result))
artifacts["general_battery_adc.csv"] = save("general_battery_adc.csv", ("dma_sample", "code"), rows)

cpu = StockArm()
rows = [(value, cpu.call(0x080122AC, value, count=1000, timeout_us=0)[0]) for value in range(256)]
artifacts["general_battery_indicator.csv"] = save("general_battery_indicator.csv", ("code", "indicator"), rows)

# Noise/squelch setters execute their actual arithmetic. Only bus writer is stubbed.
cpu = StockArm()
writes = []
def bus_write(cpu):
    writes.append((reg(cpu, 0), reg(cpu, 1)))
    return 0
cpu.intercept(0x0801E660, bus_write)
rows = []
for level in range(11):
    cpu.write(0x200035EC + 0xA4, bytes([level]))
    for noise in range(256):
        squelch = (73 * noise + 19) & 255
        writes.clear()
        cpu.call(0x08004ECC, noise, count=1000, timeout_us=0)
        cpu.call(0x08004F60, squelch, count=1000, timeout_us=0)
        assert len(writes) == 2 and [x[0] for x in writes] == [0x4F, 0x78]
        rows.append((level, noise, squelch, writes[0][1], writes[1][1]))
artifacts["general_squelch.csv"] = save("general_squelch.csv",
    ("level", "noise", "squelch", "reg_4f", "reg_78"), rows)

# VOX decision only: RF register 0x64 sample supplied; no TX controller runs.
cpu = StockArm()
sample = 0
def bus_read(cpu):
    assert reg(cpu, 0) == 0x64
    return sample
cpu.intercept(0x0801E620, bus_read)
rows = []
for threshold in range(256):
    upper = (threshold + 10) & 255
    samples = sorted({0, 1, 255, 256, 4095, 65535, threshold,
                      max(0, threshold-1), threshold+1, upper,
                      max(0, upper-1), upper+1})
    for previous in (0, 1):
        for sample in samples:
            cpu.write(0x2000112E, bytes([previous]))
            result = cpu.call(0x08005708, upper, threshold, count=1000, timeout_us=0)[0]
            assert result in (0, 1)
            rows.append((threshold, sample, previous, result))
artifacts["general_vox.csv"] = save("general_vox.csv",
    ("threshold", "sample", "previous", "result"), rows)

evidence = {
    "reference_sha256": REFERENCE_SHA256,
    "not_exact_user_mcu_image": True,
    "runtime": "Unicorn 2.1.4, Cortex-M4, Thumb, Flash+RAM only; no MMIO mappings",
    "source_identity": "related public RT-880 1.16, not acquired from user MCU",
    "functions": {
        "selection": "0x08013E3C called with r0=0; SPI reader 0x08025D30 supplies synthetic bytes; mutation-related callees denylisted; source 2 ROM bytes observed only in emulator",
        "battery_adc": "0x080121A8; ADC control callee 0x08024CC4 returns 0, DMA-complete flag supplied; actual integer conversion and stores execute",
        "battery_indicator": "0x080122AC unmodified, no intercepted callees",
        "squelch": "0x08004ECC and 0x08004F60; bus writer 0x0801E660 captures arguments, no device operation",
        "vox": "0x08005708; bus reader 0x0801E620 returns fixture sample only for reg0x64. Caller-derived upper=(threshold+10)&255 supplied; parent TX controller not executed",
    },
    "not_proved": ["actual radio calibration behavior", "all general fields used", "VOX scheduler timing", "hardware units", "physical restoration", "user-image identity"],
    "artifacts": artifacts,
}
(HERE / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
print(json.dumps(artifacts, indent=2))
