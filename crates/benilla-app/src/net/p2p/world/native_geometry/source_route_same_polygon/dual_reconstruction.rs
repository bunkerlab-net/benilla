// SPDX-License-Identifier: GPL-3.0-or-later
//! Inert bounded Chase/suspended-Wander inputs, replayed on one private fresh query.
//! The component-only return publishes both owners after loaded geometry and the
//! runtime's intact native pair have validated; ordinary restoration stays closed.
use super::super::{
    core::{Body, Corridor},
    local_corridor::LocalCorridor,
    products::{
        ground_height::MapHeightRequest,
        liquid::{LiquidRequest, LiquidStatus},
        source_query::SourceQueryGeometry,
        VmapLiquidProducts,
    },
};
use super::*;
use super::{
    chase_capture::{ChaseCapture, ChaseCaptureOperation},
    reconstruction::{
        ReopenBoundaryProof, SavedRecipe, WanderCapture, RECIPE_VERSION as WANDER_RECIPE_VERSION,
    },
};
use benilla_native_nav::{
    SourceChaseFixedCacheOperation, SourceChaseFixedCacheRecipe,
    SourceChaseFixedCacheStage as ChaseStage, SourceWanderReconstructionRecipe,
    SourceWanderReconstructionStage as WanderStage,
};
use benilla_world_runtime::native::{NativeError, NativeWorld, PendingWanderAndChaseReopen};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

const MAX_RECIPE_BYTES: usize = 8 * 1024 * 1024;
const RECIPE_VERSION: u32 = 1;
const RECIPE_KIND: &str = "bounded-chase-suspended-normal-wander";

