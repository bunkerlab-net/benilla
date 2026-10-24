// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored full-raw-shaped inputs, never captured client rows or native admission.
//! Cast16/duration35/range35 retain the parent fixture's authored resolutions.
use super::*;
use crate::native::{synthetic_combat_fixture, synthetic_motion_fixture, synthetic_quest_fixture};
use benilla_sim::{player_progression as pg, quest_lifecycle as q};

fn progression_mainhand_profile() -> Profile {
    let (mut p, mut initial, source) = inputs();
    let (_, _, mut rows) = synthetic_quest_fixture::setup_with_levels(3, 60, true, 0x3f800000);
    p.interactions = None;
    p.npcs[0].ai.neutral_to_all = true;
    // This authored NPC interval leaves an ordinary cast window between hits.
    p.npcs[0].ai.mainhand_interval_ms = 500;
    let init::Rows::Complete(skills) = &mut initial.default_skills else {
        unreachable!()
    };
    skills.push(init::DefaultSkill { id: 136, step: 0 });
    initial.skill_lines.push(init::SkillLineRow {
        id: 136,
        category: 6,
    });
    let item = &mut initial.items[0];
    item.class = 2;
    item.subclass = 10;
    item.inventory_type = 17;
    item.armor = 0;
    item.delay_ms = 2301;
    item.max_durability = 1;
    // Progression's physical-weapon source domain requires one actual damage
    // line. The player remains idle; these fields never authorize autoattack.
    item.damage[0] = init::ItemDamage {
        min_bits: 1.0f32.to_bits(),
        max_bits: 2.0f32.to_bits(),
        school: 0,
    };
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();

    // The same authored level rows as the existing progression fixture, with
    // a complete staff SkillLine dependency and initialized skill state.
    rows.levels = pg::Catalog::new(
        1,
        8,
        rows.levels.rules(),
        (1..=3)
            .map(|level| pg::LevelStatsRow {
                race: 1,
                class: 8,
                level,
                attributes: init::Attributes([17, 23, 29, 31, 37].map(|v| v + level as i32 - 1)),
            })
            .collect(),
        (1..=3)
            .map(|level| pg::ClassLevelStatsRow {
                class: 8,
                level,
                base_health: 5000 + (level - 1) * 10,
                base_mana: 59 + (level - 1) * 6,
            })
            .collect(),
        (1..=3)
            .map(|level| pg::XpRow {
                level,
                xp_for_next_level: level * 40,
            })
            .collect(),
        initial
            .skill_lines
            .iter()
            .map(|line| pg::SkillDependency {
                skill: line.id,
                line: Some(*line),
                race_class: init::Rows::Complete(vec![]),
            })
            .collect(),
    )
    .unwrap();
    rows.skills = [95, 8, 136]
        .into_iter()
        .enumerate()
        .map(|(position, id)| {
            let skill = &p.combat.as_ref().unwrap().skills[&id];
            pg::SkillState {
                id,
                position: position as u8,
                value: skill.value,
                maximum: skill.maximum,
                step: skill.step,
                permanent_bonus: skill.permanent_bonus,
                temporary_bonus: skill.temporary_bonus,
                status: pg::SkillStatus::Unchanged,
            }
        })
        .collect();
    rows.quests[0].special_flags = 0;
    rows.quests[0].required_creature = [0; 4];
    rows.quests[0].required_creature_count = [0; 4];
    let relation = q::CreatureQuestRelation {
        creature: 60001,
        quest: 70001,
    };
    rows.relations = q::QuestRelations {
        starters: vec![relation.clone()],
        finishers: vec![relation],
    };
    rows.npc_rows = vec![QuestNpcRow {
        entry: 60001,
        npc_flags: 2,
        unit_flags: 0,
    }];
    rows.kill.creatures.retain(|row| row.entry == 60001);
    rows.abilities = p
        .abilities
        .iter()
        .cloned()
        .map(|ability| AbilityRecipe {
            ability,
            cast_time: Some(CastTimeRow {
                id: 16,
                base_ms: 200,
                per_level_ms: 0,
                minimum_ms: 0,
            }),
        })
        .collect();
    p.initialize_quest_progression(&initial, rows).unwrap();
    p.source_motion = Some(synthetic_motion_fixture::genesis(&p, 2));
    p.source_motion
        .as_mut()
        .unwrap()
        .spell_hits
        .get_mut(&133)
        .unwrap()
        .attributes
        .copy_from_slice(&source.spells[0].words[6..11]);
    let mut relations = synthetic_combat_fixture::source_rows(&p);
    relations.npcs[0].npc_flags = 2;
    for (i, row) in relations.factions.iter_mut().enumerate() {
        row.faction = 96001 + i as u32;
        row.reputation_list_id = Some(-1);
        row.group_mask = 0;
        row.friend_group_mask = 0;
        row.enemy_group_mask = 0;
    }
    p.initialize_interactions(relations).unwrap();
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: 60001,
        extra_flags: 0,
        static_flags1: 0,
        static_flags2: 0,
        static_flags3: 0,
        static_flags4: 0,
    }])
    .unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
    fixture::initialize_authored_location(&mut p);
    p
}

fn reopened(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}

fn reset_json(w: &NativeWorld) -> serde_json::Value {
    serde_json::to_value(w.checkpoint().unwrap()).unwrap()["state"]["combat"]["melee_reset"].clone()
}

fn mainhand_broken(w: &NativeWorld) -> bool {
    w.state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(15)
        .unwrap()
        .is_broken()
}

