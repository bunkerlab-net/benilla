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
    query: RetainedSourceQuery,
}

/// This borrow is only the query fields of the actual modeled owner constructor.
/// It is neither createFilter evidence nor proof that a whole calculate ran.
pub(crate) struct SourceConstructorQuery<'a> {
    provider: Rc<()>,
    index: usize,
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
    index: usize,
    npc: NpcId,
    construction: QueryBinding,
    first_event: PathfinderDispatchReceipt,
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

    pub(super) fn prepare_constructor(
        &mut self,
        context: &AuthorityRequest,
    ) -> Result<RegisteredConstructorHandle, ConstructorQueryError> {
        let query = self.retain_constructor(context)?;
        Ok(RegisteredConstructorHandle {
            provider: query.provider,
            index: query.index,
            npc: query.entry.npc,
            construction: query.entry.construction,
            first_event: query.entry.first_event.clone(),
            dispatch: query.dispatch,
            snapshot: query.snapshot,
            from: query.from,
            to: query.to,
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
            .get_mut(handle.index)
            .ok_or(ConstructorQueryError::MissingConstructor)?;
        if entry.npc != handle.npc
            || entry.construction != handle.construction
            || !entry.first_event.same_live_dispatch(&handle.first_event)
        {
            return Err(ConstructorQueryError::Binding);
        }
        entry
            .query
            .check_query(self.navigation.query())
            .map_err(ConstructorQueryError::Native)?;
        Ok(SourceConstructorQuery {
            provider: Rc::clone(&self.identity),
            index: handle.index,
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
            index,
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
mod tests;
