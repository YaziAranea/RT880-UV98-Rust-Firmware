"""Compare already decompiled CPS sources; never load or execute the target EXE."""

from pathlib import Path
import hashlib
import json
import re

HERE = Path(__file__).resolve().parent
RT_MAIN = HERE / 'rt880-v110-decompiled' / 'DM550_NOLCD' / 'MainForm.cs'
UV_MAIN = HERE.parent / 'uv98-cps-decompiled' / 'DM550_NOLCD' / 'MainForm.cs'
METHODS = ['mbtnOpen_Click', 'mbtnSave_Click', 'ArraySum', 'Comm_DataReceived',
           'RevDateOperation', 'SaveData', 'DataToPanel', 'SaveSingle',
           'SaveCHName', 'SaveSingleFM', 'GetInt32Data', 'GetInt16Data',
           'ReadCsvColumn', 'btnCHImport_Click', 'btnCHExport_Click']


def digest(data):
    return hashlib.sha256(data).hexdigest()


def extract_method(text, name):
    match = re.search(r'^\t(?:public|private|protected)[^\n]*\b' + re.escape(name)
                      + r'\([^\n]*\)\n\t\{\n', text, re.M)
    if not match:
        return None
    end = text.index('\n\t}\n', match.end()) + len('\n\t}\n')
    return text[match.start():end], text[:match.start()].count('\n') + 1


def main():
    rt = RT_MAIN.read_text(encoding='utf-8-sig')
    uv = UV_MAIN.read_text(encoding='utf-8-sig')
    comparisons = {}
    for name in METHODS:
        left, right = extract_method(rt, name), extract_method(uv, name)
        comparisons[name] = {
            'rt880_line': left[1] if left else None,
            'uv98_line': right[1] if right else None,
            'same_normalized_decompiled_body': bool(left and right and left[0] == right[0]),
            'rt880_method_sha256': digest(left[0].encode()) if left else None,
            'uv98_method_sha256': digest(right[0].encode()) if right else None,
        }
        if name in ('mbtnOpen_Click', 'mbtnSave_Click'):
            comparisons[name]['same_except_extension'] = bool(
                left and right and left[0].replace('.880dmf', '.98pdmf') == right[0])
    sizes = {name: int(size) for name, size in re.findall(
        r'public byte\[\] (buf\w+) = new byte\[(\d+)\]', rt)}
    save, _line = extract_method(rt, 'mbtnSave_Click')
    names = re.findall(r'fileStream.Write\((buf\w+), 0, \1.Length\)', save)
    offset, sections = 0, []
    for name in names:
        sections.append({'buffer': name, 'file_offset': offset, 'bytes': sizes[name]})
        offset += sizes[name]
    report = {
        'target_exe': 'RT-880 CPS_V1_10_250806.exe',
        'target_sha256': 'b2533229c783c86150a4230a8a51c5ee274092e9f1b036bbb7a5b511c34dabd6',
        'target_bytes': 1677824,
        'target_executed': False, 'device_accessed': False,
        'method': 'ILSpy 8.2.0 static decompilation, source inspection only',
        'rt880_mainform_sha256': digest(RT_MAIN.read_bytes()),
        'uv98_mainform_sha256': digest(UV_MAIN.read_bytes()),
        'config_file': {'extension': '.880dmf', 'expected_saved_bytes': offset,
                        'sections': sections, 'header_or_file_checksum_in_save_handler': False},
        'method_comparison': comparisons,
        'same_supporting_sources': {
            name: (RT_MAIN.parent / name).read_bytes() == (UV_MAIN.parent / name).read_bytes()
            for name in ('ReadForm.cs', 'WriteForm.cs', 'PortForm.cs', 'NewFileForm.cs', 'DesClass.cs')
        },
        'same_default_arrays': {},
    }
    for name in ('bufCFGInit', 'bufVFOInit', 'bufAPRSInit'):
        pattern = r'public byte\[\] ' + name + r' = new byte\[\d+\]\n\t\{(.*?)\n\t\};'
        left, right = re.search(pattern, rt, re.S), re.search(pattern, uv, re.S)
        report['same_default_arrays'][name] = bool(left and right and left[1] == right[1])
    (HERE / 'triage.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'file_bytes': offset, 'method_comparison': {
        name: value['same_normalized_decompiled_body'] for name, value in comparisons.items()
    }}, indent=2))


if __name__ == '__main__':
    main()
