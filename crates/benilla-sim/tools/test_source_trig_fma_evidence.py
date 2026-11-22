#!/usr/bin/env python3
"""Keep both trig oracle generators inside new assigned evidence directories."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import tempfile
import unittest

import source_trig_fma_generate as full
import source_trig_fma_degrees_generate as degrees

GENERATORS = (full, degrees)


class EvidenceDirectoryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='.trig-evidence-test-', dir=Path(__file__).parent)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.assigned = self.root / 'assigned'
        self.assigned.mkdir()

    def test_new_nested_child_is_created_inside_assigned_directory(self):
        for generator in GENERATORS:
            with self.subTest(generator=generator.__name__):
                child = Path(generator.__name__) / 'rustc-1.100'
                actual = generator.create_evidence(child, self.assigned)
                self.assertEqual(actual, self.assigned / child)
                self.assertTrue(actual.is_dir())

    def test_parent_traversal_is_refused_even_if_it_resolves_inside(self):
        for generator in GENERATORS:
            for child in ('../escape', 'nested/../../escape', 'nested/../inside'):
                with self.subTest(generator=generator.__name__, child=child):
                    with self.assertRaisesRegex(SystemExit, 'relative child'):
                        generator.create_evidence(Path(child), self.assigned)
        self.assertEqual(list(self.assigned.iterdir()), [])
        self.assertFalse((self.root / 'escape').exists())

    def test_absolute_paths_are_refused_even_inside_assigned_directory(self):
        for generator in GENERATORS:
            with self.subTest(generator=generator.__name__):
                with self.assertRaisesRegex(SystemExit, 'relative child'):
                    generator.create_evidence(self.assigned / 'inside', self.assigned)
        self.assertEqual(list(self.assigned.iterdir()), [])

    def test_assigned_directory_itself_is_refused(self):
        for generator in GENERATORS:
            with self.subTest(generator=generator.__name__):
                with self.assertRaisesRegex(SystemExit, 'relative child'):
                    generator.create_evidence(Path('.'), self.assigned)

    def test_symlink_components_are_refused_including_dangling_links(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (self.assigned / 'outside-link').symlink_to(outside, target_is_directory=True)
        (self.assigned / 'inside-link').symlink_to(self.assigned, target_is_directory=True)
        (self.assigned / 'dangling-link').symlink_to(self.root / 'missing', target_is_directory=True)
        for generator in GENERATORS:
            for child in ('outside-link/new', 'inside-link/new', 'dangling-link/new', 'outside-link'):
                with self.subTest(generator=generator.__name__, child=child):
                    with self.assertRaisesRegex(SystemExit, 'symlink'):
                        generator.create_evidence(Path(child), self.assigned)
        self.assertEqual(list(outside.iterdir()), [])
        self.assertFalse((self.root / 'missing').exists())

    def test_symlink_in_assigned_directory_is_refused(self):
        alias = self.root / 'alias'
        alias.symlink_to(self.assigned, target_is_directory=True)
        for generator in GENERATORS:
            with self.subTest(generator=generator.__name__):
                with self.assertRaisesRegex(SystemExit, 'symlink'):
                    generator.create_evidence(Path('new'), alias)
        self.assertEqual(list(self.assigned.iterdir()), [])

    def test_existing_directory_is_refused_without_modifying_receipt(self):
        existing = self.assigned / 'existing'
        existing.mkdir()
        receipt = existing / 'receipt.json'
        receipt.write_bytes(b'original receipt\n')
        for generator in GENERATORS:
            with self.subTest(generator=generator.__name__):
                with self.assertRaisesRegex(SystemExit, 'already exists'):
                    generator.create_evidence(Path('existing'), self.assigned)
        self.assertEqual(receipt.read_bytes(), b'original receipt\n')


if __name__ == '__main__':
    unittest.main()
