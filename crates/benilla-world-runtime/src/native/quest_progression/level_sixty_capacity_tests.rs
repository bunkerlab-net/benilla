// SPDX-License-Identifier: GPL-3.0-or-later
//! Measured authored composition, not a proof of every possible server population.
use super::*;
use crate::native::synthetic_fixture::{float, player, point};
use crate::native::synthetic_quest_fixture::{self as fixture, command, QuestAuthority};
use std::time::Instant;

fn world() -> NativeWorld {
    let (mut profile, mut input, mut rows) =
        fixture::setup_with_levels(60, 60, false, 1.0f32.to_bits());
    let mut target = profile.npcs[0].clone();
    target.max_health = 1_000_000;
    target.ai.reaction = ai::Reaction::Defensive;
    target.ai.spawn = point(2.0);
    profile.npcs[0] = target.clone();
    for spawn in 4..=19 {
        let mut npc = target.clone();
        npc.ai.creature_id = spawn;
        npc.entry = 60_000 + spawn as u32;
        npc.ai.spawn = point(if spawn == 19 { 1_000.0 } else { 2.0 });
        profile.npcs.push(npc.clone());
        let mut kill = rows.kill.creatures[0].clone();
        kill.entry = npc.entry;
        kill.reputation = kill::ReputationAdmission::ProvenAbsentOnKillRow { entry: npc.entry };
        rows.kill.creatures.push(kill);
    }
    for ability in [&mut profile.abilities[0], &mut rows.abilities[0].ability] {
        let AbilityAdmission::Available(spec) = &mut ability.admission else {
            unreachable!()
        };
        let spell::MageEffect::PeriodicDamage { duration_ms, .. } = &mut spec.effects[1] else {
            unreachable!()
        };
        *duration_ms = 120_000;
    }
    // Every extra book entry has explicit authored raw, learning, and unavailable metadata.
    for (index, id) in (60100..60162).enumerate() {
        let mut raw = input
            .spells
            .iter()
            .find(|row| row.id == 60002)
            .unwrap()
            .clone();
        raw.id = id;
        raw.words[0] = id;
        input.spells.push(raw);
        let init::Rows::Complete(defaults) = &mut input.default_spells else {
            unreachable!()
        };
        defaults.push(id);
        let init::Rows::Complete(links) = &mut input.skill_abilities else {
            unreachable!()
        };
        links.push(init::SkillAbilityRow {
            row_id: index as u32 + 3,
            skill: 8,
            spell: id,
            race_mask: 0,
            class_mask: 0,
            required_value: 0,
            learn_on_get_skill: 0,
            forward_spell_id: 0,
            max_value: 0,
        });
        let reason = "authored unavailable active at the admitted book limit".to_owned();
        input.active_support.push(init::ActiveAbilitySupport {
            spell_id: id,
            unavailable_reason: Some(reason.clone()),
        });
        let ability = Ability {
            id,
            school: 4,
            admission: AbilityAdmission::Unavailable { reason },
        };
        profile.abilities.push(ability.clone());
        rows.abilities.push(AbilityRecipe {
            ability,
            cast_time: None,
        });
    }
    assert_eq!(profile.abilities.len(), 64);
    profile.initialize_player(&input, float(100.0), 1).unwrap();
    profile.initialize_quest_progression(&input, rows).unwrap();
    profile.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    NativeWorld::synthetic_component_test(profile, [1; 16], player(), 42).unwrap()
}

#[derive(Default)]
struct AuthorityWithoutSwings(QuestAuthority);
impl Authority for AuthorityWithoutSwings {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let mut reply = self.0.resolve(request)?;
        // Passive damage/threat is unsupported. Use supported defensive AI with an
        // explicit authored no-initiation gate, rather than inventing a melee outcome.
        if let Facts::Npc(facts) = &mut reply.facts {
            facts.can_initiate_attack = false;
        }
        Ok(reply)
    }
}

fn checkpoint_if_due(world: &mut NativeWorld) {
    if world.records().len() >= 10 {
        let checkpoint = world.checkpoint().unwrap();
        world.checkpoint_committed(&checkpoint).unwrap();
        assert!(world.records().is_empty());
    }
}

fn tick(world: &mut NativeWorld) {
    checkpoint_if_due(world);
    world
        .step(
            world.state.tick + 1,
            None,
            &mut AuthorityWithoutSwings::default(),
        )
        .unwrap();
}

