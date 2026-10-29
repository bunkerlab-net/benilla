// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored rows exercise runtime dispatch/claim validation, not native queries or geometry.
use super::*;
use crate::native::{
    synthetic_combat_fixture as combat, synthetic_fixture as f, synthetic_motion_fixture as motion,
};

fn profile() -> Profile {
    profile_with_enabled_configuration(true)
}
fn profile_with_enabled_configuration(enabled: bool) -> Profile {
    let mut p = combat::profile();
    p.npcs[0].ai.spawn = f::point(8.);
    p.initialize_creature_capabilities(motion::creature_capability_sources(&p))
        .unwrap();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-dispatch-map-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut dbc = b"WDBC".to_vec();
    for value in [1_u32, 42, 168, 9] {
        dbc.extend(value.to_le_bytes());
    }
    let mut row = [0_u32; 42];
    row[0] = p.map_id;
    row[1] = 1;
    for value in row {
        dbc.extend(value.to_le_bytes());
    }
    dbc.extend(b"\0Fixture\0");
    std::fs::write(
        &path,
        synthetic_creature_model_fixture::archive(&[("DBFilesClient\\Map.dbc", dbc)]),
    )
    .unwrap();
    let mut chain = benilla_formats::Chain::open(&path).unwrap();
    let maps = benilla_formats::load_map_catalog(&mut chain).unwrap();
    drop(chain);
    std::fs::remove_file(path).unwrap();
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        &maps,
    )
    .unwrap();
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn route(q: &AuthorityRequest) -> Option<&CreatureRouteRequest> {
    match &q.query {
        Query::SourceMotion(MotionQuery::Route(r)) => Some(r),
        _ => None,
    }
}
fn captured(p: Profile) -> (NativeWorld, CreatureRouteRequest) {
    let mut w = world(p);
    let mut a = motion::SyntheticMotionAuthority::default();
    loop {
        assert!(w.state().tick < 10);
        w.step(w.state().tick + 1, None, &mut a).unwrap();
        if let Some(r) = w
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .find_map(|r| route(&r.request))
        {
            let r = r.clone();
            return (w, r);
        }
    }
}

#[test]
fn saved_dispatch_metadata_comparison_never_recreates_live_authority() {
    let (world, _) = captured(profile());
    let original = world
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find(|reply| route(&reply.request).is_some())
        .unwrap()
        .request
        .clone();
    let decoded: AuthorityRequest =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert!(decoded
        .validate_saved_pathfinder_dispatch(world.profile())
        .is_ok());
    assert_eq!(
        decoded.require_pathfinder_dispatch(world.profile()),
        Err(NativeError::AuthorityUnavailable(
            "live pathfinder dispatch"
        )),
    );
    assert_eq!(
        route(&decoded)
            .unwrap()
            .pathfinder_dispatch
            .as_ref()
            .unwrap()
            .claim_constructor_query(),
        Err(NativeError::AuthorityUnavailable(
            "live pathfinder dispatch"
        )),
    );
    for changed in 0..6 {
        let mut wrong = decoded.clone();
        match changed {
            0 => wrong.binding.world[0] ^= 1,
            1 => wrong.binding.tick += 1,
            2 => wrong.binding.profile[0] ^= 1,
            3 => wrong.binding.geometry[0] ^= 1,
            4 => wrong.binding.npc.as_mut().unwrap().generation += 1,
            5 => {
                let Query::SourceMotion(MotionQuery::Route(request)) = &mut wrong.query else {
                    unreachable!();
                };
                request.request.launch_origin.x = f::real(100.);
            }
            _ => unreachable!(),
        }
        assert!(
            wrong
                .validate_saved_pathfinder_dispatch(world.profile())
                .is_err(),
            "saved dispatch metadata mutation {changed}",
        );
    }
}

