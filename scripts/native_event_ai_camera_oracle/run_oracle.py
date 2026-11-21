#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pinned unchanged CMaNGOS arithmetic, independently compared with binary32 model.

No Cargo/rustc, runtime worktree writes, source downloads, game data, or membership
inference. Each run gets a new directory; failed compiler/execution logs persist.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
SHA = '410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b'
REVISION = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def bits(number):
    return struct.unpack('<I', struct.pack('<f', number))[0]


def value(word):
    return struct.unpack('<f', struct.pack('<I', word))[0]


def f32(number):
    return value(bits(number))


def full_method(text, signature):
    start = text.index(signature)
    brace = text.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end] + '\n', text[:start].count('\n') + 1, text[:end].count('\n') + 1


def inputs():
    out = []
    def add(name, body=(0., 0., 0.), speaker=(0., 0., 0.), body_reach=1.5,
            speaker_reach=1.5, say=25., visibility=100.):
        out.append(dict(name=name, body_position_bits=list(map(bits, body)),
                        speaker_position_bits=list(map(bits, speaker)),
                        body_reach_bits=bits(body_reach), speaker_reach_bits=bits(speaker_reach),
                        say_range_bits=bits(say), visibility_bits=bits(visibility)))
    for visibility_label, visibility in [
        ('below200', value(bits(200.) - 1)), ('at200', 200.),
        ('above200', value(bits(200.) + 1)), ('below400', value(bits(400.) - 1)),
        ('at400', 400.), ('above400', value(bits(400.) + 1)),
    ]:
        multiplier = 4 if visibility >= 400 else 2 if visibility >= 200 else 1
        threshold = 25. * multiplier + 3.
        for side, offset in [('below', -1), ('equal', 0), ('above', 1)]:
            add(f'{visibility_label}_z_threshold_{side}', body=(0., 0., value(bits(threshold) + offset)), visibility=visibility)
    add('both_reaches_extend_range', body=(26., 0., 0.))
    add('no_reaches_do_not_extend_range', body=(26., 0., 0.), body_reach=0., speaker_reach=0.)
    add('sum_reaches_before_range', body=(25., 0., 0.), body_reach=value(0x35800000), speaker_reach=value(0x35800000))
    add('zero_range_zero_distance_strict', say=0., body_reach=0., speaker_reach=0.)
    add('zero_range_positive_reach_inside', body=(0., 0., 0.), say=0., body_reach=1., speaker_reach=0.)
    add('zero_range_positive_reach_equal', body=(0., 0., 1.), say=0., body_reach=1., speaker_reach=0.)
    for label, offset in [('below', -1), ('equal', 0), ('above', 1)]:
        add(f'three_axis_3_4_12_range_{label}', body=(3., 4., 12.), body_reach=0., speaker_reach=0., say=value(bits(13.) + offset))
    add('xy_then_z_association', body=(1., 2.**-12, 2.**-12), body_reach=0., speaker_reach=0., say=1.)
    add('z_is_not_discarded', body=(0., 0., 29.))
    add('signed_xyz_and_translated_origin', body=(32760., 1.25, -32760.), speaker=(32759., -1.75, -32750.), say=8.)
    add('signed_zero_strict', body=(-0., 0., -0.), speaker=(0., -0., 0.), say=0., body_reach=0., speaker_reach=0.)
    add('body_threshold_exceeds_cell_cap', body=(550., 0., 0.), say=600., body_reach=0., speaker_reach=0.)
    for say in [value(0x3eaaaaab), value(0x3f800001), value(0x41c7ffff), value(0x41c80001)]:
        for visibility in [100., 200., 400.]:
            add(f'range_bits_{bits(say):08x}_visibility_{int(visibility)}', say=say, visibility=visibility)
    # A deterministic broad numeric supplement; compact edge vectors remain separate.
    randomizer = random.Random(0x43414d455241)
    broad = []
    for index in range(2048):
        speaker = [f32(randomizer.uniform(-30000., 30000.)) for _ in range(3)]
        body = [f32(origin + randomizer.uniform(-100., 100.)) for origin in speaker]
        add(f'seeded_{index:04d}', body=body, speaker=speaker,
            body_reach=f32(randomizer.uniform(0., 5.)),
            speaker_reach=f32(randomizer.uniform(0., 5.)),
            say=f32(randomizer.uniform(0., 150.)),
            visibility=randomizer.choice([100., value(bits(200.)-1), 200., value(bits(400.)-1), 400.]))
        broad.append(out.pop())
    return out, broad


