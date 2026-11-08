080039c0: 10b5       push      {r4, lr}
080039c2: 40f2be31   movw      r1, #0x3be
080039c6: 1320       movs      r0, #0x13
080039c8: 1af04afe   bl        #0x801e660
080039cc: 40f27b31   movw      r1, #0x37b
080039d0: 1220       movs      r0, #0x12
080039d2: 1af045fe   bl        #0x801e660
080039d6: 40f27b21   movw      r1, #0x27b
080039da: 1120       movs      r0, #0x11
080039dc: 1af040fe   bl        #0x801e660
080039e0: 7a21       movs      r1, #0x7a
080039e2: 1020       movs      r0, #0x10
080039e4: 1af03cfe   bl        #0x801e660
080039e8: 1921       movs      r1, #0x19
080039ea: 1420       movs      r0, #0x14
080039ec: 1af038fe   bl        #0x801e660
080039f0: 42f63821   movw      r1, #0x2a38
080039f4: 4920       movs      r0, #0x49
080039f6: 1af033fe   bl        #0x801e660
080039fa: 48f22041   movw      r1, #0x8420
080039fe: 7b20       movs      r0, #0x7b
08003a00: 1af02efe   bl        #0x801e660
08003a04: 10bd       pop       {r4, pc}
08003a06: 0000       movs      r0, r0
08003a08: 10b5       push      {r4, lr}
08003a0a: 0446       mov       r4, r0
08003a0c: 46f24000   movw      r0, #0x6040
08003a10: 0549       ldr       r1, [pc, #0x14] ; literal[08003a28]=20000800
08003a12: 0880       strh      r0, [r1]
08003a14: 0846       mov       r0, r1
08003a16: 4078       ldrb      r0, [r0, #1]
08003a18: 2043       orrs      r0, r4
08003a1a: 4870       strb      r0, [r1, #1]
08003a1c: 0846       mov       r0, r1
08003a1e: 0188       ldrh      r1, [r0]
08003a20: 4720       movs      r0, #0x47
08003a22: 1af057fe   bl        #0x801e6d4
08003a26: 10bd       pop       {r4, pc}
08003a28: 0008       lsrs      r0, r0, #0x20
08003a2a: 0020       movs      r0, #0
08003a2c: 2de9f041   push.w    {r4, r5, r6, r7, r8, lr}
08003a30: 0646       mov       r6, r0
08003a32: 0d46       mov       r5, r1
08003a34: 1446       mov       r4, r2
08003a36: 0027       movs      r7, #0
08003a38: b846       mov       r8, r7
08003a3a: c5b1       cbz       r5, #0x8003a6e
08003a3c: 4ff20b57   movw      r7, #0xf50b
08003a40: 052c       cmp       r4, #5
08003a42: 13d2       bhs       #0x8003a6c
08003a44: dfe804f0   tbb       [pc, r4]
08003a48: 0306       lsls      r3, r0, #0x18
08003a4a: 090c       lsrs      r1, r1, #0x10
08003a4c: 0f00       movs      r7, r1
08003a4e: 4ef21c67   movw      r7, #0xe61c
08003a52: 0be0       b         #0x8003a6c
08003a54: 4df62277   movw      r7, #0xdf22
08003a58: 08e0       b         #0x8003a6c
08003a5a: 4df22d47   movw      r7, #0xd42d
08003a5e: 05e0       b         #0x8003a6c
08003a60: 4cf63547   movw      r7, #0xcc35
08003a64: 02e0       b         #0x8003a6c
08003a66: 4ff60227   movw      r7, #0xfa02
08003a6a: 00bf       nop       
08003a6c: 2fe0       b         #0x8003ace
08003a6e: 48f6e337   movw      r7, #0x8be3
08003a72: 43f22d58   movw      r8, #0x352d
08003a76: 072c       cmp       r4, #7
08003a78: 28d2       bhs       #0x8003acc
08003a7a: dfe804f0   tbb       [pc, r4]
08003a7e: 0409       lsrs      r4, r0, #4
08003a80: 0e13       asrs      r6, r1, #0xc
08003a82: 181d       adds      r0, r3, #4
08003a84: 2200       movs      r2, r4
08003a86: 48f6d737   movw      r7, #0x8bd7
08003a8a: 43f20758   movw      r8, #0x3507
08003a8e: 1de0       b         #0x8003acc
08003a90: 48f6ed27   movw      r7, #0x8aed
08003a94: 43f2e158   movw      r8, #0x35e1
08003a98: 18e0       b         #0x8003acc
08003a9a: 48f69627   movw      r7, #0x8a96
08003a9e: 43f20668   movw      r8, #0x3606
08003aa2: 13e0       b         #0x8003acc
08003aa4: 48f60817   movw      r7, #0x8908
08003aa8: 43f27978   movw      r8, #0x3779
08003aac: 0ee0       b         #0x8003acc
08003aae: 48f6de57   movw      r7, #0x8dde
08003ab2: 43f25a38   movw      r8, #0x335a
08003ab6: 09e0       b         #0x8003acc
08003ab8: 48f6b367   movw      r7, #0x8eb3
08003abc: 43f2ba28   movw      r8, #0x32ba
08003ac0: 04e0       b         #0x8003acc
08003ac2: 48f68877   movw      r7, #0x8f88
08003ac6: 43f20128   movw      r8, #0x3201
08003aca: 00bf       nop       
08003acc: 00bf       nop       
08003ace: 8eb1       cbz       r6, #0x8003af4
08003ad0: 35b1       cbz       r5, #0x8003ae0
08003ad2: 012d       cmp       r5, #1
08003ad4: 0dd1       bne       #0x8003af2
08003ad6: 3946       mov       r1, r7
08003ad8: 7420       movs      r0, #0x74
08003ada: 1af0fbfd   bl        #0x801e6d4
08003ade: 08e0       b         #0x8003af2
08003ae0: 3946       mov       r1, r7
08003ae2: 4420       movs      r0, #0x44
08003ae4: 1af0f6fd   bl        #0x801e6d4
08003ae8: 4146       mov       r1, r8
08003aea: 4520       movs      r0, #0x45
08003aec: 1af0f2fd   bl        #0x801e6d4
08003af0: 00bf       nop       
08003af2: 11e0       b         #0x8003b18
08003af4: 35b1       cbz       r5, #0x8003b04
08003af6: 012d       cmp       r5, #1
08003af8: 0dd1       bne       #0x8003b16
08003afa: 3946       mov       r1, r7
08003afc: 7520       movs      r0, #0x75
08003afe: 1af0e9fd   bl        #0x801e6d4
08003b02: 08e0       b         #0x8003b16
08003b04: 3946       mov       r1, r7
08003b06: 5420       movs      r0, #0x54
08003b08: 1af0e4fd   bl        #0x801e6d4
08003b0c: 4146       mov       r1, r8
08003b0e: 5520       movs      r0, #0x55
08003b10: 1af0e0fd   bl        #0x801e6d4
08003b14: 00bf       nop       
08003b16: 00bf       nop       
08003b18: bde8f081   pop.w     {r4, r5, r6, r7, r8, pc}
08003b1c: 10b5       push      {r4, lr}
08003b1e: 40f2be31   movw      r1, #0x3be
08003b22: 1320       movs      r0, #0x13
08003b24: 1af0d6fd   bl        #0x801e6d4
08003b28: 40f27b31   movw      r1, #0x37b
08003b2c: 1220       movs      r0, #0x12
08003b2e: 1af0d1fd   bl        #0x801e6d4
08003b32: 40f27b21   movw      r1, #0x27b
08003b36: 1120       movs      r0, #0x11
08003b38: 1af0ccfd   bl        #0x801e6d4
08003b3c: 7a21       movs      r1, #0x7a
08003b3e: 1020       movs      r0, #0x10
08003b40: 1af0c8fd   bl        #0x801e6d4
08003b44: 1921       movs      r1, #0x19
08003b46: 1420       movs      r0, #0x14
08003b48: 1af0c4fd   bl        #0x801e6d4
08003b4c: 42f63821   movw      r1, #0x2a38
08003b50: 4920       movs      r0, #0x49
08003b52: 1af0bffd   bl        #0x801e6d4
08003b56: 48f22041   movw      r1, #0x8420
08003b5a: 7b20       movs      r0, #0x7b
08003b5c: 1af0bafd   bl        #0x801e6d4
08003b60: 10bd       pop       {r4, pc}
08003b62: 0000       movs      r0, r0
08003b64: 2de9f041   push.w    {r4, r5, r6, r7, r8, lr}
08003b68: 0546       mov       r5, r0
08003b6a: 0f46       mov       r7, r1
08003b6c: 1646       mov       r6, r2
08003b6e: 3c46       mov       r4, r7
08003b70: 35b1       cbz       r5, #0x8003b80
08003b72: 012d       cmp       r5, #1
08003b74: 09d0       beq       #0x8003b8a
08003b76: 022d       cmp       r5, #2
08003b78: 18d0       beq       #0x8003bac
08003b7a: 032d       cmp       r5, #3
08003b7c: 4bd1       bne       #0x8003c16
08003b7e: 16e0       b         #0x8003bae
08003b80: 0021       movs      r1, #0
08003b82: 5120       movs      r0, #0x51
08003b84: 1af0a6fd   bl        #0x801e6d4
08003b88: 45e0       b         #0x8003c16
08003b8a: 40f29d10   movw      r0, #0x19d
08003b8e: 6043       muls      r0, r4, r0
08003b90: c821       movs      r1, #0xc8
08003b92: b0fbf1f4   udiv      r4, r0, r1
08003b96: 49f24101   movw      r1, #0x9041
08003b9a: 5120       movs      r0, #0x51
08003b9c: 1af09afd   bl        #0x801e6d4
08003ba0: c4f30c01   ubfx      r1, r4, #0, #0xd
08003ba4: 0720       movs      r0, #7
08003ba6: 1af095fd   bl        #0x801e6d4
08003baa: 34e0       b         #0x8003c16
08003bac: 00bf       nop       
08003bae: 012e       cmp       r6, #1
08003bb0: 01d0       beq       #0x8003bb6
08003bb2: 032e       cmp       r6, #3
08003bb4: 05d1       bne       #0x8003bc2
08003bb6: 0121       movs      r1, #1
08003bb8: 2046       mov       r0, r4
08003bba: 05f0b1fe   bl        #0x8009920
08003bbe: 0446       mov       r4, r0
08003bc0: 04e0       b         #0x8003bcc
08003bc2: 0021       movs      r1, #0
08003bc4: 2046       mov       r0, r4
08003bc6: 05f0abfe   bl        #0x8009920
08003bca: 0446       mov       r4, r0
08003bcc: 032d       cmp       r5, #3
08003bce: 04d1       bne       #0x8003bda
08003bd0: 4ff42040   mov.w     r0, #0xa000
08003bd4: 1149       ldr       r1, [pc, #0x44] ; literal[08003c1c]=20000800
08003bd6: 0880       strh      r0, [r1]
08003bd8: 03e0       b         #0x8003be2
08003bda: 4ff40040   mov.w     r0, #0x8000
08003bde: 0f49       ldr       r1, [pc, #0x3c] ; literal[08003c1c]=20000800
08003be0: 0880       strh      r0, [r1]
08003be2: 0e48       ldr       r0, [pc, #0x38] ; literal[08003c1c]=20000800
08003be4: 0088       ldrh      r0, [r0]
08003be6: 40f02c01   orr       r1, r0, #0x2c
08003bea: 5120       movs      r0, #0x51
08003bec: 1af072fd   bl        #0x801e6d4
08003bf0: 40f6d721   movw      r1, #0xad7
08003bf4: 0720       movs      r0, #7
08003bf6: 1af06dfd   bl        #0x801e6d4
08003bfa: c4f30b01   ubfx      r1, r4, #0, #0xc
08003bfe: 0820       movs      r0, #8
08003c00: 1af068fd   bl        #0x801e6d4
08003c04: 4ff40040   mov.w     r0, #0x8000
08003c08: 00eb1430   add.w     r0, r0, r4, lsr #12
08003c0c: 81b2       uxth      r1, r0
08003c0e: 0820       movs      r0, #8
08003c10: 1af060fd   bl        #0x801e6d4
08003c14: 00bf       nop       
08003c16: 00bf       nop       
08003c18: bde8f081   pop.w     {r4, r5, r6, r7, r8, pc}
08003c1c: 0008       lsrs      r0, r0, #0x20
08003c1e: 0020       movs      r0, #0
08003c20: 10b5       push      {r4, lr}
08003c22: 3120       movs      r0, #0x31
08003c24: 1af038fd   bl        #0x801e698
08003c28: 0b49       ldr       r1, [pc, #0x2c] ; literal[08003c58]=20000800
08003c2a: 0880       strh      r0, [r1]
08003c2c: 0846       mov       r0, r1
08003c2e: 0078       ldrb      r0, [r0]
08003c30: 20f00800   bic       r0, r0, #8
08003c34: 0870       strb      r0, [r1]
08003c36: 0846       mov       r0, r1
08003c38: 0188       ldrh      r1, [r0]
08003c3a: 3120       movs      r0, #0x31
08003c3c: 1af04afd   bl        #0x801e6d4
08003c40: 46f63831   movw      r1, #0x6b38
08003c44: 2820       movs      r0, #0x28
08003c46: 1af045fd   bl        #0x801e6d4
08003c4a: 4af64031   movw      r1, #0xab40
08003c4e: 2920       movs      r0, #0x29
08003c50: 1af040fd   bl        #0x801e6d4
08003c54: 10bd       pop       {r4, pc}
08003c56: 0000       movs      r0, r0
08003c58: 0008       lsrs      r0, r0, #0x20
08003c5a: 0020       movs      r0, #0
08003c5c: 10b5       push      {r4, lr}
08003c5e: 0446       mov       r4, r0
08003c60: 48f22c00   movw      r0, #0x802c
08003c64: 1449       ldr       r1, [pc, #0x50] ; literal[08003cb8]=20000800
08003c66: 0880       strh      r0, [r1]
08003c68: 200e       lsrs      r0, r4, #0x18
08003c6a: 30b1       cbz       r0, #0x8003c7a
08003c6c: 0846       mov       r0, r1
08003c6e: 4078       ldrb      r0, [r0, #1]
08003c70: 20f00800   bic       r0, r0, #8
08003c74: 0830       adds      r0, #8
08003c76: 4870       strb      r0, [r1, #1]
08003c78: 05e0       b         #0x8003c86
08003c7a: 0f48       ldr       r0, [pc, #0x3c] ; literal[08003cb8]=20000800
08003c7c: 4078       ldrb      r0, [r0, #1]
08003c7e: 20f00800   bic       r0, r0, #8
08003c82: 0d49       ldr       r1, [pc, #0x34] ; literal[08003cb8]=20000800
08003c84: 4870       strb      r0, [r1, #1]
08003c86: 0c48       ldr       r0, [pc, #0x30] ; literal[08003cb8]=20000800
08003c88: 0188       ldrh      r1, [r0]
08003c8a: 5120       movs      r0, #0x51
08003c8c: 1af022fd   bl        #0x801e6d4
08003c90: 40f6d721   movw      r1, #0xad7
08003c94: 0720       movs      r0, #7
08003c96: 1af01dfd   bl        #0x801e6d4
08003c9a: c4f30b01   ubfx      r1, r4, #0, #0xc
08003c9e: 0820       movs      r0, #8
08003ca0: 1af018fd   bl        #0x801e6d4
08003ca4: 4ff40040   mov.w     r0, #0x8000
08003ca8: 00eb1430   add.w     r0, r0, r4, lsr #12
08003cac: 81b2       uxth      r1, r0
08003cae: 0820       movs      r0, #8
08003cb0: 1af010fd   bl        #0x801e6d4
08003cb4: 10bd       pop       {r4, pc}
08003cb6: 0000       movs      r0, r0
08003cb8: 0008       lsrs      r0, r0, #0x20
08003cba: 0020       movs      r0, #0
08003cbc: 10b5       push      {r4, lr}
08003cbe: 0446       mov       r4, r0
08003cc0: 2146       mov       r1, r4
08003cc2: 4020       movs      r0, #0x40
08003cc4: 1af006fd   bl        #0x801e6d4
08003cc8: 10bd       pop       {r4, pc}
08003cca: 0000       movs      r0, r0
08003ccc: 70b5       push      {r4, r5, r6, lr}
08003cce: 0546       mov       r5, r0
08003cd0: 2c46       mov       r4, r5
08003cd2: 0121       movs      r1, #1
08003cd4: 2046       mov       r0, r4
08003cd6: 0ef0abfe   bl        #0x8012a30
08003cda: 0748       ldr       r0, [pc, #0x1c] ; literal[08003cf8]=20003064
08003cdc: 0088       ldrh      r0, [r0]
08003cde: 2844       add       r0, r5
08003ce0: a0f50044   sub.w     r4, r0, #0x8000
08003ce4: a1b2       uxth      r1, r4
08003ce6: 3820       movs      r0, #0x38
08003ce8: 1af0f4fc   bl        #0x801e6d4
08003cec: 210c       lsrs      r1, r4, #0x10
08003cee: 3920       movs      r0, #0x39
08003cf0: 1af0f0fc   bl        #0x801e6d4
08003cf4: 70bd       pop       {r4, r5, r6, pc}
08003cf6: 0000       movs      r0, r0
08003cf8: 6430       adds      r0, #0x64
08003cfa: 0020       movs      r0, #0
08003cfc: 10b5       push      {r4, lr}
08003cfe: 0c4a       ldr       r2, [pc, #0x30] ; literal[08003d30]=200010ec
08003d00: 1268       ldr       r2, [r2]
08003d02: d27b       ldrb      r2, [r2, #0xf]
08003d04: 1109       lsrs      r1, r2, #4
08003d06: 0a4a       ldr       r2, [pc, #0x28] ; literal[08003d30]=200010ec
08003d08: 1268       ldr       r2, [r2]
08003d0a: d27c       ldrb      r2, [r2, #0x13]
08003d0c: c2f30210   ubfx      r0, r2, #4, #3
08003d10: 00f044fa   bl        #0x800419c
08003d14: 01f000fb   bl        #0x8005318
08003d18: 00f0f6f8   bl        #0x8003f08
08003d1c: 0020       movs      r0, #0
08003d1e: 00f073f8   bl        #0x8003e08
08003d22: 0020       movs      r0, #0
08003d24: fff770fe   bl        #0x8003a08
08003d28: 00f0caf8   bl        #0x8003ec0
08003d2c: 10bd       pop       {r4, pc}
08003d2e: 0000       movs      r0, r0
08003d30: ec10       asrs      r4, r5, #3
08003d32: 0020       movs      r0, #0
08003d34: 10b5       push      {r4, lr}
08003d36: 4ff40041   mov.w     r1, #0x8000
08003d3a: 0020       movs      r0, #0
08003d3c: 1af0cafc   bl        #0x801e6d4
08003d40: 0a20       movs      r0, #0xa
08003d42: 04f01dfb   bl        #0x8008380
08003d46: 0021       movs      r1, #0
08003d48: 0846       mov       r0, r1
08003d4a: 1af0c3fc   bl        #0x801e6d4
08003d4e: 41f60f51   movw      r1, #0x1d0f
08003d52: 3720       movs      r0, #0x37
08003d54: 1af0befc   bl        #0x801e6d4
08003d58: fff7e0fe   bl        #0x8003b1c
08003d5c: 0021       movs      r1, #0
08003d5e: 3320       movs      r0, #0x33
08003d60: 1af0b8fc   bl        #0x801e6d4
08003d64: 0021       movs      r1, #0
08003d66: 3420       movs      r0, #0x34
08003d68: 1af0b4fc   bl        #0x801e6d4
08003d6c: 0021       movs      r1, #0
08003d6e: 3520       movs      r0, #0x35
08003d70: 1af0b0fc   bl        #0x801e6d4
08003d74: 44f65841   movw      r1, #0x4c58
08003d78: 1e20       movs      r0, #0x1e
08003d7a: 1af0abfc   bl        #0x801e6d4
08003d7e: 4af25661   movw      r1, #0xa656
08003d82: 1f20       movs      r0, #0x1f
08003d84: 1af0a6fc   bl        #0x801e6d4
08003d88: 4af23701   movw      r1, #0xa037
08003d8c: 3e20       movs      r0, #0x3e
08003d8e: 1af0a1fc   bl        #0x801e6d4
08003d92: 4ef65011   movw      r1, #0xe950
08003d96: 7d20       movs      r0, #0x7d
08003d98: 1af09cfc   bl        #0x801e6d4
08003d9c: 4ff49d51   mov.w     r1, #0x13a0
08003da0: 2620       movs      r0, #0x26
08003da2: 1af097fc   bl        #0x801e6d4
08003da6: 0f48       ldr       r0, [pc, #0x3c] ; literal[08003de4]=20003044
08003da8: c27a       ldrb      r2, [r0, #0xb]
08003daa: 0121       movs      r1, #1
08003dac: 0020       movs      r0, #0
08003dae: fff73dfe   bl        #0x8003a2c
08003db2: 0c48       ldr       r0, [pc, #0x30] ; literal[08003de4]=20003044
08003db4: 827a       ldrb      r2, [r0, #0xa]
08003db6: 0021       movs      r1, #0
08003db8: 0846       mov       r0, r1
08003dba: fff737fe   bl        #0x8003a2c
08003dbe: 0948       ldr       r0, [pc, #0x24] ; literal[08003de4]=20003044
08003dc0: 427a       ldrb      r2, [r0, #9]
08003dc2: 0121       movs      r1, #1
08003dc4: 0846       mov       r0, r1
08003dc6: fff731fe   bl        #0x8003a2c
08003dca: 0648       ldr       r0, [pc, #0x18] ; literal[08003de4]=20003044
08003dcc: 027a       ldrb      r2, [r0, #8]
08003dce: 0021       movs      r1, #0
08003dd0: 0120       movs      r0, #1
08003dd2: fff72bfe   bl        #0x8003a2c
08003dd6: 00f031f8   bl        #0x8003e3c
08003dda: 0020       movs      r0, #0
08003ddc: fff714fe   bl        #0x8003a08
08003de0: 10bd       pop       {r4, pc}
08003de2: 0000       movs      r0, r0
08003de4: 4430       adds      r0, #0x44
08003de6: 0020       movs      r0, #0
08003de8: 10b5       push      {r4, lr}
08003dea: 0446       mov       r4, r0
08003dec: 4ef64010   movw      r0, #0xe940
08003df0: 2044       add       r0, r4
08003df2: 0449       ldr       r1, [pc, #0x10] ; literal[08003e04]=20000800
08003df4: 0880       strh      r0, [r1]
08003df6: 0846       mov       r0, r1
08003df8: 0188       ldrh      r1, [r0]
08003dfa: 7d20       movs      r0, #0x7d
08003dfc: 1af06afc   bl        #0x801e6d4
08003e00: 10bd       pop       {r4, pc}
08003e02: 0000       movs      r0, r0
08003e04: 0008       lsrs      r0, r0, #0x20
08003e06: 0020       movs      r0, #0
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
