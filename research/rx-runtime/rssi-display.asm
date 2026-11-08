08007c3a: fdf781f9   bl        #0x8004f40
08007c3e: 0546       mov       r5, r0
08007c40: 6d10       asrs      r5, r5, #1
08007c42: a72d       cmp       r5, #0xa7
08007c44: 01dd       ble       #0x8007c4a
08007c46: 0027       movs      r7, #0
08007c48: 02e0       b         #0x8007c50
08007c4a: c5f1a700   rsb.w     r0, r5, #0xa7
08007c4e: c7b2       uxtb      r7, r0
08007c50: 0321       movs      r1, #3
08007c52: 3846       mov       r0, r7
08007c54: 13f036fe   bl        #0x801b8c4

0802416a: e0f7e9fe   bl        #0x8004f40
0802416e: 00b2       sxth      r0, r0
08024170: 7c49       ldr       r1, [pc, #0x1f0] ; literal[08024364]=200018e8
08024172: 0880       strh      r0, [r1]
08024174: 0846       mov       r0, r1
08024176: 0678       ldrb      r6, [r0]
08024178: a02e       cmp       r6, #0xa0
0802417a: 02dd       ble       #0x8024182
0802417c: 0020       movs      r0, #0
0802417e: 0646       mov       r6, r0
08024180: 03e0       b         #0x802418a
08024182: c6f1a000   rsb.w     r0, r6, #0xa0
08024186: c0b2       uxtb      r0, r0
08024188: 0646       mov       r6, r0
