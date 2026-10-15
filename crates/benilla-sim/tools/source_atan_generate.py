#!/usr/bin/env python3
"""Compile pinned binary64 atan and original empty selector, entirely offline.

Usage: source_atan_generate.py MANGOS_ROOT GLIBC_ROOT [--check] [--sanitize]
Expected values never use Python/Rust arithmetic or host atan/atanf/atan2.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures/source_atan"
MANIFEST = HERE / "source_atan_source_hashes.sha256"
GLIBC_PIN = "74f59e9271cbb4071671e5a474e7d4f1622b186f"
MANGOS_PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
SELECTOR_SEED = 0xA7A64105
SELECTOR_SAMPLES = 4096
ATAN_STRIDE = 4093


def guarded_sources(mangos, glibc):
    sources = {}
    for line in MANIFEST.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        expected, qualified = line.split()
        family, name = qualified.split("/", 1)
        root = glibc if family == "glibc" else mangos
        raw = (root / name).read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit("Wrong pinned source: " + qualified)
        sources[qualified] = raw
    return sources


def coefficient_digest(sources):
    table = sources["glibc/sysdeps/ieee754/dbl-64/uatan.tbl"].decode()
    big, little = table.split("#else\n#ifdef LITTLE_ENDI")
    def words(section, swap):
        pairs = re.findall(r"\{\{(0X[0-9A-F]{8}), (0X[0-9A-F]{8})\}", section)
        if len(pairs) != 241 * 7:
            raise SystemExit("Original atan table shape drift")
        return [(int(b, 16) << 32) | int(a, 16) if swap else
                (int(a, 16) << 32) | int(b, 16) for a, b in pairs]
    big_words, little_words = words(big, False), words(little, True)
    if big_words != little_words:
        raise SystemExit("Original atan endian tables disagree")
    return hashlib.sha256(struct.pack("<1687Q", *big_words)).hexdigest()


def exact_method(source, signature):
    if source.count(signature) != 1:
        raise SystemExit("Expected one original method: " + signature)
    begin = source.index(signature)
    depth = 0
    for end in range(source.index("{", begin), len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            return source[begin:end + 1]
    raise SystemExit("Unclosed original method: " + signature)


def original_line(source, token):
    lines = [line for line in source.splitlines() if token in line]
    if len(lines) != 1:
        raise SystemExit("Expected one source constant: " + token)
    return lines[0]


def harness(sources):
    selector = sources["cmangos/src/game/Maps/ObjectPosSelector.cpp"].decode()
    constants = [original_line(selector, "#define OCCUPY_POS_ANGLE_ATAN_FACTOR"),
                 original_line(sources["cmangos/src/game/Entities/ObjectDefines.h"].decode(),
                               "static constexpr float const DEFAULT_WORLD_OBJECT_SIZE"),
                 original_line(sources["cmangos/src/shared/Common.h"].decode(), "define M_PI_F ")]
    signatures = ("ObjectPosSelector::ObjectPosSelector(",
                  "void ObjectPosSelector::InitializeAngle()",
                  "void ObjectPosSelector::InitializeAngle(UsedAreaSide side)",
                  "bool ObjectPosSelector::NextAngle(",
                  "bool ObjectPosSelector::NextSideAngle(",
                  "bool ObjectPosSelector::NextUsedAngle(")
    methods = "\n\n".join(exact_method(selector, signature) for signature in signatures)
    result = (HERE / "source_atan_oracle.cpp").read_text()
    for marker, replacement in (("// PINNED_CONSTANTS", "\n".join(constants)),
                                ("// PINNED_SELECTOR_METHODS", methods)):
        if result.count(marker) != 1:
            raise SystemExit("Harness marker drift: " + marker)
        result = result.replace(marker, replacement)
    return selector[:selector.index("*/") + 2] + "\n" + result


def compile_oracle(sources, work, sanitize=False, receipts=None):
    version = subprocess.check_output(["gcc", "-dumpfullversion"], text=True).strip()
    cpp_version = subprocess.check_output(["g++", "-dumpfullversion"], text=True).strip()
    target = subprocess.check_output(["gcc", "-dumpmachine"], text=True).strip()
    if version != "14.2.0" or cpp_version != version or target != "x86_64-linux-gnu":
        raise SystemExit("Requires audited GCC/G++ 14.2.0 x86_64-linux-gnu source profile")
    for key, data in sources.items():
        if key.startswith("glibc/") and Path(key).name in (
                "s_atan.c", "dla.h", "mydefs.h", "uatan.tbl", "atnat.h",
                "math-underflow.h", "math-barriers.h"):
            (work / Path(key).name).write_bytes(data)
    (work / "ObjectPosSelector.h").write_bytes(
        sources["cmangos/src/game/Maps/ObjectPosSelector.h"])
    (work / "Common.h").write_text("#pragma once\n// No declarations added to the original selector.\n")
    # Only byte order, an unused alias and round-mode context need adapters.
    # All numeric macros, tables and the underflow-force body remain original.
    (work / "endian.h").write_text('''
#if !defined(__x86_64__) || __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error Source profile requires little-endian x86_64
#endif
#define LITTLE_ENDI 1
#define HIGH_HALF 1
#define LOW_HALF 0
''')
    (work / "libm-alias-double.h").write_text("// Alias suppressed by -D__atan=atan.\n")
    (work / "fenv_private.h").write_text('''
#include <stdlib.h>
#define SET_RESTORE_ROUND(mode) do { if (fegetround() != (mode)) abort(); } while (0)
''')
    (work / "profile.h").write_text('''
#include <float.h>
#include <math.h>
#if defined(__FP_FAST_FMA) || defined(__FMA__) || defined(DLA_FMS)
#error This source profile requires the original unfused EMULV branch
#endif
_Static_assert(FLT_EVAL_METHOD == 0 && sizeof(double) == 8 && sizeof(int) == 4, "IEEE profile");
''')
    text = harness(sources)
    (work / "oracle.cpp").write_text(text)
    flags = ["-O2", "-march=x86-64", "-msse2", "-mfpmath=sse", "-mno-fma",
             "-fno-fast-math", "-ffp-contract=off", "-frounding-math", "-fno-builtin",
             "-fno-strict-aliasing", "-Wall", "-Wextra", "-Werror"]
    if sanitize:
        flags += ["-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all"]
    commands = [
        ["gcc", "-std=gnu11", *flags, "-fexcess-precision=standard", "-D__atan=atan",
         "-D__STDC_WANT_IEC_60559_TYPES_EXT__=1",
         "-include", str(work / "profile.h"), "-I", str(work), "-c",
         str(work / "s_atan.c"), "-o", str(work / "atan.o")],
        ["g++", "-std=c++20", *flags, "-I", str(work), str(work / "oracle.cpp"),
         str(work / "atan.o"), "-lm", "-o", str(work / "oracle")],
    ]
    for command in commands:
        subprocess.run(command, check=True)
    binary = work / "oracle"
    undefined = subprocess.check_output(["nm", "-u", str(binary)], text=True)
    symbols = subprocess.check_output(["nm", "-C", str(binary)], text=True)
    assembly = subprocess.check_output(["objdump", "-d", "-C", str(binary)], text=True)
    if re.search(r"\batan(?:f|2|2f)?(?:@|\s|$)", undefined):
        raise SystemExit("Unexpected host atan dependency")
    if not re.search(r" T atan$", symbols, re.M):
        raise SystemExit("Missing locally compiled original atan")
    if re.search(r"\b(?:v?fmadd|v?fmsub|v?fnmadd|v?fnmsub)", assembly):
        raise SystemExit("Unexpected fused arithmetic")
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        for path in work.iterdir():
            if path.suffix in (".h", ".c", ".cpp", ".tbl"):
                shutil.copy2(path, receipts / path.name)
        (receipts / "symbols.txt").write_text(symbols)
        (receipts / "undefined-symbols.txt").write_text(undefined)
        (receipts / "oracle.disassembly.txt").write_text(assembly)
        (receipts / "profile.json").write_text(json.dumps({
            "gcc": version, "g++": cpp_version, "target": target, "commands": commands,
            "glibc_revision": GLIBC_PIN, "cmangos_revision": MANGOS_PIN,
            "rounding": "FE_TONEAREST; FTZ=DAZ=0; FLT_EVAL_METHOD=0",
            "arithmetic": "unfused SSE2 baseline; original non-FMA EMULV",
            "sanitize": sanitize,
        }, indent=2) + "\n")
    return binary


def f32_bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def atan_cases():
    for name, word in (("positive_zero", 0), ("negative_zero", 0x80000000),
                       ("min_subnormal", 1), ("max_subnormal", 0x007fffff),
                       ("min_normal", 0x00800000), ("max_finite", 0x7f7fffff),
                       ("positive_infinity", 0x7f800000)):
        yield name, word
    for name, word in zip("ABCDE", (0x325db3d0, 0x3d800000, 0x3f800000, 0x41800000, 0x59a4ff90)):
        for delta in (-1, 0, 1):
            yield f"boundary_{name}_{delta}", word + delta
    for exponent in range(1, 255):
        for delta in (-1, 0, 1):
            yield f"exponent_{exponent}_{delta}", (exponent << 23) + delta
    for index in range(16, 256):
        # These operations generate inputs only. Outputs come from original C.
        for kind, value in (("direct", (index + 0.5) / 256),
                            ("reciprocal", 256 / (index + 0.5))):
            word = f32_bits(value)
            for delta in (-1, 0, 1):
                yield f"table_{kind}_{index}_{delta}", word + delta


def selector_cases():
    for delta in (-1, 0, 1):
        yield f"exact_pi_{delta}", 0x42142315 + delta, 0x3f800000
        yield f"exact_pi_runtime_bounded_{delta}", 0x40142315 + delta, 0x3d800000
    yield "mixed_double_increment", 0x3f800000, 0x3f800000
    values = [0, 0x80000000, 1, 0x007fffff, 0x00800000, 0x3ec72b02,
              0x3f000000, 0x3f800000, 0x40000000, 0x46fff000, 0x7f7fffff]
    for i, size in enumerate(values):
        for j, distance in enumerate(values):
            yield f"matrix_{i}_{j}", size, distance
    # Near-zero half angles, source default repairs, overflowing numerator,
    # and many ULP neighborhoods all retain complete terminal sequences.
    for size in (1, 0x00800000, 0x325db3d0, 0x3d800000, 0x3ec72b02,
                 0x3f000000, 0x3f800000, 0x40800000, 0x7f0e38e4):
        for delta in (-1, 0, 1):
            yield f"size_neighbor_{size:08x}_{delta}", size + delta, 0x3f800000


def atan_samples():
    yield from range(0, 0x7f800001, ATAN_STRIDE)
    yield 0x7f800000
    yield 0x80000000


def selector_samples():
    state = SELECTOR_SEED
    def word():
        nonlocal state
        state ^= (state << 13) & 0xffffffff
        state ^= state >> 17
        state ^= (state << 5) & 0xffffffff
        return state % 0x7f800000
    for _ in range(SELECTOR_SAMPLES):
        yield word(), word()


def observe(binary, operation, inputs):
    rows = list(inputs)
    payload = "".join(operation + " " + " ".join(f"{w:08x}" for w in row) + "\n" for row in rows)
    result = subprocess.run([str(binary)], input=payload, text=True, capture_output=True, check=True)
    outputs = result.stdout.splitlines()
    if len(outputs) != len(rows):
        raise SystemExit("Oracle output count mismatch")
    for row, output in zip(rows, outputs):
        tokens = output.split()
        if operation == "atan":
            if len(tokens) != 2:
                raise SystemExit("Invalid atan result")
            yield row, [int(token, 16) for token in tokens]
        else:
            count = int(tokens[4])
            if len(tokens) != 5 + count or count > 628:
                raise SystemExit("Invalid selector sequence")
            yield row, [int(token, 16) for token in tokens[:4]] + [count] + [
                int(token, 16) for token in tokens[5:]]


def fixture_contents(binary):
    cases = list(atan_cases())
    lines = ["# name input_binary32 atan_binary64 narrowed_binary32"]
    for (name, _), (inputs, output) in zip(cases, observe(binary, "atan", ([word] for _, word in cases))):
        lines.append(f"{name} {inputs[0]:08x} {output[0]:016x} {output[1]:08x}")
    result = {"atan.tsv": "\n".join(lines) + "\n"}
    cases = list(selector_cases())
    lines = ["# name size dist ratio half final_plus final_minus count angles..."]
    for (name, _, _), (inputs, output) in zip(cases, observe(binary, "selector", ((a, b) for _, a, b in cases))):
        words = " ".join(f"{word:08x}" for word in (*inputs, *output[:4]))
        angles = " ".join(f"{word:08x}" for word in output[5:])
        lines.append(f"{name} {words} {output[4]} {angles}".rstrip())
    result["selector.tsv"] = "\n".join(lines) + "\n"
    lines = ["# operation count generator_parameter sha256_little_endian_words"]
    digest = hashlib.sha256()
    count = 0
    for inputs, output in observe(binary, "atan", ([word] for word in atan_samples())):
        digest.update(struct.pack("<IQI", inputs[0], *output))
        count += 1
    lines.append(f"atan {count} {ATAN_STRIDE} {digest.hexdigest()}")
    digest = hashlib.sha256()
    for inputs, output in observe(binary, "selector", selector_samples()):
        digest.update(struct.pack("<" + "I" * (len(inputs) + len(output)), *inputs, *output))
    lines.append(f"selector {SELECTOR_SAMPLES} {SELECTOR_SEED:08x} {digest.hexdigest()}")
    result["sample-digests.tsv"] = "\n".join(lines) + "\n"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mangos", type=Path)
    parser.add_argument("glibc", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--receipt-dir", type=Path)
    args = parser.parse_args()
    sources = guarded_sources(args.mangos, args.glibc)
    with tempfile.TemporaryDirectory(prefix="benilla-atan64-oracle-") as temporary:
        binary = compile_oracle(sources, Path(temporary), args.sanitize, args.receipt_dir)
        contents = fixture_contents(binary)
    contents["coefficients.sha256"] = coefficient_digest(sources) + "  cij_u64_le_241x7\n"
    FIXTURES.mkdir(parents=True, exist_ok=True)
    for name, content in contents.items():
        path = FIXTURES / name
        if args.check:
            if path.read_text() != content:
                raise SystemExit("Fixture differs: " + str(path))
        else:
            path.write_text(content)
        print(("checked " if args.check else "wrote ") + str(path))


if __name__ == "__main__":
    main()
