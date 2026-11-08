08012a30: 2de9f041   push.w    {r4, r5, r6, r7, r8, lr}
08012a34: 0446       mov       r4, r0
08012a36: 0f46       mov       r7, r1
08012a38: d248       ldr       r0, [pc, #0x348] ; literal[08012d84]=00cf8500
08012a3a: 8442       cmp       r4, r0
08012a3c: 11d3       blo       #0x8012a62
08012a3e: d248       ldr       r0, [pc, #0x348] ; literal[08012d88]=010980c0
08012a40: 8442       cmp       r4, r0
08012a42: 0ed8       bhi       #0x8012a62
08012a44: 0026       movs      r6, #0
08012a46: d148       ldr       r0, [pc, #0x344] ; literal[08012d8c]=00cdfe60
08012a48: 201a       subs      r0, r4, r0
08012a4a: d149       ldr       r1, [pc, #0x344] ; literal[08012d90]=0007a120
08012a4c: b0fbf1f0   udiv      r0, r0, r1
08012a50: c5b2       uxtb      r5, r0
08012a52: 17b9       cbnz      r7, #0x8012a5a
08012a54: 0320       movs      r0, #3
08012a56: cf49       ldr       r1, [pc, #0x33c] ; literal[08012d94]=20000839
08012a58: 0870       strb      r0, [r1]
08012a5a: 0320       movs      r0, #3
08012a5c: ce49       ldr       r1, [pc, #0x338] ; literal[08012d98]=20000838
08012a5e: 0870       strb      r0, [r1]
08012a60: 3ee1       b         #0x8012ce0
08012a62: ce48       ldr       r0, [pc, #0x338] ; literal[08012d9c]=02625a00
08012a64: 8442       cmp       r4, r0
08012a66: 0fd3       blo       #0x8012a88
08012a68: cd48       ldr       r0, [pc, #0x334] ; literal[08012da0]=02dc6c00
08012a6a: 8442       cmp       r4, r0
08012a6c: 0cd8       bhi       #0x8012a88
08012a6e: 0126       movs      r6, #1
08012a70: ca48       ldr       r0, [pc, #0x328] ; literal[08012d9c]=02625a00
08012a72: 201a       subs      r0, r4, r0
08012a74: c649       ldr       r1, [pc, #0x318] ; literal[08012d90]=0007a120
08012a76: b0fbf1f0   udiv      r0, r0, r1
08012a7a: c5b2       uxtb      r5, r0
08012a7c: 0420       movs      r0, #4
08012a7e: c549       ldr       r1, [pc, #0x314] ; literal[08012d94]=20000839
08012a80: 0870       strb      r0, [r1]
08012a82: c549       ldr       r1, [pc, #0x314] ; literal[08012d98]=20000838
08012a84: 0870       strb      r0, [r1]
08012a86: 2be1       b         #0x8012ce0
08012a88: c648       ldr       r0, [pc, #0x318] ; literal[08012da4]=001b7740
08012a8a: 8442       cmp       r4, r0
08012a8c: 1fd3       blo       #0x8012ace
08012a8e: c648       ldr       r0, [pc, #0x318] ; literal[08012da8]=0061a800
08012a90: 8442       cmp       r4, r0
08012a92: 1cd8       bhi       #0x8012ace
08012a94: 0226       movs      r6, #2
08012a96: c548       ldr       r0, [pc, #0x314] ; literal[08012dac]=0016e360
08012a98: 201a       subs      r0, r4, r0
08012a9a: bd49       ldr       r1, [pc, #0x2f4] ; literal[08012d90]=0007a120
08012a9c: b0fbf1f0   udiv      r0, r0, r1
08012aa0: c5b2       uxtb      r5, r0
08012aa2: 4fb9       cbnz      r7, #0x8012ab8
08012aa4: c248       ldr       r0, [pc, #0x308] ; literal[08012db0]=0044aa20
08012aa6: 8442       cmp       r4, r0
08012aa8: 03d2       bhs       #0x8012ab2
08012aaa: 0120       movs      r0, #1
08012aac: b949       ldr       r1, [pc, #0x2e4] ; literal[08012d94]=20000839
08012aae: 0870       strb      r0, [r1]
08012ab0: 02e0       b         #0x8012ab8
08012ab2: 0220       movs      r0, #2
08012ab4: b749       ldr       r1, [pc, #0x2dc] ; literal[08012d94]=20000839
08012ab6: 0870       strb      r0, [r1]
08012ab8: be48       ldr       r0, [pc, #0x2f8] ; literal[08012db4]=0030d400
08012aba: 8442       cmp       r4, r0
08012abc: 03d2       bhs       #0x8012ac6
08012abe: 0120       movs      r0, #1
08012ac0: b549       ldr       r1, [pc, #0x2d4] ; literal[08012d98]=20000838
08012ac2: 0870       strb      r0, [r1]
08012ac4: 0ce1       b         #0x8012ce0
08012ac6: 0320       movs      r0, #3
08012ac8: b349       ldr       r1, [pc, #0x2cc] ; literal[08012d98]=20000838
08012aca: 0870       strb      r0, [r1]
08012acc: 08e1       b         #0x8012ce0
08012ace: b648       ldr       r0, [pc, #0x2d8] ; literal[08012da8]=0061a800
08012ad0: 8442       cmp       r4, r0
08012ad2: 11d3       blo       #0x8012af8
08012ad4: ab48       ldr       r0, [pc, #0x2ac] ; literal[08012d84]=00cf8500
08012ad6: 8442       cmp       r4, r0
08012ad8: 0ed8       bhi       #0x8012af8
08012ada: 0326       movs      r6, #3
08012adc: b648       ldr       r0, [pc, #0x2d8] ; literal[08012db8]=005b8d80
08012ade: 201a       subs      r0, r4, r0
08012ae0: ab49       ldr       r1, [pc, #0x2ac] ; literal[08012d90]=0007a120
08012ae2: b0fbf1f0   udiv      r0, r0, r1
08012ae6: c5b2       uxtb      r5, r0
08012ae8: 17b9       cbnz      r7, #0x8012af0
08012aea: 0320       movs      r0, #3
08012aec: a949       ldr       r1, [pc, #0x2a4] ; literal[08012d94]=20000839
08012aee: 0870       strb      r0, [r1]
08012af0: 0320       movs      r0, #3
08012af2: a949       ldr       r1, [pc, #0x2a4] ; literal[08012d98]=20000838
08012af4: 0870       strb      r0, [r1]
08012af6: f3e0       b         #0x8012ce0
08012af8: a348       ldr       r0, [pc, #0x28c] ; literal[08012d88]=010980c0
08012afa: 8442       cmp       r4, r0
08012afc: 11d3       blo       #0x8012b22
08012afe: af48       ldr       r0, [pc, #0x2bc] ; literal[08012dbc]=016e3600
08012b00: 8442       cmp       r4, r0
08012b02: 0ed8       bhi       #0x8012b22
08012b04: 0426       movs      r6, #4
08012b06: ae48       ldr       r0, [pc, #0x2b8] ; literal[08012dc0]=01036640
08012b08: 201a       subs      r0, r4, r0
08012b0a: a149       ldr       r1, [pc, #0x284] ; literal[08012d90]=0007a120
08012b0c: b0fbf1f0   udiv      r0, r0, r1
08012b10: c5b2       uxtb      r5, r0
08012b12: 17b9       cbnz      r7, #0x8012b1a
08012b14: 0320       movs      r0, #3
08012b16: 9f49       ldr       r1, [pc, #0x27c] ; literal[08012d94]=20000839
08012b18: 0870       strb      r0, [r1]
08012b1a: 0320       movs      r0, #3
08012b1c: 9e49       ldr       r1, [pc, #0x278] ; literal[08012d98]=20000838
08012b1e: 0870       strb      r0, [r1]
08012b20: dee0       b         #0x8012ce0
08012b22: a648       ldr       r0, [pc, #0x298] ; literal[08012dbc]=016e3600
08012b24: 8442       cmp       r4, r0
08012b26: 1cd3       blo       #0x8012b62
08012b28: a648       ldr       r0, [pc, #0x298] ; literal[08012dc4]=01e84800
08012b2a: 8442       cmp       r4, r0
08012b2c: 19d8       bhi       #0x8012b62
08012b2e: 0526       movs      r6, #5
08012b30: a248       ldr       r0, [pc, #0x288] ; literal[08012dbc]=016e3600
08012b32: 201a       subs      r0, r4, r0
08012b34: 9649       ldr       r1, [pc, #0x258] ; literal[08012d90]=0007a120
08012b36: b0fbf1f0   udiv      r0, r0, r1
08012b3a: c5b2       uxtb      r5, r0
08012b3c: a248       ldr       r0, [pc, #0x288] ; literal[08012dc8]=018cba80
08012b3e: 8442       cmp       r4, r0
08012b40: 07d2       bhs       #0x8012b52
08012b42: 17b9       cbnz      r7, #0x8012b4a
08012b44: 0320       movs      r0, #3
08012b46: 9349       ldr       r1, [pc, #0x24c] ; literal[08012d94]=20000839
08012b48: 0870       strb      r0, [r1]
08012b4a: 0320       movs      r0, #3
08012b4c: 9249       ldr       r1, [pc, #0x248] ; literal[08012d98]=20000838
08012b4e: 0870       strb      r0, [r1]
08012b50: c6e0       b         #0x8012ce0
08012b52: 17b9       cbnz      r7, #0x8012b5a
08012b54: 0420       movs      r0, #4
08012b56: 8f49       ldr       r1, [pc, #0x23c] ; literal[08012d94]=20000839
08012b58: 0870       strb      r0, [r1]
08012b5a: 0420       movs      r0, #4
08012b5c: 8e49       ldr       r1, [pc, #0x238] ; literal[08012d98]=20000838
08012b5e: 0870       strb      r0, [r1]
08012b60: bee0       b         #0x8012ce0
08012b62: 9848       ldr       r0, [pc, #0x260] ; literal[08012dc4]=01e84800
08012b64: 8442       cmp       r4, r0
08012b66: 11d3       blo       #0x8012b8c
08012b68: 8c48       ldr       r0, [pc, #0x230] ; literal[08012d9c]=02625a00
08012b6a: 8442       cmp       r4, r0
08012b6c: 0ed8       bhi       #0x8012b8c
08012b6e: 0626       movs      r6, #6
08012b70: 9448       ldr       r0, [pc, #0x250] ; literal[08012dc4]=01e84800
08012b72: 201a       subs      r0, r4, r0
08012b74: 8649       ldr       r1, [pc, #0x218] ; literal[08012d90]=0007a120
08012b76: b0fbf1f0   udiv      r0, r0, r1
08012b7a: c5b2       uxtb      r5, r0
08012b7c: 17b9       cbnz      r7, #0x8012b84
08012b7e: 0420       movs      r0, #4
08012b80: 8449       ldr       r1, [pc, #0x210] ; literal[08012d94]=20000839
08012b82: 0870       strb      r0, [r1]
08012b84: 0420       movs      r0, #4
08012b86: 8449       ldr       r1, [pc, #0x210] ; literal[08012d98]=20000838
08012b88: 0870       strb      r0, [r1]
08012b8a: a9e0       b         #0x8012ce0
08012b8c: 8448       ldr       r0, [pc, #0x210] ; literal[08012da0]=02dc6c00
08012b8e: 8442       cmp       r4, r0
08012b90: 11d3       blo       #0x8012bb6
08012b92: 8e48       ldr       r0, [pc, #0x238] ; literal[08012dcc]=03567e00
08012b94: 8442       cmp       r4, r0
08012b96: 0ed8       bhi       #0x8012bb6
08012b98: 0726       movs      r6, #7
08012b9a: 8148       ldr       r0, [pc, #0x204] ; literal[08012da0]=02dc6c00
08012b9c: 201a       subs      r0, r4, r0
08012b9e: 7c49       ldr       r1, [pc, #0x1f0] ; literal[08012d90]=0007a120
08012ba0: b0fbf1f0   udiv      r0, r0, r1
08012ba4: c5b2       uxtb      r5, r0
08012ba6: 17b9       cbnz      r7, #0x8012bae
08012ba8: 0420       movs      r0, #4
08012baa: 7a49       ldr       r1, [pc, #0x1e8] ; literal[08012d94]=20000839
08012bac: 0870       strb      r0, [r1]
08012bae: 0420       movs      r0, #4
08012bb0: 7949       ldr       r1, [pc, #0x1e4] ; literal[08012d98]=20000838
08012bb2: 0870       strb      r0, [r1]
08012bb4: 94e0       b         #0x8012ce0
08012bb6: 8548       ldr       r0, [pc, #0x214] ; literal[08012dcc]=03567e00
08012bb8: 8442       cmp       r4, r0
08012bba: 11d3       blo       #0x8012be0
08012bbc: 8448       ldr       r0, [pc, #0x210] ; literal[08012dd0]=03ef1480
08012bbe: 8442       cmp       r4, r0
08012bc0: 0ed8       bhi       #0x8012be0
08012bc2: 0826       movs      r6, #8
08012bc4: 8148       ldr       r0, [pc, #0x204] ; literal[08012dcc]=03567e00
08012bc6: 201a       subs      r0, r4, r0
08012bc8: 7149       ldr       r1, [pc, #0x1c4] ; literal[08012d90]=0007a120
08012bca: b0fbf1f0   udiv      r0, r0, r1
08012bce: c5b2       uxtb      r5, r0
08012bd0: 17b9       cbnz      r7, #0x8012bd8
08012bd2: 0420       movs      r0, #4
08012bd4: 6f49       ldr       r1, [pc, #0x1bc] ; literal[08012d94]=20000839
08012bd6: 0870       strb      r0, [r1]
08012bd8: 0420       movs      r0, #4
08012bda: 6f49       ldr       r1, [pc, #0x1bc] ; literal[08012d98]=20000838
08012bdc: 0870       strb      r0, [r1]
08012bde: 7fe0       b         #0x8012ce0
08012be0: 7c48       ldr       r0, [pc, #0x1f0] ; literal[08012dd4]=0501bd00
08012be2: 8442       cmp       r4, r0
08012be4: 11d3       blo       #0x8012c0a
08012be6: 7c48       ldr       r0, [pc, #0x1f0] ; literal[08012dd8]=057bcf00
08012be8: 8442       cmp       r4, r0
08012bea: 0ed8       bhi       #0x8012c0a
08012bec: 0926       movs      r6, #9
08012bee: 7948       ldr       r0, [pc, #0x1e4] ; literal[08012dd4]=0501bd00
08012bf0: 201a       subs      r0, r4, r0
08012bf2: 6749       ldr       r1, [pc, #0x19c] ; literal[08012d90]=0007a120
08012bf4: b0fbf1f0   udiv      r0, r0, r1
08012bf8: c5b2       uxtb      r5, r0
08012bfa: 17b9       cbnz      r7, #0x8012c02
08012bfc: 0520       movs      r0, #5
08012bfe: 6549       ldr       r1, [pc, #0x194] ; literal[08012d94]=20000839
08012c00: 0870       strb      r0, [r1]
08012c02: 0520       movs      r0, #5
08012c04: 6449       ldr       r1, [pc, #0x190] ; literal[08012d98]=20000838
08012c06: 0870       strb      r0, [r1]
08012c08: 6ae0       b         #0x8012ce0
08012c0a: 7348       ldr       r0, [pc, #0x1cc] ; literal[08012dd8]=057bcf00
08012c0c: 8442       cmp       r4, r0
08012c0e: 11d3       blo       #0x8012c34
08012c10: 7248       ldr       r0, [pc, #0x1c8] ; literal[08012ddc]=05f5e100
08012c12: 8442       cmp       r4, r0
08012c14: 0ed8       bhi       #0x8012c34
08012c16: 0a26       movs      r6, #0xa
08012c18: 6f48       ldr       r0, [pc, #0x1bc] ; literal[08012dd8]=057bcf00
08012c1a: 201a       subs      r0, r4, r0
08012c1c: 5c49       ldr       r1, [pc, #0x170] ; literal[08012d90]=0007a120
08012c1e: b0fbf1f0   udiv      r0, r0, r1
08012c22: c5b2       uxtb      r5, r0
08012c24: 17b9       cbnz      r7, #0x8012c2c
08012c26: 0520       movs      r0, #5
08012c28: 5a49       ldr       r1, [pc, #0x168] ; literal[08012d94]=20000839
08012c2a: 0870       strb      r0, [r1]
08012c2c: 0520       movs      r0, #5
08012c2e: 5a49       ldr       r1, [pc, #0x168] ; literal[08012d98]=20000838
08012c30: 0870       strb      r0, [r1]
08012c32: 55e0       b         #0x8012ce0
08012c34: 6948       ldr       r0, [pc, #0x1a4] ; literal[08012ddc]=05f5e100
08012c36: 8442       cmp       r4, r0
08012c38: 11d3       blo       #0x8012c5e
08012c3a: 6948       ldr       r0, [pc, #0x1a4] ; literal[08012de0]=066ff300
08012c3c: 8442       cmp       r4, r0
08012c3e: 0ed8       bhi       #0x8012c5e
08012c40: 0b26       movs      r6, #0xb
08012c42: 6648       ldr       r0, [pc, #0x198] ; literal[08012ddc]=05f5e100
08012c44: 201a       subs      r0, r4, r0
08012c46: 5249       ldr       r1, [pc, #0x148] ; literal[08012d90]=0007a120
08012c48: b0fbf1f0   udiv      r0, r0, r1
08012c4c: c5b2       uxtb      r5, r0
08012c4e: 17b9       cbnz      r7, #0x8012c56
08012c50: 0520       movs      r0, #5
08012c52: 5049       ldr       r1, [pc, #0x140] ; literal[08012d94]=20000839
08012c54: 0870       strb      r0, [r1]
08012c56: 0520       movs      r0, #5
08012c58: 4f49       ldr       r1, [pc, #0x13c] ; literal[08012d98]=20000838
08012c5a: 0870       strb      r0, [r1]
08012c5c: 40e0       b         #0x8012ce0
08012c5e: 6048       ldr       r0, [pc, #0x180] ; literal[08012de0]=066ff300
08012c60: 8442       cmp       r4, r0
08012c62: 11d3       blo       #0x8012c88
08012c64: 5f48       ldr       r0, [pc, #0x17c] ; literal[08012de4]=06ea0500
08012c66: 8442       cmp       r4, r0
08012c68: 0ed8       bhi       #0x8012c88
08012c6a: 0c26       movs      r6, #0xc
08012c6c: 5c48       ldr       r0, [pc, #0x170] ; literal[08012de0]=066ff300
08012c6e: 201a       subs      r0, r4, r0
08012c70: 4749       ldr       r1, [pc, #0x11c] ; literal[08012d90]=0007a120
08012c72: b0fbf1f0   udiv      r0, r0, r1
08012c76: c5b2       uxtb      r5, r0
08012c78: 17b9       cbnz      r7, #0x8012c80
08012c7a: 0520       movs      r0, #5
08012c7c: 4549       ldr       r1, [pc, #0x114] ; literal[08012d94]=20000839
08012c7e: 0870       strb      r0, [r1]
08012c80: 0520       movs      r0, #5
08012c82: 4549       ldr       r1, [pc, #0x114] ; literal[08012d98]=20000838
08012c84: 0870       strb      r0, [r1]
08012c86: 2be0       b         #0x8012ce0
08012c88: 5648       ldr       r0, [pc, #0x158] ; literal[08012de4]=06ea0500
08012c8a: 8442       cmp       r4, r0
08012c8c: 11d3       blo       #0x8012cb2
08012c8e: 5648       ldr       r0, [pc, #0x158] ; literal[08012de8]=07641700
08012c90: 8442       cmp       r4, r0
08012c92: 0ed8       bhi       #0x8012cb2
08012c94: 0d26       movs      r6, #0xd
08012c96: 5348       ldr       r0, [pc, #0x14c] ; literal[08012de4]=06ea0500
08012c98: 201a       subs      r0, r4, r0
08012c9a: 3d49       ldr       r1, [pc, #0xf4] ; literal[08012d90]=0007a120
08012c9c: b0fbf1f0   udiv      r0, r0, r1
08012ca0: c5b2       uxtb      r5, r0
08012ca2: 17b9       cbnz      r7, #0x8012caa
08012ca4: 0520       movs      r0, #5
08012ca6: 3b49       ldr       r1, [pc, #0xec] ; literal[08012d94]=20000839
08012ca8: 0870       strb      r0, [r1]
08012caa: 0520       movs      r0, #5
08012cac: 3a49       ldr       r1, [pc, #0xe8] ; literal[08012d98]=20000838
08012cae: 0870       strb      r0, [r1]
08012cb0: 16e0       b         #0x8012ce0
08012cb2: 4d48       ldr       r0, [pc, #0x134] ; literal[08012de8]=07641700
08012cb4: 8442       cmp       r4, r0
08012cb6: 11d3       blo       #0x8012cdc
08012cb8: 4c48       ldr       r0, [pc, #0x130] ; literal[08012dec]=07bfa480
08012cba: 8442       cmp       r4, r0
08012cbc: 0ed8       bhi       #0x8012cdc
08012cbe: 0d26       movs      r6, #0xd
08012cc0: 4948       ldr       r0, [pc, #0x124] ; literal[08012de8]=07641700
08012cc2: 201a       subs      r0, r4, r0
08012cc4: 3249       ldr       r1, [pc, #0xc8] ; literal[08012d90]=0007a120
08012cc6: b0fbf1f0   udiv      r0, r0, r1
08012cca: c5b2       uxtb      r5, r0
08012ccc: 17b9       cbnz      r7, #0x8012cd4
08012cce: 0520       movs      r0, #5
08012cd0: 3049       ldr       r1, [pc, #0xc0] ; literal[08012d94]=20000839
08012cd2: 0870       strb      r0, [r1]
08012cd4: 0520       movs      r0, #5
08012cd6: 3049       ldr       r1, [pc, #0xc0] ; literal[08012d98]=20000838
08012cd8: 0870       strb      r0, [r1]
08012cda: 01e0       b         #0x8012ce0
08012cdc: bde8f081   pop.w     {r4, r5, r6, r7, r8, pc}
08012ce0: 4348       ldr       r0, [pc, #0x10c] ; literal[08012df0]=2000081a
08012ce2: 0078       ldrb      r0, [r0]
08012ce4: 8642       cmp       r6, r0
08012ce6: 28d0       beq       #0x8012d3a
08012ce8: 4148       ldr       r0, [pc, #0x104] ; literal[08012df0]=2000081a
08012cea: 0670       strb      r6, [r0]
08012cec: b046       mov       r8, r6
08012cee: c8ebc800   rsb       r0, r8, r8, lsl #3
08012cf2: 4fea0018   lsl.w     r8, r0, #4
08012cf6: 08f12008   add.w     r8, r8, #0x20
08012cfa: 3e48       ldr       r0, [pc, #0xf8] ; literal[08012df4]=200010e9
08012cfc: 0078       ldrb      r0, [r0]
08012cfe: 28b9       cbnz      r0, #0x8012d0c
08012d00: 7022       movs      r2, #0x70
08012d02: 4146       mov       r1, r8
08012d04: 3c48       ldr       r0, [pc, #0xf0] ; literal[08012df8]=20003064
08012d06: 13f013f8   bl        #0x8025d30
08012d0a: 16e0       b         #0x8012d3a
08012d0c: 3948       ldr       r0, [pc, #0xe4] ; literal[08012df4]=200010e9
08012d0e: 0078       ldrb      r0, [r0]
08012d10: 0128       cmp       r0, #1
08012d12: 06d1       bne       #0x8012d22
08012d14: 7022       movs      r2, #0x70
08012d16: 08f58051   add.w     r1, r8, #0x1000
08012d1a: 3748       ldr       r0, [pc, #0xdc] ; literal[08012df8]=20003064
08012d1c: 13f008f8   bl        #0x8025d30
08012d20: 0be0       b         #0x8012d3a
08012d22: 3448       ldr       r0, [pc, #0xd0] ; literal[08012df4]=200010e9
08012d24: 0078       ldrb      r0, [r0]
08012d26: 0228       cmp       r0, #2
08012d28: 07d1       bne       #0x8012d3a
08012d2a: 3448       ldr       r0, [pc, #0xd0] ; literal[08012dfc]=08028344
08012d2c: 00eb0801   add.w     r1, r0, r8
08012d30: 7023       movs      r3, #0x70
08012d32: 0022       movs      r2, #0
08012d34: 3048       ldr       r0, [pc, #0xc0] ; literal[08012df8]=20003064
08012d36: f5f745fa   bl        #0x80081c4
08012d3a: 0f2d       cmp       r5, #0xf
08012d3c: 00dd       ble       #0x8012d40
08012d3e: 0f25       movs      r5, #0xf
08012d40: 2d48       ldr       r0, [pc, #0xb4] ; literal[08012df8]=20003064
08012d42: 1030       adds      r0, #0x10
08012d44: 405d       ldrb      r0, [r0, r5]
08012d46: 2e49       ldr       r1, [pc, #0xb8] ; literal[08012e00]=2000081b
08012d48: 0870       strb      r0, [r1]
08012d4a: 2b48       ldr       r0, [pc, #0xac] ; literal[08012df8]=20003064
08012d4c: 2030       adds      r0, #0x20
08012d4e: 405d       ldrb      r0, [r0, r5]
08012d50: 2c49       ldr       r1, [pc, #0xb0] ; literal[08012e04]=2000081c
08012d52: 0870       strb      r0, [r1]
08012d54: 9fb9       cbnz      r7, #0x8012d7e
08012d56: 2848       ldr       r0, [pc, #0xa0] ; literal[08012df8]=20003064
08012d58: 3030       adds      r0, #0x30
08012d5a: 405d       ldrb      r0, [r0, r5]
08012d5c: 2a49       ldr       r1, [pc, #0xa8] ; literal[08012e08]=2000081d
08012d5e: 0870       strb      r0, [r1]
08012d60: 2548       ldr       r0, [pc, #0x94] ; literal[08012df8]=20003064
08012d62: 4030       adds      r0, #0x40
08012d64: 405d       ldrb      r0, [r0, r5]
08012d66: 2949       ldr       r1, [pc, #0xa4] ; literal[08012e0c]=2000081f
08012d68: 0870       strb      r0, [r1]
08012d6a: 2348       ldr       r0, [pc, #0x8c] ; literal[08012df8]=20003064
08012d6c: 5030       adds      r0, #0x50
08012d6e: 405d       ldrb      r0, [r0, r5]
08012d70: 2749       ldr       r1, [pc, #0x9c] ; literal[08012e10]=2000081e
08012d72: 0870       strb      r0, [r1]
08012d74: 2048       ldr       r0, [pc, #0x80] ; literal[08012df8]=20003064
08012d76: 6030       adds      r0, #0x60
08012d78: 405d       ldrb      r0, [r0, r5]
08012d7a: 2649       ldr       r1, [pc, #0x98] ; literal[08012e14]=20000820
08012d7c: 0870       strb      r0, [r1]
08012d7e: 00bf       nop       
08012d80: ace7       b         #0x8012cdc
08012d82: 0000       movs      r0, r0
08012d84: 0085       strh      r0, [r0, #0x28]
08012d86: cf00       lsls      r7, r1, #3
08012d88: c080       strh      r0, [r0, #6]
08012d8a: 0901       lsls      r1, r1, #4
08012d8c: 60fecd00   cdp2      p0, #6, c0, c0, c13, #6
08012d90: 20a1       adr       r1, #0x80
08012d92: 0700       movs      r7, r0
08012d94: 3908       lsrs      r1, r7, #0x20
08012d96: 0020       movs      r0, #0
08012d98: 3808       lsrs      r0, r7, #0x20
08012d9a: 0020       movs      r0, #0
08012d9c: 005a       ldrh      r0, [r0, r0]
08012d9e: 6202       lsls      r2, r4, #9
08012da0: 006c       ldr       r0, [r0, #0x40]
08012da2: dc02       lsls      r4, r3, #0xb
08012da4: 4077       strb      r0, [r0, #0x1d]
08012da6: 1b00       movs      r3, r3
08012da8: 00a8       add       r0, sp, #0
08012daa: 6100       lsls      r1, r4, #1
08012dac: 60e3       b         #0x8013470
08012dae: 1600       movs      r6, r2
08012db0: 20aa       add       r2, sp, #0x80
08012db2: 4400       lsls      r4, r0, #1
08012db4: 00d4       bmi       #0x8012db8
08012db6: 3000       movs      r0, r6
08012db8: 808d       ldrh      r0, [r0, #0x2c]
08012dba: 5b00       lsls      r3, r3, #1
08012dbc: 0036       adds      r6, #0
08012dbe: 6e01       lsls      r6, r5, #5
08012dc0: 4066       str       r0, [r0, #0x64]
08012dc2: 0301       lsls      r3, r0, #4
08012dc4: 0048       ldr       r0, [pc, #0] ; literal[08012dc8]=018cba80
08012dc6: e801       lsls      r0, r5, #7
08012dc8: 80ba       .byte     0x80, 0xba
08012dca: 8c01       lsls      r4, r1, #6
08012dcc: 007e       ldrb      r0, [r0, #0x18]
08012dce: 5603       lsls      r6, r2, #0xd
08012dd0: 8014       asrs      r0, r0, #0x12
08012dd2: ef03       lsls      r7, r5, #0xf
08012dd4: 00bd       pop       {pc}
08012dd6: 0105       lsls      r1, r0, #0x14
08012dd8: 00cf       .byte     0x00, 0xcf
08012dda: 7b05       lsls      r3, r7, #0x15
08012ddc: 00e1       b         #0x8012fe0
08012dde: f505       lsls      r5, r6, #0x17
08012de0: 00f3       .byte     0x00, 0xf3
08012de2: 6f06       lsls      r7, r5, #0x19
08012de4: 0005       lsls      r0, r0, #0x14
08012de6: ea06       lsls      r2, r5, #0x1b
08012de8: 0017       asrs      r0, r0, #0x1c
08012dea: 6407       lsls      r4, r4, #0x1d
08012dec: 80a4       adr       r4, #0x200
08012dee: bf07       lsls      r7, r7, #0x1e
08012df0: 1a08       lsrs      r2, r3, #0x20
08012df2: 0020       movs      r0, #0
08012df4: e910       asrs      r1, r5, #3
08012df6: 0020       movs      r0, #0
08012df8: 6430       adds      r0, #0x64
08012dfa: 0020       movs      r0, #0
08012dfc: 4483       strh      r4, [r0, #0x1a]
08012dfe: 0208       lsrs      r2, r0, #0x20
