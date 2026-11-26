"""Execute the foreground voice stream setup and TMR2 ISR in the linked Cortex-M4 ELF.

Inputs start after RF authorization, at the inlined Stream::begin. The probe
supplies the linked slot accessor's iterator, simulated cycle counter/interrupt
flags and a timely foreground lease. No radio, startup or RF path is emulated.
"""
import argparse
import binascii
import hashlib
import json
import re
import struct
from pathlib import Path

import capstone
import unicorn
from unicorn import arm_const as arm

from check_diag_preinit import FLASH_BASE, FLASH_SIZE, RETURN_PC, read_elf, require
from check_voice_clip import (
    HEADER_SIZE, MAGIC, MAX_FRAMES, RAM_BASE, RAM_SIZE, SLOT_SIZE, linked_thumb, reference_dac,
)

COUNTER = 0xE0001004
TIMER = 0x40000000
IRQ_ENABLE = TIMER + 0x0C
IRQ_STATUS = TIMER + 0x10
DAC = 0x4000741C
NVIC_ENABLE, NVIC_DISABLE, NVIC_CLEAR = 0xE000E100, 0xE000E180, 0xE000E280
IRQ_BIT = 1 << 28
BASE_TIME = 0xFFFFC000  # Exercise counter wrap within the first few samples.
PERIOD = 6000
STACK = RAM_BASE + 0x2000


def foreground_start(image, symbols, decoder):
    """Locate the start block by its real decoder call. Reject other private ABIs."""
    main = [s for name, s in symbols.items() if name.endswith("___cortex_m_rt_main")]
    require(len(main) == 1, "missing unique linked foreground")
    address = main[0]["value"] & ~1
    code = image[address - FLASH_BASE:address - FLASH_BASE + main[0]["size"]]
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB | capstone.CS_MODE_MCLASS)
    calls = []
    for offset in range(12, len(code) - 3, 2):
        if struct.unpack_from("<H", code, offset)[0] >> 11 != 30:
            continue
        ins = next(md.disasm(code[offset:offset+4], address + offset), None)
        if ins and ins.mnemonic == "bl" and ins.op_str == f"#{decoder & ~1:#x}":
            calls.append(offset)
    require(len(calls) == 1, "unsupported ABI: expected one inlined foreground iterator start")
    offset = calls[0]
    immediate = r"(0x[0-9a-f]+|\d+)"
    start = address + offset - 12
    pre = list(md.disasm(code[offset-12:offset], start))
    iterator_registers = ()
    if [i.mnemonic for i in pre] == ["ldr", "ldr.w", "add", "add.w"]:
        literal, counter, low, high = pre
    else:
        # R57 interleaves an iterator register spill with the DWT address
        # load. LLVM emits the first pair as STRD or two word stores. Start
        # before the complete spill in either case and run the linked stores.
        for size, pair_ops in ((34, ["strd"]), (38, ["str.w", "str.w"])):
            start = address + offset - size
            pre = list(md.disasm(code[offset-size:offset], start))
            if [i.mnemonic for i in pre] == (
                ["add", "add.w", "add.w"] + pair_ops
                + ["str.w", "ldr", "stm.w", "ldr.w", "add", "add.w"]
            ):
                break
        else:
            raise AssertionError("unsupported ABI: DWT/iterator setup")
        base_low, base_high, interior, *pair, final, literal, middle, counter, low, high = pre
        spill = [re.fullmatch(pattern, i.op_str) for i, pattern in (
            (base_low, rf"r0, sp, #{immediate}"),
            (base_high, rf"r0, r0, #{immediate}"),
            (interior, rf"r1, r0, #{immediate}"),
            (final, rf"sb, \[r0, #{immediate}\]"),
        )]
        require(all(spill) and middle.op_str == "r1, {r5, r8, fp}",
                "unsupported ABI: iterator spill registers")
        base_lo, base_hi, middle_off, final_off = (int(m[1], 0) for m in spill)
        if len(pair) == 1:
            first = re.fullmatch(rf"sl, r6, \[r0, #{immediate}\]", pair[0].op_str)
            require(first, "unsupported ABI: iterator pair store registers")
        else:
            first = re.fullmatch(rf"sl, \[r0, #{immediate}\]", pair[0].op_str)
            second = re.fullmatch(rf"r6, \[r0, #{immediate}\]", pair[1].op_str)
            require(first and second, "unsupported ABI: iterator pair store registers")
            require(int(second[1], 0) == int(first[1], 0) + 4,
                    "unsupported ABI: iterator pair store offsets")
        pair_off = int(first[1], 0)
        require(middle_off == pair_off + 8 and final_off == pair_off + 20,
                "unsupported ABI: iterator spill offsets")
        iterator_registers = (
            arm.UC_ARM_REG_R10, arm.UC_ARM_REG_R6, arm.UC_ARM_REG_R5,
            arm.UC_ARM_REG_R8, arm.UC_ARM_REG_R11, arm.UC_ARM_REG_R9,
        )
    address_load = re.fullmatch(rf"r0, \[pc, #{immediate}\]", literal.op_str)
    require(address_load and counter.op_str == "sb, [r0]", "unsupported ABI: DWT counter load")
    literal_at = ((literal.address + 4) & ~3) + int(address_load[1], 0) - FLASH_BASE
    require(0 <= literal_at <= len(image) - 4
            and struct.unpack_from("<I", image, literal_at)[0] == COUNTER,
            "foreground does not read DWT CYCCNT")
    low = re.fullmatch(rf"r0, sp, #{immediate}", low.op_str)
    high = re.fullmatch(rf"r0, r0, #{immediate}", high.op_str)
    require(low and high, "unsupported ABI: iterator stack location")
    iterator = STACK + int(low[1], 0) + int(high[1], 0)
    require(STACK <= iterator <= RAM_BASE + RAM_SIZE - 24, "iterator outside test stack")
    if iterator_registers:
        require(STACK + base_lo + base_hi + pair_off == iterator,
                "iterator spill and decoder stack locations differ")
    # Stream::begin consumes the first sample, then copies the rest of its
    # iterator from an interior pointer retained in main's stack frame.
    early = list(md.disasm(code[offset+4:offset+70], address + offset + 4))
    indirect = [m for i in early if i.mnemonic == "ldr.w" and
                (m := re.fullmatch(r"r[01], \[sp, #(0x[0-9a-f]+|\d+)\]", i.op_str))]
    require(len(indirect) == 1, "unsupported ABI: iterator interior stack pointer")
    pointer_slot = STACK + int(indirect[0][1], 0)
    require(STACK <= pointer_slot < iterator, "unsupported ABI: iterator stack overlap")
    return start, iterator, pointer_slot, iterator_registers


