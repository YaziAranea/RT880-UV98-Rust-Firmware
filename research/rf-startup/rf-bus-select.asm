0801e620: 70b5       push      {r4, r5, r6, lr}
0801e622: 0446       mov       r4, r0
0801e624: 4020       movs      r0, #0x40
0801e626: 0c49       ldr       r1, [pc, #0x30] ; literal[0801e658]=40021428
0801e628: 0860       str       r0, [r1]
0801e62a: 8000       lsls      r0, r0, #2
0801e62c: 0b49       ldr       r1, [pc, #0x2c] ; literal[0801e65c]=40020000
0801e62e: 8862       str       r0, [r1, #0x28]
0801e630: 44f08000   orr       r0, r4, #0x80
0801e634: fff758f9   bl        #0x801d8e8
0801e638: 0a20       movs      r0, #0xa
0801e63a: 06f0cffd   bl        #0x80251dc
0801e63e: fff7fdf8   bl        #0x801d83c
0801e642: 0546       mov       r5, r0
0801e644: 4ff48070   mov.w     r0, #0x100
0801e648: 0449       ldr       r1, [pc, #0x10] ; literal[0801e65c]=40020000
0801e64a: 8861       str       r0, [r1, #0x18]
0801e64c: 0020       movs      r0, #0
0801e64e: fff721f9   bl        #0x801d894
0801e652: 2846       mov       r0, r5
0801e654: 70bd       pop       {r4, r5, r6, pc}
0801e656: 0000       movs      r0, r0
0801e658: 2814       asrs      r0, r5, #0x10
0801e65a: 0240       ands      r2, r0
0801e65c: 0000       movs      r0, r0
0801e65e: 0240       ands      r2, r0
0801e660: 30b5       push      {r4, r5, lr}
0801e662: 0546       mov       r5, r0
0801e664: 0c46       mov       r4, r1
0801e666: 4020       movs      r0, #0x40
0801e668: 0949       ldr       r1, [pc, #0x24] ; literal[0801e690]=40021428
0801e66a: 0860       str       r0, [r1]
0801e66c: 8000       lsls      r0, r0, #2
0801e66e: 0949       ldr       r1, [pc, #0x24] ; literal[0801e694]=40020000
0801e670: 8862       str       r0, [r1, #0x28]
0801e672: 2846       mov       r0, r5
0801e674: fff738f9   bl        #0x801d8e8
0801e678: 2012       asrs      r0, r4, #8
0801e67a: fff735f9   bl        #0x801d8e8
0801e67e: e0b2       uxtb      r0, r4
0801e680: fff732f9   bl        #0x801d8e8
0801e684: 4ff48070   mov.w     r0, #0x100
0801e688: 0249       ldr       r1, [pc, #8] ; literal[0801e694]=40020000
0801e68a: 8861       str       r0, [r1, #0x18]
0801e68c: 30bd       pop       {r4, r5, pc}
0801e68e: 0000       movs      r0, r0
0801e690: 2814       asrs      r0, r5, #0x10
0801e692: 0240       ands      r2, r0
0801e694: 0000       movs      r0, r0
0801e696: 0240       ands      r2, r0
0801e698: 70b5       push      {r4, r5, r6, lr}
0801e69a: 0446       mov       r4, r0
0801e69c: 0020       movs      r0, #0
0801e69e: fff77df8   bl        #0x801d79c
0801e6a2: 4ff48070   mov.w     r0, #0x100
0801e6a6: 0a49       ldr       r1, [pc, #0x28] ; literal[0801e6d0]=40020828
0801e6a8: 0860       str       r0, [r1]
0801e6aa: 8020       movs      r0, #0x80
0801e6ac: 0860       str       r0, [r1]
0801e6ae: 44f08000   orr       r0, r4, #0x80
0801e6b2: fff79bf8   bl        #0x801d7ec
0801e6b6: 0a20       movs      r0, #0xa
0801e6b8: 06f090fd   bl        #0x80251dc
0801e6bc: fff740f8   bl        #0x801d740
0801e6c0: 0546       mov       r5, r0
0801e6c2: 8020       movs      r0, #0x80
0801e6c4: 0249       ldr       r1, [pc, #8] ; literal[0801e6d0]=40020828
0801e6c6: 1039       subs      r1, #0x10
0801e6c8: 0860       str       r0, [r1]
0801e6ca: 2846       mov       r0, r5
0801e6cc: 70bd       pop       {r4, r5, r6, pc}
0801e6ce: 0000       movs      r0, r0
0801e6d0: 2808       lsrs      r0, r5, #0x20
0801e6d2: 0240       ands      r2, r0
0801e6d4: 70b5       push      {r4, r5, r6, lr}
0801e6d6: 0546       mov       r5, r0
0801e6d8: 0c46       mov       r4, r1
0801e6da: 0020       movs      r0, #0
0801e6dc: fff75ef8   bl        #0x801d79c
0801e6e0: 4ff48070   mov.w     r0, #0x100
0801e6e4: 0849       ldr       r1, [pc, #0x20] ; literal[0801e708]=40020828
0801e6e6: 0860       str       r0, [r1]
0801e6e8: 8020       movs      r0, #0x80
0801e6ea: 0860       str       r0, [r1]
0801e6ec: 2846       mov       r0, r5
0801e6ee: fff77df8   bl        #0x801d7ec
0801e6f2: 2012       asrs      r0, r4, #8
0801e6f4: fff77af8   bl        #0x801d7ec
0801e6f8: e0b2       uxtb      r0, r4
0801e6fa: fff777f8   bl        #0x801d7ec
0801e6fe: 8020       movs      r0, #0x80
0801e700: 0149       ldr       r1, [pc, #4] ; literal[0801e708]=40020828
0801e702: 1039       subs      r1, #0x10
0801e704: 0860       str       r0, [r1]
0801e706: 70bd       pop       {r4, r5, r6, pc}
0801e708: 2808       lsrs      r0, r5, #0x20
0801e70a: 0240       ands      r2, r0
0801e70c: 10b5       push      {r4, lr}
0801e70e: 0448       ldr       r0, [pc, #0x10] ; literal[0801e720]=0802ec10
