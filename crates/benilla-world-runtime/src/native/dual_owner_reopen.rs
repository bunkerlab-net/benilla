// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded synthetic restore of ordinary Chase and its suspended NORMAL Wander.
//! The same fresh native query must prove both independent constructor states;
//! app inventory, historical geometry and atomic publication remain app duties.
use super::wander_reopen;
use super::*;
use benilla_native_nav::{
    ReconstructedSourceWanderAndChase, SourceChaseFixedCacheChallenge, SourceChaseSemanticState,
    SourceMapQuerySlot, SourceOwnerFilter, SourceSamePolygonConfig,
    SourceWanderReconstructionChallenge, SourceWanderSemanticState,
};
use benilla_sim::creature_motion as motion;

/// One unpublished exact checkpoint and two typed fresh replay challenges.
/// No world access, deserialization or arbitrary restore permission is provided.
///
/// ```compile_fail
/// use benilla_world_runtime::native::{NativeWorld, PendingWanderAndChaseReopen};
/// fn bypass(pending: PendingWanderAndChaseReopen) -> NativeWorld { pending.world }
/// ```
pub struct PendingWanderAndChaseReopen {
    world: NativeWorld,
    checkpoint: Checkpoint,
    owner: crate::NpcId,
    wander: WanderHead,
    chase: ChaseHead,
    wander_challenge: SourceWanderReconstructionChallenge,
    chase_challenge: SourceChaseFixedCacheChallenge,
}
struct WanderHead {
    construction: motion::QueryBinding,
    cache: motion::PathFinderCache,
    request: motion::RouteRequest,
    semantic: SourceWanderSemanticState,
}
struct ChaseHead {
    construction: motion::QueryBinding,
    cache: motion::PathFinderCache,
    request: motion::RouteRequest,
    semantic: SourceChaseSemanticState,
}
impl PendingWanderAndChaseReopen {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }
    pub fn owner(&self) -> crate::NpcId {
        self.owner
    }
    pub fn wander_construction(&self) -> motion::QueryBinding {
        self.wander.construction
    }
    pub fn chase_construction(&self) -> motion::QueryBinding {
        self.chase.construction
    }
    pub fn wander_cache(&self) -> &motion::PathFinderCache {
        &self.wander.cache
    }
    pub fn chase_cache(&self) -> &motion::PathFinderCache {
        &self.chase.cache
    }
    pub fn wander_challenge(&self) -> &SourceWanderReconstructionChallenge {
        &self.wander_challenge
    }
    pub fn chase_challenge(&self) -> &SourceChaseFixedCacheChallenge {
        &self.chase_challenge
    }

    /// Compare inert saved metadata; never mint or claim a live dispatch seal.
    pub fn validate_wander_operation(&self, context: &AuthorityRequest) -> Result<(), NativeError> {
        wander_reopen::validate_wander_operation(
            &self.world,
            self.owner,
            self.wander.construction,
            &self.wander.request,
            context,
        )?;
        validate_prior_cache(context, &self.wander.cache)
    }
    pub fn validate_wander_head(&self, context: &AuthorityRequest) -> Result<(), NativeError> {
        self.validate_wander_operation(context)?;
        wander_reopen::validate_wander_head_request(route(context)?, &self.wander.request)
    }
    pub fn validate_wander_draws(
        &self,
        draws: &[Draw; 2],
        request: &motion::WanderRequest,
    ) -> Result<(), NativeError> {
        wander_reopen::validate_wander_draws(
            &self.world,
            self.owner,
            self.wander.construction,
            draws,
            request,
        )
    }
    /// Validate historical ordinary Chase identity, model, dimensions and target
    /// without promoting saved constructor metadata into live authority.
    pub fn validate_chase_operation(&self, context: &AuthorityRequest) -> Result<(), NativeError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(NativeError::AuthorityBinding);
        };
        let q = &route.request;
        let receipt = context.validate_saved_pathfinder_dispatch(&self.world.profile)?;
        let npc = self
            .world
            .state
            .npcs
            .get(&self.owner.spawn)
            .ok_or(NativeError::AuthorityBinding)?;
        let target = q.target.as_ref().ok_or(NativeError::AuthorityBinding)?;
        let source = self
            .world
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        let now = context
            .binding
            .tick
            .checked_mul(u64::from(self.world.profile.tick_ms))
            .and_then(|elapsed| self.world.profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(NativeError::AuthorityBinding)?;
        if !matches!(
            q.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) || q.wander.is_some()
            || q.launch_origin != q.origin
            || q.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || context.binding.world != self.world.state.world
            || context.binding.npc != Some(self.owner)
            || context.binding.tick == 0
            || context.binding.tick > self.world.state.tick
            || receipt.npc() != self.owner
            || receipt.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || receipt.construction_binding() != self.chase.construction
            || receipt.dispatch_binding() != q.binding
            || q.binding.query_id > self.chase.request.binding.query_id
            || q.binding.now_ms != now
            || q.binding.now_ms > self.chase.request.binding.now_ms
            || receipt.first_construction() != q.prior_cache.is_none()
            || target.id != SOLO_UNIT
            || target.bounding_radius != self.world.profile.player.bounding_radius
            || target.combat_reach != self.world.profile.player.combat_reach
            || target.collision_height != source.player_collision_height
            || npc
                .source_capabilities
                .as_ref()
                .map(CreatureCapabilityState::witness)
                != Some(&route.owner)
            || npc
                .source_collision_width
                .as_ref()
                .map(CreatureCollisionWidthState::witness)
                != route.collision_width.as_ref()
            || npc
                .source_collision_height
                .as_ref()
                .map(CreatureCollisionHeightState::witness)
                != route.collision_height.as_ref()
        {
            return Err(NativeError::AuthorityBinding);
        }
        let height = route.require_collision_height(&self.world.profile)?;
        if q.owner_collision_height.bits() != height.height().bits() {
            return Err(NativeError::AuthorityBinding);
        }
        route.require_collision_width(&self.world.profile)?;
        route.require_target_collision_height(
            &self.world.profile,
            self.world.state.world,
            self.world.state.player,
        )?;
        target.pose.validate()?;
        validate_prior_cache(context, &self.chase.cache)
    }
    pub fn validate_chase_head(&self, context: &AuthorityRequest) -> Result<(), NativeError> {
        self.validate_chase_operation(context)?;
        if route(context)? != &self.chase.request {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    /// Comparison-only preflight for the app's private atomic installation plan.
    pub fn validate_pair(
        &self,
        pair: &ReconstructedSourceWanderAndChase,
    ) -> Result<(), NativeError> {
        validate_pair(
            pair,
            &self.wander_challenge,
            &self.chase_challenge,
            self.wander.cache.revision,
            &self.wander.semantic,
            self.chase.cache.revision,
            &self.chase.semantic,
        )?;
        if self.world.checkpoint()? != self.checkpoint {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
    /// Preserve the opaque same-allocation pair until the app has bound its
    /// inventoried projection and can install both constructors atomically.
    pub fn finish(
        self,
        pair: ReconstructedSourceWanderAndChase,
    ) -> Result<(NativeWorld, ReconstructedSourceWanderAndChase), NativeError> {
        self.validate_pair(&pair)?;
        Ok((self.world, pair))
    }
}
fn route(context: &AuthorityRequest) -> Result<&motion::RouteRequest, NativeError> {
    match &context.query {
        Query::SourceMotion(MotionQuery::Route(route)) => Ok(&route.request),
        _ => Err(NativeError::AuthorityBinding),
    }
}
fn validate_prior_cache(
    context: &AuthorityRequest,
    cache: &motion::PathFinderCache,
) -> Result<(), NativeError> {
    if let Some(prior) = route(context)?.prior_cache.as_deref() {
        prior.validate_for(cache.binding.identity)?;
        if prior.binding != cache.binding
            || prior.source != cache.source
            || prior.include_flags != cache.include_flags
            || prior.exclude_flags != cache.exclude_flags
            || prior.revision >= cache.revision
        {
            return Err(NativeError::AuthorityBinding);
        }
    }
    Ok(())
}
fn validate_pair(
    pair: &ReconstructedSourceWanderAndChase,
    wander_challenge: &SourceWanderReconstructionChallenge,
    chase_challenge: &SourceChaseFixedCacheChallenge,
    wander_revision: u64,
    wander_semantic: &SourceWanderSemanticState,
    chase_revision: u64,
    chase_semantic: &SourceChaseSemanticState,
) -> Result<(), NativeError> {
    pair.check_query(pair.query())
        .map_err(|_| NativeError::AuthorityBinding)?;
    wander_reopen::validate_certificate(
        pair.wander(),
        wander_challenge,
        wander_revision,
        wander_semantic,
    )?;
    if !pair.matches_challenges(wander_challenge, chase_challenge)
        || pair.chase().witness().operation_count() != chase_revision
        || pair.chase().terminal().chase_semantic_state() != *chase_semantic
    {
        return Err(NativeError::AuthorityBinding);
    }
    Ok(())
}
impl NativeWorld {
    /// Restore only a current ordinary Chase calculation and its suspended NORMAL
    /// Wander head inside an unpublished pending value. Generic restore stays closed.
    pub fn prepare_synthetic_wander_and_chase_reopen(
        profile: Profile,
        checkpoint: Checkpoint,
    ) -> Result<PendingWanderAndChaseReopen, NativeError> {
        checkpoint.state.validate_history_budget()?;
        checkpoint.validate_encoded_budget()?;
        if checkpoint.state.domain != Domain::SyntheticComponentTest {
            return Err(NativeError::Incompatible);
        }
        let (permit, wander, chase) = pending_shape(&profile, &checkpoint)?;
        let world = Self::restore_synthetic_checkpoint(profile, checkpoint.clone(), Some(&permit))?;
        if world.checkpoint()? != checkpoint {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(PendingWanderAndChaseReopen {
            world,
            checkpoint,
            owner: permit.npc,
            wander,
            chase,
            wander_challenge: SourceWanderReconstructionChallenge::new(),
            chase_challenge: SourceChaseFixedCacheChallenge::new(),
        })
    }
}
fn unsupported() -> NativeError {
    NativeError::UnsupportedMechanic("one current NORMAL Chase with suspended NORMAL Wander")
}
fn bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
fn pending_shape(
    profile: &Profile,
    checkpoint: &Checkpoint,
) -> Result<(PendingMotionRestore, WanderHead, ChaseHead), NativeError> {
    let configuration = profile.pathfinder_configuration().ok_or_else(unsupported)?;
    let slot = SourceMapQuerySlot {
        map_id: profile.map_id,
        instance_id: configuration.selected().source_instance_id,
    };
    let config = SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    };
    let mut selected = None;
    for npc in checkpoint.state.npcs.values() {
        let Some(source) = &npc.source_motion else {
            continue;
        };
        let snapshot = &source.motion;
        if matches!(snapshot.generator, motion::Generator::Idle)
            && snapshot.spline.is_none()
            && snapshot.suspended_wander.is_none()
        {
            source.validate_saved_session_reopen()?;
            continue;
        }
        if selected.is_some() {
            return Err(unsupported());
        }
        let motion::Generator::Chase(chase) = &snapshot.generator else {
            return Err(unsupported());
        };
        let wander = snapshot.suspended_wander.as_ref().ok_or_else(unsupported)?;
        let wander_cache = wander.pathfinder_cache.as_deref().ok_or_else(unsupported)?;
        let chase_cache = chase.pathfinder_cache.as_deref().ok_or_else(unsupported)?;
        let wander_head = suspended_wander_head(snapshot, wander, wander_cache)?;
        let chase_head = current_chase_head(snapshot, chase, chase_cache)?;
        let chase_construction = chase.pathfinder_construction.ok_or_else(unsupported)?;
        let wander_construction = wander.pathfinder_construction;
        if wander_construction.identity != snapshot.identity
            || chase_construction.identity != snapshot.identity
            || wander_construction.query_id >= chase_construction.query_id
            || wander_construction.now_ms > chase_construction.now_ms
            || wander_head.request.binding.query_id >= chase_construction.query_id
            || wander_head.request.binding.now_ms > chase_construction.now_ms
            || wander_head.request.owner_collision_height
                != chase_head.route_request.owner_collision_height
        {
            return Err(NativeError::AuthorityBinding);
        }
        for (request, cache) in [
            (&wander_head.request, wander_cache),
            (&chase_head.route_request, chase_cache),
        ] {
            validate_head_binding(profile, checkpoint, snapshot, request)?;
            if cache.binding.owner_generation != npc.id.generation
                || cache.binding.navigation_policy != profile.geometry_revision
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        let filter = |cache: &motion::PathFinderCache| SourceOwnerFilter {
            include_flags: cache.include_flags,
            exclude_flags: cache.exclude_flags,
        };
        let wander_semantic = SourceWanderSemanticState {
            slot,
            query_capacity: 1024,
            config,
            filter: filter(wander_cache),
            from: bits(wander_head.request.origin),
            raw_end: bits(wander_head.request.goal),
            actual_end: bits(wander_cache.requested_end),
            storage: wander_cache.poly_refs.clone(),
            active_len: usize::from(wander_cache.active_len),
            path_type: wander_cache.path_type,
            points: wander_head.route.points.iter().copied().map(bits).collect(),
            no_path_reason: None,
        };
        let chase_semantic = SourceChaseSemanticState {
            query_policy: benilla_native_nav::SOURCE_MAP_QUERY_POLICY,
            slot,
            query_capacity: 1024,
            compatibility_id: benilla_native_nav::SOURCE_COLD_DISTANCE_COMPATIBILITY_ID,
            compatibility_version: benilla_native_nav::SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
            source_revision: benilla_native_nav::SOURCE_COLD_DISTANCE_COMPATIBILITY_REVISION,
            config,
            filter: filter(chase_cache),
            point_limit: usize::from(chase_cache.point_limit),
            initial_storage: vec![0; usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)],
            initial_active_len: 0,
            retained: chase_cache.revision > 1,
            from: bits(chase_head.route_request.origin),
            to: bits(chase_head.route_request.goal),
            actual_end: bits(chase_cache.requested_end),
            storage: chase_cache.poly_refs.clone(),
            active_len: usize::from(chase_cache.active_len),
            path_type: chase_cache.path_type,
            points: chase_head.route.points.iter().copied().map(bits).collect(),
        };
        selected = Some((
            PendingMotionRestore {
                npc: npc.id,
                snapshot: source.clone(),
            },
            WanderHead {
                construction: wander_construction,
                cache: wander_cache.clone(),
                request: wander_head.request.clone(),
                semantic: wander_semantic,
            },
            ChaseHead {
                construction: chase_construction,
                cache: chase_cache.clone(),
                request: chase_head.route_request.clone(),
                semantic: chase_semantic,
            },
        ));
    }
    selected.ok_or_else(unsupported)
}
fn validate_head_binding(
    profile: &Profile,
    checkpoint: &Checkpoint,
    snapshot: &motion::MotionSnapshot,
    request: &motion::RouteRequest,
) -> Result<(), NativeError> {
    let binding = request.binding;
    if profile.tick_ms == 0
        || binding.identity != snapshot.identity
        || binding.query_id > snapshot.query_id
        || binding.now_ms <= profile.clock_origin_ms
        || binding.now_ms > checkpoint.state.now_ms
        || !(binding.now_ms - profile.clock_origin_ms).is_multiple_of(u64::from(profile.tick_ms))
    {
        return Err(NativeError::AuthorityBinding);
    }
    request.validate_pathfinder_dispatch()?;
    Ok(())
}
fn normal_calculation(
    request: &motion::RouteRequest,
    route: &motion::CompleteRoute,
    cache: &motion::PathFinderCache,
) -> Result<(), NativeError> {
    if cache.source != motion::RouteSource::OwnerPathFinder
        || cache.path_type != 1
        || cache.active_len != 1
        || cache.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
        || route.source != cache.source
        || route.points.len() != 2
        || request
            .prior_cache
            .as_deref()
            .map_or(0, |prior| prior.revision)
            .checked_add(1)
            != Some(cache.revision)
    {
        return Err(unsupported());
    }
    motion::RouteCalculation {
        wander_no_path: None,
        source: route.source,
        outcome: motion::RouteOutcome::Complete(route.clone()),
        prior_cache: request.prior_cache.clone(),
        after_cache: cache.clone(),
    }
    .validate_for(request)?;
    Ok(())
}
fn current_chase_head<'a>(
    snapshot: &'a motion::MotionSnapshot,
    chase: &motion::ChaseSnapshot,
    cache: &motion::PathFinderCache,
) -> Result<&'a motion::ActiveSplineSnapshot, NativeError> {
    let spline = snapshot.spline.as_ref().ok_or_else(unsupported)?;
    let request = &spline.route_request;
    if chase.mode != motion::ChaseMode::Normal
        || chase.target != SOLO_UNIT
        || !matches!(
            request.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        )
        || request
            .target
            .as_ref()
            .is_none_or(|target| target.id != chase.target)
        || request.launch_origin != request.origin
        || request.wander.is_some()
        || spline.route_source != motion::RouteSource::OwnerPathFinder
        || spline.local_home_cache.is_some()
        || request.pathfinder_dispatch.as_ref().is_none_or(|dispatch| {
            dispatch.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
                || Some(dispatch.construction_binding()) != chase.pathfinder_construction
        })
    {
        return Err(unsupported());
    }
    normal_calculation(request, &spline.route, cache)?;
    Ok(spline)
}
fn suspended_wander_head<'a>(
    snapshot: &motion::MotionSnapshot,
    wander: &'a motion::WanderSnapshot,
    cache: &motion::PathFinderCache,
) -> Result<&'a motion::WanderNormalRouteHead, NativeError> {
    let normal = wander.normal_route.as_deref().ok_or_else(unsupported)?;
    let request = &normal.request;
    let sample = request.wander.as_deref().ok_or_else(unsupported)?;
    if wander.failed_route.is_some()
        || request.kind != motion::RouteKind::Wander
        || request.binding.identity != snapshot.identity
        || request.launch_origin != request.origin
        || sample.center != wander.center
        || sample.radius != wander.radius
        || sample.prepared.query_id < wander.pathfinder_construction.query_id
        || sample.prepared.now_ms < wander.pathfinder_construction.now_ms
        || request.pathfinder_dispatch.as_ref().is_none_or(|dispatch| {
            dispatch.constructor_kind() != motion::PathfinderConstructorKind::WanderInitialized
                || dispatch.construction_binding() != wander.pathfinder_construction
        })
    {
        return Err(unsupported());
    }
    normal_calculation(request, &normal.route, cache)?;
    Ok(normal)
}
#[cfg(test)]
#[path = "dual_owner_reopen_tests.rs"]
mod tests;
