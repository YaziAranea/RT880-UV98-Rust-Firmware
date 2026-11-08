08003a08: 10b5       push      {r4, lr}
08003a0a: 0446       mov       r4, r0
08003a0c: 46f24000   movw      r0, #0x6040
08003a10: 0549       ldr       r1, [pc, #0x14] ; literal[08003a28]=20000800
08003a12: 0880       strh      r0, [r1]
08003a14: 0846       mov       r0, r1
08003a16: 4078       ldrb      r0, [r0, #1]
08003a18: 2043       orrs      r0, r4
08003a1a: 4870       strb      r0, [r1, #1]
08003a1c: 0846       mov       r0, r1
08003a1e: 0188       ldrh      r1, [r0]
08003a20: 4720       movs      r0, #0x47
08003a22: 1af057fe   bl        #0x801e6d4
08003a26: 10bd       pop       {r4, pc}
08003a28: 0008       lsrs      r0, r0, #0x20
08003a2a: 0020       movs      r0, #0
08003a2c: 2de9f041   push.w    {r4, r5, r6, r7, r8, lr}
08003a30: 0646       mov       r6, r0
08003a32: 0d46       mov       r5, r1
08003a34: 1446       mov       r4, r2
08003a36: 0027       movs      r7, #0
08003a38: b846       mov       r8, r7
08003a3a: c5b1       cbz       r5, #0x8003a6e
08003a3c: 4ff20b57   movw      r7, #0xf50b
08003a40: 052c       cmp       r4, #5
08003a42: 13d2       bhs       #0x8003a6c
08003a44: dfe804f0   tbb       [pc, r4]
08003a48: 0306       lsls      r3, r0, #0x18
08003a4a: 090c       lsrs      r1, r1, #0x10
08003a4c: 0f00       movs      r7, r1
08003a4e: 4ef21c67   movw      r7, #0xe61c
08003a52: 0be0       b         #0x8003a6c
08003a54: 4df62277   movw      r7, #0xdf22
08003a58: 08e0       b         #0x8003a6c
08003a5a: 4df22d47   movw      r7, #0xd42d
08003a5e: 05e0       b         #0x8003a6c
08003a60: 4cf63547   movw      r7, #0xcc35
08003a64: 02e0       b         #0x8003a6c
08003a66: 4ff60227   movw      r7, #0xfa02
08003a6a: 00bf       nop       
08003a6c: 2fe0       b         #0x8003ace
08003a6e: 48f6e337   movw      r7, #0x8be3
08003a72: 43f22d58   movw      r8, #0x352d
08003a76: 072c       cmp       r4, #7
08003a78: 28d2       bhs       #0x8003acc
08003a7a: dfe804f0   tbb       [pc, r4]
08003a7e: 0409       lsrs      r4, r0, #4
08003a80: 0e13       asrs      r6, r1, #0xc
08003a82: 181d       adds      r0, r3, #4
08003a84: 2200       movs      r2, r4
08003a86: 48f6d737   movw      r7, #0x8bd7
08003a8a: 43f20758   movw      r8, #0x3507
08003a8e: 1de0       b         #0x8003acc
08003a90: 48f6ed27   movw      r7, #0x8aed
08003a94: 43f2e158   movw      r8, #0x35e1
08003a98: 18e0       b         #0x8003acc
08003a9a: 48f69627   movw      r7, #0x8a96
08003a9e: 43f20668   movw      r8, #0x3606
08003aa2: 13e0       b         #0x8003acc
08003aa4: 48f60817   movw      r7, #0x8908
08003aa8: 43f27978   movw      r8, #0x3779
08003aac: 0ee0       b         #0x8003acc
08003aae: 48f6de57   movw      r7, #0x8dde
08003ab2: 43f25a38   movw      r8, #0x335a
08003ab6: 09e0       b         #0x8003acc
08003ab8: 48f6b367   movw      r7, #0x8eb3
08003abc: 43f2ba28   movw      r8, #0x32ba
08003ac0: 04e0       b         #0x8003acc
08003ac2: 48f68877   movw      r7, #0x8f88
08003ac6: 43f20128   movw      r8, #0x3201
08003aca: 00bf       nop       
08003acc: 00bf       nop       
08003ace: 8eb1       cbz       r6, #0x8003af4
08003ad0: 35b1       cbz       r5, #0x8003ae0
08003ad2: 012d       cmp       r5, #1
08003ad4: 0dd1       bne       #0x8003af2
08003ad6: 3946       mov       r1, r7
08003ad8: 7420       movs      r0, #0x74
08003ada: 1af0fbfd   bl        #0x801e6d4
08003ade: 08e0       b         #0x8003af2
08003ae0: 3946       mov       r1, r7
08003ae2: 4420       movs      r0, #0x44
08003ae4: 1af0f6fd   bl        #0x801e6d4
08003ae8: 4146       mov       r1, r8
08003aea: 4520       movs      r0, #0x45
08003aec: 1af0f2fd   bl        #0x801e6d4
08003af0: 00bf       nop       
08003af2: 11e0       b         #0x8003b18
08003af4: 35b1       cbz       r5, #0x8003b04
08003af6: 012d       cmp       r5, #1
08003af8: 0dd1       bne       #0x8003b16
08003afa: 3946       mov       r1, r7
08003afc: 7520       movs      r0, #0x75
08003afe: 1af0e9fd   bl        #0x801e6d4
08003b02: 08e0       b         #0x8003b16
08003b04: 3946       mov       r1, r7
08003b06: 5420       movs      r0, #0x54
08003b08: 1af0e4fd   bl        #0x801e6d4
08003b0c: 4146       mov       r1, r8
08003b0e: 5520       movs      r0, #0x55
08003b10: 1af0e0fd   bl        #0x801e6d4
08003b14: 00bf       nop       
08003b16: 00bf       nop       
08003b18: bde8f081   pop.w     {r4, r5, r6, r7, r8, pc}
