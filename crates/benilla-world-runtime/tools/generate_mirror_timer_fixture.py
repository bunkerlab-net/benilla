#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute the exact pinned CMaNGOS MirrorTimer and ShortIntervalTimer bodies.

Usage: generate_mirror_timer_fixture.py /path/to/mangos-classic [--check] [--sanitize]
Only authored timer operations are inputs; no world or damage integration is run.
"""
import argparse
import hashlib
import os
import pathlib
import shlex
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
PLAYER = 'src/game/Entities/Player.cpp'
HEADER = 'src/game/Entities/Player.h'
TIMER = 'src/shared/Util/Timer.h'
COMMON = 'src/shared/Common.h'
METHODS = (
    'MirrorTimer::Status MirrorTimer::FetchStatus()',
    'void MirrorTimer::Stop()',
    'void MirrorTimer::Start(uint32 interval, uint32 spellId/* = 0*/)',
    'void MirrorTimer::Start(uint32 current, uint32 max, uint32 spellId)',
    'void MirrorTimer::SetRemaining(uint32 duration)',
    'void MirrorTimer::SetDuration(uint32 duration)',
    'void MirrorTimer::SetFrozen(bool state)',
    'void MirrorTimer::SetScale(int32 scale)',
    'bool MirrorTimer::Update(uint32 diff)',
)


def block(text, signature, suffix=''):
    if text.count(signature) != 1:
        raise SystemExit(f'Expected one exact source signature: {signature}')
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    if text[end:end + len(suffix)] != suffix:
        raise SystemExit(f'Expected source suffix: {signature}')
    return text[start:end + len(suffix)]


def guarded(root):
    sources = {}
    for line in (HERE / 'mirror_timer_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        sources[name] = raw.decode('latin1')
    if set(sources) != {PLAYER, HEADER, TIMER, COMMON}:
        raise SystemExit('Unexpected MirrorTimer source dependency inventory')
    return sources


def source_harness(sources):
    replacements = {
        '// PINNED_TIME_CONSTANTS': block(sources[COMMON], 'enum TimeConstants\n', ';'),
        '// PINNED_SHORT_INTERVAL_TIMER': block(sources[TIMER], 'class ShortIntervalTimer\n', ';'),
        '// PINNED_MIRROR_TIMER': block(sources[HEADER], 'class MirrorTimer\n', ';'),
        '// PINNED_MIRROR_METHODS': '\n\n'.join(block(sources[PLAYER], method) for method in METHODS),
    }
    text = (HERE / 'mirror_timer_oracle.cpp').read_text()
    for marker, original in replacements.items():
        if text.count(marker) != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker, original)
    # Retain every extracted file's upstream copyright/license notice.
    notices = '\n'.join(source[:source.index('*/') + 2] for source in sources.values())
    return notices + '\n' + text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    args = parser.parse_args()
    sources = guarded(args.source.resolve())
    cpp = source_harness(sources)
    with tempfile.TemporaryDirectory(prefix='.mirror-timer-oracle-', dir=HERE) as directory:
        work = pathlib.Path(directory)
        source = work / 'oracle.cpp'
        source.write_text(cpp)
        program = work / 'oracle.test-fixture'
        compiler = shlex.split(os.environ.get('CXX', 'g++'))
        command = compiler + ['-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                              '-pedantic-errors', str(source), '-o', str(program)]
        if args.sanitize:
            command[len(compiler):len(compiler)] = [
                '-fsanitize=undefined', '-fno-sanitize-recover=all']
        print('Compile: ' + shlex.join(command), flush=True)
        subprocess.run(command, check=True)
        output = work / 'mirror-timer.tsv'
        subprocess.run([str(program), str(output)], check=True)
        data = output.read_bytes()
        destination = HERE / 'fixtures/mirror-timer.tsv'
        if args.check:
            if not destination.is_file() or destination.read_bytes() != data:
                raise SystemExit('Mirror timer fixture differs: mirror-timer.tsv')
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        print(f'{hashlib.sha256(data).hexdigest()}  mirror-timer.tsv')
        print(f'{"Verified" if args.check else "Generated"} exact-source MirrorTimer fixture')


if __name__ == '__main__':
    main()
