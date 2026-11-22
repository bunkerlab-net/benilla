#!/usr/bin/env python3
"""Fail-closed selected compatibility provenance and fixture guards; no original cold execution."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import sys
import tempfile
import unittest
sys.dont_write_bytecode = True
import cold_distance_generate as cold
ROOT=Path(sys.argv.pop(1)).resolve()

class ColdDistanceGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources=cold.warm.guarded_sources(ROOT)
        cold.guard_inputs()
        cls.rows=cold.parse_cases((cold.FIXTURES/'cases.test-fixture').read_bytes())
        cls.traces=cold.warm.parse_traces((cold.FIXTURES/'traces.test-fixture').read_bytes())

    def named(self,label): return next(row for row in self.rows if row['label']==label)

    def test_only_two_caller_locals_change_and_full_lookup_bodies_remain_exact(self):
        harness,package=cold.build_harness(self.sources)
        self.assertEqual(package,cold.PACKAGE.read_text())
        prefix=cold.warm.source_span(self.sources[cold.warm.CPP],cold.ORIGINAL,'    dtStatus dtResult;')
        repaired=cold.repair_distance_initialization(prefix)
        self.assertEqual(repaired.replace(cold.REPAIRED,cold.ORIGINAL),prefix)
        for signature,_ in cold.warm.METHODS:
            self.assertIn(cold.warm.exact_block(self.sources[cold.warm.CPP],signature),harness)
        self.assertIn(cold.REPAIRED,harness)
        self.assertNotIn('m_defaultNavMeshQuery',harness)
        self.assertNotIn('void PathFinder::BuildPolyPath(',harness)
        self.assertNotIn('void PathFinder::BuildPointPath(',harness)
        self.assertEqual(cold.POLICY,'cmangos-buildpolypath-zero-distance-v1')
        self.assertEqual(cold.VERSION,1)

    def test_initializer_drift_missing_duplicate_preinitialized_refused(self):
        for prefix in ('',cold.ORIGINAL*2,cold.REPAIRED,cold.ORIGINAL.replace('distToEndPoly','endDistance')):
            with self.assertRaisesRegex(SystemExit,'initialization drift'): cold.repair_distance_initialization(prefix)

    def test_original_undefined_cold_is_unclaimed_and_never_invoked(self):
        # The existing original observer remains unchanged and continues excluding cold.
        original=cold.warm.parse_cases((cold.warm.FIXTURES/'cases.test-fixture').read_bytes())
        row=next(row for row in original if row['label']=='cold_without_tail_excluded')
        self.assertEqual(row['scope'],'excluded');self.assertEqual(row['phase'],'excluded')
        self.assertTrue(all(row['oracle']=='compatibility-zero-v1' for row in self.rows))
        self.assertEqual(len(self.rows),14)
        self.assertIn('original cold undefined branch NOT invoked',(cold.FIXTURES/'cases.test-fixture').read_text())

    def test_zero_append_first_then_sequential_tail_and_second_initialized_local(self):
        same=self.named('cold_same_tail');refs=cold.warm.ref_list(same['lookup_storage'])
        self.assertEqual(refs[:74],[0]*74);self.assertEqual(len(refs),75)
        self.assertEqual(same['start_ref'],same['end_ref']);self.assertEqual(same['start_distance'],'00000000')
        traces=[row['primitive'] for row in self.traces if row['label']=='cold_same_tail']
        self.assertEqual(traces,['nearest5','tail'])
        replacement=self.named('cold_replacement');refs=cold.warm.ref_list(replacement['lookup_storage'])
        self.assertEqual(refs[:74],[0]*74);self.assertEqual(len(refs),76)
        second=self.named('cold_first_miss_second_hit')
        self.assertEqual(second['start_ref'],'0');self.assertEqual(second['start_distance'],'00000000')
        self.assertNotEqual(second['end_ref'],'0');self.assertEqual(len(cold.warm.ref_list(second['lookup_storage'])),75)

    def test_bounds_and_raw_statuses_remain_outside_admission(self):
        self.assertEqual(self.named('cold_nearest10')['start_distance'],'40c00000')
        self.assertEqual(self.named('cold_far')['phase'],'far')
        for name in ('cold_both_miss','cold_first_hit_second_miss','cold_first_miss_second_hit'):
            self.assertEqual(self.named(name)['phase'],'missing')
        for name,status in [('cold_partial','40000040'),('long600_cold_rejected','40000010'),('long1100_cold_rejected','40000070'),('cold_75_truncated','40000010')]:
            self.assertEqual(self.named(name)['direct_status'],status)
        self.assertEqual(self.named('cold_74_exact')['after_active'],'74')

    def test_policy_label_and_malformed_rows_fail_closed(self):
        data=(cold.FIXTURES/'cases.test-fixture').read_text()
        for old,new in [('compatibility-zero-v1\t','original-parity\t'),('cold_same_tail\t','bad label\t')]:
            with self.assertRaises(SystemExit): cold.parse_cases(data.replace(old,new,1).encode())

    def test_compiled_selected_source_observation_matches_committed_fixture(self):
        with tempfile.TemporaryDirectory(prefix='cold-guard-') as temporary:
            work=Path(temporary);binary=cold.compile_oracle(self.sources,work)
            observed=cold.observe(binary,work)
        for name,data in observed.items(): self.assertEqual(data,(cold.FIXTURES/name).read_bytes(),name)

if __name__=='__main__': unittest.main()
