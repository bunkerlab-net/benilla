"""Closed native action proof, not a claim that global source hooks are absent.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from classic_export.core import digest
from .global_hooks_plan import reviewed_creature

POLICY = "benilla-native-body-only-command-domain-v1"
# SpellEffectDefines.h:97,121,125; Creature.h:191-198; SharedDefines.h:1409,1416.
EFFECTS = {71, 95, 99}
SKILLS = {182, 186, 333, 393}
SCRIPT_GATES = {
    "spell_food_animation": "food_animation_heartbeat_only",
    "spell_gameobject_call_for_help_on_usage": "unavailable_gameobject_executor",
    "spell_opening_capping": "unavailable_gameobject_executor",
}


def plan(source, closure):
    tables, refusals = source["tables"], []

    def refuse(table, key, reason):
        refusals.append({"table": table, "key": str(key), "reason": "loot." + reason})
    has_ids = (any(r["PickpocketLootId"] or r["SkinningLootId"] for r in tables["creature_template"])
               or any(r["DisenchantID"] for r in tables["item_template"]))

    if has_ids and (not closure or not closure["captured_inputs_complete"]):
        refuse("spell_template", "scope", "non_body_learning_closure_incomplete")
    for extension in ("learning_capture", "initial_hook_capture", "spell_script_capture", "event_ai_capture"):
        if has_ids and extension not in source:
            refuse(extension, "scope", "non_body_hook_coverage_unknown")
    for row in tables["spell_template"]:
        for slot in range(1, 4):
            effect, misc = row[f"Effect{slot}"], row[f"EffectMiscValue{slot}"]
            if effect in EFFECTS:
                refuse("spell_template", row["Id"], "non_body_spell_reachable")
            if effect in {44, 118} and misc in SKILLS:
                refuse("spell_template", row["Id"], "non_body_skill_reachable")
    for skill in sorted(set(closure["skill_ids"] if closure else []) & SKILLS):
        refuse("SkillLine", skill, "non_body_skill_reachable")
    for table in ("spell_area", "spell_proc_event"):
        if tables.get(table):
            refuse(table, "scope", "non_body_hook_execution_unproven")
    hooks = []
    for row in tables.get("spell_scripts", []):
        gate = SCRIPT_GATES.get(row["ScriptName"])
        if gate is None or row["ScriptName"] == "spell_food_animation" and row["Id"] != 433:
            refuse("spell_scripts", row["Id"], "non_body_script_hook_unreviewed")
        hooks.append({**row, "gate": gate})
    for row in tables["item_template"]:
        if row["ScriptName"]:
            refuse("item_template", row["entry"], "non_body_item_hook_unreviewed")
    for row in tables["creature_template"]:
        if row["ScriptName"]:
            refuse("creature_template", row["Entry"], "non_body_creature_callback_present_unsupported"
                   if reviewed_creature(source, row) else "non_body_creature_hook_unreviewed")
    hook_tables = ("creature_ai_scripts", "creature_template_addon", "creature_addon",
                   "spell_scripts", "spell_area", "spell_proc_event", "quest_template")
    hook_bindings = {name: digest(tables.get(name)) for name in hook_tables}
    unavailable_calls = [{"event": row["id"], "slot": slot, "action": row[f"action{slot}_type"],
                          "refusal": "creature.event_ai_matching_row_requires_execution"}
                         for row in tables.get("creature_ai_scripts", []) for slot in range(1, 4)
                         if row[f"action{slot}_type"] not in {0, 1}]
    return {"status": ("blocked" if refusals else "unavailable_proven" if has_ids
                       else "not_applicable_no_non_body_loot_ids"), "policy": POLICY,
            "command_domain": "Command_casts_use_only_bound_executors_body_loot_has_no_type_operand",
            "trainer_domain": "no_train_learn_skill_or_learn_spell_command",
            "hook_domain": "only_bound_native_executors_unimplemented_hooks_refuse_before_entry",
            "global_source_hooks_absent": False,
            "hook_table_sha256": hook_bindings,
            "unavailable_event_ai_calls": unavailable_calls,
            "native_hook_executors": ["EventAI_TEXT", "quest_script_21_presentation", "bound_spell_callbacks"],
            "learning_closure_sha256": digest(closure), "source_sha256": digest(source),
            "spell_ids": closure["spell_ids"] if closure else [],
            "skill_ids": closure["skill_ids"] if closure else [], "script_gates": hooks,
            "creature_loot_ids": [{k: r[k] for k in ("Entry", "LootId", "PickpocketLootId", "SkinningLootId")}
                                  for r in tables["creature_template"]],
            "item_loot_ids": [{"entry": r["entry"], "DisenchantID": r["DisenchantID"]}
                              for r in tables["item_template"]], "refusals": refusals}
