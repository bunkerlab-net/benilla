#!/usr/bin/env python3
"""Execute unchanged pinned hostile-reference functions with authored linked Units.
SPDX-License-Identifier: GPL-3.0-or-later
No database, install, network, Rust execution, or reconstructed threat transitions.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

SOURCE = Path(__file__).resolve().parent
FIXTURE = SOURCE.parent.parent / "tests/fixtures/hostile_reference_cmangos.test-fixture"
MANIFEST_SHA256 = "c01ab470444c7271ea75d93cd0686297fa8e92095db0e310ca64ac0d016128b4"


def float_bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def commands():
    # Constructor, add-zero, repeated addition, independent list membership,
    # sorting only on request, negative deltas and clamp-to-zero.
    yield "N"
    for guid, amount in [(1, 0), (1, 0), (1, 10)]:
        yield f"A {guid} {float_bits(amount)}"
    yield "S"
    yield f"A 1 {float_bits(5)}"
    yield "S"
    yield f"A 1 {float_bits(-5)}"
    yield "S"
    yield f"A 1 {float_bits(-100)}"
    yield "S"
    yield "C"
    # Successful feign is not any other state bit. State changes alone don't
    # refresh references; reverse-linked updateThreatTables executes refresh.
    yield "N"
    yield f"A 7 {float_bits(12)}"
    yield "S"
    for state in [0, 1, 8, 0xffff_fffb, 4]:
        yield f"F 7 {state}"
        yield "T 7"
    yield f"A 7 {float_bits(3)}"
    yield "A 7 0"
    yield "G 256"
    yield "T 7"
    yield "T 7"
    yield "G 0"
    yield "U 7"
    yield "F 7 0"
    yield "T 7"
    yield "S"
    # Accessibility is independent of online/valid/current and not terrain.
    yield "X 7 0"
    yield "U 7"
    yield "X 7 1"
    yield "U 7"
    yield "R 7"
    # Creation while already feigning constructs/link first, then moves offline.
    yield "N"
    yield "F 3 4"
    yield "A 3 0"
    yield f"A 3 {float_bits(5)}"
    yield "G 256"
    yield "U 3"
    yield "S"
    yield "C"
    # Invalidate retains identity/source/threat, drops reverse link and current.
    # Missing resolution stays offline; restored lookup rebinds the same GUID.
    yield "N"
    yield f"A 9 {float_bits(17)}"
    yield "S"
    yield "V 9"
    yield "U 9"
    yield "U 9"
    yield f"A 9 {float_bits(2)}"
    yield "L 9"
    yield "F 9 4"
    yield "U 9"
    yield "G 256"
    yield "U 9"
    yield "S"
    yield "V 9"
    yield "R 9"
    yield "C"
    # Explicit target deletion, owner clear and fresh references after cleanup.
    # These dispatch into DeleteThreatList; full death/respawn is not executed.
    yield "N"
    yield f"A 1 {float_bits(11)}"
    yield "S"
    yield "D 1"
    yield "A 1 0"
    yield "S"
    yield "F 1 4"
    yield "T 1"
    yield "D 1000"
    yield "F 1 0"
    yield "A 1 0"
    yield "S"
    yield "C"
    yield "C"
    # The offline container clears too; current stays unset on add.
    yield "N"
    yield "F 1 4"
    yield "A 1 0"
    yield "C"
    # Raw source predicate bits. These are not claims about real aura success.
    yield "N"
    words = {0, 0xffff_ffff}
    for bit in range(32):
        words.add(1 << bit)
        words.add(0xffff_ffff ^ (1 << bit))
    for state in sorted(words):
        for flags in [0, 0x100, 0xffff_feff, 0xffff_ffff]:
            yield f"P 1 {state} {flags}"
    for flags in sorted(words):
        for state in [0, 4, 0xffff_fffb, 0xffff_ffff]:
            yield f"P 1 {state} {flags}"


def checked_upstream_file(upstream, path, digest):
    original = (upstream / path).read_bytes().replace(b"\r\n", b"\n")
    if hashlib.sha256(original).hexdigest() != digest:
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
        if upstream is None:
            continue
        original = checked_upstream_file(upstream, item["source_path"], item["source_file_sha256"])
        if item.get("whole_file"):
            expected = original
        else:
            lines = original.splitlines(keepends=True)
            body = b"".join(lines[item["start_line"] - 1:item["end_line"]])
            if hashlib.sha256(body).hexdigest() != item["body_sha256"]:
                raise SystemExit("source excerpt hash mismatch: " + name)
            expected = b"".join(lines[:item["license_header_lines"]]) + b"\n" + body
        if expected != actual:
            raise SystemExit("verbatim source extraction mismatch: " + name)


def run_oracle(sanitize=False):
    checked_sources()
    compiler = next((shutil.which(name) for name in ("c++", "g++", "clang++") if shutil.which(name)), None)
    if compiler is None:
        raise SystemExit("C++ compiler required; exact-source oracle never skips")
    inputs = list(commands())
    with tempfile.TemporaryDirectory() as directory:
        binary = Path(directory) / "hostile-reference-oracle"
        flags = ["-O1", "-g", "-fsanitize=undefined", "-fno-sanitize-recover=all"] if sanitize else ["-O2"]
        # The retained upstream mixed &&/|| expression has known precedence.
        # Do not rewrite source to satisfy an optional parenthesis warning.
        subprocess.run([compiler, "-std=c++17", *flags, "-Wall", "-Wextra", "-Werror", "-Wno-parentheses",
                        "-I", str(SOURCE), str(SOURCE / "oracle.cpp"), "-o", str(binary)], check=True)
        result = subprocess.run([str(binary)], input="\n".join(inputs) + "\n", text=True, capture_output=True, check=True)
    rows = result.stdout.splitlines()
    if len(rows) != len(inputs) or any(row.split(" | ")[0] != command for row, command in zip(rows, inputs)):
        raise SystemExit("source oracle did not record every input exactly once")
    header = ("# Exact pinned CMaNGOS hostile-reference functions; authored linked-unit host\n"
              "# command | offline_predicate current_guid dirty online_count offline_count aggro_callbacks [guid:threat_bits:valid:online:accessible:reverse_links ...] | dispatched_events\n"
              "# D dispatches exact DeleteThreatList; no full damage/death/respawn or terrain parity\n")
    return (header + result.stdout).encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--upstream", type=Path)
    parser.add_argument("--sanitize", action="store_true")
    args = parser.parse_args()
    checked_sources(args.upstream)
    output = run_oracle(args.sanitize)
    if args.check:
        if FIXTURE.read_bytes() != output:
            raise SystemExit("pinned C++ fixture mismatch: " + str(FIXTURE))
    else:
        FIXTURE.write_bytes(output)
    print(f"Verified {len(list(commands()))} exact-source hostile-reference operation/predicate observations")


if __name__ == "__main__":
    main()
