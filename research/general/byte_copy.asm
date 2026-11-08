080081c4: 70b5       push      {r4, r5, r6, lr}
080081c6: 0446       mov       r4, r0
080081c8: 0020       movs      r0, #0
080081ca: 00bf       nop       
080081cc: 04e0       b         #0x80081d8
080081ce: 0d5c       ldrb      r5, [r1, r0]
080081d0: 1618       adds      r6, r2, r0
080081d2: a555       strb      r5, [r4, r6]
080081d4: 451c       adds      r5, r0, #1
080081d6: a8b2       uxth      r0, r5
080081d8: 9842       cmp       r0, r3
080081da: f8db       blt       #0x80081ce
080081dc: 70bd       pop       {r4, r5, r6, pc}
