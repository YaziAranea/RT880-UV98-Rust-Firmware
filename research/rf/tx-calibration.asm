0800549e: 4b48       ldr       r0, [pc, #0x12c] ; literal[080055cc]=200010ec
080054a0: 0068       ldr       r0, [r0]
080054a2: c07c       ldrb      r0, [r0, #0x13]
080054a4: 00f00f00   and       r0, r0, #0xf
080054a8: 28b1       cbz       r0, #0x80054b6
080054aa: 0422       movs      r2, #4
080054ac: 0121       movs      r1, #1
080054ae: 0846       mov       r0, r1
080054b0: fef70efa   bl        #0x80038d0
080054b4: 05e0       b         #0x80054c2
080054b6: 4b48       ldr       r0, [pc, #0x12c] ; literal[080055e4]=20003044
080054b8: 427a       ldrb      r2, [r0, #9]
080054ba: 0121       movs      r1, #1
080054bc: 0846       mov       r0, r1
080054be: fef707fa   bl        #0x80038d0
080054c2: 4949       ldr       r1, [pc, #0x124] ; literal[080055e8]=200035ec
080054c4: 91f8ad00   ldrb.w    r0, [r1, #0xad]
080054c8: fff7e2fc   bl        #0x8004e90
080054cc: 3f48       ldr       r0, [pc, #0xfc] ; literal[080055cc]=200010ec
080054ce: 0068       ldr       r0, [r0]
080054d0: c078       ldrb      r0, [r0, #3]
080054d2: c0b9       cbnz      r0, #0x8005506
080054d4: 3d48       ldr       r0, [pc, #0xf4] ; literal[080055cc]=200010ec
080054d6: 0068       ldr       r0, [r0]
080054d8: c07c       ldrb      r0, [r0, #0x13]
080054da: 00f00f00   and       r0, r0, #0xf
080054de: 60b1       cbz       r0, #0x80054fa
080054e0: 4248       ldr       r0, [pc, #0x108] ; literal[080055ec]=20003064
080054e2: b0f80300   ldrh.w    r0, [r0, #3]
080054e6: c828       cmp       r0, #0xc8
080054e8: 07dd       ble       #0x80054fa
080054ea: 4049       ldr       r1, [pc, #0x100] ; literal[080055ec]=20003064
080054ec: b1f80310   ldrh.w    r1, [r1, #3]
080054f0: c839       subs      r1, #0xc8
080054f2: 88b2       uxth      r0, r1
080054f4: fff74ffa   bl        #0x8004996
080054f8: 1ae0       b         #0x8005530
080054fa: 3c49       ldr       r1, [pc, #0xf0] ; literal[080055ec]=20003064
080054fc: b1f80300   ldrh.w    r0, [r1, #3]
08005500: fff749fa   bl        #0x8004996
08005504: 14e0       b         #0x8005530
08005506: 3148       ldr       r0, [pc, #0xc4] ; literal[080055cc]=200010ec
08005508: 0068       ldr       r0, [r0]
0800550a: c07c       ldrb      r0, [r0, #0x13]
0800550c: 00f00f00   and       r0, r0, #0xf
08005510: 50b1       cbz       r0, #0x8005528
08005512: 3648       ldr       r0, [pc, #0xd8] ; literal[080055ec]=20003064
08005514: 4089       ldrh      r0, [r0, #0xa]
08005516: c828       cmp       r0, #0xc8
08005518: 06dd       ble       #0x8005528
0800551a: 3449       ldr       r1, [pc, #0xd0] ; literal[080055ec]=20003064
0800551c: 4989       ldrh      r1, [r1, #0xa]
0800551e: c839       subs      r1, #0xc8
08005520: 88b2       uxth      r0, r1
08005522: fff738fa   bl        #0x8004996
08005526: 03e0       b         #0x8005530
08005528: 3049       ldr       r1, [pc, #0xc0] ; literal[080055ec]=20003064
0800552a: 4889       ldrh      r0, [r1, #0xa]
0800552c: fff733fa   bl        #0x8004996
08005530: 2449       ldr       r1, [pc, #0x90] ; literal[080055c4]=20001121
08005532: 0978       ldrb      r1, [r1]
08005534: 11b9       cbnz      r1, #0x800553c
08005536: 2449       ldr       r1, [pc, #0x90] ; literal[080055c8]=2000111f
08005538: 0978       ldrb      r1, [r1]
0800553a: 09b1       cbz       r1, #0x8005540
0800553c: 0121       movs      r1, #1
0800553e: 00e0       b         #0x8005542
08005540: 0021       movs      r1, #0
08005542: 0846       mov       r0, r1
08005544: fff7c6fe   bl        #0x80052d4
08005548: fff704ff   bl        #0x8005354
0800554c: 2848       ldr       r0, [pc, #0xa0] ; literal[080055f0]=200011fa
0800554e: 0078       ldrb      r0, [r0]
08005550: f8b9       cbnz      r0, #0x8005592
08005552: 1e48       ldr       r0, [pc, #0x78] ; literal[080055cc]=200010ec
08005554: 0068       ldr       r0, [r0]
08005556: 007c       ldrb      r0, [r0, #0x10]
08005558: 0228       cmp       r0, #2
0800555a: 04d1       bne       #0x8005566
0800555c: 2548       ldr       r0, [pc, #0x94] ; literal[080055f4]=2000081b
0800555e: 0078       ldrb      r0, [r0]
08005560: fff7d4fc   bl        #0x8004f0c
08005564: 2ae0       b         #0x80055bc
08005566: 1948       ldr       r0, [pc, #0x64] ; literal[080055cc]=200010ec
08005568: 0068       ldr       r0, [r0]
0800556a: 007c       ldrb      r0, [r0, #0x10]
0800556c: 0128       cmp       r0, #1
0800556e: 0bd1       bne       #0x8005588
08005570: 2049       ldr       r1, [pc, #0x80] ; literal[080055f4]=2000081b
08005572: 0978       ldrb      r1, [r1]
08005574: 204a       ldr       r2, [pc, #0x80] ; literal[080055f8]=2000081c
08005576: 1278       ldrb      r2, [r2]
08005578: 8c18       adds      r4, r1, r2
0800557a: 04ebd471   add.w     r1, r4, r4, lsr #31
0800557e: c1f34700   ubfx      r0, r1, #1, #8
08005582: fff7c3fc   bl        #0x8004f0c
08005586: 19e0       b         #0x80055bc
08005588: 1b48       ldr       r0, [pc, #0x6c] ; literal[080055f8]=2000081c
0800558a: 0078       ldrb      r0, [r0]
0800558c: fff7befc   bl        #0x8004f0c
08005590: 14e0       b         #0x80055bc
08005592: 7f20       movs      r0, #0x7f
08005594: 1949       ldr       r1, [pc, #0x64] ; literal[080055fc]=20000800
08005596: 0880       strh      r0, [r1]
08005598: 1348       ldr       r0, [pc, #0x4c] ; literal[080055e8]=200035ec
0800559a: b730       adds      r0, #0xb7
0800559c: 1849       ldr       r1, [pc, #0x60] ; literal[08005600]=2000081a
0800559e: 0978       ldrb      r1, [r1]
080055a0: 405c       ldrb      r0, [r0, r1]
080055a2: 1649       ldr       r1, [pc, #0x58] ; literal[080055fc]=20000800
080055a4: 4870       strb      r0, [r1, #1]
080055a6: 0846       mov       r0, r1
080055a8: 0078       ldrb      r0, [r0]
080055aa: 20f08000   bic       r0, r0, #0x80
080055ae: 8030       adds      r0, #0x80
080055b0: 0870       strb      r0, [r1]
080055b2: 0846       mov       r0, r1
080055b4: 0188       ldrh      r1, [r0]
080055b6: 3620       movs      r0, #0x36
080055b8: 19f052f8   bl        #0x801e660
080055bc: 0320       movs      r0, #3
080055be: 1149       ldr       r1, [pc, #0x44] ; literal[08005604]=20001125
080055c0: 0870       strb      r0, [r1]
080055c2: 10bd       pop       {r4, pc}
080055c4: 2111       asrs      r1, r4, #4
080055c6: 0020       movs      r0, #0
080055c8: 1f11       asrs      r7, r3, #4
080055ca: 0020       movs      r0, #0
080055cc: ec10       asrs      r4, r5, #3
080055ce: 0020       movs      r0, #0
080055d0: 3808       lsrs      r0, r7, #0x20
080055d2: 0020       movs      r0, #0
080055d4: 2814       asrs      r0, r5, #0x10
080055d6: 0240       ands      r2, r0
080055d8: 0808       lsrs      r0, r1, #0x20
080055da: 0020       movs      r0, #0
080055dc: 0a08       lsrs      r2, r1, #0x20
080055de: 0020       movs      r0, #0
080055e0: 0c08       lsrs      r4, r1, #0x20
080055e2: 0020       movs      r0, #0
080055e4: 4430       adds      r0, #0x44
080055e6: 0020       movs      r0, #0
080055e8: ec35       adds      r5, #0xec
080055ea: 0020       movs      r0, #0
080055ec: 6430       adds      r0, #0x64
080055ee: 0020       movs      r0, #0
080055f0: fa11       asrs      r2, r7, #7
080055f2: 0020       movs      r0, #0
080055f4: 1b08       lsrs      r3, r3, #0x20
080055f6: 0020       movs      r0, #0
080055f8: 1c08       lsrs      r4, r3, #0x20
080055fa: 0020       movs      r0, #0
080055fc: 0008       lsrs      r0, r0, #0x20
080055fe: 0020       movs      r0, #0
08005600: 1a08       lsrs      r2, r3, #0x20
08005602: 0020       movs      r0, #0
08005604: 2511       asrs      r5, r4, #4
08005606: 0020       movs      r0, #0
