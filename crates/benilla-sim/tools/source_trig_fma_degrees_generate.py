#!/usr/bin/env python3
"""Record pinned glibc FMA expectations for the existing ModelInstance degree grid."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from source_trig_fma_evidence import ASSIGNED, create_evidence

IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'
LIBM = '7f2ca87f652f56b094462474b076749e90e689d0ecb9cb63c7679820b271b4e7'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence', type=Path, help='New relative child directory inside f6/trig-fma')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    actual = subprocess.check_output(['docker', 'image', 'inspect', IMAGE, '--format', '{{.Id}}'], text=True).strip()
    if actual != IMAGE:
        raise SystemExit('Pinned image mismatch')
    available = shutil.disk_usage(ASSIGNED.parent).free
    if available < 4 * 1024**3:
        raise SystemExit('Less than 4 GiB free')
    evidence = create_evidence(args.evidence, ASSIGNED)
    root = Path(__file__).resolve().parents[3]
    tools = root / 'crates/benilla-sim/tools'
    fixtures = tools / 'fixtures/source_trig_fma'
    old = root / 'crates/benilla-native-geometry-tests/tools/fixtures/vmap_source'
    authored = (fixtures / 'degrees-inputs.txt').read_bytes()
    legacy = [line.split() for line in (old / 'trig-cases.tsv').read_text().splitlines() if not line.startswith('#')]
    inputs = authored + b''.join(row[0].encode() + b'\n' for row in legacy)
    base = ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--log-driver', 'none',
            '--user', f'{os.getuid()}:{os.getgid()}', '--env', 'TMPDIR=/evidence',
            '--mount', f'type=bind,src={root},dst=/source,readonly',
            '--mount', f'type=bind,src={evidence},dst=/evidence']
    library = subprocess.check_output(base + [IMAGE, 'sha256sum', '/lib/x86_64-linux-gnu/libm.so.6'], text=True).split()[0]
    if library != LIBM:
        raise SystemExit('Pinned libm mismatch')
    compile_command = base + [IMAGE, 'g++', '-std=c++17', '-O3', '-ffp-contract=off', '-fno-fast-math',
                              '/source/crates/benilla-sim/tools/source_trig_fma_degrees.cpp',
                              '-Wl,--no-as-needed', '-lm', '-ldl', '-o', '/evidence/degrees-oracle']
    subprocess.run(compile_command, check=True)
    commands = {
        'fma': base + [IMAGE, '/evidence/degrees-oracle'],
        'masked': base + ['--env', 'GLIBC_TUNABLES=glibc.cpu.hwcaps=-FMA,-AVX2', IMAGE, '/evidence/degrees-oracle'],
    }
    report = {'image': actual, 'libm_sha256': library, 'free_bytes_before': available,
              'compile_command': compile_command, 'commands': commands,
              'source_sha256': sha(tools / 'source_trig_fma_degrees.cpp'),
              'binary_sha256': sha(evidence / 'degrees-oracle')}
    outputs = ['degrees-oracle']
    for name, command in commands.items():
        vectors = subprocess.run(command[:2] + ['-i'] + command[2:], input=inputs, capture_output=True, check=True)
        for suffix, data in [('vectors.tsv', vectors.stdout), ('vectors.log', vectors.stderr)]:
            output = f'degrees-{name}-{suffix}'
            (evidence / output).write_bytes(data)
            outputs.append(output)
        info = json.loads(vectors.stderr.decode().splitlines()[-1])
        offsets = ('0x73720', '0x73280') if name == 'fma' else ('0x72810', '0x72250')
        if (info['sinf_offset'], info['cosf_offset']) != offsets:
            raise SystemExit('Unexpected dispatch')
        rows = [row.split() for row in vectors.stdout.decode().splitlines()]
        if len(rows) != 14 + len(legacy):
            raise SystemExit('Incomplete vector set')
        if [[row[0], row[2], row[3]] for row in rows[14:]] != legacy:
            raise SystemExit('An existing authored boundary expectation changed')
        report[name] = info
    hashes = {name: hashlib.sha256() for name in commands}
    count, sine_differences, cosine_differences = 0, 0, 0
    differences = []
    logs = {name: (evidence / f'degrees-{name}-grid.log').open('wb') for name in commands}
    processes = {name: subprocess.Popen(command + ['--grid'], stdout=subprocess.PIPE, stderr=logs[name])
                 for name, command in commands.items()}
    while True:
        blocks = {name: process.stdout.read(12 * 32768) for name, process in processes.items()}
        if len(blocks['fma']) != len(blocks['masked']) or len(blocks['fma']) % 12:
            raise SystemExit('Incomplete degree grid stream')
        if not blocks['fma']:
            break
        for name, block in blocks.items():
            hashes[name].update(block)
        for fma, masked in zip(struct.iter_unpack('<III', blocks['fma']), struct.iter_unpack('<III', blocks['masked'])):
            if fma[0] != masked[0]:
                raise SystemExit('Degree input order mismatch')
            sine_differences += fma[1] != masked[1]
            cosine_differences += fma[2] != masked[2]
            if fma != masked:
                differences.append({'degrees': f'{fma[0]:08x}', 'fma': [f'{v:08x}' for v in fma[1:]],
                                    'masked': [f'{v:08x}' for v in masked[1:]]})
            count += 1
    for name, process in processes.items():
        if process.wait() != 0:
            raise SystemExit(f'{name} degree grid failed')
        logs[name].close()
        outputs.append(f'degrees-{name}-grid.log')
    if count != 4_251_072:
        raise SystemExit('Degree grid count mismatch')
    if hashes['masked'].hexdigest() != (old / 'trig-grid.sha256').read_text().strip():
        raise SystemExit('Masked grid does not reproduce the existing unfused expectation')
    if not differences:
        raise SystemExit('Degree grid does not distinguish FMA')
    report.update({'grid_count': count, 'sine_differences': sine_differences,
                   'cosine_differences': cosine_differences, 'differences': differences,
                   'grid_sha256': {name: value.hexdigest() for name, value in hashes.items()},
                   'outputs_sha256': {name: sha(evidence / name) for name in outputs}})
    (evidence / 'degrees-receipt.json').write_text(json.dumps(report, indent=2) + '\n')
    expected = '\n'.join((evidence / 'degrees-fma-vectors.tsv').read_text().splitlines()[:14]) + '\n'
    for name, content in [('degrees.tsv', expected), ('degrees-grid.sha256', hashes['fma'].hexdigest() + '\n')]:
        path = fixtures / name
        if args.check:
            if path.read_text() != content:
                raise SystemExit(f'Degree fixture differs: {path}')
        else:
            path.write_text(content)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
