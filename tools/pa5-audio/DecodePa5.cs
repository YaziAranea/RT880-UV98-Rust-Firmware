// Offline decoder for the manual PA5 experiment. No device or serial APIs.
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;

public sealed class Pa5Report {
    public uint Sequence, SampleRate = 8000, Samples = 2048;
    public int Minimum, Maximum, ClipLow, ClipHigh;
    public double DcCodes, AcRmsCodes;
    public uint FirstCycle, LastCycle, MaxLateCycles, MinIntervalCycles, MaxIntervalCycles;
    public int? ContextVersion, Squelch, RxToneRaw, RxToneMode;
    public uint? FrequencyHz;
    public string Modulation, CaptureKind;
    public bool? Monitor, RxToneRequired, AudioOpenBefore, AudioOpenAfter;
    public double? ExpectedToneHz, EstimatedToneHz, ToneRmsCodes, ResidualRmsCodes, ToneToResidualDb;
    public string RawFile, WavFile;
    public bool HardwareQualified = false;
}

public sealed class Pa5Issue {
    public uint? Sequence;
    public string Reason;
}

public sealed class Pa5Session {
    public Pa5Report[] Blocks;
    public Pa5Issue[] Issues;
}

public static class DecodePa5 {
    const uint Header = 0xa5100000U, Context = 0xa6100000U;
    const uint FirstChunk = 0xa5200000U, Trailer = 0xa51f0000U;
    const int ChunkCount = 342;
    const uint Period = 6000, Intervals = 2047;
    static readonly Regex Pattern = new Regex(
        @"UV98D1 ([0-9A-F]{8}) ([0-9A-F]{8}) ([0-9A-F]{8}) ([0-9A-F]{8})\*([0-9A-F]{2})\r\n");

    sealed class Block {
        internal readonly Pa5Report Report = new Pa5Report();
        internal readonly List<ushort> Values = new List<ushort>(2048);
        internal int Stage;
    }

    public static uint Crc(byte[] data) {
        uint crc = 0xffffffff;
        foreach (byte b in data) {
            crc ^= b;
            for (int i = 0; i < 8; ++i) crc = (crc >> 1) ^ ((crc & 1) != 0 ? 0xedb88320U : 0U);
        }
        return ~crc;
    }

    public static Pa5Session Decode(byte[] data, string folder, double expectedToneHz) {
        if (data.Length > 1024 * 1024) throw new InvalidDataException("Capture exceeds 1 MiB");
        if (double.IsNaN(expectedToneHz) ||
            (expectedToneHz != 0 && (expectedToneHz < 10 || expectedToneHz > 3900)))
            throw new ArgumentOutOfRangeException("expectedToneHz");

        var results = new List<Pa5Report>();
        var issues = new List<Pa5Issue>();
        Block block = null;
        bool ignoringFragment = false;
        foreach (Match match in Pattern.Matches(Encoding.ASCII.GetString(data))) {
            uint code = Convert.ToUInt32(match.Groups[1].Value, 16);
            if ((code & 0xff000000U) != 0xa5000000U && code != Context) continue;
            if (code == Header) {
                if (block != null) AddIssue(issues, block, "Incomplete audio block before next header");
                block = new Block();
                ignoringFragment = false;
            } else if (block == null) {
                // The stream can start mid-export or lose a later header.
                if (!ignoringFragment) {
                    AddIssue(issues, null, "Partial audio export without header");
                    ignoringFragment = true;
                }
                continue;
            }
            uint a = Convert.ToUInt32(match.Groups[2].Value, 16);
            uint b = Convert.ToUInt32(match.Groups[3].Value, 16);
            uint c = Convert.ToUInt32(match.Groups[4].Value, 16);
            if (code == Header) block.Report.Sequence = a;
            try {
                int sum = 0;
                for (int i = 0; i < 42; ++i) sum = (sum + (byte)match.Value[i]) & 255;
                if (sum != Convert.ToInt32(match.Groups[5].Value, 16))
                    throw new InvalidDataException("Audio frame checksum mismatch");
                if (ProcessFrame(block, code, a, b, c)) {
                    results.Add(WriteBlock(block, b, folder, results.Count, expectedToneHz));
                    block = null;
                }
            } catch (InvalidDataException error) {
                AddIssue(issues, block, error.Message);
                // Continue at the next header, including a retransmission with the same sequence.
                block = null;
                ignoringFragment = true;
            }
        }
        if (block != null) AddIssue(issues, block, "Truncated audio block; press # again while listening");
        if (results.Count == 0 && issues.Count == 0)
            AddIssue(issues, null, "No complete audio block; start listening before pressing #");
        return new Pa5Session { Blocks = results.ToArray(), Issues = issues.ToArray() };
    }

