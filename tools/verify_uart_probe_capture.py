"""Check actual emulated UART bytes with the production RX decoder; no COM."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    project = Path(__file__).resolve().parent.parent
    capture = project/'tools/boot-log'
    if not capture.is_dir(): capture = project.parent/'live-hardware'
    sys.path.insert(0,str(capture))
    from capture_boot_log import StreamDecoder
    audit = json.loads(args.report.read_text(encoding='utf-8'))
    if not audit['passed']: raise ValueError('Instruction audit failed')
    results = []
    for case in audit['cases']:
        raw = case['wire_ascii'].encode('ascii')
        decoder, events = StreamDecoder(), []
        # One-byte chunks exercise prefix/record boundaries for actual ARM output.
        for byte in raw: events.extend(decoder.feed(bytes([byte])))
        events.extend(decoder.finish())
        words = [[e['code'],*e['details']] for e in events if e['kind']=='record']
        if words != case['frames']: raise AssertionError(case['name'])
        results.append({'name':case['name'],'passed':True,'raw_bytes':len(raw),
                        'raw_sha256':hashlib.sha256(raw).hexdigest(),'decoded_words':words})
    report = {'passed':True,'scope':'linked ARM/MMIO bytes -> host stream decoder',
              'device_accessed':False,'physical_hardware_verified':False,
              'elf_sha256':audit['elf_sha256'],'cases':results,
              'source_report_sha256':hashlib.sha256(args.report.read_bytes()).hexdigest()}
    with args.output.open('x',encoding='utf-8') as stream: json.dump(report,stream,indent=2)
    print(json.dumps({'passed':True,'cases':len(results),'device_accessed':False}))

if __name__ == '__main__': main()
