.syntax unified
.thumb
.section .data.ram_func, "awx", %progbits
.p2align 2
.global __uv98_flash_op
.global __uv98_flash_op_end
.type __uv98_flash_op, %function
.thumb_func

/* r0 address, r1 word, r2 mode (FPRGM or SECERS), r3 cycle deadline.
 * No literal pool, call or Flash load during OBF. PRIMASK is restored after
 * lock. Access valid FLASH->ADDR and main-array addresses only. */
__uv98_flash_op:
    push    {r4-r7, lr}
    mrs     r4, primask
    cpsid   i

    movw    r6, #0xf000
    movt    r6, #0x0803
    subs    r7, r0, r6
    cmp.w   r7, #0x1000
    bhs     .Lcontroller
    tst     r7, #3
    bne     .Lcontroller
    cmp     r2, #1
    beq     .Lcheck_bus
    cmp     r2, #2
    bne     .Lcontroller
    ubfx    r7, r7, #0, #11
    cmp     r7, #0
    bne     .Lcontroller

.Lcheck_bus:
    movw    r5, #0x2000
    movt    r5, #0x4002             /* FLASH register base */
    ldr     r6, [r5, #0x0c]
    tst     r6, #1                  /* OBF */
    bne     .Lawait_watchdog
    ldr     r6, [r5, #0x10]
    cmp     r6, #0x80               /* OPLK, no inherited operation or IRQ */
    beq     .Linitial_lock
    cmp     r6, #0                  /* inherit only idle, otherwise refuse */
    bne     .Lcontroller
    movs    r7, #0x80
    str     r7, [r5, #0x10]
    ldr     r6, [r5, #0x10]
    cmp     r6, r7
    bne     .Lcontroller
.Linitial_lock:
    movs    r7, #0x34               /* W1C PRGMERR, EPPERR, ODF */
    str     r7, [r5, #0x0c]
    ldr     r6, [r5, #0x0c]
    tst     r6, #0x35
    bne     .Lcontroller

    movw    r6, #0x0123
    movt    r6, #0x4567
    str     r6, [r5, #4]
    movw    r6, #0x89ab
    movt    r6, #0xcdef
    str     r6, [r5, #4]
    ldr     r6, [r5, #0x10]
    cmp     r6, #0
    bne     .Lbad_mode
    str     r2, [r5, #0x10]
    ldr     r6, [r5, #0x10]
    cmp     r6, r2
    bne     .Lbad_mode

    movw    r6, #0x1004
    movt    r6, #0xe000             /* DWT_CYCCNT */
    ldr     r12, [r6]
    mov     r7, r3                  /* iteration cap if DWT stalls */
    cmp     r2, #1
    bne     .Lsector
    str     r1, [r0]                /* program one aligned word */
    b       .Lwait
.Lsector:
    str     r0, [r5, #0x14]         /* FLASH_ADDR is write-only (RM §5.8.6) */
    movs    r1, #0x42               /* SECERS | ERSTR */
    str     r1, [r5, #0x10]
.Lwait:
    dsb     sy
.Lpoll:
    ldr     r1, [r5, #0x0c]
    tst     r1, #1
    beq     .Lcomplete
    ldr     r2, [r6]
    subs    r2, r2, r12
    cmp     r2, r3
    bhs     .Lawait_watchdog
    subs    r7, r7, #1
    bne     .Lpoll
.Lawait_watchdog:
    /* Returning to Flash while busy can stall execution. Keep IRQs masked
     * and wait for the already armed independent watchdog in SRAM. */
    b       .Lawait_watchdog

.Lcomplete:
    ands    r1, r1, #0x34
    cmp     r1, #0x20               /* ODF set and no error */
    bne     .Loperation_fault
    movs    r0, #0
    b       .Llock
.Loperation_fault:
    movs    r0, #2
    b       .Llock
.Lbad_mode:
    movs    r0, #1
.Llock:
    movs    r6, #0x80
    str     r6, [r5, #0x10]         /* clear mode bits and set OPLK */
    ldr     r1, [r5, #0x10]
    cmp     r1, r6
    bne     .Lawait_watchdog
    movs    r6, #0x34
    str     r6, [r5, #0x0c]
    ldr     r1, [r5, #0x0c]
    tst     r1, #0x35
    bne     .Lcontroller
    b       .Lreturn
.Lcontroller:
    movs    r0, #1
.Lreturn:
    dsb     sy
    isb     sy
    msr     primask, r4
    pop     {r4-r7, pc}
__uv98_flash_op_end:
.size __uv98_flash_op, . - __uv98_flash_op
