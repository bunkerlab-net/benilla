"""Checked learning inputs, never loaded order, hook absence, or a spellbook.

CMaNGOS 8ec338a1, SQLStorages.cpp:40,55; DBCStructure.h:716-814.
SpellMgr.cpp:56-73,137-141 defines missing duration/cast-time lookups.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from classic_export.core import digest
from classic_export.spell_numeric import WORD_FIELDS, FLOAT_FIELDS, scope
from .dbc import binary32_bits, signed

FORMAT = "benilla-native-learning-numeric-candidate-v1"


def candidate(source, dbcs, closure, spells, loader_plan):
    selected = closure["spell_ids"]
    if "spell_numeric_capture" not in source:
        return {"format": FORMAT, "status": "blocked", "selected_spell_count": len(selected),
                "checked_numeric_count": 0,
                "sub_obligations": {"complete_effective_projection": False, "loaded_order": False,
                                    "initial_hooks": False, "initial_cast_dispatch": False,
                                    "effective_learning": False},
                "refusals": [{"table": "spell_template", "key": [],
                              "reason": "player.learning_numeric_complete_rows_required"}]}
    indexed = {row["id"]: row for row in spells}
    rows, refusals = [], []

    def refuse(spell, reason):
        refusals.append({"table": "spell_template", "key": [spell], "reason": reason})

    binding = closure["dependency_request"]
    if binding["source_sha256"] != digest(source) or binding["dbc_sha256"] != {
            name: table.sha256 for name, table in sorted(dbcs.items())}:
        refuse(0, "player.learning_numeric_input_binding")
    if not loader_plan["loaded_learning_maps_resolved"]:
        refuse(0, "player.learning_numeric_adapter_prerequisites")
    for spell in selected:
        captured = scope(source, spell)
        client = dbcs.get("Spell")
        client = client.by_id.get(spell) if client else None
        if captured is None or client is None:
            refuse(spell, "player.learning_numeric_complete_rows_required")
            continue
        sql = captured["row"]
        words = list(client.words)
        for name, column in WORD_FIELDS.items():
            value = sql[name]
            words[column] = binary32_bits(value["decimal"]) if name in FLOAT_FIELDS else value & 0xffffffff
        words[161] = sql["SpellFamilyFlags"] & 0xffffffff
        words[162] = sql["SpellFamilyFlags"] >> 32
        if words != indexed[spell]["server_numeric_words"]:
            refuse(spell, "player.learning_numeric_projection_conflict")
        lookups = []
        for table, key in (("SpellCastTimes", "CastingTimeIndex"), ("SpellDuration", "DurationIndex"),
                           ("SpellRange", "rangeIndex"), ("SpellCategory", "Category"),
                           ("SpellFocusObject", "RequiresSpellFocus"),
                           *(("SpellRadius", f"EffectRadiusIndex{i}") for i in range(1, 4))):
            entry = sql[key]
            if not entry and table not in {"SpellCastTimes", "SpellDuration", "SpellRange"}:
                lookups.append({"table": table, "id": entry, "status": "source_zero_no_lookup", "words": None})
                continue
            dbc = dbcs.get(table)
            row = dbc.by_id.get(entry) if dbc else None
            if dbc is None or row is None and (entry or table == "SpellRange"):
                refuse(spell, "player.learning_numeric_auxiliary_lookup_required")
            lookups.append({"table": table, "id": entry,
                            "status": "present" if row else "verified_absent" if dbc else "unknown",
                            "words": list(row.words) if row else None})
        duration = next(r for r in lookups if r["table"] == "SpellDuration")
        duration_ms = signed(duration["words"][1]) if duration["words"] else (
            0 if duration["status"] == "verified_absent" else None)
        if duration_ms == -0x80000000:
            refuse(spell, "player.learning_numeric_duration_domain")
        elif duration_ms is not None and duration_ms != -1:
            duration_ms = abs(duration_ms)
        server_only = {name: binary32_bits(sql[name]["decimal"]) if name in FLOAT_FIELDS else sql[name]
                       for name in captured["row"] if name not in WORD_FIELDS and name != "SpellFamilyFlags"}
        rows.append({"id": spell, "client_words_sha256": digest(list(client.words)),
                     "effective_words_sha256": digest(words), "duration_ms": duration_ms,
                     "auxiliary_lookups": lookups, "server_only": server_only,
                     "numeric_capture_sha256": captured["coverage_sha256"]})
    complete = closure["captured_inputs_complete"] and not refusals and len(rows) == len(selected)
    outcomes = {status: sum(lookup["status"] == status for row in rows for lookup in row["auxiliary_lookups"])
                for status in ("present", "verified_absent", "source_zero_no_lookup", "unknown")}
    return {"format": FORMAT, "status": "checked" if complete else "blocked",
            "source_identity": closure["dependency_request"]["source_sha256"],
            "dbc_binding_sha256": digest(closure["dependency_request"]["dbc_sha256"]),
            "seed_independent": True, "effective_learning_proven": False,
            "sub_obligations": {"complete_effective_projection": complete,
                                "loaded_order": False, "initial_hooks": False,
                                "initial_cast_dispatch": False, "effective_learning": False},
            "selected_spell_count": len(selected), "checked_numeric_count": len(rows) if complete else 0,
            "rows_sha256": digest(rows), "auxiliary_lookup_count": sum(outcomes.values()) - outcomes["source_zero_no_lookup"],
            "auxiliary_outcomes": outcomes,
            "server_only_field_count": sum(len(r["server_only"]) for r in rows), "refusals": refusals}
