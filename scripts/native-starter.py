#!/usr/bin/env python3
"""Prepare exact native starter bindings from a private v2 capture and local DBCs.

No SQL, network, game-content download or gameplay activation.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys
import tempfile

from classic_export.core import MAX_BUNDLE_BYTES, ExportError, canonical, check, decode_json
from native_starter.compiler import MAX_REPORT_BYTES, Request, compile_starter, verify_preparation
from native_starter.dbc import load_directory, load_extraction_manifest, read_regular


def read_json(path: Path, limit: int):
    return decode_json(read_regular(path, limit), limit)


def write_new(path: Path, value) -> None:
    """Private atomic no-replace publication; no source-file mutation on any path."""
    payload = canonical(value) + b"\n"
    check(len(payload) <= MAX_REPORT_BYTES+1, "output.byte_limit")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix=".native-starter-", dir=path.parent, delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.link(temporary, path)
        if os.name == "posix":
            descriptor = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("prepare", "verify"):
        cmd = sub.add_parser(command)
        cmd.add_argument("--source", type=Path, required=True)
        cmd.add_argument("--dbc-dir", type=Path, required=True,
                         help="completed benilla-native-dbc output directory, including extraction.json")
        if command == "prepare":
            cmd.add_argument("--race", type=int, required=True)
            cmd.add_argument("--class", dest="class_id", type=int, required=True)
            cmd.add_argument("--gender", type=int, required=True)
            cmd.add_argument("--output", type=Path, required=True)
        else:
            cmd.add_argument("preparation", type=Path)
    args = parser.parse_args()
    try:
        source = read_json(args.source, MAX_BUNDLE_BYTES)
        tables, errors = load_directory(args.dbc_dir)
        extraction = load_extraction_manifest(args.dbc_dir, tables)
        if args.command == "verify":
            preparation = read_json(args.preparation, MAX_REPORT_BYTES+1)
            verify_preparation(preparation, source, tables, errors, extraction)
            print("Verified exact preparation against source and local DBC bytes; native gameplay remains blocked")
            return 0
        check(not args.output.resolve().is_relative_to(args.dbc_dir.resolve()), "output.inside_dbc_input_directory")
        check(not args.output.exists(), "output.already_exists")
        request = Request.parse({"race": args.race, "class": args.class_id, "gender": args.gender})
        preparation = compile_starter(source, tables, request, errors, extraction)
        write_new(args.output, preparation)
        print(f"Prepared exact native bindings; {len(preparation['unsupported'])} blockers; "
              f"{len(preparation['required_additional_rows'])} dependency requests; "
              f"sha256={preparation['preparation_sha256']}; no runtime export")
        # A produced report is useful, but must not look like a successful game compile.
        return 3
    except ExportError as error:
        print(str(error), file=sys.stderr)
        return 2
    except (OSError, ValueError, TypeError, KeyError, OverflowError, RecursionError):
        print("compiler.failed: no gameplay activated; check private local inputs and permissions", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("compiler.cancelled: no gameplay activated", file=sys.stderr)
        return 130

if __name__ == "__main__":
    raise SystemExit(main())
