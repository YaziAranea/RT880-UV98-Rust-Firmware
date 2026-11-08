08002d64: 1cb5       push      {r2, r3, r4, lr}
08002d66: 01a8       add       r0, sp, #4
08002d68: 21f0dafe   bl        #0x8024b20
08002d6c: 0220       movs      r0, #2
08002d6e: 8df80400   strb.w    r0, [sp, #4]
08002d72: 0020       movs      r0, #0
08002d74: 8df80500   strb.w    r0, [sp, #5]
08002d78: 01a8       add       r0, sp, #4
08002d7a: 21f0bbfe   bl        #0x8024af4
08002d7e: 6846       mov       r0, sp
08002d80: 21f093fe   bl        #0x8024aaa
08002d84: 0120       movs      r0, #1
08002d86: 8df80000   strb.w    r0, [sp]
08002d8a: 0020       movs      r0, #0
08002d8c: 8df80100   strb.w    r0, [sp, #1]
08002d90: 8df80200   strb.w    r0, [sp, #2]
08002d94: 0220       movs      r0, #2
08002d96: 8df80300   strb.w    r0, [sp, #3]
08002d9a: 6946       mov       r1, sp
08002d9c: 2a48       ldr       r0, [pc, #0xa8] ; literal[08002e48]=40012000
08002d9e: 21f06bfe   bl        #0x8024a78
08002da2: 0021       movs      r1, #0
08002da4: 2848       ldr       r0, [pc, #0xa0] ; literal[08002e48]=40012000
08002da6: 21f092ff   bl        #0x8024cce
08002daa: 0423       movs      r3, #4
08002dac: 0122       movs      r2, #1
08002dae: 0721       movs      r1, #7
08002db0: 2548       ldr       r0, [pc, #0x94] ; literal[08002e48]=40012000
08002db2: 21f0defe   bl        #0x8024b72
08002db6: 0423       movs      r3, #4
08002db8: 0222       movs      r2, #2
08002dba: 0c21       movs      r1, #0xc
08002dbc: 2248       ldr       r0, [pc, #0x88] ; literal[08002e48]=40012000
08002dbe: 21f0d8fe   bl        #0x8024b72
08002dc2: 0022       movs      r2, #0
08002dc4: 0721       movs      r1, #7
08002dc6: 2048       ldr       r0, [pc, #0x80] ; literal[08002e48]=40012000
08002dc8: 21f073ff   bl        #0x8024cb2
08002dcc: 0121       movs      r1, #1
08002dce: 1e48       ldr       r0, [pc, #0x78] ; literal[08002e48]=40012000
08002dd0: 21f0aafe   bl        #0x8024b28
08002dd4: 0121       movs      r1, #1
08002dd6: 1c48       ldr       r0, [pc, #0x70] ; literal[08002e48]=40012000
08002dd8: 21f0abfe   bl        #0x8024b32
08002ddc: 0022       movs      r2, #0
08002dde: 4ff48071   mov.w     r1, #0x100
08002de2: 1948       ldr       r0, [pc, #0x64] ; literal[08002e48]=40012000
08002de4: 21f08dff   bl        #0x8024d02
08002de8: 0721       movs      r1, #7
08002dea: 1748       ldr       r0, [pc, #0x5c] ; literal[08002e48]=40012000
08002dec: 21f084ff   bl        #0x8024cf8
08002df0: 1649       ldr       r1, [pc, #0x58] ; literal[08002e4c]=00800200
08002df2: 1548       ldr       r0, [pc, #0x54] ; literal[08002e48]=40012000
08002df4: 21f070ff   bl        #0x8024cd8
08002df8: 0122       movs      r2, #1
08002dfa: 9106       lsls      r1, r2, #0x1a
08002dfc: 1248       ldr       r0, [pc, #0x48] ; literal[08002e48]=40012000
08002dfe: 21f0adfe   bl        #0x8024b5c
08002e02: 0122       movs      r2, #1
08002e04: 4021       movs      r1, #0x40
08002e06: 1048       ldr       r0, [pc, #0x40] ; literal[08002e48]=40012000
08002e08: 21f0a8fe   bl        #0x8024b5c
08002e0c: 0121       movs      r1, #1
08002e0e: 0e48       ldr       r0, [pc, #0x38] ; literal[08002e48]=40012000
08002e10: 21f094fe   bl        #0x8024b3c
08002e14: 00bf       nop       
08002e16: 4021       movs      r1, #0x40
08002e18: 0b48       ldr       r0, [pc, #0x2c] ; literal[08002e48]=40012000
08002e1a: 21f097fe   bl        #0x8024b4c
08002e1e: 0028       cmp       r0, #0
08002e20: f9d0       beq       #0x8002e16
08002e22: 0948       ldr       r0, [pc, #0x24] ; literal[08002e48]=40012000
08002e24: 21f048fe   bl        #0x8024ab8
08002e28: 00bf       nop       
08002e2a: 0748       ldr       r0, [pc, #0x1c] ; literal[08002e48]=40012000
08002e2c: 21f04afe   bl        #0x8024ac4
08002e30: 0028       cmp       r0, #0
08002e32: fad1       bne       #0x8002e2a
08002e34: 0448       ldr       r0, [pc, #0x10] ; literal[08002e48]=40012000
08002e36: 21f04efe   bl        #0x8024ad6
08002e3a: 00bf       nop       
08002e3c: 0248       ldr       r0, [pc, #8] ; literal[08002e48]=40012000
08002e3e: 21f050fe   bl        #0x8024ae2
08002e42: 0028       cmp       r0, #0
08002e44: fad1       bne       #0x8002e3c
08002e46: 1cbd       pop       {r2, r3, r4, pc}
08002e48: 0020       movs      r0, #0
08002e4a: 0140       ands      r1, r0
08002e4c: 0002       lsls      r0, r0, #8
08002e4e: 8000       lsls      r0, r0, #2
08002e50: 5a20       movs      r0, #0x5a
08002e52: 0149       ldr       r1, [pc, #4] ; literal[08002e58]=200018d4
08002e54: 0860       str       r0, [r1]
08002e56: 7047       bx        lr
08002e58: d418       adds      r4, r2, r3
08002e5a: 0020       movs      r0, #0
08002e5c: 0020       movs      r0, #0
08002e5e: 0449       ldr       r1, [pc, #0x10] ; literal[08002e70]=200018dc
08002e60: 0880       strh      r0, [r1]
08002e62: 0449       ldr       r1, [pc, #0x10] ; literal[08002e74]=200018de
08002e64: 0870       strb      r0, [r1]
08002e66: 0449       ldr       r1, [pc, #0x10] ; literal[08002e78]=200018e0
08002e68: 0880       strh      r0, [r1]
08002e6a: 0449       ldr       r1, [pc, #0x10] ; literal[08002e7c]=200018d8
08002e6c: 0860       str       r0, [r1]
08002e6e: 7047       bx        lr
08002e70: dc18       adds      r4, r3, r3
08002e72: 0020       movs      r0, #0
08002e74: de18       adds      r6, r3, r3
08002e76: 0020       movs      r0, #0
08002e78: e018       adds      r0, r4, r3
08002e7a: 0020       movs      r0, #0
08002e7c: d818       adds      r0, r3, r3
08002e7e: 0020       movs      r0, #0
08002e80: f0b5       push      {r4, r5, r6, r7, lr}
08002e82: 0346       mov       r3, r0
08002e84: 0c46       mov       r4, r1
08002e86: 0025       movs      r5, #0
08002e88: 0021       movs      r1, #0
08002e8a: 0020       movs      r0, #0
08002e8c: 01e0       b         #0x8002e92
08002e8e: 4e1c       adds      r6, r1, #1
08002e90: b1b2       uxth      r1, r6
08002e92: 9142       cmp       r1, r2
08002e94: 02da       bge       #0x8002e9c
08002e96: 5e5c       ldrb      r6, [r3, r1]
08002e98: 7e2e       cmp       r6, #0x7e
08002e9a: f8d0       beq       #0x8002e8e
08002e9c: 33e0       b         #0x8002f06
08002e9e: 5e5c       ldrb      r6, [r3, r1]
08002ea0: 032e       cmp       r6, #3
08002ea2: 14d0       beq       #0x8002ece
08002ea4: 5e5c       ldrb      r6, [r3, r1]
08002ea6: 832e       cmp       r6, #0x83
08002ea8: 03d1       bne       #0x8002eb2
08002eaa: 4e1c       adds      r6, r1, #1
08002eac: 9e5d       ldrb      r6, [r3, r6]
08002eae: f02e       cmp       r6, #0xf0
08002eb0: 0dd0       beq       #0x8002ece
08002eb2: 5e5c       ldrb      r6, [r3, r1]
08002eb4: 832e       cmp       r6, #0x83
08002eb6: 03d1       bne       #0x8002ec0
08002eb8: 4e1c       adds      r6, r1, #1
08002eba: 9e5d       ldrb      r6, [r3, r6]
08002ebc: f12e       cmp       r6, #0xf1
08002ebe: 06d0       beq       #0x8002ece
08002ec0: 5e5c       ldrb      r6, [r3, r1]
08002ec2: 832e       cmp       r6, #0x83
08002ec4: 15d1       bne       #0x8002ef2
08002ec6: 4e1c       adds      r6, r1, #1
08002ec8: 9e5d       ldrb      r6, [r3, r6]
08002eca: 702e       cmp       r6, #0x70
08002ecc: 11d1       bne       #0x8002ef2
08002ece: 0125       movs      r5, #1
08002ed0: 4ff0030c   mov.w     ip, #3
08002ed4: 0646       mov       r6, r0
08002ed6: 471c       adds      r7, r0, #1
08002ed8: b8b2       uxth      r0, r7
08002eda: 04f806c0   strb.w    ip, [r4, r6]
08002ede: 4e1c       adds      r6, r1, #1
08002ee0: b1b2       uxth      r1, r6
08002ee2: 4ff0f00c   mov.w     ip, #0xf0
08002ee6: 0646       mov       r6, r0
08002ee8: 471c       adds      r7, r0, #1
08002eea: b8b2       uxth      r0, r7
08002eec: 04f806c0   strb.w    ip, [r4, r6]
08002ef0: 07e0       b         #0x8002f02
08002ef2: 5e5c       ldrb      r6, [r3, r1]
08002ef4: 4fea660c   asr.w     ip, r6, #1
08002ef8: 0646       mov       r6, r0
08002efa: 471c       adds      r7, r0, #1
08002efc: b8b2       uxth      r0, r7
08002efe: 04f806c0   strb.w    ip, [r4, r6]
08002f02: 4e1c       adds      r6, r1, #1
08002f04: b1b2       uxth      r1, r6
08002f06: 561e       subs      r6, r2, #1
08002f08: 8e42       cmp       r6, r1
08002f0a: 01dd       ble       #0x8002f10
08002f0c: 002d       cmp       r5, #0
08002f0e: c6d0       beq       #0x8002e9e
08002f10: 09e0       b         #0x8002f26
08002f12: 0e46       mov       r6, r1
08002f14: 4f1c       adds      r7, r1, #1
08002f16: b9b2       uxth      r1, r7
08002f18: 13f806c0   ldrb.w    ip, [r3, r6]
08002f1c: 0646       mov       r6, r0
08002f1e: 471c       adds      r7, r0, #1
08002f20: b8b2       uxth      r0, r7
08002f22: 04f806c0   strb.w    ip, [r4, r6]
08002f26: d61e       subs      r6, r2, #3
08002f28: 8e42       cmp       r6, r1
08002f2a: f2dc       bgt       #0x8002f12
08002f2c: f0bd       pop       {r4, r5, r6, r7, pc}
08002f2e: 0000       movs      r0, r0
08002f30: 1cb5       push      {r2, r3, r4, lr}
08002f32: 0446       mov       r4, r0
08002f34: 6846       mov       r0, sp
08002f36: 22f08bfa   bl        #0x8025450
08002f3a: 9cb9       cbnz      r4, #0x8002f64
08002f3c: 0120       movs      r0, #1
08002f3e: 1049       ldr       r1, [pc, #0x40] ; literal[08002f80]=40020000
08002f40: 8862       str       r0, [r1, #0x28]
08002f42: 0220       movs      r0, #2
08002f44: 8862       str       r0, [r1, #0x28]
08002f46: 0f49       ldr       r1, [pc, #0x3c] ; literal[08002f84]=40021428
08002f48: 0860       str       r0, [r1]
08002f4a: 0020       movs      r0, #0
08002f4c: 8df80600   strb.w    r0, [sp, #6]
08002f50: 0420       movs      r0, #4
08002f52: 0090       str       r0, [sp]
08002f54: 0220       movs      r0, #2
08002f56: 8df80500   strb.w    r0, [sp, #5]
08002f5a: 6946       mov       r1, sp
08002f5c: 0848       ldr       r0, [pc, #0x20] ; literal[08002f80]=40020000
08002f5e: 22f081fa   bl        #0x8025464
08002f62: 0be0       b         #0x8002f7c
08002f64: 0020       movs      r0, #0
08002f66: 8df80600   strb.w    r0, [sp, #6]
08002f6a: 0420       movs      r0, #4
08002f6c: 0090       str       r0, [sp]
08002f6e: 0120       movs      r0, #1
08002f70: 8df80500   strb.w    r0, [sp, #5]
08002f74: 6946       mov       r1, sp
08002f76: 0248       ldr       r0, [pc, #8] ; literal[08002f80]=40020000
08002f78: 22f074fa   bl        #0x8025464
08002f7c: 1cbd       pop       {r2, r3, r4, pc}
08002f7e: 0000       movs      r0, r0
08002f80: 0000       movs      r0, r0
08002f82: 0240       ands      r2, r0
08002f84: 2814       asrs      r0, r5, #0x10
08002f86: 0240       ands      r2, r0
08002f88: 70b5       push      {r4, r5, r6, lr}
08002f8a: 0446       mov       r4, r0
08002f8c: 0d46       mov       r5, r1
08002f8e: 022d       cmp       r5, #2
08002f90: 05d1       bne       #0x8002f9e
08002f92: 6422       movs      r2, #0x64
08002f94: 4ff4dc70   mov.w     r0, #0x1b8
08002f98: 15f058f9   bl        #0x801824c
08002f9c: 70bd       pop       {r4, r5, r6, pc}
08002f9e: 0b2c       cmp       r4, #0xb
08002fa0: 1dd0       beq       #0x8002fde
08002fa2: 0c2c       cmp       r4, #0xc
08002fa4: 49d0       beq       #0x800303a
08002fa6: 0d2c       cmp       r4, #0xd
08002fa8: 76d1       bne       #0x8003098
08002faa: 4048       ldr       r0, [pc, #0x100] ; literal[080030ac]=20001283
08002fac: 0078       ldrb      r0, [r0]
08002fae: 10b9       cbnz      r0, #0x8002fb6
08002fb0: 0cf0fefd   bl        #0x800fbb0
08002fb4: 0ce0       b         #0x8002fd0
08002fb6: 0122       movs      r2, #1
08002fb8: ff21       movs      r1, #0xff
08002fba: 0020       movs      r0, #0
08002fbc: 1cf0a2fb   bl        #0x801f704
08002fc0: 09f01afa   bl        #0x800c3f8
08002fc4: 0a20       movs      r0, #0xa
08002fc6: 3a49       ldr       r1, [pc, #0xe8] ; literal[080030b0]=2000083b
08002fc8: 0870       strb      r0, [r1]
08002fca: 0020       movs      r0, #0
08002fcc: 3749       ldr       r1, [pc, #0xdc] ; literal[080030ac]=20001283
08002fce: 0870       strb      r0, [r1]
08002fd0: 5022       movs      r2, #0x50
08002fd2: 0421       movs      r1, #4
08002fd4: 4ff4dc70   mov.w     r0, #0x1b8
08002fd8: 15f038f9   bl        #0x801824c
08002fdc: 63e0       b         #0x80030a6
08002fde: 3548       ldr       r0, [pc, #0xd4] ; literal[080030b4]=20000a56
08002fe0: b0f81205   ldrh.w    r0, [r0, #0x512]
08002fe4: 8528       cmp       r0, #0x85
08002fe6: 0cdc       bgt       #0x8003002
08002fe8: 3248       ldr       r0, [pc, #0xc8] ; literal[080030b4]=20000a56
08002fea: 90f82a05   ldrb.w    r0, [r0, #0x52a]
08002fee: 801c       adds      r0, r0, #2
08002ff0: 0321       movs      r1, #3
08002ff2: 90fbf1f2   sdiv      r2, r0, r1
08002ff6: 01fb1200   mls       r0, r1, r2, r0
08002ffa: 2e49       ldr       r1, [pc, #0xb8] ; literal[080030b4]=20000a56
08002ffc: 81f82a05   strb.w    r0, [r1, #0x52a]
08003000: 0ce0       b         #0x800301c
08003002: 2c49       ldr       r1, [pc, #0xb0] ; literal[080030b4]=20000a56
08003004: 91f82a15   ldrb.w    r1, [r1, #0x52a]
08003008: c81c       adds      r0, r1, #3
0800300a: c117       asrs      r1, r0, #0x1f
0800300c: 00eb9171   add.w     r1, r0, r1, lsr #30
08003010: 8910       asrs      r1, r1, #2
08003012: a0eb8101   sub.w     r1, r0, r1, lsl #2
08003016: 274a       ldr       r2, [pc, #0x9c] ; literal[080030b4]=20000a56
08003018: 82f82a15   strb.w    r1, [r2, #0x52a]
0800301c: 0120       movs      r0, #1
0800301e: 1cf009fb   bl        #0x801f634
08003022: 0122       movs      r2, #1
08003024: ff21       movs      r1, #0xff
08003026: 0020       movs      r0, #0
08003028: 1cf06cfb   bl        #0x801f704
0800302c: 6422       movs      r2, #0x64
0800302e: 0221       movs      r1, #2
08003030: 40f25c60   movw      r0, #0x65c
08003034: 15f00af9   bl        #0x801824c
08003038: 35e0       b         #0x80030a6
0800303a: 1e48       ldr       r0, [pc, #0x78] ; literal[080030b4]=20000a56
0800303c: b0f81205   ldrh.w    r0, [r0, #0x512]
08003040: 8528       cmp       r0, #0x85
08003042: 0cdc       bgt       #0x800305e
08003044: 1b48       ldr       r0, [pc, #0x6c] ; literal[080030b4]=20000a56
08003046: 90f82a05   ldrb.w    r0, [r0, #0x52a]
0800304a: 401c       adds      r0, r0, #1
0800304c: 0321       movs      r1, #3
0800304e: 90fbf1f2   sdiv      r2, r0, r1
08003052: 01fb1200   mls       r0, r1, r2, r0
08003056: 1749       ldr       r1, [pc, #0x5c] ; literal[080030b4]=20000a56
08003058: 81f82a05   strb.w    r0, [r1, #0x52a]
0800305c: 0ce0       b         #0x8003078
0800305e: 1549       ldr       r1, [pc, #0x54] ; literal[080030b4]=20000a56
08003060: 91f82a15   ldrb.w    r1, [r1, #0x52a]
08003064: 481c       adds      r0, r1, #1
08003066: c117       asrs      r1, r0, #0x1f
08003068: 00eb9171   add.w     r1, r0, r1, lsr #30
0800306c: 8910       asrs      r1, r1, #2
0800306e: a0eb8101   sub.w     r1, r0, r1, lsl #2
08003072: 104a       ldr       r2, [pc, #0x40] ; literal[080030b4]=20000a56
08003074: 82f82a15   strb.w    r1, [r2, #0x52a]
08003078: 0120       movs      r0, #1
0800307a: 1cf0dbfa   bl        #0x801f634
0800307e: 0122       movs      r2, #1
08003080: ff21       movs      r1, #0xff
08003082: 0020       movs      r0, #0
08003084: 1cf03efb   bl        #0x801f704
08003088: 6422       movs      r2, #0x64
0800308a: 0221       movs      r1, #2
0800308c: 40f25c60   movw      r0, #0x65c
08003090: 15f0dcf8   bl        #0x801824c
08003094: 07e0       b         #0x80030a6
08003096: ffe7       b         #0x8003098
08003098: 6422       movs      r2, #0x64
0800309a: 0221       movs      r1, #2
0800309c: 4ff4dc70   mov.w     r0, #0x1b8
080030a0: 15f0d4f8   bl        #0x801824c
080030a4: 00bf       nop       
080030a6: 00bf       nop       
080030a8: 00bf       nop       
080030aa: 77e7       b         #0x8002f9c
080030ac: 8312       asrs      r3, r0, #0xa
080030ae: 0020       movs      r0, #0
080030b0: 3b08       lsrs      r3, r7, #0x20
080030b2: 0020       movs      r0, #0
080030b4: 560a       lsrs      r6, r2, #9
080030b6: 0020       movs      r0, #0
080030b8: 10b5       push      {r4, lr}
080030ba: 0024       movs      r4, #0
080030bc: 1323       movs      r3, #0x13
080030be: 0022       movs      r2, #0
080030c0: 46a1       adr       r1, #0x118
080030c2: 4b48       ldr       r0, [pc, #0x12c] ; literal[080031f0]=20000a56
080030c4: 05f071f8   bl        #0x80081aa
080030c8: 1323       movs      r3, #0x13
080030ca: 0022       movs      r2, #0
080030cc: 49a1       adr       r1, #0x124
080030ce: 4848       ldr       r0, [pc, #0x120] ; literal[080031f0]=20000a56
080030d0: 1330       adds      r0, #0x13
080030d2: 05f06af8   bl        #0x80081aa
080030d6: 1323       movs      r3, #0x13
080030d8: 0022       movs      r2, #0
080030da: 4ba1       adr       r1, #0x12c
080030dc: 4448       ldr       r0, [pc, #0x110] ; literal[080031f0]=20000a56
080030de: 2630       adds      r0, #0x26
080030e0: 05f063f8   bl        #0x80081aa
080030e4: 1323       movs      r3, #0x13
080030e6: 0022       movs      r2, #0
080030e8: 4ca1       adr       r1, #0x130
080030ea: 4148       ldr       r0, [pc, #0x104] ; literal[080031f0]=20000a56
080030ec: 3930       adds      r0, #0x39
080030ee: 05f05cf8   bl        #0x80081aa
080030f2: 0823       movs      r3, #8
080030f4: 0022       movs      r2, #0
080030f6: 4ea1       adr       r1, #0x138
080030f8: 3d48       ldr       r0, [pc, #0xf4] ; literal[080031f0]=20000a56
080030fa: 4c30       adds      r0, #0x4c
080030fc: 05f055f8   bl        #0x80081aa
