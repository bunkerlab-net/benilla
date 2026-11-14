#!/usr/bin/env python3
"""Exercise CI apt retries without sudo, networking or host apt changes."""

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci-apt-install.sh"


class AptInstallTests(unittest.TestCase):
    def run_install(self, mode="fallback", arch="amd64", unresolved=()):
        with tempfile.TemporaryDirectory(dir=ROOT, prefix=".ci-apt-test-") as temp:
            root = Path(temp)
            apt = root / "apt"
            sources = apt / "sources.list.d"
            sources.mkdir(parents=True)
            (apt / "apt-mirrors.txt").write_text("https://security.ubuntu.com/ubuntu/\n")
            (sources / "ubuntu.sources").write_text(
                "Types: deb\nURIs: mirror+file:/etc/apt/apt-mirrors.txt\n"
                "Suites: resolute resolute-updates resolute-backports resolute-security\n"
                "Components: main universe\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n"
            )
            (apt / "sources.list").write_text(
                "deb https://security.ubuntu.com/ubuntu/ resolute-security main\n"
            )
            (sources / "vendor.list").write_text("deb https://example.org/packages stable main\n")
            log = root / "calls.jsonl"
            stub = root / "stub"
            stub.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
command = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["STUB_LOG"], "a") as output:
    output.write(json.dumps([command, *args]) + "\\n")
if command == "sudo":
    os.execvp(args[0], args)
if command == "sleep":
    sys.exit(0)
if command == "dpkg":
    print(os.environ["STUB_ARCH"])
    sys.exit(0)
if command == "getent":
    host = args[-1]
    bad = os.environ["STUB_UNRESOLVED"].split(",")
    sys.exit(2 if host in bad else 0)
if command == "apt-get":
    mode = os.environ["STUB_MODE"]
    mirror = (pathlib.Path(os.environ["CI_APT_DIR"]) / "apt-mirrors.txt").read_text()
    calls = [json.loads(line) for line in pathlib.Path(os.environ["STUB_LOG"]).read_text().splitlines()]
    updates = sum(call[0] == "apt-get" and "update" in call for call in calls)
    if mode == "success" or (mode == "transient" and updates >= 2):
        sys.exit(0)
    if mode == "install-transient" and ("update" in args or updates >= 2):
        sys.exit(0)
    if mode == "fail-install" and "update" in args:
        sys.exit(0)
    if mode == "fallback" and "security.ubuntu.com" not in mirror:
        sys.exit(0)
    print("Temporary failure resolving 'security.ubuntu.com'", file=sys.stderr)
    sys.exit(100)
