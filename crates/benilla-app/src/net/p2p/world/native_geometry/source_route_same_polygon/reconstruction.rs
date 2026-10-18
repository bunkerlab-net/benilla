// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded accepted-input capture; bytes alone grant no loaded owner authority.
#[cfg(not(benilla_headless_geometry))]
use super::super::core::Corridor;
use super::*;
use benilla_native_nav::{
    SourceWanderReconstructionOperation, SourceWanderReconstructionRecipe, SourceWanderWitness,
};
use benilla_world_runtime::native::{Draw, DrawPurpose, NativeError, NativeWorld};
use serde::{Deserialize, Serialize};

const MAX_RECIPE_BYTES: usize = 8 * 1024 * 1024;
const RECIPE_VERSION: u32 = 2;

#[derive(Clone, Default)]
pub(super) struct WanderCapture {
    native: SourceWanderWitness,
    constructor: Option<AuthorityRequest>,
    operations: Vec<SavedOperation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedOperation {
    ordinal: u64,
    preparation: AuthorityRequest,
    calculation: AuthorityRequest,
    draws: [Draw; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedRecipe {
    version: u32,
    world: [u8; 16],
    profile: [u8; 32],
    geometry: [u8; 32],
    checkpoint: [u8; 32],
    inventory: [u8; 32],
    epoch: [u8; 32],
    navigation: [u8; 32],
    configuration: [u8; 32],
    npc: NpcId,
    construction: motion::QueryBinding,
    native_version: u32,
    compatibility_version: u32,
    slot: [u32; 2],
    capacity: u32,
    normalization: [bool; 2],
    filter: [u16; 2],
    operation_count: u64,
    constructor: AuthorityRequest,
    operations: Vec<SavedOperation>,
}
impl WanderCapture {
    pub(super) fn record(
        &mut self,
        candidate: &SamePolygonCandidate,
        draws: &[Draw],
    ) -> Result<(), Failure> {
        let terminal = candidate.terminal.wander().ok_or(Failure::Binding)?;
        let prefix = candidate.wander_prefix.as_ref().ok_or(Failure::Binding)?;
        let npc = candidate.context.binding.npc.ok_or(Failure::Binding)?;
        let selected: Vec<_> = draws.iter().filter(|d| matches!(d.purpose,
            DrawPurpose::WanderAngle { npc: owner } | DrawPurpose::WanderRadius { npc: owner } if owner == npc)).cloned().collect();
        let draws: [Draw; 2] = selected.try_into().map_err(|_| Failure::Binding)?;
        if !matches!(draws[0].purpose, DrawPurpose::WanderAngle { npc: owner } if owner == npc)
            || !matches!(draws[1].purpose, DrawPurpose::WanderRadius { npc: owner } if owner == npc)
        {
            return Err(Failure::Binding);
        }
        if self.constructor.is_none() {
            if !prefix.prepared.validation.dispatch.first_construction() {
                return Err(Failure::Binding);
            }
            self.constructor = Some(prefix.prepared.validation.context.clone());
        }
        self.native.record(terminal).map_err(|_| Failure::Binding)?;
        let recipe = self.native.recipe().map_err(|_| Failure::Binding)?;
        self.operations.push(SavedOperation {
            ordinal: recipe
                .operation_count
                .checked_sub(1)
                .ok_or(Failure::Binding)?,
            preparation: prefix.prepared.validation.context.clone(),
            calculation: candidate.context.clone(),
            draws,
        });
        self.operations.retain(|operation| {
            recipe
                .operations
                .iter()
                .any(|input| input.ordinal == operation.ordinal)
        });
        let bytes = serde_json::to_vec(&self.operations).map_err(|_| Failure::Invalid)?;
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
impl SavedRecipe {
    fn native(&self) -> Result<SourceWanderReconstructionRecipe, Failure> {
        let operations = self
            .operations
            .iter()
            .map(|saved| {
                let Query::SourceMotion(MotionQuery::Route(route)) = &saved.calculation.query
                else {
                    return Err(Failure::Binding);
                };
                Ok(SourceWanderReconstructionOperation {
                    ordinal: saved.ordinal,
                    from: [
                        route.request.origin.x.bits(),
                        route.request.origin.y.bits(),
                        route.request.origin.z.bits(),
                    ],
                    raw_end: [
                        route.request.goal.x.bits(),
                        route.request.goal.y.bits(),
                        route.request.goal.z.bits(),
                    ],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SourceWanderReconstructionRecipe {
            version: self.native_version,
            compatibility_version: self.compatibility_version,
            slot: benilla_native_nav::SourceMapQuerySlot {
                map_id: self.slot[0],
                instance_id: self.slot[1],
            },
            query_capacity: self.capacity,
            config: benilla_native_nav::SourceSamePolygonConfig {
                normalize_z: self.normalization[0],
                ignore_normalization: self.normalization[1],
            },
            filter: SourceOwnerFilter {
                include_flags: self.filter[0],
                exclude_flags: self.filter[1],
            },
            operation_count: self.operation_count,
            operations,
        })
    }
}
impl SourceSamePolygonRouteProvider {
    pub(in super::super) fn encode_wander_reconstruction(
        &self,
        session: &Session,
        world: &NativeWorld,
        boundary: &RouteCommitBoundary,
    ) -> Result<Vec<u8>, NativeError> {
        boundary.validate_world(world)?;
        self.check_commit_binding(session, boundary)
            .map_err(|_| NativeError::AuthorityBinding)?;
        if self.committed_boundary.as_ref() != Some(boundary) {
            return Err(NativeError::AuthorityBinding);
        }
        let checkpoint = world.checkpoint()?;
        let pending = NativeWorld::prepare_synthetic_wander_reopen(
            world.profile().clone(),
            checkpoint.clone(),
        )?;
        let entry = self
            .candidates
            .iter()
            .find(|e| e.npc == pending.owner() && e.construction == pending.construction())
            .ok_or(NativeError::AuthorityBinding)?;
        let head = entry
            .committed
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        if &head.after_cache != pending.cache() {
            return Err(NativeError::AuthorityBinding);
        }
        let capture = entry
            .reconstruction
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "bounded Wander capture unavailable",
            ))?;
        let native = capture
            .native
            .recipe()
            .map_err(|_| NativeError::AuthorityBinding)?;
        if native.operation_count != pending.cache().revision {
            return Err(NativeError::AuthorityBinding);
        }
        let binding = self.queries.snapshot_binding();
        let recipe = SavedRecipe {
            version: RECIPE_VERSION,
            world: session.world,
            profile: session.profile,
            geometry: session.geometry,
            checkpoint: checkpoint.identity().state_digest,
            inventory: binding.inventory,
            epoch: binding.source_epoch,
            navigation: binding.policy,
            configuration: self.queries.configuration_binding(),
            npc: entry.npc,
            construction: entry.construction,
            native_version: native.version,
            compatibility_version: native.compatibility_version,
            slot: [native.slot.map_id, native.slot.instance_id],
            capacity: native.query_capacity,
            normalization: [
                native.config.normalize_z,
                native.config.ignore_normalization,
            ],
            filter: [native.filter.include_flags, native.filter.exclude_flags],
            operation_count: native.operation_count,
            constructor: capture
                .constructor
                .clone()
                .ok_or(NativeError::AuthorityBinding)?,
            operations: capture.operations.clone(),
        };
        if recipe.native().map_err(|_| NativeError::AuthorityBinding)? != native {
            return Err(NativeError::AuthorityBinding);
        }
        let bytes = serde_json::to_vec(&recipe).map_err(|_| NativeError::InvalidCheckpoint)?;
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(NativeError::TooLarge);
        }
        Ok(bytes)
    }
}

/// Created only after the pending runtime consumed matching actual native proof.
/// A checkpoint, bool or ref vector cannot construct this app-only capability.
pub(in super::super) struct ReopenBoundaryProof {
    checkpoint: benilla_world_runtime::native::Checkpoint,
}
impl ReopenBoundaryProof {
    pub(in super::super) fn validate(&self, world: &NativeWorld) -> Result<(), NativeError> {
        if world.checkpoint()? != self.checkpoint {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

impl SourceSamePolygonRouteProvider {
    /// Keep the exclusive store lease and its exact paired bytes owned until the
    /// fresh runtime, loaded query and commit boundary can be published together.
    #[cfg(not(benilla_headless_geometry))]
    pub(in super::super) fn reopen_wander_store(
        self,
        session: &Session,
        pending: benilla_world_runtime::native::PendingWanderReopen,
        store: benilla_world_store::native::wander_reconstruction::WanderStore,
        corridor: &mut super::super::local_corridor::LocalCorridor<'_, '_, '_>,
        deadline: Instant,
    ) -> Result<
        (
            benilla_world_store::native::wander_reconstruction::WanderStore,
            NativeWorld,
            Self,
            RouteCommitBoundary,
        ),
        NativeError,
    > {
        if store.requires_reopen() || store.recipe().checkpoint() != pending.checkpoint() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let (world, provider, boundary) = self.reopen_wander(
            session,
            pending,
            store.recipe().recipe_bytes(),
            corridor,
            deadline,
        )?;
        Ok((store, world, provider, boundary))
    }

    /// Consume fresh unpublished providers and return the world/owner/boundary only
    /// after every loaded query, runtime proof and constructor installation succeeds.
    #[cfg(not(benilla_headless_geometry))]
    pub(in super::super) fn reopen_wander(
        mut self,
        session: &Session,
        pending: benilla_world_runtime::native::PendingWanderReopen,
        bytes: &[u8],
        corridor: &mut super::super::local_corridor::LocalCorridor<'_, '_, '_>,
        deadline: Instant,
    ) -> Result<(NativeWorld, Self, RouteCommitBoundary), NativeError> {
        use benilla_native_nav::SourceWanderReconstructionStage as Stage;
        require_time(deadline).map_err(|_| NativeError::AuthorityBinding)?;
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(NativeError::TooLarge);
        }
        let saved: SavedRecipe =
            serde_json::from_slice(bytes).map_err(|_| NativeError::InvalidCheckpoint)?;
        if serde_json::to_vec(&saved).map_err(|_| NativeError::InvalidCheckpoint)? != bytes {
            return Err(NativeError::InvalidCheckpoint);
        }
        let navigation = self.queries.snapshot_binding();
        if corridor.map != session.map || corridor.revision != self.raw_geometry() {
            return Err(NativeError::AuthorityBinding);
        }
        if saved.version != RECIPE_VERSION
            || saved.world != session.world
            || saved.profile != session.profile
            || saved.geometry != session.geometry
            || saved.geometry
                != self
                    .binding
                    .geometry_revision()
                    .map_err(|_| NativeError::AuthorityBinding)?
            || saved.profile != self.profile.digest()
            || saved.checkpoint != pending.checkpoint().identity().state_digest
            || saved.inventory != navigation.inventory
            || saved.epoch != navigation.source_epoch
            || saved.navigation != navigation.policy
            || saved.configuration != self.queries.configuration_binding()
            || saved.npc != pending.owner()
            || saved.construction != pending.construction()
            || saved.operation_count != pending.cache().revision
            || !self.candidates.is_empty()
            || !self.pending.is_empty()
            || !self.wander_prefixes.is_empty()
            || self.queries.constructor_count() != 0
            || self.last_notice.is_some()
            || self
                .committed_boundary
                .as_ref()
                .is_some_and(|b| b.tick() != 0 || b.heads().iter().any(|h| h.cache().is_some()))
        {
            return Err(NativeError::AuthorityBinding);
        }
        let native = saved.native().map_err(|_| NativeError::InvalidCheckpoint)?;
        native
            .validate()
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        require_time(deadline).map_err(|_| NativeError::AuthorityBinding)?;
        session
            .reconstructed_wander_body(&pending, &saved.constructor)
            .map_err(|_| NativeError::AuthorityBinding)?;
        for operation in &saved.operations {
            require_time(deadline).map_err(|_| NativeError::AuthorityBinding)?;
            session
                .reconstructed_wander_body(&pending, &operation.preparation)
                .map_err(|_| NativeError::AuthorityBinding)?;
            session
                .reconstructed_wander_body(&pending, &operation.calculation)
                .map_err(|_| NativeError::AuthorityBinding)?;
            let Query::SourceMotion(MotionQuery::Route(prepare)) = &operation.preparation.query
            else {
                return Err(NativeError::AuthorityBinding);
            };
            let Query::SourceMotion(MotionQuery::Route(route)) = &operation.calculation.query
            else {
                return Err(NativeError::AuthorityBinding);
            };
            let sample = route
                .request
                .wander
                .as_deref()
                .ok_or(NativeError::AuthorityBinding)?;
            if prepare.request.kind != motion::RouteKind::WanderPrepare
                || route.request.kind != motion::RouteKind::Wander
                || sample.prepared != prepare.request.binding
                || prepare.request.origin != route.request.origin
                || prepare.request.goal != sample.center
                || prepare.request.prior_cache != route.request.prior_cache
                || prepare.owner != route.owner
                || operation.preparation.binding != operation.calculation.binding
                || route.request.launch_origin != route.request.origin
                || sample
                    .endpoint()
                    .map_err(|_| NativeError::AuthorityBinding)?
                    != route.request.goal
                || route
                    .request
                    .prior_cache
                    .as_ref()
                    .map_or(0, |cache| cache.revision)
                    != operation.ordinal
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        pending.validate_wander_head(
            &saved
                .operations
                .last()
                .ok_or(NativeError::AuthorityBinding)?
                .calculation,
        )?;
        let mut observed_failure = None;
        let certificate = self
            .queries
            .nav_query()
            .reconstruct_wander_observed(&native, pending.challenge(), deadline, |stage| {
                let result: Result<(), Failure> = (|| {
                    require_time(deadline)?;
                    match stage {
                        Stage::Constructor => {
                            let bound = self.filter.begin(session, &saved.constructor)?;
                            let filter = bound.query(
                                RouteFilterRequest {
                                    snapshot: self.binding.liquid,
                                    context: saved.constructor.clone(),
                                    first_ticket: 1,
                                },
                                deadline,
                            )?;
                            if filter.filter().include_flags() != native.filter.include_flags
                                || filter.filter().exclude_flags() != native.filter.exclude_flags
                            {
                                return Err(Failure::Binding);
                            }
                        }
                        Stage::Prepared { ordinal, prefix } => {
                            let operation = saved
                                .operations
                                .iter()
                                .find(|op| op.ordinal == ordinal)
                                .ok_or(Failure::Binding)?;
                            if prefix.current_selected() || prefix.cleared_active_len() != 0 {
                                return Err(Failure::Binding);
                            }
                            let bound = self.filter.begin(session, &operation.preparation)?;
                            let update = bound.query_retained_update(
                                RouteFilterRequest {
                                    snapshot: self.binding.liquid,
                                    context: operation.preparation.clone(),
                                    first_ticket: 3,
                                },
                                native.filter,
                                None,
                                deadline,
                            )?;
                            if update.filter() != native.filter {
                                return Err(Failure::Binding);
                            }
                        }
                        Stage::Selected { ordinal } => {
                            let operation = saved
                                .operations
                                .iter()
                                .find(|op| op.ordinal == ordinal)
                                .ok_or(Failure::Binding)?;
                            let Query::SourceMotion(MotionQuery::Route(route)) =
                                &operation.calculation.query
                            else {
                                return Err(Failure::Binding);
                            };
                            pending
                                .validate_wander_draws(
                                    &operation.draws,
                                    route.request.wander.as_deref().ok_or(Failure::Binding)?,
                                )
                                .map_err(|_| Failure::Binding)?;
                            self.filter.begin(session, &operation.calculation)?;
                            same_polygon_model(&self.profile, session.world, self.player, route)?;
                            if route
                                .request
                                .origin
                                .distance_squared(route.request.goal)
                                .map_err(|_| Failure::Invalid)?
                                .as_f32()
                                >= 0.01
                                && route
                                    .require_owner_ignore_pathfinding(&self.profile)
                                    .map_err(|_| Failure::Binding)?
                                    .ignore_pathfinding()
                            {
                                return Err(Failure::UnsupportedQuery);
                            }
                        }
                        Stage::Terminal { ordinal, terminal } => {
                            let operation = saved
                                .operations
                                .iter()
                                .find(|op| op.ordinal == ordinal)
                                .ok_or(Failure::Binding)?;
                            self.verify_reconstructed_terminal(
                                &operation.calculation,
                                terminal,
                                session
                                    .reconstructed_wander_body(&pending, &operation.calculation)?,
                                corridor,
                                deadline,
                            )?;
                        }
                    }
                    Ok(())
                })();
                result.map_err(|error| {
                    observed_failure = Some(error);
                    benilla_native_nav::SourceWanderReconstructionError::UnsupportedLineage
                })
            })
            .map_err(|_| NativeError::AuthorityBinding)?;
        if observed_failure.is_some() {
            return Err(NativeError::AuthorityBinding);
        }
        let origin = self
            .queries
            .validate_reconstructed(&pending, &saved.constructor, &certificate)
            .map_err(|_| NativeError::AuthorityBinding)?;
        let checkpoint = pending.checkpoint().clone();
        let cache = pending.cache().clone();
        let (world, certificate) = pending.finish(certificate)?;
        let proof = ReopenBoundaryProof { checkpoint };
        let boundary = RouteCommitBoundary::capture_reconstructed(&world, &proof)?;
        let (terminal, witness) = self
            .queries
            .install_reconstructed(origin, certificate)
            .map_err(|_| NativeError::AuthorityBinding)?;
        let final_operation = saved
            .operations
            .last()
            .ok_or(NativeError::AuthorityBinding)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &final_operation.calculation.query
        else {
            return Err(NativeError::AuthorityBinding);
        };
        let dispatch = route
            .pathfinder_dispatch
            .clone()
            .ok_or(NativeError::AuthorityBinding)?;
        // Historical metadata remains inert; only newly issued runtime dispatches
        // can resolve subsequent calculations. This head is genuine loaded state.
        let candidate = Rc::new(SamePolygonCandidate {
            context: final_operation.calculation.clone(),
            dispatch,
            terminal: SamePolygonTerminal::Wander(terminal),
            wander_tail: None,
            wander_prefix: None,
            after_cache: cache,
        });
        self.candidates.push(GeneratorCandidates {
            npc: saved.npc,
            construction: saved.construction,
            kind: motion::PathfinderConstructorKind::WanderInitialized,
            proposals: vec![Rc::clone(&candidate)],
            certified: vec![],
            committed: Some(candidate),
            reconstruction: Some(WanderCapture {
                native: witness,
                constructor: Some(saved.constructor),
                operations: saved.operations,
            }),
        });
        self.check_commit_binding(session, &boundary)
            .map_err(|_| NativeError::AuthorityBinding)?;
        self.committed_boundary = Some(boundary.clone());
        boundary.validate_world(&world)?;
        Ok((world, self, boundary))
    }

    #[cfg(not(benilla_headless_geometry))]
    fn verify_reconstructed_terminal(
        &self,
        context: &AuthorityRequest,
        terminal: &LoadedSourceWanderTerminal,
        body: super::super::core::Body,
        corridor: &mut super::super::local_corridor::LocalCorridor<'_, '_, '_>,
        deadline: Instant,
    ) -> Result<(), Failure> {
        use super::super::products::{
            ground_height::MapHeightRequest,
            liquid::{LiquidRequest, LiquidStatus},
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding);
        };
        if terminal.no_path_reason()
            == Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance)
        {
            return Ok(());
        }
        let position = bits(route.request.origin);
        let water_request = LiquidRequest {
            snapshot: self.binding.liquid,
            ticket: 5,
            position,
        };
        let water = self
            .filter
            .dry_liquid()
            .query(water_request, deadline)
            .map_err(Failure::product)?;
        if water.request != water_request || water.status != LiquidStatus::NoWater {
            return Err(Failure::Binding);
        }
        let height = self
            .wander_height
            .as_ref()
            .ok_or(Failure::MissingGeometry)?;
        let ground_request = MapHeightRequest {
            snapshot: height.binding(),
            ticket: 6,
            position,
            swim: false,
        };
        let ground = height
            .query(ground_request, deadline)
            .map_err(Failure::product)?;
        if ground.request != ground_request
            || !f32::from_bits(ground.height).is_finite()
            || (route.request.origin.z.as_f32() - f32::from_bits(ground.height)).abs() > 0.5
        {
            return Err(Failure::UnsupportedQuery);
        }
        if terminal.path_type() == 8 {
            return Ok(());
        }
        if terminal.points() != [position, terminal.actual_end()] {
            return Err(Failure::Binding);
        }
        corridor.certify(
            position.map(f32::from_bits),
            terminal.actual_end().map(f32::from_bits),
            body,
        )?;
        let height = route.request.owner_collision_height.bits();
        let [from, to] =
            chase_gate::los_endpoints_bits(position, terminal.actual_end(), height, height)
                .map_err(|_| Failure::Invalid)?;
        let ray = RayRequest {
            snapshot: self.vmap.binding(),
            ticket: 7,
            from,
            to,
            ignore_m2: false,
        };
        let reply = self.vmap.query(ray, deadline).map_err(Failure::product)?;
        if reply.request != ray || !matches!(reply.result, StaticRay::Miss) {
            return Err(Failure::Obstacle);
        }
        require_time(deadline)
    }
}
