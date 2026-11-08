08011ac8: 1cb5       push      {r2, r3, r4, lr}
08011aca: 6846       mov       r0, sp
08011acc: 13f0c0fc   bl        #0x8025450
08011ad0: 0220       movs      r0, #2
08011ad2: 8df80700   strb.w    r0, [sp, #7]
08011ad6: 0020       movs      r0, #0
08011ad8: 8df80400   strb.w    r0, [sp, #4]
08011adc: 0120       movs      r0, #1
08011ade: 8df80600   strb.w    r0, [sp, #6]
08011ae2: 40f20320   movw      r0, #0x203
08011ae6: 0090       str       r0, [sp]
08011ae8: 0020       movs      r0, #0
08011aea: 8df80500   strb.w    r0, [sp, #5]
08011aee: 6946       mov       r1, sp
08011af0: a348       ldr       r0, [pc, #0x28c] ; literal[08011d80]=40020000
08011af2: 13f0b7fc   bl        #0x8025464
08011af6: 0120       movs      r0, #1
08011af8: a149       ldr       r1, [pc, #0x284] ; literal[08011d80]=40020000
08011afa: 8862       str       r0, [r1, #0x28]
08011afc: 0220       movs      r0, #2
08011afe: 8862       str       r0, [r1, #0x28]
08011b00: 0002       lsls      r0, r0, #8
08011b02: 8862       str       r0, [r1, #0x28]
08011b04: 0220       movs      r0, #2
08011b06: 8df80700   strb.w    r0, [sp, #7]
08011b0a: 0020       movs      r0, #0
08011b0c: 8df80400   strb.w    r0, [sp, #4]
08011b10: 0120       movs      r0, #1
08011b12: 8df80600   strb.w    r0, [sp, #6]
08011b16: 4ff42860   mov.w     r0, #0xa80
08011b1a: 0090       str       r0, [sp]
08011b1c: 0020       movs      r0, #0
08011b1e: 8df80500   strb.w    r0, [sp, #5]
08011b22: 6946       mov       r1, sp
08011b24: 9748       ldr       r0, [pc, #0x25c] ; literal[08011d84]=40020400
08011b26: 13f09dfc   bl        #0x8025464
08011b2a: 4ff40060   mov.w     r0, #0x800
08011b2e: 9549       ldr       r1, [pc, #0x254] ; literal[08011d84]=40020400
08011b30: 2831       adds      r1, #0x28
08011b32: 0860       str       r0, [r1]
08011b34: 8010       asrs      r0, r0, #2
08011b36: 0860       str       r0, [r1]
08011b38: 0020       movs      r0, #0
08011b3a: f1f7f9f9   bl        #0x8002f30
08011b3e: 8020       movs      r0, #0x80
08011b40: 9049       ldr       r1, [pc, #0x240] ; literal[08011d84]=40020400
08011b42: 1831       adds      r1, #0x18
08011b44: 0860       str       r0, [r1]
08011b46: 0220       movs      r0, #2
08011b48: 8df80700   strb.w    r0, [sp, #7]
08011b4c: 0020       movs      r0, #0
08011b4e: 8df80400   strb.w    r0, [sp, #4]
08011b52: 0120       movs      r0, #1
08011b54: 8df80600   strb.w    r0, [sp, #6]
08011b58: 48f23000   movw      r0, #0x8030
08011b5c: 0090       str       r0, [sp]
08011b5e: 0020       movs      r0, #0
08011b60: 8df80500   strb.w    r0, [sp, #5]
08011b64: 6946       mov       r1, sp
08011b66: 8848       ldr       r0, [pc, #0x220] ; literal[08011d88]=40020800
08011b68: 13f07cfc   bl        #0x8025464
08011b6c: 4ff40040   mov.w     r0, #0x8000
08011b70: 8549       ldr       r1, [pc, #0x214] ; literal[08011d88]=40020800
08011b72: 2831       adds      r1, #0x28
08011b74: 0860       str       r0, [r1]
08011b76: 1020       movs      r0, #0x10
08011b78: 0860       str       r0, [r1]
08011b7a: 2020       movs      r0, #0x20
08011b7c: 0860       str       r0, [r1]
08011b7e: 0220       movs      r0, #2
08011b80: 8df80700   strb.w    r0, [sp, #7]
08011b84: 0020       movs      r0, #0
08011b86: 8df80400   strb.w    r0, [sp, #4]
08011b8a: 0120       movs      r0, #1
08011b8c: 8df80600   strb.w    r0, [sp, #6]
08011b90: 40f20310   movw      r0, #0x103
08011b94: 0090       str       r0, [sp]
08011b96: 0020       movs      r0, #0
08011b98: 8df80500   strb.w    r0, [sp, #5]
08011b9c: 6946       mov       r1, sp
08011b9e: 7b48       ldr       r0, [pc, #0x1ec] ; literal[08011d8c]=40021400
08011ba0: 13f060fc   bl        #0x8025464
08011ba4: 4ff48070   mov.w     r0, #0x100
08011ba8: 7849       ldr       r1, [pc, #0x1e0] ; literal[08011d8c]=40021400
08011baa: 2831       adds      r1, #0x28
08011bac: 0860       str       r0, [r1]
08011bae: 0220       movs      r0, #2
08011bb0: 0860       str       r0, [r1]
08011bb2: 0020       movs      r0, #0
08011bb4: 8df80600   strb.w    r0, [sp, #6]
08011bb8: 40f20440   movw      r0, #0x404
08011bbc: 0090       str       r0, [sp]
08011bbe: 0120       movs      r0, #1
08011bc0: 8df80500   strb.w    r0, [sp, #5]
08011bc4: 6946       mov       r1, sp
08011bc6: 6e48       ldr       r0, [pc, #0x1b8] ; literal[08011d80]=40020000
08011bc8: 13f04cfc   bl        #0x8025464
08011bcc: 0020       movs      r0, #0
08011bce: 8df80600   strb.w    r0, [sp, #6]
08011bd2: 5020       movs      r0, #0x50
08011bd4: 0090       str       r0, [sp]
08011bd6: 0120       movs      r0, #1
08011bd8: 8df80500   strb.w    r0, [sp, #5]
08011bdc: 6946       mov       r1, sp
08011bde: 6948       ldr       r0, [pc, #0x1a4] ; literal[08011d84]=40020400
08011be0: 13f040fc   bl        #0x8025464
08011be4: 0220       movs      r0, #2
08011be6: 8df80700   strb.w    r0, [sp, #7]
08011bea: 0020       movs      r0, #0
08011bec: 8df80400   strb.w    r0, [sp, #4]
08011bf0: 0120       movs      r0, #1
08011bf2: 8df80600   strb.w    r0, [sp, #6]
08011bf6: 0002       lsls      r0, r0, #8
08011bf8: 0090       str       r0, [sp]
08011bfa: 0020       movs      r0, #0
08011bfc: 8df80500   strb.w    r0, [sp, #5]
08011c00: 6946       mov       r1, sp
08011c02: 5f48       ldr       r0, [pc, #0x17c] ; literal[08011d80]=40020000
08011c04: 13f02efc   bl        #0x8025464
08011c08: 0220       movs      r0, #2
08011c0a: 8df80700   strb.w    r0, [sp, #7]
08011c0e: 0020       movs      r0, #0
08011c10: 8df80400   strb.w    r0, [sp, #4]
08011c14: 0120       movs      r0, #1
08011c16: 8df80600   strb.w    r0, [sp, #6]
08011c1a: 4ff47270   mov.w     r0, #0x3c8
08011c1e: 0090       str       r0, [sp]
08011c20: 0020       movs      r0, #0
08011c22: 8df80500   strb.w    r0, [sp, #5]
08011c26: 6946       mov       r1, sp
08011c28: 5748       ldr       r0, [pc, #0x15c] ; literal[08011d88]=40020800
08011c2a: 13f01bfc   bl        #0x8025464
08011c2e: 0820       movs      r0, #8
08011c30: 5549       ldr       r1, [pc, #0x154] ; literal[08011d88]=40020800
08011c32: 2831       adds      r1, #0x28
08011c34: 0860       str       r0, [r1]
08011c36: 0220       movs      r0, #2
08011c38: 8df80700   strb.w    r0, [sp, #7]
08011c3c: 0020       movs      r0, #0
08011c3e: 8df80400   strb.w    r0, [sp, #4]
08011c42: 0120       movs      r0, #1
08011c44: 8df80600   strb.w    r0, [sp, #6]
08011c48: 4020       movs      r0, #0x40
08011c4a: 0090       str       r0, [sp]
08011c4c: 0020       movs      r0, #0
08011c4e: 8df80500   strb.w    r0, [sp, #5]
08011c52: 6946       mov       r1, sp
08011c54: 4d48       ldr       r0, [pc, #0x134] ; literal[08011d8c]=40021400
08011c56: 13f005fc   bl        #0x8025464
08011c5a: 4ff48070   mov.w     r0, #0x100
08011c5e: 4849       ldr       r1, [pc, #0x120] ; literal[08011d80]=40020000
08011c60: 8862       str       r0, [r1, #0x28]
08011c62: 4020       movs      r0, #0x40
08011c64: 4949       ldr       r1, [pc, #0x124] ; literal[08011d8c]=40021400
08011c66: 2831       adds      r1, #0x28
08011c68: 0860       str       r0, [r1]
08011c6a: c000       lsls      r0, r0, #3
08011c6c: 4649       ldr       r1, [pc, #0x118] ; literal[08011d88]=40020800
08011c6e: 2831       adds      r1, #0x28
08011c70: 0860       str       r0, [r1]
08011c72: 8020       movs      r0, #0x80
08011c74: 0860       str       r0, [r1]
08011c76: 4000       lsls      r0, r0, #1
08011c78: 0860       str       r0, [r1]
08011c7a: 4020       movs      r0, #0x40
08011c7c: 0860       str       r0, [r1]
08011c7e: 0220       movs      r0, #2
08011c80: 8df80700   strb.w    r0, [sp, #7]
08011c84: 0020       movs      r0, #0
08011c86: 8df80400   strb.w    r0, [sp, #4]
08011c8a: 0120       movs      r0, #1
08011c8c: 8df80600   strb.w    r0, [sp, #6]
08011c90: 40f20740   movw      r0, #0x407
08011c94: 0090       str       r0, [sp]
08011c96: 0020       movs      r0, #0
08011c98: 8df80500   strb.w    r0, [sp, #5]
08011c9c: 6946       mov       r1, sp
08011c9e: 3948       ldr       r0, [pc, #0xe4] ; literal[08011d84]=40020400
08011ca0: 13f0e0fb   bl        #0x8025464
08011ca4: 4ff48060   mov.w     r0, #0x400
08011ca8: 3649       ldr       r1, [pc, #0xd8] ; literal[08011d84]=40020400
08011caa: 1831       adds      r1, #0x18
08011cac: 0860       str       r0, [r1]
08011cae: 0420       movs      r0, #4
08011cb0: 0860       str       r0, [r1]
08011cb2: 0220       movs      r0, #2
08011cb4: 0860       str       r0, [r1]
08011cb6: 0120       movs      r0, #1
08011cb8: 0860       str       r0, [r1]
08011cba: 0020       movs      r0, #0
08011cbc: 8df80600   strb.w    r0, [sp, #6]
08011cc0: 4ff4e050   mov.w     r0, #0x1c00
08011cc4: 0090       str       r0, [sp]
08011cc6: 0120       movs      r0, #1
08011cc8: 8df80500   strb.w    r0, [sp, #5]
08011ccc: 6946       mov       r1, sp
08011cce: 2e48       ldr       r0, [pc, #0xb8] ; literal[08011d88]=40020800
08011cd0: 13f0c8fb   bl        #0x8025464
08011cd4: 0020       movs      r0, #0
08011cd6: 8df80600   strb.w    r0, [sp, #6]
08011cda: 0420       movs      r0, #4
08011cdc: 0090       str       r0, [sp]
08011cde: 0120       movs      r0, #1
08011ce0: 8df80500   strb.w    r0, [sp, #5]
08011ce4: 6946       mov       r1, sp
08011ce6: 2a48       ldr       r0, [pc, #0xa8] ; literal[08011d90]=40020c00
08011ce8: 13f0bcfb   bl        #0x8025464
08011cec: 0120       movs      r0, #1
08011cee: 8df80700   strb.w    r0, [sp, #7]
08011cf2: 0020       movs      r0, #0
08011cf4: 8df80400   strb.w    r0, [sp, #4]
08011cf8: 0220       movs      r0, #2
08011cfa: 8df80600   strb.w    r0, [sp, #6]
08011cfe: 41f60800   movw      r0, #0x1808
08011d02: 0090       str       r0, [sp]
08011d04: 0020       movs      r0, #0
08011d06: 8df80500   strb.w    r0, [sp, #5]
08011d0a: 6946       mov       r1, sp
08011d0c: 1c48       ldr       r0, [pc, #0x70] ; literal[08011d80]=40020000
08011d0e: 13f0a9fb   bl        #0x8025464
08011d12: 0822       movs      r2, #8
08011d14: 0c21       movs      r1, #0xc
08011d16: 1a48       ldr       r0, [pc, #0x68] ; literal[08011d80]=40020000
08011d18: 13f0eafb   bl        #0x80254f0
08011d1c: 0822       movs      r2, #8
08011d1e: 0b21       movs      r1, #0xb
08011d20: 1748       ldr       r0, [pc, #0x5c] ; literal[08011d80]=40020000
08011d22: 13f0e5fb   bl        #0x80254f0
08011d26: 0722       movs      r2, #7
08011d28: 0321       movs      r1, #3
08011d2a: 1548       ldr       r0, [pc, #0x54] ; literal[08011d80]=40020000
08011d2c: 13f0e0fb   bl        #0x80254f0
08011d30: 0320       movs      r0, #3
08011d32: 8df80600   strb.w    r0, [sp, #6]
08011d36: 8020       movs      r0, #0x80
08011d38: 0090       str       r0, [sp]
08011d3a: 6946       mov       r1, sp
08011d3c: 1048       ldr       r0, [pc, #0x40] ; literal[08011d80]=40020000
08011d3e: 13f091fb   bl        #0x8025464
08011d42: 0320       movs      r0, #3
08011d44: 8df80600   strb.w    r0, [sp, #6]
08011d48: 0420       movs      r0, #4
08011d4a: 0090       str       r0, [sp]
08011d4c: 6946       mov       r1, sp
08011d4e: 0e48       ldr       r0, [pc, #0x38] ; literal[08011d88]=40020800
08011d50: 13f088fb   bl        #0x8025464
08011d54: 3020       movs      r0, #0x30
08011d56: 0090       str       r0, [sp]
08011d58: 0320       movs      r0, #3
08011d5a: 8df80600   strb.w    r0, [sp, #6]
08011d5e: 0020       movs      r0, #0
08011d60: 8df80400   strb.w    r0, [sp, #4]
08011d64: 8df80500   strb.w    r0, [sp, #5]
08011d68: 0120       movs      r0, #1
08011d6a: 8df80700   strb.w    r0, [sp, #7]
08011d6e: 6946       mov       r1, sp
08011d70: 0348       ldr       r0, [pc, #0xc] ; literal[08011d80]=40020000
08011d72: 13f077fb   bl        #0x8025464
08011d76: 0020       movs      r0, #0
08011d78: 0bf08cfd   bl        #0x801d894
08011d7c: 1cbd       pop       {r2, r3, r4, pc}
08011d7e: 0000       movs      r0, r0
08011d80: 0000       movs      r0, r0
08011d82: 0240       ands      r2, r0
08011d84: 0004       lsls      r0, r0, #0x10
08011d86: 0240       ands      r2, r0
08011d88: 0008       lsrs      r0, r0, #0x20
08011d8a: 0240       ands      r2, r0
08011d8c: 0014       asrs      r0, r0, #0x10
08011d8e: 0240       ands      r2, r0
08011d90: 000c       lsrs      r0, r0, #0x10
08011d92: 0240       ands      r2, r0
08011d94: 70b5       push      {r4, r5, r6, lr}
08011d96: 0446       mov       r4, r0
08011d98: 0e46       mov       r6, r1
08011d9a: 0025       movs      r5, #0
08011d9c: 022e       cmp       r6, #2
08011d9e: 2cd1       bne       #0x8011dfa
08011da0: 192c       cmp       r4, #0x19
08011da2: 28d2       bhs       #0x8011df6
08011da4: dfe804f0   tbb       [pc, r4]
08011da8: 191a       subs      r1, r3, r0
08011daa: 1b1c       adds      r3, r3, #0
08011dac: 1d1e       subs      r5, r3, #0
08011dae: 1f20       movs      r0, #0x1f
08011db0: 2122       movs      r2, #0x21
08011db2: 2727       movs      r7, #0x27
08011db4: 2727       movs      r7, #0x27
08011db6: 2727       movs      r7, #0x27
08011db8: 2727       movs      r7, #0x27
08011dba: 2727       movs      r7, #0x27
08011dbc: 2727       movs      r7, #0x27
08011dbe: 0d11       asrs      r5, r1, #4
08011dc0: 1500       movs      r5, r2
08011dc2: 4948       ldr       r0, [pc, #0x124] ; literal[08011ee8]=200035ec
08011dc4: 90f8c750   ldrb.w    r5, [r0, #0xc7]
08011dc8: 16e0       b         #0x8011df8
08011dca: 4748       ldr       r0, [pc, #0x11c] ; literal[08011ee8]=200035ec
08011dcc: 90f8c950   ldrb.w    r5, [r0, #0xc9]
08011dd0: 12e0       b         #0x8011df8
08011dd2: 4548       ldr       r0, [pc, #0x114] ; literal[08011ee8]=200035ec
08011dd4: 90f8cb50   ldrb.w    r5, [r0, #0xcb]
08011dd8: 0ee0       b         #0x8011df8
08011dda: 00bf       nop       
08011ddc: 00bf       nop       
08011dde: 00bf       nop       
08011de0: 00bf       nop       
08011de2: 00bf       nop       
08011de4: 00bf       nop       
08011de6: 00bf       nop       
08011de8: 00bf       nop       
08011dea: 00bf       nop       
08011dec: 3e48       ldr       r0, [pc, #0xf8] ; literal[08011ee8]=200035ec
08011dee: c630       adds      r0, #0xc6
08011df0: a11d       adds      r1, r4, #6
08011df2: 455c       ldrb      r5, [r0, r1]
08011df4: 00e0       b         #0x8011df8
08011df6: 00bf       nop       
08011df8: 58e0       b         #0x8011eac
08011dfa: 162c       cmp       r4, #0x16
08011dfc: 3cd0       beq       #0x8011e78
08011dfe: 06dc       bgt       #0x8011e0e
08011e00: 0a2c       cmp       r4, #0xa
08011e02: 12d0       beq       #0x8011e2a
08011e04: 0d2c       cmp       r4, #0xd
08011e06: 07d0       beq       #0x8011e18
08011e08: 0f2c       cmp       r4, #0xf
08011e0a: 4dd1       bne       #0x8011ea8
08011e0c: 18e0       b         #0x8011e40
08011e0e: 172c       cmp       r4, #0x17
08011e10: 3ad0       beq       #0x8011e88
08011e12: 182c       cmp       r4, #0x18
08011e14: 48d1       bne       #0x8011ea8
08011e16: 3fe0       b         #0x8011e98
08011e18: fef774fd   bl        #0x8010904
08011e1c: 5022       movs      r2, #0x50
08011e1e: 0421       movs      r1, #4
08011e20: 4ff4dc70   mov.w     r0, #0x1b8
08011e24: 06f012fa   bl        #0x801824c
08011e28: 70bd       pop       {r4, r5, r6, pc}
08011e2a: 3049       ldr       r1, [pc, #0xc0] ; literal[08011eec]=2000805c
08011e2c: 0020       movs      r0, #0
08011e2e: 0df013fa   bl        #0x801f258
08011e32: 6422       movs      r2, #0x64
08011e34: 0221       movs      r1, #2
08011e36: 40f25c60   movw      r0, #0x65c
08011e3a: 06f007fa   bl        #0x801824c
08011e3e: f3e7       b         #0x8011e28
08011e40: 0022       movs      r2, #0
08011e42: 2b49       ldr       r1, [pc, #0xac] ; literal[08011ef0]=000c0005
08011e44: 2b48       ldr       r0, [pc, #0xac] ; literal[08011ef4]=40004400
08011e46: 14f0fdf9   bl        #0x8026244
08011e4a: 0021       movs      r1, #0
08011e4c: 2948       ldr       r0, [pc, #0xa4] ; literal[08011ef4]=40004400
08011e4e: 14f0       .byte     0x14, 0xf0
