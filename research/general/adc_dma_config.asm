0800822c: 00b5       push      {lr}
0800822e: 85b0       sub       sp, #0x14
08008230: 1c48       ldr       r0, [pc, #0x70] ; literal[080082a4]=40026008
08008232: 1df0c9f8   bl        #0x80253c8
08008236: 6846       mov       r0, sp
08008238: 1df059f8   bl        #0x80252ee
0800823c: 0220       movs      r0, #2
0800823e: adf80a00   strh.w    r0, [sp, #0xa]
08008242: 0020       movs      r0, #0
08008244: adf80800   strh.w    r0, [sp, #8]
08008248: 1748       ldr       r0, [pc, #0x5c] ; literal[080082a8]=200010cc
0800824a: 0190       str       r0, [sp, #4]
0800824c: 0120       movs      r0, #1
0800824e: 8df80f00   strb.w    r0, [sp, #0xf]
08008252: 8df80d00   strb.w    r0, [sp, #0xd]
08008256: 1548       ldr       r0, [pc, #0x54] ; literal[080082ac]=4001204c
08008258: 0090       str       r0, [sp]
0800825a: 0120       movs      r0, #1
0800825c: 8df80e00   strb.w    r0, [sp, #0xe]
08008260: 0020       movs      r0, #0
08008262: 8df80c00   strb.w    r0, [sp, #0xc]
08008266: 0220       movs      r0, #2
08008268: 8df81100   strb.w    r0, [sp, #0x11]
0800826c: 0120       movs      r0, #1
0800826e: 8df81000   strb.w    r0, [sp, #0x10]
08008272: 6946       mov       r1, sp
08008274: 0b48       ldr       r0, [pc, #0x2c] ; literal[080082a4]=40026008
08008276: 1df06df8   bl        #0x8025354
0800827a: 0121       movs      r1, #1
0800827c: 0948       ldr       r0, [pc, #0x24] ; literal[080082a4]=40026008
0800827e: 0838       subs      r0, #8
08008280: 1df0daf8   bl        #0x8025438
08008284: 0521       movs      r1, #5
08008286: 0748       ldr       r0, [pc, #0x1c] ; literal[080082a4]=40026008
08008288: fc30       adds      r0, #0xfc
0800828a: 1df0dcf8   bl        #0x8025446
0800828e: 0022       movs      r2, #0
08008290: 0221       movs      r1, #2
08008292: 0448       ldr       r0, [pc, #0x10] ; literal[080082a4]=40026008
08008294: 1df08ff8   bl        #0x80253b6
08008298: 0121       movs      r1, #1
0800829a: 0248       ldr       r0, [pc, #8] ; literal[080082a4]=40026008
0800829c: 1df022f8   bl        #0x80252e4
080082a0: 05b0       add       sp, #0x14
080082a2: 00bd       pop       {pc}
080082a4: 0860       str       r0, [r1]
080082a6: 0240       ands      r2, r0
080082a8: cc10       asrs      r4, r1, #3
080082aa: 0020       movs      r0, #0
080082ac: 4c20       movs      r0, #0x4c
080082ae: 0140       ands      r1, r0
