// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored configuration/ownership tests; two-owner execution remains rejected.
use super::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::creature_motion as m;

fn second_owner(npc: &NpcProfile) -> NpcMotionGenesis {
    NpcMotionGenesis {
        npc_policy: MotionPolicy {
            walk_speed: f::real(1.5),
            run_speed: npc.ai.run_speed,
            settings: m::GroundSettings {
                combat_reach: npc.ai.combat_reach,
                bounding_radius: npc.ai.bounding_radius,
                collision_height: f::real(3.5),
                no_wounded_slowdown: false,
                backpedal_enabled: true,
                fanning_enabled: true,
            },
            audit: m::MotionAudit {
                flight_swimming_transport: m::Evidence::Absent,
                immobilization: m::Evidence::Absent,
                non_melee_casting: m::Evidence::Absent,
                speed_auras: m::Evidence::Absent,
                combat_scripts: m::Evidence::Absent,
                target_airborne_or_transport: m::Evidence::Absent,
            },
            home: HomeAudit {
                reset_position_override: m::Evidence::Absent,
                restore_temporary_faction: m::Evidence::Absent,
                addon_effects: m::Evidence::Absent,
                scripted_home_events: m::Evidence::Absent,
                temporary_summon_callback: m::Evidence::Absent,
            },
        },
        npc_can_enter_combat: false,
        npc_stand_state: 0,
        npc_spawn_orientation: Orientation::new(2.5).unwrap(),
        npc_initial_active: true,
        chase_walking: true,
        home_walking: true,
        creature_detection_range: 25,
    }
}
fn two_owner_configuration() -> Profile {
    let mut p = fixture::profile();
    let mut npc = p.npcs[0].clone();
    npc.ai.creature_id = 7;
    npc.ai.spawn = f::point(32.0);
    npc.ai.run_speed = f::real(4.0);
    npc.ai.combat_reach = f::real(2.0);
    npc.ai.bounding_radius = f::real(0.75);
    npc.ai.detection = f::real(25.0);
    npc.entry = 700;
    p.source_motion
        .as_mut()
        .unwrap()
        .npcs
        .insert(7, second_owner(&npc));
    p.npcs.push(npc);
    p
}
#[test]
fn exact_key_coverage_rejects_missing_extra_foreign_and_player_owners() {
    for case in 0..5 {
        let mut p = fixture::profile();
        let g = p.source_motion.as_mut().unwrap();
        match case {
            0 => {
                g.npcs.remove(&2);
            }
            1 => {
                let value = g.npcs.remove(&2).unwrap();
                g.npcs.insert(7, value);
            }
            2 => {
                g.npcs.insert(7, second_owner(&p.npcs[0]));
            }
            3 => {
                let value = g.npcs.remove(&2).unwrap();
                g.npcs.insert(SOLO_UNIT, value);
                p.npcs[0].ai.creature_id = SOLO_UNIT;
            }
            _ => {
                let value = g.npcs.remove(&2).unwrap();
                g.npcs.insert(0, value);
                p.npcs[0].ai.creature_id = 0;
            }
        }
        assert!(p.validate().is_err(), "case {case}");
        assert!(NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).is_err());
    }
    let mut p = fixture::profile();
    assert!(p.npc_motion(7).is_err());
    assert!(p.npc_motion(SOLO_UNIT).is_err());
    p.source_motion
        .as_mut()
        .unwrap()
        .npcs
        .insert(7, second_owner(&p.npcs[0]));
    assert!(
        p.npc_motion(7).is_err(),
        "a map entry alone is not a profile owner"
    );
    p.source_motion = None;
    assert!(p.npc_motion(2).is_err());
}
#[test]
fn per_owner_configuration_is_independent_but_execution_stays_singleton() {
    let p = two_owner_configuration();
    p.source_motion
        .as_ref()
        .unwrap()
        .validate_owners(&p)
        .unwrap();
    let a = p.npc_motion(2).unwrap();
    let b = p.npc_motion(7).unwrap();
    assert_ne!(a.npc_policy.run_speed, b.npc_policy.run_speed);
    assert_ne!(a.npc_policy.walk_speed, b.npc_policy.walk_speed);
    assert_ne!(
        a.npc_policy.settings.combat_reach,
        b.npc_policy.settings.combat_reach
    );
    assert_ne!(
        a.npc_policy.settings.bounding_radius,
        b.npc_policy.settings.bounding_radius
    );
    assert_ne!(
        a.npc_policy.settings.collision_height,
        b.npc_policy.settings.collision_height
    );
    assert_ne!(a.npc_spawn_orientation, b.npc_spawn_orientation);
    assert_ne!(a.npc_initial_active, b.npc_initial_active);
    assert_ne!(a.npc_can_enter_combat, b.npc_can_enter_combat);
    assert_ne!(a.chase_walking, b.chase_walking);
    assert_ne!(a.home_walking, b.home_walking);
    assert_ne!(a.creature_detection_range, b.creature_detection_range);
    assert!(p.validate().is_err());
    assert!(NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).is_err());
}
#[test]
fn owner_policy_swaps_and_shared_aggro_rate_disagreement_reject() {
    for case in 0..6 {
        let mut p = two_owner_configuration();
        let g = p.source_motion.as_mut().unwrap();
        match case {
            0 => {
                let a = g.npcs.remove(&2).unwrap();
                let b = g.npcs.remove(&7).unwrap();
                g.npcs.insert(2, b);
                g.npcs.insert(7, a);
            }
            1 => g.aggro_rate = f::real(2.0),
            2 => p.npcs[1].ai.aggro_rate = f::real(2.0),
            3 => g.npcs.get_mut(&7).unwrap().npc_policy.run_speed = f::real(8.0),
            4 => g.npcs.get_mut(&7).unwrap().npc_policy.settings.combat_reach = f::real(4.0),
            _ => g.npcs.get_mut(&7).unwrap().creature_detection_range = 24,
        }
        assert!(
            p.source_motion
                .as_ref()
                .unwrap()
                .validate_owners(&p)
                .is_err(),
            "case {case}"
        );
    }
}
#[test]
fn new_and_restore_select_the_passed_owner_without_first_owner_fallback() {
    let p = two_owner_configuration();
    let owner = p.npc(7).unwrap();
    let ai = ai::SoloCreatureAi::new(owner.ai.clone()).unwrap();
    let source = NpcMotion::new(&p, owner).unwrap().unwrap();
    assert_eq!(source.motion.pose().position, owner.ai.spawn);
    assert_eq!(
        source.motion.pose().orientation.bits(),
        p.npc_motion(7).unwrap().npc_spawn_orientation.bits()
    );
    assert!(source.active);
    let id = crate::NpcId {
        spawn: 7,
        generation: 0,
    };
    assert_eq!(
        NpcMotion::restore(source.snapshot(), &p, &ai, p.clock_origin_ms, id).unwrap(),
        source
    );
    let first = NpcMotion::new(&p, p.npc(2).unwrap()).unwrap().unwrap();
    assert!(NpcMotion::restore(first.snapshot(), &p, &ai, p.clock_origin_ms, id).is_err());
    assert!(NpcMotion::restore(
        source.snapshot(),
        &p,
        &ai,
        p.clock_origin_ms,
        crate::NpcId {
            spawn: 99,
            generation: 0
        }
    )
    .is_err());
    let mut missing = p.clone();
    missing.source_motion.as_mut().unwrap().npcs.remove(&7);
    assert!(NpcMotion::new(&missing, owner).is_err());
    assert!(NpcMotion::restore(source.snapshot(), &missing, &ai, p.clock_origin_ms, id).is_err());
}
#[test]
fn scalar_schema_missing_fields_and_duplicate_serialized_owners_reject() {
    let g = fixture::profile().source_motion.unwrap();
    let encoded = serde_json::to_string(&g).unwrap();
    assert_eq!(serde_json::from_str::<MotionGenesis>(&encoded).unwrap(), g);
    let entry = serde_json::to_string(g.npc(2).unwrap()).unwrap();
    let duplicate = encoded.replace(
        &format!("\"npcs\":{{\"2\":{entry}}}"),
        &format!("\"npcs\":{{\"2\":{entry},\"2\":{entry}}}"),
    );
    assert_ne!(duplicate, encoded);
    assert!(serde_json::from_str::<MotionGenesis>(&duplicate).is_err());
    let value = serde_json::to_value(&g).unwrap();
    for field in [
        "npcs",
        "aggro_rate",
        "creature_initial_aggro_delay_ms",
        "relocation_aggro_delay_ms",
    ] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<MotionGenesis>(missing).is_err(),
            "{field}"
        );
    }
    for field in [
        "npc_policy",
        "npc_spawn_orientation",
        "npc_initial_active",
        "creature_detection_range",
    ] {
        let mut missing = value.clone();
        missing["npcs"]["2"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<MotionGenesis>(missing).is_err(),
            "{field}"
        );
    }
    let mut old = value.clone();
    old.as_object_mut().unwrap().remove("npcs");
    old.as_object_mut().unwrap().remove("aggro_rate");
    old.as_object_mut()
        .unwrap()
        .extend(value["npcs"]["2"].as_object().unwrap().clone());
    assert!(serde_json::from_value::<MotionGenesis>(old).is_err());
    let mut per_owner_world_rate = value.clone();
    per_owner_world_rate["npcs"]["2"]["aggro_rate"] = serde_json::to_value(g.aggro_rate).unwrap();
    assert!(serde_json::from_value::<MotionGenesis>(per_owner_world_rate).is_err());
    let mut shared_owner_field = value;
    shared_owner_field["npc_can_enter_combat"] = true.into();
    assert!(serde_json::from_value::<MotionGenesis>(shared_owner_field).is_err());
}
#[test]
fn model_height_initialization_cannot_repair_missing_motion_owner() {
    let mut p = synthetic_creature_model_fixture::profile();
    p.source_motion.as_mut().unwrap().npcs.remove(&2);
    let before = p.clone();
    assert!(p.initialize_creature_collision_heights().is_err());
    assert_eq!(p, before);
}
#[test]
fn every_owner_and_shared_configuration_change_is_profile_bound() {
    let p = fixture::profile();
    for case in 0..8 {
        let mut next = p.clone();
        let g = next.source_motion.as_mut().unwrap();
        match case {
            0 => g.npcs.get_mut(&2).unwrap().npc_spawn_orientation = Orientation::new(2.5).unwrap(),
            1 => g.npcs.get_mut(&2).unwrap().npc_initial_active = true,
            2 => g.npcs.get_mut(&2).unwrap().npc_can_enter_combat = false,
            3 => g.player_can_enter_combat = false,
            4 => g.player_collision_height = f::real(3.0),
            5 => g.creature_initial_aggro_delay_ms += 1,
            6 => g.relocation_aggro_delay_ms += 1,
            _ => {
                g.aggro_rate = f::real(2.0);
                next.npcs[0].ai.aggro_rate = f::real(2.0);
            }
        }
        next.validate().unwrap();
        assert_ne!(p.digest(), next.digest());
    }
}

#[test]
fn authored_singleton_fixture_names_a_nondefault_owner_explicitly() {
    let mut p = f::profile();
    p.npcs[0].ai.creature_id = 3;
    p.source_motion = Some(fixture::genesis(&p, 3));
    p.validate().unwrap();
    assert!(p.npc_motion(2).is_err());
    assert_eq!(
        p.source_motion
            .as_ref()
            .unwrap()
            .npcs
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![3]
    );
    let w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
    assert_eq!(w.state().npcs.keys().copied().collect::<Vec<_>>(), vec![3]);
    assert_eq!(
        w.state().npcs[&3]
            .source_motion
            .as_ref()
            .unwrap()
            .motion
            .snapshot()
            .identity
            .creature,
        3
    );
}
