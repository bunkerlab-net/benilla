#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail-closed source/transport checks; no Cargo and no game data."""
import hashlib
import pathlib
import subprocess
import tempfile
import unittest
from unittest import mock

import generate_defense_skill_fixture as oracle


class OracleInputTests(unittest.TestCase):
    def test_crlf_changes_transport_only(self):
        source = oracle.ROOT / "tools/defense_skill_source/Player.cpp.inc"
        raw = oracle.canonical_text(source)
        digest = oracle.SNIPPETS[source.name]
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / source.name
            path.write_bytes(raw.replace(b"\n", b"\r\n"))
            self.assertEqual(oracle.checked_text(path, digest), raw.decode())
            path.write_bytes(raw.replace(b"\n", b"\r", 1))
            with self.assertRaisesRegex(SystemExit, "Bare carriage return"):
                oracle.checked_text(path, digest)
            path.write_bytes(raw.replace(b"0.22", b"0.23", 1))
            with self.assertRaisesRegex(SystemExit, "altered source"):
                oracle.checked_text(path, digest)

    def test_complete_method_extraction(self):
        sample = "before void Player::Test() { if (true) { call(); } } after"
        self.assertEqual(oracle.body(sample, "void Player::Test("), "void Player::Test() { if (true) { call(); } }\n")

    def test_no_compiler_fails_instead_of_skipping(self):
        with tempfile.TemporaryDirectory() as temp:
            with mock.patch.object(oracle.shutil, "which", return_value=None):
                with self.assertRaisesRegex(SystemExit, r"No native C\+\+ compiler found"):
                    oracle.native_compiler(None, pathlib.Path(temp))

    def test_harness_members_do_not_shadow_pinned_method_locals(self):
        harness = (oracle.ROOT / "tools/defense_skill_oracle.cpp").read_text()
        for marker, name in (("PINNED_ROLL", "Util.h.inc"),
                             ("PINNED_METHODS", "Player.cpp.inc")):
            source = oracle.checked_text(
                oracle.ROOT / "tools/defense_skill_source" / name, oracle.SNIPPETS[name])
            harness = harness.replace("// " + marker + "\n", source)
        harness = harness.replace("// CANONICAL_LOG_BITS\n",
                                  "const uint64_t canonical_log_bits[294] = {};\n")
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            cpp = root / "oracle.cpp"
            cpp.write_text(harness)
            compiler, flags, env, msvc = oracle.native_compiler(None, root)
            if msvc:
                outputs = ["/c", str(cpp), "/Fo" + str(root / "oracle.obj")]
            else:
                # GCC's default warnings omit member shadowing; match MSVC C4458.
                flags += ["-Wshadow"]
                outputs = ["-c", str(cpp), "-o", str(root / "oracle.o")]
            subprocess.run(compiler + flags + outputs, env=env, cwd=root, check=True)

    def test_locked_excerpts_preserve_source_copyright(self):
        for name, digest in oracle.SNIPPETS.items():
            text = oracle.checked_text(oracle.ROOT / "tools/defense_skill_source" / name, digest)
            self.assertIn("CMaNGOS", text)
            self.assertIn("GNU General Public License", text)
            self.assertEqual(hashlib.sha256(text.encode()).hexdigest(), digest)


if __name__ == "__main__":
    unittest.main()
