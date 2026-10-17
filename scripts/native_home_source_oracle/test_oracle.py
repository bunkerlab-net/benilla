#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Cheap Home contracts; --core also checks pinned extraction and negative guards."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import struct
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import run_oracle as oracle

ROOT = Path(__file__).resolve().parent
CORE = None


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def fragments():
    result = {}
    for part in (ROOT / 'source_excerpts.test-fixture').read_text().split('\n// EXCERPT ')[1:]:
        name, data = part.split('\n', 1)
        if name in result:
            raise ValueError('duplicate excerpt: ' + name)
        result[name] = data[:-1]
    return result


class SourceContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads((ROOT / 'vectors.json').read_text())
        cls.rows = {row['case']: row for row in cls.document['snapshots']}
        cls.meta = json.loads((ROOT / 'source_intervals.json').read_text())
        cls.excerpts = fragments()

    def row(self, name):
        return self.rows[name]

    def ordered(self, trace, *events):
        indices = [trace.index(event) for event in events]
        self.assertEqual(indices, sorted(set(indices)))

    def test_pins_and_bounded_provenance(self):
        self.assertEqual(self.document['schema'], 1)
        self.assertEqual(self.document['source_revision'], oracle.PIN)
        self.assertEqual(self.meta['source_revision'], oracle.PIN)
        self.assertEqual(self.meta['classic_db_revision'], oracle.CLASSIC_DB_PIN)
        self.assertFalse(self.meta['classic_db_rows_consumed'])
        self.assertEqual(len(self.rows), len(self.document['snapshots']))
        self.assertEqual(len(self.rows), 15)
        self.assertEqual(self.meta['source_manifest_sha256'], oracle.SOURCE_MANIFEST_SHA256)
        self.assertEqual(hashlib.sha256((ROOT / 'source_hashes.sha256').read_bytes()).hexdigest(), oracle.SOURCE_MANIFEST_SHA256)
        self.assertIn('production admission', self.meta['excluded'])
        self.assertIn('cross-polygon routes', self.meta['excluded'])

    def test_exact_source_excerpts_and_compilation_labels(self):
        records = self.meta['source_intervals']
        self.assertEqual(len({record['name'] for record in records}), len(records))
        self.assertEqual(set(self.excerpts), {r['name'] for r in records} | {'BuildPolyPath.selected.inc'})
        driver = (ROOT / 'oracle_driver.cpp').read_text()
        for record in records:
            with self.subTest(excerpt=record['name']):
                data = self.excerpts[record['name']].encode('latin1')
                self.assertEqual(hashlib.sha256(data).hexdigest(), record['sha256'])
                self.assertEqual(len(data), record['last_byte_exclusive'] - record['first_byte'])
                self.assertGreaterEqual(record['last_line'], record['first_line'])
                self.assertEqual('#include "' + record['name'] + '"' in driver, record['compiled'])
        self.assertIn('#include "BuildPolyPath.selected.inc"', driver)
        oracle.verify_contract(self.excerpts)

    def test_only_two_cold_distance_initializers_are_patched(self):
        lines = (ROOT / 'cold-distance-zero.patch').read_text().splitlines()
        self.assertEqual([x for x in lines if x.startswith('-') and not x.startswith('---')],
                         ['-    float distToStartPoly, distToEndPoly;'])
        self.assertEqual([x for x in lines if x.startswith('+') and not x.startswith('+++')],
                         ['+    float distToStartPoly = 0.0f, distToEndPoly = 0.0f;'])
        self.assertEqual(self.meta['compatibility_policy'], oracle.POLICY)
        self.assertEqual(self.document['compatibility_policy'], oracle.POLICY)
        self.assertFalse(self.meta['original_cold_execution'])
        self.assertFalse(self.meta['wander_endpoint_repair_applied'])
        self.assertFalse(any('ComputePathToRandomPoint' in name for name in self.excerpts))

    def test_semantic_mutation_guards(self):
        mutations = [
            ('HomeSetTarget.inc', '), true);', '), false);'),
            ('HomeSetTarget.inc', '> 150.f * 150.f', '>= 150.f * 150.f'),
            ('HomeSetTarget.inc', 'DIST_CALC_NONE', 'DIST_CALC_BOUNDING_RADIUS'),
            ('HomeSetTarget.inc', 'if (pos.IsEmpty())', 'if (!pos.IsEmpty())'),
            ('HomeSetTarget.inc', '!owner.IsInWorld() ||', '!owner.IsInWorld() &&'),
            ('MoveHome.inc', 'Clear(false);', 'Clear(true);'),
            ('Mutate.inc', 'top()->Interrupt(*m_owner);', '/* interrupt omitted */'),
            ('DirectExpire.inc', 'if (reset && top() == nowTop)', 'if (reset)'),
            ('RandomReset.inc', 'i_nextMoveTimer.Reset(0);', 'i_nextMoveTimer.Reset(500);'),
            ('InterruptMoving.inc', 'UpdateSplinePosition(true);', 'UpdateSplinePosition(false);'),
            ('LaunchStart.prefix.inc', 'args.path[0] = real_position;', '/* stale first point */'),
            ('Calculate.inc', 'm_straightLine = straightLine;', 'm_straightLine = true;'),
            ('BuildPolyPath.selected.inc', 'distToEndPoly = 0.0f;', 'distToEndPoly = 1.0f;'),
            ('GetDistance.context.inc', 'default: return dist;', 'default: return sqrt(dist);'),
        ]
        for name, before, after in mutations:
            with self.subTest(excerpt=name, mutation=before):
                changed = dict(self.excerpts)
                self.assertIn(before, changed[name])
                changed[name] = changed[name].replace(before, after)
                with self.assertRaisesRegex(ValueError, 'source contract changed'):
                    oracle.verify_contract(changed)

    def test_callback_order_and_same_poly_return_mutations_reject(self):
        changed = dict(self.excerpts)
        changed['HomeFinalize.inc'] = changed['HomeFinalize.inc'].replace(
            'owner.LoadCreatureAddon(true);\n        owner.TriggerHomeEvents();',
            'owner.TriggerHomeEvents();\n        owner.LoadCreatureAddon(true);')
        with self.assertRaisesRegex(ValueError, 'source contract changed'):
            oracle.verify_contract(changed)
        changed = dict(self.excerpts)
        prefix = changed['BuildPolyPath.original-prefix.inc']
        first = prefix.index('if (startPoly == endPoly)')
        changed['BuildPolyPath.original-prefix.inc'] = prefix[:first] + prefix[first:].replace('return;', '/* no terminal */')
        with self.assertRaisesRegex(ValueError, 'source contract changed'):
            oracle.verify_contract(changed)

    def test_force_true_straight_false_returns_normal_before_rewrite(self):
        for row in self.rows.values():
            if row['path_type'] != 1:
                continue
            with self.subTest(case=row['case']):
                self.assertTrue(row['force'])
                self.assertFalse(row['straight'])
                self.assertEqual((row['active_polys'], row['velocity_bits']), (1, 0))
                self.assertEqual(len(row['launched_points_bits']), 2)
                self.assertEqual(row['launched_points_bits'][-1], row['goal_bits'])
                self.assertEqual(len(row['storage']), 75)
                self.assertEqual(row['storage'][0], row['storage'][74])
                self.assertGreater(row['storage'][0], 0)
                self.assertEqual(row['storage'][1:74], [0] * 73)
                self.assertEqual([x for x in row['trace'] if x.startswith('detour:')],
                                 ['detour:nearest:5', 'detour:closest-retained'])

    def test_combat_origin_is_first_capture_not_respawn(self):
        row = self.row('combat-origin-preserved')
        self.assertEqual(row['goal_bits'], [bits(14), bits(15), 0])
        self.assertEqual(row['facing_bits'], bits(.75))
        self.assertEqual(row['trace'].count('owner:capture-combat-start'), 1)
        self.assertNotIn('owner:respawn-read', row['trace'])
        self.assertFalse(row['walk'])

    def test_zero_xyz_falls_back_even_with_nonzero_orientation(self):
        row = self.row('zero-origin-respawn-fallback')
        self.assertEqual(row['goal_bits'], [bits(8), bits(8), 0])
        self.assertEqual(row['facing_bits'], bits(.25))
        self.assertEqual(row['trace'].count('owner:respawn-read'), 1)
        self.assertTrue(row['walk'])

    def test_idle_launch_correction_is_not_a_prequery_relocation(self):
        row = self.row('idle-default-stale-launch')
        self.assertEqual(row['query_origin_bits'], [bits(20), bits(20), 0])
        self.assertEqual(row['launched_points_bits'][0], [bits(21), bits(20), 0])
        self.assertNotIn('owner:forced-relocation', row['trace'])
        self.ordered(row['trace'], 'detour:nearest:5', 'external:spline-compute-position', 'external:spline-launch')

    def test_retained_wander_second_interrupt_precedes_home_query(self):
        row = self.row('wander-chase-home-arrival-reset')
        self.assertEqual(row['query_origin_bits'], [bits(21), bits(20), 0])
        self.assertEqual(row['launched_points_bits'][0], row['query_origin_bits'])
        self.ordered(row['trace'], 'chase:finalize', 'external:spline-compute-position',
                     'owner:forced-relocation', 'spline:interrupt', 'owner:active',
                     'owner:combat-start-read', 'owner:distance-check', 'detour:nearest:5', 'external:spline-launch')

    def test_second_interrupt_precedes_both_directions_of_far_boundary(self):
        far = self.row('wander-interrupt-crosses-far-boundary-excluded')
        near = self.row('wander-interrupt-crosses-near-boundary-excluded')
        self.assertEqual(far['distance_squared_bits'], bits(151 * 151))
        self.assertEqual(near['distance_squared_bits'], bits(149 * 149))
        self.assertEqual((far['launches'], far['constructors']), (0, 1))
        self.assertEqual((near['launches'], near['constructors']), (1, 2))
        self.assertIn('owner:despawn:1', far['trace'])
        self.assertNotIn('owner:despawn:1', near['trace'])
        self.assertEqual(near['query_origin_bits'], [bits(159), bits(10), 0])
        self.assertEqual(near['path_type'], 17)
        for row in [far, near]:
            self.ordered(row['trace'], 'chase:finalize', 'owner:forced-relocation',
                         'owner:active', 'owner:distance-check')
            self.assertFalse(any(x.startswith('detour:') for x in row['trace']))

    def test_arrival_callbacks_clear_then_reset_retained_wander(self):
        row = self.row('wander-chase-home-arrival-reset')
        self.ordered(row['trace'], 'owner:evade:0', 'callback:addon-reload',
                     'callback:home-events', 'owner:inactive', 'owner:clear-combat-start',
                     'owner:add-state:32768', 'owner:clear-target')
        self.assertEqual((row['active'], row['evade'], row['stack']), (False, 0, 1))
        self.assertEqual((row['retained_wander_count'], row['retained_wander_timer']), (2, 0))
        self.assertEqual((row['constructors'], row['launches']), (3, 1))
        self.assertEqual(row['combat_start_bits'], [0, 0, 0])
        self.assertFalse(any(x.startswith('rng:') for x in row['trace']))

    def test_interrupted_home_clears_evade_without_arrival_callbacks(self):
        row = self.row('home-interrupted-before-arrival')
        self.assertEqual((row['active'], row['evade'], row['stack']), (True, 0, 2))
        self.assertNotEqual(row['combat_start_bits'], [0, 0, 0])
        self.assertFalse(any(x.startswith('callback:') for x in row['trace']))

    def test_already_active_owner_and_faction_restore(self):
        row = self.row('already-active-arrival-faction-restore')
        self.assertTrue(row['active'])
        self.assertNotIn('owner:inactive', row['trace'])
        self.ordered(row['trace'], 'owner:restore-faction', 'callback:addon-reload', 'callback:home-events')
        self.assertEqual(row['combat_start_bits'], [0, 0, 0])

    def test_lost_control_and_not_move_are_distinct(self):
        lost = self.row('lost-control-refusal')
        blocked = self.row('not-move-no-path')
        self.assertEqual(lost['trace'], [])
        self.assertEqual(blocked['trace'], ['owner:active', 'owner:evade:1'])
        for row in [lost, blocked]:
            self.assertEqual((row['constructors'], row['launches'], row['stack']), (0, 0, 1))

    def test_far_distance_equality_and_adjacent_float(self):
        equal = self.row('outdoor-distance-equality-excluded')
        above = self.row('outdoor-distance-next-float-excluded')
        self.assertEqual(equal['distance_squared_bits'], bits(150 * 150))
        self.assertGreater(above['distance_squared_bits'], equal['distance_squared_bits'])
        self.assertEqual((equal['launches'], equal['constructors']), (1, 1))
        self.assertNotIn('owner:despawn:1', equal['trace'])
        for row in [above, self.row('outdoor-long-distance-excluded')]:
            self.assertEqual((row['launches'], row['constructors']), (0, 0))
            self.ordered(row['trace'], 'callback:home-events', 'owner:clear-combat-start',
                         'owner:respawn-delay:5', 'owner:despawn:1')

    def test_excluded_shortcuts_and_real_polygon_miss(self):
        for name in ['ignore-pathfinding-excluded', 'outdoor-distance-equality-excluded']:
            row = self.row(name)
            self.assertEqual((row['path_type'], row['active_polys'], row['velocity_bits']), (17, 0, 0))
            self.assertFalse(any(x.startswith('detour:') for x in row['trace']))
        row = self.row('missing-polygon-400-excluded')
        self.assertEqual((row['path_type'], row['active_polys'], row['velocity_bits']), (8, 0, bits(400)))
        self.assertEqual([x for x in row['trace'] if x.startswith('detour:')],
                         ['detour:nearest:5', 'detour:closest-retained', 'detour:nearest:5', 'detour:nearest:10'])


class CheckoutContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if CORE is None:
            raise unittest.SkipTest("pinned checkout requires --core")
        cls.extraction = oracle.extract(CORE)

    def test_pinned_checkout_matches_bundled_source(self):
        oracle.verify_bundle(*self.extraction)

    def test_every_whole_file_hash_rejects_drift(self):
        sources = self.extraction[0]
        original_read = Path.read_bytes
        for name in sources:
            with self.subTest(source=name):
                path = CORE / name
                raw = original_read(path)

                def read_bytes(candidate):
                    return raw + b'\n// changed\n' if candidate == path else original_read(candidate)

                with mock.patch.object(Path, 'read_bytes', read_bytes):
                    with self.assertRaisesRegex(ValueError, 'pinned source hash mismatch'):
                        oracle.extract(CORE)

    def test_rehashed_bundled_excerpt_cannot_bypass_original_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ['source_excerpts.test-fixture', 'source_intervals.json', 'cold-distance-zero.patch']:
                shutil.copyfile(ROOT / name, root / name)
            package = root / 'source_excerpts.test-fixture'
            package.write_text(package.read_text().replace('> 150.f * 150.f', '>= 150.f * 150.f'))
            meta = copy.deepcopy(json.loads((root / 'source_intervals.json').read_text()))
            for record in meta['source_intervals']:
                if record['name'] == 'HomeSetTarget.inc':
                    changed = self.extraction[1]['HomeSetTarget.inc'].replace('> 150.f * 150.f', '>= 150.f * 150.f')
                    record['sha256'] = oracle.sha(changed.encode('latin1'))
            (root / 'source_intervals.json').write_text(json.dumps(meta))
            with self.assertRaisesRegex(ValueError, 'bundled source mismatch'):
                oracle.verify_bundle(*self.extraction, bundle=root)
            shutil.copyfile(ROOT / 'source_excerpts.test-fixture', package)
            with self.assertRaisesRegex(ValueError, 'bundled source metadata mismatch'):
                oracle.verify_bundle(*self.extraction, bundle=root)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--core', type=Path)
    args = parser.parse_args()
    CORE = args.core.resolve() if args.core else None
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SourceContract)
    if CORE is not None:
        suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(CheckoutContract))
    sys.exit(not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful())
