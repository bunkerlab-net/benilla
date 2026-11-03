"""Ordinary quest/giver preparation, without filtering eager selected rows.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
ObjectMgr.cpp:4231-4880,5775-5909,7940-8035; Player.cpp:12087-12181.
"""
from classic_export import quest_presentation, quest_relationships, quest_mechanics, quest_endpoints
from classic_export.core import digest
from classic_export.quest_columns import QUEST_TEXT
from . import canonical_order

ADAPTER = "quest_lifecycle::CapturedOrdinaryQuests + captured_quests::CapturedQuestPresentation"

def supported_plan(source, dbcs):
    relationships = quest_relationships.resolve(source)
    refusals = list(relationships["refusals"])
    value = source.get("quest_giver_presentation_capture")
    raw_resolved = True
    def refuse(key, field, target, reason):
        nonlocal raw_resolved
        raw_resolved = False
        refusals.append({"quest": key, "field": field, "target": target, "reason": reason})
    if value is None:
        refuse(0, "quest_giver_presentation_capture", 0, "unknown_coverage")
        return {"status": "blocked", "adapter": ADAPTER, "refusals": refusals,
                "raw_presentation_resolved": False, "ordinary_catalog_resolved": False,
                "source_sha256": digest(source), "input_bindings": None}
    quest_presentation.validate(source)
    mechanics = quest_mechanics.supported(source)
    for row in mechanics["refusals"]:
        refuse(row["quest"], row["field"], row["target"], row["reason"])
    if source.get("quest_mechanics_capture"):
        for row in source["quest_mechanics_capture"]["rows"]["dbscripts_on_quest_end"]:
            if row["command"] == 1 and (dbcs.get("Emotes") is None or row["datalong"] not in dbcs["Emotes"].by_id):
                refuse(0, "Emotes", row["datalong"], "unknown_lookup")
            elif row["command"] == 1 and dbcs["Emotes"].by_id[row["datalong"]].words[4] != 0:
                refuse(0, "Emotes.EmoteType", row["datalong"], "state_emote_not_supported")
    raw = value["rows"]
    emotes = dbcs.get("Emotes")
    if emotes is None:
        refuse(0, "Emotes", 0, "unknown_coverage_reextract_v4")
    for row in raw["quest_template"]:
        for field in (*quest_presentation.DELAYS,
                      *(f"{prefix}{i}" for prefix in ("DetailsEmote", "DetailsEmoteDelay", "OfferRewardEmote", "OfferRewardEmoteDelay") for i in range(1, 5))):
            if row[field] != 0:
                refuse(row["entry"], field, row[field], "unsupported_packet_shape")
        for field in QUEST_TEXT:
            text = row[field]
            # QuestDef.cpp:58 loads NULL through Field::GetCppString as empty.
            # GossipDef.cpp:531-534 skips request-items when that text is empty.
            if field == "RequestItemsText" and text is None:
                continue
            if text is None or any(ord(c) < 32 and c not in "\n\t" for c in text):
                refuse(row["entry"], field, 0, "invalid_text")
    entries = set(value["selectors"]["entries"])
    spawns = {row["id"] for row in source["tables"]["creature"]}
    for quest in value["selectors"]["quests"]:
        for table in ("creature_questrelation", "creature_involvedrelation"):
            if not any(row["quest"] == quest and row["id"] in spawns for row in source["tables"][table]) and not quest_endpoints.proved(source, table, quest):
                refuse(quest, table, 0, "missing_spawned_relation")
    for row in raw["questgiver_greeting"]:
        if row["Type"] != 0 or (emotes is not None and row["EmoteId"] not in emotes.by_id):
            refuse(0, "questgiver_greeting", row["Entry"], "invalid_type_or_emote")
    for row in source["tables"]["creature_template"]:
        if row["Entry"] in entries:
            if row["NpcFlags"] & 2 == 0:
                refuse(0, "NpcFlags", row["Entry"], "missing_questgiver_flag")
            if row["ScriptName"]:
                refuse(0, "ScriptName", row["Entry"], "registered_callbacks_unresolved")
            if row["GossipMenuId"] and row["Entry"] not in mechanics["givers"]:
                refuse(0, "GossipMenuId", row["GossipMenuId"], "gossip_applicability_unresolved")
    presentation_resolved = raw_resolved
    order = canonical_order.relationships(source)
    if source.get("quest_mechanics_capture"):
        mechanics["loaded_script_order"] = order["scripts"]
    for field in ("loaded_relation_order", "effective_giver_selection", "registered_callbacks",
                  "global_spell_area_and_item_effects", "session_locale_and_dialog_policy"):
        if field == "loaded_relation_order" and order["status"] == "complete_inputs":
            if source.get("quest_mechanics_capture") and order["scripts"] is None:
                refuse(0, "loaded_script_order", 0, "independent_context_required")
            continue
        refuse(0, field, 0, "independent_context_required")
    return {"status": "blocked", "adapter": ADAPTER, "source_sha256": digest(source),
            "raw_presentation_resolved": presentation_resolved, "ordinary_catalog_resolved": False,
            "refusals": refusals, "selected_quest_ids": value["selectors"]["quests"],
            "canonical_order": order,
            "input_bindings": {"capture_sha256": digest(value), "Emotes_sha256": emotes.sha256 if emotes else None},
            **({"mechanics": mechanics} if source.get("quest_mechanics_capture") else {}),
            **({"endpoints": source["quest_endpoint_capture"]["endpoints"]} if source.get("quest_endpoint_capture") else {}),
            "field_dispositions": {"quest_template": {"consumed_by_ordinary_presentation": [*QUEST_TEXT, *quest_presentation.DELAYS,
                *sorted(name for name in raw["quest_template"][0] if "Emote" in name)] if raw["quest_template"] else []},
                "remaining": "eligibility_credit_reward_rates_hooks_and_loaded_order"},
            "runtime_export": None}
