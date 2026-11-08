080050c6: 4048       ldr       r0, [pc, #0x100] ; literal[080051c8]=200010ec
080050c8: 0068       ldr       r0, [r0]
080050ca: c07c       ldrb      r0, [r0, #0x13]
080050cc: 00f00f00   and       r0, r0, #0xf
080050d0: 28b1       cbz       r0, #0x80050de
080050d2: 0422       movs      r2, #4
080050d4: 0121       movs      r1, #1
080050d6: 0020       movs      r0, #0
080050d8: fef7fafb   bl        #0x80038d0
080050dc: 05e0       b         #0x80050ea
080050de: 3c48       ldr       r0, [pc, #0xf0] ; literal[080051d0]=20003044
080050e0: c27a       ldrb      r2, [r0, #0xb]
080050e2: 0121       movs      r1, #1
080050e4: 0020       movs      r0, #0
080050e6: fef7f3fb   bl        #0x80038d0
080050ea: 3748       ldr       r0, [pc, #0xdc] ; literal[080051c8]=200010ec
080050ec: 0068       ldr       r0, [r0]
080050ee: 0078       ldrb      r0, [r0]
080050f0: 30b1       cbz       r0, #0x8005100
080050f2: 0128       cmp       r0, #1
080050f4: 1ad0       beq       #0x800512c
080050f6: 0228       cmp       r0, #2
080050f8: 2ed0       beq       #0x8005158
080050fa: 0328       cmp       r0, #3
080050fc: 43d1       bne       #0x8005186
080050fe: 2ce0       b         #0x800515a
08005100: 3448       ldr       r0, [pc, #0xd0] ; literal[080051d4]=200035ec
08005102: 90f8ae40   ldrb.w    r4, [r0, #0xae]
08005106: 4ff6ff70   movw      r0, #0xffff
0800510a: 00ea0414   and.w     r4, r0, r4, lsl #4
0800510e: 04f53040   add.w     r0, r4, #0xb000
08005112: 84b2       uxth      r4, r0
08005114: 2f48       ldr       r0, [pc, #0xbc] ; literal[080051d4]=200035ec
08005116: 90f8af00   ldrb.w    r0, [r0, #0xaf]
0800511a: 2044       add       r0, r4
0800511c: 84b2       uxth      r4, r0
0800511e: 0020       movs      r0, #0
08005120: 2d49       ldr       r1, [pc, #0xb4] ; literal[080051d8]=20000810
08005122: 0870       strb      r0, [r1]
08005124: 0120       movs      r0, #1
08005126: fef7c1fb   bl        #0x80038ac
0800512a: 2ce0       b         #0x8005186
0800512c: 2948       ldr       r0, [pc, #0xa4] ; literal[080051d4]=200035ec
0800512e: 90f8b040   ldrb.w    r4, [r0, #0xb0]
08005132: 4ff6ff70   movw      r0, #0xffff
08005136: 00ea0414   and.w     r4, r0, r4, lsl #4
0800513a: 04f53040   add.w     r0, r4, #0xb000
0800513e: 84b2       uxth      r4, r0
08005140: 2448       ldr       r0, [pc, #0x90] ; literal[080051d4]=200035ec
08005142: 90f8b100   ldrb.w    r0, [r0, #0xb1]
08005146: 2044       add       r0, r4
08005148: 84b2       uxth      r4, r0
0800514a: 0120       movs      r0, #1
0800514c: 2249       ldr       r1, [pc, #0x88] ; literal[080051d8]=20000810
0800514e: 0870       strb      r0, [r1]
08005150: 0720       movs      r0, #7
08005152: fef7abfb   bl        #0x80038ac
08005156: 16e0       b         #0x8005186
08005158: 00bf       nop       
0800515a: 1e48       ldr       r0, [pc, #0x78] ; literal[080051d4]=200035ec
0800515c: 90f8b040   ldrb.w    r4, [r0, #0xb0]
08005160: 4ff6ff70   movw      r0, #0xffff
08005164: 00ea0414   and.w     r4, r0, r4, lsl #4
08005168: 04f53040   add.w     r0, r4, #0xb000
0800516c: 84b2       uxth      r4, r0
0800516e: 1948       ldr       r0, [pc, #0x64] ; literal[080051d4]=200035ec
08005170: 90f8b100   ldrb.w    r0, [r0, #0xb1]
08005174: 2044       add       r0, r4
08005176: 84b2       uxth      r4, r0
08005178: 0120       movs      r0, #1
0800517a: 1749       ldr       r1, [pc, #0x5c] ; literal[080051d8]=20000810
0800517c: 0870       strb      r0, [r1]
0800517e: 0520       movs      r0, #5
08005180: fef794fb   bl        #0x80038ac
08005184: 00bf       nop       
08005186: 00bf       nop       
08005188: 2046       mov       r0, r4
0800518a: 00f0edfa   bl        #0x8005768
