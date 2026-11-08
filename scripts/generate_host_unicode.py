"""Reproduce compact scalar tables from each supported CPython's own behavior.

Run with native uv, once per supported interpreter. --check never writes files.
Every lowercase entry is checked against the host regex engine's scalar oracle;
classification and decimal values come from public str/unicodedata operations.
"""

import _sre
import argparse
import hashlib
import json
import struct
import sys
import unicodedata
from pathlib import Path

DIRECTORY = Path(__file__).resolve().parents[1] / "crates/python/src/host_unicode_data"
VERSIONS = {11: "14.0.0", 12: "15.0.0", 13: "15.1.0", 14: "16.0.0", 15: "17.0.0"}


def generate():
    # Bounded: the fixed Unicode domain, never graph-derived data.
    blocks, indexes, lowers = [], [], []
    known = {}
    for start in range(0, 0x110000, 256):
        block = bytearray()
        for scalar in range(start, start + 256):
            char = chr(scalar)
            digit = unicodedata.decimal(char, -1)
            block.append(
                (digit + 1)
                | (16 if char.isalnum() or char == "_" else 0)
                | (32 if char.isspace() else 0)
            )
            lower = ord(char.lower()[0])
            if lower != _sre.unicode_tolower(scalar):
                raise ValueError(f"Host simple lowercase differs at U+{scalar:04X}")
            if lower != scalar:
                lowers.append((scalar, lower))
        data = bytes(block)
        if data not in known:
            known[data] = len(blocks)
            blocks.append(data)
        indexes.append(known[data])
    flags = struct.pack("<4352H", *indexes) + b"".join(blocks)
    lower = b"".join(struct.pack("<II", *pair) for pair in lowers)
    return {"flags": flags, "lower": lower}, len(blocks)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    arguments = parser.parse_args()
    minor = sys.version_info.minor
    if (
        sys.implementation.name != "cpython"
        or sys.version_info.major != 3
        or minor not in VERSIONS
        or unicodedata.unidata_version != VERSIONS[minor]
    ):
        raise ValueError("Unsupported host Python/Unicode database; audit it before generating")
    generated, blocks = generate()
    metadata = DIRECTORY / "provenance.json"
    records = json.loads(metadata.read_text())
    row = next(row for row in records if int(row["python"].split(".")[1]) == minor)
    for kind, data in generated.items():
        path = DIRECTORY / f"{minor}-{kind}.bin"
        if arguments.check:
            if path.read_bytes() != data:
                raise ValueError(f"Generated {kind} table differs from the host interpreter")
            if hashlib.sha256(data).hexdigest() != row[kind + "_sha256"]:
                raise ValueError(f"Generated {kind} provenance differs")
        else:
            path.write_bytes(data)
            row[kind + "_sha256"] = hashlib.sha256(data).hexdigest()
            row[kind + "_bytes"] = len(data)
    if arguments.write:
        row.update(
            python=sys.version.split()[0],
            unicode=unicodedata.unidata_version,
            blocks=blocks,
            verified_scalars=0x110000,
        )
        metadata.write_text(json.dumps(records, indent=2) + "\n")
    print(f"Verified all 1,114,112 codepoints for CPython 3.{minor}, Unicode {VERSIONS[minor]}")


if __name__ == "__main__":
    main()
