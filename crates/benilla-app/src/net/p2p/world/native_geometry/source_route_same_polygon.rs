// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-ordered dry Chase calculation on the actual retained query. A terminal
//! proposal is not a Route: only the separate concrete LocalCorridor resolver can
//! certify it. Generic/headless support adapters cannot perform that conversion.
use super::{
    core::{Failure, PlayerEnvironmentPolicy, Session},
    products::{
        ground_height::{MapHeightSnapshot, SnapshotBinding as HeightBinding},
        liquid::SnapshotBinding as LiquidBinding,
        navigation::SnapshotBinding as NavigationBinding,
        source_query::SourceQueryProjection,
        terrain_wet_liquid::TerrainLiquidSnapshot,
        vmap::{RayReply, RayRequest, SnapshotBinding as VmapBinding, StaticRay, VmapSnapshot},
    },
    source_near_point::{SourceNearPointConfiguration, SourceNearPointProvider},
    source_pathfinder_query::{ConstructorQueryError, SourcePathfinderQueryProvider},
    source_player_environment::SourcePlayerEnvironmentProvider,
    source_route_chase_gate::{
        gate_model, ChaseGateModel, ChaseGateObservations, ChaseGateRejection, ChaseGateSelection,
        LoadedChaseGate, Observation as GateObservation,
    },
    source_route_filter::{
        RouteFilterDependency, RouteFilterRequest, RouteFilterUpdateDependency,
        SourceRouteFilterProvider,
    },
};
use benilla_native_nav::{
    LoadedSourceSamePolygon, LoadedSourceTilePresence, SourceOwnerFilter, SourceSamePolygonBudget,
    SourceSamePolygonConfig, SourceSamePolygonError, SOURCE_COLD_DISTANCE_COMPATIBILITY_ID,
    SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION, SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
    SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
    SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
};
use benilla_sim::{chase_gate, creature_ai::Point, creature_motion as motion};
use benilla_world_runtime::{
    native::{
        AuthorityRequest, Facts, MotionQuery, OwnerIgnorePathfindingReceipt,
        PathfinderDispatchReceipt, Profile, Query, CREATURE_COLLISION_HEIGHT_POLICY,
        OWNER_IGNORE_PATHFINDING_POLICY, PLAYER_COLLISION_HEIGHT_POLICY, PLAYER_ENVIRONMENT_POLICY,
    },
    NpcId, PlayerId,
};
use sha2::{Digest, Sha256};
use std::{rc::Rc, time::Instant};

