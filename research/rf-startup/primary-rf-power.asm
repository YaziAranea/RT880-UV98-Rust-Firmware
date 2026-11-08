08004f90: 67fb10bd   smmlsr    sp, r7, r0, fp
08004f94: ec35       adds      r5, #0xec
08004f96: 0020       movs      r0, #0
08004f98: 7b63       str       r3, [r7, #0x34]
08004f9a: 0208       lsrs      r2, r0, #0x20
08004f9c: 0008       lsrs      r0, r0, #0x20
08004f9e: 0020       movs      r0, #0
08004fa0: 10b5       push      {r4, lr}
08004fa2: 41f60f71   movw      r1, #0x1f0f
08004fa6: 3720       movs      r0, #0x37
08004fa8: 19f05afb   bl        #0x801e660
08004fac: 0a20       movs      r0, #0xa
08004fae: 03f0e7f9   bl        #0x8008380
08004fb2: 4ff40071   mov.w     r1, #0x200
08004fb6: 3020       movs      r0, #0x30
08004fb8: 19f052fb   bl        #0x801e660
08004fbc: 4bf6f171   movw      r1, #0xbff1
08004fc0: 3020       movs      r0, #0x30
08004fc2: 19f04dfb   bl        #0x801e660
08004fc6: 10bd       pop       {r4, pc}
08004fc8: 10b5       push      {r4, lr}
08004fca: 3320       movs      r0, #0x33
08004fcc: 19f028fb   bl        #0x801e620
08004fd0: 0649       ldr       r1, [pc, #0x18] ; literal[08004fec]=20000800
08004fd2: 0880       strh      r0, [r1]
08004fd4: 0846       mov       r0, r1
08004fd6: 0078       ldrb      r0, [r0]
08004fd8: 00f0cc00   and       r0, r0, #0xcc
08004fdc: 0870       strb      r0, [r1]
08004fde: 0846       mov       r0, r1
08004fe0: 0188       ldrh      r1, [r0]
08004fe2: 3320       movs      r0, #0x33
08004fe4: 19f03cfb   bl        #0x801e660
08004fe8: 10bd       pop       {r4, pc}
08004fea: 0000       movs      r0, r0
08004fec: 0008       lsrs      r0, r0, #0x20
08004fee: 0020       movs      r0, #0
08004ff0: 10b5       push      {r4, lr}
08004ff2: 3320       movs      r0, #0x33
08004ff4: 19f014fb   bl        #0x801e620
08004ff8: 2449       ldr       r1, [pc, #0x90] ; literal[0800508c]=20000800
08004ffa: 0880       strh      r0, [r1]
08004ffc: 0846       mov       r0, r1
08004ffe: 0078       ldrb      r0, [r0]
08005000: 00f0cc00   and       r0, r0, #0xcc
08005004: 0870       strb      r0, [r1]
08005006: 2248       ldr       r0, [pc, #0x88] ; literal[08005090]=20000839
08005008: 0078       ldrb      r0, [r0]
0800500a: 0628       cmp       r0, #6
0800500c: 26d2       bhs       #0x800505c
0800500e: dfe800f0   tbb       [pc, r0]
08005012: 0304       lsls      r3, r0, #0x10
08005014: 050d       lsrs      r5, r0, #0x14
08005016: 151d       adds      r5, r2, #4
08005018: 20e0       b         #0x800505c
0800501a: 00bf       nop       
0800501c: 1b48       ldr       r0, [pc, #0x6c] ; literal[0800508c]=20000800
0800501e: 0078       ldrb      r0, [r0]
08005020: 20f00100   bic       r0, r0, #1
08005024: 401c       adds      r0, r0, #1
08005026: 1949       ldr       r1, [pc, #0x64] ; literal[0800508c]=20000800
08005028: 0870       strb      r0, [r1]
0800502a: 17e0       b         #0x800505c
0800502c: 1748       ldr       r0, [pc, #0x5c] ; literal[0800508c]=20000800
0800502e: 0078       ldrb      r0, [r0]
08005030: 20f01000   bic       r0, r0, #0x10
08005034: 1030       adds      r0, #0x10
08005036: 1549       ldr       r1, [pc, #0x54] ; literal[0800508c]=20000800
08005038: 0870       strb      r0, [r1]
0800503a: 0fe0       b         #0x800505c
0800503c: 1348       ldr       r0, [pc, #0x4c] ; literal[0800508c]=20000800
0800503e: 0078       ldrb      r0, [r0]
08005040: 20f02000   bic       r0, r0, #0x20
08005044: 2030       adds      r0, #0x20
08005046: 1149       ldr       r1, [pc, #0x44] ; literal[0800508c]=20000800
08005048: 0870       strb      r0, [r1]
0800504a: 07e0       b         #0x800505c
0800504c: 0f48       ldr       r0, [pc, #0x3c] ; literal[0800508c]=20000800
0800504e: 0078       ldrb      r0, [r0]
08005050: 20f00200   bic       r0, r0, #2
08005054: 801c       adds      r0, r0, #2
08005056: 0d49       ldr       r1, [pc, #0x34] ; literal[0800508c]=20000800
08005058: 0870       strb      r0, [r1]
0800505a: 00bf       nop       
0800505c: 00bf       nop       
0800505e: 0b48       ldr       r0, [pc, #0x2c] ; literal[0800508c]=20000800
08005060: 0188       ldrh      r1, [r0]
08005062: 3320       movs      r0, #0x33
08005064: 19f0fcfa   bl        #0x801e660
08005068: 0948       ldr       r0, [pc, #0x24] ; literal[08005090]=20000839
0800506a: 0078       ldrb      r0, [r0]
0800506c: 0328       cmp       r0, #3
0800506e: 03d0       beq       #0x8005078
08005070: 0748       ldr       r0, [pc, #0x1c] ; literal[08005090]=20000839
08005072: 0078       ldrb      r0, [r0]
08005074: 0428       cmp       r0, #4
08005076: 03d1       bne       #0x8005080
08005078: 0120       movs      r0, #1
0800507a: 0649       ldr       r1, [pc, #0x18] ; literal[08005094]=40021428
0800507c: 0860       str       r0, [r1]
0800507e: 03e0       b         #0x8005088
08005080: 0120       movs      r0, #1
08005082: 0449       ldr       r1, [pc, #0x10] ; literal[08005094]=40021428
08005084: 1039       subs      r1, #0x10
08005086: 0860       str       r0, [r1]
08005088: 10bd       pop       {r4, pc}
0800508a: 0000       movs      r0, r0
0800508c: 0008       lsrs      r0, r0, #0x20
0800508e: 0020       movs      r0, #0
08005090: 3908       lsrs      r1, r7, #0x20
08005092: 0020       movs      r0, #0
08005094: 2814       asrs      r0, r5, #0x10
08005096: 0240       ands      r2, r0
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
080051e4: 10b5       push      {r4, lr}
080051e6: 0948       ldr       r0, [pc, #0x24] ; literal[0800520c]=200035ec
080051e8: 90f8a400   ldrb.w    r0, [r0, #0xa4]
080051ec: 08b9       cbnz      r0, #0x80051f2
080051ee: 0120       movs      r0, #1
080051f0: 10bd       pop       {r4, pc}
080051f2: 0c20       movs      r0, #0xc
080051f4: 19f014fa   bl        #0x801e620
080051f8: 0549       ldr       r1, [pc, #0x14] ; literal[08005210]=20000800
080051fa: 0880       strh      r0, [r1]
080051fc: 0846       mov       r0, r1
080051fe: 0078       ldrb      r0, [r0]
08005200: c0f34000   ubfx      r0, r0, #1, #1
08005204: 0349       ldr       r1, [pc, #0xc] ; literal[08005214]=20001120
08005206: 0978       ldrb      r1, [r1]
08005208: 0843       orrs      r0, r1
0800520a: f1e7       b         #0x80051f0
0800520c: ec35       adds      r5, #0xec
0800520e: 0020       movs      r0, #0
08005210: 0008       lsrs      r0, r0, #0x20
08005212: 0020       movs      r0, #0
08005214: 2011       asrs      r0, r4, #4
08005216: 0020       movs      r0, #0
08005218: 10b5       push      {r4, lr}
0800521a: 0446       mov       r4, r0
0800521c: 1448       ldr       r0, [pc, #0x50] ; literal[08005270]=20000800
0800521e: 0480       strh      r4, [r0]
08005220: 0088       ldrh      r0, [r0]
08005222: 00ebc010   add.w     r0, r0, r0, lsl #7
08005226: 46f6dc01   movw      r1, #0x68dc
0800522a: 01ebc000   add.w     r0, r1, r0, lsl #3
0800522e: 1049       ldr       r1, [pc, #0x40] ; literal[08005270]=20000800
08005230: 0880       strh      r0, [r1]
08005232: 0846       mov       r0, r1
08005234: 0188       ldrh      r1, [r0]
08005236: 7120       movs      r0, #0x71
08005238: 19f012fa   bl        #0x801e660
0800523c: 3120       movs      r0, #0x31
0800523e: 19f0eff9   bl        #0x801e620
08005242: 0b49       ldr       r1, [pc, #0x2c] ; literal[08005270]=20000800
08005244: 0880       strh      r0, [r1]
08005246: 2cb9       cbnz      r4, #0x8005254
08005248: 0846       mov       r0, r1
0800524a: 0078       ldrb      r0, [r0]
0800524c: 20f00200   bic       r0, r0, #2
08005250: 0870       strb      r0, [r1]
08005252: 06e0       b         #0x8005262
08005254: 0648       ldr       r0, [pc, #0x18] ; literal[08005270]=20000800
08005256: 0078       ldrb      r0, [r0]
08005258: 20f00200   bic       r0, r0, #2
0800525c: 801c       adds      r0, r0, #2
0800525e: 0449       ldr       r1, [pc, #0x10] ; literal[08005270]=20000800
08005260: 0870       strb      r0, [r1]
08005262: 0348       ldr       r0, [pc, #0xc] ; literal[08005270]=20000800
08005264: 0188       ldrh      r1, [r0]
08005266: 3120       movs      r0, #0x31
08005268: 19f0faf9   bl        #0x801e660
0800526c: 10bd       pop       {r4, pc}
0800526e: 0000       movs      r0, r0
08005270: 0008       lsrs      r0, r0, #0x20
08005272: 0020       movs      r0, #0
08005274: 10b5       push      {r4, lr}
08005276: 0446       mov       r4, r0
08005278: 7320       movs      r0, #0x73
0800527a: 19f0d1f9   bl        #0x801e620
0800527e: 0b49       ldr       r1, [pc, #0x2c] ; literal[080052ac]=20000800
08005280: 0880       strh      r0, [r1]
08005282: 2cb1       cbz       r4, #0x8005290
08005284: 0846       mov       r0, r1
08005286: 0078       ldrb      r0, [r0]
08005288: 20f01000   bic       r0, r0, #0x10
0800528c: 0870       strb      r0, [r1]
0800528e: 06e0       b         #0x800529e
08005290: 0648       ldr       r0, [pc, #0x18] ; literal[080052ac]=20000800
08005292: 0078       ldrb      r0, [r0]
08005294: 20f01000   bic       r0, r0, #0x10
08005298: 1030       adds      r0, #0x10
0800529a: 0449       ldr       r1, [pc, #0x10] ; literal[080052ac]=20000800
0800529c: 0870       strb      r0, [r1]
0800529e: 0348       ldr       r0, [pc, #0xc] ; literal[080052ac]=20000800
080052a0: 0188       ldrh      r1, [r0]
080052a2: 7320       movs      r0, #0x73
080052a4: 19f0dcf9   bl        #0x801e660
080052a8: 10bd       pop       {r4, pc}
080052aa: 0000       movs      r0, r0
080052ac: 0008       lsrs      r0, r0, #0x20
080052ae: 0020       movs      r0, #0
080052b0: 10b5       push      {r4, lr}
080052b2: fff789fe   bl        #0x8004fc8
080052b6: 0021       movs      r1, #0
080052b8: 3020       movs      r0, #0x30
080052ba: 19f0d1f9   bl        #0x801e660
080052be: 4ff4e851   mov.w     r1, #0x1d00
080052c2: 3720       movs      r0, #0x37
080052c4: 19f0ccf9   bl        #0x801e660
080052c8: 0120       movs      r0, #1
080052ca: 0149       ldr       r1, [pc, #4] ; literal[080052d0]=20001124
080052cc: 0870       strb      r0, [r1]
080052ce: 10bd       pop       {r4, pc}
080052d0: 2411       asrs      r4, r4, #4
080052d2: 0020       movs      r0, #0
080052d4: 10b5       push      {r4, lr}
080052d6: 0446       mov       r4, r0
080052d8: 41f60f51   movw      r1, #0x1d0f
080052dc: 3720       movs      r0, #0x37
080052de: 19f0bff9   bl        #0x801e660
080052e2: 0a20       movs      r0, #0xa
080052e4: 03f04cf8   bl        #0x8008380
080052e8: 40f28f21   movw      r1, #0x28f
080052ec: 5220       movs      r0, #0x52
080052ee: 19f0b7f9   bl        #0x801e660
080052f2: 4ff40071   mov.w     r1, #0x200
080052f6: 3020       movs      r0, #0x30
080052f8: 19f0b2f9   bl        #0x801e660
080052fc: 2cb9       cbnz      r4, #0x800530a
080052fe: 4cf2fe11   movw      r1, #0xc1fe
08005302: 3020       movs      r0, #0x30
08005304: 19f0acf9   bl        #0x801e660
08005308: 04e0       b         #0x8005314
0800530a: 4cf2fa31   movw      r1, #0xc3fa
0800530e: 3020       movs      r0, #0x30
08005310: 19f0a6f9   bl        #0x801e660
08005314: 10bd       pop       {r4, pc}
08005316: 0000       movs      r0, r0
08005318: 10b5       push      {r4, lr}
0800531a: 4ff40040   mov.w     r0, #0x8000
0800531e: 0a49       ldr       r1, [pc, #0x28] ; literal[08005348]=40020828
08005320: 0860       str       r0, [r1]
08005322: 0011       asrs      r0, r0, #4
08005324: 0949       ldr       r1, [pc, #0x24] ; literal[0800534c]=40020428
08005326: 0860       str       r0, [r1]
