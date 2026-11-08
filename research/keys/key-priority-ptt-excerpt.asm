; Bounded static excerpt; literal pools may decode as instructions.
; Reference SHA256: eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
08007980: 10b5       push      {r4, lr}
08007982: 0bf07ffb   bl        #0x8013084
08007986: 5d49       ldr       r1, [pc, #0x174] ; literal[08007afc]=20001107
08007988: 4870       strb      r0, [r1, #1]
0800798a: 0846       mov       r0, r1
0800798c: 4078       ldrb      r0, [r0, #1]
0800798e: ff28       cmp       r0, #0xff
08007990: 03d1       bne       #0x800799a
08007992: 0cf0e9f9   bl        #0x8013d68
08007996: 5949       ldr       r1, [pc, #0x164] ; literal[08007afc]=20001107
08007998: 4870       strb      r0, [r1, #1]
0800799a: 5948       ldr       r0, [pc, #0x164] ; literal[08007b00]=200035ec
0800799c: 90f87300   ldrb.w    r0, [r0, #0x73]
080079a0: 40b1       cbz       r0, #0x80079b4
080079a2: 5648       ldr       r0, [pc, #0x158] ; literal[08007afc]=20001107
080079a4: 4078       ldrb      r0, [r0, #1]
080079a6: 1628       cmp       r0, #0x16
080079a8: 03d0       beq       #0x80079b2
080079aa: 5448       ldr       r0, [pc, #0x150] ; literal[08007afc]=20001107
080079ac: 4078       ldrb      r0, [r0, #1]
080079ae: 1728       cmp       r0, #0x17
080079b0: 00d1       bne       #0x80079b4
080079b2: 10bd       pop       {r4, pc}
080079b4: 4021       movs      r1, #0x40
080079b6: 5348       ldr       r0, [pc, #0x14c] ; literal[08007b04]=40020400
080079b8: 1df091fd   bl        #0x80254de
080079bc: 0028       cmp       r0, #0
080079be: 34d1       bne       #0x8007a2a
080079c0: 5148       ldr       r0, [pc, #0x144] ; literal[08007b08]=20001125
080079c2: 0078       ldrb      r0, [r0]
080079c4: 0328       cmp       r0, #3
080079c6: 24d1       bne       #0x8007a12
