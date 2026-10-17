// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored rows exercise runtime dispatch/claim validation, not native queries or geometry.
use super::*;
use crate::native::{
    synthetic_combat_fixture as combat, synthetic_fixture as f, synthetic_motion_fixture as motion,
};

fn profile() -> Profile {
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
            enabled: true,
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
    assert_eq!(NATIVE_FORMAT_VERSION, 48);
    assert_eq!(NATIVE_POLICY, "benilla-native-wander-leg-v48");
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
