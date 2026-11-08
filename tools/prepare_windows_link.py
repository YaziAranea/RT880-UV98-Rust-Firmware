"""Create only an OpenGL import library for Rust's self-contained gnullvm linker.

Reads names from the installed Windows DLL. Does not load or copy DLL code,
install an SDK, or change Windows/toolchain configuration.
"""
import argparse
import os
from pathlib import Path
import re
import struct
import subprocess


def exported_names(data):
    def word(offset):
        return struct.unpack_from('<I', data, offset)[0]
    pe = word(0x3C)
    if data[pe:pe+4] != b'PE\0\0' or struct.unpack_from('<H', data, pe+4)[0] != 0x8664:
        raise ValueError('Expected x64 Windows PE')
    optional = pe + 24
    if struct.unpack_from('<H', data, optional)[0] != 0x20B:
        raise ValueError('Expected PE32+')
    count = struct.unpack_from('<H', data, pe+6)[0]
    section_start = optional + struct.unpack_from('<H', data, pe+20)[0]
    def offset(rva):
        for index in range(count):
            at = section_start + index*40
            virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from('<IIII', data, at+8)
            if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
                return raw_offset + rva - virtual_address
        raise ValueError(f'Unmapped export RVA {rva:x}')
    exports = offset(word(optional+112))
    names_count = word(exports+24)
    names_offset = offset(word(exports+32))
    result = []
    for index in range(names_count):
        start = offset(word(names_offset+index*4))
        name = data[start:data.index(b'\0', start)].decode('ascii')
        if not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*', name):
            raise ValueError('Unexpected OpenGL export name')
        result.append(name)
    return sorted(set(result))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sysroot', required=True, type=Path)
    args = parser.parse_args()
    project = Path(__file__).resolve().parent.parent
    destination = project/'target'/'windows-imports'
    destination.mkdir(parents=True, exist_ok=True)
    source = Path(os.environ['WINDIR'])/'System32'/'opengl32.dll'
    names = exported_names(source.read_bytes())
    definition = destination/'opengl32.def'
    definition.write_text('LIBRARY OPENGL32.dll\nEXPORTS\n' + '\n'.join(names) + '\n', encoding='ascii')
    linker = args.sysroot/'lib/rustlib/x86_64-pc-windows-gnullvm/bin/rust-lld.exe'
    subprocess.run([str(linker), '-flavor', 'link', '/lib', '/machine:x64',
                    '/def:'+str(definition), '/out:'+str(destination/'libopengl32.a')], check=True)
    print(f'Prepared local OpenGL import library: {len(names)} exported names, no DLL code copied')


if __name__ == '__main__':
    main()