fn cast(world: &mut NativeWorld, target: u64) -> u64 {
    checkpoint_if_due(world);
    let id = world.state.next_cast_id;
    let input = command(
        world,
        Command::Cast {
            ability: 60001,
            target: world.state.npcs[&target].id,
        },
    );
    world
        .step(
            world.state.tick + 1,
            Some(input),
            &mut AuthorityWithoutSwings::default(),
        )
        .unwrap();
    assert!(
        world.state.casts.contains_key(&id),
        "authored cast must actually prepare"
    );
    for _ in 0..10 {
        if !matches!(
            world.state.casts[&id].execution.state(),
            spell::CastState::Casting { .. }
        ) {
            return id;
        }
        tick(world);
    }
    panic!("authored cast did not launch")
}

fn fill_current_defense(world: &mut NativeWorld) {
    let genesis = world.profile.quest_progression.as_ref().unwrap();
    let progression = world.state.quest_progression.as_mut().unwrap();
    let combat = world.state.combat.as_mut().unwrap();
    let skill = &combat.skills[&combat::DEFENSE_SKILL];
    let (first, maximum) = (skill.value + 1, skill.maximum);
    for value in first..=maximum {
        combat.skills.get_mut(&combat::DEFENSE_SKILL).unwrap().value = value;
        progression
            .defense_change(
                genesis,
                combat,
                world.state.health,
                world.state.resources.caster.mana,
            )
            .unwrap();
    }
    progression.validate_derived(genesis, combat).unwrap();
}

fn apply_item_break(world: &mut NativeWorld) {
    let genesis = world.profile.quest_progression.as_ref().unwrap();
    let progression = world.state.quest_progression.as_mut().unwrap();
    let combat = world.state.combat.as_mut().unwrap();
    assert_eq!(
        progression
            .derived_calls
            .iter()
            .filter(|call| matches!(call, DerivedCall::Defense { .. }))
            .count(),
        299
    );
    let before = combat.clone();
    let live = init::Resources {
        health: world.state.health,
        max_health: combat.stats.max_health,
        mana: world.state.resources.caster.mana,
        max_mana: combat.stats.max_mana,
    };
    let recompute = combat.apply_item_break(4, live).unwrap();
    progression
        .item_break(
            genesis,
            &before,
            combat,
            4,
            &recompute,
            live.health,
            live.mana,
        )
        .unwrap();
    let mut equipment = combat.equipment.snapshot();
    equipment
        .items
        .iter_mut()
        .find(|item| item.slot == 4)
        .unwrap()
        .current_durability = 0;
    combat.equipment = equipment::EquipmentState::from_snapshot(equipment).unwrap();
    world.state.health = recompute.resources.health;
    world.state.resources.caster.mana = recompute.resources.mana;
    world.state.resources = mana::MageManaState::from_state(
        world.state.resources.caster.clone(),
        recompute.resources.max_mana,
        recompute.mana_regeneration,
        world.state.resources.regen_elapsed_ms(),
        world.state.resources.last_mana_use_timer_ms(),
    )
    .unwrap();
    assert_eq!(
        progression
            .derived_calls
            .iter()
            .filter(|call| matches!(call, DerivedCall::ItemBreak { .. }))
            .count(),
        1
    );
    progression.validate_derived(genesis, combat).unwrap();
}

