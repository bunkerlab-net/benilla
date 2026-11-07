// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored configuration/ownership tests; two-owner execution remains rejected.
use super::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::creature_motion as m;

fn second_owner(npc: &NpcProfile) -> NpcMotionGenesis {
    NpcMotionGenesis {
        default_movement: benilla_sim::creature_motion::DefaultMovement::Idle,
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
                addon_closure: HomeAddonClosure::Absent,
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

fn home_audit(p: &mut Profile) -> &mut HomeAudit {
    &mut p
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .npc_policy
        .home
}

fn home_generation_source(p: &Profile, sheath: Option<u32>) -> CreatureGenerationSource {
    use benilla_sim::creature_init as init;
    let npc = &p.npcs[0];
    let mut input = synthetic_creature_model_fixture::input();
    input.template.faction = npc.faction_template;
    input.template.regenerate_stats = npc.health_regeneration.regenerate_stats;
    input.spawn_addons = init::Rows::Complete(
        sheath
            .into_iter()
            .map(|sheath_state| init::SpawnAddonRow {
                guid: npc.ai.creature_id,
                addon: init::Addon {
                    mount: 0,
                    stand_state: 0,
                    sheath_state,
                    emote: 0,
                    move_flags: u32::MAX,
                    auras: vec![],
                },
            })
            .collect(),
    );
    CreatureGenerationSource {
        spawn_id: npc.ai.creature_id,
        spawn_entry: npc.entry,
        requested_spawn_template_entry: 0,
        respawn_delay_min_seconds: 2,
        respawn_delay_max_seconds: 3,
        input,
    }
}

#[test]
fn home_addon_absence_rejects_present_spawn_and_template_rows_without_installing() {
    use benilla_sim::creature_init as init;
    let mut empty = fixture::profile();
    empty
        .initialize_creature_generations(
            init::Rows::Complete(vec![home_generation_source(&empty, None)]),
            vec![2],
        )
        .unwrap();
    for template in [false, true] {
        let mut p = fixture::profile();
        let mut raw = home_generation_source(&p, Some(2));
        if template {
            let init::Rows::Complete(rows) = &raw.input.spawn_addons else {
                unreachable!()
            };
            raw.input.template_addons = init::Rows::Complete(vec![init::TemplateAddonRow {
                entry: raw.spawn_entry,
                addon: rows[0].addon.clone(),
            }]);
            raw.input.spawn_addons = init::Rows::Complete(vec![]);
        }
        let before = p.clone();
        assert_eq!(
            p.initialize_creature_generations(init::Rows::Complete(vec![raw.clone()]), vec![2]),
            Err(NativeError::UnsupportedMechanic(
                "source Home addon closure"
            ))
        );
        assert_eq!(p, before);

        home_audit(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
            source_identity: raw.identity(),
        };
        p.initialize_creature_generations(init::Rows::Complete(vec![raw]), vec![2])
            .unwrap();
        home_audit(&mut p).addon_closure = HomeAddonClosure::Absent;
        assert_eq!(
            p.validate(),
            Err(NativeError::UnsupportedMechanic(
                "source Home addon closure"
            ))
        );
    }
}

#[test]
fn home_source_closure_rejects_missing_stale_and_unrelated_generation_sources() {
    use benilla_sim::creature_init as init;
    for sheath in 0..=2 {
        let mut p = fixture::profile();
        let raw = home_generation_source(&p, Some(sheath));
        home_audit(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
            source_identity: raw.identity(),
        };
        assert_eq!(
            p.validate(),
            Err(NativeError::AuthorityUnavailable(
                "source Home generation source"
            ))
        );
        p.initialize_creature_generations(init::Rows::Complete(vec![raw.clone()]), vec![2])
            .unwrap();
        let before = p.clone();
        let mut changed = raw.clone();
        let init::Rows::Complete(rows) = &mut changed.input.spawn_addons else {
            unreachable!()
        };
        rows[0].addon.move_flags = 0;
        assert_ne!(changed.identity(), raw.identity());
        assert_eq!(
            p.initialize_creature_generations(init::Rows::Complete(vec![changed]), vec![2]),
            Err(NativeError::AuthorityBinding)
        );
        assert_eq!(p, before);

        let mut unrelated = raw;
        unrelated.spawn_id = 3;
        unrelated.input.spawn_id = 3;
        let init::Rows::Complete(rows) = &mut unrelated.input.spawn_addons else {
            unreachable!()
        };
        rows[0].guid = 3;
        CreatureGenerationCatalog::validate_sources(
            init::Rows::Complete(vec![unrelated.clone()]),
            vec![3],
        )
        .unwrap();
        home_audit(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
            source_identity: unrelated.identity(),
        };
        assert_eq!(p.validate(), Err(NativeError::AuthorityBinding));
    }
}

#[test]
fn source_owned_home_addons_do_not_open_any_other_home_callback() {
    use benilla_sim::creature_init as init;
    let mut p = fixture::profile();
    let raw = home_generation_source(&p, Some(2));
    home_audit(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    p.initialize_creature_generations(init::Rows::Complete(vec![raw]), vec![2])
        .unwrap();
    for proof in [m::Evidence::Present, m::Evidence::Unknown] {
        for field in 0..4 {
            let mut changed = p.clone();
            let home = home_audit(&mut changed);
            match field {
                0 => home.reset_position_override = proof,
                1 => home.restore_temporary_faction = proof,
                2 => home.scripted_home_events = proof,
                3 => home.temporary_summon_callback = proof,
                _ => unreachable!(),
            }
            let expected = match proof {
                m::Evidence::Present => {
                    NativeError::UnsupportedMechanic("source Home callbacks/reset override")
                }
                m::Evidence::Unknown => {
                    NativeError::AuthorityUnavailable("captured source Home lifecycle evidence")
                }
                m::Evidence::Absent => unreachable!(),
            };
            assert_eq!(changed.validate(), Err(expected), "field {field}");
        }
    }
}

#[test]
fn home_addon_closure_is_required_and_never_decoded_as_an_absence_default() {
    let p = fixture::profile();
    let encoded = serde_json::to_value(&p.source_motion).unwrap();
    for case in 0..4 {
        let mut changed = encoded.clone();
        let home = changed["npcs"]["2"]["npc_policy"]["home"]
            .as_object_mut()
            .unwrap();
        match case {
            0 => {
                home.remove("addon_closure");
            }
            1 => {
                home.remove("addon_closure");
                home.insert("addon_effects".into(), "Absent".into());
            }
            2 => {
                home.insert("addon_closure".into(), "Present".into());
            }
            3 => {
                home.insert(
                    "addon_closure".into(),
                    serde_json::json!({"GenerationSource": {}}),
                );
            }
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<MotionGenesis>(changed).is_err(),
            "case {case}"
        );
    }
}
