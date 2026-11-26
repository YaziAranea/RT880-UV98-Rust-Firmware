.syntax unified
.thumb
.section .text.uv98_memmove, "ax", %progbits
.p2align 1

/* AAPCS: r0=dst, r1=src, r2=len. Keep r0 intact; only the bulk loop uses
 * callee-saved registers. Different alignments use byte loads and stores. */
.global __wrap___aeabi_memmove
.type __wrap___aeabi_memmove, %function
.thumb_func
__wrap___aeabi_memmove:
    cmp     r2, #0
    beq     .Ldone
    cmp     r0, r1
    beq     .Ldone
    mov     r3, r0
    bhi     .Lb_start

    eor.w   ip, r3, r1
    tst.w   ip, #3
    bne     .Lf_bytes
.Lf_align:
    tst.w   r1, #3
    beq     .Lf_aligned
    ldrb.w  ip, [r1], #1
    strb.w  ip, [r3], #1
    subs    r2, #1
    bne     .Lf_align
    bx      lr
.Lf_aligned:
    cmp     r2, #16
    blo     .Lf_words
    push    {r4-r7}
.Lf_block:
    ldmia   r1!, {r4-r7}
    stmia   r3!, {r4-r7}
    subs    r2, #16
    cmp     r2, #16
    bhs     .Lf_block
    pop     {r4-r7}
.Lf_words:
    cmp     r2, #4
    blo     .Lf_bytes
    ldr.w   ip, [r1], #4
    str.w   ip, [r3], #4
    subs    r2, #4
    b       .Lf_words
.Lf_bytes:
    cbz     r2, .Ldone
.Lf_byte:
    ldrb.w  ip, [r1], #1
    strb.w  ip, [r3], #1
    subs    r2, #1
    bne     .Lf_byte
.Ldone:
    bx      lr

.Lb_start:
    adds    r1, r1, r2
    adds    r3, r3, r2
    eor.w   ip, r3, r1
    tst.w   ip, #3
    bne     .Lb_bytes
.Lb_align:
    tst.w   r1, #3
    beq     .Lb_aligned
    ldrb.w  ip, [r1, #-1]!
    strb.w  ip, [r3, #-1]!
    subs    r2, #1
    bne     .Lb_align
    bx      lr
.Lb_aligned:
    cmp     r2, #16
    blo     .Lb_words
    push    {r4-r7}
.Lb_block:
    ldmdb   r1!, {r4-r7}
    stmdb   r3!, {r4-r7}
    subs    r2, #16
    cmp     r2, #16
    bhs     .Lb_block
    pop     {r4-r7}
.Lb_words:
    cmp     r2, #4
    blo     .Lb_bytes
    ldr.w   ip, [r1, #-4]!
    str.w   ip, [r3, #-4]!
    subs    r2, #4
    b       .Lb_words
.Lb_bytes:
    cmp     r2, #0
    beq     .Ldone
.Lb_byte:
    ldrb.w  ip, [r1, #-1]!
    strb.w  ip, [r3, #-1]!
    subs    r2, #1
    bne     .Lb_byte
    bx      lr

.size __wrap___aeabi_memmove, . - __wrap___aeabi_memmove
.global __wrap___aeabi_memmove4
.thumb_set __wrap___aeabi_memmove4, __wrap___aeabi_memmove
.global __wrap___aeabi_memmove8
.thumb_set __wrap___aeabi_memmove8, __wrap___aeabi_memmove
