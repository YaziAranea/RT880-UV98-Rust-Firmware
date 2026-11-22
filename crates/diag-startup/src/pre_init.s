/* AT32F423 diagnostic pre-init. Called by cortex-m-rt Reset before RAM init.
 * r0-r3/r12 only, no stack/RAM/BL/Rust. Tail UART helper preserves LR.
 * See research/diag-startup/EVIDENCE-RU.txt for limitations and invariants.
 */
.syntax unified
.thumb

.macro diag_modify address, clear, set
    diag_load r0, \address
    ldr r1, [r0]
    diag_load r2, \clear
    bic r1, r1, r2
    diag_load r2, \set
    orr r1, r1, r2
    str r1, [r0]
.endm

/* Terminal-only checks/bitbang. No stack, BL, RAM, or callee-saved register.
 * r12 carries only a shifted24-bit RF word and is cleared before UART setup.
 * The500-iteration phase loop supplies >=500core cycles (>=10us at <=50MHz).
 * Actual pin timing/voltage and RF-chip acceptance remain unmeasured.
 */
.macro diag_rf_require address, mask, expected, bad=.Ldiag_terminal_report
    diag_load r0, \address
    ldr r1, [r0]
    diag_load r2, \mask
    and r1, r1, r2
    diag_load r2, \expected
    cmp r1, r2
    bne.w \bad
.endm

