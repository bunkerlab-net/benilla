// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact native straight segments, certified in one immutable installed scene observation.
use super::super::products::vmap::QueryBudget;
use super::super::source_route_filter::resident::retained::straight_retained::{
    MAX_SOURCE_REVISION, STRAIGHT_MAX_POINTS,
};
use super::three_epoch::{StagedWarmProposal, WarmDependency, WarmHead, WarmPredecessor};
use super::*;
use benilla_sim::creature_ai::Point;
use benilla_world_runtime::PlayerId;
use std::rc::Rc;

#[path = "straight_loop/detail_diagnostic.rs"]
mod detail_diagnostic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum StraightSegmentPhase {
    Corridor,
    SourceLos,
}

/// Hooks observe completed checks only; they cannot supply clearance or mutate this scene.
#[allow(clippy::too_many_arguments)]
pub(super) fn certify(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    predecessor: WarmPredecessor,
    context: &AuthorityRequest,
    player: PlayerId,
    first_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    mut on_segment: impl FnMut(StraightSegmentPhase, usize),
    after: impl FnOnce(),
) -> Result<(StagedWarmProposal, Facts), ResidentFilterError> {
    provider.require_finite_straight_routes()?;
    let prior = predecessor.current_straight_native()?;
    if !(1..MAX_SOURCE_REVISION).contains(&prior.source_revision()) {
        return Err(Failure::UnsupportedQuery.into());
    }
    // Reserve the complete ticket interval before source work or a scene borrow.
    let first_segment_ticket = first_ticket.checked_add(4).ok_or(Failure::Invalid)?;
    first_segment_ticket
        .checked_add((STRAIGHT_MAX_POINTS - 2) as u64)
        .ok_or(Failure::Invalid)?;
    let origin = Rc::clone(predecessor.origin());
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
    if !owner.same_owner(&origin.installed_owner) {
        return Err(Failure::Binding.into());
    }
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let dependency = provider.observe_joined_straight(
        &origin.dependency,
        &prior,
        context,
        player,
        first_ticket,
        deadline,
        &cancelled,
    )?;
    let deadline = dependency.deadline(deadline);
    let observation = provider.straight_for(
        &dependency,
        &origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    if !(2..=STRAIGHT_MAX_POINTS).contains(&observation.points.len()) {
        return Err(Failure::UnsupportedQuery.into());
    }
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    let mut points = Vec::new();
    let mut segments = Vec::new();
    points
        .try_reserve_exact(observation.points.len())
        .map_err(|_| Failure::QueueCapacity)?;
    segments
        .try_reserve_exact(observation.points.len() - 1)
        .map_err(|_| Failure::QueueCapacity)?;
    for bits in observation.points {
        let [x, y, z] = bits.map(f32::from_bits);
        let point = Point::new(x, y, z).map_err(|_| Failure::Invalid)?;
        // No rounding, signed-zero repair or replacement of native interior samples.
        if [point.x.bits(), point.y.bits(), point.z.bits()] != *bits {
            return Err(Failure::UnsupportedQuery.into());
        }
        points.push(point);
    }
    let map = compatibility.source_binding().0.map;
    let raw = origin.raw_revision;
    // The whole polyline shares one source-ray ceiling, never one allowance per segment.
    let mut segment_budget = QueryBudget::new(deadline);
    for (index, pair) in observation.points.windows(2).enumerate() {
        provider.straight_for(
            &dependency,
            &origin.dependency,
            context,
            deadline,
            &cancelled,
        )?;
        provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
        let from = pair[0].map(f32::from_bits);
        let to = pair[1].map(f32::from_bits);
        let ticket = first_segment_ticket
            .checked_add(u64::try_from(index).map_err(|_| Failure::Invalid)?)
            .ok_or(Failure::Invalid)?;
        let coverage = terrain
            .prove(map, from, to, observation.body.radius.as_f32())
            .map_err(|error| {
                eprintln!(
                    "resident straight coverage refusal: tick={} query={} prior-revision={} map={map} segment={index}/{} error={error:?} body={:?} native-point-bits={:?}",
                    context.binding.tick,
                    route.request.binding.query_id,
                    prior.source_revision(),
                    observation.points.len() - 1,
                    observation.body,
                    observation.points,
                );
                // Diagnostic only: inspect THIS failed terminal and exact loaded
                // query, never calculate another route or alter the refusal.
                match detail_diagnostic::inspect(
                    &terrain,
                    compatibility,
                    owner,
                    provider,
                    &dependency,
                    &origin,
                    context,
                    observation.points,
                    observation.body,
                    deadline,
                    &cancelled,
                ) {
                    Ok(report) => eprintln!("{report}"),
                    Err(detail_error) => eprintln!(
                        "resident straight detail diagnostic unavailable: tick={} query={} error={detail_error:?}; original coverage refusal unchanged",
                        context.binding.tick, route.request.binding.query_id,
                    ),
                }
                Failure::MissingGeometry
            })?;
        if coverage.map != map
            || coverage.inventory_digest != owner.inventory_digest()
            || core::geometry_revision(
                coverage.map,
                coverage.wdt_source_digest,
                &coverage.tiles,
                &coverage.models,
                coverage.inventory_digest,
            ) != raw
        {
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
        on_segment(StraightSegmentPhase::Corridor, index);
        provider.straight_segment_los(
            &dependency,
            &origin.dependency,
            context,
            index,
            ticket,
            &mut segment_budget,
            deadline,
            &cancelled,
        )?;
        on_segment(StraightSegmentPhase::SourceLos, index);
        segments.push(SegmentChecks {
            loaded_coverage: Check::Passed,
            line_of_sight: Check::Passed,
            body_sweep: Check::Passed,
            continuous_ground_support: Check::Passed,
            dry_ground: Check::Passed,
        });
    }
    after();
    provider.straight_for(
        &dependency,
        &origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let cache = provider.straight_cache_for(
        &dependency,
        &origin.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    let mut identity = Sha256::new();
    identity.update(b"benilla-resident-installed-finite-straight-chase-route-v1");
    identity.update(context.binding.world);
    identity.update(context.binding.profile);
    identity.update(context.binding.geometry);
    identity.update(raw);
    identity.update(route.owner.npc().generation.to_le_bytes());
    identity.update(route.request.binding.query_id.to_le_bytes());
    identity.update(route.request.binding.now_ms.to_le_bytes());
    identity.update(cache.revision.to_le_bytes());
    identity.update((observation.points.len() as u64).to_le_bytes());
    for point in observation.points {
        for value in point {
            identity.update(value.to_le_bytes());
        }
    }
    identity.update((segments.len() as u64).to_le_bytes());
    identity.update((cache.poly_refs.len() as u64).to_le_bytes());
    identity.update(cache.active_len.to_le_bytes());
    identity.update(cache.path_type.to_le_bytes());
    for poly in &cache.poly_refs {
        identity.update(poly.to_le_bytes());
    }
    let calculation = motion::RouteCalculation {
        source: motion::RouteSource::OwnerPathFinder,
        outcome: motion::RouteOutcome::Complete(motion::CompleteRoute {
            source: motion::RouteSource::OwnerPathFinder,
            path_identity: identity.finalize().into(),
            points,
            segments,
        }),
        wander_no_path: None,
        prior_cache: route.request.prior_cache.clone(),
        after_cache: cache,
    };
    calculation
        .validate_for(&route.request)
        .map_err(|_| Failure::Binding)?;
    provider.straight_for(
        &dependency,
        &origin.dependency,
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
                origin,
                dependency: WarmDependency::Straight(Box::new(dependency)),
                installed_owner: owner.clone(),
                raw_revision: raw,
                reply: AuthorityReply {
                    request: context.clone(),
                    facts: facts.clone(),
                },
            }),
            predecessor,
        },
        facts,
    ))
}
