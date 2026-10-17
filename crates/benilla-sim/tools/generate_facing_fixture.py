#!/usr/bin/env python3
"""Compile unchanged pinned CMaNGOS bodies and glibc atan2f; write/check bit oracles.

Usage: generate_facing_fixture.py MANGOS_ROOT GLIBC_ATAN2F [--check]
No game data, network, native runtime/producer, or host atan2 is used.
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
        path = glibc if name.endswith("e_atan2f.c") else mangos / name
        data = path.read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong pinned source: " + str(path))
        result[Path(name).name] = data.decode()
    return result


def build(source, directory):
    # The source remains unchanged. Its private compatibility header supplies
    # bit casts and branch hints; recording a true hint does not alter arithmetic.
    rare_line = next(i for i, line in enumerate(source["e_atan2f.c"].splitlines(), 1)
                     if "asuint64 (r) + 8" in line)
    (directory / "math_config.h").write_text(r'''
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
extern int oracle_rare;
static inline int oracle_unlikely(int value, int line) {
    if (value && line == RARE_LINE) oracle_rare = 1;
    return value;
}
#undef __glibc_unlikely
#undef __glibc_likely
#define __glibc_unlikely(x) oracle_unlikely((x), __LINE__)
#define __glibc_likely(x) (x)
static inline uint32_t asuint(float x) { uint32_t u; memcpy(&u, &x, 4); return u; }
static inline uint64_t asuint64(double x) { uint64_t u; memcpy(&u, &x, 8); return u; }
static inline double asdouble(uint64_t u) { double x; memcpy(&x, &u, 8); return x; }
'''.replace("RARE_LINE", str(rare_line)))
    (directory / "libm-alias-finite.h").write_text("#define libm_alias_finite(a,b)\n")
    (directory / "e_atan2f.c").write_text(source["e_atan2f.c"])
    common = source["Common.h"]
    pi_definition = next(line for line in common.splitlines() if "define M_PI_F " in line)
    map_body = extract(source["MapManager.h"], "static float NormalizeOrientation(float o)")
    obj = source["Object.cpp"]
    functions = [extract(obj, signature) for signature in (
        "float WorldObject::GetAngleAt(float x, float y, float ox, float oy)",
        "float WorldObject::GetAngleAt(float x, float y, const WorldObject* obj) const",
        "bool WorldObject::HasInArcAt(float x, float y, float o, const WorldObject* target, float arc")]
    harness = r'''
#include <cmath>
#include <math.h>
#include <cfloat>
#include <type_traits>
#include <cfenv>
#include <cfloat>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <limits>
extern "C" float __ieee754_atan2f(float y, float x);
extern "C" { int oracle_rare = 0; }
static float from_bits(uint32_t u) { float f; std::memcpy(&f, &u, 4); return f; }
static uint32_t bits(float f) { uint32_t u; std::memcpy(&u, &f, 4); return u; }
// This substitution only binds std::atan2(float,float) to the pinned source.
// fmod is the source's ordinary float overload, used solely in this oracle.
namespace oracle_math {
static float atan2(float y, float x) { return __ieee754_atan2f(y, x); }
static float fmod(float x, float y) { return std::fmod(x, y); }
}
struct Log { void outError(const char*, const char*) { std::abort(); } } sLog;
PI_DEFINITION
#define std oracle_math
struct MapManager { MAP_BODY };
#undef std
struct WorldObject {
    float x, y;
    float GetPositionX() const { return x; }
    float GetPositionY() const { return y; }
    std::string GetGuidStr() const { return "oracle"; }
    static float GetAngleAt(float, float, float, float);
    float GetAngleAt(float, float, const WorldObject*) const;
    bool HasInArcAt(float, float, float, const WorldObject*, float) const;
};
#define std oracle_math
FUNCTIONS
#undef std
static uint32_t next(uint32_t& s) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; }
int main(int argc, char**) {
    static_assert(sizeof(float) == 4 && sizeof(double) == 8);
    static_assert(FLT_EVAL_METHOD == 0, "source profile forbids excess precision");
    static_assert(std::is_same_v<decltype(std::atan2(1.f, 1.f)), float>);
    static_assert(std::is_same_v<decltype(atan2(1.f, 1.f)), float>);
    static_assert(FLT_EVAL_METHOD == 0);
    static_assert(std::numeric_limits<float>::is_iec559);
    static_assert(std::numeric_limits<double>::is_iec559);
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    if (bits(M_PI_F) != 0x40490fdb || bits(2.0f * M_PI_F) != 0x40c90fdb) return 2;
    if (argc > 1) {
        uint32_t state = 0xa17a2f41;
        unsigned found = 0;
        for (unsigned n = 0; n < 100000000 && found < 8; ++n) {
            uint32_t y = 0x3f000000 + (next(state) & 0xffffff);
            uint32_t x = 0x3f000000 + (next(state) & 0xffffff);
            oracle_rare = 0;
            uint32_t r = bits(__ieee754_atan2f(from_bits(y), from_bits(x)));
            if (oracle_rare) { std::printf("%08x %08x %08x\n", y, x, r); ++found; }
        }
        return found ? 0 : 3;
    }
    char kind[16];
    while (std::scanf("%15s", kind) == 1) {
        uint32_t a[7] = {};
        unsigned count = std::strcmp(kind,"atan2") == 0 ? 2 :
            std::strcmp(kind,"normalize") == 0 ? 1 :
            std::strcmp(kind,"bearing") == 0 ? 4 : 7;
        for (unsigned i = 0; i < count; ++i)
            if (std::scanf("%x", &a[i]) != 1) return 4;
        oracle_rare = 0;
        uint32_t answer;
        if (std::strcmp(kind,"atan2") == 0)
            answer = bits(__ieee754_atan2f(from_bits(a[0]), from_bits(a[1])));
        else if (std::strcmp(kind,"normalize") == 0)
            answer = bits(MapManager::NormalizeOrientation(from_bits(a[0])));
        else if (std::strcmp(kind,"bearing") == 0)
            answer = bits(WorldObject::GetAngleAt(from_bits(a[0]), from_bits(a[1]),
                                                 from_bits(a[2]), from_bits(a[3])));
        else {
            WorldObject origin{from_bits(a[0]), from_bits(a[1])};
            WorldObject target{from_bits(a[3]), from_bits(a[4])};
            answer = origin.HasInArcAt(origin.x, origin.y, from_bits(a[2]),
                                      a[6] ? &origin : &target, from_bits(a[5]));
        }
        std::printf("%08x %d\n", answer, oracle_rare);
    }
}
'''
    harness = harness.replace("PI_DEFINITION", pi_definition).replace("MAP_BODY", map_body)
    harness = harness.replace("FUNCTIONS", "\n".join(functions))
    (directory / "oracle.cpp").write_text(harness)
    flags = ["-O2", "-Wall", "-Wextra", "-Werror", "-fno-fast-math",
             "-ffp-contract=off", "-frounding-math", "-fno-builtin-atan2", "-fno-builtin-atan2f",
             "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all"]
    subprocess.run(["gcc", "-std=c11", *flags, "-I", str(directory), "-c",
                    str(directory / "e_atan2f.c"), "-o", str(directory / "atan2.o")], check=True)
    binary = directory / "oracle"
    subprocess.run(["g++", "-std=c++17", *flags, str(directory / "oracle.cpp"),
                    str(directory / "atan2.o"), "-lm", "-o", str(binary)], check=True)
    # Dynamic symbol proof: arithmetic support fma/fmod is fine; host atan2 is not.
    symbols = subprocess.check_output(["nm", "-u", str(binary)], text=True)
    if any("atan2" in line for line in symbols.splitlines()):
        raise SystemExit("Oracle unexpectedly depends on host atan2")
    return binary


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
    # Inputs discovered by the optional deterministic native rare-branch search.
    # Regeneration requires each of these to take the source's rare path.
    rare = [(0x3F7D14F8, 0x3FA6C629), (0x3F6E9547, 0x3F0BAAA4),
            (0x3FB0D97E, 0x3F790791), (0x3F050B7B, 0x3F27960E),
            (0x3F8EF599, 0x3F8FF3E7), (0x3F85D09B, 0x3F4754D0),
            (0x3F35EDED, 0x3F8BB809), (0x3F1CE8E3, 0x3F1D2203)]
    for i, (y, x) in enumerate(rare):
        yield f"rare_fallback_{i}", "atan2", [y, x]
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                for sy in (0, SIGN):
                    for sx in (0, SIGN):
                        yield f"fallback_neighbor_{i}_{dy}_{dx}_{sy >> 31}_{sx >> 31}", "atan2", [(y + dy) | sy, (x + dx) | sx]


def run(binary, requests):
    data = "".join(kind + " " + " ".join(f"{value:08x}" for value in values) + "\n"
                   for kind, values in requests)
    output = subprocess.check_output([str(binary)], input=data, text=True).splitlines()
    if len(output) != len(requests):
        raise SystemExit("Oracle result count differs")
    return [(int(row.split()[0], 16), int(row.split()[1])) for row in output]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mangos_root", type=Path)
    parser.add_argument("glibc_atan2f", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--find-rare", action="store_true")
    args = parser.parse_args()
    source = checked_sources(args.mangos_root, args.glibc_atan2f)
    with tempfile.TemporaryDirectory(prefix="benilla-facing-oracle-") as temp:
        binary = build(source, Path(temp))
        if args.find_rare:
            print(subprocess.check_output([str(binary), "find-rare"], text=True), end="")
            return
        entries = list(cases())
        results = run(binary, [(kind, values) for _, kind, values in entries])
        rows = ["# name operation raw_binary32_inputs expected_bits rare_atan2_branch\n"]
        for (name, kind, values), (answer, rare) in zip(entries, results):
            if name.startswith("rare_fallback_") and not rare:
                raise SystemExit("Rare fallback case no longer enters rare path")
            rows.append(" ".join([name, kind, *(f"{v:08x}" for v in values), f"{answer:08x}", str(rare)]) + "\n")
        digest_rows = ["# operation count xorshift32_seed sha256(inputs_and_answer_as_le_u32)\n"]
        for kind, count in SAMPLES:
            inputs = list(samples(kind, count))
            outputs = run(binary, [(kind, values) for values in inputs])
            digest = hashlib.sha256()
            for values, (answer, _) in zip(inputs, outputs):
                digest.update(struct.pack("<" + "I" * (len(values) + 1), *values, answer))
            digest_rows.append(f"{kind} {count} {SEED:08x} {digest.hexdigest()}\n")
    for name, content in (("oracle.tsv", "".join(rows)), ("sample-digests.tsv", "".join(digest_rows))):
        path = FIXTURES / name
        if args.check:
            if path.read_text() != content:
                raise SystemExit("Facing oracle fixture differs: " + name)
        else:
            path.write_text(content)
    print(f"Verified {len(entries)} facing source cases and {sum(count for _, count in SAMPLES)} deterministic samples")


if __name__ == "__main__":
    main()
