"""Save bounded key-path excerpts from a pinned related reference image.

Read-only source analysis, no device or firmware execution. Requires Capstone.
"""
from pathlib import Path
import argparse
import hashlib
import os
import struct
import sys

HERE = Path(__file__).resolve().parent
EXPECTED = "eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc"
BASE = 0x08002800
RANGES = {
    "matrix-code": (0x08013084, 0x0801317C),
    "auxiliary-code": (0x08013D68, 0x08013DA0),
    "matrix-rows": (0x0801FA9C, 0x0801FB5C),
    "matrix-column-order": (0x0801FBA0, 0x0801FC44),
    "matrix-init-excerpt": (0x08011C7E, 0x08011CEC),
    "gpio-input-helper": (0x080254DE, 0x080254F0),
    "key-priority-ptt-excerpt": (0x08007980, 0x080079C8),
    "ptt-consumer-excerpt": (0x08022270, 0x080222A0),
    "adc-source": (0x080121A8, 0x08012204),
    "pc01-alternate-function": (0x0801EADC, 0x0801EB30),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", default=os.environ.get("UV98_STOCK_REFERENCE"))
    parser.add_argument("--deps", default=os.environ.get("UV98_RESEARCH_DEPS"))
    args = parser.parse_args()
    if not args.reference:
        parser.error("--reference or UV98_STOCK_REFERENCE is required")
    if args.deps:
        sys.path.insert(0, str(Path(args.deps).resolve()))
    from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_LITTLE_ENDIAN
    from capstone.arm import ARM_OP_MEM, ARM_REG_PC
    data = Path(args.reference).read_bytes()
    if hashlib.sha256(data).hexdigest() != EXPECTED:
        raise ValueError("Unexpected related reference firmware SHA256")
    md = Cs(CS_ARCH_ARM, CS_MODE_THUMB | CS_MODE_LITTLE_ENDIAN)
    md.detail = True
    md.skipdata = True
    for name, (lo, hi) in RANGES.items():
        lines = ["; Bounded static excerpt; literal pools may decode as instructions.",
                 "; Reference SHA256: " + EXPECTED]
        for ins in md.disasm(data[lo - BASE:hi - BASE], lo):
            comment = ""
            if ins.id:
                for operand in ins.operands:
                    if operand.type == ARM_OP_MEM and operand.mem.base == ARM_REG_PC:
                        loc = ((ins.address + 4) & ~3) + operand.mem.disp
                        if BASE <= loc <= BASE + len(data) - 4:
                            literal = struct.unpack_from("<I", data, loc - BASE)[0]
                            comment = f" ; literal[{loc:08x}]={literal:08x}"
            lines.append(f"{ins.address:08x}: {ins.bytes.hex():<10} "
                         f"{ins.mnemonic:<9} {ins.op_str}{comment}")
        (HERE / (name + ".asm")).write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Saved {len(RANGES)} bounded excerpts")


if __name__ == "__main__":
    main()
