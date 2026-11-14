"""Bounded AGGRO text plan, not runtime admission.

SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
CreatureEventAIMgr.cpp:210-269,594-614; CreatureEventAI.cpp:102-163,676-730;
ObjectMgr.cpp:7782-7880.
"""
from typing import Any


def supported_plan(source: dict[str, Any], entry: int, scope: dict[str, Any]) -> dict[str, Any] | None:
    tables = source["tables"]
    coverage = source.get("event_text_capture", {})
    if coverage.get("status") != "complete":
        return None
    rows = scope["matching_rows"]
    if not rows:
        return None
    broadcasts = {row["Id"]: row for row in tables.get("broadcast_text", [])}
    for row in rows:
        aggro = (row["event_type"] == 4 and row["event_flags"] == 0
                 and all(row[f"event_param{i}"] == 0 for i in range(1, 7)))
        ooc = (row["event_type"] == 1 and row["event_flags"] in (0, 1)
               and all(0 <= row[f"event_param{i}"] <= 0xffffffff for i in range(1, 5))
               and row["event_param1"] <= row["event_param2"]
               and row["event_param3"] <= row["event_param4"]
               and row["event_param5"] == row["event_param6"] == 0)
        if (not (aggro or ooc) or row["event_inverse_phase_mask"] != 0
                or not 1 <= row["event_chance"] <= 100
                or any(key.startswith("action4_") for key in row)
                or row["action1_type"] != 1
                or any(row[f"action{i}_type"] != 0 or
                       any(row[f"action{i}_param{j}"] != 0 for j in range(1, 4))
                       for i in (2, 3))):
            return None
        ids = [row[f"action1_param{i}"] for i in range(1, 4)]
        if ids[0] <= 0 or any(value < 0 for value in ids) or (ids[1] == 0 and ids[2] != 0):
            return None
        for text_id in filter(None, ids):
            text = broadcasts.get(text_id)
            if (text is None or text["ChatTypeID"] != 0 or text["LanguageID"] not in (0, 7)
                    or not text["Text"] or any(text[field] != 0 for field in
                    ("SoundEntriesID1", "SoundEntriesID2", "EmoteID1", "EmoteID2", "EmoteID3",
                     "EmoteDelay1", "EmoteDelay2", "EmoteDelay3"))):
                return None
    spawns = [row for row in tables["creature"] if row["id"] == entry]
    return {"policy": "cmangos-8ec338a1-aggro-ooc-text-v3", "status": "supported_catalog_plan",
            "scope": "captured_current_entry_and_selected_spawn_guids",
            "owners": [{"database_guid": spawn["guid"], "captured_entry": entry,
                        "event_ids": [row["id"] for selector in (entry, -spawn["guid"])
                                      for row in rows if row["creature_id"] == selector]}
                       for spawn in spawns],
            "runtime_configuration": "unknown", "loaded_audience": "unknown",
            "runtime_admission": False}


def discharges(issue: dict[str, Any], creatures: list[dict[str, Any]]) -> bool:
    key = issue["key"]
    if isinstance(key, list) and len(key) == 1:
        key = key[0]
    for creature in creatures:
        rows = (creature.get("event_ai_capture") or {}).get("matching_rows", [])
        if creature["event_ai_plan"] is None or not any(row["event_type"] == 1 for row in rows):
            continue
        if (issue["table"] == "creature_ai_scripts"
                and issue["reason"] == "creature.event_ai_matching_row_requires_execution"
                and any(str(row["id"]) == str(key) for row in rows)):
            return True
        source = creature["source_row"]
        if (issue["table"] == "creature_template"
                and issue["reason"] == "creature.ai_or_spell_dependency"
                and str(creature["entry"]) == str(key) and source["AIName"] == "EventAI"
                and not any(source[field] for field in ("ScriptName", "SpellList", "CharmedSpellList"))):
            return True
    return False
