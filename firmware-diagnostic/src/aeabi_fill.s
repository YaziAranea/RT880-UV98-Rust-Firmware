.syntax unified
.thumb
.section .text.uv98_eabi_fill, "ax", %progbits
.p2align 1

/* Keep the toolchain's optimized word-aligned copy for aligned data.
 * Every 8-byte aligned address also satisfies its 4-byte contract. */
.global __wrap___aeabi_memcpy8
.type __wrap___aeabi_memcpy8, %function
.thumb_func
__wrap___aeabi_memcpy8:
    b.w     __aeabi_memcpy4
.size __wrap___aeabi_memcpy8, . - __wrap___aeabi_memcpy8

/* AEABI fill takes dst, length and byte. Align the cursor before word
 * stores and keep the final one to three bytes inside the request. */
.global __wrap___aeabi_memclr
.type __wrap___aeabi_memclr, %function
.thumb_func
__wrap___aeabi_memclr:
    movs    r2, #0
    b       __wrap___aeabi_memset
.size __wrap___aeabi_memclr, . - __wrap___aeabi_memclr
.global __wrap___aeabi_memclr4
.thumb_set __wrap___aeabi_memclr4, __wrap___aeabi_memclr
.global __wrap___aeabi_memclr8
.thumb_set __wrap___aeabi_memclr8, __wrap___aeabi_memclr

.global __wrap___aeabi_memset
.type __wrap___aeabi_memset, %function
.thumb_func
__wrap___aeabi_memset:
    mov     r3, r0
    cmp     r1, #4
    blo     .Lfill_tail
    uxtb    r2, r2
    orr.w   r2, r2, r2, lsl #8
    orr.w   r2, r2, r2, lsl #16
.Lfill_align:
    tst.w   r3, #3
    beq     .Lfill_aligned
    strb    r2, [r3], #1
    subs    r1, #1
    b       .Lfill_align
.Lfill_aligned:
    cmp     r1, #32
    blo     .Lfill_words
    push    {r4-r7}
    mov     r4, r2
    mov     r5, r2
    mov     r6, r2
    mov     r7, r2
.Lfill_block:
    stmia   r3!, {r4-r7}
    stmia   r3!, {r4-r7}
    subs    r1, #32
    cmp     r1, #32
    bhs     .Lfill_block
    pop     {r4-r7}
.Lfill_words:
    cmp     r1, #4
    blo     .Lfill_tail
    str     r2, [r3], #4
    subs    r1, #4
    b       .Lfill_words
.Lfill_tail:
    cbz     r1, .Lfill_done
.Lfill_byte:
    strb    r2, [r3], #1
    subs    r1, #1
    bne     .Lfill_byte
.Lfill_done:
    bx      lr
.size __wrap___aeabi_memset, . - __wrap___aeabi_memset
.global __wrap___aeabi_memset4
.thumb_set __wrap___aeabi_memset4, __wrap___aeabi_memset
.global __wrap___aeabi_memset8
.thumb_set __wrap___aeabi_memset8, __wrap___aeabi_memset
