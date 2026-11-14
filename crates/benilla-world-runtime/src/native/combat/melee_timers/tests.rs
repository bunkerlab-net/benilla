// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored synthetic initializer rows; no installed content or admission.
use super::*;
use crate::native::synthetic_fixture::{initialized_profile, initialized_starter_input};

fn weapon_profile(delay: u32) -> Profile {
    let mut input = initialized_starter_input();
    let init::Rows::Complete(skills) = &mut input.default_skills else {
        unreachable!()
    };
    skills.push(init::DefaultSkill { id: 136, step: 0 });
    input.skill_lines.push(init::SkillLineRow {
        id: 136,
        category: 6,
    });
    let item = &mut input.items[0];
    item.class = 2;
    item.subclass = 10;
    item.inventory_type = 17;
    item.delay_ms = delay;
    // All five damage lines remain zero: IsWeapon is the actual class predicate.
    assert!(item.damage.iter().all(|d| d.max_bits == 0));
    let mut profile = initialized_profile();
    profile
        .initialize_player(&input, spell::SpellFloat::ZERO, 1)
        .unwrap();
    profile
}

fn state(profile: &Profile) -> PlayerCombatState {
    PlayerCombatState::new(profile.combat.as_ref().unwrap(), 0).unwrap()
}

#[test]
fn initialized_zero_timers_actual_delay_and_zero_damage_weapon() {
    for delay in [0, 1, 1700, 2301, 16_777_217, 16_777_219] {
        let profile = weapon_profile(delay);
        let genesis = profile.combat.as_ref().unwrap();
        let mut state = state(&profile);
        assert_eq!(state.melee_timers.remaining_ms(), [0, 0]);
        let expected = match delay {
            0 => 2000,
            16_777_217 => 16_777_216,
            16_777_219 => 16_777_220,
            _ => delay,
        };
        assert_eq!(
            state.source_attack_times(genesis).unwrap(),
            [expected, 2000]
        );
        assert_eq!(state.reset_melee_timers(genesis).unwrap(), [expected, 2000]);
        assert_eq!(state.melee_timers.remaining_ms(), [expected, 0]);
        state.advance_melee_timers(100);
        assert_eq!(
            state.melee_timers.remaining_ms(),
            [expected.saturating_sub(100), 0]
        );
        state.advance_melee_timers(u32::MAX);
        assert_eq!(state.melee_timers.remaining_ms(), [0, 0]);
    }
}

#[test]
fn proven_absent_weapon_is_distinct_from_missing_source() {
    let profile = initialized_profile();
    let genesis = profile.combat.as_ref().unwrap();
    let mut state = state(&profile);
    assert_eq!(state.reset_melee_timers(genesis).unwrap(), [2000, 2000]);
    assert_eq!(state.melee_timers.remaining_ms(), [2000, 0]);
    let mut missing = genesis.clone();
    missing.equipment_weapons.clear();
    assert!(state.reset_melee_timers(&missing).is_err());
    let mut missing = genesis.clone();
    missing.equipment.items.clear();
    assert!(state.reset_melee_timers(&missing).is_err());
    let mut input = initialized_starter_input();
    input.item_grants = init::Rows::Missing;
    let mut rejected = profile.clone();
    assert!(rejected
        .initialize_player(&input, spell::SpellFloat::ZERO, 1)
        .is_err());
    assert_eq!(rejected, profile);
}

