# Battery voltage and level on the RT880 profile

## Input

- RT880 V2 schematic, page 3, SHA-256 `b7858c30fbe6eb8bf57967d57d9985b29951ccffa766ca5ac9240796c9fab32a`. The BATT1 rail reaches `LOWBATTY-DET` through R559 (300 kΩ). R560 (47 kΩ) and C547 (102P) shunt the sense node to ground. MCU U502 pin 23 receives this node. U502 pin 10 receives `KEYIN-AD`. The pin labels are visible in [the MCU crop](../board/mcu-detail.png).
- AT32F423 LQFP64 pin table in [datasheet text](../board/at32-datasheet-text.txt), lines 1509–1512 and 1576–1580. Pin 23 is PA7, ADC1_IN7. Pin 10 is PC2, ADC1_IN12.
- RT880-labelled 1.16 reference BIN SHA-256 `eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc` loaded at `0x08002800`. [ADC setup](../board/gpio-board-init.asm) at `0x08002DAA–0x08002DBE` passes ADC1, channel 7, rank 1 and channel 12, rank 2 to `0x08024B72`. That helper writes the channel into the rank's 5-bit OSQ3 field at `0x08024BDA–0x08024BF8`. [DMA setup](../general/adc_dma_config.asm) moves two ordinary ADC data halfwords into `0x200010CC`.

## Stock voltage and level

[Conversion](../general/adc_conversion.asm) at `0x080121D2–0x080121E4` reads the first DMA halfword and stores `floor(raw * 3 / 51) + 2` at `0x200010E4`. The adjacent halfword is copied unchanged for the side keys. `0x08021E74–0x08021E86` copies the voltage code as a byte to `0x2000116A`.

Two stock string builders independently confirm the units. The following excerpt is from the reference image above.

```text
0801c388: movs      r1, #2
0801c38a: ldr       r0, [pc, #0xfc] ; literal = 0x2000116a
0801c38c: ldrb      r0, [r0]
0801c38e: bl        #0x0801b8c4
0801c392: ldr       r0, [pc, #0xdc] ; literal = 0x20003034
0801c394: ldrb      r2, [r0]        ; first digit
0801c3a0: movs      r2, #0x2e       ; '.'
0801c3ac: ldr       r0, [pc, #0xc0] ; literal = 0x20003034
0801c3ae: ldrb      r2, [r0, #1]    ; second digit
0801c3ba: movs      r2, #0x56       ; 'V'
```

`0x0801B8C4–0x0801B8F4` writes the requested number of decimal digits as ASCII at `0x20003034`. The builder appends each indicated byte to its text buffer. The same sequence appears at `0x0801CC08–0x0801CC44`. The stock display unit is therefore 0.1 V. The traced conversion does not consult tuning battery points or the ADC correction byte. The display estimate uses stock arithmetic. Accuracy on the owner's UV98 remains unmeasured.

The [stock indicator](../general/battery_indicator.asm) maps decivolts to an ordinal display value using these points.

| Decivolts | Ordinal |
|---|---:|
| Below 62 | 0 |
| 62–63 | 1 |
| 64–65 | 2 |
| 66 | 3 |
| 67–68 | 4 |
| 69–81 | Decivolts minus 63 |
| 82 or more | 19 |

## Rust ADC behavior

The PA7 and PC2 capabilities are disjoint from other GPIO owners. One foreground driver owns ADC1 and ADCCOM. Both channels use the AT32F423 SDK sampling-time value 7 (640.5 ADC cycles). Ordinary sampling uses rank 1 throughout. Each battery read switches to channel 7, discards one conversion, reads one result, restores channel 12 and discards one conversion. Channel readback and trigger/overflow checks gate returned data. A failed conversion or unverifiable mux selection disables the ADC before best-effort channel 12 restoration. The terminal ADC state yields errors to the key and battery callers.

`BatteryMonitor` displays the first valid sample immediately and averages two successive raw samples before decivolt quantization. The conservative 4.0–9.0 V validity window admits readings around the schematic's nominal 7.4 V pack and the stock 5.8 V shutdown region. It rejects rail/impossible readings, clears the filter and returns no voltage. The validity window controls only the display.

The reference schematic and related stock bytes establish the profile. ADC sampling, voltage accuracy, settling behavior and PTT response on the owner's physical radio require live verification.