/// Inventory-owned inputs, before any native query is allocated.
pub(in super::super) struct SamePolygonReopenProducts<'a> {
    pub inventory: &'a VmapLiquidProducts,
    pub environment: TerrainLiquidSnapshot<'static>,
    pub height: MapHeightSnapshot<'static>,
    pub near_configuration: SourceNearPointConfiguration,
    pub raw_geometry: [u8; 32],
    pub configuration: SamePolygonConfiguration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedDualRecipe {
    version: u32,
    kind: String,
    wander: SavedRecipe,
    chase: SavedChaseRecipe,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedChaseRecipe {
    construction: motion::QueryBinding,
    native_version: u32,
    compatibility_version: u32,
    slot: [u32; 2],
    capacity: u32,
    point_limit: u32,
    normalization: [bool; 2],
    filter: [u16; 2],
    operation_count: u64,
    constructor: AuthorityRequest,
    operations: Vec<ChaseCaptureOperation>,
}
impl SavedChaseRecipe {
    fn native(&self) -> Result<SourceChaseFixedCacheRecipe, NativeError> {
        let operations = self
            .operations
            .iter()
            .map(|saved| {
                let route = route(&saved.calculation)?;
                Ok(SourceChaseFixedCacheOperation {
                    ordinal: saved.ordinal,
                    from: bits(route.request.origin),
                    to: bits(route.request.goal),
                })
            })
            .collect::<Result<Vec<_>, NativeError>>()?;
        Ok(SourceChaseFixedCacheRecipe {
            version: self.native_version,
            compatibility_version: self.compatibility_version,
            slot: benilla_native_nav::SourceMapQuerySlot {
                map_id: self.slot[0],
                instance_id: self.slot[1],
            },
            query_capacity: self.capacity,
            point_limit: self.point_limit,
            config: SourceSamePolygonConfig {
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

impl SamePolygonBinding {
    pub(super) fn from_staged_geometry(
        raw_geometry: [u8; 32],
        geometry: &SourceQueryGeometry,
        configuration: SamePolygonConfiguration,
    ) -> Result<Self, Failure> {
        let result = Self {
            raw_geometry,
            liquid: geometry.liquid().binding(),
            navigation: geometry.navigation_binding(),
            vmap: geometry.static_vmap().binding(),
            selected_configuration: geometry.configuration_binding(),
            configuration,
            near_point: None,
            environment_policy: None,
        };
        result.geometry_revision()?;
        Ok(result)
    }
}

/// Rejection-only test observation; it cannot supply a query, certificate or owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum DualReopenStage {
    WanderReplayed,
    OwnersReplayed,
    ConstructorsInstalled,
    BeforePublication,
}

impl SourceSamePolygonRouteProvider {
    /// Exercise the same registry preflight with an independently retained live
    /// query; the probe restores each mutation before the ordinary install runs.
    pub(in super::super) fn assert_reconstructed_batch_rejections(
        &self,
        queries: &mut SourcePathfinderQueryProvider,
        plan: &mut super::super::source_pathfinder_query::ReconstructedConstructorPairPlan,
        wander: &benilla_native_nav::ReconstructedSourceWanderTerminal,
        chase: &benilla_native_nav::ReconstructedSourceChaseFixedCache,
    ) {
        super::super::source_pathfinder_query::pair_tests::assert_rejections(
            queries,
            plan,
            wander,
            chase,
            &self.queries,
        );
    }
    pub(in super::super) fn encode_wander_and_chase_reconstruction(
        &self,
        session: &Session,
        world: &NativeWorld,
        boundary: &RouteCommitBoundary,
    ) -> Result<Vec<u8>, NativeError> {
        boundary.validate_world(world)?;
        self.check_commit_binding(session, boundary)
            .map_err(binding_error)?;
        if self.committed_boundary.as_ref() != Some(boundary) {
            return Err(NativeError::AuthorityBinding);
        }
        let checkpoint = world.checkpoint()?;
        let pending = NativeWorld::prepare_synthetic_wander_and_chase_reopen(
            world.profile().clone(),
            checkpoint.clone(),
        )?;
        let entry = |construction, kind, cache: &motion::PathFinderCache| {
            let entry = self
                .candidates
                .iter()
                .find(|entry| {
                    entry.npc == pending.owner()
                        && entry.construction == construction
                        && entry.kind == kind
                })
                .ok_or(NativeError::AuthorityBinding)?;
            if entry.committed.as_ref().map(|head| &head.after_cache) != Some(cache) {
                return Err(NativeError::AuthorityBinding);
            }
            Ok(entry)
        };
        let wander = entry(
            pending.wander_construction(),
            motion::PathfinderConstructorKind::WanderInitialized,
            pending.wander_cache(),
        )?;
        let chase = entry(
            pending.chase_construction(),
            motion::PathfinderConstructorKind::ChaseLazy,
            pending.chase_cache(),
        )?;
        let wander_capture =
            wander
                .reconstruction
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "bounded Wander capture unavailable",
                ))?;
        let chase_capture =
            chase
                .chase_reconstruction
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "bounded Chase capture unavailable",
                ))?;
        let native_wander = wander_capture.native.recipe().map_err(binding_error)?;
        let native_chase = chase_capture.native.recipe().map_err(binding_error)?;
        let navigation = self.queries.snapshot_binding();
        let saved = SavedDualRecipe {
            version: RECIPE_VERSION,
            kind: RECIPE_KIND.to_owned(),
            wander: SavedRecipe {
                version: WANDER_RECIPE_VERSION,
                world: session.world,
                profile: session.profile,
                geometry: session.geometry,
                checkpoint: checkpoint.identity().state_digest,
                inventory: navigation.inventory,
                epoch: navigation.source_epoch,
                navigation: navigation.policy,
                configuration: self.queries.configuration_binding(),
                npc: pending.owner(),
                construction: pending.wander_construction(),
                native_version: native_wander.version,
                compatibility_version: native_wander.compatibility_version,
                slot: [native_wander.slot.map_id, native_wander.slot.instance_id],
                capacity: native_wander.query_capacity,
                normalization: [
                    native_wander.config.normalize_z,
                    native_wander.config.ignore_normalization,
                ],
                filter: [
                    native_wander.filter.include_flags,
                    native_wander.filter.exclude_flags,
                ],
                operation_count: native_wander.operation_count,
                constructor: wander_capture
                    .constructor
                    .clone()
                    .ok_or(NativeError::AuthorityBinding)?,
                operations: wander_capture.operations.clone(),
            },
            chase: SavedChaseRecipe {
                construction: pending.chase_construction(),
                native_version: native_chase.version,
                compatibility_version: native_chase.compatibility_version,
                slot: [native_chase.slot.map_id, native_chase.slot.instance_id],
                capacity: native_chase.query_capacity,
                point_limit: native_chase.point_limit,
                normalization: [
                    native_chase.config.normalize_z,
                    native_chase.config.ignore_normalization,
                ],
                filter: [
                    native_chase.filter.include_flags,
                    native_chase.filter.exclude_flags,
                ],
                operation_count: native_chase.operation_count,
                constructor: chase_capture
                    .constructor
                    .clone()
                    .ok_or(NativeError::AuthorityBinding)?,
                operations: chase_capture.operations.clone(),
            },
        };
        if saved.wander.native().map_err(binding_error)? != native_wander
            || saved.chase.native()? != native_chase
            || native_wander.operation_count != pending.wander_cache().revision
            || native_chase.operation_count != pending.chase_cache().revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        validate_history(&saved, session, &pending)?;
        let bytes = serde_json::to_vec(&saved).map_err(|_| NativeError::InvalidCheckpoint)?;
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(NativeError::TooLarge);
        }
        Ok(bytes)
    }

    /// Retain the exact paired checkpoint bytes and exclusive store lease until
    /// both reconstructed owners and their providers are ready for publication.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn reopen_wander_and_chase_store(
        session: &Session,
        pending: PendingWanderAndChaseReopen,
        store: benilla_world_store::native::wander_reconstruction::WanderStore,
        profile: &Profile,
        player: PlayerId,
        products: SamePolygonReopenProducts<'_>,
        corridor: &mut LocalCorridor<'_, '_, '_>,
        deadline: Instant,
    ) -> Result<
        (
            benilla_world_store::native::wander_reconstruction::WanderStore,
            NativeWorld,
            SamePolygonProviders,
            RouteCommitBoundary,
        ),
        NativeError,
    > {
        if store.requires_reopen() || store.recipe().checkpoint() != pending.checkpoint() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let (world, providers, boundary) = Self::reopen_wander_and_chase(
            session,
            pending,
            store.recipe().recipe_bytes(),
            profile,
            player,
            products,
            corridor,
            deadline,
        )?;
        Ok((store, world, providers, boundary))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn reopen_wander_and_chase(
        session: &Session,
        pending: PendingWanderAndChaseReopen,
        bytes: &[u8],
        profile: &Profile,
        player: PlayerId,
        products: SamePolygonReopenProducts<'_>,
        corridor: &mut LocalCorridor<'_, '_, '_>,
        deadline: Instant,
    ) -> Result<(NativeWorld, SamePolygonProviders, RouteCommitBoundary), NativeError> {
        Self::reopen_wander_and_chase_observed(
            session,
            pending,
            bytes,
            profile,
            player,
            products,
            corridor,
            deadline,
            |_| Ok(()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn reopen_wander_and_chase_observed(
        session: &Session,
        pending: PendingWanderAndChaseReopen,
        bytes: &[u8],
        profile: &Profile,
        player: PlayerId,
        products: SamePolygonReopenProducts<'_>,
        corridor: &mut LocalCorridor<'_, '_, '_>,
        deadline: Instant,
        observe: impl FnMut(DualReopenStage) -> Result<(), NativeError>,
    ) -> Result<(NativeWorld, SamePolygonProviders, RouteCommitBoundary), NativeError> {
        Self::reopen_wander_and_chase_with_checks(
            session,
            pending,
            bytes,
            profile,
            player,
            products,
            corridor,
            deadline,
            observe,
            |_, _, _, _| Ok(()),
        )
    }

    // Keep the store lease wrapper's frame separate from this bounded staging
    // frame; inlining would retain several complete ownership tuples at once.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn reopen_wander_and_chase_with_checks(
        session: &Session,
        pending: PendingWanderAndChaseReopen,
        bytes: &[u8],
        profile: &Profile,
        player: PlayerId,
        products: SamePolygonReopenProducts<'_>,
        corridor: &mut LocalCorridor<'_, '_, '_>,
        deadline: Instant,
        observe: impl FnMut(DualReopenStage) -> Result<(), NativeError>,
        before_install: impl FnOnce(
            &mut SourcePathfinderQueryProvider,
            &mut super::super::source_pathfinder_query::ReconstructedConstructorPairPlan,
            &benilla_native_nav::ReconstructedSourceWanderTerminal,
            &benilla_native_nav::ReconstructedSourceChaseFixedCache,
        ) -> Result<(), NativeError>,
    ) -> Result<(NativeWorld, SamePolygonProviders, RouteCommitBoundary), NativeError> {
        let observe = RefCell::new(observe);
        // The acceptance callback owns one complete pending world. Keep it off
        // every nested native/loader callback frame without cloning its authority.
        let pending = Box::new(pending);
        require_time(deadline).map_err(binding_error)?;
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(NativeError::TooLarge);
        }
        let saved: SavedDualRecipe =
            serde_json::from_slice(bytes).map_err(|_| NativeError::InvalidCheckpoint)?;
        if serde_json::to_vec(&saved).map_err(|_| NativeError::InvalidCheckpoint)? != bytes
            || saved.version != RECIPE_VERSION
            || saved.kind != RECIPE_KIND
            || saved.wander.version != WANDER_RECIPE_VERSION
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let native_wander = saved.wander.native().map_err(binding_error)?;
        let native_chase = saved.chase.native()?;
        native_wander.validate().map_err(binding_error)?;
        native_chase.validate().map_err(binding_error)?;
        let stage = products
            .inventory
            .stage_source_query_projection(profile)
            .map_err(binding_error)?;
        let mut binding = SamePolygonBinding::from_staged_geometry(
            products.raw_geometry,
            stage.geometry(),
            products.configuration,
        )
        .and_then(|binding| binding.with_near_point(&products.height, products.near_configuration))
        .map_err(binding_error)?;
        let environment = products.environment.binding();
        if environment.map != binding.vmap.map
            || environment.content != binding.vmap.content
            || environment.inventory != binding.vmap.inventory
            || environment.source_epoch != binding.vmap.source_epoch
        {
            return Err(NativeError::AuthorityBinding);
        }
        binding.environment_policy = Some(environment.policy);
        let navigation = stage.geometry().navigation_binding();
        if corridor.map != session.map
            || corridor.revision != products.raw_geometry
            || saved.wander.world != session.world
            || saved.wander.profile != session.profile
            || saved.wander.profile != profile.digest()
            || saved.wander.geometry != session.geometry
            || saved.wander.geometry != binding.geometry_revision().map_err(binding_error)?
            || saved.wander.checkpoint != pending.checkpoint().identity().state_digest
            || saved.wander.inventory != navigation.inventory
            || saved.wander.epoch != navigation.source_epoch
            || saved.wander.navigation != navigation.policy
            || saved.wander.configuration != stage.geometry().configuration_binding()
            || saved.wander.npc != pending.owner()
            || saved.wander.construction != pending.wander_construction()
            || saved.chase.construction != pending.chase_construction()
            || saved.wander.operation_count != pending.wander_cache().revision
            || saved.chase.operation_count != pending.chase_cache().revision
            || native_wander.config != products.configuration.normalization
            || native_chase.config != products.configuration.normalization
        {
            return Err(NativeError::AuthorityBinding);
        }
        let (wander_bodies, chase_bodies) = validate_history(&saved, session, &pending)?;
        let filter = SourceRouteFilterProvider::bind_with_same_polygon(
            session,
            profile,
            stage.filter_liquid().map_err(binding_error)?,
            binding,
        )
        .map_err(binding_error)?;
        let checkpoint = pending.checkpoint().clone();
        let wander_cache = pending.wander_cache().clone();
        let chase_cache = pending.chase_cache().clone();
        let wander_challenge = pending.wander_challenge().clone();
        let chase_challenge = pending.chase_challenge().clone();
        let corridor = RefCell::new(corridor);
        let observer = GeometryObserver {
            session,
            profile,
            player,
            filter: &filter,
            height: &products.height,
            deadline,
        };
        let ((world, mut plan), projection, wander_certificate, chase_certificate) = stage.reconstruct_wander_and_chase_observed(
            &native_wander, &wander_challenge, &native_chase, &chase_challenge, deadline,
            |geometry, event| {
                let final_terminal = matches!(&event, WanderStage::Terminal { ordinal, .. } if ordinal.checked_add(1) == Some(native_wander.operation_count));
                observer.wander(&saved.wander, &native_wander, &wander_bodies, geometry, event, &mut corridor.borrow_mut())
                    .map_err(|_| benilla_native_nav::SourceWanderReconstructionError::UnsupportedLineage)?;
                if final_terminal {
                    observe.borrow_mut()(DualReopenStage::WanderReplayed)
                        .map_err(|_| benilla_native_nav::SourceWanderReconstructionError::UnsupportedLineage)?;
                }
                Ok(())
            },
            |geometry, event| observer.chase(&saved.chase, &native_chase, &chase_bodies, geometry, event, &mut corridor.borrow_mut())
                .map_err(|_| benilla_native_nav::SourceChaseFixedCacheError::UnsupportedLineage),
            |geometry, pair| {
                let plan = SourcePathfinderQueryProvider::prepare_pair(
                    &pending, profile, geometry, &saved.wander.constructor, &saved.chase.constructor, &pair,
                ).map_err(binding_error)?;
                observe.borrow_mut()(DualReopenStage::OwnersReplayed)?;
                require_time(deadline).map_err(binding_error)?;
                let (world, pair) = (*pending).finish(pair)?;
                Ok::<_, NativeError>(((Box::new(world), Box::new(plan)), pair))
            },
        ).map_err(binding_error)?;
        let mut providers = SamePolygonProducts {
            projection,
            environment: products.environment,
            height: products.height,
            near_configuration: products.near_configuration,
            raw_geometry: products.raw_geometry,
            configuration: products.configuration,
        }
        .bind(session, profile, player)
        .map_err(binding_error)?;
        if providers.route.binding != binding {
            return Err(NativeError::AuthorityBinding);
        }
        before_install(
            &mut providers.route.queries,
            &mut plan,
            &wander_certificate,
            &chase_certificate,
        )?;
        let installed = providers
            .route
            .queries
            .install_batch(*plan, wander_certificate, chase_certificate)
            .map_err(binding_error)?;
        observe.borrow_mut()(DualReopenStage::ConstructorsInstalled)?;
        let boundary = RouteCommitBoundary::capture_reconstructed(
            &world,
            &ReopenBoundaryProof { checkpoint },
        )?;
        let wander_context = saved
            .wander
            .operations
            .last()
            .ok_or(NativeError::AuthorityBinding)?
            .calculation
            .clone();
        let chase_context = saved
            .chase
            .operations
            .last()
            .ok_or(NativeError::AuthorityBinding)?
            .calculation
            .clone();
        providers
            .route
            .candidates
            .try_reserve(2)
            .map_err(|_| NativeError::TooLarge)?;
        let wander_candidate = reconstructed_candidate(
            wander_context,
            SamePolygonTerminal::Wander(installed.wander_terminal),
            wander_cache,
        )?;
        let chase_candidate = reconstructed_candidate(
            chase_context,
            SamePolygonTerminal::Chase(installed.chase_terminal),
            chase_cache,
        )?;
        providers.route.candidates.push(GeneratorCandidates {
            npc: saved.wander.npc,
            construction: saved.wander.construction,
            kind: motion::PathfinderConstructorKind::WanderInitialized,
            proposals: vec![Rc::clone(&wander_candidate)],
            certified: vec![],
            committed: Some(wander_candidate),
            reconstruction: Some(WanderCapture {
                native: installed.wander_witness,
                constructor: Some(saved.wander.constructor),
                operations: saved.wander.operations,
            }),
            chase_reconstruction: None,
        });
        providers.route.candidates.push(GeneratorCandidates {
            npc: saved.wander.npc,
            construction: saved.chase.construction,
            kind: motion::PathfinderConstructorKind::ChaseLazy,
            proposals: vec![Rc::clone(&chase_candidate)],
            certified: vec![],
            committed: Some(chase_candidate),
            reconstruction: None,
            chase_reconstruction: Some(ChaseCapture {
                native: installed.chase_witness,
                constructor: Some(saved.chase.constructor),
                operations: saved.chase.operations,
            }),
        });
        providers
            .route
            .check_commit_binding(session, &boundary)
            .map_err(binding_error)?;
        providers.route.committed_boundary = Some(boundary.clone());
        boundary.validate_world(&world)?;
        // Re-encoding exercises both resumed witnesses and the original counts
        // before any world/provider is returned to its caller.
        if providers
            .route
            .encode_wander_and_chase_reconstruction(session, &world, &boundary)?
            != bytes
        {
            return Err(NativeError::AuthorityBinding);
        }
        observe.borrow_mut()(DualReopenStage::BeforePublication)?;
        require_time(deadline).map_err(binding_error)?;
        Ok((*world, providers, boundary))
    }
}

fn reconstructed_candidate(
    context: AuthorityRequest,
    terminal: SamePolygonTerminal,
    after_cache: motion::PathFinderCache,
) -> Result<Rc<SamePolygonCandidate>, NativeError> {
    let dispatch = route(&context)?
        .pathfinder_dispatch
        .clone()
        .ok_or(NativeError::AuthorityBinding)?;
    Ok(Rc::new(SamePolygonCandidate {
        context,
        dispatch,
        terminal,
        wander_tail: None,
        wander_prefix: None,
        after_cache,
    }))
}

fn route(
    context: &AuthorityRequest,
) -> Result<&benilla_world_runtime::native::CreatureRouteRequest, NativeError> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(NativeError::AuthorityBinding);
    };
    Ok(route)
}
fn binding_error(_: impl std::fmt::Debug) -> NativeError {
    NativeError::AuthorityBinding
}

