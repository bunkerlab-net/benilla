#!/usr/bin/env python3
"""Generate source facing vectors inside the pinned F0 glibc 2.36 builder.

Usage: generate_facing_fixture.py MANGOS_ROOT GLIBC_FLOAT_SOURCE_DIR --evidence-dir MAIN_CONFIG_PATH
No game data, network or host-toolchain arithmetic is used.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import hashlib
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT / "fixtures/facing"
SEED = 0xA17A2F41
SAMPLES = (("atan2", 131072), ("normalize", 8192), ("bearing", 8192), ("arc", 8192))
SIGN = 0x80000000
PI = 0x40490FDB
TAU = 0x40C90FDB
LIMIT = 0x41C90FDB
COORD = 0x46FFF000
IMAGE = "sha256:5397aff8205e0d96686d9a74bd0c9542830d170c167c8d242a3c13e9f269e9a8"
PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"


def extract(source, signature):
    """Keep the function signature and body byte-for-byte (no expression rewrites)."""
    start = source.index(signature)
    opening = source.index("{", start)
    level = 0
    for end in range(opening, len(source)):
        level += (source[end] == "{") - (source[end] == "}")
        if level == 0:
            return source[start:end + 1]
    raise SystemExit("Unclosed source function: " + signature)


def checked_sources(mangos, glibc):
    result = {}
    for line in (ROOT / "facing_source_hashes.sha256").read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        digest, name = line.split()
        path = glibc / Path(name).name if name.startswith("sysdeps/") else mangos / name
        data = path.read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong pinned source: " + str(path))
        result[Path(name).name] = data.decode()
    return result


def build(source, directory):
    common = source["Common.h"]
    pi_definition = next(line for line in common.splitlines() if "define M_PI_F " in line)
    map_body = extract(source["MapManager.h"], "static float NormalizeOrientation(float o)")
    functions = [extract(source["Object.cpp"], signature) for signature in (
        "float WorldObject::GetAngleAt(float x, float y, float ox, float oy)",
        "float WorldObject::GetAngleAt(float x, float y, const WorldObject* obj) const",
        "bool WorldObject::HasInArcAt(float x, float y, float o, const WorldObject* target, float arc")]
    harness = (ROOT / "facing_oracle.cpp").read_text().replace("PI_DEFINITION", pi_definition)
    harness = harness.replace("MAP_BODY", map_body).replace("FUNCTIONS", "\n".join(functions))
    (directory / "oracle.cpp").write_text(harness)
    flags = ["g++", "-std=c++17", "-O3", "-DNDEBUG", "-ffp-contract=off", "-fno-fast-math",
             "-fno-omit-frame-pointer", "-g", "/evidence/oracle.cpp", "-o", "/evidence/oracle"]
    import os
    command = ["docker", "run", "--rm", "-i", "--network=none", "--user", f"{os.getuid()}:{os.getgid()}",
               "--mount", f"type=bind,src={directory.resolve()},dst=/evidence", IMAGE]
    subprocess.run(command + flags, check=True)
    receipt = {"image": IMAGE, "command": flags, "source_sha256": {
        name: hashlib.sha256(raw.encode()).hexdigest() for name, raw in source.items()},
        "harness_sha256": hashlib.sha256(harness.encode()).hexdigest(),
        "scope": "Authored source component vectors, not sealed F1 acceptance"}
    return command + ["/evidence/oracle"], receipt


def random_words():
    state = SEED
    while True:
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        yield state


def bounded(word, maximum):
    return (word & SIGN) | ((word & 0x7FFFFFFF) % (maximum + 1))


def samples(kind, count):
    words = random_words()
    for _ in range(count):
        if kind == "atan2":
            yield [bounded(next(words), 0x7F7FFFFF) for _ in range(2)]
        elif kind == "normalize":
            yield [bounded(next(words), LIMIT)]
        elif kind == "bearing":
            yield [bounded(next(words), COORD) for _ in range(4)]
        else:
            yield [bounded(next(words), bound) for bound in (COORD, COORD, LIMIT, COORD, COORD, LIMIT)] + [next(words) & 1]


def cases():
    # Every expected result is supplied by the compiled pinned sources below.
    values = [0, 1, 0x007FFFFF, 0x00800000, 0x3F000000, 0x3F800000,
              PI, TAU, COORD, 0x7F7FFFFF]
    signed = values + [value | SIGN for value in values]
    for i, y in enumerate(signed):
        for j, x in enumerate(signed):
            yield f"atan2_matrix_{i}_{j}", "atan2", [y, x]
    for e in range(1, 255, 7):
        for offset in (-1, 0, 1):
            y = max(1, (e << 23) + offset)
            for sign in (0, SIGN):
                yield f"ratio_exponent_{e}_{offset}_{sign >> 31}", "atan2", [y | sign, 0x3F800000]
    for distance in (25, 26, 27):
        for side in (-1, 1):
            for offset in (-1, 0, 1):
                y = 0x3F800000 + side * (distance << 23) + offset
                for sy in (0, SIGN):
                    for sx in (0, SIGN):
                        yield f"ratio_branch_{distance}_{side}_{offset}_{sy >> 31}_{sx >> 31}", "atan2", [y | sy, 0x3F800000 | sx]
    for value in (0, 1, 0x007FFFFF, 0x00800000, 0x3F800000, PI, TAU, TAU + 0x800000, LIMIT):
        for offset in (-1, 0, 1):
            v = value + offset
            if 0 <= v <= LIMIT:
                for sign in (0, SIGN):
                    yield f"normalize_{v:08x}_{sign >> 31}", "normalize", [v | sign]
    points = [(0, 0), (SIGN, SIGN), (0x3F800000, 0), (0, 0x3F800000),
              (0xBF800000, 0), (0, 0xBF800000), (0x3F800000, 0x3F800000),
              (0xBF800000, 0x3F800000), (0xBF800000, 0xBF800000),
              (0x3F800000, 0xBF800000), (COORD, COORD | SIGN),
              (COORD | SIGN, COORD), (1, 1 | SIGN), (0x3F9E0652, 0xC187E6B7)]
    for i, origin in enumerate(points):
        for j, target in enumerate(points):
            yield f"bearing_{i}_{j}", "bearing", [*origin, *target]
    # Inclusive PI borders and adjacent rays, exact full turns, signed-zero,
    # noncanonical orientations, coincident positions and identity early-return.
    rays = [(0x3F800000, 0), (0, 0x3F800000), (1, 0x3F800000),
            (SIGN | 1, 0x3F800000), (0xBF800000, 0), (0, 0xBF800000),
            (1, 0xBF800000), (SIGN | 1, 0xBF800000), (0, 0), (SIGN, SIGN)]
    rays += [(x | sign, y) for x in (0x33800000, 0x34000000, 0x34800000, 0x35000000)
             for sign in (0, SIGN) for y in (0x3F800000, 0xBF800000)]
    angles = [0, SIGN, PI, PI | SIGN, TAU - 1, TAU, TAU | SIGN, LIMIT, LIMIT | SIGN]
    for i, ray in enumerate(rays):
        for j, arc in enumerate(angles):
            for k, orientation in enumerate(angles):
                yield f"arc_{i}_{j}_{k}", "arc", [0, 0, orientation, *ray, arc, 0]
    for i, arc in enumerate(angles):
        yield f"arc_identity_{i}", "arc", [COORD, COORD | SIGN, LIMIT | SIGN, 0, 0, arc, 1]
    # Retain the eight former CORE-MATH hard inputs and their neighbours.
    # Their classification is input provenance, not a glibc 2.36 branch counter.
    rare = [(0x3F7D14F8, 0x3FA6C629), (0x3F6E9547, 0x3F0BAAA4),
            (0x3FB0D97E, 0x3F790791), (0x3F050B7B, 0x3F27960E),
            (0x3F8EF599, 0x3F8FF3E7), (0x3F85D09B, 0x3F4754D0),
            (0x3F35EDED, 0x3F8BB809), (0x3F1CE8E3, 0x3F1D2203)]
    for i, (y, x) in enumerate(rare):
        yield f"hard_input_{i}", "atan2", [y, x]
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                for sy in (0, SIGN):
                    for sx in (0, SIGN):
                        yield f"fallback_neighbor_{i}_{dy}_{dx}_{sy >> 31}_{sx >> 31}", "atan2", [(y + dy) | sy, (x + dx) | sx]


def run(binary, requests):
    data = "".join(kind + " " + " ".join(f"{value:08x}" for value in values) + "\n"
                   for kind, values in requests)
    output = subprocess.check_output(binary, input=data, text=True).splitlines()
    if len(output) != len(requests):
        raise SystemExit("Oracle result count differs")
    return [int(row, 16) for row in output]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mangos_root", type=Path)
    parser.add_argument("glibc_float_source_dir", type=Path)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = checked_sources(args.mangos_root, args.glibc_float_source_dir)
    args.evidence_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="facing236-", dir=args.evidence_dir) as temp:
        binary, receipt = build(source, Path(temp))
        entries = list(cases())
        results = run(binary, [(kind, values) for _, kind, values in entries])
        rows = ["# name operation raw_binary32_inputs expected_bits legacy_hard_input\n"]
        for (name, kind, values), answer in zip(entries, results):
            rows.append(" ".join([name, kind, *(f"{v:08x}" for v in values), f"{answer:08x}",
                                  str(int(name.startswith("hard_input_")))]) + "\n")
        digest_rows = ["# operation count xorshift32_seed sha256(inputs_and_answer_as_le_u32)\n"]
        for kind, count in SAMPLES:
            inputs = list(samples(kind, count))
            outputs = run(binary, [(kind, values) for values in inputs])
            digest = hashlib.sha256()
            for values, answer in zip(inputs, outputs):
                digest.update(struct.pack("<" + "I" * (len(values) + 1), *values, answer))
            digest_rows.append(f"{kind} {count} {SEED:08x} {digest.hexdigest()}\n")
    products = {"oracle.tsv": "".join(rows), "sample-digests.tsv": "".join(digest_rows)}
    receipt["outputs"] = {name: hashlib.sha256(content.encode()).hexdigest() for name, content in products.items()}
    products["provenance.json"] = __import__("json").dumps(receipt, indent=2) + "\n"
    for name, content in products.items():
        path = FIXTURES / name
        if args.check:
            if path.read_text() != content:
                raise SystemExit("Facing oracle fixture differs: " + name)
        else:
            path.write_text(content)
    print(f"Verified {len(entries)} facing source cases and {sum(count for _, count in SAMPLES)} deterministic samples")


if __name__ == "__main__":
    main()
