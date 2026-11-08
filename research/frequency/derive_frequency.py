"""Extract and exercise bounded RT880 reference routines entirely offline.

Uses the parent's hash-pinned StockArm emulator: Flash/RAM only, no MMIO.
SPI reads and register-bus writes are explicit RAM-only test doubles.
No serial/network/device access and no firmware boot flow.
"""

from pathlib import Path
import argparse
import csv
import hashlib
import json
import os
import random
import re
import struct
import sys

HERE = Path(__file__).resolve().parent
WORK = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))
from stock_emu import StockArm, GPRS, REFERENCE_SHA256, DEFAULT_IMAGE, UC_HOOK_CODE

BASE = 0x08002800
SELECTOR = 0x08012A30
RECORD_RAM = 0x20003064
CACHE_SLOT = 0x2000081A
SOURCE_FLAG = 0x200010E9
SPI_READ = 0x08025D30

# Branch order and exact unsigned inclusive comparisons from 0x08012A30.
# Frequencies are stock integer codes; no measured physical units are asserted.
WINDOWS = [
    (13600000, 17400000, 13500000, 0),
    (40000000, 48000000, 40000000, 1),
    (1800000, 6400000, 1500000, 2),
    (6400000, 13600000, 6000000, 3),
    (17400000, 24000000, 17000000, 4),
    (24000000, 32000000, 24000000, 5),
    (32000000, 40000000, 32000000, 6),
    (48000000, 56000000, 48000000, 7),
    (56000000, 66000000, 56000000, 8),
    (84000000, 92000000, 84000000, 9),
    (92000000, 100000000, 92000000, 10),
    (100000000, 108000000, 100000000, 11),
    (108000000, 116000000, 108000000, 12),
    (116000000, 124000000, 116000000, 13),
    (124000000, 130000000, 124000000, 13),
]


