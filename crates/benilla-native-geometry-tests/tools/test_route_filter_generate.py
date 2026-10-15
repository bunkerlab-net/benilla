#!/usr/bin/env python3
"""Fail-closed guards for unchanged source filter scalar observations."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import route_filter_generate as oracle

ROOT = Path(sys.argv.pop(1)).resolve()


def rows():
    result = []
    fields = ('case', 'label') + oracle.INPUT_FIELDS + oracle.OUTPUT_FIELDS
    for line in oracle.FIXTURE.read_text().splitlines():
        if line.startswith('#'): continue
        row = dict(zip(fields, line.split('\t'), strict=True))
        for key, value in row.items():
            if key not in ('label', 'trace', 'masks'):
                row[key] = int(value, 16 if key in ('x', 'y', 'z') else 10)
        result.append(row)
    return result


class RouteFilterOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guarded_sources(ROOT)
        cls.rows = rows()
        cls.temporary = tempfile.TemporaryDirectory(prefix='test-route-filter-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.binary = oracle.compile_oracle(cls.sources, Path(cls.temporary.name))

    def named(self, label): return next(row for row in self.rows if row['label'] == label)

    def test_full_hash_guards_missing_files_and_crlf_normalization(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, source in self.sources.items():
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.encode('latin1').replace(b'\n', b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root), self.sources)
            self.assertEqual(len(self.sources), 6)
            for name, source in self.sources.items():
                with self.subTest(name=name):
                    (root / name).write_text(source + '\n// hash probe\n', encoding='latin1')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(root)
                    (root / name).write_text(source, encoding='latin1')
            (root / next(iter(self.sources))).unlink()
            with self.assertRaises(FileNotFoundError): oracle.guarded_sources(root)

    def test_complete_original_methods_declarations_notices_and_hash_constants(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        for name, signature, _ in oracle.METHODS:
            original = oracle.exact_block(self.sources[name], signature)
            self.assertIn(original, harness); self.assertIn(original, package)
        for source in self.sources.values():
            if source.lstrip().startswith('/*'):
                self.assertIn(source[:source.index('*/') + 2], package)
        self.assertIn('#ifdef ENABLE_PLAYERBOTS', package)
        self.assertIn('if (m_vmgr->isHeightCalcEnabled())', package)
        self.assertIn('return const_cast<TerrainInfo*>(this)->GetGrid(x, y);', package)
        self.assertIn('MAP_LIQUID_TYPE_WATER | MAP_LIQUID_TYPE_OCEAN', package)
        sim = (oracle.HERE.parents[1] / 'benilla-sim/src/route_filter.rs').read_text()
        for line in oracle.MANIFEST.read_text().splitlines():
            if line and not line.startswith('#'): self.assertIn(line.split()[0], sim)

    def test_extraction_and_location_fail_closed(self):
        with self.assertRaisesRegex(SystemExit, 'Expected one source block'):
            oracle.exact_block('void f() {}\nvoid f() {}', 'void f()')
        with self.assertRaisesRegex(SystemExit, 'Unclosed source block'):
            oracle.exact_block('void f() {', 'void f()')
        sources = dict(self.sources)
        name = 'src/game/MotionGenerators/PathFinder.cpp'
        sources[name] = '\n' + sources[name]
        with self.assertRaisesRegex(SystemExit, 'Pinned source location drift'):
            oracle.build_harness(sources)

    def test_harness_marker_drift_is_rejected(self):
        real = Path.read_text
        for mutate in (lambda text: text.replace('// PINNED_METHODS', ''),
                       lambda text: text + '\n// PINNED_METHODS\n'):
            def read(path, *args, **kwargs):
                text = real(path, *args, **kwargs)
                return mutate(text) if path.name == 'route_filter_oracle.cpp' else text
            with mock.patch.object(Path, 'read_text', read):
                with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'):
                    oracle.build_harness(self.sources)

    def test_package_drift_fails_before_compilation_without_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary) / 'source.test-fixture'; package.write_text('drift')
            with mock.patch.object(oracle, 'PACKAGE', package), \
                 mock.patch.object(oracle, 'compile_oracle') as compiler, \
                 mock.patch.object(sys, 'argv', ['oracle', str(ROOT), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'): oracle.main()
            compiler.assert_not_called(); self.assertEqual(package.read_text(), 'drift')

    def test_fixture_drift_is_rejected_without_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary) / 'cases.test-fixture'; fixture.write_text('drift')
            with mock.patch.object(oracle, 'FIXTURE', fixture), \
                 mock.patch.object(oracle, 'compile_oracle', return_value=self.binary), \
                 mock.patch.object(sys, 'argv', ['oracle', str(ROOT), '--check']):
                with self.assertRaisesRegex(SystemExit, 'Route-filter fixture differs'): oracle.main()
            self.assertEqual(fixture.read_text(), 'drift')

    def test_missing_or_wrong_compiler_and_missing_source_symbols_fail(self):
        with mock.patch.object(oracle.shutil, 'which', return_value=None):
            with self.assertRaisesRegex(SystemExit, 'Missing required g\\+\\+'):
                oracle.compile_oracle(self.sources, Path(self.temporary.name))
        with mock.patch.object(oracle, 'run', return_value=subprocess.CompletedProcess([], 0, b'wrong')):
            with self.assertRaisesRegex(SystemExit, 'Unsupported audited compiler profile'):
                oracle.compile_oracle(self.sources, Path(self.temporary.name))
        with self.assertRaisesRegex(SystemExit, 'Missing unchanged source symbol'):
            oracle.verify_symbols('')
        with mock.patch.object(oracle, 'run', side_effect=subprocess.CalledProcessError(1, 'g++')):
            with self.assertRaises(subprocess.CalledProcessError):
                oracle.compile_oracle(self.sources, Path(self.temporary.name))

    def test_exact_fixture_reproduction(self):
        self.assertEqual(oracle.observe(self.binary, oracle.cases()), oracle.FIXTURE.read_text())
        self.assertEqual(len(self.rows), 87)

    def test_dry_predicate_order_masks_and_no_get_nav_terrain(self):
        for walk, swim, include in ((0, 0, 0), (0, 1, 12), (1, 0, 1), (1, 1, 13)):
            row = self.named(f'dry_capabilities_{walk}_{swim}')
            self.assertEqual((row['initial_include'], row['include'], row['exclude']), (include, include, 0))
            self.assertEqual(row['trace'], 'TWSIEHGAHGO')
            self.assertEqual(row['masks'], '15,10')
            self.assertEqual((row['include_calls'], row['exclude_calls'], row['read_calls']), (1, 1, 0))
        self.assertEqual(self.named('signed_zero_owner_position')['x'], 0x80000000)
        self.assertEqual(self.named('signed_zero_owner_position')['z'], 0x80000000)

    def test_can_check_short_circuit_and_missing_grid_control(self):
        for label in ('height_enabled_grid_present', 'height_enabled_no_grid'):
            row = self.named(label)
            self.assertEqual(row['trace'], 'TWSIEHAHO')
            self.assertEqual((row['height_calls'], row['grid_calls'], row['liquid_calls']), (2, 0, 2))
        row = self.named('cannot_check_no_grid')
        self.assertEqual(row['trace'], 'TWSIEHGHG')
        self.assertEqual(row['masks'], '-')
        self.assertEqual((row['liquid_calls'], row['read_calls']), (0, 0))

    def test_first_predicate_uses_any_nonzero_and_short_circuits_underwater(self):
        for status in (1, 2, 4, 8, 15):
            for kind in (1, 2, 4, 8, 0, 3, 10, 16, 32, 63):
                row = self.named(f'first_wet_{status}_type_{kind}')
                self.assertEqual(row['trace'], 'TWSIEHGARAI')
                self.assertEqual(row['masks'], '15,15')
                self.assertEqual((row['height_calls'], row['read_calls'], row['include_calls']), (1, 1, 2))
                # These are explicit source controls; Rust must not invent a wet filter.
                expected = {1: 9, 2: 5, 4: 9, 8: 5}.get(kind, 1)
                self.assertEqual(row['include'], expected)
                self.assertEqual(row['exclude'], 0)
        self.assertEqual(self.named('first_wet_nav_no_water')['include'], 1)

    def test_second_predicate_uses_underwater_bit_then_source_nav_lookup(self):
        for status in (1, 2, 4, 8, 15):
            for kind in (1, 2, 4, 8, 10):
                row = self.named(f'second_status_{status}_type_{kind}')
                wet = status & 8 != 0
                self.assertEqual(row['masks'], '15,10,15' if wet else '15,10')
                self.assertEqual(row['trace'], 'TWSIEHGAHGORAI' if wet else 'TWSIEHGAHGO')
                self.assertEqual(row['read_calls'], int(wet))
        self.assertEqual(self.named('second_underwater_nav_no_water')['include'], 1)
        self.assertEqual(self.named('wet_swim_only_initial_flags')['initial_include'], 12)
        self.assertEqual(self.named('wet_both_capabilities')['initial_include'], 13)

    def test_binary_input_domain_and_partial_records_fail_closed(self):
        good = list(oracle.cases()[2][1])
        invalid = []
        for index in range(4):
            row = good.copy(); row[index] = 2; invalid.append(struct.pack('<13I', *row))
        for index in (4, 5, 6):
            for word in (0x7f800000, 0xff800000, 0x7fc00001):
                row = good.copy(); row[index] = word; invalid.append(struct.pack('<13I', *row))
        for index, value in ((7, 16), (9, 16), (11, 16), (8, 64), (10, 64), (12, 64)):
            row = good.copy(); row[index] = value; invalid.append(struct.pack('<13I', *row))
        invalid.extend((b'\x00', struct.pack('<12I', *good[:12])))
        for raw in invalid:
            result = subprocess.run([str(self.binary)], input=raw, capture_output=True)
            self.assertNotEqual(result.returncode, 0, (raw, result.stdout))


if __name__ == '__main__': unittest.main()
