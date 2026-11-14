"""Pinned ordinary-creature combat selection, not effective runtime admission.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
CreatureAISelector.cpp:38-90; CreatureAIRegistry.cpp:35-43;
CreatureEventAI.cpp:51-62; CreatureAI.cpp:35-46; Unit.cpp:6228-6238,11768-11805;
Creature.cpp:555-559,2154-2227. Upstream GPL-2.0-or-later.
"""
from __future__ import annotations
from typing import Any
from .dbc import signed, binary32_bits

POLICY = "cmangos-8ec338a1-creature-combat-evidence-v1"
REGISTERED = {"NullAI", "GuardAI", "PetAI", "TotemAI", "EventAI", "GuardianAI", "PossessedAI"}
COMBAT_POLICY = ["aggro_rate_binary32", "initial_and_relocation_aggro_delay_ms",
                 "aggro_help_delay_ms", "family_assistance_radius_binary32",
                 "family_assistance_delay_ms", "check_for_help_radius_binary32",
                 "leash_and_melee_policy"]
COLD_EVIDENCE = ["source_and_configuration_identity", "map_and_grid_activation_sequence",
                 "persistent_spawn_state", "ordered_cell_grid_and_world_membership",
                 "cold_entry_and_consecutive_update_order", "shared_rng_draw_schedule"]


def plan(creature: dict[str, Any], generation: dict[str, Any], event_scope: Any,
         event_plan: Any, dbcs: dict[str, Any], rules: Any) -> dict[str, Any]:
    # This is the ordinary, uncharmed, non-pet/non-totem selector branch only.
    # Script registry/control/entry overrides remain independent requirements.
    name = creature["AIName"]
    if creature["ScriptName"]:
        factory, branch = None, "script_registry_required"
    elif name:
        factory = name if name in REGISTERED else "NullCreatureAI"
        branch = "explicit_name" if name in REGISTERED else "unregistered_name_falls_back_to_null"
    elif creature["ExtraFlags"] & 0x400:
        factory, branch = "GuardAI", "guard_flag"
    else:
        factory, branch = "EventAI", "highest_permit"
    faction_table = dbcs.get("FactionTemplate")
    faction = faction_table.by_id.get(creature["Faction"]) if faction_table else None
    neutral = None
    if faction is not None:
        raw_table = dbcs.get("Faction")
        raw = raw_table.by_id.get(faction.words[1]) if raw_table else None
        if not faction.words[1]:
            neutral = True
        elif raw_table is not None:
            neutral = False if raw and signed(raw.words[1]) >= 0 else (
                not any(faction.words[6:10]) and not faction.words[4] and not faction.words[5])
    defensive = bool(creature["ExtraFlags"] & 2 or creature["StaticFlags2"] & 0x4000000)
    combat_allowed = not (creature["StaticFlags1"] & 0x2000000 and not creature["StaticFlags2"] & 0x4000000)
    event_supported = (event_scope is not None and
                       (not event_scope["matching_rows"] or event_plan is not None))
    ai_refusals = ["effective_script_registry_control_and_spawn_selection_required"]
    if factory != "EventAI":
        ai_refusals.append("selected_factory_not_supported")
    if not event_supported:
        ai_refusals.append("event_ai_execution_or_coverage_required")
    if (creature["StaticFlags1"] or creature["StaticFlags2"] & ~0x150
            or creature["StaticFlags3"] or creature["StaticFlags4"]):
        ai_refusals.append("combat_authority_static_flags_unsupported")
    if creature["ExtraFlags"] & ~0x80002:
        ai_refusals.append("combat_authority_extra_flags_unsupported")
    if generation["status"] != "complete_inputs":
        ai_refusals.append("creature_generation_inputs_required")
    ai_refusals.append("source_melee_motion_hooks_and_current_target_context_required")
    family_radius = rules["combat"]["family_assistance_radius_bits"] if rules is not None else None
    template_radius = creature["CallForHelp"] > 0
    return {"policy": POLICY, "production_admission": False,
        "ai_selection": {"scope": "ordinary_uncontrolled_creature_before_external_overrides",
            "factory": factory, "branch": branch, "neutral_to_all": neutral,
            "event_ai_permit": 800 if name == "EventAI" else (
                100 if creature["ExtraFlags"] & 2 or neutral else 200)
                if neutral is not None or creature["ExtraFlags"] & 2 else None,
            "initial_reaction": "Defensive" if defensive else "Aggressive",
            "can_enter_combat_from_static_flags": combat_allowed,
            "event_rows_supported": event_supported, "refusals": ai_refusals},
        "melee": {"runtime_consumer": "existing_creature_generation_and_native_mainhand",
            "base_attack_time_ms": creature["MeleeBaseAttackTime"],
            "damage_school": creature["DamageSchool"],
            "generation_inputs_complete": generation["status"] == "complete_inputs",
            "resolved_combat_stats": None},
        "aggro": {"runtime_consumer": "creature_ai::attack_distance_and_source_combat_authority",
            "detection_range": creature["Detection"],
            "level_interval": [creature["MinLevel"], creature["MaxLevel"]],
            "neutral_is_not_forced_hostile": True,
            "required_policy": COMBAT_POLICY,
            "required_context": ["directional_faction_and_current_reputation", "detect_range_aura_closure",
                                 "attackability_control_and_event_notify_state", "current_spatial_evidence"]},
        "assistance": {"call_for_help": creature["CallForHelp"],
            "pull_radius_source": "template" if creature["CallForHelp"] > 0 else "world_rules_family_radius",
            "family_assistance_radius_bits": family_radius,
            "pull_radius_bits": binary32_bits(str(creature["CallForHelp"])) if template_radius else family_radius,
            "pull_radius_supported": template_radius or family_radius is not None,
            "runtime_traversal_proven": False,
            "required_context": ["ordered_current_generation_grid_and_world_traversal",
                "complete_linking_group_instance_and_control_hooks", "current_reactions_range_and_los",
                "nonempty_delayed_dispatch_not_supported", "direct_call_for_help_not_supported"]},
        "cold_loader": {"sql_order_is_runtime_order": False, "required_evidence": COLD_EVIDENCE}}
