#!/usr/bin/env python3
"""Record authored binary64 sine outputs in the approved pinned image."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--already-compiled', action='store_true')
    args = parser.parse_args()
    root = args.evidence.resolve()
    source = Path(__file__).resolve().parents[3]
    actual = subprocess.check_output(['docker', 'image', 'inspect', IMAGE, '--format', '{{.Id}}'], text=True).strip()
    if actual != IMAGE:
        raise SystemExit('Pinned image identity mismatch')
    available = int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:'))) * 1024
    if available < 4 * 1024**3:
        raise SystemExit(f'Insufficient available memory: {available}')
    root.mkdir(parents=True, exist_ok=True)
    fixture = source / 'crates/benilla-sim/tools/fixtures'
    authored = (fixture / 'source_sine/inputs.txt').read_text().splitlines()
    moon = [record for line in (fixture / 'source_moon/expected.jsonl').read_text().splitlines()
            for record in json.loads(line)['records'] if record[0] == 'sine']
    (root / 'inputs.txt').write_text('\n'.join(authored + [record[1] for record in moon]) + '\n')
    base = ['docker', 'run', '--rm', '--network', 'none', '--user', f'{os.getuid()}:{os.getgid()}',
            '--mount', f'type=bind,src={source},dst=/source,readonly',
            '--mount', f'type=bind,src={root},dst=/evidence']
    compile_command = base + [IMAGE, 'g++', '-std=c++17', '-O3', '-DNDEBUG', '-ffp-contract=off', '-fno-fast-math', '-fno-omit-frame-pointer', '-g', '/source/crates/benilla-sim/tools/source_sine_oracle.cpp', '-ldl', '-o', '/evidence/oracle']
    if not args.already_compiled:
        subprocess.run(compile_command, check=True)
    symbols = {}
    for name, env in [('default', []), ('non_fma', ['-e', 'GLIBC_TUNABLES=glibc.cpu.hwcaps=-FMA,-AVX2'])]:
        command = base + ['-i'] + env + [IMAGE, '/evidence/oracle']
        result = subprocess.run(command, input=(root / 'inputs.txt').read_bytes(), capture_output=True, check=True)
        (root / f'{name}.tsv').write_bytes(result.stdout)
        (root / f'{name}-symbols.txt').write_bytes(result.stderr)
        fields = result.stderr.decode().split()
        symbols[name] = {'function': fields[0], 'library': fields[1], 'offset': fields[2]}
        address = int(fields[2], 16)
        disassembly = subprocess.check_output(base + [IMAGE, 'objdump', '-d', f'--start-address={address}', f'--stop-address={address + 4096}', fields[1]])
        (root / f'{name}.disassembly').write_bytes(disassembly)
    library_hash = subprocess.check_output(base + [IMAGE, 'sha256sum', '/lib/x86_64-linux-gnu/libm.so.6'], text=True).split()[0]
    for name, command in [
        ('branred.disassembly', ['objdump', '-d', '--start-address=0x66b80', '--stop-address=0x67400', '/lib/x86_64-linux-gnu/libm.so.6']),
        ('sincostab.words', ['objdump', '-s', '--start-address=0xa9d40', '--stop-address=0xaab00', '/lib/x86_64-linux-gnu/libm.so.6']),
    ]:
        (root / name).write_bytes(subprocess.check_output(base + [IMAGE] + command))
    report = {'image': actual, 'compile_command': compile_command, 'binary_sha256': hashlib.sha256((root / 'oracle').read_bytes()).hexdigest(), 'libm_sha256': library_hash, 'symbols': symbols, 'available_bytes': available}
    default = (root / 'default.tsv').read_text().splitlines()
    non_fma = (root / 'non_fma.tsv').read_text().splitlines()
    if len(default) != len(non_fma) or len(default) != len(authored) + len(moon):
        raise SystemExit('Oracle count mismatch')
    moon_matches = sum(row.split() == record[1:] for row, record in zip(default[len(authored):], moon))
    if moon_matches != len(moon):
        raise SystemExit('Dispatched sine differs from registered moon observations')
    report.update({
        'authored_cases': len(authored),
        'moon_sine_calls': len(moon),
        'moon_matches': moon_matches,
        'authored_variant_differences': sum(a != b for a, b in zip(default[:len(authored)], non_fma[:len(authored)])),
        'moon_variant_differences': sum(a != b for a, b in zip(default[len(authored):], non_fma[len(authored):])),
        'input_sha256': hashlib.sha256((root / 'inputs.txt').read_bytes()).hexdigest(),
        'default_sha256': hashlib.sha256((root / 'default.tsv').read_bytes()).hexdigest(),
        'non_fma_sha256': hashlib.sha256((root / 'non_fma.tsv').read_bytes()).hexdigest(),
        'historical_scope': 'Current same-host probe only; historical dispatch requires actual acquisition-bound round-14-or-later receipt.',
    })
    (root / 'oracle-receipt.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
