// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed cold origin plus one owned warm terminal; only staging owns an old head.
use super::super::source_route_filter::resident::retained::{
    second_retained::ResidentJoinedSecondRetainedDependency, ResidentJoinedRetainedDependency,
};
use super::*;
use benilla_world_runtime::PlayerId;
use std::rc::Rc;

// Each head owns one bounded move-only box; no dependency owns a previous head.
pub(super) enum WarmDependency {
    First(Box<ResidentJoinedRetainedDependency>),
    Second(Box<ResidentJoinedSecondRetainedDependency>),
}
impl WarmDependency {
    pub(super) fn check_cache(&self, cache: &motion::PathFinderCache) -> Result<(), Failure> {
        match self {
            Self::First(dependency) => dependency.check_historical_native_cache_for_test(cache),
            Self::Second(dependency) => dependency.check_historical_native_cache_for_test(cache),
        }
        .map_err(|_| Failure::Binding)
    }
    pub(super) fn storage_bytes(&self) -> usize {
        match self {
            Self::First(dependency) => dependency.storage_footprint_for_test(),
            Self::Second(dependency) => dependency.storage_footprint_for_test(),
        }
    }
}
/// Never owns an earlier warm head, even for the first retained calculation.
pub(super) struct WarmHead {
    pub(super) origin: Rc<ResidentColdCandidate>,
    pub(super) dependency: WarmDependency,
    pub(super) installed_owner: benilla_formats::NativeInventorySource,
    pub(super) raw_revision: [u8; 32],
    pub(super) reply: AuthorityReply,
}
pub(super) enum WarmPredecessor {
    Cold(Rc<ResidentColdCandidate>),
    Warm(Rc<WarmHead>),
}
pub(super) struct StagedWarmProposal {
    pub(super) head: Rc<WarmHead>,
    pub(super) predecessor: WarmPredecessor,
}
impl StagedWarmProposal {
    pub(super) fn first(candidate: super::retained::ResidentRetainedCandidate) -> Self {
        let super::retained::ResidentRetainedCandidate {
            predecessor,
            dependency,
            installed_owner,
            raw_revision,
            reply,
        } = candidate;
        Self {
            head: Rc::new(WarmHead {
                origin: Rc::clone(&predecessor),
                dependency: WarmDependency::First(Box::new(dependency)),
                installed_owner,
                raw_revision,
                reply,
            }),
            predecessor: WarmPredecessor::Cold(predecessor),
        }
    }
}

/// Private to the concrete certifier/ledger module. The caller selects and checks
/// the committed predecessor before this synchronous attempt and before staging.
/// No reply getter, borrowed ECS proof or historical freshness escapes.
#[allow(clippy::too_many_arguments)]
pub(super) fn certify(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    predecessor: Rc<WarmHead>,
    context: &AuthorityRequest,
    player: PlayerId,
    first_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
) -> Result<(StagedWarmProposal, Facts), ResidentFilterError> {
    provider.require_two_retained_routes()?;
    let WarmDependency::First(prior) = &predecessor.dependency else {
        return Err(Failure::UnsupportedQuery.into());
    };
    let segment_ticket = first_ticket.checked_add(4).ok_or(Failure::Invalid)?;
    let mut state = SystemState::<(
        Option<Res<NativeStaticMapIndex>>,
        NativeTerrainCoverage,
        WorldCollision,
        SpatialQuery,
    )>::new(world);
    let (installed, terrain, collision, spatial) = state.get(world);
    let owner = installed
        .as_ref()
        .and_then(|index| index.source_provenance())
        .ok_or(Failure::Binding)?;
    if !owner.same_owner(&predecessor.installed_owner) {
        return Err(Failure::Binding.into());
    }
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let dependency = provider.observe_joined_second_retained(
        &predecessor.origin.dependency,
        prior,
        context,
        player,
        first_ticket,
        deadline,
        &cancelled,
    )?;
    let deadline = dependency.deadline(deadline);
    let observation = provider.second_retained_for(
        &dependency,
        &predecessor.origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    let [from, to] = observation
        .terminal
        .points
        .map(|point| point.map(f32::from_bits));
    let map = compatibility.source_binding().0.map;
    let coverage = terrain
        .prove(map, from, to, observation.body.radius.as_f32())
        .map_err(|_| Failure::MissingGeometry)?;
    if coverage.map != map || coverage.inventory_digest != owner.inventory_digest() {
        return Err(Failure::Binding.into());
    }
    let raw = core::geometry_revision(
        coverage.map,
        coverage.wdt_source_digest,
        &coverage.tiles,
        &coverage.models,
        coverage.inventory_digest,
    );
    if raw != predecessor.raw_revision {
        return Err(Failure::Binding.into());
    }
    LocalCorridor {
        terrain: &terrain,
        collision: &collision,
        spatial: &spatial,
        map,
        revision: raw,
    }
    .certify(from, to, observation.body)?;
    provider.second_retained_segment_los(
        &dependency,
        &predecessor.origin.dependency,
        context,
        segment_ticket,
        deadline,
        &cancelled,
    )?;
    // Test-only source/cancel/deadline hook, still inside this one immutable scene borrow.
    // It cannot mutate the borrowed ECS scene or supply a clearance result.
    after();
    provider.second_retained_for(
        &dependency,
        &predecessor.origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let cache = provider.second_retained_cache(
        &dependency,
        &predecessor.origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    let mut identity = Sha256::new();
    identity.update(b"benilla-resident-installed-two-retained-chase-route-v1");
    identity.update(context.binding.world);
    identity.update(context.binding.profile);
    identity.update(context.binding.geometry);
    identity.update(raw);
    identity.update(route.owner.npc().generation.to_le_bytes());
    identity.update(route.request.binding.query_id.to_le_bytes());
    identity.update(route.request.binding.now_ms.to_le_bytes());
    identity.update(cache.revision.to_le_bytes());
    for point in observation.terminal.points {
        for value in point {
            identity.update(value.to_le_bytes());
        }
    }
    for poly in &cache.poly_refs {
        identity.update(poly.to_le_bytes());
    }
    let calculation = motion::RouteCalculation {
        source: motion::RouteSource::OwnerPathFinder,
        outcome: motion::RouteOutcome::Complete(motion::CompleteRoute {
            source: motion::RouteSource::OwnerPathFinder,
            path_identity: identity.finalize().into(),
            points: vec![route.request.origin, cache.requested_end],
            segments: vec![SegmentChecks {
                loaded_coverage: Check::Passed,
                line_of_sight: Check::Passed,
                body_sweep: Check::Passed,
                continuous_ground_support: Check::Passed,
                dry_ground: Check::Passed,
            }],
        }),
        wander_no_path: None,
        prior_cache: route.request.prior_cache.clone(),
        after_cache: cache,
    };
    calculation
        .validate_for(&route.request)
        .map_err(|_| Failure::Binding)?;
    provider.second_retained_for(
        &dependency,
        &predecessor.origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    provider.verify_installed_owner(compatibility, owner, deadline, cancelled)?;
    let facts = Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
        request: motion::Query::Route(route.request.clone()),
        fact: motion::Fact::RouteCalculation(Box::new(calculation)),
    })));
    Ok((
        StagedWarmProposal {
            head: Rc::new(WarmHead {
                origin: Rc::clone(&predecessor.origin),
                dependency: WarmDependency::Second(Box::new(dependency)),
                installed_owner: owner.clone(),
                raw_revision: raw,
                reply: AuthorityReply {
                    request: context.clone(),
                    facts: facts.clone(),
                },
            }),
            predecessor: WarmPredecessor::Warm(predecessor),
        },
        facts,
    ))
}
