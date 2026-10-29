// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::creature_motion as m;

fn profile() -> Profile {
    let mut p = fixture::profile();
    p.initialize_creature_capabilities(fixture::creature_capability_sources(&p))
        .unwrap();
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn route(request: &AuthorityRequest) -> Option<&CreatureRouteRequest> {
    match &request.query {
        Query::SourceMotion(MotionQuery::Route(route)) => Some(route),
        _ => None,
    }
}

#[test]
fn actual_near_point_then_route_binds_owned_context_and_exact_inner_reply() {
    let mut w = world(profile());
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let record = w.last_tick().unwrap();
    let (near_index, near) = record
        .replies
        .iter()
        .enumerate()
        .find_map(|(index, reply)| match &reply.request.query {
            Query::SourceMotion(MotionQuery::NearPoint(near)) => Some((index, near)),
            _ => None,
        })
        .unwrap();
    let (route_index, reply, query) = record
        .replies
        .iter()
        .enumerate()
        .find_map(|(index, reply)| route(&reply.request).map(|q| (index, reply, q)))
        .unwrap();
    assert!(near_index < route_index);
    assert_eq!(query.owner, near.searcher);
    assert_eq!(
        query.owner,
        *w.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
    );
    assert_eq!(reply.request.binding.npc, Some(query.owner.npc()));
    assert_eq!(
        query.request.binding.identity.creature,
        query.owner.npc().spawn
    );
    assert_eq!(query.request.binding.identity.map, w.profile().map_id);
    assert_eq!(
        query.request.binding.identity.content.0,
        w.profile().content_digest
    );
    assert_eq!(
        query.request.binding.identity.geometry.0,
        w.profile().geometry_revision
    );
    assert_eq!(query.request.binding.now_ms, w.state().now_ms);
    assert_eq!(query.owner_position, near.request.owner.position);
    assert_eq!(query.owner_position, query.request.origin);
    assert_eq!(query.request.kind, m::RouteKind::ChaseInitial);
    let Facts::SourceMotion(MotionFacts::Spatial(inner)) = &reply.facts else {
        panic!("route must retain its simulation reply");
    };
    assert_eq!(inner.request, m::Query::Route(query.request.clone()));
    let m::Fact::Route(m::RouteOutcome::Complete(path)) = &inner.fact else {
        panic!("authored fixture route");
    };
    assert_eq!(path.source, m::RouteSource::SyntheticFixture);
    assert_eq!(path.points, vec![query.request.origin, query.request.goal]);
    assert!(!a.queries.iter().any(|q| matches!(
        q.query,
        Query::SourceMotion(MotionQuery::Spatial(m::Query::Route(_)))
    )));

    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let encoded = w.checkpoint().unwrap().encode().unwrap();
    let reopened = NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&encoded).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.state_digest(), w.state_digest());
}

#[test]
fn home_filter_owner_uses_current_map_pose_not_computed_spline_launch_position() {
    let mut p = profile();
    // The fixture walks a 2.5-yard/second run spline; use a short leash so
    // CombatManager requests Home while that first spline is still advancing.
    p.npcs[0].ai.hard_leash_radius = Some(f::real(1.0));
    let mut w = world(p);
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SyntheticMotionAuthority::default();
    loop {
        assert!(
            w.state().tick < 30,
            "missing Home transition: {:?}",
            w.state().npcs[&2]
                .source_motion
                .as_ref()
                .unwrap()
                .motion
                .snapshot()
        );
        step(&mut w, &mut a);
        let Some(home) = w.last_tick().unwrap().replies.iter().find_map(|reply| {
            route(&reply.request).filter(|q| q.request.kind == m::RouteKind::HomeForceDestination)
        }) else {
            continue;
        };
        let motion = &w.state().npcs[&2].source_motion.as_ref().unwrap().motion;
        assert_eq!(home.owner_position, home.request.origin);
        assert_eq!(home.owner_position, motion.pose().position);
        assert_ne!(home.owner_position, home.request.launch_origin);
        assert_eq!(
            motion.spline_position().unwrap(),
            home.request.launch_origin
        );
        break;
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}

#[test]
fn dispatch_origin_contract_is_executing_map_pose_even_for_home() {
    let source = include_str!("../../../benilla-sim/src/creature_motion/state.rs");
    let dispatch = source
        .split("    fn dispatch(")
        .nth(1)
        .unwrap()
        .split("    fn near_point(")
        .next()
        .unwrap();
    assert!(dispatch.contains("self.is_moving() && kind != RouteKind::HomeForceDestination"));
    assert!(dispatch.contains("self.relocate(true, target, authority, events)?;"));
    assert!(dispatch.contains(
        "let launch_origin = if self.is_moving() {\n            self.spline_position()?"
    ));
    assert!(dispatch.contains("origin: self.state.map_pose.position,"));
    assert!(dispatch.contains("ask(authority, Query::Route(request.clone()))?"));
}

#[test]
fn unselected_fixture_route_keeps_original_spatial_boundary() {
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    assert!(a.queries.iter().any(|q| matches!(
        q.query,
        Query::SourceMotion(MotionQuery::Spatial(m::Query::Route(_)))
    )));
    assert!(!a.queries.iter().any(|q| route(q).is_some()));
}

#[test]
fn respawned_route_uses_current_generation_and_rejects_stale_or_forged_witnesses() {
    let mut p = profile();
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let mut w = world(p);
    let old = w.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .witness()
        .clone();
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 25);
        step(&mut w, &mut a);
    }
    a.visits = Some(true);
    while !a.queries.iter().any(|q| route(q).is_some()) {
        assert!(w.state().tick < 35);
        step(&mut w, &mut a);
    }
    let query = a.queries.iter().find_map(route).unwrap();
    let npc = w.state().npcs[&2].id;
    assert_eq!(query.owner.npc(), npc);
    assert_eq!(npc.generation, 1);
    query.owner.validate_for(w.profile(), npc).unwrap();
    assert_eq!(
        old.validate_for(w.profile(), npc),
        Err(NativeError::AuthorityBinding)
    );
    for field in [
        "inhabit_type",
        "capability_movement_flags",
        "source_binding",
        "npc",
    ] {
        let mut value = serde_json::to_value(&query.owner).unwrap();
        match field {
            "source_binding" => {
                value[field][0] = ((value[field][0].as_u64().unwrap() + 1) % 256).into()
            }
            "npc" => value[field]["spawn"] = 3.into(),
            _ => value[field] = 7.into(),
        }
        let forged: CreatureCapabilityWitness = serde_json::from_value(value).unwrap();
        assert_eq!(
            forged.validate_for(w.profile(), npc),
            Err(NativeError::AuthorityBinding)
        );
    }
}

