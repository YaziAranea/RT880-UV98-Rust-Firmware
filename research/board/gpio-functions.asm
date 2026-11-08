08025450: 4ff6ff71   movw      r1, #0xffff
08025454: 0160       str       r1, [r0]
08025456: 0021       movs      r1, #0
08025458: 8171       strb      r1, [r0, #6]
0802545a: 0171       strb      r1, [r0, #4]
0802545c: 4171       strb      r1, [r0, #5]
0802545e: 0121       movs      r1, #1
08025460: c171       strb      r1, [r0, #7]
08025462: 7047       bx        lr
08025464: 70b5       push      {r4, r5, r6, lr}
08025466: 0a46       mov       r2, r1
08025468: 0021       movs      r1, #0
0802546a: 1488       ldrh      r4, [r2]
0802546c: a3b2       uxth      r3, r4
0802546e: 33e0       b         #0x80254d8
08025470: 03f00104   and       r4, r3, #1
08025474: 6cb3       cbz       r4, #0x80254d2
08025476: 0468       ldr       r4, [r0]
08025478: 4e00       lsls      r6, r1, #1
0802547a: 0325       movs      r5, #3
0802547c: b540       lsls      r5, r6
0802547e: ac43       bics      r4, r5
08025480: 0460       str       r4, [r0]
08025482: 9479       ldrb      r4, [r2, #6]
08025484: 4d00       lsls      r5, r1, #1
08025486: ac40       lsls      r4, r5
08025488: 0568       ldr       r5, [r0]
0802548a: 2c43       orrs      r4, r5
0802548c: 0460       str       r4, [r0]
0802548e: 4468       ldr       r4, [r0, #4]
08025490: 0125       movs      r5, #1
08025492: 8d40       lsls      r5, r1
08025494: ac43       bics      r4, r5
08025496: 4460       str       r4, [r0, #4]
08025498: 4468       ldr       r4, [r0, #4]
0802549a: 1579       ldrb      r5, [r2, #4]
0802549c: 8d40       lsls      r5, r1
0802549e: 2c43       orrs      r4, r5
080254a0: 4460       str       r4, [r0, #4]
080254a2: 8468       ldr       r4, [r0, #8]
080254a4: 4e00       lsls      r6, r1, #1
080254a6: 0325       movs      r5, #3
080254a8: b540       lsls      r5, r6
080254aa: ac43       bics      r4, r5
080254ac: 8460       str       r4, [r0, #8]
080254ae: d479       ldrb      r4, [r2, #7]
080254b0: 4d00       lsls      r5, r1, #1
080254b2: ac40       lsls      r4, r5
080254b4: 8568       ldr       r5, [r0, #8]
080254b6: 2c43       orrs      r4, r5
080254b8: 8460       str       r4, [r0, #8]
080254ba: c468       ldr       r4, [r0, #0xc]
080254bc: 4e00       lsls      r6, r1, #1
080254be: 0325       movs      r5, #3
080254c0: b540       lsls      r5, r6
080254c2: ac43       bics      r4, r5
080254c4: c460       str       r4, [r0, #0xc]
080254c6: 5479       ldrb      r4, [r2, #5]
080254c8: 4d00       lsls      r5, r1, #1
080254ca: ac40       lsls      r4, r5
080254cc: c568       ldr       r5, [r0, #0xc]
080254ce: 2c43       orrs      r4, r5
080254d0: c460       str       r4, [r0, #0xc]
080254d2: 5b10       asrs      r3, r3, #1
080254d4: 4c1c       adds      r4, r1, #1
080254d6: a1b2       uxth      r1, r4
080254d8: 002b       cmp       r3, #0
080254da: c9dc       bgt       #0x8025470
080254dc: 70bd       pop       {r4, r5, r6, pc}
080254de: 0246       mov       r2, r0
080254e0: 0020       movs      r0, #0
080254e2: 1369       ldr       r3, [r2, #0x10]
080254e4: 0b40       ands      r3, r1
080254e6: 8b42       cmp       r3, r1
080254e8: 00d0       beq       #0x80254ec
080254ea: 00e0       b         #0x80254ee
080254ec: 0120       movs      r0, #1
080254ee: 7047       bx        lr
080254f0: f0b5       push      {r4, r5, r6, r7, lr}
080254f2: 0023       movs      r3, #0
080254f4: 0024       movs      r4, #0
080254f6: 4d07       lsls      r5, r1, #0x1d
080254f8: ed0e       lsrs      r5, r5, #0x1b
080254fa: 02fa05f3   lsl.w     r3, r2, r5
080254fe: cd10       asrs      r5, r1, #3
08025500: 5db1       cbz       r5, #0x802551a
08025502: 456a       ldr       r5, [r0, #0x24]
08025504: 4e07       lsls      r6, r1, #0x1d
08025506: f70e       lsrs      r7, r6, #0x1b
08025508: 0f26       movs      r6, #0xf
0802550a: be40       lsls      r6, r7
0802550c: b543       bics      r5, r6
0802550e: 4562       str       r5, [r0, #0x24]
08025510: 456a       ldr       r5, [r0, #0x24]
08025512: 45ea0304   orr.w     r4, r5, r3
08025516: 4462       str       r4, [r0, #0x24]
08025518: 0ae0       b         #0x8025530
0802551a: 056a       ldr       r5, [r0, #0x20]
0802551c: 4e07       lsls      r6, r1, #0x1d
0802551e: f70e       lsrs      r7, r6, #0x1b
08025520: 0f26       movs      r6, #0xf
08025522: be40       lsls      r6, r7
08025524: b543       bics      r5, r6
08025526: 0562       str       r5, [r0, #0x20]
08025528: 056a       ldr       r5, [r0, #0x20]
0802552a: 45ea0304   orr.w     r4, r5, r3
0802552e: 0462       str       r4, [r0, #0x20]
