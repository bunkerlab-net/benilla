// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::classic_spell::SpellFloat;

#[test]
fn explicit_policy_rejects_inconsistent_consumers_atomically() {
    for field in ["progression", "kill", "reputation", "loot"] {
        let (mut profile, input, mut rows) = synthetic_loot_fixture::setup();
        match field {
            "progression" => rows.world_rules.progression.always_max_skill_for_level = false,
            "kill" => rows.world_rules.kill.kill_rate_bits = 0,
            "reputation" => rows.world_rules.reputation.gain_rate = SpellFloat::ZERO,
            "loot" => rows.world_rules.loot.money_rate_bits = 0,
            _ => unreachable!(),
        }
        let before = profile.clone();
        assert!(
            profile.initialize_quest_progression(&input, rows).is_err(),
            "{field}"
        );
        assert_eq!(profile, before);
    }
}

#[test]
fn absent_consumers_require_stock_inactive_settings() {
    for field in ["reputation", "loot"] {
        let (mut profile, input, mut rows) = synthetic_quest_fixture::setup();
        match field {
            "reputation" => rows.world_rules.reputation.gain_rate = SpellFloat::ZERO,
            "loot" => rows.world_rules.loot.quality_rate_bits[0] = 0,
            _ => unreachable!(),
        }
        let error = profile
            .initialize_quest_progression(&input, rows)
            .unwrap_err();
        assert_eq!(
            error,
            NativeError::UnsupportedMechanic(match field {
                "reputation" => "reputation rate configured but no reputation host",
                _ => "drop rate configured but no loot host",
            })
        );
    }
}

#[test]
fn bound_rates_and_start_money_change_effective_profile_identity() {
    let (mut baseline, input, rows) = synthetic_quest_fixture::setup();
    baseline
        .initialize_quest_progression(&input, rows.clone())
        .unwrap();
    for field in ["kill", "quest", "money"] {
        let (mut changed, _, _) = synthetic_quest_fixture::setup();
        let mut rows = rows.clone();
        match field {
            "kill" => {
                rows.world_rules.kill.kill_rate_bits = 0;
                rows.kill.rules = rows.world_rules.kill;
            }
            "quest" => rows.world_rules.quest_xp_rate_bits = 0,
            "money" => rows.world_rules.start_player_money += 1,
            _ => unreachable!(),
        }
        changed.initialize_quest_progression(&input, rows).unwrap();
        assert_ne!(changed.digest(), baseline.digest(), "{field}");
    }
}

#[test]
fn combat_consumers_and_absent_hosts_refuse_every_unbound_setting() {
    let fields = [
        "aggro_rate_bits",
        "family_assistance_radius_bits",
        "family_assistance_delay_ms",
        "family_flee_assistance_radius_bits",
        "family_flee_delay_ms",
        "check_for_help_radius_bits",
        "check_for_help_aggro_delay_ms",
        "linking_aggro_delay_ms",
        "respawn_aggro_delay_ms",
        "relocation_notify_delay_ms",
        "leash_radius_bits",
        "listen_say_range_bits",
        "listen_yell_range_bits",
        "listen_text_emote_range_bits",
    ];
    for field in fields {
        let (mut profile, input, mut rows) = synthetic_quest_fixture::setup();
        let mut value = serde_json::to_value(&rows.world_rules).unwrap();
        value["combat"][field] = 0.into();
        rows.world_rules = serde_json::from_value(value).unwrap();
        let before = profile.clone();
        assert!(
            profile.initialize_quest_progression(&input, rows).is_err(),
            "{field}"
        );
        assert_eq!(profile, before, "{field}");
    }
}

