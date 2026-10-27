// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-shaped player/NPC inputs and real synthetic-host transitions.
//! These fixtures never grant native content or spatial admission.
use super::*;
use crate::native::{
    creature_generation_tests, synthetic_combat_fixture, synthetic_fixture as fixture,
    synthetic_motion_fixture, synthetic_quest_fixture,
    synthetic_two_owner_fixture::CompleteAuthority,
};
use benilla_sim::{player_progression as pg, starter_init as init};

pub(in crate::native) fn profile() -> Profile {
    profile_with_health(1)
}

pub(in crate::native) fn profile_with_health(health: u32) -> Profile {
    profile_with_parameters(health, false)
}

pub(in crate::native) fn fragile_profile() -> Profile {
    profile_with_parameters(60, true)
}

fn profile_with_parameters(health: u32, fragile_player: bool) -> Profile {
    profile_with_home_case(health, fragile_player, None)
}

fn profile_with_home_case(health: u32, fragile_player: bool, home_spawn: Option<f32>) -> Profile {
    profile_with_options(health, fragile_player, home_spawn, false)
}

fn profile_with_options(
    health: u32,
    fragile_player: bool,
    home_spawn: Option<f32>,
    fragile_weapon: bool,
) -> Profile {
    let (mut profile, mut input, mut rows) = synthetic_quest_fixture::setup();
    if fragile_player {
        input.create_health = 1;
        input.create_attributes.0[2] = 1;
    }
    let init::Rows::Complete(skills) = &mut input.default_skills else {
        unreachable!()
    };
    for id in [136, 162] {
        skills.push(init::DefaultSkill { id, step: 0 });
        input.skill_lines.push(init::SkillLineRow { id, category: 6 });
    }
    let mut weapon = input.items[0].clone();
    weapon.id = 900002;
    weapon.class = 2;
    weapon.subclass = 10;
    weapon.inventory_type = 17;
    weapon.armor = 0;
    weapon.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
    weapon.stats[0] = init::ItemStat { kind: 3, value: 3 };
    weapon.damage[0] = init::ItemDamage {
        min_bits: 2.0f32.to_bits(),
        max_bits: 4.0f32.to_bits(),
        school: 0,
    };
    weapon.delay_ms = 2500;
    if fragile_weapon {
        weapon.max_durability = 1;
    }
    input.items.push(weapon);
    let init::Rows::Complete(grants) = &mut input.item_grants else {
        unreachable!()
    };
    grants.push(init::ItemGrant {
        source: init::GrantSource::Custom,
        source_order: 1,
        item: 900002,
        count: 1,
    });
    profile
        .initialize_player(&input, crate::native::synthetic_fixture::float(100.0), 1)
        .unwrap();
    for (position, id) in [(2, 136), (3, 162)] {
        let skill = &profile.combat.as_ref().unwrap().skills[&id];
        rows.skills.push(pg::SkillState {
            id,
            position,
            value: skill.value,
            maximum: skill.maximum,
            step: skill.step,
            permanent_bonus: skill.permanent_bonus,
            temporary_bonus: skill.temporary_bonus,
            status: pg::SkillStatus::Unchanged,
        });
    }
    // The existing fixture's authored level rows, extended with an actual
    // staff SkillLine lookup. These are test rows, never native admission.
    rows.levels = pg::Catalog::new(
        1,
        8,
        rows.levels.rules(),
        (1..=3)
            .map(|level| pg::LevelStatsRow {
                race: 1,
                class: 8,
                level,
                attributes: init::Attributes(
                    [17, 23, if fragile_player { 1 } else { 29 }, 31, 37]
                        .map(|v| v + level as i32 - 1),
                ),
            })
            .collect(),
        (1..=3)
            .map(|level| pg::ClassLevelStatsRow {
                class: 8,
                level,
                base_health: (if fragile_player { 1 } else { 5000 }) + (level - 1) * 10,
                base_mana: 59 + (level - 1) * 6,
            })
            .collect(),
        (1..=3)
            .map(|level| pg::XpRow {
                level,
                xp_for_next_level: level * 40,
            })
            .collect(),
        input
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
    // Preserve distinct owners; this fixture deliberately has no passive script writer.
    for npc in &mut profile.npcs {
        npc.ai.reaction = ai::Reaction::Defensive;
        npc.faction_template = if npc.ai.creature_id == 2 { 2 } else { 1 };
        npc.ai.neutral_to_all = npc.ai.creature_id == 3;
        npc.ai.spawn = fixture::point(if npc.ai.creature_id == 2 { 2.0 } else { 4.0 });
    }
    if let Some(spawn) = home_spawn {
        // Author the source profile before motion, relations, and raw creature
        // initialization. The real AI must enter Home; no phase is injected.
        let target = profile
            .npcs
            .iter_mut()
            .find(|npc| npc.ai.creature_id == 2)
            .unwrap();
        target.ai.reaction = ai::Reaction::Aggressive;
        target.ai.spawn = fixture::point(spawn);
        target.ai.hard_leash_radius = Some(fixture::real(0.1));
        profile.player.orientation = Orientation::new(std::f32::consts::PI).unwrap();
    }
    let mut relations = synthetic_combat_fixture::source_rows(&profile);
    for npc in &mut relations.npcs {
        npc.npc_flags = u32::from(npc.entry == 60003) * 2;
    }
    profile.initialize_interactions(relations).unwrap();
    let mut single = profile.clone();
    single.npcs = vec![profile.npcs[0].clone()];
    let mut motion = synthetic_motion_fixture::genesis(&single, 2);
    single.npcs = vec![profile.npcs[1].clone()];
    motion
        .npcs
        .extend(synthetic_motion_fixture::genesis(&single, 3).npcs);
    // Explicit authored facing: the victim initially faces the player, so
    // parry/block are reachable rather than silently restricted to rear dodge.
    motion.npcs.get_mut(&2).unwrap().npc_spawn_orientation =
        Orientation::new(std::f32::consts::PI).unwrap();
    profile.source_motion = Some(motion);
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(
            synthetic_motion_fixture::creature_capability_sources(&profile),
        )
        .unwrap(),
    );
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(
            crate::native::synthetic_two_owner_fixture::visitor_input(&profile, vec![2, 3]),
        )
        .unwrap(),
    );
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|npc| NpcCombatAuthoritySource {
                    entry: npc.entry,
                    extra_flags: 2,
                    static_flags1: 0,
                    static_flags2: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                })
                .collect(),
        )
        .unwrap();
    let mut sources = creature_generation_tests::complete_scene_sources(&profile);
    let target = sources.iter_mut().find(|source| source.spawn_id == 2).unwrap();
    let benilla_sim::creature_init::Rows::Complete(spawns) = &mut target.input.spawn_template else {
        unreachable!()
    };
    spawns[0].current_health = health;
    profile = creature_generation_tests::install(profile, sources, vec![2, 3]);
    profile.initialize_quest_progression(&input, rows).unwrap();
    profile.initialize_facing_authority().unwrap();
    if fragile_player {
        use benilla_sim::player_death as death;
        profile.configure_player_death(
            death::DeathConfig {
                map_instanceable: false,
                pve_reclaim_delay_enabled: true,
            },
            death::DeathSupport::checked(&death::REQUIRED_ABSENCES.map(|entry| (entry, Some(false))))
                .unwrap(),
        ).unwrap();
    }
    profile.initialize_player_melee(1).unwrap();
    profile
}

