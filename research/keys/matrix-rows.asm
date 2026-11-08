; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
0801fa9c: 70b5       push      {r4, r5, r6, lr}
0801fa9e: 0546       mov       r5, r0
0801faa0: 0124       movs      r4, #1
0801faa2: a800       lsls      r0, r5, #2
0801faa4: 04fa00f0   lsl.w     r0, r4, r0
0801faa8: 84b2       uxth      r4, r0
0801faaa: 4ff48061   mov.w     r1, #0x400
0801faae: 2b48       ldr       r0, [pc, #0xac] ; literal[0801fb5c]=40020800
0801fab0: 05f015fd   bl        #0x80254de
0801fab4: 28b9       cbnz      r0, #0x801fac2
0801fab6: 2a48       ldr       r0, [pc, #0xa8] ; literal[0801fb60]=20001134
0801fab8: 0088       ldrh      r0, [r0]
0801faba: 2043       orrs      r0, r4
0801fabc: 2849       ldr       r1, [pc, #0xa0] ; literal[0801fb60]=20001134
0801fabe: 0880       strh      r0, [r1]
0801fac0: 04e0       b         #0x801facc
0801fac2: 2748       ldr       r0, [pc, #0x9c] ; literal[0801fb60]=20001134
0801fac4: 0088       ldrh      r0, [r0]
0801fac6: a043       bics      r0, r4
0801fac8: 2549       ldr       r1, [pc, #0x94] ; literal[0801fb60]=20001134
0801faca: 0880       strh      r0, [r1]
0801facc: 0124       movs      r4, #1
0801face: 0120       movs      r0, #1
0801fad0: 00eb8500   add.w     r0, r0, r5, lsl #2
0801fad4: 04fa00f0   lsl.w     r0, r4, r0
0801fad8: 84b2       uxth      r4, r0
0801fada: 4ff40061   mov.w     r1, #0x800
0801fade: 1f48       ldr       r0, [pc, #0x7c] ; literal[0801fb5c]=40020800
0801fae0: 05f0fdfc   bl        #0x80254de
0801fae4: 28b9       cbnz      r0, #0x801faf2
0801fae6: 1e48       ldr       r0, [pc, #0x78] ; literal[0801fb60]=20001134
0801fae8: 0088       ldrh      r0, [r0]
0801faea: 2043       orrs      r0, r4
0801faec: 1c49       ldr       r1, [pc, #0x70] ; literal[0801fb60]=20001134
0801faee: 0880       strh      r0, [r1]
0801faf0: 04e0       b         #0x801fafc
0801faf2: 1b48       ldr       r0, [pc, #0x6c] ; literal[0801fb60]=20001134
0801faf4: 0088       ldrh      r0, [r0]
0801faf6: a043       bics      r0, r4
0801faf8: 1949       ldr       r1, [pc, #0x64] ; literal[0801fb60]=20001134
0801fafa: 0880       strh      r0, [r1]
0801fafc: 0124       movs      r4, #1
0801fafe: 0220       movs      r0, #2
0801fb00: 00eb8500   add.w     r0, r0, r5, lsl #2
0801fb04: 04fa00f0   lsl.w     r0, r4, r0
0801fb08: 84b2       uxth      r4, r0
0801fb0a: 4ff48051   mov.w     r1, #0x1000
0801fb0e: 1348       ldr       r0, [pc, #0x4c] ; literal[0801fb5c]=40020800
0801fb10: 05f0e5fc   bl        #0x80254de
0801fb14: 28b9       cbnz      r0, #0x801fb22
0801fb16: 1248       ldr       r0, [pc, #0x48] ; literal[0801fb60]=20001134
0801fb18: 0088       ldrh      r0, [r0]
0801fb1a: 2043       orrs      r0, r4
0801fb1c: 1049       ldr       r1, [pc, #0x40] ; literal[0801fb60]=20001134
0801fb1e: 0880       strh      r0, [r1]
0801fb20: 04e0       b         #0x801fb2c
0801fb22: 0f48       ldr       r0, [pc, #0x3c] ; literal[0801fb60]=20001134
0801fb24: 0088       ldrh      r0, [r0]
0801fb26: a043       bics      r0, r4
0801fb28: 0d49       ldr       r1, [pc, #0x34] ; literal[0801fb60]=20001134
0801fb2a: 0880       strh      r0, [r1]
0801fb2c: 0124       movs      r4, #1
0801fb2e: 0320       movs      r0, #3
0801fb30: 00eb8500   add.w     r0, r0, r5, lsl #2
0801fb34: 04fa00f0   lsl.w     r0, r4, r0
0801fb38: 84b2       uxth      r4, r0
0801fb3a: 0421       movs      r1, #4
0801fb3c: 0948       ldr       r0, [pc, #0x24] ; literal[0801fb64]=40020c00
0801fb3e: 05f0cefc   bl        #0x80254de
0801fb42: 28b9       cbnz      r0, #0x801fb50
0801fb44: 0648       ldr       r0, [pc, #0x18] ; literal[0801fb60]=20001134
0801fb46: 0088       ldrh      r0, [r0]
0801fb48: 2043       orrs      r0, r4
0801fb4a: 0549       ldr       r1, [pc, #0x14] ; literal[0801fb60]=20001134
0801fb4c: 0880       strh      r0, [r1]
0801fb4e: 04e0       b         #0x801fb5a
0801fb50: 0348       ldr       r0, [pc, #0xc] ; literal[0801fb60]=20001134
0801fb52: 0088       ldrh      r0, [r0]
0801fb54: a043       bics      r0, r4
0801fb56: 0249       ldr       r1, [pc, #8] ; literal[0801fb60]=20001134
0801fb58: 0880       strh      r0, [r1]
0801fb5a: 70bd       pop       {r4, r5, r6, pc}
