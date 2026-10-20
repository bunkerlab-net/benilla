// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded real first-cold certification probe, separate from observation-only joins.
use super::super::super::{
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
};
use super::*;
use benilla_world_runtime::native::AuthorityRequest;

impl InstalledFixture {
    #[allow(clippy::too_many_arguments)]
    fn certify_cold(
        &mut self,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        ticket: u64,
        deadline: Instant,
    ) -> Result<
        (
            super::super::super::source_route_resident_local::ResidentColdCandidate,
            benilla_world_runtime::native::Facts,
        ),
        ResidentFilterError,
    > {
        let (constructor, water) = runtime::prerequisites(provider, context, ticket, deadline);
        self.advance_physics();
        super::super::super::source_route_resident_local::certify(
            self.app.world_mut(),
            compatibility,
            provider,
            context,
            constructor,
            water,
            ticket + 4,
            deadline,
            || false,
            || {},
        )
    }
}

#[test]
fn resident_first_cold_real_route_feasibility() {
    runtime::with_route_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut candidates = vec![];
            let result = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let dispatch = context.require_pathfinder_dispatch(&p).unwrap();
                    eprintln!(
                        "resident first cold probe: tick={} first={} prior_candidates={}",
                        context.binding.tick,
                        dispatch.first_construction(),
                        candidates.len()
                    );
                    if !dispatch.first_construction() {
                        return Err(
                            benilla_world_runtime::native::NativeError::AuthorityUnavailable(
                                "resident subsequent dispatch remains unsupported",
                            ),
                        );
                    }
                    let (candidate, reply) = fixture
                        .certify_cold(compatibility, provider, context, 101, runtime::until())
                        .unwrap();
                    candidates.push(candidate);
                    Ok(reply)
                },
            );
            assert_eq!(candidates.len(), 1);
            let world =
                result.expect("one genuine first cold calculation must finish its outer tick");
            let routes: Vec<_> = world
                .last_tick()
                .unwrap()
                .replies
                .iter()
                .filter(|reply| {
                    matches!(
                        reply.request.query,
                        benilla_world_runtime::native::Query::SourceMotion(
                            benilla_world_runtime::native::MotionQuery::Route(_)
                        )
                    )
                })
                .collect();
            assert_eq!(routes.len(), 1);
            let benilla_world_runtime::native::Facts::SourceMotion(
                benilla_world_runtime::native::MotionFacts::Spatial(reply),
            ) = &routes[0].facts
            else {
                panic!("spatial");
            };
            let benilla_sim::creature_motion::Fact::RouteCalculation(calculation) = &reply.fact
            else {
                panic!("calculation");
            };
            assert_eq!(
                calculation.source,
                benilla_sim::creature_motion::RouteSource::OwnerPathFinder
            );
            assert!(calculation.prior_cache.is_none());
            assert_eq!(calculation.after_cache.revision, 1);
            assert_eq!(calculation.after_cache.active_len, 1);
            assert_eq!(calculation.after_cache.path_type, 1);
            assert_eq!(calculation.after_cache.include_flags, 13);
            assert_eq!(calculation.after_cache.exclude_flags, 0);
            assert_eq!(
                calculation.after_cache.poly_refs.len(),
                usize::from(benilla_sim::creature_motion::PATHFINDER_CACHE_POINT_LIMIT)
            );
            eprintln!(
                "resident cold committed runtime tick={}",
                world.state().tick
            );
        },
    );
}
