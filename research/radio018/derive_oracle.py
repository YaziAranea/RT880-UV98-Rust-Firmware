"""Bounded original Thumb RF calls; no hardware, MMIO, firmware boot or COM.

NOR serves a synthetic calibration block. RF registers are a coherent map.
Only named RF/NOR/delay helpers and exact GPIO stores are intercepted.
"""
from pathlib import Path
import csv
import hashlib
import json
import struct
import sys

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parents[1]
WORK = PROJECT.parent
sys.path.insert(0, str(WORK / "calibration-analysis"))
from stock_emu import StockArm, REFERENCE_SHA256, GPRS, UC_ARM_REG_PC, UC_HOOK_CODE

IMAGE = WORK / "reference-firmware/RT-880_Upgrade_V1_16_250915.nicsure.bin"


def calibration():
    block = bytearray([0xff] * 4096)
    block[0] = 0x9a
    block[8:12] = bytes([2, 3, 4, 1])
    for slot in range(15):
        rec = bytearray((slot * 17 + i * 3 + 5) & 255 for i in range(112))
        struct.pack_into("<H", rec, 0, 0x8000 + slot)
        block[32 + slot * 112:32 + (slot + 1) * 112] = rec
    return block


def setup():
    m, events, regs = StockArm(IMAGE), [], {}
    block = calibration()
    m.write(0x20003044, block[:32])
    m.write(0x2000081a, b"\xff")
    m.word(0x200010ec, 0x20005000)
    m.write(0x200035ec + 0xa4, b"\x05")
    m.write(0x200035ec + 0xad, b"\x10")
    m.write(0x200035ec + 0xb2, b"\x00")
    def nor(e):
        dest, address, length = [e.uc.reg_read(r) for r in GPRS[:3]]
        assert dest == 0x20003064 and length == 112
        assert address >= 32 and (address - 32) % 112 == 0 and address + length <= len(block)
        e.write(dest, block[address:address + length])
        return 0
    m.intercept(0x08025D30, nor)
    def write(e):
        reg, word = e.uc.reg_read(GPRS[0]), e.uc.reg_read(GPRS[1]) & 65535
        regs[reg] = word
        events.append(["write", "A", reg, word])
        return 0
    def read(e):
        reg = e.uc.reg_read(GPRS[0])
        value = regs.get(reg, 0x55a0)
        events.append(["read", "A", reg, value])
        return value
    m.intercept(0x0801E660, write)
    m.intercept(0x0801E620, read)
    m.intercept(0x08008380, lambda e: events.append(["delay", "ms", 0, e.uc.reg_read(GPRS[0])]))
    stores = {
        0x0800507c: (0x40021428, 1, "F", 0, 0),
        0x08005086: (0x40021418, 1, "F", 0, 1),
        0x08005320: (0x40020828, 0x8000, "C", 15, 0),
        0x08005326: (0x40020428, 0x0800, "B", 11, 0),
        0x0800535c: (0x40020828, 0x8000, "C", 15, 0),
        0x08005362: (0x40020428, 0x0800, "B", 11, 0),
        0x08005392: (0x40020818, 0x8000, "C", 15, 1),
        0x08005398: (0x40020428, 0x0800, "B", 11, 0),
        0x080053a4: (0x40020818, 0x8000, "C", 15, 1),
        0x080053ac: (0x40020418, 0x0800, "B", 11, 1),
        0x08005430: (0x40021428, 1, "F", 0, 0),
        0x0800543a: (0x40021418, 1, "F", 0, 1),
    }
    def gpio(uc, pc, size, _):
        address, word, port, pin, high = stores[pc]
        assert size == 2 and uc.reg_read(GPRS[0]) == word and uc.reg_read(GPRS[1]) == address, hex(pc)
        events.append(["gpio", port, pin, high])
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
    for pc in stores:
        m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, gpio, begin=pc, end=pc))
    return m, events, regs, block


def save(name, header, rows):
    with (HERE / name).open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(header)
        w.writerows(rows)


