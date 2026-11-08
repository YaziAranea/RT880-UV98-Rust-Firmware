0801d740: 70b5       push      {r4, r5, r6, lr}
0801d742: 0024       movs      r4, #0
0801d744: 0120       movs      r0, #1
0801d746: 00f029f8   bl        #0x801d79c
0801d74a: 4ff48070   mov.w     r0, #0x100
0801d74e: 1249       ldr       r1, [pc, #0x48] ; literal[0801d798]=40020828
0801d750: 0860       str       r0, [r1]
0801d752: 0025       movs      r5, #0
0801d754: 1ce0       b         #0x801d790
0801d756: 4ff6ff70   movw      r0, #0xffff
0801d75a: 00ea4404   and.w     r4, r0, r4, lsl #1
0801d75e: 4ff48070   mov.w     r0, #0x100
0801d762: 0d49       ldr       r1, [pc, #0x34] ; literal[0801d798]=40020828
0801d764: 1039       subs      r1, #0x10
0801d766: 0860       str       r0, [r1]
0801d768: 4021       movs      r1, #0x40
0801d76a: 0b48       ldr       r0, [pc, #0x2c] ; literal[0801d798]=40020828
0801d76c: 2838       subs      r0, #0x28
0801d76e: 07f0b6fe   bl        #0x80254de
0801d772: 08b1       cbz       r0, #0x801d778
0801d774: 44f00104   orr       r4, r4, #1
0801d778: 0a20       movs      r0, #0xa
0801d77a: 07f02ffd   bl        #0x80251dc
0801d77e: 4ff48070   mov.w     r0, #0x100
0801d782: 0549       ldr       r1, [pc, #0x14] ; literal[0801d798]=40020828
0801d784: 0860       str       r0, [r1]
0801d786: 0a20       movs      r0, #0xa
0801d788: 07f028fd   bl        #0x80251dc
0801d78c: 681c       adds      r0, r5, #1
0801d78e: c5b2       uxtb      r5, r0
0801d790: 102d       cmp       r5, #0x10
0801d792: e0db       blt       #0x801d756
0801d794: 2046       mov       r0, r4
0801d796: 70bd       pop       {r4, r5, r6, pc}
0801d798: 2808       lsrs      r0, r5, #0x20
0801d79a: 0240       ands      r2, r0
0801d79c: 1cb5       push      {r2, r3, r4, lr}
0801d79e: 0446       mov       r4, r0
0801d7a0: 6846       mov       r0, sp
0801d7a2: 07f055fe   bl        #0x8025450
0801d7a6: 94b9       cbnz      r4, #0x801d7ce
0801d7a8: 0220       movs      r0, #2
0801d7aa: 8df80700   strb.w    r0, [sp, #7]
0801d7ae: 0020       movs      r0, #0
0801d7b0: 8df80400   strb.w    r0, [sp, #4]
0801d7b4: 0120       movs      r0, #1
0801d7b6: 8df80600   strb.w    r0, [sp, #6]
0801d7ba: 4020       movs      r0, #0x40
0801d7bc: 0090       str       r0, [sp]
0801d7be: 0020       movs      r0, #0
0801d7c0: 8df80500   strb.w    r0, [sp, #5]
0801d7c4: 6946       mov       r1, sp
0801d7c6: 0848       ldr       r0, [pc, #0x20] ; literal[0801d7e8]=40020800
0801d7c8: 07f04cfe   bl        #0x8025464
0801d7cc: 0be0       b         #0x801d7e6
0801d7ce: 0020       movs      r0, #0
0801d7d0: 8df80600   strb.w    r0, [sp, #6]
0801d7d4: 4020       movs      r0, #0x40
0801d7d6: 0090       str       r0, [sp]
0801d7d8: 0020       movs      r0, #0
0801d7da: 8df80500   strb.w    r0, [sp, #5]
0801d7de: 6946       mov       r1, sp
0801d7e0: 0148       ldr       r0, [pc, #4] ; literal[0801d7e8]=40020800
0801d7e2: 07f03ffe   bl        #0x8025464
0801d7e6: 1cbd       pop       {r2, r3, r4, pc}
0801d7e8: 0008       lsrs      r0, r0, #0x20
0801d7ea: 0240       ands      r2, r0
0801d7ec: 10b5       push      {r4, lr}
0801d7ee: 0246       mov       r2, r0
0801d7f0: 1346       mov       r3, r2
0801d7f2: 0024       movs      r4, #0
0801d7f4: 1de0       b         #0x801d832
0801d7f6: 4ff48070   mov.w     r0, #0x100
0801d7fa: 0f49       ldr       r1, [pc, #0x3c] ; literal[0801d838]=40020828
0801d7fc: 0860       str       r0, [r1]
0801d7fe: 03f08000   and       r0, r3, #0x80
0801d802: 20b1       cbz       r0, #0x801d80e
0801d804: 4020       movs      r0, #0x40
0801d806: 0c49       ldr       r1, [pc, #0x30] ; literal[0801d838]=40020828
0801d808: 1039       subs      r1, #0x10
0801d80a: 0860       str       r0, [r1]
0801d80c: 02e0       b         #0x801d814
0801d80e: 4020       movs      r0, #0x40
0801d810: 0949       ldr       r1, [pc, #0x24] ; literal[0801d838]=40020828
0801d812: 0860       str       r0, [r1]
0801d814: 0a20       movs      r0, #0xa
0801d816: 07f0e1fc   bl        #0x80251dc
0801d81a: 4ff48070   mov.w     r0, #0x100
0801d81e: 0649       ldr       r1, [pc, #0x18] ; literal[0801d838]=40020828
0801d820: 1039       subs      r1, #0x10
0801d822: 0860       str       r0, [r1]
0801d824: 5806       lsls      r0, r3, #0x19
0801d826: 030e       lsrs      r3, r0, #0x18
0801d828: 0a20       movs      r0, #0xa
0801d82a: 07f0d7fc   bl        #0x80251dc
0801d82e: 601c       adds      r0, r4, #1
0801d830: c4b2       uxtb      r4, r0
0801d832: 082c       cmp       r4, #8
0801d834: dfdb       blt       #0x801d7f6
0801d836: 10bd       pop       {r4, pc}
0801d838: 2808       lsrs      r0, r5, #0x20
0801d83a: 0240       ands      r2, r0
0801d83c: 70b5       push      {r4, r5, r6, lr}
0801d83e: 0024       movs      r4, #0
0801d840: 0120       movs      r0, #1
0801d842: 00f027f8   bl        #0x801d894
0801d846: 4020       movs      r0, #0x40
0801d848: 1049       ldr       r1, [pc, #0x40] ; literal[0801d88c]=40021428
0801d84a: 0860       str       r0, [r1]
0801d84c: 0025       movs      r5, #0
0801d84e: 19e0       b         #0x801d884
0801d850: 4ff6ff70   movw      r0, #0xffff
0801d854: 00ea4404   and.w     r4, r0, r4, lsl #1
0801d858: 4020       movs      r0, #0x40
0801d85a: 0c49       ldr       r1, [pc, #0x30] ; literal[0801d88c]=40021428
0801d85c: 1039       subs      r1, #0x10
0801d85e: 0860       str       r0, [r1]
0801d860: c100       lsls      r1, r0, #3
0801d862: 0b48       ldr       r0, [pc, #0x2c] ; literal[0801d890]=40020800
0801d864: 07f03bfe   bl        #0x80254de
0801d868: 08b1       cbz       r0, #0x801d86e
0801d86a: 44f00104   orr       r4, r4, #1
0801d86e: 0a20       movs      r0, #0xa
0801d870: 07f0b4fc   bl        #0x80251dc
0801d874: 4020       movs      r0, #0x40
0801d876: 0549       ldr       r1, [pc, #0x14] ; literal[0801d88c]=40021428
0801d878: 0860       str       r0, [r1]
0801d87a: 0a20       movs      r0, #0xa
0801d87c: 07f0aefc   bl        #0x80251dc
0801d880: 681c       adds      r0, r5, #1
0801d882: c5b2       uxtb      r5, r0
0801d884: 102d       cmp       r5, #0x10
0801d886: e3db       blt       #0x801d850
0801d888: 2046       mov       r0, r4
0801d88a: 70bd       pop       {r4, r5, r6, pc}
0801d88c: 2814       asrs      r0, r5, #0x10
0801d88e: 0240       ands      r2, r0
0801d890: 0008       lsrs      r0, r0, #0x20
0801d892: 0240       ands      r2, r0
0801d894: 1cb5       push      {r2, r3, r4, lr}
0801d896: 0446       mov       r4, r0
0801d898: 6846       mov       r0, sp
0801d89a: 07f0d9fd   bl        #0x8025450
0801d89e: 94b9       cbnz      r4, #0x801d8c6
0801d8a0: 0220       movs      r0, #2
0801d8a2: 8df80700   strb.w    r0, [sp, #7]
0801d8a6: 0020       movs      r0, #0
0801d8a8: 8df80400   strb.w    r0, [sp, #4]
0801d8ac: 0120       movs      r0, #1
0801d8ae: 8df80600   strb.w    r0, [sp, #6]
0801d8b2: 4002       lsls      r0, r0, #9
0801d8b4: 0090       str       r0, [sp]
0801d8b6: 0020       movs      r0, #0
0801d8b8: 8df80500   strb.w    r0, [sp, #5]
0801d8bc: 6946       mov       r1, sp
0801d8be: 0948       ldr       r0, [pc, #0x24] ; literal[0801d8e4]=40020800
0801d8c0: 07f0d0fd   bl        #0x8025464
0801d8c4: 0ce0       b         #0x801d8e0
0801d8c6: 0020       movs      r0, #0
0801d8c8: 8df80600   strb.w    r0, [sp, #6]
0801d8cc: 4ff40070   mov.w     r0, #0x200
0801d8d0: 0090       str       r0, [sp]
0801d8d2: 0020       movs      r0, #0
0801d8d4: 8df80500   strb.w    r0, [sp, #5]
0801d8d8: 6946       mov       r1, sp
0801d8da: 0248       ldr       r0, [pc, #8] ; literal[0801d8e4]=40020800
0801d8dc: 07f0c2fd   bl        #0x8025464
0801d8e0: 1cbd       pop       {r2, r3, r4, pc}
0801d8e2: 0000       movs      r0, r0
0801d8e4: 0008       lsrs      r0, r0, #0x20
0801d8e6: 0240       ands      r2, r0
0801d8e8: 10b5       push      {r4, lr}
0801d8ea: 0246       mov       r2, r0
0801d8ec: 1346       mov       r3, r2
0801d8ee: 0024       movs      r4, #0
0801d8f0: 1ce0       b         #0x801d92c
0801d8f2: 4020       movs      r0, #0x40
0801d8f4: 0f49       ldr       r1, [pc, #0x3c] ; literal[0801d934]=40021428
0801d8f6: 0860       str       r0, [r1]
0801d8f8: 03f08000   and       r0, r3, #0x80
0801d8fc: 18b1       cbz       r0, #0x801d906
0801d8fe: 4815       asrs      r0, r1, #0x15
0801d900: 0d49       ldr       r1, [pc, #0x34] ; literal[0801d938]=40020818
0801d902: 0860       str       r0, [r1]
0801d904: 04e0       b         #0x801d910
0801d906: 4ff40070   mov.w     r0, #0x200
0801d90a: 0b49       ldr       r1, [pc, #0x2c] ; literal[0801d938]=40020818
0801d90c: 1031       adds      r1, #0x10
0801d90e: 0860       str       r0, [r1]
0801d910: 0a20       movs      r0, #0xa
0801d912: 07f063fc   bl        #0x80251dc
0801d916: 4020       movs      r0, #0x40
0801d918: 0649       ldr       r1, [pc, #0x18] ; literal[0801d934]=40021428
0801d91a: 1039       subs      r1, #0x10
0801d91c: 0860       str       r0, [r1]
0801d91e: 5806       lsls      r0, r3, #0x19
0801d920: 030e       lsrs      r3, r0, #0x18
0801d922: 0a20       movs      r0, #0xa
0801d924: 07f05afc   bl        #0x80251dc
0801d928: 601c       adds      r0, r4, #1
0801d92a: c4b2       uxtb      r4, r0
0801d92c: 082c       cmp       r4, #8
0801d92e: e0db       blt       #0x801d8f2
0801d930: 10bd       pop       {r4, pc}
0801d932: 0000       movs      r0, r0
0801d934: 2814       asrs      r0, r5, #0x10
0801d936: 0240       ands      r2, r0
0801d938: 1808       lsrs      r0, r3, #0x20
0801d93a: 0240       ands      r2, r0
0801d93c: 0146       mov       r1, r0
0801d93e: 0929       cmp       r1, #9
0801d940: 03dc       bgt       #0x801d94a
0801d942: 01f13000   add.w     r0, r1, #0x30
0801d946: c0b2       uxtb      r0, r0
0801d948: 7047       bx        lr
0801d94a: 0a29       cmp       r1, #0xa
0801d94c: 05db       blt       #0x801d95a
0801d94e: 0d29       cmp       r1, #0xd
0801d950: 03dc       bgt       #0x801d95a
0801d952: 01f1       .byte     0x01, 0xf1
