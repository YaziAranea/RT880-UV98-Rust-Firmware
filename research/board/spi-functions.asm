08025a4c: 70b5       push      {r4, r5, r6, lr}
08025a4e: 0446       mov       r4, r0
08025a50: 0021       movs      r1, #0
08025a52: 0f48       ldr       r0, [pc, #0x3c] ; literal[08025a90]=40003800
08025a54: 00f086f8   bl        #0x8025b64
08025a58: 0021       movs      r1, #0
08025a5a: 0d48       ldr       r0, [pc, #0x34] ; literal[08025a90]=40003800
08025a5c: 00f07df8   bl        #0x8025b5a
08025a60: 2146       mov       r1, r4
08025a62: 0b48       ldr       r0, [pc, #0x2c] ; literal[08025a90]=40003800
08025a64: 00f077f8   bl        #0x8025b56
08025a68: 00bf       nop       
08025a6a: 0121       movs      r1, #1
08025a6c: 0848       ldr       r0, [pc, #0x20] ; literal[08025a90]=40003800
08025a6e: 00f07ef8   bl        #0x8025b6e
08025a72: 0028       cmp       r0, #0
08025a74: f9d0       beq       #0x8025a6a
08025a76: 0648       ldr       r0, [pc, #0x18] ; literal[08025a90]=40003800
08025a78: 00f069f8   bl        #0x8025b4e
08025a7c: c5b2       uxtb      r5, r0
08025a7e: 00bf       nop       
08025a80: 8021       movs      r1, #0x80
08025a82: 0348       ldr       r0, [pc, #0xc] ; literal[08025a90]=40003800
08025a84: 00f073f8   bl        #0x8025b6e
08025a88: 0028       cmp       r0, #0
08025a8a: f9d1       bne       #0x8025a80
08025a8c: 2846       mov       r0, r5
08025a8e: 70bd       pop       {r4, r5, r6, pc}
08025a90: 0038       subs      r0, #0
08025a92: 0040       ands      r0, r0
08025a94: 70b5       push      {r4, r5, r6, lr}
08025a96: 0446       mov       r4, r0
08025a98: 0d46       mov       r5, r1
08025a9a: a526       movs      r6, #0xa5
08025a9c: 16e0       b         #0x8025acc
08025a9e: 00bf       nop       
08025aa0: 0221       movs      r1, #2
08025aa2: 0d48       ldr       r0, [pc, #0x34] ; literal[08025ad8]=40003800
08025aa4: 00f063f8   bl        #0x8025b6e
08025aa8: 0028       cmp       r0, #0
08025aaa: f9d0       beq       #0x8025aa0
08025aac: 3146       mov       r1, r6
08025aae: 0a48       ldr       r0, [pc, #0x28] ; literal[08025ad8]=40003800
08025ab0: 00f051f8   bl        #0x8025b56
08025ab4: 00bf       nop       
08025ab6: 0121       movs      r1, #1
08025ab8: 0748       ldr       r0, [pc, #0x1c] ; literal[08025ad8]=40003800
08025aba: 00f058f8   bl        #0x8025b6e
08025abe: 0028       cmp       r0, #0
08025ac0: f9d0       beq       #0x8025ab6
08025ac2: 0548       ldr       r0, [pc, #0x14] ; literal[08025ad8]=40003800
08025ac4: 00f043f8   bl        #0x8025b4e
08025ac8: 2070       strb      r0, [r4]
08025aca: 641c       adds      r4, r4, #1
08025acc: 281e       subs      r0, r5, #0
08025ace: a5f10105   sub.w     r5, r5, #1
08025ad2: e4d1       bne       #0x8025a9e
08025ad4: 70bd       pop       {r4, r5, r6, pc}
08025ad6: 0000       movs      r0, r0
08025ad8: 0038       subs      r0, #0
08025ada: 0040       ands      r0, r0
08025adc: 38b5       push      {r3, r4, r5, lr}
08025ade: 0446       mov       r4, r0
08025ae0: 0d46       mov       r5, r1
08025ae2: 17e0       b         #0x8025b14
08025ae4: 00bf       nop       
08025ae6: 0221       movs      r1, #2
08025ae8: 0d48       ldr       r0, [pc, #0x34] ; literal[08025b20]=40003800
08025aea: 00f040f8   bl        #0x8025b6e
08025aee: 0028       cmp       r0, #0
08025af0: f9d0       beq       #0x8025ae6
08025af2: 2178       ldrb      r1, [r4]
08025af4: 0a48       ldr       r0, [pc, #0x28] ; literal[08025b20]=40003800
08025af6: 00f02ef8   bl        #0x8025b56
08025afa: 00bf       nop       
08025afc: 0121       movs      r1, #1
08025afe: 0848       ldr       r0, [pc, #0x20] ; literal[08025b20]=40003800
08025b00: 00f035f8   bl        #0x8025b6e
08025b04: 0028       cmp       r0, #0
08025b06: f9d0       beq       #0x8025afc
08025b08: 0548       ldr       r0, [pc, #0x14] ; literal[08025b20]=40003800
08025b0a: 00f020f8   bl        #0x8025b4e
08025b0e: c0b2       uxtb      r0, r0
08025b10: 0090       str       r0, [sp]
08025b12: 641c       adds      r4, r4, #1
08025b14: 281e       subs      r0, r5, #0
08025b16: a5f10105   sub.w     r5, r5, #1
08025b1a: e3d1       bne       #0x8025ae4
08025b1c: 38bd       pop       {r3, r4, r5, pc}
08025b1e: 0000       movs      r0, r0
08025b20: 0038       subs      r0, #0
08025b22: 0040       ands      r0, r0
08025b24: 0021       movs      r1, #0
08025b26: 0170       strb      r1, [r0]
08025b28: 4170       strb      r1, [r0, #1]
08025b2a: 8170       strb      r1, [r0, #2]
08025b2c: c170       strb      r1, [r0, #3]
08025b2e: 0171       strb      r1, [r0, #4]
08025b30: 4171       strb      r1, [r0, #5]
08025b32: 8171       strb      r1, [r0, #6]
08025b34: 0121       movs      r1, #1
08025b36: c171       strb      r1, [r0, #7]
08025b38: 7047       bx        lr
08025b3a: 0268       ldr       r2, [r0]
08025b3c: 61f38612   bfi       r2, r1, #6, #1
08025b40: 0260       str       r2, [r0]
08025b42: 7047       bx        lr
08025b44: 4268       ldr       r2, [r0, #4]
08025b46: 61f38202   bfi       r2, r1, #2, #1
08025b4a: 4260       str       r2, [r0, #4]
08025b4c: 7047       bx        lr
08025b4e: 0146       mov       r1, r0
08025b50: c868       ldr       r0, [r1, #0xc]
08025b52: 80b2       uxth      r0, r0
08025b54: 7047       bx        lr
08025b56: c160       str       r1, [r0, #0xc]
08025b58: 7047       bx        lr
08025b5a: 4268       ldr       r2, [r0, #4]
08025b5c: 61f30002   bfi       r2, r1, #0, #1
08025b60: 4260       str       r2, [r0, #4]
08025b62: 7047       bx        lr
08025b64: 4268       ldr       r2, [r0, #4]
08025b66: 61f34102   bfi       r2, r1, #1, #1
08025b6a: 4260       str       r2, [r0, #4]
08025b6c: 7047       bx        lr
08025b6e: 0246       mov       r2, r0
08025b70: 0020       movs      r0, #0
08025b72: 9368       ldr       r3, [r2, #8]
08025b74: 0b40       ands      r3, r1
08025b76: 03b9       cbnz      r3, #0x8025b7a
08025b78: 00e0       b         #0x8025b7c
08025b7a: 0120       movs      r0, #1
08025b7c: 7047       bx        lr
08025b7e: c269       ldr       r2, [r0, #0x1c]
08025b80: 22f40062   bic       r2, r2, #0x800
08025b84: c261       str       r2, [r0, #0x1c]
08025b86: 0a78       ldrb      r2, [r1]
08025b88: 62b9       cbnz      r2, #0x8025ba4
08025b8a: 0268       ldr       r2, [r0]
08025b8c: 22f40042   bic       r2, r2, #0x8000
08025b90: 0260       str       r2, [r0]
08025b92: 0268       ldr       r2, [r0]
08025b94: 22f48042   bic       r2, r2, #0x4000
08025b98: 0260       str       r2, [r0]
08025b9a: 0268       ldr       r2, [r0]
08025b9c: 22f48062   bic       r2, r2, #0x400
08025ba0: 0260       str       r2, [r0]
08025ba2: 36e0       b         #0x8025c12
08025ba4: 0a78       ldrb      r2, [r1]
08025ba6: 012a       cmp       r2, #1
08025ba8: 0ed1       bne       #0x8025bc8
08025baa: 0268       ldr       r2, [r0]
08025bac: 22f40042   bic       r2, r2, #0x8000
08025bb0: 0260       str       r2, [r0]
08025bb2: 0268       ldr       r2, [r0]
08025bb4: 22f48042   bic       r2, r2, #0x4000
08025bb8: 0260       str       r2, [r0]
08025bba: 0268       ldr       r2, [r0]
08025bbc: 22f48062   bic       r2, r2, #0x400
08025bc0: 02f58062   add.w     r2, r2, #0x400
08025bc4: 0260       str       r2, [r0]
08025bc6: 24e0       b         #0x8025c12
08025bc8: 0a78       ldrb      r2, [r1]
08025bca: 022a       cmp       r2, #2
08025bcc: 0ed1       bne       #0x8025bec
08025bce: 0268       ldr       r2, [r0]
08025bd0: 22f40042   bic       r2, r2, #0x8000
08025bd4: 02f50042   add.w     r2, r2, #0x8000
08025bd8: 0260       str       r2, [r0]
08025bda: 0268       ldr       r2, [r0]
08025bdc: 22f48042   bic       r2, r2, #0x4000
08025be0: 0260       str       r2, [r0]
08025be2: 0268       ldr       r2, [r0]
08025be4: 22f48062   bic       r2, r2, #0x400
08025be8: 0260       str       r2, [r0]
08025bea: 12e0       b         #0x8025c12
08025bec: 0a78       ldrb      r2, [r1]
08025bee: 032a       cmp       r2, #3
08025bf0: 0fd1       bne       #0x8025c12
08025bf2: 0268       ldr       r2, [r0]
08025bf4: 22f40042   bic       r2, r2, #0x8000
08025bf8: 02f50042   add.w     r2, r2, #0x8000
08025bfc: 0260       str       r2, [r0]
08025bfe: 0268       ldr       r2, [r0]
08025c00: 22f48042   bic       r2, r2, #0x4000
08025c04: 02f58042   add.w     r2, r2, #0x4000
08025c08: 0260       str       r2, [r0]
08025c0a: 0268       ldr       r2, [r0]
08025c0c: 22f48062   bic       r2, r2, #0x400
08025c10: 0260       str       r2, [r0]
08025c12: cb79       ldrb      r3, [r1, #7]
08025c14: 0268       ldr       r2, [r0]
08025c16: 63f34922   bfi       r2, r3, #9, #1
08025c1a: 0260       str       r2, [r0]
08025c1c: 4a78       ldrb      r2, [r1, #1]
08025c1e: 012a       cmp       r2, #1
08025c20: 09d1       bne       #0x8025c36
08025c22: ca79       ldrb      r2, [r1, #7]
08025c24: 012a       cmp       r2, #1
08025c26: 06d1       bne       #0x8025c36
08025c28: 0268       ldr       r2, [r0]
08025c2a: 22f48072   bic       r2, r2, #0x100
08025c2e: 02f58072   add.w     r2, r2, #0x100
08025c32: 0260       str       r2, [r0]
08025c34: 03e0       b         #0x8025c3e
08025c36: 0268       ldr       r2, [r0]
08025c38: 22f48072   bic       r2, r2, #0x100
08025c3c: 0260       str       r2, [r0]
08025c3e: 4b78       ldrb      r3, [r1, #1]
08025c40: 0268       ldr       r2, [r0]
08025c42: 63f38202   bfi       r2, r3, #2, #1
08025c46: 0260       str       r2, [r0]
08025c48: 8a78       ldrb      r2, [r1, #2]
08025c4a: 072a       cmp       r2, #7
08025c4c: 0ddc       bgt       #0x8025c6a
08025c4e: 4268       ldr       r2, [r0, #4]
08025c50: 22f40072   bic       r2, r2, #0x200
08025c54: 4260       str       r2, [r0, #4]
08025c56: 4268       ldr       r2, [r0, #4]
08025c58: 22f48072   bic       r2, r2, #0x100
08025c5c: 4260       str       r2, [r0, #4]
08025c5e: 8b78       ldrb      r3, [r1, #2]
08025c60: 0268       ldr       r2, [r0]
08025c62: 63f3c502   bfi       r2, r3, #3, #3
08025c66: 0260       str       r2, [r0]
08025c68: 20e0       b         #0x8025cac
08025c6a: 8a78       ldrb      r2, [r1, #2]
08025c6c: 0a2a       cmp       r2, #0xa
08025c6e: 0ed1       bne       #0x8025c8e
08025c70: 4268       ldr       r2, [r0, #4]
08025c72: 22f40072   bic       r2, r2, #0x200
08025c76: 02f50072   add.w     r2, r2, #0x200
08025c7a: 4260       str       r2, [r0, #4]
08025c7c: 4268       ldr       r2, [r0, #4]
08025c7e: 22f48072   bic       r2, r2, #0x100
08025c82: 4260       str       r2, [r0, #4]
08025c84: 0268       ldr       r2, [r0]
08025c86: 22f03802   bic       r2, r2, #0x38
08025c8a: 0260       str       r2, [r0]
08025c8c: 0ee0       b         #0x8025cac
08025c8e: 4268       ldr       r2, [r0, #4]
08025c90: 22f40072   bic       r2, r2, #0x200
08025c94: 4260       str       r2, [r0, #4]
08025c96: 4268       ldr       r2, [r0, #4]
08025c98: 22f48072   bic       r2, r2, #0x100
08025c9c: 02f58072   add.w     r2, r2, #0x100
08025ca0: 4260       str       r2, [r0, #4]
08025ca2: 8a78       ldrb      r2, [r1, #2]
08025ca4: 0368       ldr       r3, [r0]
08025ca6: 62f3c503   bfi       r3, r2, #3, #3
08025caa: 0360       str       r3, [r0]
08025cac: cb78       ldrb      r3, [r1, #3]
08025cae: 0268       ldr       r2, [r0]
08025cb0: 63f3c712   bfi       r2, r3, #7, #1
08025cb4: 0260       str       r2, [r0]
08025cb6: 0b79       ldrb      r3, [r1, #4]
08025cb8: 0268       ldr       r2, [r0]
08025cba: 63f3cb22   bfi       r2, r3, #0xb, #1
08025cbe: 0260       str       r2, [r0]
08025cc0: 4b79       ldrb      r3, [r1, #5]
08025cc2: 0268       ldr       r2, [r0]
08025cc4: 63f34102   bfi       r2, r3, #1, #1
08025cc8: 0260       str       r2, [r0]
08025cca: 8b79       ldrb      r3, [r1, #6]
08025ccc: 0268       ldr       r2, [r0]
08025cce: 63f30002   bfi       r2, r3, #0, #1
08025cd2: 0260       str       r2, [r0]
08025cd4: 7047       bx        lr
08025cd6: 0000       movs      r0, r0
08025cd8: 70b5       push      {r4, r5, r6, lr}
08025cda: 0646       mov       r6, r0
08025cdc: 0c46       mov       r4, r1
08025cde: 1546       mov       r5, r2
08025ce0: 002d       cmp       r5, #0
08025ce2: 21d9       bls       #0x8025d28
08025ce4: b5f5807f   cmp.w     r5, #0x100
08025ce8: 1ed8       bhi       #0x8025d28
08025cea: 00f00df9   bl        #0x8025f08
08025cee: 4ff48050   mov.w     r0, #0x1000
08025cf2: 0e49       ldr       r1, [pc, #0x38] ; literal[08025d2c]=40020428
08025cf4: 0860       str       r0, [r1]
08025cf6: 0220       movs      r0, #2
08025cf8: fff7a8fe   bl        #0x8025a4c
08025cfc: c4f30740   ubfx      r0, r4, #0x10, #8
08025d00: fff7a4fe   bl        #0x8025a4c
08025d04: c4f30720   ubfx      r0, r4, #8, #8
08025d08: fff7a0fe   bl        #0x8025a4c
08025d0c: e0b2       uxtb      r0, r4
08025d0e: fff79dfe   bl        #0x8025a4c
08025d12: 2946       mov       r1, r5
08025d14: 3046       mov       r0, r6
08025d16: fff7e1fe   bl        #0x8025adc
08025d1a: 4ff48050   mov.w     r0, #0x1000
08025d1e: 0349       ldr       r1, [pc, #0xc] ; literal[08025d2c]=40020428
08025d20: 1039       subs      r1, #0x10
08025d22: 0860       str       r0, [r1]
08025d24: 00f082f8   bl        #0x8025e2c
08025d28: 70bd       pop       {r4, r5, r6, pc}
08025d2a: 0000       movs      r0, r0
08025d2c: 2804       lsls      r0, r5, #0x10
08025d2e: 0240       ands      r2, r0
