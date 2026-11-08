08018010: 2a20       movs      r0, #0x2a
08018012: 00f0b7f8   bl        #0x8018184
08018016: 0020       movs      r0, #0
08018018: 00f0c6f8   bl        #0x80181a8
0801801c: 0020       movs      r0, #0
0801801e: 00f0c3f8   bl        #0x80181a8
08018022: 0120       movs      r0, #1
08018024: 00f0c0f8   bl        #0x80181a8
08018028: 3f20       movs      r0, #0x3f
0801802a: 00f0bdf8   bl        #0x80181a8
0801802e: 2b20       movs      r0, #0x2b
08018030: 00f0a8f8   bl        #0x8018184
08018034: 0020       movs      r0, #0
08018036: 00f0b7f8   bl        #0x80181a8
0801803a: 0020       movs      r0, #0
0801803c: 00f0b4f8   bl        #0x80181a8
08018040: 0020       movs      r0, #0
08018042: 00f0b1f8   bl        #0x80181a8
08018046: ef20       movs      r0, #0xef
08018048: 00f0aef8   bl        #0x80181a8
0801804c: 0020       movs      r0, #0
0801804e: fff7eefe   bl        #0x8017e2e
08018052: 08bd       pop       {r3, pc}
08018054: 3c08       lsrs      r4, r7, #0x20
08018056: 0020       movs      r0, #0
08018058: 3e08       lsrs      r6, r7, #0x20
0801805a: 0020       movs      r0, #0
0801805c: 4008       lsrs      r0, r0, #1
0801805e: 0020       movs      r0, #0
08018060: 1808       lsrs      r0, r3, #0x20
08018062: 0240       ands      r2, r0
08018064: 1fb5       push      {r0, r1, r2, r3, r4, lr}
08018066: 02a8       add       r0, sp, #8
08018068: 0df0f2f9   bl        #0x8025450
0801806c: 0020       movs      r0, #0
0801806e: 8df80c00   strb.w    r0, [sp, #0xc]
08018072: 0120       movs      r0, #1
08018074: 8df80f00   strb.w    r0, [sp, #0xf]
08018078: 0020       movs      r0, #0
0801807a: 8df80d00   strb.w    r0, [sp, #0xd]
0801807e: 0220       movs      r0, #2
08018080: 8df80e00   strb.w    r0, [sp, #0xe]
08018084: 8003       lsls      r0, r0, #0xe
08018086: 0290       str       r0, [sp, #8]
08018088: 02a9       add       r1, sp, #8
0801808a: 3048       ldr       r0, [pc, #0xc0] ; literal[0801814c]=40020000
0801808c: 0df0eaf9   bl        #0x8025464
08018090: 0622       movs      r2, #6
08018092: 0f21       movs      r1, #0xf
08018094: 2d48       ldr       r0, [pc, #0xb4] ; literal[0801814c]=40020000
08018096: 0df02bfa   bl        #0x80254f0
0801809a: 0020       movs      r0, #0
0801809c: 8df80c00   strb.w    r0, [sp, #0xc]
080180a0: 8df80d00   strb.w    r0, [sp, #0xd]
080180a4: 0120       movs      r0, #1
080180a6: 8df80e00   strb.w    r0, [sp, #0xe]
080180aa: 8df80f00   strb.w    r0, [sp, #0xf]
080180ae: 4ff4c040   mov.w     r0, #0x6000
080180b2: 0290       str       r0, [sp, #8]
080180b4: 02a9       add       r1, sp, #8
080180b6: 2648       ldr       r0, [pc, #0x98] ; literal[08018150]=40020800
080180b8: 0df0d4f9   bl        #0x8025464
080180bc: 4ff48040   mov.w     r0, #0x4000
080180c0: 2349       ldr       r1, [pc, #0x8c] ; literal[08018150]=40020800
080180c2: 2831       adds      r1, #0x28
080180c4: 0860       str       r0, [r1]
080180c6: 4010       asrs      r0, r0, #1
080180c8: 0860       str       r0, [r1]
080180ca: 0020       movs      r0, #0
080180cc: 8df80c00   strb.w    r0, [sp, #0xc]
080180d0: 0120       movs      r0, #1
080180d2: 8df80f00   strb.w    r0, [sp, #0xf]
080180d6: 0020       movs      r0, #0
080180d8: 8df80d00   strb.w    r0, [sp, #0xd]
080180dc: 0220       movs      r0, #2
080180de: 8df80e00   strb.w    r0, [sp, #0xe]
080180e2: 2820       movs      r0, #0x28
080180e4: 0290       str       r0, [sp, #8]
080180e6: 02a9       add       r1, sp, #8
080180e8: 1a48       ldr       r0, [pc, #0x68] ; literal[08018154]=40020400
080180ea: 0df0bbf9   bl        #0x8025464
080180ee: 0622       movs      r2, #6
080180f0: 0521       movs      r1, #5
080180f2: 1848       ldr       r0, [pc, #0x60] ; literal[08018154]=40020400
080180f4: 0df0fcf9   bl        #0x80254f0
080180f8: 0622       movs      r2, #6
080180fa: 0321       movs      r1, #3
080180fc: 1548       ldr       r0, [pc, #0x54] ; literal[08018154]=40020400
080180fe: 0df0f7f9   bl        #0x80254f0
08018102: 6846       mov       r0, sp
08018104: 0df00efd   bl        #0x8025b24
08018108: 0320       movs      r0, #3
0801810a: 8df80000   strb.w    r0, [sp]
0801810e: 0120       movs      r0, #1
08018110: 8df80100   strb.w    r0, [sp, #1]
08018114: 0020       movs      r0, #0
08018116: 8df80200   strb.w    r0, [sp, #2]
0801811a: 8df80300   strb.w    r0, [sp, #3]
0801811e: 8df80400   strb.w    r0, [sp, #4]
08018122: 0120       movs      r0, #1
08018124: 8df80500   strb.w    r0, [sp, #5]
08018128: 0020       movs      r0, #0
0801812a: 8df80600   strb.w    r0, [sp, #6]
0801812e: 8df80700   strb.w    r0, [sp, #7]
08018132: 6946       mov       r1, sp
08018134: 0848       ldr       r0, [pc, #0x20] ; literal[08018158]=40003c00
08018136: 0df022fd   bl        #0x8025b7e
0801813a: 0121       movs      r1, #1
0801813c: 0648       ldr       r0, [pc, #0x18] ; literal[08018158]=40003c00
0801813e: 0df001fd   bl        #0x8025b44
08018142: 0121       movs      r1, #1
08018144: 0448       ldr       r0, [pc, #0x10] ; literal[08018158]=40003c00
08018146: 0df0f8fc   bl        #0x8025b3a
0801814a: 1fbd       pop       {r0, r1, r2, r3, r4, pc}
0801814c: 0000       movs      r0, r0
0801814e: 0240       ands      r2, r0
08018150: 0008       lsrs      r0, r0, #0x20
08018152: 0240       ands      r2, r0
08018154: 0004       lsls      r0, r0, #0x10
08018156: 0240       ands      r2, r0
08018158: 003c       subs      r4, #0
0801815a: 0040       ands      r0, r0
0801815c: 70b5       push      {r4, r5, r6, lr}
0801815e: 0546       mov       r5, r0
08018160: 0c46       mov       r4, r1
08018162: 2a20       movs      r0, #0x2a
08018164: 00f00ef8   bl        #0x8018184
08018168: 2046       mov       r0, r4
0801816a: 00f046f8   bl        #0x80181fa
0801816e: 2b20       movs      r0, #0x2b
