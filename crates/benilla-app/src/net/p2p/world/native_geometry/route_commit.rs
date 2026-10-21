// SPDX-License-Identifier: GPL-3.0-or-later
//! App-only evidence of an actual committed controller transition. These values are
//! deliberately not serializable: a record or cache DTO cannot reopen a loaded owner.
use benilla_sim::creature_motion::{
    Generator, PathFinderCache, PathfinderConstructorKind, QueryBinding,
};
use benilla_world_runtime::{native as n, Digest, NpcId, PlayerId, WorldId};
use std::sync::Arc;

/// A fresh controller attachment, distinct even for equal-byte saved worlds.
#[derive(Clone, Debug)]
pub(crate) struct RouteCommitSession(Arc<()>);
impl PartialEq for RouteCommitSession {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for RouteCommitSession {}

/// The retained PathFinder state of one actual MotionMaster owner. Home's local
/// constructor is never retained, and historical splines supply no owner head.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RouteCommitOwnerHead {
    kind: Option<PathfinderConstructorKind>,
    construction: Option<QueryBinding>,
    cache: Option<Box<PathFinderCache>>,
}
impl RouteCommitOwnerHead {
    pub(crate) fn constructor_kind(&self) -> Option<PathfinderConstructorKind> {
        self.kind
    }
    pub(crate) fn construction(&self) -> Option<QueryBinding> {
        self.construction
    }
    pub(crate) fn cache(&self) -> Option<&PathFinderCache> {
        self.cache.as_deref()
    }
}

