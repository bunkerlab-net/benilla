// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion, *,
};

fn profile() -> Profile {
    let mut p = models::profile();
    p.initialize_creature_collision_heights().unwrap();
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn source(spawn: u64, width: f32, height: f32) -> CreatureCollisionWidthSource {
    CreatureCollisionWidthSource::from_initial_model(
        spawn,
        models::selection(),
        &models::catalog_with_height_fields(width, height, 2.5, 1.75, false),
    )
    .unwrap()
}
fn with_raw_height(height: f32) -> Profile {
    let mut p = models::profile();
    p.initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![source(
        2, 1.375, height,
    )]))
    .unwrap();
    p
}
fn route(q: &AuthorityRequest) -> Option<&CreatureRouteRequest> {
    match &q.query {
        Query::SourceMotion(MotionQuery::Route(r)) => Some(r),
        _ => None,
    }
}
fn rehashed(w: &NativeWorld, edit: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut c = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&c.state).unwrap();
    edit(&mut state);
    c.state = serde_json::from_value(state).unwrap();
    c.digest = crate::digest(b"benilla-native-state-v39\0", &c.state);
    c.checksum = crate::digest(
        b"benilla-native-checkpoint-v39\0",
        &(&c.state, c.digest, c.history_tip),
    );
    c
}

#[test]
fn height_is_explicitly_derived_from_owned_width_source_and_updates_motion_atomically() {
    let mut p = models::profile();
    assert!(p.creature_collision_heights.is_none());
    let before = p.clone();
    p.initialize_creature_collision_heights().unwrap();
    assert_ne!(p.digest(), before.digest());
    let w = world(p.clone());
    let height = w.state().npcs[&2]
        .creature_collision_height()
        .unwrap()
        .witness();
    let width = w.state().npcs[&2]
        .creature_collision_width()
        .unwrap()
        .witness();
    assert_eq!(height.native_display_id(), 34);
    assert_eq!(height.model_id(), 9);
    assert_eq!(height.object_scale(), f::float(1.25));
    assert_eq!(height.raw_height(), f::float(2.875));
    assert_eq!(height.height(), f::float(3.59375));
    assert_eq!(height.width_source_binding(), width.source_binding());
    assert_eq!(
        height.height().bits(),
        p.source_motion
            .as_ref()
            .unwrap()
            .npc(2)
            .unwrap()
            .npc_policy
            .settings
            .collision_height
            .bits()
    );
    assert_ne!(height.height().bits(), p.npcs[0].ai.bounding_radius.bits());
    assert_ne!(height.height(), width.width());
    assert_eq!(
        p.creature_collision_heights
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .initial_height()
            .height(),
        height.height()
    );
    p.initialize_creature_collision_heights().unwrap();
    assert_eq!(p, *w.profile());
}

#[test]
fn exact_signed_zero_fallback_and_small_nonzero_height_remain_distinct() {
    for raw in [0.0, -0.0, f32::MIN_POSITIVE] {
        let mut p = with_raw_height(raw);
        p.initialize_creature_collision_heights().unwrap();
        let w = world(p);
        let height = w.state().npcs[&2]
            .creature_collision_height()
            .unwrap()
            .witness();
        assert_eq!(height.raw_height().bits(), raw.to_bits());
        assert_eq!(
            height.height().bits(),
            if raw == 0.0 {
                0x4002_007e
            } else {
                (raw * 1.25).to_bits()
            }
        );
        assert_ne!(height.height().bits(), 0x3f02_7bb3);
    }
}

#[test]
fn invalid_raw_heights_remain_width_only_data_and_height_failure_is_atomic() {
    for raw in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.25,
        f32::MAX,
        f32::from_bits(1),
        900_000.0,
    ] {
        let mut p = with_raw_height(raw);
        p.validate().unwrap();
        let before = p.clone();
        assert!(
            p.initialize_creature_collision_heights().is_err(),
            "{raw:?}"
        );
        assert_eq!(p, before);
        let w = world(p);
        assert!(w.state().npcs[&2].creature_collision_width().is_some());
        assert!(w.state().npcs[&2].creature_collision_height().is_none());
    }
}

