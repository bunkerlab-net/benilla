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
from classic_export import learning, loot, loot_loader
from classic_export.columns import INITIAL_HOOK_SCHEMA
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
    cmd = sub.add_parser("catalog-schema")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--maps", nargs=2, type=int, choices=[0, 1], required=True)
    cmd.add_argument("--pilot", choices=["world_template"], required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("catalog-schema-verify")
    cmd.add_argument("pilot", type=Path)
    cmd = sub.add_parser("catalog-capture")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--dbc-directory", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    for name in ("catalog-verify", "catalog-publish"):
        cmd = sub.add_parser(name)
        cmd.add_argument("root", type=Path)
        cmd.add_argument("--candidate", action="store_true")
        cmd.add_argument("--receipt", type=Path)
        if name == "catalog-publish":
            cmd.add_argument("--rust-receipt", type=Path, required=True)
    cmd = sub.add_parser("catalog-spawn-capture")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("catalog-spawn-verify")
    cmd.add_argument("root", type=Path)
    cmd.add_argument("--occurrences", type=Path, required=True)
    cmd.add_argument("--receipt", type=Path, required=True)
    cmd.add_argument("--build-indexes", action="store_true")
    cmd = sub.add_parser("catalog-group-capture")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("catalog-group-verify")
    cmd.add_argument("root", type=Path)
    cmd.add_argument("--occurrences", type=Path, required=True)
    cmd.add_argument("--receipt", type=Path, required=True)
    cmd = sub.add_parser("catalog-pool-capture")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("catalog-pool-verify")
    cmd.add_argument("root", type=Path)
    cmd.add_argument("--occurrences", type=Path, required=True)
    cmd.add_argument("--receipt", type=Path, required=True)
    cmd.add_argument("--records", type=Path, help="new directory for exact dispositions and hierarchy")
    cmd = sub.add_parser("catalog-event-capture")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("catalog-event-verify")
    cmd.add_argument("root", type=Path)
    cmd.add_argument("--occurrences", type=Path, required=True)
    cmd.add_argument("--initial-snapshot", type=Path, required=True)
    cmd.add_argument("--environment", type=Path, required=True)
    cmd.add_argument("--records", type=Path, required=True)
    cmd.add_argument("--receipt", type=Path, required=True)
    cmd = sub.add_parser("catalog-dbc-verify")
    cmd.add_argument("root", type=Path)
    cmd.add_argument("--receipt", type=Path, required=True)
    cmd.add_argument("--records", type=Path, help="new directory for exact server DBC records")
    cmd.add_argument("--occurrences", type=Path, required=True)
    cmd = sub.add_parser("catalog-transport-capture")
    cmd.add_argument("--credentials-file", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    for name in ["inspect", "capture", "skill-training", "reputation-policy", "global-hooks"]:
        cmd = sub.add_parser(name)
        cmd.add_argument("--database", required=True)
        cmd.add_argument("--user", required=True, help="existing SELECT-only account; never created here")
        cmd.add_argument("--port", type=int, default=3306, help="127.0.0.1 only")
        cmd.add_argument("--output", type=Path, required=True, help="new private local file, preferably under benilla-config")
        if name == "global-hooks":
            cmd.add_argument("--source", type=Path, required=True)
            cmd.add_argument("--build-configuration", type=Path, required=True,
                             help="explicit declared CMaNGOS build switches, not an observed binary")
            cmd.add_argument("--quiescent-snapshot", action="store_true")
        if name in {"skill-training", "reputation-policy"}:
            cmd.add_argument("--source", type=Path, required=True)
            cmd.add_argument("--dbc-dir", type=Path, required=True)
            cmd.add_argument("--gender", type=int, required=True)
            cmd.add_argument("--quiescent-snapshot", action="store_true",
                             help="operator confirms this database has remained unchanged since source capture")
        if name == "capture":
            cmd.add_argument("--selection", type=Path, required=True)
            cmd.add_argument("--dependency-request", type=Path, help="bounded native preparation recapture request")
            cmd.add_argument("--body-loot", action="store_true",
                             help="capture bounded selected body/reference loot dependencies for source-v2")
            cmd.add_argument("--initial-hooks", action="store_true",
                             help="capture selected raw spell_area and spell_proc_event inputs for source-v2")
            cmd.add_argument("--spell-numeric", action="store_true",
                             help="capture complete selected effective spell SQL numeric rows and facing")
            cmd.add_argument("--quest-presentation", action="store_true",
                             help="capture same-snapshot ordinary quest text, greeting and locale inputs")
            cmd.add_argument("--quest-mechanics", action="store_true",
                             help="capture completion scripts, gossip menus, conditions and text dependencies")
            cmd.add_argument("--addon-loader", action="store_true",
                             help="capture selected raw addon spell loader lookups")
            cmd.add_argument("--participants", action="store_true",
                             help="capture conservative non-render participants and runtime creation candidates")
            cmd.add_argument("--quiescent-snapshot", action="store_true",
                             help="operator confirms no writer can change this already prepared local snapshot")
    cmd = sub.add_parser("plan")
    cmd.add_argument("--initial-hooks", action="store_true")
    cmd.add_argument("--body-loot", action="store_true")
    cmd.add_argument("--schema", type=Path, required=True)
    cmd.add_argument("--selection", type=Path, required=True)
    cmd.add_argument("--output", type=Path, required=True)
    cmd = sub.add_parser("validate")
    cmd.add_argument("bundle", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "catalog-event-capture":
            from classic_export.catalog_schema import CatalogSource
            from classic_export.catalog_events import capture_occurrences
            source = CatalogSource(args.credentials_file)
            try:
                result = capture_occurrences(source, args.output)
            finally:
                source.close()
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-event-verify":
            from classic_export.catalog_io import Catalog
            from classic_export.catalog_events import EventViews, initial_state
            state = initial_state(args.initial_snapshot, args.environment)
            with Catalog(args.root) as catalog:
                views = EventViews(catalog)
                result = views.report(views.verify_occurrences(args.occurrences), state, args.records)
            write_new(args.receipt, result)
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-dbc-verify":
            from classic_export.catalog_io import Catalog
            from classic_export.catalog_dbc import DbcViews
            with Catalog(args.root) as catalog:
                result = DbcViews(catalog).report(args.occurrences, args.records)
            write_new(args.receipt, result)
            print(canonical(result).decode())
            return 0
        if args.command in ("catalog-group-capture", "catalog-pool-capture", "catalog-transport-capture"):
            from classic_export.catalog_schema import CatalogSource
            if args.command == "catalog-group-capture":
                from classic_export.catalog_groups import capture_occurrences
            elif args.command == "catalog-transport-capture":
                from classic_export.catalog_transports import capture_occurrences
            else:
                from classic_export.catalog_pools import capture_occurrences
            source = CatalogSource(args.credentials_file)
            try:
                result = capture_occurrences(source, args.output)
            finally:
                source.close()
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-group-verify":
            from classic_export.catalog_io import Catalog
            from classic_export.catalog_groups import GroupViews
            with Catalog(args.root) as catalog:
                views = GroupViews(catalog)
                result = views.report(views.verify_occurrences(args.occurrences))
            write_new(args.receipt, result)
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-pool-verify":
            from classic_export.catalog_io import Catalog
            from classic_export.catalog_spawns import SpawnViews
            from classic_export.catalog_pools import PoolViews
            with Catalog(args.root) as catalog:
                spawns = SpawnViews(catalog)
                result = PoolViews(catalog, spawns).verify_occurrences(args.occurrences, args.records)
            write_new(args.receipt, result)
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-spawn-capture":
            from classic_export.catalog_schema import CatalogSource
            from classic_export.catalog_spawns import capture_occurrences
            source = CatalogSource(args.credentials_file)
            try:
                result = capture_occurrences(source, args.output)
            finally:
                source.close()
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-spawn-verify":
            from classic_export.catalog_io import Catalog
            from classic_export.catalog_spawns import SpawnViews
            with Catalog(args.root) as catalog:
                views = SpawnViews(catalog)
                if args.build_indexes:
                    views.materialize_indexes()
                else:
                    views.verify_indexes()
                result = views.report(views.verify_occurrences(args.occurrences))
            write_new(args.receipt, result)
            print(canonical(result).decode())
            return 0
        if args.command == "catalog-capture":
            from classic_export.catalog_schema import CatalogSource
            from classic_export.catalog_io import capture as catalog_capture
            check(not args.output.exists(), "output.already_exists")
            source = CatalogSource(args.credentials_file)
            try:
                receipt = catalog_capture(source,args.output,args.dbc_directory)
            finally:
                source.close()
            print(canonical({k:v for k,v in receipt.items() if k != "queries"}).decode())
            return 0
        if args.command in {"catalog-verify", "catalog-publish"}:
            from classic_export.catalog_io import Catalog, publish
            with Catalog(args.root,candidate=args.candidate or args.command == "catalog-publish") as catalog:
                report = catalog.report()
                if args.command == "catalog-publish":
                    rust = read_json(args.rust_receipt,1048576)
                    publish(catalog,rust)
            if args.receipt is not None:
                write_new(args.receipt,report)
            print(canonical(report).decode())
            return 0
        if args.command == "catalog-schema-verify":
            from classic_export.catalog_schema import verify_pilot
            print(canonical(verify_pilot(read_json(args.pilot, 1048576))).decode())
            return 0
        if args.command == "catalog-schema":
            from classic_export.catalog_schema import CatalogSource, pilot, verify_pilot, policy
            check(args.maps == [0, 1], "catalog.scope")
            check(not args.output.exists(), "output.already_exists")
            source = CatalogSource(args.credentials_file)
            try:
                output = pilot(source)
            finally:
                source.close()
            args.output.mkdir()
            write_new(args.output / "pilot.json", output)
            write_new(args.output / "loader-ledger.json", policy())
            print(canonical(verify_pilot(output)).decode())
            return 0
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
            if args.initial_hooks:
                check(selection.starter is not None, "initial_hooks.starter_selection_required")
                minimum.update(INITIAL_HOOK_SCHEMA)
            if args.body_loot:
                check(selection.starter is not None, "loot.starter_selection_required")
                check(type(schema) is dict and type(schema.get("tables")) is dict, "schema.tables")
                minimum.update(set(loot.TABLES) & set(schema["tables"]))
            profiles = schema_profile(schema, minimum)
            output = {"format": "benilla-classic-capture-plan-v2" if selection.starter is not None else "benilla-classic-capture-plan-v1", "selection": selection.json(),
                      "core_revision": CORE_REVISION, "database_revision": DATABASE_REVISION,
                      "schema_sha256": digest(schema), "capture_tables": sorted(minimum),
                      "quiescent_required": any(p["engine"] == "MyISAM" for p in profiles.values()),
                      "body_loot_requested": args.body_loot,
                      "body_loot_missing_tables": sorted(set(loot.TABLES) - set(profiles)) if args.body_loot else [],
                      "body_loot_loader_scans": [{"query": loot_loader.Query(table).sql()[0],
                          "row_limit": loot_loader.MAX_SCAN_ROWS, "byte_limit": loot_loader.MAX_SCAN_BYTES}
                          for table in loot.TABLES if table in profiles] if args.body_loot else [],
                      "runnable": False, "unreviewed_columns": {t: p["unreviewed_columns"] for t,p in profiles.items() if p["unreviewed_columns"]}}
        else:
            # Import/authentication occurs only in an explicit user's local read action.
            selection = Selection.parse(read_json(args.selection, 65536)) if args.command == "capture" else None
            dependency_request = None
            if args.command == "capture" and args.dependency_request is not None:
                dependency_request = learning.request(read_json(args.dependency_request, 65536), selection)
            from classic_export.driver import LocalSqlSource
            source = LocalSqlSource(args.database, args.user, args.port)
            try:
                if args.command == "global-hooks":
                    from classic_export import global_hooks
                    bundle = read_json(args.source, MAX_BUNDLE_BYTES)
                    output = global_hooks.recapture(source, bundle, read_json(args.build_configuration, 4096),
                                                    args.quiescent_snapshot)
                elif args.command in {"skill-training", "reputation-policy"}:
                    from classic_export import skill_training
                    from native_starter.compiler import Request
                    from native_starter.dbc import load_directory, load_extraction_manifest, read_regular
                    bundle = decode_json(read_regular(args.source, MAX_BUNDLE_BYTES), MAX_BUNDLE_BYTES)
                    dbcs, errors = load_directory(args.dbc_dir)
                    check(not errors, "skill_training.dbc_errors")
                    load_extraction_manifest(args.dbc_dir, dbcs)
                    starter = bundle["selection"]["starter"]
                    request = Request.parse({"race": starter["race"], "class": starter["class"],
                                             "gender": args.gender})
                    if args.command == "reputation-policy":
                        from classic_export import reputation
                        output = reputation.recapture(source, bundle, dbcs, args.quiescent_snapshot)
                    else:
                        output = skill_training.recapture(source, bundle, dbcs, request, args.quiescent_snapshot)
                else:
                    output = source.inspect() if args.command == "inspect" else capture(source, selection, args.quiescent_snapshot,
                        dependency_request, capture_initial_hooks=args.initial_hooks, capture_body_loot=args.body_loot,
                        capture_spell_numeric=args.spell_numeric, capture_quest_presentation=args.quest_presentation,
                        capture_quest_mechanics=args.quest_mechanics,
                        capture_addon_loader=args.addon_loader,
                        capture_participants=args.participants,
                        capture_quest_relationships=args.quest_presentation)
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