/// Exactly one ordered record per NPC. The selected motion model retains at most
/// one suspended Wander below its current Chase or Home generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteCommitHead {
    npc: NpcId,
    current: RouteCommitOwnerHead,
    suspended_wander: Option<RouteCommitOwnerHead>,
}
impl RouteCommitHead {
    pub(crate) fn npc(&self) -> NpcId {
        self.npc
    }
    pub(crate) fn constructor_kind(&self) -> Option<PathfinderConstructorKind> {
        self.current.constructor_kind()
    }
    pub(crate) fn construction(&self) -> Option<QueryBinding> {
        self.current.construction()
    }
    pub(crate) fn cache(&self) -> Option<&PathFinderCache> {
        self.current.cache()
    }
    pub(crate) fn suspended_wander(&self) -> Option<&RouteCommitOwnerHead> {
        self.suspended_wander.as_ref()
    }
    pub(crate) fn owner_heads(&self) -> impl Iterator<Item = &RouteCommitOwnerHead> {
        std::iter::once(&self.current).chain(self.suspended_wander())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteCommitBoundary {
    session: RouteCommitSession,
    world: WorldId,
    player: PlayerId,
    profile: Digest,
    geometry: Digest,
    authority: Digest,
    tick: u64,
    record: Digest,
    state: Digest,
    heads: Vec<RouteCommitHead>,
}
impl RouteCommitBoundary {
    /// Check an existing session boundary without transferring its allocation identity.
    pub(super) fn validate_world(&self, world: &n::NativeWorld) -> Result<(), n::NativeError> {
        let checkpoint = world.checkpoint()?.identity();
        let actual = Self::capture(
            world,
            self.session.clone(),
            checkpoint.history_tip,
            checkpoint.state_digest,
        );
        if *self != actual {
            return Err(n::NativeError::AuthorityBinding);
        }
        Ok(())
    }
    /// Called by controller attachment, before any selected route is resolved.
    /// Existing owner caches cannot be transferred into a fresh installation.
    pub(crate) fn capture_initial(world: &n::NativeWorld) -> Result<Self, n::NativeError> {
        world.validate_saved_session_reopen()?;
        let checkpoint = world.checkpoint()?.identity();
        let boundary = Self::capture(
            world,
            RouteCommitSession(Arc::new(())),
            checkpoint.history_tip,
            checkpoint.state_digest,
        );
        if boundary
            .owner_heads()
            .any(|(_, head)| head.cache().is_some())
        {
            return Err(n::NativeError::UnsupportedMechanic(
                "route commit attachment requires a cache-free world",
            ));
        }
        Ok(boundary)
    }
    #[cfg(test)]
    pub(super) fn capture_reconstructed(
        world: &n::NativeWorld,
        proof: &super::source_route_same_polygon::ReopenBoundaryProof,
    ) -> Result<Self, n::NativeError> {
        proof.validate(world)?;
        let checkpoint = world.checkpoint()?.identity();
        Ok(Self::capture(
            world,
            RouteCommitSession(Arc::new(())),
            checkpoint.history_tip,
            checkpoint.state_digest,
        ))
    }
    fn capture(
        world: &n::NativeWorld,
        session: RouteCommitSession,
        record: Digest,
        state: Digest,
    ) -> Self {
        let heads = world
            .state()
            .npcs
            .values()
            .map(|npc| {
                let (current, suspended_wander) = npc.source_motion.as_ref().map_or_else(
                    || (RouteCommitOwnerHead::default(), None),
                    |source| {
                        let snapshot = source.motion.snapshot();
                        let current = match snapshot.generator {
                            Generator::Chase(chase) => RouteCommitOwnerHead {
                                kind: Some(PathfinderConstructorKind::ChaseLazy),
                                construction: chase.pathfinder_construction,
                                cache: chase.pathfinder_cache,
                            },
                            Generator::Wander(wander) => RouteCommitOwnerHead {
                                kind: Some(PathfinderConstructorKind::WanderInitialized),
                                construction: Some(wander.pathfinder_construction),
                                cache: wander.pathfinder_cache,
                            },
                            _ => RouteCommitOwnerHead::default(),
                        };
                        let suspended_wander =
                            snapshot
                                .suspended_wander
                                .map(|wander| RouteCommitOwnerHead {
                                    kind: Some(PathfinderConstructorKind::WanderInitialized),
                                    construction: Some(wander.pathfinder_construction),
                                    cache: wander.pathfinder_cache,
                                });
                        (current, suspended_wander)
                    },
                );
                RouteCommitHead {
                    npc: npc.id,
                    current,
                    suspended_wander,
                }
            })
            .collect();
        Self {
            session,
            world: world.state().world,
            player: world.state().player,
            profile: world.profile_digest(),
            geometry: world.profile().geometry_revision,
            authority: world.profile().authority_policy,
            tick: world.state().tick,
            record,
            state,
            heads,
        }
    }
    pub(crate) fn session(&self) -> &RouteCommitSession {
        &self.session
    }
    pub(crate) fn world(&self) -> WorldId {
        self.world
    }
    pub(crate) fn player(&self) -> PlayerId {
        self.player
    }
    pub(crate) fn profile(&self) -> Digest {
        self.profile
    }
    pub(crate) fn geometry(&self) -> Digest {
        self.geometry
    }
    pub(crate) fn authority(&self) -> Digest {
        self.authority
    }
    pub(crate) fn tick(&self) -> u64 {
        self.tick
    }
    pub(crate) fn record(&self) -> Digest {
        self.record
    }
    pub(crate) fn state(&self) -> Digest {
        self.state
    }
    pub(crate) fn heads(&self) -> &[RouteCommitHead] {
        &self.heads
    }
    pub(crate) fn owner_heads(&self) -> impl Iterator<Item = (NpcId, &RouteCommitOwnerHead)> {
        self.heads
            .iter()
            .flat_map(|head| head.owner_heads().map(move |owner| (head.npc(), owner)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteCommitNotice {
    boundary: RouteCommitBoundary,
    previous_record: Digest,
    before: Digest,
    routes: Vec<n::AuthorityReply>,
    #[cfg(test)]
    draws: Vec<n::Draw>,
}
impl RouteCommitNotice {
    /// The controller calls this only after successful step_batch and input retirement,
    /// before projection/checkpoint can clear the journal. No record-only constructor exists.
    pub(crate) fn capture(
        world: &n::NativeWorld,
        previous: &RouteCommitBoundary,
    ) -> Result<Self, n::NativeError> {
        let record = world.last_tick().ok_or(n::NativeError::InvalidState)?;
        if previous.tick.checked_add(1) != Some(world.state().tick)
            || record.tick != world.state().tick
            || record.profile != world.profile_digest()
            || record.previous_record != previous.record
            || record.before != previous.state
            || record.after != world.state_digest()
            || previous.world != world.state().world
            || previous.player != world.state().player
            || previous.profile != world.profile_digest()
            || previous.geometry != world.profile().geometry_revision
            || previous.authority != world.profile().authority_policy
        {
            return Err(n::NativeError::AuthorityBinding);
        }
        Ok(Self {
            boundary: RouteCommitBoundary::capture(
                world,
                previous.session.clone(),
                record.digest(),
                record.after,
            ),
            previous_record: record.previous_record,
            before: record.before,
            #[cfg(test)]
            draws: record.draws.clone(),
            routes: record
                .replies
                .iter()
                .filter(|reply| {
                    matches!(
                        &reply.request.query,
                        n::Query::SourceMotion(n::MotionQuery::Route(route))
                            if route.request.kind != benilla_sim::creature_motion::RouteKind::WanderPrepare
                    )
                })
                .cloned()
                .collect(),
        })
    }
    pub(crate) fn boundary(&self) -> &RouteCommitBoundary {
        &self.boundary
    }
    pub(crate) fn previous_record(&self) -> Digest {
        self.previous_record
    }
    pub(crate) fn before(&self) -> Digest {
        self.before
    }
    #[cfg(test)]
    pub(crate) fn draws(&self) -> &[n::Draw] {
        &self.draws
    }
    pub(crate) fn routes(&self) -> &[n::AuthorityReply] {
        &self.routes
    }
}

/// Selected owned-route installation supplies an explicit paired sink. No default
/// implementation exists, and proposal authority itself cannot acknowledge commits.
pub(crate) trait RouteCommitSink: Send {
    fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), n::NativeError>;
    fn deliver(&mut self, notice: &RouteCommitNotice) -> Result<(), n::NativeError>;
}

/// Deliberate corruptions of genuine captured evidence for negative provider tests.
/// This module is absent from production and cannot construct an initial notice.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "negative app-only provider fixtures are absent from headless harnesses"
)]
pub(crate) mod authored_conflicts {
    use super::RouteCommitNotice;

    pub(crate) fn crossed_owner_heads(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let [a, b] = wrong.boundary.heads.as_mut_slice() else {
            panic!("authored conflict requires exactly two genuine owners");
        };
        assert!(a.current.cache.is_some() && b.current.cache.is_some());
        std::mem::swap(&mut a.current, &mut b.current);
        wrong
    }
    pub(crate) fn wrong_record(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        wrong.boundary.record[0] ^= 1;
        wrong
    }
    pub(crate) fn wrong_current_cache(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let cache = wrong
            .boundary
            .heads
            .iter_mut()
            .find_map(|head| head.current.cache.as_mut())
            .expect("authored conflict requires a genuine current cache");
        cache.revision = cache
            .revision
            .checked_add(1)
            .expect("bounded fixture cache revision");
        wrong
    }
    pub(crate) fn crossed_owner_roles(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let head = wrong
            .boundary
            .heads
            .iter_mut()
            .find(|head| head.current.cache.is_some() && head.suspended_wander.is_some())
            .expect("authored conflict requires current and suspended owners");
        std::mem::swap(&mut head.current, head.suspended_wander.as_mut().unwrap());
        wrong
    }
    pub(crate) fn wrong_suspended_cache(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let cache = wrong
            .boundary
            .heads
            .iter_mut()
            .find_map(|head| head.suspended_wander.as_mut()?.cache.as_mut())
            .expect("authored conflict requires a genuine suspended cache");
        cache.revision = cache
            .revision
            .checked_add(1)
            .expect("bounded fixture cache revision");
        wrong
    }
    pub(crate) fn wrong_suspended_construction(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let construction = wrong
            .boundary
            .heads
            .iter_mut()
            .find_map(|head| head.suspended_wander.as_mut()?.construction.as_mut())
            .expect("authored conflict requires a genuine suspended constructor");
        construction.query_id = construction
            .query_id
            .checked_add(1)
            .expect("bounded fixture constructor query id");
        wrong
    }
    pub(crate) fn without_routes(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        assert!(
            !wrong.routes.is_empty(),
            "authored conflict requires an actual Route"
        );
        wrong.routes.clear();
        wrong
    }
    pub(crate) fn duplicate_route(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let route = wrong
            .routes
            .first()
            .expect("authored conflict requires an actual Route")
            .clone();
        wrong.routes.push(route);
        wrong
    }
    pub(crate) fn foreign_session(
        notice: &RouteCommitNotice,
        initial: &super::RouteCommitBoundary,
    ) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        wrong.boundary.session = initial.session.clone();
        wrong
    }
    pub(crate) fn skipped_tick(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        wrong.boundary.tick += 1;
        wrong
    }
    pub(crate) fn stale_generation(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        wrong.boundary.heads.last_mut().unwrap().npc.generation += 1;
        wrong
    }
    pub(crate) fn omitted_head(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        wrong.boundary.heads.pop().unwrap();
        wrong
    }
    pub(crate) fn decoded_dispatch(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        for reply in &mut wrong.routes {
            reply.request =
                serde_json::from_slice(&serde_json::to_vec(&reply.request).unwrap()).unwrap();
        }
        assert_eq!(
            wrong, *notice,
            "serialization erases the live seal, not request values"
        );
        wrong
    }
    pub(crate) fn uncertified_reply(notice: &RouteCommitNotice) -> RouteCommitNotice {
        use benilla_sim::creature_motion::{Fact, RouteOutcome};
        use benilla_world_runtime::native::{Facts, MotionFacts};
        let mut wrong = notice.clone();
        let Facts::SourceMotion(MotionFacts::Spatial(reply)) =
            &mut wrong.routes.last_mut().unwrap().facts
        else {
            panic!("actual spatial Route");
        };
        let Fact::RouteCalculation(calculation) = &mut reply.fact else {
            panic!("actual calculation");
        };
        let RouteOutcome::Complete(path) = &mut calculation.outcome else {
            panic!("actual Complete route");
        };
        path.path_identity[0] ^= 1;
        wrong
    }
    pub(crate) fn reordered_routes(notice: &RouteCommitNotice) -> RouteCommitNotice {
        assert!(
            notice.routes.len() >= 2,
            "two actual owner calculations required"
        );
        let mut wrong = notice.clone();
        wrong.routes.reverse();
        wrong
    }
    pub(crate) fn wrong_last_cache(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let cache = wrong
            .boundary
            .heads
            .last_mut()
            .unwrap()
            .current
            .cache
            .as_mut()
            .unwrap();
        cache.revision += 1;
        wrong
    }
    pub(crate) fn substitute_dispatch(
        notice: &RouteCommitNotice,
        request: &benilla_world_runtime::native::AuthorityRequest,
    ) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        let original = &mut wrong.routes.first_mut().unwrap().request;
        assert_eq!(
            *original, *request,
            "distinct genuine receipt must have equal serialized request"
        );
        *original = request.clone();
        wrong
    }
    pub(crate) fn extra_head(notice: &RouteCommitNotice) -> RouteCommitNotice {
        let mut wrong = notice.clone();
        wrong
            .boundary
            .heads
            .push(wrong.boundary.heads.last().unwrap().clone());
        wrong
    }
}
