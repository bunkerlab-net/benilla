"""Complete SELECT-only schema inventory, independent of historical box captures.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import json
import re
from pathlib import Path

from .core import CORE_REVISION, DATABASE_REVISION, canonical, check, digest

POLICY_PATH = Path(__file__).with_name("catalog_coverage.json")
FORMAT = "benilla-classic-catalog-schema-v3"
QUERIES = (
    "SELECT TABLE_NAME,ENGINE,TABLE_TYPE,TABLE_COLLATION,AUTO_INCREMENT,CREATE_OPTIONS FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() ORDER BY TABLE_NAME",
    "SELECT TABLE_NAME,COLUMN_NAME,ORDINAL_POSITION,COLUMN_DEFAULT,IS_NULLABLE,DATA_TYPE,COLUMN_TYPE,CHARACTER_MAXIMUM_LENGTH,CHARACTER_OCTET_LENGTH,NUMERIC_PRECISION,NUMERIC_SCALE,DATETIME_PRECISION,CHARACTER_SET_NAME,COLLATION_NAME,EXTRA,GENERATION_EXPRESSION FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() ORDER BY TABLE_NAME,ORDINAL_POSITION",
    "SELECT TABLE_NAME,INDEX_NAME,NON_UNIQUE,SEQ_IN_INDEX,COLUMN_NAME,COLLATION,SUB_PART,NULLABLE,INDEX_TYPE,IGNORED FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=DATABASE() ORDER BY TABLE_NAME,INDEX_NAME,SEQ_IN_INDEX",
    "SELECT TABLE_NAME,VIEW_DEFINITION,CHECK_OPTION,IS_UPDATABLE,SECURITY_TYPE,CHARACTER_SET_CLIENT,COLLATION_CONNECTION FROM information_schema.VIEWS WHERE TABLE_SCHEMA=DATABASE() ORDER BY TABLE_NAME",
)


class CatalogSource:
    """Streaming raw protocol values; PyMySQL converters never round SQL fields."""
    def __init__(self, credentials_file):
        import pymysql
        c = json.loads(Path(credentials_file).read_text())
        check(type(c) is dict and type(c.get("port")) is int and c["port"] == 33306,
              "catalog.connection")
        check(bool(re.fullmatch(r"[A-Za-z0-9_]{1,64}", c["database"])), "catalog.connection")
        self.connection = pymysql.connect(
            host="127.0.0.1", port=c["port"], user=c["reader"], password=c["reader_password"],
            database=c["database"], charset="utf8mb4", autocommit=True, local_infile=False,
            client_flag=0, connect_timeout=5, read_timeout=60, write_timeout=10,
            read_default_file=None, init_command=None)
        self.queries = []

    def metadata(self, sql):
        import pymysql
        check(sql.startswith("SELECT ") and ";" not in sql, "query.not_single_select")
        self.queries.append(sql)
        with self.connection.cursor(pymysql.cursors.DictCursor) as cursor:
            cursor.execute(sql)
            result = []
            while (row := cursor.fetchone()) is not None:
                check(len(result) < 16384, "catalog.schema_limit")
                result.append({k: str(v) if v is not None and not isinstance(v, (str, int)) else v
                               for k, v in row.items()})
            return result

    def rows(self, table):
        check(bool(re.fullmatch(r"[A-Za-z0-9_]{1,64}", table)), "catalog.table")
        yield from self.query_rows("SELECT * FROM `" + table + "`")

    def query_rows(self, sql):
        import pymysql
        check(sql.startswith("SELECT ") and ";" not in sql, "query.not_single_select")
        self.queries.append(sql)
        # Disabling both protocol conversion and text decoding preserves SQL bytes and NULL.
        old_decoders, old_unicode = self.connection.decoders, self.connection.use_unicode
        self.connection.decoders, self.connection.use_unicode = {}, False
        try:
            with self.connection.cursor(pymysql.cursors.SSCursor) as cursor:
                cursor.execute(sql)
                while (row := cursor.fetchone()) is not None:
                    check(all(v is None or type(v) is bytes for v in row), "catalog.raw_field")
                    yield row
        finally:
            self.connection.decoders, self.connection.use_unicode = old_decoders, old_unicode

    def close(self):
        self.connection.close()


def inventory(source):
    tables, columns, indexes, views = (source.metadata(q) for q in QUERIES)
    result = {"format": FORMAT, "core_revision": CORE_REVISION,
              "database_revision": DATABASE_REVISION, "tables": {}}
    for row in tables:
        name = row.pop("TABLE_NAME")
        check(bool(re.fullmatch(r"[A-Za-z0-9_]{1,64}", name)) and name not in result["tables"],
              "catalog.table")
        result["tables"][name] = {"table": row, "columns": [], "indexes": [], "view": None}
    for rows, member in ((columns, "columns"), (indexes, "indexes")):
        for row in rows:
            name = row.pop("TABLE_NAME")
            check(name in result["tables"], "catalog.table")
            result["tables"][name][member].append(row)
    for row in views:
        name = row.pop("TABLE_NAME")
        check(name in result["tables"], "catalog.table")
        result["tables"][name]["view"] = row
    return result


def policy():
    return json.loads(POLICY_PATH.read_text())


def verify_schema(schema, coverage=None):
    p = policy()
    check(type(schema) is dict and set(schema) == {"format", "core_revision", "database_revision", "tables"},
          "catalog.schema")
    check(schema["format"] == FORMAT and schema["core_revision"] == CORE_REVISION
          and schema["database_revision"] == DATABASE_REVISION, "catalog.version")
    tables = schema["tables"]
    check(type(tables) is dict and set(tables) == set(p["tables"]), "catalog.table")
    for name, table in tables.items():
        check(digest(table) == p["tables"][name]["schema_sha256"], "catalog.schema_mismatch")
    if coverage is not None:
        check(type(coverage) is dict and set(coverage) == {"policy_sha256", "tables", "complete"},
              "catalog.coverage")
        check(coverage["policy_sha256"] == digest(p) and set(coverage["tables"]) == set(tables),
              "catalog.coverage")
        for name, entry in coverage["tables"].items():
            check(type(entry) is dict and set(entry) == {"status", "loaders"}, "catalog.coverage")
            check(type(entry["loaders"]) is list and all(x in p["loaders"] for x in entry["loaders"]),
                  "catalog.loader")
            expected = p["tables"][name]
            check(entry == {k: expected[k] for k in ("status", "loaders")}, "catalog.coverage")
        # Inventory, source occurrence verification and owner review are distinct obligations.
        check(coverage["complete"] is False, "catalog.incomplete")
    return {"tables": len(tables), "columns": sum(len(t["columns"]) for t in tables.values()),
            "schema_sha256": digest(schema)}


def coverage_inventory():
    p = policy()
    return {"policy_sha256": digest(p), "complete": False,
            "tables": {name: {k: t[k] for k in ("status", "loaders")}
                       for name, t in p["tables"].items()}}


def pilot(source):
    schema = inventory(source)
    coverage = coverage_inventory()
    verify_schema(schema, coverage)
    rows = [[None if v is None else v.hex() for v in row] for row in source.rows("world_template")]
    check(len(rows) == 2 and sorted(bytes.fromhex(r[0]) for r in rows) == [b"0", b"1"],
          "catalog.pilot")
    return {"format": "benilla-classic-catalog-pilot-v3", "schema": schema, "coverage": coverage,
            "rows": rows, "source_snapshot": "phase0-sequential-diagnostic-not-F1",
            "complete": False}


def verify_pilot(value):
    check(type(value) is dict and set(value) == {"format", "schema", "coverage", "rows", "source_snapshot", "complete"},
          "catalog.pilot")
    check(value["format"] == "benilla-classic-catalog-pilot-v3"
          and value["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
          and value["complete"] is False, "catalog.incomplete")
    result = verify_schema(value["schema"], value["coverage"])
    rows = value["rows"]
    check(type(rows) is list and len(rows) == 2 and all(type(r) is list and len(r) == 2
          and all(type(c) is str and re.fullmatch(r"(?:[0-9a-f]{2})*", c) for c in r) for r in rows),
          "catalog.pilot")
    check(sorted(bytes.fromhex(r[0]) for r in rows) == [b"0", b"1"], "catalog.pilot")
    result.update(pilot_sha256=digest(rows), complete=False)
    return result
