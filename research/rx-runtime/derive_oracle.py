"""Bounded RX/audio/RSSI stock observations. No device or MMIO mapping.

Only RF helper calls and the listed GPIO/SysTick instruction PCs are replaced.
All masks, branches, arithmetic and requested register order execute stock Thumb.
The SysTick model supplies COUNTFLAG immediately: it proves requested LOAD values,
not elapsed time. This is not a boot or a complete receiver transition.
"""
import csv
import hashlib
import json
import os
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

IMAGE = Path(os.environ.get("UV98_STOCK_REFERENCE", str(WORK / "reference-firmware/RT-880_Upgrade_V1_16_250915.nicsure.bin")))


def stop_at(m, address):
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
        lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
        begin=address, end=address))


def capture():
    m, events, supplied = StockArm(IMAGE), [], {}
    for address, chip in [(0x0801E660, "A"), (0x0801E6D4, "B")]:
        def write(e, chip=chip):
            events.append(["write", chip, e.uc.reg_read(GPRS[0]), e.uc.reg_read(GPRS[1]) & 0xffff])
            return 0
        m.intercept(address, write)
    for address, chip in [(0x0801E620, "A"), (0x0801E698, "B")]:
        def read(e, chip=chip):
            reg = e.uc.reg_read(GPRS[0])
            value = supplied[chip, reg]
            events.append(["read", chip, reg, value])
            return value
        m.intercept(address, read)
    return m, events, supplied


def save_csv(name, header, rows):
    with (HERE / name).open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f, lineterminator="\n")
        writer.writerow(header)
        writer.writerows(rows)


def gpio_store(m, pc, address, high, events):
    def hook(uc, actual_pc, size, _):
        assert size == 2 and actual_pc == pc
        assert uc.reg_read(GPRS[0]) == 0x100
        assert uc.reg_read(GPRS[1]) == address
        events.append(["gpio", "F", 8, int(high)])
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, hook, begin=pc, end=pc))


