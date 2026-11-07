#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""No-compiler extraction, adapter boundary, and generated-fixture checks."""
import hashlib
import pathlib
import tempfile
import unittest

import generate_starter_callbacks_fixture as oracle


class CallbackOracleTests(unittest.TestCase):
    def test_source_integrity_and_transport(self):
        text = 'unchanged source\n'
        digest = hashlib.sha256(text.encode()).hexdigest()
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / 'source.cpp'
            path.write_bytes(text.replace('\n', '\r\n').encode())
            self.assertEqual(oracle.checked_text(path, digest), text)
            for raw in [b'changed source\n', b'unchanged source\r']:
                path.write_bytes(raw)
                with self.assertRaises(ValueError):
                    oracle.checked_text(path, digest)

    def test_complete_body_and_signature_drift(self):
        source = 'prefix void Player::SetSkill(int x) { if (x) { call(); } } suffix'
        signature = 'void Player::SetSkill('
        expected = 'void Player::SetSkill(int x) { if (x) { call(); } }\n'
        self.assertEqual(oracle.body(source, signature), expected)
        for invalid in [source + source, source.replace('SetSkill', 'Other'), source[:-9]]:
            with self.assertRaises(ValueError):
                oracle.body(invalid, signature)

    def test_marker_and_scope_drift(self):
        parts = {key: f'original {key}\n' for key in set(oracle.SIGNATURES) | {'STATUS'}}
        harness = ''.join('// PINNED_' + key + '\n' for key in parts)
        assembled = oracle.assemble(parts, harness)
        for fragment in parts.values():
            self.assertIn(fragment, assembled)
        for invalid in [harness + '// PINNED_TRAIN\n', harness.replace('// PINNED_TRAIN\n', ''), harness + '// PINNED_UNKNOWN\n']:
            with self.assertRaises(ValueError):
                oracle.assemble(parts, invalid)
        with self.assertRaises(ValueError):
            oracle.assemble({'TRAIN': parts['TRAIN']}, harness)

    def test_template_declares_authored_endpoints(self):
        source = (oracle.HERE / 'starter_callbacks_oracle.cpp').read_text()
        self.assertIn('NOT Player methods', source)
        self.assertIn('not addSpell closure', source)
        self.assertIn('unexpected starter callback spell', source)
        for marker in ['SET_EXISTING', 'SET_ID', 'TRAIN', 'STATUS']:
            self.assertEqual(source.count('// PINNED_' + marker + '\n'), 1)

    def rows(self):
        lines = oracle.OUTPUT.read_text().splitlines()
        self.assertTrue(all(line.startswith('#') for line in lines[:3]))
        rows = [line.split('\t') for line in lines[3:]]
        self.assertTrue(all(len(row) == 4 for row in rows))
        self.assertEqual(len(rows), 37)
        self.assertEqual(len({row[0] for row in rows}), len(rows))
        return {row[0]: row[1:] for row in rows}

    def test_new_callback_before_reset_reapplication(self):
        rows = self.rows()
        events, skill, known = rows['new_ordinary_callback_aura']
        self.assertLess(events.index('authored-install-perm'), events.index('field:2=0'))
        self.assertLess(events.index('field:2=0'), events.index('aura-temp'))
        self.assertEqual(events.count('aura-perm'), 2)
        self.assertTrue(skill.endswith(':327682;'))  # final temp2/permanent5, not permanent10
        self.assertEqual(known, '70001,')
        existing = rows['existing_ordinary_retained_aura'][0]
        self.assertNotIn('field:2=0', existing)
        self.assertNotIn('aura-', existing)
        defense = rows['new_defense_callback_aura'][0]
        self.assertLess(defense.index('add-endpoint'), defense.index('defense;'))
        self.assertLess(defense.index('defense;'), defense.index('field:2=0'))

    def test_starter_threshold_order_and_state(self):
        rows = self.rows()
        for threshold in range(3):
            for order in ['forward', 'reverse']:
                events, skill, known = rows[f'starter_threshold{threshold}_{order}']
                self.assertTrue(skill.startswith('60001:0:2:60001:327681:'))
                self.assertNotIn('add-endpoint:70002', events)
                self.assertNotIn('add-endpoint:70003', events)
                if threshold == 2:
                    self.assertNotIn('add-endpoint', events)
                    self.assertTrue(skill.endswith(':0;'))
                    self.assertEqual(known, '')
                else:
                    self.assertTrue(skill.endswith(':327682;'))
                    self.assertEqual(known, '70001,70004,')
                    self.assertEqual(events.index('add-endpoint:70001') < events.index('add-endpoint:70004'), order == 'forward')
                    self.assertTrue(events.endswith('field:2=0;aura-temp;aura-perm;'))

    def test_training_filters_and_deletion_boundary(self):
        rows = self.rows()
        for name, (events, _, _) in rows.items():
            if name.startswith('training_'):
                self.assertNotIn('endpoint:70002', events)
                self.assertNotIn('endpoint:70003', events)
                if name.endswith('_world'):
                    self.assertNotIn('add-endpoint', events)
                else:
                    self.assertNotIn('learn-endpoint', events)
        self.assertIn('add-endpoint:70001', rows['repair_step1_missing'][0])
        for step in [2, 3]:
            self.assertNotIn('add-endpoint', rows[f'repair_step{step}_missing'][0])
        for step in [1, 2, 3]:
            self.assertNotIn('add-endpoint', rows[f'repair_step{step}_known'][0])
        self.assertIn('remove-dependent-endpoint:70001', rows['remove_new_outside_starter'][0])
        self.assertEqual(rows['remove_new_outside_starter'][1], '')
        self.assertEqual(rows['invalid_id_and_end_noop'], ['', '', ''])


if __name__ == '__main__':
    unittest.main()
