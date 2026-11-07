#!/usr/bin/env python3
"""Verify one serial, pretty-format libtest run from its stdout, not merged stderr."""

import argparse
from pathlib import Path
import re
import sys


def verify(output: str, names: list[str]) -> None:
    if not names or len(names) != len(set(names)):
        raise ValueError("expected test names must be nonempty and unique")
    count = len(names)
    lines = output.splitlines()
    headers = [i for i, line in enumerate(lines) if line.startswith("running ")]
    summaries = [i for i, line in enumerate(lines) if line.startswith("test result:")]
    header = f"running {count} test{'s' if count != 1 else ''}"
    if len(headers) != 1 or lines[headers[0]] != header:
        raise ValueError(f"expected exactly one {header!r} header")
    if len(summaries) != 1:
        raise ValueError("expected exactly one test result summary")
    start, end = headers[0], summaries[0]
    if any(lines[:start]) or any(lines[end + 1 :]):
        raise ValueError("unexpected stdout outside the libtest run")
    expected = {f"test {name} ... ok" for name in names}
    results = lines[start + 1 : start + 1 + count]
    if end <= start + count or len(results) != count or set(results) != expected:
        raise ValueError("expected each named test to pass exactly once after the header")
    if any(line.startswith("test ") for line in lines[start + count + 1 : end]):
        raise ValueError("unexpected or duplicate test result")
    summary = (
        rf"test result: ok\. {count} passed; 0 failed; 0 ignored; "
        r"0 measured; \d+ filtered out; finished in \d+(?:\.\d+)?s"
    )
    if re.fullmatch(summary, lines[end]) is None:
        raise ValueError("test summary does not prove the exact passing count")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("names", nargs="+")
    args = parser.parse_args()
    try:
        verify(args.log.read_text(encoding="utf-8"), args.names)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"libtest verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
