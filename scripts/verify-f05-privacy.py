#!/usr/bin/env python3
"""Compile actual F05 misuse probes in a disposable, offline checkout; never run tests."""

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile


TARGET = "benilla_native_corridor_tests"
SOURCE = Path("crates/benilla-native-corridor-tests/src/lib.rs")
PREFIX = "F05_PRIVACY_"
IMPORTS = """
use super::products::source_height::{
    inventory::OwnedInventoryCompatibility,
    owned::{
        f05_scope::{self, F05HeightFrame, F05InventoryFrame},
        freshness::{F05Completion, F05Diagnostics, F05Guard, F05VerifiedSuccess},
        OwnedSourceProducts,
    },
    CompositionError,
};
use super::source_route_filter::resident::owned::{
    f05::{DriverCustody, F05ResumePermit, InactiveF05Proof, ResidentF05Driver},
    OwnedResident,
};
use std::{rc::Rc, sync::Arc};
"""


@dataclass(frozen=True)
class Probe:
    name: str
    code: str
    words: tuple[str, ...]
    body: str

    def module(self) -> str:
        return (
            "\n#[cfg(all(test, not(benilla_headless_geometry)))]\n"
            "#[allow(dead_code, unused_imports, unused_variables)]\n"
            "mod f05_privacy_probe {\n" + IMPORTS + self.body + "\n}\n"
        )


def probes() -> list[Probe]:
    result = []
    for operand in ("height", "compatibility"):
        name = f"escape_{operand}"
        result.append(Probe(name, "E0521", ("borrowed data escapes",), f"""
fn probe(h: &F05HeightFrame, i: &F05InventoryFrame,
         current: &Rc<OwnedInventoryCompatibility>, permit: &F05ResumePermit) {{
    let mut escaped = None;
    let _ = f05_scope::with_f05_views(h, i, current, permit, |height, compatibility| {{
        escaped = Some({operand}); // {PREFIX}{name}
        Ok::<(), CompositionError>(())
    }});
    std::hint::black_box(escaped);
}}
"""))
    constructors = (
        ("construct_resume_permit", "F05ResumePermit", "products, custody, invocation, guard"),
        ("construct_inactive_proof", "InactiveF05Proof", "products, custody, invocation, guard"),
        ("construct_driver_custody", "DriverCustody", "identity: custody, cancel_after_staging: false"),
        ("construct_height_frame", "F05HeightFrame", "products, custody, invocation, guard"),
        ("construct_inventory_frame", "F05InventoryFrame", "inventory, products, custody, invocation"),
    )
    for name, kind, fields in constructors:
        result.append(Probe(name, "E0451", (kind, "private"), f"""
fn probe(products: Arc<OwnedSourceProducts>, inventory: Rc<OwnedInventoryCompatibility>,
         custody: Rc<()>, invocation: Rc<()>, guard: Rc<F05Guard>) -> {kind} {{
    {kind} {{ {fields} }} // {PREFIX}{name}
}}
"""))
    for kind in ("F05ResumePermit", "InactiveF05Proof", "F05HeightFrame", "F05InventoryFrame",
                 "F05VerifiedSuccess", "F05Completion"):
        name = f"clone_{kind}"
        result.append(Probe(name, "E0277", (kind, "Clone"), f"""
fn require_clone<T: Clone>() {{}}
fn probe() {{ require_clone::<{kind}>(); }} // {PREFIX}{name}
"""))
    # Struct-update probes use an existing value, but attempt to bypass the sealed
    # constructors. Rewriting completion.result could otherwise forge actual success.
    for name, kind, field in (
        ("construct_verified_success", "F05VerifiedSuccess", "diagnostics: F05Diagnostics::default()"),
        ("rewrite_completion_success", "F05Completion", "result: Ok(())"),
    ):
        result.append(Probe(name, "E0451", (kind, "private"), f"""
fn probe(existing: {kind}) -> {kind} {{
    {kind} {{ {field}, ..existing }} // {PREFIX}{name}
}}
"""))
    result.append(Probe("escape_mutable_owner", "E0616", ("owner", "ResidentF05Driver", "private"), f"""
fn probe(driver: &mut ResidentF05Driver) -> &mut OwnedResident {{
    &mut driver.owner // {PREFIX}escape_mutable_owner
}}
"""))
    return result


def records(output: str) -> list[dict]:
    result = []
    for line in output.splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            result.append(value)
    return result


def compiler_errors(items: list[dict]) -> list[dict]:
    result = []
    for item in items:
        if item.get("reason") != "compiler-message":
            continue
        message = item.get("message", {})
        if message.get("level") != "error":
            continue
        # rustc's count-only footer is not an additional semantic diagnostic.
        if (not message.get("spans") and message.get("code") is None
                and re.fullmatch(r"aborting due to \d+ previous errors?(?:; \d+ warnings? emitted)?",
                                 message.get("message", ""))):
            continue
        result.append(item)
    return result


