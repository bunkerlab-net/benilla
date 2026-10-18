#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Byte oracle from selected pinned CMaNGOS source and entirely authored inputs.

Compiles only original DBC/ADT readers and ConvertADT, with local fixture bytes
substituting for MPQ transport. Never builds/runs ad, reads game assets, or imports
the Rust implementation. A pinned source checkout and g++ are required.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
SYSTEM = "contrib/extractor/System.cpp"
HASHES = {
    SYSTEM: "ca6d3ad1641cd998c62dc865499d275bd79f371526a7277bcb0581b50033af95",
    "contrib/extractor/dbcfile.h": "93e573dd1d2e27ee2e345697e288e0403c79215669747907832a3be23c75dfee",
    "contrib/extractor/dbcfile.cpp": "553475f64e1a409db973fd3583577b6b4bb9e1248c14c1b0dad4ac3f9cccbb06",
    "contrib/extractor/loadlib/adt.h": "f628f107b8505362a51ecba0d0f53c69cfc531c7c22d72a36145e64dd3132166",
    "contrib/extractor/loadlib/adt.cpp": "008cefcf246750a85af4796381eed7df2b9f22fb3c7e3f8983f9cd497f06ac61",
    "contrib/extractor/loadlib/loadlib.h": "645feea5d6a93c05f45d1d05cca22819ce579fdc0c9ffb56c9e7430812696af3",
    "contrib/extractor/loadlib/loadlib.cpp": "4354ae954f6e376d42ddce312fa17573946e515646cc4be5044bbb4c040499b2",
    "src/game/Maps/GridMapDefines.h": "3a901f22b6d65edae74a93b5cd2be793ebc5c3884eb2c0aa06a37265ef53e54e",
}
CASES = {
    "flat": "Exactly-flat float grid collapses to MHGT header only",
    "varied": "Interleaved 145-height V9/V8 deinterleave and per-cell base addition",
    "seams": "Conflicting shared V9 edges; later MCIN row-major cell wins",
    "clamp_mixed": "Mixed heights below and above source -500 minimum",
    "clamp_flat": "All heights below minimum collapse after clamping",
    "tiny_range": "Nonzero range below 0.005 remains float data with -f 0",
    "area_full": "Mixed known, missing, zero, out-of-range, sentinel and narrowed flags",
    "area_fallback": "Uniform unknown area collapses to 0xffff",
    "holes": "All 256 row-major hole masks, including each single bit",
    "nonflat_holes": "Nonflat height data with the same discriminating hole masks",
    "upper_sentinel": "Source minimum scan starts at 20000, even above all samples",
    "missing_mcvt": "Absent MCVT uses the complete cell base height",
    "missing_mcvt_signed_zero": "Absent MCVT preserves negative-zero base bits",
}
AREA_ROWS = [(0, 99), (1, 0x1234), (2, 17), (3, 0xffff), (4, 0),
             (5, 0x1fffe), (8, 0x1234), (2, 0x10002)]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def chunk(tag, payload):
    return tag[::-1].encode("ascii") + struct.pack("<I", len(payload)) + payload


def dbc(fields, rows, strings=b"\0"):
    return (b"WDBC" + struct.pack("<4I", len(rows), fields, fields * 4, len(strings))
            + b"".join(struct.pack("<" + "I" * fields, *row) for row in rows) + strings)


def authored_common():
    map_row = [0] * 42
    map_row[0:2] = [1, 1]
    area_rows = []
    for area, flag in AREA_ROWS:
        row = [0] * 25
        row[0], row[3] = area, flag
        area_rows.append(row)
    main = bytearray(64 * 64 * 8)
    struct.pack_into("<I", main, (31 * 64 + 32) * 8, 1)
    return {
        "Map.dbc": dbc(42, [map_row], b"\0AuthoredOracle\0"),
        "AreaTable.dbc": dbc(25, area_rows),
        "LiquidType.dbc": dbc(4, [[1, 0, 0, 3]]),
        "source.wdt": (chunk("MVER", struct.pack("<I", 18))
                       + chunk("MPHD", bytes(32)) + chunk("MAIN", main)
                       + chunk("MWMO", b"")),
    }


