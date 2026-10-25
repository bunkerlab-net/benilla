// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only dispatch receipt. This sidecar never enters a mailbox queue.
use super::super::products::source_height::freshness_timings::{self, Phase, Timer};
use super::super::{
    source_route_chase_gate::gate_model,
    source_route_filter::{
        resident::{owned::OwnedResident, ResidentProviderIdentity},
        validate_route_context,
    },
};
use super::*;

/// Work remains the sole lease owner and remains Send. The caller-thread sidecar
/// pins the genuine resident allocation; no Rc is added to cross-thread Work.
pub(crate) struct ResidentDispatchWork {
    work: Work,
    validation: ResidentDispatchValidation,
}
pub(in super::super) struct ResidentDispatchValidation {
    session: Session,
    profile: Profile,
    player: PlayerId,
    resident: ResidentProviderIdentity,
    request: AuthorityRequest,
    lease: JobLease,
}
impl ResidentDispatchValidation {
    fn validate_request(
        mailbox: &Mailbox,
        resident: &OwnedResident,
        profile: &Profile,
        player: PlayerId,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<(), Failure> {
        let (_, session, selected_profile, selected_player) = resident.dispatch_selection();
        if session != &mailbox.session || selected_profile != profile || selected_player != player {
            return Err(Failure::Binding);
        }
        if !mailbox.active.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        Self::validate_shape(session, profile, player, request)
    }
    pub(in super::super) fn validate_shape(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        request: &AuthorityRequest,
    ) -> Result<(), Failure> {
        session.check_request_binding(request)?;
        validate_route_context(session, profile, request)?;
        let event = request
            .require_pathfinder_dispatch(profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        // Shared cold/retained Chase shape only. Constructor, cache ancestry,
        // policy, source freshness and concrete clearance remain provider work.
        if event.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || !matches!(
                route.request.kind,
                motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
            )
            || route.request.launch_origin != route.request.origin
            || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
            || event.first_construction() != route.request.prior_cache.is_none()
        {
            return Err(Failure::UnsupportedQuery);
        }
        if route
            .require_owner_ignore_pathfinding(profile)
            .map_err(|_| Failure::Binding)?
            .ignore_pathfinding()
        {
            return Err(Failure::UnsupportedQuery);
        }
        // Validate genuine resident model/target witnesses without retaining a
        // SamePolygonModel or selecting/constructing any native query.
        gate_model(profile, session.world, player, route)?;
        Ok(())
    }
    fn bind(
        mailbox: &Mailbox,
        resident: &OwnedResident,
        profile: &Profile,
        player: PlayerId,
        work: &Work,
        deadline: Instant,
    ) -> Result<Self, Failure> {
        Self::validate_request(
            mailbox,
            resident,
            profile,
            player,
            &work.request,
            deadline.min(work.deadline),
        )?;
        if !work.active_for(&mailbox.active) {
            return Err(Failure::Deadline);
        }
        let (identity, session, _, _) = resident.dispatch_selection();
        Ok(Self {
            session: session.clone(),
            profile: profile.clone(),
            player,
            resident: identity.clone(),
            request: work.request.clone(),
            lease: work.lease(),
        })
    }
    fn job<'a>(
        &'a self,
        work: &'a Work,
        mailbox: &Mailbox,
        resident: &OwnedResident,
        invocation_deadline: Instant,
    ) -> Result<ResidentDispatchJob<'a>, Failure> {
        check_owned_route(
            &self.session,
            &self.request,
            &self.lease,
            &mailbox.session,
            work,
        )?;
        if !work.active_for(&mailbox.active) {
            return Err(Failure::Deadline);
        }
        let job = ResidentDispatchJob {
            validation: self,
            work,
            deadline: invocation_deadline.min(work.deadline),
        };
        job.require_resident(resident)?;
        Ok(job)
    }
}
impl ResidentDispatchWork {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn capture(
        mailbox: &Mailbox,
        resident: &OwnedResident,
        profile: &Profile,
        player: PlayerId,
        ticket: u64,
        request: AuthorityRequest,
        deadline: Instant,
    ) -> Result<Self, Failure> {
        let (reply, _) = bounded(1);
        let work = Work::new(ticket, request, deadline, reply, mailbox.active.clone());
        let validation =
            ResidentDispatchValidation::bind(mailbox, resident, profile, player, &work, deadline)?;
        Ok(Self { work, validation })
    }
    pub(crate) fn job<'a>(
        &'a self,
        mailbox: &Mailbox,
        resident: &OwnedResident,
        invocation_deadline: Instant,
    ) -> Result<ResidentDispatchJob<'a>, Failure> {
        self.validation
            .job(&self.work, mailbox, resident, invocation_deadline)
    }
}

/// Invocation-local view only. It grants no queue access, geometry fact, native
/// constructor, RouteJob conversion, or authority to publish/acknowledge.
pub(crate) struct ResidentDispatchJob<'a> {
    validation: &'a ResidentDispatchValidation,
    work: &'a Work,
    deadline: Instant,
}
impl ResidentDispatchJob<'_> {
    pub(crate) fn require_resident(&self, resident: &OwnedResident) -> Result<(), Failure> {
        let saved = self.validation;
        let (identity, session, profile, player) = resident.dispatch_selection();
        if !saved.resident.same_allocation(identity)
            || &saved.session != session
            || &saved.profile != profile
            || saved.player != player
        {
            return Err(Failure::Binding);
        }
        if !saved.lease.is_active() || Instant::now() >= self.deadline {
            return Err(Failure::Deadline);
        }
        Ok(())
    }
    /// Immutable selection only; this cannot replace live source or physics checks.
    pub(in super::super) fn require_provider(
        &self,
        provider: &super::super::source_route_filter::resident::ResidentFilterProvider<
            '_,
            '_,
            '_,
            '_,
        >,
        session: &Session,
        profile: &Profile,
    ) -> Result<(), Failure> {
        let timer = Timer::start(Phase::ProviderGuard);
        if freshness_timings::mismatch(Phase::ProviderActive, || {
            !self.validation.lease.is_active() || Instant::now() >= self.deadline
        }) {
            return Err(Failure::Deadline);
        }
        if freshness_timings::mismatch(Phase::ProviderLease, || {
            !self.validation.lease.same_job(&self.work.lease())
        }) || freshness_timings::mismatch(Phase::ProviderIdentity, || {
            !self.validation.resident.matches(provider)
        }) || freshness_timings::mismatch(Phase::ProviderSession, || {
            &self.validation.session != session
        }) || freshness_timings::mismatch(Phase::ProviderProfile, || {
            &self.validation.profile != profile
        }) || freshness_timings::mismatch(Phase::ProviderRequest, || {
            self.validation.request != self.work.request
        }) {
            return Err(Failure::Binding);
        }
        timer.finish(true);
        Ok(())
    }
    pub(crate) fn request(&self) -> &AuthorityRequest {
        &self.work.request
    }
    pub(crate) fn profile(&self) -> &Profile {
        &self.validation.profile
    }
    pub(crate) fn player(&self) -> PlayerId {
        self.validation.player
    }
    pub(in super::super) fn lease(&self) -> JobLease {
        self.work.lease()
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
}

#[path = "resident_dispatch/tests.rs"]
#[cfg(test)]
mod tests;

#[path = "resident_dispatch/fifo.rs"]
pub(crate) mod fifo;

#[path = "resident_dispatch/transport.rs"]
pub(crate) mod transport;