#[test]
fn runtime_constructor_event_is_live_bound_and_one_use_but_transcript_is_not() {
    let (w, r) = captured(profile());
    let receipt = r.require_pathfinder_dispatch(w.profile()).unwrap();
    assert!(receipt.first_construction());
    assert_eq!(receipt.npc(), w.state().npcs[&2].id);
    assert_eq!(receipt.construction_binding(), r.request.binding);
    assert_eq!(receipt.dispatch_binding(), r.request.binding);
    assert_eq!(
        receipt.configuration_binding(),
        w.profile().pathfinder_configuration().unwrap().binding()
    );
    let cloned = receipt.clone();
    assert!(receipt.same_live_dispatch(&cloned));
    receipt.claim_constructor_query().unwrap();
    assert_eq!(
        cloned.claim_constructor_query(),
        Err(NativeError::AuthorityBinding)
    );
    let transcript: CreatureRouteRequest =
        serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(transcript, r);
    assert!(!receipt.same_live_dispatch(transcript.pathfinder_dispatch.as_ref().unwrap()));
    assert!(matches!(
        transcript.require_pathfinder_dispatch(w.profile()),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(transcript
        .pathfinder_dispatch
        .unwrap()
        .claim_constructor_query()
        .is_err());
    let mut wrong_goal = r.clone();
    wrong_goal.request.goal = f::point(80.);
    assert_eq!(
        wrong_goal.require_pathfinder_dispatch(w.profile()),
        Err(NativeError::AuthorityBinding)
    );
    let mut missing = serde_json::to_value(&r).unwrap();
    missing
        .as_object_mut()
        .unwrap()
        .remove("pathfinder_dispatch");
    assert!(serde_json::from_value::<CreatureRouteRequest>(missing).is_err());
}

#[test]
fn claimed_constructor_then_authority_failure_rolls_back_and_retry_gets_fresh_claim() {
    struct ClaimThenFail {
        p: Profile,
        base: motion::SyntheticMotionAuthority,
        fail: bool,
        seen: Vec<CreatureRouteRequest>,
    }
    impl Authority for ClaimThenFail {
        fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if let Some(r) = route(q) {
                let receipt = r.require_pathfinder_dispatch(&self.p)?;
                assert!(receipt.first_construction());
                receipt.claim_constructor_query()?;
                self.seen.push(r.clone());
                if self.fail {
                    return Err(NativeError::AuthorityUnavailable(
                        "authored failure after constructor claim",
                    ));
                }
            }
            self.base.resolve(q)
        }
    }
    let p = profile();
    let mut w = world(p.clone());
    let mut a = ClaimThenFail {
        p,
        base: Default::default(),
        fail: true,
        seen: vec![],
    };
    loop {
        let before = w.checkpoint().unwrap();
        let result = w.step(w.state().tick + 1, None, &mut a);
        if !a.seen.is_empty() {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "authored failure after constructor claim"
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        assert!(w.state().tick < 10);
    }
    a.fail = false;
    w.step(w.state().tick + 1, None, &mut a).unwrap();
    assert_eq!(a.seen.len(), 2);
    assert_eq!(a.seen[0], a.seen[1]);
    assert!(!a.seen[0]
        .pathfinder_dispatch
        .as_ref()
        .unwrap()
        .same_live_dispatch(a.seen[1].pathfinder_dispatch.as_ref().unwrap()));
}

#[test]
fn serialized_exact_replay_regenerates_events_without_rehydrating_queries() {
    let p = profile();
    let initial = world(p.clone()).checkpoint().unwrap();
    let (w, _) = captured(p);
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut corrupt = records.clone();
    let request = corrupt
        .iter_mut()
        .flat_map(|r| &mut r.replies)
        .find_map(|r| {
            if let Query::SourceMotion(MotionQuery::Route(q)) = &mut r.request.query {
                Some(q)
            } else {
                None
            }
        })
        .unwrap();
    request.pathfinder_dispatch = None;
    let mut replay = world(w.profile().clone());
    assert!(replay.replay(&corrupt).is_err());
}

#[test]
fn old_native29_through_native31_are_not_migrated() {
    let (w, _) = captured(profile());
    for (version, policy) in [
        (29, "benilla-native-owner-ignore-pathfinding-v29"),
        (30, "benilla-native-pathfinder-constructor-dispatch-v30"),
        (31, "benilla-native-first-owned-pathfinder-cache-v31"),
    ] {
        let mut old = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
        old["version"] = version.into();
        old["state"]["version"] = version.into();
        old["state"]["policy"] = policy.into();
        let old: Checkpoint = serde_json::from_value(old).unwrap();
        assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
        assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), old).is_err());
        let mut records = w.records().to_vec();
        records[0].version = version;
        assert!(decode_records(&encode_records(&records).unwrap()).is_err());
        assert!(world(w.profile().clone()).replay(&records).is_err());
    }
    assert_eq!(NATIVE_FORMAT_VERSION, 74);
    assert_eq!(NATIVE_POLICY, "benilla-native-mainhand-auto-equip-v74");
    assert_eq!(crate::FORMAT_VERSION, 2);
}

