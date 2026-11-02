// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored staff, stats and faction projections; no database or DBC rows.
use super::*;
use crate::native::{
    synthetic_fixture as f, synthetic_player_melee_fixture as fixture,
    synthetic_player_melee_fixture::CapturedStaffAuthority, synthetic_quest_fixture as quests,
};
use benilla_sim::player_melee::{
    CreatureDefense, MageAttack, MeleeInput, MeleeOutcome, MeleeRolls,
};

fn world(health: u32) -> NativeWorld {
    NativeWorld::synthetic_component_test(
        fixture::captured_staff_setup(health).0,
        [1; 16],
        f::player(),
        42,
    )
    .unwrap()
}
fn binding(world: &NativeWorld) -> quest_progression::MeleeExecutionBinding {
    quest_progression::MeleeExecutionBinding::capture(&world.profile, &world.state).unwrap()
}
fn defense(level: u8) -> CreatureDefense {
    CreatureDefense {
        level,
        armor: 60,
        can_react: true,
        casting_non_melee: false,
        can_dodge: false,
        can_parry: false,
        can_block: false,
        front_facing: true,
        shield_block_value: 0,
    }
}

#[test]
fn captured_staff_learning_outfit_and_live_level_damage_use_existing_host() {
    let mut w = world(1);
    let before = binding(&w).current_operands(&w.profile).unwrap();
    assert_eq!(
        (before.level, before.weapon_item, before.skill_id),
        (1, Some(35), 136)
    );
    assert_eq!(before.attack_times_ms, [2000, 2000]);
    assert_eq!(
        (before.minimum_damage.bits(), before.maximum_damage.bits()),
        (4.0f32.to_bits(), 8.0f32.to_bits())
    );
    assert_eq!(
        w.state.npcs[&2].initialized_creature().unwrap().stats.armor,
        60
    );
    let mut authority = CapturedStaffAuthority::default();
    let target = w.state.npcs[&2].id;
    let command = quests::command(&w, Command::AttackStart { target });
    w.step(1, Some(command), &mut authority).unwrap();
    for _ in 0..200 {
        if w.state.npcs[&2].health == 0 {
            break;
        }
        w.step(w.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_eq!(w.state.npcs[&2].health, 0);
    let after = binding(&w).current_operands(&w.profile).unwrap();
    assert_eq!(after.level, 2);
    // Strength 18 gives AP8, hence 3+8/7 and 7+8/7 at two seconds per swing.
    assert_eq!(
        (after.minimum_damage.bits(), after.maximum_damage.bits()),
        (0x4084_9249, 0x4102_4925)
    );
    for source in [before, after] {
        let plan = melee::prepare(MeleeInput {
            attacker: MageAttack {
                level: source.level,
                weapon_skill: source.effective_skill,
                minimum_damage: source.minimum_damage,
                maximum_damage: source.maximum_damage,
                critical_chance: source.critical_chance,
                hit_chance: source.hit_chance,
            },
            defender: defense(source.level),
        })
        .unwrap();
        assert_eq!(plan.damage_range(), (4, 8));
        assert_eq!(plan.chances().glance, 0);
        for (damage, expected) in [(4, 3), (8, 7)] {
            let result = melee::resolve(
                plan,
                MeleeRolls {
                    damage,
                    outcome: 10000,
                    glance: None,
                },
            )
            .unwrap();
            assert_eq!(result.outcome(), MeleeOutcome::Normal);
            assert_eq!(result.total_damage(), expected);
        }
    }
}

#[test]
fn captured_staff_deterministic_outcome_table_keeps_low_level_glancing_disabled() {
    let w = world(60);
    let source = binding(&w).current_operands(&w.profile).unwrap();
    let input = |level| MeleeInput {
        attacker: MageAttack {
            level,
            weapon_skill: u16::from(level) * 5,
            minimum_damage: source.minimum_damage,
            maximum_damage: source.maximum_damage,
            critical_chance: f::float(5.0),
            hit_chance: f::float(0.0),
        },
        defender: defense(level),
    };
    let low = melee::prepare(input(1)).unwrap();
    assert_eq!(
        (
            low.chances().miss,
            low.chances().critical,
            low.chances().glance
        ),
        (500, 500, 0)
    );
    for (die, outcome, expected) in [
        (1, MeleeOutcome::Miss, 0),
        (500, MeleeOutcome::Miss, 0),
        (501, MeleeOutcome::Critical, 14),
        (1000, MeleeOutcome::Critical, 14),
        (1001, MeleeOutcome::Normal, 7),
        (10000, MeleeOutcome::Normal, 7),
    ] {
        let hit = melee::resolve(
            low,
            MeleeRolls {
                damage: 8,
                outcome: die,
                glance: None,
            },
        )
        .unwrap();
        assert_eq!((hit.outcome(), hit.total_damage()), (outcome, expected));
    }
    // RollMeleeOutcomeAgainst permits glancing only when the victim is above level ten.
    let high = melee::prepare(input(11)).unwrap();
    assert_eq!(
        (
            high.chances().miss,
            high.chances().glance,
            high.chances().critical
        ),
        (500, 1100, 500)
    );
    for (die, glance, outcome, expected) in [
        (500, None, MeleeOutcome::Miss, 0),
        (501, Some(60), MeleeOutcome::Glancing, 4),
        (1600, Some(89), MeleeOutcome::Glancing, 6),
        (1601, None, MeleeOutcome::Critical, 14),
        (2101, None, MeleeOutcome::Normal, 7),
    ] {
        let hit = melee::resolve(
            high,
            MeleeRolls {
                damage: 8,
                outcome: die,
                glance,
            },
        )
        .unwrap();
        assert_eq!((hit.outcome(), hit.total_damage()), (outcome, expected));
    }
}

#[test]
fn neutral_faction_25_is_attackable_and_retaliates_but_friendly_target_is_denied() {
    let mut w = world(60);
    let mut authority = CapturedStaffAuthority::default();
    for tick in 1..=3 {
        w.step(tick, None, &mut authority).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    assert_eq!(w.profile.npcs[0].faction_template, 25);
    let target = w.state.npcs[&2].id;
    let relation = interactions::player_melee::MeleeRelationContext::capture(
        &w.profile,
        &w.state,
        &w.state.npcs[&2],
    )
    .unwrap();
    assert!(relation.can_attack_now(&w.profile).unwrap());
    assert!(relation.victim_can_attack(&w.profile).unwrap());
    let friendly = w.state.npcs[&3].id;
    let command = quests::command(&w, Command::AttackStart { target: friendly });
    w.step(4, Some(command), &mut authority).unwrap();
    assert!(w.last_tick().unwrap().events.iter().any(|event| matches!(
        event,
        Event::InputResult {
            accepted: false,
            ..
        }
    )));
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .melee_timers
        .victim()
        .is_none());
    let command = quests::command(&w, Command::AttackStart { target });
    w.step(5, Some(command), &mut authority).unwrap();
    for _ in 0..100 {
        if w.state.npcs[&2].ai.phase() == ai::Phase::Combat {
            break;
        }
        w.step(w.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Combat);
    assert!(w.records().iter().any(|record| record
        .events
        .iter()
        .any(|event| matches!(event, Event::PlayerMelee { .. }))));
    let mut resumed =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    w.step(w.state.tick + 1, None, &mut authority).unwrap();
    resumed
        .step(
            resumed.state.tick + 1,
            None,
            &mut CapturedStaffAuthority::default(),
        )
        .unwrap();
    assert_eq!(w.last_tick(), resumed.last_tick());
    assert_eq!(w.state_digest(), resumed.state_digest());
}

#[test]
fn captured_staff_refusals_do_not_install_or_replace_policy() {
    let (profile, original) = fixture::captured_staff_setup(60);
    type Change = fn(&mut init::StarterInput);
    let cases: [(&str, Change); 10] = [
        ("captured melee item_template[35]", |input| {
            input.items.retain(|row| row.id != 35)
        }),
        ("captured melee two-hand staff type", |input| {
            input
                .items
                .iter_mut()
                .find(|row| row.id == 35)
                .unwrap()
                .subclass = 7
        }),
        ("captured melee two-hand staff type", |input| {
            input
                .items
                .iter_mut()
                .find(|row| row.id == 35)
                .unwrap()
                .inventory_type = 22
        }),
        ("captured melee two-hand staff type", |input| {
            input
                .items
                .iter_mut()
                .find(|row| row.id == 35)
                .unwrap()
                .subclass = 19
        }),
        ("captured melee positive bounded staff delay", |input| {
            input
                .items
                .iter_mut()
                .find(|row| row.id == 35)
                .unwrap()
                .delay_ms = 0
        }),
        ("captured melee one physical damage line", |input| {
            input
                .items
                .iter_mut()
                .find(|row| row.id == 35)
                .unwrap()
                .damage[0]
                .school = 2
        }),
        ("captured melee one physical damage line", |input| {
            input
                .items
                .iter_mut()
                .find(|row| row.id == 35)
                .unwrap()
                .damage[1]
                .max_bits = 1.0f32.to_bits()
        }),
        ("captured melee staff/unarmed SkillLine", |input| {
            input.skill_lines.retain(|row| row.id != 136)
        }),
        ("captured melee starter content binding", |input| {
            input.create_attributes.0[0] += 1
        }),
        ("captured melee starter content binding", |input| {
            input.skill_race_class = init::Rows::Missing
        }),
    ];
    for (reason, change) in cases {
        let mut p = profile.clone();
        p.player_melee = None;
        let before = p.digest();
        let mut input = original.clone();
        change(&mut input);
        assert!(
            matches!(p.initialize_captured_player_melee(&input, 1),
            Err(NativeError::UnsupportedMechanic(actual)) if actual == reason),
            "{reason}"
        );
        assert_eq!(p.digest(), before);
        assert!(p.player_melee.is_none());
    }
    for slot in [16, 17] {
        let mut p = profile.clone();
        let weapons = &mut p.combat.as_mut().unwrap().equipment_weapons;
        let mut extra = weapons.iter().find(|row| row.slot == 15).unwrap().clone();
        extra.slot = slot;
        weapons.push(extra);
        assert!(matches!(
            p.initialize_captured_player_melee(&original, 1),
            Err(NativeError::UnsupportedMechanic(
                "captured melee offhand/ranged equipment"
            ))
        ));
    }
    let mut missing_skill = profile.clone();
    missing_skill.combat.as_mut().unwrap().skills.remove(&136);
    assert!(matches!(
        missing_skill.initialize_captured_player_melee(&original, 1),
        Err(NativeError::UnsupportedMechanic(
            "captured melee initialized staff/unarmed skill"
        ))
    ));
    let mut unknown = profile.clone();
    unknown.interactions = None;
    let before = unknown.digest();
    assert!(unknown
        .initialize_captured_player_melee(&original, 1)
        .is_err());
    assert_eq!(unknown.digest(), before);
    let mut invalid_gain = profile;
    let before = invalid_gain.digest();
    assert!(invalid_gain
        .initialize_captured_player_melee(&original, u32::MAX)
        .is_err());
    assert_eq!(invalid_gain.digest(), before);
}
