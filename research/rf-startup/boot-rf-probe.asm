080033c0: c07b       ldrb      r0, [r0, #0xf]
080033c2: 64f30710   bfi       r0, r4, #4, #4
080033c6: c873       strb      r0, [r1, #0xf]
080033c8: 0846       mov       r0, r1
080033ca: 8088       ldrh      r0, [r0, #4]
080033cc: c0f30b06   ubfx      r6, r0, #0, #0xc
080033d0: 0846       mov       r0, r1
080033d2: c089       ldrh      r0, [r0, #0xe]
080033d4: 8988       ldrh      r1, [r1, #4]
080033d6: 60f30b01   bfi       r1, r0, #0, #0xc
080033da: 0e48       ldr       r0, [pc, #0x38] ; literal[08003414]=20003b7c
080033dc: 8180       strh      r1, [r0, #4]
080033de: c089       ldrh      r0, [r0, #0xe]
080033e0: 66f30b00   bfi       r0, r6, #0, #0xc
080033e4: 0b49       ldr       r1, [pc, #0x2c] ; literal[08003414]=20003b7c
080033e6: c881       strh      r0, [r1, #0xe]
080033e8: 0a48       ldr       r0, [pc, #0x28] ; literal[08003414]=20003b7c
080033ea: 0c49       ldr       r1, [pc, #0x30] ; literal[0800341c]=200010ec
080033ec: 0860       str       r0, [r1]
080033ee: 0968       ldr       r1, [r1]
080033f0: 897c       ldrb      r1, [r1, #0x12]
080033f2: 01f01f00   and       r0, r1, #0x1f
080033f6: 10f00bfd   bl        #0x8013e10
080033fa: 0949       ldr       r1, [pc, #0x24] ; literal[08003420]=20001174
080033fc: 0860       str       r0, [r1]
080033fe: 0748       ldr       r0, [pc, #0x1c] ; literal[0800341c]=200010ec
08003400: 0068       ldr       r0, [r0]
08003402: 1cf04dfa   bl        #0x801f8a0
08003406: 0020       movs      r0, #0
08003408: 0649       ldr       r1, [pc, #0x18] ; literal[08003424]=2000112b
0800340a: 0870       strb      r0, [r1]
0800340c: bde8f081   pop.w     {r4, r5, r6, r7, r8, pc}
08003410: ec3a       subs      r2, #0xec
08003412: 0020       movs      r0, #0
08003414: 7c3b       subs      r3, #0x7c
08003416: 0020       movs      r0, #0
08003418: ec35       adds      r5, #0xec
0800341a: 0020       movs      r0, #0
0800341c: ec10       asrs      r4, r5, #3
0800341e: 0020       movs      r0, #0
08003420: 7411       asrs      r4, r6, #5
08003422: 0020       movs      r0, #0
08003424: 2b11       asrs      r3, r5, #4
08003426: 0020       movs      r0, #0
08003428: 2de9f047   push.w    {r4, r5, r6, r7, r8, sb, sl, lr}
0800342c: 0346       mov       r3, r0
0800342e: 0d46       mov       r5, r1
08003430: 1646       mov       r6, r2
08003432: 0021       movs      r1, #0
08003434: 0024       movs      r4, #0
08003436: 0022       movs      r2, #0
08003438: 0027       movs      r7, #0
0800343a: 8c46       mov       ip, r1
0800343c: 8846       mov       r8, r1
0800343e: 8946       mov       sb, r1
08003440: 142d       cmp       r5, #0x14
08003442: 02da       bge       #0x800344a
08003444: 0020       movs      r0, #0
08003446: bde8f087   pop.w     {r4, r5, r6, r7, r8, sb, sl, pc}
0800344a: 37e0       b         #0x80034bc
0800344c: 0022       movs      r2, #0
0800344e: 31e0       b         #0x80034b4
08003450: 585c       ldrb      r0, [r3, r1]
08003452: 9040       lsls      r0, r2
08003454: 00f0ff0c   and       ip, r0, #0xff
08003458: 481c       adds      r0, r1, #1
0800345a: 185c       ldrb      r0, [r3, r0]
0800345c: c2f1080a   rsb.w     sl, r2, #8
08003460: 40fa0af0   asr.w     r0, r0, sl
08003464: 6044       add       r0, ip
08003466: 00f0ff0c   and       ip, r0, #0xff
0800346a: 481c       adds      r0, r1, #1
0800346c: 185c       ldrb      r0, [r3, r0]
0800346e: 9040       lsls      r0, r2
08003470: 00f0ff08   and       r8, r0, #0xff
08003474: 881c       adds      r0, r1, #2
08003476: 185c       ldrb      r0, [r3, r0]
08003478: c2f1080a   rsb.w     sl, r2, #8
0800347c: 40fa0af0   asr.w     r0, r0, sl
08003480: 4044       add       r0, r8
08003482: 00f0ff08   and       r8, r0, #0xff
08003486: 881c       adds      r0, r1, #2
08003488: 185c       ldrb      r0, [r3, r0]
0800348a: 9040       lsls      r0, r2
0800348c: 00f0ff09   and       sb, r0, #0xff
08003490: c81c       adds      r0, r1, #3
08003492: 185c       ldrb      r0, [r3, r0]
08003494: c2f1080a   rsb.w     sl, r2, #8
08003498: 40fa0af0   asr.w     r0, r0, sl
0800349c: 4844       add       r0, sb
0800349e: 00f0ff09   and       sb, r0, #0xff
080034a2: b445       cmp       ip, r6
080034a4: 04d1       bne       #0x80034b0
080034a6: b045       cmp       r8, r6
080034a8: 02d1       bne       #0x80034b0
080034aa: b145       cmp       sb, r6
080034ac: 00d1       bne       #0x80034b0
080034ae: 09e0       b         #0x80034c4
080034b0: 501c       adds      r0, r2, #1
080034b2: c2b2       uxtb      r2, r0
080034b4: 082a       cmp       r2, #8
080034b6: cbdb       blt       #0x8003450
080034b8: 481c       adds      r0, r1, #1
080034ba: 81b2       uxth      r1, r0
080034bc: e81e       subs      r0, r5, #3
080034be: 8842       cmp       r0, r1
080034c0: c4dc       bgt       #0x800344c
080034c2: 00bf       nop       
080034c4: a942       cmp       r1, r5
080034c6: 01d1       bne       #0x80034cc
080034c8: 0020       movs      r0, #0
080034ca: bce7       b         #0x8003446
080034cc: 681a       subs      r0, r5, r1
080034ce: 87b2       uxth      r7, r0
080034d0: 0024       movs      r4, #0
080034d2: 11e0       b         #0x80034f8
080034d4: 6018       adds      r0, r4, r1
080034d6: 401c       adds      r0, r0, #1
080034d8: 185c       ldrb      r0, [r3, r0]
080034da: c2f1080a   rsb.w     sl, r2, #8
080034de: 40fa0af0   asr.w     r0, r0, sl
080034e2: 04eb010a   add.w     sl, r4, r1
080034e6: 13f80aa0   ldrb.w    sl, [r3, sl]
080034ea: 0afa02fa   lsl.w     sl, sl, r2
080034ee: 40ea0a00   orr.w     r0, r0, sl
080034f2: 1855       strb      r0, [r3, r4]
080034f4: 601c       adds      r0, r4, #1
080034f6: 84b2       uxth      r4, r0
080034f8: 781e       subs      r0, r7, #1
080034fa: a042       cmp       r0, r4
080034fc: eadc       bgt       #0x80034d4
080034fe: 6018       adds      r0, r4, r1
08003500: 185c       ldrb      r0, [r3, r0]
08003502: 9040       lsls      r0, r2
08003504: 00f0ff0a   and       sl, r0, #0xff
08003508: 781e       subs      r0, r7, #1
0800350a: 03f800a0   strb.w    sl, [r3, r0]
0800350e: 3846       mov       r0, r7
08003510: 99e7       b         #0x8003446
08003512: 0000       movs      r0, r0
08003514: 7cb5       push      {r2, r3, r4, r5, r6, lr}
08003516: 0546       mov       r5, r0
08003518: 0ff0b4fd   bl        #0x8013084
0800351c: 0446       mov       r4, r0
0800351e: 11f077f9   bl        #0x8014810
08003522: 04f0cdfd   bl        #0x80080c0
08003526: 002d       cmp       r5, #0
08003528: 46d1       bne       #0x80035b8
0800352a: 22f023fc   bl        #0x8025d74
0800352e: ad49       ldr       r1, [pc, #0x2b4] ; literal[080037e4]=200011c2
08003530: 0880       strh      r0, [r1]
08003532: 2046       mov       r0, r4
08003534: 10f082fc   bl        #0x8013e3c
08003538: 01f042fc   bl        #0x8004dc0
0800353c: 6420       movs      r0, #0x64
0800353e: 04f01fff   bl        #0x8008380
08003542: 1021       movs      r1, #0x10
08003544: a848       ldr       r0, [pc, #0x2a0] ; literal[080037e8]=40020400
08003546: 21f0caff   bl        #0x80254de
0800354a: 18b1       cbz       r0, #0x8003554
0800354c: 0120       movs      r0, #1
0800354e: a749       ldr       r1, [pc, #0x29c] ; literal[080037ec]=200011fc
08003550: 0870       strb      r0, [r1]
08003552: 02e0       b         #0x800355a
08003554: 0020       movs      r0, #0
08003556: a549       ldr       r1, [pc, #0x294] ; literal[080037ec]=200011fc
08003558: 0870       strb      r0, [r1]
0800355a: 2046       mov       r0, r4
0800355c: 0ef052ff   bl        #0x8012404
08003560: 032c       cmp       r4, #3
08003562: 0ed1       bne       #0x8003582
08003564: a248       ldr       r0, [pc, #0x288] ; literal[080037f0]=200035ec
08003566: 90f8a700   ldrb.w    r0, [r0, #0xa7]
0800356a: 08b9       cbnz      r0, #0x8003570
0800356c: 0120       movs      r0, #1
0800356e: 00e0       b         #0x8003572
08003570: 0020       movs      r0, #0
08003572: 9f49       ldr       r1, [pc, #0x27c] ; literal[080037f0]=200035ec
08003574: 81f8a700   strb.w    r0, [r1, #0xa7]
08003578: 0846       mov       r0, r1
0800357a: 90f8a700   ldrb.w    r0, [r0, #0xa7]
0800357e: 9d49       ldr       r1, [pc, #0x274] ; literal[080037f4]=20000846
08003580: c871       strb      r0, [r1, #7]
08003582: 1cb9       cbnz      r4, #0x800358c
08003584: 0020       movs      r0, #0
08003586: 9a49       ldr       r1, [pc, #0x268] ; literal[080037f0]=200035ec
08003588: 81f86900   strb.w    r0, [r1, #0x69]
0800358c: 052c       cmp       r4, #5
0800358e: 04d1       bne       #0x800359a
08003590: 0120       movs      r0, #1
08003592: 9949       ldr       r1, [pc, #0x264] ; literal[080037f8]=200011f9
08003594: 0870       strb      r0, [r1]
08003596: 9949       ldr       r1, [pc, #0x264] ; literal[080037fc]=200011fa
08003598: 0870       strb      r0, [r1]
0800359a: 9948       ldr       r0, [pc, #0x264] ; literal[08003800]=20003044
0800359c: 4078       ldrb      r0, [r0, #1]
0800359e: 58b9       cbnz      r0, #0x80035b8
080035a0: 9348       ldr       r0, [pc, #0x24c] ; literal[080037f0]=200035ec
080035a2: 90f87400   ldrb.w    r0, [r0, #0x74]
080035a6: 18b9       cbnz      r0, #0x80035b0
080035a8: 0120       movs      r0, #1
080035aa: 9649       ldr       r1, [pc, #0x258] ; literal[08003804]=40021428
080035ac: 0860       str       r0, [r1]
080035ae: 03e0       b         #0x80035b8
080035b0: 0120       movs      r0, #1
080035b2: 9449       ldr       r1, [pc, #0x250] ; literal[08003804]=40021428
080035b4: 1039       subs      r1, #0x10
080035b6: 0860       str       r0, [r1]
080035b8: 00f0bcfb   bl        #0x8003d34
080035bc: 00f080fc   bl        #0x8003ec0
080035c0: 9148       ldr       r0, [pc, #0x244] ; literal[08003808]=200011fb
080035c2: 0178       ldrb      r1, [r0]
080035c4: 0020       movs      r0, #0
080035c6: 1df0affc   bl        #0x8020f28
080035ca: 8f48       ldr       r0, [pc, #0x23c] ; literal[08003808]=200011fb
080035cc: 0178       ldrb      r1, [r0]
080035ce: 0120       movs      r0, #1
080035d0: 1df0aafc   bl        #0x8020f28
080035d4: 8c48       ldr       r0, [pc, #0x230] ; literal[08003808]=200011fb
080035d6: 0178       ldrb      r1, [r0]
080035d8: 0220       movs      r0, #2
080035da: 1df0a5fc   bl        #0x8020f28
080035de: 0020       movs      r0, #0
080035e0: 8949       ldr       r1, [pc, #0x224] ; literal[08003808]=200011fb
080035e2: 0870       strb      r0, [r1]
080035e4: 8249       ldr       r1, [pc, #0x208] ; literal[080037f0]=200035ec
080035e6: 91f87500   ldrb.w    r0, [r1, #0x75]
080035ea: fff79bfe   bl        #0x8003324
080035ee: 8048       ldr       r0, [pc, #0x200] ; literal[080037f0]=200035ec
080035f0: 90f87500   ldrb.w    r0, [r0, #0x75]
080035f4: 8549       ldr       r1, [pc, #0x214] ; literal[0800380c]=2000110e
080035f6: 0870       strb      r0, [r1]
080035f8: 8548       ldr       r0, [pc, #0x214] ; literal[08003810]=200011bc
080035fa: 0078       ldrb      r0, [r0]
080035fc: 0228       cmp       r0, #2
080035fe: 1ad1       bne       #0x8003636
08003600: 1af034fe   bl        #0x801e26c
08003604: 0020       movs      r0, #0
08003606: 40f23f13   movw      r3, #0x13f
0800360a: 0246       mov       r2, r0
0800360c: ef21       movs      r1, #0xef
0800360e: 0090       str       r0, [sp]
08003610: 14f0eefb   bl        #0x8017df0
08003614: 0020       movs      r0, #0
08003616: 40f2bf61   movw      r1, #0x6bf
0800361a: 1023       movs      r3, #0x10
0800361c: 7da2       adr       r2, #0x1f4
0800361e: cde90010   strd      r1, r0, [sp]
08003622: d421       movs      r1, #0xd4
08003624: 1820       movs      r0, #0x18
08003626: 05f0dcfe   bl        #0x80093e2
0800362a: 0120       movs      r0, #1
0800362c: 01f04efa   bl        #0x8004acc
