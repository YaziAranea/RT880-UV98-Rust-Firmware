080182bc: 10b5       push      {r4, lr}
080182be: 3648       ldr       r0, [pc, #0xd8] ; literal[08018398]=20001206
080182c0: 0078       ldrb      r0, [r0]
080182c2: 28b1       cbz       r0, #0x80182d0
080182c4: 3448       ldr       r0, [pc, #0xd0] ; literal[08018398]=20001206
080182c6: 0078       ldrb      r0, [r0]
080182c8: 401e       subs      r0, r0, #1
080182ca: 3349       ldr       r1, [pc, #0xcc] ; literal[08018398]=20001206
080182cc: 0870       strb      r0, [r1]
080182ce: 06e0       b         #0x80182de
080182d0: 3248       ldr       r0, [pc, #0xc8] ; literal[0801839c]=20001204
080182d2: 0088       ldrh      r0, [r0]
080182d4: 18b1       cbz       r0, #0x80182de
080182d6: 4ff48070   mov.w     r0, #0x100
080182da: 3149       ldr       r1, [pc, #0xc4] ; literal[080183a0]=40021418
080182dc: 0860       str       r0, [r1]
080182de: 3148       ldr       r0, [pc, #0xc4] ; literal[080183a4]=20001210
080182e0: 0088       ldrh      r0, [r0]
080182e2: 28b1       cbz       r0, #0x80182f0
080182e4: 2f48       ldr       r0, [pc, #0xbc] ; literal[080183a4]=20001210
080182e6: 0088       ldrh      r0, [r0]
080182e8: 401e       subs      r0, r0, #1
080182ea: 2e49       ldr       r1, [pc, #0xb8] ; literal[080183a4]=20001210
080182ec: 0880       strh      r0, [r1]
080182ee: 10bd       pop       {r4, pc}

0801f084: 0648       ldr       r0, [pc, #0x18] ; literal[0801f0a0]=20001204
0801f086: 0088       ldrh      r0, [r0]
0801f088: b0f5004f   cmp.w     r0, #0x8000
0801f08c: 06d1       bne       #0x801f09c
0801f08e: 0020       movs      r0, #0
0801f090: 0349       ldr       r1, [pc, #0xc] ; literal[0801f0a0]=20001204
0801f092: 0880       strh      r0, [r1]
0801f094: 4ff48070   mov.w     r0, #0x100
0801f098: 0249       ldr       r1, [pc, #8] ; literal[0801f0a4]=40021428
0801f09a: 0860       str       r0, [r1]
0801f09c: 7047       bx        lr
0801f09e: 0000       movs      r0, r0
0801f0a0: 0412       asrs      r4, r0, #8
0801f0a2: 0020       movs      r0, #0
0801f0a4: 2814       asrs      r0, r5, #0x10
0801f0a6: 0240       ands      r2, r0

0801fc70: 0549       ldr       r1, [pc, #0x14] ; literal[0801fc88]=20001204
0801fc72: 0988       ldrh      r1, [r1]
0801fc74: 0143       orrs      r1, r0
0801fc76: 044a       ldr       r2, [pc, #0x10] ; literal[0801fc88]=20001204
0801fc78: 1180       strh      r1, [r2]
0801fc7a: 1146       mov       r1, r2
0801fc7c: 0988       ldrh      r1, [r1]
0801fc7e: 41f40041   orr       r1, r1, #0x8000
0801fc82: 1180       strh      r1, [r2]
0801fc84: 7047       bx        lr
0801fc86: 0000       movs      r0, r0
0801fc88: 0412       asrs      r4, r0, #8
0801fc8a: 0020       movs      r0, #0