class Probe:
    def __init__(self, image, player):
        self.player = player
        self.samples = bytearray()
        self.cpu = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
        self.cpu.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
        self.cpu.mem_map(FLASH_BASE, FLASH_SIZE)
        self.cpu.mem_write(FLASH_BASE, image)
        self.cpu.mem_protect(FLASH_BASE, FLASH_SIZE, unicorn.UC_PROT_READ | unicorn.UC_PROT_EXEC)
        for base, size in ((RAM_BASE, RAM_SIZE), (TIMER, 4096), (DAC & ~4095, 4096),
                           (COUNTER & ~4095, 4096), (NVIC_ENABLE & ~4095, 4096)):
            self.cpu.mem_map(base, size)
        self.cpu.reg_write(arm.UC_ARM_REG_SP, STACK)
        self.cpu.hook_add(unicorn.UC_HOOK_MEM_WRITE, self.write_dac, begin=DAC, end=DAC+3)

    def write_dac(self, _cpu, _access, _addr, size, value, _user):
        require(size == 4 and 0 <= value <= 255, "unexpected DAC access")
        self.samples.append(value)

    def read(self, address, size=4):
        return int.from_bytes(self.cpu.mem_read(address, size), "little")

    def put(self, address, value):
        self.cpu.mem_write(address, struct.pack("<I", value & 0xffffffff))

    def call(self, entry, budget):
        self.cpu.reg_write(arm.UC_ARM_REG_LR, RETURN_PC | 1)
        self.cpu.emu_start(entry, RETURN_PC, count=budget)
        require(self.cpu.reg_read(arm.UC_ARM_REG_PC) == RETURN_PC
                and self.cpu.reg_read(arm.UC_ARM_REG_SP) == STACK, "linked call did not return cleanly")

    def begin(self, accessor, slot, layout, expected):
        start, iterator, pointer_slot, iterator_registers = layout
        self.cpu.reg_write(arm.UC_ARM_REG_R0, iterator)
        self.call(accessor, 400_000)
        require(self.read(iterator) == slot + HEADER_SIZE, "unexpected iterator result ABI")
        for index, register in enumerate(iterator_registers):
            self.cpu.reg_write(register, self.read(iterator + 4 * index))
        self.put(pointer_slot, iterator + 4)
        self.put(COUNTER, BASE_TIME)
        self.cpu.reg_write(arm.UC_ARM_REG_PRIMASK, 1)
        # Stop just after the real start sequence writes TMR2_CTRL1. The
        # enclosing foreground critical-section return is outside this probe.
        mark = self.cpu.hook_add(unicorn.UC_HOOK_MEM_WRITE, lambda cpu, *_: cpu.emu_stop(),
                                 begin=TIMER, end=TIMER+3)
        self.cpu.emu_start(start | 1, RETURN_PC, count=700)
        self.cpu.hook_del(mark)
        self.cpu.reg_write(arm.UC_ARM_REG_PRIMASK, 0)
        require(self.samples == expected[:1] and self.read(TIMER) == 1 and self.read(IRQ_ENABLE) == 1
                and self.read(NVIC_ENABLE) == IRQ_BIT, "linked foreground did not start TMR2/DAC")
        require(self.read(self.player) == 0 and self.read(self.player+4) == slot + HEADER_SIZE
                and all(self.read(self.player+n) == BASE_TIME for n in (28, 32, 36))
                and self.read(self.player+40, 2) == MAX_FRAMES * 2 - 1
                and self.read(self.player+44, 1) == 0, "unsupported linked Player/Stream ABI")

    def interrupt(self, handler, cycles, pending=True):
        self.put(COUNTER, cycles)
        self.put(IRQ_STATUS, int(pending))
        self.call(handler, 1500)
        require(self.read(IRQ_STATUS) == 0 and self.cpu.reg_read(arm.UC_ARM_REG_PRIMASK) == 0,
                "timer flag or interrupt mask not restored")


