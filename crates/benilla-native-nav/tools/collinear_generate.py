#!/usr/bin/env python3
"""Regenerate authored collinear polygons and unmodified pinned Detour results."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
parser = argparse.ArgumentParser()
parser.add_argument('--source', type=Path, required=True)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
source = args.source / 'dep/recastnavigation'
for line in (HERE.parent / 'native/UPSTREAM-SHA256SUMS').read_text().splitlines():
    digest, name = line.split()
    assert hashlib.sha256((source / name).read_bytes()).hexdigest() == digest, name
with tempfile.TemporaryDirectory() as tmp:
    temp = Path(tmp)
    files = ['DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
             'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode']
    subprocess.run(['c++', '-std=c++20', '-O2', '-DDT_POLYREF64=1',
                    '-fno-fast-math', '-ffp-contract=off',
                    '-I' + str(source / 'Detour/Include'), str(HERE / 'collinear_oracle.cpp'),
                    *[str(source / 'Detour/Source' / (f + '.cpp')) for f in files],
                    '-o', str(temp / 'oracle')], check=True)
    subprocess.run([str(temp / 'oracle'), str(temp)], check=True)
    dest = HERE / 'fixtures/collinear'
    dest.mkdir(exist_ok=True)
    for name in ['params.mmap', 'corner.mmtile', 'corner.tsv', 'flat.mmtile', 'flat.tsv',
                 'rounded.mmtile', 'rounded.tsv']:
        data = (temp / name).read_bytes()
        if args.check:
            assert (dest / name).read_bytes() == data, name
        else:
            (dest / name).write_bytes(data)
        print(name, len(data), hashlib.sha256(data).hexdigest())
