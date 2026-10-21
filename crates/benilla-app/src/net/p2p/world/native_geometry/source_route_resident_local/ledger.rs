// SPDX-License-Identifier: GPL-3.0-or-later
//! One cold Route epoch, not a retained-query or gameplay installation. This ledger
//! never turns a historical reply back into fresh authority and never owns an ECS borrow.
use super::super::{
    core::Session,
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_route_filter::resident::ResidentProviderIdentity,
};
use super::*;
use benilla_world_runtime::{native::Profile, NpcId, PlayerId};

const MAX_CANDIDATES: usize = 16;
struct Entry {
    id: u64,
    candidate: Box<ResidentColdCandidate>,
    committed: bool,
}
pub(in super::super) struct ResidentLedger {
    session: Session,
    profile: Profile,
    player: PlayerId,
    provider: ResidentProviderIdentity,
    owner: benilla_formats::NativeInventorySource,
    raw: Option<[u8; 32]>,
    boundary: Option<RouteCommitBoundary>,
    last_notice: Option<RouteCommitNotice>,
    candidates: Vec<Entry>,
    next_id: u64,
    route_epoch_committed: bool,
}
/// Includes every mutable ledger field. Candidate addresses identify immutable
/// opaque objects, including their private live seals, rather than serialized DTOs.
#[derive(Clone, Debug)]
pub(in super::super) struct Diagnostic {
    profile: Profile,
    boundary: Option<RouteCommitBoundary>,
    last_notice: Option<RouteCommitNotice>,
    candidates: Vec<(u64, usize, bool)>,
    capacity: usize,
    raw: Option<[u8; 32]>,
    next_id: u64,
    route_epoch_committed: bool,
}
impl PartialEq for Diagnostic {
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.boundary == other.boundary
            && self.candidates == other.candidates
            && self.capacity == other.capacity
            && self.raw == other.raw
            && self.next_id == other.next_id
            && self.route_epoch_committed == other.route_epoch_committed
            && match (&self.last_notice, &other.last_notice) {
                (Some(a), Some(b)) => same_notice(&self.profile, a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
impl Eq for Diagnostic {}
impl ResidentLedger {
    pub(in super::super) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        deadline: Instant,
    ) -> Result<Self, ResidentFilterError> {
        let identity = provider.ledger_identity(session, profile)?;
        provider
            .verify_installed_owner(compatibility, compatibility.owner(), deadline, || false)?;
        Ok(Self {
            session: session.clone(),
            profile: profile.clone(),
            player,
            provider: identity,
            owner: compatibility.owner().clone(),
            raw: None,
            boundary: None,
            last_notice: None,
            candidates: vec![],
            next_id: 0,
            route_epoch_committed: false,
        })
    }
    fn binding(&self, boundary: &RouteCommitBoundary) -> Result<(), Failure> {
        if boundary.world() != self.session.world
            || boundary.player() != self.player
            || boundary.profile() != self.profile.digest()
            || boundary.profile() != self.session.profile
            || boundary.geometry() != self.session.geometry
            || boundary.authority() != self.session.policy
            || boundary.heads().len() != self.profile.npcs.len()
        {
            return Err(Failure::Binding);
        }
        let mut previous = None;
        for head in boundary.heads() {
            if !self.session.bodies.contains_key(&head.npc().spawn)
                || previous.is_some_and(|spawn| spawn >= head.npc().spawn)
                || head.suspended_wander().is_some()
                || !matches!(
                    head.constructor_kind(),
                    None | Some(motion::PathfinderConstructorKind::ChaseLazy)
                )
                || head.cache().is_some() && head.construction().is_none()
                || head.construction().is_some() && head.constructor_kind().is_none()
            {
                return Err(Failure::Binding);
            }
            previous = Some(head.npc().spawn);
        }
        Ok(())
    }
    pub(in super::super) fn initialize(
        &mut self,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        self.binding(boundary)?;
        if let Some(old) = &self.boundary {
            return if old == boundary && self.last_notice.is_none() {
                Ok(())
            } else {
                Err(Failure::Binding)
            };
        }
        if !self.candidates.is_empty()
            || boundary
                .owner_heads()
                .any(|(_, head)| head.cache().is_some())
        {
            return Err(Failure::Binding);
        }
        self.boundary = Some(boundary.clone());
        Ok(())
    }
    /// Reserve before any source/concrete work. Successful emission and staging
    /// are one synchronous operation; failed attempts do not consume ledger slots.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        constructor: ResidentFilterDependency,
        water: ResidentChaseWaterDependency,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        let previous = self.boundary.as_ref().ok_or(Failure::Binding)?;
        // Synchronous controller pairing only: its ACK barrier excludes another
        // attempt after a successful step. Until then only an equal-input retry
        // may supersede this owner's live receipt, never another construction.
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply.request.binding.npc == context.binding.npc
                && entry.candidate.reply.request.binding.tick == context.binding.tick
                && entry.candidate.reply.request != *context
        }) {
            return Err(Failure::UnsupportedQuery.into());
        }
        let dispatch = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if self.route_epoch_committed {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || !self.owner.same_owner(compatibility.owner())
            || previous.tick().checked_add(1) != Some(context.binding.tick)
            || !dispatch.first_construction()
            || self.candidates.iter().any(|entry| {
                same_reply_dispatch(&self.profile, &entry.candidate.reply.request, context)
            })
        {
            return Err(Failure::Binding.into());
        }
        if self.candidates.len() == MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        self.candidates
            .try_reserve(1)
            .map_err(|_| Failure::QueueCapacity)?;
        let (candidate, facts) = super::certify(
            world,
            compatibility,
            provider,
            context,
            constructor,
            water,
            ticket,
            deadline,
            cancelled,
            || {},
        )?;
        if !candidate.installed_owner.same_owner(&self.owner)
            || self.raw.is_some_and(|raw| raw != candidate.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        self.validate_candidate(&candidate)?;
        self.raw = Some(candidate.raw_revision);
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Box::new(candidate),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    fn validate_candidate(&self, candidate: &ResidentColdCandidate) -> Result<(), Failure> {
        let reply = &candidate.reply;
        let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
            return Err(Failure::Binding);
        };
        let dispatch = reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let calculation = calculation(reply)?;
        calculation
            .validate_for(&route.request)
            .map_err(|_| Failure::Binding)?;
        let include = benilla_sim::route_filter::initial_include_flags(
            route.owner.capabilities().map_err(|_| Failure::Binding)?,
        )
        .ok_or(Failure::Binding)?;
        if !candidate.installed_owner.same_owner(&self.owner)
            || !self.provider.matches_cache(&calculation.after_cache)
            || route.request.kind != motion::RouteKind::ChaseInitial
            || !dispatch.first_construction()
            || route.request.prior_cache.is_some()
            || calculation.prior_cache.is_some()
            || calculation.source != motion::RouteSource::OwnerPathFinder
            || calculation.after_cache.revision != 1
            || calculation.after_cache.active_len != 1
            || calculation.after_cache.path_type != 1
            || calculation.after_cache.include_flags != include
            || calculation.after_cache.exclude_flags != 0
            || calculation.after_cache.binding.owner_generation != route.owner.npc().generation
            || calculation.after_cache.binding.navigation_policy != self.session.geometry
            || reply.request.binding.world != self.session.world
            || reply.request.binding.profile != self.session.profile
            || reply.request.binding.geometry != self.session.geometry
            || reply.request.binding.npc != Some(route.owner.npc())
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    /// Historical bookkeeping only: no source read, wall-clock or physics query.
    /// All selections and heads validate before any boundary/pruning/publication.
    pub(in super::super) fn acknowledge(
        &mut self,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        let boundary = notice.boundary();
        self.binding(boundary)?;
        if self
            .last_notice
            .as_ref()
            .is_some_and(|old| same_notice(&self.profile, old, notice))
        {
            return Ok(());
        }
        let previous = self.boundary.as_ref().ok_or(Failure::Binding)?;
        if boundary.session() != previous.session()
            || previous.tick().checked_add(1) != Some(boundary.tick())
            || notice.previous_record() != previous.record()
            || notice.before() != previous.state()
            || boundary.record() == previous.record()
            || self.route_epoch_committed && !notice.routes().is_empty()
        {
            return Err(Failure::Binding);
        }
        let mut selected: Vec<(NpcId, usize)> = vec![];
        let mut last_id = None;
        for reply in notice.routes() {
            if reply.request.binding.tick != boundary.tick() {
                return Err(Failure::Binding);
            }
            let mut matching = self.candidates.iter().enumerate().filter(|(_, entry)| {
                entry.candidate.reply == *reply
                    && same_reply_dispatch(
                        &self.profile,
                        &entry.candidate.reply.request,
                        &reply.request,
                    )
            });
            let (index, entry) = matching.next().ok_or(Failure::Binding)?;
            if matching.next().is_some()
                || entry.committed
                || last_id.is_some_and(|id| id >= entry.id)
            {
                return Err(Failure::Binding);
            }
            self.validate_candidate(&entry.candidate)?;
            if self.raw != Some(entry.candidate.raw_revision) {
                return Err(Failure::Binding);
            }
            let npc = reply.request.binding.npc.ok_or(Failure::Binding)?;
            // Old and retried calculations can have identical bytes/cache. Only
            // the last synchronous attempt can be the successful controller step.
            // Retain older allocations diagnostically until this whole ACK validates.
            if self.candidates.iter().any(|later| {
                later.id > entry.id
                    && later.candidate.reply.request.binding.npc == Some(npc)
                    && later.candidate.reply.request.binding.tick == boundary.tick()
            }) {
                return Err(Failure::Binding);
            }

            if selected.iter().any(|(owner, _)| *owner == npc) {
                return Err(Failure::Binding);
            }
            let old = previous
                .heads()
                .iter()
                .find(|head| head.npc() == npc)
                .ok_or(Failure::Binding)?;
            if old.cache().is_some() {
                return Err(Failure::Binding);
            }
            selected.push((npc, index));
            last_id = Some(entry.id);
        }
        for (old, head) in previous.heads().iter().zip(boundary.heads()) {
            if old.npc() != head.npc() {
                return Err(Failure::Binding);
            }
            if let Some((_, index)) = selected.iter().find(|(npc, _)| *npc == head.npc()) {
                let reply = &self.candidates[*index].candidate.reply;
                let dispatch = reply
                    .request
                    .require_pathfinder_dispatch(&self.profile)
                    .map_err(|_| Failure::Binding)?;
                if head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
                    || head.construction() != Some(dispatch.construction_binding())
                    || head.cache() != Some(&calculation(reply)?.after_cache)
                {
                    return Err(Failure::Binding);
                }
            } else if head != old {
                // Deliberately no Home/Wander/replacement/drop-owner lifecycle here.
                // A cache-free Chase may become the active generator before its first query.
                if self.route_epoch_committed
                    || head.cache().is_some()
                    || old.cache().is_some()
                    || old.construction().is_some()
                    || head.construction().is_some()
                    || old.constructor_kind().is_some()
                    || head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
                {
                    return Err(Failure::Binding);
                }
            }
        }
        // No failure path follows this line. Staged rollback ancestors survive
        // earlier idle notices; they retire only once their tick is acknowledged.
        let selected_ids: Vec<_> = selected
            .iter()
            .map(|(_, index)| self.candidates[*index].id)
            .collect();
        for entry in &mut self.candidates {
            if selected_ids.contains(&entry.id) {
                entry.committed = true;
            }
        }
        self.candidates.retain(|entry| {
            entry.committed || entry.candidate.reply.request.binding.tick > boundary.tick()
        });
        self.route_epoch_committed |= !selected_ids.is_empty();
        self.boundary = Some(boundary.clone());
        self.last_notice = Some(notice.clone());
        Ok(())
    }
    /// Source-only diagnostic check, never a reply re-emitter or physics certificate.
    pub(in super::super) fn check_fresh_candidate(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<(), ResidentFilterError> {
        if !self.provider.matches(provider) {
            return Err(Failure::Binding.into());
        }
        let candidate = self
            .candidates
            .iter()
            .find(|entry| {
                same_reply_dispatch(&self.profile, &entry.candidate.reply.request, context)
            })
            .ok_or(Failure::Binding)?;
        provider
            .joined_for(&candidate.candidate.dependency, context, deadline, || false)
            .map(|_| ())
    }
    pub(in super::super) fn require_open_epoch(&self) -> Result<(), Failure> {
        if self.route_epoch_committed {
            Err(Failure::UnsupportedQuery)
        } else {
            Ok(())
        }
    }
    pub(in super::super) fn diagnostic(&self) -> Diagnostic {
        Diagnostic {
            profile: self.profile.clone(),
            boundary: self.boundary.clone(),
            last_notice: self.last_notice.clone(),
            candidates: self
                .candidates
                .iter()
                .map(|entry| {
                    (
                        entry.id,
                        (&*entry.candidate as *const ResidentColdCandidate) as usize,
                        entry.committed,
                    )
                })
                .collect(),
            capacity: self.candidates.capacity(),
            raw: self.raw,
            next_id: self.next_id,
            route_epoch_committed: self.route_epoch_committed,
        }
    }
    pub(in super::super) fn counts(&self) -> (usize, usize) {
        (
            self.candidates.len(),
            self.candidates
                .iter()
                .filter(|entry| entry.committed)
                .count(),
        )
    }
}
fn calculation(reply: &AuthorityReply) -> Result<&motion::RouteCalculation, Failure> {
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &reply.facts else {
        return Err(Failure::Binding);
    };
    let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
        return Err(Failure::Binding);
    };
    if spatial.request != motion::Query::Route(route.request.clone()) {
        return Err(Failure::Binding);
    }
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        return Err(Failure::Binding);
    };
    Ok(calculation)
}
fn same_reply_dispatch(profile: &Profile, a: &AuthorityRequest, b: &AuthorityRequest) -> bool {
    a == b
        && a.require_pathfinder_dispatch(profile)
            .ok()
            .zip(b.require_pathfinder_dispatch(profile).ok())
            .is_some_and(|(a, b)| a.same_live_dispatch(b))
}
fn same_notice(profile: &Profile, a: &RouteCommitNotice, b: &RouteCommitNotice) -> bool {
    a == b
        && a.routes()
            .iter()
            .zip(b.routes())
            .all(|(a, b)| same_reply_dispatch(profile, &a.request, &b.request))
}
