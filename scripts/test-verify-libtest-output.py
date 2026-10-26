#!/usr/bin/env python3
"""No-compile fixtures for the actual joined-suite CI shell gates."""

import importlib.util
import json
import sys
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_libtest_output", ROOT / "scripts/verify-libtest-output.py"
)
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)
DEFAULT_BUDGET_PROBE = "resident_default_budget_controller_saves_cold_and_first_retained"
F05_CONTROLLER_TESTS = ('resident_f05_default_budget_actual_controller_comparison', 'resident_f05_held_source_control_cancel_and_quit_never_ack', 'resident_f05_current_scene_and_source_changes_refuse_before_save')
COMPARISON_PATHS = (
    '.github/workflows/resident-controller-profile-comparison.yml',
    'crates/benilla-app/src/net/p2p/world/native_geometry/mod.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/core.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/resident_dispatch.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/resident_dispatch/tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/freshness_timings.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/inventory.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/inventory_tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/tests/wrapper_equivalence.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/owned/freshness.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/owned/freshness/tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/resident_dispatch/fifo.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/resident_dispatch/fifo/tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/resident_dispatch/transport.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/resident_result_controller_tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/retained_phased_controller_tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/retained.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/retained/observation_equivalence.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/chase_los.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/chase_los/equivalence.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/retained/prefix_equivalence.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/owned.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height/owned/f05_scope.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/f05.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/tests/f05_equivalence.rs',
    'scripts/verify-f05-privacy.py',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/phased.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/tests.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/tests/cold_phased_negatives.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned/transport.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_resident_local.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_resident_local/ledger.rs',
    'crates/benilla-app/src/net/p2p/world/native_geometry/source_route_resident_local/retained.rs',
    'crates/benilla-native-vmap-source/src/lib.rs',
    'crates/benilla-native-vmap-source/src/map_bundle.rs',
    'crates/benilla-native-vmap-source/src/source.rs',
    'crates/benilla-native-vmap-source/src/source_tests.rs',
    'crates/benilla-native-vmap-source/src/profiling.rs',
    'crates/benilla-native-vmap-source/src/profiling_source_tests.rs',
)
SUITES = {
    "Verify resident FIFO selection": (
        "benilla-native-corridor-tests",
        "core::resident_dispatch::fifo::tests::",
        (
            "resident_fifo_preserves_original_identity_capacity_and_attempt_accounting",
            "resident_fifo_tail_fill_retires_exact_work_without_releasing_busy_worker",
            "resident_fifo_cancellation_holds_slot_until_drain_and_reuses_one_worker",
            "resident_fifo_expiry_foreign_owner_and_attachment_never_resume",
            "resident_fifo_disconnect_stays_terminal_and_drop_revokes_without_joining",
            "resident_fifo_ready_rechecks_foreign_source_owner_and_changed_source_without_ack",
        ),
    ),
    "Verify resident dispatch receipt identity": (
        "benilla-native-corridor-tests",
        "core::resident_dispatch::tests::",
        (
            "resident_dispatch_preserves_original_work_selection_without_queue_admission",
            "resident_dispatch_refuses_changed_context_body_player_and_branch_tampering",
            "resident_dispatch_refuses_foreign_work_attachment_dispatch_and_resident_allocation",
            "resident_dispatch_revocation_and_original_expiry_prevent_certification",
            "resident_dispatch_refuses_foreign_capture_and_dropped_work_before_resume",
            "resident_dispatch_profile_modes_preserve_provider_guard_short_circuit",
        ),
    ),
    "Verify resident observation without transport admission": (
        "benilla-native-geometry-tests",
        "core::staged_job_tests::source_observation_work_retains_exact_resident_request_without_transport_admission",
        ("",),
    ),
    "Verify bounded capture freshness worker": (
        "benilla-native-geometry-tests",
        "products::source_height::owned::freshness::tests::",
        (
            "captures_are_send_sync_and_caller_operations_finish_while_worker_is_held",
            "one_slot_includes_completed_undrained_work_and_reuses_the_same_worker",
            "cancellation_retirement_and_original_expiry_do_not_release_a_held_slot",
            "cancellation_after_success_and_after_publication_cannot_escape_the_token",
            "completed_source_errors_keep_their_provenance_after_cancellation_or_retirement",
            "observations_reject_equal_byte_foreign_captures_and_replacement_work",
            "drained_observations_still_reject_retired_expired_and_stopped_owners",
            "already_expired_original_lease_is_refused_without_occupying_the_worker",
            "drained_observation_is_revoked_by_explicit_cancel_or_a_new_accepted_request",
            "dropped_worker_never_joins_a_held_verification_and_releases_owned_captures",
            "disconnected_worker_stays_stopped_instead_of_spawning_a_replacement",
            "resumed_observation_clamps_deadline_and_uses_original_request_on_caller_thread",
            "resumed_observation_rejects_foreign_capture_or_work_before_callback",
            "resumed_observation_keeps_revocation_live_until_callback_returns",
            "resumed_observation_refuses_late_slice_expiry_and_changed_current_sources",
            "resumed_observation_cannot_renew_original_expiry_after_ready",
        ),
    ),
    "Verify cooperative terrain freshness cancellation": (
        "benilla-native-terrain",
        "source::tests::cancellation::",
        (
            "cancellation_before_any_access_wins_even_when_directory_has_disappeared",
            "every_app_independent_and_final_checkpoint_stops_then_allows_retry",
            "source_failure_is_not_repolled_or_reclassified_as_cancellation",
            "only_the_private_marker_classifies_cancellation_through_contexts",
            "prepared_public_api_cancels_retries_and_preserves_evidence_and_diagnostics",
            "uncancelled_api_keeps_corruption_winner_and_absence_fail_closed",
        ),
    ),
    "Verify inventory freshness wrapper error order": (
        "benilla-native-corridor-tests",
        "products::source_height::inventory::inventory_tests::",
        (
            "inventory_f04_and_f05_poll_order_on_live_sources",
            "inventory_f04_postpoll_masks_real_source_fault_but_f05_preserves_it",
            "inventory_f04_prefix_cancel_and_deadline_skip_source",
        ),
    ),
    'Verify one-slot F05 raw worker': (
        "benilla-native-corridor-tests",
        'products::source_height::owned::freshness::tests::f05::',
        (
            'continuation_keeps_original_guard_and_occupied_slot_until_exact_raw_drain',
            'raw_success_and_source_error_survive_late_cancel_retirement_and_stop',
            'original_callback_expiry_cancel_and_stop_keep_running_continuation_tombstoned',
            'foreign_products_work_worker_and_revoked_guard_cannot_submit_continuation',
            'control_wake_precedes_held_completion_and_disconnect_never_fabricates_raw_result',
            'dropped_continuation_worker_does_not_join_and_pinned_guard_remains_cancelled',
            'foreign_raw_result_identity_never_releases_original_slot_or_mints_completion',
            'completion_must_match_original_guard_before_success_token_is_minted',
            'f05_only_gate_never_blocks_observation_and_rechecks_original_guard_after_release',
        ),
    ),
    'Verify one-slot F05 private source frames': (
        "benilla-native-corridor-tests",
        'source_route_filter::resident::owned::f05::tests::',
        (
            'f05_source_frames_preserve_scope_exit_debt_and_exact_inventory_custody',
            'f05_source_frames_inner_error_and_cancelled_exit_match_original_wrappers',
            'f05_source_frames_failed_entry_skips_reader_and_exit',
            'f05_actual_fifo_dropped_receiver_retires_only_new_unacknowledged_stage',
        ),
    ),
    "Verify scoped resident source ownership": (
        "benilla-native-corridor-tests",
        "source_route_filter::resident::owned::tests::",
        (
            "scoped_resident_keeps_cold_ack_and_retained_native_owner_across_views",
            "scoped_resident_refuses_foreign_owner_expiry_cancel_and_changed_sources_without_state_loss",
            "scoped_resident_bind_rejects_equal_byte_foreign_inventory_and_different_source_products",
            "fifo_resident_ready_certifies_current_physics_without_transport_success_or_ack",
            "fifo_retained_ready_preserves_committed_cold_without_current_transport_or_ack",
            "resident_result_transport_requeues_exact_work_and_rejects_forged_receipts",
            "resident_result_transport_dropped_reply_cannot_promote_staging",
            "resident_result_transport_rechecks_geometry_after_worker_completion",
            "resident_result_transport_tail_full_and_post_mint_expiry_retire_only_unsent_candidate",
            "cold_phased_negatives::cold_fifo_final_retry_drop_tail_full_and_post_mint_expiry_do_not_ack",
            "cold_phased_negatives::cold_fifo_prepared_refusals_preserve_original_work_and_uncommitted_ledger",
            "wrapper_equivalence::owned_wrapper_old_literal_and_operand_refactor_preserve_unwind_and_source_order",
            "owned_retained_prefix_literal_oracle_preserves_two_source_effects",
            "owned_retained_observation_literal_oracle_preserves_final_source_effect",
            "owned_los_literal_oracle_preserves_source_and_invocation_operands",
        ),
    ),
    "Verify two-turn retained controller transport": (
        "benilla-app",
        "net::p2p::native_geometry::retained_phased_controller_tests::retained_phased_fifo_saves_after_distinct_owner_turns",
        ("",),
    ),
    "Verify two-turn cold controller transport": (
        "benilla-app",
        "net::p2p::native_geometry::retained_phased_controller_tests::cold_phased_fifo_saves_after_distinct_owner_turns",
        ("",),
    ),
    "Verify resident result controller transport": (
        "benilla-app",
        "net::p2p::native_geometry::resident_result_controller_tests::",
        (
            "resident_result_transport_actual_controller_saves_cold_and_first_retained",
            "resident_result_transport_checkpoint_failure_precedes_ack_and_recovery_never_reruns",
            "resident_result_transport_lost_ack_retries_exact_notice_without_reexecution",
            "resident_result_transport_changed_source_allows_historical_ack_not_fresh_geometry",
            "resident_result_transport_worker_ready_rechecks_current_physics_before_reply",
        ),
    ),
    "Verify owned source bundle prerequisite": (
        "benilla-native-vmap-source",
        "tests::doodad_projection_capture_tests::owned_bundle_",
        (
            "cancellation_before_and_at_final_assembly_check_returns_no_bundle",
            "moves_outlives_borrowed_capture_and_preserves_products",
            "recheck_rejects_source_change_during_and_after_assembly",
        ),
    ),
    "Verify owned height reader prerequisite": (
        "benilla-native-geometry-tests",
        "products::source_height::owned::tests::",
        (
            "owned_sources_move_outlive_borrowed_views_and_preserve_exact_height",
            "owned_sources_recheck_after_reader_and_preserve_cancellation",
            "owned_sources_refuse_changed_or_missing_sources_before_reader",
            "owned_sources_prepare_once_and_reuse_exact_height_liquid_and_ray_after_moves",
            "owned_sources_construction_refuses_cancellation_changed_and_deleted_sources",
            "owned_sources_preserve_typed_terrain_cancellation_and_genuine_source_failure",
        ),
    ),
    "Verify joined resident installed corridor": (
        "benilla-native-corridor-tests",
        "real_corridor_fixture::installed_source::joined::",
        (
            "resident_joined_actual_installed_owner_and_corridor",
            "resident_joined_rechecks_live_geometry_and_final_source_after_segment",
        ),
    ),
    "Verify joined resident source geometry": (
        "benilla-native-geometry-tests",
        "source_pathfinder_query::resident_joined_tests::",
        (
            "resident_joined_cold_preserves_runtime_source_ancestry_and_expiry",
            "resident_joined_missing_generated_endpoint_refuses_calculation",
            "resident_joined_rejected_chase_prefix_never_calculates",
        ),
    ),
}
PROGRESS = (
    "mbx[progress]: 4m elapsed; 0 hits, 7 misses, 1 bypassed, 0 not looked up; "
    "~0s compiler work saved\n"
)


