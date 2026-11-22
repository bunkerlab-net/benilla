#!/usr/bin/env python3
"""Hash-guard and input-coverage tests; authored data only, no game installation.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import generate_creature_capabilities_fixture as oracle


class SourceGuards(unittest.TestCase):
    def test_retained_source_hashes(self):
        oracle.checked_sources()

    def test_each_fragment_mutation_rejects(self):
        manifest = json.loads((oracle.SOURCE / "source_manifest.json").read_bytes())
        for name in manifest["files"]:
            with self.subTest(fragment=name), tempfile.TemporaryDirectory() as directory:
                retained = Path(directory) / "source"
                shutil.copytree(oracle.SOURCE, retained)
                path = retained / name
                path.write_bytes(path.read_bytes() + b"\n")
                with self.assertRaisesRegex(SystemExit, "pinned source hash mismatch"):
                    oracle.checked_sources(retained=retained)

    def test_manifest_mutation_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            retained = Path(directory) / "source"
            shutil.copytree(oracle.SOURCE, retained)
            path = retained / "source_manifest.json"
            path.write_bytes(path.read_bytes() + b"\n")
            with self.assertRaisesRegex(SystemExit, "manifest hash mismatch"):
                oracle.checked_sources(retained=retained)

    def test_missing_source_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):
                oracle.checked_sources(upstream=Path(directory))

    def test_runtime_review_hashes_are_pinned(self):
        manifest = json.loads((oracle.SOURCE / "source_manifest.json").read_bytes())
        self.assertEqual(manifest["reviewed_runtime_sources"], {
            "src/game/Entities/Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
            "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
        })

    def test_full_source_guard_normalizes_only_crlf_and_rejects_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            upstream = Path(directory)
            path = upstream / "source.cpp"
            original = b"authored source\nsecond line\n"
            digest = hashlib.sha256(original).hexdigest()
            path.write_bytes(original.replace(b"\n", b"\r\n"))
            self.assertEqual(oracle.checked_upstream_file(upstream, path.name, digest), original)
            path.write_bytes(original + b" ")
            with self.assertRaisesRegex(SystemExit, "upstream source hash mismatch"):
                oracle.checked_upstream_file(upstream, path.name, digest)

    def test_all_habitats_and_independent_flags(self):
        flags = set(oracle.flag_words())
        self.assertEqual(len(flags), 127)
        self.assertTrue({0, 0xffffffff}.issubset(flags))
        for bit in range(32):
            self.assertIn(1 << bit, flags)
            self.assertIn(0xffffffff ^ (1 << bit), flags)
        rows = list(oracle.commands())
        self.assertEqual(len(rows), len(set(rows)))
        for habitat in range(1, 8):
            self.assertEqual(sum(row.startswith(f"P {habitat} ") for row in rows), 127)
        self.assertEqual(sum(row.startswith("R ") for row in rows), 262)
        self.assertIn("R 0 0", rows)
        self.assertIn("R 8 0", rows)
        self.assertIn("R 4294967295 0", rows)


if __name__ == "__main__":
    unittest.main()
