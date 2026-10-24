#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Source integrity/schema checks only; these tests never invoke a compiler."""
import hashlib
import pathlib
import tempfile
import unittest

import generate_initial_cast_phase_fixture as oracle


class SourceIntegrityTests(unittest.TestCase):
    def test_retained_source_is_hash_locked_and_copyrighted(self):
        manifest, snippets = oracle.verify_source()
        self.assertEqual(manifest["source_pin"], oracle.PIN)
        self.assertEqual(len(snippets), 17)
        for name, text in snippets.items():
            self.assertIn("CMaNGOS", text)
            self.assertIn("GNU General Public License", text)
            self.assertEqual(hashlib.sha256(text.encode()).hexdigest(), manifest["snippets"][name]["sha256"])

    def test_source_transport_and_mutation_fail_closed(self):
        manifest, snippets = oracle.verify_source()
        text = snippets["EffectAmount.inc"]
        digest = manifest["snippets"]["EffectAmount.inc"]["sha256"]
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / "source.inc"
            path.write_bytes(text.replace("\n", "\r\n").encode())
            self.assertEqual(oracle.checked_text(path, digest), text)
            path.write_text(text.replace("basePoints += randvalue", "basePoints -= randvalue"))
            with self.assertRaisesRegex(SystemExit, "altered source"):
                oracle.checked_text(path, digest)
            path.write_bytes(text.encode().replace(b"\n", b"\r", 1))
            with self.assertRaisesRegex(SystemExit, "Bare carriage return"):
                oracle.checked_text(path, digest)

    def test_assembled_harness_retains_all_original_excerpts(self):
        _, snippets = oracle.verify_source()
        harness = oracle.assemble(snippets)
        for text in snippets.values():
            self.assertIn(text, harness)
        self.assertNotIn("// PINNED_", harness)
        self.assertNotIn("// GENERATED_UNREACHED_HANDLERS", harness)
        self.assertIn('throw std::runtime_error("unreached handler EffectUnused")', harness)
        self.assertIn("(*this.*SpellEffects[eff])(i);", harness)

    def test_inputs_are_authored_not_expected_output(self):
        rows = oracle.authored_cases()
        self.assertEqual(len({row["name"] for row in rows}), len(rows))
        for row in rows:
            self.assertNotIn("events", row)
            self.assertNotIn("final_weapon_mask", row)
            for entry in row["spells"]:
                self.assertEqual(len(entry["effects"]), 3)
                self.assertEqual(entry["resolved_cast_time_ms"], 0)
        by_name = {row["name"]: row for row in rows}
        self.assertEqual([by_name[f"dodge81_roll_{i}"]["rolls"] for i in range(7)], [[i] for i in range(7)])
        self.assertEqual(by_name["detect3050_no_dispatch"]["spells"][0]["effects"][0]["points_per_level_bits"], oracle.fbits(5.))
        self.assertTrue(all(row["source_only"] for row in rows if row["name"].startswith("source_only_")))
        direct = by_name["authored_direct_passive_projection"]["spells"]
        for entry in direct:
            if entry["spell_id"] in (81, 522, 668, 3050):
                self.assertEqual((entry["item_class"], entry["item_mask"]), (-1, 0xffffffff))
            elif entry["spell_id"] in (204, 20597, 20598, 20599, 20864):
                self.assertEqual((entry["item_class"], entry["item_mask"]), (-1, 0))

    def test_complete_method_extraction(self):
        text = "prefix void Thing::method() { if (true) { nested(); } } suffix"
        self.assertEqual(oracle.body(text, "void Thing::method("), "void Thing::method() { if (true) { nested(); } }\n")


if __name__ == "__main__":
    unittest.main()
