#!/usr/bin/env python3
"""Fail-closed guards for full source near-point traces with scalar callbacks."""
# SPDX-License-Identifier: GPL-3.0-or-later
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import near_point_empty_generate as oracle

ROOTS = [pathlib.Path(sys.argv.pop(1)).resolve() for _ in range(3)]
HEX_FIELDS = set(oracle.FLOAT_FIELDS) | {'result_x', 'result_y', 'result_z',
    'raw_angle', 'query_x', 'query_y', 'query_z', 'ground_reply', 'adjusted_z',
    'los_src_x', 'los_src_y', 'los_src_z', 'los_dest_x', 'los_dest_y', 'los_dest_z'}
TEXT_FIELDS = {'label', 'ground_script', 'static_script', 'dynamic_script', 'outcome', 'trace'}


def read_rows(name, fields):
    result = []
    for line in (oracle.FIXTURES / name).read_text().splitlines():
        if line.startswith('#'): continue
        tokens = line.split('\t')
        if len(tokens) != len(fields): raise AssertionError('Fixture schema mismatch')
        row = dict(zip(fields, tokens, strict=True))
        for key in fields:
            if key in HEX_FIELDS: row[key] = int(row[key], 16)
            elif key not in TEXT_FIELDS: row[key] = int(row[key])
        result.append(row)
    return result


class EmptySourceOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guarded_sources(*ROOTS)
        cls.rows = read_rows('cases.test-fixture', ('case', 'label') + oracle.INPUT_FIELDS + oracle.OUTPUT_FIELDS)
        cls.heights = read_rows('heights.test-fixture', ('case',) + oracle.HEIGHT_FIELDS)
        cls.temporary = tempfile.TemporaryDirectory(prefix='test-empty-source-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.binary = oracle.compile_oracle(cls.sources, pathlib.Path(cls.temporary.name))

    def named(self, label):
        return [row for row in self.rows if row['label'] == label]

    def events(self, row):
        return [h for h in self.heights if h['case'] == row['case']]

    def test_all_full_file_guards_and_missing_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            families = ('cmangos', 'glibc', 'glibcfloat')
            for name, raw in self.sources.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            self.assertEqual(len(self.sources), 23)
            for name, raw in self.sources.items():
                with self.subTest(name=name):
                    (root / name).write_bytes(raw + b'\n/* source guard probe */\n')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(*(root / family for family in families))
                    (root / name).write_bytes(raw)
            missing = root / 'cmangos/src/game/Maps/ObjectPosSelector.cpp'
            missing.unlink()
            with self.assertRaises(FileNotFoundError):
                oracle.guarded_sources(*(root / family for family in families))

    def test_full_source_methods_cpp_header_and_notices_unchanged(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        selector = self.sources['cmangos/src/game/Maps/ObjectPosSelector.cpp'].decode('latin1')
        self.assertIn(selector, harness)
        self.assertIn(selector, package)
        self.assertIn(self.sources['cmangos/src/game/Maps/ObjectPosSelector.h'].decode('latin1'), package)
        for name, signature in (
            ('Object.cpp', 'void WorldObject::GetNearPointAt('),
            ('Object.cpp', 'void WorldObject::GetNearPoint2dAt('),
            ('Object.cpp', 'bool WorldObject::IsWithinLOS('),
            ('Unit.cpp', 'void Unit::UpdateAllowedPositionZ('),
        ):
            text = self.sources['cmangos/src/game/Entities/' + name].decode('latin1')
            method = oracle.exact_method(text, signature)
            self.assertIn(method, harness); self.assertIn(method, package)
        text = self.sources['cmangos/src/game/Maps/Map.cpp'].decode('latin1')
        self.assertIn(oracle.exact_method(text, 'bool Map::IsInLineOfSight('), harness)
        for name, raw in self.sources.items():
            if name.endswith('COPYING.LIB'):
                self.assertIn(raw.decode('latin1'), package)
            else:
                text = raw.decode('latin1')
                self.assertIn(text[:text.index('*/') + 2], package)
        near = oracle.exact_method(self.sources['cmangos/src/game/Entities/Object.cpp'].decode('latin1'),
                                   'void WorldObject::GetNearPointAt(')
        self.assertIn('//    Cell::VisitAllObjects(this, worker, dist);', near)
        self.assertEqual(near.count('selector.InitializeAngle();'), 2)
        self.assertIn('while (selector.NextUsedAngle(angle))', near)
        self.assertNotIn('FallbackRequired', harness)
        self.assertIn('static_assert(std::is_same_v<decltype(atan(1.0f)), double>);', harness)

    def test_package_drift_fails_before_compilation_and_preserves_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary) / 'source.test-fixture'
            path.write_text('deliberate package drift')
            with mock.patch.object(oracle, 'PACKAGE', path), \
                 mock.patch.object(oracle, 'compile_oracle') as compiler, \
                 mock.patch.object(sys, 'argv', ['oracle.py', *map(str, ROOTS), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'): oracle.main()
                compiler.assert_not_called()
            self.assertEqual(path.read_text(), 'deliberate package drift')

    def test_marker_and_extraction_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary)
            original = (oracle.HERE / 'near_point_empty_oracle.cpp').read_text()
            for changed in (original + '\n// PINNED_COMPLETE_SELECTOR_CPP\n',
                            original.replace('// PINNED_GET_NEAR_POINT_2D', '// REMOVED')):
                (path / 'near_point_empty_oracle.cpp').write_text(changed)
                with mock.patch.object(oracle, 'HERE', path):
                    with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'):
                        oracle.build_harness(self.sources)
        for text in ('void f() {}\nvoid f() {}', 'void other() {}'):
            with self.assertRaisesRegex(SystemExit, 'Expected one source method'):
                oracle.exact_method(text, 'void f(')
        with self.assertRaisesRegex(SystemExit, 'Unclosed source method'):
            oracle.exact_method('void f() {', 'void f(')

    def test_missing_or_mismatched_compiler_is_error(self):
        with mock.patch.object(oracle.subprocess, 'run', side_effect=FileNotFoundError('missing compiler')):
            with self.assertRaises(FileNotFoundError): oracle.run(['g++', '--version'])
        with mock.patch.object(oracle.atan.subprocess, 'check_output', return_value='0.0\n'):
            with self.assertRaisesRegex(SystemExit, r'Requires audited GCC/G\+\+ 14.2.0'):
                oracle.atan.compile_oracle(self.sources, pathlib.Path(self.temporary.name))

    def test_binding_and_fused_arithmetic_guards(self):
        symbols = subprocess.check_output(['nm','-C',str(self.binary)],text=True)
        undefined = subprocess.check_output(['nm','-u',str(self.binary)],text=True)
        assembly = subprocess.check_output(['objdump','-d','-C',str(self.binary)],text=True)
        oracle.verify_symbols(symbols,undefined,assembly)
        with self.assertRaisesRegex(SystemExit,'Missing pinned numeric symbol'):
            oracle.verify_symbols(symbols.replace(' T atan\n',' T wrong_atan\n'),undefined,assembly)
        with self.assertRaisesRegex(SystemExit,'Host transcendental fallback'):
            oracle.verify_symbols(symbols,undefined+'\n U atan@GLIBC_2.2.5\n',assembly)
        with self.assertRaisesRegex(SystemExit,'Unexpected fused arithmetic'):
            oracle.verify_symbols(symbols,undefined,assembly+'\n vfmadd132ss %xmm0,%xmm1,%xmm2\n')
        with self.assertRaisesRegex(SystemExit,'Missing trace forwarding binding'):
            oracle.verify_symbols(symbols,undefined,assembly.replace('<source_cos>','<host_cos>'))

    def test_fixture_reproduction_and_schema(self):
        actual = oracle.observe(self.binary, oracle.cases())
        for name, text in actual.items(): self.assertEqual(text, (oracle.FIXTURES / name).read_text())
        self.assertEqual(len(self.rows), len(oracle.cases()))
        self.assertEqual([r['case'] for r in self.rows], list(range(len(self.rows))))
        self.assertEqual(len(self.heights), sum(r['height_calls'] for r in self.rows))
        self.assertTrue(all(p.suffix == '.test-fixture' for p in oracle.FIXTURES.iterdir()))

    def test_exact_callback_trace_and_short_circuit_order(self):
        for row in self.rows:
            events = self.events(row)
            with self.subTest(label=row['label']):
                self.assertEqual([h['height_index'] for h in events], list(range(len(events))))
                trace = ''
                for n, h in enumerate(events):
                    if h['xy_index'] >= 0:
                        self.assertEqual(h['xy_index'], n)
                        self.assertEqual(h['query_z'], row['pos_z'])
                        trace += 'I'
                    else:
                        self.assertEqual(n, len(events)-1)
                        self.assertEqual(h['raw_angle'], 0)
                    if n == 0: trace += 'C' + ('R' if row['detect'] else '')
                    trace += 'H'
                    if h['static_called']:
                        trace += 'PS'
                        self.assertEqual((h['los_dest_x'],h['los_dest_y'],h['los_dest_z']),
                                         (h['query_x'],h['query_y'],h['adjusted_z']))
                        self.assertEqual((h['los_src_x'],h['los_src_y']), (row['target_x'],row['target_y']))
                        self.assertEqual(h['los_src_z'], oracle.bits(oracle.value(row['target_z']) +
                                                                  oracle.value(row['collision_height'])))
                    else:
                        self.assertEqual(tuple(h[k] for k in oracle.HEIGHT_FIELDS if k.startswith('los_')), (0,)*6)
                    self.assertEqual(h['dynamic_called'], int(bool(h['static_called'] and h['static_reply'])))
                    if h['dynamic_called']: trace += 'D'
                    self.assertEqual((h['swim'],h['ignore_m2']), (0,0))
                self.assertEqual(trace,row['trace'])
                self.assertEqual(row['static_calls'],sum(h['static_called'] for h in events))
                self.assertEqual(row['dynamic_calls'],sum(h['dynamic_called'] for h in events))
                self.assertEqual(row['config_calls'],1)
                self.assertEqual(row['radius_calls'],row['detect'])

    def test_first_plus_minus_later_and_vertical_short_circuit(self):
        for label, count in (('primary_accept',1), ('first_plus_static_reject',2),
                ('first_plus_dynamic_reject',2), ('first_minus_mixed_reject',3),
                ('later_success_vertical_and_static',4), ('later_success_alternating_los',4)):
            row, = self.named(label)
            self.assertEqual(row['xy_calls'],count)
            self.assertEqual(row['height_calls'],count)
            self.assertEqual(row['outcome'],'primary-accepted' if count == 1 else 'fallback-accepted')
        row, = self.named('later_success_vertical_and_static')
        self.assertEqual([h['static_called'] for h in self.events(row)], [0,0,1,1])
        row, = self.named('left_associated_sum')
        self.assertEqual(self.events(row)[0]['static_called'],0)
        self.assertEqual(row['xy_calls'],2)

    def test_full_exhaustion_final_first_xy_carried_z_and_no_los(self):
        for row in self.rows:
            if row['outcome'] != 'exhausted': continue
            events = self.events(row)
            first, last, final = events[0], events[-2], events[-1]
            self.assertEqual(row['height_calls'],row['xy_calls']+1)
            self.assertEqual(final['xy_index'],-1)
            self.assertEqual((final['query_x'],final['query_y']), (first['query_x'],first['query_y']))
            self.assertEqual(final['query_z'],last['adjusted_z'])
            self.assertEqual((final['static_called'],final['dynamic_called']),(0,0))
            self.assertEqual((row['result_x'],row['result_y'],row['result_z']),
                             (first['query_x'],first['query_y'],final['adjusted_z']))
            self.assertTrue(row['trace'].endswith('HH') or row['trace'].endswith('PSH') or row['trace'].endswith('PSDH'))
        row, = self.named('exhausted_final_invalid_preserves_last_z')
        final = self.events(row)[-1]
        self.assertEqual((final['query_z'],final['adjusted_z'],row['result_z']), (oracle.bits(13),)*3)
        self.assertNotEqual(final['query_z'],row['pos_z'])
        self.assertEqual(final['ground_reply'],oracle.bits(-200000))
        self.assertEqual((final['static_reply'],final['dynamic_reply']),(1,1))

    def test_exact_pi_zero_half_upper_raw_angle_and_signed_zero(self):
        row, = self.named('exact_pi_neighbor_0')
        angles = [h['raw_angle'] for h in self.events(row) if h['xy_index'] >= 0]
        self.assertEqual(angles[-2:],[0x40490fdb,0xc0490fdb])
        row, = self.named('zero_half_angle_628')
        self.assertEqual((row['xy_calls'],row['height_calls']),(629,630))
        row, = self.named('upper_raw_fallback_angle')
        self.assertIn(0x4116cbe4,[h['raw_angle'] for h in self.events(row)])
        row, = self.named('negative_raw_fallback_angle')
        self.assertEqual(self.events(row)[-1]['raw_angle'],0xbc23d70a)
        for row in self.named('signed_zero_z'): self.assertEqual(row['result_z'],row['pos_z'])
        rows = self.named('ground_validity_threshold')
        self.assertEqual([r['outcome'] for r in rows],['primary-accepted','primary-accepted','exhausted'])

    def test_loaded_companion_rows_are_explicitly_scalar_witnesses(self):
        row, = self.named('loaded_wall_scalar_companion')
        self.assertEqual((row['xy_calls'],row['static_calls'],row['dynamic_calls']),(6,6,1))
        self.assertEqual(self.events(row)[-1]['raw_angle'],0x3f389abe)
        row, = self.named('loaded_carried_z_scalar_companion')
        self.assertEqual((row['xy_calls'],row['height_calls'],row['static_calls']),(7,8,0))
        self.assertEqual([h['query_z'] for h in self.events(row)], [oracle.bits(11)]*7+[oracle.bits(7)])
        row, = self.named('loaded_subnormal_scalar_companion')
        self.assertEqual(row['result_x'],1)
        row, = self.named('loaded_blocked_exhaustion_scalar_companion')
        self.assertEqual((row['xy_calls'],row['height_calls'],row['static_calls'],row['dynamic_calls']),
                         (7,8,7,0))
        self.assertEqual(row['outcome'],'exhausted')
        self.assertEqual(self.events(row)[-1]['static_called'],0)
        self.assertEqual(row['result_z'],oracle.bits(7))

    def test_nonfinite_domain_flags_and_partial_input_fail_closed(self):
        baseline = oracle.cases()[0]
        def status(words, replies=None):
            record = (baseline[0],words,replies or baseline[2])
            return subprocess.run([str(self.binary)],input=oracle.encode([record]),capture_output=True).returncode
        for field in range(11):
            for bad in (0x7f800000,0xff800000,0x7fc01234):
                words = list(baseline[1]); words[field] = bad
                self.assertEqual(status(words),3)
        for field in (3,4,5,6,10):
            words = list(baseline[1]); words[field] = oracle.bits(-1)
            self.assertEqual(status(words),3)
        for field in (0,1):
            words = list(baseline[1]); words[field] = 0x46fff001
            self.assertEqual(status(words),3)
        words = list(baseline[1]); words[6] = 0x40c90fdc
        self.assertEqual(status(words),3)
        words = list(baseline[1]); words[11] = 2
        self.assertEqual(status(words),4)
        self.assertEqual(status(baseline[1],[(0x7fc00000,1,1)]),3)
        for reply in ((0,2,1),(0,1,2)): self.assertEqual(status(baseline[1],[reply]),4)
        self.assertEqual(subprocess.run([str(self.binary)],input=b'partial',capture_output=True).returncode,6)
        self.assertEqual(subprocess.run([str(self.binary)],input=struct.pack('<13I',*baseline[1],0),
                                       capture_output=True).returncode,4)


if __name__ == '__main__': unittest.main(verbosity=2)
