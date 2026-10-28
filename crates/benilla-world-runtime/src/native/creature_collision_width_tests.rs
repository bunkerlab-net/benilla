// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion, *,
};
use benilla_sim::creature_init as init;

fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn source(width: f32) -> CreatureCollisionWidthSource {
    CreatureCollisionWidthSource::from_initial_model(
        2,
        models::selection(),
        &models::catalog_with_width(width),
    )
    .unwrap()
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
    c.digest = crate::digest(b"benilla-native-state-v55\0", &c.state);
    c.checksum = crate::digest(
        b"benilla-native-checkpoint-v55\0",
        &(&c.state, c.digest, c.history_tip),
    );
    c
}

#[test]
fn initial_receipt_and_actual_loaded_model_own_width_not_render_scale_or_radius() {
    let p = models::profile();
    let w = world(p);
    let width = w.state().npcs[&2]
        .creature_collision_width()
        .unwrap()
        .witness();
    assert_eq!(width.native_display_id(), 34);
    assert_eq!(width.model_id(), 9);
    assert_eq!(width.object_scale(), f::float(1.25));
    assert_eq!(width.raw_width(), f::float(1.375));
    assert_eq!(width.width(), f::float(1.71875));
    assert_ne!(
        width.width().bits(),
        w.profile().npcs[0].ai.bounding_radius.bits()
    );
    width
        .validate_for(w.profile(), w.state().npcs[&2].id)
        .unwrap();
    let other = CreatureCollisionWidthSource::from_initial_model(
        2,
        models::selection(),
        &models::catalog_with_fields(1.375, 9.0, 1.75, false),
    )
    .unwrap();
    assert_eq!(other.initial_width().width(), width.width());
    assert_ne!(
        serde_json::to_value(&other).unwrap(),
        serde_json::to_value(source(1.375)).unwrap()
    );
}

#[test]
fn genuine_signed_zero_uses_exact_source_fallback_and_small_nonzero_is_retained() {
    for zero in [0.0, -0.0] {
        let s = source(zero);
        assert_eq!(s.initial_width().raw_width().bits(), zero.to_bits());
        assert_eq!(s.initial_width().width().bits(), 0x3f02_7bb3);
    }
    let s = source(f32::MIN_POSITIVE);
    assert_eq!(
        s.initial_width().width().bits(),
        (f32::MIN_POSITIVE * 1.25).to_bits()
    );
    assert_ne!(s.initial_width().width().bits(), 0x3f02_7bb3);
}

#[test]
fn raw_invalid_missing_short_and_scale_mismatched_rows_do_not_invent_a_width() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.125, f32::MAX] {
        assert!(
            CreatureCollisionWidthSource::from_initial_model(
                2,
                models::selection(),
                &models::catalog_with_width(value)
            )
            .is_err(),
            "{value}"
        );
    }
    for catalog in [
        benilla_formats::CreatureCatalog::default(),
        models::catalog_with_fields(1.0, 2.5, 1.75, true),
        models::catalog_with_fields(1.0, 2.5, 1.5, false),
    ] {
        assert!(
            CreatureCollisionWidthSource::from_initial_model(2, models::selection(), &catalog)
                .is_err()
        );
    }
    assert!(CreatureCollisionWidthSource::from_initial_model(
        1,
        models::selection(),
        &models::catalog()
    )
    .is_err());
}

#[test]
fn mutable_initialized_model_does_not_rewrite_the_opaque_selection() {
    let input = models::input();
    let (mut out, receipt) = init::prepare(&input)
        .unwrap()
        .resolve_with_model_receipt(init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: None,
        })
        .unwrap();
    out.model.display_id = 31;
    out.model.scale = f::float(99.0);
    assert_eq!(out.model.display_id, 31);
    assert_eq!(out.model.scale, f::float(99.0));
    let bound =
        CreatureCollisionWidthSource::from_initial_model(2, receipt, &models::catalog()).unwrap();
    assert_eq!(bound.initial_width().native_display_id(), 34);
    assert_eq!(bound.initial_width().object_scale(), f::float(1.25));
}

