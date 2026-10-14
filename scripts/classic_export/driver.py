"""Optional local PyMySQL adapter. Imported only by the user's inspect/capture command.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import getpass
import json
import warnings
import re
from typing import Any

from .columns import COLUMNS
from .core import (ExportError, MAX_ROW_BYTES, MAX_SCHEMA_BYTES, MAX_TABLE_BYTES, MAX_BUNDLE_BYTES, canonical, check,
                   integer, normalize_version_notes, schema_profile)
from .capture import Query

class LocalSqlSource:
    def __init__(self, database: str, user: str, port: int = 3306):
        check(bool(re.fullmatch(r"[A-Za-z0-9_]{1,64}", database)), "connection.database_name")
        check(type(user) is str and bool(re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", user)), "connection.user_name")
        integer(port, 1, 65535, "connection.port")
        try:
            import pymysql
        except ImportError:
            raise ExportError("connection.install_official_pymysql_locally") from None
        try:
            with warnings.catch_warnings():
                warnings.simplefilter("error", getpass.GetPassWarning)
                password = getpass.getpass("Local SELECT-only database password (not saved): ")
        except (getpass.GetPassWarning, EOFError):
            raise ExportError("connection.secure_terminal_required") from None
        try:
            self.connection = pymysql.connect(
                host="127.0.0.1", port=port, database=database, user=user, password=password,
                charset="utf8mb4", cursorclass=pymysql.cursors.SSDictCursor,
                connect_timeout=5, read_timeout=10, write_timeout=10,
                autocommit=False, local_infile=False, client_flag=0,
                read_default_file=None, init_command=None)
        except Exception:
            raise ExportError("connection.failed") from None
        finally:
            password = None  # No persistence/logging; Python does not guarantee memory zeroization.

    def _select(self, sql: str, params: tuple[Any, ...], limit: int) -> list[dict[str, Any]]:
        # Only internal compiled statements reach this private method.
        check(sql.startswith("SELECT ") and ";" not in sql, "query.not_single_select")
        try:
            with self.connection.cursor() as cursor:
                cursor.execute(sql, params)
                rows = []
                total_bytes = 0
                while True:
                    row = cursor.fetchone()
                    if row is None:
                        break
                    check(len(rows) < limit, "query.row_limit")
                    # Bound retained response bytes incrementally, before appending another row.
                    size = len(json.dumps(row, ensure_ascii=False, separators=(",", ":"),
                                          default=str, allow_nan=False).encode("utf-8"))
                    check(size <= MAX_ROW_BYTES, "query.row_byte_limit")
                    total_bytes += size
                    check(total_bytes <= MAX_TABLE_BYTES, "query.table_byte_limit")
                    self._captured_bytes = getattr(self, "_captured_bytes", 0) + size
                    check(self._captured_bytes <= MAX_BUNDLE_BYTES, "query.aggregate_byte_limit")
                    rows.append(row)
                return rows
        except ExportError:
            raise
        except Exception:
            raise ExportError("query.failed") from None

    def inspect(self) -> dict[str, Any]:
        names = tuple(sorted(COLUMNS))
        placeholders = ",".join(["%s"]*len(names))
        tables = self._select(
            "SELECT TABLE_NAME AS name, ENGINE AS engine, TABLE_TYPE AS table_type "
            "FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME IN ("
            + placeholders + ") ORDER BY TABLE_NAME LIMIT %s", names + (len(names)+1,), len(names)+1)
        check(len(tables) <= len(names), "schema.table_limit")
        columns = self._select(
            "SELECT TABLE_NAME AS table_name,COLUMN_NAME AS name,COLUMN_TYPE AS type,"
            "IS_NULLABLE AS nullable,ORDINAL_POSITION AS ordinal FROM information_schema.COLUMNS "
            "WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME IN (" + placeholders + ") "
            "ORDER BY TABLE_NAME,ORDINAL_POSITION LIMIT 4097", names, 4097)
        check(len(columns) <= 4096, "schema.column_limit")
        result: dict[str, Any] = {"format_version": 1, "tables": {}}
        for row in tables:
            check(row["name"] in COLUMNS and row["name"] not in result["tables"], "schema.table_name")
            result["tables"][row["name"]] = {"engine": row["engine"], "table_type": row["table_type"], "columns": []}
        for row in columns:
            check(row["table_name"] in result["tables"], "schema.column_table")
            check(row["nullable"] in {"YES", "NO"}, "schema.nullability")
            result["tables"][row["table_name"]]["columns"].append({
                "name": row["name"], "type": row["type"], "nullable": row["nullable"] == "YES", "ordinal": row["ordinal"]})
        if "spell_scripts" in result["tables"]:
            # Exact current registry primary key; never infer it from row uniqueness.
            keys = self._select(
                "SELECT COLUMN_NAME AS name,ORDINAL_POSITION AS ordinal "
                "FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA=DATABASE() "
                "AND TABLE_NAME=%s AND CONSTRAINT_NAME=%s ORDER BY ORDINAL_POSITION LIMIT 3",
                ("spell_scripts", "PRIMARY"), 3)
            check(len(keys) <= 2 and all(type(row) is dict and set(row) == {"name", "ordinal"}
                  and row["ordinal"] == index for index, row in enumerate(keys, 1)),
                  "schema.registry_key_metadata")
            result["tables"]["spell_scripts"]["primary_key"] = [row["name"] for row in keys]
        from .initial_hooks import PRIMARY_KEYS
        for table, expected in PRIMARY_KEYS.items():
            if table not in result["tables"]:
                continue
            keys = self._select(
                "SELECT COLUMN_NAME AS name,ORDINAL_POSITION AS ordinal "
                "FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA=DATABASE() "
                "AND TABLE_NAME=%s AND CONSTRAINT_NAME=%s ORDER BY ORDINAL_POSITION LIMIT %s",
                (table, "PRIMARY", len(expected) + 1), len(expected) + 1)
            check(len(keys) <= len(expected) and all(type(row) is dict and set(row) == {"name", "ordinal"}
                  and row["ordinal"] == index for index, row in enumerate(keys, 1)),
                  "schema.initial_hook_key_metadata")
            result["tables"][table]["primary_key"] = [row["name"] for row in keys]
        check(len(canonical(result)) <= MAX_SCHEMA_BYTES, "schema.byte_limit")
        profiles = schema_profile(result, set())
        self._version_profile = profiles.get("db_version")
        return result

    def start_read(self, engines: set[str], quiescent: bool) -> str:
        check(type(quiescent) is bool, "snapshot.quiescent_flag")
        check(engines <= {"InnoDB", "MyISAM"}, "snapshot.engine")
        check("MyISAM" not in engines or quiescent, "snapshot.quiescent_required_for_myisam")
        try:
            self.connection.rollback()  # End any metadata-read transaction before setting characteristics.
            with self.connection.cursor() as cursor:
                cursor.execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                cursor.execute("START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY")
        except Exception:
            raise ExportError("snapshot.readonly_transaction_failed") from None
        return "operator_quiescent_snapshot" if quiescent else "transactional_snapshot"

    def fetch(self, query: Query) -> list[dict[str, Any]]:
        sql, params = query.sql()
        return self._select(sql, params, query.limit+1)

    def version_notes(self) -> list[dict[str, str | None]]:
        profile = getattr(self, "_version_profile", None)
        check(profile is not None and set(profile["columns"]) == set(COLUMNS["db_version"]),
              "schema.db_version_inspection_required")
        rows = self._select("SELECT version,creature_ai_version FROM db_version LIMIT 2", (), 2)
        return normalize_version_notes(rows, profile)

    def finish_read(self) -> None:
        try:
            self.connection.rollback()
        except Exception:
            raise ExportError("snapshot.rollback_failed") from None

    def close(self) -> None:
        self.connection.close()
