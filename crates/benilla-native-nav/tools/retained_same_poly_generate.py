#!/usr/bin/env python3
"""One live repaired cold entry then original defined retained same-polygon entries."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
sys.dont_write_bytecode = True
import cold_same_poly_generate as cold_same

point = cold_same.point
cold = cold_same.cold
HERE = Path(__file__).resolve().parent
PACKAGE = HERE / 'retained_same_poly_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/retained-same-poly'
FILES = ('cases.test-fixture',)
FIELDS = ('label sequence step oracle geometry from to normalize_z ignore_normalization query_nodes '
          'before_active before_storage start_ref start_distance after_start_storage end_ref end_distance '
          'lookup_storage after_active after_storage path_type actual_end points').split()


def guard_inputs():
    cold_same.guard_inputs()


def build_harness(sources):
    harness, cold_package = cold_same.build_harness(sources)
    if cold_package != cold_same.PACKAGE.read_text():
        raise SystemExit('Existing cold same-polygon source package differs')
    old_main = point.exact_block(harness, 'int main(int argc,char** argv)')
    retained_main = old_main.replace('int main(', '[[maybe_unused]] static int retained_cold_fixture_main(', 1)
    retained_main = retained_main[:-1] + '    return 0;\n}'
    harness = harness.replace(old_main, retained_main, 1)
    marker = '    void BuildPointPath(const float*,const float*);'
    declaration = ('    void original_retained_same_polygon(const Vector3& startPos, const Vector3& endPos,\n'
                   '        ColdEvidence& evidence);\n')
    if harness.count(marker) != 1:
        raise SystemExit('Retained method declaration drift')
    harness = harness.replace(marker, declaration + marker, 1)
    source = sources[point.CPP]
    prefix = point.source_span(source, cold.ORIGINAL, '    dtStatus dtResult;')
    # Only a read-only snapshot separates the unchanged original endpoint calls.
    instrumented = prefix.replace('    dtPolyRef endPoly =',
        '    evidence.afterStart = m_pathPolyRefs;\n    dtPolyRef endPoly =', 1)
    far = point.source_span(source, '    bool farFromPoly = ', '    if (farFromPoly)')
    same = point.source_span(source, '    // start and end are on same polygon',
                             '    // look for startPoly/endPoly in current path')
    extension = (HERE / 'retained_same_poly_oracle.cpp').read_text()
    for marker, fragment in (('// ORIGINAL_RETAINED_ENDPOINT_PREFIX', instrumented),
                             ('// ORIGINAL_FAR_PREDICATE', far),
                             ('// ORIGINAL_SAME_POLYGON_TERMINAL', same)):
        if extension.count(marker) != 1:
            raise SystemExit('Retained terminal insertion drift: ' + marker)
        extension = extension.replace(marker, fragment, 1)
    harness += '\n' + extension
    package = ('// SPDX-License-Identifier: GPL-3.0-or-later\n// CMaNGOS ' + point.PIN + '\n'
               '// One live PathFinder and source1024 query, including all retained tail slots.\n'
               '// The first entry uses the unchanged cold_same_poly source package and selected +0 repair.\n'
               '// Every later entry requires active>0 and executes ORIGINAL caller locals, without repair.\n'
               '// Source lookup bodies, terminal methods, G3D/Detour and licenses are in the guarded\n'
               '// point_path_source_fragments.test-fixture and cold_same_poly_source_fragments.test-fixture.\n'
               '// ORIGINAL DEFINED RETAINED CALLER PREFIX\n' + prefix + '\n'
               '// ORIGINAL FAR PREDICATE\n' + far + '\n'
               '// ORIGINAL SAME-POLYGON TERMINAL\n' + same)
    return harness, package.rstrip() + '\n'


def parse_cases(data):
    rows = point.parse_table(data, FIELDS)
    previous = None
    for step, row in enumerate(rows):
        try:
            if row['sequence'] != 'retained_main' or row['step'] != str(step): raise ValueError()
            expected_oracle = 'compatibility-zero-v1' if step == 0 else 'original-retained-defined'
            if row['oracle'] != expected_oracle or row['geometry'] != 'precision': raise ValueError()
            if row['normalize_z'] != '0' or row['ignore_normalization'] != '0': raise ValueError()
            if row['query_nodes'] != '1024' or row['before_active'] != ('0' if step == 0 else '1'): raise ValueError()
            before = point.ref_list(row['before_storage'])
            after_start = point.ref_list(row['after_start_storage'])
            lookup = point.ref_list(row['lookup_storage'])
            after = point.ref_list(row['after_storage'])
            if not all(74 <= len(refs) <= 4096 for refs in (before, after_start, lookup, after)): raise ValueError()
            if step == 0:
                if before != [0] * 74: raise ValueError()
            elif row['before_storage'] != previous['after_storage']: raise ValueError()
            for key in ('from', 'to', 'actual_end'):
                if len(point.points(row[key], 1)) != 1: raise ValueError()
            start, end = int(row['start_ref']), int(row['end_ref'])
            if not 0 < start < (1 << 64) or start != end: raise ValueError()
            for key in ('start_distance', 'end_distance'):
                if not 0 <= point.finite_bits(row[key]) <= 7: raise ValueError()
            if after_start not in (before, before + [start]): raise ValueError()
            if lookup not in (after_start, after_start + [end]): raise ValueError()
            if after != [start] + lookup[1:]: raise ValueError()
            if row['after_active'] != '1' or row['path_type'] != '1': raise ValueError()
            if row['points'] != row['from'] + ';' + row['to'] or row['actual_end'] != row['to']: raise ValueError()
        except (ValueError, OverflowError):
            raise SystemExit('Invalid retained same-polygon row: ' + row['label']) from None
        previous = row
    if len(rows) < 3:
        raise SystemExit('Retained same-polygon sequence must include two continuations')
    return rows


def compile_oracle(sources, work, sanitize=False, receipts=None):
    harness, _ = build_harness(sources)
    binary, symbols, profile = cold_same.compile_harness(sources, work, harness, sanitize,
        ('PathFinder::compatibility_same_polygon(', 'PathFinder::original_retained_same_polygon('))
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        shutil.copy2(work / 'oracle.cpp', receipts / 'oracle.cpp')
        shutil.copy2(work / point.TILE_HEADER, receipts / 'DetourNavMesh_observed.h')
        (receipts / 'symbols.txt').write_text(symbols)
        (receipts / 'profile.json').write_text(json.dumps({
            'cmangos_revision': point.PIN, **profile, 'sanitize': sanitize,
            'initial_policy': cold.POLICY, 'initial_oracle_label': 'compatibility-zero-v1',
            'retained_oracle_label': 'original-retained-defined',
            'source_amendment': {'cold_original': cold.ORIGINAL.strip(), 'cold_repaired': cold.REPAIRED.strip(),
                                 'retained': None},
            'original_cold_label': 'undefined-not-invoked-no-parity-claim',
            'query_nodes': 1024, 'straight_line': True, 'normalize_z': False, 'ignore_normalization': False,
            'live_object_sequence': True, 'retained_precondition': 'nonempty valid active reference scan',
            'tile_lifetime_profile': 'unchanged-original-empirical', 'tile_lifetime_amendment': None,
            'observed_tile_header_sha256': hashlib.sha256((work / point.TILE_HEADER).read_bytes()).hexdigest(),
            'scope': 'cache constructor projection; one repaired cold same-polygon terminal then original defined retained same-polygon terminals on the same live PathFinder/query',
            'excluded': ['original uninitialized cold caller', 'complete owner constructor', 'default query pointer repair',
                         'other BuildPolyPath branches and locals', 'BuildPointPath execution', 'world normalization',
                         'owner lifecycle', 'Route admission', 'gameplay'],
            'source_hashes_sha256': hashlib.sha256(point.MANIFEST.read_bytes()).hexdigest(),
            'point_input_hashes_sha256': hashlib.sha256((HERE / 'point_path_input_hashes.sha256').read_bytes()).hexdigest(),
            'cold_input_hashes_sha256': hashlib.sha256(cold.INPUT_MANIFEST.read_bytes()).hexdigest(),
            'cold_source_package_sha256': hashlib.sha256(cold_same.PACKAGE.read_bytes()).hexdigest(),
            'harness_sha256': hashlib.sha256((work / 'oracle.cpp').read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return binary


def observe(binary, work):
    output = work / 'output'; output.mkdir(exist_ok=True)
    point.run([str(binary), str(output), str(HERE / 'fixtures')])
    if {path.name for path in output.iterdir()} != set(FILES):
        raise SystemExit('Unexpected retained same-polygon oracle output')
    result = {name: (output / name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=Path); parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true'); parser.add_argument('--receipt-dir', type=Path)
    args = parser.parse_args(); sources = point.guarded_sources(args.mangos); guard_inputs()
    _, package = build_harness(sources)
    if args.check and PACKAGE.read_text() != package: raise SystemExit('Retained same-polygon source package differs')
    with tempfile.TemporaryDirectory(prefix='retained-same-poly-source-') as temporary:
        work = Path(temporary); binary = compile_oracle(sources, work, args.sanitize, args.receipt_dir)
        observed = observe(binary, work)
    if args.check:
        for name, data in observed.items():
            if (FIXTURES / name).read_bytes() != data: raise SystemExit('Retained same-polygon fixture differs: ' + name)
    else:
        PACKAGE.write_text(package); FIXTURES.mkdir(parents=True, exist_ok=True)
        for name, data in observed.items(): (FIXTURES / name).write_bytes(data)
    for name, data in observed.items():
        print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:
        (args.receipt_dir / 'output_hashes.json').write_text(json.dumps({name: hashlib.sha256(data).hexdigest() for name, data in observed.items()}, indent=2) + '\n')


if __name__ == '__main__': main()
