"""Bounded reference clock calculator with synthetic CRM registers, not boot.

Requires research/stock_emu.py and Unicorn. The only added MMIO page is ordinary
emulated memory at 0x40023000. No device, serial port or hardware clock is touched.
"""
from pathlib import Path
import json
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from stock_emu import StockArm, REFERENCE_SHA256
from unicorn import UC_PROT_READ, UC_PROT_WRITE

cases = [
    ("stock_pll_48mhz", 0x31801, 0x100A, 0x1000, 48_000_000),
    ("same_pll_ahb_div2", 0x31801, 0x108A, 0x1000, 24_000_000),
    ("hick_8mhz", 0x31801, 0, 0, 8_000_000),
    ("hick_direct_48mhz", 0x31801, 0, 0x5000, 48_000_000),
]
results = []
for name, pll, cfg, misc1, expected in cases:
    model = StockArm()
    model.uc.mem_map(0x40023000, 0x1000, UC_PROT_READ | UC_PROT_WRITE)
    for address, value in [(0x40023804, pll), (0x40023808, cfg), (0x400238A0, misc1)]:
        model.uc.mem_write(address, struct.pack('<I', value))
    model.call(0x08025F6C, count=2000)
    actual = struct.unpack('<I', model.read(0x20001914, 4))[0]
    assert actual == expected, (name, actual, expected)
    results.append(dict(name=name, pllcfg=hex(pll), cfg=hex(cfg), misc1=hex(misc1), expected_hz=expected, actual_hz=actual))

report = dict(reference_sha256=REFERENCE_SHA256, function='0x08025F6C',
              tests=results, all_passed=True, physical_measurement=False,
              scope='Only stock clock calculation with synthetic read-only CRM inputs; no oscillator startup, power/RF state, bootloader or interrupts are modeled.',
              derived_stock_profile=dict(core_hz=48_000_000, apb1_hz=24_000_000, apb2_hz=48_000_000, spi2_sck_hz=12_000_000),
              formula='8MHz * NS96 / (MS1 * FR8) / 2; AHB /1, APB1 /2, APB2 /1, SPI2 /2')
Path(__file__).with_name('clock-oracle.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
print('4 bounded stock clock-calculator cases passed (synthetic registers only).')
