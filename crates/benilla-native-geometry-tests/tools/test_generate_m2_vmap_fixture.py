#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent structural checks for the authored exact-source M2 fixtures."""
import hashlib
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import generate_m2_vmap_fixture as generator

FIXTURES = generator.HERE / 'fixtures/m2_vmap_source'


class AuthoredM2Fixtures(unittest.TestCase):
    def test_original_extraction_preserves_vertex_bits_and_reverses_indices(self):
        for stem, version in [('authored', 256), ('signed-zero', 257)]:
            with self.subTest(stem=stem):
                source = (FIXTURES / f'{stem}-m2.m2.test-fixture').read_bytes()
                raw = (FIXTURES / f'{stem}-m2.raw.vmap').read_bytes()
                self.assertEqual(source[:4], b'MD20')
                self.assertEqual(struct.unpack_from('<I', source, 4)[0], version)
                self.assertEqual(len(source), 816)
                count, index_offset, vertices, vertex_offset = struct.unpack_from('<4I', source, 236)
                self.assertEqual((count, index_offset, vertices, vertex_offset), (96, 324, 25, 516))
                self.assertEqual(raw[:8], b'VMAPs05\0')
                self.assertEqual(struct.unpack_from('<5I', raw, 8), (25, 1, 0, 0, 0))
                self.assertEqual(raw[28:56], bytes(28))
                self.assertEqual(raw[56:60], b'GRP ')
                self.assertEqual(struct.unpack_from('<3I', raw, 60), (8, 1, 96))
                self.assertEqual(raw[72:76], b'INDX')
                self.assertEqual(struct.unpack_from('<2I', raw, 76), (196, 96))
                original = struct.unpack_from('<96H', source, index_offset)
                expected = tuple(value for i in range(0, 96, 3)
                                 for value in (original[i], original[i+2], original[i+1]))
                self.assertEqual(struct.unpack_from('<96H', raw, 84), expected)
                self.assertEqual(raw[276:280], b'VERT')
                self.assertEqual(struct.unpack_from('<2I', raw, 280), (304, 25))
                self.assertEqual(raw[288:], source[vertex_offset:])
                self.assertEqual(len(raw), 588)
        signed = (FIXTURES / 'signed-zero-m2.raw.vmap').read_bytes()
        self.assertEqual(struct.unpack_from('<3I', signed, 288), (0x80000000,) * 3)

    def test_original_serialization_preserves_zero_group_bounds(self):
        for stem in ['authored', 'signed-zero']:
            serialized = (FIXTURES / f'products/{stem}.m2.vmo').read_bytes()
            self.assertEqual(serialized[:8], b'VMAP_7.0')
            self.assertEqual(serialized[20:24], b'GMOD')
            self.assertEqual(struct.unpack_from('<I', serialized, 24)[0], 1)
            self.assertEqual(serialized[28:52], bytes(24))
            group_bih = serialized.index(b'GBIH')
            self.assertEqual(serialized[group_bih+4:group_bih+28], bytes(24))
            self.assertNotEqual(serialized[serialized.index(b'MBIH')+4:][:24], bytes(24))

        tile = (FIXTURES / 'products/000_32_32.vmtile').read_bytes()
        self.assertEqual(struct.unpack_from('<I', tile, 8)[0], 3)
        at = 12
        for expected_id in [101, 102, 103]:
            self.assertEqual(struct.unpack_from('<I', tile, at)[0], 5)
            self.assertEqual(struct.unpack_from('<I', tile, at+6)[0], expected_id)
            if expected_id == 103:
                self.assertEqual(struct.unpack_from('<3f', tile, at+10), (70, 20, 4))
                self.assertEqual(struct.unpack_from('<3f', tile, at+22), (13, 37, -9))
                self.assertEqual(struct.unpack_from('<f', tile, at+34)[0], 1.75)
            name_length = struct.unpack_from('<I', tile, at+62)[0]
            at += 70 + name_length
        self.assertEqual(at, len(tile))

    def test_changed_extractor_source_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            name = 'guarded.cpp'
            original = b'original source\n'
            digest = hashlib.sha256(original).hexdigest()
            (root / 'm2_vmap_source_hashes.sha256').write_text(f'{digest}  {name}\n')
            (root / name).write_bytes(original)
            with mock.patch.object(generator, 'HERE', root), \
                 mock.patch.object(generator, 'guarded_sources', return_value={}):
                self.assertEqual(generator.sources(root)[name], original.decode())
                (root / name).write_bytes(original + b'changed\n')
                with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                    generator.sources(root)

    def test_hash_mismatch_prevents_compilation(self):
        with mock.patch.object(sys, 'argv', ['generator', '/unused']), \
             mock.patch.object(generator, 'sources', side_effect=SystemExit('Wrong pinned source hash')), \
             mock.patch.object(generator.subprocess, 'run') as compiler:
            with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                generator.main()
            compiler.assert_not_called()

    def test_method_extraction_is_verbatim_and_stops_at_matching_brace(self):
        source = 'unrelated\nbool Sample::run(int n)\n{\n  if (n) { return true; }\n  return false;\n}\nignored'
        expected = 'bool Sample::run(int n)\n{\n  if (n) { return true; }\n  return false;\n}\n'
        self.assertEqual(generator.method(source, 'bool Sample::run('), expected)


@unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'), 'set CMANGOS_SOURCE for original-source rebuild checks')
class PinnedSourceIntegration(unittest.TestCase):
    def test_extracted_fragments_are_exact_current_guarded_source(self):
        root = pathlib.Path(os.environ['CMANGOS_SOURCE']).resolve()
        extracted, collision = generator.fragments(generator.sources(root))
        checked = (FIXTURES / 'source-fragments.cpp.test-fixture').read_bytes()
        self.assertEqual(checked, (extracted + collision).encode())
        model = (root / generator.EXTRACTOR / 'model.cpp').read_text()
        for signature in ['Model::Model(', 'bool Model::open(',
                          'bool Model::ConvertToVMAPModel(', 'Vec3D fixCoordSystem(']:
            self.assertIn(generator.method(model, signature), extracted)

    def test_every_generated_fixture_matches_without_replacement(self):
        before = {path.relative_to(FIXTURES): path.read_bytes()
                  for path in FIXTURES.rglob('*') if path.is_file()}
        result = subprocess.run([sys.executable, str(generator.HERE / 'generate_m2_vmap_fixture.py'),
                                 os.environ['CMANGOS_SOURCE'], '--check'],
                                check=True, text=True, capture_output=True)
        self.assertIn('Verified 12 authored M2 oracle files', result.stdout)
        after = {path.relative_to(FIXTURES): path.read_bytes()
                 for path in FIXTURES.rglob('*') if path.is_file()}
        self.assertEqual(before, after)


if __name__ == '__main__':
    unittest.main()
