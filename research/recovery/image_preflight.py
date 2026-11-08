"""Strict local-file structural inspection for the related RT880 app profile.

No serial transport, firmware execution, device access, image writing, padding,
or flashing is implemented. Passing checks does not qualify UV98 hardware or
verify recovery. Source bytes and Intel HEX presence are preserved exactly.
Python 3.10+, standard library only.
"""
from dataclasses import dataclass
from pathlib import Path
import argparse
import hashlib
import json
import re
import struct
import sys

APP_BASE = 0x08002800
APP_END = 0x08040000  # exclusive; related 256 KiB MCU profile
APP_BYTES = APP_END - APP_BASE
RAM_BASE = 0x20000000
RAM_END = 0x2000C000  # exclusive for data, allowed as initial empty stack top
BLOCK_BYTES = 1024  # transport-size observation, not an erase-sector size
MAX_HEX_BYTES = 8 * 1024 * 1024


class PreflightError(ValueError):
    pass


@dataclass(frozen=True)
class Segment:
    address: int
    data: bytes


@dataclass(frozen=True)
class Image:
    source_format: str
    source_sha256: str
    segments: tuple[Segment, ...]
    record_count: int = 0
    start_linear_address: int | None = None


def parse_bin(source: bytes) -> Image:
    if not 8 <= len(source) <= APP_BYTES:
        raise PreflightError(f"BIN size must be 8..{APP_BYTES} bytes; no truncation is allowed")
    if source.startswith((b"MZ", b"\x7fELF")):
        raise PreflightError("executable/container input is not an application-only BIN")
    return Image("bin", hashlib.sha256(source).hexdigest(), (Segment(APP_BASE, source),))


def parse_hex(source: bytes) -> Image:
    if not source or len(source) > MAX_HEX_BYTES:
        raise PreflightError("HEX input is empty or exceeds the bounded 8 MiB text limit")
    try:
        text = source.decode("ascii")
    except UnicodeDecodeError as error:
        raise PreflightError("HEX must be ASCII") from error
    # Internal working buffers are never exported. Missing bytes remain absent
    # in the returned segments and are never assigned a padding value.
    values = bytearray(APP_BYTES)
    present = bytearray(APP_BYTES)
    upper = 0
    eof = False
    start_linear = None
    count = 0
    for line_number, original in enumerate(text.splitlines(), 1):
        line = original.strip()
        if not line:
            continue
        if eof:
            raise PreflightError(f"line {line_number}: record after EOF")
        if not line.startswith(":") or not re.fullmatch(r"[0-9A-Fa-f]+", line[1:]):
            raise PreflightError(f"line {line_number}: malformed record")
        if len(line[1:]) % 2:
            raise PreflightError(f"line {line_number}: odd hex digit count")
        record = bytes.fromhex(line[1:])
        if len(record) < 5 or len(record) != record[0] + 5:
            raise PreflightError(f"line {line_number}: length mismatch")
        if sum(record) & 255:
            raise PreflightError(f"line {line_number}: checksum mismatch")
        length = record[0]
        offset = int.from_bytes(record[1:3], "big")
        kind = record[3]
        data = record[4:-1]
        count += 1
        if kind == 0:
            if not length:
                raise PreflightError(f"line {line_number}: empty data record")
            if offset + length > 0x10000:
                raise PreflightError(f"line {line_number}: data crosses a 16-bit address boundary")
            address = upper + offset
            if address < APP_BASE or address + length > APP_END:
                raise PreflightError(f"line {line_number}: data outside the related application window")
            index = address - APP_BASE
            if any(present[index:index + length]):
                raise PreflightError(f"line {line_number}: overlapping data records")
            values[index:index + length] = data
            present[index:index + length] = bytes([1]) * length
        elif kind == 1:
            if length != 0 or offset != 0:
                raise PreflightError(f"line {line_number}: invalid EOF record")
            eof = True
        elif kind == 4:
            if length != 2 or offset != 0:
                raise PreflightError(f"line {line_number}: invalid extended linear address")
            upper = int.from_bytes(data, "big") << 16
        elif kind == 5:
            if length != 4 or offset != 0 or start_linear is not None:
                raise PreflightError(f"line {line_number}: invalid or repeated start-linear record")
            start_linear = int.from_bytes(data, "big")
        else:
            raise PreflightError(f"line {line_number}: unsupported record type {kind:02X}")
    if not eof:
        raise PreflightError("missing EOF record")
    segments = []
    index = 0
    while index < APP_BYTES:
        if not present[index]:
            index += 1
            continue
        begin = index
        while index < APP_BYTES and present[index]:
            index += 1
        segments.append(Segment(APP_BASE + begin, bytes(values[begin:index])))
    if not segments:
        raise PreflightError("HEX contains no application data")
    return Image("hex", hashlib.sha256(source).hexdigest(), tuple(segments), count, start_linear)


