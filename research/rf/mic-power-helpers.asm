08004e90: 10b5       push      {r4, lr}
08004e92: 0446       mov       r4, r0
08004e94: 4ef64010   movw      r0, #0xe940
08004e98: 2044       add       r0, r4
08004e9a: 0449       ldr       r1, [pc, #0x10] ; literal[08004eac]=20000800
08004e9c: 0880       strh      r0, [r1]
08004e9e: 0846       mov       r0, r1
08004ea0: 0188       ldrh      r1, [r0]
08004ea2: 7d20       movs      r0, #0x7d
08004ea4: 19f0dcfb   bl        #0x801e660
08004ea8: 10bd       pop       {r4, pc}
08004eaa: 0000       movs      r0, r0
08004eac: 0008       lsrs      r0, r0, #0x20
08004eae: 0020       movs      r0, #0
08004eb0: 10b5       push      {r4, lr}
08004eb2: 6520       movs      r0, #0x65
08004eb4: 19f0b4fb   bl        #0x801e620
08004eb8: 0349       ldr       r1, [pc, #0xc] ; literal[08004ec8]=20000800
08004eba: 0880       strh      r0, [r1]
08004ebc: 0846       mov       r0, r1
08004ebe: 0078       ldrb      r0, [r0]
08004ec0: 00f07f00   and       r0, r0, #0x7f
08004ec4: 10bd       pop       {r4, pc}
08004ec6: 0000       movs      r0, r0
08004ec8: 0008       lsrs      r0, r0, #0x20
08004eca: 0020       movs      r0, #0
08004ecc: 10b5       push      {r4, lr}
08004ece: 0446       mov       r4, r0
08004ed0: 0b48       ldr       r0, [pc, #0x2c] ; literal[08004f00]=200035ec
08004ed2: 90f8a400   ldrb.w    r0, [r0, #0xa4]
08004ed6: 0b49       ldr       r1, [pc, #0x2c] ; literal[08004f04]=08026370
08004ed8: 095c       ldrb      r1, [r1, r0]
08004eda: a01c       adds      r0, r4, #2
08004edc: 0844       add       r0, r1
08004ede: 0a49       ldr       r1, [pc, #0x28] ; literal[08004f08]=20000800
08004ee0: 4870       strb      r0, [r1, #1]
08004ee2: 201f       subs      r0, r4, #4
08004ee4: 0649       ldr       r1, [pc, #0x18] ; literal[08004f00]=200035ec
08004ee6: 91f8a410   ldrb.w    r1, [r1, #0xa4]
08004eea: 064a       ldr       r2, [pc, #0x18] ; literal[08004f04]=08026370
08004eec: 515c       ldrb      r1, [r2, r1]
08004eee: 0844       add       r0, r1
08004ef0: 0549       ldr       r1, [pc, #0x14] ; literal[08004f08]=20000800
08004ef2: 0870       strb      r0, [r1]
08004ef4: 0846       mov       r0, r1
08004ef6: 0188       ldrh      r1, [r0]
08004ef8: 4f20       movs      r0, #0x4f
08004efa: 19f0b1fb   bl        #0x801e660
08004efe: 10bd       pop       {r4, pc}
08004f00: ec35       adds      r5, #0xec
08004f02: 0020       movs      r0, #0
08004f04: 7063       str       r0, [r6, #0x34]
08004f06: 0208       lsrs      r2, r0, #0x20
08004f08: 0008       lsrs      r0, r0, #0x20
08004f0a: 0020       movs      r0, #0
08004f0c: 10b5       push      {r4, lr}
08004f0e: 0446       mov       r4, r0
08004f10: 7f20       movs      r0, #0x7f
08004f12: 0a49       ldr       r1, [pc, #0x28] ; literal[08004f3c]=20000800
08004f14: 0880       strh      r0, [r1]
08004f16: a006       lsls      r0, r4, #0x1a
08004f18: 000e       lsrs      r0, r0, #0x18
08004f1a: 4870       strb      r0, [r1, #1]
08004f1c: 0cb1       cbz       r4, #0x8004f22
08004f1e: 0120       movs      r0, #1
08004f20: 00e0       b         #0x8004f24
08004f22: 0020       movs      r0, #0
08004f24: 0549       ldr       r1, [pc, #0x14] ; literal[08004f3c]=20000800
08004f26: 0978       ldrb      r1, [r1]
08004f28: 60f3c711   bfi       r1, r0, #7, #1
08004f2c: 0348       ldr       r0, [pc, #0xc] ; literal[08004f3c]=20000800
08004f2e: 0170       strb      r1, [r0]
08004f30: 0188       ldrh      r1, [r0]
08004f32: 3620       movs      r0, #0x36
08004f34: 19f094fb   bl        #0x801e660
08004f38: 10bd       pop       {r4, pc}
08004f3a: 0000       movs      r0, r0
08004f3c: 0008       lsrs      r0, r0, #0x20
08004f3e: 0020       movs      r0, #0
