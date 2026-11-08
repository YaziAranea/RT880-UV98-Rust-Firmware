"""Generate bounded RF vectors from the reviewed machine code, entirely offline.

Writes CSV/evidence only. MMIO stays unmapped. Register I/O is intercepted;
mid-function slices stop at explicit branch boundaries. No target boot or TX.
"""
from pathlib import Path
import csv
import hashlib
import json
import os
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stock_emu import StockArm, REFERENCE_SHA256, UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_PC, UC_HOOK_CODE

PROJECT = Path(os.environ.get('UV98_RUST_PROJECT', str(HERE.parents[1] / 'uv98-rust-stage')))
OUT = PROJECT / 'crates' / 'stock-model' / 'tests' / 'fixtures' / 'rf-oracle.csv'
rows = []
counts = {}


def add(kind, *values):
    rows.append([kind, *values])
    counts[kind] = counts.get(kind, 0) + 1


def capture_machine():
    machine = StockArm()
    writes = []
    def capture(m):
        writes.append((m.uc.reg_read(UC_ARM_REG_R0), m.uc.reg_read(UC_ARM_REG_R1) & 0xFFFF))
        return 0
    machine.intercept(0x0801E660, capture)
    machine.intercept(0x0801E6D4, capture)
    return machine, writes


def stop_at(machine, address):
    # Return from a reviewed slice before reaching its enclosing function's
    # epilogue. No stack/entry state is claimed for the enclosing function.
    def stop(uc, pc, size, user):
        uc.reg_write(UC_ARM_REG_PC, StockArm.STOP)
    machine.hooks.append(machine.uc.hook_add(UC_HOOK_CODE, stop, begin=address, end=address))


m, writes = capture_machine()
for value in [*range(256), 256, 65535, 0xFFFFFFFF]:
    expected = None
    for entry in (0x08004F0C, 0x08003E08):
        writes.clear()
        m.call(entry, value)
        assert len(writes) == 1 and writes[0][0] == 0x36
        if expected is not None:
            assert writes[0][1] == expected
        expected = writes[0][1]
    add('power_helper', value, expected)

for value in (0, 1, 18, 63, 255, 256, 0x16BF, 0x16C0, 0xFFFF, 0x10000, 0xFFFFFFFF):
    expected = None
    for entry in (0x08004E90, 0x08003DE8):
        writes.clear()
        m.call(entry, value)
        assert len(writes) == 1 and writes[0][0] == 0x7D
        if expected is not None:
            assert writes[0][1] == expected
        expected = writes[0][1]
    add('mic_helper', value, expected)

read_value = 0
def register_read(m):
    return read_value
m.intercept(0x0801E620, register_read)
m.intercept(0x0801E698, register_read)
for reg1 in (*range(10), 0xFFFF):
    read_value = reg1
    for value in (*range(16), *(0xB340 + i for i in range(16)), 0xFFFF, 0xFFFE):
        writes.clear()
        m.call(0x08005768, value)
        assert len(writes) == 1 and writes[0][0] == 0x48
        add('volume_helper', value, reg1, writes[0][1])

for value in (0, 1, 7, 8, 15, 0xA5A5, 0xFFFF):
    read_value = value
    expected = None
    for entry in (0x08004518, 0x08003C20):
        writes.clear()
        m.call(entry)
        assert [address for address, _ in writes] == [0x31, 0x28, 0x29]
        if expected is not None:
            assert writes == expected
        expected = writes.copy()
    add('compander', value, *(word for _, word in expected))

# Field ordering is identical to the Rust FilterField ordering.
for field, (direction, kind) in enumerate(((1, 0), (1, 1), (0, 0), (0, 1))):
    for index in (*range(9), 15, 0xF0, 255):
        expected = None
        for entry in (0x080038D0, 0x08003A2C):
            writes.clear()
            m.call(entry, direction, kind, index)
            assert len(writes) in (1, 2)
            if expected is not None:
                assert writes == expected
            expected = writes.copy()
        second = expected[1] if len(expected) == 2 else (0, 0)
        add('filter', field, index, *expected[0], int(len(expected) == 2), *second)

m = StockArm()
codes = []
def capture_code(machine):
    codes.append(machine.uc.reg_read(UC_ARM_REG_R0))
    return 0
m.intercept(0x08004F0C, capture_code)
m.intercept(0x08003E08, capture_code)
stop_at(m, 0x080055BC)
stop_at(m, 0x0800416A)
m.word(0x200010EC, 0x20006000)
m.write(0x200011FA, b'\0')
m.write(0x2000110E, b'\0')  # other-record path consequently selects record 1
for path, entry, channel in ((0, 0x08005552, 0x20006000), (1, 0x080040DC, 0x20003B1C)):
    for mode in (0, 1, 2, 3, 255):
        for high, low in ((0, 0), (0, 255), (1, 0), (43, 28), (42, 20), (255, 255)):
            m.write(channel + 0x10, bytes([mode]))
            m.write(0x2000081B, bytes([high, low]))
            codes.clear()
            m.call(entry)
            assert len(codes) == 1
            add('power_select', path, mode, high, low, codes[0])

m, writes = capture_machine()
stop_at(m, 0x08005530)
stop_at(m, 0x080040CA)
m.word(0x200010EC, 0x20006000)
for path, entry, channel in ((0, 0x080054CC, 0x20006000), (1, 0x080040A8, 0x20003B1C)):
    for byte3 in (0, 1, 255):
        for byte13 in (0, 1, 0x10, 0xFF):
            for wide, narrow in ((0, 65535), (199, 201), (200, 199), (201, 200), (0x34E2, 0xFABC), (65535, 0)):
                m.write(channel + 3, bytes([byte3]))
                m.write(channel + 0x13, bytes([byte13]))
                m.write(0x20003067, wide.to_bytes(2, 'little'))
                m.write(0x2000306E, narrow.to_bytes(2, 'little'))
                writes.clear()
                # 040A8 is entered with r0=1, selecting the second channel record.
                m.call(entry, 1)
                assert len(writes) == 1 and writes[0][0] == 0x40
                add('deviation_select', path, byte3, byte13, wide, narrow, writes[0][1])

OUT.parent.mkdir(parents=True, exist_ok=True)
with OUT.open('w', newline='', encoding='utf-8') as f:
    f.write('# machine-code oracle; RT880 reference SHA256 ' + REFERENCE_SHA256 + '\n')
    csv.writer(f, lineterminator='\n').writerows(rows)
summary = {
    'reference_sha256': REFERENCE_SHA256,
    'fixture_sha256': hashlib.sha256(OUT.read_bytes()).hexdigest(),
    'vectors': len(rows),
    'counts': counts,
    'output': OUT.relative_to(PROJECT).as_posix(),
    'mmio_mapped': False,
    'intercepts': ['0801E660/0801E6D4 capture register writes',
                   '0801E620/0801E698 return supplied register read value',
                   '08004F0C/08003E08 capture power argument in selection slices'],
    'slice_stops': ['080055BC', '0800416A', '08005530', '080040CA'],
    'scope': 'Selected pure/basic-block behavior only; not startup or whole-firmware execution',
}
(HERE / 'oracle-summary.json').write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8')
print(json.dumps(summary, indent=2))
