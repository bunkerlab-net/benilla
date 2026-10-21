// SPDX-License-Identifier: GPL-3.0-or-later
//! Synchronous resident cold Chase conversion. Only the concrete installed scene
//! can certify an uncommitted historical candidate. The component-only ledger
//! promotes it solely against an ordered actual controller commit notice.
use super::{
    core::{self, Corridor, Failure},
    local_corridor::LocalCorridor,
    products::source_height::inventory::InventoryCompatibility,
    source_route_filter::resident::{
        chase_los::water::ResidentChaseWaterDependency, joined::ResidentJoinedColdDependency,
        ResidentFilterDependency, ResidentFilterError, ResidentFilterProvider,
    },
};
use avian3d::prelude::SpatialQuery;
use benilla_sim::{
    creature_ai::movement::{Check, SegmentChecks},
    creature_motion as motion,
};
use benilla_world::{
    collision::WorldCollision,
    terrain_stream::{NativeStaticMapIndex, NativeTerrainCoverage},
};
use benilla_world_runtime::native::{
    AuthorityReply, AuthorityRequest, Facts, MotionFacts, MotionQuery, Query,
};
use bevy::{
    ecs::system::SystemState,
    prelude::{Res, World},
};
use sha2::{Digest, Sha256};
use std::time::Instant;

pub(super) struct ResidentColdCandidate {
    dependency: ResidentJoinedColdDependency,
    installed_owner: benilla_formats::NativeInventorySource,
    raw_revision: [u8; 32],
    reply: AuthorityReply,
}
impl ResidentColdCandidate {
    #[cfg(test)]
    pub(super) fn dependency(&self) -> &ResidentJoinedColdDependency {
        &self.dependency
    }
}

/// All geometry inputs are borrowed in the same immutable ECS observation. There
/// is no generic Corridor adapter and no constructor from copied observations.
#[allow(clippy::too_many_arguments)]
pub(super) fn certify(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
    constructor: ResidentFilterDependency,
    water: ResidentChaseWaterDependency,
    ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    #[cfg(test)] after: impl FnOnce(),
) -> Result<(ResidentColdCandidate, Facts), ResidentFilterError> {
    provider.require_first_cold_route()?;
    // Callers advance physics on this World first. Every resource below comes
    // from the same immutable ECS observation, never a detached equal-byte index.
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
    let segment_ticket = ticket.checked_add(2).ok_or(Failure::Invalid)?;
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let dependency = provider.observe_joined_cold(
        constructor,
        water,
        context,
        ticket,
        Default::default(),
        deadline,
        &cancelled,
    )?;
    let deadline = dependency.deadline(deadline);
    let observation = provider.joined_for(&dependency, context, deadline, &cancelled)?;
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    let [from, to] = observation.terminal.points.map(|p| p.map(f32::from_bits));
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
    LocalCorridor {
        terrain: &terrain,
        collision: &collision,
        spatial: &spatial,
        map,
        revision: raw,
    }
    .certify(from, to, observation.body)?;
    provider.joined_segment_los(&dependency, context, segment_ticket, deadline, &cancelled)?;
    #[cfg(test)]
    after();
    provider.joined_for(&dependency, context, deadline, &cancelled)?;
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let cache = provider.joined_cache(&dependency, context, deadline, &cancelled)?;
    let mut identity = Sha256::new();
    identity.update(b"benilla-resident-installed-first-cold-chase-route-v1");
    identity.update(context.binding.world);
    identity.update(context.binding.profile);
    identity.update(context.binding.geometry);
    identity.update(raw);
    identity.update(route.owner.npc().generation.to_le_bytes());
    identity.update(route.request.binding.query_id.to_le_bytes());
    identity.update(route.request.binding.now_ms.to_le_bytes());
    for p in observation.terminal.points {
        for v in p {
            identity.update(v.to_le_bytes());
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
        prior_cache: None,
        after_cache: cache,
    };
    calculation
        .validate_for(&route.request)
        .map_err(|_| Failure::Binding)?;
    // Final source checks follow cache construction too; no historical ACK is done here.
    provider.joined_for(&dependency, context, deadline, &cancelled)?;
    provider.verify_installed_owner(compatibility, owner, deadline, cancelled)?;
    let facts = Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
        request: motion::Query::Route(route.request.clone()),
        fact: motion::Fact::RouteCalculation(Box::new(calculation)),
    })));
    // The sole emission is made under this fresh borrow. The retained historical
    // reply is private and cannot authorize a delayed or repeated clearance.
    Ok((
        ResidentColdCandidate {
            dependency,
            installed_owner: owner.clone(),
            raw_revision: raw,
            reply: AuthorityReply {
                request: context.clone(),
                facts: facts.clone(),
            },
        },
        facts,
    ))
}

#[cfg(test)]
#[allow(dead_code, reason = "app-only resident controller component fixture")]
#[path = "source_route_resident_local/ledger.rs"]
pub(super) mod ledger;
