// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored static spawn inputs through normal commands and NativeStore reopen.
use super::*;
use benilla_sim::creature_init as init;
use serde_json::json;

fn data() -> n::Profile {
    let raw = |value| serde_json::from_value(value).unwrap();
    let capture = n::CapturedSpawnContext {
        spawns: init::Rows::Complete(vec![raw(json!({
            "guid": 2, "id": 60001, "map": 7, "position_x": {"decimal":"5"},
            "position_y": {"decimal":"0"}, "position_z": {"decimal":"0"},
            "orientation": {"decimal":"0"}, "spawntimesecsmin": 2,
            "spawntimesecsmax": 2, "spawndist": {"decimal":"0"},
            "MovementType": 0, "spawnMask": 1
        }))]),
        spawn_data: init::Rows::Complete(vec![]),
        spawn_templates: init::Rows::Complete(vec![raw(json!({
            "Entry": 0, "NpcFlags": -1, "UnitFlags": -1, "Faction": 0,
            "ModelId": 0, "EquipmentId": -1, "CurHealth": 1, "CurMana": 0,
            "SpawnFlags": 0, "RelayId": 0, "StringId": 0, "Name": "Authored default"
        }))]),
        exclusions: [
            "creature_spawn_entry",
            "creature_conditional_spawn",
            "spawn_group_spawn",
            "creature_linking",
            "creature_linking_template",
            "game_event_creature",
            "game_event_creature_data",
            "pool_creature",
            "pool_creature_template",
        ]
        .into_iter()
        .map(|table| (table.into(), init::Rows::Complete(vec![])))
        .collect(),
    };
    let plan = capture.compile(7, &[2]).unwrap();
    let mut base = fixture::profile();
    let mut movement = n::synthetic_motion_fixture::genesis(&base, 2);
    plan.bind_motion(7, &mut base.npcs[0], movement.npcs.get_mut(&2).unwrap())
        .unwrap();
    base.source_motion = Some(movement);
    let mut profile =
        n::synthetic_creature_model_fixture::with_complete_scene_generations_and_source_edit(
            base,
            |source| {
                let f = |value: f32| {
                    benilla_sim::classic_spell::SpellFloat::from_bits(value.to_bits()).unwrap()
                };
                source.input.template.displays = [init::DisplayChoice {
                    id: 0,
                    probability: 0,
                }; 4];
                source.input.template.displays[0] = init::DisplayChoice {
                    id: 60001,
                    probability: 1,
                };
                let init::Rows::Complete(models) = &source.input.models else {
                    unreachable!()
                };
                let mut model = models[0];
                model.id = 60001;
                model.other_gender_id = 0;
                source.input.models = init::Rows::Complete(vec![model]);
                source.input.displays = init::Rows::Complete(vec![init::DisplayRow {
                    id: 60001,
                    scale: f(1.0),
                }]);
                *source = plan.bind_generation(source.clone()).unwrap();
            },
        );
    // This authored corridor exercises the existing cache-free Idle reopen route.
    // Loaded Wander reopening remains governed by its independent native proof.
    profile.source_motion = None;
    profile
}
fn step(c: &mut NativeController) {
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(out.ticks, 1);
}
#[test]
fn captured_spawn_kill_respawn_save_reopen_preserves_timer_position_rng_and_suffix() {
    let h = Harness::new();
    let profile = data();
    let mut c = h.start(profile.clone());
    let initial_id = c.world.state().npcs[&2].id;
    c.receive(cast(&c)).unwrap();
    for _ in 0..100 {
        step(&mut c);
        if c.world.state().npcs[&2].health == 0 {
            break;
        }
    }
    assert_eq!(c.world.state().npcs[&2].health, 0);
    let respawn_at_ms = (c.world.state().now_ms / 1000 + 2) * 1000;
    c.shutdown().unwrap();
    let dead = c.world.checkpoint().unwrap();
    drop(c);
    let mut c = h.reopen(profile.clone());
    assert_eq!(c.world.checkpoint().unwrap(), dead);
    for _ in 0..100 {
        if c.world.state().now_ms < respawn_at_ms {
            assert_eq!(c.world.state().npcs[&2].id, initial_id);
        }
        step(&mut c);
        if c.world.state().npcs[&2].id != initial_id {
            break;
        }
    }
    assert_eq!(c.world.state().now_ms, respawn_at_ms);
    let npc = &c.world.state().npcs[&2];
    assert_eq!(npc.id.generation, initial_id.generation + 1);
    assert_eq!(npc.ai.position(), fixture::point(5.0));
    assert_eq!(npc.ai.effective_profile().respawn_delay_seconds, 2);
    c.shutdown().unwrap();
    let saved = c.world.checkpoint().unwrap();
    let mut uninterrupted =
        n::NativeWorld::restore_synthetic_component_test(profile.clone(), saved.clone()).unwrap();
    drop(c);
    let mut c = h.reopen(profile);
    assert_eq!(c.world.checkpoint().unwrap(), saved);
    for _ in 0..10 {
        step(&mut c);
        uninterrupted
            .step(
                uninterrupted.state().tick + 1,
                None,
                &mut fixture::KernelAuthority::default(),
            )
            .unwrap();
        assert_eq!(
            c.world.checkpoint().unwrap(),
            uninterrupted.checkpoint().unwrap()
        );
    }
}
