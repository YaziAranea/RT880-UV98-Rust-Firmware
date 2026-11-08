"""List PE imports without loading or executing the file."""
from pathlib import Path
import json
import struct
import sys


def imports(path):
    data = Path(path).read_bytes()
    def word(offset): return struct.unpack_from('<I', data, offset)[0]
    pe = word(0x3C)
    if data[pe:pe+4] != b'PE\0\0': raise ValueError('Not PE')
    optional = pe+24
    magic = struct.unpack_from('<H', data, optional)[0]
    if magic != 0x20B: raise ValueError('Expected PE32+')
    count = struct.unpack_from('<H', data, pe+6)[0]
    sections = optional + struct.unpack_from('<H', data, pe+20)[0]
    def offset(rva):
        for index in range(count):
            at = sections + index*40
            size, address, rawsize, rawoffset = struct.unpack_from('<IIII', data, at+8)
            if address <= rva < address+max(size, rawsize): return rawoffset+rva-address
        raise ValueError('Unmapped RVA')
    address = word(optional+120)
    if not address: return []
    at = offset(address)
    result = []
    while any(data[at:at+20]):
        name = offset(word(at+12))
        result.append(data[name:data.index(b'\0', name)].decode('ascii'))
        at += 20
    return sorted(set(result), key=str.lower)


if __name__ == '__main__':
    print(json.dumps({'file': Path(sys.argv[1]).name, 'imports': imports(sys.argv[1])}, indent=2))
