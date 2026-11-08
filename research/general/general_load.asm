08013e3c: 10b5       push      {r4, lr}
08013e3e: 0446       mov       r4, r0
08013e40: 0a2c       cmp       r4, #0xa
08013e42: 12d1       bne       #0x8013e6a
08013e44: fdf7b4fd   bl        #0x80119b0
08013e48: 4ff48052   mov.w     r2, #0x1000
08013e4c: 1146       mov       r1, r2
08013e4e: 2148       ldr       r0, [pc, #0x84] ; literal[08013ed4]=20003bf0
08013e50: 11f06eff   bl        #0x8025d30
08013e54: 0020       movs      r0, #0
08013e56: 11f0c5ff   bl        #0x8025de4
08013e5a: 4ff48052   mov.w     r2, #0x1000
08013e5e: 0021       movs      r1, #0
08013e60: 1c48       ldr       r0, [pc, #0x70] ; literal[08013ed4]=20003bf0
08013e62: 12f061f8   bl        #0x8025f28
08013e66: fdf7b3fd   bl        #0x80119d0
08013e6a: 0020       movs      r0, #0
08013e6c: 1a49       ldr       r1, [pc, #0x68] ; literal[08013ed8]=200010e9
08013e6e: 0870       strb      r0, [r1]
08013e70: 2022       movs      r2, #0x20
08013e72: 0021       movs      r1, #0
08013e74: 1948       ldr       r0, [pc, #0x64] ; literal[08013edc]=20003044
08013e76: 11f05bff   bl        #0x8025d30
08013e7a: 1848       ldr       r0, [pc, #0x60] ; literal[08013edc]=20003044
08013e7c: 0078       ldrb      r0, [r0]
08013e7e: 9a28       cmp       r0, #0x9a
08013e80: 14d0       beq       #0x8013eac
08013e82: 0120       movs      r0, #1
08013e84: 1449       ldr       r1, [pc, #0x50] ; literal[08013ed8]=200010e9
08013e86: 0870       strb      r0, [r1]
08013e88: 2022       movs      r2, #0x20
08013e8a: 0103       lsls      r1, r0, #0xc
08013e8c: 1348       ldr       r0, [pc, #0x4c] ; literal[08013edc]=20003044
08013e8e: 11f04fff   bl        #0x8025d30
08013e92: 1248       ldr       r0, [pc, #0x48] ; literal[08013edc]=20003044
08013e94: 0078       ldrb      r0, [r0]
08013e96: 9a28       cmp       r0, #0x9a
08013e98: 08d0       beq       #0x8013eac
08013e9a: 0220       movs      r0, #2
08013e9c: 0e49       ldr       r1, [pc, #0x38] ; literal[08013ed8]=200010e9
08013e9e: 0870       strb      r0, [r1]
08013ea0: 2023       movs      r3, #0x20
08013ea2: 0022       movs      r2, #0
08013ea4: 0e49       ldr       r1, [pc, #0x38] ; literal[08013ee0]=08028344
08013ea6: 0d48       ldr       r0, [pc, #0x34] ; literal[08013edc]=20003044
08013ea8: f4f78cf9   bl        #0x80081c4
08013eac: 0c2c       cmp       r4, #0xc
08013eae: 10d1       bne       #0x8013ed2
08013eb0: 0a48       ldr       r0, [pc, #0x28] ; literal[08013edc]=20003044
08013eb2: 4078       ldrb      r0, [r0, #1]
08013eb4: 08b9       cbnz      r0, #0x8013eba
08013eb6: 0120       movs      r0, #1
08013eb8: 00e0       b         #0x8013ebc
08013eba: 0020       movs      r0, #0
08013ebc: 0749       ldr       r1, [pc, #0x1c] ; literal[08013edc]=20003044
08013ebe: 4870       strb      r0, [r1, #1]
08013ec0: fdf776fd   bl        #0x80119b0
08013ec4: 2022       movs      r2, #0x20
08013ec6: 0021       movs      r1, #0
08013ec8: 0448       ldr       r0, [pc, #0x10] ; literal[08013edc]=20003044
08013eca: 11f0b9ff   bl        #0x8025e40
08013ece: fdf77ffd   bl        #0x80119d0
08013ed2: 10bd       pop       {r4, pc}
08013ed4: f03b       subs      r3, #0xf0
08013ed6: 0020       movs      r0, #0
08013ed8: e910       asrs      r1, r5, #3
08013eda: 0020       movs      r0, #0
08013edc: 4430       adds      r0, #0x44
08013ede: 0020       movs      r0, #0
08013ee0: 4483       strh      r4, [r0, #0x1a]
08013ee2: 0208       lsrs      r2, r0, #0x20
