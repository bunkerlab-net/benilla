// SPDX-License-Identifier: GPL-3.0-or-later
//! SyntheticFixture cache lifecycle only; no loaded/source cache producer.
use crate::native::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::creature_motion as m;

fn profile() -> Profile {
    let mut p = fixture::profile();
    p.initialize_creature_capabilities(fixture::creature_capability_sources(&p))
        .unwrap();
    p
}
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], f::player(), 1234).unwrap()
}
#[derive(Default)]
struct SyntheticCacheAuthority {
    inner: fixture::SyntheticMotionAuthority,
    corrupt_generation: bool,
    fail_after_route: bool,
    calculated: bool,
}
impl Authority for SyntheticCacheAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if self.fail_after_route && self.calculated {
            return Err(NativeError::AuthorityUnavailable(
                "synthetic later frame failure",
            ));
        }
        let mut reply = self.inner.resolve(request)?;
        let Query::SourceMotion(MotionQuery::Route(q)) = &request.query else {
            return Ok(reply);
        };
        let Facts::SourceMotion(MotionFacts::Spatial(inner)) = &mut reply.facts else {
            panic!("motion reply");
        };
        let m::Fact::Route(outcome) = &inner.fact else {
            panic!("fixture route");
        };
        let mut after = q
            .request
            .prior_cache
            .as_deref()
            .cloned()
            .unwrap_or_else(|| m::PathFinderCache {
                binding: m::PathFinderCacheBinding {
                    identity: q.request.binding.identity,
                    owner_generation: q.owner.npc().generation,
                    inventory: [3; 32],
                    source_epoch: [4; 32],
                    navigation_policy: [5; 32],
                },
                revision: 0,
                source: m::RouteSource::SyntheticFixture,
                poly_refs: (0..77)
                    .map(|i| if !(2..74).contains(&i) { i + 1 } else { 0 })
                    .collect(),
                active_len: 2,
                point_limit: 74,
                path_type: 1,
                requested_end: q.request.goal,
                include_flags: 1,
                exclude_flags: 0,
            });
        after.revision += 1;
        after.requested_end = q.request.goal;
        if self.corrupt_generation {
            after.binding.owner_generation += 1;
        }
        inner.fact = m::Fact::RouteCalculation(Box::new(m::RouteCalculation {
            source: m::RouteSource::SyntheticFixture,
            outcome: outcome.clone(),
            prior_cache: q.request.prior_cache.clone(),
            after_cache: after,
        }));
        self.calculated = true;
        Ok(reply)
    }
}
fn seed(w: &mut NativeWorld, a: &mut SyntheticCacheAuthority) {
    while !a.calculated {
        assert!(w.state().tick < 10);
        w.step(w.state().tick + 1, None, a).unwrap();
    }
}
#[test]
fn synthetic_fixture_native_adapter_checks_after_cache_generation_atomically() {
    let mut w = world();
    let mut a = SyntheticCacheAuthority {
        corrupt_generation: true,
        ..Default::default()
    };
    loop {
        let before = w.checkpoint().unwrap();
        let result = w.step(w.state().tick + 1, None, &mut a);
        if a.calculated {
            assert_eq!(result, Err(NativeError::AuthorityBinding));
            assert_eq!(w.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        assert!(w.state().tick < 10);
    }
}
#[test]
fn synthetic_fixture_native_cache_validates_restore_owner_and_rejects_source_rehydration() {
    let mut w = world();
    let mut a = SyntheticCacheAuthority::default();
    seed(&mut w, &mut a);
    let npc = &w.state().npcs[&2];
    let source = npc.source_motion.as_ref().unwrap();
    let snapshot = source.snapshot();
    assert_eq!(
        NpcMotion::restore(
            snapshot.clone(),
            w.profile(),
            &npc.ai,
            w.state().now_ms,
            npc.id
        )
        .unwrap(),
        *source
    );
    let mut wrong_npc = npc.id;
    wrong_npc.generation += 1;
    assert!(NpcMotion::restore(
        snapshot.clone(),
        w.profile(),
        &npc.ai,
        w.state().now_ms,
        wrong_npc
    )
    .is_err());
    let mut forged = snapshot;
    let m::Generator::Chase(chase) = &mut forged.motion.generator else {
        panic!("chase");
    };
    chase.pathfinder_cache.as_mut().unwrap().source = m::RouteSource::OwnerPathFinder;
    assert!(NpcMotion::restore(forged, w.profile(), &npc.ai, w.state().now_ms, npc.id).is_err());
    let checkpoint = Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn synthetic_fixture_later_native_frame_failure_rolls_back_cache_and_world() {
    let mut w = world();
    let mut a = SyntheticCacheAuthority {
        fail_after_route: true,
        ..Default::default()
    };
    loop {
        let before = w.checkpoint().unwrap();
        let result = w.step(w.state().tick + 1, None, &mut a);
        if a.calculated {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "synthetic later frame failure"
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
            break;
        }
        result.unwrap();
        assert!(w.state().tick < 10);
    }
}

#[test]
fn synthetic_fixture_native_adapter_rejects_stale_prior_before_external_route() {
    let mut w = world();
    let mut a = SyntheticCacheAuthority::default();
    seed(&mut w, &mut a);
    let source = w
        .state
        .npcs
        .get_mut(&2)
        .unwrap()
        .source_motion
        .as_mut()
        .unwrap();
    let mut snapshot = source.motion.snapshot();
    let m::Generator::Chase(chase) = &mut snapshot.generator else {
        panic!("chase")
    };
    // Sim deliberately cannot authenticate a generation. The runtime owns that check.
    chase
        .pathfinder_cache
        .as_mut()
        .unwrap()
        .binding
        .owner_generation += 1;
    chase.recheck_ms = 0;
    source.motion = m::MotionState::restore(snapshot).unwrap();
    source.motion.speed_changed();
    let before = w.checkpoint().unwrap();
    let prior_calls = a
        .inner
        .queries
        .iter()
        .filter(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_))))
        .count();
    assert_eq!(
        w.step(w.state().tick + 1, None, &mut a),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(
        a.inner
            .queries
            .iter()
            .filter(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_))))
            .count(),
        prior_calls
    );
}
