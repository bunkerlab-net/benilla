// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored level rows exercise source progression and the real stock-field projection.
use super::*;
use n::{synthetic_fixture as fixture, synthetic_quest_fixture as quests};

fn profile(rate: u32) -> n::Profile {
    let (mut profile, input, data) = quests::setup_with_levels(60, 60, true, rate);
    profile.initialize_quest_progression(&input, data).unwrap();
    profile.admin = Some(n::AdminPolicy {
        world: [1; 16],
        world_owner: fixture::player().owner,
        allowlist: Default::default(),
        revision: 1,
    });
    profile
}

fn world(rate: u32) -> n::NativeWorld {
    n::NativeWorld::synthetic_component_test(profile(rate), [1; 16], fixture::player(), 42).unwrap()
}

fn fields(events: &[SessionEvent], guid: u64) -> ObjectFields {
    let mut result = ObjectFields::from_pairs(&[]);
    for event in events {
        if let SessionEvent::ObjectValues { guid: id, fields }
        | SessionEvent::ObjectCreate {
            guid: id, fields, ..
        } = event
        {
            if *id == guid {
                result.merge(fields.clone());
            }
        }
    }
    result
}

fn apply(
    world: &mut n::NativeWorld,
    projection: &mut NativeProjection,
    operation: n::AdminOperation,
) -> Vec<SessionEvent> {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&(world.state().next_sequence + 1).to_le_bytes());
    let request = n::AdminRequest {
        key,
        world: world.state().world,
        player: world.state().player,
        policy_revision: 1,
        operation,
    };
    let input = quests::command(world, n::Command::Admin(request));
    world
        .step(
            world.state().tick + 1,
            Some(input),
            &mut quests::QuestAuthority::default(),
        )
        .unwrap();
    projection.advance(world).unwrap()
}

fn assert_projection(world: &n::NativeWorld, fields: &ObjectFields, points: u32) {
    let progression = world.state().quest_progression.as_ref().unwrap();
    let player = progression.player.view();
    assert_eq!(fields.unit_level(), Some(player.level));
    assert_eq!(fields.player_xp(), Some(player.xp));
    assert_eq!(fields.player_next_level_xp(), Some(player.next_level_xp));
    assert_eq!(fields.player_talent_points(), Some(points));
    assert!(fields.raw_fields().any(|pair| pair == (1102, points)));
    assert_eq!(fields.unit_health(), Some(world.state().health));
    assert_eq!(fields.unit_max_health(), Some(world.player_limits().0));
    assert_eq!(
        fields.unit_power(0),
        Some(world.state().resources.caster.mana)
    );
    assert_eq!(fields.unit_max_power(0), Some(world.player_limits().1));
    assert_eq!(
        fields.unit_base_health(),
        Some(5000 + (player.level - 1) * 10)
    );
    assert_eq!(fields.unit_base_mana(), Some(59 + (player.level - 1) * 6));
    for (index, stat) in world
        .state()
        .combat
        .as_ref()
        .unwrap()
        .stats
        .attributes
        .iter()
        .enumerate()
    {
        assert_eq!(fields.unit_stat(index as u8), Some(*stat as u32));
    }
    assert_eq!(
        fields.player_skill(0).unwrap().max,
        (player.level * 5) as u16
    );
    assert_eq!(player.free_talent_points, points);
    assert_eq!(player.used_talent_points, 0);
}