#[test]
fn selected_config_absence_stays_explicit_and_generation_rejects_old_event() {
    let mut p = profile();
    p.pathfinder_configuration = None;
    let (_, r) = captured(p);
    assert!(r.request.pathfinder_dispatch.is_some());
    assert!(r.pathfinder_dispatch.is_none());
    let (w, r) = captured(profile());
    let mut wrong = r.owner.npc();
    wrong.generation += 1;
    assert_eq!(
        r.pathfinder_dispatch
            .unwrap()
            .validate_for(w.profile(), wrong),
        Err(NativeError::AuthorityBinding)
    );
}

#[test]
fn actual_respawn_constructs_with_new_generation_and_rejects_the_previous_event() {
    let mut p = profile();
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let (mut w, old) = captured(p);
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = combat::SpatialOnly::default();
    a.base.visits = Some(false);
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 30);
        w.step(w.state().tick + 1, None, &mut a).unwrap();
    }
    a.base.visits = Some(true);
    loop {
        assert!(w.state().tick < 40);
        w.step(w.state().tick + 1, None, &mut a).unwrap();
        if let Some(r) = w
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .find_map(|r| route(&r.request))
        {
            let receipt = r.require_pathfinder_dispatch(w.profile()).unwrap();
            assert_eq!(receipt.npc().generation, 1);
            assert!(receipt.first_construction());
            let mut forged = r.clone();
            forged.pathfinder_dispatch = old.pathfinder_dispatch;
            assert_eq!(
                forged.require_pathfinder_dispatch(w.profile()),
                Err(NativeError::AuthorityBinding)
            );
            break;
        }
    }
}

#[test]
fn later_frame_failure_discards_staged_constructor_and_retry_can_claim_again() {
    struct LaterFailure {
        p: Profile,
        base: motion::SyntheticMotionAuthority,
        fail: bool,
        seen: Vec<PathfinderDispatchReceipt>,
    }
    impl Authority for LaterFailure {
        fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if self.fail && !self.seen.is_empty() {
                return Err(NativeError::AuthorityUnavailable(
                    "authored later frame failure",
                ));
            }
            if let Some(r) = route(q) {
                let receipt = r.require_pathfinder_dispatch(&self.p)?;
                receipt.claim_constructor_query()?;
                self.seen.push(receipt.clone());
            }
            self.base.resolve(q)
        }
        fn observe_derived(&mut self, _: &AuthorityReply) -> Result<(), NativeError> {
            if self.fail && !self.seen.is_empty() {
                Err(NativeError::AuthorityUnavailable(
                    "authored later frame failure",
                ))
            } else {
                Ok(())
            }
        }
    }
    let p = profile();
    let mut w = world(p.clone());
    let mut a = LaterFailure {
        p,
        base: Default::default(),
        fail: true,
        seen: vec![],
    };
    loop {
        let before = w.checkpoint().unwrap();
        let result = w.step(w.state().tick + 1, None, &mut a);
        if !a.seen.is_empty() {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "authored later frame failure"
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        assert!(w.state().tick < 10);
    }
    a.fail = false;
    w.step(w.state().tick + 1, None, &mut a).unwrap();
    assert_eq!(a.seen.len(), 2);
    assert_eq!(a.seen[0], a.seen[1]);
    assert!(!a.seen[0].same_live_dispatch(&a.seen[1]));
}

