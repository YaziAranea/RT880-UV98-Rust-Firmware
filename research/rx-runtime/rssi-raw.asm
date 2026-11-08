08004f40: 10b5       push      {r4, lr}
08004f42: 6720       movs      r0, #0x67
08004f44: 19f06cfb   bl        #0x801e620
08004f48: 0449       ldr       r1, [pc, #0x10] ; literal[08004f5c]=20000800
08004f4a: 0880       strh      r0, [r1]
08004f4c: 0846       mov       r0, r1
08004f4e: 0088       ldrh      r0, [r0]
08004f50: c0f30800   ubfx      r0, r0, #0, #9
08004f54: 0880       strh      r0, [r1]
08004f56: 0846       mov       r0, r1
08004f58: 0088       ldrh      r0, [r0]
08004f5a: 10bd       pop       {r4, pc}
08004f5c: 0008       lsrs      r0, r0, #0x20
08004f5e: 0020       movs      r0, #0
