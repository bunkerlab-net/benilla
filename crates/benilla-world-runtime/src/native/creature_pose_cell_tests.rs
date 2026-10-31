// SPDX-License-Identifier: GPL-3.0-or-later
//! Completed-pose observations only; no grid loading, registration or lookup claim.
use super::*;
use benilla_sim::resident_map::compute_cell_pair;
use synthetic_source_spline_fixture::CompleteAuthority;
fn profile() -> Profile {
    let mut p = synthetic_source_spline_fixture::surviving_profile();
    p.initialize_map_classification(&synthetic_map_classification_fixture::catalog(p.map_id, 0))
        .unwrap();
    p.initialize_creature_pose_cells().unwrap();
    p
}
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}
fn assert_index(w: &NativeWorld) {
    let index = w.state.creature_pose_cells().unwrap();
    assert_eq!(index.entries().count(), w.state.npcs.len());
    for (&spawn, row) in index.entries() {
        let npc = &w.state.npcs[&spawn];
        let source = w
            .profile
            .creature_generations
            .as_ref()
            .unwrap()
            .source(spawn)
            .unwrap();
        assert_eq!(row.npc(), npc.id);
        assert_eq!(u64::from(row.identity().database_guid()), spawn);
        assert_eq!(row.identity().entry(), source.input.template.entry);
        assert_eq!(
            row.cell(),
            compute_cell_pair(npc.ai.position().x.bits(), npc.ai.position().y.bits()).unwrap()
        );
        let expected = w
            .state
            .npcs
            .iter()
            .filter_map(|(&id, npc)| {
                (compute_cell_pair(npc.ai.position().x.bits(), npc.ai.position().y.bits()).unwrap()
                    == row.cell())
                .then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(index.in_cell(row.cell()).collect::<Vec<_>>(), expected);
    }
}
fn step(w: &mut NativeWorld, command: Option<Command>) {
    let input = command.map(|command| synthetic_quest_fixture::command(w, command));
    w.step(w.state.tick + 1, input, &mut CompleteAuthority::default())
        .unwrap();
    assert_index(w);
}
fn reopen(w: &NativeWorld) {
    let cp = Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    let restored = NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    assert_index(&restored);
}
#[test]
fn committed_pose_index_tracks_actual_chase_corpse_and_respawn_without_registration_claim() {
    let mut w = world();
    assert_index(&w);
    let initial = w.checkpoint().unwrap();
    let before = w
        .state
        .creature_pose_cells()
        .unwrap()
        .entries()
        .map(|(&id, row)| (id, row.identity()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let victim = w.state.npcs[&3].id;
    let target = w.state.npcs[&2].id;
    let original_pose = w.state.npcs[&2].ai.position();
    step(
        &mut w,
        Some(Command::Cast {
            ability: 133,
            target: victim,
        }),
    );
    for _ in 0..100 {
        if w.state.npcs[&3].health == 0 {
            break;
        }
        step(&mut w, None);
    }
    assert_eq!(w.state.npcs[&3].health, 0);
    reopen(&w);
    step(
        &mut w,
        Some(Command::MovementPose {
            destination: synthetic_fixture::point(8.0),
            orientation: Orientation::new(std::f32::consts::PI).unwrap(),
            movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
        }),
    );
    step(
        &mut w,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    let mut moved = false;
    let mut hidden = false;
    for _ in 0..180 {
        let generation = w.state.npcs[&3].source_generation.as_ref().unwrap();
        hidden |= generation.current_corpse_removed_tick().is_some()
            && w.state.npcs[&3].id.generation == victim.generation;
        moved |= w.state.npcs[&2].ai.position() != original_pose;
        if w.state.npcs[&3].id.generation > victim.generation && moved {
            break;
        }
        step(&mut w, None);
    }
    assert!(moved);
    assert!(hidden);
    assert!(w.state.npcs[&3].id.generation > victim.generation);
    for (&id, row) in w.state.creature_pose_cells().unwrap().entries() {
        assert_eq!(row.identity(), before[&id]);
    }
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    assert_index(&replay);
}
#[test]
fn missing_source_owners_and_postselection_source_changes_fail_closed() {
    let mut absent = synthetic_source_spline_fixture::surviving_profile();
    let digest = absent.digest();
    assert!(absent.initialize_creature_pose_cells().is_err());
    assert_eq!(absent.digest(), digest);
    for kind in 0..3 {
        let mut p = profile();
        match kind {
            0 => p.map_classification = None,
            1 => p.source_motion = None,
            _ => p.creature_generations = None,
        };
        assert!(p.validate().is_err());
    }
    let mut p = profile();
    p.npcs[0].entry ^= 1;
    assert!(p.validate().is_err());
    let mut p = profile();
    let catalog = p.creature_generations.as_ref().unwrap();
    let order = catalog.cold_entry_order().to_vec();
    let mut sources = order
        .iter()
        .map(|spawn| catalog.source(*spawn).unwrap().clone())
        .collect::<Vec<_>>();
    sources[0].respawn_delay_max_seconds += 1;
    p.creature_generations = Some(
        CreatureGenerationCatalog::validate_sources(
            benilla_sim::creature_init::Rows::Complete(sources),
            order,
        )
        .unwrap(),
    );
    assert!(p.validate().is_err());
}
#[test]
fn rehashed_pose_cell_or_static_identity_corruption_and_missing_field_are_rejected() {
    let w = world();
    for kind in 0..5 {
        let mut cp = w.checkpoint().unwrap();
        let mut value = serde_json::to_value(&cp.state).unwrap();
        match kind {
            0 => {
                let x = value["creature_pose_cells"]["entries"]["2"]["cell"]["x"]
                    .as_u64()
                    .unwrap();
                value["creature_pose_cells"]["entries"]["2"]["cell"]["x"] =
                    serde_json::json!(x ^ 1);
            }
            1 => {
                let guid = value["creature_pose_cells"]["entries"]["2"]["identity"]["guid"]
                    .as_u64()
                    .unwrap();
                value["creature_pose_cells"]["entries"]["2"]["identity"]["guid"] =
                    serde_json::json!(guid ^ 1);
            }
            2 => {
                value["creature_pose_cells"]["entries"]
                    .as_object_mut()
                    .unwrap()
                    .remove("2");
            }
            3 => value["creature_pose_cells"]["cells"] = serde_json::json!({}),
            _ => value["creature_pose_cells"] = serde_json::Value::Null,
        }
        assert_ne!(value, serde_json::to_value(&cp.state).unwrap());
        cp.state = serde_json::from_value(value).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v82\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v82\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
    let mut missing = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    missing["state"]
        .as_object_mut()
        .unwrap()
        .remove("creature_pose_cells");
    assert!(serde_json::from_value::<Checkpoint>(missing).is_err());
}

#[test]
fn invalid_deserialized_cell_never_aliases_a_valid_pose_bucket() {
    let w = world();
    let index = w.state.creature_pose_cells().unwrap();
    let invalid = serde_json::from_value::<benilla_sim::resident_map::CellPair>(
        serde_json::json!({"x":0,"y":1024}),
    )
    .unwrap();
    assert!(!invalid.validate());
    assert_eq!(index.in_cell(invalid).count(), 0);
    let valid = index.entries().next().unwrap().1.cell();
    assert!(valid.x() > 0);
    let alias = serde_json::from_value::<benilla_sim::resident_map::CellPair>(
        serde_json::json!({"x":valid.x()-1,"y":valid.y()+1024}),
    )
    .unwrap();
    assert!(!alias.validate());
    assert_eq!(index.in_cell(alias).count(), 0);
}

#[test]
fn contradictory_event_ai_database_identity_refuses_observation_initialization_atomically() {
    // Reuse the source-composition order exercised by addon_composition/assistance:
    // bind actual generation, home, capability and visitor addon owners together.
    // The Fireball fixture seals an independent raw interaction owner; replacing
    // its AI afterward is correctly refused before this test's target boundary.
    let mut p = synthetic_combat_fixture::profile();
    p.npcs[0].ai.spawn = synthetic_fixture::point(6.0);
    let raw = synthetic_creature_generation_fixture::complete_scene_sources(&p).remove(0);
    p.source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .npc_policy
        .home
        .addon_closure = HomeAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    p = synthetic_creature_generation_fixture::install(p, vec![raw.clone()], vec![2]);
    p.creature_generations = None;
    let mut visitors = synthetic_two_owner_fixture::visitor_input(&p, vec![2]);
    let SourceVisitorRows::Complete(owners) = &mut visitors.owners else {
        unreachable!()
    };
    owners[0].addon_closure = SourceVisitorAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    p.source_visitors = Some(SourceVisitorCatalog::validate_sources(visitors).unwrap());
    let CreatureCapabilityRows::Complete(mut capabilities) =
        synthetic_motion_fixture::creature_capability_sources(&p)
    else {
        unreachable!()
    };
    capabilities[0].addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    p.initialize_creature_generation_capabilities(
        benilla_sim::creature_init::Rows::Complete(vec![raw]),
        vec![2],
        CreatureCapabilityRows::Complete(capabilities),
    )
    .unwrap();
    let mut p = synthetic_event_ai_fixture::install(p, 100);
    p.initialize_map_classification(&synthetic_map_classification_fixture::catalog(p.map_id, 0))
        .unwrap();
    p.validate().unwrap();
    assert_ne!(
        p.event_ai()
            .unwrap()
            .catalog()
            .owner(2)
            .unwrap()
            .database_guid,
        2
    );
    let digest = p.digest();
    assert_eq!(
        p.initialize_creature_pose_cells(),
        Err(NativeError::UnsupportedMechanic(
            "conflicting source creature database identity"
        ))
    );
    assert_eq!(p.digest(), digest);
    assert!(p.creature_pose_cells.is_none());
}