#[test]
fn live_event_cannot_be_relabeled_to_another_same_profile_world_or_tick() {
    let (w, _) = captured(profile());
    let context = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find(|r| route(&r.request).is_some())
        .unwrap()
        .request
        .clone();
    let receipt = context.require_pathfinder_dispatch(w.profile()).unwrap();
    assert_eq!(receipt.world(), [8; 16]);
    assert_eq!(receipt.dispatch_tick(), context.binding.tick);
    for case in 0..5 {
        let mut forged = context.clone();
        match case {
            0 => forged.binding.world = [9; 16],
            1 => forged.binding.tick += 1,
            2 => forged.binding.profile[0] ^= 1,
            3 => forged.binding.authority_policy[0] ^= 1,
            4 => forged.binding.geometry[0] ^= 1,
            _ => unreachable!(),
        }
        assert_eq!(
            forged.require_pathfinder_dispatch(w.profile()),
            Err(NativeError::AuthorityBinding)
        );
    }
    receipt.claim_constructor_query().unwrap();
    let mut missing = serde_json::to_value(receipt).unwrap();
    missing.as_object_mut().unwrap().remove("world");
    assert!(serde_json::from_value::<PathfinderDispatchReceipt>(missing).is_err());
}

#[test]
fn outer_dispatch_dedup_matches_full_validation_and_preserves_error_order() {
    // The explicit original composition remains the synchronous reference.
    fn full(context: &AuthorityRequest, profile: &Profile) -> Result<(), NativeError> {
        let Query::SourceMotion(MotionQuery::Route(request)) = &context.query else {
            return Err(NativeError::AuthorityBinding);
        };
        request
            .require_pathfinder_dispatch(profile)?
            .validate_for_context(profile, &context.binding)
    }
    let (w, _) = captured(profile());
    let context = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find(|r| route(&r.request).is_some())
        .unwrap()
        .request
        .clone();
    for case in 0..19 {
        let mut changed = context.clone();
        let Query::SourceMotion(MotionQuery::Route(request)) = &mut changed.query else {
            unreachable!()
        };
        match case {
            0 => (),
            1 => changed.binding.world[0] ^= 1,
            2 => changed.binding.tick += 1,
            3 => changed.binding.profile[0] ^= 1,
            4 => changed.binding.authority_policy[0] ^= 1,
            5 => changed.binding.geometry[0] ^= 1,
            6 => changed.binding.npc = None,
            7 => changed.binding.npc.as_mut().unwrap().generation += 1,
            8 => {
                changed.binding.npc.as_mut().unwrap().spawn = u64::MAX;
                changed.binding.world[0] ^= 1;
            }
            9 | 10 => {
                request.pathfinder_dispatch = None;
                if case == 9 {
                    changed.binding.npc = None;
                } else {
                    changed.binding.npc.as_mut().unwrap().spawn = u64::MAX;
                }
            }
            11 => {
                request.pathfinder_dispatch = serde_json::from_value(
                    serde_json::to_value(&request.pathfinder_dispatch).unwrap(),
                )
                .unwrap();
                changed.binding.npc = None;
                changed.binding.world[0] ^= 1;
            }
            12 => {
                request.request.pathfinder_dispatch = None;
                request.pathfinder_dispatch = None;
                changed.binding.npc = None;
            }
            13 => {
                request.request.goal = f::point(80.);
                changed.binding.npc = None;
            }
            14 => request.owner_position = f::point(80.),
            15 => request.request.target.as_mut().unwrap().id += 1,
            16 => request.request.target.as_mut().unwrap().pose.position = f::point(80.),
            17 => request.request.target.as_mut().unwrap().collision_height = f::real(1.),
            18 => {
                request
                    .pathfinder_dispatch
                    .as_mut()
                    .unwrap()
                    .evidence
                    .request_binding[0] ^= 1
            }
            _ => unreachable!(),
        }
        let expected = full(&changed, w.profile());
        assert_eq!(
            changed.require_pathfinder_dispatch(w.profile()).map(|_| ()),
            expected,
            "case={case}"
        );
        let explicit = match case {
            0 => Ok(()),
            8 => Err(NativeError::UnknownTarget),
            9 | 10 => Err(NativeError::AuthorityUnavailable(
                "owned pathfinder constructor dispatch",
            )),
            11 => Err(NativeError::AuthorityUnavailable(
                "live pathfinder dispatch",
            )),
            12 => Err(NativeError::Motion(m::MotionError::Missing(
                "pathfinder generator constructor dispatch",
            ))),
            _ => Err(NativeError::AuthorityBinding),
        };
        assert_eq!(expected, explicit, "case={case}");
    }
    // Invalid/missing profile sources still run inner validation first.
    for case in 0..3 {
        let mut changed = w.profile().clone();
        match case {
            0 => changed.pathfinder_configuration = None,
            1 => changed.tick_ms = 0,
            2 => changed.geometry_revision[0] ^= 1,
            _ => unreachable!(),
        }
        let expected = full(&context, &changed);
        assert!(expected.is_err());
        assert_eq!(
            context.require_pathfinder_dispatch(&changed).map(|_| ()),
            expected
        );
    }
    // Neither validation path consumes the live constructor claim.
    let receipt = context.require_pathfinder_dispatch(w.profile()).unwrap();
    full(&context, w.profile()).unwrap();
    receipt.claim_constructor_query().unwrap();
    context.require_pathfinder_dispatch(w.profile()).unwrap();
    full(&context, w.profile()).unwrap();
    assert_eq!(
        receipt.claim_constructor_query(),
        Err(NativeError::AuthorityBinding)
    );
}

