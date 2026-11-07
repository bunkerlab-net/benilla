#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored map/tile products through unchanged pinned CMaNGOS source.

Compile complete original TileAssembler, ModelInstance, WorldModel, BIH,
VMapManager2 and MapTree translation units, with original G3D dependencies.
The harness supplies only Platform/Define.h integer aliases. Source names,
placements and raw geometry are fixed authored literals, never game assets.
Only the unchanged original source receives the trailing-NUL name fixture;
the Python harness never uses a NUL-containing name as a filesystem path.

TSV columns are case, dir_bin hex, raw models JSON, generated products JSON,
and consumer lookups JSON. JSON objects contain filenames and hex bytes.
The companion map-queries.tsv records public-manager queries on three geometrically
valid map-0 placements, including original WMO extraction and MODF capture.
All floating-point inputs/results are binary32 bit hex;
world and internal roundtrip coordinates come from the original public converter.
No Rust implementation, held compiler patch or listfile oracle is consulted.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
HERE = Path(__file__).resolve().parent
MODEL_NAMES = frozenset({'Hull.wmo', 'Decoy.wmo', 'Foo.m2'})
GEOMETRY_TOOLS = HERE.parent.parent / 'benilla-native-geometry-tests/tools'
CAPTURED = GEOMETRY_TOOLS / 'fixtures/captured_wmo_source'


def array(tag, count, payload):
    return tag + struct.pack('<II', 4 + len(payload), count) + payload


def raw_model(root=5875):
    """One authored triangle in one dry group; source-compatible raw framing."""
    vertices = [(-1.0, -2.0, -3.0), (4.0, 0.0, 6.0), (0.0, 5.0, 1.0)]
    group = struct.pack('<II6fI', 0x2000, 23, -1, -2, -3, 4, 5, 6, 0)
    group += array(b'GRP ', 1, struct.pack('<I', 1))
    group += array(b'INDX', 3, struct.pack('<3H', 0, 1, 2))
    group += array(b'VERT', 3, b''.join(struct.pack('<3f', *v) for v in vertices))
    return b'VMAPs05\0' + struct.pack('<III', 3, 1, root) + group


def spawn(map_id, tile, identifier, bounds, *, flags=4, adt=11,
          position=(0, 0, 0), rotation=(0, 0, 0), scale=1, name=b'Hull.wmo'):
    """Author the original input record layout, not expected output bytes."""
    assert name in (b'Hull.wmo', b'Hull.wmo\0', b'Decoy.wmo', b'Foo.m2', b'Foo.m2\0')
    result = struct.pack('<4IHI7f', map_id, *tile, flags, adt, identifier,
                         *position, *rotation, scale)
    if flags & 4:
        result += struct.pack('<6f', *bounds)
    result += struct.pack('<I', len(name)) + name
    return result


