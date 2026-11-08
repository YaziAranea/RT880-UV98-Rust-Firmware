08004bc0: 0249       ldr       r1, [pc, #8] ; literal[08004bcc]=20000800
08004bc2: 0880       strh      r0, [r1]
08004bc4: 0846       mov       r0, r1
08004bc6: 0078       ldrb      r0, [r0]
08004bc8: 10bd       pop       {r4, pc}
08004bca: 0000       movs      r0, r0
08004bcc: 0008       lsrs      r0, r0, #0x20
08004bce: 0020       movs      r0, #0
08004bd0: 10b5       push      {r4, lr}
08004bd2: 0446       mov       r4, r0
08004bd4: 04f52040   add.w     r0, r4, #0xa000
08004bd8: 81b2       uxth      r1, r0
08004bda: 4d20       movs      r0, #0x4d
08004bdc: 19f040fd   bl        #0x801e660
08004be0: 04f5de40   add.w     r0, r4, #0x6f00
08004be4: 81b2       uxth      r1, r0
08004be6: 4e20       movs      r0, #0x4e
08004be8: 19f03afd   bl        #0x801e660
08004bec: 10bd       pop       {r4, pc}
08004bee: 0000       movs      r0, r0
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
08004dc0: 10b5       push      {r4, lr}
08004dc2: 4ff40041   mov.w     r1, #0x8000
08004dc6: 0020       movs      r0, #0
08004dc8: 19f04afc   bl        #0x801e660
08004dcc: 0a20       movs      r0, #0xa
08004dce: 03f0d7fa   bl        #0x8008380
08004dd2: 0021       movs      r1, #0
08004dd4: 0846       mov       r0, r1
08004dd6: 19f043fc   bl        #0x801e660
08004dda: 41f60f51   movw      r1, #0x1d0f
08004dde: 3720       movs      r0, #0x37
08004de0: 19f03efc   bl        #0x801e660
08004de4: fef7ecfd   bl        #0x80039c0
08004de8: 0021       movs      r1, #0
08004dea: 3320       movs      r0, #0x33
08004dec: 19f038fc   bl        #0x801e660
08004df0: 0021       movs      r1, #0
08004df2: 3420       movs      r0, #0x34
08004df4: 19f034fc   bl        #0x801e660
08004df8: 0021       movs      r1, #0
08004dfa: 3520       movs      r0, #0x35
08004dfc: 19f030fc   bl        #0x801e660
08004e00: 44f65841   movw      r1, #0x4c58
08004e04: 1e20       movs      r0, #0x1e
08004e06: 19f02bfc   bl        #0x801e660
08004e0a: 4af25661   movw      r1, #0xa656
08004e0e: 1f20       movs      r0, #0x1f
08004e10: 19f026fc   bl        #0x801e660
08004e14: 4af23701   movw      r1, #0xa037
08004e18: 3e20       movs      r0, #0x3e
08004e1a: 19f021fc   bl        #0x801e660
08004e1e: 4ef65011   movw      r1, #0xe950
08004e22: 7d20       movs      r0, #0x7d
08004e24: 19f01cfc   bl        #0x801e660
08004e28: 4ff49d51   mov.w     r1, #0x13a0
08004e2c: 2620       movs      r0, #0x26
08004e2e: 19f017fc   bl        #0x801e660
08004e32: 1648       ldr       r0, [pc, #0x58] ; literal[08004e8c]=20003044
08004e34: c27a       ldrb      r2, [r0, #0xb]
08004e36: 0121       movs      r1, #1
08004e38: 0020       movs      r0, #0
08004e3a: fef749fd   bl        #0x80038d0
08004e3e: 1348       ldr       r0, [pc, #0x4c] ; literal[08004e8c]=20003044
08004e40: 827a       ldrb      r2, [r0, #0xa]
08004e42: 0021       movs      r1, #0
08004e44: 0846       mov       r0, r1
08004e46: fef743fd   bl        #0x80038d0
08004e4a: 1048       ldr       r0, [pc, #0x40] ; literal[08004e8c]=20003044
08004e4c: 427a       ldrb      r2, [r0, #9]
08004e4e: 0121       movs      r1, #1
08004e50: 0846       mov       r0, r1
08004e52: fef73dfd   bl        #0x80038d0
08004e56: 0d48       ldr       r0, [pc, #0x34] ; literal[08004e8c]=20003044
08004e58: 027a       ldrb      r2, [r0, #8]
08004e5a: 0021       movs      r1, #0
08004e5c: 0120       movs      r0, #1
08004e5e: fef737fd   bl        #0x80038d0
08004e62: 00f09df8   bl        #0x8004fa0
08004e66: 0020       movs      r0, #0
08004e68: 00f00dfc   bl        #0x8005686
08004e6c: 0021       movs      r1, #0
08004e6e: 0120       movs      r0, #1
08004e70: 00f012fc   bl        #0x8005698
08004e74: 0020       movs      r0, #0
08004e76: 00f0cff9   bl        #0x8005218
08004e7a: fdf7e9ff   bl        #0x8002e50
08004e7e: fdf7edff   bl        #0x8002e5c
08004e82: 0020       movs      r0, #0
08004e84: fef712fd   bl        #0x80038ac
08004e88: 10bd       pop       {r4, pc}
08004e8a: 0000       movs      r0, r0
08004e8c: 4430       adds      r0, #0x44
08004e8e: 0020       movs      r0, #0
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
08004f40: 10b5       push      {r4, lr}
08004f42: 6720       movs      r0, #0x67
08004f44: 19f06cfb   bl        #0x801e620
