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