def runtime_formula(row):
    """Independent step-rounded transcription of the inspected Rust expressions.

    Python binary64 exactly represents products/sums of these binary32 inputs
    before each explicit pack rounds to binary32. Inputs avoid overflow, NaNs,
    subnormals and double-rounding-sensitive underflow; no fused operations.
    """
    body = list(map(value, row['body_position_bits']))
    speaker = list(map(value, row['speaker_position_bits']))
    dx, dy, dz = [f32(a-b) for a, b in zip(body, speaker)]
    xx, yy, zz = f32(dx*dx), f32(dy*dy), f32(dz*dz)
    xy = f32(xx+yy)
    squared = f32(xy+zz)
    reach = f32(value(row['body_reach_bits'])+value(row['speaker_reach_bits']))
    visibility = value(row['visibility_bits'])
    multiplier = 4. if visibility >= 400. else 2. if visibility >= 200. else 1.
    effective = f32(value(row['say_range_bits'])*multiplier)
    maximum = f32(effective+reach)
    maximum_squared = f32(maximum*maximum)
    return dict(expected_effective_range_bits=bits(effective),
                expected_distance_squared_bits=bits(squared),
                expected_delivered=squared < maximum_squared)


def record_command(run, name, command, **kwargs):
    (run / (name + '.command.json')).write_text(json.dumps(command, indent=2)+'\n')
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)
    (run / (name + '.stdout')).write_bytes(result.stdout)
    (run / (name + '.stderr')).write_bytes(result.stderr)
    (run / (name + '.returncode')).write_text(str(result.returncode)+'\n')
    if result.returncode:
        raise RuntimeError(f'{name} failed ({result.returncode}); see {run}')
    return result.stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--compiler', default='g++')
    parser.add_argument('--core', type=Path, required=True, help='pinned CMaNGOS checkout root')
    parser.add_argument('--runtime-source', type=Path, required=True, help='audience.rs to inspect without compiling')
    parser.add_argument('--output', type=Path, required=True, help='verification output directory; new runs never overwrite old ones')
    args = parser.parse_args()
    source_file = args.core.resolve() / 'src/game/Entities/Object.cpp'
    runtime_file = args.runtime_source.resolve()
    output_root = args.output.resolve()
    output_root.mkdir(parents=True, exist_ok=True)
    runs = output_root / 'runs'
    runs.mkdir(parents=True, exist_ok=True)
    number = 1
    while (runs / f'{number:03d}').exists():
        number += 1
    run = runs / f'{number:03d}'
    run.mkdir()
    try:
        source_bytes = source_file.read_bytes()
        if sha(source_bytes) != SHA:
            raise RuntimeError('pinned Object.cpp SHA-256 mismatch')
        source = source_bytes.decode('utf-8')
        distance, first, last = full_method(source, 'float WorldObject::GetDistance(const WorldObject* obj, bool is3D, DistanceCalculation distcalc) const')
        within, first2, last2 = full_method(source, 'bool WorldObject::_IsWithinDist(WorldObject const* obj, float dist2compare, bool is3D) const')
        start = source.index('            float sayRange = sWorld.getConfig(CONFIG_FLOAT_LISTEN_RANGE_SAY);', source.index('void WorldObject::MonsterText('))
        end = source.index('            _DoLocalizedTextAround(', start)
        snippet = source[start:end]
        copyright_header = source[:source.index('*/')+2]
        # The two complete method definitions and range statement snippet retain
        # their original bytes, including indentation/comments, unchanged.
        extracted = copyright_header + '\n\n' + distance + '\n' + within + '\nfloat WorldObject::ExtractedSayRange() const\n{\n' + snippet + '    return sayRange;\n}\n'
        (run/'source-excerpts.inc').write_text(extracted)
        shutil.copyfile(ROOT/'oracle_driver.cpp', run/'oracle_driver.cpp')
        shutil.copyfile(Path(__file__), run/'run_oracle.py')
        runtime_bytes = runtime_file.read_bytes()
        runtime = runtime_bytes.decode()
        required = [
            'let dx = body.x.as_f32() - speaker.x.as_f32();',
            'let dy = body.y.as_f32() - speaker.y.as_f32();',
            'let dz = body.z.as_f32() - speaker.z.as_f32();',
            'let xy = dx * dx + dy * dy;', 'xy + dz * dz\n    }',
            'let size_factor = self.body_combat_reach.as_f32() + self.speaker_combat_reach.as_f32();',
            'let maximum = self.effective_say_range()?.as_f32() + size_factor;',
            'Ok(distance_squared < maximum * maximum)',
            'let multiplier = if visibility >= 400.0 {\n            4.0\n        } else if visibility >= 200.0 {\n            2.0\n        } else {\n            1.0\n        };',
            'Real::new(self.say_range.as_f32() * multiplier)?',
        ]
        for expression in required:
            if expression not in runtime:
                raise RuntimeError('runtime formula changed: '+expression)
        (run/'runtime-formula-expressions.txt').write_text('\n'.join(required)+'\n')
        metadata = dict(source_revision=REVISION, source_file=str(source_file), source_sha256=SHA,
            methods=[dict(name='GetDistance(WorldObject*,bool,DistanceCalculation)', first_line=first,last_line=last,sha256=sha(distance.encode())),
                     dict(name='_IsWithinDist',first_line=first2,last_line=last2,sha256=sha(within.encode()))],
            range_snippet=dict(first_line=source[:start].count('\n')+1,last_line=source[:end].count('\n'),sha256=sha(snippet.encode())),
            runtime_file=str(runtime_file),runtime_sha256=sha(runtime_bytes),compiler=args.compiler,
            receiver_contract='camera body receiver, speaker argument, is3D=true, DIST_CALC_NONE; config and visibility getters return supplied binary32 values',
            exclusions=['Rust binary execution','loaded-cell membership','full camera traversal','production/native admission'])
        (run/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
        record_command(run,'compiler-version',[args.compiler,'--version'])
        flags = ['-std=c++17','-O2','-Wall','-Wextra','-Werror','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all']
        record_command(run,'compile',[args.compiler,*flags,str(run/'oracle_driver.cpp'),'-o',str(run/'camera-oracle')])
        edges, broad = inputs()
        cases = edges+broad
        lines=[]
        for row in cases:
            words=row['body_position_bits']+row['speaker_position_bits']+[row[key] for key in ['body_reach_bits','speaker_reach_bits','say_range_bits','visibility_bits']]
            lines.append(row['name']+' '+' '.join(f'{word:08x}' for word in words))
        input_bytes=('\n'.join(lines)+'\n').encode()
        (run/'inputs.txt').write_bytes(input_bytes)
        output=record_command(run,'execute',[str(run/'camera-oracle')],input=input_bytes)
        results=output.decode().splitlines()
        if len(results)!=len(cases):
            raise RuntimeError('oracle output cardinality mismatch')
        failures=[]
        for row,line in zip(cases,results):
            name,effective,squared,delivered=line.split()
            if name!=row['name']:
                raise RuntimeError('oracle output identity/order mismatch')
            actual=dict(expected_effective_range_bits=int(effective,16),expected_distance_squared_bits=int(squared,16),expected_delivered=bool(int(delivered)))
            model=runtime_formula(row)
            if actual!=model:
                failures.append(dict(name=name,source=actual,python_runtime_formula=model))
            row.update(actual)
        (run/'vectors.json').write_text(json.dumps(dict(schema=1,source_revision=REVISION,source_sha256=SHA,cases=edges),indent=2)+'\n')
        (run/'broad-vectors.json').write_text(json.dumps(dict(schema=1,cases=broad),separators=(',',':'))+'\n')
        (run/'comparison-failures.json').write_text(json.dumps(failures,indent=2)+'\n')
        # Mutant calculations explain why specific edge vectors are discriminating.
        witnesses={}
        row=next(r for r in edges if r['name']=='sum_reaches_before_range')
        effective=value(row['expected_effective_range_bits'])
        sequential=f32(f32(effective+value(row['body_reach_bits']))+value(row['speaker_reach_bits']))
        witnesses['reaches_added_sequentially']=dict(source_delivered=row['expected_delivered'],mutant_delivered=value(row['expected_distance_squared_bits'])<f32(sequential*sequential))
        witnesses['yz_grouped_first']=dict(source_squared_bits=bits(f32(f32(1.+2.**-24)+2.**-24)),mutant_squared_bits=bits(f32(1.+f32(2.**-24+2.**-24))))
        report=dict(edge_vectors=len(edges),broad_vectors=len(broad),total=len(cases),differences=len(failures),sanitizer_stderr_bytes=(run/'execute.stderr').stat().st_size,mutant_witnesses=witnesses,
                    conclusion='Pinned unchanged C++ source agrees with independently step-rounded binary32 transcription of the inspected Rust formula. No compiled Rust or loaded-cell membership claim.')
        (run/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
        if failures:
            raise RuntimeError(f'{len(failures)} numerical mismatches')
        manifest = {path.name: dict(sha256=sha(path.read_bytes()), bytes=path.stat().st_size)
                    for path in sorted(run.iterdir()) if path.is_file()}
        (run/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
        (output_root/'latest-run.txt').write_text(str(run.relative_to(output_root))+'\n')
        print(json.dumps(dict(run=str(run),**report),indent=2))
    except Exception as error:
        (run/'FAILED.txt').write_text(str(error)+'\n')
        raise


if __name__=='__main__':
    main()
