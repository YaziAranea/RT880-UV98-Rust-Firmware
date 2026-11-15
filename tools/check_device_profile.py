"""Execute the linked calibration-digest accessor before and after personalization.

This checks actual ARM instructions, not a source-text assertion. It does not
access a radio or establish compatibility of another PCB/calibration set.
"""
import argparse
import hashlib
import json
from pathlib import Path

from check_diag_preinit import FLASH_BASE, FLASH_SIZE, RETURN_PC, read_elf, require
import unicorn
from unicorn import arm_const as arm

MAGIC = b"UV98CALPROFILE1\0"


def check(path):
    data, image, symbols = read_elf(path)
    profile = symbols.get("UV98_CALIBRATION_PROFILE")
    require(profile is not None and profile["size"] == 48, "missing 48-byte profile")
    offset = profile["value"] - FLASH_BASE
    require(image[offset:offset + 16] == MAGIC, "profile magic differs")
    require(image.count(MAGIC) == 1, "profile magic is not unique")
    accessors = [s for name, s in symbols.items() if "expected_calibration_digest" in name]
    require(len(accessors) == 1, "expected one linked calibration accessor")
    original = image[offset + 16:offset + 48]
    replacement = hashlib.sha256(b"synthetic-public-profile-execution-check").digest()
    cases = []
    for name, digest in (("original", original), ("personalized", replacement)):
        memory = bytearray(image)
        memory[offset + 16:offset + 48] = digest
        cpu = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
        cpu.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
        cpu.mem_map(FLASH_BASE, FLASH_SIZE)
        cpu.mem_write(FLASH_BASE, bytes(memory))
        cpu.mem_map(0x20000000, 0xc000)
        cpu.mem_write(0x20000100, b"\xff" * 32)
        cpu.reg_write(arm.UC_ARM_REG_SP, 0x2000bff0)
        cpu.reg_write(arm.UC_ARM_REG_R0, 0x20000100)
        cpu.reg_write(arm.UC_ARM_REG_LR, RETURN_PC | 1)
        cpu.emu_start(accessors[0]["value"] | 1, RETURN_PC, count=10000)
        require(cpu.reg_read(arm.UC_ARM_REG_PC) == RETURN_PC, "accessor did not return")
        actual = bytes(cpu.mem_read(0x20000100, 32))
        require(actual == digest, f"{name}: digest was folded or read incorrectly")
        cases.append({"name": name, "passed": True})
    return {"passed": True, "elf_sha256": hashlib.sha256(data).hexdigest(),
            "profile_address": hex(profile["value"]), "digest_bytes": 32,
            "cases": cases, "hardware_qualified": False, "device_accessed": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = check(args.elf)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print("PASS: linked ARM reads original and personalized calibration digest; no device accessed")
