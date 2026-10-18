// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic-domain pending restore for one current NORMAL or MinimumDistance
//! NoPath dry Wander owner.
//! Native replay proves retained semantic state, not composite app geometry or
//! historical observations. The app must bind its actual products and publish
//! the fresh provider and returned world together. Native admission stays closed.
use super::*;
use benilla_native_nav::{
    ReconstructedSourceWanderTerminal, SourceMapQuerySlot, SourceOwnerFilter,
    SourceSamePolygonConfig, SourceWanderReconstructionChallenge, SourceWanderSemanticState,
};
use benilla_sim::creature_motion as motion;

/// An unpublished, fully checked component world awaiting actual native replay.
/// No Clone, Deserialize, world borrow or public release-with-permission exists.
/// The original checkpoint, operational revision, RNG, clocks and spline remain
/// unchanged; only the separate loaded provider acquires fresh native ownership.
///
/// ```compile_fail
/// use benilla_world_runtime::native::{NativeWorld, PendingWanderReopen};
/// fn bypass(pending: PendingWanderReopen) -> NativeWorld { pending.world.clone() }
/// ```
pub struct PendingWanderReopen {
    world: NativeWorld,
    checkpoint: Checkpoint,
    owner: crate::NpcId,
    construction: motion::QueryBinding,
    cache: motion::PathFinderCache,
    head: motion::RouteRequest,
    semantic: SourceWanderSemanticState,
    challenge: SourceWanderReconstructionChallenge,
}

impl PendingWanderReopen {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }
    pub fn owner(&self) -> crate::NpcId {
        self.owner
    }
    pub fn construction(&self) -> motion::QueryBinding {
        self.construction
    }
    pub fn cache(&self) -> &motion::PathFinderCache {
        &self.cache
    }
    pub fn challenge(&self) -> &SourceWanderReconstructionChallenge {
        &self.challenge
    }

    /// Validate an inert historical Prepare/Wander request against this exact
    /// saved owner and profile. Deserialized dispatches remain without live seals;
    /// successful comparison grants no query, constructor or Route authority.
    pub fn validate_wander_operation(&self, context: &AuthorityRequest) -> Result<(), NativeError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(NativeError::AuthorityBinding);
        };
        if !matches!(
            route.request.kind,
            motion::RouteKind::WanderPrepare | motion::RouteKind::Wander
        ) {
            return Err(NativeError::AuthorityBinding);
        }
        let receipt = context.validate_saved_pathfinder_dispatch(&self.world.profile)?;
        let npc = self
            .world
            .state
            .npcs
            .get(&self.owner.spawn)
            .ok_or(NativeError::AuthorityBinding)?;
        let latest = self.head.binding;
        let now = context
            .binding
            .tick
            .checked_mul(u64::from(self.world.profile.tick_ms))
            .and_then(|elapsed| self.world.profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(NativeError::AuthorityBinding)?;
        if context.binding.world != self.world.state.world
            || context.binding.npc != Some(self.owner)
            || context.binding.tick == 0
            || context.binding.tick > self.world.state.tick
            || receipt.npc() != self.owner
            || receipt.constructor_kind() != motion::PathfinderConstructorKind::WanderInitialized
            || receipt.construction_binding() != self.construction
            || receipt.dispatch_binding() != route.request.binding
            || route.request.binding.query_id > latest.query_id
            || route.request.binding.now_ms != now
            || route.request.binding.now_ms > latest.now_ms
            || route.request.target.is_some()
            || route.target_player_height.is_some()
            || route.target_player_water.is_some()
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
        route.request.validate_wander()?;
        Ok(())
    }

    /// Bind the final retained calculation to the exact checkpoint-owned request,
    /// including its query/tick, prior cache and random sample. Successful native
    /// geometry alone cannot relabel the restored provider's current head.
    pub fn validate_wander_head(&self, context: &AuthorityRequest) -> Result<(), NativeError> {
        self.validate_wander_operation(context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(NativeError::AuthorityBinding);
        };
        validate_wander_head_request(&route.request, &self.head)
    }

    /// Check retained angle/radius samples against the saved counter-addressable
    /// stream without advancing gameplay RNG or disclosing its seed. This proves
    /// the values and owner bounds, not omitted historical dispatch or ACK events.
    pub fn validate_wander_draws(
        &self,
        draws: &[Draw; 2],
        request: &motion::WanderRequest,
    ) -> Result<(), NativeError> {
        let motion::DefaultMovement::Wander { center, radius } = self
            .world
            .profile
            .npc_motion(self.owner.spawn)?
            .default_movement
        else {
            return Err(NativeError::AuthorityBinding);
        };
        let motion = self
            .world
            .state
            .npcs
            .get(&self.owner.spawn)
            .and_then(|npc| npc.source_motion.as_ref())
            .ok_or(NativeError::AuthorityBinding)?
            .motion
            .snapshot();
        if request.center != center
            || request.radius != radius
            || request.prepared.identity != self.construction.identity
            || request.prepared.query_id < self.construction.query_id
            || request.prepared.query_id > motion.query_id
            || request.prepared.now_ms < self.construction.now_ms
            || request.prepared.now_ms < self.world.profile.clock_origin_ms
            || request.prepared.now_ms > self.world.state.now_ms
            || !(request.prepared.now_ms - self.world.profile.clock_origin_ms)
                .is_multiple_of(u64::from(self.world.profile.tick_ms))
        {
            return Err(NativeError::AuthorityBinding);
        }
        validate_draw_pair(&self.world.state, self.owner, draws, request)
    }

    /// Consume one pending attempt and its genuinely executed native replay.
    /// This component-only API does not authenticate app inventory or geometry;
    /// the private app coordinator must check the certificate's exact loaded
    /// allocation against its bound products before publishing the pair.
    pub fn finish(
        self,
        certificate: ReconstructedSourceWanderTerminal,
    ) -> Result<(NativeWorld, ReconstructedSourceWanderTerminal), NativeError> {
        validate_certificate(
            &certificate,
            &self.challenge,
            self.cache.revision,
            &self.semantic,
        )?;
        if self.world.checkpoint()? != self.checkpoint {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok((self.world, certificate))
    }
}

