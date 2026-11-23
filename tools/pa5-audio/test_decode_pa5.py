"""Exercise the shipped Windows decoder, offline; never opens a COM port."""
from pathlib import Path
import json
import math
import struct
import subprocess
import tempfile
import unittest
import wave
import zlib

ROOT = Path(__file__).resolve().parent

def frame(code, a, b, c):
    prefix = f"UV98D1 {code:08X} {a:08X} {b:08X} {c:08X}".encode()
    return prefix + f"*{sum(prefix) & 255:02X}\r\n".encode()

def capture(samples, sequence=7):
    raw = struct.pack('<2048H', *samples)
    frames = [frame(0xA5100000, sequence, 8000, 2048),
              frame(0xA5100001, 0xFFFF0000, (0xFFFF0000 + 2047*6000) & 0xffffffff, 100),
              frame(0xA5100002, 5900, 6100, 48_000_000)]
    padded = raw + b'\0'*8
    for chunk in range(342):
        frames.append(frame(0xA5200000 + chunk, *struct.unpack_from('<3I', padded, chunk*12)))
    frames.append(frame(0xA51F0000, sequence, zlib.crc32(raw), 2048))
    return frames

class DecoderTest(unittest.TestCase):
    def test_shipped_decoder_and_rejections(self):
        samples = [round(2048 + 1000*math.sin(2*math.pi*i/8)) for i in range(2048)]
        frames = capture(samples)
        fm = 1 | (2 << 2) | (1 << 6) | (2 << 8) | (1 << 10) | (1 << 13) | (3 << 14) | (1230 << 16)
        quiet = (15 << 2) | (1 << 8) | (1 << 12)
        impossible_timing = frames.copy()
        impossible_timing[1] = frame(0xA5100001, 0, 2047*6000, 0)
        impossible_span = frames.copy()
        impossible_span[2] = frame(0xA5100002, 6000, 6100, 48000000)
        # Each result is the count of valid blocks and the count of issue records.
        cases = {
            'tone': (frames, [1, 0]),
            'truncated': (frames[:-1], [0, 1]),
            'missing': (frames[:40] + frames[41:], [0, 1]),
            'duplicate': (frames[:40] + [frames[39]] + frames[40:], [0, 1]),
            'bad_crc': (frames[:-1] + [frame(0xA51F0000, 7, 0, 2048)], [0, 1]),
            'wrong_timing': (frames[:2] + [frame(0xA5100002, 4000, 9000, 48000000)] + frames[3:], [0, 1]),
            'impossible_timing': (impossible_timing, [0, 1]),
            'impossible_span': (impossible_span, [0, 1]),
            'out_of_range': (capture([5000]*2048), [0, 1]),
            'too_big': ([b'x'*(1024*1024 + 1)], [-1, 0]),
            'no_block': ([frame(0x124, 1, 42, 0)], [0, 1]),
            'two_blocks': (frames + capture(samples, 8), [2, 0]),
            'repeat_after_truncation': (frames[:100] + frames, [1, 1]),
            'repeat_after_invalid': (impossible_timing + frames, [1, 1]),
            'valid_then_truncated': (frames + capture(samples, 8)[:-1], [1, 1]),
            'valid_then_header_missing': (frames + capture(samples, 8)[1:], [1, 1]),
            'starts_mid_export': (frames[40:] + frames, [1, 1]),
            'context_open': (frames[:3] + [frame(0xA6100000, 7, 14_550_000, fm)] + frames[3:], [1, 0]),
            'context_quiet': (frames[:3] + [frame(0xA6100000, 7, 0, quiet)] + frames[3:], [1, 0]),
            'context_wrong_sequence': (frames[:3] + [frame(0xA6100000, 8, 14_550_000, fm)] + frames[3:], [0, 1]),
            'clipped': (capture([0, 4095] * 1024), [1, 0]),
            'silence': (capture([2048] * 2048), [1, 0]),
        }
        damaged = frames.copy()
        damaged[7] = damaged[7][:43] + b'00\r\n'
        cases['bad_line_sum'] = (damaged, [0, 1])
        with tempfile.TemporaryDirectory(prefix='uv98-pa5-test-') as tmp:
            tmp = Path(tmp)
            for name, (data, _) in cases.items():
                (tmp / (name + '.bin')).write_bytes(b''.join(data))
            script = tmp / 'run.ps1'
            script.write_text("""param($Decoder, $Folder)
$ErrorActionPreference = 'Stop'
Add-Type -Path $Decoder
$result = @{}
foreach ($file in Get-ChildItem -LiteralPath $Folder -Filter '*.bin') {
    $output = Join-Path $Folder $file.BaseName
    New-Item -ItemType Directory -Path $output | Out-Null
    try {
        $session = [DecodePa5]::Decode([IO.File]::ReadAllBytes($file.FullName), $output, 1000)
        $session.Blocks | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $output 'report.json') -Encoding UTF8
        $result[$file.BaseName] = @($session.Blocks.Length, $session.Issues.Length)
    } catch { $result[$file.BaseName] = @(-1, 0) }
}
$result | ConvertTo-Json -Compress
""", encoding='utf-8')
            run = subprocess.run(['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', str(script), str(ROOT/'DecodePa5.cs'), str(tmp)], capture_output=True, text=True)
            self.assertEqual(run.returncode, 0, run.stderr)
            observed = json.loads(run.stdout)
            self.assertEqual(observed, {k: v[1] for k,v in cases.items()})
            report = json.loads((tmp/'tone/report.json').read_text(encoding='utf-8-sig'))
            self.assertAlmostEqual(report['DcCodes'], 2048)
            self.assertGreater(report['AcRmsCodes'], 700)
            self.assertEqual(report['EstimatedToneHz'], 1000)
            self.assertGreater(report['ToneRmsCodes'], 700)
            self.assertLess(report['ResidualRmsCodes'], 0.1)
            self.assertIsNone(report['CaptureKind'])
            self.assertIsNone(report['AudioOpenBefore'])
            expected_context = {
                'ContextVersion': 1, 'FrequencyHz': 145_500_000, 'Modulation': 'FM',
                'Squelch': 2, 'Monitor': False, 'RxToneRequired': True, 'RxToneRaw': 1230, 'RxToneMode': 3,
                'CaptureKind': 'OpenRx', 'AudioOpenBefore': True, 'AudioOpenAfter': False,
            }
            report_open = json.loads((tmp/'context_open/report.json').read_text(encoding='utf-8-sig'))
            self.assertEqual({key: report_open[key] for key in expected_context}, expected_context)
            report_quiet = json.loads((tmp/'context_quiet/report.json').read_text(encoding='utf-8-sig'))
            self.assertEqual(report_quiet['CaptureKind'], 'QuietControl')
            self.assertFalse(report_quiet['AudioOpenBefore'])
            self.assertIsNone(report_quiet['AudioOpenAfter'])
            self.assertIsNone(report_quiet['FrequencyHz'])
            report_clipped = json.loads((tmp/'clipped/report.json').read_text(encoding='utf-8-sig'))
            self.assertEqual((report_clipped['ClipLow'], report_clipped['ClipHigh']), (1024, 1024))
            report_silence = json.loads((tmp/'silence/report.json').read_text(encoding='utf-8-sig'))
            self.assertIsNone(report_silence['EstimatedToneHz'])
            self.assertEqual((report_silence['ToneRmsCodes'], report_silence['ResidualRmsCodes']), (0, 0))
            self.assertEqual(Path(report['RawFile']).read_bytes(), struct.pack('<2048H', *samples))
            with wave.open(report['WavFile'], 'rb') as wav:
                self.assertEqual((wav.getnchannels(), wav.getframerate(), wav.getnframes(), wav.getsampwidth()), (1,8000,2048,2))
                pcm = struct.unpack('<2048h', wav.readframes(2048))
                self.assertEqual(pcm[0], 0)
                self.assertEqual(pcm[2], 8000)

    def test_offline_command_reports_a_failed_export_and_a_recovered_repeat(self):
        frames = capture([2048]*2048)
        with tempfile.TemporaryDirectory(prefix='uv98-pa5-command-') as tmp:
            tmp = Path(tmp)
            for name, records, succeeds in [('truncated', frames[:100], False),
                                             ('recovered', frames[:100] + frames, True)]:
                source, output = tmp/f'{name}.bin', tmp/name
                source.write_bytes(b''.join(records))
                run = subprocess.run(['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass',
                                      '-File', str(ROOT/'Capture-PA5.ps1'), '-Decode', str(source),
                                      '-Output', str(output)], capture_output=True, text=True)
                self.assertEqual(run.returncode == 0, succeeds, run.stderr)
                issues = json.loads((output/'issues.json').read_text(encoding='utf-8-sig'))
                self.assertEqual(len(issues), 1)
                self.assertEqual(issues[0]['Sequence'], 7)
                self.assertEqual((output/'report.json').exists(), succeeds)

if __name__ == '__main__':
    unittest.main()
