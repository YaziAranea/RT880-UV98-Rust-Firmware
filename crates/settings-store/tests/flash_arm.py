"""Offline AT32F423 MMIO model for the linked SRAM flash routine.

Usage: python flash_arm.py PATH_TO_ELF_OR_ASSEMBLED_OBJECT
Requires Unicorn 2. This test maps only emulated memory.
"""

import argparse
import struct
import unittest
from pathlib import Path

from unicorn import (
    UC_ARCH_ARM,
    UC_HOOK_CODE,
    UC_HOOK_MEM_READ,
    UC_HOOK_MEM_WRITE,
    UC_MEM_READ,
    UC_MODE_MCLASS,
    UC_MODE_THUMB,
    Uc,
)
from unicorn.arm_const import (
    UC_ARM_REG_LR,
    UC_ARM_REG_PC,
    UC_ARM_REG_PRIMASK,
    UC_ARM_REG_R0,
    UC_ARM_REG_R1,
    UC_ARM_REG_R2,
    UC_ARM_REG_R3,
    UC_ARM_REG_SP,
    UC_CPU_ARM_CORTEX_M4,
)

FLASH = 0x40023C00
UNLOCK = FLASH + 0x04
STATUS = FLASH + 0x0C
CONTROL = FLASH + 0x10
ERASE_ADDRESS = FLASH + 0x14
CYCLE_COUNT = 0xE0001004
FLASH_START = 0x08000000
JOURNAL_START = 0x0803F000
JOURNAL_END = 0x08040000
PAGE_BYTES = 2048
SRAM = 0x20000000
SRAM_BYTES = 48 * 1024
RETURN_PC = 0x08002800
BUSY = 0x01
ERROR_FLAGS = 0x14
DONE = 0x20
LOCKED = 0x80
PROGRAM = 0x01
ERASE = 0x02
KEYS = (0x45670123, 0xCDEF89AB)


def load_code(path):
    data = Path(path).read_bytes()
    header = struct.unpack_from("<16sHHIIIIIHHHHHH", data)
    ident, kind, machine = header[:3]
    if ident[:6] != b"\x7fELF\x01\x01" or machine != 40 or kind not in (1, 2):
        raise ValueError("expected a little-endian ELF32 ARM object or executable")
    section_offset, entry_size, count = header[6], header[11], header[12]
    sections = [
        struct.unpack_from("<10I", data, section_offset + index * entry_size)
        for index in range(count)
    ]
    symbol_table = next(section for section in sections if section[1] == 2)
    strings = sections[symbol_table[6]]
    string_data = data[strings[4]:strings[4] + strings[5]]
    symbols = {}
    for offset in range(symbol_table[4], symbol_table[4] + symbol_table[5], symbol_table[9]):
        name_offset, value, _, _, _, section = struct.unpack_from("<IIIBBH", data, offset)
        name = string_data[name_offset:].split(b"\x00", 1)[0].decode()
        symbols[name] = (value, section)
    start, section_index = symbols["__uv98_flash_op"]
    end, end_section = symbols["__uv98_flash_op_end"]
    assert section_index == end_section and start & 1
    start &= ~1
    section = sections[section_index]
    address, offset, size = section[3:6]
    assert 0 <= start - address < end - address <= size
    if kind == 1:
        assert not any(part[1] in (4, 9) and part[7] == section_index for part in sections)
        execution_address = SRAM + 0x100
    else:
        data_start = symbols["__sdata"][0]
        data_end = symbols["__edata"][0]
        assert SRAM <= data_start <= start < end <= data_end <= SRAM + SRAM_BYTES
        execution_address = start
    return execution_address, data[offset + start - address:offset + end - address]


