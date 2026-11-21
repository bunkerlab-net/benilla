#!/usr/bin/env python3
"""Noncompiling source/management guards and generated origin-product mutation tests."""
# SPDX-License-Identifier: GPL-3.0-or-later
import copy
import json
import os
from pathlib import Path
import sys
import subprocess
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import retained_straight_origin_generate as oracle

SOURCE = Path(sys.argv.pop(1)).resolve()


class OriginSourceGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guard_sources(SOURCE)
        cls.harness, cls.package, cls.spans = oracle.build_harness(cls.sources)

    def test_complete_original_methods_and_warm_locals(self):
        source = self.sources[oracle.straight.CPP]
        for signature, _ in oracle.straight.METHODS:
            self.assertIn(oracle.straight.exact_block(source, signature), self.harness)
        warm = oracle.straight.exact_block(self.harness, 'const char* PathFinder::original_retained_straight(')
        prefix = oracle.straight.source_span(source, oracle.cold_same.cold.ORIGINAL, '    dtStatus dtResult;')
        self.assertIn(prefix, warm.replace('    evidence.afterStart = m_pathPolyRefs;\n', '', 1))
        self.assertIn(oracle.cold_same.cold.ORIGINAL, warm)
        self.assertNotIn(oracle.cold_same.cold.REPAIRED, warm)
        self.assertIn('require(m_polyLength>0,', warm)
        self.assertIn('m_navMesh->isValidPolyRef(m_pathPolyRefs[i])', warm)
        self.assertIn('selected_transition(startPoly,endPoly,startPoint,endPoint);', warm)

    def test_exact_existing_cold_projection_and_one_repair(self):
        cold_harness, _ = oracle.cold_same.build_harness(self.sources)
        signature = 'void PathFinder::compatibility_same_polygon('
        self.assertEqual(oracle.straight.exact_block(cold_harness, signature),
                         oracle.straight.exact_block(self.harness, signature))
        self.assertEqual(self.harness.count(oracle.cold_same.cold.REPAIRED), 1)
        self.assertNotIn('m_defaultNavMeshQuery', self.harness)
        self.assertNotIn('void PathFinder::BuildPolyPath(', self.harness)
        for name in ('struct ConstructorCache', 'struct ColdEvidence'):
            self.assertEqual(oracle.straight.exact_block(cold_harness, name),
                             oracle.straight.exact_block(self.harness, name))

    def test_live_objects_and_no_prior_type_assignment(self):
        main = oracle.straight.exact_block(self.harness, 'int main(int argc,char** argv)')
        step = oracle.straight.exact_block(self.harness, 'static void origin_step(')
        sequence = oracle.straight.exact_block(self.harness, 'static void origin_sequence(')
        self.assertEqual(main.count('Loaded '), 2)
        self.assertEqual(main.count('PathFinder '), 2)
        self.assertIn('normal.m_navMesh!=incomplete.m_navMesh', main)
        self.assertIn('&normal.observer.query!=&incomplete.observer.query', main)
        self.assertNotIn('PathFinder path(', step)
        self.assertNotIn('path.m_type=', step)
        self.assertNotIn('path.m_pathPolyRefs=', step)
        self.assertNotIn('configure(', step)
        self.assertIn('const auto previous=path.m_type;', step)
        self.assertIn('path.compatibility_same_polygon(', step)
        self.assertIn('else selected_phase=path.original_retained_straight(', step)
        self.assertIn('&path==live_object', sequence)
        self.assertIn('path.m_navMeshQuery==live_observer', sequence)
        self.assertIn('live_query==&mesh.query', sequence)
        self.assertIn('path.m_filter.getIncludeFlags()==13', step)
        self.assertIn('path.m_filter.getExcludeFlags()==0', step)

    def test_schema_is_original_straight_plus_origin_metadata(self):
        self.assertEqual(len(set(oracle.FIELDS)), len(oracle.FIELDS))
        self.assertEqual(oracle.FIELDS[9:], oracle.straight.FIELDS[1:])
        self.assertIn('# ' + '\\t'.join(oracle.FIELDS) + '\\n', self.harness)
        self.assertIn('# ' + '\\t'.join(oracle.TRACE_FIELDS) + '\\n', self.harness)
        self.assertNotIn('CASE_HEADER', self.harness)

    def test_pin_hash_and_source_span_mutation_rejected(self):
        damaged = dict(self.sources)
        damaged[oracle.straight.CPP] = damaged[oracle.straight.CPP].replace(
            oracle.cold_same.cold.ORIGINAL, '    float distToStartPoly = 1, distToEndPoly = 1;', 1)
        with self.assertRaises(SystemExit):
            oracle.build_harness(damaged)
        with self.assertRaises(SystemExit):
            oracle.replace_once('marker marker', 'marker', 'replacement')
        self.assertEqual(self.spans['retained_prefix']['source'], oracle.straight.CPP + ':410-415')

    def test_unmanaged_compiler_is_rejected_without_execution(self):
        with mock.patch.dict(os.environ, {'CXX': '/usr/bin/g++', 'MBX': '/safe/managed/mbx'}), \
                mock.patch.object(oracle.shutil, 'which', return_value='/usr/bin/g++'):
            with self.assertRaises(SystemExit):
                oracle.compiler_command()

    def test_empty_compiler_and_low_space_are_rejected_without_execution(self):
        with mock.patch.dict(os.environ, {'CXX': ''}):
            with self.assertRaises(SystemExit):
                oracle.compiler_command()
        with mock.patch.object(oracle.shutil, 'disk_usage', return_value=mock.Mock(free=oracle.MIN_FREE - 1)):
            with self.assertRaises(SystemExit):
                oracle.require_space(oracle.ROOT)

    def test_failed_command_preserves_and_prints_both_streams_without_retry(self):
        failure = subprocess.CalledProcessError(1, ['compiler'], output=b'captured stdout\n',
                                               stderr=b'actual compiler error\n')
        with mock.patch.object(oracle.straight, 'run', side_effect=failure) as run, \
                mock.patch.object(Path, 'write_bytes') as write_bytes, \
                mock.patch.object(Path, 'write_text') as write_text, \
                mock.patch('builtins.print') as printed:
            with self.assertRaises(subprocess.CalledProcessError) as observed:
                oracle.run_preserving_diagnostics(['compiler'], oracle.ROOT)
            self.assertIs(observed.exception, failure)
            run.assert_called_once()
            self.assertEqual([call.args[0] for call in write_bytes.call_args_list],
                             [b'captured stdout\n', b'actual compiler error\n'])
            self.assertEqual(json.loads(write_text.call_args.args[0])['returncode'], 1)
            self.assertTrue(any('actual compiler error' in str(call.args) for call in printed.call_args_list))

    def test_no_temporary_directory_or_cleanup_and_serial_tus(self):
        generator = Path(oracle.__file__).read_text()
        self.assertNotIn('TemporaryDirectory(', generator)
        self.assertNotIn('rmtree(', generator)
        self.assertNotIn('.unlink(', generator)
        self.assertIn("'TMPDIR': str(work / 'tmp')", generator)
        self.assertIn("['-c', str(source), '-o', str(output)]", generator)
        self.assertIn('for index, source in enumerate(inputs):', generator)
        self.assertIn('path.is_relative_to(managed)', generator)

    def test_provenance_binds_originals_harness_and_authored_inputs(self):
        receipt = oracle.provenance(self.sources, self.harness, self.package, self.spans)
        self.assertEqual(receipt['source_hashes'][oracle.straight.CPP],
                         oracle.sha(self.sources[oracle.straight.CPP].encode('latin1')))
        self.assertIn('straight_path_source_fragments.test-fixture', receipt['local_dependency_hashes'])
        self.assertIn('cold_same_poly_source_fragments.test-fixture', receipt['local_dependency_hashes'])
        self.assertEqual(receipt['original_cold'], 'undefined-not-invoked-no-parity-claim')
        self.assertEqual(receipt['query_nodes'], 1024)
        self.assertEqual(receipt['include_filter'], 13)
        self.assertEqual(len(receipt['authored_input_hashes']), 2)


class OriginProductGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not oracle.FIXTURES.exists():
            raise unittest.SkipTest('No source-executed origin products yet; no expected outputs synthesized')
        cls.products = {name: (oracle.FIXTURES / name).read_bytes() for name in oracle.FILES}
        cls.rows, cls.traces = oracle.verify_products(cls.products)

    def changed_row(self, label, key, value):
        rows = copy.deepcopy(self.rows)
        next(row for row in rows if row['label'] == label)[key] = value
        result = dict(self.products)
        result['cases.test-fixture'] = ('# ' + '\t'.join(oracle.FIELDS) + '\n' + '\n'.join(
            '\t'.join(row[field] for field in oracle.FIELDS) for row in rows) + '\n').encode('ascii')
        return result

    def test_five_actual_rows_and_manifest_outputs(self):
        self.assertEqual(len(self.rows), 5)
        receipt = json.loads(oracle.PROVENANCE.read_text())
        self.assertEqual(receipt['case_count'], len(self.rows))
        self.assertEqual(receipt['trace_count'], len(self.traces))
        for name, data in self.products.items():
            self.assertEqual(receipt['output_hashes'][name], oracle.sha(data))

    def test_synthetic_previous_type_rejected(self):
        with self.assertRaises(SystemExit):
            oracle.verify_products(self.changed_row('incomplete_no_hit', 'previous_type', '1'))

    def test_state_adjacency_filter_and_repair_label_rejected(self):
        for key, value in [('before_storage', ','.join(['0'] * 74)), ('include_filter', '1'),
                           ('oracle', 'compatibility-zero-v1'), ('point_limit', '75')]:
            with self.subTest(key=key), self.assertRaises(SystemExit):
                oracle.verify_products(self.changed_row('normal_no_hit', key, value))

    def test_extra_product_and_missing_trace_rejected(self):
        bad = dict(self.products, extra=b'unexpected')
        with self.assertRaises(SystemExit):
            oracle.verify_products(bad)
        bad = dict(self.products)
        lines = bad['traces.test-fixture'].splitlines()
        del lines[1]
        bad['traces.test-fixture'] = b'\n'.join(lines) + b'\n'
        with self.assertRaises(SystemExit):
            oracle.verify_products(bad)


if __name__ == '__main__':
    unittest.main()
