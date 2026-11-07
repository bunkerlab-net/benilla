#!/usr/bin/env python3
"""Generate authored mixed water observations with pinned original Detour."""
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
for name, digest in {
    'MoveMapSharedDefines.h': '7994c9cbb53c81bc30b92b71201e17081040e5988c0e54d9425e0746dec7fd16',
    'PathFinder.cpp': '0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef',
}.items():
    data = (args.source / 'src/game/MotionGenerators' / name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == digest, name
flags = (args.source / 'src/game/MotionGenerators/MoveMapSharedDefines.h').read_text()
flags = flags[flags.index('enum NavArea'):flags.index('#endif')]
with tempfile.TemporaryDirectory() as tmp:
    temp = Path(tmp)
    (temp / 'source_nav_flags.h').write_text(flags)
    files = ['DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
             'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode']
    subprocess.run(['c++', '-std=c++20', '-O2', '-DDT_POLYREF64=1',
                    '-fno-fast-math', '-ffp-contract=off', '-I' + str(temp),
                    '-I' + str(source / 'Detour/Include'), str(HERE / 'water_oracle.cpp'),
                    *[str(source / 'Detour/Source' / (f + '.cpp')) for f in files],
                    '-o', str(temp / 'oracle')], check=True)
    subprocess.run([str(temp / 'oracle'), str(HERE / 'fixtures'), str(temp)], check=True)
    dest = HERE / 'fixtures/water'
    dest.mkdir(exist_ok=True)
    for name in ['mixed.mmtile', 'cases.tsv']:
        data = (temp / name).read_bytes()
        if args.check:
            assert (dest / name).read_bytes() == data, name
        else:
            (dest / name).write_bytes(data)
        print(name, len(data), hashlib.sha256(data).hexdigest())
