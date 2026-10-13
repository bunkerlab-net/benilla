"""Bounded native SQL projection, deliberately not a gameplay compiler.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal
from typing import Any, Protocol

from .columns import COLUMNS, STARTER_SCHEMA
from . import event_ai, starter, learning, native_spawn, spell_registry
from .core import (CORE_REVISION, DATABASE_REVISION, FORMAT, STARTER_FORMAT, KEYS, MAX_BUNDLE_BYTES,
                   MAX_ITEMS, MAX_QUESTS, MAX_ROWS, MAX_SCHEMA_BYTES, MAX_SPELLS, Selection,
                   canonical, check, digest, integer, normalize_rows, normalize_version_notes,
                   referenced_items, report, require_ids, required_tables, schema_profile)

@dataclass(frozen=True)
class Query:
    table: str
    mode: str
    ids: tuple[int, ...] = ()
    selection: Selection | None = None
    limit: int = MAX_ROWS

    def sql(self) -> tuple[str, tuple[Any, ...]]:
        check(self.table in COLUMNS and self.table in KEYS, "query.unreviewed_table")
        check(self.table != event_ai.TABLE or self.mode == "event_ai_selectors",
              "query.event_ai_mode")
        check(self.table not in native_spawn.TABLES or self.mode == "native_spawn_selectors",
              "query.native_spawn_mode")
        check(self.table != spell_registry.TABLE or self.mode == "spell_registry_selectors",
              "query.spell_registry_mode")
        integer(self.limit, 1, MAX_ROWS, "query.limit")
        columns = ",".join(f"`{name}` AS `{name}`" for name in COLUMNS[self.table])
        order = ",".join(f"`{name}`" for name in KEYS[self.table])
        if self.table in STARTER_SCHEMA:
            check(self.mode == "starter" and self.selection is not None and not self.ids,
                  "query.starter")
            selected = Selection.parse(self.selection.json()).starter
            check(selected is not None, "query.starter_selection")
            check(self.limit <= starter.STARTER_LIMITS[self.table], "query.starter_limit")
            where, params = starter.predicate(self.table, selected)
        elif self.mode == "event_ai_selectors":
            check(self.table == event_ai.TABLE and self.selection is None
                  and 0 < len(self.ids) <= event_ai.MAX_SELECTORS, "query.event_ai_selectors")
            check(len(set(self.ids)) == len(self.ids), "query.duplicate_ids")
            for value in self.ids:
                integer(value, -0x7fffffff, 0x7fffffff, "query.event_ai_selector")
                check(value != 0, "query.event_ai_zero_selector")
            where = f"`creature_id` IN ({','.join(['%s']*len(self.ids))})"
            params = self.ids
        elif self.mode == "native_spawn_selectors":
            check(self.table in native_spawn.TABLES and self.selection is None
                  and 0 < len(self.ids) <= MAX_ROWS, "query.native_spawn_selectors")
            check(len(set(self.ids)) == len(self.ids), "query.duplicate_ids")
            for value in self.ids:
                integer(value, 0 if self.table == "creature_spawn_data_template" else 1,
                        0xffffffff, "query.native_spawn_selector")
            field = native_spawn.SELECTOR_COLUMNS[self.table]
            where = f"`{field}` IN ({','.join(['%s']*len(self.ids))})"
            params = self.ids
        elif self.mode == "spell_registry_selectors":
            check(self.table == spell_registry.TABLE and self.selection is None
                  and 0 < len(self.ids) <= MAX_SPELLS and self.limit <= MAX_SPELLS,
                  "query.spell_registry_selectors")
            check(len(set(self.ids)) == len(self.ids), "query.duplicate_ids")
            for value in self.ids:
                integer(value, 1, 0x7fffffff, "spell_registry.selector_domain")
            where = f"`Id` IN ({','.join(['%s']*len(self.ids))})"
            params = self.ids
        elif self.mode == "learning_presence":
            check(self.table == "spell_learn_spell" and self.selection is None and not self.ids
                  and self.limit == 1, "query.learning_presence")
            where, params = "1=1", ()
        elif self.mode == "region":
            check(self.table == "creature" and self.selection is not None and not self.ids, "query.region")
            # Revalidate programmatic callers too.
            s = Selection.parse(self.selection.json())
            where = "`map`=%s AND `position_x` BETWEEN %s AND %s AND `position_y` BETWEEN %s AND %s AND `position_z` BETWEEN %s AND %s"
            params: tuple[Any, ...] = (s.map_id,)
            for low, high in zip(s.minimum, s.maximum):
                params += (Decimal(low)/1000, Decimal(high)/1000)
        else:
            check(self.selection is None and 0 < len(self.ids) <= MAX_ROWS, "query.ids")
            check(len(set(self.ids)) == len(self.ids), "query.duplicate_ids")
            for value in self.ids:
                integer(value, 1, 0xffffffff, "query.id")
            if self.mode == "ids":
                field = KEYS[self.table][0]
            elif self.mode == "quest_ids":
                check(self.table in {"creature_questrelation", "creature_involvedrelation"}, "query.quest_ids")
                field = "quest"
            else:
                raise ValueError("query.unreviewed_mode")
            where = f"`{field}` IN ({','.join(['%s']*len(self.ids))})"
            params = self.ids
        return f"SELECT {columns} FROM `{self.table}` WHERE {where} ORDER BY {order} LIMIT %s", params + (self.limit if self.mode == "learning_presence" else self.limit+1,)

class ReadSource(Protocol):
    def inspect(self) -> dict[str, Any]: ...
    def start_read(self, engines: set[str], quiescent: bool) -> str: ...
    def fetch(self, query: Query) -> list[dict[str, Any]]: ...
    def version_notes(self) -> list[dict[str, str | None]]: ...
    def finish_read(self) -> None: ...


def capture(source: ReadSource, selection: Selection, quiescent: bool = False,
            dependency_request: dict[str, Any] | None = None) -> dict[str, Any]:
    selection = Selection.parse(selection.json())
    if dependency_request is not None:
        learning.request(dependency_request, selection)
    inventory = source.inspect()
    minimum = required_tables(selection)
    profiles = schema_profile(inventory, minimum)
    engines = {p["engine"] for p in profiles.values()}
    tables: dict[str, list[dict[str, Any]]] = {}
    required: dict[str, set[int]] = {}
    try:
        mode = source.start_read(engines, quiescent)
        check(mode in {"transactional_snapshot", "operator_quiescent_snapshot"}, "snapshot.mode")
        fetched_bytes = len(canonical(inventory))
        def account(rows):
            nonlocal fetched_bytes
            fetched_bytes += len(canonical(rows))
            check(fetched_bytes <= MAX_BUNDLE_BYTES, "capture.aggregate_byte_limit")
            return rows
        version_before = normalize_version_notes(source.version_notes(), profiles["db_version"])
        def get(table: str, ids: set[int], limit: int = MAX_ROWS, mode: str = "ids") -> list[dict[str, Any]]:
            check(len(ids) <= limit, f"closure.limit:{table}")
            if not ids:
                return []
            check(table in profiles, f"schema.missing_table:{table}")
            query = Query(table, mode, tuple(sorted(ids)), limit=limit)
            query.sql()  # Validate even fake drivers and non-SQL consumers.
            rows = source.fetch(query)
            return account(normalize_rows(table, rows, profiles[table], limit))

        if selection.starter is not None:
            for table, limit in starter.STARTER_LIMITS.items():
                query = Query(table, "starter", selection=selection, limit=limit)
                query.sql()
                tables[table] = account(normalize_rows(table, source.fetch(query), profiles[table], limit))
            starter.validate_rows(tables, selection.starter)
        starter_items, starter_spells = starter.references(tables)

        spawns_query = Query("creature", "region", selection=selection)
        spawns_query.sql()
        spawns = account(normalize_rows("creature", source.fetch(spawns_query), profiles["creature"], MAX_ROWS))
        tables["creature"] = spawns
        creature_ids = {r["id"] for r in spawns if r["id"] > 0}
        quest_ids = set(selection.quest_ids)
        for table in ["creature_questrelation", "creature_involvedrelation"]:
            rows = get(table, creature_ids)
            tables[table] = rows
            quest_ids.update(r["quest"] for r in rows)
        quests = get("quest_template", quest_ids, MAX_QUESTS)
        require_ids("quest_template", quests, quest_ids)
        tables["quest_template"] = quests
        required["quest_template"] = quest_ids
        for table in ["creature_questrelation", "creature_involvedrelation"]:
            rows = get(table, quest_ids, mode="quest_ids")
            combined = {tuple(r[k] for k in KEYS[table]): r for r in tables[table]}
            for row in rows:
                key = tuple(row[k] for k in KEYS[table])
                check(key not in combined or combined[key] == row, "snapshot.conflicting_rows")
                combined[key] = row
            check(len(combined) <= MAX_ROWS, "closure.relations_limit")
            tables[table] = sorted(combined.values(), key=lambda r: tuple(r[k] for k in KEYS[table]))
            creature_ids.update(r["id"] for r in rows)
        for quest in quests:
            for i in range(1, 5):
                entry = quest[f"ReqCreatureOrGOId{i}"]
                if entry > 0:
                    creature_ids.add(entry)
        creatures: dict[int, dict[str, Any]] = {}
        for _ in range(16):
            missing = creature_ids - set(creatures)
            if not missing:
                break
            rows = get("creature_template", missing)
            require_ids("creature_template", rows, missing)
            creatures.update((r["Entry"], r) for r in rows)
            for row in rows:
                creature_ids.update(row[k] for k in ["KillCredit1", "KillCredit2"] if row[k] > 0)
            check(len(creature_ids) <= MAX_ROWS, "closure.creature_limit")
        check(creature_ids == set(creatures), "closure.creature_depth")
        tables["creature_template"] = [creatures[k] for k in sorted(creatures)]
        required["creature_template"] = creature_ids
        if selection.starter is not None:
            selected = tuple(event_ai.selectors(tables))
            query = Query(event_ai.TABLE, "event_ai_selectors", selected)
            if selected:
                query.sql()
            # An empty selector set is complete without asking SQL for IN ().
            rows = source.fetch(query) if selected else []
            tables[event_ai.TABLE] = account(normalize_rows(event_ai.TABLE, rows,
                                                            profiles[event_ai.TABLE], MAX_ROWS))
        if selection.starter is not None:
            # These reads share the same transaction/quiescent snapshot and budgets
            # as creature rows. Read all rows, including neutral-looking hooks.
            tables["creature_spawn_data"] = get("creature_spawn_data", {r["guid"] for r in spawns},
                                                mode="native_spawn_selectors")
            for table, ids in native_spawn.selectors(tables).items():
                if table != "creature_spawn_data":
                    tables[table] = get(table, set(ids), mode="native_spawn_selectors")
        model_ids = native_spawn.model_roots(tables)
        models: dict[int, dict[str, Any]] = {}
        # Every pass visits a new ID; recursive gender chains and cycles terminate
        # at a bounded visited set, never an arbitrary depth cutoff.
        for _ in range(MAX_ROWS + 1):
            missing = model_ids - set(models)
            if not missing:
                break
            rows = get("creature_model_info", missing)
            require_ids("creature_model_info", rows, missing)
            models.update((row["modelid"], row) for row in rows)
            if selection.starter is not None:
                model_ids.update(integer(row["modelid_other_gender"], 0, 0xffffffff,
                                         "native_spawn.model_domain") for row in rows
                                 if row["modelid_other_gender"])
            check(len(model_ids) <= MAX_ROWS, "native_spawn.model_limit")
        check(model_ids == set(models), "native_spawn.model_closure")
        tables["creature_model_info"] = [models[key] for key in sorted(models)]
        required["creature_model_info"] = model_ids
        class_ids = {r["UnitClass"] for r in creatures.values() if r["UnitClass"] > 0}
        # Fetch the small reviewed class table, then retain only referenced levels.
        stats = get("creature_template_classlevelstats", class_ids)
        for creature in creatures.values():
            low = integer(creature["MinLevel"], 1, 65, "creature.min_level")
            integer(creature["MaxLevel"], low, 65, "creature.max_level")
            integer(creature["UnitClass"], 0, 8, "creature.unit_class")
        needed_stats = {(r["UnitClass"], level) for r in creatures.values()
                        for level in range(r["MinLevel"], r["MaxLevel"]+1)}
        stats = [r for r in stats if (r["Class"], r["Level"]) in needed_stats]
        tables["creature_template_classlevelstats"] = stats
        item_ids = referenced_items(quests) | starter_items
        if dependency_request is not None:
            item_ids.update(dependency_request["item_ids"])
        items = get("item_template", item_ids, MAX_ITEMS)
        require_ids("item_template", items, item_ids)
        tables["item_template"] = items
        required["item_template"] = item_ids
        spell_ids = set(selection.spell_ids) | starter_spells
        if dependency_request is not None:
            spell_ids.update(dependency_request["spell_ids"])
        for quest in quests:
            spell_ids.update(quest[k] for k in ["SrcSpell", "RewSpell", "RewSpellCast"] if quest[k] > 0)
        for item in items:
            spell_ids.update(item[f"spellid_{i}"] for i in range(1,6) if item[f"spellid_{i}"] > 0)
            if selection.starter is not None and item["requiredspell"]:
                spell_ids.add(integer(item["requiredspell"], 1, 0xffffffff, "reference.required_spell"))
        spells: dict[int, dict[str, Any]] = {}
        global_learning_rows = []
        if selection.starter is not None:
            # Native LoadSpellLearnSpells exits before its effect scan if the
            # entire SQL table is empty. Selected-empty rows cannot prove that.
            query = Query("spell_learn_spell", "learning_presence", limit=1)
            query.sql()
            global_learning_rows = account(normalize_rows("spell_learn_spell", source.fetch(query),
                                                          profiles["spell_learn_spell"], 1))
        if selection.starter is not None:
            tables.update({table: [] for table in learning.TABLES})
        # Each productive pass consumes a previously unseen spell. Cycles terminate
        # at the visited set, not at an arbitrary sixteen-edge truncation.
        for _ in range(MAX_SPELLS + 1):
            missing = spell_ids - set(spells)
            if not missing:
                break
            rows = get("spell_template", missing, MAX_SPELLS)
            require_ids("spell_template", rows, missing)
            spells.update((r["Id"], r) for r in rows)
            if selection.starter is not None:
                for table in learning.TABLES:
                    tables[table].extend(get(table, missing))
                    check(len(tables[table]) <= MAX_ROWS, "learning.row_limit")
                spell_ids.update(learning.references(tables))
            for row in rows:
                spell_ids.update(row[f"EffectTriggerSpell{i}"] for i in range(1,4) if row[f"EffectTriggerSpell{i}"] > 0)
            check(len(spell_ids) <= MAX_SPELLS, "closure.spell_limit")
        check(spell_ids == set(spells), "closure.spell_depth")
        if selection.starter is not None:
            for table in learning.TABLES:
                tables[table].sort(key=lambda r: tuple(r[k] for k in KEYS[table]))
        if spells or selection.starter is not None:
            tables["spell_template"] = [spells[k] for k in sorted(spells)]
            required["spell_template"] = spell_ids
        if selection.starter is not None:
            registry_ids = set(spell_registry.selected_ids(tables))
            tables[spell_registry.TABLE] = get(spell_registry.TABLE, registry_ids, MAX_SPELLS,
                                               mode="spell_registry_selectors")
        check(version_before == normalize_version_notes(source.version_notes(), profiles["db_version"]), "snapshot.version_changed")
        findings = full_report(tables, profiles, selection)
        bundle = {
            "format": STARTER_FORMAT if selection.starter is not None else FORMAT, "core_revision": CORE_REVISION, "database_revision": DATABASE_REVISION,
            "revision_claims_verified_against_installation": False, "client_build": 5875,
            "selection": selection.json(), "consistency": mode, "schema": inventory,
            "schema_sha256": digest(inventory), "db_version": version_before,
            "tables": tables, "table_sha256": {k: digest(v) for k, v in tables.items()},
            "required_ids": {k: sorted(v) for k, v in required.items()},
            "report": {"runnable": False, "capture_complete": False,
                       "scope": "selected native projection; required server mechanics remain unimplemented",
                       "unsupported": sorted(findings, key=lambda x: (x["table"], str(x["key"]), x["reason"]))},
        }
        if selection.starter is not None:
            bundle["starter_required_keys"] = starter.keys(tables)
            bundle["event_ai_capture"] = event_ai.evidence(tables)
            bundle["native_spawn_capture"] = native_spawn.evidence(bundle)
            bundle["spell_script_capture"] = spell_registry.evidence(bundle)
            bundle["learning_capture"] = learning.evidence(tables, dependency_request, global_learning_rows)
        check(len(canonical(bundle)) <= MAX_BUNDLE_BYTES, "bundle.byte_limit")
        validate_bundle(bundle)
        return bundle
    finally:
        source.finish_read()


def validate_bundle(bundle: Any) -> None:
    expected = {"format", "core_revision", "database_revision", "revision_claims_verified_against_installation",
                "client_build", "selection", "consistency", "schema", "schema_sha256", "db_version", "tables",
                "table_sha256", "required_ids", "report"}
    check(type(bundle) is dict, "bundle.fields")
    if bundle.get("format") == STARTER_FORMAT:
        expected.add("starter_required_keys")
        if "event_ai_capture" in bundle:
            expected.add("event_ai_capture")
        if "learning_capture" in bundle:
            expected.add("learning_capture")
        if "native_spawn_capture" in bundle:
            expected.add("native_spawn_capture")
        if "spell_script_capture" in bundle:
            expected.add("spell_script_capture")
    check(set(bundle) == expected, "bundle.fields")
    check(bundle["format"] in {FORMAT, STARTER_FORMAT} and bundle["core_revision"] == CORE_REVISION
          and bundle["database_revision"] == DATABASE_REVISION and bundle["client_build"] == 5875,
          "bundle.profile")
    check(bundle["revision_claims_verified_against_installation"] is False, "bundle.unverified_provenance")
    selection = Selection.parse(bundle["selection"])
    check(bundle["format"] == (STARTER_FORMAT if selection.starter is not None else FORMAT),
          "bundle.selection_version")
    check(bundle["consistency"] in {"transactional_snapshot", "operator_quiescent_snapshot"}, "bundle.consistency")
    check(type(bundle["tables"]) is dict and set(bundle["tables"]) <= set(KEYS), "bundle.tables")
    check(selection.starter is not None or not (set(bundle["tables"]) & set(STARTER_SCHEMA)),
          "bundle.unselected_starter_tables")
    mandatory = {"creature", "creature_template", "creature_model_info", "creature_template_classlevelstats",
                 "creature_questrelation", "creature_involvedrelation", "quest_template", "item_template"}
    if selection.starter is not None:
        mandatory.update(STARTER_SCHEMA)
        mandatory.add("spell_template")
    check(mandatory <= set(bundle["tables"]), "bundle.missing_tables")
    check(len(canonical(bundle["schema"])) <= MAX_SCHEMA_BYTES, "bundle.schema_byte_limit")
    profiles = schema_profile(bundle["schema"], set(bundle["tables"]) | {"db_version"})
    check(bundle["consistency"] == "operator_quiescent_snapshot" or all(p["engine"] == "InnoDB" for p in profiles.values()), "bundle.myisam_consistency")
    check(digest(bundle["schema"]) == bundle["schema_sha256"], "bundle.schema_hash")
    check(type(bundle["table_sha256"]) is dict and set(bundle["table_sha256"]) == set(bundle["tables"]), "bundle.table_hashes")
    for table, rows in bundle["tables"].items():
        check(type(rows) is list, "bundle.rows")
        # Convert tagged native decimals back for the same typed validator.
        decoded = [{k: (v["decimal"] if type(v) is dict and set(v) == {"decimal"} else v)
                    for k, v in row.items()} for row in rows if type(row) is dict]
        check(len(decoded) == len(rows), "bundle.row_type")
        normal = normalize_rows(table, decoded, profiles[table],
                                {"quest_template": MAX_QUESTS, "item_template": MAX_ITEMS,
                                 "spell_template": MAX_SPELLS, **starter.STARTER_LIMITS}.get(table, MAX_ROWS))
        check(normal == rows and digest(rows) == bundle["table_sha256"][table], "bundle.table_hash_or_order")
    if selection.starter is not None:
        starter.validate_rows(bundle["tables"], selection.starter)
        check(canonical(bundle["starter_required_keys"]) == canonical(starter.keys(bundle["tables"])), "bundle.starter_keys")
    check((event_ai.TABLE in bundle["tables"]) == ("event_ai_capture" in bundle),
          "event_ai.coverage_required_with_table")
    if "event_ai_capture" in bundle:
        check(selection.starter is not None, "event_ai.starter_selection_required")
        event_ai.validate(bundle["event_ai_capture"], bundle["tables"])
    check(bool(set(learning.TABLES) & set(bundle["tables"])) == ("learning_capture" in bundle),
          "learning.coverage_required_with_tables")
    if "learning_capture" in bundle:
        check(selection.starter is not None, "learning.starter_selection_required")
        learning.validate(bundle["learning_capture"], bundle["tables"], selection, profiles["spell_learn_spell"])
    check(bool(set(native_spawn.TABLES) & set(bundle["tables"])) == ("native_spawn_capture" in bundle),
          "native_spawn.coverage_required_with_tables")
    if "native_spawn_capture" in bundle:
        check(selection.starter is not None, "native_spawn.starter_selection_required")
        native_spawn.validate(bundle)
    check((spell_registry.TABLE in bundle["tables"]) == ("spell_script_capture" in bundle),
          "spell_registry.coverage_required_with_table")
    if "spell_script_capture" in bundle:
        check(selection.starter is not None, "spell_registry.starter_selection_required")
        spell_registry.validate(bundle)
    check(type(bundle["required_ids"]) is dict and set(bundle["required_ids"]) <= set(bundle["tables"]), "bundle.references")
    reference_tables = {"creature_template", "creature_model_info", "quest_template", "item_template"}
    if "spell_template" in bundle["tables"]:
        reference_tables.add("spell_template")
    check(set(bundle["required_ids"]) == reference_tables, "bundle.required_reference_tables")
    for table, ids in bundle["required_ids"].items():
        check(type(ids) is list and len(ids) == len(set(ids)), "bundle.reference_ids")
        for value in ids:
            integer(value, 1, 0xffffffff, "bundle.reference_id")
        require_ids(table, bundle["tables"][table], set(ids))
    check(normalize_version_notes(bundle["db_version"], profiles["db_version"]) == bundle["db_version"], "bundle.db_version")
    creatures = {r["Entry"] for r in bundle["tables"]["creature_template"]}
    quests = {r["entry"] for r in bundle["tables"]["quest_template"]}
    items = {r["entry"] for r in bundle["tables"]["item_template"]}
    spells = {r["Id"] for r in bundle["tables"].get("spell_template", [])}
    starter_items, starter_spells = starter.references(bundle["tables"])
    check(starter_items <= items, "bundle.starter_item_reference")
    check(starter_spells <= spells, "bundle.starter_spell_reference")
    check(set(selection.quest_ids) <= quests and set(selection.spell_ids) <= spells, "bundle.selected_ids_missing")
    for row in bundle["tables"]["creature"]:
        check(row["id"] == 0 or row["id"] in creatures, "bundle.spawn_reference")
        check(row["map"] == selection.map_id, "bundle.map")
        for field, low, high in zip(["position_x", "position_y", "position_z"], selection.minimum, selection.maximum):
            value = Decimal(row[field]["decimal"])
            check(Decimal(low)/1000 <= value <= Decimal(high)/1000, "bundle.outside_region")
    for table in ["creature_questrelation", "creature_involvedrelation"]:
        check(all(r["id"] in creatures and r["quest"] in quests for r in bundle["tables"][table]), "bundle.quest_relation_reference")
    for row in bundle["tables"]["quest_template"]:
        check(all(row[f"ReqCreatureOrGOId{i}"] <= 0 or row[f"ReqCreatureOrGOId{i}"] in creatures for i in range(1,5)), "bundle.objective_reference")
    check(referenced_items(bundle["tables"]["quest_template"]) <= items, "bundle.quest_item_reference")
    models = {r["modelid"] for r in bundle["tables"]["creature_model_info"]}
    for row in bundle["tables"]["creature_template"]:
        low = integer(row["MinLevel"], 1, 65, "creature.min_level")
        integer(row["MaxLevel"], low, 65, "creature.max_level")
        check(all(row[f"DisplayId{i}"] == 0 or row[f"DisplayId{i}"] in models for i in range(1,5)), "bundle.model_reference")
        check(all(row[k] == 0 or row[k] in creatures for k in ["KillCredit1", "KillCredit2"]), "bundle.kill_credit_reference")
    for row in bundle["tables"]["quest_template"]:
        check(all(row[k] == 0 or row[k] in spells for k in ["SrcSpell", "RewSpell", "RewSpellCast"]), "bundle.quest_spell_reference")
    for row in bundle["tables"]["item_template"]:
        if selection.starter is not None:
            check(row["requiredspell"] == 0 or row["requiredspell"] in spells, "bundle.item_required_spell_reference")
        check(all(row[f"spellid_{i}"] <= 0 or row[f"spellid_{i}"] in spells for i in range(1,6)), "bundle.item_spell_reference")
    for row in bundle["tables"].get("spell_template", []):
        check(all(row[f"EffectTriggerSpell{i}"] == 0 or row[f"EffectTriggerSpell{i}"] in spells for i in range(1,4)), "bundle.trigger_reference")
    status = bundle["report"]
    check(type(status) is dict and set(status) == {"runnable", "capture_complete", "scope", "unsupported"}, "bundle.report")
    check(status["runnable"] is False and status["capture_complete"] is False, "bundle.not_runnable")
    check(type(status["scope"]) is str and len(status["scope"]) <= 256, "bundle.report_scope")
    check(type(status["unsupported"]) is list and len(status["unsupported"]) <= MAX_ROWS*8, "bundle.report_limit")
    for issue in status["unsupported"]:
        check(type(issue) is dict and set(issue) == {"table", "key", "status", "reason"}, "bundle.issue_fields")
        check(issue["table"] in COLUMNS and issue["status"] == "blocked", "bundle.issue")
        check(type(issue["key"]) is str and len(issue["key"]) <= 128, "bundle.issue_key")
        check(type(issue["reason"]) is str and len(issue["reason"]) <= 128, "bundle.issue_reason")
    required_findings = full_report(bundle["tables"], profiles, selection)
    check(status["unsupported"] == required_findings, "bundle.missing_or_inconsistent_blocker")
    check(len(canonical(bundle)) <= MAX_BUNDLE_BYTES, "bundle.byte_limit")


def full_report(tables: dict[str, list[dict[str, Any]]], profiles: dict[str, Any], selection: Selection) -> list[dict[str, Any]]:
    findings = report(tables, profiles) + starter.blockers(tables, selection)
    needed_stats = set()
    for row in tables["creature_template"]:
        low = integer(row["MinLevel"], 1, 65, "creature.min_level")
        high = integer(row["MaxLevel"], low, 65, "creature.max_level")
        unit_class = integer(row["UnitClass"], 0, 8, "creature.unit_class")
        needed_stats.update((unit_class, level) for level in range(low, high+1))
    actual_stats = {(row["Class"], row["Level"]) for row in tables["creature_template_classlevelstats"]}
    for unit_class, level in sorted(needed_stats-actual_stats):
        findings.append({"table": "creature_template_classlevelstats", "key": f"{unit_class}:{level}",
                         "status": "blocked", "reason": "creature.missing_classlevelstats"})
    spawned = {row["id"] for row in tables["creature"]}
    for quest in tables["quest_template"]:
        targets = {quest[f"ReqCreatureOrGOId{i}"] for i in range(1,5) if quest[f"ReqCreatureOrGOId{i}"] > 0}
        if not targets <= spawned:
            findings.append({"table": "quest_template", "key": str(quest["entry"]),
                             "status": "blocked", "reason": "quest.objective_outside_selection"})
        for table in ["creature_questrelation", "creature_involvedrelation"]:
            if not any(row["quest"] == quest["entry"] and row["id"] in spawned for row in tables[table]):
                findings.append({"table": table, "key": str(quest["entry"]),
                                 "status": "blocked", "reason": "quest.relation_missing_or_outside_selection"})
    return sorted(findings, key=lambda row: (row["table"], str(row["key"]), row["reason"]))