const MAX_GENERATORS: usize = 256;
const MAX_CANDIDATES_PER_GENERATOR: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SamePolygonConfiguration {
    pub normalization: SourceSamePolygonConfig,
    pub line_of_sight_enabled: bool,
    pub cold_budget: SourceSamePolygonBudget,
    pub retained_budget: SourceSamePolygonBudget,
}
impl SamePolygonConfiguration {
    fn validate(self) -> Result<(), Failure> {
        if self.normalization.normalize_z
            || self.normalization.ignore_normalization
            || !self.line_of_sight_enabled
        {
            return Err(Failure::UnsupportedQuery);
        }
        if self.cold_budget.lookup_primitives > SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES
            || self.retained_budget.lookup_primitives
                > SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES
            || self.cold_budget.terminal_primitives != 1
            || self.retained_budget.terminal_primitives != 1
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SamePolygonBinding {
    pub raw_geometry: [u8; 32],
    pub liquid: LiquidBinding,
    pub navigation: NavigationBinding,
    pub vmap: VmapBinding,
    pub selected_configuration: [u8; 32],
    pub configuration: SamePolygonConfiguration,
    near_point: Option<SamePolygonNearPoint>,
    environment_policy: Option<[u8; 32]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SamePolygonNearPoint {
    height: HeightBinding,
    revision: [u8; 32],
}
impl SamePolygonBinding {
    /// Bind actual shared loaded products, not relabeled legacy512 geometry.
    pub(crate) fn from_projection(
        raw_geometry: [u8; 32],
        projection: &SourceQueryProjection,
        configuration: SamePolygonConfiguration,
    ) -> Result<Self, Failure> {
        let binding = Self {
            raw_geometry,
            liquid: projection.liquid().binding(),
            navigation: projection.navigation().binding(),
            vmap: projection.static_vmap().binding(),
            selected_configuration: projection.navigation().configuration_binding(),
            configuration,
            near_point: None,
            environment_policy: None,
        };
        binding.geometry_revision()?;
        Ok(binding)
    }
    fn with_near_point(
        mut self,
        snapshot: &MapHeightSnapshot<'_>,
        configuration: SourceNearPointConfiguration,
    ) -> Result<Self, Failure> {
        let height = snapshot.binding();
        if snapshot.static_vmap().binding() != self.vmap
            || height.map != self.vmap.map
            || height.content != self.vmap.content
            || height.inventory != self.vmap.inventory
            || height.source_epoch != self.vmap.source_epoch
        {
            return Err(Failure::Binding);
        }
        self.near_point = Some(SamePolygonNearPoint {
            height,
            revision: super::source_near_point::geometry_revision(snapshot, configuration)?,
        });
        Ok(self)
    }
    pub(super) fn check_near_point(
        self,
        snapshot: &MapHeightSnapshot<'_>,
        configuration: SourceNearPointConfiguration,
    ) -> Result<(), Failure> {
        if self.near_point.is_none() || self.with_near_point(snapshot, configuration)? != self {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    pub(crate) fn geometry_revision(self) -> Result<[u8; 32], Failure> {
        self.configuration.validate()?;
        let a = self.liquid;
        let b = self.navigation;
        let c = self.vmap;
        if self.raw_geometry == [0; 32]
            || self.selected_configuration == [0; 32]
            || a.map != b.map
            || a.map != c.map
            || a.content != b.content
            || a.content != c.content
            || a.source_epoch != b.source_epoch
            || a.source_epoch != c.source_epoch
            || a.inventory != b.inventory
            || a.inventory != c.inventory
            || a.source_epoch == [0; 32]
        {
            return Err(Failure::Binding);
        }
        let mut hash = Sha256::new();
        hash.update(b"benilla-owned-dry-same-polygon-chase-proposal-v1");
        hash.update(self.raw_geometry);
        hash.update(super::source_route_filter::geometry_revision(a));
        hash.update(super::source_pathfinder_query::geometry_revision(b));
        hash.update(c.policy);
        hash.update(self.selected_configuration);
        for policy in [
            chase_gate::POLICY,
            CREATURE_COLLISION_HEIGHT_POLICY,
            PLAYER_COLLISION_HEIGHT_POLICY,
            PLAYER_ENVIRONMENT_POLICY,
            OWNER_IGNORE_PATHFINDING_POLICY,
            SOURCE_COLD_DISTANCE_COMPATIBILITY_ID,
            SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION,
        ] {
            hash.update((policy.len() as u64).to_le_bytes());
            hash.update(policy.as_bytes());
        }
        for source in chase_gate::SOURCE_SHA256 {
            hash.update(source.as_bytes());
        }
        hash.update(SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION.to_le_bytes());
        hash.update([
            u8::from(self.configuration.normalization.normalize_z),
            u8::from(self.configuration.normalization.ignore_normalization),
            u8::from(self.configuration.line_of_sight_enabled),
        ]);
        for budget in [
            self.configuration.cold_budget,
            self.configuration.retained_budget,
        ] {
            hash.update((budget.lookup_primitives as u64).to_le_bytes());
            hash.update((budget.terminal_primitives as u64).to_le_bytes());
        }
        hash.update(b"ordinary-zero-offset-chase;straight-true;force-false;exact-two-point-endpoints;uncommitted");
        if let Some(near_point) = self.near_point {
            hash.update(b"genuine-primary-near-point");
            hash.update(near_point.revision);
        }
        if let Some(policy) = self.environment_policy {
            hash.update(b"loaded-player-environment");
            hash.update(policy);
        }
        Ok(hash.finalize().into())
    }
}

/// Loaded inputs for a single explicitly selected composition. Their actual
/// shared inventory/epoch and component policy revisions determine one identity.
pub(crate) struct SamePolygonProducts {
    pub projection: SourceQueryProjection,
    pub environment: TerrainLiquidSnapshot<'static>,
    pub height: MapHeightSnapshot<'static>,
    pub near_configuration: SourceNearPointConfiguration,
    pub raw_geometry: [u8; 32],
    pub configuration: SamePolygonConfiguration,
}
pub(super) struct SamePolygonProviders {
    pub route: SourceSamePolygonRouteProvider,
    pub environment: SourcePlayerEnvironmentProvider,
    pub near_point: SourceNearPointProvider,
}
impl SamePolygonProducts {
    pub(crate) fn binding(&self) -> Result<SamePolygonBinding, Failure> {
        let mut binding = SamePolygonBinding::from_projection(
            self.raw_geometry,
            &self.projection,
            self.configuration,
        )?
        .with_near_point(&self.height, self.near_configuration)?;
        let environment = self.environment.binding();
        if environment.map != binding.vmap.map
            || environment.content != binding.vmap.content
            || environment.inventory != binding.vmap.inventory
            || environment.source_epoch != binding.vmap.source_epoch
        {
            return Err(Failure::Binding);
        }
        binding.environment_policy = Some(environment.policy);
        Ok(binding)
    }
    pub(super) fn bind(
        self,
        session: &Session,
        profile: &Profile,
        player: PlayerId,
    ) -> Result<SamePolygonProviders, SamePolygonError> {
        let binding = self.binding()?;
        Ok(SamePolygonProviders {
            environment: SourcePlayerEnvironmentProvider::bind(
                session,
                profile,
                player,
                self.environment,
            )?,
            near_point: SourceNearPointProvider::bind_with_same_polygon(
                session,
                profile,
                self.height,
                self.near_configuration,
                binding,
            )?,
            route: SourceSamePolygonRouteProvider::bind_revision(
                session,
                profile,
                player,
                self.projection,
                binding,
            )?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SamePolygonError {
    Dependency(Failure),
    Constructor(ConstructorQueryError),
    Native(SourceSamePolygonError),
    GateRejected(ChaseGateRejection),
    CandidateBudget,
    /// No support-only adapter or source NORMAL bit may fill this missing proof.
    StrictProductionCorridorUnavailable,
}
impl SamePolygonError {
    pub(super) fn failure(self) -> Failure {
        match self {
            Self::Dependency(failure) => failure,
            Self::Constructor(ConstructorQueryError::Native(
                benilla_native_nav::Error::Deadline,
            ))
            | Self::Native(SourceSamePolygonError::Boundary(benilla_native_nav::Error::Deadline)) => {
                Failure::Deadline
            }
            _ => Failure::MissingGeometry,
        }
    }
}
impl From<Failure> for SamePolygonError {
    fn from(error: Failure) -> Self {
        Self::Dependency(error)
    }
}
impl From<ConstructorQueryError> for SamePolygonError {
    fn from(error: ConstructorQueryError) -> Self {
        Self::Constructor(error)
    }
}
impl From<SourceSamePolygonError> for SamePolygonError {
    fn from(error: SourceSamePolygonError) -> Self {
        Self::Native(error)
    }
}

/// This proposal has no standalone Route conversion. It retains the
/// native allocation and the exact constructor/configuration/loaded-cache lineage.
pub(crate) struct SamePolygonProposal {
    context: AuthorityRequest,
    dispatch: PathfinderDispatchReceipt,
    constructor_filter: Option<RouteFilterDependency>,
    model: ChaseGateModel,
    gate: ChaseGateObservations,
    owner_ignore: OwnerIgnorePathfindingReceipt,
    tiles: LoadedSourceTilePresence,
    filter: RouteFilterUpdateDependency,
    candidate: Rc<SamePolygonCandidate>,
}
struct SamePolygonCandidate {
    context: AuthorityRequest,
    dispatch: PathfinderDispatchReceipt,
    terminal: LoadedSourceSamePolygon,
    after_cache: motion::PathFinderCache,
}
impl SamePolygonProposal {
    pub(crate) fn terminal(&self) -> &LoadedSourceSamePolygon {
        &self.candidate.terminal
    }
    pub(crate) fn after_cache(&self) -> &motion::PathFinderCache {
        &self.candidate.after_cache
    }
}
struct GeneratorCandidates {
    npc: NpcId,
    construction: motion::QueryBinding,
    proposals: Vec<Rc<SamePolygonCandidate>>,
}
/// Bound before runtime is driven; consume requests inside its authority callback.
/// A captured unclaimed event alone does not establish this installation ordering.
/// This non-Send provider owns the actual query registry, filter and VMap products.
pub(crate) struct SourceSamePolygonRouteProvider {
    profile: Profile,
    player: PlayerId,
    binding: SamePolygonBinding,
    queries: SourcePathfinderQueryProvider,
    filter: SourceRouteFilterProvider,
    vmap: VmapSnapshot,
    candidates: Vec<GeneratorCandidates>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    ConstructorQuery,
    ConstructorFilter,
    Gate(GateObservation),
    SelectedQuery,
    OwnerIgnore,
    HaveTile,
    CalculateFilter,
    Terminal,
}
impl SourceSamePolygonRouteProvider {
    pub(super) fn raw_geometry(&self) -> [u8; 32] {
        self.binding.raw_geometry
    }
    /// A fresh loaded source ray for the exact terminal segment, separate from
    /// the Chase gate. The proposal is private and must still belong to this owner.
    pub(super) fn segment_line_of_sight(
        &self,
        session: &Session,
        proposal: &SamePolygonProposal,
        ticket: u64,
        deadline: Instant,
    ) -> Result<RayReply, SamePolygonError> {
        self.filter.begin(session, &proposal.context)?;
        if ticket == 0
            || !self.candidates.iter().any(|entry| {
                entry
                    .proposals
                    .iter()
                    .any(|candidate| Rc::ptr_eq(candidate, &proposal.candidate))
            })
        {
            return Err(Failure::Binding.into());
        }
        require_time(deadline)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &proposal.context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        let height = proposal.model.owner_height.height().bits();
        if height != route.request.owner_collision_height.bits() {
            return Err(Failure::UnresolvedBody.into());
        }
        let [from, to] = chase_gate::los_endpoints_bits(
            bits(route.request.origin),
            bits(route.request.goal),
            height,
            height,
        )
        .map_err(|_| Failure::Invalid)?;
        let ray = RayRequest {
            snapshot: self.vmap.binding(),
            ticket,
            from,
            to,
            ignore_m2: false,
        };
        let reply = self.vmap.query(ray, deadline).map_err(Failure::product)?;
        if reply.request != ray {
            return Err(Failure::Binding.into());
        }
        if !matches!(reply.result, StaticRay::Miss) {
            return Err(Failure::Obstacle.into());
        }
        require_time(deadline)?;
        Ok(reply)
    }
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        projection: SourceQueryProjection,
        raw_geometry: [u8; 32],
        configuration: SamePolygonConfiguration,
    ) -> Result<Self, SamePolygonError> {
        let binding =
            SamePolygonBinding::from_projection(raw_geometry, &projection, configuration)?;
        Self::bind_revision(session, profile, player, projection, binding)
    }
    fn bind_revision(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        projection: SourceQueryProjection,
        binding: SamePolygonBinding,
    ) -> Result<Self, SamePolygonError> {
        let environment = PlayerEnvironmentPolicy::from_profile(profile, player)?;
        environment.check(session)?;
        if session.player_environment.as_ref() != Some(&environment) {
            return Err(Failure::Binding.into());
        }
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(Failure::MissingGeometry)?
            .snapshot_binding();
        if source.map != binding.vmap.map
            || source.content != binding.vmap.content
            || source.inventory != binding.vmap.inventory
            || source.source_epoch != binding.vmap.source_epoch
        {
            return Err(Failure::Binding.into());
        }
        let (liquid, navigation, vmap) = projection.into_parts();
        let filter =
            SourceRouteFilterProvider::bind_with_same_polygon(session, profile, liquid, binding)?;
        let queries = SourcePathfinderQueryProvider::bind_with_same_polygon(
            session, profile, navigation, binding,
        )?;
        Ok(Self {
            profile: profile.clone(),
            player,
            binding,
            queries,
            filter,
            vmap,
            candidates: vec![],
        })
    }
    /// The prefix-only entry stays unavailable. The app-only LocalCorridor resolver
    /// must certify every exact segment before constructing a successful reply.
    pub(crate) fn resolve(
        &mut self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<Facts, SamePolygonError> {
        self.calculate(session, context, first_ticket, deadline)?;
        Err(SamePolygonError::StrictProductionCorridorUnavailable)
    }
    pub(crate) fn calculate(
        &mut self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<SamePolygonProposal, SamePolygonError> {
        #[cfg(not(test))]
        {
            self.calculate_observed(session, context, first_ticket, deadline, |_| {})
        }
        #[cfg(test)]
        {
            let started = Instant::now();
            let mut last_completed = None;
            let result =
                self.calculate_observed(session, context, first_ticket, deadline, |stage| {
                    last_completed = Some(stage);
                });
            if let Err(error) = &result {
                let ended = Instant::now();
                eprintln!(
                    "native geometry route calculate failure: tick={} last_completed={last_completed:?} \
                     error={error:?} elapsed={:?} deadline_expired_before={} deadline_expired_after={}",
                    context.binding.tick,
                    ended.duration_since(started),
                    started >= deadline,
                    ended >= deadline,
                );
            }
            result
        }
    }
    fn calculate_observed(
        &mut self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<SamePolygonProposal, SamePolygonError> {
        if first_ticket == 0 {
            return Err(Failure::Binding.into());
        }
        first_ticket.checked_add(5).ok_or(Failure::Invalid)?;
        require_time(deadline)?;
        let bound_filter = self.filter.begin(session, context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if !matches!(
            route.request.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) || route.request.launch_origin != route.request.origin
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let event = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let model = gate_model(&self.profile, session.world, self.player, route)?;
        let npc = route.owner.npc();
        let construction = event.construction_binding();
        let index = if let Some(index) = self
            .candidates
            .iter()
            .position(|entry| entry.npc == npc && entry.construction == construction)
        {
            index
        } else {
            if !event.first_construction() || route.request.prior_cache.is_some() {
                return Err(Failure::Binding.into());
            }
            if self.candidates.len() == MAX_GENERATORS {
                return Err(SamePolygonError::CandidateBudget);
            }
            self.candidates
                .try_reserve(1)
                .map_err(|_| SamePolygonError::CandidateBudget)?;
            self.candidates.push(GeneratorCandidates {
                npc,
                construction,
                proposals: vec![],
            });
            self.candidates.len() - 1
        };
        let entry = &mut self.candidates[index];
        // The runtime-owned exact prior selects a native object, never rehydrates
        // serialized refs. Keep every ancestor: later ticks can still roll back as
        // part of step_batch. No implicit promotion, pruning or resolve-time commit.
        let prior = match route.request.prior_cache.as_deref() {
            None if event.first_construction() => None,
            Some(prior) if !event.first_construction() => {
                let mut matching = entry.proposals.iter().filter(|candidate| {
                    candidate.after_cache == *prior
                        && candidate.context.binding.tick <= context.binding.tick
                        && candidate.dispatch.dispatch_binding().query_id
                            < event.dispatch_binding().query_id
                });
                let candidate = matching.next().ok_or(Failure::Binding)?;
                // Equal cache DTOs from divergent speculative contexts are ambiguous.
                if matching.next().is_some() {
                    return Err(Failure::Binding.into());
                }
                Some(Rc::clone(candidate))
            }
            _ => return Err(Failure::Binding.into()),
        };
        let retry = entry
            .proposals
            .iter()
            .position(|candidate| candidate.context == *context);
        if retry.is_none() {
            if entry.proposals.len() == MAX_CANDIDATES_PER_GENERATOR {
                return Err(SamePolygonError::CandidateBudget);
            }
            entry
                .proposals
                .try_reserve(1)
                .map_err(|_| SamePolygonError::CandidateBudget)?;
        }
        let mut constructor = self.queries.retain_constructor(context)?;
        observe(Observation::ConstructorQuery);
        let constructor_filter = if event.first_construction() {
            let result = bound_filter.query(
                RouteFilterRequest {
                    snapshot: self.binding.liquid,
                    context: context.clone(),
                    first_ticket,
                },
                deadline,
            )?;
            observe(Observation::ConstructorFilter);
            Some(result)
        } else {
            None
        };
        let gate = LoadedChaseGate {
            profile: &self.profile,
            player: self.player,
            liquid: self.filter.dry_liquid(),
            vmap: &self.vmap,
            model: &model,
        }
        .query(context, first_ticket + 2, deadline, |gate| {
            observe(Observation::Gate(gate))
        })?;
        if let ChaseGateSelection::NormalRouteUnavailable(reason) = gate.selection {
            return Err(SamePolygonError::GateRejected(reason));
        }
        // PathFinder.cpp: destination/start checks and selection, owned ignore bit,
        // both HaveTile calls, then a fresh updateFilter. Constructor queries above
        // are separate observations, before TargetedMovementGenerator's Chase gate.
        let selected = constructor.select_for_calculate(deadline)?;
        observe(Observation::SelectedQuery);
        let owner_ignore = route
            .require_owner_ignore_pathfinding(&self.profile)
            .map_err(|_| Failure::Binding)?
            .clone();
        observe(Observation::OwnerIgnore);
        if owner_ignore.ignore_pathfinding() {
            return Err(Failure::UnsupportedQuery.into());
        }
        require_time(deadline)?;
        let tiles = selected
            .tile_presence(deadline)
            .map_err(|e| SamePolygonError::Constructor(ConstructorQueryError::Native(e)))?;
        selected
            .check_tile_presence(&tiles)
            .map_err(|e| SamePolygonError::Constructor(ConstructorQueryError::Native(e)))?;
        observe(Observation::HaveTile);
        if !tiles.both_present() {
            return Err(Failure::MissingGeometry.into());
        }
        let retained_filter = prior
            .as_ref()
            .map(|prior| prior.terminal.compatibility().filter())
            .unwrap_or(SourceOwnerFilter {
                include_flags: 1,
                exclude_flags: 0,
            });
        if constructor_filter.as_ref().is_some_and(|created| {
            created.filter().include_flags() != retained_filter.include_flags
                || created.filter().exclude_flags() != retained_filter.exclude_flags
        }) {
            return Err(Failure::Binding.into());
        }
        let filter = bound_filter.query_retained_update(
            RouteFilterRequest {
                snapshot: self.binding.liquid,
                context: context.clone(),
                first_ticket: first_ticket + 4,
            },
            retained_filter,
            None,
            deadline,
        )?;
        observe(Observation::CalculateFilter);
        let config = self.binding.configuration;
        let terminal = if let Some(prior) = &prior {
            selected.loaded_retained_same_polygon(
                &prior.terminal,
                config.normalization,
                config.retained_budget,
                deadline,
            )?
        } else {
            selected.loaded_cold_same_polygon(
                SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
                config.normalization,
                config.cold_budget,
                deadline,
            )?
        };
        terminal
            .check_query(&selected)
            .map_err(|_| Failure::Binding)?;
        observe(Observation::Terminal);
        if terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.path_type() != 1
            || terminal.compatibility().active_len() != 1
        {
            return Err(Failure::Binding.into());
        }
        let after_cache = motion::PathFinderCache {
            binding: motion::PathFinderCacheBinding {
                identity: route.request.binding.identity,
                owner_generation: npc.generation,
                inventory: self.binding.navigation.inventory,
                source_epoch: self.binding.navigation.source_epoch,
                navigation_policy: self.binding.geometry_revision()?,
            },
            revision: match &prior {
                Some(prior) => prior
                    .after_cache
                    .revision
                    .checked_add(1)
                    .ok_or(Failure::Invalid)?,
                None => 1,
            },
            source: motion::RouteSource::OwnerPathFinder,
            poly_refs: terminal.compatibility().storage().to_vec(),
            active_len: 1,
            point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
            path_type: 1,
            requested_end: route.request.goal,
            include_flags: filter.filter().include_flags,
            exclude_flags: filter.filter().exclude_flags,
        };
        after_cache
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        require_time(deadline)?;
        let candidate = if let Some(index) = retry {
            let previous = &entry.proposals[index];
            if previous.after_cache != after_cache
                || previous.terminal.compatibility() != terminal.compatibility()
                || previous.terminal.points() != terminal.points()
            {
                return Err(Failure::Binding.into());
            }
            Rc::clone(previous)
        } else {
            let candidate = Rc::new(SamePolygonCandidate {
                context: context.clone(),
                dispatch: event.clone(),
                terminal,
                after_cache,
            });
            entry.proposals.push(Rc::clone(&candidate));
            candidate
        };
        // Repeated calculations reuse the immutable native candidate but return
        // fresh callback/constructor/filter observations, including its live seal.
        Ok(SamePolygonProposal {
            context: context.clone(),
            dispatch: event.clone(),
            constructor_filter,
            model,
            gate,
            owner_ignore,
            tiles,
            filter,
            candidate,
        })
    }
}
fn bits(point: Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
#[cfg_attr(test, track_caller)]
fn require_time(deadline: Instant) -> Result<(), Failure> {
    if Instant::now() >= deadline {
        #[cfg(test)]
        eprintln!(
            "native geometry route deadline failure: checkpoint={} overdue={:?}",
            std::panic::Location::caller(),
            Instant::now().saturating_duration_since(deadline),
        );
        Err(Failure::Deadline)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_route_same_polygon/tests.rs"]
pub(crate) mod tests;