#[test]
fn initial_delta_and_restored_level_sixty_fields_keep_unspent_points_and_xp_lock() {
    let mut world = world(1.0f32.to_bits());
    let mut projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    assert_projection(
        &world,
        &fields(
            &projection.initial_events(&world).unwrap(),
            projection.player_guid(),
        ),
        0,
    );

    // Source kill XP establishes a nonzero balance before direct level changes.
    let cast = projection
        .local(
            &world,
            UiRequest::CastAtNpc {
                ability: 60001,
                target: NpcHandle {
                    id: world.state().npcs[&2].id,
                },
            },
        )
        .unwrap();
    let mut authority = quests::QuestAuthority::default();
    world.step(1, Some(cast), &mut authority).unwrap();
    projection.advance(&world).unwrap();
    for _ in 0..20 {
        if world.state().npcs[&2].health == 0 {
            break;
        }
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
        projection.advance(&world).unwrap();
    }
    assert_eq!(world.player_level(), 2);
    assert_eq!(
        world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .player
            .view()
            .xp,
        10
    );
    apply(
        &mut world,
        &mut projection,
        n::AdminOperation::SetXpGain { enabled: false },
    );

    for level in 3..=60 {
        let events = apply(&mut world, &mut projection, n::AdminOperation::GainOneLevel);
        let levels: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::LevelUp(value) => Some(value.level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, [level]);
        assert!(!events
            .iter()
            .any(|event| matches!(event, SessionEvent::XpGain(_))));
        let progression = world.state().quest_progression.as_ref().unwrap();
        assert_eq!(progression.player.view().xp, 10);
        assert!(!progression.xp_enabled);
        if matches!(level, 9 | 10 | 59 | 60) {
            let points = level.saturating_sub(9);
            assert_projection(&world, &fields(&events, projection.player_guid()), points);
            let checkpoint =
                n::Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap();
            let restored = n::NativeWorld::restore_synthetic_component_test(
                world.profile().clone(),
                checkpoint,
            )
            .unwrap();
            let resumed = NativeProjection::new(
                &restored,
                restored.state().player,
                &super::quests::tests::Catalog::new(&restored),
            )
            .unwrap();
            let initial = resumed.initial_events(&restored).unwrap();
            assert_projection(&restored, &fields(&initial, resumed.player_guid()), points);
            assert!(!initial
                .iter()
                .any(|event| matches!(event, SessionEvent::LevelUp(_) | SessionEvent::XpGain(_))));
            assert!(
                !restored
                    .state()
                    .quest_progression
                    .as_ref()
                    .unwrap()
                    .xp_enabled
            );
        }
    }
    let capped = apply(&mut world, &mut projection, n::AdminOperation::GainOneLevel);
    assert_eq!(world.player_level(), 60);
    assert!(!capped
        .iter()
        .any(|event| matches!(event, SessionEvent::LevelUp(_))));
    assert_eq!(
        world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .player
            .view()
            .free_talent_points,
        51
    );
}

#[test]
fn talent_rate_projects_source_truncation_at_ten_and_sixty() {
    let mut world = world(1.5f32.to_bits());
    let mut projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    for level in 2..=60 {
        let events = apply(&mut world, &mut projection, n::AdminOperation::GainOneLevel);
        if matches!(level, 10 | 60) {
            assert_projection(
                &world,
                &fields(&events, projection.player_guid()),
                if level == 10 { 1 } else { 76 },
            );
        }
    }
}

#[test]
fn applied_level_one_aura_projects_its_original_binding_after_sixty_and_restore() {
    let (mut profile, input, mut data) = quests::setup_with_levels(60, 60, true, 1.0f32.to_bits());
    profile.npcs[0].max_health = 100_000;
    profile.npcs[0].ai.spawn = fixture::point(2.0);
    for ability in [&mut profile.abilities[0], &mut data.abilities[0].ability] {
        let n::AbilityAdmission::Available(spec) = &mut ability.admission else {
            unreachable!()
        };
        let spell::MageEffect::PeriodicDamage { duration_ms, .. } = &mut spec.effects[1] else {
            unreachable!()
        };
        *duration_ms = 20_000;
    }
    profile
        .initialize_player(&input, fixture::float(100.0), 1)
        .unwrap();
    profile.initialize_quest_progression(&input, data).unwrap();
    profile.admin = Some(n::AdminPolicy {
        world: [1; 16],
        world_owner: fixture::player().owner,
        allowlist: Default::default(),
        revision: 1,
    });
    let mut world =
        n::NativeWorld::synthetic_component_test(profile.clone(), [1; 16], fixture::player(), 42)
            .unwrap();
    let mut projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    let target = world.state().npcs[&2].id;
    let input = quests::command(
        &world,
        n::Command::Cast {
            ability: 60001,
            target,
        },
    );
    let mut authority = quests::QuestAuthority::default();
    world.step(1, Some(input), &mut authority).unwrap();
    projection.advance(&world).unwrap();
    for _ in 0..20 {
        if world.state().npcs[&2].aura_bindings.contains_key(&60001) {
            break;
        }
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
        projection.advance(&world).unwrap();
    }
    let original = world.state().npcs[&2].aura_bindings[&60001].clone();
    assert_eq!(original.level, 1);
    for _ in 2..=60 {
        apply(&mut world, &mut projection, n::AdminOperation::GainOneLevel);
    }
    assert_eq!(world.player_level(), 60);
    assert_eq!(world.state().npcs[&2].aura_bindings[&60001], original);
    let restored = n::NativeWorld::restore_synthetic_component_test(
        profile,
        n::Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), world.state_digest());
    let projection = NativeProjection::new(
        &restored,
        restored.state().player,
        &super::quests::tests::Catalog::new(&restored),
    )
    .unwrap();
    let initial = projection.initial_events(&restored).unwrap();
    assert_projection(&restored, &fields(&initial, projection.player_guid()), 51);
    let aura_fields = fields(&initial, projection.npc_guid(target).unwrap());
    assert!(aura_fields
        .unit_auras()
        .any(|aura| aura.spell_id == 60001 && aura.level == 1));
}