#[test]
fn render_scale_bits_are_retained_but_never_multiply_height() {
    let mut p = models::profile();
    p.initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
        CreatureCollisionWidthSource::from_initial_model(
            2,
            models::selection(),
            &models::catalog_with_height_fields(1.375, 2.875, 99.0, 1.75, false),
        )
        .unwrap(),
    ]))
    .unwrap();
    p.initialize_creature_collision_heights().unwrap();
    assert_ne!(p.digest(), profile().digest());
    assert_eq!(
        p.creature_collision_heights
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .initial_height()
            .height(),
        f::float(3.59375)
    );
}

#[test]
fn retained_height_rows_must_agree_even_for_a_width_only_catalog() {
    for raw in [3.0, f32::NAN, -0.0] {
        assert!(CreatureCollisionWidthCatalog::validate_sources(
            CreatureCollisionWidthRows::Complete(vec![
                source(2, 1.375, 2.875),
                source(3, 1.375, raw),
            ])
        )
        .is_err());
    }
    // The same raw NaN payload is coherent, even though height opt-in rejects it.
    CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(vec![
        source(2, 1.375, f32::from_bits(0x7fc0_0042)),
        source(3, 1.375, f32::from_bits(0x7fc0_0042)),
    ]))
    .unwrap();
}

#[test]
fn missing_sources_and_changed_width_catalog_or_motion_scalar_cannot_mint_height() {
    let mut p = motion::profile();
    let before = p.clone();
    assert!(p.initialize_creature_collision_heights().is_err());
    assert_eq!(p, before);
    let p = profile();
    for field in 0..5 {
        let mut changed = p.clone();
        match field {
            0 => changed.creature_collision_widths = None,
            1 => {
                changed
                    .source_motion
                    .as_mut()
                    .unwrap()
                    .npcs
                    .get_mut(&2)
                    .unwrap()
                    .npc_policy
                    .settings
                    .collision_height = ai::Real::new(7.0).unwrap()
            }
            2 => changed.npcs[0].display_id = 31,
            3 => changed.source_motion = None,
            _ => {
                let mut other = with_raw_height(3.0);
                other.initialize_creature_collision_heights().unwrap();
                changed.creature_collision_heights = other.creature_collision_heights;
            }
        }
        assert!(changed.validate().is_err());
        let before = changed.clone();
        assert!(changed.initialize_creature_collision_heights().is_err());
        assert_eq!(changed, before);
    }
    for (width, height) in [(2.0, 2.875), (1.375, 3.0)] {
        let mut changed = p.clone();
        assert!(changed
            .initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
                source(2, width, height)
            ]))
            .is_err());
        assert_eq!(changed, p);
        changed.creature_collision_widths = Some(
            CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(
                vec![source(2, width, height)],
            ))
            .unwrap(),
        );
        let before = changed.clone();
        assert!(changed.initialize_creature_collision_heights().is_err());
        assert_eq!(changed, before);
    }
    let mut invalid = models::profile();
    invalid.content_digest = [0; 32];
    let before = invalid.clone();
    assert!(invalid.initialize_creature_collision_heights().is_err());
    assert_eq!(invalid, before);
}

#[test]
fn actual_route_freezes_height_and_checkpoint_and_replay_roundtrip_exactly() {
    let mut w = world(profile());
    let initial = w.checkpoint().unwrap();
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let request = a.queries.iter().find_map(route).unwrap();
    let owned = w.state().npcs[&2]
        .creature_collision_height()
        .unwrap()
        .witness();
    assert_eq!(
        request.require_collision_height(w.profile()).unwrap(),
        owned
    );
    assert_eq!(request.collision_height.as_ref(), Some(owned));
    assert_eq!(
        request.request.owner_collision_height.bits(),
        owned.height().bits()
    );
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
    let checkpoint = Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint).unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
}

