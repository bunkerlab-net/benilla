#!/usr/bin/env python3
"""Selected zero-distance compatibility oracle; never executes undefined original cold reads."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
sys.dont_write_bytecode = True
import warm_cache_generate as warm

HERE = Path(__file__).resolve().parent
POLICY = 'cmangos-buildpolypath-zero-distance-v1'
VERSION = 1
INPUT_MANIFEST = HERE / 'cold_distance_input_hashes.sha256'
INPUT_FILES = {'warm-cache/precision.test-fixture'}
ORIGINAL = '    float distToStartPoly, distToEndPoly;'
REPAIRED = '    float distToStartPoly = 0.0f, distToEndPoly = 0.0f;'
PACKAGE = HERE / 'cold_distance_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/cold-distance'
FILES = ('cases.test-fixture', 'traces.test-fixture')
FIELDS = ('label oracle geometry from to start_ref start_distance end_ref end_distance '
          'lookup_storage phase direct_status after_active after_storage').split()


def guard_inputs():
    warm.guard_inputs()
    for name, expected in warm.manifest_rows(INPUT_MANIFEST, INPUT_FILES).items():
        if hashlib.sha256((HERE/'fixtures'/name).read_bytes()).hexdigest() != expected:
            raise SystemExit('Cold authored input fixture drift: ' + name)


def repair_distance_initialization(prefix):
    if prefix.count(ORIGINAL) != 1 or REPAIRED in prefix:
        raise SystemExit('Cold distance initialization drift')
    repaired = prefix.replace(ORIGINAL, REPAIRED, 1)
    if repaired.replace(REPAIRED, ORIGINAL, 1) != prefix:
        raise SystemExit('Unrelated cold distance repair')
    return repaired


def build_harness(sources):
    source = sources[warm.CPP]
    prefix = warm.source_span(source, ORIGINAL, '    dtStatus dtResult;')
    if source[:source.index(ORIGINAL)].count('\n') + 1 != 410:
        raise SystemExit('Cold source location drift')
    repaired = repair_distance_initialization(prefix)
    harness, _ = warm.build_harness(sources)
    old_main = warm.exact_block(harness, 'int main(int argc,char** argv)')
    retained_main = old_main.replace('int main(', '[[maybe_unused]] static int retained_warm_fixture_main(', 1)
    retained_main = retained_main[:-1] + '    return 0;\n}'
    harness = harness.replace(old_main, retained_main, 1)
    extension = (HERE / 'cold_distance_oracle.cpp').read_text()
    declarations, extension = extension.split('// INSERTED_BEFORE_PATHFINDER', 1)
    harness = harness.replace('class PathFinder {', declarations + '\nclass PathFinder {', 1)
    marker = '    Transition selected_transition('
    declaration = ('    ColdPair compatibility_endpoints(const Vector3& startPos, const Vector3& endPos,\n'
                   '        FILE* trace, const char* label);\n')
    if harness.count(marker) != 1: raise SystemExit('Cold class insertion drift')
    harness = harness.replace(marker, declaration + marker, 1)
    # Observers run around the unchanged original two lookup calls, without substituting answers.
    instrumented = repaired.replace('    dtPolyRef startPoly =',
        '    observer.begin(trace,label,0,m_polyLength);\n    dtPolyRef startPoly =', 1)
    instrumented = instrumented.replace('    dtPolyRef endPoly =',
        '    observer.begin(trace,label,1,m_polyLength);\n    dtPolyRef endPoly =', 1)
    if extension.count('// REPAIRED_SOURCE_ENDPOINT_PREFIX') != 1:
        raise SystemExit('Cold prefix insertion drift')
    harness += extension.replace('// REPAIRED_SOURCE_ENDPOINT_PREFIX', instrumented, 1)
    package = ('// SPDX-License-Identifier: GPL-3.0-or-later\n// CMaNGOS ' + warm.PIN + '\n// Selected compatibility policy: ' + POLICY + '\n'
               '// Original cold undefined branch is NOT an oracle result. Full unchanged context:\n'
               '// warm_cache_source_fragments.test-fixture (BuildPolyPath, constructor, lookup bodies).\n'
               '// ORIGINAL CALLER PREFIX, PathFinder.cpp:410-415\n' + prefix +
               '\n// REPAIRED CALLER PREFIX: sole semantic amendment\n' + repaired)
    return harness, package.rstrip() + "\n"


def parse_cases(data):
    rows = warm.parse_table(data, FIELDS)
    for row in rows:
        try:
            if row['oracle'] != 'compatibility-zero-v1': raise ValueError()
            if row['geometry'] not in {'precision','existing','grid','long600','long1100'}: raise ValueError()
            if row['phase'] not in {'same','replacement','missing','far'}: raise ValueError()
            for field in ('from','to'):
                values = row[field].split(',')
                if len(values) != 3: raise ValueError()
                for value in values: warm.finite_bits(value)
            for field in ('start_distance','end_distance'):
                if warm.finite_bits(row[field]) < 0: raise ValueError()
            for field in ('start_ref','end_ref'):
                if not 0 <= int(row[field]) < (1 << 64): raise ValueError()
            for field in ('lookup_storage','after_storage'):
                if len(warm.ref_list(row[field])) < 74: raise ValueError()
            if not 0 <= int(row['after_active']) <= 74: raise ValueError()
            if row['direct_status'] not in {'00000000','40000000','40000040','40000010','40000070'}: raise ValueError()
        except (ValueError, OverflowError):
            raise SystemExit('Invalid cold oracle row: ' + row['label']) from None
    return rows


def compile_oracle(sources, work, sanitize=False, receipts=None):
    compiler = shutil.which('g++')
    if not compiler: raise SystemExit('Missing required g++ compiler')
    version = warm.run([compiler,'-dumpfullversion'],stdout=subprocess.PIPE).stdout.decode().strip()
    target = warm.run([compiler,'-dumpmachine'],stdout=subprocess.PIPE).stdout.decode().strip()
    if version != '14.2.0' or target != 'x86_64-linux-gnu':
        raise SystemExit('Unsupported audited compiler profile: '+version+' '+target)
    harness, _ = build_harness(sources)
    (work/'oracle.cpp').write_text(harness,encoding='latin1')
    for name, content in sources.items():
        if name.startswith('dep/recastnavigation/'):
            dest=work/name;dest.parent.mkdir(parents=True,exist_ok=True)
            # Existing separately documented production tile-lifetime profile, not a new repair.
            if name == warm.TILE_HEADER: content=warm.repair_tile_header(content)
            dest.write_text(content,encoding='latin1')
    detour=work/'dep/recastnavigation/Detour'
    flags=['-std=c++20','-O2','-fno-inline','-march=x86-64','-mno-fma','-fno-fast-math',
           '-ffp-contract=off','-DDT_POLYREF64=1','-DWARM_CACHE_REPAIR_TILE_LIFETIME=1',
           '-Wall','-Wextra','-Werror','-Wno-class-memaccess']
    if sanitize: flags += ['-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all']
    command=[compiler,*flags,'-I'+str(detour/'Include'),str(work/'oracle.cpp'),
             *[str(detour/f'Source/{name}.cpp') for name in warm.DETOUR_FILES],'-o',str(work/'oracle')]
    warm.run(command)
    symbols=warm.run(['nm','-C',str(work/'oracle')],stdout=subprocess.PIPE).stdout.decode()
    warm.verify_symbols(symbols)
    if 'PathFinder::compatibility_endpoints(' not in symbols: raise SystemExit('Missing cold repair symbol')
    if receipts:
        receipts.mkdir(parents=True,exist_ok=True);shutil.copy2(work/'oracle.cpp',receipts/'oracle.cpp')
        (receipts/'symbols.txt').write_text(symbols)
        (receipts/'profile.json').write_text(json.dumps({
            'cmangos_revision':warm.PIN,'compatibility_policy':POLICY,'compatibility_version':VERSION,
            'oracle_label':'compatibility-zero-v1','original_cold_label':'undefined-not-invoked-no-parity-claim',
            'source_amendment':{'original':ORIGINAL.strip(),'repaired':REPAIRED.strip()},
            'compiler':version,'target':target,'command':command,'sanitize':sanitize,
            'query_nodes':1024,'production_lookup_query_nodes':512,'production_direct_query_nodes':1024,
            'tile_lifetime_profile':'existing-default-constructor-only-repaired',
            'scope':'repaired caller-local endpoint prefix, unchanged lookup bodies and selected polygon-storage fragments; real Detour',
            'excluded':['owner lifecycle','selected config','SetCurrentNavMesh','HaveTile','BuildPointPath','Route admission','default query pointer repair'],
            'source_hashes_sha256':hashlib.sha256(warm.MANIFEST.read_bytes()).hexdigest(),
            'warm_input_hashes_sha256':hashlib.sha256((HERE/'warm_cache_input_hashes.sha256').read_bytes()).hexdigest(),
            'cold_input_hashes_sha256':hashlib.sha256(INPUT_MANIFEST.read_bytes()).hexdigest(),
            'harness_sha256':hashlib.sha256((work/'oracle.cpp').read_bytes()).hexdigest(),
        },indent=2)+'\n')
    return work/'oracle'


def observe(binary,work):
    output=work/'output';output.mkdir(exist_ok=True)
    warm.run([str(binary),str(output),str(HERE/'fixtures')])
    if {path.name for path in output.iterdir()} != set(FILES): raise SystemExit('Unexpected cold oracle output')
    result={name:(output/name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture']);warm.parse_traces(result['traces.test-fixture'])
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos',type=Path);parser.add_argument('--check',action='store_true')
    parser.add_argument('--sanitize',action='store_true');parser.add_argument('--receipt-dir',type=Path)
    args=parser.parse_args();sources=warm.guarded_sources(args.mangos);guard_inputs()
    _,package=build_harness(sources)
    if args.check and PACKAGE.read_text()!=package: raise SystemExit('Cold source package differs')
    with tempfile.TemporaryDirectory(prefix='cold-distance-source-') as temporary:
        work=Path(temporary);binary=compile_oracle(sources,work,args.sanitize,args.receipt_dir)
        observed=observe(binary,work)
    if args.check:
        for name,data in observed.items():
            if (FIXTURES/name).read_bytes()!=data: raise SystemExit('Cold fixture differs: '+name)
    else:
        PACKAGE.write_text(package);FIXTURES.mkdir(parents=True,exist_ok=True)
        for name,data in observed.items(): (FIXTURES/name).write_bytes(data)
    for name,data in observed.items(): print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:
        (args.receipt_dir/'output_hashes.json').write_text(json.dumps({name:hashlib.sha256(data).hexdigest() for name,data in observed.items()},indent=2)+'\n')

if __name__=='__main__': main()