def test_header(count):
    return f"running {count} test{'s' if count != 1 else ''}\n"


def fixture(names):
    count = len(names)
    return (
        "\n" + test_header(count)
        + "".join(f"test {name} ... ok\n" for name in names)
        + f"\nsuccesses:\n\n---- {names[0]} stdout ----\nfixture diagnostics\n"
        + "\nsuccesses:\n"
        + "".join(f"    {name}\n" for name in names)
        + f"\ntest result: ok. {count} passed; 0 failed; 0 ignored; "
        + "0 measured; 739 filtered out; finished in 15.18s\n\n"
    )


def workflow_gate(title, path=".github/workflows/ci.yml"):
    workflow = (ROOT / path).read_text()
    block = workflow.split(f"      - name: {title}\n", 1)[1]
    block = block.split("\n      - ", 1)[0]
    return textwrap.dedent(block.split("        run: |\n", 1)[1])


class ExactProofTests(unittest.TestCase):
    def variants(self):
        for title, (package, prefix, short_names) in SUITES.items():
            names = [prefix + name for name in short_names]
            yield title, package, prefix, names, fixture(names)

    def assert_rejected(self, transform):
        for title, _, _, names, output in self.variants():
            with self.subTest(suite=title):
                with self.assertRaises(ValueError):
                    VERIFIER.verify(transform(output, names), names)

    def test_resident_dispatch_manifest_matches_source_tests(self):
        _, _, names = SUITES["Verify resident dispatch receipt identity"]
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/resident_dispatch/tests.rs").read_text()
        actual = re.findall(r"(?m)^#\[test\]\s*\nfn (\w+)\(", source)
        self.assertEqual(set(names), set(actual))
        self.assertEqual(len(names), len(actual))

    def test_inventory_wrapper_manifest_matches_actual_nonheadless_tests(self):
        package, _, names = SUITES["Verify inventory freshness wrapper error order"]
        self.assertEqual(package, "benilla-native-corridor-tests")
        root = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/products/source_height"
        source = (root / "inventory_tests.rs").read_text()
        actual = re.findall(r"(?m)^#\[test\]\s*\nfn (\w+)\(", source)
        self.assertEqual(set(names), set(actual))
        self.assertEqual(len(names), len(actual))
        registration = (root / "inventory.rs").read_text()
        self.assertIn('#[cfg(all(test, not(benilla_headless_geometry)))]\n#[path = "inventory_tests.rs"]\nmod inventory_tests;', registration)

    def test_scoped_resident_manifest_matches_parent_and_nested_cold_tests(self):
        _, _, names = SUITES["Verify scoped resident source ownership"]
        root = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/resident_owned"
        parent = (root / "tests.rs").read_text()
        self.assertIn('#[path = "tests/cold_phased_negatives.rs"]\nmod cold_phased_negatives;', parent)
        pattern = r"(?m)^#\[test\]\s*\nfn (\w+)\("
        actual = set(re.findall(pattern, parent))
        cold = (root / "tests/cold_phased_negatives.rs").read_text()
        actual.update("cold_phased_negatives::" + name for name in re.findall(pattern, cold))
        self.assertIn('#[path = "tests/wrapper_equivalence.rs"]\nmod wrapper_equivalence;', parent)
        wrappers = (root / "tests/wrapper_equivalence.rs").read_text()
        actual.update("wrapper_equivalence::" + name for name in re.findall(pattern, wrappers))
        self.assertEqual(set(names), actual)
        self.assertEqual(len(names), len(actual))
        self.assertEqual(len(names), 15)

    def test_scoped_resident_gate_rejects_old_nine_test_evidence(self):
        _, prefix, short_names = SUITES["Verify scoped resident source ownership"]
        names = [prefix + name for name in short_names]
        old_names = [name for name in names if "::cold_phased_negatives::" not in name and "::wrapper_equivalence::" not in name and not name.endswith("::owned_retained_prefix_literal_oracle_preserves_two_source_effects") and not name.endswith("::owned_retained_observation_literal_oracle_preserves_final_source_effect") and not name.endswith("::owned_los_literal_oracle_preserves_source_and_invocation_operands")]
        self.assertEqual(len(old_names), 9)
        with self.assertRaises(ValueError):
            VERIFIER.verify(fixture(old_names), names)
        for cold_name in set(names) - set(old_names):
            with self.subTest(test=cold_name), self.assertRaises(ValueError):
                VERIFIER.verify(fixture(names).replace(f"test {cold_name} ... ok\n", ""), names)

    def test_retained_prefix_gate_rejects_legacy_twelve_results(self):
        _, prefix, short_names = SUITES["Verify scoped resident source ownership"]
        added = "owned_retained_prefix_literal_oracle_preserves_two_source_effects"
        names = [prefix + name for name in short_names]
        legacy = [name for name in names if name != prefix + added and name != prefix + "owned_retained_observation_literal_oracle_preserves_final_source_effect" and name != prefix + "owned_los_literal_oracle_preserves_source_and_invocation_operands"]
        self.assertEqual(len(legacy), 12)
        with self.assertRaises(ValueError):
            VERIFIER.verify(fixture(legacy), names)
        root = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter"
        self.assertIn(
            '#[cfg(all(test, not(benilla_headless_geometry)))]\n#[path = "retained.rs"]\npub(crate) mod retained;',
            (root / "resident.rs").read_text(),
        )
        self.assertIn(
            '#[path = "retained/prefix_equivalence.rs"]\npub(crate) mod prefix_equivalence;',
            (root / "retained.rs").read_text(),
        )

    def test_retained_observation_gate_rejects_legacy_thirteen_results(self):
        _, prefix, short_names = SUITES["Verify scoped resident source ownership"]
        added = "owned_retained_observation_literal_oracle_preserves_final_source_effect"
        names = [prefix + name for name in short_names]
        legacy = [name for name in names if name != prefix + added and name != prefix + "owned_los_literal_oracle_preserves_source_and_invocation_operands"]
        self.assertEqual(len(legacy), 13)
        with self.assertRaises(ValueError):
            VERIFIER.verify(fixture(legacy), names)
        root = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter"
        self.assertIn(
            '#[path = "retained/observation_equivalence.rs"]\npub(crate) mod observation_equivalence;',
            (root / "retained.rs").read_text(),
        )

    def test_los_gate_rejects_legacy_fourteen_results(self):
        _, prefix, short_names = SUITES["Verify scoped resident source ownership"]
        added = "owned_los_literal_oracle_preserves_source_and_invocation_operands"
        names = [prefix + name for name in short_names]
        legacy = [name for name in names if name != prefix + added]
        self.assertEqual(len(legacy), 14)
        with self.assertRaises(ValueError):
            VERIFIER.verify(fixture(legacy), names)
        source = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/source_route_filter/chase_los.rs"
        self.assertIn(
            '#[cfg(all(test, not(benilla_headless_geometry)))]\n#[path = "chase_los/equivalence.rs"]\npub(super) mod equivalence;',
            source.read_text(),
        )

    def test_accepts_exact_two_and_three_with_captured_diagnostics(self):
        for title, _, _, names, output in self.variants():
            with self.subTest(suite=title):
                VERIFIER.verify(output, names)
                VERIFIER.verify(output.replace("\n", "\r\n"), names)

    def test_rejects_missing_named_result_even_with_passing_summary(self):
        self.assert_rejected(lambda out, names: out.replace(f"test {names[0]} ... ok\n", ""))

    def test_rejects_wrong_named_result(self):
        self.assert_rejected(lambda out, names: out.replace(f"test {names[0]}", "test wrong"))

    def test_rejects_ignored_and_failed_named_results(self):
        for status in ("ignored", "FAILED"):
            with self.subTest(status=status):
                self.assert_rejected(lambda out, names: out.replace(" ... ok", f" ... {status}", 1))

    def test_rejects_duplicate_replacing_required_result(self):
        self.assert_rejected(
            lambda out, names: (
                out.replace(f"test {names[1]}", f"test {names[0]}")
                if len(names) > 1 else
                out.replace(f"test {names[0]} ... ok\n", f"test {names[0]} ... ok\n" * 2, 1)
            )
        )

    def test_rejects_extra_duplicate_or_unexpected_result(self):
        for row in ("test {name} ... ok\n", "test extra ... ok\n", "test extra ... FAILED\n"):
            with self.subTest(row=row):
                self.assert_rejected(
                    lambda out, names: out.replace("\nsuccesses:", "\n" + row.format(name=names[0]) + "successes:", 1)
                )

    def test_rejects_missing_wrong_and_duplicate_header(self):
        for replacement in ("", "running 0 tests\n", "wrong positive count"):
            with self.subTest(replacement=replacement):
                self.assert_rejected(
                    lambda out, names: out.replace(
                        test_header(len(names)),
                        test_header(len(names) + 1) if replacement == "wrong positive count" else replacement,
                    )
                )
        self.assert_rejected(lambda out, names: test_header(len(names)) + out)

    def test_rejects_missing_and_duplicate_summary(self):
        self.assert_rejected(lambda out, names: out[:out.index("test result:")])
        self.assert_rejected(lambda out, names: out + out[out.index("test result:"):])

    def test_rejects_wrong_summary_counts_or_status(self):
        for old, new in (
            ("test result: ok.", "test result: FAILED."),
            ("0 failed", "1 failed"),
            ("0 ignored", "1 ignored"),
            ("0 measured", "1 measured"),
            (" passed;", " passed; unexpected"),
        ):
            with self.subTest(field=old):
                self.assert_rejected(lambda out, names: out.replace(old, new))
        self.assert_rejected(
            lambda out, names: out.replace(f"{len(names)} passed", "0 passed")
        )

    def test_rejects_summary_only_and_captured_result_spoof(self):
        self.assert_rejected(lambda out, names: out[out.index("test result:"):])
        self.assert_rejected(
            lambda out, names: out.replace(f"test {names[0]} ... ok\n", "", 1).replace(
                "fixture diagnostics", f"test {names[0]} ... ok"
            )
        )

    def test_rejects_progress_interleaved_in_result(self):
        self.assert_rejected(lambda out, names: out.replace(" ... ok\n", f" ... {PROGRESS}ok\n", 1))

    def test_rejects_ambiguous_detached_ok_and_truncated_output(self):
        self.assert_rejected(lambda out, names: out.replace(" ... ok\n", " ... \nok\n", 1))
        self.assert_rejected(lambda out, names: out[:out.index(" ... ok") + 5])

    def test_rejects_multiple_runs_and_trailing_output(self):
        self.assert_rejected(lambda out, names: out + out)
        self.assert_rejected(lambda out, names: out + "test extra ... ok\n")

    def test_rejects_empty_or_duplicate_expected_names(self):
        for names in ([], ["duplicate", "duplicate"]):
            with self.subTest(names=names), self.assertRaises(ValueError):
                VERIFIER.verify("", names)

    def run_gate(self, title, package, prefix, output, exit_code=0, merge=False, tee_failure=False):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            mbx = temp / "mbx"
            mbx.write_text(
                "#!/usr/bin/env python3\n"
                "import os, sys\n"
                "expected = ['test', '--locked', '-p', os.environ['EXPECTED_PACKAGE'], "
                "'--lib', os.environ['EXPECTED_PREFIX'], '--', '--show-output', "
                "'--test-threads=1', '--color', 'never']\n"
                "if os.environ['EXPECTED_EXACT'] == '1': expected.insert(expected.index('--') + 1, '--exact')\n"
                "for skip in reversed(os.environ['EXPECTED_SKIP'].split()): expected[expected.index('--') + 1:expected.index('--') + 1] = ['--skip', skip]\n"
                "assert sys.argv[1:] == expected, sys.argv[1:]\n"
                "data = os.environ['FAKE_OUTPUT'].encode()\n"
                "cut = data.find(b' ... ') + len(b' ... ')\n"
                "os.write(1, data[:cut])\n"
                "os.write(2, os.environ['FAKE_PROGRESS'].encode())\n"
                "os.write(1, data[cut:])\n"
                "sys.exit(int(os.environ['FAKE_EXIT']))\n"
            )
            mbx.chmod(0o755)
            if tee_failure:
                tee = temp / "tee"
                tee.write_text("#!/usr/bin/env bash\n/usr/bin/tee \"$@\"\nexit 31\n")
                tee.chmod(0o755)
            env = dict(
                os.environ,
                PATH=f"{temp}:{os.environ['PATH']}",
                RUNNER_TEMP=str(temp),
                EXPECTED_PACKAGE=package,
                EXPECTED_PREFIX=prefix,
                EXPECTED_EXACT="1" if title in ("Verify resident observation without transport admission", "Verify two-turn retained controller transport", "Verify two-turn cold controller transport") else "0",
                EXPECTED_SKIP=" ".join((DEFAULT_BUDGET_PROBE, *F05_CONTROLLER_TESTS)) if title == "Verify resident result controller transport" else "",
                FAKE_OUTPUT=output,
                FAKE_PROGRESS=PROGRESS,
                FAKE_EXIT=str(exit_code),
            )
            shell = workflow_gate(title)
            if merge:
                shell = shell.replace(' | tee "$log"', ' 2>&1 | tee "$log"')
            result = subprocess.run(
                ["bash", "-c", shell], cwd=ROOT, env=env, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10,
            )
            logs = list(temp.glob("*.log"))
            self.assertEqual(len(logs), 1)
            return result, logs[0].read_text()

    def test_real_shell_gates_separate_stderr_and_pass_exact_stdout(self):
        for title, package, prefix, _, output in self.variants():
            with self.subTest(suite=title):
                result, log = self.run_gate(title, package, prefix, output)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(log, output)
                self.assertIn(PROGRESS, result.stderr)
                self.assertNotIn("mbx[progress]", log)

    def test_original_merged_shell_gate_regression_is_rejected(self):
        for title, package, prefix, _, output in self.variants():
            with self.subTest(suite=title):
                result, log = self.run_gate(title, package, prefix, output, merge=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f" ... {PROGRESS}ok\n", log)

    def test_real_shell_gates_reject_bad_results_despite_good_summary(self):
        for title, package, prefix, names, output in self.variants():
            for status in ("ignored", "FAILED", "ok\n" + f"test {names[0]} ... ok"):
                with self.subTest(suite=title, status=status):
                    bad = output.replace(" ... ok", f" ... {status}", 1)
                    result, _ = self.run_gate(title, package, prefix, bad)
                    self.assertNotEqual(result.returncode, 0)

    def test_real_shell_gates_preserve_command_failure_with_valid_output(self):
        for title, package, prefix, _, output in self.variants():
            with self.subTest(suite=title):
                result, log = self.run_gate(title, package, prefix, output, exit_code=17)
                self.assertEqual(result.returncode, 17)
                self.assertEqual(log, output)

    def test_real_shell_gates_preserve_tee_failure_with_valid_output(self):
        for title, package, prefix, _, output in self.variants():
            with self.subTest(suite=title):
                result, log = self.run_gate(title, package, prefix, output, tee_failure=True)
                self.assertEqual(result.returncode, 31)
                self.assertEqual(log, output)

    def test_missing_log_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as temp:
            result = subprocess.run(
                ["python3", str(ROOT / "scripts/verify-libtest-output.py"), f"{temp}/missing", "test"],
                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("libtest verification failed", result.stderr)

    def test_fixture_gate_is_wired_before_compilation(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        command = "run: python3 scripts/test-verify-libtest-output.py"
        fixture_step = "- name: Test exact libtest proof gates\n        " + command

        def assert_wired(job):
            steps = re.split(r"(?=^      - )", job, flags=re.MULTILINE)
            self.assertEqual(job.count(command), 1)
            # Exact step shape also rejects conditional or continue-on-error gates.
            gates = [i for i, step in enumerate(steps) if step.strip() == fixture_step]
            self.assertEqual(len(gates), 1)
            compilation = [
                i for i, step in enumerate(steps)
                if re.search(r"\b(?:mbx|cargo)\s+(?:build|check|clippy|test|rustc|doc)\b", step)
            ]
            self.assertTrue(compilation)
            self.assertLess(gates[0], min(compilation))

        for name in ("workspace-gates", "workspace-tests", "player-gates"):
            with self.subTest(job=name):
                jobs = re.findall(
                    rf"^  {re.escape(name)}:\n(.*?)(?=^  \S|\Z)",
                    workflow, flags=re.MULTILINE | re.DOTALL,
                )
                self.assertEqual(len(jobs), 1)
                job = jobs[0]
                assert_wired(job)
                gate = "      " + fixture_step + "\n"
                # Mutation checks prevent accepting two gates in one job while
                # the other job has none, or a gate after compilation.
                for bad in (
                    job.replace(gate, ""),
                    job.replace(gate, gate + gate),
                    job.replace(gate, "") + gate,
                    job.replace(gate, gate + "        if: false\n"),
                    job.replace(gate, gate + "        continue-on-error: true\n"),
                ):
                    with self.assertRaises(AssertionError):
                        assert_wired(bad)


    def test_native_resident_proofs_require_navigation_helper_first(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        job = workflow.split("  workspace-gates:\n", 1)[1].split("\n  workspace-tests:", 1)[0]
        helper = "Verify navigation helper prerequisite"
        consumers = (
            "Verify resident FIFO selection",
            "Verify resident dispatch receipt identity",
            "Verify scoped resident source ownership",
            "Verify resident result controller transport",
        )

        def assert_order(candidate):
            steps = re.split(r"(?=^      - )", candidate, flags=re.MULTILINE)
            indexes = {}
            for title in (helper, *consumers):
                matching = [i for i, step in enumerate(steps) if step.startswith(f"      - name: {title}\n")]
                self.assertEqual(len(matching), 1)
                index = matching[0]
                self.assertNotRegex(steps[index], r"(?m)^        (?:if|continue-on-error):")
                indexes[title] = index
            self.assertIn(
                "mbx test --locked -p benilla-native-nav-generator --test process empty_request_runs_without_inherited_environment -- --exact",
                steps[indexes[helper]],
            )
            for title in consumers:
                self.assertLess(indexes[helper], indexes[title])

        assert_order(job)
        steps = re.split(r"(?=^      - )", job, flags=re.MULTILINE)
        helper_step = next(step for step in steps if step.startswith(f"      - name: {helper}\n"))
        for title in consumers:
            consumer = next(step for step in steps if step.startswith(f"      - name: {title}\n"))
            with self.subTest(consumer=title):
                reordered = job.replace(consumer, "").replace(helper_step, consumer + helper_step)
                with self.assertRaises(AssertionError):
                    assert_order(reordered)
        for bad in (
            job.replace(helper_step, ""),
            job.replace(helper_step, helper_step + helper_step),
            job.replace(helper_step, helper_step.replace("        shell: bash\n", "        if: false\n        shell: bash\n")),
            job.replace(helper_step, helper_step.replace("        shell: bash\n", "        continue-on-error: true\n        shell: bash\n")),
        ):
            with self.assertRaises(AssertionError):
                assert_order(bad)

    def test_comparison_format_preflight_follows_setup_and_precedes_compilation(self):
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        workflow = (ROOT / path).read_text()
        title = "Check formatting and module resolution"
        preflight = f"      - name: {title}\n        run: cargo fmt --all -- --check\n"
        self.assertIn("run: cargo fmt --all -- --check", (ROOT / ".github/workflows/ci.yml").read_text())

        def assert_order(candidate):
            steps = re.split(r"(?=^      - )", candidate, flags=re.MULTILINE)
            indexes = {}
            for name in ("Toolchain", "Set up Mr Boxington", title):
                matching = [i for i, step in enumerate(steps) if step.startswith(f"      - name: {name}\n")]
                self.assertEqual(len(matching), 1)
                indexes[name] = matching[0]
            self.assertEqual(steps[indexes[title]], preflight)
            self.assertLess(indexes["Toolchain"], indexes[title])
            self.assertLess(indexes[title], indexes["Set up Mr Boxington"])
            compilation = [
                i for i, step in enumerate(steps)
                if re.search(r"\b(?:mbx|cargo)\s+(?:build|check|clippy|test|rustc|doc)\b", step)
            ]
            self.assertTrue(compilation)
            self.assertLess(indexes[title], min(compilation))

        assert_order(workflow)
        steps = re.split(r"(?=^      - )", workflow, flags=re.MULTILINE)
        toolchain = next(step for step in steps if step.startswith("      - name: Toolchain\n"))
        for bad in (
            workflow.replace(preflight, ""),
            workflow.replace(preflight, preflight + preflight),
            workflow.replace(preflight, "").replace(toolchain, preflight + toolchain),
            workflow.replace(preflight, "") + preflight,
            workflow.replace(preflight, preflight.replace("        run:", "        if: false\n        run:")),
            workflow.replace(preflight, preflight.replace("        run:", "        continue-on-error: true\n        run:")),
            workflow.replace(preflight, preflight.replace("cargo fmt --all -- --check", "cargo fmt --all")),
        ):
            with self.assertRaises(AssertionError):
                assert_order(bad)

    def test_controller_profile_comparison_is_scoped_and_profile_only(self):
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        workflow = (ROOT / path).read_text()
        expected_trigger = (
            "  push:\n"
            "    branches: [feat/psk-p2p-foundation]\n"
            "    paths:\n"
            + "".join(f"      - {path}\n" for path in COMPARISON_PATHS)
        )

        def assert_trigger(candidate):
            trigger = re.search(r"(?ms)^on:\n(.*?)(?=^\S)", candidate)
            self.assertIsNotNone(trigger)
            self.assertEqual(trigger.group(1).rstrip() + "\n", expected_trigger)
            self.assertEqual(len(COMPARISON_PATHS), len(set(COMPARISON_PATHS)))
            self.assertNotRegex(expected_trigger, r"[*?]")

        assert_trigger(workflow)
        for bad in (
            workflow.replace("branches: [feat/psk-p2p-foundation]", "branches: [feat/psk-p2p-foundation, main]"),
            workflow.replace("    paths:\n", "    paths:\n      - '**'\n"),
            workflow.replace(f"      - {path}\n", ""),
            workflow.replace(f"      - {COMPARISON_PATHS[1]}\n", ""),
            workflow.replace(f"      - {COMPARISON_PATHS[1]}\n", f"      - {COMPARISON_PATHS[1]}\n" * 2),
        ):
            with self.assertRaises(AssertionError):
                assert_trigger(bad)
        self.assertNotIn("workflow_dispatch:", workflow)
        self.assertNotIn("pull_request:", workflow)
        self.assertIn("  group: resident-controller-profile-comparison-${{ github.ref }}\n", workflow)
        self.assertIn("          cache-generation: resident-controller-profile-comparison-ubuntu26-v1\n", workflow)
        normal = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertNotIn("compare-controller-profiles:", normal)
        self.assertNotIn("compare_controller_profiles:", normal)
        job = workflow.split("  compare-controller-profiles:\n", 1)[1]
        self.assertNotIn("    if:", job.split("    steps:", 1)[0])
        self.assertIn("    timeout-minutes: 150\n", job)
        self.assertNotIn("continue-on-error:", job)
        shell = workflow_gate("Compare the same five controller fixtures serially", path)
        self.assertIn("set -euo pipefail", shell)
        self.assertIn("for profile in test release; do", shell)
        self.assertIn('args=()\n', shell)
        self.assertIn('if [[ "$profile" == release ]]; then\n    args+=(--release)\n  fi', shell)
        commands = [line for line in shell.splitlines() if line.lstrip().startswith("mbx test ")]
        self.assertEqual(len(commands), 2)
        self.assertTrue(all('mbx test --locked "${args[@]}" -p ' in line for line in commands))
        self.assertTrue(all("--show-output --test-threads=1 --color never" in line for line in commands))
        self.assertIn("-p benilla-native-nav-generator --test process empty_request_runs_without_inherited_environment -- --exact", commands[0])
        package, prefix, names = SUITES["Verify resident result controller transport"]
        self.assertIn(f"-p {package} --lib {prefix} --", commands[1])
        self.assertEqual(
            re.findall(r"net::p2p::native_geometry::resident_result_controller_tests::\w+", shell),
            [prefix + name for name in names],
        )
        self.assertEqual(shell.count('python3 scripts/verify-libtest-output.py "$log"'), 2)
        self.assertNotIn("--no-default-features", shell)
        self.assertNotIn("--nocapture", shell)

    def test_controller_proof_selectors_exclude_only_default_budget_probe(self):
        _, prefix, names = SUITES["Verify resident result controller transport"]
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/resident_result_controller_tests.rs").read_text()
        actual = re.findall(r"(?m)^fn (resident_\w+)\(\)", source)
        self.assertEqual(set(actual), {*names, DEFAULT_BUDGET_PROBE, *F05_CONTROLLER_TESTS})

        def assert_selection(command):
            args = shlex.split(command)
            skips = [args[i + 1] for i, arg in enumerate(args) if arg == "--skip"]
            self.assertEqual(skips, [DEFAULT_BUDGET_PROBE, *F05_CONTROLLER_TESTS])
            self.assertIn(prefix, args)
            selected = [prefix + name for name in actual if not any(skip in prefix + name for skip in skips)]
            self.assertEqual(set(selected), {prefix + name for name in names})

        for title, path in (
            ("Verify resident result controller transport", ".github/workflows/ci.yml"),
            ("Compare the same five controller fixtures serially", ".github/workflows/resident-controller-profile-comparison.yml"),
        ):
            with self.subTest(workflow=path):
                shell = workflow_gate(title, path)
                commands = [line for line in shell.splitlines() if "mbx test " in line and prefix in line]
                self.assertEqual(len(commands), 1)
                command = commands[0]
                assert_selection(command)
                for bad in (
                    command.replace(f"--skip {DEFAULT_BUDGET_PROBE} ", ""),
                    command.replace(f"--skip {DEFAULT_BUDGET_PROBE}", "--skip resident_"),
                ):
                    with self.assertRaises(AssertionError):
                        assert_selection(bad)

        # The observed six-result output must remain invalid, not be tolerated
        # by weakening the five-proof verifier to allow ignored results.
        expected = [prefix + name for name in names]
        output = fixture(expected).replace("running 5 tests", "running 6 tests")
        output = output.replace("\nsuccesses:", f"\ntest {prefix}{DEFAULT_BUDGET_PROBE} ... ignored\nsuccesses:", 1)
        output = output.replace("0 ignored", "1 ignored")
        with self.assertRaises(ValueError):
            VERIFIER.verify(output, expected)

    def test_collision_observer_controls_require_explicit_feature_and_exact_tests(self):
        title = "Verify scoped runtime collision observer controls"
        shell = workflow_gate(title)
        names = [
            "profiling::tests::runtime_scopes_are_bounded_nested_unwind_safe_and_thread_local",
            "source::tests::profiling_source_tests::runtime_diagnostics_preserve_reads_receipts_and_every_cancellation_checkpoint",
        ]
        self.assertIn("set -euo pipefail", shell)
        self.assertIn("status=0", shell)
        self.assertIn('exit "$status"', shell)
        self.assertEqual(re.findall(r"(?m)^  ([a-z_]+::[a-z_:]+)$", shell), names)
        self.assertIn(
            'if ! mbx test --locked -p benilla-native-vmap-source --features synthetic-map-tests --lib "$name" -- --exact --show-output --test-threads=1 --color never | tee "$log"; then status=1; fi',
            shell,
        )
        self.assertIn(
            'if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi',
            shell,
        )
        self.assertNotIn("--skip", shell)
        self.assertNotIn("continue-on-error", shell)
        for name, path in zip(names, ["profiling.rs", "profiling_source_tests.rs"]):
            source = (ROOT / "crates/benilla-native-vmap-source/src" / path).read_text()
            self.assertIn("fn " + name.rsplit("::", 1)[1] + "()", source)

    def test_hash_hex_candidate_is_bounded_test_only_and_rehashes_both_full_paths(self):
        directory = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry"
        source = (directory / "hash_hex_primitive_tests.rs").read_text()
        module = (directory / "mod.rs").read_text()
        self.assertIn('#[cfg(all(test, not(benilla_headless_geometry)))]\nmod hash_hex_primitive_tests;', module)
        self.assertEqual(re.findall(r"(?m)^fn (\w+)\(\) \{", source), ["hash_hex_encoding_matches_original", "fixture_hash_hex_primitive_comparison"])
        self.assertIn('format!("{:x}", Sha256::digest(payload))', source)
        self.assertIn('fixed_hex(&Sha256::digest(payload))', source)
        self.assertIn('drop(black_box(encode(black_box(payload.as_slice()))))', source)
        self.assertIn('drop(black_box(encode(black_box(digest))))', source)
        self.assertIn('[[false, true], [true, false]].into_iter().enumerate()', source)
        self.assertIn('const MAX_FULL_ROUNDS: usize = 128;', source)
        self.assertIn('const MAX_ENCODING_ROUNDS: usize = 4096;', source)
        self.assertIn('const MAX_BYTES_PER_SAMPLE: usize = 64 * 1024 * 1024;', source)
        self.assertIn('assert_eq!(source_counts.payload_hash.started, lengths.len())', source)
        self.assertIn('assert_eq!(source_counts.payload_hash.succeeded, lengths.len())', source)
        self.assertIn('histogram.values().sum::<usize>()', source)
        self.assertIn('deterministic length-matched synthetic bytes, not captured payloads', source)
        self.assertNotIn('unsafe', source)
        original = (ROOT / "crates/benilla-native-vmap-source/src/source.rs").read_text()
        self.assertIn('sha256: profile_value!(PayloadHash, format!("{:x}", Sha256::digest(&payload)))', original)
        self.assertNotIn('fixed_hex', original)

    def test_hash_hex_primitive_gate_is_exact_and_preserves_both_failures(self):
        path = ".github/workflows/resident-hash-hex-primitive.yml"
        shell = workflow_gate("Verify encoder equivalence and measure the bounded primitive", path)
        names = ["hash_hex_encoding_matches_original", "fixture_hash_hex_primitive_comparison"]
        prefix = "net::p2p::native_geometry::hash_hex_primitive_tests::"
        workflow = (ROOT / path).read_text()
        trigger_paths = [line.strip()[2:] for line in workflow.split("    paths:\n", 1)[1].split("\n\n", 1)[0].splitlines()]
        self.assertEqual(trigger_paths, [path,
            "crates/benilla-app/src/net/p2p/world/native_geometry/hash_hex_primitive_tests.rs",
            "crates/benilla-app/src/net/p2p/world/native_geometry/mod.rs",
            "crates/benilla-native-vmap-source/src/source.rs",
            "crates/benilla-native-vmap-source/src/map_bundle.rs",
        ])
        self.assertIn("      - name: Verify encoder equivalence and measure the bounded primitive\n        id: primitive\n", workflow)
        upload = workflow.split("      - name: Preserve primitive logs\n", 1)[1]
        self.assertIn("if: ${{ always() && steps.primitive.outcome != 'skipped' }}", upload)
        self.assertIn("if-no-files-found: error", upload)
        self.assertNotIn("if-no-files-found: ignore", upload)
        self.assertIn("for test in " + " ".join(names) + "; do", shell)
        self.assertIn('if [[ "$test" == fixture_hash_hex_primitive_comparison ]]; then args+=(--ignored); fi', shell)
        self.assertIn('if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi', shell)
        self.assertIn('exit "$status"', shell)
        self.assertNotIn('--skip', shell)
        self.assertNotIn('--nocapture', shell)
        self.assertNotIn('continue-on-error', workflow)
        for bad, code, wrong in (("", 0, False), (names[0], 1, False), (names[0], 0, True), (names[1], 1, True)):
            with self.subTest(bad=bad, code=code, wrong=wrong), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                mbx = root / "mbx"
                mbx.write_text("#!" + sys.executable + "\n" + textwrap.dedent(r"""
                    import json, os, sys
                    from pathlib import Path
                    name = sys.argv[sys.argv.index('--lib') + 1]
                    short = name.rsplit('::', 1)[-1]
                    assert all(x in sys.argv for x in ['test', '--locked', '--release', '--exact', '--show-output', '--test-threads=1'])
                    assert ('--ignored' in sys.argv) == (short == 'fixture_hash_hex_primitive_comparison')
                    with Path(os.environ['CALLS']).open('a') as out: out.write(short + '\n')
                    selected = short == os.environ['BAD']
                    print(json.loads(os.environ['OUTPUTS'])[short if not selected or os.environ['WRONG'] == '0' else 'wrong'], end='')
                    sys.exit(int(os.environ['EXIT']) if selected else 0)
                """))
                mbx.chmod(0o755)
                outputs = {name: fixture([prefix + name]) for name in names}
                outputs['wrong'] = fixture(['unexpected'])
                calls = root / 'calls'
                env = dict(os.environ, RUNNER_TEMP=str(root), PATH=str(root) + os.pathsep + os.environ['PATH'], CALLS=str(calls), BAD=bad, EXIT=str(code), WRONG=str(int(wrong)), OUTPUTS=json.dumps(outputs))
                result = subprocess.run(['bash', '-c', shell], cwd=ROOT, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, not bad, result.stdout + result.stderr)
                self.assertEqual(calls.read_text().splitlines(), names)

    def test_revision_comparison_pins_inputs_and_builds_before_execution(self):
        path = ".github/workflows/resident-instrumentation-revision-comparison.yml"
        workflow = (ROOT / path).read_text()
        self.assertIn("0d522190eca69f2c56d582d874a3c19d7df8d435", workflow)
        self.assertIn("10fff5126b5edaaed3330cb7374099cae95e971f", workflow)
        self.assertIn("c506058280ea97f266a07932909556da4243e467", workflow)
        self.assertIn("f725aad16265f70657e83784438f1f24a3311a19", workflow)
        self.assertIn('assert inventories[0] == inventories[1]', workflow)
        self.assertIn('assert git("rev-parse", "HEAD") == head', workflow)
        self.assertIn('assert git("rev-parse", "HEAD^{tree}") == tree', workflow)
        self.assertEqual(workflow.split("    paths:\n", 1)[1].split("\n\n", 1)[0].strip(), "- " + path)
        build = workflow_gate("Build both revisions before running either comparison", path)
        run = workflow_gate("Run each pinned artifact once and verify both raw logs", path)
        self.assertLess(workflow.index("Build both revisions before running either comparison"), workflow.index("Run each pinned artifact once and verify both raw logs"))
        self.assertIn('"--locked", "--release", "--no-run"', build)
        self.assertIn('"--message-format=json"', build)
        self.assertIn('and m.get("executable") and m["profile"]["test"]', build)
        self.assertIn('and m["target"]["name"] == target_name', build)
        self.assertIn('and target_kind in m["target"]["kind"]', build)
        self.assertIn('len(artifacts) != 1', build)
        self.assertIn('f"resident-target-{label}"', build)
        self.assertNotIn('run_exact(', build)
        self.assertNotIn('"--no-run"', run)
        self.assertNotIn('continue-on-error', workflow)
        self.assertNotIn('--skip', workflow)
        self.assertIn('sys.exit(status)', run)
        self.assertIn('Fixed baseline-then-current order', workflow)
        self.assertIn('not source-hook causation', workflow)

    def test_revision_comparison_build_discovery_rejects_ambiguous_or_invalid_artifacts(self):
        path = ".github/workflows/resident-instrumentation-revision-comparison.yml"
        shell = workflow_gate("Build both revisions before running either comparison", path)
        for fault in ("none", "duplicate", "not-test", "wrong-kind", "malformed"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                tools = root / "tools"
                tools.mkdir()
                for label in ("baseline", "current"):
                    (root / label).mkdir()
                cargo = tools / "cargo"
                cargo.write_text("#!" + sys.executable + "\n" + textwrap.dedent(r"""
                    import json, os, sys
                    from pathlib import Path
                    assert all(flag in sys.argv for flag in ['+1.98.1', 'test', '--locked', '--release', '--no-run', '--message-format=json'])
                    package = sys.argv[sys.argv.index('-p') + 1]
                    app = package == 'benilla-app'
                    label = Path.cwd().name
                    with Path(os.environ['CALLS']).open('a') as out:
                        out.write(label + ':' + ('app' if app else 'helper') + '\n')
                    target = Path(os.environ['CARGO_TARGET_DIR']) / 'release/deps'
                    target.mkdir(parents=True, exist_ok=True)
                    executable = target / ('app' if app else 'helper')
                    executable.touch()
                    artifact = {'reason': 'compiler-artifact', 'executable': str(executable), 'profile': {'test': True}, 'target': {'name': 'benilla_app' if app else 'process', 'kind': ['lib' if app else 'test']}}
                    fault = os.environ['FAULT'] if label == 'baseline' and app else 'none'
                    if fault == 'not-test': artifact['profile']['test'] = False
                    if fault == 'wrong-kind': artifact['target']['kind'] = ['bin']
                    if fault == 'malformed': print('not JSON')
                    else: print(json.dumps(artifact))
                    if fault == 'duplicate': print(json.dumps(artifact))
                """))
                cargo.chmod(0o755)
                calls = root / "calls"
                env = dict(os.environ, GITHUB_WORKSPACE=str(root), RUNNER_TEMP=str(root), CALLS=str(calls), FAULT=fault, PATH=str(tools) + os.pathsep + os.environ["PATH"])
                result = subprocess.run(["bash", "-c", shell], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(calls.read_text().splitlines(), ["baseline:helper", "baseline:app", "current:helper", "current:app"])
                records = json.loads((root / "resident-revision-comparison/build-records.json").read_text())
                self.assertEqual([record["build_ok"] for record in records], [fault == "none", True])
                self.assertNotEqual(records[0]["target"], records[1]["target"])

    def test_revision_comparison_runs_both_once_and_retains_process_or_proof_failure(self):
        path = ".github/workflows/resident-instrumentation-revision-comparison.yml"
        shell = workflow_gate("Run each pinned artifact once and verify both raw logs", path)
        name = "net::p2p::native_geometry::retained_phased_controller_tests::cold_phased_frame_cadenced_comparison"
        helper = "empty_request_runs_without_inherited_environment"
        cases = ((None, 0, False), ("baseline", 1, False), ("baseline", 0, True), ("current", 1, True), ("baseline", None, False))
        for bad_label, exit_code, bad_proof in cases:
            with self.subTest(label=bad_label, exit=exit_code, bad_proof=bad_proof), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                logs = root / "resident-revision-comparison"
                logs.mkdir()
                tools = root / "tools"
                tools.mkdir()
                for tool in ("rustc", "rustup"):
                    executable = tools / tool
                    executable.write_text("#!/bin/sh\necho /unused/runtime/path\n")
                    executable.chmod(0o755)
                (root / "scripts").mkdir()
                (root / "scripts/verify-libtest-output.py").write_bytes((ROOT / "scripts/verify-libtest-output.py").read_bytes())
                records = []
                for label in ("baseline", "current"):
                    checkout = root / label
                    checkout.mkdir()
                    (checkout / "Cargo.toml").write_text('[workspace.package]\nversion="0.2.0"\n')
                    target = root / ("resident-target-" + label)
                    (target / "release/deps").mkdir(parents=True)
                    record = {"label": label, "root": str(checkout), "target": str(target), "build_ok": True}
                    for kind, package, selected in (("helper", "benilla-native-nav-generator", helper), ("app", "benilla-app", name)):
                        manifest = checkout / "crates" / package
                        manifest.mkdir(parents=True)
                        (manifest / "Cargo.toml").write_text('[package]\nname="' + package + '"\nversion="0.2.0"\n')
                        executable = target / "release/deps" / kind
                        bad = label == bad_label and kind == "app"
                        output = fixture(["unexpected" if bad and bad_proof else selected])
                        executable.write_text(
                            "#!" + sys.executable + "\nimport os,sys\nfrom pathlib import Path\n"
                            + "assert sys.argv[1] == " + repr(selected) + "\n"
                            + "assert '--exact' in sys.argv and '--show-output' in sys.argv\n"
                            + "assert ('--ignored' in sys.argv) == " + repr(kind == "app") + "\n"
                            + "with Path(os.environ['CALLS']).open('a') as f: f.write(" + repr(label + ":" + kind + "\n") + ")\n"
                            + "print(" + repr(output) + ", end='')\n"
                            + "sys.exit(" + str(exit_code if bad else 0) + ")\n"
                        )
                        executable.chmod(0o755)
                        if bad and exit_code is None:
                            executable.unlink()
                        record[kind] = {"executable": str(executable), "library_paths": []}
                    records.append(record)
                (logs / "build-records.json").write_text(json.dumps(records))
                calls = root / "calls"
                env = dict(os.environ, GITHUB_WORKSPACE=str(root), RUNNER_TEMP=str(root), CALLS=str(calls), PATH=str(tools) + os.pathsep + os.environ["PATH"])
                result = subprocess.run(["bash", "-c", shell], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, bad_label is None, result.stdout + result.stderr)
                expected = ["baseline:helper", "current:helper", "baseline:app", "current:app"]
                if exit_code is None:
                    expected.remove("baseline:app")
                self.assertEqual(calls.read_text().splitlines(), expected)
                for label in ("baseline", "current"):
                    self.assertIn("RESULT " + label + "/app:", result.stdout)
                    self.assertTrue((logs / (label + "-app.stdout.log")).exists())

    def test_observer_comparison_keeps_exact_scope_and_original_budget(self):
        title = "Measure resident observer overhead without changing the Work budget"
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        workflow = (ROOT / path).read_text()
        step = workflow.split(f"      - name: {title}\n", 1)[1].split("\n      - ", 1)[0]
        self.assertIn("if: ${{ !cancelled() && steps.serial_comparison.outcome == 'success' }}", step)
        self.assertNotIn("continue-on-error", step)
        shell = workflow_gate(title, path)
        self.assertIn("set -euo pipefail", shell)
        self.assertIn("status=0", shell)
        self.assertIn("exit \"$status\"", shell)
        self.assertIn(
            "name=net::p2p::native_geometry::retained_phased_controller_tests::resident_observer_off_count_clocked_comparison",
            shell,
        )
        self.assertIn(
            'if ! mbx test --locked --release -p benilla-app --lib "$name" -- --exact --ignored --show-output --test-threads=1 --color never | tee "$log"; then status=1; fi',
            shell,
        )
        self.assertIn(
            'if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi',
            shell,
        )
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/retained_phased_controller_tests.rs").read_text()
        observer = source.split("fn resident_observer_off_count_clocked_comparison() {", 1)[1]
        self.assertIn("for mode in [Off, CountOnly, Clocked]", observer)
        self.assertIn("std::panic::catch_unwind(std::panic::AssertUnwindSafe(||", observer)
        self.assertIn("run_profiled(mode, &mut phases_on_failure)", observer)
        self.assertIn("if let Ok(evidence) = &outcome", observer)
        self.assertIn("resident observer failed records", observer)
        self.assertIn("outcomes.iter().all(Result::is_ok)", observer)
        self.assertIn("assert_eq!(off.cold_trace, clocked.cold_trace)", observer)
        self.assertIn("assert_eq!(off.retained_trace, clocked.retained_trace)", observer)
        runner = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/resident_result_controller_tests.rs").read_text()
        self.assertRegex(
            runner,
            r"run_inner\(\s*Mode::ColdPhasedFrameCadenced,\s*Some\(profile\),\s*phases_on_failure,\s*false,?\s*\)",
        )
        self.assertIn("freshness=deferred", runner)
        self.assertIn("if profiling.is_some()", runner)
        self.assertIn("freshness = freshness.legacy_only()", runner)
        self.assertNotIn("--skip", shell)
        self.assertNotIn("--nocapture", shell)

    def test_phased_comparison_runs_exact_ignored_tests_and_accumulates_failure(self):
        shell = workflow_gate(
            "Compare two-turn retained certification with synchronous baseline",
            ".github/workflows/resident-controller-profile-comparison.yml",
        )
        self.assertIn("set -euo pipefail", shell)
        self.assertIn("status=0", shell)
        self.assertIn(
            "for test in retained_phased_default_budget_comparison retained_phased_frame_cadenced_comparison; do",
            shell,
        )
        self.assertIn(
            'name="net::p2p::native_geometry::retained_phased_controller_tests::$test"', shell,
        )
        self.assertIn(
            'if ! mbx test --locked --release -p benilla-app --lib "$name" -- --exact --ignored --show-output --test-threads=1 --color never | tee "$log"; then status=1; fi',
            shell,
        )
        self.assertIn(
            'if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi',
            shell,
        )
        self.assertIn('exit "$status"', shell)
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/retained_phased_controller_tests.rs").read_text()
        self.assertEqual(source.count('std::panic::catch_unwind(|| run('), 4)
        self.assertIn('phased.max_owner_service < original.max_owner_service', source)
        self.assertIn('assert_eq!(original.retained_trace.len(), 87)', source)
        self.assertIn('assert_eq!(phased.retained_trace.len(), 93)', source)

    def test_cold_comparison_preserves_retained_baseline_and_exact_selectors(self):
        shell = workflow_gate(
            "Compare cold split with retained-only split baseline",
            ".github/workflows/resident-controller-profile-comparison.yml",
        )
        self.assertIn("set -euo pipefail", shell)
        self.assertIn("status=0", shell)
        self.assertIn("for test in cold_phased_default_budget_comparison cold_phased_frame_cadenced_comparison; do", shell)
        self.assertIn('name="net::p2p::native_geometry::retained_phased_controller_tests::$test"', shell)
        self.assertIn('if ! mbx test --locked --release -p benilla-app --lib "$name" -- --exact --ignored --show-output --test-threads=1 --color never | tee "$log"; then status=1; fi', shell)
        self.assertIn('if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi', shell)
        self.assertIn('exit "$status"', shell)
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/retained_phased_controller_tests.rs").read_text()
        cold = source[source.index("fn compare_cold("):]
        self.assertIn('compare_cold(Mode::PhasedDefaultBudget, Mode::ColdPhasedDefaultBudget)', source)
        self.assertIn('compare_cold(Mode::PhasedFrameCadenced, Mode::ColdPhasedFrameCadenced)', source)
        self.assertIn('assert_eq!(original.cold_trace.len(), 71)', cold)
        self.assertIn('assert_eq!(original.retained_trace, phased.retained_trace)', cold)
        self.assertIn('phased.max_cold_owner_service < original.max_cold_owner_service', cold)
        self.assertNotIn('phased.max_owner_service < original.max_owner_service', cold)

    def test_default_budget_probe_is_explicit_release_and_exact_single_result(self):
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        shell = workflow_gate("Validate the default Route budget in the release component fixture", path)
        _, prefix, _ = SUITES["Verify resident result controller transport"]
        name = prefix + DEFAULT_BUDGET_PROBE

        def assert_gate(candidate):
            lines = [line.strip() for line in candidate.splitlines() if line.strip()]
            self.assertIn("set -euo pipefail", lines)
            self.assertEqual([line for line in lines if line.startswith("name=")], [f"name={name}"])
            commands = [shlex.split(line) for line in lines if line.startswith("mbx ")]
            self.assertEqual(commands, [[
                "mbx", "test", "--locked", "--release", "-p", "benilla-app", "--lib", "$name",
                "--", "--exact", "--ignored", "--show-output", "--test-threads=1", "--color", "never",
                "|", "tee", "$log",
            ]])
            verifiers = [shlex.split(line) for line in lines if "verify-libtest-output.py" in line]
            self.assertEqual(verifiers, [["python3", "scripts/verify-libtest-output.py", "$log", "$name"]])

        assert_gate(shell)
        for bad in (
            shell.replace("--release ", ""),
            shell.replace("--exact ", ""),
            shell.replace("--ignored ", ""),
            shell.replace(f"name={name}", f"name={prefix}"),
            shell.replace('"$log" "$name"', '"$log" "$name" another_test'),
            shell.replace('"$log" "$name"', '"$log" "$name" || true'),
        ):
            with self.assertRaises(AssertionError):
                assert_gate(bad)
        VERIFIER.verify(fixture([name]), [name])
        with self.assertRaises(ValueError):
            VERIFIER.verify(fixture([name]).replace(" ... ok", " ... ignored"), [name])

    def test_frame_cadenced_probes_are_release_exact_and_outside_five_proof_selector(self):
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        prefix = "net::p2p::native_geometry::frame_cadenced_controller_tests::"
        probes = {
            "Validate frame-cadenced mailbox cold and retained saves":
                "frame_cadenced_mailbox_saves_cold_and_first_retained",
            "Validate missed-frame mailbox refusal without publication":
                "frame_cadenced_mailbox_missed_frames_refuse_without_publication",
        }
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/frame_cadenced_controller_tests.rs").read_text()
        self.assertEqual(set(re.findall(r"#\[test\]\s*#\[ignore[^\n]*\]\s*fn (\w+)\(", source)), set(probes.values()))
        for title, probe in probes.items():
            shell = workflow_gate(title, path)
            name = prefix + probe

            def assert_gate(candidate):
                lines = [line.strip() for line in candidate.splitlines() if line.strip()]
                self.assertIn("set -euo pipefail", lines)
                self.assertEqual([line for line in lines if line.startswith("name=")], [f"name={name}"])
                commands = [shlex.split(line) for line in lines if line.startswith("mbx ")]
                self.assertEqual(commands, [[
                    "mbx", "test", "--locked", "--release", "-p", "benilla-app", "--lib", "$name",
                    "--", "--exact", "--ignored", "--show-output", "--test-threads=1", "--color", "never",
                    "|", "tee", "$log",
                ]])
                verifiers = [shlex.split(line) for line in lines if "verify-libtest-output.py" in line]
                self.assertEqual(verifiers, [["python3", "scripts/verify-libtest-output.py", "$log", "$name"]])

            assert_gate(shell)
            for bad in (
                shell.replace("--release ", ""),
                shell.replace("--exact ", ""),
                shell.replace("--ignored ", ""),
                shell.replace(f"name={name}", f"name={prefix}"),
                shell.replace('"$log" "$name"', '"$log" "$name" || true'),
            ):
                with self.assertRaises(AssertionError):
                    assert_gate(bad)
            VERIFIER.verify(fixture([name]), [name])
            with self.assertRaises(ValueError):
                VERIFIER.verify(fixture([name]).replace(" ... ok", " ... ignored"), [name])

    def test_diagnostic_probes_do_not_hide_later_results_after_failure(self):
        workflow = (ROOT / ".github/workflows/resident-controller-profile-comparison.yml").read_text()
        titles = (
            "Compare two-turn retained certification with synchronous baseline",
            "Compare cold split with retained-only split baseline",
            "Validate the default Route budget in the release component fixture",
            "Validate frame-cadenced mailbox cold and retained saves",
            "Validate missed-frame mailbox refusal without publication",
        )
        condition = "${{ !cancelled() && steps.serial_comparison.outcome == 'success' }}"

        def assert_flow(candidate):
            self.assertNotIn("continue-on-error:", candidate)
            steps = re.split(r"(?=^      - name: )", candidate, flags=re.MULTILINE)
            serial = [step for step in steps if step.startswith(
                "      - name: Compare the same five controller fixtures serially\n")]
            self.assertEqual(len(serial), 1)
            self.assertIn("        id: serial_comparison\n", serial[0])
            self.assertNotRegex(serial[0], r"(?m)^        if:")
            self.assertEqual(candidate.count("        id: serial_comparison\n"), 1)
            for title in titles:
                probe = [step for step in steps if step.startswith(f"      - name: {title}\n")]
                self.assertEqual(len(probe), 1)
                self.assertEqual(re.findall(r"(?m)^        if: (.*)$", probe[0]), [condition])
                self.assertIn("          set -euo pipefail\n", probe[0])
                self.assertNotIn("|| true", probe[0])

        assert_flow(workflow)
        for bad in (
            workflow.replace("!cancelled() && ", ""),
            workflow.replace("!cancelled()", "success()"),
            workflow.replace("steps.serial_comparison.outcome", "steps.serial_comparison.conclusion"),
            workflow.replace("        id: serial_comparison\n", ""),
            workflow.replace("        shell: bash\n", "        continue-on-error: true\n        shell: bash\n", 1),
        ):
            with self.assertRaises(AssertionError):
                assert_flow(bad)

        # Truth table for the exact guarded expression above, not a GitHub runner
        # emulation. A status function suppresses the implicit success() guard.
        expression = condition.removeprefix("${{ ").removesuffix(" }}")
        expression = expression.replace("!cancelled()", "not cancelled").replace("&&", "and")
        expression = expression.replace("steps.serial_comparison.outcome", "serial_outcome")
        for cancelled in (False, True):
            for serial_outcome in ("success", "failure", "skipped", "cancelled"):
                for earlier_probe_failed in (False, True):
                    run_probe = eval(expression, {"__builtins__": {}}, {
                        "cancelled": cancelled, "serial_outcome": serial_outcome,
                    })
                    self.assertEqual(run_probe, not cancelled and serial_outcome == "success")
                    if earlier_probe_failed and serial_outcome == "success" and not cancelled:
                        self.assertTrue(run_probe, "earlier diagnostic failure must not mask this probe")
                    if serial_outcome != "success" or cancelled:
                        self.assertFalse(run_probe, "failed/skipped prerequisite or cancellation must block probes")

    def test_f05_worker_and_private_scope_manifests_match_actual_tests(self):
        base = ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry"
        cases = (
            ("Verify one-slot F05 raw worker", "products/source_height/owned/freshness/tests.rs", True),
            ("Verify one-slot F05 private source frames", "source_route_filter/resident_owned/tests/f05_equivalence.rs", False),
        )
        for title, relative, nested in cases:
            source = (base / relative).read_text()
            if nested:
                source = source.split("mod f05 {", 1)[1]
            actual = re.findall(r"#\[test\]\s*fn (\w+)\(", source)
            names = SUITES[title][2]
            self.assertEqual(set(actual), set(names), title)
            self.assertEqual(len(actual), len(names), title)
            for missing in names:
                expected = [SUITES[title][1] + name for name in names]
                output = "running %d tests\n" % (len(expected) - 1)
                output += "".join(f"test {name} ... ok\n" for name in expected if not name.endswith("::" + missing))
                output += "\ntest result: ok. %d passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n" % (len(expected) - 1)
                with self.assertRaises(ValueError):
                    VERIFIER.verify(output, expected)

    def test_f05_release_fault_gates_attempt_both_despite_failure(self):
        title = "Verify release F05 controls and current scene refusals"
        paths = (".github/workflows/ci.yml", ".github/workflows/resident-controller-profile-comparison.yml")
        names = F05_CONTROLLER_TESTS[1:]
        for path in paths:
            shell = workflow_gate(title, path)
            self.assertIn("set -euo pipefail", shell)
            self.assertIn("for test in " + " ".join(names) + "; do", shell)
            self.assertIn('--release -p benilla-app --lib "$name" -- --exact --ignored', shell)
            self.assertIn('if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi', shell)
            self.assertIn('exit "$status"', shell)
            with tempfile.TemporaryDirectory() as temp:
                temp = Path(temp)
                mbx = temp / "mbx"
                mbx.write_text("#!/usr/bin/env python3\nimport os,sys\nfrom pathlib import Path\nname=sys.argv[sys.argv.index('--lib')+1]\nwith open(os.environ['CALLS'],'a') as f:f.write(name+'\\n')\nassert '--release' in sys.argv and '--exact' in sys.argv and '--ignored' in sys.argv\nprint('running 1 test')\nprint('test '+name+' ... ok')\nprint('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s')\nsys.exit(1 if name.endswith('never_ack') else 0)\n")
                mbx.chmod(0o755)
                env = dict(os.environ, PATH=f"{temp}:{os.environ['PATH']}", RUNNER_TEMP=str(temp), CALLS=str(temp / "calls"))
                run = subprocess.run(["bash", "-c", shell], cwd=ROOT, env=env, capture_output=True, text=True, timeout=10)
                self.assertNotEqual(run.returncode, 0)
                calls = (temp / "calls").read_text().splitlines()
                self.assertEqual([name.rsplit("::", 1)[1] for name in calls], list(names))
        workflow = (ROOT / paths[1]).read_text()
        section = workflow.split("      - name: " + title + "\n", 1)[1].split("      - name:", 1)[0]
        self.assertIn("if: ${{ !cancelled() && steps.serial_comparison.outcome == 'success' }}", section)

    def test_f05_paired_release_gate_preserves_exact_attempt_and_full_trace(self):
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        title = "Compare one parked F05 against cold and retained split"
        shell = workflow_gate(title, path)
        self.assertIn("set -euo pipefail", shell)
        self.assertIn("status=0", shell)
        self.assertIn("name=net::p2p::native_geometry::resident_result_controller_tests::" + F05_CONTROLLER_TESTS[0], shell)
        self.assertEqual(shell.count('mbx test --locked --release -p benilla-app --lib "$name" -- --exact --ignored --show-output --test-threads=1 --color never'), 1)
        self.assertIn('if ! python3 scripts/verify-libtest-output.py "$log" "$name"; then status=1; fi', shell)
        self.assertIn('exit "$status"', shell)
        self.assertNotIn("--skip", shell)
        workflow = (ROOT / path).read_text()
        section = workflow.split("      - name: " + title + "\n", 1)[1].split("      - name:", 1)[0]
        self.assertIn("if: ${{ !cancelled() && steps.serial_comparison.outcome == 'success' }}", section)
        source = (ROOT / "crates/benilla-app/src/net/p2p/world/native_geometry/resident_result_controller_tests.rs").read_text()
        self.assertIn("run_inner(mode, None, &mut Vec::new(), false)", source)
        self.assertIn("run_inner(Mode::ColdPhasedDefaultBudget, None, &mut Vec::new(), true)", source)
        self.assertIn("assert_eq!(baseline.len(), 59)", source)
        self.assertIn("assert_eq!(parked.len(), 60)", source)
        self.assertIn("assert_eq!(parked[11], baseline[3])", source)
        self.assertIn("for slot in [0, 4, 58]", source)

    def test_f05_privacy_gate_runs_real_probe_parser_selftests(self):
        self.assertEqual(workflow_gate("Verify F05 private capability compiler boundaries").strip(), "python3 scripts/verify-f05-privacy.py")
        result = subprocess.run([sys.executable, str(ROOT / "scripts/verify-f05-privacy.py"), "--self-test"], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_f05_short_diagnostic_preserves_both_exit_statuses_and_exact_manifest(self):
        shell = workflow_gate("Diagnose exact F05 private source frames")
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        diagnostic = workflow.split("  f05-private-diagnostic:\n", 1)[1].split("\n  workspace-tests:", 1)[0]
        self.assertIn("${{ (github.event_name == 'pull_request' && github.event.pull_request.head.ref == 'feat/psk-p2p-foundation' && github.event.pull_request.head.repo.full_name == github.repository) || (github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/feat/psk-p2p-foundation') }}", diagnostic)
        self.assertIn("    runs-on: ubuntu-26.04\n    timeout-minutes: 150\n", diagnostic)
        self.assertNotIn("continue-on-error", diagnostic)
        self.assertNotIn("needs:", diagnostic)
        package, prefix, names = SUITES["Verify one-slot F05 private source frames"]
        full_names = [prefix + name for name in names]
        self.assertEqual(len(full_names), 4)
        self.assertIn("set +e", shell)
        for name in full_names:
            self.assertIn(name, shell)
        for test_status, valid, large in ((0, True, False), (17, True, False), (0, False, False), (17, False, False), (17, False, True)):
            with self.subTest(test_status=test_status, valid=valid, large=large), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory)
                mbx = directory / "mbx"
                mbx.write_text("#!/usr/bin/env python3\nimport os,sys\nexpected=['test','--locked','-p','benilla-native-corridor-tests','--lib',os.environ['PREFIX'],'--','--show-output','--test-threads=1','--color','never']\nassert sys.argv[1:]==expected,sys.argv\nwith open(os.environ['CALLS'],'a') as f:f.write('attempt\\n')\nprint(os.environ['OUTPUT'],end='')\nprint('x'*(128*1024) if os.environ['LARGE']=='1' else 'stderr diagnostic only',file=sys.stderr)\nsys.exit(int(os.environ['EXIT']))\n")
                mbx.chmod(0o755)
                output = fixture(full_names if valid else full_names[:-1])
                env = dict(os.environ, PATH=f"{directory}:{os.environ['PATH']}", RUNNER_TEMP=str(directory), CALLS=str(directory / "calls"), PREFIX=prefix, OUTPUT=output, EXIT=str(test_status), LARGE="1" if large else "0")
                result = subprocess.run(["bash", "-c", shell], cwd=ROOT, env=env, text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode == 0, test_status == 0 and valid)
                self.assertEqual((directory / "calls").read_text(), "attempt\n")
                logs = directory / "f05-private-diagnostic"
                self.assertEqual((logs / "test.stdout.log").read_text(), output)
                expected_stderr = "x" * (128 * 1024) + "\n" if large else "stderr diagnostic only\n"
                self.assertEqual((logs / "test.stderr.log").read_text(), expected_stderr)
                if large:
                    self.assertLess(len(result.stdout.encode()), 70000, "huge single-line diagnostics must not flood job log")
                status = (logs / "exit-status").read_text()
                self.assertIn(f"test_exit={test_status}\n", status)
                self.assertEqual("verifier_exit=0\n" in status, valid)
                self.assertTrue((logs / "verifier.stderr.log").is_file(), "verifier runs even after cargo failure")

    def test_f05_diagnostic_artifact_is_bounded_and_missing_attempt_is_not_proof(self):
        import hashlib
        shell = workflow_gate("Prepare bounded F05 diagnostic evidence")
        for attempted in (False, True):
            with self.subTest(attempted=attempted), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory)
                source = directory / "f05-private-diagnostic"
                payload = b"x" * (2 * 1024 * 1024 + 123)
                if attempted:
                    source.mkdir()
                    (source / "attempted").write_text("true\n")
                    (source / "test.stdout.log").write_bytes(payload)
                env = dict(os.environ, RUNNER_TEMP=str(directory), F05_EVENT_SHA="abc123", F05_TEST_OUTCOME="failure" if attempted else "skipped")
                result = subprocess.run(["bash", "-c", shell], cwd=ROOT, env=env, text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                artifact = directory / "f05-private-artifact"
                metadata = json.loads((artifact / "metadata.json").read_text())
                self.assertEqual(metadata["attempted"], attempted)
                self.assertEqual(metadata["event_sha"], "abc123")
                self.assertFalse(metadata["logs"]["exit-status"]["present"])
                if attempted:
                    evidence = metadata["logs"]["test.stdout.log"]
                    retained = (artifact / "test.stdout.log").read_bytes()
                    self.assertEqual(len(retained), 2 * 1024 * 1024)
                    self.assertTrue(evidence["truncated_to_tail"])
                    self.assertEqual(evidence["original_sha256"], hashlib.sha256(payload).hexdigest())
                    self.assertEqual(evidence["retained_sha256"], hashlib.sha256(retained).hexdigest())
                    self.assertEqual((source / "test.stdout.log").read_bytes(), payload)
                else:
                    self.assertEqual(list(artifact.iterdir()), [artifact / "metadata.json"])
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        upload = workflow.split("      - name: Preserve bounded F05 diagnostic evidence\n", 1)[1].split("\n  workspace-tests:", 1)[0]
        self.assertIn("if: always()", upload)
        self.assertIn("if-no-files-found: error", upload)
        self.assertIn("actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9", upload)

    def test_full_workspace_suite_keeps_its_own_unfiltered_job(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        jobs = re.findall(
            r"^  workspace-tests:\n(.*?)(?=^  \S|\Z)", workflow,
            flags=re.MULTILINE | re.DOTALL,
        )
        self.assertEqual(len(jobs), 1)
        job = jobs[0]
        self.assertNotRegex(job, r"(?m)^    (?:if|needs|continue-on-error):")
        self.assertIn("    timeout-minutes: 150\n", job)
        command = "        run: mbx test --locked --no-fail-fast --workspace\n"
        self.assertEqual(workflow.count(command), 1)
        steps = re.split(r"(?=^      - )", job, flags=re.MULTILINE)
        self.assertIn("      - name: test\n" + command, steps)

    def test_aggregate_requires_every_job_and_rejects_non_success(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        aggregate = workflow.split("\n  gates:\n", 1)[1]
        self.assertIn("    if: always()\n", aggregate)
        self.assertIn(
            "    needs: [workspace-gates, workspace-tests, player-gates]\n", aggregate
        )
        self.assertNotIn("continue-on-error:", aggregate)
        self.assertEqual(re.findall(r"(?m)^\s+if:.*$", aggregate), ["    if: always()"])
        variables = {
            "WORKSPACE_RESULT": "workspace-gates",
            "WORKSPACE_TESTS_RESULT": "workspace-tests",
            "PLAYER_RESULT": "player-gates",
        }
        for variable, job in variables.items():
            self.assertIn(
                f"          {variable}: ${{{{ needs.{job}.result }}}}\n", aggregate
            )
        shell = workflow_gate("Require every gate job to succeed")
        success = dict(os.environ, **dict.fromkeys(variables, "success"))

        def run(env):
            return subprocess.run(
                ["bash", "-c", shell], env=env, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10,
            ).returncode

        self.assertEqual(run(success), 0)
        for variable in variables:
            for status in ("failure", "cancelled", "skipped", "", None):
                with self.subTest(job=variables[variable], status=status):
                    env = success.copy()
                    if status is None:
                        del env[variable]
                    else:
                        env[variable] = status
                    self.assertNotEqual(run(env), 0)


if __name__ == "__main__":
    unittest.main()

