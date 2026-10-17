#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent patched CMaNGOS Wander source oracle; no Rust or game assets."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
POLICY = 'cmangos-computepathtorandompoint-zero-distance-v1'
PATH = 'src/game/MotionGenerators/PathFinder.cpp'
HEADER = 'src/game/MotionGenerators/PathFinder.h'
RANDOM = 'src/game/MotionGenerators/RandomMovementGenerator.cpp'
RANDOM_HEADER = 'src/game/MotionGenerators/RandomMovementGenerator.h'
DETOUR = ('DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
          'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def exact(text, signature, semicolon=False):
    if text.count(signature) != 1:
        raise ValueError('nonunique source signature: ' + signature)
    first = text.index(signature)
    last = text.index('{', first)
    depth = 0
    while True:
        depth += (text[last] == '{') - (text[last] == '}')
        last += 1
        if not depth:
            break
    if semicolon:
        assert text[last] == ';'
        last += 1
    return first, last


def extract(core):
    sources = {}
    for line in (HERE / 'source_hashes.sha256').read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        digest, name = line.split()
        raw = (core / name).read_bytes().replace(b'\r\n', b'\n')
        if sha(raw) != digest:
            raise ValueError('pinned source hash mismatch: ' + name)
        sources[name] = raw.decode('latin1')
    records, excerpts = [], {}

    def interval(name, file, first, last, compiled=True):
        text = sources[file]
        data = text[first:last]
        excerpts[name] = data
        records.append(dict(name=name, source=file, first_byte=first, last_byte_exclusive=last,
                            first_line=text[:first].count('\n') + 1,
                            last_line=text[:last].count('\n') + (not text[:last].endswith('\n')),
                            sha256=sha(data.encode('latin1')), compiled=compiled))
        return data

    def method(name, file, signature, semicolon=False, compiled=True):
        return interval(name, file, *exact(sources[file], signature, semicolon), compiled=compiled)

    method('PathType.inc', HEADER, 'enum PathType\n', True)
    method('UnitState.inc', 'src/game/Entities/Unit.h', 'enum UnitState\n', True)
    method('ShortTimeTracker.inc', 'src/shared/Util/Timer.h', 'struct ShortTimeTracker\n', True)
    method('RandomDecl.inc', RANDOM_HEADER, 'class AbstractRandomMovementGenerator :', True)
    method('WanderDecl.inc', RANDOM_HEADER, 'class WanderMovementGenerator :', True)
    for name, sig in [('Initialize', 'void AbstractRandomMovementGenerator::Initialize('),
                      ('Finalize', 'void AbstractRandomMovementGenerator::Finalize('),
                      ('Interrupt', 'void AbstractRandomMovementGenerator::Interrupt('),
                      ('Reset', 'void AbstractRandomMovementGenerator::Reset('),
                      ('Update', 'bool AbstractRandomMovementGenerator::Update('),
                      ('SetLocation', 'int32 AbstractRandomMovementGenerator::_setLocation('),
                      ('WanderConstructor', 'WanderMovementGenerator::WanderMovementGenerator(const Creature& npc)'),
                      ('WanderFinalize', 'void WanderMovementGenerator::Finalize('),
                      ('WanderInterrupt', 'void WanderMovementGenerator::Interrupt('),
                      ('Pause', 'void WanderMovementGenerator::AddToRandomPauseTime(')]:
        method(name + '.inc', RANDOM, sig)
    for name, sig in [('ComputePathToRandomPoint.original', 'void PathFinder::ComputePathToRandomPoint('),
                      ('GetPathPolyByPosition', 'dtPolyRef PathFinder::getPathPolyByPosition('),
                      ('GetPolyByLocation', 'dtPolyRef PathFinder::getPolyByLocation('),
                      ('BuildShortcut', 'void PathFinder::BuildShortcut('),
                      ('NormalizePath', 'void PathFinder::NormalizePath('),
                      ('HaveTile', 'bool PathFinder::HaveTile(')]:
        method(name + '.inc', PATH, sig, compiled=not name.endswith('.original'))
    method('BuildPolyPath.context.inc', PATH, 'void PathFinder::BuildPolyPath(', compiled=False)
    method('PathFinderConstructor.context.inc', PATH, 'PathFinder::PathFinder(const Unit* owner,', compiled=False)
    original = excerpts['ComputePathToRandomPoint.original.inc']
    needle = '    float distanceToPoly;'
    assert original.count(needle) == 1
    patched = original.replace(needle, '    float distanceToPoly = 0.0f;', 1)
    excerpts['ComputePathToRandomPoint.patched.inc'] = patched
    patch = ''.join(difflib.unified_diff(original.splitlines(True), patched.splitlines(True),
                    fromfile='ComputePathToRandomPoint.original.inc', tofile='ComputePathToRandomPoint.patched.inc'))
    path_text = sources[PATH]
    prefix = interval('BuildPolyPath.prefix.inc', PATH, path_text.index('void PathFinder::BuildPolyPath('),
                      path_text.index('    // look for startPoly/endPoly in current path'))
    # The remaining branches are outside the admitted executable scope, never replacements.
    excerpts['BuildPolyPath.selected.inc'] = prefix + '    throw std::runtime_error("outside same-polygon oracle scope");\n}\n'
    constants = sources[HEADER]
    interval('SearchBounds.inc', HEADER, constants.index('static float NearPolySearchBound'), constants.index('\nenum PathType'))
    interval('Pi.inc', 'src/shared/Common.h', sources['src/shared/Common.h'].index('#ifndef M_PI_F'),
             sources['src/shared/Common.h'].index('#endif', sources['src/shared/Common.h'].index('#ifndef M_PI_F')) + 6)
    method('Clear.inc', HEADER, '        void clear()')
    return sources, excerpts, records, patch


def command(run, name, args, **kwargs):
    (run / (name + '.command.json')).write_text(json.dumps(args, indent=2) + '\n')
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)
    (run / (name + '.stdout')).write_bytes(result.stdout)
    (run / (name + '.stderr')).write_bytes(result.stderr)
    (run / (name + '.returncode')).write_text(str(result.returncode) + '\n')
    if result.returncode:
        raise RuntimeError(f'{name} failed ({result.returncode}); see {run}')
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--core', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--write-bundled', action='store_true')
    parser.add_argument('--check-bundled', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    number = 1
    while (args.output / f'{number:03}').exists():
        number += 1
    run = (args.output / f'{number:03}').resolve()
    run.mkdir()
    try:
        sources, excerpts, records, patch = extract(args.core)
        compiler = shutil.which('g++')
        if not compiler:
            raise RuntimeError('required g++ absent')
        version = command(run, 'compiler-version', [compiler, '-dumpfullversion']).decode().strip()
        target = command(run, 'compiler-target', [compiler, '-dumpmachine']).decode().strip()
        if version != '14.2.0' or target != 'x86_64-linux-gnu':
            raise RuntimeError('unsupported audited compiler profile')
        for name, text in excerpts.items():
            (run / name).write_text(text, encoding='latin1')
        (run / 'endpoint-zero.patch').write_text(patch)
        for name, text in sources.items():
            if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
                dest = run / name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(text, encoding='latin1')
        shutil.copyfile(HERE / 'oracle_driver.cpp', run / 'oracle_driver.cpp')
        detour = run / 'dep/recastnavigation/Detour'
        flags = ['-std=c++20', '-O2', '-fno-inline', '-march=x86-64', '-mno-fma',
                 '-fno-fast-math', '-ffp-contract=off', '-frounding-math', '-DNDEBUG',
                 '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror', '-Wno-class-memaccess',
                 '-Wno-deprecated-copy', '-Wno-unused-variable', '-fsanitize=undefined,float-cast-overflow',
                 '-fno-sanitize-recover=all']
        command(run, 'compile', [compiler, *flags, '-I' + str(detour / 'Include'),
                '-I' + str(run / 'dep/g3dlite'), str(run / 'oracle_driver.cpp'),
                *[str(detour / f'Source/{name}.cpp') for name in DETOUR], '-o', str(run / 'wander-oracle')])
        symbols = command(run, 'symbols', ['nm', '-C', str(run / 'wander-oracle')]).decode()
        for signature in ['PathFinder::ComputePathToRandomPoint(', 'AbstractRandomMovementGenerator::Update(',
                          'dtNavMeshQuery::findNearestPoly(', 'dtNavMeshQuery::getPolyHeight(',
                          'dtCreateNavMeshData(', 'G3D::Vector3::squaredMagnitude() const']:
            if signature not in symbols:
                raise RuntimeError('missing actual source symbol: ' + signature)
        observer_specs = {
            'precision-params.input': ('direct-corridor/params.test-fixture', '982c1ba0cc647668599a4f1ab17d2bf5612a9714c011d508ad18929884c6be2a'),
            'precision-tile.input': ('warm-cache/precision.test-fixture', '4a05137e1f45919c1f43e52f163af53d49a76b05ac1097310368b946c5d71cec'),
        }
        for name, (relative, digest) in observer_specs.items():
            data = (HERE.parents[1] / 'crates/benilla-native-nav/tools/fixtures' / relative).read_bytes()
            if sha(data) != digest:
                raise RuntimeError('unchanged precision fixture hash mismatch: ' + relative)
            (run / name).write_bytes(data)
        output = command(run, 'execute', [str(run / 'wander-oracle'), str(run),
                         str(run / 'precision-params.input'), str(run / 'precision-tile.input')])
        rows = [json.loads(line) for line in output.decode().splitlines()]
        vector = dict(schema=1, source_revision=PIN, compatibility_policy=POLICY,
                      geometry='original-authored-quad64 plus separately labeled unchanged precision fixture', snapshots=rows)
        encoded = json.dumps(vector, indent=2) + '\n'
        (run / 'vectors.json').write_text(encoded)
        geometry_files = {name: (run / name).read_bytes() for name in ['params.test-fixture', 'quad.test-fixture']}
        geometry_hashes = {name: sha(data) for name, data in geometry_files.items()}
        metadata = dict(source_revision=PIN, endpoint_compatibility_policy=POLICY,
                        geometry_fixtures_sha256=geometry_hashes,
                        unchanged_observer_fixtures=observer_specs,
                        original_undefined_endpoint_execution=False,
                        endpoint_original_sha256=sha(excerpts['ComputePathToRandomPoint.original.inc'].encode()),
                        endpoint_patched_sha256=sha(excerpts['ComputePathToRandomPoint.patched.inc'].encode()),
                        buildpolypath_cold_zero_policy_applied=False,
                        buildpolypath_distance_precondition='actual endpoint lookup appends retained ref; tail lookup writes distance before read',
                        compiler=version, target=target, source_intervals=records,
                        geometry='one authored numeric quad generated by pinned dtCreateNavMeshData; no install or DB data',
                        external_leaves=['owner state/lifecycle getters', 'RNG transcript values', 'MoveSplineInit launch duration and finalized observation', 'world normalization false', 'owner filter snapshot', 'navmesh pointer selection'],
                        excluded=['whole server', 'loaded world membership', 'world terrain callbacks', 'spline interpolation/clock', 'production admission', 'undefined C++ bit parity'],
                        tile_lifetime_profile='unchanged-original-empirical; existing raw-storage dtMeshTile lifetime caveat not repaired',
                        source_manifest_sha256=sha((HERE / 'source_hashes.sha256').read_bytes()))
        meta = json.dumps(metadata, indent=2) + '\n'
        (run / 'metadata.json').write_text(meta)
        package = '// SPDX-License-Identifier: GPL-3.0-or-later\n// CMaNGOS ' + PIN + '\n' + sources[PATH][:sources[PATH].index('*/') + 2] + '\n'
        for name, text in excerpts.items():
            package += '\n// EXCERPT ' + name + '\n' + text + '\n'
        bundled = {'vectors.json': encoded, 'source_intervals.json': meta,
                   'source_excerpts.test-fixture': package, 'endpoint-zero.patch': patch}
        hashes_text = ''.join(f'{digest}  fixtures/{name}\n' for name, digest in sorted(geometry_hashes.items()))
        bundled['geometry_hashes.sha256'] = hashes_text
        if args.write_bundled:
            (HERE / 'fixtures').mkdir(exist_ok=True)
            for name, data in geometry_files.items():
                (HERE / 'fixtures' / name).write_bytes(data)
            for name, text in bundled.items():
                (HERE / name).write_text(text)
        if args.check_bundled:
            for name, data in geometry_files.items():
                if (HERE / 'fixtures' / name).read_bytes() != data:
                    raise RuntimeError('bundled authored geometry differs: ' + name)
            for name, text in bundled.items():
                if (HERE / name).read_text() != text:
                    raise RuntimeError('bundled source result differs: ' + name)
        report = dict(snapshots=len(rows), cases=len({r['case'] for r in rows}),
                      sanitizer_stderr_bytes=(run / 'execute.stderr').stat().st_size,
                      result='passed source assertions and exact RNG transcript consumption', run=str(run))
        (run / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
        (run / 'manifest.json').write_text(json.dumps({p.relative_to(run).as_posix(): sha(p.read_bytes())
                    for p in sorted(run.rglob('*')) if p.is_file()}, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    except Exception as error:
        (run / 'FAILED.txt').write_text(str(error) + '\n')
        raise


if __name__ == '__main__':
    main()
