"""Run the stock RT880 name TX and RX popup with synthetic FIFO data."""
import argparse
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from stock_emu import (
    GPRS, StockArm, UC_ARM_REG_PC, UC_HOOK_CODE, UC_PROT_READ, UC_PROT_WRITE,
)

PROFILE = [
    (0x70, 0x00E0), (0x72, 0x3065), (0x58, 0x37C3),
    (0x5C, 0x5665), (0x5D, 0x0F00), (0x3F, 0x8000),
    (0x59, 0x8028), (0x59, 0x0028),
]


def arg(machine, index):
    return machine.uc.reg_read(GPRS[index])


def tx(image, payload):
    machine = StockArm(image)
    machine.uc.mem_map(0x40020000, 0x1000, UC_PROT_READ | UC_PROT_WRITE)
    machine.write(0x200035EC + 166, bytes([3]))
    machine.write(0x200035EC + 76, payload)
    writes = []
    machine.intercept(0x0801E660, lambda m: writes.append((arg(m, 0), arg(m, 1))))
    machine.intercept(0x0801E620, lambda m: int(arg(m, 0) == 0x0C))
    machine.intercept(0x08008380, lambda _: None)

    def stop_after_name(uc, *_):
        uc.reg_write(UC_ARM_REG_PC, machine.STOP)

    machine.uc.hook_add(UC_HOOK_CODE, stop_after_name, begin=0x08010388, end=0x08010388)
    machine.call(0x08010338)
    words = [word for reg, word in writes if reg == 0x5F]
    assert writes == PROFILE + [(0x5F, word) for word in words] + [
        (0x59, 0x2828), (0x02, 0), (0x70, 0), (0x58, 0),
        (0x3F, 0), (0x59, 0x0028), (0x30, 0),
    ], writes
    assert b"".join(word.to_bytes(2, "big") for word in words) == payload
    return words


def rx(image, words, crc_ok):
    machine = StockArm(image)
    words = iter(words)
    shown = []
    reads = []

    def read_register(m):
        register = arg(m, 0)
        reads.append(register)
        if register == 0x0B:
            return 0x0010 if crc_ok else 0
        assert register == 0x5F
        return next(words)

    def capture_popup(m):
        shown.append((m.read(arg(m, 2), 16), m.read(arg(m, 3), 16), arg(m, 1)))

    machine.intercept(0x0801E660, lambda _: None)
    machine.intercept(0x0801E620, read_register)
    machine.intercept(0x0800C830, capture_popup)
    machine.call(0x0801D990)
    assert reads == ([0x0B] + [0x5F] * 8 if crc_ok else [0x0B])
    return shown


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    args = parser.parse_args()

    for name in [b"AZ QX R0ABC", b"QRST UVWXYZ 0123"]:
        payload = name.ljust(16, bytes([0xFF]))
        words = tx(args.image, payload)
        assert rx(args.image, words, True) == [(b"MDC1200 Code:   ", payload, 200)]
        print("TX FIFO and RX popup:", " ".join(f"{word:04X}" for word in words), name)
    assert rx(args.image, (), False) == []
    print("CRC FAIL: no FIFO read or popup")


if __name__ == "__main__":
    main()
