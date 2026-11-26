"""Check diagnostic ARM memmove wrappers against Python and the linked baseline.

Requires Capstone and Unicorn, as do the other diagnostic ELF checks. Counts
executed Cortex-M4 instructions. These counts do not measure hardware cycles.
"""

import argparse
import hashlib
from pathlib import Path

from check_diag_preinit import FLASH_BASE, FLASH_SIZE, RETURN_PC, arm, read_elf, unicorn


ARENA = 0x20000000
ARENA_SIZE = 0x7000
STACK = ARENA + 0x9000
INITIAL = bytes(((i * 139) ^ (i >> 4) ^ (i >> 8)) & 0xFF for i in range(ARENA_SIZE))
CALLEE_REGISTERS = (arm.UC_ARM_REG_R4, arm.UC_ARM_REG_R5, arm.UC_ARM_REG_R6,
                    arm.UC_ARM_REG_R7, arm.UC_ARM_REG_R8, arm.UC_ARM_REG_R9,
                    arm.UC_ARM_REG_R10, arm.UC_ARM_REG_R11)
CALLEE_VALUES = tuple(0x11447700 + i * 0x11111 for i in range(len(CALLEE_REGISTERS)))


class Probe:
    def __init__(self, path, strict):
        raw, image, self.symbols = read_elf(path)
        self.sha256 = hashlib.sha256(raw).hexdigest()
        self.strict = strict
        self.cpu = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
        self.cpu.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
        self.cpu.mem_map(FLASH_BASE, FLASH_SIZE)
        self.cpu.mem_write(FLASH_BASE, image)
        self.cpu.mem_map(ARENA, 0x10000)
        self.cpu.hook_add(unicorn.UC_HOOK_MEM_READ | unicorn.UC_HOOK_MEM_WRITE,
                          self.access, begin=ARENA, end=ARENA + 0xFFFF)
        self.cpu.hook_add(unicorn.UC_HOOK_CODE, self.count)
        self.instructions = 0
        self.counting = False

    def access(self, _cpu, mode, address, size, _value, _context):
        if STACK - 96 <= address and address + size <= STACK:
            return
        if mode == unicorn.UC_MEM_WRITE:
            assert self.destination <= address and address + size <= self.destination + self.length
        elif self.strict:
            assert self.source <= address and address + size <= self.source + self.length

    def count(self, _cpu, _address, _size, _context):
        if self.counting:
            self.instructions += 1

    def call(self, name, r0, r1, r2, count=False):
        cpu = self.cpu
        cpu.mem_write(ARENA, INITIAL)
        address = self.symbols[name]["value"]
        assert address & 1
        for reg, value in zip(CALLEE_REGISTERS, CALLEE_VALUES):
            cpu.reg_write(reg, value)
        for reg, value in [(arm.UC_ARM_REG_SP, STACK), (arm.UC_ARM_REG_LR, RETURN_PC | 1),
                           (arm.UC_ARM_REG_R0, r0), (arm.UC_ARM_REG_R1, r1),
                           (arm.UC_ARM_REG_R2, r2)]:
            cpu.reg_write(reg, value)
        self.instructions = 0
        self.counting = count
        cpu.emu_start(address, RETURN_PC, count=250_000)
        assert cpu.reg_read(arm.UC_ARM_REG_PC) == RETURN_PC
        assert cpu.reg_read(arm.UC_ARM_REG_SP) == STACK
        assert tuple(cpu.reg_read(reg) for reg in CALLEE_REGISTERS) == CALLEE_VALUES

    def run(self, name, destination, source, length, count=False, check_bytes=True):
        self.source, self.destination, self.length = source, destination, length
        self.call(name, destination, source, length, count=count)
        if self.strict:
            assert self.cpu.reg_read(arm.UC_ARM_REG_R0) == destination
        expected = bytearray(INITIAL)
        if length:
            d, s = destination - ARENA, source - ARENA
            expected[d:d + length] = INITIAL[s:s + length]
        matches = self.cpu.mem_read(ARENA, ARENA_SIZE) == expected
        if check_bytes:
            assert matches
        return self.instructions, matches


