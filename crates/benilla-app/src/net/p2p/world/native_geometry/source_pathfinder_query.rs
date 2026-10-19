// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual constructor-dispatch to retained source1024 query dependency. No filter,
//! polygon cache, Route fact or movement admission is supplied by this boundary.
use super::{
    core::Session,
    products::{navigation::SnapshotBinding, source_query::SourceMapQuerySnapshot},
};
use benilla_native_nav::{
    LoadedSourceTilePresence, QueryProfile, RetainedSourceQuery, SelectedSourceQuery,
    SourceMapCoordError, SourceMapQuerySlot, SourceQuerySelectionError, SOURCE_MAP_QUERY_POLICY,
};
use benilla_sim::{creature_ai::Point, creature_motion::QueryBinding};
use benilla_world_runtime::{
    native::{AuthorityRequest, MotionQuery, PathfinderDispatchReceipt, Profile, Query},
    NpcId,
};
use sha2::{Digest, Sha256};
use std::{rc::Rc, time::Instant};

const MAX_CONSTRUCTORS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConstructorQueryError {
    Binding,
    MissingConstructor,
    Budget,
    Coordinates(SourceMapCoordError),
    Native(benilla_native_nav::Error),
}

/// One immutable profile/world and preloaded source map query. Entries are live
/// capabilities, not serialized registry state or reconstructed after-cache refs.
/// The native query uses Rc/RefCell: this provider stays on its owner thread and
/// must use non-send placement, never an ordinary Send + Sync Bevy Resource.
pub(crate) struct SourcePathfinderQueryProvider {
    identity: Rc<()>,
    world: [u8; 16],
    profile: Profile,
    navigation: SourceMapQuerySnapshot,
    constructors: Vec<RegisteredConstructor>,
}
struct RegisteredConstructor {
    npc: NpcId,
    construction: QueryBinding,
    first_event: PathfinderDispatchReceipt,
    // Reopened entries use a fresh local identity, never a historical live seal.
    reopened: Option<Rc<()>>,
    query: RetainedSourceQuery,
}

/// Private unpublished installation plan. Historical dispatch is metadata only;
/// actual query ownership comes exclusively from the executed native certificate.
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(super) struct ReconstructedConstructorOrigin {
    provider: Rc<()>,
    challenge: benilla_native_nav::SourceWanderReconstructionChallenge,
    npc: NpcId,
    construction: QueryBinding,
    first_event: PathfinderDispatchReceipt,
    snapshot: SnapshotBinding,
    configuration: [u8; 32],
    semantic: benilla_native_nav::SourceWanderSemanticState,
    recipe: benilla_native_nav::SourceWanderReconstructionRecipe,
}

/// Prepared before the pending world consumes its intact native pair. The guard
/// retains that exact allocation; there is no provider identity before adoption.
/// Private fields and move-only ownership prevent a decoded or duplicated plan.
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(super) struct ReconstructedConstructorPairPlan {
    query: RetainedSourceQuery,
    world: [u8; 16],
    profile: Profile,
    npc: NpcId,
    snapshot: SnapshotBinding,
    configuration: [u8; 32],
    wander_construction: QueryBinding,
    chase_construction: QueryBinding,
    wander_first: PathfinderDispatchReceipt,
    chase_first: PathfinderDispatchReceipt,
    wander_challenge: benilla_native_nav::SourceWanderReconstructionChallenge,
    chase_challenge: benilla_native_nav::SourceChaseFixedCacheChallenge,
    wander_semantic: benilla_native_nav::SourceWanderSemanticState,
    chase_semantic: benilla_native_nav::SourceChaseSemanticState,
    wander_recipe: benilla_native_nav::SourceWanderReconstructionRecipe,
    chase_recipe: benilla_native_nav::SourceChaseFixedCacheRecipe,
}

/// Independent replayed owners returned only after both registry entries exist.
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(super) struct InstalledConstructorPair {
    pub(super) wander_terminal: benilla_native_nav::LoadedSourceWanderTerminal,
    pub(super) wander_witness: benilla_native_nav::SourceWanderWitness,
    pub(super) chase_terminal: benilla_native_nav::LoadedSourceSamePolygon,
    pub(super) chase_witness: benilla_native_nav::SourceChaseFixedCacheWitness,
}

/// This borrow is only the query fields of the actual modeled owner constructor.
/// It is neither createFilter evidence nor proof that a whole calculate ran.
pub(crate) struct SourceConstructorQuery<'a> {
    provider: Rc<()>,
    entry: &'a mut RegisteredConstructor,
    snapshot: SnapshotBinding,
    dispatch: PathfinderDispatchReceipt,
    from: [u32; 3],
    to: [u32; 3],
}

/// A successfully validated registered constructor, never a second native query.
/// Private fields retain the exact allocation and cannot adopt a replacement seal.
pub(super) struct RegisteredConstructorHandle {
    provider: Rc<()>,
    npc: NpcId,
    construction: QueryBinding,
    first_event: PathfinderDispatchReceipt,
    reopened: Option<Rc<()>>,
    dispatch: PathfinderDispatchReceipt,
    snapshot: SnapshotBinding,
    from: [u32; 3],
    to: [u32; 3],
}

