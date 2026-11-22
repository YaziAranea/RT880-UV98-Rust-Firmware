/* Pre-RAM TX-only diagnostics. No stack, data RAM, BL or callee-saved registers.
 * r12: bit31 UART ready, bit30 checksum frozen, bits24..28 hex shift,
 * bits16..19 shared failure code nibble or GPIO descriptor index,
 * low16 ASCII sum (low8 transmitted).
 * LR preserved. Even 255 per frame byte cannot carry out of the sum field.
 * UART byte helper's private tail-branch ABI: r0 byte, r3 Thumb continuation;
 * clobbers r1/r2/r12 only. Clock decode never writes a clock source/divider.
 */
.syntax unified
.thumb
.macro diag_load reg, value
    /* MOVW zero-extends the immediate without changing flags. */
    movw \reg, #(\value & 0xffff)
    .if ((\value >> 16) & 0xffff) != 0
        movt \reg, #((\value >> 16) & 0xffff)
    .endif
.endm

.macro diag_send_r0
    /* The nearby continuation already includes the Thumb bit for BX r3. */
    adr.w r3, .Ltx_return\@+1
    b.w __diag_uart_byte
.Ltx_return\@:
.endm

.macro diag_byte value
    movs r0, #\value
    diag_send_r0
.endm

/* kind 0=constant, 1=MMIO word, 2=MMIO halfword, 3=IPSR, 4=CONTROL, 5=MSP,
 * 6=early failure code, 7=GPIO port, 8=GPIO lock readback, 9=GPIO pin mask.
 * Kinds 6..9 use the tag saved in r12.
 * Stackless pre-init leaves CONTROL/MSP unchanged throughout each frame.
 * Identity registers
 * are immutable; fault registers are best-effort samples, not an atomic dump. */
.macro diag_hex kind, value
    /* The former eight-way macro expansion repeated this code for every
     * word of every frame. A bounded loop keeps the identical MMIO rereads,
     * byte order/checksum and stackless UART ABI while saving flash. */
    bic r12, r12, #0x1f000000
    orr r12, r12, #0x1c000000
