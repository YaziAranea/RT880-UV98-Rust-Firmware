/* Research profile inferred from related RT-880 sources, NOT hardware-qualified.
 * The stock loader is outside the application FLASH region.
 * External SPI storage, including calibration, is not mapped here.
 */
MEMORY
{
    FLASH : ORIGIN = 0x08002800, LENGTH = 256K - 0x2800
    RAM   : ORIGIN = 0x20000000, LENGTH = 48K
}

ASSERT(ORIGIN(FLASH) == 0x08002800, "Unexpected application start");
ASSERT(ORIGIN(FLASH) + LENGTH(FLASH) == 0x08040000, "Unexpected flash boundary");
ASSERT(ORIGIN(RAM) + LENGTH(RAM) == 0x2000C000, "Unexpected RAM boundary");