#[test]
fn frozen_owner_dispatch_matches_full_errors_and_keeps_exact_live_inputs() {
    let (w, _) = captured(profile());
    let context = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find(|reply| route(&reply.request).is_some())
        .unwrap()
        .request
        .clone();
    let mut selected = Arc::new(w.profile().clone());
    let frozen = ValidatedRouteModelProfile::new(Arc::clone(&selected)).unwrap();
    for case in 0..27 {
        let mut changed = context.clone();
        let Query::SourceMotion(MotionQuery::Route(request)) = &mut changed.query else {
            unreachable!()
        };
        match case {
            0 => (),
            1 => changed.binding.world[0] ^= 1,
            2 => changed.binding.tick += 1,
            3 => changed.binding.profile[0] ^= 1,
            4 => changed.binding.authority_policy[0] ^= 1,
            5 => changed.binding.geometry[0] ^= 1,
            6 => changed.binding.npc = None,
            7 => changed.binding.npc.as_mut().unwrap().generation += 1,
            8 => {
                changed.binding.npc.as_mut().unwrap().spawn = u64::MAX;
                changed.binding.world[0] ^= 1;
            }
            9 => {
                request.pathfinder_dispatch = None;
                changed.binding.npc = None;
            }
            10 => {
                request.pathfinder_dispatch.as_mut().unwrap().live = None;
                changed.binding.npc = None;
            }
            11 => {
                request.request.pathfinder_dispatch = None;
                request.pathfinder_dispatch = None;
                changed.binding.npc = None;
            }
            12 => request.request.goal = f::point(80.),
            13 => request.owner_position = f::point(80.),
            14 => request.request.target.as_mut().unwrap().id += 1,
            15 => request.request.target.as_mut().unwrap().pose.position = f::point(80.),
            16 => request.request.target.as_mut().unwrap().collision_height = f::real(1.),
            17..=24 => {
                let receipt = request.pathfinder_dispatch.as_mut().unwrap();
                match case {
                    17 => receipt.evidence.policy = "invented-dispatch".into(),
                    18 => receipt.evidence.profile_binding[0] ^= 1,
                    19 => receipt.evidence.configuration_binding[0] ^= 1,
                    20 => receipt.evidence.npc.generation += 1,
                    21 => receipt.evidence.request_binding[0] ^= 1,
                    22 => receipt.evidence.world[0] ^= 1,
                    23 => receipt.evidence.dispatch_tick += 1,
                    24 => {
                        let mut dispatch = serde_json::to_value(receipt.evidence.dispatch).unwrap();
                        dispatch["construction"]["query_id"] = 0.into();
                        receipt.evidence.dispatch = serde_json::from_value(dispatch).unwrap();
                    }
                    _ => unreachable!(),
                }
            }
            25 => {
                request.pathfinder_dispatch.as_mut().unwrap().live = None;
                request
                    .pathfinder_dispatch
                    .as_mut()
                    .unwrap()
                    .evidence
                    .policy = "invalid".into();
            }
            26 => {
                request.request.goal = f::point(80.);
                changed.binding.npc.as_mut().unwrap().spawn = u64::MAX;
            }
            _ => unreachable!(),
        }
        // Freeze the route under its genuine owner context; each changed outer
        // callback still reaches the dispatch validator's original error order.
        let owned = frozen
            .validate_owner_context(Arc::new(route(&changed).unwrap().clone()), &context.binding)
            .unwrap();
        let full = changed.require_pathfinder_dispatch(w.profile()).map(|_| ());
        assert_eq!(
            owned
                .require_pathfinder_dispatch(&changed.binding)
                .map(|_| ()),
            full,
            "case={case}"
        );
        if case == 0 {
            assert!(full.is_ok());
        } else {
            assert!(full.is_err(), "case={case}");
        }
        match case {
            8 => assert_eq!(full, Err(NativeError::UnknownTarget)),
            9 => assert_eq!(
                full,
                Err(NativeError::AuthorityUnavailable(
                    "owned pathfinder constructor dispatch"
                ))
            ),
            10 => assert_eq!(
                full,
                Err(NativeError::AuthorityUnavailable(
                    "live pathfinder dispatch"
                ))
            ),
            11 => assert_eq!(
                full,
                Err(NativeError::Motion(m::MotionError::Missing(
                    "pathfinder generator constructor dispatch"
                )))
            ),
            25 | 26 => assert_eq!(full, Err(NativeError::AuthorityBinding)),
            _ => (),
        }
    }
    let mut request = Arc::new(route(&context).unwrap().clone());
    let owner = frozen
        .validate_owner_context(Arc::clone(&request), &context.binding)
        .unwrap();
    let receipt = owner.require_pathfinder_dispatch(&context.binding).unwrap();
    assert!(receipt.same_live_dispatch(context.require_pathfinder_dispatch(w.profile()).unwrap()));
    Arc::make_mut(&mut selected).tick_ms += 1;
    Arc::make_mut(&mut request).request.goal = f::point(80.);
    assert!(!frozen.matches_profile(&selected));
    assert!(!owner.matches_route(w.profile(), &request));
    let transcript: CreatureRouteRequest =
        serde_json::from_value(serde_json::to_value(owner.route()).unwrap()).unwrap();
    assert_eq!(&transcript, owner.route());
    assert!(!owner.matches_route(w.profile(), &transcript));
    let (_, replacement) = captured(w.profile().clone());
    assert_eq!(&replacement, owner.route());
    assert!(!owner.matches_route(w.profile(), &replacement));
    // Validation never takes the constructor claim, including after another
    // caller consumes it. Only the provider's explicit claim is one-use.
    receipt.claim_constructor_query().unwrap();
    owner.require_pathfinder_dispatch(&context.binding).unwrap();
    assert_eq!(
        receipt.claim_constructor_query(),
        Err(NativeError::AuthorityBinding)
    );
}

