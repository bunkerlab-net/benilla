#!/usr/bin/env python3
"""Forward a CI command's output and save a bounded diagnostic tail, preserving its exit."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

LOG_LIMIT = 8 * 1024 * 1024
TAIL_LIMIT = LOG_LIMIT - 256  # Reserve room for the fixed diagnostic header.


def run(command, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    # Verify that the artifact destination is writable before starting the command.
    with path.open("wb") as log:
        try:
            child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        except OSError as error:
            code = 127 if isinstance(error, FileNotFoundError) else 126
            message = f"[ci-log] command could not start; errno: {error.errno}; exit: {code}\n"
            log.write(message.encode("ascii"))
            print(message, end="", file=sys.stderr)
            return code
        with child:
            tail = bytearray()
            total = 0
            console = sys.stdout.fileno()
            while chunk := child.stdout.read1(65536):
                total += len(chunk)
                tail.extend(chunk)
                if len(tail) > TAIL_LIMIT:
                    del tail[:-TAIL_LIMIT]
                if console is not None:
                    try:
                        pending = memoryview(chunk)
                        while pending:
                            pending = pending[os.write(console, pending):]
                    except OSError:
                        # A closed CI console must not close the child's pipe or kill it.
                        console = None
            code = child.wait()
            header = (
                f"[ci-log] child exit: {code}; output bytes: {total}; "
                f"saved tail bytes: {len(tail)}; console closed: {console is None}\n"
            ).encode("ascii")
            try:
                log.write(header)
                log.write(tail)
                log.flush()
            except OSError:
                print("ci-log: cannot save diagnostic output", file=sys.stderr)
                if code == 0:
                    return 1
            # Popen uses negative signal numbers on POSIX; shells use 128 + signal.
            return code if code >= 0 else 128 - code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", required=True, type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("a command is required after --")
    return run(command, args.log)


if __name__ == "__main__":
    sys.exit(main())
