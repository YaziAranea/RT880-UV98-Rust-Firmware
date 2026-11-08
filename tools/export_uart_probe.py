"""Export an already audited stackless UART probe; never opens a device."""
import argparse
import hashlib
import json
from pathlib import Path
from check_diag_preinit import read_elf
from check_uart_probe import check_layout

BASE, END = 0x08002800, 0x08040000

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('elf', type=Path)
    parser.add_argument('audit', type=Path)
    parser.add_argument('output', type=Path, help='New directory')
    args = parser.parse_args()
    data, mapped, symbols = read_elf(args.elf)
    digest = hashlib.sha256(data).hexdigest()
    audit = json.loads(args.audit.read_text(encoding='utf-8'))
    if not audit['passed'] or audit['elf_sha256'] != digest:
        raise ValueError('Linked instruction audit does not match this ELF')
    layout = check_layout(data, mapped, symbols)
    image = bytearray(b'\xff' * (END - BASE))
    populated = 0
    for segment in layout['load_segments']:
        address = segment['physical_address']
        address = int(address, 16) if isinstance(address, str) else address
        offset, count = segment['file_offset'], segment['file_bytes']
        if not BASE <= address <= address + count <= END:
            raise ValueError('Protected/out-of-window load')
        image[address-BASE:address-BASE+count] = data[offset:offset+count]
        populated += count
    report = {'version':'0.1.2', 'kind':'minimal_uart_observer_application',
              'elf_sha256':digest, 'bin_sha256':hashlib.sha256(image).hexdigest(),
              'bin_bytes':len(image), 'file_backed_load_bytes':populated,
              'application_base':hex(BASE), 'application_end_exclusive':hex(END),
              'bootloader_prefix_included':False, 'external_nor_included':False,
              'hardware_verified':False, 'device_accessed':False, 'layout':layout}
    args.output.mkdir(parents=True, exist_ok=False)
    for name, content in [('UV98-uart-probe-0.1.2.bin', bytes(image)),
                          ('UV98-uart-probe-0.1.2.elf', data),
                          ('linked-audit.json', args.audit.read_bytes()),
                          ('image-layout.json', (json.dumps(report,indent=2)+'\n').encode())]:
        with (args.output/name).open('xb') as stream: stream.write(content)
        if (args.output/name).read_bytes() != content: raise IOError('Artifact readback mismatch')
    print(json.dumps({k:report[k] for k in ('bin_bytes','file_backed_load_bytes','bin_sha256','elf_sha256')},indent=2))

if __name__ == '__main__': main()
