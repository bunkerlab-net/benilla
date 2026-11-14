#!/usr/bin/env python3
"""Fail-closed provenance, loaded-query controls, and checkout guards."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import hashlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import direct_corridor_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


class DirectCorridorOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guarded_sources(ROOT)
        cls.data = (oracle.FIXTURES / 'cases.test-fixture').read_bytes()
        cls.rows = oracle.parse_cases(cls.data)
        cls.temporary = tempfile.TemporaryDirectory(prefix='test-direct-corridor-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.work = Path(cls.temporary.name)
        cls.binary = oracle.compile_oracle(cls.sources, cls.work)
        cls.observed = oracle.observe(cls.binary, cls.work)

    def named(self, name): return next(row for row in self.rows if row['label'] == name)
    def refs(self, row, name='direct_corridor'): return [int(ref) for ref in row[name].split(',')]

    def test_full_hashes_cover_every_compiled_source_header_and_license(self):
        self.assertEqual(len(self.sources), 21)
        self.assertEqual(set(self.sources), oracle.SOURCE_FILES)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, source in self.sources.items():
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.encode('latin1').replace(b'\n', b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root), self.sources)
            for name, source in self.sources.items():
                with self.subTest(name=name):
                    (root / name).write_text(source + '\n// drift\n', encoding='latin1')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'): oracle.guarded_sources(root)
                    (root / name).write_text(source, encoding='latin1')
            (root / oracle.CPP).unlink()
            with self.assertRaises(FileNotFoundError): oracle.guarded_sources(root)

    def test_malformed_incomplete_duplicate_and_unsafe_manifests_fail_closed(self):
        valid = oracle.MANIFEST.read_text()
        first = next(line for line in valid.splitlines() if not line.startswith('#'))
        for malformed in (valid + first + '\n', valid.replace(first, ''), 'bad row surplus\n',
                          valid.replace(first.split()[1], '../escape'), valid.replace(first[:64], 'z'*64)):
            with tempfile.TemporaryDirectory() as temporary:
                manifest = Path(temporary) / 'manifest'; manifest.write_text(malformed)
                with mock.patch.object(oracle, 'MANIFEST', manifest):
                    with self.assertRaisesRegex(SystemExit, 'hash manifest'): oracle.guarded_sources(ROOT)

    def test_complete_original_bodies_and_map_pool_source_are_preserved(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        for signature, _ in oracle.METHODS:
            self.assertIn(oracle.exact_block(self.sources[oracle.CPP], signature), package)
        for name, source in self.sources.items():
            if name.startswith('dep/recastnavigation/'):
                self.assertEqual((self.work / name).read_text(encoding='latin1'), source)
        self.assertIn('query->init(mmap->navMesh, 1024)', package)
        self.assertIn('m_nodePool->getNode(neighbourRef, crossSide)', package)
        self.assertIn('neighbourNode->pidx != 0 && neighbourNode->pidx == bestNode->pidx', package)
        self.assertIn('if (length > maxPath)', package)
        self.assertIn(self.sources['LICENSE'], package)
        self.assertIn(self.sources['dep/recastnavigation/License.txt'], package)
        self.assertIn(self.sources[oracle.CPP].split('#include', 1)[0].rstrip(), package)
        self.assertNotIn('benilla_nav_', harness)
        self.assertNotIn('benilla_source_', harness)
        self.assertNotIn('getPolyByLocation(', harness)

    def test_missing_duplicate_unclosed_and_shifted_source_blocks_fail_closed(self):
        for text in ('void f() {} void f() {}', 'void other() {}'):
            with self.assertRaisesRegex(SystemExit, 'Expected one source block'): oracle.exact_block(text, 'void f()')
        for text in ('void f() {', 'void f()'):
            with self.assertRaisesRegex(SystemExit, 'Unclosed source block'): oracle.exact_block(text, 'void f()')
        for name in (oracle.CPP, oracle.MOVEMAP):
            sources = dict(self.sources); sources[name] = '\n' + sources[name]
            with self.assertRaisesRegex(SystemExit, 'Pinned source location drift'): oracle.build_harness(sources)
        sources = dict(self.sources); sources[oracle.MOVEMAP] = sources[oracle.MOVEMAP].replace('query->init(mmap->navMesh, 1024)', 'query->init(mmap->navMesh, 512)')
        with self.assertRaisesRegex(SystemExit, 'Source map node pool differs'): oracle.build_harness(sources)

    def test_missing_duplicate_harness_markers_fail_closed(self):
        real = Path.read_text
        for mutate in (lambda s: s.replace('// ORIGINAL_TILE_LAYOUT', ''), lambda s: s+'\n// ORIGINAL_TILE_LAYOUT\n'):
            def read(path, *args, **kwargs):
                value = real(path, *args, **kwargs)
                return mutate(value) if path.name == 'direct_corridor_oracle.cpp' else value
            with mock.patch.object(Path, 'read_text', read):
                with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'): oracle.build_harness(self.sources)

    def test_package_drift_fails_before_compilation_and_does_not_rewrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary) / 'package.test-fixture'; package.write_text('drift')
            with mock.patch.object(oracle, 'PACKAGE', package), mock.patch.object(oracle, 'compile_oracle') as compiler, \
                 mock.patch.object(sys, 'argv', ['oracle', str(ROOT), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'): oracle.main()
            compiler.assert_not_called(); self.assertEqual(package.read_text(), 'drift')

    def test_fixture_drift_fails_without_rewriting(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixtures = Path(temporary)
            for name in oracle.FILES: (fixtures / name).write_bytes(b'drift')
            with mock.patch.object(oracle, 'FIXTURES', fixtures), mock.patch.object(oracle, 'compile_oracle', return_value=self.binary), \
                 mock.patch.object(sys, 'argv', ['oracle', str(ROOT), '--check']):
                with self.assertRaisesRegex(SystemExit, 'Direct-corridor fixture differs'): oracle.main()
            self.assertTrue(all((fixtures / name).read_bytes() == b'drift' for name in oracle.FILES))

    def test_missing_wrong_failed_compiler_and_missing_or_bridge_symbols_fail_closed(self):
        with mock.patch.object(oracle.shutil, 'which', return_value=None):
            with self.assertRaisesRegex(SystemExit, 'Missing required g\\+\\+'): oracle.compile_oracle(self.sources, self.work)
        with mock.patch.object(oracle, 'run', return_value=subprocess.CompletedProcess([], 0, b'wrong')):
            with self.assertRaisesRegex(SystemExit, 'Unsupported audited compiler profile'): oracle.compile_oracle(self.sources, self.work)
        with mock.patch.object(oracle, 'run', side_effect=subprocess.CalledProcessError(1, 'g++')):
            with self.assertRaises(subprocess.CalledProcessError): oracle.compile_oracle(self.sources, self.work)
        with self.assertRaisesRegex(SystemExit, 'Missing unchanged source symbol'): oracle.verify_symbols('')
        symbols = oracle.run(['nm', '-C', str(self.binary)], stdout=subprocess.PIPE).stdout.decode()
        with self.assertRaisesRegex(SystemExit, 'Missing actual Detour symbol'): oracle.verify_symbols(symbols.replace('dtCreateNavMeshData(', 'removed('))
        for bridge in ('benilla_nav_create(', 'benilla_source_direct_step('):
            with self.assertRaisesRegex(SystemExit, 'Production bridge linked'): oracle.verify_symbols(symbols + bridge)

    def test_lifetime_amendment_is_exactly_one_default_constructor(self):
        original = self.sources[oracle.TILE_HEADER]; repaired = oracle.repair_tile_header(original)
        self.assertEqual(repaired.replace('\tdtMeshTile() = default;\n', '', 1), original)
        self.assertEqual(repaired.count('dtMeshTile() = default;'), 1)
        for bad in (original.replace('struct dtMeshTile\n{', 'struct dtMeshTile {'), repaired, original+'\nstruct dtMeshTile\n{\n'):
            with self.assertRaisesRegex(SystemExit, 'Tile-lifetime repair insertion drift'): oracle.repair_tile_header(bad)
        work = self.work / 'lifetime'; work.mkdir()
        binary = oracle.compile_oracle(self.sources, work, repair_lifetime=True)
        self.assertEqual(oracle.observe(binary, work), self.observed)
        for name, source in self.sources.items():
            if name.startswith('dep/recastnavigation/'):
                self.assertEqual((work / name).read_text(encoding='latin1'), repaired if name == oracle.TILE_HEADER else source)

    def test_reused_authored_inputs_are_hashbound(self):
        oracle.guard_inputs(); real = Path.read_bytes
        def read(path):
            data = real(path)
            return data+b'drift' if path.name == '0003232.mmtile' else data
        with mock.patch.object(Path, 'read_bytes', read):
            with self.assertRaisesRegex(SystemExit, 'Authored input fixture drift'): oracle.guard_inputs()

    def test_exact_reproduction_native_products_and_safe_extensions(self):
        self.assertEqual(len(self.rows), 22)
        for name, data in self.observed.items():
            self.assertEqual(data, (oracle.FIXTURES / name).read_bytes())
            self.assertTrue(name.endswith('.test-fixture'))
            if name not in {'params.test-fixture', 'cases.test-fixture'}:
                self.assertEqual(struct.unpack_from('<III', data), (0x4d4d4150, 7, 8))
                self.assertEqual(struct.unpack_from('<I', data, 12)[0]+20, len(data))
        self.assertEqual(len(self.observed['params.test-fixture']), 28)
        for names in (('evil.mmtile',), ('../outside.test-fixture',), ('x.test-fixture', 'x.test-fixture')):
            with mock.patch.object(oracle, 'FILES', names), mock.patch.object(oracle, 'run') as run:
                with self.assertRaisesRegex(SystemExit, 'Unsafe fixture output filename'): oracle.observe(self.binary, self.work)
                run.assert_not_called()
        output = self.work / 'output'; extra = output / 'unexpected.mmtile'; extra.write_bytes(b'wrong')
        try:
            with self.assertRaisesRegex(SystemExit, 'Unexpected oracle output files'): oracle.observe(self.binary, self.work)
        finally: extra.unlink()

    def test_bad_case_schema_duplicate_labels_and_numeric_domain_fail_closed(self):
        lines = self.data.decode().splitlines(); header, row = lines[1], lines[2]
        for bad in ('\n'.join(lines[:1]+lines[2:]), '\n'.join(lines+[header]), '\n'.join(lines+[row]), header+'\n', header+'\n'+row+'\textra'):
            with self.assertRaises(SystemExit): oracle.parse_cases(bad.encode())
        for field, value in (('nodes','1025'), ('capacity','0'), ('capacity','1101'), ('from','7fc00000,00000000,00000000'),
                             ('geometry','invented'), ('from_ref','0'), ('direct_nodes','1025'), ('from_over','2'),
                             ('direct_status','success'), ('direct_corridor','0'), ('direct_corridor','-')):
            values = row.split('\t'); values[oracle.FIELDS.index(field)] = value
            with self.assertRaisesRegex(SystemExit, 'Invalid oracle row'): oracle.parse_cases((header+'\n'+'\t'.join(values)).encode())

    def test_fresh_same_polygon_query_has_no_history_dependent_nodes(self):
        row = self.named('same_polygon')
        self.assertEqual(row['direct_status'], '40000000'); self.assertEqual(row['sliced_status'], '40000000')
        self.assertEqual(row['direct_nodes'], '0'); self.assertEqual(row['sliced_nodes'], '0')
        self.assertNotEqual(row['from'], row['from_nearest']); self.assertNotEqual(row['to'], row['to_nearest'])

    def test_cross_side_route_really_diverges_at_equal_pool_and_capacity(self):
        row = self.named('cross_side_route_diverges')
        self.assertEqual(row['nodes'], '1024'); self.assertEqual(row['capacity'], '74')
        self.assertEqual(row['direct_status'], '40000000'); self.assertEqual(row['sliced_status'], '40000000')
        base = 4503599627370496
        self.assertEqual(self.refs(row), [base, base+(1<<31), base+(3<<31)])
        self.assertEqual(self.refs(row, 'sliced_corridor'), [base, base+(2<<31), base+(3<<31)])
        self.assertEqual(row['direct_nodes'], '5'); self.assertEqual(row['direct_state_nodes'], '2')
        self.assertEqual(row['sliced_nodes'], '4')
        reverse = self.named('cross_side_reverse')
        self.assertEqual(reverse['direct_state_nodes'], '3')
        self.assertEqual(self.refs(reverse), list(reversed(self.refs(row))))

    def test_exact_capacity_is_not_truncation_in_original_direct(self):
        for label, count in (('two_exact_capacity',2), ('cross_side_exact_capacity',3), ('seventyfour_exact_capacity',74),
                             ('long600_full',600), ('long1100_exact_pool1024_full',1024)):
            row = self.named(label)
            self.assertEqual(len(self.refs(row)), count); self.assertEqual(int(row['capacity']), count)
            self.assertEqual(row['direct_status'], '40000000'); self.assertEqual(row['sliced_status'], '40000010')
        for label in ('two_truncated', 'cross_side_truncated', 'seventyfive_truncated'):
            self.assertEqual(self.named(label)['direct_status'], '40000010')

    def test_disconnected_partial_and_real_pool_exhaustion_details(self):
        self.assertEqual(self.named('disconnected_partial')['direct_status'], '40000040')
        a, b = self.named('long600_1024'), self.named('long600_512')
        self.assertEqual(a['direct_status'], '40000010'); self.assertEqual(a['direct_nodes'], '600')
        self.assertEqual(b['direct_status'], '40000070'); self.assertEqual(b['direct_nodes'], '512')
        self.assertEqual(self.named('long1100_1024')['direct_status'], '40000070')
        exact = self.named('long1100_exact_pool1024_full')
        self.assertEqual(exact['direct_nodes'], '1024'); self.assertEqual(exact['direct_status'], '40000000')
        partial = self.named('long1100_outnodes_capacity1024')
        self.assertEqual(partial['direct_nodes'], '1024'); self.assertEqual(len(self.refs(partial)), 1024)
        self.assertEqual(partial['direct_status'], '40000060'); self.assertEqual(partial['sliced_status'], '40000070')
        self.assertNotEqual(self.refs(partial)[-1], int(partial['to_ref']))

    def test_actual_autocrlf_checkout_preserves_all_hashed_and_generated_bytes(self):
        with tempfile.TemporaryDirectory(prefix='direct-corridor-crlf-') as temporary:
            checkout = Path(temporary)
            files = [oracle.HERE / name for name in ('direct_corridor_generate.py', 'direct_corridor_test_generate.py',
                'direct_corridor_oracle.cpp', 'direct_corridor_README.md', 'direct_corridor_source_hashes.sha256',
                'direct_corridor_input_hashes.sha256', 'direct_corridor_source_fragments.test-fixture')]
            files += [oracle.HERE / '.gitattributes'] + list(oracle.FIXTURES.iterdir())
            files += [oracle.HERE / 'fixtures' / name for name in oracle.INPUT_FILES]
            expected = {}
            for source in files:
                relative = source.relative_to(oracle.HERE); dest = checkout / relative; dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, dest); expected[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
            oracle.run(['git', 'init', '-q', str(checkout)])
            oracle.run(['git', '-C', str(checkout), '-c', 'core.autocrlf=true', 'add', '.'])
            for relative in expected: (checkout / relative).unlink()
            oracle.run(['git', '-C', str(checkout), '-c', 'core.autocrlf=true', 'checkout-index', '--all', '--force'])
            for relative, digest in expected.items():
                if relative.name == '.gitattributes': continue # Attribute content is semantic, not a hashed input.
                self.assertEqual(hashlib.sha256((checkout / relative).read_bytes()).hexdigest(), digest, str(relative))
            with mock.patch.object(oracle, 'HERE', checkout): oracle.guard_inputs()


if __name__ == '__main__': unittest.main()