def cell_values(case, row, col):
    slot = row * 16 + col
    base, heights, area, holes = 7.25, [0.0] * 145, 1, 0
    if case in ("varied", "nonflat_holes"):
        base = slot / 8.0 - 10.0
        heights = [(i - 72) / 32.0 for i in range(145)]
    elif case == "seams":
        base = slot * 10.0
        heights = [i / 8.0 for i in range(145)]
    elif case == "clamp_mixed":
        base = -501.0 + row / 2.0 + col / 16.0
        heights = [-(i % 7) / 8.0 for i in range(145)]
    elif case == "clamp_flat":
        base = -650.0 + slot / 4.0
    elif case == "tiny_range":
        base = 7.0
        heights = [i / 65536.0 for i in range(145)]
    elif case == "area_full":
        area = [0, 1, 2, 3, 4, 5, 8, 9, 0xffffffff][slot % 9]
    elif case == "area_fallback":
        area = 3
    elif case == "upper_sentinel":
        base = 25000.0 + slot
    elif case == "missing_mcvt_signed_zero":
        base = -0.0
    if case in ("holes", "nonflat_holes"):
        holes = (1 << (slot % 16)) if slot < 128 else ((slot * 257) & 0xffff)
    return base, heights, area, holes


def authored_adt(case):
    if case not in CASES:
        raise ValueError("unknown authored case")
    cells = []
    for row in range(16):
        for col in range(16):
            base, heights, area, holes = cell_values(case, row, col)
            header = bytearray(128)
            # Deliberately conflicting coordinate metadata proves MCIN ownership.
            struct.pack_into("<II", header, 4, 15 - col, 15 - row)
            has_mcvt = case not in ("missing_mcvt", "missing_mcvt_signed_zero")
            struct.pack_into("<I", header, 20, 136 if has_mcvt else 0)
            struct.pack_into("<I", header, 52, area)
            struct.pack_into("<H", header, 60, holes)
            struct.pack_into("<f", header, 112, base)
            mcvt = chunk("MCVT", struct.pack("<145f", *heights)) if has_mcvt else b""
            cells.append(chunk("MCNK", header + mcvt))
    mhdr = bytearray(64)
    struct.pack_into("<I", mhdr, 4, 64)
    mcin = bytearray(4096)
    offset = 84 + 4104
    for slot, cell in enumerate(cells):
        struct.pack_into("<II", mcin, slot * 16, offset, len(cell))
        offset += len(cell)
    return (chunk("MVER", struct.pack("<I", 18)) + chunk("MHDR", mhdr)
            + chunk("MCIN", mcin) + b"".join(cells))


def extract(core):
    sources = {}
    for name, digest in HASHES.items():
        raw = (core / name).read_bytes()
        if sha(raw) != digest:
            raise ValueError("pinned source hash mismatch: " + name)
        sources[name] = raw.decode("utf-8")
    text = sources[SYSTEM]
    intervals = []

    def interval(name, start, end):
        result = text[start:end]
        intervals.append({"name": name, "source": SYSTEM,
                          "first_byte": start, "last_byte_exclusive": end,
                          "first_line": text[:start].count("\n") + 1,
                          "last_line": text[:end].count("\n"),
                          "sha256": sha(result.encode())})
        return result

    globals_ = interval("globals", text.index("typedef struct\n"),
                        text.index("//**************************************************"))
    options = interval("options", text.index("// This option allow limit minimum height"),
                       text.index("// List MPQ for extract from"))
    readers = interval("dbc_readers", text.index("uint32 ReadMapDBC()"),
                       text.index("//\n// Adt file convertor function and data"))
    converter = interval("converter", text.index("// Map file format data"),
                         text.index("void ExtractMapsFromMpq()"))
    notice = text[:text.index("#include")]
    return sources, intervals, {
        "// PINNED_GLOBALS": globals_, "// PINNED_OPTIONS": options,
        "// PINNED_DBC_READERS": readers, "// PINNED_CONVERTER": converter,
    }, notice


