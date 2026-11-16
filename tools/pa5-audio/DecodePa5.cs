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
    public string RawFile, WavFile;
    public bool HardwareQualified = false;
}

public static class DecodePa5 {
    public static uint Crc(byte[] data) {
        uint crc = 0xffffffff;
        foreach (byte b in data) {
            crc ^= b;
            for (int i = 0; i < 8; ++i) crc = (crc >> 1) ^ ((crc & 1) != 0 ? 0xedb88320U : 0U);
        }
        return ~crc;
    }

    public static Pa5Report[] Decode(byte[] data, string folder) {
        if (data.Length > 1024 * 1024) throw new InvalidDataException("Capture exceeds 1 MiB");
        var results = new List<Pa5Report>();
        var pattern = new Regex(@"UV98D1 ([0-9A-F]{8}) ([0-9A-F]{8}) ([0-9A-F]{8}) ([0-9A-F]{8})\*([0-9A-F]{2})\r\n");
        var values = new List<ushort>();
        Pa5Report report = null;
        int stage = 0;
        foreach (Match m in pattern.Matches(Encoding.ASCII.GetString(data))) {
            uint code = Convert.ToUInt32(m.Groups[1].Value, 16);
            if ((code & 0xff000000U) != 0xa5000000U) continue;
            int sum = 0;
            for (int i = 0; i < 42; ++i) sum = (sum + (byte)m.Value[i]) & 255;
            if (sum != Convert.ToInt32(m.Groups[5].Value, 16)) throw new InvalidDataException("Audio frame checksum mismatch");
            uint a = Convert.ToUInt32(m.Groups[2].Value, 16), b = Convert.ToUInt32(m.Groups[3].Value, 16), c = Convert.ToUInt32(m.Groups[4].Value, 16);
            if (code == 0xa5100000U) {
                if (report != null) throw new InvalidDataException("Incomplete audio block before next header");
                if (b != 8000 || c != 2048) throw new InvalidDataException("Unsupported sample rate or block length");
                report = new Pa5Report(); report.Sequence = a; values.Clear(); stage = 1;
            } else if (report == null) {
                // Listening may begin in the middle of an earlier export.
                continue;
            } else if (code == 0xa5100001U && stage == 1) {
                report.FirstCycle = a; report.LastCycle = b; report.MaxLateCycles = c; stage = 2;
            } else if (code == 0xa5100002U && stage == 2) {
                report.MinIntervalCycles = a; report.MaxIntervalCycles = b;
                uint elapsed = unchecked(report.LastCycle - report.FirstCycle);
                if (c != 48000000 || report.MaxLateCycles > 300 || a < 5700 || b > 6300 || a > b || elapsed < 12281700 || elapsed > 12282300)
                    throw new InvalidDataException("Invalid sample timing; block is not an 8 kHz capture");
                stage = 3;
            } else if (stage >= 3 && stage <= 344 && code == 0xa5200000U + (uint)(stage - 3)) {
                foreach (uint word in new uint[] { a, b, c }) {
                    foreach (ushort sample in new ushort[] { (ushort)(word & 65535), (ushort)(word >> 16) }) {
                        if (values.Count < 2048) {
                            if (sample > 4095) throw new InvalidDataException("ADC sample outside 12-bit range");
                            values.Add(sample);
                        } else if (sample != 0) throw new InvalidDataException("Nonzero final padding");
                    }
                }
                ++stage;
            } else if (code == 0xa51f0000U && stage == 345) {
                if (a != report.Sequence || c != 2048 || values.Count != 2048) throw new InvalidDataException("Audio trailer mismatch");
                var raw = new byte[4096];
                long sumCodes = 0, sumSquares = 0;
                report.Minimum = 4095; report.Maximum = 0;
                for (int i = 0; i < values.Count; ++i) {
                    int sample = values[i]; raw[i*2] = (byte)sample; raw[i*2+1] = (byte)(sample >> 8);
                    sumCodes += sample; sumSquares += sample * sample;
                    report.Minimum = Math.Min(report.Minimum, sample); report.Maximum = Math.Max(report.Maximum, sample);
                    if (sample == 0) ++report.ClipLow; if (sample == 4095) ++report.ClipHigh;
                }
                if (Crc(raw) != b) throw new InvalidDataException("Whole-block CRC32 mismatch");
                report.DcCodes = sumCodes / 2048.0;
                report.AcRmsCodes = Math.Sqrt(Math.Max(0, sumSquares / 2048.0 - report.DcCodes * report.DcCodes));
                string name = "pa5-" + results.Count.ToString("D3") + "-" + report.Sequence.ToString();
                report.RawFile = Path.Combine(folder, name + ".adc.u16le");
                report.WavFile = Path.Combine(folder, name + ".wav");
                if (File.Exists(report.RawFile) || File.Exists(report.WavFile)) throw new IOException("Output already exists");
                // Fixed gain x8, DC removed. No normalisation or noise reduction.
                // Raw ADC codes above preserve absolute levels for analysis.
                var wav = new MemoryStream();
                using (var writer = new BinaryWriter(wav, Encoding.ASCII, true)) {
                    writer.Write(Encoding.ASCII.GetBytes("RIFF")); writer.Write(4132);
                    writer.Write(Encoding.ASCII.GetBytes("WAVEfmt ")); writer.Write(16);
                    writer.Write((ushort)1); writer.Write((ushort)1); writer.Write(8000); writer.Write(16000);
                    writer.Write((ushort)2); writer.Write((ushort)16);
                    writer.Write(Encoding.ASCII.GetBytes("data")); writer.Write(4096);
                    foreach (ushort sample in values) writer.Write((short)Math.Round((sample - report.DcCodes) * 8));
                }
                using (var file = new FileStream(report.RawFile, FileMode.CreateNew)) file.Write(raw, 0, raw.Length);
                byte[] wave = wav.ToArray();
                using (var file = new FileStream(report.WavFile, FileMode.CreateNew)) file.Write(wave, 0, wave.Length);
                results.Add(report); report = null; stage = 0;
            } else throw new InvalidDataException("Missing, duplicated or out-of-order audio frame");
        }
        if (report != null) throw new InvalidDataException("Truncated audio block; press # again while listening");
        if (results.Count == 0) throw new InvalidDataException("No complete audio block; start listening before pressing #");
        return results.ToArray();
    }
}