.Lhex_digit\@:
        .if \kind == 3
            mrs r0, IPSR
        .elseif \kind == 4
            mrs r0, CONTROL
        .elseif \kind == 5
            mrs r0, MSP
        .elseif \kind == 6
            ubfx r0, r12, #16, #4
            movt r0, #0xe100
        .elseif \kind >= 7
            ubfx r1, r12, #16, #4
            movw r0, #:lower16:.Ldiag_gpio_lock_descriptors
            movt r0, #:upper16:.Ldiag_gpio_lock_descriptors
            add r0, r0, r1, lsl #3
            .if \kind == 9
                ldr r0, [r0, #4]
            .else
                ldr r0, [r0]
                .if \kind == 8
                    ldr r0, [r0, #0x1c]
                .endif
            .endif
        .else
            diag_load r0, \value
            .if \kind == 1
                ldr r0, [r0]
            .elseif \kind == 2
                ldrh r0, [r0]
            .endif
        .endif
        ubfx r1, r12, #24, #5
        lsr r0, r0, r1
        and r0, r0, #15
        cmp r0, #10
        ite lo
        addlo r0, #48
        addhs r0, #55
        diag_send_r0
        tst r12, #0x1f000000
        beq .Lhex_done\@
        sub r12, r12, #0x04000000
        b .Lhex_digit\@
.Lhex_done\@:
.endm

.macro diag_frame code, ak=1, av=0xe0042000, bk=1, bv=0xe0042020, ck=2, cv=0x1ffff7e0, code_kind=0, tagged=0
    tst r12, #0x80000000
    beq.w .Lframe_done\@
    and r12, r12, #0x80000000
    .if \tagged
        /* Keep the fault tag after the first UART byte overwrites r0. */
        bfi r12, r0, #16, #4
    .endif
    .irp char,85,86,57,56,68,49,32
        diag_byte \char
    .endr
    diag_hex \code_kind, \code
    diag_byte 32
    diag_hex \ak, \av
    diag_byte 32
    diag_hex \bk, \bv
    diag_byte 32
    diag_hex \ck, \cv
    orr r12, r12, #0x40000000
    diag_byte 42
    ubfx r0, r12, #4, #4
    cmp r0, #10
    ite lo
    addlo r0, #48
    addhs r0, #55
    diag_send_r0
    and r0, r12, #15
    cmp r0, #10
    ite lo
    addlo r0, #48
    addhs r0, #55
    diag_send_r0
    diag_byte 13
    diag_byte 10
    /* Drain TC before a caller may change APB2. Timeout disables further TX. */
    tst r12, #0x80000000
    beq .Lframe_done\@
    diag_load r0, 0x40011400
    movw r1, #8192
.Ldrain\@:
    ldr r2, [r0]
    tst r2, #0x40
    bne .Lframe_done\@
    subs r1, #1
    bne .Ldrain\@
    bic r12, r12, #0x80000000
.Lframe_done\@:
.endm

/* Result r3=rounded USART DIV. Accept direct HICK, or exact known HICK PLL
 * parameters (48 MHz); AHB/APB2 prescalers decoded per vendor CRM tables.
 * Caller repeats decode and compares divisor to reject visible clock changes.
 */
.macro diag_baud bad
    diag_load r0, 0x40023800
    ldr r1, [r0, #8]
    and r2, r1, #15
    cmp r2, #0
    beq .Lhick\@
    cmp r2, #10
    bne.w \bad
    ldr r2, [r0]
    diag_load r3, 0x03000003
    and r2, r2, r3
    cmp r2, r3
    bne.w \bad
    ldr r2, [r0, #4]
    diag_load r3, 0x00477fcf
    and r2, r2, r3
    diag_load r3, 0x00031801
    cmp r2, r3
    bne.w \bad
    ldr r2, [r0, #0xa0]
    tst r2, #0x1000
    beq.w \bad
    diag_load r3, 48000000
    b .Lbus\@
.Lhick\@:
    ldr r2, [r0]
    and r2, r2, #3
    cmp r2, #3
    bne.w \bad
    diag_load r3, 8000000
    ldr r2, [r0, #0xa0]
    and r2, r2, #0x5000
    cmp r2, #0x5000
    bne .Ldirect_div\@
    diag_load r3, 48000000
.Ldirect_div\@:
    ldr r2, [r0, #0xa4]
    ubfx r2, r2, #16, #3
    cmp r2, #4
    bhi.w \bad
    lsr r3, r3, r2
.Lbus\@:
    ubfx r2, r1, #4, #4
    cmp r2, #8
    blo .Lapb\@
    cmp r2, #12
    ite lo
    sublo r2, #7
    subhs r2, #6
    lsr r3, r3, r2
.Lapb\@:
    ubfx r2, r1, #13, #3
    cmp r2, #4
    blo .Lrounded\@
    subs r2, #3
    lsr r3, r3, r2
.Lrounded\@:
    movw r2, #57600
    add r3, r3, r2
    diag_load r2, 115200
    udiv r3, r3, r2
    cmp r3, #16
    blo.w \bad
    movw r2, #65535
    cmp r3, r2
    bhi.w \bad
.endm

/* Only after the stock-compatible HICK8 bootstrap and DMA/USB quiesce.
 * This diagnostic target is explicitly for the physically observed F423.
 * GPIOA PA11 alone is owned; identity guards still precede every RF request.
 * Unknown clock/locked TX does not stop original startup: logging is absent.
 * Init has no frame output; terminal caller attempts RF low requests first. */
.macro diag_uart_init
    mov.w r12, #0
    diag_baud .Luart_absent\@
    mov r12, r3
    diag_baud .Luart_absent\@
    cmp r3, r12
    bne.w .Luart_absent\@
    /* Retain DIV in r12 while r0-r3 perform independent GPIO/CRM setup. */
    diag_modify 0x40023830, 0, 1
    ldr r1, [r0]
    tst r1, #1
    beq.w .Luart_absent\@
    diag_load r0, 0x4002001c
    ldr r1, [r0]
    tst r1, #0x10000
    beq .Ltx_unlocked\@
    tst r1, #0x800
    bne.w .Luart_absent\@
.Ltx_unlocked\@:
    diag_modify 0x40023844, 0, 0x20
    ldr r1, [r0]
    tst r1, #0x20
    beq.w .Luart_absent\@
    diag_modify 0x40023824, 0, 0x20
    diag_modify 0x40023824, 0x20, 0
    diag_load r0, 0x40011400
    movs r1, #0
    str r1, [r0, #12]
    str r1, [r0, #16]
    str r1, [r0, #20]
    str r12, [r0, #8]
    movw r1, #0x2008
    str r1, [r0, #12]
    /* Proven stock TX AF8, push-pull, STRONGER drive1, no pulls. */
    diag_modify 0x40020004, 0x800, 0
    diag_modify 0x40020008, 0x00c00000, 0x00400000
    diag_modify 0x4002000c, 0x00c00000, 0
    diag_modify 0x40020024, 0x0000f000, 0x00008000
    diag_modify 0x40020000, 0x00c00000, 0x00800000
    dsb
    mov.w r12, #0x80000000
    b .Luart_init_done\@
.Luart_absent\@:
    mov.w r12, #0
.Luart_init_done\@:
.endm

.section .text.__diag_uart_byte, "ax", %progbits
.global __diag_uart_byte
.type __diag_uart_byte, %function
.thumb_func
__diag_uart_byte:
    tst r12, #0x80000000
    beq .Lbyte_return
    movw r1, #8192
.Lbyte_poll:
    diag_load r2, 0x40011400
    ldr r2, [r2]
    tst r2, #0x80
    bne .Lbyte_ready
    subs r1, #1
    bne .Lbyte_poll
    bic r12, r12, #0x80000000
    b .Lbyte_return
.Lbyte_ready:
    diag_load r2, 0x40011404
    str r0, [r2]
    tst r12, #0x40000000
    it eq
    addeq r12, r12, r0
.Lbyte_return:
    bx r3
.size __diag_uart_byte, . - __diag_uart_byte
