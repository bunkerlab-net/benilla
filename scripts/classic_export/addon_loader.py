"""Bounded selected addon spell lookups, not spell execution or global closure.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 ObjectMgr.cpp:622-658,665-716.
"""
from .core import canonical, check, digest, integer

FORMAT = "benilla-classic-addon-loader-lookups-v1"
FIELDS = ("Id", "CastingTimeIndex", *(f"{name}{i}" for name in
          ("Effect", "EffectImplicitTargetA", "EffectImplicitTargetB") for i in range(1, 4)))
TABLES = ("creature_addon", "creature_template_addon")


def selectors(bundle):
    ids = set()
    for table in TABLES:
        for row in bundle["tables"].get(table, []):
            for part in (row["auras"] or "").split(" "):
                if part and len(part) <= 10 and part.isascii() and part.isdecimal() and 0 < int(part) <= 0xffffffff:
                    ids.add(int(part))
    check(len(ids) <= 512, "addon_loader.selector_limit")
    return sorted(ids)


def evidence(bundle, rows):
    return {"format": FORMAT, "selected_ids": selectors(bundle), "rows": rows,
            "rows_sha256": digest(rows), "strict_log_filter": None,
            "snapshot": {"consistency": bundle["consistency"],
                "selection_sha256": digest(bundle["selection"]),
                "schema_sha256": bundle["schema_sha256"],
                "db_version_sha256": digest(bundle["db_version"]),
                "addon_tables_sha256": {t: digest(bundle["tables"][t]) for t in TABLES}}}


def validate(bundle):
    check(set(TABLES) <= set(bundle["tables"]), "addon_loader.tables_required")
    value = bundle["addon_loader_capture"]
    check(type(value) is dict and type(value.get("rows")) is list and len(value["rows"]) <= 512,
          "addon_loader.rows")
    rows = value["rows"]
    ids = []
    for row in rows:
        check(type(row) is dict and set(row) == set(FIELDS), "addon_loader.row_fields")
        for field in FIELDS:
            integer(row[field], 1 if field == "Id" else 0, 0xffffffff, "addon_loader.numeric_domain")
        ids.append(row["Id"])
        existing = next((s for s in bundle["tables"]["spell_template"] if s["Id"] == row["Id"]), None)
        check(existing is None or all(existing[f] == row[f] for f in FIELDS), "addon_loader.conflicting_spell")
    check(ids == sorted(set(ids)) and set(ids) <= set(selectors(bundle)), "addon_loader.row_selection")
    check(canonical(value) == canonical(evidence(bundle, rows)), "addon_loader.coverage_mismatch")
