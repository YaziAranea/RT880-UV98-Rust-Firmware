; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
0801fba0: 10b5       push      {r4, lr}
0801fba2: 2648       ldr       r0, [pc, #0x98] ; literal[0801fc3c]=20001132
0801fba4: 0078       ldrb      r0, [r0]
0801fba6: fff779ff   bl        #0x801fa9c
0801fbaa: 2448       ldr       r0, [pc, #0x90] ; literal[0801fc3c]=20001132
0801fbac: 0078       ldrb      r0, [r0]
0801fbae: 30b1       cbz       r0, #0x801fbbe
0801fbb0: 0128       cmp       r0, #1
0801fbb2: 0fd0       beq       #0x801fbd4
0801fbb4: 0228       cmp       r0, #2
0801fbb6: 18d0       beq       #0x801fbea
0801fbb8: 0328       cmp       r0, #3
0801fbba: 2dd1       bne       #0x801fc18
0801fbbc: 20e0       b         #0x801fc00
0801fbbe: 0420       movs      r0, #4
0801fbc0: 1f49       ldr       r1, [pc, #0x7c] ; literal[0801fc40]=40020428
0801fbc2: 0860       str       r0, [r1]
0801fbc4: 0002       lsls      r0, r0, #8
0801fbc6: 1e49       ldr       r1, [pc, #0x78] ; literal[0801fc40]=40020428
0801fbc8: 1039       subs      r1, #0x10
0801fbca: 0860       str       r0, [r1]
0801fbcc: 0120       movs      r0, #1
0801fbce: 1b49       ldr       r1, [pc, #0x6c] ; literal[0801fc3c]=20001132
0801fbd0: 0870       strb      r0, [r1]
0801fbd2: 30e0       b         #0x801fc36
0801fbd4: 0220       movs      r0, #2
0801fbd6: 1a49       ldr       r1, [pc, #0x68] ; literal[0801fc40]=40020428
0801fbd8: 0860       str       r0, [r1]
0801fbda: 0420       movs      r0, #4
0801fbdc: 1849       ldr       r1, [pc, #0x60] ; literal[0801fc40]=40020428
0801fbde: 1039       subs      r1, #0x10
0801fbe0: 0860       str       r0, [r1]
0801fbe2: 0220       movs      r0, #2
0801fbe4: 1549       ldr       r1, [pc, #0x54] ; literal[0801fc3c]=20001132
0801fbe6: 0870       strb      r0, [r1]
0801fbe8: 25e0       b         #0x801fc36
0801fbea: 0120       movs      r0, #1
0801fbec: 1449       ldr       r1, [pc, #0x50] ; literal[0801fc40]=40020428
0801fbee: 0860       str       r0, [r1]
0801fbf0: 0220       movs      r0, #2
0801fbf2: 1349       ldr       r1, [pc, #0x4c] ; literal[0801fc40]=40020428
0801fbf4: 1039       subs      r1, #0x10
0801fbf6: 0860       str       r0, [r1]
0801fbf8: 0320       movs      r0, #3
0801fbfa: 1049       ldr       r1, [pc, #0x40] ; literal[0801fc3c]=20001132
0801fbfc: 0870       strb      r0, [r1]
0801fbfe: 1ae0       b         #0x801fc36
0801fc00: 4ff48060   mov.w     r0, #0x400
0801fc04: 0e49       ldr       r1, [pc, #0x38] ; literal[0801fc40]=40020428
0801fc06: 0860       str       r0, [r1]
0801fc08: 0120       movs      r0, #1
0801fc0a: 0d49       ldr       r1, [pc, #0x34] ; literal[0801fc40]=40020428
0801fc0c: 1039       subs      r1, #0x10
0801fc0e: 0860       str       r0, [r1]
0801fc10: 0020       movs      r0, #0
0801fc12: 0a49       ldr       r1, [pc, #0x28] ; literal[0801fc3c]=20001132
0801fc14: 0870       strb      r0, [r1]
0801fc16: 0ee0       b         #0x801fc36
0801fc18: 4ff48060   mov.w     r0, #0x400
0801fc1c: 0849       ldr       r1, [pc, #0x20] ; literal[0801fc40]=40020428
0801fc1e: 1039       subs      r1, #0x10
0801fc20: 0860       str       r0, [r1]
0801fc22: 0420       movs      r0, #4
0801fc24: 0860       str       r0, [r1]
0801fc26: 0220       movs      r0, #2
0801fc28: 0860       str       r0, [r1]
0801fc2a: 0120       movs      r0, #1
0801fc2c: 0860       str       r0, [r1]
0801fc2e: 0020       movs      r0, #0
0801fc30: 0249       ldr       r1, [pc, #8] ; literal[0801fc3c]=20001132
0801fc32: 0870       strb      r0, [r1]
0801fc34: 00bf       nop       
0801fc36: 00bf       nop       
0801fc38: 10bd       pop       {r4, pc}
0801fc3a: 0000       movs      r0, r0
0801fc3c: 3211       asrs      r2, r6, #4
0801fc3e: 0020       movs      r0, #0
0801fc40: 2804       lsls      r0, r5, #0x10
0801fc42: 0240       ands      r2, r0