#[test]
fn frozen_owner_dispatch_defers_missing_or_disabled_configuration_errors() {
    let (w, _) = captured(profile());
    let context = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find(|reply| route(&reply.request).is_some())
        .unwrap()
        .request
        .clone();
    for disabled in [false, true] {
        let mut changed = if disabled {
            profile_with_enabled_configuration(false)
        } else {
            w.profile().clone()
        };
        if !disabled {
            changed.pathfinder_configuration = None;
        }
        let frozen = ValidatedRouteModelProfile::new(Arc::new(changed.clone())).unwrap();
        let mut binding = context.binding.clone();
        binding.profile = changed.digest();
        let owner = frozen
            .validate_owner_context(Arc::new(route(&context).unwrap().clone()), &binding)
            .unwrap();
        let expected = if disabled {
            NativeError::UnsupportedMechanic("pathfinder disabled constructor configuration")
        } else {
            NativeError::AuthorityUnavailable("selected pathfinder configuration")
        };
        assert_eq!(
            context.require_pathfinder_dispatch(&changed).map(|_| ()),
            Err(expected.clone())
        );
        assert_eq!(
            owner
                .require_pathfinder_dispatch(&context.binding)
                .map(|_| ()),
            Err(expected)
        );
    }
    let mut invalid = w.profile().clone();
    invalid.tick_ms = 0;
    assert_eq!(
        ValidatedRouteModelProfile::new(Arc::new(invalid.clone())).unwrap_err(),
        context.require_pathfinder_dispatch(&invalid).unwrap_err(),
    );
}

