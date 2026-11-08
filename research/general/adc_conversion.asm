080121a8: 10b5       push      {r4, lr}
080121aa: 0121       movs      r1, #1
080121ac: 1048       ldr       r0, [pc, #0x40] ; literal[080121f0]=40012000
080121ae: 12f089fd   bl        #0x8024cc4
080121b2: 02e0       b         #0x80121ba
080121b4: 0120       movs      r0, #1
080121b6: f6f7e3f8   bl        #0x8008380
080121ba: 0e48       ldr       r0, [pc, #0x38] ; literal[080121f4]=200010d8
080121bc: 0068       ldr       r0, [r0]
080121be: 18b9       cbnz      r0, #0x80121c8
080121c0: 0d48       ldr       r0, [pc, #0x34] ; literal[080121f8]=200010d4
080121c2: 0068       ldr       r0, [r0]
080121c4: 0028       cmp       r0, #0
080121c6: f5d0       beq       #0x80121b4
080121c8: 0020       movs      r0, #0
080121ca: 0b49       ldr       r1, [pc, #0x2c] ; literal[080121f8]=200010d4
080121cc: 0860       str       r0, [r1]
080121ce: 0949       ldr       r1, [pc, #0x24] ; literal[080121f4]=200010d8
080121d0: 0860       str       r0, [r1]
080121d2: 0a48       ldr       r0, [pc, #0x28] ; literal[080121fc]=200010cc
080121d4: 0088       ldrh      r0, [r0]
080121d6: 00eb4000   add.w     r0, r0, r0, lsl #1
080121da: 3321       movs      r1, #0x33
080121dc: 90fbf1f0   sdiv      r0, r0, r1
080121e0: 801c       adds      r0, r0, #2
080121e2: 0749       ldr       r1, [pc, #0x1c] ; literal[08012200]=200010e4
080121e4: 0880       strh      r0, [r1]
080121e6: 0548       ldr       r0, [pc, #0x14] ; literal[080121fc]=200010cc
080121e8: 4088       ldrh      r0, [r0, #2]
080121ea: 4880       strh      r0, [r1, #2]
080121ec: 10bd       pop       {r4, pc}
080121ee: 0000       movs      r0, r0
080121f0: 0020       movs      r0, #0
080121f2: 0140       ands      r1, r0
080121f4: d810       asrs      r0, r3, #3
080121f6: 0020       movs      r0, #0
080121f8: d410       asrs      r4, r2, #3
080121fa: 0020       movs      r0, #0
080121fc: cc10       asrs      r4, r1, #3
080121fe: 0020       movs      r0, #0
08012200: e410       asrs      r4, r4, #3
08012202: 0020       movs      r0, #0
