08004cca: 3748       ldr       r0, [pc, #0xdc] ; literal[08004da8]=200011f9
08004ccc: 0078       ldrb      r0, [r0]
08004cce: a8b9       cbnz      r0, #0x8004cfc
08004cd0: 2e48       ldr       r0, [pc, #0xb8] ; literal[08004d8c]=200010ec
08004cd2: 0068       ldr       r0, [r0]
08004cd4: c078       ldrb      r0, [r0, #3]
08004cd6: 40b9       cbnz      r0, #0x8004cea
08004cd8: 3448       ldr       r0, [pc, #0xd0] ; literal[08004dac]=2000081d
08004cda: 0078       ldrb      r0, [r0]
08004cdc: 00f0f6f8   bl        #0x8004ecc
08004ce0: 3348       ldr       r0, [pc, #0xcc] ; literal[08004db0]=2000081f
08004ce2: 0078       ldrb      r0, [r0]
08004ce4: 00f03cf9   bl        #0x8004f60
08004ce8: 38e0       b         #0x8004d5c
08004cea: 3248       ldr       r0, [pc, #0xc8] ; literal[08004db4]=2000081e
08004cec: 0078       ldrb      r0, [r0]
08004cee: 00f0edf8   bl        #0x8004ecc
08004cf2: 3148       ldr       r0, [pc, #0xc4] ; literal[08004db8]=20000820
08004cf4: 0078       ldrb      r0, [r0]
08004cf6: 00f033f9   bl        #0x8004f60
08004cfa: 2fe0       b         #0x8004d5c
08004cfc: 2848       ldr       r0, [pc, #0xa0] ; literal[08004da0]=200035ec
08004cfe: 90f8b500   ldrb.w    r0, [r0, #0xb5]
08004d02: 2e49       ldr       r1, [pc, #0xb8] ; literal[08004dbc]=20000800
08004d04: 4870       strb      r0, [r1, #1]
08004d06: 2648       ldr       r0, [pc, #0x98] ; literal[08004da0]=200035ec
08004d08: 90f8b500   ldrb.w    r0, [r0, #0xb5]
08004d0c: 0628       cmp       r0, #6
08004d0e: 05dd       ble       #0x8004d1c
08004d10: 2348       ldr       r0, [pc, #0x8c] ; literal[08004da0]=200035ec
08004d12: 90f8b500   ldrb.w    r0, [r0, #0xb5]
08004d16: 801f       subs      r0, r0, #6
08004d18: 0870       strb      r0, [r1]
08004d1a: 02e0       b         #0x8004d22
08004d1c: 0020       movs      r0, #0
08004d1e: 2749       ldr       r1, [pc, #0x9c] ; literal[08004dbc]=20000800
08004d20: 0870       strb      r0, [r1]
08004d22: 2648       ldr       r0, [pc, #0x98] ; literal[08004dbc]=20000800
08004d24: 0188       ldrh      r1, [r0]
08004d26: 4f20       movs      r0, #0x4f
08004d28: 19f09afc   bl        #0x801e660
08004d2c: 1c48       ldr       r0, [pc, #0x70] ; literal[08004da0]=200035ec
08004d2e: 90f8b400   ldrb.w    r0, [r0, #0xb4]
08004d32: 2249       ldr       r1, [pc, #0x88] ; literal[08004dbc]=20000800
08004d34: 4870       strb      r0, [r1, #1]
08004d36: 1a48       ldr       r0, [pc, #0x68] ; literal[08004da0]=200035ec
08004d38: 90f8b400   ldrb.w    r0, [r0, #0xb4]
08004d3c: 0628       cmp       r0, #6
08004d3e: 05dd       ble       #0x8004d4c
08004d40: 1748       ldr       r0, [pc, #0x5c] ; literal[08004da0]=200035ec
08004d42: 90f8b400   ldrb.w    r0, [r0, #0xb4]
08004d46: 801f       subs      r0, r0, #6
08004d48: 0870       strb      r0, [r1]
08004d4a: 02e0       b         #0x8004d52
08004d4c: 0020       movs      r0, #0
08004d4e: 1b49       ldr       r1, [pc, #0x6c] ; literal[08004dbc]=20000800
08004d50: 0870       strb      r0, [r1]
08004d52: 1a48       ldr       r0, [pc, #0x68] ; literal[08004dbc]=20000800
08004d54: 0188       ldrh      r1, [r0]
08004d56: 7820       movs      r0, #0x78
08004d58: 19f082fc   bl        #0x801e660
