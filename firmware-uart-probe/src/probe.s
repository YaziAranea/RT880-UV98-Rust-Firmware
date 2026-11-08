/* Stackless UART application observer. No RF, LCD, NOR, WDT, Flash/PWC writes.
 * No debugger-ID gate. First complete frame precedes every ID-register read.
 * r4-r11 retain entry snapshots until sent; r12 holds UART-ready/checksum.
 * UART helper tail ABI: r0 byte, r3 Thumb continuation; r1/r2/r12 scratch.
 * Reset never returns; caller registers need not be restored.
 */
.syntax unified
.thumb

.macro load32 reg, value
    movw \reg, #(\value & 0xffff)
    movt \reg, #((\value >> 16) & 0xffff)
.endm
.macro modify address, clear, set
    load32 r0, \address
    ldr r1, [r0]
    load32 r2, \clear
    bic r1, r1, r2
    load32 r2, \set
    orr r1, r1, r2
    str r1, [r0]
.endm
.macro poll address, mask, expected
    load32 r0, \address
    load32 r3, 100000
.Lpoll\@:
    ldr r1, [r0]
    load32 r2, \mask
    and r1, r1, r2
    load32 r2, \expected
    cmp r1, r2
    beq .Lpolled\@
    subs r3, #1
    bne .Lpoll\@
    b.w ProbeStop
.Lpolled\@:
.endm
.macro settle
    .rept 40
        nop
    .endr
    dsb
    isb
.endm
.macro send_r0
    movw r3, #:lower16:.Lsent\@
    movt r3, #:upper16:.Lsent\@
    orr r3, r3, #1
    b.w ProbeUartByte
.Lsent\@:
.endm
.macro byte value
    movs r0, #\value
    send_r0
.endm
/* kind0=constant, kind1=CPU register. No MMIO reads inside a frame. */
.macro hexword kind, value
    .irp shift,28,24,20,16,12,8,4,0
        .if \kind == 0
            load32 r0, \value
        .else
            mov r0, \value
        .endif
        .if \shift != 0
            lsrs r0, r0, #\shift
        .endif
        and r0, r0, #15
        cmp r0, #10
        ite lo
        addlo r0, #48
        addhs r0, #55
        send_r0
    .endr
.endm
.macro frame code, ak=0, av=0, bk=0, bv=0, ck=0, cv=0
    tst r12, #0x80000000
    beq.w ProbeStop
    and r12, r12, #0x80000000
    .irp char,85,86,57,56,68,49,32
        byte \char
    .endr
    hexword 0, \code
    byte 32
    hexword \ak, \av
    byte 32
    hexword \bk, \bv
    byte 32
    hexword \ck, \cv
    orr r12, r12, #0x40000000
    byte 42
    ubfx r0, r12, #4, #4
    cmp r0, #10
    ite lo
    addlo r0, #48
    addhs r0, #55
    send_r0
    and r0, r12, #15
    cmp r0, #10
    ite lo
    addlo r0, #48
    addhs r0, #55
    send_r0
    byte 13
    byte 10
    /* Full TDC drain; no clock change or ID read can overtake a frame. */
    load32 r0, 0x40011400
    movw r1, #8192
.Ldrain\@:
    ldr r2, [r0]
    tst r2, #0x40
    bne .Ldrained\@
    subs r1, #1
    bne .Ldrain\@
    mov.w r12, #0
    b.w ProbeStop
.Ldrained\@:
.endm

.section .vector_table, "a", %progbits
.global ProbeVectors
.type ProbeVectors, %object
ProbeVectors:
    .word 0x200099c8
    .word Reset
    /* Same Reset address as the known working stock. Reserved entries point
       to the one terminal handler too; all configurable interrupts are masked. */
    .rept 174
        .word ProbeFault
    .endr
.size ProbeVectors, . - ProbeVectors