fn validate_history(
    saved: &SavedDualRecipe,
    session: &Session,
    pending: &PendingWanderAndChaseReopen,
) -> Result<(Vec<Body>, Vec<Body>), NativeError> {
    session
        .reconstructed_dual_wander_body(pending, &saved.wander.constructor)
        .map_err(binding_error)?;
    session
        .reconstructed_dual_chase_body(pending, &saved.chase.constructor)
        .map_err(binding_error)?;
    let mut wander_bodies = Vec::new();
    for operation in &saved.wander.operations {
        session
            .reconstructed_dual_wander_body(pending, &operation.preparation)
            .map_err(binding_error)?;
        let body = session
            .reconstructed_dual_wander_body(pending, &operation.calculation)
            .map_err(binding_error)?;
        let prepare = route(&operation.preparation)?;
        let route = route(&operation.calculation)?;
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
            || sample.endpoint().map_err(binding_error)? != route.request.goal
            || route
                .request
                .prior_cache
                .as_ref()
                .map_or(0, |cache| cache.revision)
                != operation.ordinal
        {
            return Err(NativeError::AuthorityBinding);
        }
        pending.validate_wander_draws(&operation.draws, sample)?;
        wander_bodies.push(body);
    }
    pending.validate_wander_head(
        &saved
            .wander
            .operations
            .last()
            .ok_or(NativeError::AuthorityBinding)?
            .calculation,
    )?;
    if saved
        .chase
        .operations
        .first()
        .map(|operation| &operation.calculation)
        != Some(&saved.chase.constructor)
    {
        return Err(NativeError::AuthorityBinding);
    }
    let mut chase_bodies = Vec::new();
    for operation in &saved.chase.operations {
        let body = session
            .reconstructed_dual_chase_body(pending, &operation.calculation)
            .map_err(binding_error)?;
        let route = route(&operation.calculation)?;
        if !matches!(
            route.request.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) || route.request.target.is_none()
            || route.request.wander.is_some()
            || route.request.launch_origin != route.request.origin
            || route
                .request
                .prior_cache
                .as_ref()
                .map_or(0, |cache| cache.revision)
                != operation.ordinal
        {
            return Err(NativeError::AuthorityBinding);
        }
        chase_bodies.push(body);
    }
    pending.validate_chase_head(
        &saved
            .chase
            .operations
            .last()
            .ok_or(NativeError::AuthorityBinding)?
            .calculation,
    )?;
    Ok((wander_bodies, chase_bodies))
}

