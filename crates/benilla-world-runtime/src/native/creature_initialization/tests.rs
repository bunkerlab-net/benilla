// SPDX-License-Identifier: GPL-3.0-or-later
//! Negative lifecycle/query tests. Positive dry proof belongs to the byte-backed
//! geometry integration tests; these invented snapshot hashes never supply a dry fact.
use super::*;
use crate::native::{synthetic_creature_model_fixture as models, synthetic_fixture as fixture};

fn dry_profile() -> Profile {
    let mut profile = models::profile();
    let mut input = models::input();
    input.template.faction = 1;
    input.template.regenerate_stats = 2;
    input.template.maximum_level = 3;
    let init::Rows::Complete(levels) = &mut input.class_levels else {
        unreachable!()
    };
    let first = levels[0];
    for level in 2..=3 {
        levels.push(init::ClassLevelRow { level, ..first });
    }
    let init::Rows::Complete(spawns) = &mut input.spawn_template else {
        unreachable!()
    };
    spawns[0].spawn_flags = 1;
    profile
        .initialize_creature_generations(
            init::Rows::Complete(vec![CreatureGenerationSource {
                spawn_id: 2,
                spawn_entry: 60001,
                requested_spawn_template_entry: 0,
                respawn_delay_min_seconds: 2,
                respawn_delay_max_seconds: 3,
                input,
            }]),
            vec![2],
        )
        .unwrap();
    profile.initialize_creature_collision_heights().unwrap();
    let mut row = profile
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let CreatureCapabilityRows::Complete(templates) = &mut row.templates else {
        unreachable!()
    };
    templates[0].inhabit_type = 3;
    row.initialization = CreatureInitializationClosure::DryWater {
        map: profile.map_id,
        respawn_position: profile.npcs[0].ai.spawn,
        snapshot: CreatureInitializationSnapshot {
            map: profile.map_id,
            content: profile.content_digest,
            source_epoch: [4; 32],
            inventory: [5; 32],
            policy: [6; 32],
            effective_config: [7; 32],
            vmap_height_enabled: false,
        },
    };
    profile
        .initialize_creature_capabilities(CreatureCapabilityRows::Complete(vec![row]))
        .unwrap();
    profile
}

#[test]
fn declared_dry_source_cannot_construct_live_flags_without_an_observation() {
    let profile = dry_profile();
    let npc = crate::NpcId {
        spawn: 2,
        generation: 0,
    };
    let capabilities = profile
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source_capabilities(&profile, npc)
        .unwrap();
    assert!(capabilities.can_walk() && capabilities.can_swim() && !capabilities.can_fly());
    assert!(CreatureCapabilityState::new(&profile, npc).is_err());
    assert!(matches!(
        NativeWorld::synthetic_component_test(profile, [8; 16], fixture::player(), 19),
        Err(NativeError::AuthorityUnavailable(
            "creature initialization liquid observation"
        ))
    ));
}