def check(path):
    raw, image, symbols = read_elf(path)
    policy = symbols["UV98_RELEASE_POLICY"]["value"] - FLASH_BASE
    version = struct.unpack_from("<I", image, policy+12)[0]
    require(version in (0x564B3032, 0x564B3033), "expected a VK02 or VK03 voice-key image")
    accessor = linked_thumb(symbols, "uv98_diag_firmware", "voice_clip", "7samples")
    decoder = linked_thumb(symbols, "uv98_voice_clip", "Samples", "Iterator", "4next")
    layout = foreground_start(image, symbols, decoder)
    players = [s for name, s in symbols.items() if name.endswith("voice_keyer6PLAYER")]
    require(len(players) == 1 and players[0]["size"] == 48, "unsupported Player symbol or size")
    player = players[0]["value"]
    require(RAM_BASE <= player < STACK - 48, "unexpected Player RAM location")
    vector = symbols["__vector_table"]["value"]
    handler = symbols["TMR2"]["value"]
    require(handler & 1 and handler == struct.unpack_from("<I", image, vector - FLASH_BASE + 4 * 44)[0],
            "vector entry for external IRQ28 does not point to linked Thumb TMR2")
    slot = symbols["UV98_VOICE_CLIP"]
    require(slot["size"] == SLOT_SIZE, "unexpected slot size")
    at = slot["value"] - FLASH_BASE
    original = image[at:at+SLOT_SIZE]
    require(original.startswith(MAGIC) and image.count(MAGIC) == 1, "missing unique voice slot")
    # A full synthetic slot also exercises the 12,000-sample hard boundary.
    body = struct.pack("<H", MAX_FRAMES) + bytes((i * 83 + 7) & 255 for i in range(MAX_FRAMES // 2))
    full = MAGIC + struct.pack("<H", binascii.crc_hqx(body, 0xffff)) + body
    cases = {}
    for name in ("original", "full", "stalled_irq", "lease_expired", "spurious"):
        clip = original if name == "original" else full
        expected = reference_dac(clip)
        probe = Probe(image[:at] + clip + image[at+SLOT_SIZE:], player)
        probe.begin(accessor, slot["value"], layout, expected)
        if name == "spurious":
            probe.interrupt(handler, BASE_TIME+PERIOD, False)
            require(probe.samples == expected[:1] and probe.read(TIMER) == 1
                    and probe.read(IRQ_ENABLE) == 1, "spurious IRQ advanced or stopped stream")
            cases[name] = {"spurious_writes": 0}
            continue
        complete = name in ("original", "full")
        count = len(expected) if complete else (1 if name == "stalled_irq" else 160)
        for n in range(1, count+1):
            now = BASE_TIME + (PERIOD * 3 // 2 if name == "stalled_irq" else PERIOD * n)
            if complete and n % 32 == 0 and n < count:
                # Model a granted lease every 4 ms. Physical input qualification
                # and the separate foreground maintain function are out of scope.
                probe.put(player+32, now)
            probe.interrupt(handler, now)
        require(probe.samples == expected[:count] + bytes([128]), f"{name}: wrong DAC sequence/park")
        require(probe.read(TIMER) == probe.read(IRQ_ENABLE) == probe.read(player+4) == 0
                and probe.read(NVIC_DISABLE) == probe.read(NVIC_CLEAR) == IRQ_BIT
                and probe.read(player+44, 1) == int(complete), f"{name}: wrong stop state")
        cases[name] = {"dac_samples_before_park": count, "completed": complete,
                       "dac_sha256": hashlib.sha256(probe.samples[:-1]).hexdigest()}
    return {"passed": True, "elf_sha256": hashlib.sha256(raw).hexdigest(), "cases": cases,
            "release_policy": f"{version:#010x}", "foreground_start": f"{layout[0]:#010x}",
            "tmr2_vector": f"{handler:#010x}",
            "scope": __doc__, "hardware_qualified": False, "device_accessed": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = check(args.elf)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print("PASS: linked foreground and TMR2 match DAC reference, EOF and both abort deadlines")
