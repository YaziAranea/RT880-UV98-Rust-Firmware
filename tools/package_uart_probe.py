"""Seal the reviewed UART observer build; no device access."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parent.parent
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build_directory',type=Path)
    args = parser.parse_args()
    folder = args.build_directory.resolve()
    audit=json.loads((folder/'linked-audit.json').read_text(encoding='utf-8'))
    capture=json.loads((folder/'capture-integration.json').read_text(encoding='utf-8'))
    layout=json.loads((folder/'image/image-layout.json').read_text(encoding='utf-8'))
    verification=json.loads((folder/'verification.json').read_text(encoding='utf-8-sig'))
    elf=folder/'image/UV98-uart-probe-0.1.2.elf'
    binary=folder/'image/UV98-uart-probe-0.1.2.bin'
    if not audit['passed'] or not capture['passed'] or verification['status']!='passed':
        raise ValueError('Build/audits failed')
    if not sha(elf)==layout['elf_sha256']==audit['elf_sha256']==capture['elf_sha256']:
        raise ValueError('ELF evidence mismatch')
    if sha(binary)!=layout['bin_sha256'] or sha(folder/'linked-audit.json')!=capture['source_report_sha256']:
        raise ValueError('BIN/audit evidence mismatch')
    sources = [p for name in ('crates','apps','firmware','firmware-diagnostic','firmware-uart-probe',
                              'tools','research','reference','.cargo')
               for p in (ROOT/name).rglob('*')
               if p.is_file() and '__pycache__' not in p.parts and p.suffix!='.pyc']
    sources += [p for p in ROOT.iterdir() if p.is_file() and p.suffix in ('.toml','.lock','.ps1','.txt')]
    with zipfile.ZipFile(folder/'source.zip','x',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for path in sorted(set(sources)): z.write(path,path.relative_to(ROOT).as_posix())
        for name in ('capture_boot_log.py','test_capture_boot_log.py','BOOT-LOG-CAPTURE-RU.txt'):
            z.write(ROOT.parent/'live-hardware'/name,'tools/boot-log/'+name)
    (folder/'capture').mkdir()
    for name in ('capture_boot_log.py','test_capture_boot_log.py','BOOT-LOG-CAPTURE-RU.txt'):
        with (folder/'capture'/name).open('xb') as out: out.write((ROOT.parent/'live-hardware'/name).read_bytes())
    with (folder/'README-RU.txt').open('xb') as out: out.write((ROOT/'firmware-uart-probe/README-RU.txt').read_bytes())
    manifest={'kind':'uart_observer_application_build','version':'0.1.2',
              'source_files':len(set(sources))+3,'device_accessed_during_build':False,
              'files':[{'path':p.relative_to(folder).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p)}
                       for p in sorted(folder.rglob('*')) if p.is_file()]}
    with (folder/'manifest.json').open('x',encoding='utf-8') as out: json.dump(manifest,out,indent=2)
    archive=Path(str(folder)+'.zip')
    with zipfile.ZipFile(archive,'x',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for path in sorted(folder.rglob('*')):
            if path.is_file(): z.write(path,path.relative_to(folder).as_posix())
    with zipfile.ZipFile(archive) as z:
        if z.testzip(): raise ValueError('ZIP integrity failed')
        for item in manifest['files']:
            if hashlib.sha256(z.read(item['path'])).hexdigest()!=item['sha256']: raise ValueError('ZIP file mismatch')
    with Path(str(archive)+'.sha256').open('x',encoding='ascii') as out: out.write(sha(archive)+'  '+archive.name+'\n')
    print(json.dumps({'archive':str(archive),'sha256':sha(archive),'source_files':manifest['source_files']},indent=2))

if __name__=='__main__': main()
