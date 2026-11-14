#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Cheap provenance and discriminating-vector gates, independent of Rust."""
import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent


class SourceContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads((ROOT / 'vectors.json').read_text())
        cls.rows = {(row['case'], row['frame']): row for row in cls.document['snapshots']}
        cls.meta = json.loads((ROOT / 'source_intervals.json').read_text())

    def row(self, case, frame):
        return self.rows[(case, frame)]

    def test_one_scalar_patch_is_distinct(self):
        patch = (ROOT / 'endpoint-zero.patch').read_text().splitlines()
        self.assertEqual([r for r in patch if r.startswith('-') and not r.startswith('---')], ['-    float distanceToPoly;'])
        self.assertEqual([r for r in patch if r.startswith('+') and not r.startswith('+++')], ['+    float distanceToPoly = 0.0f;'])
        self.assertEqual(self.meta['endpoint_compatibility_policy'], 'cmangos-computepathtorandompoint-zero-distance-v1')
        self.assertFalse(self.meta['buildpolypath_cold_zero_policy_applied'])
        self.assertFalse(self.meta['original_undefined_endpoint_execution'])

    def test_source_package_hashes(self):
        package = (ROOT / 'source_excerpts.test-fixture').read_text()
        fragments = {}
        for part in package.split('\n// EXCERPT ')[1:]:
            name, data = part.split('\n', 1)
            fragments[name] = data[:-1]
        for record in self.meta['source_intervals']:
            self.assertEqual(hashlib.sha256(fragments[record['name']].encode('latin1')).hexdigest(), record['sha256'])
        for suffix in ['original', 'patched']:
            data = fragments['ComputePathToRandomPoint.' + suffix + '.inc']
            self.assertEqual(hashlib.sha256(data.encode('latin1')).hexdigest(), self.meta['endpoint_' + suffix + '_sha256'])

    def test_initialize_does_not_sample_or_launch(self):
        row = self.row('pause_relaunch', 'initialized')
        self.assertEqual((row['draws'], row['launches'], row['timer'], row['count']), (0, 0, 0, 1))
        self.assertEqual(row['storage'], [0] * 74)

    def test_real_endpoint_lookup_warms_source_tail(self):
        row = self.row('pause_relaunch', 'first_launch')
        calls = [x for x in row['trace'] if x.startswith('detour:')]
        self.assertEqual(calls, ['detour:nearest:5', 'detour:height', 'detour:closest-retained', 'detour:closest-retained'])
        self.assertEqual(len(row['storage']), 75)
        self.assertEqual(row['storage'][0], row['storage'][74])
        self.assertGreater(row['storage'][0], 0)
        self.assertEqual((row['active_polys'], row['path_type']), (1, 1))

    def test_moving_freezes_pause_then_equality_relaunches(self):
        self.assertEqual(self.row('pause_relaunch', 'moving_timer_frozen')['timer'], 3000)
        self.assertEqual(self.row('pause_relaunch', 'pause_remaining_one')['launches'], 1)
        row = self.row('pause_relaunch', 'second_launch')
        self.assertEqual((row['timer'], row['count'], row['launches'], row['draws']), (0, 2, 2, 6))

    def test_failure_does_not_roll_success_delays(self):
        for case, delay in [('failure_retry_500', 500), ('failure_retry_100', 100), ('zero_duration', 500)]:
            row = self.row(case, 'failed_launch' if case == 'zero_duration' else 'failure')
            self.assertEqual((row['timer'], row['count']), (delay, 1))
            self.assertFalse(any(x.startswith('rng:integer:1:3') for x in row['trace']))

    def test_rng_order_and_walk_threshold(self):
        for case, walk in [('run_roll_14_run', False), ('run_roll_15_walk', True)]:
            row = self.row(case, 'launch')
            draws = [x for x in row['trace'] if x.startswith('rng:')]
            self.assertTrue(draws[0].startswith('rng:normalized:'))
            self.assertTrue(draws[1].startswith('rng:normalized:'))
            self.assertTrue(draws[2].startswith('rng:integer:0:99:'))
            self.assertTrue(draws[3].startswith('rng:integer:1:3:'))
            self.assertTrue(draws[4].startswith('rng:integer:3000:10000:'))
            self.assertIs(row['walk'], walk)

    def test_negative_geometry_is_actual_not_scripted(self):
        missed = self.row('actual_filtered_poly_miss', 'result')
        self.assertEqual([x for x in missed['trace'] if x.startswith('detour:')], ['detour:nearest:5', 'detour:nearest:10'])
        height = self.row('actual_outside_poly_height_failure', 'result')
        self.assertEqual([x for x in height['trace'] if x.startswith('detour:')], ['detour:nearest:5', 'detour:height'])
        self.assertEqual((missed['path_type'], height['path_type']), (8, 8))
        shortcut = self.row('no_mesh_source_shortcut', 'launch')
        self.assertEqual(shortcut['path_type'], 3)
        self.assertFalse(any(x.startswith('detour:') for x in shortcut['trace']))

    def test_authored_geometry_hashes(self):
        for name, digest in self.meta['geometry_fixtures_sha256'].items():
            self.assertEqual(hashlib.sha256((ROOT / 'fixtures' / name).read_bytes()).hexdigest(), digest)
        lines = (ROOT / 'geometry_hashes.sha256').read_text().splitlines()
        self.assertEqual(len(lines), 2)
        for line in lines:
            digest, path = line.split()
            self.assertEqual(hashlib.sha256((ROOT / path).read_bytes()).hexdigest(), digest)

    def test_defined_geometry_failures_preserve_cache_then_retry(self):
        for case, length in [('height_failure_retry', 75), ('missing_poly_retry', 74)]:
            row = self.row(case, 'failure')
            self.assertEqual((row['timer'], row['draws'], row['path_type'], row['active_polys']), (500, 2, 8, 0))
            self.assertEqual(len(row['storage']), length)
            self.assertEqual(row['points_bits'], [])
            self.assertEqual(self.row(case, 'one_ms_remaining')['draws'], 2)
            row = self.row(case, 'retry_success')
            self.assertEqual((row['timer'], row['count'], row['draws'], row['path_type']), (4000, 2, 6, 1))
        miss = self.row('actual_missing_poly_inside_loaded_tile', 'result')
        self.assertEqual([x for x in miss['trace'] if x.startswith('detour:')], ['detour:nearest:5', 'detour:nearest:10'])
        self.assertIn('owner:water', miss['trace'])
        self.assertIn('owner:above-ground', miss['trace'])

    def test_unchanged_precision_geometry_observer(self):
        names = ['precision_same_polygon', 'precision_endpoint_height_failure', 'precision_endpoint_missing']
        for name, path_type in zip(names, [1, 8, 8]):
            row = self.row(name, 'result')
            self.assertEqual(row['geometry'], 'unchanged-existing-precision-fixture')
            self.assertEqual(row['path_type'], path_type)
            self.assertIn('owner:water', row['trace'])
            self.assertIn('owner:above-ground', row['trace'])
        height = self.row('precision_endpoint_height_failure', 'result')
        self.assertEqual(height['endpoint_bits'], [0x41300000, 0x40a00000, 0])
        self.assertEqual([x for x in height['trace'] if x.startswith('detour:')], ['detour:nearest:5', 'detour:height'])
        self.assertEqual((len(height['storage']), height['active_polys'], height['points_bits']), (75, 0, []))
        missing = self.row('precision_endpoint_missing', 'result')
        self.assertEqual([x for x in missing['trace'] if x.startswith('detour:')], ['detour:nearest:5', 'detour:nearest:10'])
        self.assertEqual((len(missing['storage']), missing['active_polys'], missing['points_bits']), (74, 0, []))

    def test_close_target_rejects_after_two_draws(self):
        below = self.row('distance_below_threshold', 'result')
        self.assertEqual((below['draws'], below['launches'], below['timer']), (2, 0, 500))
        self.assertFalse(any(x.startswith('detour:') for x in below['trace']))
        self.assertNotIn('owner:water', below['trace'])
        self.assertNotIn('owner:above-ground', below['trace'])
        self.assertEqual((below['endpoint_bits'], below['storage']), ([0, 0, 0], [0] * 74))
        self.assertEqual(self.row('distance_above_threshold', 'result')['launches'], 1)

    def test_reset_and_pause_adjustment(self):
        self.assertEqual(self.row('pause_adjust_reset', 'clamped_one')['timer'], 1)
        self.assertEqual(self.row('pause_adjust_reset', 'passed_unchanged')['timer'], 0)
        row = self.row('pause_adjust_reset', 'reset')
        self.assertEqual((row['count'], row['timer'], row['draws']), (3, 0, 4))
        self.assertEqual(row['storage'], [0] * 74)

    def test_bounded_semantic_witness_and_discriminating_omissions(self):
        witness = json.loads((ROOT / 'bounded-witness.json').read_text())
        self.assertEqual(witness['operations'], 4096)
        self.assertEqual(witness['retained_operations'], 4)
        self.assertEqual(witness['appending_operations'], 1)
        self.assertEqual(witness['retained_indices'], [0, 4093, 4094, 4095])
        self.assertEqual(len(witness['final_storage']), 75)
        self.assertEqual(witness['final_storage'][1:74], [0] * 73)
        self.assertGreater(witness['final_storage'][0], 0)
        self.assertEqual(witness['final_storage'][0], witness['final_storage'][74])
        self.assertEqual(witness['final_endpoint_bits'], [0x42280000, 0x41200000, 0])
        self.assertEqual((witness['final_path_type'], witness['final_active_polys'],
                          witness['final_points']), (8, 0, 0))
        self.assertGreater(witness['before_active_differences'], 0)
        self.assertEqual(witness['continuation_operations'], 5)
        for key in ['fresh_query_allocation', 'semantic_state_equal',
                    'continuation_state_and_trace_equal', 'omit_last_normal_detected',
                    'omit_endpoint_writer_detected', 'omit_appender_detected']:
            self.assertIs(witness[key], True)


if __name__ == '__main__':
    unittest.main()