#[test]
fn forged_route_height_owner_identity_width_or_motion_scalar_is_rejected() {
    let mut w = world(profile());
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let original = a.queries.iter().find_map(route).unwrap();
    for field in [
        "native_display_id",
        "model_id",
        "object_scale",
        "raw_height",
        "height",
        "source_binding",
        "width_source_binding",
        "content",
        "profile",
        "npc",
    ] {
        let mut json = serde_json::to_value(original).unwrap();
        let value = &mut json["collision_height"];
        match field {
            "source_binding" | "width_source_binding" | "content" | "profile" => {
                value[field][0] = (value[field][0].as_u64().unwrap() ^ 0xff).into()
            }
            "npc" => value[field]["generation"] = 1.into(),
            "object_scale" | "raw_height" | "height" => value[field] = 0x4000_0000_u32.into(),
            _ => value[field] = 1.into(),
        }
        let forged: CreatureRouteRequest = serde_json::from_value(json).unwrap();
        assert!(
            forged.require_collision_height(w.profile()).is_err(),
            "{field}"
        );
    }
    for field in 0..9 {
        let mut forged = original.clone();
        match field {
            0 => forged.collision_height = None,
            1 => forged.collision_width = None,
            2 => forged.request.owner_collision_height = ai::Real::new(7.0).unwrap(),
            3 => forged.request.binding.identity.creature += 1,
            4 => forged.request.binding.identity.map += 1,
            5 => forged.request.binding.identity.content.0[0] ^= 1,
            6 => forged.request.binding.identity.geometry.0[0] ^= 1,
            7 => forged.owner_position.x = ai::Real::new(99.0).unwrap(),
            _ => {
                let mut width = serde_json::to_value(&forged.collision_width).unwrap();
                width["width"] = 0x4000_0000_u32.into();
                forged.collision_width = serde_json::from_value(width).unwrap();
            }
        }
        assert!(
            forged.require_collision_height(w.profile()).is_err(),
            "{field}"
        );
    }
    let mut other_profile = w.profile().clone();
    other_profile.npcs[0].name.push('x');
    other_profile.validate().unwrap();
    assert!(original.require_collision_height(&other_profile).is_err());
    other_profile = w.profile().clone();
    other_profile
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .npc_policy
        .settings
        .collision_height = ai::Real::new(7.0).unwrap();
    assert!(original.require_collision_height(&other_profile).is_err());
}

#[test]
fn rehashed_checkpoint_height_source_and_generation_forgery_are_rejected() {
    let w = world(profile());
    for field in [
        "native_display_id",
        "model_id",
        "object_scale",
        "raw_height",
        "height",
        "source_binding",
        "width_source_binding",
        "content",
        "profile",
        "npc",
        "missing",
    ] {
        let changed = rehashed(&w, |state| {
            let witness = &mut state["npcs"]["2"]["source_collision_height"]["witness"];
            match field {
                "source_binding" | "width_source_binding" | "content" | "profile" => {
                    witness[field][0] = (witness[field][0].as_u64().unwrap() ^ 0xff).into()
                }
                "npc" => witness[field]["generation"] = 1.into(),
                "missing" => {
                    state["npcs"]["2"]["source_collision_height"] = serde_json::Value::Null
                }
                "object_scale" | "raw_height" | "height" => witness[field] = 0x4000_0000_u32.into(),
                _ => witness[field] = 1.into(),
            }
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), changed).is_err(),
            "{field}"
        );
    }
}

#[test]
fn serialized_route_echo_forgery_replay_fails_atomically() {
    let mut w = world(profile());
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let checkpoint = w.checkpoint().unwrap();
    step(&mut w, &mut a);
    for field in 0..4 {
        let mut record = w.last_tick().unwrap().clone();
        let reply = record
            .replies
            .iter_mut()
            .find(|r| route(&r.request).is_some())
            .unwrap();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut reply.request.query else {
            unreachable!()
        };
        let mut value = serde_json::to_value(&route.collision_height).unwrap();
        match field {
            0 => value = serde_json::Value::Null,
            1 => value["height"] = 0x4000_0000_u32.into(),
            2 => value["npc"]["generation"] = 1.into(),
            _ => route.request.owner_collision_height = ai::Real::new(7.0).unwrap(),
        }
        route.collision_height = serde_json::from_value(value).unwrap();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint.clone())
                .unwrap();
        assert!(replay.replay(&[record]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), checkpoint);
    }
}

