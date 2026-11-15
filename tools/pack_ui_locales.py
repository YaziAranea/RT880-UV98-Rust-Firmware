#!/usr/bin/env python3
"""Build the no-heap UI message catalog. No build-time dependency on Python.

Message IDs are the stable, zero-based row numbers of messages.tsv. Append new
messages; do not reorder existing rows. Rust uses ``\x1fNNN`` (three hex digits)
inside ordinary format strings. Numbers, protocol abbreviations, and saved
channel/zone names are not translated.
"""
from collections import Counter
import argparse
import csv
import re
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / "crates/handheld-ui/locales"


def audit_sources(count):
    """Catch untranslated literals and invalid IDs in production screen code."""
    lexer = re.compile(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"', re.S)
    for filename in ["controller.rs", "render.rs", "utility.rs"]:
        source = (DIRECTORY.parent / "src" / filename).read_text(encoding="utf-8")
        masked = lexer.sub(lambda match: " " * len(match.group()), source)
        test_spans = []
        start = 0
        while (marker := source.find("#[cfg(test)]\nmod tests", start)) >= 0:
            opening = source.index("{", marker)
            depth = 1
            for position in range(opening + 1, len(source)):
                depth += (masked[position] == "{") - (masked[position] == "}")
                if depth == 0:
                    test_spans.append((marker, position + 1))
                    start = position + 1
                    break
            else:
                raise AssertionError(f"unclosed tests module in {filename}")
        for match in lexer.finditer(source):
            if not match.group().startswith('"') or any(a <= match.start() < b for a, b in test_spans):
                continue
            literal = match.group()
            assert not re.search("[А-Яа-яЁё]", literal), f"untranslated UI literal in {filename}: {literal}"
            for message_id in re.findall(r"\\x1f([0-9A-Fa-f]{3})", literal):
                assert int(message_id, 16) < count, f"invalid message ID {message_id} in {filename}"


def build():
    with (DIRECTORY / "messages.tsv").open(encoding="utf-8", newline="") as handle:
        rows = list(csv.reader(handle, delimiter="\t"))
    assert rows.pop(0) == ["ru", "en", "de"]
    assert all(len(row) == 3 and all(row) for row in rows)
    assert len({row[0] for row in rows}) == len(rows), "duplicate Russian key"
    assert len(rows) < 4096
    messages = [message for row in rows for message in row]
    alphabet = sorted(set("".join(messages)))
    assert len(alphabet) < 256 and all(ord(c) <= 0xffff for c in alphabet)
    lookup = {char: i for i, char in enumerate(alphabet)}
    encoded = [bytes(lookup[c] for c in message) for message in messages]
    pairs = []
    depths = [0] * len(alphabet)
    while len(alphabet) + len(pairs) < 256:
        counts = Counter(pair for message in encoded for pair in zip(message, message[1:]))
        if not counts:
            break
        pair, count = min(counts.items(), key=lambda item: (-item[1], item[0]))
        if count <= 2:
            break
        token = len(alphabet) + len(pairs)
        encoded = [message.replace(bytes(pair), bytes([token])) for message in encoded]
        pairs.append(pair)
        depths.append(1 + max(depths[pair[0]], depths[pair[1]]))
    assert max(depths) < 24, "decoder's fixed stack would overflow"
    offsets = [0]
    for message in encoded:
        offsets.append(offsets[-1] + len(message))
    assert offsets[-1] <= 65535
    result = struct.pack("<HBB", len(rows), len(alphabet), len(pairs))
    result += b"".join(struct.pack("<H", ord(char)) for char in alphabet)
    result += bytes(value for pair in pairs for value in pair)
    result += b"".join(struct.pack("<H", offset) for offset in offsets)
    result += b"".join(encoded)
    return result, rows, max(depths)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    packed, rows, depth = build()
    audit_sources(len(rows))
    destination = DIRECTORY / "messages.bin"
    if args.check:
        assert destination.read_bytes() == packed, "messages.bin needs regeneration"
    else:
        destination.write_bytes(packed)
    print(f"{len(rows)} messages / 3 languages: {len(packed)} bytes; decode depth {depth}")


if __name__ == "__main__":
    main()
