"""Offline linked-ARM audit of the minimal UART observer.

Runs the actual instructions against a bounded synthetic Cortex-M4/MMIO model.
Never accesses serial hardware. This does not model oscillator accuracy, pin
electronics, reset handoff, automatic exception stacking or physical RF state.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct

from check_diag_preinit import read_elf, require, capstone, unicorn, arm, cs_arm

FLASH_BASE, FLASH_END = 0x08000000, 0x08040000
APP_BASE, RESET, MSP = 0x08002800, 0x08002AC1, 0x200099C8
CRM, GPIOA, UART = 0x40023800, 0x40020000, 0x40011400
CPUID, PID, SERIES, FLASH_KIB = 0xE000ED00, 0xE0042000, 0xE0042020, 0x1FFFF7E0
ID_REGISTERS = (CPUID, PID, SERIES, FLASH_KIB)
NORMAL_CODES = [0x200, 0x201, 0x202, 0x203, 0x210, 0x211, 0x212,
                0x213, 0x214, 0x215, 0x216, 0x217, 0x220]
GPIO_MASKS = {GPIOA: 3 << 22, GPIOA + 4: 1 << 11, GPIOA + 8: 3 << 22,
              GPIOA + 12: 3 << 22, GPIOA + 0x24: 15 << 12}


def check_layout(data, image, symbols):
    """Strict application-only, fully file-backed, zero-static-RAM layout."""
    header = struct.unpack_from("<16sHHIIIIIHHHHHH", data)
    _, _, _, _, entry, phoff, _, _, _, phsize, phnum, _, _, _ = header
    require(phsize == 32 and phoff + phnum * phsize <= len(data), "bad program header table")
    loads = []
    for index in range(phnum):
        kind, offset, virtual, physical, count, memory, flags, alignment = struct.unpack_from(
            "<8I", data, phoff + index * phsize)
        if kind != 1 or memory == 0:
            continue
        require(count == memory, "UART probe must have no zero-fill/static RAM PT_LOAD")
        require(APP_BASE <= virtual < virtual + memory <= FLASH_END, "PT_LOAD outside application Flash")
        require(physical == virtual, "UART probe LMA/VMA differ")
        require(offset + count <= len(data), "truncated PT_LOAD")
        require(alignment in (0, 1) or alignment & (alignment - 1) == 0, "bad load alignment")
        for earlier in loads:
            left = int(earlier["physical_address"], 16)
            require(max(left, physical) >= min(left + earlier["file_bytes"], physical + count),
                    "overlapping PT_LOAD")
        loads.append({"index": index, "file_offset": offset,
                      "virtual_address": f"0x{virtual:08X}", "physical_address": f"0x{physical:08X}",
                      "file_bytes": count, "memory_bytes": memory, "flags": flags})
    require(loads, "no application loads")
    require(any(int(x["physical_address"], 16) == APP_BASE and x["file_bytes"] >= 704 for x in loads),
            "vector table is not entirely file-backed")
    vectors = struct.unpack_from("<176I", image, APP_BASE - FLASH_BASE)
    require(vectors[0] == MSP, "MSP differs from proven working stock vector")
    require(vectors[1] == RESET and entry == RESET, "ELF entry/Reset differs from stock vector address")
    require(symbols.get("Reset", {}).get("value") == RESET, "Reset symbol mismatch")
    require(any(x["flags"] & 1 and int(x["physical_address"], 16) <= (RESET & ~1)
                < int(x["physical_address"], 16) + x["file_bytes"] for x in loads),
            "Reset not in executable file-backed load")
    fault = symbols.get("ProbeFault", {}).get("value")
    require(fault is not None and fault & 1, "missing Thumb fault target")
    for index, value in enumerate(vectors[2:], 2):
        require(value in (0, fault), f"unexpected vector {index}: {value:#x}")
    return {"initial_msp": f"0x{MSP:08X}", "reset_vector": f"0x{RESET:08X}",
            "vector_address": f"0x{APP_BASE:08X}", "vector_bytes": 704,
            "static_ram_bytes": 0, "load_segments": loads}


def decode_functions(image, symbols):
    decoder = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB | capstone.CS_MODE_MCLASS)
    decoder.detail = True
    decoded, functions = {}, []
    for name, symbol in symbols.items():
        if symbol["info"] & 15 != 2 or not symbol["size"]:
            continue
        start, size = symbol["value"] & ~1, symbol["size"]
        if not APP_BASE <= start < FLASH_END:
            continue
        require(symbol["value"] & 1 and start + size <= FLASH_END, f"invalid function {name}")
        raw = image[start - FLASH_BASE:start - FLASH_BASE + size]
        instructions = list(decoder.disasm(raw, start))
        require(sum(ins.size for ins in instructions) == size, f"incomplete disassembly: {name}")
        for ins in instructions:
            require(ins.mnemonic not in ("push", "pop", "bl", "blx"),
                    f"call/stack opcode at {ins.address:#x}: {ins.mnemonic}")
            for operand in ins.operands:
                if operand.type == cs_arm.ARM_OP_MEM:
                    require(operand.mem.base != cs_arm.ARM_REG_SP and operand.mem.index != cs_arm.ARM_REG_SP,
                            f"stack-relative memory at {ins.address:#x}")
            decoded[ins.address] = ins
        functions.append({"name": name, "address": f"0x{start:08X}", "bytes": size})
    require(all(name in symbols for name in ("Reset", "ProbeFault", "ProbeStop", "ProbeUartByte")),
            "required sized entry/fault/stop/UART symbols missing")
    return decoded, functions


def parse_wire(wire):
    frames, complete = [], len(wire) // 47 * 47
    for offset in range(0, complete, 47):
        frame = wire[offset:offset + 47]
        require(frame[:7] == b"UV98D1 " and frame[42:43] == b"*" and frame[45:] == b"\r\n",
                f"bad wire syntax {frame!r}")
        require(int(frame[43:45], 16) == sum(frame[:42]) & 255, "wire checksum mismatch")
        words = [int(part, 16) for part in frame[7:42].split(b" ")]
        require(len(words) == 4, "wrong word count")
        frames.append(words)
    return frames, wire[complete:]


def run_case(name, image, symbols, decoded, overrides=None, *, expected_codes=None,
             hick_ready=True, switch_ready=True, uart_flags=0xC0, block_after=None,
             serial_delay=0, control=0, ipsr=0, fault_read=None, fault_entry=False,
             uart_ready=True, instruction_budget=2_000_000):
    overrides = overrides or {}
    machine = unicorn.Uc(unicorn.UC_ARCH_ARM, unicorn.UC_MODE_THUMB | unicorn.UC_MODE_MCLASS)
    machine.ctl_set_cpu_model(arm.UC_CPU_ARM_CORTEX_M4)
    for address, size in ((FLASH_BASE, FLASH_END - FLASH_BASE), (0x1FFFF000, 0x1000),
                          (0x40000000, 0x30000), (0xE000E000, 0x1000), (0xE0042000, 0x1000)):
        machine.mem_map(address, size)
    machine.mem_write(FLASH_BASE, image)
    # SRAM intentionally unmapped. All explicit memory/stack use fails.
    initial = {CRM: 3, CRM + 4: 0x24003010, CRM + 8: 0,
               CRM + 0x30: 0x01400000, CRM + 0x34: 0x80, CRM + 0xA0: 0, CRM + 0xA4: 0,
               GPIOA: 0x51555555, GPIOA + 4: 0x5555, GPIOA + 8: 0xAAAAAAAA,
               GPIOA + 12: 0x55555555, GPIOA + 0x24: 0xA5A5A5A5,
               UART: uart_flags, UART + 8: 69, UART + 12: 0x2008 if fault_entry else 0,
               CPUID: 0x410FC241, PID: 0x700A3243, SERIES: 0x1200, FLASH_KIB: 256,
               0xE000E004: 3, 0xE000ED28: 0x8200, 0xE000ED2C: 0x40000000}
    initial.update(overrides)
    for address, value in initial.items():
        machine.mem_write(address, struct.pack("<I", value))
    for reg in (arm.UC_ARM_REG_MSP, arm.UC_ARM_REG_PSP, arm.UC_ARM_REG_SP):
        machine.reg_write(reg, MSP)
    machine.reg_write(arm.UC_ARM_REG_CONTROL, control)
    machine.reg_write(arm.UC_ARM_REG_IPSR, ipsr)
    machine.reg_write(arm.UC_ARM_REG_PRIMASK, 0)
    machine.reg_write(arm.UC_ARM_REG_R12, 0x80000000 if fault_entry and uart_ready else 0x12345678)
    reads, writes, wire, id_reads = [], [], bytearray(), []
    status_reads, steps, delay = 0, 0, 0
    result = {"termination": "instruction_limit"}

    def read32(address):
        return int.from_bytes(machine.mem_read(address, 4), "little")

    def code_hook(uc, address, size, _):
        nonlocal steps
        require(address in decoded, f"{name}: execution escaped audited functions at {address:#x}")
        steps += 1
        if decoded[address].mnemonic == "wfi":
            result.update(termination="terminal_wfi", stop_address=f"0x{address:08X}")
            uc.emu_stop()

    def memory_hook(uc, access, address, size, value, _):
        nonlocal status_reads, delay
        flash = FLASH_BASE <= address < FLASH_END
        require(not 0x20000000 <= address < 0x40000000, f"{name}: explicit SRAM access {address:#x}")
        if access == unicorn.UC_MEM_WRITE:
            require(size == 4, f"{name}: non-word MMIO write {address:#x}")
            allowed = address in {CRM, CRM + 8, CRM + 0xA0, CRM + 0xA4, CRM + 0x10,
                                  CRM + 0x14, CRM + 0x24, CRM + 0x30, CRM + 0x34, CRM + 0x44,
                                  UART + 4, UART + 8, UART + 12, UART + 16, UART + 20,
                                  0xE000E010, 0xE000E014, 0xE000E018, 0xE000ED04,
                                  0xE000ED08, 0xE000ED10} or address in GPIO_MASKS or (
                                      0xE000E180 <= address < 0xE000E200 or
                                      0xE000E280 <= address < 0xE000E300)
            require(allowed and not flash, f"{name}: write outside probe whitelist {address:#x}={value:#x}")
            if address in GPIO_MASKS:
                require((read32(address) ^ value) & ~GPIO_MASKS[address] == 0,
                        f"{name}: UART changed a non-PA11 GPIO field")
            masks = {CRM: 1, CRM + 8: 3 | (15 << 4) | (7 << 13), CRM + 0xA0: 1 << 14,
                     CRM + 0xA4: (7 << 16) | 0x30, CRM + 0x10: 0x01400000, CRM + 0x14: 0x80,
                     CRM + 0x24: 0x20, CRM + 0x30: 0x01400001, CRM + 0x34: 0x80,
                     CRM + 0x44: 0x20, 0xE000ED10: 6}
            if address in masks:
                require((read32(address) ^ value) & ~masks[address] == 0,
                        f"{name}: unexpected CRM field change at {address:#x}")
            if address == CRM + 8 and (read32(address) ^ value) & ((15 << 4) | (7 << 13)):
                require(read32(CRM + 8) & 15 == 0 and read32(CRM + 0xA0) & 0x4000 == 0
                        and read32(CRM + 0xA4) & 0x70000 == 0x40000,
                        f"{name}: bus divisors changed before slow direct HICK was selected")
            if address == CRM + 0xA4 and (read32(address) & 0x70000) != (value & 0x70000) and value & 0x70000 == 0:
                require(read32(CRM + 8) & 0xE0FF == 0 and read32(CRM + 0xA0) & 0x4000 == 0,
                        f"{name}: HICK direct raised before checking clock source and bus divisors")
            if address == UART + 4:
                require(read32(UART) & 0x80, f"{name}: TX write without TDBE")
                require(read32(UART + 12) == 0x2008 and read32(UART + 16) == 0 and read32(UART + 20) == 0,
                        f"{name}: UART RX/IRQ/DMA/flow control enabled")
                require(read32(UART + 8) == 69, f"{name}: UART divisor is not HICK8/115200")
                if not fault_entry:
                    require(read32(CRM) & 3 == 3 and read32(CRM + 8) & 0xE0FF == 0
                            and read32(CRM + 0xA0) & 0x4000 == 0
                            and read32(CRM + 0xA4) & 0x70030 == 0,
                            f"{name}: UART bytes emitted before checked direct HICK8/APB2 baseline")
                wire.append(value & 255)
                delay = serial_delay
            writes.append((address, size, value))
            return
        if flash:
            return
        allowed_reads = {CRM, CRM + 4, CRM + 8, CRM + 0x10, CRM + 0x14,
                         CRM + 0x24, CRM + 0x30, CRM + 0x34, CRM + 0x44,
                         CRM + 0xA0, CRM + 0xA4, UART, GPIOA + 0x1C,
                         0xE000ED10, 0xE000ED28, 0xE000ED2C, *ID_REGISTERS, *GPIO_MASKS}
        require(address in allowed_reads, f"{name}: read outside observer whitelist {address:#x}")
        require(address != UART + 4, f"{name}: UART receive/data register read")
        if address in ID_REGISTERS:
            frames, _partial = parse_wire(bytes(wire))
            require(frames and frames[0] == [0x200, 0x102, 0, 0],
                    f"{name}: ID read before complete constant entry marker")
            before_code = {CPUID: 0x210, PID: 0x212, SERIES: 0x214, FLASH_KIB: 0x216}[address]
            require(frames[-1] == [before_code, address, 0, 0],
                    f"{name}: ID read not preceded by its complete address marker")
            id_reads.append({"address": f"0x{address:08X}", "wire_bytes_before": len(wire)})
            if fault_read == address:
                result.update(termination="injected_id_read_fault", fault_address=f"0x{address:08X}")
                uc.emu_stop()
                return
        if address == CRM:
            current = read32(CRM)
            ready = 2 if current & 1 and hick_ready else 0
            uc.mem_write(CRM, struct.pack("<I", (current & ~2) | ready))
        elif address == CRM + 8 and switch_ready:
            current = read32(CRM + 8)
            uc.mem_write(CRM + 8, struct.pack("<I", (current & ~12) | ((current & 3) << 2)))
        elif address == UART:
            status_reads += 1
            flags = uart_flags
            if block_after is not None and len(wire) >= block_after:
                flags = 0
            elif delay:
                flags = 0
                delay -= 1
            uc.mem_write(UART, struct.pack("<I", flags))
        reads.append((address, size))

    machine.hook_add(unicorn.UC_HOOK_CODE, code_hook)
    machine.hook_add(unicorn.UC_HOOK_MEM_READ | unicorn.UC_HOOK_MEM_WRITE, memory_hook)
    start = symbols["ProbeFault" if fault_entry else "Reset"]["value"]
    machine.emu_start(start, 0, count=instruction_budget)
    require(result["termination"] in ("terminal_wfi", "injected_id_read_fault"),
            f"{name}: did not terminate within {instruction_budget} instructions")
    frames, partial = parse_wire(bytes(wire))
    if expected_codes is not None:
        require([frame[0] for frame in frames] == expected_codes,
                f"{name}: wrong event sequence {[hex(frame[0]) for frame in frames]}")
    if frames and frames[0][0] == 0x200:
        require(frames[0] == [0x200, 0x102, 0, 0], f"{name}: first marker depends on identity")
    for frame in frames:
        if frame[0] == 0x201:
            require(frame[1:] == [control, ipsr, MSP], f"{name}: entry context snapshot differs")
        elif frame[0] == 0x202:
            require(frame[1:3] == [initial[CRM], initial[CRM + 8]], f"{name}: original CRM snapshot lost")
        elif frame[0] == 0x203:
            require(frame[1:] == [initial[CRM + 0xA0], initial[CRM + 0xA4], initial[CRM + 4]],
                    f"{name}: original MISC/PLL snapshot lost")
        elif frame[0] in (0x211, 0x213, 0x215, 0x217):
            register = {0x211: CPUID, 0x213: PID, 0x215: SERIES, 0x217: FLASH_KIB}[frame[0]]
            require(frame[1] == initial[register], f"{name}: identity value differs")
        elif frame[0] == 0xE2FF0000:
            require(frame[1:] == [ipsr, initial[0xE000ED28], initial[0xE000ED2C]],
                    f"{name}: exception details differ")
    if expected_codes == NORMAL_CODES:
        require(len(id_reads) >= 4 and not partial, f"{name}: missing IDs or partial stream")
        require(read32(GPIOA + 8) & GPIO_MASKS[GPIOA + 8] == 1 << 22,
                f"{name}: drive strength does not match stock STRONGER=1")
    return {"name": name, "passed": True, **result, "instructions_executed": steps,
            "wire_ascii": bytes(wire).decode("ascii"), "frames": frames, "partial_bytes": len(partial),
            "mmio_writes": len(writes), "mmio_reads": len(reads), "uart_status_reads": status_reads,
            "id_reads": id_reads, "ram_accesses": 0, "non_uart_gpio_writes": 0}


def audit(path):
    data, image, symbols = read_elf(path)
    layout = check_layout(data, image, symbols)
    decoded, functions = decode_functions(image, symbols)
    cases = []
    def run(name, overrides=None, **kwargs):
        cases.append(run_case(name, image, symbols, decoded, overrides,
                              expected_codes=kwargs.pop("expected_codes", NORMAL_CODES), **kwargs))
    run("hick8_default")
    run("pll48_inherited", {CRM: 0x03000003, CRM + 4: 0x31801, CRM + 8: 0x100A, CRM + 0xA0: 0x1000})
    run("different_pll_inherited", {CRM: 0x03000003, CRM + 4: 0x42801, CRM + 8: 0x800A, CRM + 0xA0: 0x1000})
    run("hext_inherited", {CRM: 0x00030000, CRM + 8: 5})
    run("hick48_direct", {CRM + 0xA0: 0x5000})
    run("hick_div16_bus_prescalers", {CRM + 8: 0xE0F0, CRM + 0xA4: 0x40000})
    run("different_ids_still_reported", {PID: 0xFFFFFFFF, SERIES: 0xABCD9876, FLASH_KIB: 1024})
    run("zero_ids_still_reported", {PID: 0, SERIES: 0, FLASH_KIB: 0})
    run("entry_handler_context_observed", ipsr=3)
    run("entry_psp_context_observed", control=2)
    run("uart_busy_between_bytes", serial_delay=3)
    run("hick_ready_timeout", {CRM: 0, CRM + 8: 5}, hick_ready=False, expected_codes=[])
    run("source_switch_timeout", {CRM + 8: 0xA}, switch_ready=False, expected_codes=[])
    run("uart_tdbe_timeout", uart_flags=0, expected_codes=[])
    run("uart_tc_timeout", uart_flags=0x80, expected_codes=[0x200])
    run("uart_partial_frame_timeout", block_after=13, expected_codes=[])
    for register, previous in ((CPUID, 0x210), (PID, 0x212), (SERIES, 0x214), (FLASH_KIB, 0x216)):
        prefix = NORMAL_CODES[:NORMAL_CODES.index(previous) + 1]
        run(f"fault_on_read_{register:08x}", fault_read=register, expected_codes=prefix)
    run("fault_handler_uart_ready", fault_entry=True, ipsr=3, expected_codes=[0xE2FF0000])
    run("fault_handler_before_uart", fault_entry=True, uart_ready=False, ipsr=3, expected_codes=[])
    return {"passed": True, "validation_scope": "actual linked ARM instructions with synthetic MMIO",
            "physical_hardware_verified": False, "device_access": False,
            "elf": str(path.resolve()), "elf_sha256": hashlib.sha256(data).hexdigest(),
            "layout": layout, "functions": functions, "static_instruction_count": len(decoded),
            "static_checks": {"no_explicit_sram": True, "no_stack_or_call_opcodes": True},
            "emulator": {"name": "Unicorn", "version": unicorn.__version__, "cpu": "Cortex-M4"},
            "disassembler": {"name": "Capstone", "version": capstone.__version__},
            "cases": cases, "terminal_cases": [], "exception_cases": [],
            "limits": {"automatic_exception_stacking_modeled": False,
                       "injected_id_fault_model": "stop at the MMIO read hook; exception handler tested separately",
                       "privilege_access_enforcement_modeled": False,
                       "physical_clock_timing_verified": False,
                       "mmio_values_are_synthetic": True}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    report = audit(args.elf)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    with args.report.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    print(f"PASS: UART probe linked ARM audit, {len(report['cases'])} cases; physical hardware verified: false")


if __name__ == "__main__":
    main()
