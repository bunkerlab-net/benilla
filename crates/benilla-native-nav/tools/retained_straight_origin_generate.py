#!/usr/bin/env python3
"""Independent same-object cold-policy origin to original retained straight observations."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import struct
import subprocess
import sys

sys.dont_write_bytecode = True
import cold_same_poly_generate as cold_same
import straight_path_generate as straight

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
FIXTURES = HERE / 'fixtures/retained-straight-origin'
PACKAGE = HERE / 'retained_straight_origin_source_fragments.test-fixture'
PROVENANCE = HERE / 'retained_straight_origin_provenance.json'
FILES = ('cases.test-fixture', 'traces.test-fixture')
FIELDS = ('label sequence step oracle query_nodes include_filter exclude_filter point_limit '
          'after_start_storage').split() + straight.FIELDS[1:]
TRACE_FIELDS = straight.TRACE_FIELDS
MIN_FREE = 1342177280  # Explicit approved launch/run floor: 1.25 GiB, not MBX's GC trigger.


def sha(data):
    return hashlib.sha256(data).hexdigest()


def replace_once(text, marker, value):
    if text.count(marker) != 1:
        raise SystemExit('Origin insertion drift: ' + marker)
    return text.replace(marker, value, 1)


def build_harness(sources):
    harness, straight_package = straight.build_harness(sources)
    if straight_package.encode('latin1') != straight.PACKAGE.read_bytes():
        raise SystemExit('Existing guarded straight source package differs')
    cold_harness, cold_package = cold_same.build_harness(sources)
    if cold_package.encode('utf-8') != cold_same.PACKAGE.read_bytes():
        raise SystemExit('Existing guarded cold source package differs')
    old_main = straight.exact_block(harness, 'int main(int argc,char** argv)')
    dormant = old_main.replace('int main(', '[[maybe_unused]] static int retained_original_fixture_main(', 1)
    dormant = dormant[:-1] + '    return 0;\n}'
    harness = replace_once(harness, old_main, dormant)
    declarations = '\n'.join(straight.exact_block(cold_harness, name) + ';'
                             for name in ('struct ConstructorCache', 'struct ColdEvidence'))
    harness = replace_once(harness, 'class PathFinder {', declarations + '\nclass PathFinder {')
    methods = ('    void compatibility_same_polygon(const Vector3&,const Vector3&,ColdEvidence&);\n'
               '    const char* original_retained_straight(const Vector3&,const Vector3&,ColdEvidence&);\n')
    harness = replace_once(harness, '    void BuildPointPath(const float*,const float*);',
                           methods + '    void BuildPointPath(const float*,const float*);')
    source = sources[straight.CPP]
    prefix = straight.source_span(source, cold_same.cold.ORIGINAL, '    dtStatus dtResult;')
    far = straight.source_span(source, '    bool farFromPoly = ', '    if (farFromPoly)')
    if source[:source.index(cold_same.cold.ORIGINAL)].count('\n') + 1 != 410:
        raise SystemExit('Original caller-prefix location drift')
    instrumented = replace_once(prefix, '    dtPolyRef endPoly =',
                                 '    evidence.afterStart = m_pathPolyRefs;\n    dtPolyRef endPoly =')
    extension = (HERE / 'retained_straight_origin_oracle.cpp').read_text()
    for marker, content in (('// ORIGINAL_RETAINED_ENDPOINT_PREFIX', instrumented),
                            ('// ORIGINAL_FAR_PREDICATE', far),
                            ('# CASE_HEADER', '# ' + '\\t'.join(FIELDS)),
                            ('# TRACE_HEADER', '# ' + '\\t'.join(TRACE_FIELDS))):
        extension = replace_once(extension, marker, content)
    cold_method = straight.exact_block(cold_harness, 'void PathFinder::compatibility_same_polygon(')
    harness += '\n' + cold_method + '\n' + extension
    package = ('// SPDX-License-Identifier: GPL-3.0-or-later\n// CMaNGOS ' + straight.PIN + '\n'
               '// Existing straight/cold packages and licenses are unchanged and hash-bound by provenance.\n'
               '// Original uninitialized cold prefix is review-only; only selected +0 cold policy executes.\n'
               '// ORIGINAL CONSTRUCTOR CACHE PROJECTION\n' + declarations + '\n'
               '// COLD COMPATIBILITY METHOD (existing guarded projection, unchanged)\n' + cold_method + '\n'
               '// ORIGINAL DEFINED RETAINED PREFIX\n' + prefix + '\n'
               '// ORIGINAL FAR PREDICATE\n' + far)
    spans = {
        'constructor_cache': {'source': straight.CPP + ':37-39; PathFinder.h:115-120', 'sha256': sha(declarations.encode('latin1'))},
        'retained_prefix': {'source': straight.CPP + ':410-415', 'sha256': sha(prefix.encode('latin1'))},
        'far_predicate': {'source': straight.CPP + ':452', 'sha256': sha(far.encode('latin1'))},
        'cold_method': {'source': 'existing cold_same_poly source projection', 'sha256': sha(cold_method.encode('latin1'))},
    }
    for signature, line in straight.METHODS:
        body = straight.exact_block(source, signature)
        spans[signature] = {'source': straight.CPP + ':' + str(line), 'sha256': sha(body.encode('latin1'))}
    suffix = straight.source_span(source, '    // start and end are on same polygon',
                                  '\nvoid PathFinder::BuildPointPath(').rstrip()[:-1]
    spans['selected_suffix'] = {'source': straight.CPP + ':506-792', 'sha256': sha(suffix.encode('latin1'))}
    return harness, package.rstrip() + '\n', spans


def guard_sources(root):
    sources = straight.guarded_sources(root)
    # Verify both existing source manifests, not merely their shared pin label.
    cold_sources = cold_same.point.guarded_sources(root)
    if any(sources[name] != content for name, content in cold_sources.items()):
        raise SystemExit('Cold and straight pinned source inputs differ')
    straight.guard_inputs()
    cold_same.guard_inputs()
    return sources


def verify_products(result):
    if set(result) != set(FILES):
        raise SystemExit('Unexpected origin products')
    rows = straight.parse_table(result[FILES[0]], FIELDS)
    # Reuse the complete original straight schema/numeric-domain validator.
    projected = ('# ' + '\t'.join(straight.FIELDS) + '\n' + '\n'.join(
        '\t'.join(row[key] for key in straight.FIELDS) for row in rows) + '\n').encode('ascii')
    straight.parse_cases(projected)
    expected = [('normal', 0, 'cold'), ('normal', 1, 'no_hit'),
                ('incomplete', 0, 'cold'), ('incomplete', 1, 'hit'), ('incomplete', 2, 'no_hit')]
    if [(r['sequence'], r['step'], r['label']) for r in rows] != [
            (name, str(step), name + '_' + suffix) for name, step, suffix in expected]:
        raise SystemExit('Missing or reordered origin sequence')
    previous = {}
    for row in rows:
        try:
            if tuple(row[k] for k in ('query_nodes', 'include_filter', 'exclude_filter', 'point_limit')) != ('1024', '13', '0', '74'):
                raise ValueError()
            if any(row[k] != '0' for k in ('optimize', 'normalize', 'ignore', 'force')):
                raise ValueError()
            if row['collision_width'] != '3e000000' or row['geometry'] != 'precision' or row['scope'] != 'loaded':
                raise ValueError()
            cold = row['step'] == '0'
            if row['oracle'] != ('compatibility-zero-v1' if cold else 'original-retained-defined'):
                raise ValueError()
            before = straight.ref_list(row['before_storage'])
            start = straight.ref_list(row['after_start_storage'])
            lookup = straight.ref_list(row['lookup_storage'])
            if start not in (before, before + [int(row['start_ref'])]) or lookup not in (start, start + [int(row['end_ref'])]):
                raise ValueError()
            if cold:
                if before != [0] * 74 or row['before_active'] != '0' or row['previous_type'] != '0':
                    raise ValueError()
                if row['phase'] != 'same' or row['start_ref'] != row['end_ref'] or row['path_type'] != '1':
                    raise ValueError()
                if row['points'] != row['from'] + ';' + row['to']:
                    raise ValueError()
            else:
                prior = previous[row['sequence']]
                if any(row[a] != prior[b] for a, b in (('before_active', 'after_active'),
                    ('before_storage', 'after_storage'), ('previous_type', 'path_type'))):
                    raise ValueError()
                if row['phase'] != 'replacement' or row['ray_status'] != '40000000':
                    raise ValueError()
            if row['label'].endswith('_no_hit'):
                if row['ray_hit'] != '7f7fffff' or row['width_calls'] != '0':
                    raise ValueError()
                if row['path_type'] != ('1' if row['sequence'] == 'normal' else '4'):
                    raise ValueError()
            elif row['label'] == 'incomplete_hit':
                if row['ray_hit'] == '7f7fffff' or row['width_calls'] != '1' or row['path_type'] != '4':
                    raise ValueError()
            previous[row['sequence']] = row
        except (ValueError, KeyError, OverflowError):
            raise SystemExit('Invalid origin row: ' + row['label']) from None
    traces = straight.parse_auxiliary(result[FILES[1]], True)
    events = {row['label']: [] for row in rows}
    for row in traces:
        parent, _, index = row['label'].rpartition('_')
        if parent not in events or index != f"{len(events[parent]):04d}" or row['ordinal'] != str(len(events[parent])):
            raise SystemExit('Origin trace ancestry/order differs')
        if row['primitive'] not in {'closest', 'nearest5', 'nearest10', 'raycast', 'height'}:
            raise SystemExit('Unselected original primitive executed')
        events[parent].append(row)
    for row in rows:
        observed = events[row['label']]
        if not observed:
            raise SystemExit('Missing origin primitive trace')
        heights = [e for e in observed if e['primitive'] == 'height']
        rays = [e for e in observed if e['primitive'] == 'raycast']
        if len(heights) != int(row['height_calls']) or sum(bool(int(e['status'], 16) & 0x80000000) for e in heights) != int(row['height_failures']):
            raise SystemExit('Origin height summary differs')
        if len(rays) != (row['ray_status'] != '-'):
            raise SystemExit('Origin ray count differs')
        if rays:
            ray = rays[0]
            if any(row[a] != ray[b] for a, b in (('ray_status', 'status'), ('ray_hit', 'scalar'),
                    ('ray_normal', 'output'), ('ray_active', 'count'))):
                raise SystemExit('Origin ray summary differs')
            if straight.ref_list(row['ray_storage'])[:int(row['ray_active'])] != straight.ref_list(ray['refs']):
                raise SystemExit('Origin ray storage differs')
            expected_hit = '-' if row['ray_hit'] == '7f7fffff' else struct.pack('!f',
                straight.finite_bits(row['ray_hit']) - straight.finite_bits(row['collision_width'])).hex()
            if row['adjusted_hit'] != expected_hit:
                raise SystemExit('Origin full-width subtraction differs')
    return rows, traces


def require_space(path):
    free = shutil.disk_usage(path).free
    if free < MIN_FREE:
        raise SystemExit(f'Free-space floor: {free} bytes is below {MIN_FREE}; preserving all scratch')


def managed_build_dir(path):
    target = ROOT / 'target'
    cache = os.environ.get('MBX_CACHE_DIR')
    if not cache or not target.is_symlink():
        raise SystemExit('Existing managed target symlink and MBX_CACHE_DIR required')
    managed = target.resolve(strict=True)
    if not managed.is_relative_to((Path(cache) / 'targets').resolve(strict=True)):
        raise SystemExit('Target does not resolve under the existing MBX targets directory')
    path = path.resolve()
    if path == managed or not path.is_relative_to(managed):
        raise SystemExit('--build-dir must be strictly inside the existing managed target')
    if path.exists():
        raise SystemExit('Build directory already exists; use a new managed subdirectory; nothing deleted')
    require_space(managed)
    return path


def compiler_command():
    command = shlex.split(os.environ.get('CXX', 'g++'))
    if not command:
        raise SystemExit('Empty CXX command')
    executable = shutil.which(command[0])
    mbx = os.environ.get('MBX')
    if not executable or not mbx or Path(executable).resolve() != Path(mbx).resolve():
        raise SystemExit('CXX must resolve to the existing MBX compiler shim; invoke through mbx exec')
    # Preserve shim spelling (g++), not its resolved mbx executable name.
    command[0] = executable
    return command


def run_preserving_diagnostics(command, work, **kwargs):
    try:
        return straight.run(command, **kwargs)
    except subprocess.CalledProcessError as error:
        # Keep the original failure and all captured compiler output. Never retry here.
        streams = {'stdout': error.stdout, 'stderr': error.stderr}
        for name, content in streams.items():
            data = content.encode() if isinstance(content, str) else (content or b'')
            try:
                (work / ('failed-command-' + name + '.txt')).write_bytes(data)
            except OSError as failure:
                print('Could not preserve failed-command ' + name + ': ' + str(failure), file=sys.stderr)
            if data:
                print('Failed command ' + name + ':', file=sys.stderr)
                print(data.decode(errors='replace'), file=sys.stderr, end='' if data.endswith(b'\n') else '\n')
        try:
            (work / 'failed-command.json').write_text(json.dumps({
                'command': list(map(str, command)), 'returncode': error.returncode,
            }, indent=2) + '\n')
        except OSError as failure:
            print('Could not preserve failed-command metadata: ' + str(failure), file=sys.stderr)
        raise


def compile_oracle(sources, work, harness, sanitize=False, repair_lifetime=False):
    compiler = compiler_command()
    env = {**os.environ, 'LC_ALL': 'C', 'TMPDIR': str(work / 'tmp')}
    (work / 'tmp').mkdir()
    def run(command, **kwargs):
        require_space(work)
        return run_preserving_diagnostics(command, work, env=env, **kwargs)
    version = run(compiler + ['-dumpfullversion'], stdout=subprocess.PIPE).stdout.decode().strip()
    target = run(compiler + ['-dumpmachine'], stdout=subprocess.PIPE).stdout.decode().strip()
    if (version, target) != ('14.2.0', 'x86_64-linux-gnu'):
        raise SystemExit('Unsupported audited compiler profile: ' + version + ' ' + target)
    (work / 'oracle.cpp').write_bytes(harness.encode('latin1'))
    for name, content in sources.items():
        if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
            destination = work / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            if repair_lifetime and name == straight.TILE_HEADER:
                content = straight.repair_tile_header(content)
            destination.write_bytes(content.encode('latin1'))
    detour = work / 'dep/recastnavigation/Detour'
    flags = ['-std=c++20', '-O2', '-fno-inline', '-march=x86-64', '-mno-fma', '-fno-fast-math',
             '-ffp-contract=off', '-DNDEBUG', '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror',
             '-Wno-class-memaccess', '-Wno-deprecated-copy', '-Wno-error=unused-variable', '-fdiagnostics-color=never']
    if sanitize:
        flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    if repair_lifetime:
        flags += ['-DSTRAIGHT_PATH_REPAIR_TILE_LIFETIME=1']
    includes = ['-I' + str(detour / 'Include'), '-I' + str(work / 'dep/g3dlite')]
    inputs = [work / 'oracle.cpp'] + [detour / f'Source/{name}.cpp' for name in straight.DETOUR_FILES]
    # Separate serial compiler invocations let MBX cache each original TU. No new Cargo target.
    objects, commands, warnings = [], [], ''
    for index, source in enumerate(inputs):
        output = work / f'object-{index}.o'
        command = compiler + flags + includes + ['-c', str(source), '-o', str(output)]
        completed = run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        diagnostic = completed.stderr.decode()
        warnings += diagnostic
        if index == 0:
            straight.verify_warnings(diagnostic, harness, work / 'oracle.cpp')
        elif diagnostic:
            raise SystemExit('Unexpected original Detour compiler diagnostics: ' + diagnostic)
        objects.append(output)
        commands.append(command)
    binary = work / 'oracle'
    link = compiler + flags + list(map(str, objects)) + ['-o', str(binary)]
    completed = run(link, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if completed.stderr:
        raise SystemExit('Unexpected link diagnostics: ' + completed.stderr.decode())
    commands.append(link)
    symbols = run(['nm', '-C', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    straight.verify_symbols(symbols)
    for method in ('PathFinder::compatibility_same_polygon(', 'PathFinder::original_retained_straight('):
        if method not in symbols:
            raise SystemExit('Missing origin source symbol: ' + method)
    (work / 'symbols.txt').write_text(symbols)
    (work / 'compiler_stderr.txt').write_text(warnings)
    return binary, {'compiler': version, 'target': target, 'commands': commands,
                    'sanitize': sanitize, 'repair_tile_lifetime': repair_lifetime,
                    'observed_tile_header_sha256': sha((work / straight.TILE_HEADER).read_bytes())}


def provenance(sources, harness, package, spans):
    dependencies = ['straight_path_generate.py', 'straight_path_oracle.cpp',
        'straight_path_source_hashes.sha256', 'straight_path_input_hashes.sha256',
        'straight_path_source_fragments.test-fixture', 'cold_same_poly_generate.py',
        'cold_same_poly_oracle.cpp', 'cold_same_poly_source_fragments.test-fixture',
        'cold_distance_generate.py', 'cold_distance_input_hashes.sha256',
        'point_path_generate.py', 'point_path_oracle.cpp', 'point_path_source_hashes.sha256',
        'point_path_input_hashes.sha256', 'point_path_source_fragments.test-fixture',
        'retained_straight_origin_generate.py', 'retained_straight_origin_oracle.cpp']
    return {'cmangos_revision': straight.PIN,
        'scope': 'Two independent live original PathFinder/query allocations, each cold-policy entry then retained straight',
        'initial_policy': cold_same.cold.POLICY, 'original_cold': 'undefined-not-invoked-no-parity-claim',
        'query_nodes': 1024, 'include_filter': 13, 'exclude_filter': 0, 'point_limit': 74,
        'straight_line': True, 'use_straight_path': False, 'optimize': False,
        'normalize_z': False, 'ignore_normalization': False, 'force_destination': False,
        'collision_width_bits': '3e000000', 'previous_type': 'actual preceding terminal, never seeded/reset',
        'source_hashes': {name: sha(text.encode('latin1')) for name, text in sorted(sources.items())},
        'local_dependency_hashes': {name: sha((HERE / name).read_bytes()) for name in dependencies},
        'authored_input_hashes': {name: sha((HERE / 'fixtures' / name).read_bytes()) for name in
            ('direct-corridor/params.test-fixture', 'warm-cache/precision.test-fixture')},
        'source_spans': spans, 'harness_sha256': sha(harness.encode('latin1')),
        'source_package_sha256': sha(package.encode('latin1')),
        'excluded': ['original uninitialized cold caller', 'complete owner constructor',
            'default-query-pointer repair', 'complete Calculate/BuildPolyPath', 'owner collision-width derivation',
            'active normalization/world callbacks', 'smooth fallback', 'Route/gameplay admission'],
        'lifetime_caveat': 'Unchanged original dtMeshTile raw-storage lane is empirical, not a standards-defined lifetime proof'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=Path)
    parser.add_argument('--build-dir', required=True, type=Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    parser.add_argument('--repair-tile-lifetime', action='store_true')
    args = parser.parse_args()
    sources = guard_sources(args.mangos.resolve(strict=True))
    harness, package, spans = build_harness(sources)
    receipt = provenance(sources, harness, package, spans)
    if args.check and PACKAGE.read_bytes() != package.encode('latin1'):
        raise SystemExit('Origin source package differs')
    work = managed_build_dir(args.build_dir)
    compiler_command()  # Fail before creating scratch when not inside the approved MBX wrapper.
    work.mkdir(parents=True)
    binary, profile = compile_oracle(sources, work, harness, args.sanitize, args.repair_tile_lifetime)
    output = work / 'output'
    output.mkdir()
    require_space(work)
    run_preserving_diagnostics([str(binary), str(output), str(HERE / 'fixtures')], work,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if {path.name for path in output.iterdir()} != set(FILES):
        raise SystemExit('Unexpected origin output files')
    result = {name: (output / name).read_bytes() for name in FILES}
    rows, traces = verify_products(result)
    require_space(work)
    receipt['output_hashes'] = {name: sha(data) for name, data in result.items()}
    receipt['case_count'], receipt['trace_count'] = len(rows), len(traces)
    (work / 'profile.json').write_text(json.dumps({**receipt, **profile}, indent=2) + '\n')
    encoded_receipt = (json.dumps(receipt, indent=2) + '\n').encode()
    if args.check:
        for name, data in result.items():
            if (FIXTURES / name).read_bytes() != data:
                raise SystemExit('Origin fixture differs: ' + name)
        if PROVENANCE.read_bytes() != encoded_receipt:
            raise SystemExit('Origin provenance differs')
    else:
        # Only verified original-source execution may create expected fixture products.
        if PACKAGE.exists() or PROVENANCE.exists() or FIXTURES.exists():
            raise SystemExit('Origin products already exist; use --check; no output overwritten')
        PACKAGE.write_bytes(package.encode('latin1'))
        PROVENANCE.write_bytes(encoded_receipt)
        FIXTURES.mkdir()
        for name, data in result.items():
            (FIXTURES / name).write_bytes(data)
    for name, data in result.items():
        print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={sha(data)}')
    print('Preserved managed build and receipts:', work)


if __name__ == '__main__':
    main()
