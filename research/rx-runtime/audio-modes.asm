080038ac: 10b5       push      {r4, lr}
080038ae: 0446       mov       r4, r0
080038b0: 46f24000   movw      r0, #0x6040
080038b4: 0549       ldr       r1, [pc, #0x14] ; literal[080038cc]=20000800
080038b6: 0880       strh      r0, [r1]
080038b8: 0846       mov       r0, r1
080038ba: 4078       ldrb      r0, [r0, #1]
080038bc: 2043       orrs      r0, r4
080038be: 4870       strb      r0, [r1, #1]
080038c0: 0846       mov       r0, r1
080038c2: 0188       ldrh      r1, [r0]
080038c4: 4720       movs      r0, #0x47
080038c6: 1af0cbfe   bl        #0x801e660
080038ca: 10bd       pop       {r4, pc}
080038cc: 0008       lsrs      r0, r0, #0x20
080038ce: 0020       movs      r0, #0

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