    static void AddIssue(List<Pa5Issue> issues, Block block, string reason) {
        issues.Add(new Pa5Issue { Sequence = block == null ? (uint?)null : block.Report.Sequence, Reason = reason });
    }

    static bool ProcessFrame(Block block, uint code, uint a, uint b, uint c) {
        var report = block.Report;
        if (code == Header && block.Stage == 0) {
            if (b != 8000 || c != 2048)
                throw new InvalidDataException("Unsupported sample rate or block length");
        } else if (code == Header + 1 && block.Stage == 1) {
            report.FirstCycle = a;
            report.LastCycle = b;
            report.MaxLateCycles = c;
        } else if (code == Header + 2 && block.Stage == 2) {
            CheckTiming(report, a, b, c);
        } else if (code == Context && block.Stage == 3 && !report.ContextVersion.HasValue) {
            ReadContext(report, a, b, c);
            return false;
        } else if (block.Stage >= 3 && block.Stage < 3 + ChunkCount &&
                   code == FirstChunk + (uint)(block.Stage - 3)) {
            AddWord(block.Values, a);
            AddWord(block.Values, b);
            AddWord(block.Values, c);
        } else if (code == Trailer && block.Stage == 3 + ChunkCount) {
            if (a != report.Sequence || c != 2048 || block.Values.Count != 2048)
                throw new InvalidDataException("Audio trailer mismatch");
            return true;
        } else {
            throw new InvalidDataException("Missing, duplicated or out-of-order audio frame");
        }
        ++block.Stage;
        return false;
    }

    static void CheckTiming(Pa5Report report, uint minimum, uint maximum, uint clock) {
        uint elapsed = unchecked(report.LastCycle - report.FirstCycle);
        uint late = report.MaxLateCycles;
        // The deviation of one interval is the difference of two trigger
        // latenesses. Both reported interval extrema must occur in the span.
        if (clock != 48000000 || late > 300 || minimum > maximum ||
            minimum < Period - late || maximum > Period + late ||
            elapsed < Period * Intervals - late || elapsed > Period * Intervals + late ||
            elapsed < minimum * (Intervals - 1) + maximum ||
            elapsed > maximum * (Intervals - 1) + minimum)
            throw new InvalidDataException("Invalid sample timing; impossible interval metadata");
        report.MinIntervalCycles = minimum;
        report.MaxIntervalCycles = maximum;
    }

    static bool? Flag(uint bits) {
        if (bits == 3) throw new InvalidDataException("Invalid PA5 context flag");
        if (bits == 0) return null;
        return bits == 2;
    }

    static void ReadContext(Pa5Report report, uint sequence, uint frequency10Hz, uint bits) {
        uint mode = bits & 3;
        uint sql = (bits >> 2) & 15;
        if (sequence != report.Sequence || frequency10Hz > uint.MaxValue / 10 ||
            mode == 3 || (sql > 10 && sql != 15))
            throw new InvalidDataException("Invalid PA5 context frame");
        report.ContextVersion = 1;
        report.FrequencyHz = frequency10Hz == 0 ? (uint?)null : frequency10Hz * 10;
        report.Modulation = mode == 1 ? "FM" : mode == 2 ? "AM" : null;
        report.Squelch = sql == 15 ? (int?)null : (int)sql;
        report.Monitor = Flag((bits >> 6) & 3);
        report.AudioOpenBefore = Flag((bits >> 8) & 3);
        report.AudioOpenAfter = Flag((bits >> 10) & 3);
        bool quiet = (bits & 0x1000) != 0;
        if (report.AudioOpenBefore == quiet)
            throw new InvalidDataException("PA5 capture kind disagrees with the initial audio gate");
        report.CaptureKind = quiet ? "QuietControl" : "OpenRx";
        report.RxToneRequired = mode == 0 ? (bool?)null : (bits & 0x2000) != 0;
        report.RxToneMode = mode == 0 ? (int?)null : (int)((bits >> 14) & 3);
        report.RxToneRaw = mode == 0 ? (int?)null : (int)(bits >> 16);
    }

    static void AddWord(List<ushort> values, uint word) {
        AddSample(values, (ushort)(word & 65535));
        AddSample(values, (ushort)(word >> 16));
    }

    static void AddSample(List<ushort> values, ushort sample) {
        if (values.Count < 2048) {
            if (sample > 4095) throw new InvalidDataException("ADC sample outside 12-bit range");
            values.Add(sample);
        } else if (sample != 0) {
            throw new InvalidDataException("Nonzero final padding");
        }
    }

