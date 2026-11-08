08005098: 10b5       push      {r4, lr}
0800509a: 0024       movs      r4, #0
0800509c: 0220       movs      r0, #2
0800509e: 4949       ldr       r1, [pc, #0x124] ; literal[080051c4]=20001125
080050a0: 0870       strb      r0, [r1]
080050a2: 4948       ldr       r0, [pc, #0x124] ; literal[080051c8]=200010ec
080050a4: 0068       ldr       r0, [r0]
080050a6: 407c       ldrb      r0, [r0, #0x11]
080050a8: 0228       cmp       r0, #2
080050aa: 05d1       bne       #0x80050b8
080050ac: 4748       ldr       r0, [pc, #0x11c] ; literal[080051cc]=20001126
080050ae: 0078       ldrb      r0, [r0]
080050b0: 40f00800   orr       r0, r0, #8
080050b4: 4549       ldr       r1, [pc, #0x114] ; literal[080051cc]=20001126
080050b6: 0870       strb      r0, [r1]
080050b8: 4349       ldr       r1, [pc, #0x10c] ; literal[080051c8]=200010ec
080050ba: 0968       ldr       r1, [r1]
080050bc: c97c       ldrb      r1, [r1, #0x13]
080050be: 01f00f00   and       r0, r1, #0xf
080050c2: 00f0a9f8   bl        #0x8005218
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
0800518e: 1348       ldr       r0, [pc, #0x4c] ; literal[080051dc]=200039ec
08005190: 90f8ed00   ldrb.w    r0, [r0, #0xed]
08005194: 88b1       cbz       r0, #0x80051ba
08005196: 1148       ldr       r0, [pc, #0x44] ; literal[080051dc]=200039ec
08005198: 90f82800   ldrb.w    r0, [r0, #0x28]
0800519c: 68b1       cbz       r0, #0x80051ba
0800519e: 0f48       ldr       r0, [pc, #0x3c] ; literal[080051dc]=200039ec
080051a0: 90f82800   ldrb.w    r0, [r0, #0x28]
080051a4: 401e       subs      r0, r0, #1
080051a6: 0e49       ldr       r1, [pc, #0x38] ; literal[080051e0]=2000110e
080051a8: 0978       ldrb      r1, [r1]
080051aa: 8842       cmp       r0, r1
080051ac: 05d1       bne       #0x80051ba
080051ae: 0b48       ldr       r0, [pc, #0x2c] ; literal[080051dc]=200039ec
080051b0: 90f8f500   ldrb.w    r0, [r0, #0xf5]
080051b4: 0128       cmp       r0, #1
080051b6: 00d1       bne       #0x80051ba
080051b8: 10bd       pop       {r4, pc}
080051ba: 0420       movs      r0, #4
080051bc: 1af058fd   bl        #0x801fc70
080051c0: 00bf       nop       
080051c2: f9e7       b         #0x80051b8
080051c4: 2511       asrs      r5, r4, #4
080051c6: 0020       movs      r0, #0
080051c8: ec10       asrs      r4, r5, #3
080051ca: 0020       movs      r0, #0
080051cc: 2611       asrs      r6, r4, #4
080051ce: 0020       movs      r0, #0
080051d0: 4430       adds      r0, #0x44
080051d2: 0020       movs      r0, #0
080051d4: ec35       adds      r5, #0xec
080051d6: 0020       movs      r0, #0
080051d8: 1008       lsrs      r0, r2, #0x20
080051da: 0020       movs      r0, #0
080051dc: ec39       subs      r1, #0xec
080051de: 0020       movs      r0, #0
080051e0: 0e11       asrs      r6, r1, #4
080051e2: 0020       movs      r0, #0

08005768: 10b5       push      {r4, lr}
0800576a: 0446       mov       r4, r0
0800576c: 0120       movs      r0, #1
0800576e: 18f057ff   bl        #0x801e620
08005772: 1349       ldr       r1, [pc, #0x4c] ; literal[080057c0]=20000800
08005774: 0880       strh      r0, [r1]
08005776: 0846       mov       r0, r1
08005778: 0088       ldrh      r0, [r0]
0800577a: 0828       cmp       r0, #8
0800577c: 19d2       bhs       #0x80057b2
0800577e: dfe800f0   tbb       [pc, r0]
08005782: 1218       adds      r2, r2, r0
08005784: 0405       lsls      r4, r0, #0x14
08005786: 1318       adds      r3, r2, r0
08005788: 0607       lsls      r6, r0, #0x1c
0800578a: 00bf       nop       
0800578c: 00bf       nop       
0800578e: 00bf       nop       
08005790: 04f00f00   and       r0, r4, #0xf
08005794: 0328       cmp       r0, #3
08005796: 01dd       ble       #0x800579c
08005798: e01e       subs      r0, r4, #3
0800579a: 84b2       uxth      r4, r0
0800579c: 2146       mov       r1, r4
0800579e: 4820       movs      r0, #0x48
080057a0: 18f05eff   bl        #0x801e660
080057a4: 0ae0       b         #0x80057bc
080057a6: 00bf       nop       
080057a8: 2146       mov       r1, r4
080057aa: 4820       movs      r0, #0x48
080057ac: 18f058ff   bl        #0x801e660
080057b0: 04e0       b         #0x80057bc
080057b2: 2146       mov       r1, r4
080057b4: 4820       movs      r0, #0x48
080057b6: 18f053ff   bl        #0x801e660
080057ba: 00bf       nop       
080057bc: 00bf       nop       
080057be: 10bd       pop       {r4, pc}
080057c0: 0008       lsrs      r0, r0, #0x20
080057c2: 0020       movs      r0, #0
