#!/usr/bin/env python3
"""Locate sustained 1400 Hz candidates in recorded receiver audio."""

import argparse
from dataclasses import dataclass
import json
import math
from pathlib import Path
import struct
import sys
import wave


TARGET_HZ = 1400  # SmarTrunk Systems ST-853 manual, section 3.2.
WINDOW_MS = 40
HOP_MS = 10
SEARCH_STEP_HZ = 10
MIN_TONE_FRACTION = 0.75


@dataclass(frozen=True)
class Settings:
    channel: int = 1
    tolerance_hz: float = 20.0
    min_duration_ms: float = 100.0
    min_rms_dbfs: float = -45.0

    def validate(self):
        if not 5 <= self.tolerance_hz <= 50:
            raise ValueError("frequency tolerance must be 5 to 50 Hz")
        if not 30 <= self.min_duration_ms <= 60_000:
            raise ValueError("minimum duration must be 30 to 60000 ms")
        if not -90 <= self.min_rms_dbfs <= 0:
            raise ValueError("minimum RMS must be -90 to 0 dBFS")


def read_channel(audio, count, channel):
    """Read one 1-based channel without averaging other tracks into it."""
    width = audio.getsampwidth()
    channels = audio.getnchannels()
    raw = audio.readframes(count)
    if len(raw) != count * channels * width:
        raise ValueError("truncated WAV PCM data")
    scale = 1 << (width * 8 - 1)
    offset = channel - 1
    if width == 1:
        return [(value - 128) / scale for value in raw[offset::channels]]
    if width in (2, 4):
        code = "h" if width == 2 else "i"
        samples = struct.unpack(f"={count * channels}{code}", raw)
        return [value / scale for value in samples[offset::channels]]
    return [
        int.from_bytes(raw[first:first + 3], sys.byteorder, signed=True) / scale
        for first in range(offset * width, len(raw), channels * width)
    ]


class ToneDetector:
    def __init__(self, sample_rate, settings):
        self.window = round(sample_rate * WINDOW_MS / 1000)
        self.weights = [0.5 - 0.5 * math.cos(math.tau * i / self.window)
                        for i in range(self.window)]
        self.weight_sum = sum(self.weights)
        self.floor = 10 ** (settings.min_rms_dbfs / 20)
        self.tolerance = settings.tolerance_hz
        # One bin beyond either tolerance boundary permits peak interpolation.
        edge = math.ceil(self.tolerance / SEARCH_STEP_HZ) + 1
        self.frequencies = [TARGET_HZ + offset * SEARCH_STEP_HZ
                            for offset in range(-edge, edge + 1)]
        self.coefficients = [2 * math.cos(math.tau * hz / sample_rate)
                             for hz in self.frequencies]
        self.center = edge

    @staticmethod
    def power(weighted, coefficient):
        previous = before = 0.0
        for value in weighted:
            previous, before = value + coefficient * previous - before, previous
        return previous * previous + before * before - coefficient * previous * before

    def measure(self, samples):
        mean = sum(x * w for x, w in zip(samples, self.weights)) / self.weight_sum
        centered = [x - mean for x in samples]
        weighted = [x * w for x, w in zip(centered, self.weights)]
        energy = sum(x * w for x, w in zip(centered, weighted))
        rms = math.sqrt(energy / self.weight_sum)
        if rms < self.floor:
            return None

        # A quick test at evenly spaced coarse bins eliminates most non-tones.
        # Fine bins and interpolation are only needed near a plausible peak.
        normalization = 2 / (self.weight_sum * energy)
        powers = [None] * len(self.frequencies)
        for i in range(0, len(powers), 2):
            powers[i] = self.power(weighted, self.coefficients[i]) * normalization
        if max(value for value in powers if value is not None) < 0.60:
            return None
        for i in range(1, len(powers), 2):
            powers[i] = self.power(weighted, self.coefficients[i]) * normalization
        peak = max(range(len(powers)), key=powers.__getitem__)
        if peak in (0, len(powers) - 1):
            return None
        a, b, c = (math.log(max(value, 1e-30)) for value in powers[peak - 1:peak + 2])
        curvature = a - 2 * b + c
        offset = 0.5 * (a - c) / curvature if curvature < 0 else 0.0
        frequency = self.frequencies[peak] + offset * SEARCH_STEP_HZ
        fraction = min(1.0, math.exp(b - 0.25 * (a - c) * offset))
        if abs(frequency - TARGET_HZ) > self.tolerance or fraction < MIN_TONE_FRACTION:
            return None
        return frequency, fraction, rms


