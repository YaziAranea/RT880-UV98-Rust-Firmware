"""Bounded RX-only capture of UV98 diagnostic early boot logs.

Default is a local plan: no COM port is opened unless --listen is supplied.
--decode reads a saved local file and also never opens a port. This tool has
no serial write operation, loader command, firmware image or RF command.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import time

MARKER = b"UV98D1 "
FRAME_BYTES = 47
MAX_LINE_BYTES = 160
MAX_CAPTURE_BYTES = 128 * 1024
READ_CHUNK_BYTES = 1024
MAX_DURATION_SECONDS = 120.0
POLL_SECONDS = 0.1
FRAME = re.compile(rb"UV98D1 ([0-9A-F]{8}) ([0-9A-F]{8}) ([0-9A-F]{8}) ([0-9A-F]{8})\*([0-9A-F]{2})\r\n")
STAGES = {
    0x1: "early_identity", 0x2: "early_rf_off_requests", 0x3: "early_preinit_complete",
    0x100: "rust_entry", 0x101: "early_quiesce_ok", 0x102: "initialize_begin",
    0x103: "initialize_ok", 0x104: "dwt_ok", 0x110: "rf_shutdown_begin",
    0x111: "rf_shutdown_ok", 0x120: "peripherals_prepared", 0x121: "app_start_begin",
    0x122: "app_start_ok", 0x123: "rf_verify_backlight_begin", 0x124: "ready_marker",
    0x130: "key_input", 0x131: "heartbeat",
    0x132: "ui_key_screen_channel",
    0x140: "lcd_initialize", 0x141: "calibration_read", 0x142: "channel_read",
    0x143: "render", 0x144: "app_complete",
    0x150: "nor_prefix_bytes_00_11", 0x151: "nor_prefix_bytes_12_23",
    0x152: "nor_spi2_ctrl1_ctrl2_sts", 0x153: "nor_gpiob_cfgr_odrvr_muxh",
    0x154: "nor_gpiob_idt_odt_pull",
    0x160: "lcd_spi3_ctrl1_ctrl2_sts", 0x161: "lcd_gpio_idt_b_a_c",
    0x170: "radio_state_frequency_power",
    0x171: "sidekeys_adc_raw_key_pb6",
    0x172: "loop_render_timing_rows",
    0x173: "radio_fault_state_code_cleanup",
    0x174: "receive_signal_status_agc",
    0x175: "receive_gain_audio_registers",
    0x176: "loop_work_and_redraw_backlog",
    0xE001: "error_watchdog_feed", 0xE002: "error_zero_constant",
    0xE00F: "error_early_quiesce", 0xE010: "error_initialize",
    0xE011: "error_mcu_peripherals_taken", 0xE012: "error_core_peripherals_taken",
    0xE013: "error_no_cycle_counter", 0xE014: "error_cycle_counter_stopped",
    0xE015: "error_cycle_time_initialize", 0xE016: "error_cycle_time_sample",
    0xE020: "error_rf_shutdown_initialize", 0xE021: "error_rf_shutdown_run",
    0xE022: "error_rf_verify_off", 0xE023: "error_backlight",
    0xE030: "error_key_timing", 0xE031: "error_app_start", 0xE032: "error_app_sample",
    0xE033: "error_nor_prefix_read",
    0xE040: "error_uart_takeover", 0xE0FF: "panic",
    0xE1000001: "early_error_pid", 0xE1000002: "early_error_flash",
    0xE1000003: "early_error_irq_banks", 0xE1000004: "early_error_lext",
    0xE1000005: "early_error_gpio_clock", 0xE1000006: "early_error_pin_lock",
    0xE1000007: "early_error_series",
    0xE1000008: "early_error_context_control_ipsr_msp",
    0xE1000009: "early_error_family_series_expected_mask",
    0xE1FF0000: "exception_ipsr_cfsr_hfsr",
    0x200: "probe_first_marker", 0x201: "probe_entry_context",
    0x202: "probe_initial_crm", 0x203: "probe_initial_misc_pll",
    0x210: "probe_before_cpuid", 0x211: "probe_cpuid",
    0x212: "probe_before_pid", 0x213: "probe_pid",
    0x214: "probe_before_series", 0x215: "probe_series",
    0x216: "probe_before_flash_size", 0x217: "probe_flash_size",
    0x220: "probe_complete",
    0xE2FF0000: "probe_exception_ipsr_cfsr_hfsr",
    # 0.1.3 pre_init.s: 302 observes clocks AFTER HICK8 normalization.
    # Watchdog/guard records are observations, not evidence of passed checks.
    0x300: "preinit_bootstrap_marker",
    0x301: "preinit_context_control_ipsr_msp",
    0x302: "preinit_normalized_crm_ctrl_cfg_pllcfg",
    0x303: "preinit_guard_lext_ictr_ahben1",
    0x304: "preinit_gpio_lock_a_b_c",
    0x305: "preinit_gpio_lock_f_wdt_sts_winv",
    0x306: "preinit_wdt_div_rld_wwdt_ctrl",
}
# Rust fatal headers reserve E000..E0FF, including new0.1.8 integration stages.
# The header still needs the strict length/truncated/reserved fields below.
# Radio metadata0173 is NOT an ErrorText header despite carrying an error code.
RUST_FAULTS = set(range(0xE000, 0xE100))


def code_label(code):
    if code in STAGES:
        return STAGES[code]
    if code in RUST_FAULTS:
        return "rust_fault"
    if 0x1F000000 <= code <= 0x1F000007:
        return "fault_text_chunk"
    return "unknown_code"


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def validate_duration(value):
    duration = float(value)
    if not math.isfinite(duration) or not 1 <= duration <= MAX_DURATION_SECONDS:
        raise ValueError("Duration must be finite and between 1 and 120 seconds")
    return duration


def parse_frame(raw):
    """Strict complete-frame parser, not a serial or hardware operation."""
    if len(raw) != FRAME_BYTES:
        raise ValueError("frame_length")
    match = FRAME.fullmatch(raw)
    if match is None:
        raise ValueError("frame_syntax")
    if (sum(raw[:42]) & 255) != int(match[5], 16):
        raise ValueError("frame_checksum")
    values = [int(match[index], 16) for index in range(1, 5)]
    result = {"code": values[0], "code_hex": f"0x{values[0]:08X}",
            "label": code_label(values[0]),
            "details": values[1:], "details_hex": [f"0x{x:08X}" for x in values[1:]],
            "checksum_valid": True}
    # Name transmitted fields without inferring physical RF, watts, button B,
    # measured clock accuracy, or a successful boot from a software state code.
    code, a, b, c = values
    if code == 0x170:
        result["decoded_details"] = {"state_code": a, "frequency_10hz": b, "power_code": c}
    elif code == 0x171:
        result["decoded_details"] = {"adc_raw": a, "stock_auxiliary_code": b,
            "stock_auxiliary_hex": f"0x{b:02X}",
            "auxiliary_name": {0x16: "Aux16", 0x17: "Aux17", 0x18: "Aux18", 0xFF: "Released"}.get(b, "Unknown"),
            "pb6_pressed_code": c, "pb6_pressed": bool(c) if c in (0, 1) else None}
    elif code == 0x172:
        result["decoded_details"] = {"max_loop_us": a, "max_render_us": b, "rows": c}
    elif code == 0x173:
        result["decoded_details"] = {"state_code": a, "fault_code": b, "cleanup_failures": c}
    elif code == 0x174:
        result["decoded_details"] = {"frequency_10hz": a, "raw_rssi":b>>16, "status_0c":b&65535,
            "am":bool(c&1), "monitor":bool(c&2), "audio_open":bool(c&4), "am_gain_index":(c>>8)&255}
    elif code == 0x175:
        result["decoded_details"] = {"register_7e":a>>16,"register_13":a&65535,
            "register_47":b>>16,"register_48":b&65535,"register_01":c}
    elif code == 0x176:
        result["decoded_details"] = {"loop_count": a, "active_loop_us": b,
            "max_redraw_backlog_ms": c}
    return result


class StreamDecoder:
    """Resynchronizes across noise, fragmentation and power loss mid-record.

    A new marker before newline rejects the unfinished record; it never joins
    records from two starts. The buffer stays <=MAX_LINE_BYTES between calls.
    The caller bounds total input, so event history is bounded as well.
    """
    def __init__(self):
        self.buffer = bytearray()
        self.buffer_offset = 0
        self.input_bytes = 0
        self.noise_bytes = 0
        self.valid_frames = 0
        self.rejected_frames = 0
        self.last_record = None
        self.ready_markers = 0
        self.early_identity_markers = 0
        self.rust_entry_markers = 0
        self.pending_text = None
        self.finished = False

    def _drop(self, count):
        del self.buffer[:count]
        self.buffer_offset += count

    def _rejection(self, count, reason, received_ms):
        self.rejected_frames += 1
        return {"kind": "rejected", "byte_offset": self.buffer_offset,
                "bytes": count, "reason": reason,
                "prefix_hex": bytes(self.buffer[:min(count, MAX_LINE_BYTES)]).hex(),
                "received_monotonic_ms": received_ms}

    def _finish_text(self, reason):
        pending = self.pending_text
        if pending is None:
            return []
        self.pending_text = None
        data = bytes(pending.pop("data"))
        length = pending["expected_bytes"]
        complete = reason == "complete" and len(data) >= length and not any(data[length:])
        return [{"kind": "fault_text", **pending, "complete": complete,
                 "stop_reason": reason if complete or reason != "complete" else "nonzero_padding",
                 "received_bytes": min(len(data), length), "raw_hex": data[:length].hex(),
                 "text_ascii": data[:length].decode("ascii", errors="backslashreplace")}]

    def _annotate(self, event):
        code, details = event["code"], event["details"]
        extra = []
        is_text = 0x1F000000 <= code <= 0x1F000007
        if self.pending_text is not None and not is_text:
            extra.extend(self._finish_text("interrupted_by_record"))
        if code == 0x124:
            self.ready_markers += 1
        elif code == 0x1:
            self.early_identity_markers += 1
        elif code == 0x100:
            self.rust_entry_markers += 1
        if code in RUST_FAULTS and 1 <= details[0] <= 96 and details[1] in (0, 1) and details[2] == 0:
            self.pending_text = {"fault_code_hex": event["code_hex"],
                                 "byte_offset": event["byte_offset"], "expected_bytes": details[0],
                                 "source_truncated": bool(details[1]), "next_index": 0, "data": bytearray()}
        if is_text:
            index = code & 7
            data = b"".join(value.to_bytes(4, "little") for value in details)
            event["text_chunk_index"] = index
            event["text_chunk_hex"] = data.hex()
            event["text_chunk_associated"] = False
            if self.pending_text is not None:
                if index != self.pending_text["next_index"]:
                    extra.extend(self._finish_text("missing_or_reordered_chunk"))
                else:
                    event["text_chunk_associated"] = True
                    self.pending_text["data"].extend(data)
                    self.pending_text["next_index"] += 1
                    if len(self.pending_text["data"]) >= self.pending_text["expected_bytes"]:
                        extra.extend(self._finish_text("complete"))
        return extra

    def feed(self, data, received_ms=None):
        if self.finished:
            raise ValueError("Decoder already finished")
        self.buffer.extend(data)
        self.input_bytes += len(data)
        events = []
        while self.buffer:
            start = self.buffer.find(MARKER)
            if start < 0:
                # Preserve the possible beginning of a marker crossing chunks.
                count = max(0, len(self.buffer) - len(MARKER) + 1)
                if count:
                    events.extend(self._finish_text("unexpected_bytes"))
                self.noise_bytes += count
                self._drop(count)
                break
            if start:
                events.extend(self._finish_text("unexpected_bytes"))
                self.noise_bytes += start
                self._drop(start)
            newline = self.buffer.find(b"\n")
            restart = self.buffer.find(MARKER, len(MARKER))
            if restart >= 0 and (newline < 0 or restart < newline):
                events.extend(self._finish_text("rejected_record"))
                events.append(self._rejection(restart, "new_marker_before_terminator", received_ms))
                self._drop(restart)
                continue
            if newline >= 0:
                count = newline + 1
                raw = bytes(self.buffer[:count])
                try:
                    value = parse_frame(raw)
                except ValueError as error:
                    events.extend(self._finish_text("rejected_record"))
                    events.append(self._rejection(count, str(error), received_ms))
                else:
                    self.valid_frames += 1
                    event = {"kind": "record", "byte_offset": self.buffer_offset,
                             "bytes": count, "received_monotonic_ms": received_ms, **value}
                    self.last_record = event
                    events.append(event)
                    events.extend(self._annotate(event))
                self._drop(count)
                continue
            if len(self.buffer) > MAX_LINE_BYTES:
                events.extend(self._finish_text("rejected_record"))
                count = len(self.buffer) - len(MARKER) + 1
                events.append(self._rejection(count, "unterminated_frame_limit", received_ms))
                self._drop(count)
                continue
            break
        return events

    def finish(self):
        if self.finished:
            return []
        self.finished = True
        events = []
        if self.buffer.startswith(MARKER):
            events.append(self._rejection(len(self.buffer), "partial_at_capture_end", None))
        else:
            self.noise_bytes += len(self.buffer)
        self._drop(len(self.buffer))
        events.extend(self._finish_text("capture_ended"))
        return events

    def summary(self):
        return {"valid_records": self.valid_frames, "rejected_records": self.rejected_frames,
                "noise_bytes": self.noise_bytes, "last_record": self.last_record,
                "device_boot_outcome": "not_inferred_from_capture",
                "ready_markers_observed": self.ready_markers,
                "early_identity_markers_observed": self.early_identity_markers,
                "rust_entry_markers_observed": self.rust_entry_markers,
                "timestamps_are_host_receive_times": True}


def require_port(port_name, ports):
    matching = [port for port in ports if port.device == port_name]
    if port_name != "COM20" or len(matching) != 1 or (matching[0].vid, matching[0].pid) != (0x1A86, 0x7523):
        raise ValueError("Expected COM20 / CH340 VID1A86 PID7523")
    port = matching[0]
    return {"port": port_name, "vid": port.vid, "pid": port.pid,
            "description": port.description, "hwid": port.hwid}


class ReceiveOnlyPort:
    """Capture adapter deliberately exposes no send/write operation."""
    def __init__(self, serial_port):
        self.__serial_port = serial_port

    @property
    def timeout(self):
        return self.__serial_port.timeout

    @timeout.setter
    def timeout(self, value):
        # pyserial on Windows reconfigures the whole DCB even when assigning
        # the existing timeout. Keep the polling loop RX-only without issuing
        # SetCommState ten times per second; still shorten the final read to
        # the actual deadline and propagate any real configuration failure.
        if self.__serial_port.timeout != value:
            self.__serial_port.timeout = value

    def read(self, count):
        return self.__serial_port.read(count)

    def close(self):
        self.__serial_port.close()


def serial_factory(port_name):
    import serial
    from serial.tools import list_ports
    identity = require_port(port_name, list_ports.comports())
    port = serial.Serial(port=None, baudrate=115200, bytesize=8, parity="N", stopbits=1,
                         timeout=POLL_SECONDS, write_timeout=1,
                         xonxoff=False, rtscts=False, dsrdtr=False)
    port.dtr = False
    port.rts = False
    port.port = port_name
    try:
        port.open()
    except BaseException:
        port.close()
        raise
    return ReceiveOnlyPort(port), identity


def write_json_new(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())


def save_events(stream, events):
    for event in events:
        stream.write(json.dumps(event) + "\n")
    stream.flush()


def capture(port_name, output, duration, *, factory=serial_factory, clock=time.monotonic,
            announce=lambda text: print(text, flush=True), max_bytes=MAX_CAPTURE_BYTES):
    duration = validate_duration(duration)
    if not 1 <= max_bytes <= MAX_CAPTURE_BYTES:
        raise ValueError("Invalid capture byte limit")
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    decoder = StreamDecoder()
    report = {"schema": 1, "kind": "rx_only_boot_log_capture", "status": "incomplete",
              "started_utc": utc_now(), "port": port_name, "duration_limit_seconds": duration,
              "byte_limit": max_bytes, "line_buffer_limit": MAX_LINE_BYTES,
              "serial": {"baudrate": 115200, "bytesize": 8, "parity": "N", "stopbits": 1,
                         "dtr": False, "rts": False, "flow_control": False},
              "transmitted_bytes": 0, "firmware_or_loader_commands_sent": False,
              "device_accessed": False, "port_closed": False, "raw_bytes": 0,
              "capture_does_not_prove_boot_success_or_failure": True}
    write_json_new(output / "started.json", report)
    port = None
    failure = None
    try:
        with (output / "rx.bin").open("xb") as raw, \
                (output / "chunks.jsonl").open("x", encoding="utf-8") as chunks, \
                (output / "records.jsonl").open("x", encoding="utf-8") as records:
            try:
                port, identity = factory(port_name)
                report["device_accessed"] = True
                report["identity"] = identity
                start = clock()
                deadline = start + duration
                report["listening_started_utc"] = utc_now()
                announce(f"LISTENING {port_name}: RX only, {duration:g}s; power cycle without PTT now.")
                while True:
                    remaining_time = deadline - clock()
                    if remaining_time <= 0:
                        report["status"] = "capture_complete"
                        report["stop_reason"] = "duration_elapsed"
                        break
                    remaining_bytes = max_bytes - report["raw_bytes"]
                    if remaining_bytes <= 0:
                        report["status"] = "capture_limit_reached"
                        report["stop_reason"] = "byte_limit"
                        break
                    count = min(READ_CHUNK_BYTES, remaining_bytes)
                    port.timeout = min(POLL_SECONDS, remaining_time)
                    data = port.read(count)
                    if len(data) > count:
                        raise ValueError("Transport returned more than requested")
                    if not data:
                        continue
                    received_ms = round((clock() - start) * 1000, 3)
                    offset = report["raw_bytes"]
                    raw.write(data)
                    raw.flush()
                    report["raw_bytes"] += len(data)
                    save_events(chunks, [{"utc": utc_now(), "received_monotonic_ms": received_ms,
                                          "byte_offset": offset, "bytes": len(data),
                                          "sha256": sha256(data)}])
                    events = decoder.feed(data, received_ms)
                    save_events(records, events)
                    for event in events:
                        if event["kind"] == "record":
                            announce(f"BOOTLOG {event['label']} {event['code_hex']} {' '.join(event['details_hex'])}")
                report["listening_elapsed_seconds"] = round(clock() - start, 6)
            finally:
                save_events(records, decoder.finish())
    except BaseException as error:
        report["status"] = "failed"
        report["error"] = f"{type(error).__name__}: {error}"
        failure = error
    finally:
        if port is not None:
            try:
                port.close()
                report["port_closed"] = True
            except BaseException as error:
                report["status"] = "failed"
                report["close_error"] = f"{type(error).__name__}: {error}"
                failure = failure or error
        report["finished_utc"] = utc_now()
        report["decoder"] = decoder.summary()
        report["artifacts"] = {path.name: {"bytes": path.stat().st_size, "sha256": sha256(path.read_bytes())}
                               for path in sorted(output.iterdir()) if path.is_file()}
        write_json_new(output / "manifest.json", report)
    if failure is not None:
        raise failure
    return report


def decode_file(source, output):
    """Decode a bounded saved stream; preserve all source bytes in the result."""
    if not source.is_file():
        raise ValueError("Decode input must be an existing regular file")
    with source.open("rb") as stream:
        data = stream.read(MAX_CAPTURE_BYTES + 1)
    if len(data) > MAX_CAPTURE_BYTES:
        raise ValueError("Saved capture exceeds the 128 KiB decoder bound")
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    decoder = StreamDecoder()
    with (output / "rx.bin").open("xb") as raw:
        raw.write(data)
    with (output / "records.jsonl").open("x", encoding="utf-8") as records:
        for offset in range(0, len(data), READ_CHUNK_BYTES):
            save_events(records, decoder.feed(data[offset:offset + READ_CHUNK_BYTES]))
        save_events(records, decoder.finish())
    report = {"schema": 1, "kind": "offline_boot_log_decode", "status": "decoded",
              "source": str(source.resolve()), "source_sha256": sha256(data),
              "raw_bytes": len(data), "source_bytes_copied_unchanged": True,
              "device_accessed": False, "transmitted_bytes": 0, "decoder": decoder.summary()}
    write_json_new(output / "manifest.json", report)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--listen", action="store_true", help="Explicitly open RX-only COM capture")
    mode.add_argument("--decode", type=Path, help="Decode a saved local RX file without COM access")
    parser.add_argument("--port", default="COM20")
    parser.add_argument("--duration", type=float, default=30.0)
    parser.add_argument("--output", type=Path, required=True, help="New output directory; existing paths refused")
    args = parser.parse_args(argv)
    try:
        validate_duration(args.duration)
        if args.decode is not None:
            result = decode_file(args.decode, args.output)
        elif args.listen:
            result = capture(args.port, args.output, args.duration)
        else:
            print(json.dumps({"mode": "plan_only", "port": args.port, "duration_seconds": args.duration,
                              "new_output_directory": str(args.output.resolve()),
                              "device_accessed": False, "transmitted_bytes": 0}, indent=2))
            return 0
        print(json.dumps({"status": result["status"], "raw_bytes": result["raw_bytes"],
                          "valid_records": result["decoder"]["valid_records"],
                          "rejected_records": result["decoder"]["rejected_records"],
                          "boot_outcome": "not_inferred", "transmitted_bytes": 0}, indent=2))
        return 0
    except KeyboardInterrupt:
        print("Capture interrupted; retained files are incomplete. No serial data was transmitted.")
        return 130
    except Exception as error:
        print(f"Capture/decode failed: {type(error).__name__}: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
