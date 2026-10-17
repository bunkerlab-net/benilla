// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact loaded Chase straight selection for the explicit dry, warm-cache domain.
//! A rejected gate exposes unavailable normal routing, never a fabricated path fact.
use super::{
    core::{Failure, PlayerEnvironmentPolicy, Session},
    products::{
        dry_route::DryRouteProjection,
        liquid::{
            DryLiquidSnapshot, LiquidReply, LiquidRequest, LiquidStatus,
            SnapshotBinding as LiquidBinding,
        },
        navigation::{SnapshotBinding as NavigationBinding, WarmCacheLineage},
        vmap::{RayReply, RayRequest, SnapshotBinding as VmapBinding, StaticRay, VmapSnapshot},
    },
    source_route_straight_path::{
        self, BoundStraightPathQuery, SourceRouteStraightPathProvider, StraightPathBinding,
        StraightPathConfiguration, StraightPathDependency, StraightPathRequest,
    },
};
use benilla_sim::{chase_gate, creature_init::ModelSelectionReceipt, creature_motion::RouteKind};
use benilla_world_runtime::{
    native::{
        AuthorityRequest, CreatureCollisionHeightWitness, CreatureCollisionWidthWitness,
        CreatureRouteRequest, MotionQuery, NativeError, PlayerCollisionHeightWitness,
        PlayerEnvironmentTarget, Profile, Query, CREATURE_COLLISION_HEIGHT_POLICY,
        PLAYER_COLLISION_HEIGHT_POLICY, PLAYER_ENVIRONMENT_POLICY,
    },
    PlayerId,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Explicit source LOS selection; no implicit renderer setting or default enable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChaseGateConfiguration {
    pub straight: StraightPathConfiguration,
    pub line_of_sight_enabled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChaseGateBinding {
    pub straight: StraightPathBinding,
    pub vmap: VmapBinding,
    pub configuration: ChaseGateConfiguration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChaseGateRequest {
    pub snapshot: ChaseGateBinding,
    pub context: AuthorityRequest,
    pub first_ticket: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChaseGateRejection {
    Distance,
    Vertical,
    LineOfSight,
    OwnerWater,
    TargetWater,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChaseGateSelection {
    Straight,
    /// Source selects normal calculate; that independent route remains unavailable.
    NormalRouteUnavailable(ChaseGateRejection),
}
/// The actual selected model receipt authenticates reach independently of height.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChaseGateModel {
    pub selection: ModelSelectionReceipt,
    pub width: CreatureCollisionWidthWitness,
    pub owner_height: CreatureCollisionHeightWitness,
    pub target_height: PlayerCollisionHeightWitness,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChaseGateOwnerWater {
    pub reply: LiquidReply,
    pub required_type: u8,
    pub collision_height_bits: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ChaseGateDependency {
    request: ChaseGateRequest,
    model: ChaseGateModel,
    selection: ChaseGateSelection,
    line_of_sight: Option<RayReply>,
    owner_water: Option<ChaseGateOwnerWater>,
    target_water: Option<PlayerEnvironmentTarget>,
    straight: Option<StraightPathDependency>,
}
impl ChaseGateDependency {
    pub(crate) fn request(&self) -> &ChaseGateRequest {
        &self.request
    }
    pub(crate) fn model(&self) -> &ChaseGateModel {
        &self.model
    }
    pub(crate) fn selection(&self) -> ChaseGateSelection {
        self.selection
    }
    pub(crate) fn line_of_sight(&self) -> Option<RayReply> {
        self.line_of_sight
    }
    pub(crate) fn owner_water(&self) -> Option<ChaseGateOwnerWater> {
        self.owner_water
    }
    pub(crate) fn target_water(&self) -> Option<&PlayerEnvironmentTarget> {
        self.target_water.as_ref()
    }
    pub(crate) fn straight_dependency(&self) -> Option<&StraightPathDependency> {
        self.straight.as_ref()
    }
}

pub(crate) struct SourceRouteChaseGateProvider {
    straight: SourceRouteStraightPathProvider,
    profile: Profile,
    player: PlayerId,
    vmap: VmapSnapshot,
    configuration: ChaseGateConfiguration,
}
pub(crate) struct BoundChaseGateQuery<'a> {
    provider: &'a SourceRouteChaseGateProvider,
    straight: BoundStraightPathQuery<'a>,
    context: AuthorityRequest,
    model: ChaseGateModel,
}
impl SourceRouteChaseGateProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        projection: DryRouteProjection,
        lineage: WarmCacheLineage,
        configuration: ChaseGateConfiguration,
    ) -> Result<Self, Failure> {
        if !configuration.line_of_sight_enabled {
            return Err(Failure::UnsupportedQuery);
        }
        let selected = PlayerEnvironmentPolicy::from_profile(profile, player)?;
        selected.check(session)?;
        if session.player_environment.as_ref() != Some(&selected) {
            return Err(Failure::UnsupportedQuery);
        }
        let (liquid, navigation, vmap) = projection.into_parts();
        let b = vmap.binding();
        let environment = profile
            .player_environment
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?
            .snapshot_binding();
        if environment.map != b.map
            || environment.content != b.content
            || environment.source_epoch != b.source_epoch
            || environment.inventory != b.inventory
        {
            return Err(Failure::Binding);
        }
        let straight = SourceRouteStraightPathProvider::bind_with_chase_gate(
            session,
            profile,
            liquid,
            navigation,
            b,
            lineage,
            configuration,
        )?;
        Ok(Self {
            straight,
            profile: profile.clone(),
            player,
            vmap,
            configuration,
        })
    }
    pub(crate) fn binding(&self) -> ChaseGateBinding {
        ChaseGateBinding {
            straight: self.straight.binding(),
            vmap: self.vmap.binding(),
            configuration: self.configuration,
        }
    }
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundChaseGateQuery<'_>, Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if !matches!(
            route.request.kind,
            RouteKind::ChaseInitial | RouteKind::ChaseReplan
        ) {
            return Err(Failure::UnsupportedQuery);
        }
        // This checks the entire session (including selected environment), owner,
        // current pose, query generation/tick, source-bound dry capabilities and warm cache.
        let straight = self.straight.begin(session, context)?;
        let model = gate_model(&self.profile, session.world, self.player, route)?;
        Ok(BoundChaseGateQuery {
            provider: self,
            straight,
            context: context.clone(),
            model,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Observation {
    LineOfSight(RayReply),
    OwnerWater(ChaseGateOwnerWater),
    TargetWater(bool),
    Straight,
}
impl BoundChaseGateQuery<'_> {
    pub(crate) fn query(
        &self,
        request: ChaseGateRequest,
        deadline: Instant,
    ) -> Result<ChaseGateDependency, Failure> {
        self.query_observed(request, deadline, |_| {})
    }
    /// Observation is after each real query and cannot inject a predicate answer.
    fn query_observed(
        &self,
        request: ChaseGateRequest,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<ChaseGateDependency, Failure> {
        if request.snapshot != self.provider.binding()
            || request.context != self.context
            || request.first_ticket == 0
        {
            return Err(Failure::Binding);
        }
        // LOS and owner-water precede all nine calculate/retry tickets.
        request
            .first_ticket
            .checked_add(10)
            .ok_or(Failure::Invalid)?;
        require_time(deadline)?;
        let observed = LoadedChaseGate {
            profile: &self.provider.profile,
            player: self.provider.player,
            liquid: self.provider.straight.dry_liquid(),
            vmap: &self.provider.vmap,
            model: &self.model,
        }
        .query(
            &request.context,
            request.first_ticket,
            deadline,
            &mut observe,
        )?;
        let mut result = ChaseGateDependency {
            request: request.clone(),
            model: self.model.clone(),
            selection: observed.selection,
            line_of_sight: observed.line_of_sight,
            owner_water: observed.owner_water,
            target_water: observed.target_water,
            straight: None,
        };
        if result.selection != ChaseGateSelection::Straight {
            return Ok(result);
        }
        result.straight = Some(self.straight.query(
            StraightPathRequest {
                snapshot: request.snapshot.straight,
                context: request.context,
                first_ticket: request.first_ticket + 2,
            },
            deadline,
        )?);
        observe(Observation::Straight);
        require_time(deadline)?;
        Ok(result)
    }
}
/// Shared gate-only observation path. It executes the same actual loaded LOS,
/// fresh owner liquid and owned target cache before either calculate backend.
pub(super) struct LoadedChaseGate<'a> {
    pub profile: &'a Profile,
    pub player: PlayerId,
    pub liquid: &'a DryLiquidSnapshot,
    pub vmap: &'a VmapSnapshot,
    pub model: &'a ChaseGateModel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChaseGateObservations {
    pub selection: ChaseGateSelection,
    pub line_of_sight: Option<RayReply>,
    pub owner_water: Option<ChaseGateOwnerWater>,
    pub target_water: Option<PlayerEnvironmentTarget>,
}
impl ChaseGateObservations {
    fn reject(mut self, reason: ChaseGateRejection) -> Self {
        self.selection = ChaseGateSelection::NormalRouteUnavailable(reason);
        self
    }
}
pub(super) fn gate_model(
    profile: &Profile,
    world: [u8; 16],
    player: PlayerId,
    route: &CreatureRouteRequest,
) -> Result<ChaseGateModel, Failure> {
    gate_model_with_initialized(profile, world, player, route, None)
}
pub(super) fn gate_model_with_initialized(
    profile: &Profile,
    world: [u8; 16],
    player: PlayerId,
    route: &CreatureRouteRequest,
    initialized: Option<&super::source_route_filter::InitializedOwnerValidation>,
) -> Result<ChaseGateModel, Failure> {
    if let Some(initialized) = initialized {
        let validated = initialized.for_route(profile, route)?;
        let target_height = initialized
            .target_height_for(profile, route, world, player)
            .map_err(native_failure)?;
        return Ok(ChaseGateModel {
            selection: validated.selection.clone(),
            width: validated.width.clone(),
            owner_height: validated.height.clone(),
            target_height: target_height.clone(),
        });
    }
    let width = route
        .require_collision_width(profile)
        .map_err(native_failure)?;
    let selection = width
        .validated_model_selection(profile, route.owner.npc(), route.owner.initialization())
        .map_err(native_failure)?;
    let owner_height = route
        .require_collision_height(profile)
        .map_err(native_failure)?;
    let target_height = route
        .require_target_collision_height(profile, world, player)
        .map_err(native_failure)?;
    Ok(ChaseGateModel {
        selection,
        width: width.clone(),
        owner_height: owner_height.clone(),
        target_height: target_height.clone(),
    })
}
impl LoadedChaseGate<'_> {
    pub(super) fn query(
        &self,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        observe: impl FnMut(Observation),
    ) -> Result<ChaseGateObservations, Failure> {
        self.query_with_initialized(context, first_ticket, deadline, None, observe)
    }
    pub(super) fn query_with_initialized(
        &self,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        initialized: Option<&super::source_route_filter::InitializedOwnerValidation>,
        mut observe: impl FnMut(Observation),
    ) -> Result<ChaseGateObservations, Failure> {
        if first_ticket == 0 {
            return Err(Failure::Binding);
        }
        first_ticket.checked_add(1).ok_or(Failure::Invalid)?;
        require_time(deadline)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if !matches!(
            route.request.kind,
            RouteKind::ChaseInitial | RouteKind::ChaseReplan
        ) {
            return Err(Failure::UnsupportedQuery);
        }
        let bits = |p: benilla_sim::creature_ai::Point| [p.x.bits(), p.y.bits(), p.z.bits()];
        let owner = bits(route.owner_position);
        let goal = bits(route.request.goal);
        let mut result = ChaseGateObservations {
            selection: ChaseGateSelection::Straight,
            line_of_sight: None,
            owner_water: None,
            target_water: None,
        };
        let numeric =
            chase_gate::numeric_gate_bits(owner, goal, self.model.selection.combat_reach().bits())
                .map_err(|_| Failure::Invalid)?;
        match numeric {
            chase_gate::NumericGate::Distance => {
                return Ok(result.reject(ChaseGateRejection::Distance))
            }
            chase_gate::NumericGate::Vertical => {
                return Ok(result.reject(ChaseGateRejection::Vertical))
            }
            chase_gate::NumericGate::Pass => (),
        }
        let [from, to] = chase_gate::los_endpoints_bits(
            owner,
            goal,
            self.model.owner_height.height().bits(),
            self.model.target_height.height().bits(),
        )
        .map_err(|_| Failure::Invalid)?;
        let ray = RayRequest {
            snapshot: self.vmap.binding(),
            ticket: first_ticket,
            from,
            to,
            ignore_m2: false,
        };
        let reply = self.vmap.query(ray, deadline).map_err(Failure::product)?;
        if reply.request != ray {
            return Err(Failure::Binding);
        }
        result.line_of_sight = Some(reply);
        observe(Observation::LineOfSight(reply));
        require_time(deadline)?;
        if !matches!(reply.result, StaticRay::Miss) {
            return Ok(result.reject(ChaseGateRejection::LineOfSight));
        }
        let liquid = LiquidRequest {
            snapshot: self.liquid.binding(),
            ticket: first_ticket + 1,
            position: owner,
        };
        let reply = self
            .liquid
            .query(liquid, deadline)
            .map_err(Failure::product)?;
        if reply.request != liquid {
            return Err(Failure::Binding);
        }
        let water = ChaseGateOwnerWater {
            reply,
            required_type: chase_gate::LIQUID_MASK,
            collision_height_bits: chase_gate::LIQUID_COLLISION_HEIGHT_BITS,
        };
        result.owner_water = Some(water);
        observe(Observation::OwnerWater(water));
        require_time(deadline)?;
        let status = match reply.status {
            LiquidStatus::NoWater => 0,
        };
        if chase_gate::terrain_is_in_water(status) {
            return Ok(result.reject(ChaseGateRejection::OwnerWater));
        }
        // Player::IsInWater reads its current owned cache; never query target XYZ.
        let target = match initialized {
            Some(validated) => {
                validated.target_water_for(self.profile, route, context.binding.world, self.player)
            }
            None => route.require_target_water(self.profile, context.binding.world, self.player),
        }
        .map_err(native_failure)?;
        result.target_water = Some(target.clone());
        observe(Observation::TargetWater(target.in_water()));
        require_time(deadline)?;
        if target.in_water() {
            return Ok(result.reject(ChaseGateRejection::TargetWater));
        }
        Ok(result)
    }
}
fn require_time(deadline: Instant) -> Result<(), Failure> {
    if Instant::now() >= deadline {
        Err(Failure::Deadline)
    } else {
        Ok(())
    }
}
fn native_failure(error: NativeError) -> Failure {
    match error {
        NativeError::AuthorityUnavailable(_) => Failure::MissingGeometry,
        _ => Failure::Binding,
    }
}
/// The explicit dry projection keeps the original inventory/epoch and distinct policies.
pub(crate) fn geometry_revision(
    liquid: LiquidBinding,
    navigation: NavigationBinding,
    vmap: VmapBinding,
    configuration: ChaseGateConfiguration,
) -> Result<[u8; 32], Failure> {
    if !configuration.line_of_sight_enabled {
        return Err(Failure::UnsupportedQuery);
    }
    if liquid.map != vmap.map
        || navigation.map != vmap.map
        || liquid.content != vmap.content
        || navigation.content != vmap.content
        || liquid.source_epoch != vmap.source_epoch
        || navigation.source_epoch != vmap.source_epoch
        || liquid.inventory != vmap.inventory
        || navigation.inventory != vmap.inventory
        || vmap.source_epoch == [0; 32]
    {
        return Err(Failure::Binding);
    }
    let mut hash = Sha256::new();
    hash.update(b"benilla-source-dry-warm-chase-owned-ignore-pathfinding-tile-straight-gate-v3");
    hash.update(source_route_straight_path::geometry_revision(
        liquid,
        navigation,
        configuration.straight,
    )?);
    hash.update(vmap.policy);
    hash.update([u8::from(configuration.line_of_sight_enabled)]);
    for policy in [
        chase_gate::POLICY,
        CREATURE_COLLISION_HEIGHT_POLICY,
        PLAYER_COLLISION_HEIGHT_POLICY,
        PLAYER_ENVIRONMENT_POLICY,
    ] {
        hash.update((policy.len() as u64).to_le_bytes());
        hash.update(policy.as_bytes());
    }
    for source in chase_gate::SOURCE_SHA256 {
        hash.update(source.as_bytes());
    }
    hash.update(
        b"goal-not-target-pose;ignore-m2-false;fresh-owner-mask15;cached-player;normal-unavailable",
    );
    Ok(hash.finalize().into())
}
#[cfg(test)]
#[path = "source_route_chase_gate/tests.rs"]
mod tests;