def am_agc():
    m, events, regs, _ = setup()
    m.write(0x20000810, b"\x01")
    m.write(0x20001125, b"\x02")
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
        lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
        begin=0x080242ac, end=0x080242ac))
    inputs = {(raw, 90, 142, 0) for raw in range(512)}
    inputs.update((raw, index, previous, hold)
        for raw in (0, 128, 129, 130, 141, 142, 143, 160, 162, 180, 511)
        for index in (1, 2, 45, 90, 96)
        for previous in (0, 1, 142, 511)
        for hold in (0, 1, 250))
    rows = []
    for raw, index, previous, hold in sorted(inputs):
        m.word(0x200018d4, index)
        m.write(0x200018dc, struct.pack("<H", previous))
        m.write(0x200018de, bytes([hold]))
        regs[0x67] = raw
        events.clear()
        m.call(0x08024140, count=5000)
        assert len(events) == 2 and events[0][:3] == ["read", "A", 0x67] and events[1][:3] == ["write", "A", 0x13]
        rows.append([raw, index, previous, hold,
            int.from_bytes(m.read(0x200018d4, 4), "little"),
            int.from_bytes(m.read(0x200018dc, 2), "little"), m.read(0x200018de, 1)[0],
            events[1][3], int.from_bytes(m.read(0x200018e0, 2), "little", signed=True)])
    save("am-agc.csv", ["raw", "index", "previous", "hold", "next_index", "next_previous", "next_hold", "reg13", "rssi_correction"], rows)
    data = IMAGE.read_bytes()
    table = data[0x0802ead8 - 0x08002800:0x0802ead8 - 0x08002800 + 97 * 3]
    save("am-gain-table.csv", ["index", "reg13", "gain"],
        [[i, int.from_bytes(table[i*3:i*3+2], "little"), int.from_bytes(table[i*3+2:i*3+3], "little", signed=True)] for i in range(97)])
    return len(rows)


def tone_status():
    m, events, regs, _ = setup()
    m.write(0x20001125, b"\x01")  # acquisition; no active-RX IRQ callback
    rows = []
    for kind in range(4):
        m.write(0x20005005, bytes([kind << 4]))
        for upper in range(256):
            value = upper << 8
            regs[0x0c] = value
            events.clear()
            result = m.call(0x08004444)[0]
            assert events == [["read", "A", 0x0c, value]]
            rows.append([kind << 12, value, result])
    save("tone-status.csv", ["tone", "reg0c", "status"], rows)
    return len(rows)


