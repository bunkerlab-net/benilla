#!/usr/bin/env python3
"""Compile exact pinned EventProcessor.cpp and generate/check synthetic traces.
SPDX-License-Identifier: GPL-3.0-or-later
No database, install, runtime producer or network access.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
HASHES = {
    "EventProcessor.cpp": "164cda6f21f72d60e6c426ad04d39036565eda6e60e5e10fce390c43eb396e2f",
    "EventProcessor.h": "8e85449a46540a7f999f1ac3a3132c2d2b3256b9c29b41a7e2c29ae78cf812c9",
}

def manual_commands():
    # Equal keys, coarse/zero updates, modification reinsertion, callback-created
    # due events, same-event re-add, retained detached events, abort, KillEvent,
    # repeated nondeletable Abort, force cleanup, and insertion after KillAll.
    yield from """P
N 1 5 0 0 1
N 2 5 0 0 1
N 3 5 0 0 1
M 1 5
U 4
U 0
U 1
P
N 1 0 1 3 1
N 2 0 0 0 1
U 10
P
N 1 20 0 0 1
N 2 1 3 1 1
N 3 1 4 1 1
N 4 1 5 5 1
U 5
P
N 1 1 6 2 1
U 10
U 0
U 1
U 100
P
N 1 1 2 0 1
U 8
K 1
M 1 100
R 1 8 0
U 0
R 1 9 1
T 1 0 0
U 1
P
N 1 1 0 0 0
N 2 1 0 0 1
B 2 1
X 0
X 0
N 3 0 0 0 1
U 1
P
N 1 1 0 0 0
X 1
N 2 0 0 0 1
U 0
P
N 1 1 0 0 1
B 1 1
K 1
U 2
P
N 1 1 0 0 0
B 1 1
U 1
P
N 1 0 0 0 1
B 1 1
B 1 0
U 0
P
N 1 0 3 2 1
N 2 20 0 0 1
B 1 1
U 5
P
N 1 0 4 2 1
N 2 0 0 0 1
B 1 1
U 0
P
N 1 0 5 2 1
B 1 1
U 0
P
N 1 0 9 2 1
N 2 0 0 0 1
B 1 1
U 0
P
N 1 10 10 0 1
X 0
X 0
P
N 1 0 7 0 1
N 2 0 8 0 1
N 3 0 9 4 1
N 4 0 0 0 1
U 0
P
U 4294967295
N 1 4294967296 0 0 1
U 1
""".splitlines()


def generated_commands(seed):
    # A specified integer generator, only for reproducible authored test commands.
    yield "P"
    state = seed + 1
    def draw():
        nonlocal state
        state = (1664525 * state + 1013904223) & 0xffffffff
        return state
    for event in range(1, 33):
        yield f"N {event} {draw() % 33} 0 0 {draw() % 2}"
    for _ in range(96):
        action = draw() % 5
        event = 1 + draw() % 32
        if action == 0:
            yield f"M {event} {draw() % 40}"
        elif action == 1:
            yield f"B {event} {draw() % 2}"
        elif action == 2:
            yield f"K {event}"
        elif action == 3:
            yield f"U {draw() % 3}"
        else:
            yield "X 0"
    yield "U 100"
    yield "X 1"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--upstream", type=Path, help="optional pinned src/framework/Utilities directory")
    parser.add_argument("--trace-output", type=Path, help="write complete readable traces for diagnosis")
    args = parser.parse_args()
    for name, digest in HASHES.items():
        if hashlib.sha256((ROOT / "event_processor_source" / name).read_bytes()).hexdigest() != digest:
            raise SystemExit("pinned source hash mismatch: " + name)
        if args.upstream:
            if hashlib.sha256((args.upstream / name).read_bytes()).hexdigest() != digest:
                raise SystemExit("upstream source hash mismatch: " + name)
    compiler = next((shutil.which(c) for c in ("c++", "g++", "clang++") if shutil.which(c)), None)
    if compiler is None:
        raise SystemExit("C++ compiler required; this check never skips")
    with tempfile.TemporaryDirectory() as directory:
        binary = Path(directory) / "event-oracle"
        subprocess.run([compiler, "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror", "-I", str(ROOT / "event_processor_source"), str(ROOT / "event_processor_oracle.cpp"), str(ROOT / "event_processor_source/EventProcessor.cpp"), "-o", str(binary)], check=True)
        def run(inputs):
            return subprocess.run([str(binary)], input="\n".join(inputs) + "\n", text=True, capture_output=True, check=True).stdout
        inputs = list(manual_commands())
        output = run(inputs)
        full_trace = output
        rows, count = len(output.splitlines()), len(inputs)
        digest_rows = []
        for seed in range(24):
            commands = list(generated_commands(seed))
            generated = run(commands)
            full_trace += generated
            generated_rows = len(generated.splitlines())
            digest_rows.append(f"{seed} {len(commands)} {generated_rows} {hashlib.sha256(generated.encode()).hexdigest()}")
            rows += generated_rows
            count += len(commands)
    files = {
        "event_processor_cmangos.tsv": output,
        "event_processor_cmangos_digests.tsv": "\n".join(digest_rows) + "\n",
    }
    for name, contents in files.items():
        path = ROOT.parent / "tests/fixtures" / name
        if args.check:
            if path.read_text() != contents:
                raise SystemExit("pinned C++ fixture mismatch: " + name)
        else:
            path.write_text(contents)
    if args.trace_output:
        args.trace_output.write_text(full_trace)
    print(f"Verified {count} commands and {rows} trace rows against exact pinned EventProcessor.cpp")

if __name__ == "__main__":
    main()
