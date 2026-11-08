"""Offline bounded annotated disassembly and literal-reference inventory."""
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "work/hardware_readback/deps"))
from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_LITTLE_ENDIAN
from capstone.arm import ARM_OP_MEM, ARM_REG_PC

BASE = 0x08002800
DATA = (ROOT / "work/reference-firmware/RT-880_Upgrade_V1_16_250915.nicsure.bin").read_bytes()
MD = Cs(CS_ARCH_ARM, CS_MODE_THUMB | CS_MODE_LITTLE_ENDIAN)
MD.detail = True
MD.skipdata = True

def u32(addr):
    return struct.unpack_from("<I", DATA, addr - BASE)[0]

def annotate(ins):
    comment = ""
    if ins.id:
        for op in ins.operands:
            if op.type == ARM_OP_MEM and op.mem.base == ARM_REG_PC:
                loc = ((ins.address + 4) & ~3) + op.mem.disp
                if BASE <= loc < BASE + len(DATA) - 3:
                    comment = f" ; literal[{loc:08x}]={u32(loc):08x}"
    return f"{ins.address:08x}: {ins.bytes.hex():<10} {ins.mnemonic:<9} {ins.op_str}{comment}"

def disasm(lo, hi):
    return "\n".join(annotate(i) for i in MD.disasm(DATA[lo - BASE:hi - BASE], lo))

if __name__ == "__main__":
    if sys.argv[1] == "range":
        print(disasm(int(sys.argv[2], 16), int(sys.argv[3], 16)))
    elif sys.argv[1] in ("refs", "ref-range"):
        targets = ({int(x, 16) for x in sys.argv[2:]} if sys.argv[1] == "refs"
                   else set(range(int(sys.argv[2], 16), int(sys.argv[3], 16))))
        for off in range(0, len(DATA) - 3, 2):
            for ins in MD.disasm(DATA[off:off+4], BASE+off, count=1):
                if not ins.mnemonic.startswith("ldr"):
                    continue
                line = annotate(ins)
                if any(f"={value:08x}" in line for value in targets):
                    print(line)
    elif sys.argv[1] == "data":
        lo, hi = (int(x, 16) for x in sys.argv[2:])
        print(DATA[lo - BASE:hi - BASE].hex(" "))
    elif sys.argv[1] == "evidence":
        ranges = {
            "general_load": (0x08013E3C, 0x08013EE4),
            "byte_copy": (0x080081C4, 0x080081DE),
            "adc_dma_config": (0x0800822C, 0x080082B0),
            "adc_conversion": (0x080121A8, 0x08012204),
            "battery_indicator": (0x080122AC, 0x0801234A),
            "noise_word": (0x08004ECC, 0x08004F0C),
            "squelch_word": (0x08004F60, 0x08004FA0),
            "squelch_selection": (0x08004CCA, 0x08004D5C),
            "vox_compare": (0x08005708, 0x08005768),
            "vox_controller": (0x080227C4, 0x080228C0),
        }
        for name, (lo, hi) in ranges.items():
            (Path(__file__).parent / (name + ".asm")).write_text(disasm(lo, hi) + "\n", encoding="utf-8")
        print("Saved", len(ranges), "bounded annotated disassemblies")