impl RegisteredConstructorHandle {
    /// Private retained-state invariant: a valid handle from another owner or
    /// dispatch must not be substituted into this calculation's prepared fields.
    pub(super) fn matches_context(&self, context: &AuthorityRequest) -> bool {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return false;
        };
        context.binding.npc == Some(self.npc)
            && route.pathfinder_dispatch.as_ref().is_some_and(|dispatch| {
                self.dispatch.same_live_dispatch(dispatch)
                    && self.construction == dispatch.construction_binding()
            })
            && self.from
                == [
                    route.request.origin.x.bits(),
                    route.request.origin.y.bits(),
                    route.request.origin.z.bits(),
                ]
            && self.to
                == [
                    route.request.goal.x.bits(),
                    route.request.goal.y.bits(),
                    route.request.goal.z.bits(),
                ]
    }
}

pub(crate) fn geometry_revision(binding: SnapshotBinding) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"benilla-owned-pathfinder-constructor-query-v1");
    hash.update(SOURCE_MAP_QUERY_POLICY);
    hash.update(binding.map.to_le_bytes());
    hash.update(binding.content);
    hash.update(binding.source_epoch);
    hash.update(binding.inventory);
    hash.update(binding.policy);
    hash.finalize().into()
}
impl SourcePathfinderQueryProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        navigation: SourceMapQuerySnapshot,
    ) -> Result<Self, ConstructorQueryError> {
        let expected = geometry_revision(navigation.binding());
        Self::bind_revision(session, profile, navigation, expected)
    }
    pub(super) fn bind_with_same_polygon(
        session: &Session,
        profile: &Profile,
        navigation: SourceMapQuerySnapshot,
        binding: super::source_route_same_polygon::SamePolygonBinding,
    ) -> Result<Self, ConstructorQueryError> {
        if navigation.binding() != binding.navigation
            || navigation.configuration_binding() != binding.selected_configuration
        {
            return Err(ConstructorQueryError::Binding);
        }
        let expected = binding
            .geometry_revision()
            .map_err(|_| ConstructorQueryError::Binding)?;
        Self::bind_revision(session, profile, navigation, expected)
    }
    pub(super) fn bind_with_resident_filter(
        session: &Session,
        profile: &Profile,
        navigation: SourceMapQuerySnapshot,
        height: super::products::source_height::Binding,
    ) -> Result<Self, ConstructorQueryError> {
        let expected =
            super::source_route_filter::resident::geometry_revision(height, navigation.binding());
        Self::bind_revision(session, profile, navigation, expected)
    }
    fn bind_revision(
        session: &Session,
        profile: &Profile,
        navigation: SourceMapQuerySnapshot,
        expected: [u8; 32],
    ) -> Result<Self, ConstructorQueryError> {
        profile
            .validate()
            .map_err(|_| ConstructorQueryError::Binding)?;
        let configuration = profile
            .pathfinder_configuration()
            .ok_or(ConstructorQueryError::Binding)?;
        let binding = navigation.binding();
        if session.world == [0; 16]
            || session.profile != profile.digest()
            || session.policy != profile.authority_policy
            || session.content != profile.content_digest
            || session.geometry != profile.geometry_revision
            || session.geometry != expected
            || session.map != profile.map_id
            || binding.map != profile.map_id
            || binding.content != profile.content_digest
            || navigation.configuration_binding() != configuration.binding()
            || navigation.query().slot()
                != (SourceMapQuerySlot {
                    map_id: profile.map_id,
                    instance_id: configuration.selected().source_instance_id,
                })
            || navigation.query().query_profile() != QueryProfile::SourceMapInstance1024
            || navigation.query().query_capacity() != 1024
        {
            return Err(ConstructorQueryError::Binding);
        }
        Ok(Self {
            identity: Rc::new(()),
            world: session.world,
            profile: profile.clone(),
            navigation,
            constructors: vec![],
        })
    }
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.navigation.binding()
    }
    pub(crate) fn constructor_count(&self) -> usize {
        self.constructors.len()
    }

    /// Inspect a live registered constructor only after checking its retained
    /// query is still the provider's original native allocation.
    #[cfg(test)]
    pub(super) fn registered_constructor_for(
        &self,
        npc: NpcId,
        construction: QueryBinding,
    ) -> Option<(usize, PathfinderDispatchReceipt, bool)> {
        let entry = self
            .constructors
            .iter()
            .find(|entry| entry.npc == npc && entry.construction == construction)?;
        entry.query.check_query(self.navigation.query()).ok()?;
        Some((
            Rc::as_ptr(&self.identity) as usize,
            entry.first_event.clone(),
            entry.query.current_selected(),
        ))
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn nav_query(&self) -> &benilla_native_nav::SourceMapQuery {
        self.navigation.query()
    }
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn shares_native_query(&self, other: &Self) -> bool {
        self.navigation
            .query()
            .retain_default()
            .check_query(other.navigation.query())
            .is_ok()
    }
    #[cfg(test)]
    pub(super) fn snapshot_binding(&self) -> SnapshotBinding {
        self.navigation.binding()
    }
    #[cfg(test)]
    pub(super) fn configuration_binding(&self) -> [u8; 32] {
        self.navigation.configuration_binding()
    }

    /// Validate both historical constructors while the runtime pending and the
    /// fresh native pair are still intact. The inventory stage cannot swap its
    /// immutable metadata or supply a replacement query after this preparation.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn prepare_pair(
        pending: &benilla_world_runtime::native::PendingWanderAndChaseReopen,
        profile: &Profile,
        geometry: &super::products::source_query::SourceQueryGeometry,
        first_wander: &AuthorityRequest,
        first_chase: &AuthorityRequest,
        pair: &benilla_native_nav::ReconstructedSourceWanderAndChase,
    ) -> Result<ReconstructedConstructorPairPlan, ConstructorQueryError> {
        use benilla_sim::creature_motion as motion;
        profile
            .validate()
            .map_err(|_| ConstructorQueryError::Binding)?;
        pending
            .validate_pair(pair)
            .map_err(|_| ConstructorQueryError::Binding)?;
        pending
            .validate_wander_operation(first_wander)
            .map_err(|_| ConstructorQueryError::Binding)?;
        pending
            .validate_chase_operation(first_chase)
            .map_err(|_| ConstructorQueryError::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(wander)) = &first_wander.query else {
            return Err(ConstructorQueryError::Binding);
        };
        let Query::SourceMotion(MotionQuery::Route(chase)) = &first_chase.query else {
            return Err(ConstructorQueryError::Binding);
        };
        let wander_first = wander
            .pathfinder_dispatch
            .as_ref()
            .ok_or(ConstructorQueryError::Binding)?;
        let chase_first = chase
            .pathfinder_dispatch
            .as_ref()
            .ok_or(ConstructorQueryError::Binding)?;
        let checkpoint = pending.checkpoint().identity();
        let snapshot = geometry.navigation_binding();
        let configuration = profile
            .pathfinder_configuration()
            .ok_or(ConstructorQueryError::Binding)?;
        let slot = SourceMapQuerySlot {
            map_id: profile.map_id,
            instance_id: configuration.selected().source_instance_id,
        };
        let wander_recipe = pair
            .wander()
            .witness()
            .recipe()
            .map_err(|_| ConstructorQueryError::Binding)?;
        let chase_recipe = pair
            .chase()
            .witness()
            .recipe()
            .map_err(|_| ConstructorQueryError::Binding)?;
        let cold_chase = chase_recipe
            .operations
            .first()
            .ok_or(ConstructorQueryError::Binding)?;
        fn bits(point: Point) -> [u32; 3] {
            [point.x.bits(), point.y.bits(), point.z.bits()]
        }
        if checkpoint.domain != benilla_world_runtime::native::Domain::SyntheticComponentTest
            || checkpoint.world == [0; 16]
            || checkpoint.profile != profile.digest()
            || snapshot.map != profile.map_id
            || snapshot.content != profile.content_digest
            || geometry.configuration_binding() != configuration.binding()
            || pair.query().slot() != slot
            || pair.query().query_profile() != QueryProfile::SourceMapInstance1024
            || pair.query().query_capacity() != 1024
            || pending.wander_construction().query_id >= pending.chase_construction().query_id
            || pending.wander_construction().now_ms > pending.chase_construction().now_ms
            || pending.wander_construction().identity != pending.chase_construction().identity
            || wander.request.kind != motion::RouteKind::WanderPrepare
            || !matches!(
                chase.request.kind,
                motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
            )
            || wander.request.prior_cache.is_some()
            || chase.request.prior_cache.is_some()
            || chase.request.target.is_none()
            || chase.request.wander.is_some()
            || chase.request.launch_origin != chase.request.origin
            || wander_recipe.operation_count != pending.wander_cache().revision
            || chase_recipe.operation_count != pending.chase_cache().revision
            || wander_recipe.slot != slot
            || chase_recipe.slot != slot
            || cold_chase.ordinal != 0
            || cold_chase.from != bits(chase.request.origin)
            || cold_chase.to != bits(chase.request.goal)
        {
            return Err(ConstructorQueryError::Binding);
        }
        for (context, route, event, construction, kind, cache) in [
            (
                first_wander,
                wander,
                wander_first,
                pending.wander_construction(),
                motion::PathfinderConstructorKind::WanderInitialized,
                pending.wander_cache(),
            ),
            (
                first_chase,
                chase,
                chase_first,
                pending.chase_construction(),
                motion::PathfinderConstructorKind::ChaseLazy,
                pending.chase_cache(),
            ),
        ] {
            let request = &route.request;
            let dispatch = request
                .pathfinder_dispatch
                .ok_or(ConstructorQueryError::Binding)?;
            if context.binding.world != checkpoint.world
                || context.binding.profile != checkpoint.profile
                || context.binding.npc != Some(pending.owner())
                || context.binding.tick == 0
                || context.binding.tick > checkpoint.tick
                || request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
                || event.world() != checkpoint.world
                || event.npc() != pending.owner()
                || event.profile_binding() != checkpoint.profile
                || event.configuration_binding() != geometry.configuration_binding()
                || event.constructor_kind() != kind
                || dispatch.constructor_kind() != kind
                || !event.first_construction()
                || !dispatch.first_construction()
                || event.construction_binding() != construction
                || dispatch.construction_binding() != construction
                || event.dispatch_binding() != request.binding
                || cache.binding.identity != construction.identity
                || cache.binding.owner_generation != pending.owner().generation
                || cache.binding.inventory != snapshot.inventory
                || cache.binding.source_epoch != snapshot.source_epoch
                || cache.binding.navigation_policy != profile.geometry_revision
            {
                return Err(ConstructorQueryError::Binding);
            }
        }
        pair.check_query(pair.query())
            .map_err(ConstructorQueryError::Native)?;
        Ok(ReconstructedConstructorPairPlan {
            query: pair.query().retain_default(),
            world: checkpoint.world,
            profile: profile.clone(),
            npc: pending.owner(),
            snapshot,
            configuration: geometry.configuration_binding(),
            wander_construction: pending.wander_construction(),
            chase_construction: pending.chase_construction(),
            wander_first: wander_first.clone(),
            chase_first: chase_first.clone(),
            wander_challenge: pending.wander_challenge().clone(),
            chase_challenge: pending.chase_challenge().clone(),
            wander_semantic: pair.wander().terminal().semantic_state(),
            chase_semantic: pair.chase().terminal().chase_semantic_state(),
            wander_recipe,
            chase_recipe,
        })
    }

    /// Both typed certificates and the pre-adoption allocation guard must match
    /// this pristine provider. All validation and reservation precede mutation.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn install_batch(
        &mut self,
        plan: ReconstructedConstructorPairPlan,
        wander: benilla_native_nav::ReconstructedSourceWanderTerminal,
        chase: benilla_native_nav::ReconstructedSourceChaseFixedCache,
    ) -> Result<InstalledConstructorPair, ConstructorQueryError> {
        self.validate_batch(&plan, &wander, &chase)?;
        self.constructors
            .try_reserve(2)
            .map_err(|_| ConstructorQueryError::Budget)?;
        let (wander_query, wander_terminal, wander_witness) = wander.into_parts();
        let (chase_query, chase_terminal, chase_witness) = chase.into_parts();
        let wander_entry = RegisteredConstructor {
            npc: plan.npc,
            construction: plan.wander_construction,
            first_event: plan.wander_first,
            reopened: Some(Rc::new(())),
            query: wander_query,
        };
        let chase_entry = RegisteredConstructor {
            npc: plan.npc,
            construction: plan.chase_construction,
            first_event: plan.chase_first,
            reopened: Some(Rc::new(())),
            query: chase_query,
        };
        self.constructors.push(wander_entry);
        self.constructors.push(chase_entry);
        Ok(InstalledConstructorPair {
            wander_terminal,
            wander_witness,
            chase_terminal,
            chase_witness,
        })
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    fn validate_batch(
        &self,
        plan: &ReconstructedConstructorPairPlan,
        wander: &benilla_native_nav::ReconstructedSourceWanderTerminal,
        chase: &benilla_native_nav::ReconstructedSourceChaseFixedCache,
    ) -> Result<(), ConstructorQueryError> {
        use benilla_sim::creature_motion::PathfinderConstructorKind;
        if !self.constructors.is_empty()
            || self.world != plan.world
            || self.profile != plan.profile
            || self.navigation.binding() != plan.snapshot
            || self.navigation.configuration_binding() != plan.configuration
            || !wander.matches_challenge(&plan.wander_challenge)
            || !chase.matches_challenge(&plan.chase_challenge)
            || wander.terminal().semantic_state() != plan.wander_semantic
            || chase.terminal().chase_semantic_state() != plan.chase_semantic
            || wander
                .witness()
                .recipe()
                .map_err(|_| ConstructorQueryError::Binding)?
                != plan.wander_recipe
            || chase
                .witness()
                .recipe()
                .map_err(|_| ConstructorQueryError::Binding)?
                != plan.chase_recipe
        {
            return Err(ConstructorQueryError::Binding);
        }
        for (construction, event, kind) in [
            (
                plan.wander_construction,
                &plan.wander_first,
                PathfinderConstructorKind::WanderInitialized,
            ),
            (
                plan.chase_construction,
                &plan.chase_first,
                PathfinderConstructorKind::ChaseLazy,
            ),
        ] {
            if event.world() != plan.world
                || event.npc() != plan.npc
                || event.profile_binding() != self.profile.digest()
                || event.configuration_binding() != plan.configuration
                || event.constructor_kind() != kind
                || event.construction_binding() != construction
                || !event.first_construction()
            {
                return Err(ConstructorQueryError::Binding);
            }
        }
        for query in [&plan.query, wander.retained_query(), chase.retained_query()] {
            query
                .check_query(self.navigation.query())
                .map_err(ConstructorQueryError::Native)?;
        }
        Ok(())
    }

    /// Historical metadata checks do not claim or recreate its constructor seal.
    /// The caller must keep this provider unpublished until pending.finish succeeds.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn validate_reconstructed(
        &self,
        pending: &benilla_world_runtime::native::PendingWanderReopen,
        first_prepare: &AuthorityRequest,
        certificate: &benilla_native_nav::ReconstructedSourceWanderTerminal,
    ) -> Result<ReconstructedConstructorOrigin, ConstructorQueryError> {
        use benilla_sim::creature_motion as motion;
        let Query::SourceMotion(MotionQuery::Route(route)) = &first_prepare.query else {
            return Err(ConstructorQueryError::Binding);
        };
        let checkpoint = pending.checkpoint().identity();
        let binding = &first_prepare.binding;
        let request = &route.request;
        let event = route
            .pathfinder_dispatch
            .as_ref()
            .ok_or(ConstructorQueryError::Binding)?;
        let dispatch = request
            .pathfinder_dispatch
            .ok_or(ConstructorQueryError::Binding)?;
        request
            .validate_pathfinder_dispatch()
            .map_err(|_| ConstructorQueryError::Binding)?;
        route
            .owner
            .validate_context(&self.profile, binding)
            .map_err(|_| ConstructorQueryError::Binding)?;
        let now = binding
            .tick
            .checked_mul(u64::from(self.profile.tick_ms))
            .and_then(|elapsed| self.profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(ConstructorQueryError::Binding)?;
        let navigation = self.navigation.binding();
        let cache = pending.cache();
        let terminal = certificate.terminal();
        if !self.constructors.is_empty()
            || !certificate.matches_challenge(pending.challenge())
            || checkpoint.domain != benilla_world_runtime::native::Domain::SyntheticComponentTest
            || checkpoint.world != self.world
            || checkpoint.profile != self.profile.digest()
            || binding.world != self.world
            || binding.profile != checkpoint.profile
            || binding.authority_policy != self.profile.authority_policy
            || binding.geometry != self.profile.geometry_revision
            || binding.npc != Some(pending.owner())
            || binding.tick == 0
            || binding.tick > checkpoint.tick
            || request.kind != motion::RouteKind::WanderPrepare
            || request.binding.now_ms != now
            || request.binding.identity != pending.construction().identity
            || request.prior_cache.is_some()
            || request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || route.owner_position != request.origin
            || event.world() != self.world
            || event.npc() != pending.owner()
            || event.dispatch_tick() != binding.tick
            || event.profile_binding() != checkpoint.profile
            || event.configuration_binding() != self.navigation.configuration_binding()
            || event.constructor_kind() != motion::PathfinderConstructorKind::WanderInitialized
            || !event.first_construction()
            || !dispatch.first_construction()
            || event.construction_binding() != pending.construction()
            || dispatch.construction_binding() != pending.construction()
            || event.dispatch_binding() != request.binding
            || cache.binding.identity != pending.construction().identity
            || cache.binding.owner_generation != pending.owner().generation
            || cache.binding.inventory != navigation.inventory
            || cache.binding.source_epoch != navigation.source_epoch
            || cache.binding.navigation_policy != self.profile.geometry_revision
            || certificate.witness().operation_count() != cache.revision
            || terminal.slot() != self.navigation.query().slot()
            || terminal.query_capacity() != 1024
            || terminal.config().normalize_z
            || terminal.config().ignore_normalization
            || terminal.filter().include_flags != cache.include_flags
            || terminal.filter().exclude_flags != cache.exclude_flags
            || terminal.storage() != cache.poly_refs
            || terminal.active_len() != usize::from(cache.active_len)
            || terminal.path_type() != cache.path_type
            || terminal.actual_end()
                != [
                    cache.requested_end.x.bits(),
                    cache.requested_end.y.bits(),
                    cache.requested_end.z.bits(),
                ]
        {
            return Err(ConstructorQueryError::Binding);
        }
        certificate
            .retained_query()
            .check_query(self.navigation.query())
            .map_err(ConstructorQueryError::Native)?;
        let recipe = certificate
            .witness()
            .recipe()
            .map_err(|_| ConstructorQueryError::Binding)?;
        Ok(ReconstructedConstructorOrigin {
            provider: Rc::clone(&self.identity),
            challenge: pending.challenge().clone(),
            npc: pending.owner(),
            construction: pending.construction(),
            first_event: event.clone(),
            snapshot: navigation,
            configuration: self.navigation.configuration_binding(),
            semantic: terminal.semantic_state(),
            recipe,
        })
    }

    /// Consume the certificate returned by pending.finish. Failures never add an
    /// entry; the caller discards the unpublished world/provider pair.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn install_reconstructed(
        &mut self,
        origin: ReconstructedConstructorOrigin,
        certificate: benilla_native_nav::ReconstructedSourceWanderTerminal,
    ) -> Result<
        (
            benilla_native_nav::LoadedSourceWanderTerminal,
            benilla_native_nav::SourceWanderWitness,
        ),
        ConstructorQueryError,
    > {
        if !self.constructors.is_empty()
            || !Rc::ptr_eq(&origin.provider, &self.identity)
            || origin.snapshot != self.navigation.binding()
            || origin.configuration != self.navigation.configuration_binding()
            || !certificate.matches_challenge(&origin.challenge)
            || certificate.terminal().semantic_state() != origin.semantic
            || certificate
                .witness()
                .recipe()
                .map_err(|_| ConstructorQueryError::Binding)?
                != origin.recipe
        {
            return Err(ConstructorQueryError::Binding);
        }
        certificate
            .retained_query()
            .check_query(self.navigation.query())
            .map_err(ConstructorQueryError::Native)?;
        self.constructors
            .try_reserve(1)
            .map_err(|_| ConstructorQueryError::Budget)?;
        let (query, terminal, witness) = certificate.into_parts();
        self.constructors.push(RegisteredConstructor {
            npc: origin.npc,
            construction: origin.construction,
            first_event: origin.first_event,
            reopened: Some(Rc::new(())),
            query,
        });
        Ok((terminal, witness))
    }

    /// Called only after the complete outer ACK validates. Current and suspended
    /// generators keep their constructors; replaced and local Home owners do not.
    pub(super) fn retain_committed_generators(
        &mut self,
        boundary: &super::route_commit::RouteCommitBoundary,
    ) {
        self.constructors.retain(|entry| {
            entry.first_event.constructor_kind()
                != benilla_sim::creature_motion::PathfinderConstructorKind::HomeLocal
                && boundary.owner_heads().any(|(npc, head)| {
                    npc == entry.npc
                        && head.construction() == Some(entry.construction)
                        && head.constructor_kind() == Some(entry.first_event.constructor_kind())
                })
        });
    }

    pub(super) fn prepare_constructor(
        &mut self,
        context: &AuthorityRequest,
    ) -> Result<RegisteredConstructorHandle, ConstructorQueryError> {
        let query = self.retain_constructor(context)?;
        Ok(Self::constructor_handle(query))
    }

    fn constructor_handle(query: SourceConstructorQuery<'_>) -> RegisteredConstructorHandle {
        RegisteredConstructorHandle {
            provider: query.provider,
            npc: query.entry.npc,
            construction: query.entry.construction,
            first_event: query.entry.first_event.clone(),
            reopened: query.entry.reopened.clone(),
            dispatch: query.dispatch,
            snapshot: query.snapshot,
            from: query.from,
            to: query.to,
        }
    }

    /// Use only the core's completed original-job dispatch receipt. Loaded query,
    /// configuration, capacity and one-use constructor claim remain checked here.
    pub(super) fn prepare_constructor_for_job(
        &mut self,
        job: &super::core::RouteJob<'_>,
    ) -> Result<RegisteredConstructorHandle, super::source_route_same_polygon::SamePolygonError>
    {
        let event = job.validated_dispatch(&self.profile)?;
        if job.session().world != self.world {
            return Err(ConstructorQueryError::Binding.into());
        }
        let query = self.retain_validated_constructor(job.request(), event)?;
        Ok(Self::constructor_handle(query))
    }

    /// The already-owned pre-draw constructor is rebound to its sampled continuation.
    /// No constructor claim, default query allocation or SetCurrentNavMesh runs here.
    pub(super) fn continue_wander_constructor(
        &mut self,
        prefix: &RegisteredConstructorHandle,
        context: &AuthorityRequest,
    ) -> Result<RegisteredConstructorHandle, ConstructorQueryError> {
        let event = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| ConstructorQueryError::Binding)?;
        self.continue_validated_wander_constructor(prefix, context, event)
    }

    pub(super) fn continue_wander_constructor_for_job(
        &mut self,
        prefix: &RegisteredConstructorHandle,
        job: &super::core::RouteJob<'_>,
    ) -> Result<RegisteredConstructorHandle, super::source_route_same_polygon::SamePolygonError>
    {
        let event = job.validated_dispatch(&self.profile)?;
        if job.session().world != self.world {
            return Err(ConstructorQueryError::Binding.into());
        }
        Ok(self.continue_validated_wander_constructor(prefix, job.request(), event)?)
    }

    fn continue_validated_wander_constructor(
        &mut self,
        prefix: &RegisteredConstructorHandle,
        context: &AuthorityRequest,
        event: &PathfinderDispatchReceipt,
    ) -> Result<RegisteredConstructorHandle, ConstructorQueryError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(ConstructorQueryError::Binding);
        };
        if !route
            .pathfinder_dispatch
            .as_ref()
            .is_some_and(|current| current.same_live_dispatch(event))
        {
            return Err(ConstructorQueryError::Binding);
        }
        let sample = route
            .request
            .wander
            .as_deref()
            .ok_or(ConstructorQueryError::Binding)?;
        if sample.prepared != prefix.dispatch.dispatch_binding()
            || event.construction_binding() != prefix.construction
            || context.binding.npc != Some(prefix.npc)
            || event.first_construction()
        {
            return Err(ConstructorQueryError::Binding);
        }
        self.reborrow(prefix)?;
        Ok(RegisteredConstructorHandle {
            provider: Rc::clone(&prefix.provider),
            npc: prefix.npc,
            construction: prefix.construction,
            first_event: prefix.first_event.clone(),
            reopened: prefix.reopened.clone(),
            dispatch: event.clone(),
            snapshot: prefix.snapshot,
            from: [
                route.request.origin.x.bits(),
                route.request.origin.y.bits(),
                route.request.origin.z.bits(),
            ],
            to: [
                route.request.goal.x.bits(),
                route.request.goal.y.bits(),
                route.request.goal.z.bits(),
            ],
        })
    }

    /// Reborrow completed immutable constructor validation without another claim.
    /// The caller must also retain the original live mailbox job lease.
    pub(super) fn reborrow(
        &mut self,
        handle: &RegisteredConstructorHandle,
    ) -> Result<SourceConstructorQuery<'_>, ConstructorQueryError> {
        if !Rc::ptr_eq(&self.identity, &handle.provider)
            || self.navigation.binding() != handle.snapshot
        {
            return Err(ConstructorQueryError::Binding);
        }
        let entry = self
            .constructors
            .iter_mut()
            .find(|entry| entry.npc == handle.npc && entry.construction == handle.construction)
            .ok_or(ConstructorQueryError::MissingConstructor)?;
        if entry.npc != handle.npc
            || entry.construction != handle.construction
            || !match (&entry.reopened, &handle.reopened) {
                (Some(entry), Some(handle)) => Rc::ptr_eq(entry, handle),
                (None, None) => entry.first_event.same_live_dispatch(&handle.first_event),
                _ => false,
            }
        {
            return Err(ConstructorQueryError::Binding);
        }
        entry
            .query
            .check_query(self.navigation.query())
            .map_err(ConstructorQueryError::Native)?;
        Ok(SourceConstructorQuery {
            provider: Rc::clone(&self.identity),
            entry,
            snapshot: handle.snapshot,
            dispatch: handle.dispatch.clone(),
            from: handle.from,
            to: handle.to,
        })
    }

    /// Called at the Route dispatch boundary, before the straight gate. The native
    /// query was loaded when this provider was bound. Validation/reservation happen
    /// before claiming the one-use event; no fallible operation follows the claim.
    /// Failed outer Route work may retry this immutable dependency. Future mutable
    /// path/cache state must be staged separately; registry reuse is insufficient.
    pub(crate) fn retain_constructor(
        &mut self,
        context: &AuthorityRequest,
    ) -> Result<SourceConstructorQuery<'_>, ConstructorQueryError> {
        let binding = &context.binding;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(ConstructorQueryError::Binding);
        };
        let npc = binding.npc.ok_or(ConstructorQueryError::Binding)?;
        let now = binding
            .tick
            .checked_mul(u64::from(self.profile.tick_ms))
            .and_then(|elapsed| self.profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(ConstructorQueryError::Binding)?;
        if binding.world != self.world
            || binding.profile != self.profile.digest()
            || binding.authority_policy != self.profile.authority_policy
            || binding.geometry != self.profile.geometry_revision
            || binding.tick == 0
            || route.request.binding.now_ms != now
            || route.owner.npc() != npc
        {
            return Err(ConstructorQueryError::Binding);
        }
        let event = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| ConstructorQueryError::Binding)?;
        self.retain_validated_constructor(context, event)
    }

    fn retain_validated_constructor(
        &mut self,
        context: &AuthorityRequest,
        event: &PathfinderDispatchReceipt,
    ) -> Result<SourceConstructorQuery<'_>, ConstructorQueryError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(ConstructorQueryError::Binding);
        };
        let npc = context.binding.npc.ok_or(ConstructorQueryError::Binding)?;
        if event.configuration_binding() != self.navigation.configuration_binding()
            || event.first_construction() && route.request.prior_cache.is_some()
        {
            return Err(ConstructorQueryError::Binding);
        }
        let construction = event.construction_binding();
        let existing = self
            .constructors
            .iter()
            .position(|entry| entry.npc == npc && entry.construction == construction);
        let index = if let Some(index) = existing {
            let entry = &mut self.constructors[index];
            if entry.reopened.is_some() && event.first_construction() {
                return Err(ConstructorQueryError::Binding);
            }
            entry
                .query
                .check_query(self.navigation.query())
                .map_err(ConstructorQueryError::Native)?;
            if event.first_construction() && !event.same_live_dispatch(&entry.first_event) {
                // A new live dispatch after transaction rollback must claim its own
                // seal; otherwise its clone could construct an equal-byte provider.
                event
                    .claim_constructor_query()
                    .map_err(|_| ConstructorQueryError::Binding)?;
                entry.first_event = event.clone();
            }
            index
        } else {
            if !event.first_construction() {
                return Err(ConstructorQueryError::MissingConstructor);
            }
            if self.constructors.len() == MAX_CONSTRUCTORS {
                return Err(ConstructorQueryError::Budget);
            }
            self.constructors
                .try_reserve(1)
                .map_err(|_| ConstructorQueryError::Budget)?;
            let entry = RegisteredConstructor {
                npc,
                construction,
                first_event: event.clone(),
                reopened: None,
                query: self.navigation.query().retain_default(),
            };
            event
                .claim_constructor_query()
                .map_err(|_| ConstructorQueryError::Binding)?;
            let index = self.constructors.len();
            self.constructors.push(entry);
            index
        };
        fn bits(point: Point) -> [u32; 3] {
            [point.x.bits(), point.y.bits(), point.z.bits()]
        }
        Ok(SourceConstructorQuery {
            provider: Rc::clone(&self.identity),
            entry: &mut self.constructors[index],
            snapshot: self.navigation.binding(),
            dispatch: event.clone(),
            from: bits(route.request.origin),
            to: bits(route.request.goal),
        })
    }
}
impl SourceConstructorQuery<'_> {
    pub(crate) fn snapshot(&self) -> SnapshotBinding {
        self.snapshot
    }
    pub(crate) fn dispatch(&self) -> &PathfinderDispatchReceipt {
        &self.dispatch
    }
    pub(crate) fn slot(&self) -> SourceMapQuerySlot {
        self.entry.query.slot()
    }
    pub(crate) fn current_selected(&self) -> bool {
        self.entry.query.current_selected()
    }
    /// Selection follows source's destination/start checks and returns the actual
    /// borrowed query for subsequent calculate composition. The endpoints remain
    /// the original runtime dispatch values; no alternate Mesh can be supplied.
    pub(crate) fn select_for_calculate(
        &mut self,
        until: Instant,
    ) -> Result<SelectedSourceQuery<'_>, ConstructorQueryError> {
        if Instant::now() >= until {
            return Err(ConstructorQueryError::Native(
                benilla_native_nav::Error::Deadline,
            ));
        }
        let slot = self.slot();
        self.entry
            .query
            .select_for_calculate(slot, self.from, self.to)
            .map_err(|error| match error {
                SourceQuerySelectionError::Coordinates(error) => {
                    ConstructorQueryError::Coordinates(error)
                }
                SourceQuerySelectionError::Identity => ConstructorQueryError::Binding,
            })
    }
    pub(super) fn prepare_wander(
        &mut self,
        prior: Option<&benilla_native_nav::LoadedSourceWanderTerminal>,
        filter: benilla_native_nav::SourceOwnerFilter,
        config: benilla_native_nav::SourceSamePolygonConfig,
        budget: benilla_native_nav::SourceWanderBudget,
        until: Instant,
    ) -> Result<benilla_native_nav::LoadedSourceWanderPrefix, benilla_native_nav::SourceWanderError>
    {
        self.entry.query.prepare_wander_same_polygon(
            prior,
            benilla_native_nav::SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
            filter,
            config,
            budget,
            until,
        )
    }
    pub(super) fn select_wander_prefix(
        &mut self,
        prefix: &mut benilla_native_nav::LoadedSourceWanderPrefix,
        until: Instant,
    ) -> Result<(), ConstructorQueryError> {
        self.entry
            .query
            .select_wander_prefix(prefix, until)
            .map_err(ConstructorQueryError::Native)
    }
    pub(super) fn continue_wander_prefix(
        &self,
        prefix: &benilla_native_nav::LoadedSourceWanderPrefix,
    ) -> Result<SelectedSourceQuery<'_>, ConstructorQueryError> {
        self.entry
            .query
            .continue_wander_prefix(prefix, self.from, self.to)
            .map_err(ConstructorQueryError::Native)
    }
    pub(crate) fn select_and_check_tiles(
        &mut self,
        until: Instant,
    ) -> Result<LoadedSourceTilePresence, ConstructorQueryError> {
        let selected = self.select_for_calculate(until)?;
        let presence = selected
            .tile_presence(until)
            .map_err(ConstructorQueryError::Native)?;
        selected
            .check_tile_presence(&presence)
            .map_err(ConstructorQueryError::Native)?;
        Ok(presence)
    }
}

#[cfg(test)]
#[path = "source_pathfinder_query/test_archive.rs"]
pub(crate) mod test_archive;

#[cfg(test)]
#[path = "source_pathfinder_query/tests.rs"]
pub(crate) mod tests;

#[cfg(all(test, not(benilla_headless_geometry)))]
#[path = "source_pathfinder_query/pair_tests.rs"]
pub(super) mod pair_tests;
