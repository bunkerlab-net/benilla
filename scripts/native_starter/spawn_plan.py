"""Bounded static spawn loader plan for existing generation/movement owners.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from decimal import Decimal
from typing import Any
from classic_export import native_spawn, spawn_context
from classic_export.core import digest
from .dbc import binary32_bits

FORMAT = "cmangos-8ec338a1-selected-static-spawn-plan-v1"


def supported_plan(source: dict[str, Any], entry: int) -> dict[str, Any]:
    native = native_spawn.scope(source, entry)
    spawns = [row for row in source["tables"]["creature"] if row["id"] == entry]
    result = {"format": FORMAT, "entry": entry, "status": "blocked", "spawns": [], "refusals": [],
              "cold_loader_order_proven": False, "assistance_traversal_proven": False,
              "production_admission": False}
    templates = {row["Entry"]: row for row in source["tables"]["creature_template"]}
    for row in spawns:
        guid = row["guid"]
        refusals = []
        context = spawn_context.scope(source, guid, entry)
        dependency = next((value for value in native or [] if value["guid"] == guid), None)
        if context is None or dependency is None:
            refusals.append("spawn.context_coverage_required")
        else:
            refusals.extend(f"spawn.unsupported_{table}" for table, rows in context.items() if rows)
            refusals.extend(f"spawn.unsupported_{table}" for table in native_spawn.HOOK_TABLES
                            if dependency["tables"][table]["rows"])
            if any(r["guid"] == guid for r in source["tables"]["creature_spawn_entry"]):
                refusals.append("spawn.raw_entry_alternative_unimplemented")
            template = dependency["spawn_template"]["row"]
            if template is None:
                refusals.append("creature.default_spawn_template_absent")
            elif (template["RelayId"] or template["StringId"] or template["SpawnFlags"] & ~1
                  or template["NpcFlags"] not in {-1, templates[entry]["NpcFlags"]}
                  or template["EquipmentId"] < -1 or template["UnitFlags"] < -1
                  or template["UnitFlags"] > 0xffffffff):
                refusals.append("spawn.override_execution_required")
            for table in ("creature_addon", "creature_template_addon"):
                for addon in dependency["tables"][table]["rows"]:
                    if (addon["mount"] or addon["stand_state"] or addon["emote"]
                            or addon["sheath_state"] not in {0, 1, 2} or addon["auras"] not in {None, ""}):
                        refusals.append("spawn.addon_execution_required")
        if row["spawnMask"] != 1 or not entry:
            refusals.append("spawn.static_identity_required")
        if max(row["spawntimesecsmin"], row["spawntimesecsmax"]) > 0xffffffff // 9:
            refusals.append("spawn.respawn_timer_overflow")
        if row["MovementType"] not in {0, 1}:
            refusals.append("spawn.movement_type_unimplemented")
        if any(abs(Decimal(row[field]["decimal"])) > Decimal("17066.166") for field in
               ("position_x", "position_y", "position_z")):
            refusals.append("spawn.coordinate_domain")
        orientation = Decimal(row["orientation"]["decimal"])
        if not 0 <= orientation <= Decimal("6.283185307179586"):
            refusals.append("spawn.orientation_domain")
        radius = max(Decimal(row["spawndist"]["decimal"]), 0) if row["MovementType"] == 1 else 0
        if row["MovementType"] == 1 and Decimal(row["spawndist"]["decimal"]) < 0:
            refusals.append("spawn.negative_random_radius_repair")
        if radius > 150:
            refusals.append("spawn.wander_radius_domain")
        if refusals:
            result["refusals"].extend({"guid": guid, "reason": reason} for reason in sorted(set(refusals)))
        else:
            result["spawns"].append({"guid": guid, "entry": entry, "map": row["map"],
                "position_binary32": [binary32_bits(row[field]["decimal"]) for field in ("position_x", "position_y", "position_z")],
                "orientation_binary32": binary32_bits(row["orientation"]["decimal"]),
                "movement": "wander" if radius else "idle", "radius_binary32": binary32_bits(str(radius)),
                "respawn_seconds": [row["spawntimesecsmin"], max(row["spawntimesecsmin"], row["spawntimesecsmax"])],
                "spawn_template": dependency["spawn_template"]["row"],
                "input_binding": digest({"row": row, "native": dependency, "context": context,
                                         "alternatives": source["spawn_entry_capture"]})})
    if not result["refusals"] and spawns:
        result["status"] = "supported_shape"
    return result


def discharges(issue: dict[str, Any], plans: list[dict[str, Any]]) -> bool:
    guids = {str(row["guid"]) for plan in plans for row in plan["spawns"]}
    entries = {str(plan["entry"]) for plan in plans if plan["status"] == "supported_shape"}
    key = issue["key"]
    key = str(key[0]) if isinstance(key, list) and len(key) == 1 else str(key)
    return ((issue["table"] == "creature" and key in guids and issue["reason"] in
             {"movement.native_rules_required", "spawn.overrides_pools_events_not_captured",
              "creature.applicable_spawn_hooks_require_execution"})
            or (issue["table"] == "creature_template" and key in entries and issue["reason"] in
                {"creature.deterministic_respawn_selection_required", "creature.unexported_groups_assistance_context"}))

def consumes(table: str, row: dict[str, Any], plans: list[dict[str, Any]]) -> bool:
    selected = [spawn for plan in plans for spawn in plan["spawns"]]
    if table in {"creature", "creature_spawn_data", "creature_addon"}:
        field = "Guid" if table == "creature_spawn_data" else "guid"
        return any(spawn["guid"] == row[field] for spawn in selected)
    if table == "creature_spawn_data_template":
        return any(spawn["spawn_template"]["Entry"] == row["Entry"] for spawn in selected)
    if table == "creature_template_addon":
        return any(spawn["entry"] == row["entry"] for spawn in selected)
    return False
