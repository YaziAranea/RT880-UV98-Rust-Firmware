08003e08: 10b5       push      {r4, lr}
08003e0a: 0446       mov       r4, r0
08003e0c: 7f20       movs      r0, #0x7f
08003e0e: 0a49       ldr       r1, [pc, #0x28] ; literal[08003e38]=20000800
08003e10: 0880       strh      r0, [r1]
08003e12: a006       lsls      r0, r4, #0x1a
08003e14: 000e       lsrs      r0, r0, #0x18
08003e16: 4870       strb      r0, [r1, #1]
08003e18: 0cb1       cbz       r4, #0x8003e1e
08003e1a: 0120       movs      r0, #1
08003e1c: 00e0       b         #0x8003e20
08003e1e: 0020       movs      r0, #0
08003e20: 0549       ldr       r1, [pc, #0x14] ; literal[08003e38]=20000800
08003e22: 0978       ldrb      r1, [r1]
08003e24: 60f3c711   bfi       r1, r0, #7, #1
08003e28: 0348       ldr       r0, [pc, #0xc] ; literal[08003e38]=20000800
08003e2a: 0170       strb      r1, [r0]
08003e2c: 0188       ldrh      r1, [r0]
08003e2e: 3620       movs      r0, #0x36
08003e30: 1af050fc   bl        #0x801e6d4
08003e34: 10bd       pop       {r4, pc}
08003e36: 0000       movs      r0, r0
08003e38: 0008       lsrs      r0, r0, #0x20
08003e3a: 0020       movs      r0, #0
08003e3c: 10b5       push      {r4, lr}
08003e3e: 41f60f71   movw      r1, #0x1f0f
08003e42: 3720       movs      r0, #0x37
08003e44: 1af046fc   bl        #0x801e6d4
08003e48: 0a20       movs      r0, #0xa
08003e4a: 04f099fa   bl        #0x8008380
08003e4e: 4ff40071   mov.w     r1, #0x200
08003e52: 3020       movs      r0, #0x30
08003e54: 1af03efc   bl        #0x801e6d4
08003e58: 4bf6f171   movw      r1, #0xbff1
08003e5c: 3020       movs      r0, #0x30
08003e5e: 1af039fc   bl        #0x801e6d4
08003e62: 10bd       pop       {r4, pc}
08003e64: 10b5       push      {r4, lr}
08003e66: 0446       mov       r4, r0
08003e68: 1448       ldr       r0, [pc, #0x50] ; literal[08003ebc]=20000800
08003e6a: 0480       strh      r4, [r0]
08003e6c: 0088       ldrh      r0, [r0]
08003e6e: 00ebc010   add.w     r0, r0, r0, lsl #7
08003e72: 46f6dc01   movw      r1, #0x68dc
08003e76: 01ebc000   add.w     r0, r1, r0, lsl #3
08003e7a: 1049       ldr       r1, [pc, #0x40] ; literal[08003ebc]=20000800
08003e7c: 0880       strh      r0, [r1]
08003e7e: 0846       mov       r0, r1
08003e80: 0188       ldrh      r1, [r0]
08003e82: 7120       movs      r0, #0x71
08003e84: 1af026fc   bl        #0x801e6d4
08003e88: 3120       movs      r0, #0x31
08003e8a: 1af005fc   bl        #0x801e698
08003e8e: 0b49       ldr       r1, [pc, #0x2c] ; literal[08003ebc]=20000800
08003e90: 0880       strh      r0, [r1]
08003e92: 2cb9       cbnz      r4, #0x8003ea0
08003e94: 0846       mov       r0, r1
08003e96: 0078       ldrb      r0, [r0]
08003e98: 20f00200   bic       r0, r0, #2
08003e9c: 0870       strb      r0, [r1]
08003e9e: 06e0       b         #0x8003eae
08003ea0: 0648       ldr       r0, [pc, #0x18] ; literal[08003ebc]=20000800
08003ea2: 0078       ldrb      r0, [r0]
08003ea4: 20f00200   bic       r0, r0, #2
08003ea8: 801c       adds      r0, r0, #2
08003eaa: 0449       ldr       r1, [pc, #0x10] ; literal[08003ebc]=20000800
08003eac: 0870       strb      r0, [r1]
08003eae: 0348       ldr       r0, [pc, #0xc] ; literal[08003ebc]=20000800
08003eb0: 0188       ldrh      r1, [r0]
08003eb2: 3120       movs      r0, #0x31
08003eb4: 1af00efc   bl        #0x801e6d4
08003eb8: 10bd       pop       {r4, pc}
08003eba: 0000       movs      r0, r0
08003ebc: 0008       lsrs      r0, r0, #0x20
08003ebe: 0020       movs      r0, #0
08003ec0: 10b5       push      {r4, lr}
08003ec2: 0021       movs      r1, #0
08003ec4: 3020       movs      r0, #0x30
08003ec6: 1af005fc   bl        #0x801e6d4
08003eca: 4ff4e851   mov.w     r1, #0x1d00
08003ece: 3720       movs      r0, #0x37
08003ed0: 1af000fc   bl        #0x801e6d4
08003ed4: 10bd       pop       {r4, pc}
08003ed6: 10b5       push      {r4, lr}
08003ed8: 41f60f51   movw      r1, #0x1d0f
08003edc: 3720       movs      r0, #0x37
08003ede: 1af0f9fb   bl        #0x801e6d4
08003ee2: 0a20       movs      r0, #0xa
08003ee4: 04f04cfa   bl        #0x8008380
08003ee8: 40f28f21   movw      r1, #0x28f
08003eec: 5220       movs      r0, #0x52
08003eee: 1af0f1fb   bl        #0x801e6d4
08003ef2: 4ff40071   mov.w     r1, #0x200
08003ef6: 3020       movs      r0, #0x30
08003ef8: 1af0ecfb   bl        #0x801e6d4
08003efc: 4cf2fe11   movw      r1, #0xc1fe
08003f00: 3020       movs      r0, #0x30
08003f02: 1af0e7fb   bl        #0x801e6d4
08003f06: 10bd       pop       {r4, pc}
08003f08: 10b5       push      {r4, lr}
08003f0a: 3320       movs      r0, #0x33
08003f0c: 1af0c4fb   bl        #0x801e698
08003f10: 0649       ldr       r1, [pc, #0x18] ; literal[08003f2c]=20000800
08003f12: 0880       strh      r0, [r1]
08003f14: 0846       mov       r0, r1
08003f16: 0078       ldrb      r0, [r0]
08003f18: 00f0e000   and       r0, r0, #0xe0
08003f1c: 0870       strb      r0, [r1]
08003f1e: 0846       mov       r0, r1
08003f20: 0188       ldrh      r1, [r0]
08003f22: 3320       movs      r0, #0x33
08003f24: 1af0d6fb   bl        #0x801e6d4
08003f28: 10bd       pop       {r4, pc}
08003f2a: 0000       movs      r0, r0
08003f2c: 0008       lsrs      r0, r0, #0x20
08003f2e: 0020       movs      r0, #0
08003f30: 10b5       push      {r4, lr}
08003f32: 3320       movs      r0, #0x33
08003f34: 1af0b0fb   bl        #0x801e698
08003f38: 0649       ldr       r1, [pc, #0x18] ; literal[08003f54]=20000800
08003f3a: 0880       strh      r0, [r1]
08003f3c: 0846       mov       r0, r1
08003f3e: 0078       ldrb      r0, [r0]
08003f40: 40f01000   orr       r0, r0, #0x10
08003f44: 0870       strb      r0, [r1]
08003f46: 0846       mov       r0, r1
08003f48: 0188       ldrh      r1, [r0]
08003f4a: 3320       movs      r0, #0x33
08003f4c: 1af0c2fb   bl        #0x801e6d4
08003f50: 10bd       pop       {r4, pc}
08003f52: 0000       movs      r0, r0
08003f54: 0008       lsrs      r0, r0, #0x20
08003f56: 0020       movs      r0, #0
08003f58: 10b5       push      {r4, lr}
08003f5a: 00f05ef9   bl        #0x800421a
08003f5e: 8349       ldr       r1, [pc, #0x20c] ; literal[0800416c]=2000110e
08003f60: 0978       ldrb      r1, [r1]
08003f62: 09b9       cbnz      r1, #0x8003f68
08003f64: 0121       movs      r1, #1
08003f66: 00e0       b         #0x8003f6a
08003f68: 0021       movs      r1, #0
08003f6a: 01eb4101   add.w     r1, r1, r1, lsl #1
08003f6e: 804a       ldr       r2, [pc, #0x200] ; literal[08004170]=20003aec
08003f70: 02eb0111   add.w     r1, r2, r1, lsl #4
08003f74: c878       ldrb      r0, [r1, #3]
08003f76: 00f057f9   bl        #0x8004228
08003f7a: 0120       movs      r0, #1
08003f7c: fff744fd   bl        #0x8003a08
08003f80: fff74efe   bl        #0x8003c20
08003f84: 7b48       ldr       r0, [pc, #0x1ec] ; literal[08004174]=200035ec
08003f86: 90f86700   ldrb.w    r0, [r0, #0x67]
08003f8a: 0228       cmp       r0, #2
08003f8c: 0fd1       bne       #0x8003fae
08003f8e: 7749       ldr       r1, [pc, #0x1dc] ; literal[0800416c]=2000110e
08003f90: 0978       ldrb      r1, [r1]
08003f92: 09b9       cbnz      r1, #0x8003f98
08003f94: 0121       movs      r1, #1
08003f96: 00e0       b         #0x8003f9a
08003f98: 0021       movs      r1, #0
08003f9a: 01eb4101   add.w     r1, r1, r1, lsl #1
08003f9e: 744a       ldr       r2, [pc, #0x1d0] ; literal[08004170]=20003aec
08003fa0: 02eb0111   add.w     r1, r2, r1, lsl #4
08003fa4: d1f80600   ldr.w     r0, [r1, #6]
08003fa8: fff790fe   bl        #0x8003ccc
08003fac: 0ee0       b         #0x8003fcc
08003fae: 6f49       ldr       r1, [pc, #0x1bc] ; literal[0800416c]=2000110e
08003fb0: 0978       ldrb      r1, [r1]
08003fb2: 09b9       cbnz      r1, #0x8003fb8
08003fb4: 0121       movs      r1, #1
08003fb6: 00e0       b         #0x8003fba
08003fb8: 0021       movs      r1, #0
08003fba: 01eb4101   add.w     r1, r1, r1, lsl #1
08003fbe: 6c4a       ldr       r2, [pc, #0x1b0] ; literal[08004170]=20003aec
