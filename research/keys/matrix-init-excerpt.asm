; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
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