    static Pa5Report WriteBlock(Block block, uint checksum, string folder, int index, double expectedToneHz) {
        var report = block.Report;
        var raw = new byte[4096];
        long sumCodes = 0, sumSquares = 0;
        report.Minimum = 4095;
        for (int i = 0; i < block.Values.Count; ++i) {
            int sample = block.Values[i];
            raw[i * 2] = (byte)sample;
            raw[i * 2 + 1] = (byte)(sample >> 8);
            sumCodes += sample;
            sumSquares += sample * sample;
            report.Minimum = Math.Min(report.Minimum, sample);
            report.Maximum = Math.Max(report.Maximum, sample);
            if (sample == 0) ++report.ClipLow;
            if (sample == 4095) ++report.ClipHigh;
        }
        if (Crc(raw) != checksum) throw new InvalidDataException("Whole-block CRC32 mismatch");
        report.DcCodes = sumCodes / 2048.0;
        report.AcRmsCodes = Math.Sqrt(Math.Max(0, sumSquares / 2048.0 - report.DcCodes * report.DcCodes));
        if (expectedToneHz > 0) MeasureTone(report, block.Values, expectedToneHz);

        string name = "pa5-" + index.ToString("D3") + "-" + report.Sequence.ToString();
        report.RawFile = Path.Combine(folder, name + ".adc.u16le");
        report.WavFile = Path.Combine(folder, name + ".wav");
        if (File.Exists(report.RawFile) || File.Exists(report.WavFile)) throw new IOException("Output already exists");
        using (var file = new FileStream(report.RawFile, FileMode.CreateNew)) file.Write(raw, 0, raw.Length);
        using (var wave = new FileStream(report.WavFile, FileMode.CreateNew))
        using (var writer = new BinaryWriter(wave, Encoding.ASCII)) {
            writer.Write(Encoding.ASCII.GetBytes("RIFF")); writer.Write(4132);
            writer.Write(Encoding.ASCII.GetBytes("WAVEfmt ")); writer.Write(16);
            writer.Write((ushort)1); writer.Write((ushort)1); writer.Write(8000); writer.Write(16000);
            writer.Write((ushort)2); writer.Write((ushort)16);
            writer.Write(Encoding.ASCII.GetBytes("data")); writer.Write(4096);
            // Fixed gain x8, DC removed. Raw ADC codes preserve absolute levels.
            foreach (ushort sample in block.Values)
                writer.Write((short)Math.Round((sample - report.DcCodes) * 8));
        }
        return report;
    }

    static void MeasureTone(Pa5Report report, List<ushort> samples, double expectedHz) {
        report.ExpectedToneHz = expectedHz;
        // Search locally to account for the finite record length and nominal sample clock.
        int margin = (int)Math.Ceiling(Math.Max(12, expectedHz * 0.03));
        double bestEnergy = 0;
        for (int offset = -margin; offset <= margin; ++offset) {
            double hz = expectedHz + offset;
            if (hz <= 0 || hz >= report.SampleRate / 2.0) continue;
            double energy = FitTone(samples, report.DcCodes, report.SampleRate, hz);
            if (energy > bestEnergy) {
                bestEnergy = energy;
                report.EstimatedToneHz = hz;
            }
        }
        double total = report.AcRmsCodes * report.AcRmsCodes;
        bestEnergy = Math.Min(total, bestEnergy);
        double residual = total - bestEnergy;
        report.ToneRmsCodes = Math.Sqrt(bestEnergy);
        report.ResidualRmsCodes = Math.Sqrt(residual);
        if (bestEnergy > 0 && residual > 0)
            report.ToneToResidualDb = 10 * Math.Log10(bestEnergy / residual);
    }

    static double FitTone(List<ushort> samples, double dc, uint rate, double hz) {
        // Least squares with a DC term also works when the block contains partial periods.
        double omega = 2 * Math.PI * hz / rate;
        double sumSin = 0, sumCos = 0, sin2 = 0, cos2 = 0, cross = 0, xSin = 0, xCos = 0;
        for (int i = 0; i < samples.Count; ++i) {
            double s = Math.Sin(omega * i), c = Math.Cos(omega * i), x = samples[i] - dc;
            sumSin += s; sumCos += c;
            sin2 += s * s; cos2 += c * c; cross += s * c;
            xSin += x * s; xCos += x * c;
        }
        sin2 -= sumSin * sumSin / samples.Count;
        cos2 -= sumCos * sumCos / samples.Count;
        cross -= sumSin * sumCos / samples.Count;
        double determinant = sin2 * cos2 - cross * cross;
        double a = (cos2 * xSin - cross * xCos) / determinant;
        double b = (sin2 * xCos - cross * xSin) / determinant;
        return Math.Max(0, (a * xSin + b * xCos) / samples.Count);
    }
}