#[test]
fn respawn_clears_height_and_later_life_cannot_relabel_initial_selection() {
    let mut p = profile();
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let mut w = world(p);
    let old = w.state().npcs[&2]
        .creature_collision_height()
        .unwrap()
        .witness()
        .clone();
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = motion::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 25);
        step(&mut w, &mut a);
    }
    let id = w.state().npcs[&2].id;
    assert!(w.state().npcs[&2].creature_collision_height().is_none());
    assert!(old.validate_for(w.profile(), id).is_err());
    let encoded = serde_json::to_value(&old).unwrap();
    let forged = rehashed(&w, |state| {
        state["npcs"]["2"]["source_collision_height"] = serde_json::json!({"witness": encoded});
        state["npcs"]["2"]["source_collision_height"]["witness"]["npc"]["generation"] =
            id.generation.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
    a.visits = Some(true);
    while !a.queries.iter().any(|q| route(q).is_some()) {
        assert!(w.state().tick < 35);
        step(&mut w, &mut a);
    }
    let request = a.queries.iter().find_map(route).unwrap();
    assert!(request.collision_height.is_none());
    assert!(request.require_collision_height(w.profile()).is_err());
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(w.state_digest(), restored.state_digest());
}

#[test]
fn height_independent_routes_need_explicit_none_and_never_default_missing_schema_fields() {
    let mut w = world(models::profile());
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let request = a.queries.iter().find_map(route).unwrap();
    assert!(request.collision_height.is_none());
    request.require_collision_width(w.profile()).unwrap();
    assert!(request.require_collision_height(w.profile()).is_err());
    let mut json = serde_json::to_value(request).unwrap();
    assert_eq!(json["collision_height"], serde_json::Value::Null);
    json.as_object_mut().unwrap().remove("collision_height");
    assert!(serde_json::from_value::<CreatureRouteRequest>(json).is_err());
    let mut checkpoint = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    assert_eq!(
        checkpoint["state"]["npcs"]["2"]["source_collision_height"],
        serde_json::Value::Null
    );
    checkpoint["state"]["npcs"]["2"]
        .as_object_mut()
        .unwrap()
        .remove("source_collision_height");
    assert!(serde_json::from_value::<Checkpoint>(checkpoint).is_err());
}

#[test]
fn native25_and_all_earlier_checkpoints_records_and_replay_are_refused() {
    let mut w = world(profile());
    let initial = w.checkpoint().unwrap();
    step(&mut w, &mut motion::SyntheticMotionAuthority::default());
    for version in 0..26 {
        let mut old = serde_json::to_value(&initial).unwrap();
        old["version"] = version.into();
        old["state"]["version"] = version.into();
        let old: Checkpoint = serde_json::from_value(old).unwrap();
        assert!(
            Checkpoint::decode(&old.encode().unwrap()).is_err(),
            "version {version}"
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), old).is_err());
        let mut records = w.records().to_vec();
        records[0].version = version;
        assert!(decode_records(&encode_records(&records).unwrap()).is_err());
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial.clone())
                .unwrap();
        assert!(replay.replay(&records).is_err());
        assert_eq!(replay.checkpoint().unwrap(), initial);
    }
    assert_eq!(NATIVE_FORMAT_VERSION, 39);
    assert_eq!(crate::FORMAT_VERSION, 2);
}

#[test]
fn retained_spline_route_height_cannot_diverge_from_profile_even_when_rehashed() {
    for p in [profile(), models::profile()] {
        let mut w = world(p);
        let mut authority = motion::SyntheticMotionAuthority::default();
        step(&mut w, &mut authority);
        step(&mut w, &mut authority);
        let snapshot = w.state().npcs[&2]
            .source_motion
            .as_ref()
            .unwrap()
            .motion
            .snapshot();
        let request = &snapshot.spline.as_ref().unwrap().route_request;
        assert_eq!(
            request.owner_collision_height,
            w.profile()
                .source_motion
                .as_ref()
                .unwrap()
                .npc(2)
                .unwrap()
                .npc_policy
                .settings
                .collision_height
        );
        let genuine = NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            w.checkpoint().unwrap(),
        )
        .unwrap();
        assert_eq!(genuine.state_digest(), w.state_digest());
        let forged = rehashed(&w, |state| {
            state["npcs"]["2"]["source_motion"]["motion"]["spline"]["route_request"]
                ["owner_collision_height"] = 7.0_f32.to_bits().into();
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err()
        );
    }
}
