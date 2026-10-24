#!/usr/bin/env python3
"""Repaired cold lookup plus exact same-polygon terminal; no original cold execution."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
sys.dont_write_bytecode = True
import cold_distance_generate as cold
import point_path_generate as point

HERE = Path(__file__).resolve().parent
PACKAGE = HERE / 'cold_same_poly_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/cold-same-poly'
FILES = ('cases.test-fixture',)
FIELDS = ('label oracle geometry from to normalize_z ignore_normalization query_nodes '
          'constructor_active constructor_storage start_ref start_distance after_start_storage '
          'end_ref end_distance lookup_storage after_active after_storage path_type actual_end points').split()


def guard_inputs():
    # Reuse the existing pinned source and authored geometry manifests, unchanged.
    point.guard_inputs()
    cold.guard_inputs()


def build_harness(sources):
    source = sources[point.CPP]
    prefix = point.source_span(source, cold.ORIGINAL, '    dtStatus dtResult;')
    if source[:source.index(cold.ORIGINAL)].count('\n') + 1 != 410:
        raise SystemExit('Cold source location drift')
    repaired = cold.repair_distance_initialization(prefix)
    constructor = point.exact_block(source, 'PathFinder::PathFinder(const Unit* owner,')
    cache_members = point.source_span(sources[point.HEADER],
        '        uint32         m_pointPathLimit;', '        Vector3        m_startPosition;')
    initializers = point.source_span(constructor,
        '    m_pointPathLimit(MAX_POINT_PATH_LENGTH)', ' m_sourceUnit(owner)')
    # The only syntactic adaptation is removal of the trailing comma separating
    # the next original owner initializer, which this cache-only projection excludes.
    if not initializers.endswith(','):
        raise SystemExit('Constructor cache initializer boundary drift')
    initializers = initializers[:-1]
    far = point.source_span(source, '    bool farFromPoly = ', '    if (farFromPoly)')
    same = point.source_span(source, '    // start and end are on same polygon',
                             '    // look for startPoly/endPoly in current path')
    harness, _ = point.build_harness(sources)
    old_main = point.exact_block(harness, 'int main(int argc,char** argv)')
    retained_main = old_main.replace('int main(', '[[maybe_unused]] static int retained_warm_fixture_main(', 1)
    retained_main = retained_main[:-1] + '    return 0;\n}'
    harness = harness.replace(old_main, retained_main, 1)
    extension = (HERE / 'cold_same_poly_oracle.cpp').read_text()
    declarations, extension = extension.split('// INSERTED_BEFORE_PATHFINDER', 1)
    for marker, fragment in (('// ORIGINAL_CONSTRUCTOR_CACHE_INITIALIZERS', initializers),
                             ('// ORIGINAL_CONSTRUCTOR_CACHE_MEMBERS', cache_members)):
        if declarations.count(marker) != 1:
            raise SystemExit('Cold constructor insertion drift: ' + marker)
        declarations = declarations.replace(marker, fragment, 1)
    if harness.count('class PathFinder {') != 1:
        raise SystemExit('Cold class insertion drift')
    harness = harness.replace('class PathFinder {', declarations + '\nclass PathFinder {', 1)
    marker = '    void BuildPointPath(const float*,const float*);'
    declaration = ('    void compatibility_same_polygon(const Vector3& startPos, const Vector3& endPos,\n'
                   '        ColdEvidence& evidence);\n')
    if harness.count(marker) != 1:
        raise SystemExit('Cold method declaration drift')
    harness = harness.replace(marker, declaration + marker, 1)
    instrumented = repaired.replace('    dtPolyRef endPoly =',
        '    evidence.afterStart = m_pathPolyRefs;\n    dtPolyRef endPoly =', 1)
    for marker, fragment in (('// REPAIRED_SOURCE_ENDPOINT_PREFIX', instrumented),
                             ('// ORIGINAL_FAR_PREDICATE', far), ('// ORIGINAL_SAME_POLYGON_TERMINAL', same)):
        if extension.count(marker) != 1:
            raise SystemExit('Cold terminal insertion drift: ' + marker)
        extension = extension.replace(marker, fragment, 1)
    harness += '\n' + extension
    package = ('// SPDX-License-Identifier: GPL-3.0-or-later\n// CMaNGOS ' + point.PIN + '\n'
               '// Policy: ' + cold.POLICY + '; only two caller distance locals are initialized.\n'
               '// Unchanged methods, G3D, Detour, licenses and full BuildPolyPath review context:\n'
               '// point_path_source_fragments.test-fixture. The complete constructor is review-only.\n'
               '// Its cache-only initializer projection excludes owner lifecycle and query pointers.\n'
               '// ORIGINAL CONSTRUCTOR CONTEXT\n' + constructor + '\n'
               '// ORIGINAL CACHE MEMBER DECLARATIONS\n' + cache_members + '\n'
               '// ORIGINAL CACHE INITIALIZER PROJECTION\n' + initializers + '\n'
               '// ORIGINAL CALLER PREFIX (not executed)\n' + prefix + '\n'
               '// REPAIRED CALLER PREFIX (sole semantic amendment)\n' + repaired + '\n'
               '// ORIGINAL FAR PREDICATE\n' + far + '\n'
               '// ORIGINAL SAME-POLYGON TERMINAL\n' + same)
    return harness, package.rstrip() + '\n'


def parse_cases(data):
    rows = point.parse_table(data, FIELDS)
    for row in rows:
        try:
            if row['oracle'] != 'compatibility-zero-v1' or row['geometry'] != 'precision': raise ValueError()
            if row['normalize_z'] != '0' or row['ignore_normalization'] != '0': raise ValueError()
            if row['query_nodes'] != '1024' or row['constructor_active'] != '0': raise ValueError()
            if point.ref_list(row['constructor_storage']) != [0] * 74: raise ValueError()
            for key in ('from', 'to', 'actual_end'):
                if len(point.points(row[key], 1)) != 1: raise ValueError()
            for key in ('start_ref', 'end_ref'):
                if not 0 < int(row[key]) < (1 << 64): raise ValueError()
            if row['start_ref'] != row['end_ref']: raise ValueError()
            for key in ('start_distance', 'end_distance'):
                if not 0 <= point.finite_bits(row[key]) <= 7: raise ValueError()
            for key in ('after_start_storage', 'lookup_storage', 'after_storage'):
                if not 74 <= len(point.ref_list(row[key])) <= 4096: raise ValueError()
            if row['after_active'] != '1' or row['path_type'] != '1': raise ValueError()
            if len(point.points(row['points'])) != 2: raise ValueError()
        except (ValueError, OverflowError):
            raise SystemExit('Invalid cold same-polygon row: ' + row['label']) from None
    return rows


def compile_harness(sources, work, harness, sanitize=False, required_symbols=()):
    """Shared audited compile only; copies every dependency without semantic repairs."""
    compiler = shutil.which('g++')
    if not compiler: raise SystemExit('Missing required g++ compiler')
    version = point.run([compiler, '-dumpfullversion'], stdout=subprocess.PIPE).stdout.decode().strip()
    target = point.run([compiler, '-dumpmachine'], stdout=subprocess.PIPE).stdout.decode().strip()
    if version != '14.2.0' or target != 'x86_64-linux-gnu':
        raise SystemExit('Unsupported audited compiler profile: ' + version + ' ' + target)
    (work / 'oracle.cpp').write_text(harness, encoding='latin1')
    for name, content in sources.items():
        if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
            dest = work / name; dest.parent.mkdir(parents=True, exist_ok=True)
            # No dtMeshTile/default-query-pointer or other source repair is applied.
            dest.write_text(content, encoding='latin1')
    detour = work / 'dep/recastnavigation/Detour'
    flags = ['-std=c++20', '-O2', '-fno-inline', '-march=x86-64', '-mno-fma', '-fno-fast-math',
             '-ffp-contract=off', '-DNDEBUG', '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror',
             '-Wno-class-memaccess', '-Wno-deprecated-copy']
    if sanitize: flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    command = [compiler, *flags, '-I' + str(detour / 'Include'), '-I' + str(work / 'dep/g3dlite'),
               str(work / 'oracle.cpp'), *[str(detour / f'Source/{name}.cpp') for name in point.DETOUR_FILES],
               '-o', str(work / 'oracle')]
    point.run(command)
    symbols = point.run(['nm', '-C', str(work / 'oracle')], stdout=subprocess.PIPE).stdout.decode()
    point.verify_symbols(symbols)
    for symbol in required_symbols:
        if symbol not in symbols:
            raise SystemExit('Missing same-polygon terminal symbol: ' + symbol)
    return work / 'oracle', symbols, {'compiler': version, 'target': target, 'command': command}


def compile_oracle(sources, work, sanitize=False, receipts=None):
    harness, _ = build_harness(sources)
    binary, symbols, profile = compile_harness(sources, work, harness, sanitize,
        ('PathFinder::compatibility_same_polygon(',))
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        shutil.copy2(work / 'oracle.cpp', receipts / 'oracle.cpp')
        shutil.copy2(work / point.TILE_HEADER, receipts / 'DetourNavMesh_observed.h')
        (receipts / 'symbols.txt').write_text(symbols)
        (receipts / 'profile.json').write_text(json.dumps({
            'cmangos_revision': point.PIN, 'compatibility_policy': cold.POLICY,
            'compatibility_version': cold.VERSION, 'oracle_label': 'compatibility-zero-v1',
            'original_cold_label': 'undefined-not-invoked-no-parity-claim',
            'source_amendment': {'original': cold.ORIGINAL.strip(), 'repaired': cold.REPAIRED.strip()},
            **profile, 'sanitize': sanitize,
            'query_nodes': 1024, 'normalize_z': False, 'ignore_normalization': False,
            'tile_lifetime_profile': 'unchanged-original-empirical', 'tile_lifetime_amendment': None,
            'observed_tile_header_sha256': hashlib.sha256((work / point.TILE_HEADER).read_bytes()).hexdigest(),
            'scope': 'original cache constructor initializer projection; repaired cold caller prefix; exact lookup bodies and same-polygon BuildShortcut/NormalizePath-disabled terminal',
            'excluded': ['complete owner constructor', 'default query pointer repair', 'configuration defaults',
                         'owner lifecycle', 'SetCurrentNavMesh', 'HaveTile', 'other BuildPolyPath branches',
                         'BuildPointPath execution', 'normalization world callbacks', 'Route admission', 'gameplay'],
            'source_hashes_sha256': hashlib.sha256(point.MANIFEST.read_bytes()).hexdigest(),
            'point_input_hashes_sha256': hashlib.sha256((HERE / 'point_path_input_hashes.sha256').read_bytes()).hexdigest(),
            'cold_input_hashes_sha256': hashlib.sha256(cold.INPUT_MANIFEST.read_bytes()).hexdigest(),
            'harness_sha256': hashlib.sha256((work / 'oracle.cpp').read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return binary


def observe(binary, work):
    output = work / 'output'; output.mkdir(exist_ok=True)
    point.run([str(binary), str(output), str(HERE / 'fixtures')])
    if {path.name for path in output.iterdir()} != set(FILES):
        raise SystemExit('Unexpected cold same-polygon oracle output')
    result = {name: (output / name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=Path); parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true'); parser.add_argument('--receipt-dir', type=Path)
    args = parser.parse_args(); sources = point.guarded_sources(args.mangos); guard_inputs()
    _, package = build_harness(sources)
    if args.check and PACKAGE.read_text() != package: raise SystemExit('Cold same-polygon source package differs')
    with tempfile.TemporaryDirectory(prefix='cold-same-poly-source-') as temporary:
        work = Path(temporary); binary = compile_oracle(sources, work, args.sanitize, args.receipt_dir)
        observed = observe(binary, work)
    if args.check:
        for name, data in observed.items():
            if (FIXTURES / name).read_bytes() != data: raise SystemExit('Cold same-polygon fixture differs: ' + name)
    else:
        PACKAGE.write_text(package); FIXTURES.mkdir(parents=True, exist_ok=True)
        for name, data in observed.items(): (FIXTURES / name).write_bytes(data)
    for name, data in observed.items():
        print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:
        (args.receipt_dir / 'output_hashes.json').write_text(json.dumps({name: hashlib.sha256(data).hexdigest() for name, data in observed.items()}, indent=2) + '\n')


if __name__ == '__main__': main()
