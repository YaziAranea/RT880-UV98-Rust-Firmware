"""Check the linked voice image's transition from WDT setup to IRQ service.

Requires Capstone and Unicorn. Models selected startup blocks and a direct
TMR2 handler dispatch with RAM-backed MMIO. Pin changes require re-auditing the
linked blocks. This does not boot the image or model the NVIC, DAC or radio.
"""

import argparse
from hashlib import sha256
from pathlib import Path
import re
import struct
import sys

from check_diag_preinit import (
    FLASH_BASE,
    RETURN_PC,
    arm,
    capstone,
    read_elf,
    require,
    unicorn,
)


WDT_CMD = 0x40003000
TMR2_ISTS = 0x40000010
DAC2_DATA = 0x4000741C
RAM = 0x20000000
VTOR = 0x08002800
TMR2_IRQ = 28


def instructions(decoder, image, symbol):
    address = symbol["value"] & ~1
    size = symbol["size"]
    require(symbol["value"] & 1 and size > 0, "expected a sized Thumb function")
    require(FLASH_BASE <= address < address + size <= RETURN_PC,
            "function outside application flash")
    start = address - FLASH_BASE
    return list(decoder.disasm(image[start:start + size], address))


def call_to(instruction, symbol):
    return (instruction.mnemonic == "bl"
            and instruction.op_str == f"#{symbol['value'] & ~1:#x}")


def unique(matches, label):
    matches = list(matches)
    require(len(matches) == 1, f"expected one {label}, found {len(matches)}")
    return matches[0]


def startup_sites(image, symbols):
    decoder = capstone.Cs(capstone.CS_ARCH_ARM,
                          capstone.CS_MODE_THUMB | capstone.CS_MODE_MCLASS)
    decoder.skipdata = True
    wrapper = instructions(decoder, image, symbols["main"])
    main = unique((symbol for name, symbol in symbols.items()
                   if name.endswith("___cortex_m_rt_main")
                   and any(call_to(i, symbol) for i in wrapper)), "Rust main")
    ops = instructions(decoder, image, main)
    decoded = [(i.mnemonic, i.op_str) for i in ops]

    # Independent WDT feed, start and relock in the inlined successful setup.
    # The allocator may keep WDT_CMD in a low or high register. Require the
    # same word store three times, with the three exact command values.
    def wdt_block(n):
        block = decoded[n:n + 6]
        if block[::2] != [("movw", "r0, #0xaaaa"), ("movw", "r0, #0xcccc"),
                           ("movs", "r0, #0")]:
            return None
        store, operands = block[1]
        match = re.fullmatch(r"r0, \[(r[1-8]|sb|sl|fp|ip)\]", operands)
        if store in ("str", "str.w") and match and block[1::2] == [block[1]] * 3:
            return getattr(arm, "UC_ARM_REG_" + match[1].upper())
        return None

    wdt, wdt_register = unique(((n, reg) for n in range(len(ops) - 5)
                                if (reg := wdt_block(n)) is not None), "WDT key block")
    mask = unique((n for n, i in enumerate(ops[:wdt])
                   if 0 < ops[wdt].address - i.address < 0x500
                   and call_to(i, symbols["__cpsid"])
                   and decoded[n + 1:n + 3] == [
                       ("mrs", "r0, ipsr"), ("mrs", "r1, control"),
                   ]), "WDT entry mask followed by privilege checks")

    # The old image enables interrupts before WDT setup, at this main-level
    # call site. The immediately preceding stores distinguish it from a
    # PRIMASK restore inside a critical section.
    enable = symbols["__cpsie"]
    early = [n for n, i in enumerate(ops[:mask])
             if 0 < ops[mask].address - i.address < 0x500 and call_to(i, enable)]
    require(len(early) <= 1, "ambiguous pre-watchdog IRQ enable")
    if early:
        require(decoded[early[0] - 2:early[0]] == [
            ("movw", "r0, #0x1ff"), ("strh.w", "r0, [r1, #0x5ab]"),
        ], "unrecognized pre-watchdog IRQ enable")

    # checked(..., 0xE053) branches to the fatal block on an unsuccessful
    # result. Stop at the shared first instruction of normal initialization.
    check = unique((n for n in range(wdt + 6, wdt + 40)
                    if decoded[n:n + 2] == [
                        ("mvns", "r0, r1"), ("lsls", "r0, r0, #0x18"),
                    ] and ops[n + 2].mnemonic == "bne"), "WDT result check")
    continuation = check + 3
    late = ops[continuation] if call_to(ops[continuation], enable) else None
    if late is not None:
        continuation += 1
    require(not (early and late), "global IRQ enable appears both before and after WDT")
    # Normal initialization starts by filling an object on the main stack.
    # Its offset changes with the size of the application state.
    require(ops[continuation].mnemonic in ("add", "add.w", "addw")
            and re.fullmatch(r"r1, sp, #(?:0x[0-9a-f]+|[0-9]+)",
                             ops[continuation].op_str)
            and decoded[continuation + 1] == ("mov.w", "r0, #-1")
            and any(i.mnemonic in ("str", "str.w") and i.op_str.startswith("r0, [r1")
                    for i in ops[continuation + 2:continuation + 12]),
            "unrecognized normal startup continuation")
    fatal = int(ops[check + 2].op_str.removeprefix("#"), 0)
    require(any(fatal <= i.address < fatal + 0x20
                and (i.mnemonic, i.op_str) == ("movw", "r1, #0xe053") for i in ops),
            "unrecognized watchdog error target")
    return ([ops[n] for n in early], ops[mask], ops[wdt], ops[wdt + 6],
            wdt_register, ops[check], fatal, late, ops[continuation])


