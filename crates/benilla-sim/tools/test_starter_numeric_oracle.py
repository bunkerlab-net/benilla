#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""No-compiler source-integrity, adapter-boundary, and generated-vector checks."""
import collections
import hashlib
import pathlib
import tempfile
import unittest
from unittest import mock

import generate_starter_numeric_fixture as oracle


class NumericOracleTests(unittest.TestCase):
    def test_source_integrity_and_transport(self):
        text = 'unchanged source\n'
        digest = hashlib.sha256(text.encode()).hexdigest()
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / 'source.cpp'
            path.write_bytes(text.replace('\n', '\r\n').encode())
            self.assertEqual(oracle.checked_text(path, digest), text)
            for raw in [b'changed source\n', b'unchanged source\r']:
                path.write_bytes(raw)
                with self.assertRaisesRegex(ValueError, 'altered source'):
                    oracle.checked_text(path, digest)

    def test_complete_body_and_signature_drift(self):
        source = 'prefix int32 GetSpellDuration(int x) { if (x) { call(); } } suffix'
        signature = 'int32 GetSpellDuration('
        self.assertEqual(oracle.body(source, signature),
                         'int32 GetSpellDuration(int x) { if (x) { call(); } }\n')
        for invalid in [source + source, source.replace('GetSpellDuration', 'Other'), source[:-9]]:
            with self.assertRaises(ValueError):
                oracle.body(invalid, signature)

    def test_marker_and_scope_drift(self):
        parts = {key: f'original {key}\n' for key in set(oracle.SIGNATURES) | oracle.EXTRA_PARTS}
        harness = ''.join('// PINNED_' + key + '\n' for key in parts)
        assembled = oracle.assemble(parts, harness)
        for fragment in parts.values():
            self.assertIn(fragment, assembled)
        for invalid in [harness + '// PINNED_CAST_TIME\n', harness.replace('// PINNED_CAST_TIME\n', ''),
                        harness + '// PINNED_UNKNOWN\n']:
            with self.assertRaises(ValueError):
                oracle.assemble(parts, invalid)
        with self.assertRaises(ValueError):
            oracle.assemble({'CAST_TIME': parts['CAST_TIME']}, harness)

    def test_type_constant_and_extraction_checks(self):
        # Authored miniature sources exercise checks without requiring an upstream checkout.
        sources = {
            'src/game/Spells/SpellMgr.cpp': '/* CMaNGOS GPL notice */\n' + ''.join(
                signature + 'int x) { if (x) { x += 1; } return x; }\n'
                for signature in oracle.SIGNATURES.values()),
            'src/game/Server/DBCStructure.h':
                'struct SpellCastTimesEntry\n{ uint32 ID; int32 CastTime; };\n'
                'struct SpellDurationEntry\n{ uint32 ID; int32 Duration[3]; };\n'
                'uint32    baseLevel; uint32    DurationIndex; uint32    CastingTimeIndex;\n',
            'src/game/Entities/Unit.h': 'virtual uint32 GetSpellRank(SpellEntry const* spellInfo) const;',
            'src/game/Entities/Player.h': 'TRADE_SLOT_NONTRADED = 6',
            'src/game/Globals/SharedDefines.h': 'RANGED_ATTACK = 2',
            'src/game/Entities/UpdateFields.h': 'OBJECT_END = 0x06, UNIT_MOD_CAST_SPEED = 0x8b + OBJECT_END',
            'src/game/Spells/SpellDefines.h':
                'SPELL_ATTR_USES_RANGED_SLOT = 0x00000002, SPELL_ATTR_IS_ABILITY = 0x00000010, '
                'SPELL_ATTR_IS_TRADESKILL = 0x00000020, SPELL_ATTR_EX3_IGNORE_CASTER_MODIFIERS = 0x20000000, '
                'SPELLMOD_CASTING_TIME = 10',
        }
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            hashes = {}
            for path, text in sources.items():
                destination = root / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(text)
                hashes[path] = hashlib.sha256(text.encode()).hexdigest()
            with mock.patch.dict(oracle.HASHES, hashes, clear=True):
                parts = oracle.fragments(root)
                self.assertEqual(set(parts), set(oracle.SIGNATURES) | oracle.EXTRA_PARTS)
                self.assertEqual(parts['LICENSE'], '/* CMaNGOS GPL notice */\n')
                for key, signature in oracle.SIGNATURES.items():
                    self.assertEqual(parts[key], oracle.body(sources['src/game/Spells/SpellMgr.cpp'], signature))
                path = 'src/game/Server/DBCStructure.h'
                changed = sources[path].replace('uint32    baseLevel;', 'int32    baseLevel;')
                (root / path).write_text(changed)
                with self.assertRaisesRegex(ValueError, 'altered source'):
                    oracle.fragments(root)
                oracle.HASHES[path] = hashlib.sha256(changed.encode()).hexdigest()
                with self.assertRaisesRegex(ValueError, 'field drift'):
                    oracle.fragments(root)
                (root / path).write_text(sources[path])
                oracle.HASHES[path] = hashes[path]
                path = 'src/game/Spells/SpellDefines.h'
                changed = sources[path].replace('SPELLMOD_CASTING_TIME = 10', 'SPELLMOD_CASTING_TIME = 11')
                (root / path).write_text(changed)
                oracle.HASHES[path] = hashlib.sha256(changed.encode()).hexdigest()
                with self.assertRaisesRegex(ValueError, 'constant drift'):
                    oracle.fragments(root)

    def test_authored_boundaries_and_arithmetic_exclusions_are_explicit(self):
        source = (oracle.HERE / 'starter_numeric_oracle.cpp').read_text()
        for text in ['NOT their CMaNGOS implementations', 'does not prove old-triggered semantics',
                     'abs(INT32_MIN)', 'excluded rank conversion', 'excluded ranged overflow',
                     'excluded modifier overflow', 'excluded float-to-int32 conversion',
                     'excluded unsigned-to-int32 conversion', 'excluded null caster with spell',
                     '11 arithmetic/pointer exclusion probes passed']:
            self.assertIn(text, source)
        for marker in set(oracle.SIGNATURES) | oracle.EXTRA_PARTS:
            self.assertEqual(source.count('// PINNED_' + marker + '\n'), 1)
        self.assertNotIn('CalculateSpellDuration(', source)
        generator = pathlib.Path(oracle.__file__).read_text()
        self.assertIn("'-fsanitize=undefined,float-cast-overflow'", generator)
        self.assertIn("'-fno-sanitize-recover=all'", generator)
        self.assertIn("[str(binary), '--check-exclusions']", generator)

    def rows(self):
        lines = oracle.OUTPUT.read_text().splitlines()
        self.assertTrue(all(line.startswith('#') for line in lines[:4]))
        fields = ['case', 'kind', 'category', 'present', 'query_index', 'row_id', 'base',
                  'per_level', 'bound', 'rank', 'base_level', 'source_result', 'source_max_result', 'events']
        rows = []
        for line in lines[4:]:
            values = line.split('\t')
            self.assertEqual(len(values), len(fields))
            row = dict(zip(fields, values))
            for field in fields[3:-1]:
                row[field] = int(row[field])
            rows.append(row)
        self.assertEqual(len(rows), 62)
        self.assertEqual(len({row['case'] for row in rows}), len(rows))
        self.assertEqual(collections.Counter((row['kind'], row['category']) for row in rows), {
            ('D', 'admitted'): 6, ('D', 'rejected'): 9, ('D', 'source_only'): 1,
            ('C', 'admitted'): 6, ('C', 'rejected'): 14, ('C', 'source_only'): 26,
        })
        for row in rows:
            self.assertEqual(row['case'].startswith('source_only_'), row['category'] == 'source_only')
            self.assertIn(row['present'], (0, 1))
        return {row['case']: row for row in rows}

    def test_neutral_lookup_identity_and_admission_boundary(self):
        for row in self.rows().values():
            if row['category'] == 'source_only':
                continue
            bound = row['query_index'] == row['row_id']
            if row['kind'] == 'C':
                admitted = bound and (not row['present'] or (row['base'], row['per_level'], row['bound']) == (0, 0, 0))
            else:
                admitted = bound and (not row['present'] or (row['base'] == row['bound'] and
                                                           row['base'] in (-1, 0) and row['per_level'] == 0))
            self.assertEqual(admitted, row['category'] == 'admitted', row['case'])
            state = f"Found({row['row_id']})" if row['present'] and bound else 'Missing'
            label = 'cast' if row['kind'] == 'C' else 'duration'
            lookup = f"{label}-lookup:{row['query_index']}:{state};"
            self.assertEqual(row['events'].count(lookup), 1 if row['kind'] == 'C' else 2)
            if state == 'Missing':
                self.assertEqual((row['source_result'], row['source_max_result']), (0, 0))
                self.assertNotIn('rank-adapter', row['events'])
                self.assertNotIn('mod-owner-adapter', row['events'])

    def test_missing_zero_and_permanent_are_distinct(self):
        rows = self.rows()
        for kind in ('cast', 'duration'):
            self.assertEqual(rows[kind + '_missing_zero']['source_result'], 0)
            self.assertEqual(rows[kind + '_found_zero']['source_result'], 0)
            self.assertIn(':Missing;', rows[kind + '_missing_zero']['events'])
            self.assertIn(':Found(0);', rows[kind + '_found_zero']['events'])
            self.assertIn(':Missing;', rows[kind + '_found_wrong_index']['events'])
        self.assertEqual(rows['duration_missing_21']['source_result'], 0)
        self.assertEqual(rows['duration_permanent']['source_result'], -1)
        self.assertEqual(rows['source_only_duration_null_info']['events'], '')

    def test_duration_words_are_independent_and_per_level_unused(self):
        rows = self.rows()
        for name, expected in {
            'duration_positive': (250, 500), 'duration_absolute': (250, 500),
            'duration_absolute_near_limit': (2147483647, 2147483647),
            'duration_base_permanent_max_zero': (-1, 0),
            'duration_base_zero_max_permanent': (0, -1),
            'duration_ignored_per_level': (0, 0),
            'duration_ignored_minimum_per_level': (-1, -1),
        }.items():
            self.assertEqual((rows[name]['source_result'], rows[name]['source_max_result']), expected)
            self.assertEqual(rows[name]['category'], 'rejected')

    def test_rank_division_unsigned_scaling_and_minimum_clamp(self):
        rows = self.rows()
        for name, expected in {
            'cast_rank_four': 1000, 'cast_rank_five': 1100, 'cast_rank_nine': 1100,
            'cast_rank_ten': 1200, 'cast_below_base_level': 800, 'cast_negative_per_level': 800,
            'cast_negative_base_positive_scaled': 100, 'cast_minimum_clamp': 500,
            'cast_minimum_above_scaled': 900, 'cast_negative_minimum': 0,
            'cast_per_level_zero_result': 0,
        }.items():
            self.assertEqual(rows[name]['source_result'], expected)
            self.assertEqual(rows[name]['category'], 'rejected')

    def test_trigger_trade_and_missing_store_early_exits(self):
        rows = self.rows()
        triggered = rows['source_only_redundant_trigger_true']
        self.assertEqual(triggered['events'], 'redundant-trigger-adapter:1;')
        self.assertEqual(triggered['source_result'], 0)
        self.assertEqual(rows['source_only_redundant_trigger_false']['source_result'], 1000)
        matched = rows['source_only_trade_matching_item']
        self.assertNotIn('cast-lookup', matched['events'])
        self.assertEqual(matched['source_result'], 0)
        self.assertTrue(matched['events'].endswith('trade-item-adapter:6;target-item-adapter;'))
        for suffix in ('different_item', 'empty_slot', 'missing'):
            self.assertEqual(rows['source_only_trade_' + suffix]['source_result'], 1000)
        self.assertNotIn('target-item-adapter', rows['source_only_trade_empty_slot']['events'])
        missing = rows['source_only_missing_before_rank_modifiers']
        self.assertTrue(missing['events'].endswith('cast-lookup:1:Missing;'))
        for endpoint in ('rank-adapter', 'mod-owner-adapter', 'auto-repeat-adapter', 'cast-speed-adapter'):
            self.assertNotIn(endpoint, missing['events'])

    def test_optional_spell_and_caster_paths(self):
        rows = self.rows()
        for name, expected in {
            'source_only_no_spell_unit': 1200, 'source_only_no_spell_world_rank_zero': 1000,
            'source_only_no_spell_null_caster': 1000, 'source_only_no_spell_maximum_cast': 2147483647,
        }.items():
            self.assertEqual(rows[name]['source_result'], expected)
            for endpoint in ('redundant-trigger-adapter', 'mod-owner-adapter', 'cast-speed-adapter'):
                self.assertNotIn(endpoint, rows[name]['events'])
        for name in ('source_only_no_spell_world_rank_zero', 'source_only_no_spell_null_caster'):
            self.assertNotIn('rank-adapter', rows[name]['events'])

    def test_modifier_speed_ranged_order_and_final_exception(self):
        rows = self.rows()
        expected = {
            'source_only_ranged_plus_500': 1500, 'source_only_auto_repeat_no_500': 1000,
            'source_only_ranged_without_spell': 1500,
            'source_only_spellmod_before_speed_consume': 550, 'source_only_spellmod_no_consume': 550,
            'source_only_spellmod_negative_final_clamp': 0, 'source_only_speed_truncation': 500,
            'source_only_ability_skips_cast_speed': 1000, 'source_only_tradeskill_skips_cast_speed': 1000,
            'source_only_ignore_skips_cast_speed': 1000, 'source_only_ignore_does_not_skip_spellmod': 1100,
            'source_only_ranged_ability_attack_speed': 750, 'source_only_ranged_ability_auto_repeat': 1000,
            'source_only_world_mod_owner_without_speed': 1100, 'source_only_19968_after_modifiers': 0,
        }
        for name, value in expected.items():
            self.assertEqual(rows[name]['source_result'], value)
        for name in ('source_only_spellmod_before_speed_consume', 'source_only_spellmod_no_consume',
                     'source_only_spellmod_negative_final_clamp', 'source_only_19968_after_modifiers'):
            events = rows[name]['events']
            self.assertLess(events.index('spellmod-adapter'), events.index('cast-speed-adapter'))
        self.assertIn(':10:1:1000->1100;', rows['source_only_spellmod_before_speed_consume']['events'])
        self.assertIn(':10:0:1000->1100;', rows['source_only_spellmod_no_consume']['events'])
        self.assertIn('spellmod-adapter:19968:10:1:', rows['source_only_19968_after_modifiers']['events'])
        for name in ('source_only_ability_skips_cast_speed', 'source_only_tradeskill_skips_cast_speed',
                     'source_only_ignore_skips_cast_speed', 'source_only_ignore_does_not_skip_spellmod',
                     'source_only_world_mod_owner_without_speed'):
            self.assertNotIn('cast-speed-adapter', rows[name]['events'])


if __name__ == '__main__':
    unittest.main()