@dataclass
class ToneRun:
    first: int
    last: int
    windows: int = 0
    frequency_sum: float = 0.0
    fraction_min: float = 1.0
    rms_max: float = 0.0

    def append(self, start, measurement):
        frequency, fraction, rms = measurement
        self.last = start
        self.windows += 1
        self.frequency_sum += frequency
        self.fraction_min = min(self.fraction_min, fraction)
        self.rms_max = max(self.rms_max, rms)

    def candidate(self, sample_rate, frames, window, hop, min_duration_ms):
        # Centers of the qualifying windows support approximate event edges.
        # At a recording edge, mark the candidate as possibly truncated.
        at_start = self.first == 0
        at_end = frames - self.last - window < hop
        first = 0 if at_start else self.first + (window - hop) // 2
        stop = frames if at_end else self.last + (window + hop) // 2
        measured_ms = self.windows * hop * 1000 / sample_rate
        if measured_ms < min_duration_ms:
            return None
        return {
            "start_sample": first,
            "end_sample_exclusive": stop,
            "start_s": round(first / sample_rate, 6),
            "end_s": round(stop / sample_rate, 6),
            "duration_ms": round((stop - first) * 1000 / sample_rate, 2),
            "boundaries_at_recording_edge": at_start or at_end,
            "qualifying_windows": self.windows,
            "mean_frequency_hz": round(self.frequency_sum / self.windows, 2),
            "min_tone_fraction": round(self.fraction_min, 4),
            "peak_window_rms_dbfs": round(20 * math.log10(self.rms_max), 2),
        }


def find_candidates(audio, sample_rate, settings):
    detector = ToneDetector(sample_rate, settings)
    frames = audio.getnframes()
    window = detector.window
    hop = round(sample_rate * HOP_MS / 1000)
    samples = read_channel(audio, min(window, frames), settings.channel)
    run = None
    start = 0
    while len(samples) == window:
        measurement = detector.measure(samples)
        if measurement is not None:
            if run is None:
                run = ToneRun(start, start)
            run.append(start, measurement)
        elif run is not None:
            candidate = run.candidate(sample_rate, frames, window, hop, settings.min_duration_ms)
            if candidate is not None:
                yield candidate
            run = None
        count = min(hop, frames - start - window)
        samples = samples[hop:] + read_channel(audio, count, settings.channel)
        start += hop
    if run is not None:
        candidate = run.candidate(sample_rate, frames, window, hop, settings.min_duration_ms)
        if candidate is not None:
            yield candidate


def inspect(path, settings=Settings()):
    settings.validate()
    with wave.open(str(path), "rb") as audio:
        sample_rate = audio.getframerate()
        if audio.getcomptype() != "NONE" or audio.getsampwidth() not in (1, 2, 3, 4):
            raise ValueError("expected PCM WAV (8-, 16-, 24- or 32-bit integer samples)")
        if not 8000 <= sample_rate <= 192_000:
            raise ValueError("supported sample rates are 8000 to 192000 Hz")
        if not 1 <= audio.getnchannels() <= 32:
            raise ValueError("supported WAV inputs have 1 to 32 channels")
        if not 1 <= settings.channel <= audio.getnchannels():
            raise ValueError(f"select a channel from 1 to {audio.getnchannels()}")
        candidates = list(find_candidates(audio, sample_rate, settings))
        if audio.readframes(1):
            raise ValueError("incomplete trailing WAV frame")
        return {
            "schema": 1,
            "analysis": "sustained_1400_hz_tone_candidates",
            "protocol_identification": "not_performed",
            "sample_rate_hz": sample_rate,
            "frames": audio.getnframes(),
            "selected_channel": settings.channel,
            "target_hz": TARGET_HZ,
            "tolerance_hz": settings.tolerance_hz,
            "min_duration_ms": settings.min_duration_ms,
            "min_rms_dbfs": settings.min_rms_dbfs,
            "min_tone_fraction": MIN_TONE_FRACTION,
            "window_ms": WINDOW_MS,
            "hop_ms": HOP_MS,
            "candidates": candidates,
        }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wav", type=Path, help="recorded demodulated receiver audio, PCM WAV")
    parser.add_argument("--channel", type=int, default=1, help="input audio channel, 1-based")
    parser.add_argument("--tolerance-hz", type=float, default=20, help="around the 1400 Hz target")
    parser.add_argument("--min-duration-ms", type=float, default=100, help="minimum sustained tone")
    parser.add_argument("--min-rms-dbfs", type=float, default=-45, help="analysis floor for quiet input")
    args = parser.parse_args(argv)
    settings = Settings(args.channel, args.tolerance_hz, args.min_duration_ms, args.min_rms_dbfs)
    try:
        report = inspect(args.wav, settings)
    except (OSError, EOFError, ValueError, wave.Error) as error:
        parser.exit(2, f"{parser.prog}: {error}\n")
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
