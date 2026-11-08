"""Derive LCD call traces from the hash-pinned related RT880 binary, offline.

No peripheral address is mapped. Three reviewed PC14 reset stores are recorded
and skipped. LCD byte output, delays, and two costly clears are explicit test
doubles. This proves the control sequence, not physical panel behavior.
"""
from pathlib import Path
import hashlib
import json
import os
import struct
import sys

HERE = Path(__file__).resolve().parent
PROJECT = Path(os.environ.get("UV98_RUST_PROJECT", str(HERE.parents[1])))
WORK = PROJECT.parent
HELPER = PROJECT / "research"
if not (HELPER / "stock_emu.py").exists():
    HELPER = WORK / "calibration-analysis"
sys.path.insert(0, str(HELPER))
LOCAL_UNICORN = WORK / "calibration-analysis" / "deps"
if LOCAL_UNICORN.exists():
    sys.path.insert(0, str(LOCAL_UNICORN))
from stock_emu import StockArm, REFERENCE_SHA256, GPRS, UC_ARM_REG_SP, UC_ARM_REG_PC, UC_HOOK_CODE

IMAGE = Path(os.environ.get("UV98_STOCK_REFERENCE", str(
    WORK / "reference-firmware" / "RT-880_Upgrade_V1_16_250915.nicsure.bin")))
FIXTURES = PROJECT / "crates" / "display" / "tests" / "fixtures"


def trace_init():
    emu = StockArm(IMAGE)
    events = []
    emu.intercept(0x08018184, lambda e: events.append(["command", e.uc.reg_read(GPRS[0]) & 255]))
    emu.intercept(0x080181A8, lambda e: events.append(["data", e.uc.reg_read(GPRS[0]) & 255]))
    emu.intercept(0x08008380, lambda e: events.append(["delay_ms", e.uc.reg_read(GPRS[0])]))

    def rectangle(e):
        regs = [e.uc.reg_read(r) for r in GPRS[:4]]
        color = struct.unpack("<I", e.read(e.uc.reg_read(UC_ARM_REG_SP), 4))[0]
        events.append(["stock_fill_inclusive_y_y_x_x_color", *regs, color])

    emu.intercept(0x08017DF0, rectangle)
    emu.intercept(0x08017E2E, lambda e: events.append(["stock_clear_color", e.uc.reg_read(GPRS[0])]))
    expected_stores = {
        0x08017E66: (0x40020818, 0x4000, True),
        0x08017E76: (0x40020828, 0x4000, False),
        0x08017E84: (0x40020818, 0x4000, True),
    }
    seen = []

    def reset_store(uc, pc, size, unused):
        address, value, level = expected_stores[pc]
        assert uc.reg_read(GPRS[1]) == address
        assert uc.reg_read(GPRS[0]) == value
        assert size == 2
        events.append(["reset", level])
        seen.append(pc)
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)

    for pc in expected_stores:
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE, reset_store, begin=pc, end=pc))
    emu.call(0x08017E48)
    assert seen == list(expected_stores)
    split = next(i for i, e in enumerate(events) if e[0].startswith("stock_fill"))
    assert events[split] == ["stock_fill_inclusive_y_y_x_x_color", 0, 239, 0, 319, 0]
    assert events[split + 1] == ["command", 0x29]
    assert events[-1] == ["stock_clear_color", 0]
    full_window = events[split + 2:-1]
    assert full_window == [
        ["command", 0x2A], ["data", 0], ["data", 0], ["data", 1], ["data", 0x3F],
        ["command", 0x2B], ["data", 0], ["data", 0], ["data", 0], ["data", 0xEF],
    ]
    return events, events[:split], full_window


def trace_words():
    emu = StockArm(IMAGE)
    output = []
    emu.intercept(0x080181A8, lambda e: output.append(e.uc.reg_read(GPRS[0]) & 255))
    vectors = []
    for word in [0, 1, 255, 256, 0x1234, 0xF800, 0x07E0, 0x001F, 0x8410, 0xFFFF]:
        output.clear()
        emu.call(0x080181FA, word)
        assert len(output) == 2
        vectors.append({"word": word, "bytes": output.copy()})
    return vectors


def text_events(events):
    lines = []
    for kind, value in events:
        if kind in ("command", "data"):
            lines.append(f"{kind} {value:02X}")
        elif kind == "reset":
            lines.append(f"reset {int(value)}")
        else:
            lines.append(f"{kind} {value}")
    return "\n".join(lines) + "\n"


def main():
    events, prefix, full_window = trace_init()
    words = trace_words()
    FIXTURES.mkdir(parents=True, exist_ok=True)
    (FIXTURES / "stock-control-prefix.txt").write_text(text_events(prefix), encoding="ascii")
    (FIXTURES / "stock-full-window.txt").write_text(text_events(full_window), encoding="ascii")
    (FIXTURES / "stock-pixel-words.txt").write_text("".join(
        f"{v['word']:04X} {' '.join(f'{b:02X}' for b in v['bytes'])}\n" for v in words), encoding="ascii")
    report = {
        "reference_sha256": REFERENCE_SHA256,
        "reference_label": "Related RT880 1.16, not the user's extracted UV98 MCU image",
        "scope": "Offline control-flow oracle with documented intercepts; no device/MMIO access",
        "hardware_qualified": False,
        "init_address": "0x08017E48",
        "intercepts": {
            "0x08018184": "record command low byte",
            "0x080181A8": "record data low byte",
            "0x08008380": "record delay argument (milliseconds, delay-factor analysis)",
            "0x08017DF0": "record rectangle y0,y1,x0,x1 inclusive, stack color; skip pixels",
            "0x08017E2E": "record clear color; skip pixels",
        },
        "skipped_mmio_store_pcs": ["0x08017E66", "0x08017E76", "0x08017E84"],
        "init_events": events,
        "control_prefix_events": prefix,
        "explicit_full_window_events": full_window,
        "pixel_word_address": "0x080181FA",
        "pixel_word_vectors": words,
        "driver_difference": "Full 4-byte CASET/RASET, one clear before DisplayOn, not the entire stock init trace",
        "fixtures_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                            for p in sorted(FIXTURES.glob("stock-*.txt"))},
    }
    (HERE / "oracle.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": "passed", "prefix_events": len(prefix), "word_vectors": len(words),
                      "hardware_qualified": False}))


if __name__ == "__main__":
    main()
