08020f70: 8154       strb      r1, [r0, r2]
08020f72: 2946       mov       r1, r5
08020f74: 2046       mov       r0, r4
08020f76: f3f7d9f8   bl        #0x801412c
08020f7a: 00bf       nop       
08020f7c: e3e7       b         #0x8020f46
08020f7e: 0000       movs      r0, r0
08020f80: 6536       adds      r6, #0x65
08020f82: 0020       movs      r0, #0
08020f84: 08b5       push      {r3, lr}
08020f86: 4ff40050   mov.w     r0, #0x2000
08020f8a: 2f49       ldr       r1, [pc, #0xbc] ; literal[08021048]=40020000
08020f8c: 8862       str       r0, [r1, #0x28]
08020f8e: 4000       lsls      r0, r0, #1
08020f90: 8862       str       r0, [r1, #0x28]
08020f92: 2e48       ldr       r0, [pc, #0xb8] ; literal[0802104c]=200018ec
08020f94: 0078       ldrb      r0, [r0]
08020f96: 08b1       cbz       r0, #0x8020f9c
08020f98: f0f7bcfc   bl        #0x8011914
08020f9c: 6422       movs      r2, #0x64
08020f9e: 0721       movs      r1, #7
08020fa0: 40f25c60   movw      r0, #0x65c
08020fa4: f7f752f9   bl        #0x801824c
08020fa8: 02e0       b         #0x8020fb0
08020faa: 0120       movs      r0, #1
08020fac: e7f7e8f9   bl        #0x8008380
08020fb0: 2748       ldr       r0, [pc, #0x9c] ; literal[08021050]=2000120a
08020fb2: 0078       ldrb      r0, [r0]
08020fb4: 0028       cmp       r0, #0
08020fb6: f8d1       bne       #0x8020faa
08020fb8: e4f706f8   bl        #0x8004fc8
08020fbc: e4f7acf9   bl        #0x8005318
08020fc0: 2020       movs      r0, #0x20
08020fc2: 2449       ldr       r1, [pc, #0x90] ; literal[08021054]=40020828
08020fc4: 0860       str       r0, [r1]
08020fc6: 2448       ldr       r0, [pc, #0x90] ; literal[08021058]=200011fc
08020fc8: 0078       ldrb      r0, [r0]
08020fca: 10b9       cbnz      r0, #0x8020fd2
08020fcc: e4f7d4fe   bl        #0x8005d78
08020fd0: 0be0       b         #0x8020fea
08020fd2: 0022       movs      r2, #0
08020fd4: 0121       movs      r1, #1
08020fd6: 2148       ldr       r0, [pc, #0x84] ; literal[0802105c]=40000400
08020fd8: 05f07af8   bl        #0x80260d0
08020fdc: 0021       movs      r1, #0
08020fde: 1f48       ldr       r0, [pc, #0x7c] ; literal[0802105c]=40000400
08020fe0: 05f065f8   bl        #0x80260ae
08020fe4: 0120       movs      r0, #1
08020fe6: 1849       ldr       r1, [pc, #0x60] ; literal[08021048]=40020000
08020fe8: 8862       str       r0, [r1, #0x28]
08020fea: 4ff40070   mov.w     r0, #0x200
08020fee: 1c49       ldr       r1, [pc, #0x70] ; literal[08021060]=40020428
08020ff0: 0860       str       r0, [r1]
08020ff2: 0020       movs      r0, #0
08020ff4: e1f79cff   bl        #0x8002f30
08020ff8: 0020       movs      r0, #0
08020ffa: 1a49       ldr       r1, [pc, #0x68] ; literal[08021064]=200018f6
08020ffc: 0880       strh      r0, [r1]
08020ffe: e4f757f9   bl        #0x80052b0
08021002: e2f75dff   bl        #0x8003ec0
08021006: 6420       movs      r0, #0x64
08021008: e7f7baf9   bl        #0x8008380
0802100c: 0021       movs      r1, #0
0802100e: 1648       ldr       r0, [pc, #0x58] ; literal[08021068]=40007410
08021010: 04f092f8   bl        #0x8025138
08021014: 0020       movs      r0, #0
08021016: 1549       ldr       r1, [pc, #0x54] ; literal[0802106c]=2000112d
08021018: 0870       strb      r0, [r1]
0802101a: 1549       ldr       r1, [pc, #0x54] ; literal[08021070]=200011f8
0802101c: 0870       strb      r0, [r1]
0802101e: 4ff40070   mov.w     r0, #0x200
08021022: 0949       ldr       r1, [pc, #0x24] ; literal[08021048]=40020000
08021024: 8862       str       r0, [r1, #0x28]
08021026: 0020       movs      r0, #0
08021028: 40f23f13   movw      r3, #0x13f
0802102c: 0246       mov       r2, r0
0802102e: ef21       movs      r1, #0xef
08021030: 0090       str       r0, [sp]
08021032: f6f7ddfe   bl        #0x8017df0
08021036: 0020       movs      r0, #0
08021038: 0e49       ldr       r1, [pc, #0x38] ; literal[08021074]=200011f6
0802103a: 0870       strb      r0, [r1]
0802103c: 0e49       ldr       r1, [pc, #0x38] ; literal[08021078]=200011f7
0802103e: 0870       strb      r0, [r1]
