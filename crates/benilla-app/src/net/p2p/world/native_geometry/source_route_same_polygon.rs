// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-ordered dry Chase/Home/Wander calculation on the actual retained query. A terminal
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
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_near_point::{SourceNearPointConfiguration, SourceNearPointProvider},
    source_pathfinder_query::{
        ConstructorQueryError, RegisteredConstructorHandle, SourcePathfinderQueryProvider,
    },
    source_player_environment::SourcePlayerEnvironmentProvider,
    source_route_chase_gate::{
        ChaseGateModel, ChaseGateObservations, ChaseGateRejection, ChaseGateSelection,
        LoadedChaseGate, Observation as GateObservation,
    },
    source_route_filter::{
        OwnedRouteFilterContext, RouteFilterDependency, RouteFilterRequest,
        RouteFilterUpdateDependency, SourceRouteFilterProvider,
    },
};
use benilla_native_nav::{
    LoadedSourceHomeSamePolygon, LoadedSourceSamePolygon, LoadedSourceTilePresence,
    LoadedSourceWanderTerminal, SourceOwnerFilter, SourceSamePolygonBudget,
    SourceSamePolygonConfig, SourceSamePolygonError, SourceWanderError,
    SOURCE_COLD_DISTANCE_COMPATIBILITY_ID, SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION,
    SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION, SOURCE_COLD_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
    SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES, SOURCE_WANDER_ENDPOINT_COMPATIBILITY_ID,
    SOURCE_WANDER_ENDPOINT_COMPATIBILITY_REVISION, SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
};
use benilla_sim::{chase_gate, creature_ai::Point, creature_motion as motion};
use benilla_world_runtime::{
    native::{
        AuthorityReply, AuthorityRequest, Facts, MotionQuery, OwnerIgnorePathfindingReceipt,
        PathfinderDispatchReceipt, Profile, Query, CREATURE_COLLISION_HEIGHT_POLICY,
        OWNER_IGNORE_PATHFINDING_POLICY, PLAYER_COLLISION_HEIGHT_POLICY, PLAYER_ENVIRONMENT_POLICY,
    },
    NpcId, PlayerId,
};
use sha2::{Digest, Sha256};
use std::{rc::Rc, sync::Arc, time::Instant};

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
        self.geometry_revision_for_policy(
            b"benilla-owned-dry-same-polygon-chase-home-wander-commit-continuation-v6",
            b"ordinary-zero-offset-chase;distinct-respawn-centered-wander;home-local-cold-normal-force-true-straight-false;home-corrected-launch-clearance;other-straight-true-force-false;exact-two-point-endpoints;wander-loaded-water-ground-tail;outer-commit-ack;total-cap4;bounded-original-job-six-phase",
        )
    }
    fn geometry_revision_for_policy(
        self,
        domain: &[u8],
        lifecycle: &[u8],
    ) -> Result<[u8; 32], Failure> {
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
        hash.update(domain);
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
            benilla_native_nav::SOURCE_HOME_SAME_POLYGON_POLICY,
            SOURCE_WANDER_ENDPOINT_COMPATIBILITY_ID,
            SOURCE_WANDER_ENDPOINT_COMPATIBILITY_REVISION,
            motion::WANDER_POLICY_ID,
            benilla_sim::source_trig::SOFT_POLICY,
        ] {
            hash.update((policy.len() as u64).to_le_bytes());
            hash.update(policy.as_bytes());
        }
        for source in chase_gate::SOURCE_SHA256 {
            hash.update(source.as_bytes());
        }
        hash.update(SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION.to_le_bytes());
        hash.update(SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION.to_le_bytes());
        hash.update(wander::POLICY);
        for source in benilla_sim::source_trig::SOURCE_SHA256 {
            hash.update(source.as_bytes());
        }
        hash.update(
            (benilla_native_nav::SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES as u64).to_le_bytes(),
        );
        hash.update(
            (benilla_native_nav::SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES as u64).to_le_bytes(),
        );
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
        hash.update(lifecycle);
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
        let height = Arc::new(self.height);
        let mut route = SourceSamePolygonRouteProvider::bind_revision(
            session,
            profile,
            player,
            self.projection,
            binding,
        )?;
        route.wander_height = Some(Arc::clone(&height));
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
                height,
                self.near_configuration,
                binding,
            )?,
            route,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SamePolygonError {
    Dependency(Failure),
    Constructor(ConstructorQueryError),
    Native(SourceSamePolygonError),
    Wander(SourceWanderError),
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
            | Self::Native(SourceSamePolygonError::Boundary(benilla_native_nav::Error::Deadline))
            | Self::Wander(SourceWanderError::Boundary(benilla_native_nav::Error::Deadline)) => {
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

impl From<SourceWanderError> for SamePolygonError {
    fn from(error: SourceWanderError) -> Self {
        Self::Wander(error)
    }
}

/// This proposal has no standalone Route conversion. It retains the
/// native allocation and the exact constructor/configuration/loaded-cache lineage.
pub(crate) struct SamePolygonProposal {
    prior: Option<Rc<SamePolygonCandidate>>,
    filter_context: OwnedRouteFilterContext,
    constructor: RegisteredConstructorHandle,
    context: AuthorityRequest,
    dispatch: PathfinderDispatchReceipt,
    constructor_filter: Option<RouteFilterDependency>,
    model: SamePolygonModel,
    gate: Option<ChaseGateObservations>,
    owner_ignore: Option<OwnerIgnorePathfindingReceipt>,
    tiles: Option<LoadedSourceTilePresence>,
    filter: RouteFilterUpdateDependency,
    candidate: Rc<SamePolygonCandidate>,
    #[cfg(test)]
    timing: Option<CalculateTiming>,
}
#[cfg(test)]
struct CalculateTiming {
    elapsed: std::time::Duration,
    stages: [Option<(&'static str, std::time::Duration)>; 16],
    count: usize,
}
struct SamePolygonCandidate {
    context: AuthorityRequest,
    dispatch: PathfinderDispatchReceipt,
    terminal: SamePolygonTerminal,
    wander_tail: Option<wander::WanderTail>,
    wander_prefix: Option<Rc<wander::WanderPreparation>>,
    after_cache: motion::PathFinderCache,
}
impl SamePolygonProposal {
    /// Print only after the concrete resolver has finished its deadline checks.
    #[cfg(test)]
    pub(super) fn report_timing(&self) {
        if let Some(timing) = &self.timing {
            eprintln!(
                "native geometry Calculate stages: tick={} success=true elapsed={:?} completed={:?}",
                self.context.binding.tick,
                timing.elapsed,
                &timing.stages[..timing.count],
            );
        }
    }
    #[cfg(test)]
    pub(crate) fn terminal(&self) -> &LoadedSourceSamePolygon {
        match &self.candidate.terminal {
            SamePolygonTerminal::Chase(terminal) => terminal,
            SamePolygonTerminal::Home(_) | SamePolygonTerminal::Wander(_) => {
                panic!("Chase terminal accessor on another generator proposal")
            }
        }
    }
    pub(crate) fn points(&self) -> &[[u32; 3]] {
        self.candidate.terminal.points()
    }
    pub(crate) fn wander_terminal(&self) -> Option<&LoadedSourceWanderTerminal> {
        match &self.candidate.terminal {
            SamePolygonTerminal::Wander(terminal) => Some(terminal),
            SamePolygonTerminal::Chase(_) | SamePolygonTerminal::Home(_) => None,
        }
    }
    pub(crate) fn home_terminal(&self) -> Option<&LoadedSourceHomeSamePolygon> {
        match &self.candidate.terminal {
            SamePolygonTerminal::Home(terminal) => Some(terminal),
            SamePolygonTerminal::Chase(_) | SamePolygonTerminal::Wander(_) => None,
        }
    }
    pub(crate) fn home_source_receipt(&self) -> Option<HomeSourceReceipt<'_>> {
        self.home_terminal()?;
        Some(HomeSourceReceipt {
            request: &self.context,
            dispatch: &self.dispatch,
            constructor_filter: self.constructor_filter.as_ref()?,
            update_filter: &self.filter,
            owner_ignore: self.owner_ignore.as_ref()?,
            tiles: self.tiles.as_ref()?,
        })
    }
    pub(crate) fn wander_source_receipt(&self) -> Option<wander::WanderSourceReceipt<'_>> {
        let prefix = self.candidate.wander_prefix.as_ref()?;
        Some(wander::WanderSourceReceipt {
            preparation: &prefix.prepared.validation.context,
            constructor_filter: prefix.constructor_filter.as_ref(),
            update_filter: &prefix.filter,
            clear_before_active: prefix.native.before_active_len(),
            clear_after_active: prefix.native.cleared_active_len(),
            selected: prefix.native.current_selected(),
            tail: self.candidate.wander_tail.as_ref(),
        })
    }
    pub(crate) fn after_cache(&self) -> &motion::PathFinderCache {
        &self.candidate.after_cache
    }
}
/// Home's stack-local normal-path prefix. ForceDestination is selected by the
/// exact Home request; the same-polygon return precedes its fallback rewrite.
pub(crate) struct HomeSourceReceipt<'a> {
    pub request: &'a AuthorityRequest,
    pub dispatch: &'a PathfinderDispatchReceipt,
    pub constructor_filter: &'a RouteFilterDependency,
    pub update_filter: &'a RouteFilterUpdateDependency,
    pub owner_ignore: &'a OwnerIgnorePathfindingReceipt,
    pub tiles: &'a LoadedSourceTilePresence,
}
struct ValidatedCalculation {
    initialized: Option<super::source_route_filter::InitializedOwnerValidation>,
    owner_ignore: Option<OwnerIgnorePathfindingReceipt>,
    context: AuthorityRequest,
    first_ticket: u64,
    dispatch: PathfinderDispatchReceipt,
    model: SamePolygonModel,
    filter_context: OwnedRouteFilterContext,
}
struct ConstructorDependencies {
    prior: Option<Rc<SamePolygonCandidate>>,
    constructor: RegisteredConstructorHandle,
    reserved: bool,
}
struct PreparedCalculation {
    validation: ValidatedCalculation,
    dependencies: ConstructorDependencies,
}
struct GateDependencies {
    constructor_filter: Option<RouteFilterDependency>,
    gate: Option<ChaseGateObservations>,
}
struct TerminalDependencies {
    owner_ignore: Option<OwnerIgnorePathfindingReceipt>,
    tiles: Option<LoadedSourceTilePresence>,
    filter: RouteFilterUpdateDependency,
    candidate: Rc<SamePolygonCandidate>,
}

#[path = "source_route_same_polygon/staged.rs"]
mod staged;
#[path = "source_route_same_polygon/wander.rs"]
mod wander;
use wander::SamePolygonTerminal;
pub(super) use wander::{
    same_polygon_model, same_polygon_model_with_initialized, SamePolygonModel,
};

struct GeneratorCandidates {
    npc: NpcId,
    construction: motion::QueryBinding,
    kind: motion::PathfinderConstructorKind,
    proposals: Vec<Rc<SamePolygonCandidate>>,
    certified: Vec<AuthorityReply>,
    committed: Option<Rc<SamePolygonCandidate>>,
}
/// Bound before runtime is driven; consume requests inside its authority callback.
/// A captured unclaimed event alone does not establish this installation ordering.
/// This non-Send provider owns the actual query registry, filter and VMap products.
pub(crate) struct SourceSamePolygonRouteProvider {
    identity: Arc<()>,
    profile: Profile,
    player: PlayerId,
    binding: SamePolygonBinding,
    queries: SourcePathfinderQueryProvider,
    filter: SourceRouteFilterProvider,
    vmap: VmapSnapshot,
    wander_height: Option<Arc<MapHeightSnapshot<'static>>>,
    wander_prefixes: Vec<Rc<wander::WanderPreparation>>,
    candidates: Vec<GeneratorCandidates>,
    committed_boundary: Option<RouteCommitBoundary>,
    last_notice: Option<RouteCommitNotice>,
    pending: Vec<staged::PendingCalculation>,
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
    WanderClear,
    WanderWater,
    WanderGround,
}
#[cfg(test)]
impl Observation {
    fn name(self) -> &'static str {
        match self {
            Self::ConstructorQuery => "constructor_query",
            Self::ConstructorFilter => "constructor_filter",
            Self::Gate(_) => "chase_gate",
            Self::SelectedQuery => "selected_query",
            Self::OwnerIgnore => "owner_ignore",
            Self::HaveTile => "have_tile",
            Self::CalculateFilter => "calculate_filter",
            Self::Terminal => "terminal",
            Self::WanderClear => "wander_clear",
            Self::WanderWater => "wander_tail_water",
            Self::WanderGround => "wander_tail_ground",
        }
    }
}
impl SourceSamePolygonRouteProvider {
    /// Only the concrete LocalCorridor seam records successful certification. The
    /// exact complete facts, not calculation alone, may later select a commit head.
    pub(super) fn certify(
        &mut self,
        session: &Session,
        proposal: &SamePolygonProposal,
        facts: &Facts,
    ) -> Result<(), SamePolygonError> {
        super::core::validate_facts(session, &proposal.context, facts)?;
        self.record_certified(&proposal.context, &proposal.candidate, facts)
    }
    /// Only the original live job may reuse its completed immutable body check.
    pub(super) fn certify_job(
        &mut self,
        job: &super::core::RouteJob<'_>,
        facts: &Facts,
    ) -> Result<(), SamePolygonError> {
        job.validate_facts(facts)?;
        let proposal = self.terminal_for_job(job)?;
        let context = proposal.context.clone();
        let candidate = Rc::clone(&proposal.candidate);
        self.record_certified(&context, &candidate, facts)
    }
    fn record_certified(
        &mut self,
        context: &AuthorityRequest,
        candidate: &Rc<SamePolygonCandidate>,
        facts: &Facts,
    ) -> Result<(), SamePolygonError> {
        self.check_current_boundary(context)?;
        let Facts::SourceMotion(benilla_world_runtime::native::MotionFacts::Spatial(reply)) = facts
        else {
            return Err(Failure::Binding.into());
        };
        let motion::Fact::RouteCalculation(calculation) = &reply.fact else {
            return Err(Failure::Binding.into());
        };
        if calculation.after_cache != candidate.after_cache {
            return Err(Failure::Binding.into());
        }
        let entry = self
            .candidates
            .iter_mut()
            .find(|entry| {
                entry
                    .proposals
                    .iter()
                    .any(|entry| Rc::ptr_eq(entry, candidate))
            })
            .ok_or(Failure::Binding)?;
        let reply = AuthorityReply {
            request: context.clone(),
            facts: facts.clone(),
        };
        if let Some(prior) = entry
            .certified
            .iter()
            .find(|prior| prior.request == reply.request)
        {
            if *prior != reply {
                return Err(Failure::Binding.into());
            }
        } else {
            entry
                .certified
                .try_reserve(1)
                .map_err(|_| SamePolygonError::CandidateBudget)?;
            entry.certified.push(reply);
        }
        Ok(())
    }
    fn check_current_boundary(&self, context: &AuthorityRequest) -> Result<(), Failure> {
        if self
            .committed_boundary
            .as_ref()
            .is_some_and(|boundary| boundary.tick().checked_add(1) != Some(context.binding.tick))
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    fn check_commit_binding(
        &self,
        session: &Session,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        if boundary.world() != session.world
            || boundary.player() != self.player
            || boundary.profile() != session.profile
            || boundary.profile() != self.profile.digest()
            || boundary.geometry() != session.geometry
            || boundary.geometry() != self.binding.geometry_revision()?
            || boundary.authority() != session.policy
            || boundary.heads().len() != self.profile.npcs.len()
        {
            return Err(Failure::Binding);
        }
        let mut previous_spawn = None;
        for head in boundary.heads() {
            if !session.bodies.contains_key(&head.npc().spawn)
                || previous_spawn.is_some_and(|spawn| spawn >= head.npc().spawn)
                || head.cache().is_some() && head.construction().is_none()
            {
                return Err(Failure::Binding);
            }
            previous_spawn = Some(head.npc().spawn);
        }
        Ok(())
    }
    /// Reconstruction starts before any attempted calculation, including a
    /// rolled-back first tick. Ordinary same-session retry remains separate.
    pub(super) fn initialize_reconstruction(
        &mut self,
        session: &Session,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        if boundary.tick() != 0
            || self.queries.constructor_count() != 0
            || !self.candidates.is_empty()
            || !self.wander_prefixes.is_empty()
            || !self.pending.is_empty()
            || self.last_notice.is_some()
        {
            return Err(Failure::Binding);
        }
        self.initialize_commit(session, boundary)
    }
    /// Initial attachment establishes the exact cache-free journal boundary. It
    /// cannot adopt proposals or native state from a different loaded session.
    pub(super) fn initialize_commit(
        &mut self,
        session: &Session,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        self.check_commit_binding(session, boundary)?;
        if let Some(current) = &self.committed_boundary {
            return if current == boundary && self.last_notice.is_none() {
                Ok(())
            } else {
                Err(Failure::Binding)
            };
        }
        if boundary.heads().iter().any(|head| head.cache().is_some())
            || self
                .candidates
                .iter()
                .any(|entry| !entry.proposals.is_empty())
        {
            return Err(Failure::Binding);
        }
        self.committed_boundary = Some(boundary.clone());
        Ok(())
    }
    /// Validate the whole certified chain and final current Chase cache before
    /// changing any retained owner. Spline prior-cache history is not a head.
    pub(super) fn acknowledge(
        &mut self,
        session: &Session,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        let boundary = notice.boundary();
        self.check_commit_binding(session, boundary)?;
        if self.last_notice.as_ref() == Some(notice) {
            return Ok(());
        }
        let previous = self.committed_boundary.as_ref().ok_or(Failure::Binding)?;
        if boundary.session() != previous.session()
            || previous.tick().checked_add(1) != Some(boundary.tick())
            || notice.previous_record() != previous.record()
            || notice.before() != previous.state()
            || boundary.record() == previous.record()
        {
            return Err(Failure::Binding);
        }
        let mut heads: Vec<_> = self
            .candidates
            .iter()
            .map(|entry| entry.committed.clone())
            .collect();
        for reply in notice.routes() {
            if reply.request.binding.tick != boundary.tick() {
                return Err(Failure::Binding);
            }
            super::core::validate_facts(session, &reply.request, &reply.facts)?;
            let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
                return Err(Failure::Binding);
            };
            let dispatch = reply
                .request
                .require_pathfinder_dispatch(&self.profile)
                .map_err(|_| Failure::Binding)?;
            let index = self
                .candidates
                .iter()
                .position(|entry| {
                    entry.npc == route.owner.npc()
                        && entry.construction == dispatch.construction_binding()
                })
                .ok_or(Failure::Binding)?;
            let entry = &self.candidates[index];
            if !entry.certified.contains(reply) {
                return Err(Failure::Binding);
            }
            let mut matching = entry
                .proposals
                .iter()
                .filter(|candidate| candidate.context == reply.request);
            let candidate = matching.next().ok_or(Failure::Binding)?;
            if matching.next().is_some()
                || route.request.prior_cache.as_deref()
                    != heads[index].as_ref().map(|head| &head.after_cache)
                || heads[index].as_ref().is_some_and(|head| {
                    head.dispatch.dispatch_binding().query_id
                        >= dispatch.dispatch_binding().query_id
                })
            {
                return Err(Failure::Binding);
            }
            heads[index] = Some(Rc::clone(candidate));
        }
        // Every live head must be the final exact candidate of its accepted chain.
        // Explicit generator absence drops ownership rather than promoting history.
        for head in boundary.heads() {
            if let Some(cache) = head.cache() {
                let index = self
                    .candidates
                    .iter()
                    .position(|entry| {
                        entry.npc == head.npc() && Some(entry.construction) == head.construction()
                    })
                    .ok_or(Failure::Binding)?;
                if heads[index]
                    .as_ref()
                    .is_none_or(|candidate| candidate.after_cache != *cache)
                {
                    return Err(Failure::Binding);
                }
            }
        }
        for (index, entry) in self.candidates.iter_mut().enumerate() {
            let current = boundary.heads().iter().find(|head| {
                head.npc() == entry.npc && head.construction() == Some(entry.construction)
            });
            let selected = current
                .and_then(|head| head.cache())
                .and_then(|_| heads[index].take());
            entry.proposals.retain(|candidate| {
                selected
                    .as_ref()
                    .is_some_and(|head| Rc::ptr_eq(candidate, head))
            });
            entry.certified.retain(|reply| {
                selected
                    .as_ref()
                    .is_some_and(|head| head.context == reply.request)
            });
            entry.committed = selected;
        }
        // Retire replaced generators only after the whole outer ACK validates.
        // Cache-free current heads retain their constructor for a later dispatch;
        // Home's local owner and the pre-Reset default can never become live heads.
        self.candidates.retain(|entry| {
            entry.kind != motion::PathfinderConstructorKind::HomeLocal
                && boundary.heads().iter().any(|head| {
                    head.npc() == entry.npc && head.construction() == Some(entry.construction)
                })
        });
        self.queries.retain_committed_generators(boundary);
        self.wander_prefixes
            .retain(|prefix| prefix.prepared.validation.context.binding.tick > boundary.tick());
        self.committed_boundary = Some(boundary.clone());
        self.last_notice = Some(notice.clone());
        self.reap_jobs();
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn registered_generator_count(&self) -> usize {
        self.candidates.len()
    }
    #[cfg(test)]
    pub(super) fn registered_constructor_count(&self) -> usize {
        self.queries.constructor_count()
    }
    #[cfg(test)]
    pub(super) fn retained_candidate_count(&self) -> usize {
        self.candidates
            .iter()
            .map(|entry| entry.proposals.len())
            .sum()
    }
    #[cfg(test)]
    pub(super) fn certified_candidate_count(&self) -> usize {
        self.candidates
            .iter()
            .map(|entry| entry.certified.len())
            .sum()
    }
    #[cfg(test)]
    pub(super) fn committed_candidate(&self) -> Option<(usize, &motion::PathFinderCache)> {
        self.candidates
            .iter()
            .find_map(|entry| entry.committed.as_ref())
            .map(|candidate| (Rc::as_ptr(candidate) as usize, &candidate.after_cache))
    }
    #[cfg(test)]
    pub(super) fn committed_candidate_for(
        &self,
        npc: NpcId,
    ) -> Option<(usize, &motion::PathFinderCache)> {
        self.candidates
            .iter()
            .filter(|entry| entry.npc == npc)
            .find_map(|entry| entry.committed.as_ref())
            .map(|candidate| (Rc::as_ptr(candidate) as usize, &candidate.after_cache))
    }
    #[cfg(test)]
    pub(super) fn candidate_identity(&self, reply: &AuthorityReply) -> Option<usize> {
        self.candidates
            .iter()
            .flat_map(|entry| &entry.proposals)
            .find(|candidate| candidate.context == reply.request)
            .map(|candidate| Rc::as_ptr(candidate) as usize)
    }
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
        self.segment_line_of_sight_prepared(proposal, ticket, deadline)
    }
    pub(super) fn segment_line_of_sight_prepared(
        &self,
        proposal: &SamePolygonProposal,
        ticket: u64,
        deadline: Instant,
    ) -> Result<RayReply, SamePolygonError> {
        self.filter.reborrow(&proposal.filter_context)?;
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
        let corrected = if route.request.launch_origin != route.request.origin {
            if route.request.kind != motion::RouteKind::HomeForceDestination {
                return Err(Failure::Binding.into());
            }
            Some((
                route.request.launch_origin,
                ticket.checked_add(1).ok_or(Failure::Invalid)?,
            ))
        } else {
            None
        };
        let mut result = None;
        for (origin, ticket) in std::iter::once((route.request.origin, ticket)).chain(corrected) {
            let [from, to] = chase_gate::los_endpoints_bits(
                bits(origin),
                proposal.candidate.terminal.actual_end(),
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
            result = Some(reply);
        }
        result.ok_or_else(|| Failure::Binding.into())
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
            identity: Arc::new(()),
            profile: profile.clone(),
            player,
            binding,
            queries,
            filter,
            vmap,
            wander_height: None,
            wander_prefixes: vec![],
            candidates: vec![],
            committed_boundary: None,
            last_notice: None,
            pending: vec![],
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
            let mut stage_times = [None; 16];
            let mut completed_count = 0;
            let mut result =
                self.calculate_observed(session, context, first_ticket, deadline, |stage| {
                    last_completed = Some(stage);
                    if completed_count < stage_times.len() {
                        stage_times[completed_count] = Some((stage.name(), started.elapsed()));
                        completed_count += 1;
                    }
                });
            if let Ok(proposal) = &mut result {
                proposal.timing = Some(CalculateTiming {
                    elapsed: started.elapsed(),
                    stages: stage_times,
                    count: completed_count,
                });
            } else if completed_count > 0 {
                eprintln!(
                    "native geometry Calculate stages: tick={} success={} elapsed={:?} completed={:?}",
                    context.binding.tick,
                    result.is_ok(),
                    started.elapsed(),
                    &stage_times[..completed_count],
                );
            }
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
        let prepared =
            self.prepare_calculation(session, context, first_ticket, deadline, &mut observe)?;
        let gate = self.calculate_gate(&prepared, deadline, &mut observe)?;
        let terminal = self.calculate_terminal(&prepared, &gate, deadline, &mut observe)?;
        Ok(Self::assemble_proposal(prepared, gate, terminal))
    }

    fn prepare_calculation(
        &mut self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<PreparedCalculation, SamePolygonError> {
        let validation = self.validate_calculation(session, context, first_ticket, deadline)?;
        let dependencies = self.prepare_constructor(&validation, None, deadline, &mut observe)?;
        Ok(PreparedCalculation {
            validation,
            dependencies,
        })
    }

    fn validate_calculation(
        &self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<ValidatedCalculation, SamePolygonError> {
        self.check_current_boundary(context)?;
        if first_ticket == 0 {
            return Err(Failure::Binding.into());
        }
        first_ticket.checked_add(5).ok_or(Failure::Invalid)?;
        require_time(deadline)?;
        let filter_context = self.filter.prepare_context(session, context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if !matches!(
            route.request.kind,
            motion::RouteKind::ChaseInitial
                | motion::RouteKind::ChaseReplan
                | motion::RouteKind::Wander
                | motion::RouteKind::WanderPrepare
                | motion::RouteKind::HomeForceDestination
        ) || (route.request.kind != motion::RouteKind::HomeForceDestination
            && route.request.launch_origin != route.request.origin)
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let event = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if route.request.kind == motion::RouteKind::HomeForceDestination
            && (!event.first_construction()
                || event.constructor_kind() != motion::PathfinderConstructorKind::HomeLocal
                || event.construction_binding() != event.dispatch_binding()
                || route.request.prior_cache.is_some())
        {
            return Err(Failure::Binding.into());
        }
        let model = same_polygon_model(&self.profile, session.world, self.player, route)?;
        require_time(deadline)?;
        Ok(ValidatedCalculation {
            initialized: None,
            owner_ignore: None,
            context: context.clone(),
            first_ticket,
            dispatch: event.clone(),
            model,
            filter_context,
        })
    }

    fn validate_job_calculation(
        &self,
        job: &super::core::RouteJob<'_>,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<ValidatedCalculation, SamePolygonError> {
        self.check_current_boundary(job.request())?;
        if first_ticket == 0 {
            return Err(Failure::Binding.into());
        }
        first_ticket.checked_add(5).ok_or(Failure::Invalid)?;
        require_time(deadline)?;
        let model = job
            .validated_model(&self.profile, self.player, self.binding)?
            .clone();
        let dispatch = job.validated_dispatch(&self.profile)?.clone();
        let owner_ignore = job.validated_owner_ignore(&self.profile)?.clone();
        let filter_context = self.filter.prepare_job_context(job)?;
        require_time(deadline)?;
        Ok(ValidatedCalculation {
            initialized: job.validated_initialized(&self.profile)?.cloned(),
            owner_ignore: Some(owner_ignore),
            context: job.request().clone(),
            first_ticket,
            dispatch,
            model,
            filter_context,
        })
    }

    fn prepare_constructor(
        &mut self,
        validation: &ValidatedCalculation,
        job: Option<&super::core::RouteJob<'_>>,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<ConstructorDependencies, SamePolygonError> {
        require_time(deadline)?;
        self.check_validated(validation)?;
        let context = &validation.context;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if route.request.kind == motion::RouteKind::Wander {
            let dependencies = self.prepare_wander_continuation(validation, job)?;
            require_time(deadline)?;
            return Ok(dependencies);
        }
        let event = &validation.dispatch;
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
                kind: event.constructor_kind(),
                proposals: vec![],
                certified: vec![],
                committed: None,
            });
            self.candidates.len() - 1
        };
        let reservations = self.reservations(npc, construction);
        let entry = &mut self.candidates[index];
        // The runtime-owned exact prior selects a native object, never rehydrates
        // serialized refs. Keep every uncommitted ancestor until the controller
        // acknowledges the complete successful step_batch, never on resolve.
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
            if entry.proposals.len() + reservations >= MAX_CANDIDATES_PER_GENERATOR {
                return Err(SamePolygonError::CandidateBudget);
            }
            entry
                .proposals
                .try_reserve(1)
                .map_err(|_| SamePolygonError::CandidateBudget)?;
        }
        let constructor = if let Some(job) = job {
            if job.request() != context {
                return Err(Failure::Binding.into());
            }
            self.queries.prepare_constructor_for_job(job)?
        } else {
            self.queries.prepare_constructor(context)?
        };
        observe(Observation::ConstructorQuery);
        require_time(deadline)?;
        Ok(ConstructorDependencies {
            prior,
            constructor,
            reserved: retry.is_none(),
        })
    }