#[test]
fn synthetic_typed_owner_claim_replays_exact_facts_but_cannot_reopen_loaded_cache() {
    // This deliberately supplies authored cache and segment claims to test only
    // runtime ownership/replay. No native query or real geometry executes here.
    use crate::native::motion::route_cache_tests::{seed, SyntheticCacheAuthority};
    let mut w = world(profile());
    let initial = w.checkpoint().unwrap();
    let mut a = SyntheticCacheAuthority::owner_claim();
    seed(&mut w, &mut a);
    let record = w
        .records()
        .iter()
        .flat_map(|r| &r.replies)
        .find(|r| matches!(&r.request.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap();
    let Query::SourceMotion(MotionQuery::Route(request)) = &record.request.query else {
        unreachable!()
    };
    let receipt = record
        .request
        .require_pathfinder_dispatch(w.profile())
        .unwrap();
    assert!(receipt.first_construction());
    assert!(request.request.prior_cache.is_none());
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &record.facts else {
        unreachable!()
    };
    let m::Fact::RouteCalculation(calculation) = &reply.fact else {
        unreachable!()
    };
    assert_eq!(calculation.source, m::RouteSource::OwnerPathFinder);
    assert_eq!(calculation.after_cache.revision, 1);
    let source = w.state().npcs[&2].source_motion.as_ref().unwrap();
    m::MotionState::restore(source.motion.snapshot()).unwrap();
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    assert_eq!(replay.records(), w.records());
    assert!(NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap()
    )
    .is_err());
}

#[test]
fn synthetic_typed_owner_valid_first_calculation_later_failure_rolls_back_and_retries() {
    // Authored typed claims exercise the transaction, not native/geometry execution.
    use crate::native::motion::route_cache_tests::SyntheticCacheAuthority;
    let mut w = world(profile());
    let initial = w.checkpoint().unwrap();
    let mut a = SyntheticCacheAuthority::owner_claim();
    a.fail_after_route(true);
    let failed = loop {
        let before = w.checkpoint().unwrap();
        let records = w.records().to_vec();
        let result = w.step(w.state().tick + 1, None, &mut a);
        if a.calculated() {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "synthetic later frame failure",
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
            assert_eq!(w.records(), records);
            break a.last_route().unwrap().clone();
        }
        result.unwrap();
        assert!(w.state().tick < 10);
    };
    a.fail_after_route(false);
    w.step(w.state().tick + 1, None, &mut a).unwrap();
    let retried = a.last_route().unwrap();
    assert_eq!(retried, &failed);
    assert!(!retried
        .require_pathfinder_dispatch(w.profile())
        .unwrap()
        .same_live_dispatch(failed.require_pathfinder_dispatch(w.profile()).unwrap()));
    let launches = w
        .records()
        .iter()
        .flat_map(|r| &r.events)
        .filter(|event| {
            matches!(
                event,
                Event::NpcMotion {
                    event: m::Event::SplineLaunched { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(launches, 1);
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}

#[test]
fn home_local_one_use_receipt_rolls_back_replays_and_reopens_only_dead_local_evidence() {
    use crate::native::motion::route_cache_tests::SyntheticCacheAuthority;
    struct HomeClaim {
        profile: Profile,
        inner: SyntheticCacheAuthority,
        fail: bool,
        seen: Vec<AuthorityRequest>,
    }
    impl Authority for HomeClaim {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if route(request)
                .is_some_and(|route| route.request.kind == m::RouteKind::HomeForceDestination)
            {
                let receipt = request.require_pathfinder_dispatch(&self.profile)?;
                assert_eq!(
                    receipt.constructor_kind(),
                    m::PathfinderConstructorKind::HomeLocal
                );
                assert!(receipt.first_construction());
                assert_eq!(receipt.construction_binding(), receipt.dispatch_binding());
                receipt.claim_constructor_query()?;
                self.seen.push(request.clone());
                if self.fail {
                    return Err(NativeError::AuthorityUnavailable(
                        "authored Home post-claim failure",
                    ));
                }
            }
            self.inner.resolve(request)
        }
    }
    let mut p = profile();
    p.npcs[0].ai.hard_leash_radius = Some(f::real(1.));
    let mut w = world(p.clone());
    let initial = w.checkpoint().unwrap();
    let mut authority = HomeClaim {
        profile: p.clone(),
        inner: SyntheticCacheAuthority::owner_claim(),
        fail: true,
        seen: vec![],
    };
    loop {
        let before = w.checkpoint().unwrap();
        let result = w.step(w.state().tick + 1, None, &mut authority);
        if !authority.seen.is_empty() {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "authored Home post-claim failure"
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        assert!(w.state().tick < 100);
    }
    authority.fail = false;
    w.step(w.state().tick + 1, None, &mut authority).unwrap();
    assert_eq!(authority.seen.len(), 2);
    assert_eq!(authority.seen[0], authority.seen[1]);
    let failed = authority.seen[0].require_pathfinder_dispatch(&p).unwrap();
    let retried = authority.seen[1].require_pathfinder_dispatch(&p).unwrap();
    assert!(!failed.same_live_dispatch(retried));
    assert!(retried.claim_constructor_query().is_err());
    let route = route(&authority.seen[1]).unwrap();
    assert!(route.request.prior_cache.is_none());
    assert!(route.request.target.is_none());
    let transcript: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&authority.seen[1]).unwrap()).unwrap();
    assert_eq!(transcript, authority.seen[1]);
    assert!(transcript.require_pathfinder_dispatch(&p).is_err());
    let mut wrong = authority.seen[1].clone();
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut wrong.query else {
        unreachable!()
    };
    route.request.goal = f::point(99.);
    assert!(wrong.require_pathfinder_dispatch(&p).is_err());
    let npc = &w.state().npcs[&2];
    let source = npc.source_motion.as_ref().unwrap();
    let snapshot = source.snapshot();
    assert!(matches!(
        snapshot.motion.generator,
        m::Generator::Home { .. }
    ));
    let cache = snapshot
        .motion
        .spline
        .as_ref()
        .unwrap()
        .local_home_cache
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(cache.source, m::RouteSource::OwnerPathFinder);
    assert_eq!(cache.revision, 1);
    assert!(snapshot.motion.suspended_wander.is_none());
    snapshot.validate_saved_session_reopen().unwrap();
    // Only the completed Home calculation remains, never a retained PathFinder.
    let reopened =
        NativeWorld::restore_synthetic_component_test(p.clone(), w.checkpoint().unwrap()).unwrap();
    assert_eq!(reopened.state_digest(), w.state_digest());
    for case in 0..5 {
        let mut wrong = snapshot.clone();
        let cache = wrong
            .motion
            .spline
            .as_mut()
            .unwrap()
            .local_home_cache
            .as_mut()
            .unwrap();
        match case {
            0 => cache.binding.owner_generation += 1,
            1 => cache.include_flags = 13,
            2 => cache.binding.identity.creature += 1,
            3 => cache.binding.inventory = [0; 32],
            4 => cache.revision += 1,
            _ => unreachable!(),
        }
        assert!(
            NpcMotion::restore(wrong, &p, &npc.ai, w.state().now_ms, npc.id).is_err(),
            "Home restore case {case}"
        );
    }
    let mut retained_default = snapshot;
    retained_default.motion.suspended_wander = Some(m::WanderSnapshot {
        center: p.npcs[0].ai.spawn,
        radius: f::real(1.),
        next_move_timer_ms: 0,
        next_move_count: 1,
        pathfinder_cache: Some(cache.clone()),
        failed_route: None,
        normal_route: None,
        pathfinder_construction: retried.construction_binding(),
    });
    // Guard-only adversarial payload: historical Home evidence is not an excuse
    // to reopen any retained native default cache beside it.
    assert!(retained_default.validate_saved_session_reopen().is_err());
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay = NativeWorld::restore_synthetic_component_test(p, initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
