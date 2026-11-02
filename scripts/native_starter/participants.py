"""Current geometry supports only a proved-empty non-render domain.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from collections import Counter
from classic_export.core import digest
from classic_export.participants_columns import SCHEMA

FORMAT = "cmangos-8ec338a1-empty-participants-plan-v1"
TYPES = ("door", "button", "questgiver", "chest", "binder", "generic", "trap", "chair",
         "spell_focus", "text", "goober", "transport", "area_damage", "camera", "mapobject",
         "mo_transport", "duel_flag", "fishing_node", "summoning_ritual", "mailbox", "auctionhouse",
         "guardpost", "spellcaster", "meetingstone", "flagstand", "fishing_hole", "flagdrop",
         "mini_game", "unknown_28", "capture_point", "aura_generator")
CREATION_EFFECTS = {3, 28, 41, 42, 50, 56, 61, 64, 72, 73, 74, 76, 77, 85, 87, 88, 89, 90, 97, 104, 105, 106, 107, 109, 112}

def supported_plan(source):
    value = source.get("participants_capture")
    refusals, inventory = [], []
    def refuse(table, key, reason, **context):
        refusals.append({"table": table, "key": key, "reason": "participants." + reason, **context})
    if value is None:
        refuse("participants", [], "capture_required")
    else:
        raw = value["rows"]
        for table in sorted(SCHEMA):
            if value["observations"][table]["status"] == "unknown":
                refuse(table, [], "table_unknown")
        templates = {r["entry"]: r for r in raw.get("gameobject_template", [])}
        for spawn in raw.get("gameobject", []):
            alternatives = {spawn["id"]} | {r["entry"] for r in raw.get("gameobject_spawn_entry", []) if r["guid"] == spawn["guid"]}
            for entry in sorted(alternatives - {0}):
                template = templates.get(entry)
                if template is None:
                    refuse("gameobject_template", [entry], "template_unknown")
                    continue
                kind = template["type"]
                name = TYPES[kind] if kind < len(TYPES) else "unknown"
                server_field = {5: "data2", 6: "data8", 8: "data3", 30: "data6"}.get(kind)
                server_only = bool(server_field and template[server_field])
                collision = "disabled" if server_only or kind == 6 else "door_state_dependent" if kind == 0 else "enabled_when_spawned"
                inventory.append({"guid": spawn["guid"], "entry": entry, "type": kind, "type_name": name,
                    "display_id": template["displayId"], "collision_policy": collision,
                    "collision_model": "unproved", "required_port": "5.6c." + name})
                refuse("gameobject", [spawn["guid"], entry], "unsupported_" + name)
                if template["ScriptName"]:
                    refuse("gameobject_template", [entry], "runtime_script_required")
            if spawn["id"] == 0 or any(r["guid"] == spawn["guid"] for r in raw.get("gameobject_spawn_entry", [])):
                refuse("gameobject", [spawn["guid"]], "spawn_alternatives_required")
        observation = value["observations"]["gameobject"]
        if observation["status"] == "observed" and observation["selected_map_rows"] > len(raw["gameobject"]):
            refuse("gameobject", [], "exterior_bounds_coverage_required",
                   count=observation["selected_map_rows"] - len(raw["gameobject"]))
        for table in ("game_event_gameobject", "pool_gameobject", "pool_gameobject_template"):
            for row in raw.get(table, []):
                key = [row["id"] if table == "pool_gameobject_template" else row["guid"]]
                refuse(table, key, "event_membership_required" if table == "game_event_gameobject" else "pool_membership_required")
        for row in raw.get("transports", []):
            template = templates.get(row["entry"])
            if template is None:
                refuse("gameobject_template", [row["entry"]], "template_unknown")
            refuse("transports", [row["entry"]], "transport_path_coverage_required",
                   taxi_path_id=template["data0"] if template else None)
        for table in sorted(t for t in SCHEMA if t.startswith("dbscripts_")):
            for command, count in sorted(Counter(r["command"] for r in raw.get(table, [])).items()):
                refuse(table, [command], "runtime_db_script_command_required", count=count)
    # Existing selected hook/script captures define the reachable spell/AI depth.
    for extension in ("event_ai_capture", "spell_script_capture", "initial_hook_capture"):
        if extension not in source:
            refuse(extension, [], "creation_hook_coverage_required")
    for row in source["tables"].get("creature_template", []):
        if row["ScriptName"]:
            refuse("creature_template", [row["Entry"]], "runtime_script_required")
    for row in source["tables"].get("spell_scripts", []):
        if row["ScriptName"]:
            refuse("spell_scripts", [row["Id"]], "runtime_script_required")
    for row in source["tables"].get("item_template", []):
        if row["ScriptName"]:
            refuse("item_template", [row["entry"]], "runtime_script_required")
    for table in ("spell_area", "spell_proc_event"):
        if source["tables"].get(table):
            refuse(table, [], "runtime_initial_hook_required")
    for row in source["tables"].get("creature_ai_scripts", []):
        for slot in range(1, 4):
            if row[f"action{slot}_type"] not in (0, 1, 4, 5, 9, 10, 44, 54):
                refuse("creature_ai_scripts", [row["id"], slot], "runtime_event_ai_creation_path_required")
    for row in source["tables"].get("spell_template", []):
        for slot in range(1, 4):
            if row[f"Effect{slot}"] in CREATION_EFFECTS:
                refuse("spell_template", [row["Id"], slot], "runtime_spell_creation_path_required")
    return {"format": FORMAT, "status": "unknown" if value is None else "blocked" if refusals else "proved_empty",
            "capture_sha256": digest(value) if value else None, "controller_binding": "closed_no_native_capability",
            "empty_participant_obligation_discharged": value is not None and not refusals,
            "production_admission": False, "inventory": inventory, "refusals": refusals}
