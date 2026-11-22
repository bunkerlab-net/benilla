#!/usr/bin/env python3
"""Check exact pinned SpellEvent and EventProcessor decisions with an authored host.
SPDX-License-Identifier: GPL-3.0-or-later
The host is not the source Spell::update, Unit phases, AI visitor, or spell math.
No database, install, runtime producer, network or Rust implementation is used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "spell_event_source"
MANIFEST_SHA256 = "402c206882614cf849a671a0583e3f3dfad6dfca0ae595adb16e2134e43004c1"


def commands():
    # Names describe the bounded scenario, not validation of the complete host.
    yield from """P creation_tick_skip
N 1 100 -1 1 0 50
U 50 50 1
U 49 99 1
U 51 150 1
U 0 150 1
U 1 151 1
P full_unit_diff
N 1 400 -1 0 0 0
U 250 250 1
U 149 399 1
U 100 499 1
P travel_from_callback_clock
N 1 100 200 1 0 0
U 150 150 1
U 199 349 1
U 50 399 1
U 0 399 1
U 1 400 1
P instant_travel_first_callback
N 1 0 200 0 0 0
U 80 80 1
U 200 280 1
U 1 281 1
P delayed_continuation_absolute_offset
N 1 0 10 0 0 0
U 1 1 1
G 1 30
U 15 16 1
U 14 30 1
U 1 31 1
U 1 32 1
P zero_delay_codue_ai_first
I 1 1
N 2 0 0 0 0 0
I 3 1
U 10 10 1
U 1 11 1
P zero_delay_codue_spell_first
N 1 0 0 0 0 0
I 2 1
U 10 10 1
U 1 11 1
P current_reference_wait
N 1 1 -1 1 0 0
U 1 1 0
U 1 2 0
R 1 0 1
U 1 3 0
R 1 0 0
U 1 4 0
P direct_cancel_retains_event
N 1 500 -1 1 0 0
C 1
U 100 100 1
U 1 101 1
P queued_abort_calls_cancel
N 1 500 -1 0 0 0
B 1
U 10 10 1
P failed_precheck_keeps_event
N 1 500 -1 1 1 0
U 0 0 1
U 10 10 1
P nonforce_retains_nondeletable
N 1 500 -1 1 0 0
X 0 0
X 0 0
R 1 0 0
X 0 0
P force_after_current_reference_release
N 1 500 -1 1 0 0
R 1 0 0
X 1 0
P force_nondeletable_leaks_spell
N 1 500 -1 1 0 0
X 1 0
P world_stopped_force_deletes_spell
N 1 500 -1 1 0 0
X 1 1
P kill_event_destructor_cancels
N 1 500 -1 0 0 0
K 1
P abort_due_nondeletable_leaks_spell
N 1 500 -1 1 0 0
B 1
U 1 1 0
P delayed_next_offset_already_due
N 1 0 5 0 0 0
U 1 1 1
G 1 7
I 2 5
U 20 21 1
U 1 22 1
P channeled_travel_to_immediate
N 1 0 10 0 0 0
Y 1 0
U 1 1 1
U 10 11 1
U 1 12 1
C 1
U 1 13 1
P channeled_travel_other_cast_cancels
N 1 0 10 0 0 0
Y 1 1
U 1 1 1
U 10 11 1
U 1 12 1
P creation_clock_wrap_equality
N 1 10 -1 0 0 4294967295
U 4294967295 4294967295 1
U 1 0 1
U 9 9 1
""".splitlines()


def checked_sources(upstream):
    raw = (SOURCE / "source_manifest.json").read_bytes()
    if hashlib.sha256(raw).hexdigest() != MANIFEST_SHA256:
        raise SystemExit("pinned source manifest hash mismatch")
    manifest = json.loads(raw)
    for name, item in manifest["files"].items():
        actual = (SOURCE / name).read_bytes()
        if hashlib.sha256(actual).hexdigest() != item["file_sha256"]:
            raise SystemExit("pinned source hash mismatch: " + name)
        if upstream:
            original = (upstream / item["source_path"]).read_bytes()
            if hashlib.sha256(original).hexdigest() != item["source_file_sha256"]:
                raise SystemExit("upstream source hash mismatch: " + item["source_path"])
            if "start_line" in item:
                lines = original.splitlines(keepends=True)
                body = b"".join(lines[item["start_line"] - 1:item["end_line"]])
                expected = b"".join(lines[:item["license_header_lines"]]) + b"\n" + body
                if hashlib.sha256(body).hexdigest() != item["body_sha256"] or expected != actual:
                    raise SystemExit("verbatim source extraction mismatch: " + name)
            elif original != actual:
                raise SystemExit("verbatim source file mismatch: " + name)
    for source, digest in manifest["reviewed_sources"].items():
        if upstream and hashlib.sha256((upstream / source).read_bytes()).hexdigest() != digest:
            raise SystemExit("reviewed upstream source hash mismatch: " + source)
        if source.startswith("src/framework/Utilities/"):
            local = ROOT / "event_processor_source" / Path(source).name
            if hashlib.sha256(local.read_bytes()).hexdigest() != digest:
                raise SystemExit("pinned queue source hash mismatch: " + source)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--upstream", type=Path, help="optional pinned repository root")
    parser.add_argument("--sanitize", action="store_true", help="also run address/undefined sanitizers")
    parser.add_argument("--trace-output", type=Path)
    args = parser.parse_args()
    checked_sources(args.upstream)
    compiler = next((shutil.which(c) for c in ("c++", "g++", "clang++") if shutil.which(c)), None)
    if compiler is None:
        raise SystemExit("C++ compiler required; source check never skips")
    inputs = list(commands())
    with tempfile.TemporaryDirectory() as directory:
        binary = Path(directory) / "spell-event-oracle"
        flags = ["-O1", "-g", "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer"] if args.sanitize else ["-O2"]
        subprocess.run([compiler, "-std=c++17", *flags, "-Wall", "-Wextra", "-Werror", "-I", str(ROOT / "event_processor_source"), str(SOURCE / "oracle.cpp"), str(ROOT / "event_processor_source/EventProcessor.cpp"), "-o", str(binary)], check=True)
        result = subprocess.run([str(binary)], input="\n".join(inputs) + "\n", text=True, capture_output=True)
        if result.returncode:
            raise SystemExit(f"source oracle failed ({result.returncode}):\n{result.stderr}")
        output = result.stdout
    header = "# Exact pinned SpellEvent + EventProcessor; authored bounded Spell/Unit/AiNotify host\n# Schema: tools/spell_event_source/README.md; source_manifest.json contains provenance\n"
    output = header + output
    path = ROOT.parent / "tests/fixtures/spell_event_cmangos.tsv"
    if args.check:
        if path.read_text() != output:
            raise SystemExit("pinned C++ fixture mismatch: " + str(path))
    else:
        path.write_text(output)
    if args.trace_output:
        args.trace_output.write_text(output)
    decisions = sum(line.startswith("O ") for line in output.splitlines())
    scenarios = sum(line.startswith("P ") for line in inputs)
    print(f"Verified {scenarios} authored-host scenarios, {decisions} exact SpellEvent decisions, {len(output.splitlines())} trace rows")


if __name__ == "__main__":
    main()