fn validate_wander_head_request(
    request: &motion::RouteRequest,
    expected: &motion::RouteRequest,
) -> Result<(), NativeError> {
    if request.kind != motion::RouteKind::Wander || request != expected {
        return Err(NativeError::AuthorityBinding);
    }
    Ok(())
}

fn validate_draw_pair(
    state: &State,
    owner: crate::NpcId,
    draws: &[Draw; 2],
    request: &motion::WanderRequest,
) -> Result<(), NativeError> {
    let mut cursor = draws[0].first_counter;
    for (draw, purpose, normalized) in [
        (
            &draws[0],
            DrawPurpose::WanderAngle { npc: owner },
            request.angle_draw,
        ),
        (
            &draws[1],
            DrawPurpose::WanderRadius { npc: owner },
            request.radius_draw,
        ),
    ] {
        if draw.cast_id != 0
            || draw.purpose != purpose
            || draw.low != 0
            || draw.high != i64::from(u32::MAX)
            || draw.first_counter != cursor
            || cursor >= state.rng_counter
        {
            return Err(NativeError::AuthorityBinding);
        }
        let expected =
            rng::sample_inclusive_v2(state.world, state.seed, &mut cursor, 0, i64::from(u32::MAX))?;
        if draw.first_counter != expected.first_counter
            || draw.consumed != expected.consumed_words
            || draw.value != expected.value
            || cursor > state.rng_counter
            || motion::wander_normalized_word(
                u32::try_from(draw.value).map_err(|_| NativeError::AuthorityBinding)?,
            )? != normalized
        {
            return Err(NativeError::AuthorityBinding);
        }
    }
    Ok(())
}

fn validate_certificate(
    certificate: &ReconstructedSourceWanderTerminal,
    challenge: &SourceWanderReconstructionChallenge,
    revision: u64,
    semantic: &SourceWanderSemanticState,
) -> Result<(), NativeError> {
    if !certificate.matches_challenge(challenge)
        || certificate.witness().operation_count() != revision
        || certificate.terminal().semantic_state() != *semantic
    {
        return Err(NativeError::AuthorityBinding);
    }
    Ok(())
}

