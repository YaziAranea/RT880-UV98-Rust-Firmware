ENTRY(Reset)
MEMORY {
  FLASH (rx) : ORIGIN = 0x08002800, LENGTH = 0x3D800
  RAM (rw) : ORIGIN = 0x20000000, LENGTH = 0xC000
}
SECTIONS {
  .vector_table ORIGIN(FLASH) : ALIGN(4) {
    KEEP(*(.vector_table))
  } > FLASH
  .text : ALIGN(4) {
    KEEP(*(.text.Reset))
    *(.text .text.*)
  } > FLASH
  .rodata : ALIGN(4) { *(.rodata .rodata.*) } > FLASH
  .data : ALIGN(4) { *(.data .data.*) } > RAM AT> FLASH
  .bss (NOLOAD) : ALIGN(4) { *(.bss .bss.*) *(COMMON) } > RAM
  /DISCARD/ : { *(.ARM.exidx* .ARM.extab*) }
}
ASSERT(SIZEOF(.vector_table) == 704, "Expected stock-sized vector prefix");
ASSERT(ADDR(.text) == 0x08002AC0, "Reset placement differs from working stock");
ASSERT(SIZEOF(.data) == 0, "Observer must not need initialized RAM");
ASSERT(SIZEOF(.bss) == 0, "Observer must not need zeroed RAM");
ASSERT(ADDR(.rodata) + SIZEOF(.rodata) <= 0x08040000, "Application overflow");