.macro diag_rf_unlocked port, mask, bad=.Ldiag_terminal_report
    diag_load r0, \port
    ldr r1, [r0, #0x1c]
    tst r1, #0x10000
    beq .Lrf_unlocked\@
    diag_load r2, \mask
    tst r1, r2
    bne.w \bad
.Lrf_unlocked\@:
.endm

.macro diag_rf_output_present port, mask, high, fields, mode, drive, bad=.Ldiag_terminal_report
    diag_rf_require (\port + 0x14), \mask, \high, \bad
    diag_rf_require \port, \fields, \mode, \bad
    diag_rf_require (\port + 4), \mask, 0, \bad
    diag_rf_require (\port + 8), \fields, \drive, \bad
    diag_rf_require (\port + 12), \fields, 0, \bad
.endm

.macro diag_rf_phase
    movw r2, #500
.Lrf_delay\@:
    subs r2, #1
    bne .Lrf_delay\@
.endm

.macro diag_rf_write frame, cs_port, cs_pin, sck_port, sck_pin, data_pin
    /* SCK low before selecting, then address/high/low bytes MSB-first. */
    diag_load r0, \sck_port
    movw r1, #\sck_pin
    str r1, [r0, #0x28]
    diag_load r0, \cs_port
    movw r1, #\cs_pin
    str r1, [r0, #0x28]
    diag_load r12, (\frame << 8)
    movs r3, #24
.Lrf_bit\@:
    diag_load r0, \sck_port
    movw r1, #\sck_pin
    str r1, [r0, #0x28]
    diag_load r0, 0x40020800
    movw r1, #\data_pin
    lsls.w r12, r12, #1
    bcc .Lrf_zero\@
    str r1, [r0, #0x18]
    b .Lrf_setup\@
.Lrf_zero\@:
    str r1, [r0, #0x28]
.Lrf_setup\@:
    diag_rf_phase
    diag_load r0, \sck_port
    movw r1, #\sck_pin
    str r1, [r0, #0x18]
    diag_rf_phase
    subs r3, #1
    bne .Lrf_bit\@
    diag_load r0, \cs_port
    movw r1, #\cs_pin
    str r1, [r0, #0x18]
    diag_rf_phase
.endm

/* Physically exercised by UART observer 0.1.2 on PID 0x700a3246. These
 * bounded operations precede identity rejection so the rejection is visible.
 * No data RAM/stack or callee-saved register is used, including on timeout.
 */
.macro diag_poll address, mask, expected, bad
    diag_load r0, \address
    diag_load r3, 100000
.Lpoll\@:
    ldr r1, [r0]
    diag_load r2, \mask
    and r1, r1, r2
    diag_load r2, \expected
    cmp r1, r2
    beq .Lpolled\@
    subs r3, #1
    bne .Lpoll\@
    b.w \bad
.Lpolled\@:
.endm

.macro diag_settle
    .rept 40
        nop
    .endr
    dsb
    isb
.endm

.macro diag_hick8_bootstrap bad
    /* Preserve PLL/HEXT configuration, HICKDIV bit12, APB1 divider and all
     * Flash/PWC/WDT state. Slow direct HICK before switching away from PLL. */
    diag_modify 0x400238a4, 0x30, 0x30
    diag_modify 0x40023800, 0, 1
    diag_poll 0x40023800, 2, 2, \bad
    diag_modify 0x400238a4, 0x00070000, 0x00040000
    diag_settle
    diag_modify 0x400238a0, 0x00004000, 0
    diag_modify 0x40023808, 3, 0
    diag_poll 0x40023808, 12, 0, \bad
    diag_modify 0x40023808, 0x0000e0f0, 0
    diag_modify 0x400238a4, 0x00070000, 0
    diag_settle
    diag_modify 0x400238a4, 0x30, 0
    diag_poll 0x40023800, 3, 3, \bad
    diag_poll 0x40023808, 0x0000e0ff, 0, \bad
    diag_poll 0x400238a0, 0x00004000, 0, \bad
    diag_poll 0x400238a4, 0x00070030, 0, \bad
.endm

.pushsection .rodata.diag_gpio_lock, "a", %progbits
.balign 4
.Ldiag_gpio_lock_descriptors:
.popsection
.set .Ldiag_gpio_lock_index, 0

/* mode_mask: two bits per owned pin; mode_bits: output 01 per pin.
 * drive_bits: moderate drive 10 per pin. drain: 0 or owned pin mask.
 * Latch is preloaded before the output mode is exposed. No whole-port reset.
 */
.macro diag_output port, low, high, mask, mode_mask, mode_bits, drive_bits, drain, terminal=0, bad=.Ldiag_terminal_report
    diag_load r0, \port
    ldr r1, [r0, #0x1c]
    tst r1, #0x10000
    beq .Lunlocked\@
    diag_load r2, \mask
    tst r1, r2
    beq .Lunlocked\@
.if \terminal == 0
    /* Emit the exact arguments at the lock check, outside the code section. */
    .pushsection .rodata.diag_gpio_lock, "a", %progbits
    .word \port, \mask
    .popsection
    .if .Ldiag_gpio_lock_index > 15
        .error "GPIO lock descriptor index exceeds the fault tag"
    .endif
    movs r0, #.Ldiag_gpio_lock_index
    .set .Ldiag_gpio_lock_index, .Ldiag_gpio_lock_index + 1
    b.w .Ldiag_gpio_lock_fail
.else
    b.w \bad
.endif
.Lunlocked\@:
.if \low != 0
    diag_load r1, \low
    str r1, [r0, #0x28]
.endif
.if \high != 0
    diag_load r1, \high
    str r1, [r0, #0x18]
.endif
    ldr r1, [r0, #4]
    diag_load r2, \mask
    bic r1, r1, r2
    diag_load r2, \drain
    orr r1, r1, r2
    str r1, [r0, #4]
    diag_load r2, \mode_mask
    ldr r1, [r0, #12]
    bic r1, r1, r2
    str r1, [r0, #12]
    ldr r1, [r0, #8]
    bic r1, r1, r2
    diag_load r3, \drive_bits
    orr r1, r1, r3
    str r1, [r0, #8]
    ldr r1, [r0]
    bic r1, r1, r2
    diag_load r3, \mode_bits
    orr r1, r1, r3
    str r1, [r0]
.endm

.macro diag_rf_a_off bad
    diag_output 0x40020800, 0x8030, 0, 0x8030, 0xc0000f00, 0x40000500, 0x80000a00, 0, 1, \bad
    diag_output 0x40020400, 0x0800, 0, 0x0800, 0x00c00000, 0x00400000, 0x00800000, 0, 1, \bad
    diag_output 0x40021400, 0x0100, 0, 0x0100, 0x00030000, 0x00010000, 0x00020000, 0, 1, \bad
    diag_output 0x40020000, 0x0010, 0, 0x0010, 0x00000300, 0x00000100, 0x00000200, 0, 1, \bad
    dsb
    isb
    /* MCU gate LOW precedes every RF-A pulse. Reject register readback faults
     * and all RF-A pin locks before taking ownership of any serial pin. */
    diag_rf_output_present 0x40020800, 0x8030, 0, 0xc0000f00, 0x40000500, 0x80000a00, \bad
    diag_rf_output_present 0x40020400, 0x0800, 0, 0x00c00000, 0x00400000, 0x00800000, \bad
    diag_rf_output_present 0x40021400, 0x0100, 0, 0x00030000, 0x00010000, 0x00020000, \bad
    diag_rf_output_present 0x40020000, 0x0010, 0, 0x00000300, 0x00000100, 0x00000200, \bad
    diag_rf_unlocked 0x40020000, 0x0100, \bad
    diag_rf_unlocked 0x40021400, 0x0040, \bad
    diag_rf_unlocked 0x40020800, 0x0200, \bad
    diag_output 0x40020000, 0, 0x0100, 0x0100, 0x00030000, 0x00010000, 0x00020000, 0, 1, \bad
    diag_output 0x40021400, 0x0040, 0, 0x0040, 0x00003000, 0x00001000, 0x00002000, 0, 1, \bad
    diag_output 0x40020800, 0x0200, 0, 0x0200, 0x000c0000, 0x00040000, 0x00080000, 0, 1, \bad
    dsb
    diag_rf_output_present 0x40020000, 0x0100, 0x0100, 0x00030000, 0x00010000, 0x00020000, \bad
    diag_rf_output_present 0x40021400, 0x0040, 0, 0x00003000, 0x00001000, 0x00002000, \bad
    diag_rf_output_present 0x40020800, 0x0200, 0, 0x000c0000, 0x00040000, 0x00080000, \bad
    /* BK4802A register33 includes V/U PA bits3/2; MCU gates alone cannot
     * disable them. These are write requests, not RF readback or RF-off proof. */
    diag_rf_write 0x330000, 0x40020000, 0x100, 0x40021400, 0x40, 0x200
    diag_rf_write 0x36007f, 0x40020000, 0x100, 0x40021400, 0x40, 0x200
    diag_rf_write 0x300000, 0x40020000, 0x100, 0x40021400, 0x40, 0x200
    diag_rf_write 0x371d00, 0x40020000, 0x100, 0x40021400, 0x40, 0x200
    dsb
    mov.w r12, #0
.endm


.macro diag_rf_b_off bad
    diag_rf_output_present 0x40020800, 0x8030, 0, 0xc0000f00, 0x40000500, 0x80000a00, \bad
    diag_rf_output_present 0x40020400, 0x0800, 0, 0x00c00000, 0x00400000, 0x00800000, \bad
    diag_rf_output_present 0x40021400, 0x0100, 0, 0x00030000, 0x00010000, 0x00020000, \bad
    diag_rf_output_present 0x40020000, 0x0010, 0, 0x00000300, 0x00000100, 0x00000200, \bad
    diag_rf_unlocked 0x40020800, 0x01c0, \bad
    diag_output 0x40020800, 0, 0x0080, 0x0080, 0x0000c000, 0x00004000, 0x00008000, 0, 1, \bad
    diag_output 0x40020800, 0x0140, 0, 0x0140, 0x00033000, 0x00011000, 0x00022000, 0, 1, \bad
    dsb
    diag_rf_output_present 0x40020800, 0x01c0, 0x0080, 0x0003f000, 0x00015000, 0x0002a000, \bad
    diag_rf_write 0x330000, 0x40020800, 0x80, 0x40020800, 0x100, 0x40
    diag_rf_write 0x36007f, 0x40020800, 0x80, 0x40020800, 0x100, 0x40
    diag_rf_write 0x300000, 0x40020800, 0x80, 0x40020800, 0x100, 0x40
    diag_rf_write 0x371d00, 0x40020800, 0x80, 0x40020800, 0x100, 0x40
    dsb
    diag_rf_output_present 0x40020800, 0x01c0, 0x0180, 0x0003f000, 0x00015000, 0x0002a000, \bad
    mov.w r12, #0
.endm

.section .text.__pre_init, "ax", %progbits
/* The stackless UART continuation uses ADR.W with a word-aligned PC. */
.p2align 2
.global __pre_init
.type __pre_init, %function
.thumb_func
__pre_init:
    mov.w r12, #0
    cpsid i
    /* The working 0.1.2 observer establishes this fixed Cortex-M4/F423
     * bootstrap contract. Do not let an identity/context rejection hide it.
     * Runtime Reset called us with BL; preserve LR and r4-r11 unchanged. */
    diag_load r0, 0xe000e010
    movs r1, #0
    str r1, [r0]
    str r1, [r0, #4]
    str r1, [r0, #8]
    diag_load r0, 0xe000e180
    diag_load r1, 0xe000e280
    mvn r2, #0
    movs r3, #8
.Ldiag_early_irq_bank:
    str r2, [r0], #4
    str r2, [r1], #4
    subs r3, #1
    bne .Ldiag_early_irq_bank
    diag_load r0, 0xe000ed04
    diag_load r1, 0x0a000000
    str r1, [r0]
    diag_load r1, 0x08002800
    str r1, [r0, #4]
    diag_modify 0xe000ed10, 6, 0
    /* Stop bus masters before UART ownership or clock changes. SPI2/3 are
     * gated and held reset; external CS and RF controls await exact guards. */
    diag_modify 0x40023830, 0x01400000, 0
    diag_modify 0x40023834, 0x00000080, 0
    diag_modify 0x40023810, 0, 0x01400000
    diag_modify 0x40023814, 0, 0x00000080
    diag_modify 0x40023840, 0x0000c000, 0
    diag_modify 0x40023820, 0, 0x0000c000
    dsb
    isb
    diag_hick8_bootstrap .Ldiag_pre_init_fail
    /* A watchdog resets the MCU, not necessarily the RF chip. Before ANY UART
     * byte, make the same guarded shutdown attempt at normalized HICK8. The
     * original detailed guards below still diagnose every rejected profile.
     * Early rejection skips pins/frames, then continues to bounded logging. */
    mrs r0, IPSR
    cmp r0, #0
    bne.w .Ldiag_early_rf_done
    mrs r0, CONTROL
    tst r0, #3
    bne.w .Ldiag_early_rf_done
    diag_rf_require 0xe0042020, 0xff00, 0x1200, .Ldiag_early_rf_done
    diag_load r0, 0xe0042000
    ldr r1, [r0]
    diag_load r2, 0x700a3243
    cmp r1, r2
    beq .Ldiag_early_rf_pid_ok
    diag_load r2, 0x700a3246
    cmp r1, r2
    bne.w .Ldiag_early_rf_done
.Ldiag_early_rf_pid_ok:
    diag_rf_require 0xe0042020, 0xff07, 0x1200, .Ldiag_early_rf_done
    diag_load r0, 0x1ffff7e0
    ldrh r1, [r0]
    cmp r1, #256
    bne.w .Ldiag_early_rf_done
    diag_load r0, 0xe000e004
    ldr r1, [r0]
    and r1, r1, #15
    cmp r1, #7
    bhi.w .Ldiag_early_rf_done
    diag_rf_require 0x40023870, 1, 0, .Ldiag_early_rf_done
    diag_modify 0x40023830, 0, 0x27
    diag_rf_require 0x40023830, 0x27, 0x27, .Ldiag_early_rf_failure
    dsb
    diag_rf_a_off .Ldiag_early_a_failed
    diag_rf_b_off .Ldiag_early_rf_failure
    b.w .Ldiag_early_rf_done
.Ldiag_early_a_failed:
    diag_rf_b_off .Ldiag_early_rf_failure
.Ldiag_early_rf_failure:
    /* A partially configured/guard-rejected off attempt must never proceed
     * into ordinary main, even if inherited pin fields happened to look good. */
    mov.w r12, #0
    diag_uart_init
    diag_frame 0x00000300, 0, 0x00000108, 0, 0, 0, 0
    diag_frame 0xe100000a, 1, 0x4002001c, 1, 0x4002081c, 1, 0x4002141c
    b.w .Ldiag_pre_init_fail
.Ldiag_early_rf_done:
    mov.w r12, #0
    diag_uart_init
    /* First frame is constant and follows the guarded RF-off attempt. */
    diag_frame 0x00000300, 0, 0x00000108, 0, 0, 0, 0
    diag_frame 0x00000301, 4, 0, 3, 0, 5, 0
    /* Clock values here are normalized, not entry snapshots. */
    diag_frame 0x00000302, 1, 0x40023800, 1, 0x40023808, 1, 0x40023804

    mrs r0, IPSR
    cmp r0, #0
    bne.w .Ldiag_context_fail
    mrs r0, CONTROL
    tst r0, #3
    bne.w .Ldiag_context_fail

    /* Family/revision/PID/flash remain exact guards for all RF pin requests. */
    diag_load r0, 0xe0042020
    ldr r1, [r0]
    and r1, r1, #0xff00
    cmp r1, #0x1200
    bne.w .Ldiag_family_fail
    diag_frame 0x00000001

    /* Exact permitted MCU variants; this does not identify the radio PCB. */
    diag_load r0, 0xe0042000
    ldr r1, [r0]
    diag_load r2, 0x700a3243
    cmp r1, r2
    beq .Ldiag_pid_ok
    diag_load r2, 0x700a3246
    cmp r1, r2
    bne.w .Ldiag_pid_fail
.Ldiag_pid_ok:
    ldr r1, [r0, #0x20]
    diag_load r2, 0xff07
    and r1, r1, r2
    movw r2, #0x1200
    cmp r1, r2
    bne.w .Ldiag_series_fail
    diag_load r0, 0x1ffff7e0
    ldrh r1, [r0]
    cmp r1, #256
    bne.w .Ldiag_flash_fail

    /* Capture guard inputs before they can reject. Watchdog observations are
     * read-only; the Rust startup keeps the inherited watchdog policy. */
    diag_frame 0x00000303, 1, 0x40023870, 1, 0xe000e004, 1, 0x40023830
    diag_frame 0x00000306, 1, 0x40003004, 1, 0x40003008, 1, 0x40002c00

    /* Stop SysTick, disable/clear external IRQ banks, clear PendSV/SysTick. */
    diag_load r0, 0xe000e010
    movs r1, #0
    str r1, [r0]
    str r1, [r0, #4]
    str r1, [r0, #8]
    diag_load r0, 0xe000e004
    ldr r0, [r0]
    and r0, r0, #15
    adds r0, #1
    cmp r0, #8
    bhi.w .Ldiag_irq_fail
    diag_load r2, 0xe000e180
    diag_load r3, 0xe000e280
    mov.w r1, #0xffffffff
.Ldiag_irq_bank:
    str r1, [r2], #4
    str r1, [r3], #4
    subs r0, #1
    bne .Ldiag_irq_bank
    diag_load r0, 0xe000ed04
    mov.w r1, #0x0a000000
    str r1, [r0]
    diag_load r1, 0x08002800
    str r1, [r0, #4]
    diag_modify 0xe000ed10, 6, 0
    dsb
    isb

    /* Stop independent bus masters without enabling inherited channels. */
    diag_modify 0x40023830, 0x01400000, 0
    diag_modify 0x40023834, 0x00000080, 0
    diag_modify 0x40023810, 0, 0x01400000
    diag_modify 0x40023814, 0, 0x00000080
    dsb

    /* Enable GPIO register clocks to read lock state before the LEXT gate.
     * Enabling these clocks alone does not claim a pin or change a latch. */
    diag_modify 0x40023830, 0, 0x27
    ldr r1, [r0]
    and r1, r1, #0x27
    cmp r1, #0x27
    bne.w .Ldiag_gpio_fail
    dsb
    diag_frame 0x00000304, 1, 0x4002001c, 1, 0x4002041c, 1, 0x4002081c
    diag_frame 0x00000305, 1, 0x4002141c, 1, 0x4000300c, 1, 0x40003010
    /* LEXT owns PC14/15. Reject instead of modifying backup domain state. */
    diag_load r0, 0x40023870
    ldr r1, [r0]
    tst r1, #1
    bne.w .Ldiag_lext_fail

    /* Related-board off/mute requests; PF0 antenna relay is untouched. */
    diag_output 0x40020800, 0x8030, 0, 0x8030, 0xc0000f00, 0x40000500, 0x80000a00, 0
    diag_output 0x40020400, 0x0800, 0, 0x0800, 0x00c00000, 0x00400000, 0x00800000, 0
    diag_output 0x40021400, 0x0100, 0, 0x0100, 0x00030000, 0x00010000, 0x00020000, 0
    diag_output 0x40020000, 0x0010, 0, 0x0010, 0x00000300, 0x00000100, 0x00000200, 0
    diag_frame 0x00000002
    /* RF-A CS + LCD CS, NOR CS, RF-B CS + LCD reset HIGH. */
    diag_output 0x40020000, 0, 0x8100, 0x8100, 0xc0030000, 0x40010000, 0x80020000, 0
    diag_output 0x40020400, 0, 0x1000, 0x1000, 0x03000000, 0x01000000, 0x02000000, 0
    diag_output 0x40020800, 0, 0x4080, 0x4080, 0x3000c000, 0x10004000, 0x20008000, 0
    /* Keyboard columns PB0/1/2/10 released as open-drain HIGH. */
    diag_output 0x40020400, 0, 0x0407, 0x0407, 0x0030003f, 0x00100015, 0x0020002a, 0x0407
    dsb

    /* Reassert SPI2/SPI3 reset after CS requests; first abort preceded clocks. */
    diag_modify 0x40023840, 0x0000c000, 0
    diag_modify 0x40023820, 0, 0x0000c000
    dsb
    isb
    /* A rejected early RF-A pin takeover must never silently return success.
     * These exact final fields can only remain after the guarded off frames;
     * the ordinary GPIO setup above does not initialize PF6 or PC9. */
    diag_rf_unlocked 0x40020000, 0x0100, .Ldiag_rf_off_fail
    diag_rf_unlocked 0x40021400, 0x0040, .Ldiag_rf_off_fail
    diag_rf_unlocked 0x40020800, 0x0200, .Ldiag_rf_off_fail
    diag_rf_output_present 0x40020000, 0x0100, 0x0100, 0x00030000, 0x00010000, 0x00020000, .Ldiag_rf_off_fail
    diag_rf_output_present 0x40021400, 0x0040, 0x0040, 0x00003000, 0x00001000, 0x00002000, .Ldiag_rf_off_fail
    diag_rf_output_present 0x40020800, 0x0200, 0, 0x000c0000, 0x00040000, 0x00080000, .Ldiag_rf_off_fail
    diag_rf_output_present 0x40020800, 0x8030, 0, 0xc0000f00, 0x40000500, 0x80000a00, .Ldiag_rf_off_fail
    diag_rf_output_present 0x40020400, 0x0800, 0, 0x00c00000, 0x00400000, 0x00800000, .Ldiag_rf_off_fail
    diag_rf_output_present 0x40021400, 0x0100, 0, 0x00030000, 0x00010000, 0x00020000, .Ldiag_rf_off_fail
    diag_frame 0x00000003
    bx lr

.Ldiag_pid_fail:
    movs r0, #1
    b .Ldiag_default_fail_frame
.Ldiag_flash_fail:
    movs r0, #2
    b .Ldiag_default_fail_frame
.Ldiag_irq_fail:
    movs r0, #3
    b .Ldiag_default_fail_frame
.Ldiag_lext_fail:
    movs r0, #4
    b .Ldiag_default_fail_frame
.Ldiag_gpio_fail:
    movs r0, #5
    b .Ldiag_default_fail_frame
.Ldiag_series_fail:
    movs r0, #7
    b .Ldiag_default_fail_frame
.Ldiag_rf_off_fail:
    movs r0, #10
.Ldiag_default_fail_frame:
    diag_frame 0xe1000000, code_kind=6, tagged=1
    b.w .Ldiag_pre_init_fail
.Ldiag_gpio_lock_fail:
    diag_frame 0xe1000006, ak=7, bk=8, ck=9, tagged=1
    b.w .Ldiag_pre_init_fail
.Ldiag_context_fail:
    diag_frame 0xe1000008, 4, 0, 3, 0, 5, 0
    b.w .Ldiag_pre_init_fail
.Ldiag_family_fail:
    diag_frame 0xe1000009, 1, 0xe0042020, 0, 0x1200, 0, 0xff00
    b.w .Ldiag_pre_init_fail

/* No Rust handler, watchdog reload, stack access or recovery is attempted.
 * Inherited watchdog can reset the chip; this loop does not promise RF-off.
 */
.Ldiag_pre_init_fail:
    cpsid i
    wfi
    b .Ldiag_pre_init_fail
.size __pre_init, . - __pre_init
.ltorg

/* Also valid before RAM initialization, from HardFault/DefaultHandler/NMI.
 * No return: the GPIO owners have been abandoned. An unsupported memory map
 * can still fault on the identity reads; no general unknown-chip recovery.
 */
.section .text.__diag_terminal_fail_stop, "ax", %progbits
.p2align 2
.global __diag_terminal_fail_stop
.type __diag_terminal_fail_stop, %function
.thumb_func
__diag_terminal_fail_stop:
    mov.w r12, #0
    cpsid i
    diag_load r0, 0xe0042020
    ldr r1, [r0]
    and r1, r1, #0xff00
    cmp r1, #0x1200
    bne.w .Ldiag_pre_init_fail
    diag_modify 0x40023830, 0x01400000, 0
    diag_modify 0x40023834, 0x00000080, 0
    diag_modify 0x40023810, 0, 0x01400000
    diag_modify 0x40023814, 0, 0x00000080
    dsb
    diag_load r0, 0xe0042000
    ldr r1, [r0]
    diag_load r2, 0x700a3243
    cmp r1, r2
    beq .Ldiag_terminal_pid_ok
    diag_load r2, 0x700a3246
    cmp r1, r2
    bne.w .Ldiag_terminal_report
.Ldiag_terminal_pid_ok:
    ldr r1, [r0, #0x20]
    diag_load r2, 0xff07
    and r1, r1, r2
    movw r2, #0x1200
    cmp r1, r2
    bne.w .Ldiag_terminal_report
    diag_load r0, 0x1ffff7e0
    ldrh r1, [r0]
    cmp r1, #256
    bne.w .Ldiag_terminal_report
    /* A fault may arrive before normal startup stopped inherited bus masters. */
    diag_modify 0x40023830, 0x01400000, 0
    diag_modify 0x40023834, 0x00000080, 0
    diag_modify 0x40023810, 0, 0x01400000
    diag_modify 0x40023814, 0, 0x00000080
    dsb
    diag_load r0, 0x40023870
    ldr r1, [r0]
    tst r1, #1
    bne.w .Ldiag_terminal_report
    diag_modify 0x40023830, 0, 0x27
    ldr r1, [r0]
    and r1, r1, #0x27
    cmp r1, #0x27
    bne.w .Ldiag_terminal_report
    dsb
    diag_rf_a_off .Ldiag_terminal_rf_b
.Ldiag_terminal_rf_b:
    diag_rf_b_off .Ldiag_terminal_report
.Ldiag_terminal_report:
    /* RF-low attempts precede terminal clock/UART work. Abandoned SPI must
     * not continue across the frequency change even if a pin guard failed. */
    diag_modify 0x40023840, 0x0000c000, 0
    diag_modify 0x40023820, 0, 0x0000c000
    dsb
    diag_hick8_bootstrap .Ldiag_pre_init_fail
    diag_uart_init
    diag_frame 0xe1ff0000, 3, 0, 1, 0xe000ed28, 1, 0xe000ed2c
    b.w .Ldiag_pre_init_fail
.size __diag_terminal_fail_stop, . - __diag_terminal_fail_stop
.ltorg
.purgem diag_modify
.purgem diag_output
.purgem diag_poll
.purgem diag_settle
.purgem diag_hick8_bootstrap
.purgem diag_rf_require
.purgem diag_rf_unlocked
.purgem diag_rf_output_present
.purgem diag_rf_phase
.purgem diag_rf_write
.purgem diag_rf_b_off
.purgem diag_rf_a_off
