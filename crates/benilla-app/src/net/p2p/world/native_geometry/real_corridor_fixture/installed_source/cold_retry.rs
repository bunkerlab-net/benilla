// SPDX-License-Identifier: GPL-3.0-or-later
//! Same-world rollback/retry of real cold Facts, without ledger promotion or ACK.
use super::super::super::{
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::{self as cold, ResidentColdCandidate},
};
use super::*;
use benilla_world_runtime::native::{AuthorityRequest, Facts, MotionFacts, MotionQuery, Query};

struct Attempt {
    context: AuthorityRequest,
    candidate: ResidentColdCandidate,
    facts: Facts,
    deadline: Instant,
    registry: (usize, benilla_native_nav::SourceMapQuerySlot, usize),
}

#[test]
fn resident_cold_real_route_rolls_back_and_retries_fresh_same_allocation() {
    runtime::with_route_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut attempts: Vec<Attempt> = vec![];
            let fresh_scene_observations = Cell::new(0);
            let world = runtime::retry_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    assert!(
                        attempts.len() < 2,
                        "only failed first attempt and its retry"
                    );
                    let dispatch = context.require_pathfinder_dispatch(&p).unwrap();
                    assert!(
                        dispatch.first_construction(),
                        "later dispatch remains closed"
                    );
                    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                        panic!("Route callback");
                    };
                    assert!(route.request.prior_cache.is_none());
                    if let Some(old) = attempts.first() {
                        // This callback begins only after the helper proved exact world and
                        // whole-journal rollback. Before any new source observations, inspect
                        // the existing registry and its checked original native allocation.
                        assert_eq!(provider.retained_constructor_count(), 1);
                        assert_eq!(
                            provider
                                .retained_constructor_for_test(&old.context)
                                .unwrap(),
                            old.registry
                        );
                        assert!(
                            Instant::now() < old.deadline,
                            "expiry cannot mask seal refusal"
                        );
                        provider
                            .joined_for(
                                old.candidate.dependency(),
                                &old.context,
                                old.deadline,
                                || false,
                            )
                            .expect(
                                "original opaque ancestry is still fresh for its original dispatch",
                            );
                        assert_eq!(
                            serde_json::to_vec(&old.context).unwrap(),
                            serde_json::to_vec(context).unwrap()
                        );
                        assert!(!old
                            .context
                            .require_pathfinder_dispatch(&p)
                            .unwrap()
                            .same_live_dispatch(dispatch));
                        assert!(
                            matches!(
                                provider.joined_for(
                                    old.candidate.dependency(),
                                    context,
                                    old.deadline,
                                    || false
                                ),
                                Err(ResidentFilterError::Context(Failure::Binding))
                            ),
                            "fresh live dispatch cannot consume old candidate evidence"
                        );
                        // The inspector checks RetainedSourceQuery::check_query, not byte/hash
                        // equality; the returned usize identifies the unchanged provider only.
                        assert_eq!(
                            provider.retained_constructor_for_test(context).unwrap(),
                            old.registry
                        );
                    } else {
                        assert_eq!(provider.retained_constructor_count(), 0);
                    }
                    let deadline = runtime::until();
                    let (constructor, water) =
                        runtime::prerequisites(provider, context, 101, deadline);
                    fixture.advance_physics();
                    let (candidate, facts) = cold::certify(
                        fixture.app.world_mut(),
                        compatibility,
                        provider,
                        context,
                        constructor,
                        water,
                        105,
                        deadline,
                        || false,
                        || fresh_scene_observations.set(fresh_scene_observations.get() + 1),
                    )
                    .unwrap();
                    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &facts else {
                        panic!("spatial");
                    };
                    let benilla_sim::creature_motion::Fact::RouteCalculation(calculation) =
                        &spatial.fact
                    else {
                        panic!("real calculation");
                    };
                    assert_eq!(
                        calculation.source,
                        benilla_sim::creature_motion::RouteSource::OwnerPathFinder
                    );
                    assert_eq!(calculation.after_cache.revision, 1);
                    assert_eq!(calculation.after_cache.include_flags, 13);
                    assert!(calculation.prior_cache.is_none());
                    if let Some(old) = attempts.first() {
                        assert_eq!(
                            facts, old.facts,
                            "fresh calculation agrees without reusing historical Facts"
                        );
                    }
                    let registry = provider.retained_constructor_for_test(context).unwrap();
                    assert_eq!(registry.2, 1, "retry never allocates a second constructor");
                    attempts.push(Attempt {
                        context: context.clone(),
                        candidate,
                        facts: facts.clone(),
                        deadline,
                        registry,
                    });
                    Ok(facts)
                },
            )
            .expect("one genuine later failure must roll back and retry the same world");
            assert_eq!(attempts.len(), 2);
            assert_eq!(fresh_scene_observations.get(), 2);
            assert_eq!(provider.retained_constructor_count(), 1);
            let recorded: Vec<_> = world
                .records()
                .iter()
                .flat_map(|record| &record.replies)
                .filter(|reply| {
                    matches!(
                        reply.request.query,
                        Query::SourceMotion(MotionQuery::Route(_))
                    )
                })
                .collect();
            assert_eq!(
                recorded.len(),
                1,
                "no duplicate committed Route from failed attempt"
            );
            assert_eq!(recorded[0].facts, attempts[1].facts);
            let live = recorded[0].request.require_pathfinder_dispatch(&p).unwrap();
            assert!(live
                .same_live_dispatch(attempts[1].context.require_pathfinder_dispatch(&p).unwrap()));
            assert!(!live
                .same_live_dispatch(attempts[0].context.require_pathfinder_dispatch(&p).unwrap()));
            // Both opaque objects are retained diagnostic, uncommitted candidates. This
            // successful NativeWorld tick is not a provider ledger/controller ACK.
            eprintln!(
                "resident real rollback/retry: tick={} attempts=2 fresh-scenes=2 constructors=1 recorded-routes=1",
                world.state().tick
            );
        },
    );
}