def main():
    counts = {}
    m, events, regs, _ = setup()
    rows = []
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
        lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
        begin=0x08004e62, end=0x08004e62))
    for index in (0, 1, 2, 3, 4, 7, 255):
        m.write(0x20003044 + 8, bytes([index] * 4))
        events.clear()
        m.call(0x08004dc0)
        rows.extend([index, step, *event] for step, event in enumerate(events))
    save("cold-prefix.csv", ["filter_index", "step", "kind", "target", "register", "value"], rows)
    counts["cold_prefix"] = 7

    if "--skip-tones" not in sys.argv:
        m, events, regs, _ = setup()
        rows = []
        for raw in [0, 0x129e, 0x1302, 0x19f5, 0x1fff, *range(0x2000, 0x2200), *range(0x3000, 0x3200)]:
            for mode in range(4):
                events.clear()
                m.call(0x08004388, raw >> 12, raw & 0xfff, mode)
                rows.extend([raw, mode, step, *event] for step, event in enumerate(events))
        save("tone.csv", ["raw", "mode", "step", "kind", "target", "register", "value"], rows)
    counts["tone"] = 1029 * 4

    rows, case = [], 0
    for freq in (1_800_000, 3_200_000, 14_550_000, 26_000_000, 43_020_000, 84_000_000, 130_000_000):
        for width in (0, 1):
            for raw, mode, option, power in ((0, 0, 0, 0), (0x1302, 0, 0, 1), (0x31ec, 1, 1, 2)):
                m, events, regs, _ = setup()
                regs[0x33] = 0x5540  # PA/RX route requests already clear
                ch = bytearray(48)
                ch[3], ch[16], ch[18], ch[19] = width, power, mode << 5, option
                struct.pack_into("<I", ch, 10, freq)
                struct.pack_into("<H", ch, 14, raw)
                m.write(0x20005000, ch)
                m.call(0x080053f0)
                rows.extend([case, freq, width, raw, mode, option, power, step, *event]
                    for step, event in enumerate(events))
                case += 1
    save("tx-body.csv", ["case", "frequency", "width", "tone", "tone_mode", "option", "power", "step", "kind", "target", "register", "value"], rows)
    counts["tx_body"] = case
    m, events, regs, _ = setup()
    m.call(0x08004dc0)
    save("cold-full.csv", ["step", "kind", "target", "register", "value"],
        [[step, *e] for step, e in enumerate(events)])
    counts["cold_full"] = 1

    rows, case = [], 0
    for freq in (1_800_000, 14_550_000, 43_350_000, 84_000_000):
        for width in (0, 1):
            for raw, mode, level, delay, modulation in ((0, 0, 0, 0, 0), (0x1302, 0, 5, 7, 1), (0x31ec, 1, 10, 255, 0)):
                m, events, regs, _ = setup()
                regs[0x33] = 0x5540
                ch = bytearray(48)
                ch[0], ch[3], ch[18] = modulation, width, mode << 5
                struct.pack_into("<I", ch, 6, freq)
                struct.pack_into("<H", ch, 4, raw)
                m.write(0x20005000, ch)
                m.write(0x200035ec + 0xa4, bytes([level]))
                m.write(0x200035ec + 0xb2, bytes([delay]))
                # Gain039C0 then actual primary caller1F842..1F84A order.
                m.call(0x080039c0)
                m.call(0x08004228, width)
                m.call(0x08004518)
                m.call(0x08004bf0)
                rows.extend([case, freq, width, raw, mode, level, delay, modulation, step, *e]
                    for step, e in enumerate(events))
                case += 1
    save("rx-body.csv", ["case", "frequency", "width", "tone", "tone_mode", "level", "squelch_delay", "modulation", "step", "kind", "target", "register", "value"], rows)
    counts["rx_body"] = case

    rows, case = [], 0
    for modulation in (0, 1):
        for gain, dac in ((0, 0), (3, 4), (63, 15), (255, 255)):
            for reg01 in (0, 2, 6, 7, 10, 65535):
                m, events, regs, _ = setup()
                m.write(0x20005000, bytes([modulation]))
                m.write(0x200035ec + 0xae, bytes([gain, dac, gain, dac]))
                regs[1] = reg01
                m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
                    lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
                    begin=0x0800518e, end=0x0800518e))
                m.call(0x080050ea)
                rows.extend([case, modulation, gain, dac, reg01, step, *e]
                    for step, e in enumerate(events))
                case += 1
    save("audio.csv", ["case", "modulation", "gain", "dac", "reg01", "step", "kind", "target", "register", "value"], rows)
    counts["audio"] = case
    counts["am_agc"] = am_agc()
    counts["tone_status"] = tone_status()
    (HERE / "synthetic-calibration.hex").write_text(calibration().hex() + "\n", encoding="ascii")
    files = ["cold-prefix.csv", "tone.csv", "tx-body.csv", "cold-full.csv", "rx-body.csv", "audio.csv", "am-agc.csv", "am-gain-table.csv", "tone-status.csv", "synthetic-calibration.hex"]
    summary = {"reference_sha256": REFERENCE_SHA256, "counts": counts,
        "hardware_accessed": False, "mmio_mapped": False,
        "hooks": ["A register read/write", "NOR read25D30 synthetic112B", "delay8380 nominal argument", "12 exact GPIO stores", "cold stop4E62", "audio stop518E", "AM AGC stop242AC before UI"],
        "scope": "Original cold prefix04DC0..04E62 and full04DC0; complete tone4388 including original DCS9920/994C; complete ordinary TX053F0..55C2; RX039C0+4228+4518+full4BF0, width/reset/body order from1F842..1F84A; audio50EA..518E; AM AGC24140..242AC; tone status4444 with acquisition state1125=1. RF register reset not modelled, unused reads seeded55A0; ordinary branch flags zero except named AM/status fixture states.",
        "hashes": {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest() for name in files}}
    (HERE / "oracle-summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
