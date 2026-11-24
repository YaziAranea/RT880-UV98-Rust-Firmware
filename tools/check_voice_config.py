"""Read CPS-patched key settings with the actual VK03 Cortex-M4 accessor.

Exercises the private return ABI used by this build. Flash is changed only
inside the emulator; this does not emulate a radio boot or RF transmission.
"""
import argparse
import binascii
import hashlib
import json
from pathlib import Path
import struct

import unicorn
from unicorn import arm_const as arm
from check_diag_preinit import FLASH_BASE, FLASH_SIZE, RETURN_PC, read_elf, require
from check_voice_clip import RAM_BASE, RAM_SIZE, SLOT_SIZE, linked_thumb

MAGIC = b"UV98VOICECONF01\0"
SIZE = len(MAGIC) + 6


def record(key, reserved, delay):
    body = struct.pack("<BBH", key, reserved, delay)
    return MAGIC + body + struct.pack("<H", binascii.crc_hqx(body, 0xffff))


def check(path):
    raw, image, symbols = read_elf(path)
    symbol = symbols["UV98_VOICE_CONFIG"]
    offset = symbol["value"] - FLASH_BASE
    clip = symbols["UV98_VOICE_CLIP"]["value"] - FLASH_BASE
    require(symbol["size"] == SIZE and image.count(MAGIC) == 1 and
            image.find(MAGIC) == offset, "missing unique linked CPS settings record")
    require(clip + SLOT_SIZE <= offset or offset + SIZE <= clip, "clip/config overlap")
    require(image[offset:offset + SIZE] == record(255, 0, 0), "unexpected release defaults")
    entry = linked_thumb(symbols, "uv98_diag_firmware", "voice_clip", "6config")
    machine = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
    machine.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
    machine.mem_map(FLASH_BASE, FLASH_SIZE)
    machine.mem_write(FLASH_BASE, image)
    machine.mem_map(RAM_BASE, RAM_SIZE)
    cases = 0

    def read(data, expected):
        nonlocal cases
        machine.mem_write(symbol["value"], data)
        machine.reg_write(arm.UC_ARM_REG_SP, RAM_BASE + RAM_SIZE - 16)
        machine.reg_write(arm.UC_ARM_REG_LR, RETURN_PC | 1)
        machine.emu_start(entry, RETURN_PC, count=4000)
        require(machine.reg_read(arm.UC_ARM_REG_PC) == RETURN_PC, "accessor failed to return")
        value = machine.reg_read(arm.UC_ARM_REG_R0)
        tag = value & 255
        # Nested Option tag in r0: invalid config, no key, or digit + delay.
        if expected is None:
            require(tag == 2, "damaged record accepted")
        else:
            key, delay = expected
            require(tag == int(key != 255) and value >> 16 == delay and
                    (key == 255 or (value >> 8) & 255 == key), "wrong key/delay")
        cases += 1

    for key in (*range(10), 255):
        for delay in (0, 1, 437, 500, 2999, 3000):
            read(record(key, 0, delay), (key, delay))
    for key, reserved, delay in [(10, 0, 0), (37, 0, 1), (1, 1, 0), (1, 0, 3001)]:
        read(record(key, reserved, delay), None)
    for pos in range(SIZE):
        data = bytearray(record(2, 0, 500))
        data[pos] ^= 1
        read(bytes(data), None)
    return {"passed": True, "cases": cases, "elf_sha256": hashlib.sha256(raw).hexdigest(),
            "hardware_qualified": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = check(args.elf)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"PASS: {result['cases']} linked voice configuration reads")
