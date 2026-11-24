"""Execute the voice-slot accessor and IMA decoder in the linked Cortex-M4 ELF.

Requires an unstripped voice-keyer image with the R55 Rust Thumb ABI. The test
edits mapped flash in Unicorn only. It establishes no DAC, PTT or RF behavior.
"""
import argparse
import binascii
import hashlib
import json
from pathlib import Path
import struct

from check_diag_preinit import FLASH_BASE, FLASH_SIZE, RETURN_PC, read_elf, require
import unicorn
from unicorn import arm_const as arm


MAGIC = b"UV98VOICECLIP01\0"
MAX_FRAMES = 6000
HEADER_SIZE = 20
SLOT_SIZE = HEADER_SIZE + MAX_FRAMES // 2
RAM_BASE, RAM_SIZE = 0x20000000, 0xC000
RESULT = RAM_BASE + 0x100
STACK_TOP = RAM_BASE + RAM_SIZE - 16

# Standard IMA/DVI ADPCM step and index tables. The slot starts at predictor
# and index zero, and the DAC consumes a midpoint then the decoded endpoint.
STEPS = (
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45,
    50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230,
    253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963,
    1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024,
    3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623,
    27086, 29794, 32767,
)
INDEX_DELTAS = (-1, -1, -1, -1, 2, 4, 6, 8)


def reference_dac(slot):
    crc, frames = struct.unpack_from("<HH", slot, len(MAGIC))
    require(1 <= frames <= MAX_FRAMES, "reference slot length is invalid")
    end = HEADER_SIZE + (frames + 1) // 2
    require(binascii.crc_hqx(slot[18:end], 0xFFFF) == crc, "reference slot CRC is invalid")
    predictor = index = 0
    output = bytearray()
    for frame in range(frames):
        code = (slot[HEADER_SIZE + frame // 2] >> (4 * (frame % 2))) & 15
        step = STEPS[index]
        difference = step >> 3
        for bit in range(3):
            if code & (1 << bit):
                difference += step >> (2 - bit)
        previous = predictor
        predictor = max(-32768, min(32767, previous + (-difference if code & 8 else difference)))
        index = max(0, min(88, index + INDEX_DELTAS[code & 7]))
        midpoint = int((previous + predictor) / 2)  # Rust truncates toward zero.
        output.extend(((midpoint + 32768) >> 8, (predictor + 32768) >> 8))
    return bytes(output)


def replacement_slot():
    # Odd count, mixed signs and both nibbles. Skip the last high nibble.
    codes = bytes.fromhex("77777777ffffff8012375cbd9e2f40ff")
    body = struct.pack("<H", len(codes) * 2 - 1) + codes
    crc = binascii.crc_hqx(body, 0xFFFF)
    return (MAGIC + struct.pack("<H", crc) + body).ljust(SLOT_SIZE, b"\0")


def linked_thumb(symbols, *parts):
    matches = [s for name, s in symbols.items()
               if s["info"] & 15 == 2 and all(part in name for part in parts)]
    require(len(matches) == 1, f"expected one linked Thumb function matching {parts}")
    symbol = matches[0]
    entry = symbol["value"]
    start = entry & ~1
    require(entry & 1 and FLASH_BASE <= start < start + symbol["size"] <= RETURN_PC,
            f"function {parts} is outside application flash or not Thumb")
    return entry


def run_case(name, image, slot_address, accessor, decoder, expected):
    cpu = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
    cpu.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
    cpu.mem_map(FLASH_BASE, FLASH_SIZE)
    cpu.mem_write(FLASH_BASE, image)
    cpu.mem_protect(FLASH_BASE, FLASH_SIZE, unicorn.UC_PROT_READ | unicorn.UC_PROT_EXEC)
    cpu.mem_map(RAM_BASE, RAM_SIZE, unicorn.UC_PROT_READ | unicorn.UC_PROT_WRITE)
    cpu.mem_write(RESULT, b"\xff" * 32)
    cpu.reg_write(arm.UC_ARM_REG_SP, STACK_TOP)

    def call(entry, limit):
        cpu.reg_write(arm.UC_ARM_REG_R0, RESULT)
        cpu.reg_write(arm.UC_ARM_REG_LR, RETURN_PC | 1)
        cpu.emu_start(entry, RETURN_PC, count=limit)
        require(cpu.reg_read(arm.UC_ARM_REG_PC) == RETURN_PC,
                f"{name}: function at {entry:#x} failed to return")
        require(cpu.reg_read(arm.UC_ARM_REG_SP) == STACK_TOP, f"{name}: stack imbalance")

    # R55's compiled Option<Samples> has an indirect result, using the null
    # data pointer for None. Iterator::next returns its Option<u8> in r0/r1.
    call(accessor, 350_000)
    pointer, = struct.unpack("<I", cpu.mem_read(RESULT, 4))
    if expected is None:
        require(pointer == 0, f"{name}: machine accessor accepted damaged slot")
        return {"name": name, "accepted": False, "passed": True}

    require(pointer == slot_address + HEADER_SIZE, f"{name}: iterator does not use mapped slot")
    actual = bytearray()
    for index, sample in enumerate(expected):
        call(decoder, 1000)
        require(cpu.reg_read(arm.UC_ARM_REG_R0) == 1, f"{name}: early EOF at DAC sample {index}")
        value = cpu.reg_read(arm.UC_ARM_REG_R1) & 255
        require(value == sample, f"{name}: DAC sample {index} is {value}, expected {sample}")
        actual.append(value)
    call(decoder, 1000)
    require(cpu.reg_read(arm.UC_ARM_REG_R0) == 0, f"{name}: decoder ran past end of clip")
    return {"name": name, "accepted": True, "passed": True, "dac_samples": len(actual),
            "dac_sha256": hashlib.sha256(actual).hexdigest()}


def check(path):
    data, image, symbols = read_elf(path)
    slot = symbols.get("UV98_VOICE_CLIP")
    require(slot is not None and slot["size"] == SLOT_SIZE, "missing voice slot of expected size")
    address = slot["value"]
    require(FLASH_BASE <= address <= RETURN_PC - SLOT_SIZE, "voice slot outside application flash")
    offset = address - FLASH_BASE
    original = image[offset:offset + SLOT_SIZE]
    require(original.startswith(MAGIC) and image.count(MAGIC) == 1, "voice slot magic differs")
    accessor = linked_thumb(symbols, "uv98_diag_firmware", "voice_clip", "7samples")
    decoder = linked_thumb(symbols, "uv98_voice_clip", "Samples", "Iterator", "4next")
    replacement = replacement_slot()
    original_dac = reference_dac(original)
    replacement_dac = reference_dac(replacement)
    require(original_dac != replacement_dac, "replacement must change decoded samples")
    damaged = bytearray(replacement)
    damaged[HEADER_SIZE] ^= 1
    cases = []
    for name, content, expected in (("original", original, original_dac),
                                    ("personalized", replacement, replacement_dac),
                                    ("bad_crc", bytes(damaged), None)):
        memory = image[:offset] + content + image[offset + SLOT_SIZE:]
        cases.append(run_case(name, memory, address, accessor, decoder, expected))
    return {"passed": True, "elf_sha256": hashlib.sha256(data).hexdigest(),
            "slot_address": hex(address), "slot_bytes": SLOT_SIZE, "cases": cases,
            "hardware_qualified": False, "device_accessed": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = check(args.elf)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print("PASS: linked ARM decodes original and personalized voice slots, rejects bad CRC")
