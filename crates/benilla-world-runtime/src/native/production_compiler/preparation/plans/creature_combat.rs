// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
// CreatureAISelector.cpp:38-90; CreatureAIRegistry.cpp:35-43;
// CreatureEventAI.cpp:51-62; Unit.cpp:6228-6238,11768-11805;
// Creature.cpp:555-559,2154-2227. See COMBAT-ASSISTANCE.md for runtime limits.
use super::*;

pub(super) const COMBAT_POLICY: &[&str] = &[
    "aggro_rate_binary32",
    "initial_and_relocation_aggro_delay_ms",
    "aggro_help_delay_ms",
    "family_assistance_radius_binary32",
    "family_assistance_delay_ms",
    "check_for_help_radius_binary32",
    "leash_and_melee_policy",
];
pub(super) const COLD_EVIDENCE: &[&str] = &[
    "source_and_configuration_identity",
    "map_and_grid_activation_sequence",
    "persistent_spawn_state",
    "ordered_cell_grid_and_world_membership",
    "cold_entry_and_consecutive_update_order",
    "shared_rng_draw_schedule",
];

pub(super) fn plan(
    creature: &Value,
    generation: &Value,
    event_scope: &Value,
    event_plan: &Value,
    dbcs: &BTreeMap<String, dbc::Table>,
) -> Value {
    let name = creature["AIName"].as_str().unwrap();
    // Only the ordinary uncharmed non-pet/non-totem branch is inferred here.
    // Empty SQL ScriptName is not independently complete script/control evidence.
    let (factory, branch) = if nonzero(&creature["ScriptName"]) {
        (None, "script_registry_required")
    } else if !name.is_empty() {
        if [
            "NullAI",
            "GuardAI",
            "PetAI",
            "TotemAI",
            "EventAI",
            "GuardianAI",
            "PossessedAI",
        ]
        .contains(&name)
        {
            (Some(name), "explicit_name")
        } else {
            (
                Some("NullCreatureAI"),
                "unregistered_name_falls_back_to_null",
            )
        }
    } else if int(&creature["ExtraFlags"]) & 0x400 != 0 {
        (Some("GuardAI"), "guard_flag")
    } else {
        (Some("EventAI"), "highest_permit")
    };
    let faction = dbcs
        .get("FactionTemplate")
        .and_then(|t| t.get(int(&creature["Faction"]) as u32));
    let neutral = faction.and_then(|faction| {
        if faction.word(1) == 0 {
            Some(true)
        } else {
            dbcs.get("Faction").map(|table| {
                let raw = table.get(faction.word(1));
                if raw.is_some_and(|row| row.word(1) as i32 >= 0) {
                    false
                } else {
                    (6..10).all(|i| faction.word(i) == 0)
                        && faction.word(4) == 0
                        && faction.word(5) == 0
                }
            })
        }
    });
    let extra = int(&creature["ExtraFlags"]);
    let flags1 = int(&creature["StaticFlags1"]);
    let flags2 = int(&creature["StaticFlags2"]);
    let defensive = extra & 2 != 0 || flags2 & 0x4000000 != 0;
    let combat_allowed = !(flags1 & 0x2000000 != 0 && flags2 & 0x4000000 == 0);
    let event_supported = !event_scope.is_null()
        && (event_scope["matching_rows"].as_array().unwrap().is_empty() || !event_plan.is_null());
    let mut refusals = vec!["effective_script_registry_control_and_spawn_selection_required"];
    if factory != Some("EventAI") {
        refusals.push("selected_factory_not_supported");
    }
    if !event_supported {
        refusals.push("event_ai_execution_or_coverage_required");
    }
    if flags1 != 0
        || flags2 & !0x150 != 0
        || nonzero(&creature["StaticFlags3"])
        || nonzero(&creature["StaticFlags4"])
    {
        refusals.push("combat_authority_static_flags_unsupported");
    }
    if extra & !0x80002 != 0 {
        refusals.push("combat_authority_extra_flags_unsupported");
    }
    let generation_complete = generation["status"] == "complete_inputs";
    if !generation_complete {
        refusals.push("creature_generation_inputs_required");
    }
    refusals.push("source_melee_motion_hooks_and_current_target_context_required");
    let permit = if name == "EventAI" {
        Some(800)
    } else if extra & 2 != 0 || neutral == Some(true) {
        Some(100)
    } else {
        neutral.map(|_| 200)
    };
    json!({"policy":"cmangos-8ec338a1-creature-combat-evidence-v1","production_admission":false,
        "ai_selection":{"scope":"ordinary_uncontrolled_creature_before_external_overrides",
            "factory":factory,"branch":branch,"neutral_to_all":neutral,"event_ai_permit":permit,
            "initial_reaction":if defensive {"Defensive"} else {"Aggressive"},
            "can_enter_combat_from_static_flags":combat_allowed,
            "event_rows_supported":event_supported,"refusals":refusals},
        "melee":{"runtime_consumer":"existing_creature_generation_and_native_mainhand",
            "base_attack_time_ms":creature["MeleeBaseAttackTime"],"damage_school":creature["DamageSchool"],
            "generation_inputs_complete":generation_complete,"resolved_combat_stats":null},
        "aggro":{"runtime_consumer":"creature_ai::attack_distance_and_source_combat_authority",
            "detection_range":creature["Detection"],"level_interval":[creature["MinLevel"],creature["MaxLevel"]],
            "neutral_is_not_forced_hostile":true,"required_policy":COMBAT_POLICY,
            "required_context":["directional_faction_and_current_reputation","detect_range_aura_closure",
                "attackability_control_and_event_notify_state","current_spatial_evidence"]},
        "assistance":{"call_for_help":creature["CallForHelp"],
            "pull_radius_source":if int(&creature["CallForHelp"]) > 0 {"template"} else {"world_rules_family_radius"},
            "pull_radius_supported":int(&creature["CallForHelp"]) > 0,"runtime_traversal_proven":false,
            "required_context":["ordered_current_generation_grid_and_world_traversal",
                "complete_linking_group_instance_and_control_hooks","current_reactions_range_and_los",
                "nonempty_delayed_dispatch_not_supported","direct_call_for_help_not_supported"]},
        "cold_loader":{"sql_order_is_runtime_order":false,"required_evidence":COLD_EVIDENCE}})
}
