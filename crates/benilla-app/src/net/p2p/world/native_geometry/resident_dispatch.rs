// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only dispatch receipt. This sidecar never enters a mailbox queue.
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
struct ResidentDispatchValidation {
    session: Session,
    profile: Profile,
    player: PlayerId,
    resident: ResidentProviderIdentity,
    request: AuthorityRequest,
    lease: JobLease,
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
        let (identity, session, selected_profile, selected_player) = resident.dispatch_selection();
        if session != &mailbox.session || selected_profile != profile || selected_player != player {
            return Err(Failure::Binding);
        }
        if !mailbox.active.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        mailbox.session.check_request_binding(&request)?;
        validate_route_context(session, profile, &request)?;
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
        let (reply, _) = bounded(1);
        let work = Work::new(
            ticket,
            request.clone(),
            deadline,
            reply,
            mailbox.active.clone(),
        );
        let validation = ResidentDispatchValidation {
            session: session.clone(),
            profile: profile.clone(),
            player,
            resident: identity.clone(),
            request,
            lease: work.lease(),
        };
        let captured = Self { work, validation };
        captured.job(mailbox, resident, deadline)?;
        Ok(captured)
    }

    pub(crate) fn job<'a>(
        &'a self,
        mailbox: &Mailbox,
        resident: &OwnedResident,
        invocation_deadline: Instant,
    ) -> Result<ResidentDispatchJob<'a>, Failure> {
        check_owned_route(
            &self.validation.session,
            &self.validation.request,
            &self.validation.lease,
            &mailbox.session,
            &self.work,
        )?;
        if !self.work.active_for(&mailbox.active) {
            return Err(Failure::Deadline);
        }
        let job = ResidentDispatchJob {
            owner: self,
            deadline: invocation_deadline.min(self.work.deadline),
        };
        job.require_resident(resident)?;
        Ok(job)
    }
}

/// Invocation-local view only. It grants no queue access, geometry fact, native
/// constructor, RouteJob conversion, or authority to publish/acknowledge.
pub(crate) struct ResidentDispatchJob<'a> {
    owner: &'a ResidentDispatchWork,
    deadline: Instant,
}
impl ResidentDispatchJob<'_> {
    pub(crate) fn require_resident(&self, resident: &OwnedResident) -> Result<(), Failure> {
        let saved = &self.owner.validation;
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
    pub(crate) fn request(&self) -> &AuthorityRequest {
        &self.owner.work.request
    }
    pub(crate) fn profile(&self) -> &Profile {
        &self.owner.validation.profile
    }
    pub(crate) fn player(&self) -> PlayerId {
        self.owner.validation.player
    }
    pub(crate) fn lease(&self) -> JobLease {
        self.owner.work.lease()
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
}

#[path = "resident_dispatch/tests.rs"]
#[cfg(test)]
mod tests;