#[test]
fn missing_forged_or_incomplete_route_rolls_back_whole_frame_and_retries_exactly() {
    struct Changed {
        synthetic: fixture::SyntheticMotionAuthority,
        case: u8,
    }
    impl Authority for Changed {
        fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let mut answer = self.synthetic.resolve(q)?;
            if route(q).is_some() {
                match self.case {
                    0 => return Err(NativeError::AuthorityUnavailable("actual route provider")),
                    1 => {
                        let Query::SourceMotion(MotionQuery::Route(route)) =
                            &mut answer.request.query
                        else {
                            unreachable!()
                        };
                        route.owner_position = f::point(99.0);
                    }
                    2 => {
                        let Query::SourceMotion(MotionQuery::Route(route)) =
                            &mut answer.request.query
                        else {
                            unreachable!()
                        };
                        let mut value = serde_json::to_value(&route.owner).unwrap();
                        value["npc"]["generation"] = 1.into();
                        route.owner = serde_json::from_value(value).unwrap();
                    }
                    3 => {
                        let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &mut answer.facts
                        else {
                            unreachable!()
                        };
                        let m::Query::Route(route) = &mut reply.request else {
                            unreachable!()
                        };
                        route.goal = f::point(99.0);
                    }
                    4..=6 => {
                        let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &mut answer.facts
                        else {
                            unreachable!()
                        };
                        reply.fact = m::Fact::Route(match self.case {
                            4 => m::RouteOutcome::Incomplete,
                            5 => m::RouteOutcome::Truncated,
                            _ => m::RouteOutcome::Shortcut,
                        });
                    }
                    _ => unreachable!(),
                }
            }
            Ok(answer)
        }
    }
    for case in 0..7 {
        let mut w = world(profile());
        let mut a = Changed {
            synthetic: Default::default(),
            case,
        };
        step(&mut w, &mut a);
        let before = w.checkpoint().unwrap();
        assert!(w.step(2, None, &mut a).is_err(), "case {case}");
        assert_eq!(w.checkpoint().unwrap(), before);
        let mut expected =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), before).unwrap();
        step(
            &mut expected,
            &mut fixture::SyntheticMotionAuthority::default(),
        );
        step(&mut w, &mut fixture::SyntheticMotionAuthority::default());
        assert_eq!(w.last_tick(), expected.last_tick());
    }
}

#[test]
fn route_replay_rejects_forged_owner_position_generation_and_inner_clock() {
    let mut w = world(profile());
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let checkpoint = w.checkpoint().unwrap();
    step(&mut w, &mut a);
    for field in 0..6 {
        let mut forged = w.last_tick().unwrap().clone();
        let reply = forged
            .replies
            .iter_mut()
            .find(|r| route(&r.request).is_some())
            .unwrap();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut reply.request.query else {
            unreachable!()
        };
        let mut owner = serde_json::to_value(&route.owner).unwrap();
        match field {
            0 => owner["inhabit_type"] = 7.into(),
            1 => owner["capability_movement_flags"] = 0x400_u32.into(),
            2 => owner["npc"]["generation"] = 1.into(),
            3 => {
                owner["source_binding"][0] =
                    ((owner["source_binding"][0].as_u64().unwrap() + 1) % 256).into()
            }
            4 => route.owner_position = f::point(99.0),
            5 => route.request.binding.now_ms += 1,
            _ => unreachable!(),
        }
        route.owner = serde_json::from_value(owner).unwrap();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint.clone())
                .unwrap();
        assert!(replay.replay(&[forged]).is_err(), "field {field}");
        assert_eq!(replay.checkpoint().unwrap(), checkpoint);
    }
}

#[test]
fn pre_wrapper_native_format21_checkpoint_record_and_replay_are_refused() {
    let mut w = world(profile());
    let initial = w.checkpoint().unwrap();
    let mut old = serde_json::to_value(&initial).unwrap();
    old["version"] = 21.into();
    old["state"]["version"] = 21.into();
    old["state"]["policy"] = "benilla-native-creature-capabilities-v21".into();
    let old: Checkpoint = serde_json::from_value(old).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), old).is_err());
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let mut records = w.records().to_vec();
    records[0].version = 21;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial.clone())
            .unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), initial);
    assert_eq!(NATIVE_FORMAT_VERSION, 68);
    assert_eq!(crate::FORMAT_VERSION, 2);
}