def emulate(image, symbols):
    early, mask, keys, after_keys, wdt_register, check, fatal, late, normal = startup_sites(image, symbols)
    require(symbols["__vector_table"]["value"] == VTOR, "unexpected VTOR address")
    vector = struct.unpack_from("<I", image, VTOR - FLASH_BASE + 4 * (16 + TMR2_IRQ))[0]
    require(vector & 1 and vector == symbols["TMR2"]["value"]
            and vector != symbols["DefaultHandler"]["value"], "wrong TMR2 vector")

    cpu = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
    cpu.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
    cpu.mem_map(FLASH_BASE, len(image))
    cpu.mem_write(FLASH_BASE, image)
    for address in (RAM, WDT_CMD, TMR2_ISTS & ~0xFFF, DAC2_DATA & ~0xFFF, 0xE000E000):
        cpu.mem_map(address, 0x1000)
    cpu.reg_write(arm.UC_ARM_REG_SP, RAM + 0x1000)
    cpu.reg_write(arm.UC_ARM_REG_PRIMASK, 1)

    def run(start, stop, limit):
        cpu.emu_start(start | 1, stop, count=limit)
        require(cpu.reg_read(arm.UC_ARM_REG_PC) == stop,
                f"unexpected execution path from {start:#010x}")

    def primask():
        return cpu.reg_read(arm.UC_ARM_REG_PRIMASK)

    trace = ["pre_init PRIMASK=1 (seed)"]
    for instruction, action in [(i, "CPSIE") for i in early] + [(mask, "WDT CPSID")]:
        run(instruction.address, instruction.address + instruction.size, 8)
        trace.append(f"{instruction.address:#010x} {action}: PRIMASK={primask()}")
    require(primask() == 1, "WDT entry did not mask IRQs")

    writes = []
    cpu.hook_add(unicorn.UC_HOOK_MEM_WRITE,
                 lambda _cpu, _access, address, size, value, _data:
                 writes.append((address, size, value)))
    cpu.reg_write(wdt_register, WDT_CMD)
    run(keys.address, after_keys.address, 8)
    require(writes == [(WDT_CMD, 4, key) for key in (0xAAAA, 0xCCCC, 0)],
            "WDT command writes differ")
    trace.append(f"{keys.address:#010x} WDT keys AAAA/CCCC/0000")

    # Seed both results from checked() and execute the actual branch. WDT
    # configuration/polling between the separately pinned blocks is omitted.
    cpu.reg_write(arm.UC_ARM_REG_R1, 0)
    run(check.address, fatal, 12)
    require(primask() == 1, "watchdog failure path unmasked IRQs")
    trace.append(f"{fatal:#010x} watchdog failure target PRIMASK=1")
    cpu.reg_write(arm.UC_ARM_REG_R1, 0xFF)
    run(check.address, normal.address, 12)
    mask_at_main = primask()
    if late is not None:
        trace.append(f"{late.address:#010x} post-watchdog CPSIE")
    trace.append(f"{normal.address:#010x} normal startup PRIMASK={mask_at_main}")
    trace.append(f"TMR2 vector={vector:#010x}")
    if mask_at_main == 0:
        # Direct dispatch with an empty stream and a fake overflow flag.
        # An empty stream takes the keyer's idle path and parks DAC2.
        cpu.mem_write(TMR2_ISTS, struct.pack("<I", 1))
        cpu.reg_write(arm.UC_ARM_REG_LR, RETURN_PC | 1)
        writes.clear()
        run(vector & ~1, RETURN_PC, 150)
        require((TMR2_ISTS, 4, 0) in writes and (DAC2_DATA, 4, 128) in writes,
                "TMR2 idle handler did not clear overflow and park DAC2")
        require(primask() == 0, "TMR2 idle handler left IRQs masked")
        trace.append("TMR2 direct idle handler: overflow cleared, DAC2=128")
    return mask_at_main, trace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    args = parser.parse_args()

    try:
        data, image, symbols = read_elf(args.elf)
        print(f"ELF SHA-256 {sha256(data).hexdigest()}")
        mask, trace = emulate(image, symbols)
    except (AssertionError, KeyError, OSError, struct.error, unicorn.UcError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    print("\n".join(trace))
    if mask:
        print("FAIL: IRQs still globally masked after successful WDT setup")
        return 1
    print("PASS: IRQs globally enabled after successful WDT setup")
    return 0


if __name__ == "__main__":
    sys.exit(main())
