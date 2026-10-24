#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure the explicitly pinned GCC 14 uint16 unordered_set reference profile."""
import argparse
import hashlib
import os
import pathlib
import platform
import re
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = pathlib.Path(__file__).resolve().parent
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
EXTRACTOR = 'contrib/vmap_extractor/vmapextract/'
TARGET = HERE / 'fixtures/wmo_doodad_order_source'
COMPILER = pathlib.Path('/usr/bin/x86_64-linux-gnu-g++-14')
LIBSTDCXX = pathlib.Path('/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.33')
FLAGS = ['-std=c++17', '-O0', '-fno-strict-aliasing', '-Wall', '-Wextra', '-Werror']


def digest(data):
    return hashlib.sha256(data).hexdigest()


def manifest(name):
    result = {}
    for line in (HERE / name).read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        expected, path = line.split('  ', 1)
        if path in result or not re.fullmatch('[0-9a-f]{64}', expected):
            raise SystemExit(f'Invalid guard manifest: {name}')
        result[path] = expected
    if not result:
        raise SystemExit(f'Empty guard manifest: {name}')
    return result


def verify_manifest(name, root=None):
    result = {}
    for path, expected in manifest(name).items():
        actual = (root / path) if root else pathlib.Path(path)
        data = actual.read_bytes()
        if digest(data) != expected:
            raise SystemExit(f'Wrong pinned hash: {path}')
        result[path] = data
    return result


def sources(root):
    result = verify_manifest('wmo_doodad_order_source_hashes.sha256', root)
    head = subprocess.run(['git', '-C', str(root), 'rev-parse', 'HEAD'],
                          check=True, capture_output=True, text=True).stdout.strip()
    if head != PIN:
        raise SystemExit(f'Wrong CMaNGOS revision: {head}; expected {PIN}')
    return {path: data.decode('latin1') for path, data in result.items()}


def fragments(source):
    header = source[EXTRACTOR + 'wmo.h']
    platform_header = source['src/shared/Platform/Define.h']
    notice = header[:header.index('#ifndef')]
    aliases = '\n'.join(next(line for line in platform_header.splitlines()
                             if line == 'typedef std::' + name + '_t ' + name + ';')
                        for name in ['uint32', 'uint16']) + '\n'
    start = header.index('namespace WMO\n')
    end = header.index('\n}', start) + 2
    declarations = header[start:end] + '\n'
    start = header.index('struct WMODoodadData\n')
    end = header.index('\n};', start) + 3
    declarations += header[start:end] + '\n'
    exported = source[EXTRACTOR + 'vmapexport.cpp']
    swap = '\n'.join(next(line.strip() for line in exported.splitlines() if text in line)
                     for text in ['WMODoodadData& doodads = WmoDoodads[plain_name];',
                                  'std::swap(doodads, froot.DoodadData);'])
    insertion = next(line.strip() for line in exported.splitlines()
                     if 'doodads.References.insert(groupReference);' in line)
    iteration = next(line.strip() for line in source[EXTRACTOR + 'model.cpp'].splitlines()
                     if 'for (uint16 doodadIndex : doodadData.References)' in line)
    return {'types': notice + aliases + declarations, 'swap': swap, 'insert': insertion,
            'iteration': iteration}


def oracle_text(source):
    extracted = fragments(source)
    text = (HERE / 'wmo_doodad_order_oracle.cpp').read_text()
    for key in ['types', 'swap', 'insert', 'iteration']:
        text = text.replace('// PINNED_SOURCE_' + key.upper(), extracted[key])
    return text


def environment():
    # Search and loader overrides are outside this exact system profile.
    env = dict(os.environ)
    for key in ['CPATH', 'CPLUS_INCLUDE_PATH', 'C_INCLUDE_PATH', 'GCC_EXEC_PREFIX',
                'COMPILER_PATH', 'LIBRARY_PATH', 'LD_LIBRARY_PATH', 'LD_PRELOAD',
                'LD_AUDIT', 'DEPENDENCIES_OUTPUT', 'SUNPRO_DEPENDENCIES']:
        env.pop(key, None)
    env.update({'LC_ALL': 'C', 'LANG': 'C', 'PATH': '/usr/bin:/bin'})
    return env


def command(source_path, root):
    return [str(COMPILER), *FLAGS, '-I' + str(root / EXTRACTOR), str(source_path)]


def dependency_paths(source_path, root, env):
    output = subprocess.run([*command(source_path, root), '-M', '-MT', 'oracle'],
                            check=True, text=True, capture_output=True, env=env).stdout
    words = output.replace('\\\n', '').split(':', 1)[1].split()
    return sorted({pathlib.Path(word).resolve() for word in words})


def label(path, work, root):
    if path.is_relative_to(work):
        return '<generated>/' + str(path.relative_to(work))
    if path.is_relative_to(root):
        return '<cmangos>/' + str(path.relative_to(root))
    return str(path)


