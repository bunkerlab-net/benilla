#!/usr/bin/env python3
"""Smoke the actual player diagnostic dispatch without an install or saved state.

The executable is copied away from the checkout before it runs. This verifies
the report-only path, absence of startup chatter, and unchanged isolated state;
it is not a syscall tracer or a network/window sandbox.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import time


MAX_STDOUT = 256 * 1024
MAX_STDERR = 64 * 1024
MAX_LINE = 16 * 1024
HARD_TIMEOUT = 20.0
FORMAT = "benilla-collision-diagnostics-v1"
STAGES = ("selection", "capture", "placement", "assembly", "validation",
          "probe", "freshness", "complete")


class SmokeError(Exception):
    """Only fixed, non-sensitive messages may cross this boundary."""


class Drain:
    def __init__(self, pipe, limit, failed):
        self.pipe = pipe
        self.limit = limit
        self.failed = failed
        self.data = bytearray()
        self.overflow = False
        self.error = False
        self.done = threading.Event()
        self.thread = threading.Thread(target=self.read, daemon=True)
        self.thread.start()

    def read(self):
        try:
            while chunk := self.pipe.read(8192):
                remaining = self.limit - len(self.data)
                self.data.extend(chunk[:remaining])
                if len(chunk) > remaining:
                    self.overflow = True
                    self.failed.set()
        except (OSError, ValueError):
            self.error = True
            self.failed.set()
        finally:
            self.pipe.close()
            self.done.set()


def stop_process(process):
    # Kill the POSIX process group even if the immediate child already exited:
    # a descendant may still own a redirected pipe. Never wait without a bound.
    if os.name == "nt":
        if process.poll() is None:
            try:
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                               stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, timeout=2, check=False)
            except (OSError, subprocess.TimeoutExpired):
                pass
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if process.poll() is None:
        try:
            process.kill()
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        raise SmokeError("child termination timed out") from None


def run_bounded(command, *, cwd, env, timeout=HARD_TIMEOUT,
                stdout_limit=MAX_STDOUT, stderr_limit=MAX_STDERR):
    """Direct invocation waits for GUI-subsystem executables on Windows too.

    Explicit inherited pipe handles preserve redirected output; do not use
    cmd/start, PowerShell invocation, or a shell that detaches a GUI executable.
    Readers retain bounded bytes while draining both pipes concurrently.
    """
    deadline = time.monotonic() + timeout
    try:
        process = subprocess.Popen(
            command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0,
            start_new_session=os.name != "nt")
    except OSError:
        raise SmokeError("could not launch the player") from None
    failed = threading.Event()
    stdout = Drain(process.stdout, stdout_limit, failed)
    stderr = Drain(process.stderr, stderr_limit, failed)
    problem = None
    try:
        while True:
            if failed.is_set():
                problem = "child output exceeded its limit or could not be read"
                break
            if process.poll() is not None and stdout.done.is_set() and stderr.done.is_set():
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                problem = "child or redirected output exceeded the hard deadline"
                break
            failed.wait(min(0.02, remaining))
    finally:
        if problem is not None or process.poll() is None:
            stop_process(process)
        # An inherited pipe must not keep this parent alive after the deadline.
        # Readers are daemons and close their own handles; never close a buffered
        # stream from another thread or perform an unbounded join/communicate.
        for drain in (stdout, stderr):
            drain.thread.join(timeout=0.25)
    if problem is not None:
        raise SmokeError(problem)
    if stdout.overflow or stderr.overflow or stdout.error or stderr.error:
        raise SmokeError("child output exceeded its limit or could not be read")
    if process.returncode != 0:
        raise SmokeError("player did not exit successfully after emitting its report")
    if stderr.data:
        raise SmokeError("player emitted unexpected stderr; contents withheld")
    return bytes(stdout.data)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result


def validate_report(output):
    # Match the complete empty-install schema, not a substring that startup logs
    # or leaked paths could surround. Only elapsed time is allowed to vary.
    if not output.endswith(b"\n") or len(output) > MAX_STDOUT:
        raise SmokeError("diagnostic report is missing, truncated, or oversized")
    lines = output.splitlines()
    if len(lines) != len(STAGES) or any(len(line) > MAX_LINE for line in lines):
        raise SmokeError("diagnostic report has an unexpected record count or size")
    last_elapsed = 0
    for stage, line in zip(STAGES, lines):
        try:
            row = json.loads(line.decode("utf-8"), object_pairs_hook=unique_object)
        except (UnicodeError, ValueError, RecursionError):
            raise SmokeError("diagnostic report contains invalid JSON") from None
        if not isinstance(row, dict):
            raise SmokeError("diagnostic report contains an invalid record")
        elapsed = row.pop("elapsed_ms", None)
        if type(elapsed) is not int or not last_elapsed <= elapsed <= HARD_TIMEOUT * 1000:
            raise SmokeError("diagnostic report contains invalid elapsed time")
        last_elapsed = elapsed
        expected = dict(format=FORMAT, stage=stage, status="unavailable", map=0,
                        tile_x=48, tile_y=32, code="collision.prerequisite",
                        source_stage=None, metrics={}, hashes={}, probe=None,
                        gameplay_admitted=False)
        if stage == "selection":
            expected.update(status="passed", code=None)
        elif stage == "capture":
            expected.update(status="failed", code="vmap.required_member_absent",
                            source_stage="LiquidTypes", metrics={"member_lookups": 1,
                            "read_work_bytes": 0, "complete_raw_products": 0,
                            "adt_attempts": 0})
        elif stage == "complete":
            expected.update(status="failed", code=None,
                            metrics={"completed_probes": 0, "failed_stages": 1})
        # Canonical JSON comparison also rejects bool/number substitutions.
        if json.dumps(row, sort_keys=True) != json.dumps(expected, sort_keys=True):
            raise SmokeError("diagnostic report did not match the redacted empty-install failure")


def snapshot(root):
    result = {}
    for path in root.rglob("*"):
        info = path.lstat()
        if stat.S_ISDIR(info.st_mode):
            digest = None
        elif stat.S_ISREG(info.st_mode):
            hasher = hashlib.sha256()
            with path.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    hasher.update(chunk)
            digest = hasher.hexdigest()
        else:
            raise SmokeError("player created an unexpected filesystem entry")
        result[path.relative_to(root)] = (info.st_mode, info.st_size, info.st_mtime_ns, digest)
    return result


def isolated_environment(root):
    # Do not pass credentials, game paths, app switches, or display sockets into
    # the smoke. Preserve only what the platform's dynamic loader needs.
    keep = {"PATH", "SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "SYSTEMDRIVE",
            "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"}
    env = {key: value for key, value in os.environ.items() if key.upper() in keep}
    locations = {"HOME": "home", "USERPROFILE": "home", "APPDATA": "appdata",
                 "LOCALAPPDATA": "localappdata", "XDG_CONFIG_HOME": "xdg-config",
                 "XDG_DATA_HOME": "xdg-data", "XDG_CACHE_HOME": "xdg-cache",
                 "XDG_STATE_HOME": "xdg-state", "TMPDIR": "temp", "TMP": "temp",
                 "TEMP": "temp"}
    for key, folder in locations.items():
        path = root / folder
        path.mkdir(exist_ok=True)
        env[key] = str(path)
    env.update(WOW_DATA="", PYTHONDONTWRITEBYTECODE="1")
    return env


def check_player(binary):
    binary = Path(binary).resolve()
    if not binary.is_file():
        raise SmokeError("built player executable was not found")
    with tempfile.TemporaryDirectory(prefix="benilla-diagnostic-smoke-") as temporary:
        root = Path(temporary).resolve()
        player_dir = root / "player"
        player_dir.mkdir()
        copied = player_dir / binary.name
        shutil.copy2(binary, copied)
        data = root / "empty-data"
        data.mkdir()
        env = isolated_environment(root)
        before = snapshot(root)
        output = run_bounded([str(copied), "diagnose-collision", "--data", str(data),
                              "--map", "0", "--tile", "48,32",
                              "--timeout-seconds", "10"], cwd=player_dir, env=env)
        if snapshot(root) != before:
            raise SmokeError("player modified the isolated executable, data, or state folders")
        validate_report(output)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        check_player(args.binary)
    except SmokeError as error:
        print(f"Player diagnostic smoke failed: {error}", file=sys.stderr)
        return 1
    except (OSError, ValueError):
        print("Player diagnostic smoke failed: filesystem or process setup error; details withheld",
              file=sys.stderr)
        return 1
    print("Player diagnostic smoke passed: redacted failure report, exit 0, no state writes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
