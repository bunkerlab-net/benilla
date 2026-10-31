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
        if (row["event_type"] != 4 or row["event_inverse_phase_mask"] != 0
                or row["event_flags"] != 0 or not 1 <= row["event_chance"] <= 100
                or any(row[f"event_param{i}"] != 0 for i in range(1, 7))
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
    return {"policy": "cmangos-8ec338a1-aggro-text-v2", "status": "supported_catalog_plan",
            "scope": "captured_current_entry_and_selected_spawn_guids",
            "owners": [{"database_guid": spawn["guid"], "captured_entry": entry,
                        "event_ids": [row["id"] for selector in (entry, -spawn["guid"])
                                      for row in rows if row["creature_id"] == selector]}
                       for spawn in spawns],
            "runtime_configuration": "unknown", "loaded_audience": "unknown",
            "runtime_admission": False}
