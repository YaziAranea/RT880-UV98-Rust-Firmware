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
        cases = {'tone': (b''.join(frames), True),
                 'truncated': (b''.join(frames[:-1]), False),
                 'missing': (b''.join(frames[:40] + frames[41:]), False),
                 'duplicate': (b''.join(frames[:40] + [frames[39]] + frames[40:]), False),
                 'bad_crc': (b''.join(frames[:-1] + [frame(0xA51F0000, 7, 0, 2048)]), False),
                 'wrong_timing': (b''.join(frames[:2] + [frame(0xA5100002, 4000, 9000, 48000000)] + frames[3:]), False),
                 'out_of_range': (b''.join(capture([5000]*2048)), False),
                 'too_big': (b'x'*(1024*1024 + 1), False),
                 'no_block': (frame(0x124, 1, 42, 0), False),
                 'two_blocks': (b''.join(frames + capture(samples, 8)), True)}
        damaged = frames.copy()
        damaged[7] = damaged[7][:43] + b'00\r\n'
        cases['bad_line_sum'] = (b''.join(damaged), False)
        with tempfile.TemporaryDirectory(prefix='uv98-pa5-test-') as tmp:
            tmp = Path(tmp)
            for name, (data, _) in cases.items():
                (tmp / (name + '.bin')).write_bytes(data)
            script = tmp / 'run.ps1'
            script.write_text("""param($Decoder, $Folder)
$ErrorActionPreference = 'Stop'
Add-Type -Path $Decoder
$result = @{}
foreach ($file in Get-ChildItem -LiteralPath $Folder -Filter '*.bin') {
    $output = Join-Path $Folder $file.BaseName
    New-Item -ItemType Directory -Path $output | Out-Null
    try {
        $reports = [DecodePa5]::Decode([IO.File]::ReadAllBytes($file.FullName), $output)
        $reports | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $output 'report.json') -Encoding UTF8
        $result[$file.BaseName] = $true
    } catch { $result[$file.BaseName] = $false }
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
            self.assertEqual(Path(report['RawFile']).read_bytes(), struct.pack('<2048H', *samples))
            with wave.open(report['WavFile'], 'rb') as wav:
                self.assertEqual((wav.getnchannels(), wav.getframerate(), wav.getnframes(), wav.getsampwidth()), (1,8000,2048,2))
                pcm = struct.unpack('<2048h', wav.readframes(2048))
                self.assertEqual(pcm[0], 0)
                self.assertEqual(pcm[2], 8000)

if __name__ == '__main__':
    unittest.main()