def cases():
    models = {'Hull.wmo': raw_model(), 'Decoy.wmo': raw_model(99)}
    # First encounter and tile order intentionally differ from numeric ID order.
    # The later ID-90 payload is a different model, position, bounds and ADT.
    rows = [spawn(7, (9, 3), 90, (20, -2, -1, 22, 3, 5)),
            spawn(7, (3, 9), 10, (-30, -2, -1, -28, 3, 5)),
            spawn(7, (3, 9), 90, (900, 901, 902, 903, 904, 905),
                  name=b'Decoy.wmo', adt=65535, position=(100, 200, 300), scale=2),
            spawn(7, (3, 9), 10, (-999, -999, -999, 999, 999, 999), adt=4),
            spawn(7, (9, 3), 50, (4, -2, -1, 6, 3, 5)),
            spawn(7, (1, 2), 70, (50, -2, -1, 52, 3, 5)),
            spawn(7, (1, 2), 30, (-8, -2, -1, -6, 3, 5)),
            spawn(7, (9, 3), 90, (77, 88, 99, 100, 101, 102), name=b'Decoy.wmo')]
    yield ('ordinary_multitile_duplicates', b''.join(rows), models,
           {'007.vmtree', '007_01_02.vmtile', '007_03_09.vmtile',
            '007_09_03.vmtile', 'Hull.wmo.vmo'}, 'wmo')
    # Nonleaf split cases along every axis, plus intersecting boxes. Input order
    # deliberately disagrees with both spatial and unique-ID order.
    centers = [20, -30, 4, 50, -8, 70, -60]
    ids = [90, 10, 50, 70, 30, 110, 5]
    for axis in range(3):
        rows = []
        for identifier, center in zip(ids, centers):
            low, high = [-2, -3, -4], [3, 4, 5]
            low[axis], high[axis] = center, center + 2
            rows.append(spawn(12 + axis, (2, 8), identifier, (*low, *high)))
        stem = f'{12 + axis:03}'
        yield (f'nonleaf_axis_{axis}', b''.join(rows), {'Hull.wmo': raw_model()},
               {stem + '.vmtree', stem + '_02_08.vmtile', 'Hull.wmo.vmo'}, 'wmo')
    overlap = [(-100, -1, -1, 100, 1, 1), (-80, -5, -2, -70, 4, 3),
               (70, -5, -2, 80, 4, 3), (-3, -80, -2, 3, -70, 3),
               (-3, 70, -2, 3, 80, 3), (-2, -2, -90, 2, 2, -80),
               (-2, -2, 80, 2, 2, 90)]
    rows = [spawn(27, (0, 63), identifier, bound)
            for identifier, bound in zip(ids, overlap)]
    yield ('nonleaf_overlapping_bounds', b''.join(rows), {'Hull.wmo': raw_model()},
           {'027.vmtree', '027_00_63.vmtile', 'Hull.wmo.vmo'}, 'wmo')
    rows = [spawn(33, (65, 65), 90, (-4, -3, -2, 4, 3, 2), flags=6),
            spawn(33, (65, 65), 10, (-20, -10, 0, -15, -5, 8), flags=6),
            spawn(33, (65, 65), 90, (-999, -999, -999, 999, 999, 999),
                  flags=4, name=b'Decoy.wmo', adt=7),
            spawn(33, (65, 65), 50, (30, 40, -2, 35, 45, 3), flags=6)]
    yield ('global65_duplicates', b''.join(rows), models,
           {'033.vmtree', 'Hull.wmo.vmo'}, 'wmo')
    rows = [spawn(530, (65, 65), 90, (-4, -3, -2, 4, 3, 2), flags=6),
            spawn(530, (2, 1), 10, (-20, -10, 0, -15, -5, 8)),
            spawn(530, (2, 1), 50, (30, 40, -2, 35, 45, 3))]
    yield ('global65_and_regular_tile', b''.join(rows), {'Hull.wmo': raw_model()},
           {'530.vmtree', '530_02_01.vmtile', 'Hull.wmo.vmo'}, 'wmo')
    # The 65/65 range determines isTiled/GOBJ independently of WORLDSPAWN.
    rows = [spawn(44, (65, 65), 17, (-4, -3, -2, 4, 3, 2)),
            spawn(44, (65, 65), 17, (100, 200, 300, 400, 500, 600), adt=2)]
    yield ('global65_without_world_flag', b''.join(rows), {'Hull.wmo': raw_model()},
           {'044.vmtree', '044_65_65.vmtile', 'Hull.wmo.vmo'}, 'wmo')
    # The first reference is ordinary, so the original tile writer consumes all
    # entries in its one counted loop, including the later WORLDSPAWN record.
    # A leading WORLDSPAWN followed by an ordinary record is deliberately not
    # executed: its skip/count mismatch can advance the original iterator past end.
    rows = [spawn(45, (8, 6), 100, (-4, -3, -2, 4, 3, 2)),
            spawn(45, (8, 6), 200, (-20, -10, 0, -15, -5, 8), flags=6)]
    yield ('mixed_tile_nonworld_first', b''.join(rows), {'Hull.wmo': raw_model()},
           {'045.vmtree', '045_08_06.vmtile', 'Hull.wmo.vmo'}, 'wmo')
    yield ('empty_dir', b'', {}, set(), 'none')
    # Bounded records isolate name framing from the separate M2-bound oracle.
    for shortened in (False, True):
        name = b'Foo.m2\0' if shortened else b'Foo.m2'
        row = spawn(1, (4, 8), 23, (-1, -2, -3, 4, 5, 6), flags=4, adt=0,
                    position=(8, -3, 2), rotation=(0, 90, 0), scale=1.5, name=name)
        yield ('m2_shortened_name' if shortened else 'm2_plain_name', row,
               {'Foo.m2': raw_model()},
               {'001.vmtree', '001_04_08.vmtile',
                'Foo.m2' if shortened else 'Foo.m2.vmo'}, 'm2')
    # Unlike structural-only map fixtures above, these bounds exactly enclose
    # the transformed triangle. Separate directories isolate C-string shortening.
    for shortened in (False, True):
        name = b'Hull.wmo\0' if shortened else b'Hull.wmo'
        row = spawn(0, (30, 31), 23, (16599, 16098, 1, 16604, 16105, 10),
                    position=(16600, 16100, 4), name=name)
        yield ('query_wmo_shortened_name' if shortened else 'query_wmo_plain_name',
               row, {'Hull.wmo': raw_model()},
               {'000.vmtree', '000_30_31.vmtile',
                'Hull.wmo' if shortened else 'Hull.wmo.vmo'}, 'query_wmo')

    # These bytes are original ExtractSingleWmo and WMOInstance outputs, verified
    # by re-executing their guarded source before any case is assembled. The ID,
    # raw count and placement are never replaced with the authored helpers above.
    yield ('captured_wmo', (CAPTURED / 'dir_bin').read_bytes(),
           {'Hull.wmo': (CAPTURED / 'hull.raw.vmap').read_bytes()},
           {'000.vmtree', '000_30_31.vmtile', 'Hull.wmo.vmo'}, 'captured_wmo')


