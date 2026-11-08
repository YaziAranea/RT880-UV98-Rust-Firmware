"""Static Thumb disassembly helper; never executes the target firmware."""
from pathlib import Path
import sys
import struct

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "hardware_readback" / "deps"))
from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_LITTLE_ENDIAN
from capstone.arm import ARM_OP_MEM, ARM_REG_PC

BASE = 0x08002800
DATA = (ROOT / "reference-firmware" / "RT-880_Upgrade_V1_16_250915.nicsure.bin").read_bytes()


def disasm(start, end):
    md = Cs(CS_ARCH_ARM, CS_MODE_THUMB | CS_MODE_LITTLE_ENDIAN)
    md.detail = True
    md.skipdata = True
    lines = []
    for ins in md.disasm(DATA[start - BASE:end - BASE], start):
        comment = ""
        if ins.id and ins.mnemonic.startswith("ldr"):
            for operand in ins.operands:
                if operand.type == ARM_OP_MEM and operand.mem.base == ARM_REG_PC:
                    address = ((ins.address + 4) & ~3) + operand.mem.disp
                    if BASE <= address <= BASE + len(DATA) - 4:
                        word = struct.unpack_from("<I", DATA, address - BASE)[0]
                        comment = f" ; literal[{address:08x}]={word:08x}"
        lines.append(f"{ins.address:08x}: {ins.bytes.hex():<10} {ins.mnemonic:<9} {ins.op_str}{comment}")
    return "\n".join(lines)


if __name__ == "__main__":
    start, end = (int(x, 0) for x in sys.argv[1:3])
    text = disasm(start, end)
    if len(sys.argv) > 3:
        Path(sys.argv[3]).write_text(text + "\n", encoding="utf-8")
    else:
        print(text)