#[test]
fn bound_combat_policy_retains_effective_ai_motion_and_text_settings() {
    let (mut profile, input, mut rows) = synthetic_distinct_quest_fixture::setup();
    for npc in &mut profile.npcs {
        npc.ai.aggro_rate = ai::Real::new(0.5).unwrap();
        npc.ai.leash_radius = ai::Real::new(40.0).unwrap();
    }
    profile.source_motion.as_mut().unwrap().aggro_rate = ai::Real::new(0.5).unwrap();
    rows.world_rules.combat.aggro_rate_bits = 0.5f32.to_bits();
    rows.world_rules.combat.leash_radius_bits = 40f32.to_bits();
    rows.world_rules.combat.listen_say_range_bits = 35f32.to_bits();
    rows.quest_script_rules = Some(QuestScriptRules {
        say_range: ai::Real::new(35.0).unwrap(),
        visibility_distance: ai::Real::new(100.0).unwrap(),
    });
    profile
        .initialize_quest_progression(&input, rows.clone())
        .unwrap();
    assert_eq!(
        profile.quest_progression.as_ref().unwrap().combat_rules(),
        &rows.world_rules.combat
    );
    for field in [
        "aggro",
        "leash",
        "initial_delay",
        "relocation_delay",
        "remove_motion",
    ] {
        let mut changed = profile.clone();
        match field {
            "aggro" => {
                for npc in &mut changed.npcs {
                    npc.ai.aggro_rate = ai::Real::ONE;
                }
                changed.source_motion.as_mut().unwrap().aggro_rate = ai::Real::ONE;
            }
            "leash" => {
                for npc in &mut changed.npcs {
                    npc.ai.leash_radius = ai::Real::new(30.0).unwrap();
                }
            }
            "initial_delay" => {
                changed
                    .source_motion
                    .as_mut()
                    .unwrap()
                    .creature_initial_aggro_delay_ms += 1
            }
            "relocation_delay" => {
                changed
                    .source_motion
                    .as_mut()
                    .unwrap()
                    .relocation_aggro_delay_ms += 1
            }
            _ => changed.source_motion = None,
        }
        assert!(changed.validate().is_err(), "{field}");
    }
}

#[test]
fn bound_family_and_peer_help_policy_rejects_later_host_substitution() {
    let (profile, input, mut rows) = synthetic_distinct_quest_fixture::setup();
    let mut profile = synthetic_creature_model_fixture::with_complete_scene_generations(profile);
    let sources: Vec<_> = profile
        .npcs
        .iter()
        .map(|npc| CombatAssistanceSource {
            spawn_id: npc.ai.creature_id,
            entry: npc.entry,
            generation_source: profile
                .creature_generations
                .as_ref()
                .unwrap()
                .source(npc.ai.creature_id)
                .unwrap()
                .identity(),
            call_for_help: ai::Real::ZERO,
            excluded_hooks: SourceVisitorRows::Complete(vec![]),
        })
        .collect();
    profile
        .initialize_combat_assistance(sources.clone(), 77, ai::Real::new(7.0).unwrap())
        .unwrap();
    profile
        .initialize_peer_help_radius(ai::Real::new(9.0).unwrap())
        .unwrap();
    rows.world_rules.combat.family_assistance_radius_bits = 7f32.to_bits();
    rows.world_rules.combat.check_for_help_aggro_delay_ms = 77;
    rows.world_rules.combat.check_for_help_radius_bits = 9f32.to_bits();
    profile.initialize_quest_progression(&input, rows).unwrap();
    let before = profile.clone();
    assert!(profile
        .initialize_peer_help_radius(ai::Real::new(5.0).unwrap())
        .is_err());
    assert_eq!(profile, before);
    assert!(profile
        .initialize_combat_assistance(sources, 77, ai::Real::new(5.0).unwrap())
        .is_err());
    assert_eq!(profile, before);
}

#[test]
fn event_ai_and_quest_text_must_share_the_configured_say_range() {
    let (profile, input, mut rows) = synthetic_distinct_quest_fixture::setup_with_unbound_spells();
    let mut profile = synthetic_event_ai_fixture::install(profile, 100);
    let mut accepted = profile.clone();
    accepted
        .initialize_quest_progression(&input, rows.clone())
        .unwrap();
    rows.world_rules.combat.listen_say_range_bits = 35f32.to_bits();
    rows.quest_script_rules = Some(QuestScriptRules {
        say_range: ai::Real::new(35.0).unwrap(),
        visibility_distance: ai::Real::new(100.0).unwrap(),
    });
    let before = profile.clone();
    assert_eq!(
        profile
            .initialize_quest_progression(&input, rows)
            .unwrap_err(),
        NativeError::UnsupportedMechanic("world combat rules/text audience host mismatch"),
    );
    assert_eq!(profile, before);
}
