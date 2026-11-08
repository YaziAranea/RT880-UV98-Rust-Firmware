; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
0801eadc: 1cb5       push      {r2, r3, r4, lr}
0801eade: 6846       mov       r0, sp
0801eae0: 06f0b6fc   bl        #0x8025450
0801eae4: 0120       movs      r0, #1
0801eae6: 8df80500   strb.w    r0, [sp, #5]
0801eaea: 0220       movs      r0, #2
0801eaec: 8df80600   strb.w    r0, [sp, #6]
0801eaf0: 0320       movs      r0, #3
0801eaf2: 0090       str       r0, [sp]
0801eaf4: 0120       movs      r0, #1
0801eaf6: 8df80400   strb.w    r0, [sp, #4]
0801eafa: 6946       mov       r1, sp
0801eafc: 0948       ldr       r0, [pc, #0x24] ; literal[0801eb24]=40020800
0801eafe: 06f0b1fc   bl        #0x8025464
0801eb02: 0722       movs      r2, #7
0801eb04: 0021       movs      r1, #0
0801eb06: 0748       ldr       r0, [pc, #0x1c] ; literal[0801eb24]=40020800
0801eb08: 06f0f2fc   bl        #0x80254f0
0801eb0c: 0722       movs      r2, #7
0801eb0e: 0121       movs      r1, #1
0801eb10: 0448       ldr       r0, [pc, #0x10] ; literal[0801eb24]=40020800
0801eb12: 06f0edfc   bl        #0x80254f0
0801eb16: 0448       ldr       r0, [pc, #0x10] ; literal[0801eb28]=40005400
0801eb18: 0449       ldr       r1, [pc, #0x10] ; literal[0801eb2c]=20008324
0801eb1a: 0860       str       r0, [r1]
0801eb1c: 0846       mov       r0, r1
0801eb1e: 06f020fd   bl        #0x8025562
0801eb22: 1cbd       pop       {r2, r3, r4, pc}
0801eb24: 0008       lsrs      r0, r0, #0x20
0801eb26: 0240       ands      r2, r0
0801eb28: 0054       strb      r0, [r0, r0]
0801eb2a: 0040       ands      r0, r0
0801eb2c: 2483       strh      r4, [r4, #0x18]
0801eb2e: 0020       movs      r0, #0