def main():
    counts = {}
    m, events, supplied = capture()
    rssi = []
    stop_at(m, 0x08007C50)
    for upper in (0, 0xfe00):
        for low in range(512):
            value = upper | low
            supplied["A", 0x67] = value
            events.clear()
            raw = m.call(0x08004F40)[0]
            assert events == [["read", "A", 0x67, value]]
            events.clear()
            m.call(0x08007C3A)
            display = m.uc.reg_read(GPRS[7])
            assert events == [["read", "A", 0x67, value]]
            rssi.append([value, raw, display])
    save_csv("rssi.csv", ["reg67", "raw9", "display_magnitude_7c3a"], rssi)
    counts["rssi"] = len(rssi)

    m, events, supplied = capture()
    rows = []
    for level in (0, 1, 10):
        for flag in (0, 1, 0x80, 0xff):
            for raw in (0, 1, 2, 3, 4, 0x10, 0xfffe, 0xffff):
                supplied["A", 0x0c] = raw
                m.write(0x200035ec + 0xa4, bytes([level]))
                m.write(0x20001120, bytes([flag]))
                events.clear()
                result = m.call(0x080051E4)[0]
                assert events in ([], [["read", "A", 0x0c, raw]])
                rows.append([level, flag, raw, len(events), result])
    save_csv("squelch-status.csv", ["level", "software_flag", "reg0c", "read_count", "result"], rows)
    counts["squelch_status"] = len(rows)

    m, events, _ = capture()
    rows = []
    for chip, entry in [("A", 0x080038AC), ("B", 0x08003A08)]:
        for mode in range(256):
            events.clear()
            m.call(entry, mode)
            assert len(events) == 1 and events[0][:3] == ["write", chip, 0x47]
            rows.append([chip, mode, events[0][2], events[0][3]])
    save_csv("audio-mode.csv", ["chip", "mode", "register", "value"], rows)
    counts["audio_mode"] = len(rows)

    m, events, supplied = capture()
    stop_at(m, 0x0800518e)  # after actual 05768 volume helper, before UI/audio flag logic
    m.word(0x200010ec, 0x20005000)
    m.write(0x20005000, b"\0")  # channel modulation byte0: ordinary FM branch
    rows, case = [], 0
    for ae in (0, 3, 255):
        for af in (0, 4, 15, 255):
            for reg1 in (0, 2, 10):
                m.write(0x200035ec + 0xae, bytes([ae, af]))
                supplied["A", 1] = reg1
                events.clear()
                m.call(0x080050EA)
                assert [e[:3] for e in events] == [["write", "A", 0x47], ["read", "A", 1], ["write", "A", 0x48]]
                for step, (kind, chip, reg, value) in enumerate(events):
                    assert chip == "A"
                    rows.append([case, ae, af, reg1, step, kind, reg, value])
                case += 1
    save_csv("fm-audio.csv", ["case", "gain_byte_ae", "gain_byte_af", "reg01", "step", "kind", "register", "value"], rows)
    counts["fm_audio"] = case

    m, events, _ = capture()
    m.intercept(0x08008380, lambda e: events.append(["delay", "nominal_ms", 0, e.uc.reg_read(GPRS[0])]))
    rows = []
    for chip, entry in [("A", 0x08004FA0), ("B", 0x08003E3C)]:
        events.clear()
        m.call(entry)
        for step, event in enumerate(events):
            rows.append([chip, step, *event])
    save_csv("rx-start.csv", ["chip", "step", "kind", "target", "address", "value"], rows)
    counts["rx_start"] = 2

    gpio_rows = []
    for flag in (0, 1, 4, 0x8000, 0x8001, 0x8004, 0xffff):
        m, events, _ = capture()
        m.write(0x20001204, struct.pack("<H", flag))
        gpio_store(m, 0x0801F09A, 0x40021428, False, events)
        m.call(0x0801F084)
        gpio_rows.append({"fragment": "1f084", "flags_before": flag,
            "flags_after": int.from_bytes(m.read(0x20001204, 2), "little"), "events": events[:]})
        m, events, _ = capture()
        m.write(0x20001204, struct.pack("<H", flag))
        gpio_store(m, 0x080182DC, 0x40021418, True, events)
        stop_at(m, 0x080182DE)
        m.call(0x080182D0)
        gpio_rows.append({"fragment": "182d0..182de", "flags_before": flag,
            "flags_after": int.from_bytes(m.read(0x20001204, 2), "little"), "events": events[:]})
    for flag in (0, 1, 0x8000, 0xffff):
        m = StockArm(IMAGE)
        m.write(0x20001204, struct.pack("<H", flag))
        m.call(0x0801FC70, 4)
        gpio_rows.append({"fragment": "1fc70(4)", "flags_before": flag,
            "flags_after": int.from_bytes(m.read(0x20001204, 2), "little"), "events": []})
    (HERE / "pf8.json").write_text(json.dumps(gpio_rows, indent=2) + "\n", encoding="utf-8")
    counts["pf8_fragments"] = len(gpio_rows)

    clock_rows = []
    for clock in (24_000_000, 48_000_000, 96_000_000):
        m, events = StockArm(IMAGE), []
        m.word(0x20001914, clock)
        m.intercept(0x0802606C, lambda e: events.append(["clock_source_arg", e.uc.reg_read(GPRS[0])]))
        m.call(0x080251EC)
        scale_us = int.from_bytes(m.read(0x200010dc, 4), "little")
        scale_ms = int.from_bytes(m.read(0x200010e0, 4), "little")
        # Original delay8380 ->25224 remains intact. Exact MMIO instructions
        # only are intercepted. Reads supply core-clock CTRL, then COUNTFLAG.
        control = 4
        def systick(uc, pc, size, _):
            nonlocal control
            if pc == 0x0802524A:  # STR r2,[r3,#14]: LOAD
                assert uc.reg_read(GPRS[3]) == 0xe000e000
                events.append(["load", uc.reg_read(GPRS[2])])
            elif pc in (0x08025254, 0x08025288):
                assert uc.reg_read(GPRS[3]) == 0xe000e000
                events.append(["val", uc.reg_read(GPRS[2])])
            elif pc == 0x08025258:
                assert uc.reg_read(GPRS[2]) == 0xe000e000
                uc.reg_write(GPRS[2], control)
            elif pc == 0x08025266:
                assert uc.reg_read(GPRS[2]) == 0xe000e000
                uc.reg_write(GPRS[1], control | 0x10000)
            elif pc == 0x0802527A:
                assert uc.reg_read(GPRS[2]) == 0xe000e000
                uc.reg_write(GPRS[2], control)
            elif pc in (0x0802525E, 0x08025284):
                assert uc.reg_read(GPRS[3]) == 0xe000e000
                control = uc.reg_read(GPRS[2])
                events.append(["ctrl", control])
            else:
                raise AssertionError(hex(pc))
            uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
        for pc in (0x0802524A, 0x08025254, 0x08025258, 0x0802525E,
                   0x08025266, 0x0802527A, 0x08025284, 0x08025288):
            m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, systick, begin=pc, end=pc))
        m.call(0x08008380, 10)
        clock_rows.append({"clock_input_hz": clock, "us_scale": scale_us,
            "ms_scale": scale_ms, "delay_argument": 10, "events": events})
    (HERE / "delay-clock.json").write_text(json.dumps(clock_rows, indent=2) + "\n", encoding="utf-8")
    counts["nominal_delay_profiles"] = len(clock_rows)

    files = ["rssi.csv", "squelch-status.csv", "audio-mode.csv", "fm-audio.csv", "rx-start.csv", "pf8.json", "delay-clock.json"]
    summary = {"reference_sha256": REFERENCE_SHA256, "counts": counts,
        "hardware_accessed": False, "mmio_mapped": False,
        "hooks": ["A/B RF read/write helpers", "8380 RX-start delay argument capture",
            "PF8 stores1F09A/182DC", "stop7C50/518E/182DE",
            "2606C clock-source argument capture", "eight exact SysTick instruction PCs"],
        "limitations": ["related RT880 image, not exact UV98 MCU backup",
            "COUNTFLAG supplied immediately: no elapsed-time measurement",
            "no B RSSI caller proved", "audio/start are fragments, not complete cold RX startup"],
        "hashes": {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest() for name in files}}
    (HERE / "oracle-summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
