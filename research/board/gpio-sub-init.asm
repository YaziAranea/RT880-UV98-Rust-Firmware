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