class Controller:
    def __init__(
        self, code, *, control=LOCKED, status=0, completion=DONE, stuck=False,
        frozen_counter=False, ignore_unlock=False, ignore_relock=False
    ):
        self.cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB | UC_MODE_MCLASS)
        self.cpu.ctl_set_cpu_model(UC_CPU_ARM_CORTEX_M4)
        self.entry, bytecode = code
        self.end = self.entry + len(bytecode)
        self.cpu.mem_map(SRAM, SRAM_BYTES)
        self.cpu.mem_write(self.entry, bytecode)
        self.cpu.mem_map(FLASH_START, JOURNAL_END - FLASH_START)
        self.cpu.mem_map(FLASH & ~0xFFF, 0x1000)
        self.cpu.mem_map(CYCLE_COUNT & ~0xFFF, 0x1000)
        self.cpu.mem_write(JOURNAL_START, b"\xff" * (JOURNAL_END - JOURNAL_START))
        self.control = control
        self.status = status
        self.completion = completion
        self.stuck = stuck
        self.ignore_unlock = ignore_unlock
        self.ignore_relock = ignore_relock
        self.ticks = 0 if frozen_counter else 4
        self.counter = 0xFFFF_FF00
        self.erase_address = None
        self.keys = []
        self.operations = []
        self.mmio = []
        self.polls = 0
        self.previous_pc = None
        self.same_pc = 0
        self.returned = False
        self.awaiting_watchdog = False
        self.cpu.hook_add(UC_HOOK_CODE, self._instruction)
        self.cpu.hook_add(UC_HOOK_MEM_READ | UC_HOOK_MEM_WRITE, self._memory)

    def _instruction(self, cpu, pc, size, _):
        if pc == RETURN_PC:
            assert not self.status & BUSY
            self.returned = True
            cpu.emu_stop()
            return
        assert self.entry <= pc < self.end, f"execution escaped SRAM routine: {pc:#x}"
        self.counter = (self.counter + self.ticks) & 0xFFFF_FFFF
        self.same_pc = self.same_pc + 1 if self.previous_pc == pc else 0
        self.previous_pc = pc
        if self.same_pc >= 8:
            assert cpu.reg_read(UC_ARM_REG_PRIMASK) == 1
            self.awaiting_watchdog = True
            cpu.emu_stop()

    def _memory(self, cpu, access, address, size, value, _):
        read = access == UC_MEM_READ
        if SRAM <= address < SRAM + SRAM_BYTES:
            return
        assert cpu.reg_read(UC_ARM_REG_PRIMASK) == 1
        assert size == 4
        if FLASH_START <= address < JOURNAL_END:
            assert not read and not self.status & BUSY, ("main-array access", address)
            assert self.control == PROGRAM
            assert JOURNAL_START <= address < JOURNAL_END and address % 4 == 0
            self.operations.append(("program", address, value))
            self._start()
            return
        self.mmio.append((access, address, value))
        if read:
            if address == STATUS:
                self._poll()
                value = self.status
            elif address == CONTROL:
                value = self.control
            elif address == CYCLE_COUNT:
                value = self.counter
            else:
                raise AssertionError(f"read from write-only or unknown register {address:#x}")
            cpu.mem_write(address, struct.pack("<I", value))
            return
        assert not self.status & BUSY
        if address == UNLOCK:
            assert self.control == LOCKED
            self.keys.append(value)
            assert tuple(self.keys) == KEYS[:len(self.keys)]
            if len(self.keys) == 2 and not self.ignore_unlock:
                self.control = 0
        elif address == STATUS:
            assert value == ERROR_FLAGS | DONE
            self.status &= ~value
        elif address == CONTROL:
            if value == ERASE | 0x40:
                assert self.control == ERASE
                assert self.erase_address in (JOURNAL_START, JOURNAL_START + PAGE_BYTES)
                self.operations.append(("erase", self.erase_address))
                self._start()
            elif value in (PROGRAM, ERASE):
                assert self.control == 0 and tuple(self.keys) == KEYS
                self.control = value
            elif value == LOCKED:
                if not self.ignore_relock:
                    self.control = LOCKED
            else:
                raise AssertionError(f"unexpected FLASH_CTRL write {value:#x}")
        elif address == ERASE_ADDRESS:
            assert self.control == ERASE
            self.erase_address = value
        else:
            raise AssertionError(f"unexpected register write {address:#x}")

    def _start(self):
        self.status = BUSY
        self.polls = 0

    def _poll(self):
        if self.status & BUSY:
            self.polls += 1
            if not self.stuck and self.polls == 4:
                self.status = self.completion
                if self.erase_address is not None and self.completion == DONE:
                    self.cpu.mem_write(self.erase_address, b"\xff" * PAGE_BYTES)

    def run(self, address, mode, *, word=0xABCD0123, masked=0, deadline=128):
        self.cpu.reg_write(UC_ARM_REG_SP, SRAM + SRAM_BYTES - 0x100)
        self.cpu.reg_write(UC_ARM_REG_LR, RETURN_PC | 1)
        self.cpu.reg_write(UC_ARM_REG_PRIMASK, masked)
        for register, value in zip(
            (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3),
            (address, word, mode, deadline),
        ):
            self.cpu.reg_write(register, value)
        self.cpu.emu_start(self.entry | 1, RETURN_PC + 2, count=3000)
        assert self.returned or self.awaiting_watchdog, "routine did not return or reach watchdog"
        if self.returned:
            assert self.cpu.reg_read(UC_ARM_REG_PRIMASK) == masked
            assert self.cpu.reg_read(UC_ARM_REG_PC) == RETURN_PC
            return self.cpu.reg_read(UC_ARM_REG_R0)
        return None


