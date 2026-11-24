import json
import math
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest
import wave

import inspect_collect_tone as analyzer


def tone(sample_rate, seconds, frequency, amplitude=0.5):
    return [amplitude * math.sin(math.tau * frequency * i / sample_rate + 0.31)
            for i in range(round(seconds * sample_rate))]


def noise(sample_rate, seconds, amplitude, seed):
    source = random.Random(seed)
    return [source.uniform(-amplitude, amplitude) for _ in range(round(seconds * sample_rate))]


def pcm(path, sample_rate, width, *channels):
    scale = 1 << (width * 8 - 1)
    with wave.open(str(path), "wb") as output:
        output.setparams((len(channels), width, sample_rate, 0, "NONE", "not compressed"))
        data = bytearray()
        for frame in zip(*channels, strict=True):
            for sample in frame:
                value = max(-scale, min(scale - 1, round(sample * scale)))
                data += (value + 128).to_bytes(1, "little") if width == 1 else value.to_bytes(
                    width, "little", signed=True)
        output.writeframes(data)


class CollectToneTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.path = Path(directory.name) / "recording.wav"

    def analyze(self, signal, rate=8000, settings=analyzer.Settings()):
        pcm(self.path, rate, 2, signal)
        return analyzer.inspect(self.path, settings)["candidates"]

    def test_noisy_offset_candidates_across_audio_sample_rates(self):
        for rate in (8000, 11025, 16000, 22050, 44100, 48000):
            for hz in (1385, 1400, 1415):
                with self.subTest(rate=rate, frequency=hz):
                    prefix = round(rate * 0.13)
                    length = round(rate * 0.24)
                    wanted = [0.0] * prefix + tone(rate, 0.24, hz) + [0.0] * (rate // 10)
                    samples = [a + b for a, b in zip(
                        wanted, noise(rate, len(wanted) / rate, 0.14, 7), strict=True)]
                    candidates = self.analyze(samples, rate)
                    self.assertEqual(len(candidates), 1)
                    candidate = candidates[0]
                    self.assertAlmostEqual(candidate["mean_frequency_hz"], hz, delta=2)
                    self.assertAlmostEqual(candidate["start_sample"], prefix, delta=rate * 0.025)
                    self.assertAlmostEqual(candidate["end_sample_exclusive"], prefix + length,
                                           delta=rate * 0.025)
                    self.assertGreaterEqual(candidate["min_tone_fraction"], 0.75)
                    self.assertFalse(candidate["boundaries_at_recording_edge"])

    def test_pcm_widths_and_explicit_stereo_channel(self):
        rate = 8000
        for width in (1, 2, 3, 4):
            with self.subTest(width=width):
                pcm(self.path, rate, width, tone(rate, 0.2, 1000), tone(rate, 0.2, 1400))
                self.assertEqual(analyzer.inspect(self.path)["candidates"], [])
                report = analyzer.inspect(self.path, analyzer.Settings(channel=2))
                self.assertEqual(report["protocol_identification"], "not_performed")
                candidate, = report["candidates"]
                self.assertEqual(candidate["start_sample"], 0)
                self.assertEqual(candidate["end_sample_exclusive"], 1600)
                self.assertTrue(candidate["boundaries_at_recording_edge"])

    def test_negative_controls(self):
        rate = 8000
        unrelated = (tone(rate, 0.24, hz) for hz in (1250, 1360, 1440, 1500))
        # Standard DTMF pair and a different two-tone example containing 1400 Hz.
        dtmf = [a + b for a, b in zip(tone(rate, 0.24, 770, 0.3), tone(rate, 0.24, 1336, 0.3))]
        dual = [a + b for a, b in zip(tone(rate, 0.24, 1000, 0.3), tone(rate, 0.24, 1400, 0.3))]
        cases = [
            [0.0] * 1920, [0.25] * 1920, *unrelated, dtmf, dual,
            tone(rate, 0.06, 1400), tone(rate, 0.24, 1400, 0.002),
            *(noise(rate, 0.24, 0.4, seed) for seed in range(16)),
        ]
        for index, signal in enumerate(cases):
            with self.subTest(case=index):
                self.assertEqual(self.analyze(signal, rate), [])

    def test_continuous_1400_hz_interferer_cannot_identify_protocol(self):
        pcm(self.path, 8000, 2, tone(8000, 0.24, 1400))
        report = analyzer.inspect(self.path)
        self.assertEqual(len(report["candidates"]), 1)
        self.assertEqual(report["protocol_identification"], "not_performed")
        self.assertNotIn("decoded_frames", report)

    def test_gap_separates_tones_and_thresholds_can_be_adjusted(self):
        rate = 8000
        signal = tone(rate, 0.22, 1400) + [0.0] * 680 + tone(rate, 0.18, 1400)
        first, second = self.analyze(signal)
        self.assertLess(first["end_sample_exclusive"], second["start_sample"])
        self.assertEqual(second["end_sample_exclusive"], len(signal))
        self.assertEqual(len(self.analyze(tone(rate, 0.24, 1435), settings=analyzer.Settings(
            tolerance_hz=40))), 1)
        self.assertEqual(len(self.analyze(tone(rate, 0.24, 1400, 0.002), settings=analyzer.Settings(
            min_rms_dbfs=-65))), 1)

    def test_truncated_invalid_and_non_wav_inputs(self):
        pcm(self.path, 8000, 2, tone(8000, 0.2, 1400))
        with self.assertRaisesRegex(ValueError, "select a channel"):
            analyzer.inspect(self.path, analyzer.Settings(channel=2))
        with self.assertRaisesRegex(ValueError, "tolerance"):
            analyzer.inspect(self.path, analyzer.Settings(tolerance_hz=float("nan")))
        self.path.write_bytes(self.path.read_bytes()[:-7])
        with self.assertRaisesRegex(ValueError, "truncated WAV PCM data"):
            analyzer.inspect(self.path)
        self.path.write_bytes(b"not a wave file")
        with self.assertRaises((wave.Error, EOFError)):
            analyzer.inspect(self.path)

    def test_cli_writes_json_and_reports_invalid_input_as_failure(self):
        script = Path(analyzer.__file__)
        pcm(self.path, 8000, 2, [0.0] * 1760)
        result = subprocess.run([sys.executable, str(script), str(self.path)],
                                capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout)["candidates"], [])
        self.assertEqual(result.stderr, "")
        result = subprocess.run([sys.executable, str(script), str(self.path), "--channel", "2"],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        self.assertIn("select a channel", result.stderr)


if __name__ == "__main__":
    unittest.main()