def generate(core, destination):
    sources, intervals, fragments, notice = extract(core)
    compiler = shutil.which("g++")
    if not compiler:
        raise RuntimeError("g++ is required; oracle checks cannot skip")
    version = subprocess.check_output([compiler, "-dumpfullversion"], text=True).strip()
    target = subprocess.check_output([compiler, "-dumpmachine"], text=True).strip()
    if (version, target) != ("14.2.0", "x86_64-linux-gnu"):
        raise RuntimeError("unsupported audited compiler profile: " + version + " " + target)
    destination.mkdir(parents=True, exist_ok=True)
    for name, data in authored_common().items():
        (destination / name).write_bytes(data)
    for case in CASES:
        (destination / (case + ".adt")).write_bytes(authored_adt(case))
    with tempfile.TemporaryDirectory(prefix="terrain-source-oracle-") as temporary:
        temp = Path(temporary)
        for name, text in sources.items():
            if name == SYSTEM:
                continue
            relative = (name.removeprefix("contrib/extractor/")
                        if name.startswith("contrib/extractor/") else "GridMapDefines.h")
            path = temp / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        shutil.copyfile(HERE / "fixture_mpq.h", temp / "mpq_libmpq.h")
        harness = (HERE / "terrain_writer_oracle.cpp").read_text()
        for marker, fragment in fragments.items():
            if harness.count(marker) != 1:
                raise ValueError("oracle marker mismatch")
            harness = harness.replace(marker, fragment)
        (temp / "oracle.cpp").write_text(notice + harness)
        flags = ["-std=c++20", "-O0", "-fno-strict-aliasing", "-fno-fast-math",
                 "-ffp-contract=off", "-frounding-math", "-fsanitize=undefined",
                 "-fno-sanitize=alignment", "-fno-sanitize-recover=all"]
        subprocess.run([compiler, *flags, str(temp / "oracle.cpp"),
                        str(temp / "loadlib/adt.cpp"), str(temp / "loadlib/loadlib.cpp"),
                        str(temp / "dbcfile.cpp"), "-o", str(temp / "oracle")], check=True)
        result = subprocess.run([str(temp / "oracle"), str(destination.resolve()), *CASES],
                                check=True, capture_output=True)
        if result.stderr:
            raise RuntimeError("source oracle stderr: " + result.stderr.decode())
    products = []
    for case, description in CASES.items():
        raw = (destination / (case + ".map")).read_bytes()
        words = struct.unpack_from("<10I", raw)
        if raw[:8] != b"MAPSz1.4" or words[6:8] != (0, 0) or words[9] != 512:
            raise ValueError("unexpected dry source envelope")
        products.append({"name": case, "description": description,
                         "rust_supported": True,
                         "adt_bytes": len(authored_adt(case)),
                         "adt_sha256": sha(authored_adt(case)),
                         "map_bytes": len(raw), "map_sha256": sha(raw)})
    manifest = {
        "schema": 1, "source_revision": PIN, "configuration": "ad -f 0; dry authored cells",
        "map": 1, "tile_x": 31, "tile_y": 32, "directory": "AuthoredOracle",
        "source_hashes": HASHES, "source_intervals": intervals,
        "compiler": version, "compiler_target": target, "compiler_flags": flags,
        "transport_substitution": "Authored MPQFile leaf reads generated local fixture bytes",
        "source_behaviors_unchanged": ["ReadMapDBC", "ReadAreaTableDBC", "ReadLiquidTypeTableDBC",
                                       "DBCFile", "FileLoader", "ADT_file", "ConvertADT"],
        "sanitizer_limit": "Alignment excluded: source casts string literals to uint32 for FourCC",
        "excluded": ["MPQ parsing/patch winner proof", "external extractor execution", "game assets",
                     "wet terrain", "production malformed-input behavior", "VMap/MMap/world admission"],
        "common_files": {name: {"bytes": len(data), "sha256": sha(data)}
                         for name, data in authored_common().items()},
        "cases": products,
    }
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def compare(expected, actual):
    expected_names = {p.name for p in expected.iterdir() if p.is_file()}
    actual_names = {p.name for p in actual.iterdir() if p.is_file()}
    if expected_names != actual_names:
        raise ValueError("fixture file inventory differs")
    for name in sorted(expected_names):
        if (expected / name).read_bytes() != (actual / name).read_bytes():
            raise ValueError("fixture bytes differ: " + name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", required=True, type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write-fixtures", action="store_true")
    mode.add_argument("--check-fixtures", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="terrain-authored-vectors-") as temporary:
        actual = Path(temporary)
        manifest = generate(args.core, actual)
        if args.write_fixtures:
            destination = HERE / "fixtures"
            destination.mkdir(exist_ok=True)
            for path in actual.iterdir():
                shutil.copyfile(path, destination / path.name)
        else:
            compare(HERE / "fixtures", actual)
    print(f"Verified {len(manifest['cases'])} exact selected-source terrain vectors")


if __name__ == "__main__":
    main()