    fn calculate_gate(
        &self,
        prepared: &PreparedCalculation,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<GateDependencies, SamePolygonError> {
        require_time(deadline)?;
        let context = &prepared.validation.context;
        let first_ticket = prepared.validation.first_ticket;
        let bound_filter = self.filter.reborrow(&prepared.validation.filter_context)?;
        let constructor_filter = if prepared.validation.dispatch.first_construction() {
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
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if route.request.kind == motion::RouteKind::HomeForceDestination {
            if prepared.validation.model.chase.is_some()
                || constructor_filter.is_none()
                || prepared.dependencies.prior.is_some()
            {
                return Err(Failure::Binding.into());
            }
            require_time(deadline)?;
            return Ok(GateDependencies {
                constructor_filter,
                gate: None,
            });
        }
        if matches!(
            route.request.kind,
            motion::RouteKind::Wander | motion::RouteKind::WanderPrepare
        ) {
            if self.wander_height.is_none() || prepared.validation.model.chase.is_some() {
                return Err(Failure::MissingGeometry.into());
            }
            require_time(deadline)?;
            return Ok(GateDependencies {
                constructor_filter,
                gate: None,
            });
        }
        let gate = LoadedChaseGate {
            profile: &self.profile,
            player: self.player,
            liquid: self.filter.dry_liquid(),
            vmap: &self.vmap,
            model: prepared
                .validation
                .model
                .chase
                .as_ref()
                .ok_or(Failure::Binding)?,
        }
        .query_with_initialized(
            context,
            first_ticket + 2,
            deadline,
            prepared.validation.initialized.as_ref(),
            |gate| observe(Observation::Gate(gate)),
        )?;
        if let ChaseGateSelection::NormalRouteUnavailable(reason) = gate.selection {
            return Err(SamePolygonError::GateRejected(reason));
        }
        require_time(deadline)?;
        Ok(GateDependencies {
            constructor_filter,
            gate: Some(gate),
        })
    }

    fn calculate_terminal(
        &mut self,
        prepared: &PreparedCalculation,
        dependencies: &GateDependencies,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<TerminalDependencies, SamePolygonError> {
        require_time(deadline)?;
        self.check_prepared(prepared)?;
        let context = &prepared.validation.context;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if route.request.kind == motion::RouteKind::Wander {
            return self.calculate_wander_terminal(prepared, dependencies, deadline, observe);
        }
        let home = route.request.kind == motion::RouteKind::HomeForceDestination;
        if dependencies.gate.is_some() == home {
            return Err(Failure::Binding.into());
        }
        if home && prepared.dependencies.prior.is_some() {
            return Err(Failure::Binding.into());
        }
        let prior = &prepared.dependencies.prior;
        let first_ticket = prepared.validation.first_ticket;
        let constructor_filter = &dependencies.constructor_filter;
        let bound_filter = self.filter.reborrow(&prepared.validation.filter_context)?;
        // PathFinder.cpp: destination/start checks and selection, owned ignore bit,
        // both HaveTile calls, then a fresh updateFilter. Constructor queries above
        // are separate observations, before TargetedMovementGenerator's Chase gate.
        let mut constructor = self.queries.reborrow(&prepared.dependencies.constructor)?;
        let selected = constructor.select_for_calculate(deadline)?;
        observe(Observation::SelectedQuery);
        let owner_ignore = match &prepared.validation.owner_ignore {
            Some(receipt) => receipt.clone(),
            None => route
                .require_owner_ignore_pathfinding(&self.profile)
                .map_err(|_| Failure::Binding)?
                .clone(),
        };
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
        let retained_filter = match prior {
            Some(prior) => prior.terminal.filter(),
            None => {
                let created = constructor_filter.as_ref().ok_or(Failure::Binding)?;
                SourceOwnerFilter {
                    include_flags: created.filter().include_flags(),
                    exclude_flags: created.filter().exclude_flags(),
                }
            }
        };
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
        let terminal = if home {
            let terminal = selected.loaded_home_cold_same_polygon_with_filter(
                SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
                filter.filter(),
                config.normalization,
                config.cold_budget,
                deadline,
            )?;
            terminal
                .check_query(&selected)
                .map_err(|_| Failure::Binding)?;
            SamePolygonTerminal::Home(terminal)
        } else {
            let terminal = if let Some(prior) = &prior {
                selected.loaded_retained_same_polygon(
                    prior.terminal.chase().ok_or(Failure::Binding)?,
                    config.normalization,
                    config.retained_budget,
                    deadline,
                )?
            } else {
                selected.loaded_cold_same_polygon_with_filter(
                    SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
                    filter.filter(),
                    config.normalization,
                    config.cold_budget,
                    deadline,
                )?
            };
            terminal
                .check_query(&selected)
                .map_err(|_| Failure::Binding)?;
            SamePolygonTerminal::Chase(terminal)
        };
        observe(Observation::Terminal);
        if terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
            || terminal.actual_end() != bits(route.request.goal)
            || terminal.path_type() != 1
            || terminal.active_len() != 1
            || terminal.filter() != filter.filter()
        {
            return Err(Failure::Binding.into());
        }
        self.retain_terminal(
            prepared,
            terminal,
            None,
            Some(owner_ignore),
            Some(tiles),
            filter,
            deadline,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn retain_terminal(
        &mut self,
        prepared: &PreparedCalculation,
        terminal: SamePolygonTerminal,
        wander_tail: Option<wander::WanderTail>,
        owner_ignore: Option<OwnerIgnorePathfindingReceipt>,
        tiles: Option<LoadedSourceTilePresence>,
        filter: RouteFilterUpdateDependency,
        deadline: Instant,
    ) -> Result<TerminalDependencies, SamePolygonError> {
        let context = &prepared.validation.context;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let npc = route.owner.npc();
        let prior = &prepared.dependencies.prior;
        let event = &prepared.validation.dispatch;
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
            poly_refs: terminal.storage().to_vec(),
            active_len: u16::try_from(terminal.active_len()).map_err(|_| Failure::Invalid)?,
            point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
            path_type: terminal.path_type(),
            requested_end: super::source_near_point::exact_point(terminal.actual_end())?,
            include_flags: filter.filter().include_flags,
            exclude_flags: filter.filter().exclude_flags,
        };
        after_cache
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        require_time(deadline)?;
        let index = self
            .candidates
            .iter()
            .position(|entry| {
                entry.npc == npc
                    && entry.construction == prepared.validation.dispatch.construction_binding()
            })
            .ok_or(Failure::Binding)?;
        let wander_prefix = if terminal.wander().is_some() {
            Some(self.wander_prefix(&prepared.validation)?)
        } else {
            None
        };
        let entry = &mut self.candidates[index];
        let retry = entry
            .proposals
            .iter()
            .position(|candidate| candidate.context == *context);
        let candidate = if let Some(index) = retry {
            let previous = &entry.proposals[index];
            if previous.after_cache != after_cache
                || !previous.terminal.same_observations(&terminal)
                || previous.terminal.points() != terminal.points()
                || previous.wander_tail != wander_tail
            {
                return Err(Failure::Binding.into());
            }
            Rc::clone(previous)
        } else {
            if entry.proposals.len() >= MAX_CANDIDATES_PER_GENERATOR {
                return Err(SamePolygonError::CandidateBudget);
            }
            let candidate = Rc::new(SamePolygonCandidate {
                context: context.clone(),
                dispatch: event.clone(),
                terminal,
                wander_tail,
                wander_prefix,
                after_cache,
            });
            entry.proposals.push(Rc::clone(&candidate));
            candidate
        };
        // Repeated calculations reuse the immutable native candidate but return
        // fresh callback/constructor/filter observations, including its live seal.
        require_time(deadline)?;
        Ok(TerminalDependencies {
            owner_ignore,
            tiles,
            filter,
            candidate,
        })
    }

    fn assemble_proposal(
        prepared: PreparedCalculation,
        gate: GateDependencies,
        terminal: TerminalDependencies,
    ) -> SamePolygonProposal {
        SamePolygonProposal {
            prior: prepared.dependencies.prior,
            context: prepared.validation.context,
            dispatch: prepared.validation.dispatch,
            filter_context: prepared.validation.filter_context,
            constructor: prepared.dependencies.constructor,
            constructor_filter: gate.constructor_filter,
            model: prepared.validation.model,
            gate: gate.gate,
            owner_ignore: terminal.owner_ignore,
            tiles: terminal.tiles,
            filter: terminal.filter,
            candidate: terminal.candidate,
            #[cfg(test)]
            timing: None,
        }
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

#[cfg(test)]
#[path = "source_route_same_polygon/staged_tests.rs"]
mod staged_tests;

#[cfg(test)]
#[path = "source_route_same_polygon/two_owner_tests.rs"]
pub(super) mod two_owner_tests;

#[cfg(test)]
#[path = "source_route_same_polygon/dry_habitat_tests.rs"]
pub(super) mod dry_habitat_tests;