def source_matches(name: str, checkout: Path) -> bool:
    path = Path(name)
    expected = (checkout / SOURCE).resolve()
    candidates = (path,) if path.is_absolute() else (
        checkout / path, checkout / SOURCE.parent.parent / path,
    )
    return any(candidate.resolve() == expected for candidate in candidates)


def verify_negative(probe: Probe, returncode: int, items: list[dict],
                    checkout: Path, marker_line: int) -> int:
    if returncode != 101:
        raise ValueError(f"expected cargo compile-error exit 101, got {returncode}")
    finished = [x for x in items if x.get("reason") == "build-finished"]
    if len(finished) != 1 or finished[0].get("success") is not False:
        raise ValueError("missing exact failed cargo build-finished record")
    errors = compiler_errors(items)
    if not errors:
        raise ValueError("no Rust compiler error; infrastructure failure is not privacy proof")
    for item in errors:
        message = item["message"]
        code = (message.get("code") or {}).get("code")
        primary = [span for span in message.get("spans", []) if span.get("is_primary")]
        tagged = any(
            source_matches(span.get("file_name", ""), checkout)
            and span.get("line_start", 0) <= marker_line <= span.get("line_end", 0)
            for span in primary
        )
        if (item.get("target", {}).get("name") != TARGET or code != probe.code or not tagged
                or not all(word in message.get("message", "") for word in probe.words)):
            raise ValueError("unexpected compiler error, not the targeted privacy refusal: "
                             + str(code) + " " + message.get("message", ""))
    return len(errors)


def run(command: list[str], cwd: Path, env: dict, timeout: int) -> subprocess.CompletedProcess:
    process = subprocess.Popen(command, cwd=cwd, env=env, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=(os.name == "posix"))
    try:
        out, err = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        process.communicate()
        raise ValueError(f"command timed out after {timeout}s: {command[0]}") from None
    return subprocess.CompletedProcess(command, process.returncode, out, err)


def checked(command: list[str], cwd: Path, env: dict, timeout: int) -> str:
    completed = run(command, cwd, env, timeout)
    if completed.returncode:
        raise ValueError(f"command failed ({completed.returncode}): {command!r}\n{completed.stderr}")
    return completed.stdout.strip()


def describe_failure(completed: subprocess.CompletedProcess) -> str:
    errors = compiler_errors(records(completed.stdout))
    rendered = "\n".join(x["message"].get("rendered") or x["message"].get("message", "")
                         for x in errors)
    return (rendered + "\n" + completed.stderr)[-16000:]


