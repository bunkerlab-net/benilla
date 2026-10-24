// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    synthetic_creature_model_fixture as creatures, synthetic_fixture as f,
    synthetic_motion_fixture as motion, synthetic_player_model_fixture as models, *,
};
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn route(q: &AuthorityRequest) -> Option<&CreatureRouteRequest> {
    match &q.query {
        Query::SourceMotion(MotionQuery::Route(r)) => Some(r),
        _ => None,
    }
}
fn routed(p: Profile) -> (NativeWorld, motion::SyntheticMotionAuthority) {
    let mut w = world(p);
    let mut a = motion::SyntheticMotionAuthority::default();
    while !a.queries.iter().any(|q| route(q).is_some()) {
        assert!(w.state().tick < 10);
        step(&mut w, &mut a);
    }
    (w, a)
}
fn rehashed(w: &NativeWorld, edit: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut c = w.checkpoint().unwrap();
    let mut value = serde_json::to_value(&c.state).unwrap();
    edit(&mut value);
    c.state = serde_json::from_value(value).unwrap();
    c.digest = crate::digest(b"benilla-native-state-v51\0", &c.state);
    c.checksum = crate::digest(
        b"benilla-native-checkpoint-v51\0",
        &(&c.state, c.digest, c.history_tip),
    );
    c
}
#[test]
fn player_loaded_selection_derives_stored_effective_scale_and_height_atomically() {
    for gender in [0, 1] {
        let mut p = models::unowned_profile();
        let mut input = f::initialized_starter_input();
        input.gender = gender;
        p.initialize_player(&input, f::float(100.0), 1).unwrap();
        let before = p.clone();
        p.initialize_player_collision_height(&models::catalog(), &creatures::catalog())
            .unwrap();
        assert_ne!(p.digest(), before.digest());
        let source = p.player_collision_height.as_ref().unwrap();
        assert_eq!(
            (source.race(), source.class(), source.gender()),
            (1, 8, gender)
        );
        let receipt = source.initial_height();
        assert_eq!(
            receipt.native_display_id(),
            if gender == 0 { 31 } else { 34 }
        );
        assert_eq!(receipt.model_id(), if gender == 0 { 7 } else { 9 });
        assert_eq!(receipt.stored_object_scale(), f::float(4.375));
        assert_eq!(receipt.effective_object_scale(), f::float(4.375));
        assert_eq!(receipt.height(), f::float(12.578125));
        assert_eq!(
            p.source_motion
                .as_ref()
                .unwrap()
                .player_collision_height
                .bits(),
            receipt.height().bits()
        );
        let w = world(p.clone());
        let witness = w.state().player_collision_height().unwrap().witness();
        assert_eq!(witness.source_binding(), source.binding());
        assert_eq!(witness.height(), receipt.height());
        witness
            .validate_for(&p, w.state().world, w.state().player)
            .unwrap();
        p.initialize_player_collision_height(&models::catalog(), &creatures::catalog())
            .unwrap();
        assert_eq!(p, *w.profile());
    }
}
#[test]
fn signed_zero_scale_getter_and_height_fallback_are_distinct() {
    for zero in [0.0, -0.0] {
        for (display, model, raw) in [(zero, 2.5, 2.875), (1.75, zero, 2.875), (1.75, 2.5, zero)] {
            let p = models::profile_with_fields(display, model, raw);
            let height = p.player_collision_height.as_ref().unwrap().initial_height();
            if raw == 0.0 {
                assert_eq!(height.height().bits(), 0x4002_007e);
                assert_eq!(height.raw_height().bits(), raw.to_bits());
            } else {
                assert_eq!(height.stored_object_scale().bits(), 0);
                assert_eq!(height.effective_object_scale().bits(), 1.0_f32.to_bits());
                assert_eq!(height.height(), f::float(raw));
            }
        }
    }
    let p = models::profile_with_fields(1.0, 1.0, f32::MIN_POSITIVE);
    assert_eq!(
        p.player_collision_height
            .unwrap()
            .initial_height()
            .height()
            .bits(),
        f32::MIN_POSITIVE.to_bits()
    );
}
#[test]
fn missing_identity_motion_and_invalid_raw_operands_fail_without_changes() {
    let chars = models::catalog();
    for missing in [0, 1] {
        let mut p = models::unowned_profile();
        if missing == 0 {
            p.combat = None;
        } else {
            p.source_motion = None;
        }
        let before = p.clone();
        assert!(p
            .initialize_player_collision_height(&chars, &creatures::catalog())
            .is_err());
        assert_eq!(p, before);
    }
    for raw in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.25,
        f32::MAX,
        f32::from_bits(1),
        900_000.0,
    ] {
        for field in 0..3 {
            let mut operands = [1.75, 2.5, 2.875];
            operands[field] = raw;
            let mut p = models::unowned_profile();
            let before = p.clone();
            let catalog = creatures::catalog_with_height_fields(
                1.375,
                operands[2],
                operands[1],
                operands[0],
                false,
            );
            assert!(
                p.initialize_player_collision_height(&chars, &catalog)
                    .is_err(),
                "{field} {raw:?}"
            );
            assert_eq!(p, before);
        }
    }
}
#[test]
fn source_identity_and_motion_scalar_cannot_be_repaired_by_repeat_initializers() {
    let base = models::profile();
    let chars = models::catalog();
    let catalog = creatures::catalog();
    for field in 0..5 {
        let mut p = base.clone();
        match field {
            0 => p.combat.as_mut().unwrap().starter_identity.gender = 1,
            1 => p.source_motion.as_mut().unwrap().player_collision_height = f::real(99.0),
            2 => p.combat = None,
            3 => p.source_motion = None,
            _ => p
                .combat
                .as_mut()
                .unwrap()
                .initial_auras
                .push(InitialAuraMetadata {
                    spell_id: 88,
                    effect_index: 0,
                    aura_type: 61,
                    misc_value: 0,
                    amount: 0,
                    attributes: 0,
                    attributes_ex3: 0,
                    caster_aura_state: 0,
                    aura_interrupt_flags: 0,
                }),
        }
        let before = p.clone();
        assert!(p.validate().is_err());
        assert!(p
            .initialize_player_collision_height(&chars, &catalog)
            .is_err());
        assert_eq!(p, before);
        assert!(p
            .initialize_player(&f::initialized_starter_input(), f::float(100.0), 1)
            .is_err());
        assert_eq!(p, before);
    }
    let mut p = base.clone();
    let mut input = f::initialized_starter_input();
    input.gender = 1;
    assert!(p.initialize_player(&input, f::float(100.0), 1).is_err());
    assert_eq!(p, base);
    assert!(p
        .initialize_player_collision_height(
            &chars,
            &creatures::catalog_with_height_fields(1.375, 3.0, 2.5, 1.75, false)
        )
        .is_err());
    assert_eq!(p, base);
}
#[test]
fn route_and_near_point_use_actual_player_owner_pose_and_height() {
    let (w, a) = routed(models::profile());
    let owned = w.state().player_collision_height().unwrap().witness();
    let route = a.queries.iter().find_map(route).unwrap();
    assert_eq!(
        route
            .require_target_collision_height(w.profile(), w.state().world, w.state().player)
            .unwrap(),
        owned
    );
    assert_eq!(
        route
            .request
            .target
            .as_ref()
            .unwrap()
            .collision_height
            .bits(),
        owned.height().bits()
    );
    assert!(route.collision_height.is_none());
    let near = a
        .queries
        .iter()
        .find_map(|q| match &q.query {
            Query::SourceMotion(MotionQuery::NearPoint(q)) => Some(q),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        near.require_target_collision_height(w.profile(), w.state().world, w.state().player)
            .unwrap(),
        owned
    );
    assert_eq!(
        near.target_player_height.as_ref().unwrap().target(),
        &near.request.target
    );
    assert_eq!(near.request.target.pose.position, w.profile().player.start);
}
#[test]
fn forged_target_source_identity_geometry_or_pose_is_rejected() {
    let (w, a) = routed(models::profile());
    let original = a.queries.iter().find_map(route).unwrap();
    for field in [
        "native_display_id",
        "model_id",
        "stored_object_scale",
        "effective_object_scale",
        "raw_height",
        "height",
        "source_binding",
        "content",
        "profile",
        "geometry",
        "map",
        "world",
        "player",
    ] {
        let mut value = serde_json::to_value(original).unwrap();
        let s = &mut value["target_player_height"]["witness"];
        match field {
            "source_binding" | "content" | "profile" | "geometry" | "world" => {
                s[field][0] = 99.into()
            }
            "player" => s[field]["owner"][0] = 99.into(),
            "stored_object_scale" | "effective_object_scale" | "raw_height" | "height" => {
                s[field] = 0x4000_0000_u32.into()
            }
            _ => s[field] = 999.into(),
        }
        let forged: CreatureRouteRequest = serde_json::from_value(value).unwrap();
        assert!(
            forged
                .require_target_collision_height(w.profile(), w.state().world, w.state().player)
                .is_err(),
            "{field}"
        );
    }
    for field in 0..9 {
        let mut forged = original.clone();
        match field {
            0 => forged.target_player_height = None,
            1 => forged.request.target = None,
            2 => forged.request.target.as_mut().unwrap().id = 99,
            3 => forged.request.target.as_mut().unwrap().pose.position = f::point(99.0),
            4 => forged.request.target.as_mut().unwrap().collision_height = f::real(99.0),
            5 => forged.request.binding.identity.map += 1,
            6 => forged.request.binding.identity.creature += 1,
            7 => forged.request.binding.identity.geometry.0[0] ^= 1,
            _ => forged.request.binding.identity.content.0[0] ^= 1,
        }
        assert!(
            forged
                .require_target_collision_height(w.profile(), w.state().world, w.state().player)
                .is_err(),
            "{field}"
        );
    }
    assert!(original
        .require_target_collision_height(w.profile(), [9; 16], w.state().player)
        .is_err());
    let mut player = w.state().player;
    player.character = [9; 16];
    assert!(original
        .require_target_collision_height(w.profile(), w.state().world, player)
        .is_err());
    let mut p = w.profile().clone();
    p.npcs[0].name.push('x');
    p.validate().unwrap();
    assert!(original
        .require_target_collision_height(&p, w.state().world, w.state().player)
        .is_err());
}
#[test]
fn target_state_requires_explicit_none_when_optional_source_is_unavailable() {
    let (w, a) = routed(models::unowned_profile());
    assert!(w.state().player_collision_height().is_none());
    let r = a.queries.iter().find_map(route).unwrap();
    assert!(r.target_player_height.is_none());
    assert!(matches!(
        r.require_target_collision_height(w.profile(), w.state().world, w.state().player),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    let mut json = serde_json::to_value(r).unwrap();
    json.as_object_mut().unwrap().remove("target_player_height");
    assert!(serde_json::from_value::<CreatureRouteRequest>(json).is_err());
    let near = a
        .queries
        .iter()
        .find_map(|q| match &q.query {
            Query::SourceMotion(MotionQuery::NearPoint(q)) => Some(q),
            _ => None,
        })
        .unwrap();
    let mut json = serde_json::to_value(near).unwrap();
    assert!(json["target_player_height"].is_null());
    json.as_object_mut().unwrap().remove("target_player_height");
    assert!(serde_json::from_value::<CreatureNearPointRequest>(json).is_err());
    let mut json = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    assert!(json["state"]["source_player_collision_height"].is_null());
    json["state"]
        .as_object_mut()
        .unwrap()
        .remove("source_player_collision_height");
    assert!(serde_json::from_value::<Checkpoint>(json).is_err());
}
#[test]
fn rehashed_checkpoint_witness_and_missing_source_forgeries_fail() {
    let w = world(models::profile());
    for field in [
        "native_display_id",
        "model_id",
        "stored_object_scale",
        "effective_object_scale",
        "raw_height",
        "height",
        "source_binding",
        "content",
        "profile",
        "geometry",
        "map",
        "world",
        "player",
        "missing",
    ] {
        let c = rehashed(&w, |s| {
            let v = &mut s["source_player_collision_height"]["witness"];
            match field {
                "missing" => s["source_player_collision_height"] = serde_json::Value::Null,
                "source_binding" | "content" | "profile" | "geometry" | "world" => {
                    v[field][0] = 99.into()
                }
                "player" => v[field]["character"][0] = 99.into(),
                "stored_object_scale" | "effective_object_scale" | "raw_height" | "height" => {
                    v[field] = 0x4000_0000_u32.into()
                }
                _ => v[field] = 999.into(),
            }
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), c).is_err(),
            "{field}"
        );
    }
    let missing = world(models::unowned_profile());
    let receipt =
        serde_json::to_value(w.state().player_collision_height().unwrap().witness()).unwrap();
    let injected = rehashed(&missing, |s| {
        s["source_player_collision_height"] = serde_json::json!({"witness":receipt})
    });
    assert!(
        NativeWorld::restore_synthetic_component_test(missing.profile().clone(), injected).is_err()
    );
}
#[test]
fn retained_spline_target_height_and_unit_identity_are_rederived() {
    for p in [models::profile(), models::unowned_profile()] {
        let (w, _) = routed(p);
        for field in ["collision_height", "id", "bounding_radius", "combat_reach"] {
            let c = rehashed(&w, |s| {
                let target = &mut s["npcs"]["2"]["source_motion"]["motion"]["spline"]
                    ["route_request"]["target"];
                assert!(target.is_object());
                target[field] = if field == "id" {
                    99.into()
                } else {
                    0x42c6_0000_u32.into()
                };
            });
            assert!(
                NativeWorld::restore_synthetic_component_test(w.profile().clone(), c).is_err(),
                "{field}"
            );
        }
    }
}
#[test]
fn replay_is_exact_and_target_echo_forgery_is_atomic() {
    let p = models::profile();
    let initial = world(p.clone()).checkpoint().unwrap();
    let (w, _) = routed(p.clone());
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(p.clone(), initial.clone()).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let restored = NativeWorld::restore_synthetic_component_test(
        p.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    for field in 0..4 {
        let mut forged = records.clone();
        let reply = forged
            .iter_mut()
            .flat_map(|r| &mut r.replies)
            .find(|r| route(&r.request).is_some())
            .unwrap();
        let Query::SourceMotion(MotionQuery::Route(r)) = &mut reply.request.query else {
            unreachable!()
        };
        match field {
            0 => r.target_player_height = None,
            1 => r.request.target.as_mut().unwrap().pose.position = f::point(99.0),
            2 => r.request.target.as_mut().unwrap().collision_height = f::real(99.0),
            _ => {
                let mut v = serde_json::to_value(&r.target_player_height).unwrap();
                v["witness"]["player"]["owner"][0] = 99.into();
                r.target_player_height = serde_json::from_value(v).unwrap();
            }
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(p.clone(), initial.clone()).unwrap();
        assert!(replay.replay(&forged).is_err());
        assert_eq!(replay.checkpoint().unwrap(), initial);
    }
}

#[test]
fn retained_routes_require_chase_target_and_home_explicit_none() {
    use benilla_sim::creature_motion as m;
    let (w, _) = routed(models::profile());
    let forged = rehashed(&w, |s| {
        s["npcs"]["2"]["source_motion"]["motion"]["spline"]["route_request"]["target"] =
            serde_json::Value::Null;
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
    for kind in ["HomeForceDestination", "Backpedal"] {
        let forged = rehashed(&w, |s| {
            let route = &mut s["npcs"]["2"]["source_motion"]["motion"]["spline"]["route_request"];
            route["kind"] = kind.into();
            if kind == "HomeForceDestination" {
                route["target"] = serde_json::Value::Null;
            }
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err()
        );
    }
    let mut p = models::profile();
    p.npcs[0].ai.hard_leash_radius = Some(f::real(1.0));
    let mut w = world(p);
    let mut a = motion::SyntheticMotionAuthority::default();
    loop {
        assert!(w.state().tick < 30);
        step(&mut w, &mut a);
        let home = w.last_tick().unwrap().replies.iter().find_map(|r| {
            route(&r.request).filter(|q| q.request.kind == m::RouteKind::HomeForceDestination)
        });
        let Some(home) = home else {
            continue;
        };
        assert!(home.request.target.is_none());
        assert!(home.target_player_height.is_none());
        assert!(matches!(
            home.require_target_collision_height(w.profile(), w.state().world, w.state().player),
            Err(NativeError::AuthorityUnavailable(_))
        ));
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
            .unwrap();
        let target = a
            .queries
            .iter()
            .find_map(route)
            .unwrap()
            .request
            .target
            .unwrap();
        let forged = rehashed(&w, |s| {
            s["npcs"]["2"]["source_motion"]["motion"]["spline"]["route_request"]["target"] =
                serde_json::to_value(target).unwrap();
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err()
        );
        break;
    }
}

// Source-shaped authored initializer and loaded model rows, never real DBC evidence.
fn reputation_height_profile(modifier: bool) -> Profile {
    let (mut p, input, mut rows) = if modifier {
        synthetic_reputation_fixture::setup_with_modifier(10)
    } else {
        synthetic_reputation_fixture::setup()
    };
    // Keep the existing singleton motion domain; this case exercises height ownership.
    p.npcs.truncate(1);
    rows.quests.clear();
    rows.relations.starters.clear();
    rows.relations.finishers.clear();
    rows.npc_rows = vec![QuestNpcRow {
        entry: p.npcs[0].entry,
        npc_flags: 0,
        unit_flags: 0,
    }];
    rows.kill.creatures.truncate(1);
    p.initialize_quest_progression(&input, rows).unwrap();
    p.source_motion = Some(motion::genesis(&p, 2));
    p.initialize_creature_capabilities(motion::creature_capability_sources(&p))
        .unwrap();
    p.initialize_player_collision_height(&models::catalog(), &creatures::catalog())
        .unwrap();
    p
}
#[test]
fn retained_general_reputation_preserves_owned_player_dimensions_and_exact_binding() {
    let absent = reputation_height_profile(false);
    let present = reputation_height_profile(true);
    let source = present.player_collision_height.as_ref().unwrap();
    assert_eq!(source, absent.player_collision_height.as_ref().unwrap());
    assert_eq!(present.source_motion, absent.source_motion);
    assert_eq!(source.initial_height().height(), f::float(12.578125));
    assert_ne!(present.digest(), absent.digest());
    let w = world(present.clone());
    let witness = w.state().player_collision_height().unwrap().witness();
    assert_eq!(witness.height(), source.initial_height().height());
    witness
        .validate_for(&present, w.state().world, w.state().player)
        .unwrap();
    let checkpoint = w.checkpoint().unwrap();
    let reopened =
        NativeWorld::restore_synthetic_component_test(present.clone(), checkpoint.clone()).unwrap();
    assert_eq!(reopened.checkpoint().unwrap(), checkpoint);
    for variant in [
        "missing_genesis",
        "wrong_genesis",
        "amount",
        "duplicate",
        "active",
        "interrupt",
        "unknown",
        "dropped",
        "retyped",
    ] {
        let mut changed = present.clone();
        let combat = changed.combat.as_mut().unwrap();
        let index = combat
            .initial_auras
            .iter()
            .position(|a| a.aura_type == 156)
            .unwrap();
        match variant {
            "missing_genesis" => changed.quest_progression = None,
            "wrong_genesis" => changed.quest_progression = absent.quest_progression.clone(),
            "amount" => combat.initial_auras[index].amount += 1,
            "duplicate" => combat
                .initial_auras
                .push(combat.initial_auras[index].clone()),
            "active" => combat.initial_auras[index].attributes &= !0x40,
            "interrupt" => combat.initial_auras[index].aura_interrupt_flags = 0x400,
            "unknown" => combat.initial_auras[index].aura_type = 61,
            "dropped" => {
                combat.initial_auras.remove(index);
            }
            "retyped" => combat.initial_auras[index].aura_type = 22,
            _ => unreachable!(),
        }
        assert!(source.validate(&changed).is_err(), "{variant}");
        let before = changed.clone();
        assert!(
            changed
                .initialize_player_collision_height(&models::catalog(), &creatures::catalog())
                .is_err(),
            "{variant}"
        );
        assert_eq!(changed, before);
    }
}