class FlashAssemblyTests(unittest.TestCase):
    code = None

    def test_erase_both_pages_with_write_only_address_register(self):
        for page in (JOURNAL_START, JOURNAL_START + PAGE_BYTES):
            with self.subTest(page=hex(page)):
                model = Controller(self.code)
                model.cpu.mem_write(page, b"\x00" * PAGE_BYTES)
                self.assertEqual(model.run(page, ERASE), 0)
                self.assertEqual(model.operations, [("erase", page)])
                self.assertEqual(bytes(model.cpu.mem_read(page, PAGE_BYTES)), b"\xff" * PAGE_BYTES)
                self.assertEqual(model.control, LOCKED)
                self.assertEqual(model.status, 0)

    def test_program_first_and_last_word_and_restore_irq_mask(self):
        for address, masked in [(JOURNAL_START, 0), (JOURNAL_END - 4, 1)]:
            with self.subTest(address=hex(address)):
                model = Controller(self.code, control=0)
                self.assertEqual(model.run(address, PROGRAM, masked=masked), 0)
                self.assertEqual(model.operations, [("program", address, 0xABCD0123)])
                self.assertEqual(bytes(model.cpu.mem_read(address, 4)), b"\x23\x01\xcd\xab")
                self.assertEqual(model.control, LOCKED)

    def test_out_of_range_unaligned_and_wrong_mode_refused_before_mmio(self):
        for address, mode in [
            (JOURNAL_START - 4, PROGRAM),
            (JOURNAL_END, PROGRAM),
            (JOURNAL_END - 2, PROGRAM),
            (JOURNAL_START - PAGE_BYTES, ERASE),
            (JOURNAL_END, ERASE),
            (JOURNAL_START + 4, ERASE),
            (JOURNAL_START, 4),
        ]:
            with self.subTest(address=hex(address), mode=mode):
                model = Controller(self.code)
                self.assertEqual(model.run(address, mode), 1)
                self.assertEqual(model.mmio, [])
                self.assertEqual(model.operations, [])

    def test_error_flags_missing_done_unlock_failure_and_inherited_mode(self):
        for flags in (0, 0x04, 0x10, 0x24):
            with self.subTest(flags=flags):
                model = Controller(self.code, completion=flags)
                self.assertEqual(model.run(JOURNAL_START, PROGRAM), 2)
                self.assertEqual(model.control, LOCKED)
                self.assertEqual(model.status, 0)
        for kwargs in ({"ignore_unlock": True}, {"control": 4 | LOCKED}):
            model = Controller(self.code, **kwargs)
            self.assertEqual(model.run(JOURNAL_START, ERASE), 1)
            self.assertEqual(model.operations, [])

    def test_busy_timeout_and_relock_failure_remain_in_masked_sram(self):
        for kwargs in (
            {"stuck": True},
            {"stuck": True, "frozen_counter": True},
            {"status": BUSY, "stuck": True},
        ):
            with self.subTest(**kwargs):
                model = Controller(self.code, **kwargs)
                self.assertIsNone(model.run(JOURNAL_START, ERASE, deadline=24))
                self.assertTrue(model.awaiting_watchdog)
                self.assertEqual(model.status, BUSY)
        model = Controller(self.code, ignore_relock=True)
        self.assertIsNone(model.run(JOURNAL_START, ERASE))
        self.assertTrue(model.awaiting_watchdog)
        self.assertEqual(model.status, DONE)
        self.assertEqual(model.control, ERASE)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    args = parser.parse_args()
    FlashAssemblyTests.code = load_code(args.elf)
    unittest.main(argv=[__file__], verbosity=2)
