0801f828: 10b5       push      {r4, lr}
0801f82a: 0948       ldr       r0, [pc, #0x24] ; literal[0801f850]=20001122
0801f82c: 0078       ldrb      r0, [r0]
0801f82e: 18b9       cbnz      r0, #0x801f838
0801f830: 4ff40050   mov.w     r0, #0x2000
0801f834: 0749       ldr       r1, [pc, #0x1c] ; literal[0801f854]=40020000
0801f836: 8862       str       r0, [r1, #0x28]
0801f838: e5f7c4ff   bl        #0x80057c4
0801f83c: 0649       ldr       r1, [pc, #0x18] ; literal[0801f858]=200010ec
0801f83e: 0968       ldr       r1, [r1]
0801f840: c878       ldrb      r0, [r1, #3]
0801f842: e4f7f1fc   bl        #0x8004228
0801f846: e4f767fe   bl        #0x8004518
0801f84a: e5f7d1f9   bl        #0x8004bf0
0801f84e: 10bd       pop       {r4, pc}
0801f850: 2211       asrs      r2, r4, #4
0801f852: 0020       movs      r0, #0
0801f854: 0000       movs      r0, r0
0801f856: 0240       ands      r2, r0
0801f858: ec10       asrs      r4, r5, #3
0801f85a: 0020       movs      r0, #0