def expected_selection(frequency):
    for lower, upper, origin, slot in WINDOWS:
        if lower <= frequency <= upper:
            return slot, min(((frequency - origin) // 500000) & 255, 15)
    return None


def record(slot):
    data = bytearray(112)
    struct.pack_into('<H', data, 0, 0x8000 + slot - 7)
    for row in range(6):
        for point in range(16):
            data[16 + row * 16 + point] = (slot * 13 + row * 17 + point) & 255
    return bytes(data)


def annotated_disassembly():
    sys.path.insert(0, str(WORK / 'hardware_readback' / 'deps'))
    from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_LITTLE_ENDIAN

    image = DEFAULT_IMAGE.read_bytes()
    assert hashlib.sha256(image).hexdigest() == REFERENCE_SHA256
    spans = [(0x08003CCC, 0x08003CFC), (0x08004B88, 0x08004BB8),
             (0x08012A30, 0x08012E18), (0x080081C4, 0x08008214)]
    disassembler = Cs(CS_ARCH_ARM, CS_MODE_THUMB | CS_MODE_LITTLE_ENDIAN)
    disassembler.skipdata = True
    output = [f'SHA256 {REFERENCE_SHA256}', f'Mapped binary base {BASE:#010x}',
              'Literal pools are data, even where the linear sweep renders instructions.']
    for start, end in spans:
        output.append(f'\nRANGE {start:#010x}..{end:#010x} exclusive')
        for instruction in disassembler.disasm(image[start - BASE:end - BASE], start):
            address = instruction.address
            line = f'{address:08x}: {instruction.bytes.hex():<10} {instruction.mnemonic:<10} {instruction.op_str}'
            match = re.search(r'ldr(?:\.w)?\s+\w+, \[pc, #(0x[0-9a-f]+|\d+)\]', line)
            if match:
                literal = ((address + 4) & ~3) + int(match[1], 0)
                value = struct.unpack_from('<I', image, literal - BASE)[0]
                line += f' ; [{literal:#010x}]={value:#010x} ({value})'
            output.append(line)
    (HERE / 'bounded-disassembly.txt').write_text('\n'.join(output) + '\n', encoding='utf-8')


def selector_oracle():
    arm = StockArm()
    reads, points = [], []

    def read_spi(model):
        dest, offset, length = (model.uc.reg_read(reg) for reg in GPRS[:3])
        assert dest == RECORD_RAM and length == 112
        base_offset = offset % 4096
        assert base_offset >= 32 and (base_offset - 32) % 112 == 0
        slot = (base_offset - 32) // 112
        assert 0 <= slot < 15 and offset < 8192
        reads.append((offset, slot))
        model.write(dest, record(slot))
        return 0

    arm.intercept(SPI_READ, read_spi)
    arm.hooks.append(arm.uc.hook_add(
        UC_HOOK_CODE, lambda uc, pc, size, user: points.append(uc.reg_read(GPRS[5])),
        begin=0x08012D40, end=0x08012D40))
    values = {0, 1, 0xFFFFFFFF, 14500000, 43350000, 70000000, 80000000}
    for lower, upper, origin, _slot in WINDOWS:
        values.update([lower - 1, lower, lower + 1, upper - 1, upper, upper + 1])
        for point in (0, 1, 7, 14, 15, 16):
            edge = origin + point * 500000
            values.update([edge - 1, edge, edge + 1])
    randomizer = random.Random(0x12A30)
    values.update(randomizer.randrange(0, 140000001) for _ in range(100))
    rows = []
    for frequency in sorted(values):
        for mode in (0, 1):
            reads.clear()
            points.clear()
            arm.write(CACHE_SLOT, b'\xff' + b'\xa5' * 6)
            arm.write(SOURCE_FLAG, b'\0')
            arm.write(0x20000838, b'\xa5\xa5')
            arm.call(SELECTOR, frequency, mode)
            expected = expected_selection(frequency)
            if expected is None:
                assert not reads and not points
                assert arm.read(CACHE_SLOT, 7) == b'\xff' + b'\xa5' * 6
                rows.append((frequency, mode, 0, -1, -1, -1))
                continue
            slot, point = expected
            assert points == [point], (frequency, points, expected)
            assert reads == [(32 + slot * 112, slot)]
            assert arm.read(CACHE_SLOT, 1)[0] == slot
            state = arm.read(0x2000081B, 6)
            expected_bytes = [record(slot)[16 + row * 16 + point] for row in range(6)]
            assert state[:2] == bytes(expected_bytes[:2])
            # The stock stores the wide/narrow squelch outputs in a different order.
            if mode == 0:
                assert state[2:] == bytes(expected_bytes[i] for i in (2, 4, 3, 5))
            else:
                assert state[2:] == b'\xa5' * 4
            rows.append((frequency, mode, 1, arm.read(CACHE_SLOT, 1)[0], points[0], reads[0][0]))

    # Cache and source selection are separate from the pure selector API.
    ancillary = []
    for source in (0, 1, 2, 3):
        reads.clear()
        points.clear()
        arm.write(CACHE_SLOT, b'\xff')
        arm.write(SOURCE_FLAG, bytes([source]))
        arm.write(RECORD_RAM, b'\xa5' * 112)
        arm.call(SELECTOR, 14500000, 0)
        loaded = arm.read(RECORD_RAM, 112)
        if source <= 1:
            assert reads == [(source * 4096 + 32, 0)] and loaded == record(0)
        elif source == 2:
            assert not reads and loaded == arm.read(0x08028344 + 32, 112)
        else:
            assert not reads and loaded == b'\xa5' * 112
        ancillary.append({'source_flag': source, 'reads': reads.copy(),
                          'record_sha256': hashlib.sha256(loaded).hexdigest()})
    reads.clear()
    arm.write(CACHE_SLOT, b'\0')
    arm.write(SOURCE_FLAG, b'\0')
    arm.write(RECORD_RAM, b'\x5a' * 112)
    arm.call(SELECTOR, 14500000, 0)
    assert not reads and arm.read(RECORD_RAM, 112) == b'\x5a' * 112
    return rows, ancillary


def correction_oracle():
    rows = []
    for routine, writer in ((0x08003CCC, 0x0801E6D4), (0x08004B88, 0x0801E660)):
        arm = StockArm()
        writes = []
        arm.intercept(SELECTOR, lambda model: 0)  # Isolate correction with caller-supplied record.

        def capture(model):
            writes.append(tuple(model.uc.reg_read(reg) for reg in GPRS[:2]))
            return 0

        arm.intercept(writer, capture)
        for frequency in (0, 1, 32767, 32768, 14500000, 43350000, 0xFFFF8000, 0xFFFFFFFF):
            for raw in (0, 1, 32767, 32768, 32769, 65535):
                writes.clear()
                arm.write(RECORD_RAM, struct.pack('<H', raw))
                arm.call(routine, frequency)
                corrected = (frequency + raw - 32768) & 0xFFFFFFFF
                assert writes == [(0x38, corrected & 65535), (0x39, corrected >> 16)]
                observed_code = writes[0][1] | (writes[1][1] << 16)
                rows.append((f'{routine:#010x}', frequency, raw, observed_code,
                             writes[0][1], writes[1][1]))
    return rows


def write_csv(path, header, rows):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('w', newline='', encoding='utf-8') as output:
        writer = csv.writer(output, lineterminator='\n')
        writer.writerow(header)
        writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-disassembly', action='store_true',
                        help='Regenerate machine oracles without the optional Capstone dependency')
    args = parser.parse_args()
    HERE.mkdir(parents=True, exist_ok=True)
    if not args.skip_disassembly:
        annotated_disassembly()
    selections, ancillary = selector_oracle()
    corrections = correction_oracle()
    project = Path(os.environ.get('UV98_RUST_PROJECT', WORK / 'uv98-rust-stage'))
    fixtures = project / 'crates' / 'stock-model' / 'testfixtures'
    write_csv(fixtures / 'frequency-selection.csv',
              ('frequency_code', 'mode', 'valid', 'band_index', 'table_index', 'record_offset'), selections)
    write_csv(fixtures / 'frequency-correction.csv',
              ('routine', 'frequency_code', 'offset_raw', 'corrected_code', 'reg38', 'reg39'), corrections)
    report = {'reference_sha256': REFERENCE_SHA256, 'hardware_validation': False,
              'method': 'bounded offline Cortex-M4 emulation; no MMIO mapped',
              'selection_vectors': len(selections), 'correction_vectors': len(corrections),
              'source_and_cache_checks': ancillary, 'cached_record_reused_without_spi_read': True,
              'stubbed_functions': {'selector_oracle': ['0x08025D30 SPI read: synthetic RAM record'],
                                   'correction_oracle': ['0x08012A30 selector: no-op',
                                                        '0x0801E6D4/0x0801E660: capture register arguments']},
              'windows_in_branch_order': [dict(lower=a, upper=b, index_origin=c, band_index=d)
                                          for a, b, c, d in WINDOWS],
              'fixture_sha256': {name: hashlib.sha256((fixtures / name).read_bytes()).hexdigest()
                                 for name in ('frequency-selection.csv', 'frequency-correction.csv')}}
    (HERE / 'oracle-report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f'PASS: {len(selections)} selector vectors, {len(corrections)} correction vectors; offline only')


if __name__ == '__main__':
    main()
