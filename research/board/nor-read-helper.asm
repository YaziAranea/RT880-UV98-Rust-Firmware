08025d30: 70b5       push      {r4, r5, r6, lr}
08025d32: 0646       mov       r6, r0
08025d34: 0c46       mov       r4, r1
08025d36: 1546       mov       r5, r2
08025d38: 4ff48050   mov.w     r0, #0x1000
08025d3c: 0c49       ldr       r1, [pc, #0x30] ; literal[08025d70]=40020428
08025d3e: 0860       str       r0, [r1]
08025d40: 0320       movs      r0, #3
08025d42: fff783fe   bl        #0x8025a4c
08025d46: c4f30740   ubfx      r0, r4, #0x10, #8
08025d4a: fff77ffe   bl        #0x8025a4c
08025d4e: c4f30720   ubfx      r0, r4, #8, #8
08025d52: fff77bfe   bl        #0x8025a4c
08025d56: e0b2       uxtb      r0, r4
08025d58: fff778fe   bl        #0x8025a4c
08025d5c: 2946       mov       r1, r5
08025d5e: 3046       mov       r0, r6
08025d60: fff798fe   bl        #0x8025a94
08025d64: 4ff48050   mov.w     r0, #0x1000
08025d68: 0149       ldr       r1, [pc, #4] ; literal[08025d70]=40020428
08025d6a: 1039       subs      r1, #0x10
08025d6c: 0860       str       r0, [r1]
08025d6e: 70bd       pop       {r4, r5, r6, pc}
08025d70: 2804       lsls      r0, r5, #0x10
08025d72: 0240       ands      r2, r0
08025d74: 10b5       push      {r4, lr}
08025d76: 0024       movs      r4, #0
08025d78: 4ff48050   mov.w     r0, #0x1000
08025d7c: 0d49       ldr       r1, [pc, #0x34] ; literal[08025db4]=40020428
08025d7e: 0860       str       r0, [r1]
08025d80: 9020       movs      r0, #0x90
08025d82: fff763fe   bl        #0x8025a4c
08025d86: 0020       movs      r0, #0
08025d88: fff760fe   bl        #0x8025a4c
08025d8c: 0020       movs      r0, #0
08025d8e: fff75dfe   bl        #0x8025a4c
08025d92: 0020       movs      r0, #0
08025d94: fff75afe   bl        #0x8025a4c
08025d98: fff752fe   bl        #0x8025a40
08025d9c: 44ea0024   orr.w     r4, r4, r0, lsl #8
