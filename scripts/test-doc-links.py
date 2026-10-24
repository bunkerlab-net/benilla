#!/usr/bin/env python3
"""The doc-link gate must fail closed when rustdoc did not run successfully."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("doc_links", Path(__file__).with_name("doc-links.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

class DocLinkGateTests(unittest.TestCase):
    def test_cargo_failure_cannot_report_clean_links(self):
        result = subprocess.CompletedProcess([], 101, "", "dependency resolution failed")
        with patch.object(gate.subprocess, "run", return_value=result), patch.object(gate.sys, "argv", ["doc-links.py"]), contextlib.redirect_stderr(io.StringIO()) as errors, contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(gate.main(), 1)
        self.assertIn("cargo doc failed with exit code 101", errors.getvalue())
        self.assertNotIn("doc-links ok", output.getvalue())

    def test_missing_cargo_is_an_error(self):
        with patch.object(gate.subprocess, "run", side_effect=FileNotFoundError("cargo unavailable")), patch.object(gate.sys, "argv", ["doc-links.py"]), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(gate.main(), 1)

    def test_successful_rustdoc_still_checks_links(self):
        warning = "warning: unresolved link to `crate::removed_item`\n --> crates/example.rs:7"
        result = subprocess.CompletedProcess([], 0, "", warning)
        with patch.object(gate.subprocess, "run", return_value=result), patch.object(gate, "declared_names", return_value=set()), patch.object(gate.sys, "argv", ["doc-links.py"]), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(gate.main(), 1)

    def test_successful_clean_rustdoc_passes(self):
        result = subprocess.CompletedProcess([], 0, "", "")
        with patch.object(gate.subprocess, "run", return_value=result), patch.object(gate, "declared_names", return_value=set()), patch.object(gate.sys, "argv", ["doc-links.py"]), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(gate.main(), 0)

if __name__ == "__main__":
    unittest.main()