def self_test() -> None:
    probe = probes()[-1]
    checkout = Path("/tmp/f05-parser-self-test")
    message = {
        "reason": "compiler-message", "target": {"name": TARGET},
        "message": {"level": "error", "code": {"code": probe.code},
                    "message": "field owner of struct ResidentF05Driver is private",
                    "spans": [{"is_primary": True, "file_name": str(SOURCE),
                               "line_start": 101, "line_end": 101}]},
    }
    end = {"reason": "build-finished", "success": False}
    assert verify_negative(probe, 101, [message, end], checkout, 101) == 1
    for change in ("code", "span", "extra", "success", "target"):
        bad = json.loads(json.dumps(message))
        items = [bad, end]
        rc = 101
        if change == "code":
            bad["message"]["code"]["code"] = "E0433"
        elif change == "span":
            bad["message"]["spans"][0]["line_start"] = 102
            bad["message"]["spans"][0]["line_end"] = 102
        elif change == "extra":
            extra = json.loads(json.dumps(message))
            extra["message"]["code"]["code"] = "E0308"
            items.append(extra)
        elif change == "success":
            rc = 0
        else:
            bad["target"]["name"] = "other_crate"
        try:
            verify_negative(probe, rc, items, checkout, 101)
        except ValueError:
            continue
        raise AssertionError(f"parser accepted invalid evidence: {change}")
    print("PASS parser self-tests only; no Rust compilation or privacy proof performed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--target-dir", type=Path, help="optional cached target OUTSIDE original checkout")
    parser.add_argument("--release", action="store_true", help="same release profile for baseline and every probe")
    parser.add_argument("--timeout-seconds", type=int, default=3600, help="per compiler command timeout")
    parser.add_argument("--list-probes", action="store_true")
    parser.add_argument("--self-test", action="store_true", help="diagnostic parser tests only, without Rust")
    args = parser.parse_args()
    inventory = probes()
    if args.list_probes:
        for probe in inventory:
            print(f"{probe.name}: {probe.code}; {' / '.join(probe.words)}")
        return 0
    if args.self_test:
        self_test()
        return 0
    if args.timeout_seconds <= 0:
        raise ValueError("timeout must be positive")
    cargo = shutil.which(args.cargo)
    if cargo is None:
        raise ValueError("cargo unavailable; no baseline or compiler-negative probe was run")
    if shutil.which("git") is None:
        raise ValueError("git unavailable")
    env = dict(os.environ)
    for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"):
        env.pop(key, None)
    env.update({"CARGO_NET_OFFLINE": "true", "GIT_TERMINAL_PROMPT": "0",
                "GIT_LFS_SKIP_SMUDGE": "1", "GIT_ALLOW_PROTOCOL": "file",
                "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
                "GIT_OPTIONAL_LOCKS": "0"})
    flags = env.get("RUSTFLAGS", "") + " " + env.get("CARGO_ENCODED_RUSTFLAGS", "").replace("\x1f", " ")
    if re.search(r"--cfg(?:=|\s+)[\"']?benilla_headless_geometry\b", flags):
        raise ValueError("privacy gate requires the actual non-headless corridor cfg")
    repo = args.repo.resolve()
    actual = Path(checked(["git", "rev-parse", "--show-toplevel"], repo, env, 60)).resolve()
    if actual != repo:
        raise ValueError("--repo must name the checkout root")
    if checked(["git", "status", "--porcelain=v1", "--untracked-files=normal"], repo, env, 60):
        raise ValueError("compiler gate requires a clean committed checkout; static --self-test/--list-probes do not")
    revision = checked(["git", "rev-parse", "HEAD"], repo, env, 60)
    if args.target_dir:
        target = args.target_dir.resolve()
        if target == repo or repo in target.parents:
            raise ValueError("target directory must be outside the original checkout")
    else:
        target = None
    temporary_root = Path(tempfile.gettempdir()).resolve()
    if temporary_root == repo or repo in temporary_root.parents:
        raise ValueError("temporary directory must be outside the original checkout")
    print(f"F05 privacy baseline revision={revision} probes={len(inventory)}", flush=True)
    with tempfile.TemporaryDirectory(prefix="benilla-f05-privacy-") as temporary:
        workspace = Path(temporary)
        checkout = workspace / "checkout"
        checked(["git", "-c", f"core.hooksPath={os.devnull}", "-c", "filter.lfs.smudge=",
                 "-c", "filter.lfs.required=false", "clone", "--quiet", "--local", "--no-hardlinks",
                 "--no-recurse-submodules", str(repo), str(checkout)], repo, env, 120)
        cloned_revision = checked(["git", "rev-parse", "HEAD"], checkout, env, 60)
        if cloned_revision != revision:
            raise ValueError("source HEAD changed during clone; no probe accepted")
        source = checkout / SOURCE
        if checkout.resolve() not in source.resolve().parents or not source.is_file():
            raise ValueError("probe target is missing or escapes disposable checkout")
        original = source.read_bytes()
        original_text = original.decode("utf-8")
        if PREFIX in original_text or "mod f05_privacy_probe" in original_text:
            raise ValueError("committed target already contains privacy probe markers")
        build_target = target or workspace / "target"
        command = [cargo, "test", "--offline", "--locked", "-p", "benilla-native-corridor-tests",
                   "--lib", "--no-run", "--message-format=json", "--target-dir", str(build_target)]
        if args.release:
            command.append("--release")
        print("Baseline command: " + " ".join(command), flush=True)
        baseline = run(command, checkout, env, args.timeout_seconds)
        items = records(baseline.stdout)
        artifacts = [x for x in items if x.get("reason") == "compiler-artifact"
                     and x.get("target", {}).get("name") == TARGET
                     and x.get("profile", {}).get("test") is True and x.get("executable")]
        finished = [x for x in items if x.get("reason") == "build-finished"]
        if (baseline.returncode != 0 or compiler_errors(items) or not artifacts or len(finished) != 1
                or finished[0].get("success") is not True):
            raise ValueError("baseline did not compile; ZERO privacy probes accepted\n" + describe_failure(baseline))
        print("PASS actual corridor baseline compiled; tests were not executed", flush=True)
        for probe in inventory:
            text = original_text + probe.module()
            marker = PREFIX + probe.name
            lines = [n for n, line in enumerate(text.splitlines(), 1) if marker in line]
            if len(lines) != 1:
                raise ValueError(f"non-unique marker for {probe.name}")
            try:
                source.write_text(text, encoding="utf-8")
                completed = run(command, checkout, env, args.timeout_seconds)
                try:
                    count = verify_negative(probe, completed.returncode, records(completed.stdout),
                                            checkout, lines[0])
                except ValueError as error:
                    raise ValueError(f"probe {probe.name}: {error}\n" + describe_failure(completed)) from None
                print(f"PASS {probe.name}: {count} targeted {probe.code} error(s) at {SOURCE}:{lines[0]}", flush=True)
            finally:
                source.write_bytes(original)
        if (checked(["git", "rev-parse", "HEAD"], repo, env, 60) != revision
                or checked(["git", "status", "--porcelain=v1", "--untracked-files=normal"], repo, env, 60)):
            raise ValueError("original checkout changed during gate; do not attribute result to current tree")
    print(f"PASS F05 privacy: baseline compiled; {len(inventory)}/{len(inventory)} misuse probes rejected")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, UnicodeError, ValueError) as error:
        print(f"FAIL F05 privacy: {error}", file=sys.stderr)
        sys.exit(1)