def _read_covered(image: Image, address: int, length: int) -> bytes:
    for segment in image.segments:
        if segment.address <= address and address + length <= segment.address + len(segment.data):
            index = address - segment.address
            return segment.data[index:index + length]
    raise PreflightError(f"required bytes at 0x{address:08X} are absent")


def inspect(image: Image) -> dict:
    # Also validate manually constructed Images, so callers cannot bypass the
    # address/overlap invariants by skipping the file parser.
    previous_end = APP_BASE
    covered = 0
    missing = []
    for segment in image.segments:
        end = segment.address + len(segment.data)
        if not segment.data or segment.address < previous_end or end > APP_END:
            raise PreflightError("invalid, unordered, overlapping, or out-of-window segment")
        if segment.address > previous_end:
            missing.append((previous_end, segment.address))
        previous_end = end
        covered += len(segment.data)
    if not image.segments:
        raise PreflightError("no image segments")
    if previous_end < APP_END:
        missing.append((previous_end, APP_END))

    initial_sp, reset_vector = struct.unpack("<II", _read_covered(image, APP_BASE, 8))
    if not RAM_BASE < initial_sp <= RAM_END or initial_sp % 8:
        raise PreflightError("initial SP is outside the 48 KiB RAM profile or not 8-byte aligned")
    if not reset_vector & 1:
        raise PreflightError("reset vector does not have the Thumb bit set")
    reset_address = reset_vector & ~1
    if not APP_BASE + 8 <= reset_address < APP_END:
        raise PreflightError("reset vector is outside the related application-code window")
    _read_covered(image, reset_address, 2)
    if image.start_linear_address is not None:
        start_address = image.start_linear_address & ~1
        if not APP_BASE <= start_address < APP_END:
            raise PreflightError("HEX start-linear address is outside the application window")
        _read_covered(image, start_address, 2)

    span = previous_end - APP_BASE
    return {
        "kind": "application_image_structural_preflight",
        "schema_version": 1,
        "source_format": image.source_format,
        "source_sha256": image.source_sha256,
        "checks_passed": True,
        "hardware_qualified": False,
        "recovery_verified": False,
        "safe_to_flash_assessment": "not assessed; structural evidence only",
        "profile": "related RT880 application at 0x08002800; 256 KiB flash / 48 KiB RAM",
        "bin_load_base_is_assumed": image.source_format == "bin",
        "application_start": f"0x{APP_BASE:08X}",
        "application_end_exclusive": f"0x{APP_END:08X}",
        "application_window_bytes": APP_BYTES,
        "covered_bytes": covered,
        "address_span_bytes": span,
        "source_covers_full_application_window": covered == APP_BYTES,
        "span_is_1024_byte_aligned": span % BLOCK_BYTES == 0,
        "blocks_covering_span": (span + BLOCK_BYTES - 1) // BLOCK_BYTES,
        "initial_sp": f"0x{initial_sp:08X}",
        "reset_vector": f"0x{reset_vector:08X}",
        "start_linear_address": None if image.start_linear_address is None else f"0x{image.start_linear_address:08X}",
        "start_linear_equals_reset_vector": None if image.start_linear_address is None else image.start_linear_address == reset_vector,
        "hex_record_count": image.record_count,
        "segments": [{"address": f"0x{s.address:08X}", "bytes": len(s.data),
                      "sha256": hashlib.sha256(s.data).hexdigest()} for s in image.segments],
        "missing_intervals": [{"start": f"0x{a:08X}", "end_exclusive": f"0x{b:08X}", "bytes": b - a} for a, b in missing],
        "missing_bytes": APP_BYTES - covered,
        "padding_applied": False,
        "image_output_created": False,
        "limitations": [
            "No exact UV98 hardware or bootloader identification is inferred from shape/hash.",
            "No erase extent, write mapping, final ACK, or recovery path is verified.",
            "No image is truncated, repaired, relocated, filled, or written by this tool.",
            "Intel HEX start-linear metadata is not evidence of actual loader handoff.",
            "Passing vector checks does not validate code, peripherals, clocks, or RF behavior.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("--format", choices=("auto", "bin", "hex"), default="auto")
    args = parser.parse_args()
    try:
        if not args.image.is_file():
            raise PreflightError("input must be an existing regular local file")
        source_format = args.format
        if source_format == "auto":
            source_format = args.image.suffix.lower().lstrip(".")
            if source_format not in ("bin", "hex"):
                raise PreflightError("auto format requires a .bin or .hex filename")
        maximum = APP_BYTES if source_format == "bin" else MAX_HEX_BYTES
        # Bounded read also handles a file growing after its metadata check.
        with args.image.open("rb") as stream:
            source = stream.read(maximum + 1)
        image = parse_bin(source) if source_format == "bin" else parse_hex(source)
        print(json.dumps(inspect(image), indent=2))
        return 0
    except (OSError, PreflightError) as error:
        print(json.dumps({"kind": "application_image_structural_preflight",
                          "checks_passed": False, "error": str(error),
                          "hardware_qualified": False, "recovery_verified": False,
                          "safe_to_flash_assessment": "not assessed; structural evidence only"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