#[test]
fn rejected_query_occurs_after_model_gender_before_level_and_rolls_back_the_cursor() {
    let profile = dry_profile();
    let catalog = profile.creature_generations.as_ref().unwrap();
    let npc = crate::NpcId {
        spawn: 2,
        generation: 0,
    };
    let mut full_cursor = 17;
    // This detached owner is used only to compare deterministic RNG stage offsets;
    // it is never accepted as a live habitat-3 capability or a positive dry proof.
    let (_, draws) = catalog
        .initial([8; 16], profile.digest(), npc, 23, &mut full_cursor)
        .unwrap();
    let level = draws
        .iter()
        .find(|draw| matches!(draw.purpose, DrawPurpose::CreatureLevel { .. }))
        .unwrap();
    let mut cursor = 17;
    let mut calls = 0;
    let result = catalog.initial_with_initialization(
        &profile,
        [8; 16],
        npc,
        23,
        &mut cursor,
        &mut |request| {
            calls += 1;
            request.validate_for(&profile).unwrap();
            assert_eq!(request.first_counter, 17);
            assert_eq!(request.before_level_counter, level.first_counter);
            assert_eq!(request.model_roll, Some(draws[0].value as u32));
            assert_eq!(request.gender_roll, Some(draws[1].value as u32));
            assert_eq!(request.model_gender_draws, draws[..2]);
            assert_eq!(request.classification_height.bits(), 0x4002_007e);
            assert_ne!(request.classification_height, request.collision_height);
            assert_eq!(request.now_ms, profile.clock_origin_ms);
            Err(NativeError::AuthorityUnavailable(
                "missing actual-byte dry geometry",
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(calls, 1);
    assert_eq!(cursor, 17);
}

#[test]
fn model_gender_prefix_is_required_bounded_and_source_typed() {
    let profile = dry_profile();
    let catalog = profile.creature_generations.as_ref().unwrap();
    let npc = crate::NpcId {
        spawn: 2,
        generation: 0,
    };
    let mut cursor = 17;
    let result = catalog.initial_with_initialization(
        &profile,
        [8; 16],
        npc,
        23,
        &mut cursor,
        &mut |request| {
            let mut missing = serde_json::to_value(request).unwrap();
            missing
                .as_object_mut()
                .unwrap()
                .remove("model_gender_draws");
            assert!(serde_json::from_value::<CreatureInitializationRequest>(missing).is_err());
            let mut oversized = serde_json::to_value(request).unwrap();
            let first = oversized["model_gender_draws"][0].clone();
            oversized["model_gender_draws"] = serde_json::json!([first, first, first]);
            assert!(serde_json::from_value::<CreatureInitializationRequest>(oversized).is_err());
            for mutation in 0..6 {
                let mut foreign = request.clone();
                let draw = &mut foreign.model_gender_draws[0];
                match mutation {
                    0 => draw.cast_id = 1,
                    1 => draw.purpose = DrawPurpose::CreatureLevel { npc },
                    2 => draw.first_counter += 1,
                    3 => draw.consumed = 0,
                    4 => draw.low -= 1,
                    5 => draw.value = draw.high + 1,
                    _ => unreachable!(),
                }
                assert!(
                    foreign.validate_for(&profile).is_err(),
                    "prefix case {mutation}"
                );
            }
            Err(NativeError::AuthorityUnavailable(
                "negative prefix controls",
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(cursor, 17);
}

#[test]
fn missing_and_mismatched_source_closures_remain_closed() {
    let original = dry_profile();
    for mutation in 0..5 {
        let mut profile = original.clone();
        let mut row = profile
            .creature_capabilities
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .clone();
        if let CreatureInitializationClosure::DryWater {
            map,
            respawn_position,
            snapshot,
        } = &mut row.initialization
        {
            match mutation {
                0 => *map += 1,
                1 => respawn_position.x = fixture::real(123.0),
                2 => snapshot.content = [0; 32],
                3 => snapshot.effective_config = [0; 32],
                4 => row.initialization = CreatureInitializationClosure::Ground,
                _ => unreachable!(),
            }
        }
        assert!(profile
            .initialize_creature_capabilities(CreatureCapabilityRows::Complete(vec![row]))
            .is_err());
        assert_eq!(profile, original);
    }
}

#[test]
fn foreign_observation_and_every_wet_status_fail_before_level_and_commit() {
    let profile = dry_profile();
    let catalog = profile.creature_generations.as_ref().unwrap();
    let npc = crate::NpcId {
        spawn: 2,
        generation: 0,
    };
    for mutation in 0..12 {
        let mut cursor = 17;
        let result = catalog.initial_with_initialization(
            &profile,
            [8; 16],
            npc,
            23,
            &mut cursor,
            &mut |request| {
                let mut foreign = request.clone();
                match mutation {
                    0 => foreign.binding.world[0] ^= 1,
                    1 => foreign.binding.profile[0] ^= 1,
                    2 => foreign.generation_source[0] ^= 1,
                    3 => foreign.model_source[0] ^= 1,
                    4 => foreign.position.x = fixture::real(123.0),
                    5 => foreign.collision_height = fixture::float(123.0),
                    6 => foreign.now_ms += 1,
                    7 => foreign.ticket += 1,
                    8 => foreign.binding.npc.as_mut().unwrap().generation += 1,
                    9 => foreign.before_level_counter += 1,
                    10 => foreign.classification_height = fixture::float(1.0),
                    11 => foreign.required_type = 1,
                    _ => unreachable!(),
                }
                // Deliberately foreign negative input. No result is accepted as dry proof.
                Ok(CreatureInitializationFacts {
                    request: foreign,
                    status: environment::LiquidStatus::NoWater,
                    data: None,
                })
            },
        );
        assert!(result.is_err(), "foreign case {mutation}");
        assert_eq!(cursor, 17);
    }
    for status in [
        environment::LiquidStatus::AboveWater,
        environment::LiquidStatus::WaterWalk,
        environment::LiquidStatus::InWater,
        environment::LiquidStatus::UnderWater,
    ] {
        let mut cursor = 17;
        let result = catalog.initial_with_initialization(
            &profile,
            [8; 16],
            npc,
            23,
            &mut cursor,
            &mut |request| {
                Ok(CreatureInitializationFacts {
                    request: request.clone(),
                    status,
                    data: Some(environment::LiquidData {
                        entry: 1,
                        type_flags: 1,
                        level: 20f32.to_bits(),
                        depth_level: 0f32.to_bits(),
                    }),
                })
            },
        );
        assert!(matches!(
            result,
            Err(NativeError::UnsupportedMechanic(
                "wet creature initialization"
            ))
        ));
        assert_eq!(cursor, 17);
    }
}

#[test]
fn walk_in_water_and_preexisting_swimming_flags_reject_atomically() {
    let original = dry_profile();
    for mutation in 0..3 {
        let mut profile = original.clone();
        let mut source = profile
            .creature_generations
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .clone();
        match mutation {
            0 => source.input.template.extra_flags |= 0x8000,
            1 => source.input.template.unit_flags |= 0x8000,
            2 => {
                let init::Rows::Complete(spawns) = &mut source.input.spawn_template else {
                    unreachable!()
                };
                spawns[0].unit_flags = 0x8000;
            }
            _ => unreachable!(),
        }
        assert!(profile
            .initialize_creature_generations(init::Rows::Complete(vec![source]), vec![2])
            .is_err());
        assert_eq!(profile, original);
    }
}
