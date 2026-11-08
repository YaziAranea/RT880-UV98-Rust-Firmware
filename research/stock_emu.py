"""Bounded offline ARM oracle. No serial, network, or device mappings.

Only the reviewed RT880 reference hash is accepted. This runs small selected
functions in an emulator, not the firmware's boot flow or any host EXE.
Peripheral addresses are deliberately unmapped and cause a failure.
"""
from pathlib import Path
import hashlib
import os
import struct
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / 'deps'))
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_MODE_MCLASS, UC_PROT_READ, UC_PROT_WRITE, UC_PROT_EXEC, UC_HOOK_CODE
from unicorn.arm_const import (
    UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3,
    UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7,
    UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11,
    UC_ARM_REG_R12, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC,
    UC_ARM_REG_XPSR, UC_CPU_ARM_CORTEX_M4,
)

REFERENCE_SHA256 = 'eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc'
DEFAULT_IMAGE = Path(os.environ.get('UV98_STOCK_REFERENCE',
    str(HERE.parent / 'reference-firmware' / 'RT-880_Upgrade_V1_16_250915.nicsure.bin')))
GPRS = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3,
        UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7,
        UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11,
        UC_ARM_REG_R12]


class StockArm:
    STOP = 0x08000000

    def __init__(self, image=DEFAULT_IMAGE):
        data = Path(image).read_bytes()
        if hashlib.sha256(data).hexdigest() != REFERENCE_SHA256:
            raise ValueError('Unexpected reference firmware SHA256')
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB | UC_MODE_MCLASS)
        self.uc.ctl_set_cpu_model(UC_CPU_ARM_CORTEX_M4)
        self.uc.mem_map(0x08000000, 0x40000, UC_PROT_READ | UC_PROT_WRITE | UC_PROT_EXEC)
        self.uc.mem_write(0x08002800, data)
        self.uc.mem_protect(0x08000000, 0x40000, UC_PROT_READ | UC_PROT_EXEC)
        self.uc.mem_map(0x20000000, 0xC000, UC_PROT_READ | UC_PROT_WRITE)
        self.hooks = []

    def write(self, address, data):
        if not (0x20000000 <= address and address + len(data) <= 0x2000C000):
            raise ValueError('Host fixture writes are restricted to emulated RAM')
        self.uc.mem_write(address, bytes(data))

    def word(self, address, value):
        self.write(address, struct.pack('<I', value & 0xFFFFFFFF))

    def read(self, address, size):
        return bytes(self.uc.mem_read(address, size))

    def intercept(self, address, callback):
        """Replace a documented callee with a RAM-only test double.

        Callback receives this StockArm, may inspect registers/RAM, and must
        return the desired r0. Every substitution must be listed in evidence.
        """
        def hook(uc, pc, size, user):
            result = callback(self)
            if result is not None:
                uc.reg_write(UC_ARM_REG_R0, result & 0xFFFFFFFF)
            uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))
        self.hooks.append(self.uc.hook_add(UC_HOOK_CODE, hook, begin=address, end=address))

    def call(self, address, *args, count=100000, timeout_us=2000000):
        if len(args) > 4:
            raise ValueError('Only r0..r3 arguments supported; explicit stack setup required otherwise')
        for register in GPRS:
            self.uc.reg_write(register, 0)
        self.uc.reg_write(UC_ARM_REG_SP, 0x2000B000)
        self.uc.reg_write(UC_ARM_REG_LR, self.STOP | 1)
        self.uc.reg_write(UC_ARM_REG_XPSR, 0x01000000)
        for register, value in zip(GPRS, args):
            self.uc.reg_write(register, value & 0xFFFFFFFF)
        self.uc.emu_start(address | 1, self.STOP, timeout=timeout_us, count=count)
        if self.uc.reg_read(UC_ARM_REG_PC) != self.STOP:
            raise RuntimeError('Reference function did not return within the emulation budget')
        return tuple(self.uc.reg_read(register) for register in GPRS[:4])
