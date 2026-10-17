#!/usr/bin/env python3
"""Compile unchanged pinned swimming primitives against authored observations.
SPDX-License-Identifier: GPL-3.0-or-later
No database, install, network, geometry reader or Rust execution.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "creature_swimming_source"
FIXTURE = ROOT.parent / "tests/fixtures/creature_swimming_cmangos.test-fixture"
MANIFEST_SHA256 = "cef9a79098194912a466235bb553e743eca2eb393b4699216d51944b3063ac5e"


def bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def scalar_cases():
    """Exact threshold and rounding probes; values here select inputs only."""
    cases = [(0, 0, 0), (0x80000000, 0, 0), (0, 0x80000000, 0x80000000)]
    for level, ground, radius in [(2.0, 0.0, 2.0), (12.0, 10.0, 2.0),
                                  (2.03128, 0.0, 2.03128)]:
        cases.extend((bits(level) + offset, bits(ground), bits(radius))
                     for offset in (-1, 0, 1))
    cases.extend((bits(level), bits(ground), bits(radius)) for level, ground, radius in [
        (2.0, 0.0, 0.5), (2.0, 0.0, 4.0), (0.25, 0.0, -1.0),
        (-1.0, -4.0, 3.0), (1_000_000.0, 999_999.0, 1.0),
    ])
    # Normal inputs can produce an exact subnormal difference; do not revalidate
    # the intermediate as SpellFloat. The second case rounds the subtraction.
    cases.extend([(0x00800001, 0x00800000, 0),
                  (0x3f800001, 0xb3800000, 0x3f800001)])
    return cases


def commands():
    for vmap, grid in [(0, 0), (0, 1), (1, 0), (1, 1)]:
        for status in range(16):
            for level, depth, radius in scalar_cases():
                for external in (0, 1):
                    yield f"S {vmap} {grid} {status} {level} {depth} {radius} {external}"
    prior_words = (0, 0x200000, 0x100, 0x200100, 0xffffffff, 0xffdfffff)
    for habitat in range(1, 8):
        for extra in (0, 0x8000, 2, 0xffffffff):
            for prior in prior_words:
                for status in (0, 1, 2, 4, 8):
                    for vmap, grid in [(0, 0), (0, 1), (1, 0)]:
                        # Unit flag seed differs deliberately from movement flags.
                        unit = prior ^ 0x8000
                        yield (f"I {habitat} {extra} {prior} {unit} {vmap} {grid} {status} "
                               f"{bits(3.0)} 0 {bits(2.0)} 0")
    # Independent movement bits and complements prove preservation beyond modes.
    for bit in range(32):
        for prior in (1 << bit, 0xffffffff ^ (1 << bit)):
            for status in (0, 8):
                yield (f"I 3 0 {prior} 0 1 1 {status} {bits(3.0)} 0 {bits(2.0)} 0")


def checked_upstream_file(upstream, path, expected_digest):
    original = (upstream / path).read_bytes().replace(b"\r\n", b"\n")
    if hashlib.sha256(original).hexdigest() != expected_digest:
        raise SystemExit("upstream source hash mismatch: " + path)
    return original


def checked_sources(upstream=None, retained=SOURCE):
    raw = (retained / "source_manifest.json").read_bytes()
    if hashlib.sha256(raw).hexdigest() != MANIFEST_SHA256:
        raise SystemExit("pinned source manifest hash mismatch")
    manifest = json.loads(raw)
    for name, item in manifest["files"].items():
        actual = (retained / name).read_bytes()
        if hashlib.sha256(actual).hexdigest() != item["file_sha256"]:
            raise SystemExit("pinned source hash mismatch: " + name)
        if upstream is not None:
            original = checked_upstream_file(upstream, item["source_path"], item["source_file_sha256"])
            lines = original.splitlines(keepends=True)
            body = b"".join(lines[item["start_line"] - 1:item["end_line"]])
            header = item["license_header_lines"]
            expected = b"".join(lines[:header]) + b"\n" + body if header else body
            if hashlib.sha256(body).hexdigest() != item["body_sha256"] or expected != actual:
                raise SystemExit("verbatim source extraction mismatch: " + name)
    if upstream is not None:
        for path, digest in manifest["reviewed_runtime_sources"].items():
            checked_upstream_file(upstream, path, digest)
        order = manifest["reviewed_source_order"]
        text = checked_upstream_file(upstream, order["source_path"], order["source_file_sha256"]).decode()
        cursor = text.index("bool Creature::InitEntry(")
        for marker in order["ordered_markers"]:
            cursor = text.index(marker, cursor) + len(marker)


def record_fixture(target, expected, check):
    if check:
        # Git's CRLF checkout conversion changes no numeric oracle content.
        if target.read_bytes().replace(b"\r\n", b"\n") != expected:
            raise SystemExit("pinned C++ fixture mismatch: " + str(target))
    else:
        target.write_bytes(expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--upstream", type=Path)
    parser.add_argument("--sanitize", action="store_true")
    args = parser.parse_args()
    checked_sources(args.upstream)
    compiler = next((shutil.which(name) for name in ("c++", "g++", "clang++")
                     if shutil.which(name)), None)
    if compiler is None:
        raise SystemExit("C++ compiler required; exact-source oracle never skips")
    inputs = list(commands())
    if len(inputs) != len(set(inputs)):
        raise SystemExit("duplicate authored source input")
    with tempfile.TemporaryDirectory() as directory:
        binary = Path(directory) / "creature-swimming-oracle"
        flags = ["-O1", "-g", "-fsanitize=undefined", "-fno-sanitize-recover=all"] if args.sanitize else ["-O2"]
        subprocess.run([compiler, "-std=c++17", *flags, "-fno-fast-math", "-ffp-contract=off",
                        "-Wall", "-Wextra", "-Werror", str(SOURCE / "oracle.cpp"), "-o", str(binary)], check=True)
        result = subprocess.run([str(binary)], input="\n".join(inputs) + "\n",
                                text=True, capture_output=True, check=True)
    if len(result.stdout.splitlines()) != len(inputs):
        raise SystemExit("source oracle did not produce one result per input")
    header = ("# Exact pinned CMaNGOS swimming source; authored numeric/status inputs only\n"
              "# S vmap grid status level_bits depth_bits radius_bits external_data\n"
              "# I habitat extra prior_movement base_unit vmap grid status level_bits depth_bits radius_bits external_data\n"
              "# output: result projected_unit availability_calls grid_calls liquid_calls mask classification_bits external_level_bits external_depth_bits\n")
    record_fixture(FIXTURE, (header + result.stdout).encode(), args.check)
    print(f"Verified {sum(row.startswith('S ') for row in inputs)} exact scalar and "
          f"{sum(row.startswith('I ') for row in inputs)} initialization-block cases")
    if args.upstream is not None:
        print("Verified verbatim retained bodies, default argument, and reviewed model/gender-before-level source order")
    print("No complete InitEntry, lifecycle, geometry, database or gameplay admission was executed")


if __name__ == "__main__":
    main()
