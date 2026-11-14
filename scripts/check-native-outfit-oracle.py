#!/usr/bin/env python3
"""Required Linux source-oracle check; absent compiler is an error, never a skip.
Only committed original numeric test cases and the pinned standalone C++ excerpt.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def main() -> int:
    root = Path(__file__).resolve().parent / "native_starter"
    fixture = json.loads((root/"outfit_cases.json").read_text())
    compiler = next((found for name in ("c++", "g++", "clang++") if (found := shutil.which(name))), None)
    if compiler is None:
        print("Required C++ source-oracle compiler is unavailable")
        return 2
    with tempfile.TemporaryDirectory() as directory:
        binary = Path(directory)/"outfit-oracle"
        subprocess.run([compiler, "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror",
                        str(root/"outfit_oracle.cpp"), "-o", str(binary)], check=True)
        result = subprocess.run([str(binary)],
                                input="".join(" ".join(map(str, row["input"]))+"\n" for row in fixture["cases"]),
                                text=True, capture_output=True, check=True)
    actual = [int(value) for value in result.stdout.splitlines()]
    expected = [row["quantity"] for row in fixture["cases"]]
    if actual != expected or len(actual) != 378:
        print("Native outfit C++ oracle disagrees with the committed fixture")
        return 1
    print("Verified all 378 committed outfit cases against the C++ source oracle")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