struct GeometryObserver<'a> {
    session: &'a Session,
    profile: &'a Profile,
    player: PlayerId,
    filter: &'a SourceRouteFilterProvider,
    height: &'a MapHeightSnapshot<'static>,
    deadline: Instant,
}
impl GeometryObserver<'_> {
    fn constructor(
        &self,
        context: &AuthorityRequest,
        expected: SourceOwnerFilter,
    ) -> Result<(), Failure> {
        let filter = self.filter.begin(self.session, context)?.query(
            RouteFilterRequest {
                snapshot: self.filter.dry_liquid().binding(),
                context: context.clone(),
                first_ticket: 1,
            },
            self.deadline,
        )?;
        if filter.filter().include_flags() != expected.include_flags
            || filter.filter().exclude_flags() != expected.exclude_flags
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    fn update(
        &self,
        context: &AuthorityRequest,
        expected: SourceOwnerFilter,
        first_ticket: u64,
    ) -> Result<(), Failure> {
        let filter = self
            .filter
            .begin(self.session, context)?
            .query_retained_update(
                RouteFilterRequest {
                    snapshot: self.filter.dry_liquid().binding(),
                    context: context.clone(),
                    first_ticket,
                },
                expected,
                None,
                self.deadline,
            )?;
        if filter.filter() != expected {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn wander(
        &self,
        saved: &SavedRecipe,
        native: &SourceWanderReconstructionRecipe,
        bodies: &[Body],
        geometry: &SourceQueryGeometry,
        event: WanderStage<'_>,
        corridor: &mut LocalCorridor<'_, '_, '_>,
    ) -> Result<(), Failure> {
        require_time(self.deadline)?;
        let operation = |ordinal| {
            saved
                .operations
                .iter()
                .enumerate()
                .find(|(_, operation)| operation.ordinal == ordinal)
                .ok_or(Failure::Binding)
        };
        match event {
            WanderStage::Constructor => self.constructor(&saved.constructor, native.filter)?,
            WanderStage::Prepared { ordinal, prefix } => {
                let (_, operation) = operation(ordinal)?;
                if prefix.current_selected() || prefix.cleared_active_len() != 0 {
                    return Err(Failure::Binding);
                }
                self.update(&operation.preparation, native.filter, 3)?;
            }
            WanderStage::Selected { ordinal } => {
                let (_, operation) = operation(ordinal)?;
                let route = route(&operation.calculation).map_err(|_| Failure::Binding)?;
                self.filter.begin(self.session, &operation.calculation)?;
                same_polygon_model(self.profile, self.session.world, self.player, route)?;
                if route
                    .request
                    .origin
                    .distance_squared(route.request.goal)
                    .map_err(|_| Failure::Invalid)?
                    .as_f32()
                    >= 0.01
                    && route
                        .require_owner_ignore_pathfinding(self.profile)
                        .map_err(|_| Failure::Binding)?
                        .ignore_pathfinding()
                {
                    return Err(Failure::UnsupportedQuery);
                }
            }
            WanderStage::Terminal { ordinal, terminal } => {
                let (index, operation) = operation(ordinal)?;
                let route = route(&operation.calculation).map_err(|_| Failure::Binding)?;
                if terminal.from() != bits(route.request.origin)
                    || terminal.raw_end() != bits(route.request.goal)
                    || terminal.filter() != native.filter
                {
                    return Err(Failure::Binding);
                }
                if terminal.no_path_reason()
                    == Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance)
                {
                    return Ok(());
                }
                let position = bits(route.request.origin);
                let request = LiquidRequest {
                    snapshot: geometry.liquid().binding(),
                    ticket: 5,
                    position,
                };
                let water = geometry
                    .liquid()
                    .query(request, self.deadline)
                    .map_err(Failure::product)?;
                if water.request != request || water.status != LiquidStatus::NoWater {
                    return Err(Failure::Binding);
                }
                let request = MapHeightRequest {
                    snapshot: self.height.binding(),
                    ticket: 6,
                    position,
                    swim: false,
                };
                let ground = self
                    .height
                    .query(request, self.deadline)
                    .map_err(Failure::product)?;
                if ground.request != request
                    || !f32::from_bits(ground.height).is_finite()
                    || (route.request.origin.z.as_f32() - f32::from_bits(ground.height)).abs() > 0.5
                {
                    return Err(Failure::UnsupportedQuery);
                }
                if terminal.path_type() == 8 {
                    return Ok(());
                }
                if terminal.path_type() != 1
                    || terminal.active_len() != 1
                    || terminal.points() != [position, terminal.actual_end()]
                {
                    return Err(Failure::Binding);
                }
                self.segment(
                    &operation.calculation,
                    terminal.actual_end(),
                    *bodies.get(index).ok_or(Failure::Binding)?,
                    geometry,
                    corridor,
                )?;
            }
        }
        require_time(self.deadline)
    }
    #[allow(clippy::too_many_arguments)]
    fn chase(
        &self,
        saved: &SavedChaseRecipe,
        native: &SourceChaseFixedCacheRecipe,
        bodies: &[Body],
        geometry: &SourceQueryGeometry,
        event: ChaseStage<'_>,
        corridor: &mut LocalCorridor<'_, '_, '_>,
    ) -> Result<(), Failure> {
        require_time(self.deadline)?;
        let operation = |ordinal| {
            saved
                .operations
                .iter()
                .enumerate()
                .find(|(_, operation)| operation.ordinal == ordinal)
                .ok_or(Failure::Binding)
        };
        match event {
            ChaseStage::Constructor { .. } => {
                self.constructor(&saved.constructor, native.filter)?
            }
            ChaseStage::PreSelection {
                operation: input, ..
            } => {
                // TargetedMovementGenerator checks this gate before every calculate,
                // including the retained final operation whose original ordinal survives.
                let (_, operation) = operation(input.ordinal)?;
                let route = route(&operation.calculation).map_err(|_| Failure::Binding)?;
                if input.from != bits(route.request.origin) || input.to != bits(route.request.goal)
                {
                    return Err(Failure::Binding);
                }
                self.filter.begin(self.session, &operation.calculation)?;
                let model =
                    same_polygon_model(self.profile, self.session.world, self.player, route)?;
                let gate = LoadedChaseGate {
                    profile: self.profile,
                    player: self.player,
                    liquid: geometry.liquid(),
                    vmap: geometry.static_vmap(),
                    model: model.chase.as_ref().ok_or(Failure::Binding)?,
                }
                .query(&operation.calculation, 3, self.deadline, |_| {})?;
                if gate.selection != ChaseGateSelection::Straight {
                    return Err(Failure::UnsupportedQuery);
                }
            }
            ChaseStage::Selected { ordinal, .. } => {
                let (_, operation) = operation(ordinal)?;
                let route = route(&operation.calculation).map_err(|_| Failure::Binding)?;
                if route
                    .require_owner_ignore_pathfinding(self.profile)
                    .map_err(|_| Failure::Binding)?
                    .ignore_pathfinding()
                {
                    return Err(Failure::UnsupportedQuery);
                }
            }
            ChaseStage::Ready { ordinal, presence } => {
                let (_, operation) = operation(ordinal)?;
                if !presence.both_present() {
                    return Err(Failure::MissingGeometry);
                }
                self.update(&operation.calculation, native.filter, 5)?;
            }
            ChaseStage::Terminal { ordinal, terminal } => {
                let (index, operation) = operation(ordinal)?;
                let route = route(&operation.calculation).map_err(|_| Failure::Binding)?;
                if terminal.compatibility().from() != bits(route.request.origin)
                    || terminal.compatibility().to() != bits(route.request.goal)
                    || terminal.actual_end() != bits(route.request.goal)
                    || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
                    || terminal.path_type() != 1
                    || terminal.compatibility().active_len() != 1
                    || terminal.compatibility().filter() != native.filter
                {
                    return Err(Failure::Binding);
                }
                self.segment(
                    &operation.calculation,
                    terminal.actual_end(),
                    *bodies.get(index).ok_or(Failure::Binding)?,
                    geometry,
                    corridor,
                )?;
            }
        }
        require_time(self.deadline)
    }
    fn segment(
        &self,
        context: &AuthorityRequest,
        end: [u32; 3],
        body: Body,
        geometry: &SourceQueryGeometry,
        corridor: &mut LocalCorridor<'_, '_, '_>,
    ) -> Result<(), Failure> {
        let route = route(context).map_err(|_| Failure::Binding)?;
        let position = bits(route.request.origin);
        corridor.certify(position.map(f32::from_bits), end.map(f32::from_bits), body)?;
        let height = route.request.owner_collision_height.bits();
        let [from, to] = chase_gate::los_endpoints_bits(position, end, height, height)
            .map_err(|_| Failure::Invalid)?;
        let request = RayRequest {
            snapshot: geometry.static_vmap().binding(),
            ticket: 7,
            from,
            to,
            ignore_m2: false,
        };
        let reply = geometry
            .static_vmap()
            .query(request, self.deadline)
            .map_err(Failure::product)?;
        if reply.request != request || !matches!(reply.result, StaticRay::Miss) {
            return Err(Failure::Obstacle);
        }
        require_time(self.deadline)
    }
}
