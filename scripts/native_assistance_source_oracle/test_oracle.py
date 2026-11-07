#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Source extraction negative guards and source-observed assistance regressions."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
import run_oracle as oracle

CORE = None
OUTPUT = None
COMPILER = "g++"


class AssistanceOracle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        result = subprocess.run([sys.executable, str(oracle.ROOT / "run_oracle.py"),
            "--core", str(CORE), "--output", str(OUTPUT), "--compiler", COMPILER, "--check"],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        cls.run_metadata = json.loads(result.stdout)
        print("SOURCE_RUN " + json.dumps(cls.run_metadata, sort_keys=True), flush=True)
        cls.fixture = json.loads((Path(cls.run_metadata["output"]) / "vectors.json").read_text())
        cls.rows = {row["name"]: row for row in cls.fixture["cases"]}
        cls.candidate_fixture = json.loads((Path(cls.run_metadata["output"]) / "candidate_vectors.json").read_text())
        cls.candidates = {row["name"]: row for row in cls.candidate_fixture["cases"]}
        cls.helper_fixture = json.loads((Path(cls.run_metadata["output"]) / "peer_help_vectors.json").read_text())
        cls.helpers = {row["name"]: row for row in cls.helper_fixture["cases"]}
        cls.accessibility_fixture = json.loads((Path(cls.run_metadata["output"]) / "accessibility_vectors.json").read_text())
        cls.accessibility = cls.accessibility_fixture["cases"]
        cls.excerpts, cls.manifest = oracle.extract(CORE)

    def obs(self, name, index=-1):
        return self.rows[name]["observations"][index]

    def test_exact_pinned_extraction_and_placement(self):
        oracle.validate_source(CORE)
        oracle.verify_placement(self.excerpts)
        for record in self.manifest["excerpts"]:
            data = (CORE / record["source"]).read_bytes()
            self.assertEqual(data[record["first_byte"]:record["last_byte_exclusive"]], self.excerpts[record["name"]])
            self.assertEqual(oracle.sha(self.excerpts[record["name"]]), record["sha256"])

    def test_every_whole_source_file_rejects_drift(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in oracle.HASHES:
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(CORE / name, destination)
            for name in oracle.HASHES:
                with self.subTest(source=name):
                    destination = root / name
                    original = destination.read_bytes()
                    destination.write_bytes(original + b"\n// drift\n")
                    with self.assertRaisesRegex(ValueError, "pinned source SHA-256 mismatch"):
                        oracle.extract(root)
                    destination.write_bytes(original)

    def test_bundled_excerpt_tampering_rejects(self):
        for name in ["MarkCallAssistanceOnPull.inc", "SetInCombatState.inc", "EventProcessor.cpp", "CreatureUpdate.inc",
                     "AnyAssistCreatureInRangeCheck.inc", "CanAssistInCombatAgainst.inc", "UnitReaction.inc",
                     "CheckForHelp.inc", "GetAttackerForHelper.inc", "HelpWorldRadiusDefault.inc",
                     "PlayerVictimAccessibility.inc", "AccessibilityPlayerWater.inc"]:
            with self.subTest(excerpt=name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp) / "source"
                shutil.copytree(oracle.ROOT / "source", root)
                path = root / name
                path.write_bytes(path.read_bytes() + b"\n")
                with self.assertRaisesRegex(ValueError, "bundled source excerpt mismatch"):
                    oracle.validate_source(CORE, root)

    def test_rehashed_excerpt_and_manifest_cannot_bypass_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "source"
            shutil.copytree(oracle.ROOT / "source", root)
            path = root / "MarkCallAssistanceOnPull.inc"
            path.write_bytes(path.read_bytes().replace(b"return {stored, guids};", b"return {!stored, guids};"))
            manifest_path = root / "manifest.json"
            manifest = json.loads(manifest_path.read_text())
            for record in manifest["excerpts"]:
                if record["name"] == path.name:
                    record["sha256"] = oracle.sha(path.read_bytes())
            manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
            with self.assertRaisesRegex(ValueError, "bundled source manifest mismatch"):
                oracle.validate_source(CORE, root)

    def test_entry_hook_and_suffix_source_placement_guard(self):
        excerpts = dict(self.excerpts)
        text = excerpts["SetInCombatState.inc"].decode()
        text = text.replace("creature->AI()->EnterCombat(enemy);", "SWAP")
        text = text.replace("creature->SetCanCheckForHelp(false);", "creature->AI()->EnterCombat(enemy);")
        text = text.replace("SWAP", "creature->SetCanCheckForHelp(false);")
        excerpts["SetInCombatState.inc"] = text.encode()
        with self.assertRaisesRegex(ValueError, "placement changed"):
            oracle.verify_placement(excerpts)

    def test_negated_saved_boolean_guard(self):
        excerpts = dict(self.excerpts)
        excerpts["MarkCallAssistanceOnPull.inc"] = excerpts["MarkCallAssistanceOnPull.inc"].replace(b"bool stored = m_AlreadyCallAssistance;", b"bool stored = !m_AlreadyCallAssistance;")
        with self.assertRaisesRegex(ValueError, "literal saved assistance boolean changed"):
            oracle.verify_placement(excerpts)

    def test_lifecycle_event_cancellation_guard(self):
        for name in ["CreatureUpdate.inc", "CreatureSetDeathState.inc", "UnitSetDeathState.inc", "RemoveCorpse.inc", "CombatStop.inc"]:
            with self.subTest(excerpt=name):
                excerpts = dict(self.excerpts)
                excerpts[name] += b"\nKillAllEvents(true);\n"
                with self.assertRaisesRegex(ValueError, "lifecycle unexpectedly kills events"):
                    oracle.verify_placement(excerpts)

    def test_lambda_custom_abort_guard(self):
        excerpts = dict(self.excerpts)
        excerpts["UnitLambdaEvent.inc"] += b"\nvoid Abort(uint64);\n"
        with self.assertRaisesRegex(ValueError, "gained an Abort override"):
            oracle.verify_placement(excerpts)

    def test_initial_flags_and_synchronous_entry(self):
        initial, entered, before, due = self.rows["first_entry_positive_radius"]["observations"]
        self.assertTrue(initial["can_check_for_help"])
        self.assertFalse(initial["already_called"])
        self.assertTrue(initial["can_call_assistance"])
        self.assertFalse(entered["can_check_for_help"])
        self.assertTrue(entered["already_called"])
        self.assertEqual(entered["query_count"], 1)
        self.assertEqual(entered["query_radius_bits"], [oracle.bits(5.)])
        self.assertEqual([event["kind"] for event in entered["queued"]], ["ReenableHelp", "AggroLinking"])
        self.assertEqual(entered["executed"], [])
        self.assertFalse(before["can_check_for_help"])
        self.assertTrue(due["can_check_for_help"])

    def test_positive_template_radius_overrides_fallback(self):
        self.assertEqual(self.obs("first_entry_positive_radius")["query_radius_bits"], [oracle.bits(5.)])
        for name in ["zero_help_uses_fallback", "negative_help_uses_fallback"]:
            self.assertEqual(self.obs(name)["query_radius_bits"], [oracle.bits(10.)])

    def test_no_call_assist_still_sets_flag_and_queues_both(self):
        for name in ["no_call_assist_saved_false", "no_call_assist_saved_true"]:
            entered = self.obs(name, 1)
            final = self.obs(name)
            self.assertFalse(entered["can_call_assistance"])
            self.assertTrue(entered["already_called"])
            self.assertEqual(entered["query_count"], 0)
            self.assertEqual(len(entered["queued"]), 2)
            self.assertEqual(final["executed"], [1, 2])
            self.assertEqual(final["receiver_assistance_calls"], [])

    def test_literal_previous_boolean_is_not_negated(self):
        for name in ["saved_false_authored_receivers", "saved_true_authored_receivers"]:
            self.assertEqual(self.obs(name)["query_count"], 1)
        self.assertEqual(self.obs("saved_false_authored_receivers")["receiver_assistance_calls"], [])
        actual = self.obs("saved_true_authored_receivers")
        self.assertEqual(actual["receiver_ai_events"], [101, 102])
        self.assertEqual(actual["receiver_assistance_calls"], [101, 102])
        self.assertEqual(self.obs("saved_true_empty_receivers")["receiver_assistance_calls"], [])

    def test_repeated_combat_hit_has_no_new_pair(self):
        first = copy.deepcopy(self.obs("repeated_hit_no_new_pair", 1))
        second = copy.deepcopy(self.obs("repeated_hit_no_new_pair", 2))
        self.assertEqual(first, second)

    def test_source_equal_deadline_order_and_zero_delay(self):
        entry = self.obs("zero_delay_waits_for_update", 1)
        self.assertEqual([event["deadline"] for event in entry["queued"]], [0, 0])
        self.assertEqual(entry["executed"], [])
        self.assertEqual(self.obs("zero_delay_waits_for_update")["executed"], [1, 2])
        prior = self.obs("prior_equal_deadline_keeps_order")
        self.assertEqual([event["kind"] for event in prior["scheduled"]], ["PriorEvent", "ReenableHelp", "AggroLinking"])
        self.assertEqual(prior["executed"], [1, 2, 3])
        self.assertEqual(prior["help_after_execute"], [dict(id=1, can_check_for_help=False), dict(id=2, can_check_for_help=True), dict(id=3, can_check_for_help=True)])

    def test_reentrant_source_zero_delay_drains_same_update(self):
        final = self.obs("callback_entry_zero_delay_source_only")
        self.assertEqual(final["executed"], [1, 2, 3])
        self.assertEqual([event["add_clock"] for event in final["scheduled"]], [0, 7, 7])
        self.assertEqual([event["deadline"] for event in final["scheduled"]], [0, 7, 7])
        self.assertEqual(final["queue_clock"], 7)

    def test_deadlines_use_unit_queue_clock_and_u64(self):
        entered = self.obs("queue_clock_lags_map_clock", 1)
        self.assertEqual(entered["map_clock"], 1000)
        self.assertEqual([event["add_clock"] for event in entered["queued"]], [17, 17])
        self.assertEqual([event["deadline"] for event in entered["queued"]], [22, 22])
        unchanged = self.obs("queue_clock_lags_map_clock", 2)
        self.assertEqual(unchanged["queue_clock"], 17)
        self.assertEqual(unchanged["queued"], entered["queued"])
        self.assertEqual(self.obs("wide_queue_clock")["queue_clock"], 4294967301)

    def test_missing_enemy_and_missing_receiver_suppress_callbacks(self):
        missing = self.obs("missing_enemy_at_execution")
        self.assertEqual(missing["enemy_lookup_results"], [False])
        self.assertTrue(missing["can_check_for_help"])
        self.assertEqual(missing["receiver_assistance_calls"], [])
        missing_receiver = self.obs("captured_receivers_missing_at_execution")
        self.assertEqual(missing_receiver["enemy_lookup_results"], [True])
        self.assertEqual(missing_receiver["receiver_assistance_calls"], [])

    def test_combat_stop_resets_boolean_without_cancelling_events(self):
        stopped = self.obs("combat_stop_before_execution", 2)
        self.assertFalse(stopped["already_called"])
        self.assertFalse(stopped["can_check_for_help"])
        self.assertFalse(stopped["owner_in_combat"])
        self.assertEqual(len(stopped["queued"]), 2)
        final = self.obs("combat_stop_before_execution")
        self.assertEqual(final["executed"], [1, 2])
        self.assertEqual(final["receiver_assistance_calls"], [])
        self.assertTrue(final["can_check_for_help"])

    def test_old_saved_true_is_not_replaced_by_current_field(self):
        final = self.obs("stop_reentry_old_saved_true_survives")
        self.assertEqual(final["executed"], [1, 2, 3, 4])
        self.assertEqual(final["receiver_assistance_calls"], [101])
        self.assertEqual(final["query_count"], 2)

    def test_corpse_advances_pending_events_after_death_reset(self):
        died = self.obs("corpse_advances_events", 2)
        self.assertEqual(died["death_state"], "corpse")
        self.assertFalse(died["already_called"])
        self.assertEqual(len(died["queued"]), 2)
        final = self.obs("corpse_advances_events")
        self.assertEqual(final["queue_clock"], 5)
        self.assertEqual(final["executed"], [1, 2])
        self.assertEqual(final["receiver_assistance_calls"], [])

    def test_dead_freezes_and_respawn_retains_old_events(self):
        rows = self.rows["dead_freezes_respawn_retains"]["observations"]
        removed, frozen, respawned = rows[3:6]
        self.assertEqual(removed["death_state"], "dead")
        self.assertEqual(frozen["queue_clock"], 0)
        self.assertEqual(frozen["map_clock"], 100)
        self.assertEqual(frozen["queued"], removed["queued"])
        self.assertEqual(respawned["death_state"], "alive")
        self.assertEqual(respawned["queue_clock"], 0)
        self.assertEqual(respawned["queued"], removed["queued"])
        self.assertFalse(respawned["can_check_for_help"])
        self.assertFalse(respawned["already_called"])
        final = rows[-1]
        self.assertEqual(final["executed"], [1, 2, 3, 4])
        self.assertEqual(final["receiver_assistance_calls"], [101])

    def test_teardown_and_due_abort_never_execute_lambdas(self):
        for name in ["teardown_force_aborts_without_execute", "teardown_normal_aborts_without_execute", "due_abort_does_not_execute"]:
            with self.subTest(case=name):
                final = self.obs(name)
                self.assertEqual(final["executed"], [])
                self.assertEqual(final["aborted"], [1, 2])
                self.assertEqual(final["destroyed_without_execute"], [1, 2])
                self.assertEqual(final["queued"], [])
                self.assertFalse(final["can_check_for_help"])
                self.assertEqual(final["enemy_lookup_results"], [])

    def test_fixture_is_source_output_with_authored_domain_label(self):
        self.assertIn("no real Cell traversal", self.fixture["receiver_coverage"])
        self.assertEqual(len(self.fixture["cases"]), 23)
        self.assertEqual(self.run_metadata["observations"], 86)
        self.assertEqual(hashlib.sha256((oracle.ROOT / "vectors.json").read_bytes()).hexdigest(), self.run_metadata["fixture_sha256"])
        for name in ["compile", "execute"]:
            self.assertEqual((Path(self.run_metadata["output"]) / f"{name}.returncode").read_text(), "0\n")
            self.assertEqual((Path(self.run_metadata["output"]) / f"{name}.stderr").read_text(), "")


    def candidate(self, name):
        return self.candidates[name]["observation"]

    def candidate_leaves(self, name):
        return self.candidate(name)["evaluations"][0]["leaves"]

    def predicate_control(self, name, guid=101):
        return next(c for c in self.candidate(name)["independent_predicate_controls"] if c["guid"] == guid)

    def test_candidate_source_predicate_guards(self):
        oracle.verify_candidate_source(self.excerpts)
        mutations = [
            ("CanAssistInCombatAgainst.inc", b"CanAssist(who) && CanAttackOnSight(enemy)", b"CanAttackOnSight(enemy) && CanAssist(who)"),
            ("IsEnemy.inc", b"< REP_UNFRIENDLY", b"<= REP_UNFRIENDLY"),
            ("CanAttack.inc", b"IsEnemy(unit) || unit->IsEnemy(this)", b"IsEnemy(unit)"),
            ("CanAttackOnSight.inc", b"CanAttack(target) && IsEnemy(target)", b"IsEnemy(target) && CanAttack(target)"),
        ]
        for name, old, new in mutations:
            with self.subTest(source=name):
                changed = dict(self.excerpts)
                self.assertIn(old, changed[name])
                changed[name] = changed[name].replace(old, new)
                with self.assertRaisesRegex(ValueError, "candidate"):
                    oracle.verify_candidate_source(changed)
        for forbidden in ["ExtraFlags", "CanAssistTo(", "IsAlive(", "CanAggro("]:
            changed = dict(self.excerpts)
            changed["CanAssistInCombatAgainst.inc"] += forbidden.encode()
            with self.assertRaisesRegex(ValueError, "unexpected outdoor candidate predicate"):
                oracle.verify_candidate_source(changed)

    def test_candidate_self_short_circuits_every_leaf(self):
        self.assertEqual(self.candidate("self_rejected_before_all_predicates")["evaluations"],
                         [dict(guid=1, accepted=False, leaves=[])])
        self.assertEqual(self.candidate("same_entry_distinct_guid_eligible")["selected_guids"], [101])
        row = self.candidates["same_entry_distinct_guid_eligible"]["input"]
        self.assertEqual(row["authored_units"][0]["authored_entry"], row["authored_units"][2]["authored_entry"])

    def test_candidate_combat_guard_precedes_relation_and_geometry(self):
        leaves = self.candidate_leaves("outdoor_in_combat_rejected_before_relation")
        self.assertEqual(leaves, ["instanceable:0:false", "flags_524288:101:true"])
        self.assertEqual(self.candidate_leaves("semantic_denial_skips_false_range_and_los"), leaves)

    def test_candidate_owner_flag_and_relation_precede_enemy(self):
        leaves = self.candidate_leaves("owner_uninteractible_rejected_before_relation")
        self.assertEqual(leaves, ["instanceable:0:false", "flags_524288:101:false", "flags_33554432:1:true"])
        leaves = self.candidate_leaves("candidate_neutral_to_owner_rejected")
        self.assertFalse(any("range" in item or "los" in item or item.startswith("evading_home") for item in leaves))
        self.assertEqual(self.predicate_control("candidate_neutral_to_owner_rejected")["owner_reaction"], 3)

    def test_candidate_defensive_and_no_call_flags_are_not_predicates(self):
        baseline = self.candidate("hostile_defensive_eligible")
        for name in ["hostile_without_defensive_eligible", "hostile_no_call_assist_still_candidate"]:
            self.assertEqual(self.candidate(name), baseline)
        self.assertEqual(baseline["selected_guids"], [101])

    def test_candidate_neutral_player_can_attack_but_cannot_assist_combat(self):
        for name in ["candidate_neutral_to_player_attackable_not_enemy", "missing_enemy_template_attackable_not_enemy", "npc_reverse_hostility_attackable_but_not_enemy"]:
            with self.subTest(case=name):
                control = self.predicate_control(name)
                self.assertTrue(control["can_attack"])
                self.assertFalse(control["is_enemy"])
                self.assertFalse(control["can_attack_on_sight"])
                self.assertEqual(self.candidate(name)["selected_guids"], [])
                self.assertFalse(any("range" in item or "los" in item for item in self.candidate_leaves(name)))
        control = self.predicate_control("candidate_friendly_to_player_not_attackable")
        self.assertFalse(control["can_attack"])
        self.assertFalse(control["is_enemy"])

    def test_candidate_faction_mask_priority_reciprocity_and_slot_zero(self):
        accepted = ["hostile_enemy_mask_precedes_friend_mask", "reciprocal_friend_mask_assists",
                    "enemy_faction_list_matches", "enemy_faction_list_later_match",
                    "friend_faction_list_matches", "friend_faction_list_later_match",
                    "reciprocal_friend_faction_assists"]
        denied = ["enemy_faction_list_slot_zero_disables_later_match",
                  "friend_faction_list_slot_zero_disables_later_match",
                  "reciprocal_friend_slot_zero_disables_later_match",
                  "missing_candidate_template_neutral", "missing_owner_template_neutral"]
        for name in accepted:
            self.assertEqual(self.candidate(name)["selected_guids"], [101], name)
        for name in denied:
            self.assertEqual(self.candidate(name)["selected_guids"], [], name)

    def test_candidate_sight_early_exits_before_attack_flags(self):
        for name, last in [
            ("enemy_evading_precedes_attack", "evading_home:77:true"),
            ("enemy_feigning_precedes_attack", "state_4:77:true"),
            ("disabled_pet_enemy_precedes_attack", "pet_disabled:77:true"),
        ]:
            self.assertEqual(self.candidate_leaves(name)[-1], last)
            self.assertEqual(self.candidate(name)["selected_guids"], [])
        for name in ["ignoring_enemy_feign_continues", "enabled_pet_enemy_continues"]:
            self.assertEqual(self.candidate(name)["selected_guids"], [101])
        self.assertNotIn("state_4:77:true", self.candidate_leaves("ignoring_enemy_feign_continues"))

    def test_candidate_attack_flags_and_bidirectional_immunity(self):
        names = [name for name in self.candidates if name.startswith("enemy_") and name.endswith("_cannot_attack")]
        names.append("candidate_immune_to_player_cannot_attack")
        self.assertEqual(len(names), 7)
        for name in names:
            control = self.predicate_control(name)
            self.assertFalse(control["can_attack"], name)
            self.assertTrue(control["is_enemy"], name)
            self.assertFalse(any("range" in item or "los" in item for item in self.candidate_leaves(name)))

    def test_candidate_range_and_los_are_ordered_authored_leaves(self):
        passed = self.candidate_leaves("hostile_defensive_eligible")
        self.assertEqual(passed[-4:], ["range_query:1:101:1084227584", "authored_range:101:true",
                                      "los_query:1:101", "authored_los:101:true"])
        self.assertEqual(self.candidate_leaves("range_failure_skips_los")[-2:],
                         ["range_query:1:101:1084227584", "authored_range:101:false"])
        self.assertFalse(any("los" in item for item in self.candidate_leaves("range_failure_skips_los")))
        self.assertEqual(self.candidate_leaves("los_failure_after_range")[-1], "authored_los:101:false")
        self.assertEqual(self.candidate("range_failure_skips_los")["selected_guids"], [])
        self.assertEqual(self.candidate("los_failure_after_range")["selected_guids"], [])

    def test_candidate_outdoor_has_no_liveness_or_initiation_guard(self):
        for name in ["outdoor_dead_candidate_not_checked", "outdoor_dead_enemy_not_checked",
                     "outdoor_cannot_aggro_not_checked", "outdoor_stunned_candidate_not_checked"]:
            self.assertEqual(self.candidate(name), self.candidate("hostile_defensive_eligible"), name)

    def test_candidate_dungeon_branch_bypasses_outdoor_checks(self):
        for name in ["dungeon_in_combat_bypasses_outdoor_combat_guard",
                     "dungeon_neutral_player_can_join_without_is_enemy",
                     "dungeon_owner_uninteractible_bypassed"]:
            self.assertEqual(self.candidate(name)["selected_guids"], [101], name)
            leaves = self.candidate_leaves(name)
            self.assertEqual(leaves[0], "instanceable:0:true")
            self.assertFalse(any(item.startswith("flags_524288") or item == "faction_template:1" for item in leaves))
        self.assertFalse(self.predicate_control("dungeon_neutral_player_can_join_without_is_enemy")["is_enemy"])
        self.assertFalse(self.predicate_control("dungeon_owner_uninteractible_bypassed")["can_assist"])

    def test_candidate_dungeon_join_guards(self):
        self.assertEqual(self.candidate_leaves("dungeon_cannot_enter_combat_rejected"), ["instanceable:0:true"])
        self.assertEqual(self.candidate_leaves("dungeon_evading_candidate_rejected"),
                         ["instanceable:0:true", "evading_home:101:true"])
        for name in ["dungeon_cannot_aggro_rejected", "dungeon_stunned_candidate_rejected",
                     "dungeon_feigning_candidate_rejected", "dungeon_unattackable_aura_rejected",
                     "dungeon_enemy_immunity_rejected"]:
            self.assertEqual(self.candidate(name)["selected_guids"], [], name)
            self.assertFalse(any("range" in item or "los" in item for item in self.candidate_leaves(name)))

    def test_candidate_authored_sequence_preserves_duplicates_and_membership(self):
        row = self.candidate("authored_order_duplicates_and_membership")
        self.assertEqual([value["guid"] for value in row["evaluations"]], [1, 102, 103, 101, 102])
        self.assertEqual(row["selected_guids"], [102, 101, 102])
        self.assertEqual(row["evaluations"][1], row["evaluations"][4])
        self.assertTrue(self.predicate_control("authored_order_duplicates_and_membership", 104)["can_attack_on_sight"])
        self.assertNotIn(104, row["selected_guids"])
        self.assertEqual(self.candidate("authored_empty_candidate_sequence")["evaluations"], [])
        self.assertEqual(self.candidate("authored_empty_candidate_sequence")["selected_guids"], [])

    def test_candidate_fixture_is_independent_source_output_with_bounded_label(self):
        self.assertIn("no real Cell traversal", self.candidate_fixture["coverage"])
        self.assertIn("authored range/LOS outcomes", self.candidate_fixture["leaf_domain"])
        self.assertEqual(self.run_metadata["candidate_cases"], 55)
        self.assertEqual(self.run_metadata["candidate_evaluations"], 58)
        self.assertEqual(hashlib.sha256((oracle.ROOT / "candidate_vectors.json").read_bytes()).hexdigest(),
                         self.run_metadata["candidate_fixture_sha256"])
        for name in ["candidate-compile", "candidate-execute"]:
            self.assertEqual((Path(self.run_metadata["output"]) / f"{name}.returncode").read_text(), "0\n")
            self.assertEqual((Path(self.run_metadata["output"]) / f"{name}.stderr").read_text(), "")


    def help(self, name, index=-1):
        return self.helpers[name]['observations'][index]

    def help_leaves(self, name, index=-1):
        return self.help(name, index)['leaves']

    def test_peer_help_source_order_and_radius_guards(self):
        oracle.verify_peer_help_source(self.excerpts)
        mutations = [
            ('CheckForHelp.inc', b'me->IsInCombat() || !me->CanCallForAssistance()', b'!me->CanCallForAssistance() || me->IsInCombat()'),
            ('CheckForHelp.inc', b'!me->CanCheckForHelp() || !who->CanCheckForHelp()', b'!who->CanCheckForHelp() || !me->CanCheckForHelp()'),
            ('CheckForHelp.inc', b'who->IsConfused() || who->IsStunned()', b'who->IsStunned() || who->IsConfused()'),
            ('CheckForHelp.inc', b'me->CanInitiateAttack() && me->CanAttackOnSight(victim)', b'me->CanAttackOnSight(victim) && me->CanInitiateAttack()'),
            ('CheckForHelp.inc', b'victim->isInAccessablePlaceFor(me)', b'who->isInAccessablePlaceFor(me)'),
            ('CheckForHelp.inc', b'me->IsWithinDistInMap(who, distance)', b'me->IsWithinDistInMap(victim, distance)'),
            ('CheckForHelp.inc', b'IsWithinLOSInMap(who, true)', b'IsWithinLOSInMap(who, false)'),
            ('CheckForHelp.inc', b'ORDER_FLEEING', b'ORDER_RETREATING'),
            ('GetAttackerForHelper.inc', b'if (GetVictim() != nullptr)', b'if (GetVictim() == nullptr)'),
            ('GetAttackerForHelper.inc', b'return *(m_attackers.begin());', b'return *(m_attackers.rbegin());'),
            ('HelpSightCaller.inc', b'CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS', b'CONFIG_FLOAT_CREATURE_FAMILY_ASSISTANCE_RADIUS'),
            ('HelpWorldRadiusDefault.inc', b'5.0f', b'10.0f'),
        ]
        for name, old, new in mutations:
            with self.subTest(excerpt=name, mutation=old):
                changed = dict(self.excerpts)
                self.assertIn(old, changed[name])
                changed[name] = changed[name].replace(old, new)
                with self.assertRaisesRegex(ValueError, 'peer-help'):
                    oracle.verify_peer_help_source(changed)
        changed = dict(self.excerpts)
        changed['CheckForHelp.inc'] += b'// CallForHelp is not the radius source\n'
        with self.assertRaisesRegex(ValueError, 'template CallForHelp'):
            oracle.verify_peer_help_source(changed)

    def test_peer_help_active_victim_wins_over_fallback(self):
        for name in ['positive_accessibility_continues_to_attack', 'active_victim_precedes_different_attacker']:
            self.assertEqual(self.help(name)['attack_start_guids'], [77])
            self.assertEqual(self.help_leaves(name)[:2], ['get_victim:101', 'get_victim:101'])
        denied = self.help('active_denied_victim_does_not_fall_back')
        self.assertEqual(denied['attack_start_guids'], [])
        self.assertFalse(any(':78' in value for value in denied['leaves']))
        fallback = self.help('singleton_attacker_fallback')
        self.assertEqual(fallback['attack_start_guids'], [78])
        self.assertEqual(fallback['leaves'].count('get_victim:101'), 1)
        self.assertIn('accessibility_query:78:1', fallback['leaves'])
        self.assertEqual(self.help_leaves('no_victim_or_attacker_returns_before_flags'), ['get_victim:101'])

    def test_peer_help_combat_and_call_flags_have_source_order(self):
        prefix = ['get_victim:101', 'get_victim:101']
        self.assertEqual(self.help_leaves('observer_combat_before_call_flags'), prefix + ['flags_524288:1:true'])
        self.assertEqual(self.help_leaves('observer_cannot_call'), prefix + ['flags_524288:1:false', 'can_call:1:false'])
        target = prefix + ['flags_524288:1:false', 'can_call:1:true', 'can_call:101:false']
        self.assertEqual(self.help_leaves('target_cannot_call'), target)
        self.assertEqual(self.help_leaves('target_cannot_call_before_observer_help_disabled'), target)

    def test_peer_help_both_live_help_flags_are_independent(self):
        prefix = ['get_victim:101', 'get_victim:101', 'flags_524288:1:false', 'can_call:1:true', 'can_call:101:true']
        observer = prefix + ['can_check_help:1:false']
        self.assertEqual(self.help_leaves('observer_help_disabled'), observer)
        self.assertEqual(self.help_leaves('both_help_disabled_observer_first'), observer)
        self.assertEqual(self.help_leaves('target_help_disabled'), prefix + ['can_check_help:1:true', 'can_check_help:101:false'])
        for name in ['observer_help_disabled', 'both_help_disabled_observer_first', 'target_help_disabled']:
            self.assertEqual(self.help(name)['attack_start_guids'], [])

    def test_peer_help_setters_change_each_next_evaluation(self):
        for name in ['live_help_fields_rechecked', 'live_call_fields_rechecked']:
            rows = self.helpers[name]['observations']
            self.assertEqual([row['phase'] for row in rows], ['enabled', 'observer_disabled', 'target_disabled', 'reenabled'])
            self.assertEqual([row['attack_start_guids'] for row in rows], [[77], [], [], [77]])
            self.assertEqual(rows[0]['leaves'], rows[3]['leaves'])
        rows = self.helpers['live_help_fields_rechecked']['observations']
        self.assertEqual([(r['observer_can_check'], r['target_can_check']) for r in rows],
                         [(True, True), (False, True), (True, False), (True, True)])
        rows = self.helpers['live_call_fields_rechecked']['observations']
        self.assertEqual([(r['observer_can_call'], r['target_can_call']) for r in rows],
                         [(True, True), (False, True), (True, False), (True, True)])

    def test_peer_help_target_control_precedes_observer_initiation(self):
        for name, last in [
            ('target_retreating_before_confusion_and_stun', 'state_1024:101:true'),
            ('target_confused_before_stunned', 'flags_4194304:101:true'),
            ('target_stunned_flag', 'flags_262144:101:true'),
        ]:
            leaves = self.help_leaves(name)
            self.assertEqual(leaves[-1], last)
            self.assertFalse(any(value.startswith('state_12:1:') for value in leaves))
            self.assertEqual(self.help(name)['attack_start_guids'], [])
        # These deliberately split authored flag and state fields. Source target
        # IsConfused/IsStunned reads flags; observer initiation reads state bits.
        for name in ['target_confused_state_without_flag_not_checked', 'target_stunned_state_without_flag_not_checked']:
            self.assertEqual(self.help(name)['attack_start_guids'], [77])

    def test_peer_help_initiation_guards_short_circuit_sight(self):
        names = ['observer_stunned_state_denies_initiation', 'observer_feigning_state_denies_initiation',
                 'observer_spawning_denies_initiation', 'observer_uninteractible_denies_initiation',
                 'observer_cannot_aggro_denies_initiation', 'observer_forced_attack_still_requires_aggro',
                 'observer_initiation_denial_precedes_bad_victim']
        for name in names:
            self.assertEqual(self.help(name)['attack_start_guids'], [], name)
            self.assertFalse(any(value.startswith('evading_home:77:') or value.startswith('accessibility_query:')
                                 for value in self.help_leaves(name)), name)
        self.assertEqual(self.help('observer_forced_attack_bypasses_spawning')['attack_start_guids'], [77])
        self.assertNotIn('flags_33554434:1:true', self.help_leaves('observer_forced_attack_bypasses_spawning'))

    def test_peer_help_sight_denials_precede_accessibility(self):
        for name in ['victim_evading_denies_sight', 'victim_feigning_denies_sight',
                     'victim_untargetable_denies_attack', 'victim_npc_immunity_denies_attack',
                     'observer_player_immunity_denies_attack', 'observer_neutral_to_victim_denies_sight']:
            self.assertEqual(self.help(name)['attack_start_guids'], [], name)
            self.assertFalse(any(value.startswith('accessibility_query:') for value in self.help_leaves(name)), name)

    def test_peer_help_accessibility_and_visibility_are_distinct_leaves(self):
        inaccessible = self.help_leaves('inaccessible_victim_stops_before_visibility')
        self.assertEqual(inaccessible[-2:], ['accessibility_query:77:1', 'authored_accessibility:77:false'])
        self.assertFalse(any(value.startswith('visibility_query:') for value in inaccessible))
        invisible = self.help_leaves('invisible_victim_stops_before_target_range')
        self.assertEqual(invisible[-4:], ['accessibility_query:77:1', 'authored_accessibility:77:true',
                                         'visibility_query:77:1:1:false', 'authored_visibility:77:false'])
        self.assertFalse(any(value.startswith('range_query:') for value in invisible))

    def test_peer_help_positive_path_has_exact_geometry_and_late_assist_order(self):
        leaves = self.help_leaves('positive_accessibility_continues_to_attack')
        ordered = ['can_aggro:1:true', 'evading_home:77:false', 'accessibility_query:77:1',
                   'authored_accessibility:77:true', 'visibility_query:77:1:1:false',
                   'authored_visibility:77:true', 'range_query:1:101:1084227584',
                   'authored_range:101:true', 'los_query:1:101:true', 'authored_los:101:true',
                   'instanceable:0:false', 'flags_33554432:101:false', 'attack_start:77', 'ai_query:101']
        self.assertEqual([leaves.index(value) for value in ordered], sorted(leaves.index(value) for value in ordered))
        self.assertEqual(leaves.count('evading_home:77:false'), 2)  # late CanAssistInCombatAgainst repeats sight
        self.assertEqual(self.help('positive_accessibility_continues_to_attack')['attack_start_guids'], [77])
        for name in ['target_geometry_not_victim_geometry', 'victim_accessibility_not_target_accessibility']:
            self.assertEqual(self.help(name), self.help('positive_accessibility_continues_to_attack'))

    def test_peer_help_range_and_los_denials_precede_assist(self):
        range_leaves = self.help_leaves('target_range_failure_stops_before_los')
        self.assertEqual(range_leaves[-2:], ['range_query:1:101:1084227584', 'authored_range:101:false'])
        self.assertFalse(any(value.startswith('los_query:') for value in range_leaves))
        los_leaves = self.help_leaves('target_los_failure_stops_before_assist')
        self.assertEqual(los_leaves[-2:], ['los_query:1:101:true', 'authored_los:101:false'])
        for leaves in [range_leaves, los_leaves]:
            self.assertNotIn('instanceable:0:false', leaves)
        for name in ['target_uninteractible_denies_assist_after_geometry', 'target_neutral_denies_assist_after_geometry']:
            leaves = self.help_leaves(name)
            self.assertLess(leaves.index('authored_los:101:true'), leaves.index('instanceable:0:false'))
            self.assertEqual(self.help(name)['attack_start_guids'], [])

    def test_peer_help_radius_uses_world_default_not_template_call_for_help(self):
        baseline = self.help('positive_accessibility_continues_to_attack')
        self.assertEqual(baseline['radius_bits'], oracle.bits(5.))
        for name in ['world_default_ignores_template_positive', 'world_default_ignores_template_zero',
                     'world_default_ignores_template_negative']:
            self.assertEqual(self.help(name), baseline)
        override = self.help('authored_world_radius_override')
        self.assertEqual(override['radius_bits'], oracle.bits(9.))
        self.assertIn(f'range_query:1:101:{oracle.bits(9.)}', override['leaves'])

    def test_peer_help_flee_order_only_interrupts_after_attack(self):
        row = self.help('fleeing_target_interrupts_panic_after_attack')
        self.assertEqual(row['attack_start_guids'], [77])
        self.assertEqual(row['panic_interrupt_guids'], [101])
        self.assertEqual(row['leaves'][-6:], ['attack_start:77', 'ai_query:101', 'ai_query:101',
                                            'ai_order:101:2', 'motion_query:101', 'interrupt_panic:101'])
        for name in ['retreating_order_without_retreat_state_does_not_interrupt', 'target_without_ai_still_attacked']:
            self.assertEqual(self.help(name)['attack_start_guids'], [77])
            self.assertEqual(self.help(name)['panic_interrupt_guids'], [])
        denied = self.help('fleeing_denied_target_never_interrupts')
        self.assertEqual(denied['panic_interrupt_guids'], [])
        self.assertFalse(any(value.startswith('ai_query:') for value in denied['leaves']))

    def test_peer_help_direct_method_boundary_not_caller_admission(self):
        baseline = self.help('positive_accessibility_continues_to_attack')
        self.assertEqual(self.help('direct_helper_does_not_check_target_combat'), baseline)
        self.assertEqual(self.help('live_help_ignores_authored_raw_extra_flags'), baseline)
        self.assertIn('who->GetObjectGuid().IsCreature() && who->IsInCombat()', self.excerpts['HelpSightCaller.inc'].decode())
        self.assertFalse(next(row for row in self.manifest['excerpts'] if row['name'] == 'HelpSightCaller.inc')['compiled'])

    def test_peer_help_fixture_retains_bounded_source_evidence(self):
        self.assertIn('no real map', self.helper_fixture['coverage'])
        self.assertIn('zero/singleton attacker set', self.helper_fixture['leaf_domain'])
        self.assertEqual(self.run_metadata['peer_help_cases'], 51)
        self.assertEqual(self.run_metadata['peer_help_observations'], 57)
        self.assertEqual(hashlib.sha256((oracle.ROOT / 'peer_help_vectors.json').read_bytes()).hexdigest(),
                         self.run_metadata['peer_help_fixture_sha256'])
        # The three established fixtures must remain byte-for-byte unchanged.
        self.assertEqual(self.run_metadata['fixture_sha256'], '8aaaca48b61b6d2f472ef292c142ba35b9e3e341df48d83c9b243f433ecd3aea')
        self.assertEqual(self.run_metadata['candidate_fixture_sha256'], '9ba3ffe467f2cae8dd60e03356443654b51969f295e4ed5e2d79e8827057a747')
        self.assertEqual(self.run_metadata['peer_help_fixture_sha256'], '736f8f0e267d63c641f15974d67f1f3d8ddd3e502726e521c0eb8cc7432f63b8')
        for name in ['peer-help-compile', 'peer-help-execute']:
            self.assertEqual((Path(self.run_metadata['output']) / f'{name}.returncode').read_text(), '0\n')
            self.assertEqual((Path(self.run_metadata['output']) / f'{name}.stderr').read_text(), '')


    def test_accessibility_wet_cache_reads_only_helper_swim(self):
        count = 0
        for row in self.accessibility[:16]:
            value = row['input']
            if value['environment_flags'] == 1:
                count += 1
                self.assertEqual(row['observation'], dict(accessible=value['can_swim'], leaves=[
                    'cached_environment_flags:77:1',
                    'can_swim:1:' + str(value['can_swim']).lower()]))
        self.assertEqual(count, 8)

    def test_accessibility_dry_cache_walk_short_circuits_flight(self):
        count = 0
        for row in self.accessibility[:16]:
            value = row['input']
            if value['environment_flags'] == 0:
                count += 1
                leaves = ['cached_environment_flags:77:0',
                    'can_walk:1:' + str(value['can_walk']).lower()]
                if not value['can_walk']:
                    leaves.append('can_fly:1:' + str(value['can_fly']).lower())
                self.assertEqual(row['observation'], dict(
                    accessible=value['can_walk'] or value['can_fly'], leaves=leaves))
        self.assertEqual(count, 8)

    def test_accessibility_environment_water_bit_is_not_any_liquid(self):
        for row in self.accessibility[16:]:
            value = row['input']
            water = bool(value['environment_flags'] & 1)
            self.assertEqual(row['observation']['accessible'],
                value['can_swim'] if water else value['can_walk'], row['name'])
            self.assertEqual(row['observation']['leaves'][0],
                'cached_environment_flags:77:' + str(value['environment_flags']))
            self.assertEqual(row['observation']['leaves'][1].split(':')[0],
                'can_swim' if water else 'can_walk')

    def test_accessibility_virtual_dispatch_and_source_body_guards(self):
        oracle.verify_accessibility_source(self.excerpts)
        for name, original, changed in [
            ('PlayerVictimAccessibility.inc', b'unit->CanSwim()', b'CanSwim()'),
            ('PlayerVictimAccessibility.inc', b'unit->CanWalk() || unit->CanFly()', b'unit->CanFly() || unit->CanWalk()'),
            ('AccessibilityUnitWaterDispatch.inc', b'virtual bool', b'bool'),
            ('AccessibilityPlayerWater.inc', b'override', b''),
            ('AccessibilityPlayerWater.inc', b'm_environmentFlags & ENVIRONMENT_FLAG_IN_WATER', b'm_environmentFlags'),
            ('AccessibilityEnvironmentFlags.inc', b'IN_WATER       = 0x01', b'IN_WATER       = 0x02'),
        ]:
            with self.subTest(excerpt=name, mutation=changed):
                excerpts = dict(self.excerpts)
                self.assertIn(original, excerpts[name])
                excerpts[name] = excerpts[name].replace(original, changed)
                with self.assertRaisesRegex(ValueError, 'player accessibility source shape changed'):
                    oracle.verify_accessibility_source(excerpts)

    def test_accessibility_new_player_source_and_all_excerpts_are_pinned(self):
        self.assertEqual(oracle.HASHES['src/game/Entities/Player.h'],
            'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb')
        names = {'PlayerVictimAccessibility.inc', 'AccessibilityUnitWaterDispatch.inc',
            'AccessibilityPlayerWater.inc', 'AccessibilityEnvironmentFlags.inc'}
        records = [row for row in self.manifest['excerpts'] if row['name'] in names]
        self.assertEqual(len(records), 4)
        self.assertTrue(all(row['compiled'] for row in records))

    def test_accessibility_fixture_and_executable_are_independent(self):
        self.assertEqual(self.run_metadata['accessibility_cases'], 32)
        self.assertIn('authored cached environment flags and helper capability leaves',
            self.accessibility_fixture['coverage'])
        self.assertIn('no cache lifecycle', self.accessibility_fixture['coverage'])
        self.assertTrue(all('accessible' not in row['input'] for row in self.accessibility))
        self.assertEqual(hashlib.sha256((oracle.ROOT / 'accessibility_vectors.json').read_bytes()).hexdigest(),
            self.run_metadata['accessibility_fixture_sha256'])
        for name in ['accessibility-compile', 'accessibility-execute']:
            run = Path(self.run_metadata['output'])
            self.assertEqual((run / (name + '.returncode')).read_text().strip(), '0')
            self.assertEqual((run / (name + '.stderr')).read_bytes(), b'')


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compiler", default="g++")
    args, rest = parser.parse_known_args()
    CORE, OUTPUT, COMPILER = args.core.resolve(), args.output.resolve(), args.compiler
    unittest.main(argv=[sys.argv[0], *rest])