raise SystemExit("unexpected stub command: " + command)
''')
            stub.chmod(0o755)
            for name in ("sudo", "sleep", "dpkg", "getent", "apt-get"):
                (root / name).symlink_to(stub)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                       CI_APT_DIR=str(apt), STUB_LOG=str(log), STUB_MODE=mode,
                       STUB_ARCH=arch, STUB_UNRESOLVED=",".join(unresolved))
            result = subprocess.run(
                ["bash", str(SCRIPT), "pkg-config", "libwayland-dev"],
                env=env, text=True, capture_output=True, timeout=20,
            )
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            files = {str(path.relative_to(apt)): path.read_text()
                     for path in apt.rglob("*") if path.is_file()}
            return result, calls, files

    def test_dns_failure_retries_then_falls_back_preserving_security(self):
        result, calls, files = self.run_install(unresolved=("security.ubuntu.com",))
        self.assertEqual(result.returncode, 0, result.stderr)
        apt = [call for call in calls if call[0] == "apt-get"]
        self.assertEqual(sum("update" in call for call in apt), 4)
        self.assertEqual(sum("install" in call for call in apt), 1)
        self.assertEqual(apt[-1][-2:], ["pkg-config", "libwayland-dev"])
        for call in apt:
            self.assertIn("Acquire::Retries=5", call)
        self.assertFalse(any("APT::Update::Error-Mode=any" in call for call in apt))
        self.assertEqual(files["apt-mirrors.txt"], "https://archive.ubuntu.com/ubuntu/\n")
        self.assertIn("https://archive.ubuntu.com/ubuntu/ resolute-security", files["sources.list"])
        self.assertIn("resolute-backports resolute-security", files["sources.list.d/ubuntu.sources"])
        self.assertIn("Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg", files["sources.list.d/ubuntu.sources"])
        self.assertEqual(files["sources.list.d/vendor.list"], "deb https://example.org/packages stable main\n")

    def test_transient_update_and_install_failures_retry_without_rewrite(self):
        for mode in ("transient", "install-transient", "success"):
            with self.subTest(mode=mode):
                result, calls, files = self.run_install(mode)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("security.ubuntu.com", files["apt-mirrors.txt"])
                self.assertFalse(any(call[0] == "getent" for call in calls))

    def test_arm64_falls_back_to_ports_not_x86_archive(self):
        result, _, files = self.run_install(arch="arm64", unresolved=("security.ubuntu.com",))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(files["apt-mirrors.txt"], "https://ports.ubuntu.com/ubuntu-ports/\n")

    def test_unresolvable_archive_uses_resolvable_azure(self):
        result, calls, files = self.run_install(unresolved=("security.ubuntu.com", "archive.ubuntu.com"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(files["apt-mirrors.txt"], "https://azure.archive.ubuntu.com/ubuntu/\n")
        self.assertIn(["getent", "ahosts", "azure.archive.ubuntu.com"], calls)

    def test_exhaustion_fails_loud_even_with_resolvable_fallbacks(self):
        for mode in ("fail", "fail-install"):
            with self.subTest(mode=mode):
                result, calls, _ = self.run_install(mode, unresolved=("security.ubuntu.com",))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("apt installation failed after bounded retries", result.stderr)
                apt = [call for call in calls if call[0] == "apt-get"]
                self.assertEqual(sum("update" in call for call in apt), 9)
                self.assertEqual(sum("install" in call for call in apt), 9 if mode == "fail-install" else 0)

    def test_no_resolvable_mirror_fails_without_rewriting(self):
        result, _, files = self.run_install("fail", unresolved=(
            "security.ubuntu.com", "archive.ubuntu.com", "azure.archive.ubuntu.com"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("apt installation failed after bounded retries", result.stderr)
        self.assertIn("security.ubuntu.com", files["apt-mirrors.txt"])

    def test_other_apt_errors_fail_without_mirror_rewrite(self):
        result, calls, files = self.run_install("fail")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("apt installation failed after bounded retries", result.stderr)
        self.assertEqual(sum(call[0] == "apt-get" and "update" in call for call in calls), 3)
        self.assertIn("security.ubuntu.com", files["apt-mirrors.txt"])

    def test_workflows_keep_package_lists_and_check_out_before_install(self):
        expected = {
            "ci.yml": (4, "pkg-config libasound2-dev libudev-dev libwayland-dev"),
            "build.yml": (1, "build-essential pkg-config libasound2-dev libudev-dev"),
            "resident-controller-profile-comparison.yml": (1, "pkg-config libasound2-dev libudev-dev libwayland-dev"),
            "resident-hash-hex-primitive.yml": (1, "pkg-config libasound2-dev libudev-dev libwayland-dev"),
            "resident-instrumentation-revision-comparison.yml": (1, "pkg-config libasound2-dev libudev-dev libwayland-dev"),
        }
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            workflow = path.read_text()
            self.assertNotIn("apt-get", workflow, path.name)
            if path.name not in expected:
                continue
            count, packages = expected[path.name]
            command = "bash scripts/ci-apt-install.sh " + packages
            self.assertEqual(workflow.count(command), count, path.name)
            for job in re.split(r"\n(?=  \S)", workflow):
                if command in job:
                    self.assertIn("uses: actions/checkout@", job.split(command)[0])


if __name__ == "__main__":
    unittest.main()
