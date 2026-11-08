08004f60: 10b5       push      {r4, lr}
08004f62: 0446       mov       r4, r0
08004f64: 0b48       ldr       r0, [pc, #0x2c] ; literal[08004f94]=200035ec
08004f66: 90f8a400   ldrb.w    r0, [r0, #0xa4]
08004f6a: 0b49       ldr       r1, [pc, #0x2c] ; literal[08004f98]=0802637b
08004f6c: 095c       ldrb      r1, [r1, r0]
08004f6e: a01c       adds      r0, r4, #2
08004f70: 0844       add       r0, r1
08004f72: 0a49       ldr       r1, [pc, #0x28] ; literal[08004f9c]=20000800
08004f74: 4870       strb      r0, [r1, #1]
08004f76: 201f       subs      r0, r4, #4
08004f78: 0649       ldr       r1, [pc, #0x18] ; literal[08004f94]=200035ec
08004f7a: 91f8a410   ldrb.w    r1, [r1, #0xa4]
08004f7e: 064a       ldr       r2, [pc, #0x18] ; literal[08004f98]=0802637b
08004f80: 515c       ldrb      r1, [r2, r1]
08004f82: 0844       add       r0, r1
08004f84: 0549       ldr       r1, [pc, #0x14] ; literal[08004f9c]=20000800
08004f86: 0870       strb      r0, [r1]
08004f88: 0846       mov       r0, r1
08004f8a: 0188       ldrh      r1, [r0]
08004f8c: 7820       movs      r0, #0x78
08004f8e: 19f067fb   bl        #0x801e660
08004f92: 10bd       pop       {r4, pc}
08004f94: ec35       adds      r5, #0xec
08004f96: 0020       movs      r0, #0
08004f98: 7b63       str       r3, [r7, #0x34]
08004f9a: 0208       lsrs      r2, r0, #0x20
08004f9c: 0008       lsrs      r0, r0, #0x20
08004f9e: 0020       movs      r0, #0
