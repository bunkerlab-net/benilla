#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure untouched original slab predicates on explicit authored edge inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from build_original import PIN
HERE = Path(__file__).resolve().parent
SOURCE = 'dep/recastnavigation/Detour/Source/DetourNavMesh.cpp'


def capture(source, work, output):
    work.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    assert subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip() == PIN
    hashes = json.loads((HERE / 'source/source-hashes.json').read_bytes())
    raw = (source / SOURCE).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == hashes[SOURCE]
    root = source / 'dep/recastnavigation/Detour'
    sanitize = ['-fsanitize=alignment', '-fno-sanitize-recover=alignment'] if os.environ.get('BENILLA_ORACLE_SANITIZE_ALIGNMENT') == '1' else []
    command = ['g++', '-std=c++17', '-O2', '-fno-fast-math', '-ffp-contract=off', '-fno-strict-aliasing', '-fno-access-control',
               '-ffunction-sections', '-fdata-sections', '-DDT_POLYREF64', *sanitize,
               '-I'+str(root/'Include'), '-I'+str(root/'Source'), '-I'+str(source/'dep/json'),
               str(HERE/'slab_oracle.cpp'), str(root/'Source/DetourAlloc.cpp'), '-Wl,--gc-sections', '-o', str(work/'slab-oracle')]
    (work/'compile-command.json').write_text(json.dumps(command, indent=2)+'\n')
    subprocess.run(command, check=True, timeout=60)
    subprocess.run([str(work/'slab-oracle'), str(output/'expected.json')], check=True, timeout=15)
    result = json.loads((output/'expected.json').read_bytes())
    cases = {row['name']: row for row in result['cases']}
    assert len(cases) == 28
    for name in ['normal_positive_above', 'normal_negative_above', 'horizontal_touching', 'horizontal_shrink_above',
                 'vertical_positive_above', 'vertical_negative_above', 'target_climb_one_rejects']:
        assert cases[name]['connection_count'] == 0, name
    for name in ['normal_positive_equal', 'normal_negative_equal', 'horizontal_shrink_equal',
                 'vertical_positive_equal', 'vertical_negative_equal', 'target_climb_two_accepts_reverse',
                 'crossing_slopes', 'reversed_source', 'reversed_target', 'reversed_both']:
        assert cases[name]['connection_count'] == 1, name
    intervals = []
    for begin, end in [(b'inline bool overlapSlabs(', b'inline unsigned int allocLink('),
                       (b'int dtNavMesh::findConnectingPolys(', b'void dtNavMesh::unconnectLinks(')]:
        start = raw.index(begin); stop = raw.index(end, start)
        intervals.append({'start_line': raw[:start].count(b'\n')+1, 'end_line': raw[:stop].count(b'\n'),
                          'sha256': hashlib.sha256(raw[start:stop]).hexdigest()})
    manifest = {'schema': 1, 'source_pin': PIN, 'source_file': SOURCE, 'source_sha256': hashes[SOURCE],
                'source_intervals': intervals, 'compilation': 'Complete unchanged original DetourNavMesh.cpp included directly; unneeded sections discarded',
                'scope': 'Authored edge structs for source predicate behavior only; no serialized navigation output',
                'case_count': len(cases), 'finite_supported_cases': sum(row['bounded_finite_scope'] for row in result['cases']),
                'expected_sha256': hashlib.sha256((output/'expected.json').read_bytes()).hexdigest()}
    (output/'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True)+'\n')
    print('Verified 28 original slab-predicate cases (25 bounded finite, 3 unsupported arithmetic diagnostics)', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path);parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--check', action='store_true');args = parser.parse_args()
    output = args.work.resolve()/'slab-predicate-fixtures'
    capture(args.source.resolve(), args.work.resolve(), output)
    retained = HERE/'fixtures/slab-predicates'
    if args.check:
        assert {p.name:p.read_bytes() for p in output.iterdir()} == {p.name:p.read_bytes() for p in retained.iterdir()}
    else:
        import shutil
        assert not retained.exists()
        shutil.copytree(output, retained)
