#!/usr/bin/env python3
"""No-compile fixtures for the actual joined-suite CI shell gates."""

import importlib.util
import os
from pathlib import Path
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


def fixture(names):
    count = len(names)
    return (
        f"\nrunning {count} tests\n"
        + "".join(f"test {name} ... ok\n" for name in names)
        + f"\nsuccesses:\n\n---- {names[0]} stdout ----\nfixture diagnostics\n"
        + "\nsuccesses:\n"
        + "".join(f"    {name}\n" for name in names)
        + f"\ntest result: ok. {count} passed; 0 failed; 0 ignored; "
        + "0 measured; 739 filtered out; finished in 15.18s\n\n"
    )


def workflow_gate(title):
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
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
            lambda out, names: out.replace(f"test {names[1]}", f"test {names[0]}")
        )

    def test_rejects_extra_duplicate_or_unexpected_result(self):
        for row in ("test {name} ... ok\n", "test extra ... ok\n", "test extra ... FAILED\n"):
            with self.subTest(row=row):
                self.assert_rejected(
                    lambda out, names: out.replace("\nsuccesses:", "\n" + row.format(name=names[0]) + "successes:", 1)
                )

    def test_rejects_missing_wrong_and_duplicate_header(self):
        for replacement in ("", "running 0 tests\n", "running 9 tests\n"):
            with self.subTest(replacement=replacement):
                self.assert_rejected(
                    lambda out, names: out.replace(f"running {len(names)} tests\n", replacement)
                )
        self.assert_rejected(lambda out, names: f"running {len(names)} tests\n" + out)

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
        self.assertEqual(workflow.count("run: python3 scripts/test-verify-libtest-output.py"), 1)
        self.assertLess(workflow.index("- name: Test exact libtest proof gates"), workflow.index("- name: clippy"))


if __name__ == "__main__":
    unittest.main()
