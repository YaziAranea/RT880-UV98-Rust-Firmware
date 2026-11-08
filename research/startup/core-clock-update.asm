08025f6c: 2de9ff5f   push.w    {r0, r1, r2, r3, r4, r5, r6, r7, r8, sb, sl, fp, ip, lr}
08025f70: 0024       movs      r4, #0
08025f72: a146       mov       sb, r4
08025f74: a246       mov       sl, r4
08025f76: a346       mov       fp, r4
08025f78: 0020       movs      r0, #0
08025f7a: 0390       str       r0, [sp, #0xc]
08025f7c: 0290       str       r0, [sp, #8]
08025f7e: 0190       str       r0, [sp, #4]
08025f80: fff7b6f8   bl        #0x80250f0
08025f84: 0090       str       r0, [sp]
08025f86: 0098       ldr       r0, [sp]
08025f88: 20b1       cbz       r0, #0x8025f94
08025f8a: 0128       cmp       r0, #1
08025f8c: 14d0       beq       #0x8025fb8
08025f8e: 0228       cmp       r0, #2
08025f90: 48d1       bne       #0x8026024
08025f92: 15e0       b         #0x8025fc0
08025f94: 2f48       ldr       r0, [pc, #0xbc] ; literal[08026054]=400238a0
08025f96: 0068       ldr       r0, [r0]
08025f98: c0f38030   ubfx      r0, r0, #0xe, #1
08025f9c: 40b1       cbz       r0, #0x8025fb0
08025f9e: 2d48       ldr       r0, [pc, #0xb4] ; literal[08026054]=400238a0
08025fa0: 0068       ldr       r0, [r0]
08025fa2: c0f30030   ubfx      r0, r0, #0xc, #1
08025fa6: 18b1       cbz       r0, #0x8025fb0
08025fa8: 2b48       ldr       r0, [pc, #0xac] ; literal[08026058]=02dc6c00
08025faa: 2c49       ldr       r1, [pc, #0xb0] ; literal[0802605c]=20001914
08025fac: 0860       str       r0, [r1]
08025fae: 02e0       b         #0x8025fb6
08025fb0: 2b48       ldr       r0, [pc, #0xac] ; literal[08026060]=007a1200
08025fb2: 2a49       ldr       r1, [pc, #0xa8] ; literal[0802605c]=20001914
08025fb4: 0860       str       r0, [r1]
08025fb6: 39e0       b         #0x802602c
08025fb8: 2948       ldr       r0, [pc, #0xa4] ; literal[08026060]=007a1200
08025fba: 2849       ldr       r1, [pc, #0xa0] ; literal[0802605c]=20001914
08025fbc: 0860       str       r0, [r1]
08025fbe: 35e0       b         #0x802602c
08025fc0: 2448       ldr       r0, [pc, #0x90] ; literal[08026054]=400238a0
08025fc2: 9c38       subs      r0, #0x9c
08025fc4: 0068       ldr       r0, [r0]
08025fc6: c0f3805b   ubfx      fp, r0, #0x16, #1
08025fca: 2248       ldr       r0, [pc, #0x88] ; literal[08026054]=400238a0
08025fcc: 9c38       subs      r0, #0x9c
08025fce: 0068       ldr       r0, [r0]
08025fd0: c0f38814   ubfx      r4, r0, #6, #9
08025fd4: 1f48       ldr       r0, [pc, #0x7c] ; literal[08026054]=400238a0
08025fd6: 9c38       subs      r0, #0x9c
08025fd8: 0068       ldr       r0, [r0]
08025fda: 00f00f09   and       sb, r0, #0xf
08025fde: 1d48       ldr       r0, [pc, #0x74] ; literal[08026054]=400238a0
08025fe0: 9c38       subs      r0, #0x9c
08025fe2: 0068       ldr       r0, [r0]
08025fe4: c0f30240   ubfx      r0, r0, #0x10, #3
08025fe8: 1e49       ldr       r1, [pc, #0x78] ; literal[08026064]=0802ecd7
08025fea: 11f800a0   ldrb.w    sl, [r1, r0]
08025fee: bbf1000f   cmp.w     fp, #0
08025ff2: 02d1       bne       #0x8025ffa
08025ff4: 1a48       ldr       r0, [pc, #0x68] ; literal[08026060]=007a1200
08025ff6: 0390       str       r0, [sp, #0xc]
08025ff8: 01e0       b         #0x8025ffe
08025ffa: 1948       ldr       r0, [pc, #0x64] ; literal[08026060]=007a1200
08025ffc: 0390       str       r0, [sp, #0xc]
08025ffe: 09fb0afc   mul       ip, sb, sl
08026002: 0398       ldr       r0, [sp, #0xc]
08026004: a0fb0470   umull     r7, r0, r0, r4
08026008: 0146       mov       r1, r0
0802600a: 6246       mov       r2, ip
0802600c: 0023       movs      r3, #0
0802600e: 3846       mov       r0, r7
08026010: dcf776fd   bl        #0x8002b00
08026014: 0546       mov       r5, r0
08026016: 0222       movs      r2, #2
08026018: 0023       movs      r3, #0
0802601a: dcf771fd   bl        #0x8002b00
0802601e: 0f49       ldr       r1, [pc, #0x3c] ; literal[0802605c]=20001914
08026020: 0860       str       r0, [r1]
08026022: 03e0       b         #0x802602c
08026024: 0e48       ldr       r0, [pc, #0x38] ; literal[08026060]=007a1200
08026026: 0d49       ldr       r1, [pc, #0x34] ; literal[0802605c]=20001914
08026028: 0860       str       r0, [r1]
0802602a: 00bf       nop       
0802602c: 00bf       nop       
0802602e: 0948       ldr       r0, [pc, #0x24] ; literal[08026054]=400238a0
08026030: 9838       subs      r0, #0x98
08026032: 0068       ldr       r0, [r0]
08026034: c0f30310   ubfx      r0, r0, #4, #4
08026038: 0290       str       r0, [sp, #8]
0802603a: 0b49       ldr       r1, [pc, #0x2c] ; literal[08026068]=0802ecc7
0802603c: 0298       ldr       r0, [sp, #8]
0802603e: 085c       ldrb      r0, [r1, r0]
08026040: 0190       str       r0, [sp, #4]
08026042: 0648       ldr       r0, [pc, #0x18] ; literal[0802605c]=20001914
08026044: 0068       ldr       r0, [r0]
08026046: 0199       ldr       r1, [sp, #4]
08026048: c840       lsrs      r0, r1
0802604a: 0449       ldr       r1, [pc, #0x10] ; literal[0802605c]=20001914
0802604c: 0860       str       r0, [r1]
0802604e: bde8       .byte     0xbd, 0xe8