#[test]
fn explicit_catalog_missing_empty_duplicate_and_profile_mismatch_fail_atomically() {
    let mut p = models::profile();
    let before = p.clone();
    for rows in [
        CreatureCollisionWidthRows::Missing,
        CreatureCollisionWidthRows::Complete(vec![]),
        CreatureCollisionWidthRows::Complete(vec![source(1.0), source(1.0)]),
    ] {
        assert!(p.initialize_creature_collision_widths(rows).is_err());
        assert_eq!(p, before);
    }
    for field in 0..4 {
        let mut changed = p.clone();
        match field {
            0 => changed.npcs[0].display_id = 31,
            1 => changed.npcs[0].entry += 1,
            2 => changed.creature_capabilities = None,
            _ => changed.source_motion = None,
        }
        assert!(changed.validate().is_err());
    }
    let mut changed = p.clone();
    changed
        .initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![source(
            2.0,
        )]))
        .unwrap();
    assert_ne!(p.digest(), changed.digest());
    assert!(
        NativeWorld::restore_synthetic_component_test(changed, world(p).checkpoint().unwrap())
            .is_err()
    );
}

#[test]
fn conflicting_loaded_row_identities_cannot_share_one_source_catalog() {
    let second = CreatureCollisionWidthSource::from_initial_model(
        3,
        models::selection(),
        &models::catalog_with_width(2.0),
    )
    .unwrap();
    assert!(
        CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(
            vec![source(1.375), second]
        ))
        .is_err()
    );
}

#[test]
fn conflicting_sql_model_selection_identity_is_rejected() {
    let mut input = models::input();
    let init::Rows::Complete(rows) = &mut input.models else {
        unreachable!()
    };
    let original = rows.iter_mut().find(|row| row.id == 31).unwrap();
    original.gender = if original.gender == 0 { 1 } else { 0 };
    let (_, receipt) = init::prepare(&input)
        .unwrap()
        .resolve_with_model_receipt(init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: None,
        })
        .unwrap();
    let second =
        CreatureCollisionWidthSource::from_initial_model(3, receipt, &models::catalog()).unwrap();
    assert!(
        CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(
            vec![source(1.375), second]
        ))
        .is_err()
    );
}

#[test]
fn conflicting_selected_sql_model_bounds_are_rejected() {
    for radius in [true, false] {
        let mut input = models::input();
        let init::Rows::Complete(rows) = &mut input.models else {
            unreachable!()
        };
        let selected = rows.iter_mut().find(|row| row.id == 34).unwrap();
        if radius {
            selected.bounding_radius = f::float(0.75);
        } else {
            selected.combat_reach = f::float(2.0);
        }
        let (_, receipt) = init::prepare(&input)
            .unwrap()
            .resolve_with_model_receipt(init::SelectionRolls {
                model: Some(0),
                gender: Some(0),
                level: None,
            })
            .unwrap();
        let second =
            CreatureCollisionWidthSource::from_initial_model(3, receipt, &models::catalog())
                .unwrap();
        assert!(CreatureCollisionWidthCatalog::validate_sources(
            CreatureCollisionWidthRows::Complete(vec![source(1.375), second])
        )
        .is_err());
    }
}