HARNESS = r'''
#include <cfenv>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <string>
#include "TileAssembler.h"
#include "VMapManager2.h"

static bool trace_open = false;
extern "C" FILE* __real_fopen(const char*, const char*);
extern "C" FILE* __wrap_fopen(const char* path, const char* mode) {
    if (trace_open) {
        const char* base = std::strrchr(path, '/');
        base = base ? base + 1 : path;
        std::printf("OPEN ");
        for (const unsigned char* p = reinterpret_cast<const unsigned char*>(base); *p; ++p)
            std::printf("%02x", *p);
        std::printf("\n");
    }
    return __real_fopen(path, mode);
}
static std::string bits(float value) {
    uint32_t word;
    std::memcpy(&word, &value, sizeof(word));
    char buffer[11];
    std::snprintf(buffer, sizeof(buffer), "\"%08x\"", word);
    return buffer;
}
static std::string vector_bits(const G3D::Vector3& value) {
    return "[" + bits(value.x) + "," + bits(value.y) + "," + bits(value.z) + "]";
}
static const char* boolean(bool value) { return value ? "true" : "false"; }
static void record(const std::string& query, const std::string& request,
                   const std::string& result) {
    std::printf("QUERY\t%s\t%s\t%s\n", query.c_str(), request.c_str(), result.c_str());
}
struct QueryPoint {
    const char* name;
    G3D::Vector3 authored, world, roundtrip;
    QueryPoint(VMAP::VMapManager2& manager, const char* label, float x, float y, float z)
        : name(label), authored(x, y, z),
          world(manager.convertPositionToInternalRep(x, y, z)),
          roundtrip(manager.convertPositionToInternalRep(world.x, world.y, world.z)) {
        record(std::string("coordinates_") + name,
               "{\"internal_bits\":" + vector_bits(authored) + "}",
               "{\"world_bits\":" + vector_bits(world) +
               ",\"roundtrip_internal_bits\":" + vector_bits(roundtrip) + "}");
    }
};
static void height_query(VMAP::VMapManager2& manager, const char* label,
                         const QueryPoint& point, float search) {
    const float height = manager.getHeight(0, point.world.x, point.world.y, point.world.z, search);
    record(std::string("height_") + label,
           "{\"position\":\"" + std::string(point.name) + "\",\"max_search_dist_bits\":" + bits(search) + "}",
           "{\"height_bits\":" + bits(height) + "}");
}
static bool segment_queries(VMAP::VMapManager2& manager, const char* label,
                            const QueryPoint& start, const QueryPoint& end) {
    const std::string endpoints = "{\"start\":\"" + std::string(start.name) +
                                 "\",\"end\":\"" + end.name + "\"";
    for (bool ignore : {false, true}) {
        const bool clear = manager.isInLineOfSight(0, start.world.x, start.world.y, start.world.z,
                                                  end.world.x, end.world.y, end.world.z, ignore);
        record(std::string("los_") + label + (ignore ? "_ignore_m2" : "_include_m2"),
               endpoints + ",\"ignore_m2\":" + boolean(ignore) + "}",
               std::string("{\"clear\":") + boolean(clear) + "}");
    }
    bool unmodified_hit = false;
    const float modifiers[] = {0.f, -0.f, 1.f, -1.f, -100.f};
    const char* names[] = {"zero", "negative_zero", "positive", "negative", "clamped"};
    for (unsigned i = 0; i < sizeof(modifiers) / sizeof(modifiers[0]); ++i) {
        G3D::Vector3 result;
        const bool hit = manager.getObjectHitPos(0, start.world.x, start.world.y, start.world.z,
                                                 end.world.x, end.world.y, end.world.z,
                                                 result.x, result.y, result.z, modifiers[i]);
        const G3D::Vector3 internal = manager.convertPositionToInternalRep(result.x, result.y, result.z);
        record(std::string("hit_") + label + "_" + names[i],
               endpoints + ",\"modify_dist_bits\":" + bits(modifiers[i]) + "}",
               std::string("{\"hit\":") + boolean(hit) + ",\"world_bits\":" + vector_bits(result) +
               ",\"internal_bits\":" + vector_bits(internal) + "}");
        if (i == 0) unmodified_hit = hit;
    }
    return unmodified_hit;
}
static int map_queries(const char* base) {
    VMAP::VMapManager2 manager;
    // StaticMapTree::getTileFileName swaps x/y relative to dir_bin's tile fields.
    const auto loaded = manager.loadMap(base, 0, 31, 30);
    const bool tile_loaded = manager.IsTileLoaded(0, 31, 30);
    const bool transposed_loaded = manager.IsTileLoaded(0, 30, 31);
    record("load", "{\"map_id\":0,\"tile_x\":31,\"tile_y\":30}",
           "{\"load_result\":" + std::to_string(static_cast<unsigned>(loaded)) +
           ",\"tile_loaded\":" + boolean(tile_loaded) +
           ",\"transposed_tile_loaded\":" + boolean(transposed_loaded) + "}");
    if (loaded != VMAP::VMAP_LOAD_RESULT_OK || !tile_loaded || transposed_loaded) return 5;
    const QueryPoint top(manager, "hit_top", 16601.f, 16100.f, 20.f);
    const QueryPoint bottom(manager, "hit_bottom", 16601.f, 16100.f, 0.f);
    const QueryPoint short_end(manager, "short_end", 16601.f, 16100.f, 15.f);
    const QueryPoint tiny_end(manager, "tiny_end", 16601.f, 16100.f, 1e-11f);
    const QueryPoint miss_top(manager, "miss_top", 16603.f, 16104.f, 20.f);
    const QueryPoint miss_bottom(manager, "miss_bottom", 16603.f, 16104.f, 0.f);
    height_query(manager, "down", top, 20.f);
    height_query(manager, "up", bottom, -20.f);
    height_query(manager, "short_down", top, 5.f);
    height_query(manager, "short_up", bottom, -1.f);
    height_query(manager, "zero", top, 0.f);
    height_query(manager, "negative_zero", top, -0.f);
    height_query(manager, "below_down", bottom, 20.f);
    height_query(manager, "above_up", top, -20.f);
    height_query(manager, "miss_down", miss_top, 20.f);
    height_query(manager, "miss_up", miss_bottom, -20.f);
    const bool hit_down = segment_queries(manager, "down", top, bottom);
    const bool hit_up = segment_queries(manager, "up", bottom, top);
    segment_queries(manager, "miss_down", miss_top, miss_bottom);
    segment_queries(manager, "miss_up", miss_bottom, miss_top);
    segment_queries(manager, "short", top, short_end);
    segment_queries(manager, "zero", top, top);
    segment_queries(manager, "tiny", bottom, tiny_end);
    manager.unloadMap(0);
    return hit_down && hit_up ? 0 : 6;
}
int main(int argc, char** argv) {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
    static_assert(sizeof(int) == 4 && sizeof(short) == 2);
    static_assert(sizeof(G3D::Vector3) == 12 && sizeof(G3D::AABox) == 24);
    if (argc != 4 || std::fesetround(FE_TONEAREST) != 0) return 1;
    if (std::string(argv[1]) == "assemble") {
        VMAP::TileAssembler assembler(argv[2], argv[3]);
        return assembler.convertWorld2() ? 0 : 2;
    }
    if (std::string(argv[1]) == "queries") return map_queries(argv[2]);
    if (std::string(argv[1]) != "lookup") return 3;
    std::string name;
    const std::string kind(argv[3]);
    // These fixed literals are the only names exposed to the original consumer.
    if (kind == "plain") name = "Foo.m2";
    else if (kind == "nul") name = std::string("Foo.m2\0", 7);
    else if (kind == "wmo") name = "Hull.wmo";
    else if (kind == "wmo_nul") name = std::string("Hull.wmo\0", 9);
    else return 4;
    VMAP::VMapManager2 manager;
    trace_open = true;
    const bool loaded = manager.acquireModelInstance(std::string(argv[2]) + "/", name) != nullptr;
    trace_open = false;
    std::printf("LOADED %u\n", static_cast<unsigned>(loaded));
    if (loaded) manager.releaseModelInstance(name);
    return 0;
}
'''


