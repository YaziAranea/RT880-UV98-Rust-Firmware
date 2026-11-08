; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
080254de: 0246       mov       r2, r0
080254e0: 0020       movs      r0, #0
080254e2: 1369       ldr       r3, [r2, #0x10]
080254e4: 0b40       ands      r3, r1
080254e6: 8b42       cmp       r3, r1
080254e8: 00d0       beq       #0x80254ec
080254ea: 00e0       b         #0x80254ee
080254ec: 0120       movs      r0, #1
080254ee: 7047       bx        lr