#[test]
fn owned_npc_break_preserves_old_release_epoch_and_next_release_uses_unarmed_time() {
    let p = progression_mainhand_profile();
    // Select only a naturally occurring hit/slot draw after the first launch,
    // while its countdown is still nonzero. No live state or RNG is rewritten.
    let (mut w, genesis, old_reset, before_break_remaining) = (0..256)
        .find_map(|seed| {
            let mut w =
                NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), seed)
                    .unwrap();
            let genesis = w.checkpoint().unwrap();
            let mut authority = fixture::spatial_only();
            start(&mut w, &mut authority);
            advance(&mut w, &mut authority);
            advance(&mut w, &mut authority);
            if mainhand_broken(&w) || timers(&w) != [2201, 0] {
                return None;
            }
            let old_reset = reset_json(&w);
            for _ in 0..20 {
                let before = timers(&w)[0];
                advance(&mut w, &mut authority);
                if mainhand_broken(&w) {
                    return (before > 100).then_some((w, genesis, old_reset, before));
                }
            }
            None
        })
        .expect("source RNG reaches a nonlethal mainhand hit-loss after launch");
    let mut authority = fixture::spatial_only();
    assert_eq!(old_reset["intervals_ms"], serde_json::json!([2301, 2000]));
    assert_eq!(
        old_reset["released"]["progression"]["recipe"]["broken_slots"],
        serde_json::json!([])
    );
    assert_eq!(timers(&w), [before_break_remaining - 100, 0]);
    assert_eq!(reset_json(&w), old_reset);
    let combat = w.state.combat.as_ref().unwrap();
    assert_eq!(combat.equipment.item(15).unwrap().current_durability, 0);
    assert!(combat.recipe.snapshot().broken_slots.contains(&15));
    assert_eq!(
        combat
            .source_attack_times(w.profile.combat.as_ref().unwrap())
            .unwrap(),
        [2000, 2000]
    );
    assert!(w.state.health < w.profile.combat.as_ref().unwrap().initial_health);
    let tick = w.last_tick().unwrap();
    assert!(tick
        .draws
        .iter()
        .any(|draw| matches!(draw.purpose, DrawPurpose::DurabilitySlot { .. })));
    assert!(tick.events.iter().any(|event| matches!(event,
        Event::Combat(CombatEvent::Durability(changes)) if changes.stat_transitions.iter().any(|transition|
            matches!(transition, equipment::EquipmentStatTransition::Broken { slot: 15, .. })
        )
    )));
    assert!(w.state.quest_progression.as_ref().unwrap().derived_calls.iter().any(|call|
        matches!(call, DerivedCall::ItemBreak { slot: 15, before_recipe, after_recipe, .. }
            if !before_recipe.broken_slots.contains(&15) && after_recipe.broken_slots.contains(&15))
    ));
    assert_eq!(reopened(&w).checkpoint().unwrap(), w.checkpoint().unwrap());

    // A pre-break release remains authentic under the later physical gear
    // state. Replacing its historical interval/recipe by the current one does not.
    let broken = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for mutation in 0..3 {
        let mut forged = broken.clone();
        match mutation {
            0 => {
                forged["state"]["combat"]["melee_reset"]["intervals_ms"] =
                    serde_json::json!([2000, 2000])
            }
            1 => {
                forged["state"]["combat"]["melee_reset"]["released"]["progression"]["recipe"] =
                    broken["state"]["combat"]["recipe"].clone()
            }
            _ => forged["state"]["quest_progression"]["derived_calls"] = serde_json::json!([]),
        }
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(forged))
                .is_err(),
            "post-break mutation {mutation}"
        );
    }
    while w.state.now_ms < 1600 || !w.state.casts.is_empty() {
        assert!(w.state.tick < 40);
        let before = timers(&w)[0];
        let mut resumed = reopened(&w);
        advance(&mut w, &mut authority);
        advance(&mut resumed, &mut fixture::spatial_only());
        assert_eq!(w.checkpoint().unwrap(), resumed.checkpoint().unwrap());
        assert_eq!(timers(&w), [before.saturating_sub(100), 0]);
        assert_eq!(reset_json(&w), old_reset);
    }
    start(&mut w, &mut authority);
    while reset_json(&w)["cast_id"] == 1 {
        assert!(w.state.tick < 60);
        advance(&mut w, &mut authority);
    }
    assert_eq!(timers(&w), [1900, 0]);
    let valid = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let current = &valid["state"]["combat"]["melee_reset"];
    assert_eq!(current["cast_id"], 2);
    assert_eq!(current["intervals_ms"], serde_json::json!([2000, 2000]));
    for phase in ["prepared", "released"] {
        assert_eq!(
            current[phase]["progression"]["recipe"]["broken_slots"],
            serde_json::json!([15])
        );
    }
    assert!(
        current["released"]["progression"]["derived_prefix"]["count"]
            .as_u64()
            .unwrap()
            > old_reset["released"]["progression"]["derived_prefix"]["count"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(reopened(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    for mutation in 0..4 {
        let mut forged = valid.clone();
        let witness = &mut forged["state"]["combat"]["melee_reset"];
        match mutation {
            0 => witness["intervals_ms"] = serde_json::json!([2301, 2000]),
            1 => witness["released"]["progression"] = old_reset["released"]["progression"].clone(),
            2 => {
                witness["released"]["progression"]["derived_prefix"] =
                    old_reset["released"]["progression"]["derived_prefix"].clone()
            }
            _ => *witness = old_reset.clone(),
        }
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(forged))
                .is_err(),
            "new-release mutation {mutation}"
        );
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
}
