"""Check linked Thumb machine outlines for a stale return address after BL.

Tracks the incoming return address through linked outlined calls and direct
outlined tail branches. Models straight-line register moves and word stack
saves/restores. Other control flow or unproved return paths are inconclusive.
This is a targeted check, not a proof of LLVM output or hardware behavior.
"""

import argparse
from hashlib import sha256
from io import BytesIO
from pathlib import Path
import struct
import sys

from check_diag_preinit import FLASH_BASE, capstone, cs_arm as arm, read_elf, require
from elftools.elf.elffile import ELFFile

LR, SP, PC = arm.ARM_REG_LR, arm.ARM_REG_SP, arm.ARM_REG_PC
REG, IMM, MEM = arm.ARM_OP_REG, arm.ARM_OP_IMM, arm.ARM_OP_MEM
ENTRY = object()


class Unproved(Exception):
    pass


def outlines(data, image):
    table = ELFFile(BytesIO(data)).get_section_by_name(".symtab")
    require(table is not None, "ELF needs its linked symbol table")
    decoder = capstone.Cs(capstone.CS_ARCH_ARM,
                          capstone.CS_MODE_THUMB | capstone.CS_MODE_MCLASS)
    decoder.detail = True
    entries, code = {}, {}
    for symbol in table.iter_symbols():
        name = symbol.name
        if not name.startswith("OUTLINED_FUNCTION_"):
            continue
        value, size = symbol["st_value"], symbol["st_size"]
        start = value & ~1
        at = start - FLASH_BASE
        require(symbol["st_info"]["type"] == "STT_FUNC" and value & 1
                and size > 0 and 0 <= at <= len(image) - size,
                f"{name}: invalid linked Thumb symbol")
        ops = list(decoder.disasm(image[at:at + size], start))
        require(sum(i.size for i in ops) == size, f"{name}: incomplete disassembly")
        entries[start] = name, ops
        for ins in ops:
            code[ins.address] = ins
    require(entries, "no linked machine outlines to verify")
    return entries, code


def stack_offset(regs, operand):
    # Return-address tokens are integers (linked addresses) or ENTRY. Stack
    # aliases are one-element tuples so an SP offset cannot become a link.
    base = regs.get(operand.mem.base)
    if isinstance(base, tuple) and operand.mem.index == arm.ARM_REG_INVALID:
        return base[0] + operand.mem.disp
    return None


