08010c20: 02c0       stm       r0!, {r1}
08010c22: 0025       movs      r5, #0
08010c24: 0ce0       b         #0x8010c40
08010c26: 4cfa05f0   asr.w     r0, ip, r5
08010c2a: 80ea0108   eor.w     r8, r0, r1
08010c2e: 4910       asrs      r1, r1, #1
08010c30: 08f00100   and       r0, r8, #1
08010c34: 10b1       cbz       r0, #0x8010c3c
08010c36: 48f20840   movw      r0, #0x8408
08010c3a: 4140       eors      r1, r0
08010c3c: 681c       adds      r0, r5, #1
08010c3e: c5b2       uxtb      r5, r0
08010c40: 082d       cmp       r5, #8
08010c42: f0db       blt       #0x8010c26
08010c44: 501c       adds      r0, r2, #1
08010c46: 82b2       uxth      r2, r0
08010c48: a01e       subs      r0, r4, #2
08010c4a: 9042       cmp       r0, r2
08010c4c: e7dc       bgt       #0x8010c1e
08010c4e: c843       mvns      r0, r1
08010c50: 81b2       uxth      r1, r0
08010c52: 37b1       cbz       r7, #0x8010c62
08010c54: b142       cmp       r1, r6
08010c56: 02d0       beq       #0x8010c5e
08010c58: 0020       movs      r0, #0
08010c5a: bde8f083   pop.w     {r4, r5, r6, r7, r8, sb, pc}
08010c5e: 0120       movs      r0, #1
08010c60: fbe7       b         #0x8010c5a
08010c62: 9954       strb      r1, [r3, r2]
08010c64: 4fea1129   lsr.w     sb, r1, #8
08010c68: 501c       adds      r0, r2, #1
08010c6a: 03f80090   strb.w    sb, [r3, r0]
08010c6e: 0120       movs      r0, #1
08010c70: f3e7       b         #0x8010c5a
08010c72: 0000       movs      r0, r0
08010c74: 1fb5       push      {r0, r1, r2, r3, r4, lr}
08010c76: 02a8       add       r0, sp, #8
08010c78: 14f0eafb   bl        #0x8025450
08010c7c: 0020       movs      r0, #0
08010c7e: 8df80c00   strb.w    r0, [sp, #0xc]
08010c82: 0120       movs      r0, #1
08010c84: 8df80d00   strb.w    r0, [sp, #0xd]
08010c88: 8df80e00   strb.w    r0, [sp, #0xe]
08010c8c: 8df80f00   strb.w    r0, [sp, #0xf]
08010c90: 0003       lsls      r0, r0, #0xc
08010c92: 0290       str       r0, [sp, #8]
08010c94: 02a9       add       r1, sp, #8
08010c96: 2a48       ldr       r0, [pc, #0xa8] ; literal[08010d40]=40020400
08010c98: 14f0e4fb   bl        #0x8025464
08010c9c: 0120       movs      r0, #1
08010c9e: 8df80d00   strb.w    r0, [sp, #0xd]
08010ca2: 0220       movs      r0, #2
08010ca4: 8df80e00   strb.w    r0, [sp, #0xe]
08010ca8: 0003       lsls      r0, r0, #0xc
08010caa: 0290       str       r0, [sp, #8]
08010cac: 02a9       add       r1, sp, #8
08010cae: 2448       ldr       r0, [pc, #0x90] ; literal[08010d40]=40020400
08010cb0: 14f0d8fb   bl        #0x8025464
08010cb4: 0522       movs      r2, #5
08010cb6: 0d21       movs      r1, #0xd
08010cb8: 2148       ldr       r0, [pc, #0x84] ; literal[08010d40]=40020400
08010cba: 14f019fc   bl        #0x80254f0
08010cbe: 0120       movs      r0, #1
08010cc0: 8df80d00   strb.w    r0, [sp, #0xd]
08010cc4: 8003       lsls      r0, r0, #0xe
08010cc6: 0290       str       r0, [sp, #8]
08010cc8: 02a9       add       r1, sp, #8
08010cca: 1d48       ldr       r0, [pc, #0x74] ; literal[08010d40]=40020400
08010ccc: 14f0cafb   bl        #0x8025464
08010cd0: 0522       movs      r2, #5
08010cd2: 0e21       movs      r1, #0xe
08010cd4: 1a48       ldr       r0, [pc, #0x68] ; literal[08010d40]=40020400
08010cd6: 14f00bfc   bl        #0x80254f0
08010cda: 0120       movs      r0, #1
08010cdc: 8df80d00   strb.w    r0, [sp, #0xd]
08010ce0: c003       lsls      r0, r0, #0xf
08010ce2: 0290       str       r0, [sp, #8]
08010ce4: 02a9       add       r1, sp, #8
08010ce6: 1648       ldr       r0, [pc, #0x58] ; literal[08010d40]=40020400
08010ce8: 14f0bcfb   bl        #0x8025464
08010cec: 0522       movs      r2, #5
08010cee: 0f21       movs      r1, #0xf
08010cf0: 1348       ldr       r0, [pc, #0x4c] ; literal[08010d40]=40020400
08010cf2: 14f0fdfb   bl        #0x80254f0
08010cf6: 4ff48050   mov.w     r0, #0x1000
08010cfa: 1149       ldr       r1, [pc, #0x44] ; literal[08010d40]=40020400
08010cfc: 1831       adds      r1, #0x18
08010cfe: 0860       str       r0, [r1]
08010d00: 6846       mov       r0, sp
08010d02: 14f00fff   bl        #0x8025b24
08010d06: 0020       movs      r0, #0
08010d08: 8df80000   strb.w    r0, [sp]
08010d0c: 0120       movs      r0, #1
08010d0e: 8df80100   strb.w    r0, [sp, #1]
08010d12: 0020       movs      r0, #0
08010d14: 8df80200   strb.w    r0, [sp, #2]
08010d18: 8df80300   strb.w    r0, [sp, #3]
08010d1c: 8df80400   strb.w    r0, [sp, #4]
08010d20: 0120       movs      r0, #1
08010d22: 8df80500   strb.w    r0, [sp, #5]
08010d26: 8df80600   strb.w    r0, [sp, #6]
08010d2a: 8df80700   strb.w    r0, [sp, #7]
08010d2e: 6946       mov       r1, sp
08010d30: 0448       ldr       r0, [pc, #0x10] ; literal[08010d44]=40003800
08010d32: 14f024ff   bl        #0x8025b7e
08010d36: 0121       movs      r1, #1
08010d38: 0248       ldr       r0, [pc, #8] ; literal[08010d44]=40003800
08010d3a: 14f0fefe   bl        #0x8025b3a
08010d3e: 1fbd       pop       {r0, r1, r2, r3, r4, pc}
08010d40: 0004       lsls      r0, r0, #0x10
08010d42: 0240       ands      r2, r0
08010d44: 0038       subs      r0, #0
08010d46: 0040       ands      r0, r0
08010d48: 10b5       push      {r4, lr}
08010d4a: 1e48       ldr       r0, [pc, #0x78] ; literal[08010dc4]=200035cc
08010d4c: b0f80500   ldrh.w    r0, [r0, #5]
08010d50: b0f5004f   cmp.w     r0, #0x8000
08010d54: 06d1       bne       #0x8010d64
08010d56: 0623       movs      r3, #6
08010d58: 0022       movs      r2, #0
08010d5a: 1ba1       adr       r1, #0x6c
08010d5c: 1c48       ldr       r0, [pc, #0x70] ; literal[08010dd0]=200008dc
08010d5e: f7f724fa   bl        #0x80081aa
08010d62: 2ee0       b         #0x8010dc2
08010d64: 1748       ldr       r0, [pc, #0x5c] ; literal[08010dc4]=200035cc
08010d66: b0f80500   ldrh.w    r0, [r0, #5]
08010d6a: b0f5004f   cmp.w     r0, #0x8000
08010d6e: 14dd       ble       #0x8010d9a
