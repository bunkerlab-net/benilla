#!/usr/bin/env python3
"""No-compile fixtures for the actual joined-suite CI shell gates."""

import importlib.util
import os
from pathlib import Path
import re
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
        ),
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
                EXPECTED_EXACT="1" if title == "Verify resident observation without transport admission" else "0",
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

    def test_controller_profile_comparison_is_scoped_and_profile_only(self):
        path = ".github/workflows/resident-controller-profile-comparison.yml"
        workflow = (ROOT / path).read_text()
        self.assertIn(
            "on:\n  push:\n"
            "    branches: [feat/psk-p2p-foundation]\n"
            f"    paths: [{path}]\n",
            workflow,
        )
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

