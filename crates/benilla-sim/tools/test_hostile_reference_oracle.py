#!/usr/bin/env python3
"""Exact-source guards and linked-unit corpus coverage; no game install.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

path = Path(__file__).resolve().parent / "hostile_reference_source/generate.py"
spec = importlib.util.spec_from_file_location("hostile_reference_oracle", path)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class SourceGuards(unittest.TestCase):
    def test_retained_source_hashes(self):
        oracle.checked_sources()

    def test_every_retained_source_mutation_rejects(self):
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

    def test_missing_upstream_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):
                oracle.checked_sources(upstream=Path(directory))

    def test_upstream_normalizes_only_crlf(self):
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

    def test_compiled_source_fixture_under_ubsan(self):
        self.assertEqual(oracle.run_oracle(sanitize=True), oracle.FIXTURE.read_bytes())

    def test_corpus_executes_owned_lifecycle_and_independent_flag_words(self):
        commands = list(oracle.commands())
        fixture_rows = [line for line in oracle.FIXTURE.read_text().splitlines() if not line.startswith("#")]
        self.assertEqual([row.split(" | ")[0] for row in fixture_rows], commands)
        self.assertEqual(len(commands), 611)
        for command in ["A 1 0", "A 7 0", "X 7 0", "V 9", "U 9", "L 9", "R 9", "D 1", "D 1000", "C"]:
            self.assertIn(command, commands)
        self.assertGreater(commands.count("A 1 0"), 3)
        self.assertGreater(commands.count("U 9"), 3)
        predicates = [row.split() for row in commands if row.startswith("P ")]
        states = {int(row[2]) for row in predicates}
        flags = {int(row[3]) for row in predicates}
        for bit in range(32):
            for word in [1 << bit, 0xffff_ffff ^ (1 << bit)]:
                self.assertIn(word, states)
                self.assertIn(word, flags)
        self.assertEqual(len(predicates), 528)
        # The source corpus proves containers, not a Rust multi-target extension.
        for line in fixture_rows:
            _, result, events = line.split(" | ")
            row = result.split()
            self.assertLessEqual(int(row[3]) + int(row[4]), 1)
        self.assertTrue(any(row.endswith(" | O0,R") for row in fixture_rows))
        self.assertTrue(any(row.endswith(" | R") for row in fixture_rows))
        self.assertTrue(any(row.endswith(" | X0") for row in fixture_rows))
        self.assertTrue(any(row.endswith(" | X1") for row in fixture_rows))


if __name__ == "__main__":
    unittest.main()
