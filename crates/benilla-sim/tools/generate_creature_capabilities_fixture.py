#!/usr/bin/env python3
"""Compile unchanged pinned Creature predicates and the separate loader repair.
SPDX-License-Identifier: GPL-3.0-or-later
Only authored numeric inputs: no database, install, network or Rust execution.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "creature_capabilities_source"
FIXTURE = ROOT.parent / "tests/fixtures/creature_capabilities_cmangos.test-fixture"
MANIFEST_SHA256 = "fba1be9e0c8a4f62e59e2a6e19c0231b8ece43307ac908ffcec72994b433bad0"


def flag_words():
    """Independent bits, their complements, and all six relevant-bit subsets."""
    values = {0, 0xffffffff}
    for bit in range(32):
        values.add(1 << bit)
        values.add(0xffffffff ^ (1 << bit))
    bits = (0x100, 0x400, 0x800, 0x200000, 0x800000, 0x40000000)
    for subset in range(1 << len(bits)):
        values.add(sum(bit for index, bit in enumerate(bits) if subset & (1 << index)))
    # Complete unrelated-bit mask plus each flight-bit subset proves OR semantics
    # independent of every other bit, including unknown high-order flags.
    flight_bits = (0x400, 0x800000, 0x40000000)
    unrelated = 0xffffffff ^ sum(flight_bits)
    for subset in range(1 << len(flight_bits)):
        values.add(unrelated | sum(bit for index, bit in enumerate(flight_bits)
                                   if subset & (1 << index)))
    return sorted(values)


def commands():
    for habitat in range(1, 8):
        for flags in flag_words():
            yield f"P {habitat} {flags}"
    for raw in (*range(256), 256, 65535, 65536, 0x7fffffff, 0x80000000, 0xffffffff):
        yield f"R {raw} 0"


def checked_upstream_file(upstream, path, expected_digest):
    original = (upstream / path).read_bytes().replace(b"\r\n", b"\n")
    if hashlib.sha256(original).hexdigest() != expected_digest:
        raise SystemExit("upstream source hash mismatch: " + path)
    return original


def checked_sources(upstream=None, retained=SOURCE):
    manifest_bytes = (retained / "source_manifest.json").read_bytes()
    if hashlib.sha256(manifest_bytes).hexdigest() != MANIFEST_SHA256:
        raise SystemExit("pinned source manifest hash mismatch")
    manifest = json.loads(manifest_bytes)
    for name, item in manifest["files"].items():
        actual = (retained / name).read_bytes()
        if hashlib.sha256(actual).hexdigest() != item["file_sha256"]:
            raise SystemExit("pinned source hash mismatch: " + name)
        if upstream is None:
            continue
        original = checked_upstream_file(upstream, item["source_path"], item["source_file_sha256"])
        lines = original.splitlines(keepends=True)
        body = b"".join(lines[item["start_line"] - 1:item["end_line"]])
        expected = b"".join(lines[:item["license_header_lines"]]) + b"\n" + body
        if hashlib.sha256(body).hexdigest() != item["body_sha256"] or expected != actual:
            raise SystemExit("verbatim source extraction mismatch: " + name)
    # These whole-file guards pin the runtime ownership review only. The scalar
    # oracle does not extract or execute constructor, respawn or flag lifecycle.
    if upstream is not None:
        for path, digest in manifest["reviewed_runtime_sources"].items():
            checked_upstream_file(upstream, path, digest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--upstream", type=Path, help="optional pinned CMaNGOS checkout")
    parser.add_argument("--sanitize", action="store_true")
    args = parser.parse_args()
    checked_sources(args.upstream)
    if args.upstream is not None:
        print("Verified 2 additional reviewed runtime source hashes; lifecycle is not executed by this oracle")
    compiler = next((shutil.which(name) for name in ("c++", "g++", "clang++")
                     if shutil.which(name)), None)
    if compiler is None:
        raise SystemExit("C++ compiler required; exact-source oracle never skips")
    inputs = list(commands())
    with tempfile.TemporaryDirectory() as directory:
        binary = Path(directory) / "creature-capabilities-oracle"
        flags = ["-O1", "-g", "-fsanitize=undefined", "-fno-sanitize-recover=all"] if args.sanitize else ["-O2"]
        subprocess.run([compiler, "-std=c++17", *flags, "-Wall", "-Wextra", "-Werror",
                        str(SOURCE / "oracle.cpp"), "-o", str(binary)], check=True)
        result = subprocess.run([str(binary)], input="\n".join(inputs) + "\n",
                                text=True, capture_output=True, check=True)
    if len(result.stdout.splitlines()) != len(inputs):
        raise SystemExit("source oracle did not produce one result for every input")
    header = ("# Exact pinned CMaNGOS creature predicates; authored numeric inputs only\n"
              "# kind raw_habitat current_flags loaded_habitat can_walk can_swim can_fly repair_logs\n"
              "# P: validated predicate; R: separate ObjectMgr load-time repair\n")
    output = (header + result.stdout).encode()
    if args.check:
        if FIXTURE.read_bytes() != output:
            raise SystemExit("pinned C++ fixture mismatch: " + str(FIXTURE))
    else:
        FIXTURE.write_bytes(output)
    predicates = sum(line.startswith("P ") for line in inputs)
    print(f"Verified {predicates} exact predicate cases and {len(inputs) - predicates} separate loader cases")


if __name__ == "__main__":
    main()