.section .text.Reset, "ax", %progbits
.global Reset
.type Reset, %function
.thumb_func
Reset:
    /* Clear before any MMIO: an early exception must not infer UART-ready from
       an inherited arbitrary r12 value. Context is observed, not rejected. */
    mov.w r12, #0
    mrs r4, CONTROL
    mrs r5, IPSR
    mrs r6, MSP
    cpsid i
    load32 r0, 0x200099c8
    msr MSP, r0
    load32 r0, 0xe000ed08
    load32 r1, 0x08002800
    str r1, [r0]
    dsb
    isb

    load32 r0, 0x40023800
    ldr r7, [r0]
    ldr r8, [r0, #8]
    ldr r9, [r0, #0xa0]
    ldr r10, [r0, #0xa4]
    ldr r11, [r0, #4]

    load32 r0, 0xe000e010
    movs r1, #0
    str r1, [r0]
    str r1, [r0, #4]
    str r1, [r0, #8]
    /* CMSIS Cortex-M4 ICER/ICPR arrays contain eight banks. Writes to absent
       IRQ bits are ignored by the architecture; no ICTR gate can hide output. */
    load32 r0, 0xe000e180
    load32 r1, 0xe000e280
    mvn r2, #0
    movs r3, #8
.Lclear_nvic:
    str r2, [r0], #4
    str r2, [r1], #4
    subs r3, #1
    bne .Lclear_nvic
    load32 r0, 0xe000ed04
    load32 r1, 0x0a000000
    str r1, [r0]
    modify 0xe000ed10, 6, 0
    /* Stop inherited DMA/OTG masters before taking PA11 from USB. */
    modify 0x40023830, 0x01400000, 0
    modify 0x40023834, 0x00000080, 0
    modify 0x40023810, 0, 0x01400000
    modify 0x40023814, 0, 0x00000080
    dsb

    /* Establish direct HICK8, without depending on the inherited PLL/HEXT
       frequency. Preserve HICKDIV bit12, PLL/HEXT config and APB1 divisor.
       This uses stock/vendor operations composed into a smaller sequence. */
    modify 0x400238a4, 0x30, 0x30
    modify 0x40023800, 0, 1
    poll 0x40023800, 2, 2
    modify 0x400238a4, 0x00070000, 0x00040000
    settle
    modify 0x400238a0, 0x00004000, 0
    modify 0x40023808, 3, 0
    poll 0x40023808, 12, 0
    /* At HICK8/16, reduce AHB/APB2 divisors before raising direct HICK to8M. */
    modify 0x40023808, 0x0000e0f0, 0
    modify 0x400238a4, 0x00070000, 0
    settle
    modify 0x400238a4, 0x30, 0
    poll 0x40023800, 3, 3
    poll 0x40023808, 0x0000e0ff, 0
    poll 0x400238a0, 0x00004000, 0
    poll 0x400238a4, 0x00070030, 0

    /* Exact stock PA11 AF8 / push-pull / STRONGER drive1 / no pull.
       Only this pin changes. PA12 and every radio control pin are untouched. */
    modify 0x40023830, 0, 1
    dsb
    modify 0x40023844, 0, 0x20
    dsb
    modify 0x40023824, 0, 0x20
    modify 0x40023824, 0x20, 0
    load32 r0, 0x40011400
    movs r1, #0
    str r1, [r0, #12]
    str r1, [r0, #16]
    str r1, [r0, #20]
    movs r1, #69
    str r1, [r0, #8]
    modify 0x40020004, 0x00000800, 0
    modify 0x40020008, 0x00c00000, 0x00400000
    modify 0x4002000c, 0x00c00000, 0
    modify 0x40020024, 0x0000f000, 0x00008000
    load32 r0, 0x4001140c
    movw r1, #0x2008
    str r1, [r0]
    modify 0x40020000, 0x00c00000, 0x00800000
    dsb
    mov.w r12, #0x80000000

    /* No MMIO ID register has been read yet. */
    frame 0x200, 0, 0x00000102
    frame 0x201, 1, r4, 1, r5, 1, r6
    frame 0x202, 1, r7, 1, r8
    frame 0x203, 1, r9, 1, r10, 1, r11

    frame 0x210, 0, 0xe000ed00
    load32 r0, 0xe000ed00
    ldr r4, [r0]
    frame 0x211, 1, r4
    frame 0x212, 0, 0xe0042000
    load32 r0, 0xe0042000
    ldr r4, [r0]
    frame 0x213, 1, r4
    frame 0x214, 0, 0xe0042020
    load32 r0, 0xe0042020
    ldr r4, [r0]
    frame 0x215, 1, r4
    frame 0x216, 0, 0x1ffff7e0
    load32 r0, 0x1ffff7e0
    ldrh r4, [r0]
    frame 0x217, 1, r4
    frame 0x220
    b.w ProbeStop
.size Reset, . - Reset

.section .text.ProbeUartByte, "ax", %progbits
.global ProbeUartByte
.type ProbeUartByte, %function
.thumb_func
ProbeUartByte:
    movw r1, #8192
.Lbyte_poll:
    load32 r2, 0x40011400
    ldr r2, [r2]
    tst r2, #0x80
    bne .Lbyte_ready
    subs r1, #1
    bne .Lbyte_poll
    mov.w r12, #0
    b.w ProbeStop
.Lbyte_ready:
    load32 r2, 0x40011404
    str r0, [r2]
    tst r12, #0x40000000
    it eq
    addeq r12, r12, r0
    bx r3
.size ProbeUartByte, . - ProbeUartByte

.section .text.ProbeFault, "ax", %progbits
.global ProbeFault
.type ProbeFault, %function
.thumb_func
ProbeFault:
    cpsid i
    tst r12, #0x80000000
    beq.w ProbeStop
    mrs r4, IPSR
    load32 r0, 0xe000ed28
    ldr r5, [r0]
    ldr r6, [r0, #4]
    frame 0xe2ff0000, 1, r4, 1, r5, 1, r6
    b.w ProbeStop
.size ProbeFault, . - ProbeFault

.section .text.ProbeStop, "ax", %progbits
.global ProbeStop
.type ProbeStop, %function
.thumb_func
ProbeStop:
    cpsid i
    dsb
.Lhalt:
    wfi
    b .Lhalt
.size ProbeStop, . - ProbeStop