def linked_libraries(executable, env):
    output = subprocess.run(['/usr/bin/ldd', str(executable)], check=True,
                            text=True, capture_output=True, env=env).stdout
    paths = sorted({pathlib.Path(word).resolve() for word in output.split()
                    if word.startswith('/')})
    if LIBSTDCXX not in paths:
        raise SystemExit('Oracle is not linked to the pinned libstdc++ runtime')
    return paths


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little' or platform.machine() != 'x86_64' or platform.system() != 'Linux':
        raise SystemExit('Requires the documented Linux x86-64 little-endian GCC 14 profile')
    root = args.source.resolve()
    source = sources(root)
    pinned_headers = verify_manifest('wmo_doodad_order_header_hashes.sha256')
    pinned_tools = verify_manifest('wmo_doodad_order_toolchain_hashes.sha256')
    env = environment()
    compiler_version = subprocess.run([str(COMPILER), '--version'], check=True,
                                      text=True, capture_output=True, env=env).stdout.splitlines()[0]
    if compiler_version != 'x86_64-linux-gnu-g++-14 (Debian 14.2.0-19) 14.2.0':
        raise SystemExit('Unexpected compiler version: ' + compiler_version)
    with tempfile.TemporaryDirectory(prefix='.wmo-doodad-order-', dir=HERE) as temporary:
        work = pathlib.Path(temporary).resolve()
        authored = work / 'oracle.cpp'
        authored.write_text(oracle_text(source))
        dependencies = dependency_paths(authored, root, env)
        actual_headers = {str(path) for path in dependencies
                          if not path.is_relative_to(work) and not path.is_relative_to(root)}
        if actual_headers != set(pinned_headers):
            raise SystemExit('Header dependency closure differs: ' + repr(actual_headers ^ set(pinned_headers)))
        executable = work / 'oracle'
        subprocess.run([*command(authored, root), '-o', str(executable)], check=True, env=env)
        libraries = linked_libraries(executable, env)
        if not all(str(path) in pinned_tools for path in libraries):
            raise SystemExit('A runtime library is outside the pinned toolchain closure')
        symbols = subprocess.run(['/usr/bin/nm', '-D', '--undefined-only', str(executable)],
                                 check=True, text=True, capture_output=True, env=env).stdout
        prime_symbols = sorted(line.split()[-1] for line in symbols.splitlines()
                               if '_Prime_rehash_policy' in line)
        if not any('_M_need_rehash' in name and '@GLIBCXX_3.4.18' in name for name in prime_symbols):
            raise SystemExit('Oracle does not reference the pinned out-of-line prime-policy symbol')
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(executable), str(output)], check=True, env=env)
        products = {path.relative_to(output): path.read_bytes() for path in output.iterdir() if path.is_file()}
        extracted = fragments(source)
        source_fragments = ('\n'.join(extracted.values()) + '\n').encode()
        products[pathlib.Path('source-fragments.test-fixture')] = source_fragments
        rows = ['# kind\tpath\tbytes\tsha256']
        for path in dependencies:
            data = path.read_bytes()
            rows.append(f'header_dependency\t{label(path, work, root)}\t{len(data)}\t{digest(data)}')
        for path, data in sorted(pinned_tools.items()):
            rows.append(f'toolchain\t{path}\t{len(data)}\t{digest(data)}')
        for name, text in sorted(source.items()):
            data = text.encode('latin1')
            rows.append(f'cmangos_source\t{name}\t{len(data)}\t{digest(data)}')
        products[pathlib.Path('dependencies.tsv')] = ('\n'.join(rows) + '\n').encode()
        provenance = ['# item\tvalue', 'pin\t' + PIN,
                      'profile\tLinux-x86_64-little-endian-GCC14-libstdc++20250315-C++17',
                      'compiler\t' + compiler_version, 'compiler_flags\t' + ' '.join(FLAGS),
                      'libstdc++\t' + str(LIBSTDCXX), 'libstdc++_sha256\t' + digest(LIBSTDCXX.read_bytes()),
                      'libc\t' + os.confstr('CS_GNU_LIBC_VERSION'),
                      'container\tstd::unordered_set<uint16>; std::hash<uint16>; std::equal_to<uint16>; std::allocator<uint16>',
                      'lifecycle\toriginal-WMODoodadData-fresh-empty-composite-std::swap; insert-only; no-set-reserve-rehash-erase',
                      'load\tdefault-max_load_factor=1.0; identity-hash-checked-over-entire-u16-domain',
                      'growth\tactual-linked-prime-policy; all-65537-cardinalities; four-full-permutation-agreement-checks',
                      'out_of_line_prime_symbols\t' + ','.join(prime_symbols),
                      'shuffle\txorshift32-seed-0x6d2b79f5; shifts-13,17,5; descending-Fisher-Yates; uint32-wrap',
                      'affine\t(index*65521+12345)mod65536',
                      'small_permutations\tall-720-permutations-of-0,1,13,26,29,65535; every-prefix-recorded',
                      'offset_units\tu16-elements; binary-little-endian',
                      'authored_oracle_sha256\t' + digest((HERE / 'wmo_doodad_order_oracle.cpp').read_bytes()),
                      'compiled_source_sha256\t' + digest(authored.read_bytes()),
                      'source_fragments_sha256\t' + digest(source_fragments),
                      'executable_sha256\t' + digest(executable.read_bytes())]
        products[pathlib.Path('provenance.tsv')] = ('\n'.join(provenance) + '\n').encode()
        if args.check:
            existing = {path.relative_to(TARGET) for path in TARGET.rglob('*') if path.is_file()}
            if existing != set(products):
                raise SystemExit(f'Fixture file set differs: {existing ^ set(products)}')
            for name, data in products.items():
                if (TARGET / name).read_bytes() != data:
                    raise SystemExit(f'Fixture differs: {name}')
        else:
            TARGET.mkdir(parents=True, exist_ok=True)
            for name, data in products.items():
                (TARGET / name).write_bytes(data)
        print(f'{"Verified" if args.check else "Generated"} {len(products)} authored reference-container oracle files')


if __name__ == '__main__':
    main()
