#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Structural, source-guard, and independent regeneration checks for container order."""
import array
import collections
import csv
import hashlib
import itertools
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import wmo_doodad_order_generate as generator

FIXTURES = generator.TARGET


def table(name):
    text = (FIXTURES / name).read_text()
    if not text.startswith('# '):
        raise ValueError('Missing table schema')
    rows = list(csv.DictReader(text[2:].splitlines(), delimiter='\t'))
    if any(None in row or None in row.values() for row in rows):
        raise ValueError('Unexpected table column count')
    return rows


def words(name):
    data = (FIXTURES / name).read_bytes()
    if len(data) % 2:
        raise ValueError('Odd u16 byte length')
    result = array.array('H')
    result.frombytes(data)
    if sys.byteorder != 'little':
        result.byteswap()
    return result


class ReferenceContainerFixtures(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = {row['case']: row for row in table('cases.tsv')}
        cls.checkpoints = collections.defaultdict(list)
        for row in table('checkpoints.tsv'):
            cls.checkpoints[row['case']].append(row)
        cls.inputs = words('insertions.u16le')
        cls.outputs = words('iterations.u16le')
        cls.schedule = [(int(row['distinct_count']), int(row['bucket_count']))
                        for row in table('bucket_schedule.tsv')]
        cls.profile = {row['item']: row['value'] for row in table('container_profile.tsv')}
        cls.provenance = {row['item']: row['value'] for row in table('provenance.tsv')}

    def input(self, name):
        row = self.cases[name]
        start, count = int(row['input_offset']), int(row['input_count'])
        return self.inputs[start:start + count]

    def output(self, row):
        start, count = int(row['iteration_offset']), int(row['iteration_count'])
        return self.outputs[start:start + count]

    def test_compact_complete_fixture_file_set(self):
        self.assertEqual({path.name for path in FIXTURES.iterdir()},
                         {'cases.tsv', 'checkpoints.tsv', 'insertions.u16le', 'iterations.u16le',
                          'bucket_schedule.tsv', 'swap_checks.tsv', 'container_profile.tsv',
                          'provenance.tsv', 'dependencies.tsv', 'source-fragments.test-fixture'})
        self.assertEqual(len(self.cases), 753)
        self.assertEqual(self.cases.keys(), self.checkpoints.keys())
        self.assertEqual(len(self.cases), int(self.profile['cases']))
        self.assertEqual(sum(map(len, self.checkpoints.values())), int(self.profile['checkpoints']))
        self.assertLess(sum(path.stat().st_size for path in FIXTURES.iterdir()), 7 * 1024 * 1024)

    def test_offsets_are_contiguous_element_units_without_trailing_bytes(self):
        offset = 0
        for name, row in self.cases.items():
            self.assertEqual(int(row['input_offset']), offset, name)
            offset += int(row['input_count'])
        self.assertEqual(offset, len(self.inputs))
        self.assertEqual(offset, int(self.profile['literal_input_u16_count']))
        offset = 0
        for rows in self.checkpoints.values():
            for row in rows:
                self.assertEqual(int(row['iteration_offset']), offset)
                self.assertEqual(row['distinct_count'], row['iteration_count'])
                offset += int(row['iteration_count'])
        self.assertEqual(offset, len(self.outputs))
        self.assertEqual(offset, int(self.profile['literal_iteration_u16_count']))

    def test_every_literal_iteration_is_exactly_the_inserted_distinct_prefix(self):
        for name, rows in self.checkpoints.items():
            inputs = self.input(name)
            seen = set()
            previous_count = 0
            self.assertEqual(int(rows[0]['after_inserts']), 0)
            self.assertEqual(int(rows[-1]['after_inserts']), len(inputs))
            for row in rows:
                count = int(row['after_inserts'])
                self.assertGreaterEqual(count, previous_count)
                self.assertLessEqual(count, len(inputs))
                seen.update(inputs[previous_count:count])
                output = self.output(row)
                self.assertEqual(set(output), seen, name)
                self.assertEqual(len(output), len(seen), name)
                previous_count = count

    def test_actual_growth_schedule_and_bounded_capacity(self):
        self.assertEqual(self.schedule,
                         [(0, 1), (1, 13), (14, 29), (30, 59), (60, 127), (128, 257),
                          (258, 541), (542, 1109), (1110, 2357), (2358, 5087),
                          (5088, 10273), (10274, 20753), (20754, 42043), (42044, 85229)])
        self.assertEqual(self.profile['measured_cardinalities'], '65537')
        self.assertEqual(self.profile['full_permutation_growth_checks'], '262144')
        self.assertEqual(self.profile['maximum_observed_bucket_count'], '85229')
        self.assertLessEqual(max(bucket for _, bucket in self.schedule), 131072)
        for rows in self.checkpoints.values():
            for row in rows:
                distinct = int(row['distinct_count'])
                expected = next(bucket for count, bucket in reversed(self.schedule) if count <= distinct)
                self.assertEqual(int(row['bucket_count']), expected)

    def test_every_full_case_snapshots_rehash_neighbors_without_quadratic_output(self):
        needed = {0, 65535, 65536, 131072}
        for count, _ in self.schedule[1:]:
            needed.update([count - 1, count, count + 1])
        for kind in ['ascending', 'descending', 'affine', 'shuffled']:
            rows = self.checkpoints['full_' + kind]
            self.assertEqual({int(row['after_inserts']) for row in rows}, needed)
            self.assertLess(len(rows), 45)
            unique_final = next(row for row in rows if row['after_inserts'] == '65536')
            self.assertEqual(self.output(unique_final), self.output(rows[-1]))

    def test_full_permutation_inputs_are_literal_deterministic_and_followed_by_duplicates(self):
        shuffled = list(range(65536))
        state = 0x6d2b79f5
        for remaining in range(65536, 1, -1):
            state ^= (state << 13) & 0xffffffff
            state ^= state >> 17
            state ^= (state << 5) & 0xffffffff
            index = state % remaining
            shuffled[remaining - 1], shuffled[index] = shuffled[index], shuffled[remaining - 1]
        expected = {'ascending': list(range(65536)), 'descending': list(reversed(range(65536))),
                    'affine': [(index * 65521 + 12345) % 65536 for index in range(65536)],
                    'shuffled': shuffled}
        for kind, first_pass in expected.items():
            inputs = list(self.input('full_' + kind))
            self.assertEqual(inputs[:65536], first_pass)
            self.assertEqual(inputs[65536:], list(reversed(first_pass)))
            self.assertEqual(set(first_pass), set(range(65536)))

    def test_all_six_key_permutations_have_every_prefix(self):
        actual = []
        for number in range(720):
            name = 'perm6_' + str(number)
            actual.append(tuple(self.input(name)))
            self.assertEqual([int(row['after_inserts']) for row in self.checkpoints[name]], list(range(7)))
        self.assertEqual(actual, list(itertools.permutations([0, 1, 13, 26, 29, 65535])))

    def test_fresh_composite_swap_displaces_populated_map_entry(self):
        swaps = table('swap_checks.tsv')
        self.assertEqual({row['case'] for row in swaps}, self.cases.keys())
        populated = []
        for row in swaps:
            name = row['case']
            prior = int(self.cases[name]['prepopulate_count'])
            self.assertEqual(int(row['prior_distinct']), prior)
            self.assertEqual(row['reset_distinct'], '0')
            self.assertEqual(row['reset_buckets'], '1')
            self.assertEqual(row['displaced_distinct'], row['prior_distinct'])
            self.assertEqual(row['displaced_buckets'], row['prior_buckets'])
            if prior:
                self.assertEqual(prior, 65536)
                self.assertEqual(row['prior_buckets'], '85229')
                populated.append(name)
        self.assertEqual(populated, ['populated_root_replacement_empty',
                                     'populated_root_replacement_small', 'full_affine'])

    def test_small_duplicates_preserve_literal_order(self):
        inputs = self.input('duplicates')
        rows = self.checkpoints['duplicates']
        seen = set()
        for index, value in enumerate(inputs):
            if value in seen:
                self.assertEqual(self.output(rows[index]), self.output(rows[index + 1]))
            seen.add(value)
        self.assertEqual(list(self.output(self.checkpoints['singleton_max'][-1])), [65535])
        self.assertEqual(list(self.output(self.checkpoints['empty'][-1])), [])

    def test_duplicate_at_each_capacity_does_not_trigger_next_growth(self):
        for next_distinct, next_buckets in self.schedule[2:]:
            capacity = next_distinct - 1
            name = 'duplicate_at_capacity_' + str(capacity)
            self.assertEqual(list(self.input(name)), [*range(capacity), 0, capacity])
            rows = self.checkpoints[name]
            self.assertEqual([int(row['after_inserts']) for row in rows],
                             [0, capacity, capacity + 1, capacity + 2])
            self.assertEqual([int(row['distinct_count']) for row in rows],
                             [0, capacity, capacity, capacity + 1])
            self.assertEqual([int(row['bucket_count']) for row in rows],
                             [1, capacity, capacity, next_buckets])
            self.assertEqual(self.output(rows[1]), self.output(rows[2]))

    def test_collision_cases_are_actual_same_modulus_keys(self):
        for stride in [13, 29, 59, 127, 257]:
            ascending = list(self.input('collision_stride_' + str(stride)))
            self.assertTrue(all(value % stride == 0 for value in ascending))
            self.assertEqual(list(self.input('collision_reverse_' + str(stride))), list(reversed(ascending)))
            self.assertGreaterEqual(len(ascending), 255)

    def test_exact_observed_header_and_runtime_profile_is_identified(self):
        self.assertEqual(self.profile['GLIBCXX_RELEASE'], '14')
        self.assertEqual(self.profile['GLIBCXX_DATE'], '20250315')
        self.assertEqual(self.profile['sizeof_uint16'], '2')
        self.assertEqual(self.profile['sizeof_size_t'], '8')
        self.assertEqual(self.profile['sizeof_set'], '56')
        self.assertEqual(self.profile['sizeof_node'], '16')
        self.assertEqual(self.profile['sizeof_prime_policy'], '16')
        self.assertEqual(self.profile['max_load_factor'], '1')
        self.assertEqual(self.profile['hash_cached'], '0')
        self.assertEqual(self.profile['small_size_threshold'], '0')
        self.assertEqual(self.profile['hash_identity_checked_keys'], '65536')
        self.assertEqual(self.provenance['pin'], generator.PIN)
        self.assertEqual(self.provenance['compiler_flags'], ' '.join(generator.FLAGS))
        self.assertEqual(self.provenance['out_of_line_prime_symbols'],
                         '_ZNKSt8__detail20_Prime_rehash_policy14_M_need_rehashEmmm@GLIBCXX_3.4.18')

    def test_dependency_closure_records_all_guards_with_hashes_and_sizes(self):
        rows = table('dependencies.tsv')
        recorded = {(row['kind'], row['path']): row for row in rows}
        for filename, kind in [('wmo_doodad_order_header_hashes.sha256', 'header_dependency'),
                               ('wmo_doodad_order_toolchain_hashes.sha256', 'toolchain'),
                               ('wmo_doodad_order_source_hashes.sha256', 'cmangos_source')]:
            for path, expected in generator.manifest(filename).items():
                row = recorded[kind, path]
                self.assertEqual(row['sha256'], expected)
                self.assertGreater(int(row['bytes']), 0)
        headers = [row for row in rows if row['kind'] == 'header_dependency']
        self.assertEqual(len(headers), 276)
        self.assertIn(('header_dependency', '<generated>/oracle.cpp'), recorded)
        self.assertIn(('header_dependency', '<cmangos>/' + generator.EXTRACTOR + 'vec3d.h'), recorded)
        for suffix in ['/bits/hashtable.h', '/bits/hashtable_policy.h', '/bits/functional_hash.h',
                       '/bits/unordered_set.h', '/bits/c++config.h']:
            self.assertTrue(any(row['path'].endswith(suffix) for row in headers))
        library = recorded['toolchain', str(generator.LIBSTDCXX)]
        self.assertEqual(library['sha256'], self.provenance['libstdc++_sha256'])

    def test_authored_source_and_fragments_match_provenance(self):
        for key, path in [('authored_oracle_sha256', generator.HERE / 'wmo_doodad_order_oracle.cpp'),
                          ('source_fragments_sha256', FIXTURES / 'source-fragments.test-fixture')]:
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), self.provenance[key])
        fragments = (FIXTURES / 'source-fragments.test-fixture').read_text()
        for line in ['std::unordered_set<uint16> References;',
                     'std::swap(doodads, froot.DoodadData);',
                     'doodads.References.insert(groupReference);',
                     'for (uint16 doodadIndex : doodadData.References)']:
            self.assertIn(line, fragments)

    def test_guards_reject_changed_files_without_compiler_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            content = b'guarded\n'
            (root / 'guarded').write_bytes(content)
            (root / 'manifest.sha256').write_text(hashlib.sha256(content).hexdigest() + '  guarded\n')
            with mock.patch.object(generator, 'HERE', root):
                self.assertEqual(generator.verify_manifest('manifest.sha256', root), {'guarded': content})
                (root / 'guarded').write_bytes(content + b'changed')
                with mock.patch.object(generator.subprocess, 'run') as compiler:
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned hash'):
                        generator.verify_manifest('manifest.sha256', root)
                    compiler.assert_not_called()
        with mock.patch.object(sys, 'argv', ['generator', '/unused']), \
             mock.patch.object(generator, 'sources', side_effect=SystemExit('Wrong pinned hash')), \
             mock.patch.object(generator.subprocess, 'run') as compiler:
            with self.assertRaisesRegex(SystemExit, 'Wrong pinned hash'):
                generator.main()
            compiler.assert_not_called()


@unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'), 'set CMANGOS_SOURCE for source/compiler integration')
class PinnedContainerIntegration(unittest.TestCase):
    def test_original_types_and_collection_statements_are_unchanged(self):
        source = generator.sources(pathlib.Path(os.environ['CMANGOS_SOURCE']).resolve())
        extracted = generator.fragments(source)
        self.assertEqual((FIXTURES / 'source-fragments.test-fixture').read_bytes(),
                         ('\n'.join(extracted.values()) + '\n').encode())
        generated = generator.oracle_text(source)
        self.assertNotIn('PINNED_SOURCE_', generated)
        for text in extracted.values():
            self.assertIn(text, generated)

    def test_exact_regeneration_checks_without_replacing_expected_bytes(self):
        before = {path.name: path.read_bytes() for path in FIXTURES.iterdir()}
        result = subprocess.run([sys.executable, str(generator.HERE / 'wmo_doodad_order_generate.py'),
                                 os.environ['CMANGOS_SOURCE'], '--check'],
                                check=True, text=True, capture_output=True)
        self.assertIn('Verified 10 authored reference-container oracle files', result.stdout)
        self.assertEqual(before, {path.name: path.read_bytes() for path in FIXTURES.iterdir()})


if __name__ == '__main__':
    unittest.main()
