// SPDX-License-Identifier: GPL-3.0-or-later
//! One fresh retained calculation, rooted only by the private ledger's cold ACK.
use super::super::source_route_filter::resident::retained::ResidentJoinedRetainedDependency;
use super::*;
use benilla_native_nav::SourceSamePolygonBudget;
use benilla_world_runtime::PlayerId;
use std::rc::Rc;

pub(super) struct ResidentRetainedCandidate {
    pub(super) predecessor: Rc<ResidentColdCandidate>,
    pub(super) dependency: ResidentJoinedRetainedDependency,
    pub(super) installed_owner: benilla_formats::NativeInventorySource,
    pub(super) raw_revision: [u8; 32],
    pub(super) reply: AuthorityReply,
}

/// Private to the concrete certifier/ledger module. The caller selects and checks
/// the committed predecessor before this synchronous attempt and before staging.
/// No reply getter, borrowed ECS proof or historical freshness escapes.
#[allow(clippy::too_many_arguments)]
pub(super) fn certify(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    predecessor: Rc<ResidentColdCandidate>,
    context: &AuthorityRequest,
    player: PlayerId,
    first_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
) -> Result<(ResidentRetainedCandidate, Facts), ResidentFilterError> {
    provider.require_one_retained_route()?;
    certify_inner(
        world,
        compatibility,
        provider,
        predecessor,
        context,
        player,
        first_ticket,
        deadline,
        cancelled,
        after,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn certify_two_owner(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    predecessor: Rc<ResidentColdCandidate>,
    context: &AuthorityRequest,
    player: PlayerId,
    first_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
) -> Result<(ResidentRetainedCandidate, Facts), ResidentFilterError> {
    provider.require_two_owner_first_retained()?;
    certify_inner(
        world,
        compatibility,
        provider,
        predecessor,
        context,
        player,
        first_ticket,
        deadline,
        cancelled,
        after,
    )
}
#[allow(clippy::too_many_arguments)]
fn certify_inner(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    predecessor: Rc<ResidentColdCandidate>,
    context: &AuthorityRequest,
    player: PlayerId,
    first_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
) -> Result<(ResidentRetainedCandidate, Facts), ResidentFilterError> {
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
    let dependency = if provider.require_two_owner_first_retained().is_ok() {
        provider.observe_two_owner_first_retained(
            &predecessor.dependency,
            context,
            player,
            first_ticket,
            SourceSamePolygonBudget {
                // First retained lookup from full74+1 backing: per endpoint one
                // active slot, one searchable tail and at most two nearest queries.
                lookup_primitives: 8,
                terminal_primitives: 1,
            },
            deadline,
            &cancelled,
        )?
    } else {
        provider.observe_joined_retained(
            &predecessor.dependency,
            context,
            player,
            first_ticket,
            SourceSamePolygonBudget {
                // First retained lookup from full74+1 backing: per endpoint one
                // active slot, one searchable tail and at most two nearest queries.
                lookup_primitives: 8,
                terminal_primitives: 1,
            },
            deadline,
            &cancelled,
        )?
    };
    finish_scene(
        predecessor,
        dependency,
        compatibility,
        provider,
        context,
        segment_ticket,
        deadline,
        cancelled,
        after,
        owner,
        &terrain,
        &collision,
        &spatial,
    )
}

/// Owned native result only: no scene clearance, ECS borrow, or staged Facts.
pub(in super::super) struct PreparedRetained {
    pub(super) predecessor: Rc<ResidentColdCandidate>,
    dependency: ResidentJoinedRetainedDependency,
    installed_owner: benilla_formats::NativeInventorySource,
    first_ticket: u64,
    deadline: Instant,
}
impl PreparedRetained {
    pub(in super::super) fn deadline(&self) -> Instant {
        self.deadline
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    predecessor: Rc<ResidentColdCandidate>,
    context: &AuthorityRequest,
    player: PlayerId,
    first_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<PreparedRetained, ResidentFilterError> {
    provider.require_one_retained_route()?;
    first_ticket.checked_add(4).ok_or(Failure::Invalid)?;
    let mut state = SystemState::<Option<Res<NativeStaticMapIndex>>>::new(world);
    let installed = state.get(world);
    let owner = installed
        .as_ref()
        .and_then(|index| index.source_provenance())
        .ok_or(Failure::Binding)?;
    if !owner.same_owner(&predecessor.installed_owner) {
        return Err(Failure::Binding.into());
    }
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let dependency = if provider.require_two_owner_first_retained().is_ok() {
        provider.observe_two_owner_first_retained(
            &predecessor.dependency,
            context,
            player,
            first_ticket,
            SourceSamePolygonBudget {
                // First retained lookup from full74+1 backing: per endpoint one
                // active slot, one searchable tail and at most two nearest queries.
                lookup_primitives: 8,
                terminal_primitives: 1,
            },
            deadline,
            &cancelled,
        )?
    } else {
        provider.observe_joined_retained(
            &predecessor.dependency,
            context,
            player,
            first_ticket,
            SourceSamePolygonBudget {
                // First retained lookup from full74+1 backing: per endpoint one
                // active slot, one searchable tail and at most two nearest queries.
                lookup_primitives: 8,
                terminal_primitives: 1,
            },
            deadline,
            &cancelled,
        )?
    };
    Ok(PreparedRetained {
        deadline: dependency.deadline(deadline),
        dependency,
        predecessor,
        installed_owner: owner.clone(),
        first_ticket,
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    prepared: PreparedRetained,
    world: &mut World,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
) -> Result<(ResidentRetainedCandidate, Facts), ResidentFilterError> {
    let PreparedRetained {
        predecessor,
        dependency,
        installed_owner,
        first_ticket,
        deadline: original_deadline,
    } = prepared;
    let deadline = deadline.min(original_deadline);
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
    if !owner.same_owner(&installed_owner) || !owner.same_owner(&predecessor.installed_owner) {
        return Err(Failure::Binding.into());
    }
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    finish_scene(
        predecessor,
        dependency,
        compatibility,
        provider,
        context,
        segment_ticket,
        deadline,
        cancelled,
        after,
        owner,
        &terrain,
        &collision,
        &spatial,
    )
}

/// One immutable final scene island shared by synchronous and resumed callers.
#[allow(clippy::too_many_arguments)]
fn finish_scene<'w, 's>(
    predecessor: Rc<ResidentColdCandidate>,
    dependency: ResidentJoinedRetainedDependency,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
    segment_ticket: u64,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
    owner: &benilla_formats::NativeInventorySource,
    terrain: &NativeTerrainCoverage<'w, 's>,
    collision: &WorldCollision<'w, 's>,
    spatial: &SpatialQuery<'w, 's>,
) -> Result<(ResidentRetainedCandidate, Facts), ResidentFilterError> {
    let deadline = dependency.deadline(deadline);
    let observation = provider.retained_for(
        &dependency,
        &predecessor.dependency,
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
        terrain,
        collision,
        spatial,
        map,
        revision: raw,
    }
    .certify(from, to, observation.body)?;
    provider.retained_segment_los(
        &dependency,
        &predecessor.dependency,
        context,
        segment_ticket,
        deadline,
        &cancelled,
    )?;
    // Test-only source/cancel/deadline hook, still inside this one immutable scene borrow.
    // It cannot mutate the borrowed ECS scene or supply a clearance result.
    after();
    provider.retained_for(
        &dependency,
        &predecessor.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    provider.verify_installed_owner(compatibility, owner, deadline, &cancelled)?;
    let cache = provider.retained_cache(
        &dependency,
        &predecessor.dependency,
        context,
        deadline,
        &cancelled,
    )?;
    let mut identity = Sha256::new();
    if provider.require_two_owner_first_retained().is_ok() {
        identity.update(b"benilla-resident-installed-two-owner-first-retained-chase-route-v1");
        identity.update(route.owner.npc().spawn.to_le_bytes());
    } else {
        identity.update(b"benilla-resident-installed-one-retained-chase-route-v1");
    }
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
    provider.retained_for(
        &dependency,
        &predecessor.dependency,
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
        ResidentRetainedCandidate {
            predecessor,
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
