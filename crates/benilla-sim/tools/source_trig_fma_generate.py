#!/usr/bin/env python3
"""Gate the full binary32 FMA kernel against the pinned image's libm."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import urllib.request

IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'
LIBM = '7f2ca87f652f56b094462474b076749e90e689d0ecb9cb63c7679820b271b4e7'
PARENT = Path('/home/robbie/Work/benilla/benilla-config/full-server/f6')
FILES = ('s_sinf.c', 's_cosf.c', 's_sincosf.h', 'sincosf_poly.h', 's_sincosf_data.c')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--vectors-only', action='store_true')
    args = parser.parse_args()
    actual = subprocess.check_output(['docker', 'image', 'inspect', IMAGE, '--format', '{{.Id}}'], text=True).strip()
    if actual != IMAGE:
        raise SystemExit('Pinned image identity mismatch')
    root = args.evidence.resolve()
    if root.parent != PARENT or root.name != 'trig-fma':
        raise SystemExit('Output must be the assigned main-checkout f6/trig-fma directory')
    available = shutil.disk_usage(PARENT).free
    if available < 4 * 1024**3:
        raise SystemExit('Less than 4 GiB free')
    root.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).resolve().parents[3]
    tools = source / 'crates/benilla-sim/tools'
    base = ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--log-driver', 'none',
            '--user', f'{os.getuid()}:{os.getgid()}', '--env', 'TMPDIR=/evidence',
            '--mount', f'type=bind,src={source},dst=/source,readonly',
            '--mount', f'type=bind,src={root},dst=/evidence']
    library = subprocess.check_output(base + [IMAGE, 'sha256sum', '/lib/x86_64-linux-gnu/libm.so.6'], text=True).split()[0]
    if library != LIBM:
        raise SystemExit('Pinned libm hash mismatch')
    source_hashes = {}
    for name in FILES:
        url = f'https://raw.githubusercontent.com/bminor/glibc/glibc-2.36/sysdeps/ieee754/flt-32/{name}'
        with urllib.request.urlopen(url) as response:
            (root / name).write_bytes(response.read())
        source_hashes[name] = digest(root / name)
    production = sorted((source / 'crates/benilla-sim/src/source_trig_fma').glob('*.rs'))
    production_hashes = {str(p.relative_to(source)): digest(p) for p in production}
    rust = ['rustc', '--crate-type', 'cdylib', '--edition', '2024', '-C', 'opt-level=3',
            '-C', 'target-feature=+fma', str(tools / 'source_trig_fma_port.rs'), '-o', str(root / 'libport.so')]
    cpp = base + [IMAGE, 'g++', '-std=c++17', '-O3', '-ffp-contract=off', '-fno-fast-math',
                  '/source/crates/benilla-sim/tools/source_trig_fma_oracle.cpp', '-Wl,--no-as-needed',
                  '-lm', '-ldl', '-o', '/evidence/oracle']
    subprocess.run(rust, check=True)
    subprocess.run(cpp, check=True)
    report = {'image': actual, 'libm_sha256': library, 'free_bytes_before': available,
              'rust_compile': rust, 'oracle_compile': cpp,
              'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'source_sha256': source_hashes, 'stream_layout': 'For each u32 input ascending: oracle sine, oracle cosine, Rust sine, Rust cosine as four little-endian u32 words; streamed, not retained.',
              'native_adapter': 'Exact production generic core; explicit f64 mul_add, target-feature=+fma. Software adapter uses the same operations through rustc_apfloat.'}
    inputs = (tools / 'fixtures/source_trig_fma/inputs.txt').read_bytes()
    (root / 'inputs.txt').write_bytes(inputs)
    for name, env, offsets in [('fma', [], ('0x73720', '0x73280')),
                               ('masked', ['--env', 'GLIBC_TUNABLES=glibc.cpu.hwcaps=-FMA,-AVX2'], ('0x72810', '0x72250'))]:
        command = base + ['-i'] + env + [IMAGE, '/evidence/oracle']
        vectors = subprocess.run(command, input=inputs, capture_output=True, check=True)
        (root / f'{name}-vectors.tsv').write_bytes(vectors.stdout)
        (root / f'{name}-vectors.log').write_bytes(vectors.stderr)
        if name == 'fma' and vectors.stdout != (tools / 'fixtures/source_trig_fma/expected.tsv').read_bytes():
            raise SystemExit('Authored FMA vectors differ from committed expectations')
        for function, address in zip(('sinf', 'cosf'), offsets):
            disassembly = subprocess.check_output(base + [IMAGE, 'objdump', '-d', f'--start-address={address}',
                                                       f'--stop-address={int(address, 16) + 0x240}', '/lib/x86_64-linux-gnu/libm.so.6'])
            (root / f'{name}-{function}.disassembly').write_bytes(disassembly)
        if args.vectors_only:
            result = json.loads(vectors.stderr.decode().splitlines()[-1])
        else:
            command = base + env + [IMAGE, '/evidence/oracle', '--exhaustive']
            stream = hashlib.sha256()
            size = 0
            with (root / f'{name}-exhaustive.log').open('wb') as log:
                process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=log)
                while block := process.stdout.read(4 * 1024**2):
                    stream.update(block)
                    size += len(block)
                if process.wait() != 0:
                    raise SystemExit(f'{name} oracle failed')
            result = json.loads((root / f'{name}-exhaustive.log').read_text().splitlines()[-1])
            if result['count'] != 2**32 or size != 16 * 2**32:
                raise SystemExit('Incomplete exhaustive stream')
            result.update({'stream_sha256': stream.hexdigest(), 'stream_bytes': size})
        if (result['sinf_offset'], result['cosf_offset']) != offsets:
            raise SystemExit(f'{name} dispatch mismatch')
        report[name] = result
        print(name, json.dumps(result), flush=True)
    report['exhaustive'] = not args.vectors_only
    report['outputs_scope'] = 'Oracle outputs only; unrelated concurrent runtime verification logs are excluded.'
    report['outputs_sha256'] = {p.name: digest(p) for p in sorted(root.iterdir())
                               if p.is_file() and p.name != 'receipt.json' and not p.name.startswith('runtime-')}
    if production_hashes != {str(p.relative_to(source)): digest(p) for p in production}:
        raise SystemExit('Production source changed during the gate')
    report['production_source_sha256'] = production_hashes
    (root / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n')
    if report['fma']['sin_differences'] or report['fma']['cos_differences']:
        raise SystemExit('FMA gate failed')
    if report['masked']['sin_differences'] + report['masked']['cos_differences'] == 0:
        raise SystemExit('Masked variant failed to distinguish this port')


if __name__ == '__main__':
    main()
