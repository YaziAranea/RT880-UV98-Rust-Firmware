"""Verify that Rust's checked-in oracle fixtures match their ARM-run manifests."""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parent.parent
REFERENCE = 'eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc'


def load(relative):
    value = json.loads((ROOT / relative).read_text(encoding='utf-8'))
    if value['reference_sha256'] != REFERENCE:
        raise ValueError(f'Unexpected reference image: {relative}')
    return value


def verify(relative, expected_hash, expected_rows, comment_header=False):
    path = ROOT / relative
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected_hash:
        raise ValueError(f'Oracle SHA256 mismatch: {relative}')
    lines = [line for line in data.decode('utf-8').splitlines() if line and not line.startswith('#')]
    rows = len(lines) if comment_header else len(lines) - 1
    if rows != expected_rows:
        raise ValueError(f'Oracle row count mismatch: {relative}')
    return {'path': relative, 'rows': rows, 'sha256': expected_hash}


def main():
    frequency = load('research/frequency/oracle-report.json')
    general = load('research/general/evidence.json')
    rf = load('research/rf/oracle-summary.json')
    verified = []
    for name, count in [('frequency-selection.csv', frequency['selection_vectors']),
                        ('frequency-correction.csv', frequency['correction_vectors'])]:
        verified.append(verify('crates/stock-model/testfixtures/' + name,
                               frequency['fixture_sha256'][name], count))
    for name, entry in general['artifacts'].items():
        verified.append(verify('crates/stock-model/tests/fixtures/' + name,
                               entry['sha256'], entry['rows']))
    verified.append(verify('crates/stock-model/tests/fixtures/rf-oracle.csv',
                           rf['fixture_sha256'], rf['vectors'], comment_header=True))
    total = sum(item['rows'] for item in verified)
    if total != 15741:
        raise ValueError(f'Unexpected total oracle coverage: {total}')
    report = {'reference_sha256': REFERENCE, 'scope': 'selected emulated routines only',
              'hardware_qualified': False, 'total_vectors': total, 'fixtures': verified}
    destination = ROOT / 'validation' / 'oracle-integrity.json'
    destination.parent.mkdir(exist_ok=True)
    destination.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f'Oracle fixture hashes verified: {len(verified)} files, {total} reference vectors')


if __name__ == '__main__':
    main()
