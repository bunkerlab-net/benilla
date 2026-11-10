#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract pinned formation bodies and prepare or run the offline numeric oracle."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from source_transports_generate import method

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'
HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--path', action='store_true')
    args = parser.parse_args()
    assert subprocess.check_output(['git', '-C', str(args.source), 'rev-parse', 'HEAD'], text=True).strip() == PIN
    source = args.source / 'src/game/Maps/SpawnGroup.cpp'
    raw = source.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == '314dc41311ead73d64f8bf009b1bff67b5f68f7d0110ba40cc8793c37023bbb5'
    body = (HERE / 'source_formations_oracle.cpp').read_text()
    body = body.replace('// INSERT_METHODS', '\n'.join(method(raw.decode(), sig) for sig in [
        'void FormationData::FixSlotsPositions()', 'float FormationSlotData::GetAngle()',
        'void FormationSlotData::AddPositionVariation(', 'void FormationSlotData::Update()']))
    cases = [(shape, count, compact, pattern, mirror) for shape in range(7)
             for count in (0, 1, 2, 3, 12) for compact in range(2)
             for pattern in range(4) for mirror in range(2)]
    inputs = ''.join(' '.join(map(str, (i, *case))) + '\n' for i, case in enumerate(cases))
    work = args.evidence.resolve()
    if args.path:
        source = args.source / 'src/game/MotionGenerators/TargetedMovementGenerator.cpp'
        raw = source.read_bytes()
        body = (HERE / 'source_formation_path_oracle.cpp').read_text().replace(
            '// INSERT_METHODS', method(raw.decode(), 'float FormationMovementGenerator::BuildPath('))
        inputs = ''
        cases = list(range(8))
        work = work / 'path'
    work.mkdir(parents=True, exist_ok=True)
    (work / 'oracle.cpp').write_text(body)
    (work / 'inputs.txt').write_text(inputs)
    command = ['g++', '-std=c++17', '-O3', '-DNDEBUG', '-ffp-contract=off', '-fno-fast-math',
               '-fno-omit-frame-pointer', '-g', '-I/source/dep/g3dlite', '/evidence/oracle.cpp', '-ldl', '-o', '/evidence/oracle']
    if not args.run:
        print(f'Prepared {len(cases)} authored cases in {work}; no image run')
        return
    actual = subprocess.check_output(['docker', 'image', 'inspect', 'benilla-cmangos-reference:f1-proven-20261106', '--format', '{{.Id}}'], text=True).strip()
    assert actual == IMAGE, actual
    available = int(next(row.split()[1] for row in Path('/proc/meminfo').read_text().splitlines() if row.startswith('MemAvailable:')))
    assert available >= 4 * 1024 * 1024, available
    base = ['docker', 'run', '--rm', '-i', '--network', 'none', '--user', f'{os.getuid()}:{os.getgid()}',
            '--mount', f'type=bind,src={args.source.resolve()},dst=/source,readonly',
            '--mount', f'type=bind,src={work},dst=/evidence', IMAGE]
    subprocess.run(base + command, check=True)
    result = subprocess.check_output(base + ['/evidence/oracle'], input=inputs.encode())
    (work / 'expected.tsv').write_bytes(result)
    fixture = HERE / 'fixtures' / ('source_formation_path' if args.path else 'source_formations')
    fixture.mkdir(parents=True, exist_ok=True)
    (fixture / 'inputs.txt').write_text(inputs)
    (fixture / 'expected.tsv').write_bytes(result)
    receipt = {'image': actual, 'revision': PIN, 'source_sha256': hashlib.sha256(raw).hexdigest(),
               'compile_command': command, 'oracle_sha256': hashlib.sha256(body.encode()).hexdigest(),
               'inputs_sha256': hashlib.sha256(inputs.encode()).hexdigest(),
               'output_sha256': hashlib.sha256(result).hexdigest(),
               'binary_sha256': hashlib.sha256((work / 'oracle').read_bytes()).hexdigest(),
               'cases': len(cases), 'rows': len(result.splitlines()),
               'scope': 'Authored source-numeric components, not sealed F1 execution'}
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    (fixture / 'provenance.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