#[test]
fn source_break_updates_attack_time_before_zero_durability_without_reset() {
    for delay in [0, 700, 2301] {
        let profile = weapon_profile(delay);
        let genesis = profile.combat.as_ref().unwrap();
        let mut state = state(&profile);
        state.reset_melee_timers(genesis).unwrap();
        state.advance_melee_timers(100);
        let before = state.melee_timers.remaining_ms();
        let mut equipment = state.equipment.clone();
        let changes = equipment
            .apply_hit_loss::<NativeError, _>(MAINHAND, |_, _, changes| {
                assert_eq!(
                    state.equipment.item(MAINHAND).unwrap().current_durability,
                    1
                );
                assert_eq!(changes.stat_transitions.len(), 1);
                state.apply_item_break(
                    MAINHAND,
                    init::Resources {
                        health: profile.player.current_health,
                        max_health: state.stats.max_health,
                        mana: profile.player.current_mana,
                        max_mana: state.stats.max_mana,
                    },
                )?;
                assert_eq!(
                    state.melee_timers.attack_time_fields,
                    [BASE_ATTACK_FIELD; 2]
                );
                assert_eq!(state.melee_timers.remaining_ms(), before);
                assert_eq!(
                    state.equipment.item(MAINHAND).unwrap().current_durability,
                    1
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(changes.stat_transitions.len(), 1);
        state.equipment = equipment;
        state.validate_melee_timers(genesis).unwrap();
        assert_eq!(state.melee_timers.remaining_ms(), before);
        assert_eq!(state.source_attack_times(genesis).unwrap(), [2000, 2000]);
        state.reset_melee_timers(genesis).unwrap();
        assert_eq!(state.melee_timers.remaining_ms(), [2000, 0]);
    }
}

#[test]
fn equipment_class_slot_delay_and_durability_are_retained_dependencies() {
    let profile = weapon_profile(2301);
    let genesis = profile.combat.as_ref().unwrap();
    let state = state(&profile);
    for mutation in 0..8 {
        let mut changed = genesis.clone();
        match mutation {
            0 => changed.equipment_weapons[0].item_id += 1,
            1 => changed.equipment_weapons[0].class = 4,
            2 => changed.equipment_weapons[0].subclass = 19,
            3 => changed.equipment_weapons[0].inventory_type = 5,
            4 => changed.equipment_weapons[0].slot = OFFHAND,
            5 => changed.equipment_weapons[0].delay_ms = 2401,
            6 => changed.equipment.items[0].max_durability += 1,
            7 => changed.equipment.items[0].current_durability = 0,
            _ => unreachable!(),
        }
        assert!(state.validate_melee_timers(&changed).is_err(), "{mutation}");
    }
    // A valid raw delay mutation must still change profile identity even when
    // nearest-even binary32 conversion produces the same source attack time.
    let low = weapon_profile(16_777_216);
    let rounded = weapon_profile(16_777_217);
    assert_ne!(low.digest(), rounded.digest());
    assert_eq!(
        low.combat.as_ref().unwrap().melee_timers,
        rounded.combat.as_ref().unwrap().melee_timers
    );
}

#[test]
fn offhand_ranged_and_unknown_class_never_become_absent_weapon() {
    let profile = weapon_profile(2301);
    let genesis = profile.combat.as_ref().unwrap();
    for slot in [OFFHAND, RANGED] {
        let mut weapons = genesis.equipment_weapons.clone();
        let mut equipment = genesis.equipment.clone();
        let mut recipe = genesis.recipe.snapshot();
        weapons[0].slot = slot;
        equipment.items[0].slot = slot;
        recipe.items[0].slot = slot;
        assert!(PlayerMeleeTimers::from_initializer(&weapons, &equipment, &recipe).is_err());
    }
    let mut weapons = genesis.equipment_weapons.clone();
    let mut recipe = genesis.recipe.snapshot();
    weapons[0].class = 0;
    recipe.items[0].weapon = false;
    assert!(PlayerMeleeTimers::from_initializer(&weapons, &genesis.equipment, &recipe).is_err());
}

#[test]
fn restore_rejects_attacking_victim_form_disarm_speed_or_source_time() {
    let profile = weapon_profile(2301);
    let genesis = profile.combat.as_ref().unwrap();
    let state = state(&profile);
    for mutation in 0..8 {
        let mut snapshot = state.snapshot();
        match mutation {
            0 => snapshot.melee_timers.melee_attacking = true,
            1 => {
                snapshot.melee_timers.victim = Some(crate::NpcId {
                    spawn: 2,
                    generation: 1,
                })
            }
            2 => snapshot.melee_timers.shapeshift_form = 1,
            3 => snapshot.melee_timers.disarmed = true,
            4 => snapshot.melee_timers.attack_speed_factors[0] = 0x4000_0000,
            5 => snapshot.melee_timers.attack_time_fields[0] = BASE_ATTACK_FIELD,
            6 => snapshot.melee_timers.remaining_ms[1] = 1,
            7 => snapshot.melee_timers.remaining_ms[0] = 2302,
            _ => unreachable!(),
        }
        assert!(
            PlayerCombatState::restore(
                genesis,
                snapshot,
                profile.player.current_health,
                profile.player.current_mana,
                0,
                0,
                0,
                profile.tick_ms,
            )
            .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn old_or_incomplete_timer_state_has_no_serde_defaults() {
    let profile = initialized_profile();
    let state = state(&profile);
    let serialized = serde_json::to_value(state.snapshot()).unwrap();
    for field in ["melee_timers", "melee_reset"] {
        let mut missing = serialized.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<CombatSnapshot>(missing).is_err(),
            "{field}"
        );
    }
    for field in [
        "attack_time_fields",
        "attack_speed_factors",
        "remaining_ms",
        "melee_attacking",
        "victim",
        "shapeshift_form",
        "disarmed",
    ] {
        let mut missing = serialized.clone();
        missing["melee_timers"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<CombatSnapshot>(missing).is_err(),
            "{field}"
        );
    }
}

#[test]
fn unsigned_binary32_rounding_and_source_truncation_boundaries() {
    assert_eq!(attack_field(2000), BASE_ATTACK_FIELD);
    assert_eq!(
        source_attack_time(attack_field(16_777_217)).unwrap(),
        16_777_216
    );
    assert_eq!(
        source_attack_time(attack_field(16_777_219)).unwrap(),
        16_777_220
    );
    assert_eq!(
        source_attack_time(attack_field(33_554_435)).unwrap(),
        33_554_436
    );
    assert_eq!(
        source_attack_time(attack_field(u32::MAX - 128)).unwrap(),
        4_294_967_040
    );
    assert!(source_attack_time(attack_field(u32::MAX)).is_err());
    assert!(source_attack_time(0x7f80_0000).is_err());
    assert!(source_attack_time(0x7fc0_0000).is_err());
    assert!(source_attack_time(0xc4fa_0000).is_err());
}

#[test]
fn unchanged_source_numeric_subset_matches_native_timer_arithmetic() {
    use sha2::{Digest as _, Sha256};
    let fixture = include_str!("source_oracle.tsv");
    let hash = Sha256::digest(fixture.as_bytes());
    assert_eq!(
        format!("{hash:x}"),
        "7ecbb38248134df377551e4c1548eac86f61a3b9f8e01143a2e793589551e13f"
    );
    let mut rows = BTreeMap::new();
    for line in fixture.lines().filter(|line| !line.starts_with('#')) {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 3);
        let value = columns[2].parse::<u32>().unwrap();
        assert!(rows
            .insert((columns[0].to_owned(), columns[1].to_owned()), value)
            .is_none());
    }
    assert_eq!(rows.len(), 50);
    let mut compare = |case: &str, field: &str, actual: u32| {
        assert_eq!(
            rows.remove(&(case.to_owned(), field.to_owned())),
            Some(actual),
            "{case}:{field}"
        );
    };
    let absent = initialized_profile();
    let absent_genesis = absent.combat.as_ref().unwrap();
    let fresh = state(&absent);
    for index in 0..2 {
        let case = format!("initial_{index}");
        compare(&case, "timer", fresh.melee_timers.remaining_ms()[index]);
        compare(
            &case,
            "delay",
            fresh.source_attack_times(absent_genesis).unwrap()[index],
        );
    }
    let break_weapon = |profile: &Profile, state: &mut PlayerCombatState| {
        let mut equipment = state.equipment.clone();
        let changes = equipment
            .apply_hit_loss::<NativeError, _>(MAINHAND, |_, _, _| {
                state.apply_item_break(
                    MAINHAND,
                    init::Resources {
                        health: profile.player.current_health,
                        max_health: state.stats.max_health,
                        mana: profile.player.current_mana,
                        max_mana: state.stats.max_mana,
                    },
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(changes.stat_transitions.len(), 1);
        state.equipment = equipment;
    };
    for delay in [0, 1, 1700, 2000, 2900, 4000] {
        let profile = weapon_profile(delay);
        let genesis = profile.combat.as_ref().unwrap();
        let healthy = state(&profile);
        let mut broken = healthy.clone();
        break_weapon(&profile, &mut broken);
        for (mode, actual) in [
            fresh.source_attack_times(absent_genesis).unwrap()[0],
            healthy.source_attack_times(genesis).unwrap()[0],
            broken.source_attack_times(genesis).unwrap()[0],
        ]
        .into_iter()
        .enumerate()
        {
            compare(&format!("weapon_{delay}_{mode}"), "delay", actual);
        }
    }
    for diff in [0, 1, 49, 50, 100, 1999, 2000, 2001, 3000] {
        let mut current = fresh.clone();
        current.reset_melee_timers(absent_genesis).unwrap();
        current.advance_melee_timers(diff);
        compare(
            &format!("composed_reset_then_update_{diff}"),
            "base",
            current.melee_timers.remaining_ms()[0],
        );
        current.advance_melee_timers(diff);
        compare(
            &format!("composed_second_update_{diff}"),
            "base",
            current.melee_timers.remaining_ms()[0],
        );
    }
    let mut repeated = fresh.clone();
    repeated.reset_melee_timers(absent_genesis).unwrap();
    repeated.advance_melee_timers(100);
    repeated.reset_melee_timers(absent_genesis).unwrap();
    compare(
        "repeated_prefix",
        "base",
        repeated.melee_timers.remaining_ms()[0],
    );
    let speed_profile = weapon_profile(2901);
    let speed_genesis = speed_profile.combat.as_ref().unwrap();
    let mut speed = state(&speed_profile);
    compare(
        "speed_1065353216",
        "stored_bits",
        speed.melee_timers.attack_time_fields[0],
    );
    compare(
        "speed_1065353216",
        "get_attack",
        speed.source_attack_times(speed_genesis).unwrap()[0],
    );
    speed.reset_melee_timers(speed_genesis).unwrap();
    compare(
        "speed_1065353216",
        "reset",
        speed.melee_timers.remaining_ms()[0],
    );
    for delay in [0, 2900] {
        let profile = weapon_profile(delay);
        let genesis = profile.combat.as_ref().unwrap();
        let mut current = state(&profile);
        let times = current.reset_melee_timers(genesis).unwrap();
        current.advance_melee_timers(times[0] - 777);
        break_weapon(&profile, &mut current);
        let case = format!("break_{delay}");
        compare(
            &case,
            "durability",
            current.equipment.item(MAINHAND).unwrap().current_durability,
        );
        compare(
            &case,
            "delay",
            current.source_attack_times(genesis).unwrap()[0],
        );
        compare(
            &case,
            "unchanged_timer",
            current.melee_timers.remaining_ms()[0],
        );
    }
    assert!(rows.is_empty());
}