def trace(start, code):
    regs = {LR: ENTRY, SP: (0,)}
    saved = {}
    returns = [ENTRY]
    calls = {}
    address = start

    def restore(actual, at):
        expected = returns[-1]
        if actual is not expected and actual != expected:
            if isinstance(actual, int) and actual in calls:
                outer = "entry LR" if expected is ENTRY else f"{expected:#010x}"
                return (f"{at:#010x} returns to {actual:#010x} from BL at "
                        f"{calls[actual]:#010x}, expected {outer}")
            raise Unproved(f"{at:#010x}: unknown alternate return")
        returns.pop()
        return None

    for _ in range(512):
        ins = code.get(address)
        if ins is None:
            raise Unproved(f"{address:#010x}: control leaves outlined code")
        at, next_pc = ins.address, ins.address + ins.size
        mnemonic = ins.mnemonic.removesuffix(".w").removesuffix(".n")
        operands = ins.operands
        if ins.cc not in (arm.ARM_CC_INVALID, arm.ARM_CC_AL) or mnemonic.startswith("it"):
            raise Unproved(f"{at:#010x}: conditional instruction")

        if mnemonic in ("bl", "b", "blx") and len(operands) == 1:
            if operands[0].type != IMM or mnemonic == "blx":
                raise Unproved(f"{at:#010x}: unsupported branch target")
            if mnemonic == "bl":
                regs[LR] = next_pc
                calls[next_pc] = at
                returns.append(next_pc)
            address = operands[0].imm
            continue

        returning = False
        actual = None
        if mnemonic == "bx" and len(operands) == 1 and operands[0].type == REG:
            returning, actual = True, regs.get(operands[0].reg)
        elif mnemonic in ("push", "pop") and operands and all(o.type == REG for o in operands):
            if SP not in regs or not isinstance(regs[SP], tuple):
                raise Unproved(f"{at:#010x}: unknown SP in {mnemonic}")
            sp = regs[SP][0]
            words = [o.reg for o in operands]
            if mnemonic == "push":
                sp -= 4 * len(words)
                for n, reg in enumerate(words):
                    saved[sp + 4 * n] = regs.get(reg)
            else:
                for n, reg in enumerate(words):
                    regs[reg] = saved.pop(sp + 4 * n, None)
                sp += 4 * len(words)
                if PC in words:
                    returning, actual = True, regs.pop(PC)
            regs[SP] = (sp,)
        elif mnemonic == "mov" and len(operands) == 2 and all(o.type == REG for o in operands):
            destination, source = (o.reg for o in operands)
            if destination == PC:
                returning, actual = True, regs.get(source)
            else:
                regs[destination] = regs.get(source)
        elif (mnemonic in ("add", "sub") and len(operands) in (2, 3)
              and operands[0].type == REG and operands[0].reg != PC and operands[-1].type == IMM
              and all(o.type == REG for o in operands[:-1])):
            delta = operands[-1].imm * (1 if mnemonic == "add" else -1)
            base = regs.get(operands[-2].reg)
            regs[operands[0].reg] = (base[0] + delta,) if isinstance(base, tuple) else None
        elif (mnemonic in ("ldr", "str") and len(operands) in (2, 3)
              and operands[0].type == REG and operands[1].type == MEM):
            reg, mem = operands[0].reg, operands[1]
            offset = stack_offset(regs, mem)
            if offset is not None and offset % 4:
                raise Unproved(f"{at:#010x}: unaligned stack word")
            if mnemonic == "str":
                if offset is None:
                    saved.clear()
                else:
                    saved[offset] = regs.get(reg)
            else:
                value = saved.get(offset)
                if reg == PC:
                    returning, actual = True, value
                else:
                    regs[reg] = value
            if ins.writeback:
                if len(operands) == 3:
                    require(operands[2].type == IMM, "invalid linked stack adjustment")
                    if offset is not None:
                        offset += operands[2].imm
                regs[mem.mem.base] = (offset,) if offset is not None else None
        else:
            _, writes = ins.regs_access()
            if PC in writes or mnemonic in ("cbz", "cbnz", "tbb", "tbh", "svc", "bkpt"):
                raise Unproved(f"{at:#010x}: unsupported control flow ({ins.mnemonic})")
            for reg in writes:
                regs.pop(reg, None)
            if (mnemonic.startswith(("str", "stm", "vstr", "vstm"))
                    or any(o.type == MEM and o.access & capstone.CS_AC_WRITE for o in operands)):
                saved.clear()

        if returning:
            problem = restore(actual, at)
            if problem or actual is ENTRY:
                return problem
            address = actual
        else:
            address = next_pc
    raise Unproved("outline call/branch budget exhausted")


def check(path):
    raw, image, _ = read_elf(path)
    entries, code = outlines(raw, image)
    candidates = [(start, name) for start, (name, ops) in entries.items()
                  if any(i.id in (arm.ARM_INS_BL, arm.ARM_INS_BLX) for i in ops)]
    print(f"ELF SHA-256 {sha256(raw).hexdigest()}")
    print(f"{len(entries)} linked Thumb outlines, {len(candidates)} with nested BL/BLX")
    failed, unproved = [], []
    for start, name in sorted(candidates):
        prefix = f"{name} at {start:#010x}"
        try:
            problem = trace(start, code)
        except Unproved as exc:
            unproved.append(f"{prefix}: {exc}")
        else:
            if problem:
                failed.append(f"{prefix}: {problem}")
    for problem in failed:
        print(f"FAIL: {problem}")
    for problem in unproved:
        print(f"INCONCLUSIVE: {problem}")
    if failed:
        return 1
    if unproved:
        return 2
    print("PASS: no stale linked return address in the checked outlines")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    args = parser.parse_args()
    try:
        return check(args.elf)
    except (AssertionError, OSError, struct.error, capstone.CsError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