#[test]
fn actual_route_freezes_width_and_checkpoint_and_replay_roundtrip_exactly() {
    let mut w = world(models::profile());
    let initial = w.checkpoint().unwrap();
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let request = a.queries.iter().find_map(route).unwrap();
    let owned = w.state().npcs[&2]
        .creature_collision_width()
        .unwrap()
        .witness();
    assert_eq!(request.require_collision_width(w.profile()).unwrap(), owned);
    assert_eq!(request.collision_width.as_ref(), Some(owned));
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
fn rehashed_checkpoint_scalar_source_and_generation_forgery_are_rejected() {
    let w = world(models::profile());
    for field in [
        "native_display_id",
        "model_id",
        "object_scale",
        "raw_width",
        "width",
        "source_binding",
        "content",
        "npc",
        "missing",
    ] {
        let changed = rehashed(&w, |state| {
            let witness = &mut state["npcs"]["2"]["source_collision_width"]["witness"];
            match field {
                "source_binding" | "content" => witness[field][0] = 0.into(),
                "npc" => witness[field]["generation"] = 1.into(),
                "missing" => state["npcs"]["2"]["source_collision_width"] = serde_json::Value::Null,
                "object_scale" | "raw_width" | "width" => witness[field] = 0x4000_0000_u32.into(),
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
fn route_replay_rejects_missing_forged_width_and_old_generation_atomically() {
    let mut w = world(models::profile());
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let checkpoint = w.checkpoint().unwrap();
    step(&mut w, &mut a);
    for field in 0..3 {
        let mut record = w.last_tick().unwrap().clone();
        let reply = record
            .replies
            .iter_mut()
            .find(|r| route(&r.request).is_some())
            .unwrap();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut reply.request.query else {
            unreachable!()
        };
        let mut value = serde_json::to_value(&route.collision_width).unwrap();
        match field {
            0 => value = serde_json::Value::Null,
            1 => value["width"] = 0x4000_0000_u32.into(),
            _ => value["npc"]["generation"] = 1.into(),
        }
        route.collision_width = serde_json::from_value(value).unwrap();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint.clone())
                .unwrap();
        assert!(replay.replay(&[record]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), checkpoint);
    }
}

#[test]
fn respawn_invalidates_initial_width_without_blocking_width_independent_routes() {
    let mut p = models::profile();
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let mut w = world(p);
    let old = w.state().npcs[&2]
        .creature_collision_width()
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
    assert!(w.state().npcs[&2].creature_collision_width().is_none());
    assert!(old.validate_for(w.profile(), id).is_err());
    let encoded = serde_json::to_value(&old).unwrap();
    let forged = rehashed(&w, |state| {
        state["npcs"]["2"]["source_collision_width"] = serde_json::json!({"witness": encoded});
        state["npcs"]["2"]["source_collision_width"]["witness"]["npc"]["generation"] =
            id.generation.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
    a.visits = Some(true);
    while !a.queries.iter().any(|q| route(q).is_some()) {
        assert!(w.state().tick < 35);
        step(&mut w, &mut a);
    }
    let request = a.queries.iter().find_map(route).unwrap();
    assert!(request.collision_width.is_none());
    assert!(request.require_collision_width(w.profile()).is_err());
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(w.state_digest(), restored.state_digest());
}

#[test]
fn missing_optional_width_is_explicit_and_never_a_hidden_schema_default() {
    let mut p = models::profile();
    p.creature_collision_widths = None;
    let mut w = world(p);
    let mut a = motion::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let request = a.queries.iter().find_map(route).unwrap();
    assert!(request.collision_width.is_none());
    assert!(request.require_collision_width(w.profile()).is_err());
    let mut json = serde_json::to_value(request).unwrap();
    json.as_object_mut().unwrap().remove("collision_width");
    assert!(serde_json::from_value::<CreatureRouteRequest>(json).is_err());
    let mut checkpoint = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    checkpoint["state"]["npcs"]["2"]
        .as_object_mut()
        .unwrap()
        .remove("source_collision_width");
    assert!(serde_json::from_value::<Checkpoint>(checkpoint).is_err());
}

#[test]
fn native23_checkpoint_records_and_replay_are_refused_without_migration_defaults() {
    let mut w = world(models::profile());
    let initial = w.checkpoint().unwrap();
    let mut old = serde_json::to_value(&initial).unwrap();
    old["version"] = 23.into();
    old["state"]["version"] = 23.into();
    old["state"]["policy"] = "benilla-native-warm-route-cache-v23".into();
    let old: Checkpoint = serde_json::from_value(old).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), old).is_err());
    step(&mut w, &mut motion::SyntheticMotionAuthority::default());
    let mut records = w.records().to_vec();
    records[0].version = 23;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial.clone())
            .unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), initial);
    assert_eq!(NATIVE_FORMAT_VERSION, 55);
    assert_eq!(crate::FORMAT_VERSION, 2);
}