impl NativeWorld {
    /// Stage exactly one native NORMAL or MinimumDistance NoPath Wander head.
    /// A successful spline may be moving or finalized; a failed current head
    /// may have no spline or retain a finalized older success. Other owners must
    /// be cache-free Idle. Chase, Home and suspended Wander remain unsupported.
    /// The older spline prior-cache is structurally checked inert history, never
    /// installed as a loaded head. Ordinary saved-session restore stays closed.
    pub fn prepare_synthetic_wander_reopen(
        profile: Profile,
        checkpoint: Checkpoint,
    ) -> Result<PendingWanderReopen, NativeError> {
        // Bound typed callers before copying the checkpoint or native cache.
        checkpoint.state.validate_history_budget()?;
        checkpoint.validate_encoded_budget()?;
        if checkpoint.state.domain != Domain::SyntheticComponentTest {
            return Err(NativeError::Incompatible);
        }
        let (pending, construction, cache, head, semantic) =
            pending_wander_shape(&profile, &checkpoint)?;
        let world =
            Self::restore_synthetic_checkpoint(profile, checkpoint.clone(), Some(&pending))?;
        if world.checkpoint()? != checkpoint {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(PendingWanderReopen {
            world,
            checkpoint,
            owner: pending.npc,
            construction,
            cache,
            head,
            semantic,
            challenge: SourceWanderReconstructionChallenge::new(),
        })
    }
}

type PendingShape = (
    PendingWanderRestore,
    motion::QueryBinding,
    motion::PathFinderCache,
    motion::RouteRequest,
    SourceWanderSemanticState,
);

fn unsupported() -> NativeError {
    NativeError::UnsupportedMechanic(
        "one current NORMAL or MinimumDistance NoPath Wander cached owner",
    )
}

fn bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}

fn pending_wander_shape(
    profile: &Profile,
    checkpoint: &Checkpoint,
) -> Result<PendingShape, NativeError> {
    let configuration = profile.pathfinder_configuration().ok_or_else(unsupported)?;
    let mut selected = None;
    for npc in checkpoint.state.npcs.values() {
        let Some(source) = &npc.source_motion else {
            continue;
        };
        let snapshot = &source.motion;
        if snapshot.suspended_wander.is_some() {
            return Err(unsupported());
        }
        let wander = match &snapshot.generator {
            motion::Generator::Idle if snapshot.spline.is_none() => {
                source.validate_saved_session_reopen()?;
                continue;
            }
            motion::Generator::Wander(wander) => wander,
            _ => return Err(unsupported()),
        };
        if selected.is_some() {
            return Err(unsupported());
        }
        let cache = wander.pathfinder_cache.as_deref().ok_or_else(unsupported)?;
        if cache.source != motion::RouteSource::OwnerPathFinder
            || cache.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || cache.binding.navigation_policy != profile.geometry_revision
        {
            return Err(unsupported());
        }
        let (head, points, no_path_reason) = current_wander_head(snapshot, wander, cache)?;
        let binding = head.binding;
        let sample = head.wander.as_deref().ok_or_else(unsupported)?;
        let construction = wander.pathfinder_construction;
        if profile.tick_ms == 0
            || binding.identity != snapshot.identity
            || binding.query_id > snapshot.query_id
            || binding.now_ms <= profile.clock_origin_ms
            || binding.now_ms > checkpoint.state.now_ms
            || !(binding.now_ms - profile.clock_origin_ms)
                .is_multiple_of(u64::from(profile.tick_ms))
            || sample.center != wander.center
            || sample.radius != wander.radius
            || sample.prepared.query_id < construction.query_id
            || sample.prepared.now_ms < construction.now_ms
            || head.pathfinder_dispatch.as_ref().is_none_or(|dispatch| {
                dispatch.constructor_kind() != motion::PathfinderConstructorKind::WanderInitialized
                    || dispatch.construction_binding() != construction
            })
        {
            return Err(NativeError::AuthorityBinding);
        }
        head.validate_pathfinder_dispatch()?;
        // Every native semantic field comes from the exact current checkpoint
        // operation. A retained successful spline is inert history after NoPath.
        let semantic = SourceWanderSemanticState {
            slot: SourceMapQuerySlot {
                map_id: profile.map_id,
                instance_id: configuration.selected().source_instance_id,
            },
            query_capacity: 1024,
            config: SourceSamePolygonConfig {
                normalize_z: false,
                ignore_normalization: false,
            },
            filter: SourceOwnerFilter {
                include_flags: cache.include_flags,
                exclude_flags: cache.exclude_flags,
            },
            from: bits(head.origin),
            raw_end: bits(head.goal),
            actual_end: bits(cache.requested_end),
            storage: cache.poly_refs.clone(),
            active_len: usize::from(cache.active_len),
            path_type: cache.path_type,
            points,
            no_path_reason,
        };
        selected = Some((
            PendingWanderRestore {
                npc: npc.id,
                snapshot: source.clone(),
            },
            wander.pathfinder_construction,
            cache.clone(),
            head.clone(),
            semantic,
        ));
    }
    selected.ok_or_else(unsupported)
}

