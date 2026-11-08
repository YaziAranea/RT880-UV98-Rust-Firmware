08004bf0: 10b5       push      {r4, lr}
08004bf2: 6448       ldr       r0, [pc, #0x190] ; literal[08004d84]=20001125
08004bf4: 0078       ldrb      r0, [r0]
08004bf6: 0328       cmp       r0, #3
08004bf8: 16d1       bne       #0x8004c28
08004bfa: 4ff48040   mov.w     r0, #0x4000
08004bfe: 6249       ldr       r1, [pc, #0x188] ; literal[08004d88]=40020000
08004c00: 8862       str       r0, [r1, #0x28]
08004c02: 624a       ldr       r2, [pc, #0x188] ; literal[08004d8c]=200010ec
08004c04: 1268       ldr       r2, [r2]
08004c06: d27b       ldrb      r2, [r2, #0xf]
08004c08: 1109       lsrs      r1, r2, #4
08004c0a: 604a       ldr       r2, [pc, #0x180] ; literal[08004d8c]=200010ec
08004c0c: 1268       ldr       r2, [r2]
08004c0e: d27c       ldrb      r2, [r2, #0x13]
08004c10: c2f30210   ubfx      r0, r2, #4, #3
08004c14: 00f0f8fc   bl        #0x8005608
08004c18: 00f07efb   bl        #0x8005318
08004c1c: 0020       movs      r0, #0
08004c1e: 00f075f9   bl        #0x8004f0c
08004c22: 1420       movs      r0, #0x14
08004c24: 03f0acfb   bl        #0x8008380
08004c28: 5648       ldr       r0, [pc, #0x158] ; literal[08004d84]=20001125
08004c2a: 0078       ldrb      r0, [r0]
08004c2c: 0028       cmp       r0, #0
08004c2e: 0bdd       ble       #0x8004c48
08004c30: fef7c6fe   bl        #0x80039c0
08004c34: fef70cf9   bl        #0x8002e50
08004c38: fef710f9   bl        #0x8002e5c
08004c3c: 0020       movs      r0, #0
08004c3e: fef735fe   bl        #0x80038ac
08004c42: 0420       movs      r0, #4
08004c44: 19f03afb   bl        #0x801e2bc
08004c48: 5049       ldr       r1, [pc, #0x140] ; literal[08004d8c]=200010ec
08004c4a: 0968       ldr       r1, [r1]
08004c4c: d1f80600   ldr.w     r0, [r1, #6]
08004c50: fff79aff   bl        #0x8004b88
08004c54: 4d48       ldr       r0, [pc, #0x134] ; literal[08004d8c]=200010ec
08004c56: 0068       ldr       r0, [r0]
08004c58: 807c       ldrb      r0, [r0, #0x12]
08004c5a: 0421       movs      r1, #4
08004c5c: b1eb501f   cmp.w     r1, r0, lsr #5
08004c60: 1bdd       ble       #0x8004c9a
08004c62: 4a48       ldr       r0, [pc, #0x128] ; literal[08004d8c]=200010ec
08004c64: 0068       ldr       r0, [r0]
08004c66: 4079       ldrb      r0, [r0, #5]
08004c68: 0009       lsrs      r0, r0, #4
08004c6a: 4949       ldr       r1, [pc, #0x124] ; literal[08004d90]=20000808
08004c6c: 0880       strh      r0, [r1]
08004c6e: 4748       ldr       r0, [pc, #0x11c] ; literal[08004d8c]=200010ec
08004c70: 0068       ldr       r0, [r0]
08004c72: 8088       ldrh      r0, [r0, #4]
08004c74: c0f30b00   ubfx      r0, r0, #0, #0xc
08004c78: 4649       ldr       r1, [pc, #0x118] ; literal[08004d94]=2000080a
08004c7a: 0880       strh      r0, [r1]
08004c7c: 0846       mov       r0, r1
08004c7e: 0088       ldrh      r0, [r0]
08004c80: 4549       ldr       r1, [pc, #0x114] ; literal[08004d98]=2000080e
08004c82: 0880       strh      r0, [r1]
08004c84: 4149       ldr       r1, [pc, #0x104] ; literal[08004d8c]=200010ec
08004c86: 0968       ldr       r1, [r1]
08004c88: 897c       ldrb      r1, [r1, #0x12]
08004c8a: 4a09       lsrs      r2, r1, #5
08004c8c: 4049       ldr       r1, [pc, #0x100] ; literal[08004d90]=20000808
08004c8e: 0878       ldrb      r0, [r1]
08004c90: 4049       ldr       r1, [pc, #0x100] ; literal[08004d94]=2000080a
08004c92: 0988       ldrh      r1, [r1]
08004c94: fff778fb   bl        #0x8004388
08004c98: 04e0       b         #0x8004ca4
08004c9a: 3c49       ldr       r1, [pc, #0xf0] ; literal[08004d8c]=200010ec
08004c9c: 0968       ldr       r1, [r1]
08004c9e: 4869       ldr       r0, [r1, #0x14]
08004ca0: fff758fc   bl        #0x8004554
08004ca4: 3948       ldr       r0, [pc, #0xe4] ; literal[08004d8c]=200010ec
08004ca6: 0068       ldr       r0, [r0]
08004ca8: 807c       ldrb      r0, [r0, #0x12]
08004caa: 4009       lsrs      r0, r0, #5
08004cac: 3b49       ldr       r1, [pc, #0xec] ; literal[08004d9c]=2000080c
08004cae: 0870       strb      r0, [r1]
08004cb0: 3b48       ldr       r0, [pc, #0xec] ; literal[08004da0]=200035ec
08004cb2: 90f8b200   ldrb.w    r0, [r0, #0xb2]
08004cb6: 0b21       movs      r1, #0xb
08004cb8: 90fbf1f2   sdiv      r2, r0, r1
08004cbc: 01fb1200   mls       r0, r1, r2, r0
08004cc0: 3849       ldr       r1, [pc, #0xe0] ; literal[08004da4]=08026365
08004cc2: 0c5c       ldrb      r4, [r1, r0]
08004cc4: 2046       mov       r0, r4
08004cc6: fff783ff   bl        #0x8004bd0
08004cca: 3748       ldr       r0, [pc, #0xdc] ; literal[08004da8]=200011f9
08004ccc: 0078       ldrb      r0, [r0]
08004cce: a8b9       cbnz      r0, #0x8004cfc
08004cd0: 2e48       ldr       r0, [pc, #0xb8] ; literal[08004d8c]=200010ec
08004cd2: 0068       ldr       r0, [r0]
08004cd4: c078       ldrb      r0, [r0, #3]
08004cd6: 40b9       cbnz      r0, #0x8004cea
08004cd8: 3448       ldr       r0, [pc, #0xd0] ; literal[08004dac]=2000081d
08004cda: 0078       ldrb      r0, [r0]
08004cdc: 00f0f6f8   bl        #0x8004ecc
08004ce0: 3348       ldr       r0, [pc, #0xcc] ; literal[08004db0]=2000081f
08004ce2: 0078       ldrb      r0, [r0]
08004ce4: 00f03cf9   bl        #0x8004f60
08004ce8: 38e0       b         #0x8004d5c
08004cea: 3248       ldr       r0, [pc, #0xc8] ; literal[08004db4]=2000081e
08004cec: 0078       ldrb      r0, [r0]
08004cee: 00f0edf8   bl        #0x8004ecc
08004cf2: 3148       ldr       r0, [pc, #0xc4] ; literal[08004db8]=20000820
08004cf4: 0078       ldrb      r0, [r0]
08004cf6: 00f033f9   bl        #0x8004f60
08004cfa: 2fe0       b         #0x8004d5c
08004cfc: 2848       ldr       r0, [pc, #0xa0] ; literal[08004da0]=200035ec
08004cfe: 90f8b500   ldrb.w    r0, [r0, #0xb5]
08004d02: 2e49       ldr       r1, [pc, #0xb8] ; literal[08004dbc]=20000800
08004d04: 4870       strb      r0, [r1, #1]
08004d06: 2648       ldr       r0, [pc, #0x98] ; literal[08004da0]=200035ec
08004d08: 90f8b500   ldrb.w    r0, [r0, #0xb5]
08004d0c: 0628       cmp       r0, #6
08004d0e: 05dd       ble       #0x8004d1c
08004d10: 2348       ldr       r0, [pc, #0x8c] ; literal[08004da0]=200035ec
08004d12: 90f8b500   ldrb.w    r0, [r0, #0xb5]
08004d16: 801f       subs      r0, r0, #6
08004d18: 0870       strb      r0, [r1]
08004d1a: 02e0       b         #0x8004d22
08004d1c: 0020       movs      r0, #0
08004d1e: 2749       ldr       r1, [pc, #0x9c] ; literal[08004dbc]=20000800
08004d20: 0870       strb      r0, [r1]
08004d22: 2648       ldr       r0, [pc, #0x98] ; literal[08004dbc]=20000800
08004d24: 0188       ldrh      r1, [r0]
08004d26: 4f20       movs      r0, #0x4f
08004d28: 19f09afc   bl        #0x801e660
08004d2c: 1c48       ldr       r0, [pc, #0x70] ; literal[08004da0]=200035ec
08004d2e: 90f8b400   ldrb.w    r0, [r0, #0xb4]
08004d32: 2249       ldr       r1, [pc, #0x88] ; literal[08004dbc]=20000800
08004d34: 4870       strb      r0, [r1, #1]
08004d36: 1a48       ldr       r0, [pc, #0x68] ; literal[08004da0]=200035ec
08004d38: 90f8b400   ldrb.w    r0, [r0, #0xb4]
08004d3c: 0628       cmp       r0, #6
08004d3e: 05dd       ble       #0x8004d4c
08004d40: 1748       ldr       r0, [pc, #0x5c] ; literal[08004da0]=200035ec
08004d42: 90f8b400   ldrb.w    r0, [r0, #0xb4]
08004d46: 801f       subs      r0, r0, #6
08004d48: 0870       strb      r0, [r1]
08004d4a: 02e0       b         #0x8004d52
08004d4c: 0020       movs      r0, #0
08004d4e: 1b49       ldr       r1, [pc, #0x6c] ; literal[08004dbc]=20000800
08004d50: 0870       strb      r0, [r1]
08004d52: 1a48       ldr       r0, [pc, #0x68] ; literal[08004dbc]=20000800
08004d54: 0188       ldrh      r1, [r0]
08004d56: 7820       movs      r0, #0x78
08004d58: 19f082fc   bl        #0x801e660
08004d5c: 0b48       ldr       r0, [pc, #0x2c] ; literal[08004d8c]=200010ec
08004d5e: 0068       ldr       r0, [r0]
08004d60: 0078       ldrb      r0, [r0]
08004d62: 0128       cmp       r0, #1
08004d64: 03dd       ble       #0x8004d6e
08004d66: 0020       movs      r0, #0
08004d68: 00f084fa   bl        #0x8005274
08004d6c: 02e0       b         #0x8004d74
08004d6e: 0120       movs      r0, #1
08004d70: 00f080fa   bl        #0x8005274
08004d74: 00f014f9   bl        #0x8004fa0
08004d78: 00f03af9   bl        #0x8004ff0
08004d7c: 0020       movs      r0, #0
08004d7e: 0149       ldr       r1, [pc, #4] ; literal[08004d84]=20001125
08004d80: 0870       strb      r0, [r1]
08004d82: 10bd       pop       {r4, pc}
08004d84: 2511       asrs      r5, r4, #4
08004d86: 0020       movs      r0, #0
08004d88: 0000       movs      r0, r0
08004d8a: 0240       ands      r2, r0
08004d8c: ec10       asrs      r4, r5, #3
08004d8e: 0020       movs      r0, #0
08004d90: 0808       lsrs      r0, r1, #0x20
08004d92: 0020       movs      r0, #0
08004d94: 0a08       lsrs      r2, r1, #0x20
08004d96: 0020       movs      r0, #0
08004d98: 0e08       lsrs      r6, r1, #0x20
08004d9a: 0020       movs      r0, #0
08004d9c: 0c08       lsrs      r4, r1, #0x20
08004d9e: 0020       movs      r0, #0
08004da0: ec35       adds      r5, #0xec
08004da2: 0020       movs      r0, #0
08004da4: 6563       str       r5, [r4, #0x34]
08004da6: 0208       lsrs      r2, r0, #0x20
08004da8: f911       asrs      r1, r7, #7
08004daa: 0020       movs      r0, #0
08004dac: 1d08       lsrs      r5, r3, #0x20
08004dae: 0020       movs      r0, #0
08004db0: 1f08       lsrs      r7, r3, #0x20
08004db2: 0020       movs      r0, #0
08004db4: 1e08       lsrs      r6, r3, #0x20
08004db6: 0020       movs      r0, #0
08004db8: 2008       lsrs      r0, r4, #0x20
08004dba: 0020       movs      r0, #0
08004dbc: 0008       lsrs      r0, r0, #0x20
08004dbe: 0020       movs      r0, #0
