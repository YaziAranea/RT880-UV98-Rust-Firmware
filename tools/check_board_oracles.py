"""Check provenance and checked-in key/display oracle fixture hashes.

This does not rerun ARM code or validate hardware. Regeneration scripts live
under research/ and require the pinned reference image plus Unicorn/Capstone.
"""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REFERENCE = 'eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc'

def manifest(path):
    data = json.loads((ROOT / path).read_text(encoding='utf-8'))
    if data['reference_sha256'] != REFERENCE:
        raise ValueError(f'Wrong reference: {path}')
    return data

def main():
    keys = manifest('research/keys/oracle-summary.json')
    display = manifest('research/display/oracle.json')
    clock = manifest('research/startup/clock-oracle.json')
    board = manifest('research/board/oracle-summary.json')
    lcd = manifest('research/lcd-hardware/oracle.json')
    rf = manifest('research/rf-startup/oracle-summary.json')
    keyscan = manifest('research/keyscan-hardware/oracle.json')
    bands = manifest('research/band-switch/oracle-summary.json')
    bus = manifest('research/rf-bus/oracle-summary.json')
    receive = manifest('research/receive-plan/oracle-summary.json')
    runtime = manifest('research/rx-runtime/oracle-summary.json')
    checked = [(bus['fixture'], bus['fixture_sha256'])]
    checked.extend(('research/receive-plan/' + name, digest) for name, digest in receive['hashes'].items())
    checked.extend(('research/rx-runtime/' + name, digest) for name, digest in runtime['hashes'].items())
    for entry in keys['fixtures']:
        checked.append((entry['path'], entry['sha256']))
    for name, sha256 in display['fixtures_sha256'].items():
        checked.append(('crates/display/tests/fixtures/' + name, sha256))
    checked.append((board['output'], board['fixture_sha256']))
    checked.append(('crates/lcd-at32/tests/fixtures/stock-lcd-init.csv', lcd['fixture_sha256']))
    checked.append((rf['output'], rf['fixture_sha256']))
    for name, sha256 in keyscan['fixture_sha256'].items():
        checked.append(('crates/keyscan-at32/tests/fixtures/' + name, sha256))
    checked.extend(bands['sha256'].items())
    for path, expected in checked:
        if hashlib.sha256((ROOT / path).read_bytes()).hexdigest() != expected:
            raise ValueError(f'Fixture changed: {path}')
    if len(clock['tests']) != 4 or not all(t['actual_hz'] == t['expected_hz'] for t in clock['tests']):
        raise ValueError('Clock evidence mismatch')
    report = dict(reference_sha256=REFERENCE, hardware_qualified=False,
                  verified_fixtures=[dict(path=p,sha256=h) for p,h in checked],
                  key_decoder_input_rows=keys['decoder_inputs'],
                  key_decoder_calls=keys['decoder_calls'],
                  key_row_cases=keys['row_sampler_cases'],
                  display_prefix_events=len(display['control_prefix_events']),
                  display_word_cases=len(display['pixel_word_vectors']),
                  clock_cases=4,
                  rf_bus_cases=bus['cases'],
                  rf_bus_events=bus['events'],
                  band_selector_cases=bands['selector_cases'],
                  receive_composition_cases=receive['composition_cases'],
                  receive_frequency_cases=receive['frequency_cases'],
                  receive_width_cases=receive['width_cases'],
                  rx_runtime_cases=runtime['counts'],
                  spi_init_cases=board['case_count'],
                  lcd_spi3_init_cases=len(lcd['traces']),
                  rf_request_cases=rf['case_count'],
                  rf_request_events=rf['vector_count'],
                  scope='Fixture integrity only; ARM oracle regeneration is a separate offline step')
    out = ROOT / 'validation/board-oracle-integrity.json'
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(f'Board/key/display oracle fixture hashes verified: {len(checked)} files; clock report: 4 cases')

if __name__ == '__main__':
    main()
