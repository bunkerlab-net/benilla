// SPDX-License-Identifier: GPL-3.0-or-later
//! App-only evidence of an actual committed controller transition. These values are
//! deliberately not serializable: a record or cache DTO cannot reopen a loaded owner.
use benilla_sim::creature_motion::{Generator, PathFinderCache, QueryBinding};
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

/// Every current NPC is represented. Missing construction/cache explicitly means
/// the current generator owns no corresponding head; historical splines are ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteCommitHead {
    npc: NpcId,
    construction: Option<QueryBinding>,
    cache: Option<Box<PathFinderCache>>,
}
impl RouteCommitHead {
    pub(crate) fn npc(&self) -> NpcId {
        self.npc
    }
    pub(crate) fn construction(&self) -> Option<QueryBinding> {
        self.construction
    }
    pub(crate) fn cache(&self) -> Option<&PathFinderCache> {
        self.cache.as_deref()
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
        if boundary.heads.iter().any(|head| head.cache.is_some()) {
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
                let (construction, cache) =
                    npc.source_motion.as_ref().map_or((None, None), |source| {
                        match source.motion.snapshot().generator {
                            Generator::Chase(chase) => {
                                (chase.pathfinder_construction, chase.pathfinder_cache)
                            }
                            Generator::Wander(wander) => (
                                Some(wander.pathfinder_construction),
                                wander.pathfinder_cache,
                            ),
                            _ => (None, None),
                        }
                    });
                RouteCommitHead {
                    npc: npc.id,
                    construction,
                    cache,
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
        assert!(a.cache.is_some() && b.cache.is_some());
        std::mem::swap(&mut a.construction, &mut b.construction);
        std::mem::swap(&mut a.cache, &mut b.cache);
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
            .find_map(|head| head.cache.as_mut())
            .expect("authored conflict requires a genuine current cache");
        cache.revision = cache
            .revision
            .checked_add(1)
            .expect("bounded fixture cache revision");
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
}