def shared_aeabi(probe):
    lengths = (*range(33), 63, 64, 65, 127, 128, 129, 255, 256, 511, 512, 4096)
    checked = 0
    for length in lengths:
        for entry, alignment in (("memclr", 1), ("memclr4", 4), ("memclr8", 8),
                                 ("memset", 1), ("memset4", 4), ("memset8", 8), ("memcpy8", 8)):
            for offset in range(0, 16, alignment):
                dest = ARENA + 0x100 + offset
                source = ARENA + 0x4000
                for byte in ((0, 0xA5, 0xFF) if entry.startswith("memset") else (0,)):
                    expected = bytearray(INITIAL)
                    is_copy = entry == "memcpy8"
                    probe.destination, probe.source, probe.length = dest, source if is_copy else dest, length
                    if is_copy:
                        probe.call("__wrap___aeabi_" + entry, dest, source, length)
                        payload = INITIAL[source - ARENA:source - ARENA + length]
                    else:
                        probe.call("__wrap___aeabi_" + entry, dest, length, byte if entry.startswith("memset") else 0xDE)
                        payload = bytes([byte]) * length
                    expected[dest - ARENA:dest - ARENA + length] = payload
                    assert probe.cpu.mem_read(ARENA, ARENA_SIZE) == expected, (entry, length, offset, byte)
                    checked += 1
    return checked


def correctness(probe, names):
    main, aligned4, aligned8 = names
    cases = [(main, 0xFFFFFFF0, 0, 0)]
    lengths = (*range(34), 63, 64, 65, 127, 255, 256, 257, 511, 512, 513)
    for length in lengths:
        for alignment in range(4):
            source = ARENA + 0x3000 + alignment
            for delta in (-19, -16, -7, -4, -3, -2, -1, 0, 1, 2, 3, 4, 7, 16, 19):
                cases.append((main, source + delta, source, length))
            cases.append((aligned4, source - alignment - 4, source - alignment, length))
            cases.append((aligned8, source - alignment + 8, source - alignment, length))
        for dst_align in range(4):
            for src_align in range(4):
                low = ARENA + 0x440
                high = ARENA + 0x4000
                cases.append((main, low + dst_align, high + src_align, length))
                cases.append((main, high + dst_align, low + src_align, length))
    for length in (1023, 2048, 4095, 4096, 4097, 8193):
        source = ARENA + 0x3000
        for delta in (-0x3000, -17, -16, -1, 0, 1, 16, 17, 0x1000):
            cases.append((main, source + delta, source, length))
    for case in cases:
        try:
            probe.run(*case)
        except (AssertionError, unicorn.UcError) as error:
            name, dst, src, length = case
            raise AssertionError(f"{name} dst={dst:#x} src={src:#x} len={length}") from error
    return len(cases)


def benchmark(baseline, candidate):
    source = ARENA + 0x3000
    for length in (32, 256, 4096, 8193):
        for name, offset, shift in [("aligned overlap forward", -16, 0),
                                    ("aligned overlap backward", 16, 0),
                                    ("same misalignment", 16, 1),
                                    ("mixed alignment forward", -3, 0),
                                    ("mixed alignment backward", 3, 0),
                                    ("disjoint forward", -0x3000, 0)]:
            dst = source + shift + offset
            old, old_matches = baseline.run("__aeabi_memmove", dst, source + shift, length,
                                            count=True, check_bytes=False)
            new, _ = candidate.run("__wrap___aeabi_memmove", dst, source + shift, length, count=True)
            baseline_note = " (baseline bytes differ)" if not old_matches else ""
            print(f"{length:4} bytes, {name:25}: {old:6} -> {new:6} instructions{baseline_note}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--check-shared-aeabi", action="store_true")
    args = parser.parse_args()
    new = Probe(args.candidate, strict=True)
    wrappers = tuple("__wrap___aeabi_memmove" + suffix for suffix in ("", "4", "8"))
    assert len({new.symbols[name]["value"] for name in wrappers}) == 1
    assert not any(name.startswith("__aeabi_memmove") or
                   "compiler_builtins3mem7memmove" in name for name in new.symbols)
    print(f"candidate {new.sha256}")
    print(f"linked wrapper: {new.symbols[wrappers[0]]['size']} bytes")
    print(f"{correctness(new, wrappers)} overlap/alignment/length, bounds and AAPCS cases passed")
    if args.check_shared_aeabi:
        print(f"{shared_aeabi(new)} shared AEABI fill/clear/copy cases passed")
    if args.baseline:
        old = Probe(args.baseline, strict=False)
        for name in ("__aeabi_memcpy", "__aeabi_memcpy4"):
            assert old.symbols[name]["size"] == new.symbols[name]["size"]
        if "__wrap___aeabi_memcpy8" not in new.symbols:
            assert old.symbols["__aeabi_memcpy8"]["size"] == new.symbols["__aeabi_memcpy8"]["size"]
        print(f"baseline {old.sha256}")
        benchmark(old, new)
