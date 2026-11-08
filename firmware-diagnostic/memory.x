/* Related-board experimental image, not physical UV98 qualification.
 * No internal loader-prefix or external NOR address is linked into the image.
 */
MEMORY
{
    FLASH : ORIGIN = 0x08002800, LENGTH = 256K - 0x2800
    RAM   : ORIGIN = 0x20000000, LENGTH = 48K
}
/* Match the stock and physically exercised 0.1.2 observer entry vectors.
 * Keep cortex-m-rt Reset/RAM initialization; only its linker placement changes.
 */
_stack_start = 0x200099C8;
_stext = 0x08002AC0;
ASSERT(ORIGIN(FLASH) == 0x08002800, "Unexpected application start");
ASSERT(ORIGIN(FLASH) + LENGTH(FLASH) == 0x08040000, "Unexpected flash boundary");
ASSERT(ORIGIN(RAM) + LENGTH(RAM) == 0x2000C000, "Unexpected RAM boundary");
ASSERT(_stack_start == 0x200099C8, "Unexpected diagnostic stack start");
ASSERT(_stext == 0x08002AC0, "Unexpected diagnostic Reset placement");
ASSERT(__euninit + 8K <= _stack_start, "Diagnostic static RAM leaves less than 8K stack");
