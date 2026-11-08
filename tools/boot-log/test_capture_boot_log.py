"""Pure local tests: fake serial transport only; never open a COM port."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

import capture_boot_log as capture


def frame(code=0x100, a=0, b=0, c=0):
    body = f"UV98D1 {code:08X} {a:08X} {b:08X} {c:08X}".encode("ascii")
    return body + f"*{sum(body) % 256:02X}\r\n".encode("ascii")


def text_frames(data, fault=0xE010, truncated=0):
    result = frame(fault, len(data), truncated)
    for index, offset in enumerate(range(0, len(data), 12)):
        chunk = data[offset:offset + 12].ljust(12, b"\x00")
        words = [int.from_bytes(chunk[pos:pos + 4], "little") for pos in (0, 4, 8)]
        result += frame(0x1F000000 | index, *words)
    return result


class FakeClock:
    def __init__(self):
        self.value = 10.0

    def __call__(self):
        return self.value


class FakePort:
    """There is deliberately no write or send method."""
    def __init__(self, clock, parts=(), read_error=None, close_error=None):
        self.clock = clock
        self.parts = list(parts)
        self.timeout = 0.1
        self.read_error = read_error
        self.close_error = close_error
        self.closed = False
        self.reads = 0

    def read(self, count):
        self.reads += 1
        self.clock.value += self.timeout
        if not self.parts:
            if self.read_error is not None:
                raise self.read_error
            return b""
        part = self.parts.pop(0)
        if len(part) > count:
            self.parts.insert(0, part[count:])
        return part[:count]

    def close(self):
        self.closed = True
        if self.close_error is not None:
            raise self.close_error


def port_info(**changes):
    values = dict(device="COM20", vid=0x1A86, pid=0x7523, description="Fake CH340", hwid="FAKE")
    return SimpleNamespace(**(values | changes))


class DecoderTests(unittest.TestCase):
    def test_throughput_is_distinct_from_max_loop_and_rf_latency(self):
        records = capture.StreamDecoder().feed(frame(0x176, 215, 1_500_000, 310), 0)
        self.assertEqual(records[0]["decoded_details"], {
            "loop_count": 215, "active_loop_us": 1_500_000, "max_redraw_backlog_ms": 310})
        self.assertEqual(records[0]["label"], "loop_work_and_redraw_backlog")

    def test_radio018_rx_signal_and_register_readbacks(self):
        d = capture.StreamDecoder()
        records = d.feed(frame(0x174,12640000,(167<<16)|0x0402,7|(90<<8)) +
                         frame(0x175,0x302E03BE,0x6740B32C,0x1234),0)
        self.assertEqual(records[0]["decoded_details"], {"frequency_10hz":12640000,"raw_rssi":167,
            "status_0c":0x0402,"am":True,"monitor":True,"audio_open":True,"am_gain_index":90})
        self.assertEqual(records[1]["decoded_details"], {"register_7e":0x302E,"register_13":0x03BE,
            "register_47":0x6740,"register_48":0xB32C,"register_01":0x1234})


    def test_radio018_labels_fields_and_raw_stock_aux_codes(self):
        vectors = [
            (0x170, [2, 43180000, 0], "radio_state_frequency_power"),
            (0x171, [2500, 0x16, 1], "sidekeys_adc_raw_key_pb6"),
            (0x172, [9000, 8000, 24], "loop_render_timing_rows"),
            (0x173, [4, 0xE055, 3], "radio_fault_state_code_cleanup"),
        ]
        for code, values, label in vectors:
            parsed = capture.parse_frame(frame(code, *values))
            self.assertEqual(parsed["label"], label)
            self.assertEqual(parsed["details"], values)
            self.assertTrue(parsed["checksum_valid"])
        for code, name in [(0x16, "Aux16"), (0x17, "Aux17"), (0x18, "Aux18"), (0xFF, "Released"), (0xAA, "Unknown")]:
            details = capture.parse_frame(frame(0x171, 2500, code, 0))["decoded_details"]
            self.assertEqual(details["stock_auxiliary_hex"], f"0x{code:02X}")
            self.assertEqual(details["auxiliary_name"], name)
            self.assertFalse(details["pb6_pressed"])
            self.assertNotIn("ptt_b", details)
        self.assertIsNone(capture.parse_frame(frame(0x171, 0, 0, 2))["decoded_details"]["pb6_pressed"])

    def test_radio018_long_capture_retains_fatal_text_and_metadata_is_not_text_header(self):
        # Worst permitted logger profile fits comfortably inside capture limit.
        source = b"".join(frame(0x131, index) for index in range(176))
        source += frame(0x173, 4, 0xE055, 0)
        text = b"F" * 96
        source += text_frames(text, fault=0xE055)
        self.assertLess(len(source), capture.MAX_CAPTURE_BYTES)
        decoder = capture.StreamDecoder()
        events = []
        for index in range(0, len(source), 31):
            events.extend(decoder.feed(source[index:index+31]))
        events.extend(decoder.finish())
        texts = [e for e in events if e["kind"] == "fault_text"]
        self.assertEqual(len(texts), 1)
        self.assertEqual(texts[0]["fault_code_hex"], "0x0000E055")
        self.assertTrue(texts[0]["complete"])
        self.assertEqual(texts[0]["raw_hex"], text.hex())
        self.assertEqual(decoder.valid_frames, 186)
        decoder = capture.StreamDecoder()
        # Values which happen to resemble ErrorText metadata remain radio data.
        events = decoder.feed(frame(0x173, 96, 1, 0) + frame(0x1F000000)) + decoder.finish()
        self.assertFalse(any(e["kind"] == "fault_text" for e in events))
        self.assertFalse(events[-1]["text_chunk_associated"])

    def test_known_literal_and_numeric_boundaries(self):
        raw = b"UV98D1 00000100 00000000 00000000 00000000*12\r\n"
        # Fixed vector prevents the test encoder from defining the protocol.
        self.assertEqual(len(raw), 47)
        result = capture.parse_frame(raw)
        self.assertEqual(result["code"], 0x100)
        self.assertEqual(result["label"], "rust_entry")
        self.assertEqual(capture.parse_frame(frame(0xFFFFFFFF, 0xFFFFFFFF, 0x80000000, 0))["details"],
                         [0xFFFFFFFF, 0x80000000, 0])

    def test_bad_checksum_syntax_and_non_ascii(self):
        raw = frame()
        for bad in (raw[:43] + b"00" + raw[45:], raw.replace(b"UV98", b"uv98"),
                    raw[:10] + b"\xff" + raw[11:], raw[:-2] + b"\n", raw[:-1],
                    frame(0xABCD).replace(b"ABCD", b"abcd")):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                capture.parse_frame(bad)

    def test_fragmented_and_coalesced_frames_with_noise(self):
        source = b"\x06\x06READY\xff" + frame(1, 2, 3, 4) + frame(0x100) + frame(0x124)
        for width in (1, 2, 7, 46, 47, len(source)):
            decoder = capture.StreamDecoder()
            events = []
            for index in range(0, len(source), width):
                events.extend(decoder.feed(source[index:index + width], index))
                self.assertLessEqual(len(decoder.buffer), capture.MAX_LINE_BYTES)
            events.extend(decoder.finish())
            records = [event for event in events if event["kind"] == "record"]
            self.assertEqual([event["code"] for event in records], [1, 0x100, 0x124])
            self.assertEqual([event["byte_offset"] for event in records], [8, 55, 102])
            self.assertEqual(decoder.summary()["noise_bytes"], 8)
            self.assertEqual(decoder.summary()["ready_markers_observed"], 1)
            self.assertEqual(decoder.summary()["device_boot_outcome"], "not_inferred_from_capture")

    def test_bootloader_ack_stock_frames_do_not_become_records(self):
        decoder = capture.StreamDecoder()
        events = decoder.feed(b"\x06\x06\x06PROGRAM\x02" + bytes(range(256))) + decoder.finish()
        self.assertEqual(events, [])
        self.assertEqual(decoder.valid_frames, 0)

    def test_truncated_restarted_and_repeated_boots_are_preserved(self):
        decoder = capture.StreamDecoder()
        partial = frame(1)[:21]
        events = decoder.feed(partial + frame(1) + frame(0x100) + frame(1) + frame(0x100))
        self.assertEqual(events[0]["reason"], "new_marker_before_terminator")
        self.assertEqual(events[1]["byte_offset"], len(partial))
        self.assertEqual(decoder.summary()["early_identity_markers_observed"], 2)
        self.assertEqual(decoder.summary()["rust_entry_markers_observed"], 2)

    def test_huge_garbage_partial_and_finish_idempotent(self):
        decoder = capture.StreamDecoder()
        events = decoder.feed(capture.MARKER + b"A" * 20000)
        self.assertLessEqual(len(decoder.buffer), capture.MAX_LINE_BYTES)
        self.assertEqual(events[0]["reason"], "unterminated_frame_limit")
        self.assertLessEqual(len(events[0]["prefix_hex"]), capture.MAX_LINE_BYTES * 2)
        events = decoder.feed(frame(0x100)[:25]) + decoder.finish()
        self.assertEqual(events[-1]["reason"], "partial_at_capture_end")
        self.assertEqual(decoder.finish(), [])
        with self.assertRaises(ValueError):
            decoder.feed(b"x")

    def test_checksum_error_recovers_next_valid_frame(self):
        decoder = capture.StreamDecoder()
        bad = frame()[:43] + b"00\r\n"
        events = decoder.feed(bad + frame(0xDEADBEEF))
        self.assertEqual(events[0]["reason"], "frame_checksum")
        self.assertEqual(events[1]["label"], "unknown_code")
        self.assertEqual(decoder.rejected_frames, 1)

    def test_fault_text_reassembles_little_endian_and_stays_data(self):
        decoder = capture.StreamDecoder()
        source = text_frames(b"Error { expected: 1, actual: 2 }\n\x1b\xff", truncated=1)
        events = []
        for byte in source:
            events.extend(decoder.feed(bytes([byte])))
        texts = [event for event in events if event["kind"] == "fault_text"]
        self.assertEqual(len(texts), 1)
        self.assertTrue(texts[0]["complete"])
        self.assertTrue(texts[0]["source_truncated"])
        self.assertEqual(texts[0]["text_ascii"], "Error { expected: 1, actual: 2 }\n\x1b\\xff")

    def test_fault_text_missing_reordered_or_restart_not_joined(self):
        for ending in (frame(0x1F000001, 1, 2, 3), frame(1), b"unexpected noise"):
            decoder = capture.StreamDecoder()
            events = decoder.feed(frame(0xE010, 24, 0, 0) + ending) + decoder.finish()
            texts = [event for event in events if event["kind"] == "fault_text"]
            self.assertEqual(len(texts), 1)
            self.assertFalse(texts[0]["complete"])
        decoder = capture.StreamDecoder()
        events = decoder.feed(frame(0x1F000000, 1, 2, 3))
        self.assertFalse(events[0]["text_chunk_associated"])

    def test_fault_text_partial_end_bad_checksum_and_nonzero_padding(self):
        decoder = capture.StreamDecoder()
        events = decoder.feed(frame(0xE010, 24) + frame(0x1F000000, 1)) + decoder.finish()
        self.assertFalse(events[-1]["complete"])
        self.assertEqual(events[-1]["stop_reason"], "capture_ended")
        decoder = capture.StreamDecoder()
        events = decoder.feed(frame(0xE010, 1) + frame(0x1F000000, 0x4141))
        self.assertFalse(events[-1]["complete"])
        self.assertEqual(events[-1]["stop_reason"], "nonzero_padding")
        decoder = capture.StreamDecoder()
        events = decoder.feed(frame(0xE010, 24) + frame()[:43] + b"00\r\n")
        self.assertEqual(events[1]["stop_reason"], "rejected_record")

    def test_maximum_fault_text_is_exactly_eight_chunks(self):
        decoder = capture.StreamDecoder()
        source = bytes(range(32, 128))
        events = decoder.feed(text_frames(source))
        texts = [event for event in events if event["kind"] == "fault_text"]
        chunks = [event for event in events if event.get("label") == "fault_text_chunk"]
        self.assertEqual(len(chunks), 8)
        self.assertEqual(texts[0]["raw_hex"], source.hex())
        self.assertTrue(texts[0]["complete"])


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.output = self.base / "new-capture"
        self.clock = FakeClock()

    def run_capture(self, port, **kwargs):
        return capture.capture("COM20", self.output, 1,
                               factory=lambda name: (port, {"port": name}), clock=self.clock,
                               announce=lambda text: None, **kwargs)

    def test_timeout_without_rx_is_not_boot_failure(self):
        port = FakePort(self.clock)
        report = self.run_capture(port)
        self.assertEqual(report["status"], "capture_complete")
        self.assertEqual(report["raw_bytes"], 0)
        self.assertTrue(report["port_closed"])
        self.assertTrue(port.closed)
        self.assertEqual(report["decoder"]["device_boot_outcome"], "not_inferred_from_capture")
        self.assertEqual(report["transmitted_bytes"], 0)
        self.assertFalse(hasattr(port, "write"))
        self.assertLessEqual(self.clock.value, 11.000001)

    def test_raw_chunks_and_partials_retained_with_offsets_timestamps(self):
        parts = [b"\x06\xff", frame(1)[:5], frame(1)[5:] + frame(0x100), frame(0x124)[:14]]
        report = self.run_capture(FakePort(self.clock, parts))
        self.assertEqual((self.output / "rx.bin").read_bytes(), b"".join(parts))
        chunks = [json.loads(line) for line in (self.output / "chunks.jsonl").read_text().splitlines()]
        self.assertEqual([item["byte_offset"] for item in chunks], [0, 2, 7, 96])
        self.assertEqual([item["received_monotonic_ms"] for item in chunks], [100.0, 200.0, 300.0, 400.0])
        self.assertEqual(report["decoder"]["valid_records"], 2)
        self.assertEqual(report["decoder"]["rejected_records"], 1)
        self.assertEqual(report["artifacts"]["rx.bin"]["sha256"], capture.sha256(b"".join(parts)))

    def test_finite_byte_limit_keeps_exact_prefix(self):
        source = frame() * 30
        report = self.run_capture(FakePort(self.clock, [source]), max_bytes=61)
        self.assertEqual((self.output / "rx.bin").read_bytes(), source[:61])
        self.assertEqual(report["status"], "capture_limit_reached")
        self.assertEqual(report["raw_bytes"], 61)
        self.assertEqual(report["decoder"]["rejected_records"], 1)

    def test_read_error_and_interrupt_close_port_keep_manifest(self):
        for error in (OSError("fake disconnect"), KeyboardInterrupt()):
            self.output = self.base / type(error).__name__
            port = FakePort(self.clock, [frame()[:11]], read_error=error)
            with self.assertRaises(type(error)):
                self.run_capture(port)
            report = json.loads((self.output / "manifest.json").read_text())
            self.assertEqual(report["status"], "failed")
            self.assertTrue(port.closed)
            self.assertEqual(report["raw_bytes"], 11)
            self.assertEqual(report["decoder"]["rejected_records"], 1)

    def test_close_error_retains_raw_and_failure_manifest(self):
        port = FakePort(self.clock, [frame()], close_error=OSError("close failed"))
        with self.assertRaisesRegex(OSError, "close failed"):
            self.run_capture(port)
        report = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(report["status"], "failed")
        self.assertIn("close failed", report["close_error"])
        self.assertFalse(report["port_closed"])
        self.assertEqual((self.output / "rx.bin").read_bytes(), frame())

    def test_factory_error_keeps_manifest_without_claiming_open(self):
        def fail(name):
            raise OSError("port busy")
        with self.assertRaisesRegex(OSError, "port busy"):
            capture.capture("COM20", self.output, 1, factory=fail)
        report = json.loads((self.output / "manifest.json").read_text())
        self.assertFalse(report["device_accessed"])
        self.assertEqual(report["status"], "failed")

    def test_invalid_duration_or_existing_output_never_opens(self):
        with patch.object(capture, "serial_factory", side_effect=AssertionError("must not open")):
            for duration in (0, -1, 0.5, 121, float("nan"), float("inf")):
                with self.subTest(duration=duration), self.assertRaises(ValueError):
                    capture.capture("COM20", self.output, duration, factory=capture.serial_factory)
            self.output.mkdir()
            with self.assertRaises(FileExistsError):
                capture.capture("COM20", self.output, 1, factory=capture.serial_factory)
        self.assertEqual(capture.validate_duration(120), 120)

    def test_cli_plan_only_does_not_create_directory_or_access_serial(self):
        with patch.object(capture, "capture", side_effect=AssertionError("must not capture")), \
                contextlib.redirect_stdout(io.StringIO()) as output:
            result = capture.main(["--output", str(self.output)])
        self.assertEqual(result, 0)
        self.assertIn("plan_only", output.getvalue())
        self.assertFalse(self.output.exists())

    def test_offline_cli_preserves_source_and_never_accesses_serial(self):
        source = self.base / "synthetic.bin"
        data = b"\xff\x06" + frame() + frame(0x124)[:11]
        source.write_bytes(data)
        with patch.object(capture, "capture", side_effect=AssertionError("must not capture")), \
                patch.object(capture, "serial_factory", side_effect=AssertionError("must not open")), \
                contextlib.redirect_stdout(io.StringIO()):
            result = capture.main(["--decode", str(source), "--output", str(self.output)])
        self.assertEqual(result, 0)
        self.assertEqual(source.read_bytes(), data)
        self.assertEqual((self.output / "rx.bin").read_bytes(), data)
        report = json.loads((self.output / "manifest.json").read_text())
        self.assertFalse(report["device_accessed"])
        self.assertEqual(report["decoder"]["valid_records"], 1)

    def test_offline_oversized_refused_before_creating_output(self):
        source = self.base / "large.bin"
        source.write_bytes(b"x" * (capture.MAX_CAPTURE_BYTES + 1))
        with self.assertRaisesRegex(ValueError, "128 KiB"):
            capture.decode_file(source, self.output)
        self.assertFalse(self.output.exists())


class FactoryTests(unittest.TestCase):
    def test_rx_polling_does_not_reconfigure_unchanged_timeout(self):
        clock = FakeClock()
        class DriverPort(FakePort):
            @property
            def timeout(self):
                return self._timeout
            @timeout.setter
            def timeout(self, value):
                if hasattr(self, "_timeout") and value == self._timeout:
                    raise OSError("redundant DCB reconfiguration")
                self._timeout = value
        driver = DriverPort(clock, [frame(0x124)])
        with tempfile.TemporaryDirectory() as directory:
            report = capture.capture("COM20", Path(directory) / "capture", 1,
                factory=lambda _: (capture.ReceiveOnlyPort(driver), {"port": "COM20"}),
                clock=clock, announce=lambda _: None)
        self.assertEqual(report["status"], "capture_complete")
        self.assertEqual(report["decoder"]["valid_records"], 1)
        self.assertEqual(report["transmitted_bytes"], 0)
        self.assertGreater(driver.reads, 1)
        self.assertLessEqual(clock.value, 11.000001)
        self.assertTrue(driver.closed)

    def test_changed_timeout_error_is_not_hidden(self):
        class DriverPort:
            @property
            def timeout(self):
                return 0.1
            @timeout.setter
            def timeout(self, value):
                raise OSError("device unavailable")
        port = capture.ReceiveOnlyPort(DriverPort())
        with self.assertRaisesRegex(OSError, "device unavailable"):
            port.timeout = 0.025

    def test_only_expected_port_and_identity_accepted(self):
        self.assertEqual(capture.require_port("COM20", [port_info()])["vid"], 0x1A86)
        for name, ports in (("COM19", [port_info(device="COM19")]), ("COM20", []),
                            ("COM20", [port_info(pid=1)]), ("COM20", [port_info(vid=1)]),
                            ("COM20", [port_info(), port_info()])):
            with self.subTest(name=name, ports=ports), self.assertRaises(ValueError):
                capture.require_port(name, ports)

    def test_factory_configures_control_lines_before_open_no_writes(self):
        calls = []
        class Serial:
            def __init__(self, **settings):
                calls.append(("construct", settings))
                self.timeout = settings["timeout"]
                self.dtr = True
                self.rts = True
            def open(self):
                calls.append(("open", self.port, self.dtr, self.rts))
            def read(self, count):
                calls.append(("read", count))
                return b""
            def close(self):
                calls.append(("close",))
            def write(self, data):
                raise AssertionError("Serial TX is prohibited")
        serial = ModuleType("serial")
        serial.Serial = Serial
        serial_tools = ModuleType("serial.tools")
        serial_tools.list_ports = SimpleNamespace(comports=lambda: [port_info()])
        with patch.dict("sys.modules", {"serial": serial, "serial.tools": serial_tools}):
            port, identity = capture.serial_factory("COM20")
            self.assertFalse(hasattr(port, "write"))
            self.assertFalse(hasattr(port, "send"))
            port.read(10)
            port.close()
        self.assertEqual(calls[1], ("open", "COM20", False, False))
        settings = calls[0][1]
        self.assertEqual((settings["baudrate"], settings["bytesize"], settings["parity"], settings["stopbits"]),
                         (115200, 8, "N", 1))
        self.assertIsNone(settings["port"])
        self.assertFalse(settings["xonxoff"] or settings["rtscts"] or settings["dsrdtr"])
        self.assertEqual(identity["pid"], 0x7523)


if __name__ == "__main__":
    unittest.main()
