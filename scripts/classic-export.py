#!/usr/bin/env python3
"""Local read-only native capture and offline validation; never a runnable world importer.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys
import tempfile

from classic_export.capture import capture, validate_bundle
from classic_export.core import (COLUMNS, CORE_REVISION, DATABASE_REVISION, ExportError,
                                 MAX_BUNDLE_BYTES, MAX_SCHEMA_BYTES, Selection,
                                 canonical, check, decode_json, digest, required_tables, schema_profile)


def read_json(path: Path, limit: int):
    with path.open("rb") as handle:
        return decode_json(handle.read(limit+1), limit)


def write_new(path: Path, value) -> None:
    """Publish a private complete file, never overwrite an existing file, even in a race."""
    payload = canonical(value) + b"\n"
    check(len(payload) <= MAX_BUNDLE_BYTES+1, "output.byte_limit")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix=".classic-export-", dir=path.parent, delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        # Same-directory hard link is atomic and refuses an existing destination.
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
    if sys.version_info < (3, 11):
        print("Python 3.11 or later is required", file=sys.stderr)
        return 2
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ["inspect", "capture"]:
        cmd = sub.add_parser(name)
        cmd.add_argument("--database", required=True)
        cmd.add_argument("--user", required=True, help="existing SELECT-only account; never created here")
        cmd.add_argument("--port", type=int, default=3306, help="127.0.0.1 only")
        cmd.add_argument("--output", type=Path, required=True, help="new private local file, preferably under benilla-config")
        if name == "capture":
            cmd.add_argument("--selection", type=Path, required=True)
            cmd.add_argument("--quiescent-snapshot", action="store_true",
                             help="operator confirms no writer can change this already prepared local snapshot")
    cmd = sub.add_parser("plan")
    cmd.add_argument("--schema", type=Path, required=True)
    cmd.add_argument("--selection", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("validate")
    cmd.add_argument("bundle", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "validate":
            bundle = read_json(args.bundle, MAX_BUNDLE_BYTES)
            validate_bundle(bundle)
            print(f"Valid native-source projection; runnable=false; sha256={digest(bundle)}")
            return 0
        check(not args.output.exists(), "output.already_exists")
        if args.command == "plan":
            selection = Selection.parse(read_json(args.selection, 65536))
            schema = read_json(args.schema, MAX_SCHEMA_BYTES)
            minimum = required_tables(selection)
            profiles = schema_profile(schema, minimum)
            output = {"format": "benilla-classic-capture-plan-v2" if selection.starter is not None else "benilla-classic-capture-plan-v1", "selection": selection.json(),
                      "core_revision": CORE_REVISION, "database_revision": DATABASE_REVISION,
                      "schema_sha256": digest(schema), "capture_tables": sorted(minimum),
                      "quiescent_required": any(p["engine"] == "MyISAM" for p in profiles.values()),
                      "runnable": False, "unreviewed_columns": {t: p["unreviewed_columns"] for t,p in profiles.items() if p["unreviewed_columns"]}}
        else:
            # Import/authentication occurs only in an explicit user's local read action.
            selection = Selection.parse(read_json(args.selection, 65536)) if args.command == "capture" else None
            from classic_export.driver import LocalSqlSource
            source = LocalSqlSource(args.database, args.user, args.port)
            try:
                output = source.inspect() if args.command == "inspect" else capture(source, selection, args.quiescent_snapshot)
            finally:
                source.close()
        write_new(args.output, output)
        if args.command == "capture":
            count = sum(len(rows) for rows in output["tables"].values())
            print(f"Captured {count} native rows; {len(output['report']['unsupported'])} blockers; runnable=false")
        else:
            print(f"Wrote {args.command} result; no gameplay activation")
        return 0
    except ExportError as error:
        print(str(error), file=sys.stderr)
        return 2
    except (OSError, ValueError, TypeError, KeyError, RecursionError):
        print("export.failed: no content activated; check local inputs and output permissions", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("export.cancelled: no content activated", file=sys.stderr)
        return 130

if __name__ == "__main__":
    raise SystemExit(main())