#[test]
fn loaded_sixty_checkpoint_has_bounded_histories_auras_and_traveling_casts() {
    assert!(benilla_sim::npc_melee::defense_gain_chance(
        60,
        benilla_sim::npc_melee::DefenseSkill {
            value: 1,
            maximum: 300,
            gain: 1,
        },
    )
    .unwrap()
    .is_some());
    let mut world = world();
    // Preserve this capacity fixture's 299-callback, small-gap history composition.
    // The separate catch-up replay test covers defense left at 1 through level 60.
    for level in 1..=59 {
        if level > 1 {
            super::level_sixty_tests::admin(&mut world, AdminOperation::GainOneLevel);
        }
        fill_current_defense(&mut world);
        let boundary = world.checkpoint().unwrap();
        world.checkpoint_committed(&boundary).unwrap();
    }
    // Sixteen live old-level auras, each created by the real cast lifecycle.
    for target in std::iter::once(2).chain(4..=18) {
        let id = cast(&mut world, target);
        for _ in 0..30 {
            if !world.state.casts.contains_key(&id) {
                break;
            }
            tick(&mut world);
        }
        assert!(!world.state.casts.contains_key(&id));
        let boundary = world.checkpoint().unwrap();
        world.checkpoint_committed(&boundary).unwrap();
        assert_eq!(world.state.npcs[&target].aura_bindings[&60001].level, 59);
    }
    let original: Vec<_> = world
        .state
        .npcs
        .iter()
        .filter_map(|(id, npc)| {
            npc.aura_bindings
                .get(&60001)
                .map(|binding| (*id, binding.clone()))
        })
        .collect();
    assert_eq!(original.len(), 16);
    assert_eq!(
        world
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .training
            .abilities
            .len(),
        64
    );
    let mut key = [0; 16];
    key[..8].copy_from_slice(&(world.state.next_sequence + 1).to_le_bytes());
    let input = command(
        &world,
        Command::Admin(AdminRequest {
            key,
            world: world.state.world,
            player: world.state.player,
            policy_revision: 7,
            operation: AdminOperation::GainOneLevel,
        }),
    );
    world
        .step(
            world.state.tick + 1,
            Some(input),
            &mut AuthorityWithoutSwings::default(),
        )
        .unwrap();
    fill_current_defense(&mut world);
    apply_item_break(&mut world);
    let boundary = world.checkpoint().unwrap();
    world.checkpoint_committed(&boundary).unwrap();
    for _ in 0..MAX_CASTS {
        let id = cast(&mut world, 19);
        assert!(matches!(
            world.state.casts[&id].execution.state(),
            spell::CastState::Traveling { .. }
        ));
    }
    assert_eq!(world.state.casts.len(), 16);
    for (id, binding) in &original {
        assert_eq!(&world.state.npcs[id].aura_bindings[&60001], binding);
    }

    let progression = world.state.quest_progression.as_ref().unwrap();
    let work = 4 * progression.training.calls.len() as u64
        + progression.derived_calls.len() as u64
        + world
            .state
            .casts
            .values()
            .flat_map(|cast| std::iter::once(&cast.binding).chain(cast.release_binding.as_ref()))
            .chain(
                world
                    .state
                    .npcs
                    .values()
                    .flat_map(|npc| npc.aura_bindings.values()),
            )
            .map(ExecutionBinding::replay_work)
            .sum::<u64>();
    assert!(work <= 65_536);
    let checkpoint = world.checkpoint().unwrap();
    let bytes = checkpoint.encode().unwrap();
    assert!(bytes.len() < MAX_BYTES);
    let started = Instant::now();
    let restored = NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        Checkpoint::decode(&bytes).unwrap(),
    )
    .unwrap();
    let elapsed = started.elapsed();
    eprintln!("loaded level60: bytes={}, replay_work={work}, training_calls={}, derived_calls={}, auras=16, traveling_casts=16, known_abilities=64, restore_us={}", bytes.len(), progression.training.calls.len(), progression.derived_calls.len(), elapsed.as_micros());
    assert_eq!(restored.state_digest(), world.state_digest());
    assert_eq!(
        restored.state.combat.as_ref().unwrap().skills[&combat::DEFENSE_SKILL].value,
        300
    );
    assert!(restored
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(4)
        .unwrap()
        .is_broken());

    // Admission of a seventeenth active cast fails atomically, including sequence/RNG.
    let input = command(
        &world,
        Command::Cast {
            ability: 60001,
            target: world.state.npcs[&19].id,
        },
    );
    assert_eq!(
        world.step(
            world.state.tick + 1,
            Some(input),
            &mut AuthorityWithoutSwings::default()
        ),
        Err(NativeError::Limit)
    );
    assert_eq!(world.checkpoint().unwrap(), checkpoint);

    // Typed restore must enforce work bounds even when the caller bypasses decode.
    let mut forged = serde_json::to_value(&checkpoint).unwrap();
    let id = world.state.casts.keys().next().unwrap().to_string();
    forged["state"]["casts"][id]["binding"]["progression"]["derived_prefix"]["count"] =
        serde_json::json!(65_537);
    let mut forged: Checkpoint = serde_json::from_value(forged).unwrap();
    forged.digest = crate::digest(
        format!("benilla-native-state-v{}\0", NATIVE_FORMAT_VERSION).as_bytes(),
        &forged.state,
    );
    forged.checksum = crate::digest(
        format!("benilla-native-checkpoint-v{}\0", NATIVE_FORMAT_VERSION).as_bytes(),
        &(&forged.state, forged.digest, forged.history_tip),
    );
    assert!(matches!(
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), forged),
        Err(NativeError::TooLarge)
    ));
}
