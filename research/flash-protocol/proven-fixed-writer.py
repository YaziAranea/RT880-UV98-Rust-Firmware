"""Explicitly authorized iRadio application flash using pinned nicsure framing.

Only explicitly listed, reviewed fixed-hash application images are accepted.
No automatic retries, END, Unlock, arbitrary addresses, or recovery-on-error.
The first transmitted packet belongs to the erase sequence, not a read probe.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'work/uv98-rust-stage/research/recovery'))
import image_preflight

IMAGES = {
    'diagnostic018r5': ('outputs/UV98-Rust-diagnostic-0.1.8-2026-11-09-r5-build2/image/UV98-diag-0.1.8.bin',
                       'bde124a26b800352b9c75b073548f114186241f7e57024dcb5f22c3786ed5307'),
    'diagnostic018r4tx2': ('outputs/UV98-Rust-diagnostic-0.1.8-2026-11-09-r4-tx2-build2/image/UV98-diag-0.1.8.bin',
                         'd1c2c4ddd0422ad9ae4828360f62ae5ae306db888432e9faaa185024fb64be3b'),
    'diagnostic018r3tx1': ('outputs/UV98-Rust-diagnostic-0.1.8-2026-11-09-r3-tx1/image/UV98-diag-0.1.8.bin',
                         'ac4e68caf7acc5b54b0c9836510a9286d642da34e5748537f073343fd18840e4'),
    'diagnostic018r2': ('outputs/UV98-Rust-diagnostic-0.1.8-2026-11-09-r2/image/UV98-diag-0.1.8.bin',
                        '4ef92191ca4b57bc800765937920e2a2e36a8f4b4dbeb2f395e10cef967a87be'),
    'diagnostic018': ('outputs/UV98-Rust-diagnostic-0.1.8-2026-11-09/image/UV98-diag-0.1.8.bin',
                      'b063124e7ac389b1e50633c359909b3566a677b3e1c5b6fd2558e86491c6353a'),
    'diagnostic017': ('outputs/UV98-Rust-diagnostic-0.1.7-2026-11-08-r2/image/UV98-diag-0.1.7.bin',
                      'b860604ccf31d1a23644aec1ddeec911510bda69b7e79090679561b40ac17966'),
    'diagnostic016': ('outputs/UV98-Rust-diagnostic-0.1.6-2026-11-08/image/UV98-diag-0.1.6.bin',
                      '961ed5164e6022196a9bf8718a5828f691f90c540dbb30dad79496ce9b4ad6bb'),
    'diagnostic015': ('outputs/UV98-Rust-diagnostic-0.1.5-2026-11-08/image/UV98-diag-0.1.5.bin',
                      '8a5cc8b64e982d39745cfdb1e4559da99385f2541f207bb8a6f3675fb18f413b'),
    'diagnostic014': ('outputs/UV98-Rust-diagnostic-0.1.4-2026-11-08/image/UV98-diag-0.1.4.bin',
                      '40c85ea181a937355a0a66865e57e9e941c6bdb5c780b6f27871c890d29aee7e'),
    'diagnostic013': ('outputs/UV98-Rust-diagnostic-0.1.3-2026-11-08/image/UV98-diag-0.1.3.bin',
                      'c3e92869ce615dadf1813b2bfcdfe40fc319796b50be767fd21cb62ea7157ede'),
    'uartprobe012': ('outputs/UV98-UART-probe-0.1.2-2026-11-08/image/UV98-uart-probe-0.1.2.bin',
                     'e32fc1ed0b4cb4c21aa93860cf5533bdc6f75f921a7fd2fabfdc27a726b1c960'),
    'diagnostic011': ('outputs/UV98-Rust-diagnostic-0.1.1-bootlog-2026-11-08/image/UV98-diag-0.1.1.bin',
                      'feeb52ec3dbdfb14348c4a1319a87b2a42223a940908c1a6610432f32f44af7b'),
    'diagnostic': ('outputs/UV98-Rust-diagnostic-0.1.0-2026-11-08/image/UV98-diag-0.1.0.bin',
                   'c533063ff4d9d0c3da2ee9b071132160de71531b9fbf3ea29e58b1991609924d'),
    'stock': ('outputs/UV98-recovery-preparation-2026-11-08/stock-candidate/RT-880_Upgrade_V1_16_250915.bin',
              'eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc'),
}
SOURCE = ROOT / 'work/board-identification/webserial-flash880-pinned.html'
SOURCE_SHA256 = '4fce477ceab5d84dc18ba0552814fa2796fdfc4f9708c05ea657aee5f67d8e08'
BYTES = 251904
BLOCKS = 246
TIMEOUT = 5.0
# Independent oracle: exact constructPacket JavaScript from the pinned HTML,
# executed with modelSelect.selectedIndex=1 on each reviewed full BIN.
TX_SHA256 = {
    'diagnostic018r5': 'e13b6ff65b49e82d9f903d3bc179e9b6ec27d600598bc6f819354e913123b62a',
    'diagnostic018r4tx2': 'e6e894be46ea340c90429306e5228f905b03a5ab6c4164db6ec54b6065f26579',
    'diagnostic018r3tx1': 'd02f70c71c8d34283d1cb1caefdda7c40fc3106056f3ade86c9c9c55d880d125',
    'diagnostic018r2': '85b3489e33c284096a086cd4c942a4f90ea34917f5dc5f5a7615be8e85fd3512',
    'diagnostic018': 'a7be3c04378ee8434d8797d5650fe8aed0703636977158ff02fbc7f460d4c667',
    'diagnostic017': 'f85384d045eb71ed9786ba9b02645b2693bb08c62d7a438ff864698d3329a3eb',
    'diagnostic016': '0d7dcb8a7e74b8f932e92c456f29fe23ae940fe106a945d8b6d36f2805428ac3',
    'diagnostic015': '581886f0a966bffa98600622ab057dbaa4e9f5bdda6223de9ebe95460cb9c38f',
    'diagnostic014': '950bfeeca8f0fb881752ec5a52b21be54909d6a1736cc48a1a5b7a0de8b34c64',
    'diagnostic013': 'fdd399ad7e631dbf37cb0e69c0aa878030d9175487ba26482e9c21a676f09b5a',
    'uartprobe012': 'dafc8d71166b87215c8d8c3cb6ed2d1d90fa8dbb04272b71accfa95ff2f5c267',
    'diagnostic011': '7f698eba1b6b7814db5d8e17c8b112098bd26e8dac7f8876eb44578e8e68aa25',
    'diagnostic': '438011af0679f128370cef0719d7ec76d188aa91cd7a5c3edadaa0198e48f925',
    'stock': 'b7ed81f12cf1ae41ee65aa43ad1b065297d6a8230887d5ace823a4d0b92ef4e0',
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def now():
    return datetime.now(timezone.utc).isoformat()


def write_json_new(path, data):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(data, stream, indent=2)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def checked_image(name):
    path, expected = IMAGES[name]
    data = (ROOT / path).read_bytes()
    if len(data) != BYTES or sha(data) != expected:
        raise ValueError(f'{name}: exact application size/hash mismatch')
    image_preflight.inspect(image_preflight.parse_bin(data))
    return data


def packet(kind, offset, payload):
    body = bytes((kind, (offset >> 8) & 255, offset & 255)) + payload
    return body + bytes((sum(body) & 255,))  # iRadio seed 0, not Radtel seed52.


def packets(data):
    if len(data) != BYTES:
        raise ValueError('Refusing padding, truncation or partial image')
    yield 'erase-stage-1a', packet(0x39, 0x3305, b'\x10')
    yield 'erase-stage-1b', packet(0x39, 0x3305, b'\x10')
    yield 'erase-stage-2', packet(0x39, 0x3305, b'\x55')
    for offset in range(0, len(data), 1024):
        yield f'block-{offset // 1024:03}', packet(0x57, offset, data[offset:offset + 1024])


def require_port(name):
    from serial.tools import list_ports
    ports = [p for p in list_ports.comports() if p.device == name]
    if name != 'COM20' or len(ports) != 1 or (ports[0].vid, ports[0].pid) != (0x1a86, 0x7523):
        raise ValueError('Expected COM20 / CH340 VID1A86 PID7523')
    return {'port': name, 'description': ports[0].description, 'hwid': ports[0].hwid}


def serial_factory(name):
    import serial
    require_port(name)  # Recheck immediately before the exclusive open.
    port = serial.Serial(port=None, baudrate=115200, bytesize=8, parity='N', stopbits=1,
                         timeout=TIMEOUT, write_timeout=TIMEOUT,
                         xonxoff=False, rtscts=False, dsrdtr=False)
    # Match .NET SerialPort defaults used by pinned FlasherX. No line toggling
    # or reset pulse beyond assigning this explicit state before open.
    port.dtr = False
    port.rts = False
    port.port = name
    try:
        port.open()
    except BaseException:
        port.close()
        raise
    return port


def flash(name, port_name, output, factory=serial_factory, progress=print):
    # Both selected image and fallback must remain available before any erase.
    selected = checked_image(name)
    checked_image('stock')
    if sha(SOURCE.read_bytes()) != SOURCE_SHA256:
        raise ValueError('Pinned protocol source changed')
    planned = tuple(packets(selected))
    if sha(b''.join(frame for _, frame in planned)) != TX_SHA256[name]:
        raise ValueError('TX sequence differs from the pinned JavaScript oracle')
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {'schema': 1, 'status': 'incomplete', 'started_utc': now(), 'image': name,
              'image_path': IMAGES[name][0], 'image_sha256': sha(selected), 'image_bytes': len(selected),
              'model': 'iRadio UV-98', 'checksum_seed': 0, 'port': port_name,
              'serial': {'baudrate': 115200, 'bytesize': 8, 'parity': 'N', 'stopbits': 1,
                         'dtr': False, 'rts': False, 'flow_control': False, 'timeout_seconds': TIMEOUT},
              'protocol_source_sha256': SOURCE_SHA256, 'automatic_retries': 0,
              'erase_sequence_attempted': False, 'acked_blocks': 0, 'ack_count': 0,
              'final_ack_received': False, 'port_closed': False, 'mcu_readback_verified': False,
              'hardware_boot_verified': False, 'calibration_after_boot_verified': False}
    write_json_new(output / 'started.json', report)
    port = None
    try:
        with (output / 'tx.bin').open('xb') as tx, (output / 'rx.bin').open('xb') as rx, \
                (output / 'journal.jsonl').open('x', encoding='utf-8') as journal:
            def event(**fields):
                journal.write(json.dumps({'utc': now(), **fields}) + '\n')
                journal.flush()
                os.fsync(journal.fileno())

            def ack(stage):
                response = port.read(1)
                rx.write(response)
                rx.flush()
                event(event='receive', stage=stage, hex=response.hex())
                if response != b'\x06':
                    raise RuntimeError(f'{stage}: expected ACK06; got {response.hex() or "timeout"}; stopped without retry')
                report['ack_count'] += 1

            port = factory(port_name)
            event(event='port_open')
            # Observe only existing RX for 200ms. Do not discard stale input and
            # accidentally count it as acknowledgement of a later erase request.
            port.timeout = 0.2
            unsolicited = port.read(1)
            port.timeout = TIMEOUT
            if unsolicited:
                rx.write(unsolicited)
                rx.flush()
                event(event='unexpected_before_tx', hex=unsolicited.hex())
                raise RuntimeError('Unexpected input before first TX; no erase command sent')
            for stage, frame in planned:
                report['last_stage'] = stage
                event(event='tx_attempt', stage=stage, bytes=len(frame), sha256=sha(frame))
                tx.write(frame)
                tx.flush()
                os.fsync(tx.fileno())
                report['erase_sequence_attempted'] = True
                written = port.write(frame)
                event(event='tx_return', stage=stage, reported_written=written)
                if written != len(frame):
                    raise RuntimeError(f'{stage}: short serial write {written}/{len(frame)}; stopped without retry')
                ack(stage)
                if stage.startswith('block-'):
                    report['acked_blocks'] += 1
                    if report['acked_blocks'] % 16 == 0 or report['acked_blocks'] == BLOCKS:
                        progress(f"ACK: {report['acked_blocks']}/{BLOCKS} application blocks", flush=True)
                else:
                    progress(f'ACK: {stage}', flush=True)
            ack('final-completion')  # Pinned writers receive this without END TX.
            report['final_ack_received'] = True
            report['status'] = 'transfer_acknowledged'
            event(event='transfer_acknowledged', ack_count=report['ack_count'])
    except BaseException as error:
        report['status'] = 'failed'
        report['error'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        if port is not None:
            try:
                port.close()
                report['port_closed'] = True
            except Exception as error:
                report['status'] = 'failed'
                report['close_error'] = str(error)
        report['finished_utc'] = now()
        report['artifacts'] = {p.name: {'bytes': p.stat().st_size, 'sha256': sha(p.read_bytes())}
                               for p in output.iterdir() if p.is_file()}
        write_json_new(output / 'manifest.json', report)
    if report['status'] != 'transfer_acknowledged':
        raise RuntimeError('Transfer cleanup failed; see manifest')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', choices=tuple(IMAGES), required=True)
    parser.add_argument('--port', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--execute-authorized', action='store_true')
    args = parser.parse_args()
    try:
        data = checked_image(args.image)
        print(json.dumps({'image': args.image, 'bytes': len(data), 'sha256': sha(data),
                          'port': args.port, 'will_erase_and_program_application': args.execute_authorized}), flush=True)
        if not args.execute_authorized:
            print('Plan only. No port opened.')
            return 0
        require_port(args.port)
        result = flash(args.image, args.port, args.output)
        print(json.dumps({key: result[key] for key in ('status', 'acked_blocks', 'ack_count',
                         'final_ack_received', 'port_closed', 'hardware_boot_verified')}, indent=2), flush=True)
        return 0
    except Exception as error:
        print(f'Flash stopped: {error}', file=sys.stderr, flush=True)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