def compact(value):
    return json.dumps(value, separators=(',', ':'), sort_keys=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('Original binary fixture profile requires little-endian')
    source = args.source.resolve()
    revision = subprocess.check_output(
        ['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True, timeout=10).strip()
    if revision != PIN:
        raise SystemExit(f'Wrong source revision: expected {PIN}, got {revision}')
    subprocess.run(['git', '-C', str(source), 'diff', '--exit-code', 'HEAD', '--',
                    'src/game/vmap', 'src/game/Maps/GridMapDefines.h', 'dep/g3dlite'],
                   check=True, stdout=subprocess.DEVNULL, timeout=10)
    rows = [
        '# case\tdir-bin-hex\traw-models-json\tgenerated-products-json\tconsumer-lookups-json',
        f'# source-pin\t{PIN}',
        '# scope\tunchanged original convertWorld2, ModelSpawn parser/writer, BIH, raw/model writer, VMapManager2::acquireModelInstance and WorldModel::readFile',
        '# raw-models-json\tfilename -> raw hex; one dry group, root 5875 (Decoy 99), group ID 23, flags 8192, bounds [-1,-2,-3,4,5,6], vertices [[-1,-2,-3],[4,0,6],[0,5,1]], indices [0,1,2]',
        '# generated-products-json\tactual output filename -> complete output hex; filenames validated against authored expected set',
        '# consumer-lookups-json\tname_hex, fopen_basename_hex and loaded; fresh original manager for each lookup; fopen observed through link wrapper',
    ]
    query_rows = [
        '# case\tquery\trequest-json\tresult-json',
        f'# source-pin\t{PIN}',
        '# scope\tunchanged original public VMapManager2 loadMap, IsTileLoaded, convertPositionToInternalRep, getHeight, isInLineOfSight and getObjectHitPos; generated products from matching map-products.tsv case',
        '# binary32\tall *_bits values are eight-digit IEEE-754 binary32 hex words; arrays are x,y,z; C++ memcpy captures original inputs and return values without decimal conversion',
        '# load\tmap 0 dir_bin tile 30,31 is loaded with public manager tile 31,30 because StaticMapTree::getTileFileName transposes tile indices; IsTileLoaded and positive forward/reverse hit are required',
        '# coordinates\tauthored internal input -> original convertPositionToInternalRep world -> original convertPositionToInternalRep roundtrip; other query requests refer to these point names and use their world coordinates',
        '# results\theight_bits is the public getHeight result including its invalid-height sentinel; hit positions are public getObjectHitPos outputs and their original converter roundtrip; no distance is reconstructed from hit position',
        '# modifiers\tzero=+0, negative_zero=-0, positive=+1, negative=-1, clamped=-100; short search and sub-1e-10 segment are distinct from zero',
    ]
    subprocess.run([sys.executable, str(GEOMETRY_TOOLS / 'generate_wmo_vmap_fixture.py'),
                    str(source), '--check'], check=True, timeout=120)
    fixtures = list(cases())
    query_count = 0
    with tempfile.TemporaryDirectory(prefix='benilla-map-products-') as temporary:
        work = Path(temporary)
        (work / 'Platform').mkdir()
        (work / 'Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'probe.cpp').write_text(HARNESS)
        vmap = source / 'src/game/vmap'
        g3d = source / 'dep/g3dlite'
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections',
                   '-DNO_CORE_FUNCS', '-I' + str(work), '-I' + str(vmap),
                   '-I' + str(source / 'src/game'), '-I' + str(g3d), str(work / 'probe.cpp')]
        command += [str(vmap / (name + '.cpp')) for name in
                    ['TileAssembler', 'WorldModel', 'BIH', 'ModelInstance', 'VMapManager2', 'MapTree']]
        command += [str(g3d / (name + '.cpp')) for name in
                    ['Vector3', 'AABox', 'g3dmath', 'Matrix3', 'Ray', 'CollisionDetection',
                     'MemoryManager', 'ReferenceCount', 'System', 'debugAssert', 'format', 'prompt', 'Log',
                     'FileSystem', 'fileutils', 'stringutils', 'Crypto']]
        command += ['-Wl,--gc-sections', '-Wl,--wrap=fopen', '-lz', '-o', str(work / 'probe')]
        subprocess.run(command, check=True, timeout=120)
        for case, payload, models, expected_names, lookup_kind in fixtures:
            input_dir = work / case / 'input'
            output_dir = work / case / 'output'
            input_dir.mkdir(parents=True)
            output_dir.mkdir()
            (input_dir / 'dir_bin').write_bytes(payload)
            for name, raw in models.items():
                assert name in MODEL_NAMES  # Fixed, safe filesystem literals only.
                (input_dir / name).write_bytes(raw)
            subprocess.run([str(work / 'probe'), 'assemble', str(input_dir), str(output_dir)],
                           check=True, stdout=subprocess.DEVNULL, timeout=10)
            paths = list(output_dir.iterdir())
            if {path.name for path in paths} != expected_names or not all(p.is_file() for p in paths):
                raise AssertionError(f'{case}: unexpected output filenames')
            products = {path.name: path.read_bytes().hex() for path in paths}
            lookups = []
            kinds = (['plain', 'nul'] if lookup_kind == 'm2' else
                     ['wmo', 'wmo_nul'] if lookup_kind == 'query_wmo' else
                     ['wmo'] if lookup_kind == 'captured_wmo' else [lookup_kind])
            for kind in kinds:
                if kind == 'none':
                    continue
                result = subprocess.run([str(work / 'probe'), 'lookup', str(output_dir), kind],
                                        check=True, capture_output=True, text=True, timeout=10)
                opened = [line.removeprefix('OPEN ') for line in result.stdout.splitlines()
                          if line.startswith('OPEN ')]
                loaded = [line.removeprefix('LOADED ') for line in result.stdout.splitlines()
                          if line.startswith('LOADED ')]
                assert len(opened) == len(loaded) == 1, result.stdout
                names = {'plain': b'Foo.m2', 'nul': b'Foo.m2\0',
                         'wmo': b'Hull.wmo', 'wmo_nul': b'Hull.wmo\0'}
                basename = {'plain': b'Foo.m2.vmo', 'nul': b'Foo.m2',
                            'wmo': b'Hull.wmo.vmo', 'wmo_nul': b'Hull.wmo'}[kind]
                assert opened[0] == basename.hex()
                assert (loaded[0] == '1') == (basename.decode() in expected_names)
                lookups.append({'name_hex': names[kind].hex(),
                                'fopen_basename_hex': opened[0], 'loaded': loaded[0] == '1'})
            rows.append('\t'.join([case, payload.hex(),
                                   compact({name: raw.hex() for name, raw in models.items()}),
                                   compact(products), compact(lookups)]))
            if lookup_kind in ('query_wmo', 'captured_wmo'):
                result = subprocess.run([str(work / 'probe'), 'queries', str(output_dir), 'map0'],
                                        check=True, capture_output=True, text=True, timeout=10)
                records = [line.split('\t') for line in result.stdout.splitlines()
                           if line.startswith('QUERY\t')]
                assert len(records) == 66, result.stdout
                query_count += len(records)
                assert len({fields[1] for fields in records}) == len(records)
                for marker, query, request, response in records:
                    assert marker == 'QUERY'
                    query_rows.append('\t'.join([case, query, compact(json.loads(request)),
                                                 compact(json.loads(response))]))
    for filename, content, count in [('map-products.tsv', rows, len(fixtures)),
                                     ('map-queries.tsv', query_rows, query_count)]:
        output = ('\n'.join(content) + '\n').encode()
        target = HERE / 'fixtures' / filename
        if args.check:
            if target.read_bytes() != output:
                raise SystemExit(f'Original {filename} differs')
        else:
            target.write_bytes(output)
        print(f'{"Verified" if args.check else "Generated"} {count} original {filename} fixtures; '
              f'sha256={hashlib.sha256(output).hexdigest()}')


if __name__ == '__main__':
    main()
