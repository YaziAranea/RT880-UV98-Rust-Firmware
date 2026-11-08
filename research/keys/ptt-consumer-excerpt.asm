; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
08022270: 10b5       push      {r4, lr}
08022272: f848       ldr       r0, [pc, #0x3e0] ; literal[08022654]=20001166
08022274: 0078       ldrb      r0, [r0]
08022276: 00b1       cbz       r0, #0x802227a
08022278: 10bd       pop       {r4, pc}
0802227a: f748       ldr       r0, [pc, #0x3dc] ; literal[08022658]=20000836
0802227c: 0088       ldrh      r0, [r0]
0802227e: 00b1       cbz       r0, #0x8022282
08022280: fae7       b         #0x8022278
08022282: f648       ldr       r0, [pc, #0x3d8] ; literal[0802265c]=20000835
08022284: 0078       ldrb      r0, [r0]
08022286: 10b1       cbz       r0, #0x802228e
08022288: 0020       movs      r0, #0
0802228a: f449       ldr       r1, [pc, #0x3d0] ; literal[0802265c]=20000835
0802228c: 0870       strb      r0, [r1]
0802228e: 4021       movs      r1, #0x40
08022290: f348       ldr       r0, [pc, #0x3cc] ; literal[08022660]=40020400
08022292: 03f024f9   bl        #0x80254de
08022296: 18b9       cbnz      r0, #0x80222a0
08022298: 0120       movs      r0, #1
0802229a: f249       ldr       r1, [pc, #0x3c8] ; literal[08022664]=20001127
0802229c: 0870       strb      r0, [r1]
0802229e: 1ee0       b         #0x80222de