pub(in crate::native) fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], fixture::player(), 42).unwrap()
}

#[test]
fn source_shaped_mainhand_fixture_retains_complete_selected_dependencies() {
    let world = world();
    assert!(world.profile.player_melee.is_some());
    assert!(world.profile.source_combat_authority());
    assert!(world.profile.source_facing_authority());
    assert!(world
        .profile
        .combat
        .as_ref()
        .unwrap()
        .equipment_weapons
        .iter()
        .any(|weapon| weapon.slot == 15 && weapon.item_id == 900002));
    for npc in world.state.npcs.values() {
        assert!(npc.initialized_creature().is_some());
        assert!(npc.source_motion.is_some());
    }
    let restored = NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), world.state_digest());
}

fn restore(world: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn accepted_mainhand_command_reaches_damage_death_credit_and_durable_replay() {
    let mut world = world();
    let initial = world.checkpoint().unwrap();
    let target = world.state.npcs[&2].id;
    let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
    let mut authority = CompleteAuthority::default();
    let mut skill_before = world.state.combat.as_ref().unwrap().skills[&136].value;
    let mut calls_before = world.state.quest_progression.as_ref().unwrap().derived_calls.len();
    world.step(1, Some(input), &mut authority).unwrap();
    assert!(world.last_tick().unwrap().events.iter().any(|event| matches!(
        event,
        Event::InputResult { sequence: 0, accepted: true }
    )));
    assert!(world.last_tick().unwrap().events.iter().any(|event| matches!(
        event, Event::PlayerAttackStart { target: actual } if *actual == target
    )));
    // One accepted command, then ordinary scheduled ticks, never a replayed command.
    for _ in 0..100 {
        if world.state.npcs[&2].health == 0 {
            break;
        }
        skill_before = world.state.combat.as_ref().unwrap().skills[&136].value;
        calls_before = world.state.quest_progression.as_ref().unwrap().derived_calls.len();
        world.step(world.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_eq!(world.state.npcs[&2].health, 0);
    assert_eq!(world.state.quest_progression.as_ref().unwrap().player.view().level, 2);
    let lethal = world.last_tick().unwrap();
    let gain = lethal.draws.iter().position(|draw| matches!(
        draw.purpose, DrawPurpose::PlayerWeaponGain { npc, .. } if npc == target
    )).expect("lethal attack still executes the attacker weapon-skill callback after level-up");
    let outcome = lethal.draws.iter().position(|draw| matches!(
        draw.purpose, DrawPurpose::PlayerMeleeOutcome { npc, .. } if npc == target
    )).unwrap();
    assert!(outcome < gain);
    let calls = &world.state.quest_progression.as_ref().unwrap().derived_calls;
    let level = calls.iter().position(|call| matches!(
        call, quest_progression::DerivedCall::Level { level: 2, .. }
    )).expect("actual lethal level-up callback");
    let combat = world.state.combat.as_ref().unwrap();
    let expected = benilla_sim::player_melee::resolve_weapon_gain(
        2,
        combat.stats.attributes[3] as u32,
        benilla_sim::player_melee::WeaponSkill {
            value: skill_before,
            maximum: combat.skills[&136].maximum,
            gain: 1,
        },
        Some(
            benilla_sim::npc_melee::PercentRoll::from_fraction53(lethal.draws[gain].value as u64)
                .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(combat.skills[&136].value, expected.value());
    assert_eq!(
        calls[calls_before..].iter().filter(|call| matches!(
            call, quest_progression::DerivedCall::Weapon { skill: 136, .. }
        )).count(),
        usize::from(expected.update_weapon_skill())
    );
    // A failed skill probability is not a callback success. Any actual Weapon
    // callback following this lethal attack must use the new level, not genesis.
    for call in &calls[level + 1..] {
        if let quest_progression::DerivedCall::Weapon { view, .. } = call {
            assert_eq!(view.level, 2);
        }
    }
    let swing = lethal.events.iter().position(|event| matches!(
        event, Event::PlayerMelee { target: actual, effective, .. }
            if *actual == target && *effective > 0
    )).expect("actual positive player mainhand damage");
    let death = lethal.events.iter().position(|event| matches!(
        event, Event::Npc { id, effect: ai::Effect::Died } if *id == target
    )).expect("actual target death callback");
    assert!(swing < death, "source AttackState precedes DealMeleeDamage death");
    assert_eq!(world.state.npcs[&2].original_loot_recipient, Some(world.state.player));
    assert_eq!(
        world.state.quest_progression.as_ref().unwrap().credited_generations.get(&2),
        Some(&target.generation)
    );
    assert_eq!(restore(&world).state_digest(), world.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.state_digest(), world.state_digest());
}

#[test]
fn attack_commands_refuse_unselected_and_foreign_generation_atomically() {
    let mut unselected = fixture::world();
    let target = unselected.state.npcs[&2].id;
    let before = unselected.state_digest();
    let input = synthetic_quest_fixture::command(&unselected, Command::AttackStart { target });
    assert!(matches!(
        unselected.step(1, Some(input), &mut fixture::SyntheticAuthority::default()),
        Err(NativeError::UnsupportedMechanic(_))
    ));
    assert_eq!(unselected.state_digest(), before);

    let mut selected = world();
    let mut target = selected.state.npcs[&2].id;
    target.generation += 1;
    let before = selected.state_digest();
    let input = synthetic_quest_fixture::command(&selected, Command::AttackStart { target });
    assert!(matches!(
        selected.step(1, Some(input), &mut CompleteAuthority::default()),
        Err(NativeError::UnknownTarget)
    ));
    assert_eq!(selected.state_digest(), before);
    assert_eq!(selected.state.next_sequence, 0);
}

#[test]
fn client_attack_stop_without_victim_is_accepted_without_forced_cancel() {
    let mut world = world();
    let input = synthetic_quest_fixture::command(&world, Command::AttackStop);
    world.step(1, Some(input), &mut CompleteAuthority::default()).unwrap();
    let events = &world.last_tick().unwrap().events;
    assert!(events.iter().any(|event| matches!(
        event, Event::InputResult { sequence: 0, accepted: true }
    )));
    assert!(!events.iter().any(|event| matches!(
        event, Event::PlayerAttackCancel | Event::PlayerAttackStop { .. } | Event::PlayerMelee { .. }
    )));
    assert_eq!(restore(&world).state_digest(), world.state_digest());
}

#[test]
fn repeated_start_then_client_stop_preserves_exact_edges_without_a_swing() {
    let mut world = world();
    let target = world.state.npcs[&2].id;
    let commands = [
        Command::AttackStart { target },
        Command::AttackStart { target },
        Command::AttackStop,
    ].into_iter().enumerate().map(|(sequence, command)| {
        let mut input = synthetic_quest_fixture::command(&world, command);
        input.input.sequence = sequence as u64;
        input
    }).collect();
    world.step_batch(1, commands, &mut CompleteAuthority::default()).unwrap();
    let events = &world.last_tick().unwrap().events;
    assert_eq!(events.iter().filter(|event| matches!(
        event, Event::InputResult { accepted: true, .. }
    )).count(), 3);
    assert_eq!(events.iter().filter(|event| matches!(
        event, Event::PlayerAttackStart { target: actual } if *actual == target
    )).count(), 1);
    assert_eq!(events.iter().filter(|event| matches!(
        event, Event::PlayerAttackStop { target: actual } if *actual == target
    )).count(), 1);
    assert!(!events.iter().any(|event| matches!(
        event, Event::PlayerAttackCancel | Event::PlayerMelee { .. }
    )));
    assert_eq!(world.state.combat.as_ref().unwrap().melee_timers.victim(), None);
    assert_eq!(restore(&world).state_digest(), world.state_digest());
}

#[test]
fn genuine_npc_defense_reactive_refresh_expiry_death_and_restore_are_bound() {
    let profile = profile_with_health(60);
    let mut first = None;
    let mut refreshed = None;
    let mut active_cases = 0;
    // Independent, predeclared authored seeds. Every command must commit; an
    // error never triggers a replacement attempt or becomes a skipped case.
    for seed in 1..=256 {
        let mut world = NativeWorld::synthetic_component_test(
            profile.clone(),
            [1; 16],
            fixture::player(),
            seed,
        )
        .unwrap();
        let target = world.state.npcs[&2].id;
        let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
        let mut authority = CompleteAuthority::default();
        world.step(1, Some(input), &mut authority).unwrap();
        assert!(world.last_tick().unwrap().events.iter().any(|event| matches!(
            event, Event::InputResult { sequence: 0, accepted: true }
        )));
        if world.state.npcs[&2].defense_reactive.is_none() {
            continue;
        }
        active_cases += 1;
        assert_eq!(world.state.npcs[&2].defense_reactive.as_ref().unwrap()
            .remaining(world.state.now_ms, profile.tick_ms).unwrap(), 4000 - profile.tick_ms);
        assert!(world.state.npcs[&2]
            .defense_reactive_active(world.state.now_ms, profile.tick_ms)
            .unwrap());
        assert_eq!(restore(&world).state_digest(), world.state_digest());
        first.get_or_insert_with(|| world.clone());
        for _ in 0..25 {
            world.step(world.state.tick + 1, None, &mut authority).unwrap();
        }
        if world.state.npcs[&2].defense_reactive.as_ref().is_some_and(|cause| {
            cause.remaining(world.state.now_ms, profile.tick_ms).unwrap() == 4000 - profile.tick_ms
        }) {
            refreshed.get_or_insert(world);
        }
    }
    assert!(active_cases > 0, "enumeration must exercise an actual defense callback");
    let mut world = first.expect("actual source reactive required");
    let original = world.checkpoint().unwrap();
    for pointer in [
        "/state/npcs/2/defense_reactive/key/action",
        "/state/npcs/2/defense_reactive/context/relation/target/generation",
        "/state/npcs/2/defense_reactive/draws/0/value",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        let field = value.pointer_mut(pointer).expect("exact serialized proof field");
        *field = serde_json::json!(field.as_u64().unwrap() + 1);
        let mut changed: Checkpoint = serde_json::from_value(value).unwrap();
        changed.digest = crate::digest(b"benilla-native-state-v52\0", &changed.state);
        changed.checksum = crate::digest(
            b"benilla-native-checkpoint-v52\0",
            &(&changed.state, changed.digest, changed.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(profile.clone(), changed).is_err(),
            "{pointer}"
        );
    }
    let input = synthetic_quest_fixture::command(&world, Command::AttackStop);
    let mut authority = CompleteAuthority::default();
    world.step(2, Some(input), &mut authority).unwrap();
    assert!(world.state.npcs[&2].defense_reactive.is_some(), "stop must not clear defense reactive");
    while world.state.tick < 40 {
        world.step(world.state.tick + 1, None, &mut authority).unwrap();
    }
    assert!(world.state.npcs[&2].defense_reactive.is_none(), "exact expiry clears retained cause");
    assert_eq!(restore(&world).state_digest(), world.state_digest());

    let mut world = refreshed.expect("enumeration must exercise an actual refresh");
    let mut authority = CompleteAuthority::default();
    assert_eq!(restore(&world).state_digest(), world.state_digest());
    let target = world.state.npcs[&2].id;
    for _ in 0..300 {
        if world.state.npcs[&2].health == 0 { break; }
        world.step(world.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_eq!(world.state.npcs[&2].health, 0);
    assert!(world.state.npcs[&2].defense_reactive.is_none());
    assert_eq!(restore(&world).state_digest(), world.state_digest());
    for _ in 0..100 {
        if world.state.npcs[&2].id != target { break; }
        world.step(world.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_ne!(world.state.npcs[&2].id, target);
    assert!(world.state.npcs[&2].defense_reactive.is_none());
    assert_eq!(restore(&world).state_digest(), world.state_digest());
}

#[test]
fn previous_native_schema_is_not_accepted_as_player_melee_state() {
    let world = world();
    let mut value = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    value["version"] = serde_json::json!(51);
    assert!(Checkpoint::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn actual_npc_lethal_callback_closes_player_attack_before_checkpoint() {
    let profile = fragile_profile();
    let mut world =
        NativeWorld::synthetic_component_test(profile, [1; 16], fixture::player(), 42).unwrap();
    assert!(world.state.health <= 5, "authored initializer produced genuinely low health");
    let initial = world.checkpoint().unwrap();
    let target = world.state.npcs[&2].id;
    let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
    let mut authority = CompleteAuthority::default();
    world.step(1, Some(input), &mut authority).unwrap();
    assert!(world.last_tick().unwrap().events.iter().any(|event| matches!(
        event, Event::InputResult { sequence: 0, accepted: true }
    )));
    for _ in 0..50 {
        if world.state.health == 0 { break; }
        world.step(world.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_eq!(world.state.health, 0);
    assert!(world.state.npcs.contains_key(&2), "removed source owner was reinserted");
    let events = &world.last_tick().unwrap().events;
    assert!(events.iter().any(|event| matches!(
        event, Event::Melee { id, effective, .. } if *id == target && *effective > 0
    )));
    let cancel = events
        .iter()
        .position(|event| matches!(event, Event::PlayerAttackCancel))
        .expect("source forced cancel");
    let stop = events
        .iter()
        .position(|event| matches!(event, Event::PlayerAttackStop { target: actual } if *actual == target))
        .expect("source stopped original player victim");
    assert!(cancel < stop);
    assert_eq!(world.state.combat.as_ref().unwrap().melee_timers.victim(), None);
    assert!(!world.state.combat.as_ref().unwrap().melee_timers.melee_attacking());
    let checkpoint = world.checkpoint().unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), checkpoint).unwrap();
    assert_eq!(restored.state.health, 0);
    assert_eq!(restored.state.combat.as_ref().unwrap().melee_timers.victim(), None);
    assert_eq!(restored.state_digest(), world.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.state_digest(), world.state_digest());
}

#[test]
fn both_cast_and_mainhand_overlap_orders_refuse_the_whole_transaction() {
    for cast_first in [false, true] {
        let mut world = world();
        let before = world.state_digest();
        let target = world.state.npcs[&2].id;
        let mut commands = vec![
            Command::AttackStart { target },
            Command::Cast { ability: 60001, target },
        ];
        if cast_first { commands.reverse(); }
        let inputs = commands.into_iter().enumerate().map(|(sequence, command)| {
            let mut input = synthetic_quest_fixture::command(&world, command);
            input.input.sequence = sequence as u64;
            input
        }).collect();
        assert!(matches!(
            world.step_batch(1, inputs, &mut CompleteAuthority::default()),
            Err(NativeError::UnsupportedMechanic("player melee concurrent spell/projectile/aura"))
        ), "cast_first={cast_first}");
        assert_eq!(world.state_digest(), before);
        assert_eq!(world.state.next_sequence, 0);
        assert!(world.records().is_empty());
    }
}

#[test]
fn source_initialized_fishing_pole_is_refused_before_policy_selection() {
    let mut input = fixture::initialized_starter_input();
    let init::Rows::Complete(skills) = &mut input.default_skills else { unreachable!() };
    skills.push(init::DefaultSkill { id: 356, step: 1 });
    input.skill_lines.push(init::SkillLineRow { id: 356, category: 9 });
    let init::Rows::Complete(rows) = &mut input.skill_race_class else { unreachable!() };
    rows.push(init::SkillRaceClassRow {
        row_id: 900,
        skill: 356,
        race_mask: 1,
        class_mask: 128,
        flags: 0,
        tier: 900,
    });
    input.skill_tiers.push(init::SkillTierRow { id: 900, values: [1; 16], maxima: [75; 16] });
    let mut pole = input.items[0].clone();
    pole.id = 900002;
    pole.class = 2;
    pole.subclass = 20;
    pole.inventory_type = 17;
    pole.armor = 0;
    pole.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
    pole.damage[0] = init::ItemDamage {
        min_bits: 2.0f32.to_bits(),
        max_bits: 4.0f32.to_bits(),
        school: 0,
    };
    pole.delay_ms = 2500;
    input.items.push(pole);
    let init::Rows::Complete(grants) = &mut input.item_grants else { unreachable!() };
    grants.push(init::ItemGrant {
        source: init::GrantSource::Custom,
        source_order: 1,
        item: 900002,
        count: 1,
    });
    let mut profile = fixture::profile();
    profile.initialize_player(&input, fixture::float(0.0), 1).unwrap();
    assert!(profile.combat.as_ref().unwrap().equipment_weapons.iter()
        .any(|weapon| weapon.slot == 15 && weapon.subclass == 20));
    let before = profile.digest();
    assert_eq!(
        profile.initialize_player_melee(1),
        Err(NativeError::UnsupportedMechanic("player melee fishing-pole callback"))
    );
    assert_eq!(profile.digest(), before);
    assert!(profile.player_melee.is_none());
}

#[test]
fn actual_returning_home_preserves_active_range_and_facing_precedence() {
    for (spawn, expected_error) in [(16.0, 0), (8.0, 2)] {
        for already_active in [false, true] {
            let profile = profile_with_home_case(60, false, Some(spawn));
            let mut world = NativeWorld::synthetic_component_test(
                profile,
                [1; 16],
                fixture::player(),
                42,
            )
            .unwrap();
            let mut authority = CompleteAuthority::default();
            let target = world.state.npcs[&2].id;
            if already_active {
                let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
                world.step(1, Some(input), &mut authority).unwrap();
                assert_eq!(
                    world.state.combat.as_ref().unwrap().melee_timers.victim(),
                    Some(target)
                );
                assert!(world.last_tick().unwrap().events.iter().any(|event| matches!(
                    event, Event::PlayerAttackStart { target: actual } if *actual == target
                )));
            }
            for _ in 0..20 {
                if world.state.npcs[&2].ai.phase() == ai::Phase::ReturningHome {
                    break;
                }
                world
                    .step(world.state.tick + 1, None, &mut authority)
                    .unwrap();
                assert!(!world.last_tick().unwrap().events.iter().any(|event| matches!(
                    event,
                    Event::PlayerMelee { .. }
                )));
            }
            assert_eq!(world.state.npcs[&2].ai.phase(), ai::Phase::ReturningHome);
            let before = current_fields(&world.state).unwrap();
            let context = context::SourceContext::capture(
                &world.profile,
                &world.state,
                &world.state.npcs[&2],
                before,
            )
            .unwrap()
            .replay(&world.profile, &world.state)
            .unwrap();
            assert!(context.target_evading);
            assert!(context.can_attack_now);
            assert_eq!(context.in_range, expected_error == 2);
            assert!(!context.facing);
            assert_eq!(source::ready_decision(&context), (expected_error, false));
            let sequence = world.state.next_sequence;
            // Unit::Attack independently refuses an evading victim without
            // lifecycle writes or packets. For an already-active victim the
            // following scheduler still applies range/facing before evade.
            let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
            world
                .step(world.state.tick + 1, Some(input), &mut authority)
                .unwrap();
            let record = world.last_tick().unwrap();
            assert!(record.events.iter().any(|event| matches!(
                event, Event::InputResult { sequence: actual, accepted: true } if *actual == sequence
            )));
            assert!(!record.events.iter().any(|event| matches!(
                event,
                Event::PlayerAttackStart { .. }
                    | Event::PlayerAttackStop { .. }
                    | Event::PlayerAttackCancel
                    | Event::PlayerMelee { .. }
            )));
            assert!(!record.draws.iter().any(|draw| matches!(
                draw.purpose,
                DrawPurpose::PlayerMeleeDamage { .. }
                    | DrawPurpose::PlayerMeleeOutcome { .. }
                    | DrawPurpose::PlayerMeleeGlance { .. }
            )));
            let after = current_fields(&world.state).unwrap();
            assert_eq!(after.lifecycle, before.lifecycle);
            assert_eq!(
                after.last_error,
                if already_active { expected_error } else { 0 }
            );
            assert_eq!(
                after.remaining_ms,
                if already_active && expected_error == 2 {
                    [100, 100]
                } else {
                    [0, 0]
                }
            );
            assert_eq!(restore(&world).state_digest(), world.state_digest());
        }
    }
}

#[test]
fn actual_hit_done_weapon_break_precedes_unarmed_timer_reset_and_restore() {
    let profile = profile_with_options(60, false, None, true);
    let genesis = profile.combat.as_ref().unwrap();
    assert_eq!(genesis.equipment.item(15).unwrap().current_durability, 1);
    assert_eq!(
        genesis.source_attack_times_for_recipe(&genesis.recipe.snapshot()).unwrap()[0],
        2500
    );
    let mut reached = 0;
    // Fixed independent worlds, not retrying a failed transaction or injecting
    // RNG. Every draw/outcome must validate; at least one real HIT_DONE selects15.
    for seed in 1..=256 {
        let mut world = NativeWorld::synthetic_component_test(
            profile.clone(),
            [1; 16],
            fixture::player(),
            seed,
        )
        .unwrap();
        let target = world.state.npcs[&2].id;
        let input = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
        world
            .step(1, Some(input), &mut CompleteAuthority::default())
            .unwrap();
        let record = world.last_tick().unwrap();
        assert!(record.events.iter().any(|event| matches!(
            event, Event::InputResult { accepted: true, .. }
        )));
        let Some(slot_index) = record.draws.iter().position(|draw| matches!(
            draw.purpose, DrawPurpose::PlayerDurabilitySlot { npc, .. } if npc == target
        ) && draw.value == 15) else {
            continue;
        };
        reached += 1;
        assert!(record.draws[..slot_index].iter().any(|draw| matches!(
            draw.purpose, DrawPurpose::PlayerDurabilityChance { npc, .. } if npc == target
        )));
        let swing = record.events.iter().position(|event| matches!(
            event, Event::PlayerMelee { target: actual, effective, .. }
                if *actual == target && *effective > 0
        )).expect("actual surviving HIT_DONE damage");
        let broken = record.events.iter().position(|event| matches!(
            event, Event::Combat(CombatEvent::ItemStats { slot: 15, .. })
        )).expect("actual weapon break callback");
        assert!(swing < broken);
        assert!(world.state.npcs[&2].health > 0);
        let combat = world.state.combat.as_ref().unwrap();
        assert_eq!(combat.equipment.item(15).unwrap().current_durability, 0);
        validate_current(&world.profile, &world.state).unwrap();
        let source_times = world
            .profile
            .combat
            .as_ref()
            .unwrap()
            .source_attack_times_for_recipe(&combat.recipe.snapshot())
            .unwrap();
        assert_eq!(source_times[0], 2000);
        assert_eq!(combat.melee_timers.remaining_ms()[0], source_times[0]);
        assert_eq!(restore(&world).state_digest(), world.state_digest());
    }
    assert!(reached > 0, "no actual HIT_DONE weapon break observed");
}
