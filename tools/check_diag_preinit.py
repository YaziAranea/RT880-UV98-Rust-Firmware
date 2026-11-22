"""Audit linked ARM __pre_init with Capstone and Unicorn Cortex-M4.

Offline instruction/MMIO model only: no device, serial port, physical GPIO,
timing, radio safety or actual MCU compatibility is established by this check.
ELF parsing uses only the Python standard library. Dependencies are loaded from
the existing sibling research directories; nothing is downloaded or installed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys

PROJECT = Path(__file__).resolve().parent.parent
sys.path[:0] = [str(PROJECT.parent / "calibration-analysis" / "deps"),
               str(PROJECT.parent / "hardware_readback" / "deps")]

import capstone
from capstone import arm as cs_arm
import unicorn
from unicorn import arm_const as arm

FLASH_BASE, FLASH_SIZE = 0x08000000, 0x40000
RETURN_PC = FLASH_BASE + FLASH_SIZE - 16
PID = 0xE0042000
SERIES = PID + 0x20
FLASH_KIB = 0x1FFFF7E0
UART = 0x40011400
ICTR = 0xE000E004
CRM = 0x40023800
GPIO = (0x40020000, 0x40020400, 0x40020800, 0x40021400)
DIAGNOSTIC_MSP = 0x200099C8
BOOT_CODES = [0x300, 0x301, 0x302]
GUARD_CODES = BOOT_CODES + [1, 0x303, 0x306]
SNAPSHOT_CODES = GUARD_CODES + [0x304, 0x305]
NORMAL_CODES = SNAPSHOT_CODES + [2, 3]
LIVE_ENTRY = {PID: 0x700A3246, SERIES: 0x1200, FLASH_KIB: 256,
              CRM: 0x03008983, CRM + 8: 0x4000100A,
              CRM + 0xA0: 0x000F1000, CRM + 0xA4: 0x0000000D,
              CRM + 4: 0x00041801}
GPIO_OWNED_PINS = {GPIO[0]: 0x8910, GPIO[1]: 0x1C07, GPIO[2]: 0xC0B0, GPIO[3]: 0x0100}
RF_LOW_PINS = {GPIO[0]: 0x0010, GPIO[1]: 0x0800, GPIO[2]: 0x8030, GPIO[3]: 0x0100}
RF_A_SHUTDOWN = [0x330000, 0x36007F, 0x300000, 0x371D00]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def read_elf(path):
    data = path.read_bytes()
    header = struct.unpack_from("<16sHHIIIIIHHHHHH", data)
    require(header[0][:7] == b"\x7fELF\x01\x01\x01", "expected ELF32 little endian")
    require(header[1] == 2 and header[2] == 40, "expected ARM ET_EXEC")
    phoff, shoff = header[5:7]
    phsize, phnum, shsize, shnum = header[9:13]
    require(phsize == 32 and shsize == 40, "unsupported ELF table entry sizes")
    sections = [struct.unpack_from("<10I", data, shoff + i * shsize) for i in range(shnum)]
    symbols = {}
    for section in sections:
        if section[1] != 2:
            continue
        require(section[9] == 16, "unsupported symbol size")
        strings = sections[section[6]]
        names = data[strings[4]:strings[4] + strings[5]]
        for offset in range(section[4], section[4] + section[5], 16):
            name, value, size, info, _, index = struct.unpack_from("<IIIBBH", data, offset)
            end = names.find(b"\0", name)
            require(end >= name, "unterminated symbol name")
            text = names[name:end].decode("utf-8")
            if index and text:
                symbols[text] = {"value": value, "size": size, "info": info}
    image = bytearray(FLASH_SIZE)
    for i in range(phnum):
        kind, offset, _, physical, file_size, _, _, _ = struct.unpack_from("<8I", data, phoff + i * phsize)
        if kind != 1 or file_size == 0 or not FLASH_BASE <= physical < FLASH_BASE + FLASH_SIZE:
            continue
        require(physical + file_size <= FLASH_BASE + FLASH_SIZE, "load segment exceeds flash")
        require(offset + file_size <= len(data), "truncated load segment")
        image[physical - FLASH_BASE:physical - FLASH_BASE + file_size] = data[offset:offset + file_size]
    return data, bytes(image), symbols


def disassemble(image, symbol):
    start = symbol["value"] & ~1
    size = symbol["size"]
    require(symbol["value"] & 1 and size > 0, "__pre_init must be a sized Thumb function")
    require(FLASH_BASE <= start < start + size <= RETURN_PC, "function outside application flash")
    decoder = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB | capstone.CS_MODE_MCLASS)
    decoder.detail = True
    raw = image[start - FLASH_BASE:start - FLASH_BASE + size]
    instructions = list(decoder.disasm(raw, start))
    require(sum(item.size for item in instructions) == size, "incomplete function disassembly")
    for item in instructions:
        reads, writes = item.regs_access()
        registers = set(reads) | set(writes)
        require(cs_arm.ARM_REG_SP not in registers, f"stack register at {item.address:#x}")
        require(item.mnemonic not in ("bl", "blx", "push", "pop"), f"call/stack opcode at {item.address:#x}")
        require(not any(reg in registers for reg in range(cs_arm.ARM_REG_R4, cs_arm.ARM_REG_R11 + 1)),
                f"callee-saved register accessed at {item.address:#x}")
        require(cs_arm.ARM_REG_LR not in writes, f"link register clobbered at {item.address:#x}")
        if item.mnemonic == "bx":
            require(item.op_str in ("lr", "r3"), "unexpected indirect branch")
        for operand in item.operands:
            if operand.type == cs_arm.ARM_OP_MEM:
                require(operand.mem.base in (cs_arm.ARM_REG_R0, cs_arm.ARM_REG_R1,
                                             cs_arm.ARM_REG_R2, cs_arm.ARM_REG_R3, cs_arm.ARM_REG_PC),
                        f"unexpected memory base at {item.address:#x}")
    return start, instructions


def audit_reset_prefix(image, symbols, pre_init_start):
    """Resolve the short, straight-line Reset prefix up to its first call.

    Deliberately not a general runtime interpreter. Any new instruction shape
    before __pre_init requires review instead of silently assuming RAM safety.
    """
    require("Reset" in symbols, "missing Reset symbol")
    symbol = symbols["Reset"]
    start = symbol["value"] & ~1
    require(symbol["size"] > 0, "Reset has no linked size")
    vectors = struct.unpack_from("<4I", image, 0x2800)
    require(vectors[:2] == (DIAGNOSTIC_MSP, 0x08002AC1), "diagnostic vectors differ from the observed stock handoff")
    require(vectors[1] == symbol["value"], "reset vector does not target audited Reset")
    decoder = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB | capstone.CS_MODE_MCLASS)
    decoder.detail = True
    raw = image[start - FLASH_BASE:start - FLASH_BASE + symbol["size"]]
    instructions = list(decoder.disasm(raw, start))
    require(sum(item.size for item in instructions) == symbol["size"], "incomplete Reset disassembly")
    known = {}
    prefix = []
    writes = []
    for item in instructions:
        prefix.append(f"{item.address:08x}: {item.mnemonic} {item.op_str}")
        operands = item.operands
        if item.mnemonic == "bl":
            require(operands[0].type == cs_arm.ARM_OP_IMM and operands[0].imm == pre_init_start,
                    "first Reset call does not target __pre_init")
            return {"reset_vector_verified": True, "first_call_is_pre_init": True,
                    "call_address": f"0x{item.address:08x}", "prefix_ram_accesses": 0,
                    "resolved_prefix_writes": writes, "prefix_disassembly": prefix}
        if item.mnemonic == "ldr" and len(operands) == 2 and operands[1].type == cs_arm.ARM_OP_MEM:
            memory = operands[1].mem
            require(memory.base == cs_arm.ARM_REG_PC and memory.index == 0,
                    "Reset prefix contains nonliteral load")
            address = ((item.address + 4) & ~3) + memory.disp
            require(FLASH_BASE <= address <= FLASH_BASE + FLASH_SIZE - 4,
                    "Reset literal outside Flash")
            known[operands[0].reg] = struct.unpack_from("<I", image, address - FLASH_BASE)[0]
        elif item.mnemonic == "msr" and item.op_str == "msp, r0":
            require(known.get(cs_arm.ARM_REG_R0) == vectors[0], "Reset MSP differs from vector")
            require(0x20000000 < vectors[0] <= 0x2000C000, "Reset MSP outside assumed RAM")
        elif item.mnemonic == "str" and len(operands) == 2 and operands[1].type == cs_arm.ARM_OP_MEM:
            memory = operands[1].mem
            require(not item.writeback and memory.index == 0 and memory.base in known,
                    "unresolved Reset prefix write")
            address = known[memory.base] + memory.disp
            value = known.get(operands[0].reg)
            require(address == 0xE000ED08 and value == 0x08002800,
                    "Reset write before __pre_init is not the expected VTOR MMIO write")
            writes.append({"address": f"0x{address:08x}", "value": f"0x{value:08x}"})
        else:
            raise AssertionError(f"unsupported instruction before __pre_init: {prefix[-1]}")
    raise AssertionError("Reset never calls __pre_init")


def run_case(name, image, start, instructions, overrides, expect_return, expected_banks=None,
             no_gpio=False, no_writes=False, control=0, ipsr=0, terminal_low=False,
             expected_codes=None, expected_divisor=69, serial_delay=0,
             serial_block_after=None, expected_partial_bytes=0, early_bootstrap=True,
             hick_ready=True, switch_ready=True, gpio_clocks_latch=True,
             expected_rf_shutdown=None, gpio_stuck_high=None, expected_b_shutdown=None,
             late_gpio_lock=None):
    if expected_rf_shutdown is None:
        expected_rf_shutdown = expect_return or terminal_low
    if expected_b_shutdown is None:
        expected_b_shutdown = expected_rf_shutdown
    machine = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
    machine.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
    for address, size in ((FLASH_BASE, FLASH_SIZE), (0x1FFFF000, 0x1000),
                          (0x40000000, 0x30000), (0xE000E000, 0x1000), (0xE0042000, 0x1000)):
        machine.mem_map(address, size)
    machine.mem_write(FLASH_BASE, image)
    registers = {PID: 0x700A3243, SERIES: 0x1200, FLASH_KIB: 256, ICTR: 3,
                 CRM + 0x30: 0x01400000, CRM + 0x34: 0x80,
                 CRM + 0x40: 0xC000, CRM + 0x70: 0, CRM: 3, UART: 0xC0,
                 CRM + 4: 0x24003010, CRM + 8: 0, CRM + 0xA0: 0, CRM + 0xA4: 0,
                 0x40003004: 3, 0x40003008: 0xFFF, 0x4000300C: 0,
                 0x40003010: 0xFFF, 0x40002C00: 0x7F,
                 0xE000ED28: 0x00008200, 0xE000ED2C: 0x40000000}
    for port in GPIO:
        registers[port + 0x14] = 0xFFFF
        registers[port + 0x1C] = 0
    # Seed PA12 RX fields: neither UART setup nor RF masks may alter them.
    registers.update({GPIO[0]: 1 << 24, GPIO[0] + 4: 1 << 12,
                      GPIO[0] + 8: 3 << 24, GPIO[0] + 12: 2 << 24,
                      GPIO[0] + 0x24: 0xa << 16})
    registers.update(overrides)
    for address, value in registers.items():
        machine.mem_write(address, struct.pack("<I", value))
    sp = DIAGNOSTIC_MSP  # SRAM deliberately unmapped: any stack/RAM access fails.
    machine.reg_write(arm.UC_ARM_REG_SP, sp)
    machine.reg_write(arm.UC_ARM_REG_MSP, sp)
    machine.reg_write(arm.UC_ARM_REG_PSP, sp)
    machine.reg_write(arm.UC_ARM_REG_LR, RETURN_PC | 1)
    machine.reg_write(arm.UC_ARM_REG_CONTROL, control)
    machine.reg_write(arm.UC_ARM_REG_IPSR, ipsr)
    require(machine.reg_read(arm.UC_ARM_REG_CONTROL) == control, "Unicorn CONTROL setup unsupported")
    require(machine.reg_read(arm.UC_ARM_REG_IPSR) == ipsr, "Unicorn IPSR setup unsupported")
    machine.reg_write(arm.UC_ARM_REG_PRIMASK, 0)
    preserved = {}
    for index in range(4, 12):
        reg = getattr(arm, f"UC_ARM_REG_R{index}")
        preserved[reg] = 0xA5000000 + index
        machine.reg_write(reg, preserved[reg])
    decoded = {item.address: item for item in instructions}
    writes, reads, trace, serial_bytes, rf_gpio_writes, id_reads = [], [], [], [], [], []
    serial_wait = 0
    result = {"termination": "instruction_limit"}
    rf_frames, rf_frame_end_writes = {0: [], 1: []}, []
    rf_bits, rf_selected = None, None
    rf_buses = [(GPIO[0], 0x100, GPIO[3], 0x40, 0x200),
                (GPIO[2], 0x80, GPIO[2], 0x100, 0x40)]
    expected_shutdown = (expected_rf_shutdown, expected_b_shutdown)
    # Deliberately active internal PA/TX requests. This is a wire decoder/model,
    # not a physical BK4802 measurement or proof the device accepted writes.
    rf_chip = [{0x33: 0x00FF, 0x36: 0xFFFF, 0x30: 0xFFFF, 0x37: 0xFFFF} for _ in rf_buses]
    last_rf_data, last_rf_high = None, None
    owned_pins = dict(GPIO_OWNED_PINS)
    if expected_rf_shutdown or not early_bootstrap or gpio_stuck_high:
        owned_pins[GPIO[2]] |= 0x0200
        owned_pins[GPIO[3]] |= 0x0040

    if expected_b_shutdown or gpio_stuck_high or not early_bootstrap or expected_rf_shutdown:
        owned_pins[GPIO[2]] |= 0x0140

    def code_hook(uc, address, size, _):
        if address == RETURN_PC:
            result["termination"] = "returned_to_lr"
            uc.emu_stop()
            return
        require(address in decoded, f"{name}: executed outside __pre_init: {address:#x}")
        trace.append(address)
        if decoded[address].mnemonic == "wfi":
            result["termination"] = "terminal_wfi"
            result["stop_address"] = f"0x{address:08x}"
            uc.emu_stop()

    def memory_hook(uc, access, address, size, value, _):
        nonlocal serial_wait, rf_bits, rf_selected, last_rf_data, last_rf_high
        def read32(at):
            return int.from_bytes(uc.mem_read(at, 4), "little")
        require(not 0x20000000 <= address < 0x40000000, f"{name}: SRAM access {address:#x}")
        is_flash = FLASH_BASE <= address < FLASH_BASE + FLASH_SIZE
        is_mmio = (0x40000000 <= address < 0x40030000 or
                   0xE000E000 <= address < 0xE000F000 or
                   0xE0042000 <= address < 0xE0043000)
        require(is_flash or is_mmio or address == FLASH_KIB, f"{name}: unexpected memory {address:#x}")
        if access == unicorn.UC_MEM_WRITE:
            require(is_mmio, f"{name}: non-MMIO write {address:#x}")
            allowed = address in {
                CRM, CRM + 8, CRM + 0x10, CRM + 0x14, CRM + 0x20, CRM + 0x24,
                CRM + 0x30, CRM + 0x34, CRM + 0x40, CRM + 0x44, CRM + 0xA0, CRM + 0xA4,
                UART + 4, UART + 8, UART + 12, UART + 16, UART + 20,
                0xE000E010, 0xE000E014, 0xE000E018, 0xE000ED04, 0xE000ED08, 0xE000ED10,
                GPIO[0] + 0x24,
            } or any(address == port + offset for port in GPIO for offset in (0, 4, 8, 12, 0x18, 0x28)) or (
                0xE000E180 <= address < 0xE000E200 or 0xE000E280 <= address < 0xE000E300)
            require(allowed and size == 4, f"{name}: write outside word-MMIO whitelist at {address:#x}")
            masks = {CRM: 1, CRM + 8: 3 | (15 << 4) | (7 << 13),
                     CRM + 0xA0: 0x4000, CRM + 0xA4: 0x70030}
            if address in masks:
                require((read32(address) ^ value) & ~masks[address] == 0,
                        f"{name}: unexpected clock field changed at {address:#x}")
            require(address != CRM + 4, f"{name}: PLL configuration changed")
            peripheral_masks = {CRM + 0x10: 0x01400000, CRM + 0x14: 0x80,
                                CRM + 0x20: 0xC000, CRM + 0x24: 0x20,
                                CRM + 0x30: 0x01400027, CRM + 0x34: 0x80,
                                CRM + 0x40: 0xC000, CRM + 0x44: 0x20}
            if address in peripheral_masks:
                require((read32(address) ^ value) & ~peripheral_masks[address] == 0,
                        f"{name}: unrelated peripheral gate/reset changed at {address:#x}")
            for port, pins in owned_pins.items():
                offset = address - port
                if offset in (0, 4, 8, 12):
                    mask = pins if offset == 4 else sum(3 << (2 * pin) for pin in range(16) if pins & (1 << pin))
                    require((read32(address) ^ value) & ~mask == 0, f"{name}: unrelated GPIO field changed")
                elif offset in (0x18, 0x28):
                    require(value & ~pins == 0, f"{name}: unrelated GPIO latch changed")
                    if offset == 0x18:
                        require(value & RF_LOW_PINS[port] == 0, f"{name}: RF-low/backlight pin requested HIGH")
            if address == CRM + 8 and (read32(address) ^ value) & 0xE0F0:
                require(read32(CRM + 8) & 15 == 0 and read32(CRM + 0xA0) & 0x4000 == 0
                        and read32(CRM + 0xA4) & 0x70000 == 0x40000,
                        f"{name}: bus divisors changed before slow direct HICK")
            if address == CRM + 0xA4 and (read32(address) & 0x70000) != (value & 0x70000) and value & 0x70000 == 0:
                require(read32(CRM + 8) & 0xE0FF == 0 and read32(CRM + 0xA0) & 0x4000 == 0,
                        f"{name}: HICK raised before source/bus baseline check")
            if address in masks:
                require(read32(CRM + 0x30) & 0x01400000 == 0 and read32(CRM + 0x34) & 0x80 == 0,
                        f"{name}: changed clock before stopping DMA/USB")
                require(read32(CRM + 0x40) & 0xC000 == 0 and read32(CRM + 0x20) & 0xC000 == 0xC000,
                        f"{name}: changed clock before gating/resetting SPI2/3")
            if address == UART + 4:
                for chip, expected in enumerate(expected_shutdown):
                    if expected:
                        require(rf_frames[chip] == RF_A_SHUTDOWN,
                                f"{name}: UART byte precedes guarded RF shutdown")
                status = int.from_bytes(uc.mem_read(UART, 4), "little")
                require(status & 0x80 != 0, f"{name}: wrote UART DT before TDBE")
                require(int.from_bytes(uc.mem_read(UART + 12, 4), "little") == 0x2008,
                        f"{name}: UART not TX-only 8N1 with interrupts disabled")
                require(int.from_bytes(uc.mem_read(UART + 20, 4), "little") == 0,
                        f"{name}: UART DMA/flow control unexpectedly enabled")
                require(read32(UART + 16) == 0 and read32(UART + 8) == 69,
                        f"{name}: UART not 8N1 HICK8/115200")
                require(read32(GPIO[0] + 8) & (3 << 22) == 1 << 22
                        and read32(GPIO[0] + 0x24) & (15 << 12) == 8 << 12,
                        f"{name}: PA11 drive/AF differs from stock-tested UART")
                require(read32(CRM) & 3 == 3 and read32(CRM + 8) & 0xE0FF == 0
                        and read32(CRM + 0xA0) & 0x4000 == 0 and read32(CRM + 0xA4) & 0x70030 == 0,
                        f"{name}: UART bytes before verified HICK8 baseline")
                serial_bytes.append(value & 0xff)
                serial_wait = serial_delay
                # Inject a fault after the startup lock snapshots to exercise
                # the later output checks independently of initial RF shutdown.
                if late_gpio_lock is not None and len(serial_bytes) == len(SNAPSHOT_CODES) * 47:
                    port, mask = late_gpio_lock
                    uc.mem_write(port + 0x1c, struct.pack("<I", 0x10000 | mask))
            for port in GPIO:
                if port <= address < port + 0x400:
                    mask = {0: 0x00c00000, 4: 0x800, 8: 0x00c00000,
                            12: 0x00c00000, 0x24: 0xf000}.get(address - GPIO[0], 0)
                    before = int.from_bytes(uc.mem_read(address, size), "little")
                    if port != GPIO[0] or not mask or (before ^ value) & ~mask:
                        rf_gpio_writes.append((address, size, value))
            writes.append((address, size, value))
            for port in GPIO:
                if address in (port + 0x18, port + 0x28):
                    current = int.from_bytes(uc.mem_read(port + 0x14, 4), "little")
                    new = current | value if address == port + 0x18 else current & ~value
                    if gpio_stuck_high and port == gpio_stuck_high[0]:
                        new |= gpio_stuck_high[1]
                    for chip, (cs_port, cs_mask, clock_port, clock_mask, data_mask) in enumerate(rf_buses):
                        if port == cs_port and value & cs_mask:
                            if address == port + 0x28:
                                require(expected_shutdown[chip], f"{name}: RF chip selected on a rejected/startup path")
                                require(read32(PID) in (0x700A3243, 0x700A3246)
                                        and read32(SERIES) & 0xFF07 == 0x1200
                                        and read32(FLASH_KIB) & 0xFFFF == 256,
                                        f"{name}: RF frame before exact identity")
                                require(read32(CRM + 0x30) & 0x27 == 0x27,
                                        f"{name}: RF frame without GPIO clocks")
                                for gate_port, mask in RF_LOW_PINS.items():
                                    require(read32(gate_port + 0x14) & mask == 0,
                                            f"{name}: RF frame preceded MCU gate-low request")
                                require(rf_bits is None, f"{name}: nested RF-A select")
                                rf_selected = chip
                                rf_bits, last_rf_data, last_rf_high = [], None, None
                            elif rf_selected == chip:
                                require(len(rf_bits) == 24, f"{name}: RF frame has {len(rf_bits)} edges")
                                require(last_rf_high is not None and len(trace) - last_rf_high >= 1000,
                                        f"{name}: last RF clock high phase too short in instruction model")
                                frame = 0
                                for bit in rf_bits:
                                    frame = (frame << 1) | bit
                                rf_frames[chip].append(frame)
                                rf_chip[chip][frame >> 16] = frame & 0xFFFF
                                rf_frame_end_writes.append(len(writes))
                                rf_bits = rf_selected = None
                        if port == GPIO[2] and value & data_mask and rf_selected == chip:
                            last_rf_data = len(trace)
                        if port == clock_port and value & clock_mask and rf_selected == chip:
                            if address == port + 0x18 and current & clock_mask == 0 and new & clock_mask:
                                require(last_rf_data is not None and len(trace) - last_rf_data >= 1000,
                                        f"{name}: RF DATA setup lacks bounded500-iteration phase")
                                rf_bits.append(1 if read32(GPIO[2] + 0x14) & data_mask else 0)
                                last_rf_high = len(trace)
                            elif address == port + 0x28 and current & clock_mask and new & clock_mask == 0:
                                require(last_rf_high is not None and len(trace) - last_rf_high >= 1000,
                                        f"{name}: RF clock high phase lacks bounded delay")
                    uc.mem_write(port + 0x14, struct.pack("<I", new & 0xFFFFFFFF))
        elif not is_flash:
            require(address != UART + 4, f"{name}: RX/DT was read")
            if address in (PID, SERIES, FLASH_KIB):
                id_reads.append({"address": f"0x{address:08x}", "wire_bytes_before": len(serial_bytes)})
            if address == CRM:
                current = read32(CRM)
                uc.mem_write(CRM, struct.pack("<I", (current & ~2) | (2 if current & 1 and hick_ready else 0)))
            elif address == CRM + 8 and switch_ready:
                current = read32(address)
                uc.mem_write(address, struct.pack("<I", (current & ~12) | ((current & 3) << 2)))
            elif address == CRM + 0x30 and not gpio_clocks_latch:
                uc.mem_write(address, struct.pack("<I", read32(address) & ~0x26))
            if address == UART and serial_block_after is not None and len(serial_bytes) >= serial_block_after:
                uc.mem_write(UART, struct.pack("<I", 0))
            elif address == UART and UART not in overrides and serial_wait:
                uc.mem_write(UART, struct.pack("<I", 0))
                serial_wait -= 1
            elif address == UART and UART not in overrides:
                uc.mem_write(UART, struct.pack("<I", 0xC0))
            reads.append((address, size))

    machine.hook_add(unicorn.UC_HOOK_CODE, code_hook)
    machine.hook_add(unicorn.UC_HOOK_MEM_READ | unicorn.UC_HOOK_MEM_WRITE, memory_hook)
    machine.emu_start(start | 1, 0, count=2_000_000)
    require(result["termination"] == ("returned_to_lr" if expect_return else "terminal_wfi"),
            f"{name}: unexpected termination {result}")
    require(machine.reg_read(arm.UC_ARM_REG_SP) == sp, f"{name}: SP changed")
    require(machine.reg_read(arm.UC_ARM_REG_LR) == RETURN_PC | 1, f"{name}: LR changed")
    if control & 1 == 0:
        require(machine.reg_read(arm.UC_ARM_REG_PRIMASK) == 1, f"{name}: interrupts not masked")
    for reg, value in preserved.items():
        require(machine.reg_read(reg) == value, f"{name}: callee-saved register changed")
    gpio_writes = [write for write in writes if any(port <= write[0] < port + 0x400 for port in GPIO)]
    if no_gpio:
        require(not rf_gpio_writes, f"{name}: rejected profile touched non-UART GPIO")
    if no_writes:
        require(not writes, f"{name}: rejected identity performed MMIO writes")
    irq_writes = [write for write in writes if 0xE000E180 <= write[0] < 0xE000E200 or
                  0xE000E280 <= write[0] < 0xE000E300]
    if expected_banks is not None:
        expected = [(base + 4 * bank, 4, 0xFFFFFFFF)
                    for count in ([8] if early_bootstrap else []) + [expected_banks]
                    for bank in range(count) for base in (0xE000E180, 0xE000E280)]
        require(irq_writes == expected, f"{name}: IRQ bank writes differ")
    if expect_return or terminal_low:
        final = lambda address: int.from_bytes(machine.mem_read(address, 4), "little")
        for port, mask in ((GPIO[2], 0x8030), (GPIO[1], 0x800), (GPIO[3], 0x100), (GPIO[0], 0x10)):
            require(final(port + 0x14) & mask == 0, f"{name}: LOW latch request absent")
        require(final(GPIO[3] + 0x14) & 1 == 1, "relay PF0 latch changed")
        require(final(CRM + 0x30) & 0x01400000 == 0, "DMA clocks remain enabled")
        require(final(CRM + 0x34) & 0x80 == 0, "USB clock remains enabled")
        require(final(CRM + 0x10) & 0x01400000 == 0x01400000, "DMA not held reset")
        require(final(CRM + 0x14) & 0x80 == 0x80, "USB not held reset")
    if expect_return:
        require(final(GPIO[2] + 0x14) & 0x4080 == 0x4080, "LCD reset/RF-B CS not HIGH")
        require(final(CRM + 0x40) & 0xC000 == 0, "SPI2/3 clocks remain enabled")
        require(final(CRM + 0x20) & 0xC000 == 0xC000, "SPI2/3 not held reset")
        require(final(0xE000ED08) == 0x08002800, "VTOR not application base")
    wire = bytes(serial_bytes)
    require(len(wire) % 47 == expected_partial_bytes, f"{name}: unexpected partial UART frame {wire!r}")
    frames = []
    for offset in range(0, len(wire) - expected_partial_bytes, 47):
        frame = wire[offset:offset + 47]
        require(frame[:7] == b"UV98D1 " and frame[42:43] == b"*" and frame[45:] == b"\r\n",
                f"{name}: invalid wire frame {frame!r}")
        require(int(frame[43:45], 16) == sum(frame[:42]) & 0xff, f"{name}: checksum differs")
        fields = [int(part, 16) for part in frame[7:42].split(b" ")]
        require(len(fields) == 4, f"{name}: malformed hex words")
        frames.append(fields)
    if expected_codes is not None:
        require([frame[0] for frame in frames] == expected_codes,
                f"{name}: event codes {[hex(f[0]) for f in frames]} != {[hex(c) for c in expected_codes]}")
    if wire:
        divisor = int.from_bytes(machine.mem_read(UART + 8, 4), "little")
        require(divisor == expected_divisor, f"{name}: baud divisor {divisor} != {expected_divisor}")
    for frame in frames:
        if frame[0] == 0x300:
            require(frame[1:] == [0x108, 0, 0], f"{name}: first marker differs")
        elif frame[0] in (0x301, 0xE1000008):
            # Unprivileged MRS of MSP returns zero in this architectural model.
            visible_msp = 0 if control & 1 else sp
            require(frame[1:] == [control, ipsr, visible_msp],
                    f"{name}: CPU context snapshot differs: {frame[1:]}")
        elif frame[0] == 0x302:
            require(frame[1] & 3 == 3 and frame[2] & 0xE0FF == 0 and frame[3] == registers[CRM + 4],
                    f"{name}: normalized clock snapshot differs")
        elif frame[0] == 0x303:
            require(frame[1:3] == [registers[CRM + 0x70], registers[ICTR]], f"{name}: guard inputs differ")
        elif frame[0] == 0x304:
            require(frame[1:] == [registers[port + 0x1C] for port in GPIO[:3]], f"{name}: GPIO lock snapshot differs")
        elif frame[0] == 0x305:
            require(frame[1:] == [registers[GPIO[3] + 0x1C], registers[0x4000300C], registers[0x40003010]],
                    f"{name}: watchdog status/window snapshot differs")
        elif frame[0] == 0x306:
            require(frame[1:] == [registers[0x40003004], registers[0x40003008], registers[0x40002C00]],
                    f"{name}: watchdog divider/reload/WWDT snapshot differs")
        elif frame[0] == 0xe1ff0000:
            require(frame[1:] == [ipsr, registers[0xe000ed28], registers[0xe000ed2c]],
                    f"{name}: fault details differ")
    if terminal_low:
        first_tx = next(index for index, (address, _, _) in enumerate(writes) if address == UART + 4)
        for port, mask in ((GPIO[2], 0x8030), (GPIO[1], 0x800), (GPIO[3], 0x100), (GPIO[0], 0x10)):
            require(any(address == port + 0x28 and value == mask
                        for address, _, value in writes[:first_tx]),
                    f"{name}: fault frame started before RF low attempt")
    require(rf_bits is None, f"{name}: unfinished RF-A transaction")
    for chip, expected in enumerate(expected_shutdown):
        require(rf_frames[chip] == (RF_A_SHUTDOWN if expected else []),
                f"{name}: RF shutdown frames differ on chip {chip}: {rf_frames[chip]}")
        if expected:
            require(rf_chip[chip] == {0x33: 0, 0x36: 0x007F, 0x30: 0, 0x37: 0x1D00},
                    f"{name}: modeled TX/PA bits remain requested")
    if any(expected_shutdown):
        first_tx = next((index for index, (address, _, _) in enumerate(writes) if address == UART + 4), None)
        if first_tx is not None:
            require(rf_frame_end_writes[-1] <= first_tx, f"{name}: UART log precedes RF shutdown")
    for offset, mask in ((0, 3 << 24), (4, 1 << 12), (8, 3 << 24), (12, 3 << 24), (0x24, 0xf << 16)):
        value = int.from_bytes(machine.mem_read(GPIO[0] + offset, 4), "little")
        require(value & mask == registers[GPIO[0] + offset] & mask, f"{name}: PA12 RX fields changed")
    for frame in frames:
        if frame[0] == 0xe1000006:
            port, lock, required = frame[1:]
            if late_gpio_lock is not None:
                expected_port, expected_mask = late_gpio_lock
                require((port, lock, required) == (expected_port, 0x10000 | expected_mask, expected_mask),
                        f"{name}: GPIO lock details differ")
            else:
                require(lock == registers[port + 0x1c] and lock & required,
                        f"{name}: GPIO lock details not concrete")
    require(not any(0x40003000 <= address <= 0x40003010 or 0x40002C00 <= address <= 0x40002C0C
                    for address, _, _ in writes),
            f"{name}: ASM touched watchdog")
    return {"name": name, "passed": True, **result, "instructions_executed": len(trace),
            "entry_control": control, "entry_ipsr": ipsr,
            "exit_primask": machine.reg_read(arm.UC_ARM_REG_PRIMASK),
            "mmio_read_count": len(reads), "mmio_write_count": len(writes),
            "gpio_write_count": len(gpio_writes), "irq_bank_write_count": len(irq_writes),
            "stack_pointer_unchanged": True, "ram_accesses": 0,
            "wire_ascii": wire.decode("ascii"), "frames": frames,
            "rf_a_frames": [f"0x{frame:06x}" for frame in rf_frames[0]],
            "rf_a_modeled_registers": {f"0x{register:02x}": f"0x{value:04x}" for register, value in rf_chip[0].items()},
            "rf_b_frames": [f"0x{frame:06x}" for frame in rf_frames[1]],
            "rf_a_shutdown_before_fault_log": bool(expected_rf_shutdown),
            "id_reads": id_reads, "watchdog_writes": 0,
            "mmio_write_whitelist_verified": True, "unowned_gpio_fields_preserved": True,
            "uart_status_reads": sum(address == UART for address, _ in reads),
            "write_trace": [{"address": f"0x{a:08x}", "size": s, "value": f"0x{v:08x}"}
                            for a, s, v in writes]}


def check(path):
    data, image, symbols = read_elf(path)
    require("__pre_init" in symbols, "missing __pre_init symbol")
    start, instructions = disassemble(image, symbols["__pre_init"])
    reset_prefix = audit_reset_prefix(image, symbols, start)
    require("__diag_uart_byte" in symbols, "missing stackless UART byte helper")
    uart_start, uart_code = disassemble(image, symbols["__diag_uart_byte"])
    require([i.op_str for i in uart_code if i.mnemonic == "bx"] == ["r3"], "UART continuation ABI differs")
    require(not any(i.mnemonic == "bx" and i.op_str != "lr" for i in instructions),
            "pre_init has an unexpected indirect branch")
    pre_decode = instructions + uart_code
    cases = [
        ("supported_pid43_four_irq_banks", {}, True, 4, False, False),
        ("supported_pid46_one_irq_bank", {PID: 0x700A3246, ICTR: 0}, True, 1, False, False),
        ("maximum_eight_irq_banks", {ICTR: 7}, True, 8, False, False),
        ("unsupported_pid", {PID: 0xDEADBEEF}, False, 0, True, False),
        ("unsupported_series", {SERIES: 0}, False, 0, True, False),
        ("unsupported_flash_size", {FLASH_KIB: 128}, False, 0, True, False),
        ("lext_active", {CRM + 0x70: 1}, False, 4, True, False),
        ("gpio_c15_locked", {GPIO[2] + 0x1C: 0x18000}, False, 0, True, False),
        ("gpio_b11_locked", {GPIO[1] + 0x1C: 0x10800}, False, 0, False, False),
        ("gpio_f8_locked", {GPIO[3] + 0x1C: 0x10100}, False, 0, False, False),
        ("gpio_a4_locked", {GPIO[0] + 0x1C: 0x10010}, False, 0, False, False),
        ("nine_irq_banks_rejected", {ICTR: 8}, False, 0, True, False),
        ("sixteen_irq_banks_rejected", {ICTR: 15}, False, 0, True, False),
    ]
    results = [run_case(name, image, start, pre_decode, override, returns, banks, no_gpio, no_writes,
                        expected_codes=(NORMAL_CODES if returns else {
                            "unsupported_pid": BOOT_CODES + [1, 0xe1000001],
                            "unsupported_series": BOOT_CODES + [0xe1000009],
                            "unsupported_flash_size": BOOT_CODES + [1, 0xe1000002],
                            "lext_active": SNAPSHOT_CODES + [0xe1000004],
                            "gpio_c15_locked": [0x300, 0xe100000a],
                            "gpio_b11_locked": [0x300, 0xe100000a],
                            "gpio_f8_locked": [0x300, 0xe100000a],
                            "gpio_a4_locked": [0x300, 0xe100000a],
                            "nine_irq_banks_rejected": GUARD_CODES + [0xe1000003],
                            "sixteen_irq_banks_rejected": GUARD_CODES + [0xe1000003]}[name]))
               for name, override, returns, banks, no_gpio, no_writes in cases]
    for name, control, ipsr in (("handler_mode_rejected", 0, 3),
                                ("psp_thread_rejected", 2, 0),
                                ("unprivileged_thread_rejected", 1, 0)):
        results.append(run_case(name, image, start, pre_decode, {}, False, 0,
                                True, False, control=control, ipsr=ipsr,
                                expected_codes=BOOT_CODES + [0xe1000008]))

    results.append(run_case("observed_live_stock_boot_context", image, start, pre_decode,
                            LIVE_ENTRY, True, 4, control=4, expected_codes=NORMAL_CODES))
    late_gpio_outputs = [
        (GPIO[2], 0x8030), (GPIO[1], 0x0800), (GPIO[3], 0x0100), (GPIO[0], 0x0010),
        (GPIO[0], 0x8100), (GPIO[1], 0x1000), (GPIO[2], 0x4080), (GPIO[1], 0x0407),
    ]
    for index, (port, mask) in enumerate(late_gpio_outputs):
        codes = SNAPSHOT_CODES + ([2] if index >= 4 else []) + [0xe1000006]
        results.append(run_case(f"startup_late_gpio_lock_{index}", image, start, pre_decode,
                                {}, False, 4, expected_codes=codes, expected_rf_shutdown=True,
                                late_gpio_lock=(port, mask)))
    results.append(run_case("watchdog_updating_observed_without_write", image, start, pre_decode,
                            {0x4000300C: 7, 0x40003004: 6, 0x40003008: 0xABC,
                             0x40003010: 0x789, 0x40002C00: 0xFF}, True, 4,
                            expected_codes=NORMAL_CODES))
    results.append(run_case("gpio_clock_gate_readback_failure", image, start, pre_decode,
                            {}, False, 0, no_gpio=True, gpio_clocks_latch=False,
                            expected_codes=[0x300, 0xe100000a]))
    results.append(run_case("hick_ready_timeout", image, start, pre_decode,
                            {CRM: 0, CRM + 8: 5}, False, 0, no_gpio=True,
                            hick_ready=False, expected_codes=[]))
    results.append(run_case("clock_source_switch_timeout", image, start, pre_decode,
                            {CRM + 8: 0xA}, False, 0, no_gpio=True,
                            switch_ready=False, expected_codes=[]))

    # UART failures must remain bounded and may not prevent original takeover.
    for name, override, codes, delay in (
        ("uart_tdbe_timeout", {UART: 0}, [], 0),
        ("uart_tc_timeout", {UART: 0x80}, [0x300], 0),
        ("uart_busy_between_bytes", {}, NORMAL_CODES, 3),
        ("uart_pa11_locked", {GPIO[0] + 0x1c: 0x10800}, [], 0),
        ("inherited_hext_normalized", {CRM + 8: 5}, NORMAL_CODES, 0),
        ("inherited_hick_enabled_waits_ready", {CRM: 1}, NORMAL_CODES, 0),
        ("inherited_invalid_divider_normalized", {CRM + 0xa4: 5 << 16}, NORMAL_CODES, 0),
        ("inherited_direct_div16_normalized", {CRM + 0xa4: 4 << 16}, NORMAL_CODES, 0),
        ("inherited_direct_48mhz_normalized", {CRM + 0xa0: 0x5000}, NORMAL_CODES, 0),
        ("inherited_direct48_div2_normalized", {CRM + 0xa0: 0x5000, CRM + 0xa4: 1 << 16}, NORMAL_CODES, 0),
        ("inherited_bus_divisors_normalized", {CRM + 0xa0: 0x5000, CRM + 8: 0x8080}, NORMAL_CODES, 0),
        ("uart_stock_pll48", {CRM: 0x03000003, CRM + 4: 0x31801,
                              CRM + 8: 0x100a, CRM + 0xa0: 0x1000}, NORMAL_CODES, 0),
        ("uart_unknown_pll", {CRM: 0x03000003, CRM + 4: 0x31802,
                              CRM + 8: 0x100a, CRM + 0xa0: 0x1000}, NORMAL_CODES, 0),
    ):
        results.append(run_case(name, image, start, pre_decode, override, True, 4,
                                expected_codes=codes, serial_delay=delay))
    results.append(run_case("uart_timeout_partial_frame", image, start, pre_decode,
                            {}, True, 4, expected_codes=[], serial_block_after=13,
                            expected_partial_bytes=13))
    results.append(run_case("unsupported_revision_logged", image, start, pre_decode,
                            {SERIES: 0x1201}, False, 0, no_gpio=True,
                            expected_codes=BOOT_CODES + [1, 0xe1000007]))
    for name, overrides in (
        ("startup_rf_a_clock_locked", {GPIO[3] + 0x1C: 0x10040}),
        ("startup_rf_a_data_locked", {GPIO[2] + 0x1C: 0x10200}),
    ):
        results.append(run_case(name, image, start, pre_decode, overrides, False, 0,
            expected_codes=[0x300, 0xe100000a], expected_rf_shutdown=False, expected_b_shutdown=True))
    results.append(run_case("startup_rf_a_data_stuck_high", image, start, pre_decode,
        {}, False, 0, gpio_stuck_high=(GPIO[2], 0x0200), expected_rf_shutdown=False, expected_b_shutdown=True,
        expected_codes=[0x300, 0xe100000a]))
    results.append(run_case("startup_rf_b_data_locked", image, start, pre_decode,
        {GPIO[2] + 0x1C: 0x10040}, False, 0, expected_rf_shutdown=True,
        expected_b_shutdown=False, expected_codes=[0x300, 0xe100000a]))
    results.append(run_case("startup_after_watchdog_reset_with_seeded_tx_bits", image,
        start, pre_decode, {CRM + 0x74: 1 << 29, GPIO[0] + 0x14: 0xFEFF}, True, 4,
        expected_codes=NORMAL_CODES, expected_rf_shutdown=True))

    # The terminal routine branches to the three-instruction local WFI block
    # in __pre_init. Permit exactly that shared block, not arbitrary caller code.
    wfi = [i for i, item in enumerate(instructions) if item.mnemonic == "wfi"]
    require(len(wfi) == 1 and 0 < wfi[0] < len(instructions) - 1, "unexpected terminal block")
    shared_stop = instructions[wfi[0] - 1:wfi[0] + 2]
    require(shared_stop[0].mnemonic == "cpsid" and shared_stop[2].mnemonic == "b", "invalid stop loop")
    require(shared_stop[2].operands[0].type == cs_arm.ARM_OP_IMM and
            shared_stop[2].operands[0].imm == shared_stop[0].address,
            "WFI continuation does not loop back to the terminal block")
    terminal_name = "__diag_terminal_fail_stop"
    require(terminal_name in symbols, "missing ASM terminal export")
    terminal_start, terminal = disassemble(image, symbols[terminal_name])
    terminal_decode = terminal + shared_stop + uart_code
    terminal_cases = [
        ("terminal_supported_thread", {}, 0, True, False),
        ("terminal_supported_hardfault", {}, 3, True, False),
        ("terminal_unsupported_pid", {PID: 0}, 3, False, False),
        ("terminal_unsupported_family", {SERIES: 0}, 3, False, True),
        ("terminal_lext_active", {CRM + 0x70: 1}, 3, False, False),
        ("terminal_gpio_c15_locked", {GPIO[2] + 0x1C: 0x18000}, 3, False, False),
    ]
    terminal_results = [
        run_case(name, image, terminal_start, terminal_decode, overrides, False, 0,
                 no_gpio=not low, no_writes=no_writes, ipsr=ipsr, terminal_low=low,
                 early_bootstrap=False, expected_codes=[] if no_writes else [0xe1ff0000],
                 expected_rf_shutdown=low)
        for name, overrides, ipsr, low, no_writes in terminal_cases
    ]
    for name, overrides in (
        ("terminal_rf_a_cs_locked", {GPIO[0] + 0x1C: 0x10100}),
        ("terminal_rf_a_sck_locked", {GPIO[3] + 0x1C: 0x10040}),
        ("terminal_rf_a_data_locked", {GPIO[2] + 0x1C: 0x10200}),
    ):
        terminal_results.append(run_case(name, image, terminal_start, terminal_decode,
            overrides, False, 0, ipsr=3, terminal_low=True, early_bootstrap=False,
            expected_codes=[0xe1ff0000], expected_rf_shutdown=False, expected_b_shutdown=True))
    for name, overrides in (
        ("terminal_unsupported_revision", {SERIES: 0x1201}),
        ("terminal_unsupported_flash", {FLASH_KIB: 128}),
    ):
        terminal_results.append(run_case(name, image, terminal_start, terminal_decode,
            overrides, False, 0, ipsr=3, no_gpio=True, early_bootstrap=False,
            expected_codes=[0xe1ff0000]))
    terminal_results.append(run_case("terminal_gpio_clock_readback_failure", image,
        terminal_start, terminal_decode, {}, False, 0, ipsr=3, no_gpio=True,
        gpio_clocks_latch=False, early_bootstrap=False, expected_codes=[0xe1ff0000]))
    terminal_results.append(run_case("terminal_mcu_gate_stuck_high", image,
        terminal_start, terminal_decode, {}, False, 0, ipsr=3, early_bootstrap=False,
        gpio_stuck_high=(GPIO[2], 0x8000), expected_codes=[0xe1ff0000]))
    terminal_results.append(run_case("terminal_rf_a_data_stuck_high", image,
        terminal_start, terminal_decode, {}, False, 0, ipsr=3, terminal_low=True,
        early_bootstrap=False, gpio_stuck_high=(GPIO[2], 0x0200), expected_codes=[0xe1ff0000],
        expected_rf_shutdown=False, expected_b_shutdown=True))
    terminal_results.append(run_case("terminal_rf_b_data_locked", image,
        terminal_start, terminal_decode, {GPIO[2] + 0x1C: 0x10040}, False, 0,
        ipsr=3, terminal_low=True, early_bootstrap=False, expected_rf_shutdown=True,
        expected_b_shutdown=False, expected_codes=[0xe1ff0000]))
    terminal_results.append(run_case("terminal_inherited_rf_cs_low", image,
        terminal_start, terminal_decode, {GPIO[0] + 0x14: 0xFEFF}, False, 0,
        ipsr=3, terminal_low=True, early_bootstrap=False, expected_rf_shutdown=True,
        expected_codes=[0xe1ff0000]))
    exceptions = []
    for name in ("HardFault", "DefaultHandler"):
        require(name in symbols, f"missing {name}")
        entry, code = disassemble(image, symbols[name])
        require(len(code) == 1 and code[0].mnemonic in ("b", "b.w"), f"{name} is not a direct ASM branch")
        require(code[0].operands[0].type == cs_arm.ARM_OP_IMM and
                code[0].operands[0].imm == terminal_start, f"{name} target differs")
        exceptions.append(run_case(f"{name}_entry", image, entry, code + terminal_decode,
                                   {}, False, 0, ipsr=3 if name == "HardFault" else 2, terminal_low=True,
                                   early_bootstrap=False, expected_rf_shutdown=True,
                                   expected_codes=[0xe1ff0000]))
    require("NonMaskableInt" in symbols and symbols["NonMaskableInt"]["value"] == symbols["DefaultHandler"]["value"],
            "NMI is not aliased to the audited DefaultHandler")
    vectors = struct.unpack_from("<4I", image, 0x2800)
    require(vectors[2] == symbols["DefaultHandler"]["value"], "NMI vector not audited ASM entry")
    require(vectors[3] == symbols["HardFault"]["value"], "HardFault vector not audited ASM entry")
    return {"passed": True, "validation_scope": "linked startup/terminal/exception instruction execution with mocked MMIO",
            "physical_hardware_verified": False, "device_access": False,
            "elf": str(path.resolve()), "elf_sha256": hashlib.sha256(data).hexdigest(),
            "symbol": {"name": "__pre_init", "address": f"0x{start:08x}", "bytes": symbols["__pre_init"]["size"]},
            "static_instruction_count": len(instructions),
            "reset_prefix_check": reset_prefix,
            "static_checks": {"no_stack_memory_or_sp_writes": True, "no_call_instruction": True,
                              "r0_r3_r12_only_scratch": True, "uart_tail_branch_abi": True, "complete_disassembly": True},
            "emulator": {"name": "Unicorn", "version": unicorn.__version__, "cpu": "Cortex-M4", "mode": "Thumb MCLASS"},
            "disassembler": {"name": "Capstone", "version": capstone.__version__},
            "cases": results,
            "uart_helper_symbol": {"address": f"0x{uart_start:08x}", "bytes": symbols["__diag_uart_byte"]["size"]},
            "terminal_symbol": {"name": terminal_name, "address": f"0x{terminal_start:08x}", "bytes": symbols[terminal_name]["size"]},
            "terminal_cases": terminal_results,
            "exception_cases": exceptions,
            "nmi_vector_alias_verified": True,
            "limitations": ["MMIO behavior is a deterministic model, not actual AT32 peripheral execution",
                            "WFI is intercepted before execution; watchdog status is observed but watchdog/reset timing is not simulated",
                            "CONTROL and IPSR are set directly; automatic exception entry/stacking is not simulated",
                            "unprivileged CPSID behavior is not a physical CPU qualification; rejection path is modeled",
                            "GPIO latch updates are modeled; electrical RF state and PCB identity are not verified",
                            "RF-A24-bit frames and seeded PA/TX bits are a logical decoder, not measured chip readback",
                            "delay loops are instruction-count bounded; <=50MHz assumption and physical pin timing are not measured",
                            "no RAM access holds for the enumerated paths; this is not a general formal proof"],
            "disassembly": [f"{item.address:08x}: {item.mnemonic} {item.op_str}" for item in instructions],
            "terminal_disassembly": [f"{item.address:08x}: {item.mnemonic} {item.op_str}" for item in terminal]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    try:
        report = check(args.elf)
    except Exception as error:
        report = {"passed": False, "physical_hardware_verified": False,
                  "error_type": type(error).__name__, "error": str(error)}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if report["passed"]:
        count = sum(len(report[key]) for key in ("cases", "terminal_cases", "exception_cases"))
        print(f"PASS: linked startup/terminal/exception ASM, {count} emulated cases; physical hardware verified: false")
    else:
        print(f"FAIL: {report['error_type']}: {report['error']}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
