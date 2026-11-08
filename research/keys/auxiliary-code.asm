; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
08013d68: 0c48       ldr       r0, [pc, #0x30] ; literal[08013d9c]=200010e4
08013d6a: 4088       ldrh      r0, [r0, #2]
08013d6c: 40f25d71   movw      r1, #0x75d
08013d70: 8842       cmp       r0, r1
08013d72: 01da       bge       #0x8013d78
08013d74: 1720       movs      r0, #0x17
08013d76: 7047       bx        lr
08013d78: 0848       ldr       r0, [pc, #0x20] ; literal[08013d9c]=200010e4
08013d7a: 4088       ldrh      r0, [r0, #2]
08013d7c: 40f69d21   movw      r1, #0xa9d
08013d80: 8842       cmp       r0, r1
08013d82: 01da       bge       #0x8013d88
08013d84: 1620       movs      r0, #0x16
08013d86: f6e7       b         #0x8013d76
08013d88: 0448       ldr       r0, [pc, #0x10] ; literal[08013d9c]=200010e4
08013d8a: 4088       ldrh      r0, [r0, #2]
08013d8c: 40f6d651   movw      r1, #0xdd6
08013d90: 8842       cmp       r0, r1
08013d92: 01da       bge       #0x8013d98
08013d94: 1820       movs      r0, #0x18
08013d96: eee7       b         #0x8013d76
08013d98: ff20       movs      r0, #0xff
08013d9a: ece7       b         #0x8013d76
08013d9c: e410       asrs      r4, r4, #3
08013d9e: 0020       movs      r0, #0