type SelectedWanderHead<'a> = (
    &'a motion::RouteRequest,
    Vec<[u32; 3]>,
    Option<benilla_native_nav::SourceWanderNoPathReason>,
);

/// Select the current operation by its cache state, never by a convenient old
/// spline or reconstruction recipe. The complete snapshot is validated again by
/// MotionState::restore before this pending owner can be returned.
fn current_wander_head<'a>(
    snapshot: &'a motion::MotionSnapshot,
    wander: &'a motion::WanderSnapshot,
    cache: &motion::PathFinderCache,
) -> Result<SelectedWanderHead<'a>, NativeError> {
    match cache.path_type {
        1 => {
            let spline = snapshot.spline.as_ref().ok_or_else(unsupported)?;
            let prior_revision = spline
                .route_request
                .prior_cache
                .as_deref()
                .map_or(0, |prior| prior.revision);
            if wander.failed_route.is_some()
                || cache.active_len != 1
                || prior_revision.checked_add(1) != Some(cache.revision)
                || spline.route_source != motion::RouteSource::OwnerPathFinder
                || spline.route.source != motion::RouteSource::OwnerPathFinder
                || spline.route_request.kind != motion::RouteKind::Wander
                || spline.local_home_cache.is_some()
                || spline.route.points.len() != 2
            {
                return Err(unsupported());
            }
            Ok((
                &spline.route_request,
                spline.route.points.iter().copied().map(bits).collect(),
                None,
            ))
        }
        8 => {
            let failed = wander.failed_route.as_deref().ok_or_else(unsupported)?;
            if failed.reason != motion::WanderNoPathReason::MinimumDistance
                || cache.active_len != 0
                || failed.request.kind != motion::RouteKind::Wander
                || failed.request.origin != snapshot.map_pose.position
                || failed.request.binding.query_id != snapshot.query_id
                || snapshot.spline.as_ref().is_some_and(|spline| {
                    !spline.spline.finalized
                        || spline.route_source != motion::RouteSource::OwnerPathFinder
                        || spline.route.source != motion::RouteSource::OwnerPathFinder
                        || spline.route_request.kind != motion::RouteKind::Wander
                        || spline.local_home_cache.is_some()
                        || spline.route_request.binding.query_id >= failed.request.binding.query_id
                        || spline.route_request.binding.now_ms > failed.request.binding.now_ms
                })
            {
                return Err(unsupported());
            }
            motion::RouteCalculation {
                wander_no_path: Some(failed.reason),
                source: cache.source,
                outcome: motion::RouteOutcome::NoPath,
                prior_cache: failed.request.prior_cache.clone(),
                after_cache: cache.clone(),
            }
            .validate_for(&failed.request)?;
            Ok((
                &failed.request,
                Vec::new(),
                Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance),
            ))
        }
        _ => Err(unsupported()),
    }
}

#[cfg(test)]
#[path = "wander_reopen_tests.rs"]
mod tests;
