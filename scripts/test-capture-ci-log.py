#!/usr/bin/env python3
"""CI diagnostic logging regressions; no Rust build or game data required."""
import importlib.util
import os
from pathlib import Path
import shutil
import shlex
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("capture-ci-log.py").resolve()
spec = importlib.util.spec_from_file_location("capture_ci_log", SCRIPT)
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


def workflow_bash():
    # Native Windows executable search need not select the Bash already running CI.
    explicit = os.environ.get("BENILLA_TEST_BASH")
    if explicit:
        if not Path(explicit).is_file():
            raise RuntimeError(f"BENILLA_TEST_BASH is not a file: {explicit}")
        return explicit
    if os.name == "nt":
        raise RuntimeError("set BENILLA_TEST_BASH to the workflow's native Bash executable")
    resolved = shutil.which("bash")
    if resolved is None:
        raise RuntimeError("Bash is required for the pipefail regression")
    return resolved


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.log = self.root / "logs" / "native.log"

    def command(self, program):
        return [sys.executable, str(SCRIPT), "--log", str(self.log), "--",
                sys.executable, "-c", program]

    def run_capture(self, program):
        return subprocess.run(self.command(program), capture_output=True, timeout=30)

    def test_success_and_merged_unicode_output_are_byte_exact(self):
        output = "native café 雪 🐻\n".encode("utf-8")
        result = self.run_capture(
            f"import os; os.write(1, {output!r}); os.write(2, b'native stderr\\n')")
        expected = output + b"native stderr\n"
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, expected)
        self.assertEqual(result.stderr, b"")
        header, tail = self.log.read_bytes().split(b"\n", 1)
        self.assertIn(b"child exit: 0;", header)
        self.assertIn(b"console closed: False", header)
        self.assertEqual(tail, expected)

    def test_failure_retains_exact_exit_and_diagnostic(self):
        for code in (1, 37, 101):
            with self.subTest(code=code):
                result = self.run_capture(
                    f"import os, sys; os.write(1, b'actual helper failure\\n'); sys.exit({code})")
                self.assertEqual(result.returncode, code, result.stderr)
                self.assertIn(f"child exit: {code};".encode(), self.log.read_bytes())
                self.assertTrue(self.log.read_bytes().endswith(b"actual helper failure\n"))

    def test_output_cap_keeps_failure_tail_without_stopping_command(self):
        count = capture.LOG_LIMIT * 2
        result = self.run_capture(
            f"import os, sys; os.write(1, b'x' * {count}); "
            "os.write(2, b'FINAL FAILURE\\n'); sys.exit(29)")
        self.assertEqual(result.returncode, 29, result.stderr)
        self.assertEqual(len(result.stdout), count + len(b"FINAL FAILURE\n"))
        saved = self.log.read_bytes()
        self.assertLessEqual(len(saved), capture.LOG_LIMIT)
        header, tail = saved.split(b"\n", 1)
        self.assertIn(b"child exit: 29;", header)
        self.assertEqual(len(tail), capture.TAIL_LIMIT)
        self.assertTrue(tail.endswith(b"FINAL FAILURE\n"))

    def test_closed_console_does_not_break_child_output_or_replace_exit(self):
        marker = self.root / "finished"
        program = (f"import os, pathlib, sys; os.write(1, b'x' * {capture.LOG_LIMIT * 2}); "
                   f"pathlib.Path({str(marker)!r}).write_text('finished'); sys.exit(43)")
        with subprocess.Popen(self.command(program), stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE) as child:
            child.stdout.close()
            diagnostic = child.stderr.read()
            code = child.wait(timeout=30)
        self.assertEqual(code, 43, diagnostic)
        self.assertEqual(marker.read_text(), "finished")
        self.assertIn(b"child exit: 43;", self.log.read_bytes())
        self.assertIn(b"console closed: True", self.log.read_bytes())
        self.assertLessEqual(self.log.stat().st_size, capture.LOG_LIMIT)

    def test_spawn_error_is_logged_and_fails_closed(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--log", str(self.log), "--",
             str(self.root / "missing-command")], capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 127, result.stderr)
        self.assertIn(b"command could not start;", self.log.read_bytes())
        self.assertIn(b"exit: 127", self.log.read_bytes())

    def test_explicit_bash_does_not_repeat_path_search(self):
        selected = self.root / "selected bash.exe"
        selected.touch()
        with patch.dict(os.environ, {"BENILLA_TEST_BASH": str(selected)}):
            with patch.object(shutil, "which", side_effect=AssertionError("unexpected PATH search")):
                self.assertEqual(workflow_bash(), str(selected))

    def test_missing_explicit_bash_fails_instead_of_skipping(self):
        with patch.dict(os.environ, {"BENILLA_TEST_BASH": str(self.root / "missing bash")}):
            with self.assertRaisesRegex(RuntimeError, "BENILLA_TEST_BASH is not a file"):
                workflow_bash()

    def test_bash_pipefail_retains_exact_exit(self):
        bash = workflow_bash()
        fixture = self.root / "failure fixture.py"
        fixture.write_text("import os, sys; os.write(1, b'failed\\n'); sys.exit(37)\n",
                           encoding="utf-8")
        command = [Path(sys.executable).as_posix(), SCRIPT.as_posix(), "--log",
                   self.log.as_posix(), "--", Path(sys.executable).as_posix(), fixture.as_posix()]
        # Bash reads its script from stdin; Windows never reparses a nested -c command.
        script = "set -o pipefail\n" + shlex.join(command) + " | cat\n"
        result = subprocess.run(
            [bash, "--noprofile", "--norc", "-s"], input=script.encode("utf-8"),
            capture_output=True, timeout=30)
        diagnostic = f"bash={bash!r}; stdout={result.stdout!r}; stderr={result.stderr!r}"
        self.assertEqual(result.returncode, 37, diagnostic)
        self.assertEqual(result.stdout, b"failed\n", diagnostic)

    @unittest.skipIf(os.name == "nt", "POSIX signal exit convention")
    def test_signal_failure_is_not_success(self):
        result = self.run_capture("import os, signal; os.kill(os.getpid(), signal.SIGTERM)")
        self.assertEqual(result.returncode, 128 + signal.SIGTERM)
        self.assertIn(f"child exit: {-signal.SIGTERM};".encode(), self.log.read_bytes())


if __name__ == "__main__":
    unittest.main()
