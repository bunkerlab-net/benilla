// SPDX-License-Identifier: GPL-3.0-or-later
//! Only completed source stages survive a yield; physics clearance never does.
use super::super::core::{JobLease, RouteJob};
use super::*;

const MAX_PENDING_JOBS: usize = 16;

pub(super) struct PendingCalculation {
    lease: JobLease,
    first_ticket: u64,
    phase: Phase,
}
enum Phase {
    Prepared(Box<PreparedCalculation>),
    Gated(Box<PreparedCalculation>, Box<GateDependencies>),
    Terminal(Box<SamePolygonProposal>),
}
impl Phase {
    fn context(&self) -> &AuthorityRequest {
        match self {
            Self::Prepared(prepared) | Self::Gated(prepared, _) => &prepared.context,
            Self::Terminal(proposal) => &proposal.context,
        }
    }
    fn reservation(&self, npc: NpcId, construction: motion::QueryBinding) -> bool {
        let prepared = match self {
            Self::Prepared(prepared) | Self::Gated(prepared, _) => prepared,
            Self::Terminal(_) => return false,
        };
        prepared.reserved
            && prepared.context.binding.npc == Some(npc)
            && prepared.dispatch.construction_binding() == construction
    }
}
impl SourceSamePolygonRouteProvider {
    #[cfg(test)]
    pub(in super::super) fn pending_job_count(&self) -> usize {
        self.pending.len()
    }

    /// Called even when the shared frame allowance is exhausted. Work retirement
    /// revokes every lease; the owner thread alone drops its non-Send native state.
    pub(in super::super) fn reap_jobs(&mut self) {
        let next_tick = self
            .committed_boundary
            .as_ref()
            .map(|boundary| boundary.tick().checked_add(1));
        self.pending.retain(|pending| {
            pending.lease.is_active()
                && next_tick.is_none_or(|tick| tick == Some(pending.phase.context().binding.tick))
        });
    }

    pub(super) fn reservations(&self, npc: NpcId, construction: motion::QueryBinding) -> usize {
        self.pending
            .iter()
            .filter(|pending| {
                pending.lease.is_active()
                    && pending.phase.reservation(npc, construction)
                    && !self
                        .candidates
                        .iter()
                        .filter(|entry| entry.npc == npc && entry.construction == construction)
                        .any(|entry| {
                            entry
                                .proposals
                                .iter()
                                .any(|candidate| candidate.context == *pending.phase.context())
                        })
            })
            .count()
    }

    pub(super) fn check_prepared(
        &mut self,
        prepared: &PreparedCalculation,
    ) -> Result<(), SamePolygonError> {
        check_owned_context(
            &prepared.context,
            &prepared.dispatch,
            &prepared.constructor,
            &prepared.filter_context,
        )?;
        self.filter.reborrow(&prepared.filter_context)?;
        self.queries.reborrow(&prepared.constructor)?;
        let entry = self
            .candidates
            .iter()
            .find(|entry| {
                Some(entry.npc) == prepared.context.binding.npc
                    && entry.construction == prepared.dispatch.construction_binding()
            })
            .ok_or(Failure::Binding)?;
        if prepared.prior.as_ref().is_some_and(|prior| {
            !entry
                .proposals
                .iter()
                .any(|candidate| Rc::ptr_eq(candidate, prior))
        }) {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }

    fn check_terminal(&mut self, proposal: &SamePolygonProposal) -> Result<(), SamePolygonError> {
        self.queries.reborrow(&proposal.constructor)?;
        self.check_terminal_membership(proposal)
    }

    fn check_terminal_membership(
        &self,
        proposal: &SamePolygonProposal,
    ) -> Result<(), SamePolygonError> {
        check_owned_context(
            &proposal.context,
            &proposal.dispatch,
            &proposal.constructor,
            &proposal.filter_context,
        )?;
        if proposal.candidate.context != proposal.context {
            return Err(Failure::Binding.into());
        }
        self.filter.reborrow(&proposal.filter_context)?;
        let entry = self
            .candidates
            .iter()
            .find(|entry| {
                entry
                    .proposals
                    .iter()
                    .any(|candidate| Rc::ptr_eq(candidate, &proposal.candidate))
            })
            .ok_or(Failure::Binding)?;
        if proposal.prior.as_ref().is_some_and(|prior| {
            !entry
                .proposals
                .iter()
                .any(|candidate| Rc::ptr_eq(candidate, prior))
        }) {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }

    /// Advance exactly one successful source phase. True permits fresh concrete
    /// certification in this invocation; it does not produce or commit a Route.
    pub(in super::super) fn advance_job(
        &mut self,
        job: &RouteJob<'_>,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<bool, SamePolygonError> {
        self.advance_job_observed(job, first_ticket, deadline, |_| {})
    }

    pub(super) fn advance_job_observed(
        &mut self,
        job: &RouteJob<'_>,
        first_ticket: u64,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<bool, SamePolygonError> {
        job.bind_provider(&self.identity)?;
        self.check_current_boundary(job.request())?;
        self.reap_jobs();
        let lease = job.lease();
        if !lease.is_active() || first_ticket.checked_add(6).is_none() {
            return Err(Failure::Binding.into());
        }
        require_time(deadline)?;
        let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.lease.same_job(&lease))
        else {
            if self.pending.len() == MAX_PENDING_JOBS {
                return Err(SamePolygonError::CandidateBudget);
            }
            self.pending
                .try_reserve(1)
                .map_err(|_| SamePolygonError::CandidateBudget)?;
            #[cfg(test)]
            job.record_phase("prepare");
            let prepared = self.prepare_calculation(
                job.session(),
                job.request(),
                first_ticket,
                deadline,
                &mut observe,
            )?;
            if !lease.is_active() {
                return Err(Failure::Deadline.into());
            }
            self.pending.push(PendingCalculation {
                lease,
                first_ticket,
                phase: Phase::Prepared(Box::new(prepared)),
            });
            return Ok(false);
        };
        let mut pending = self.pending.swap_remove(index);
        if pending.first_ticket != first_ticket
            || pending.phase.context() != job.request()
            || !same_dispatch(pending.phase.context(), job.request())
        {
            return Err(Failure::Binding.into());
        }
        #[cfg(test)]
        job.record_phase(match &pending.phase {
            Phase::Prepared(_) => "gate",
            Phase::Gated(..) => "native_terminal",
            Phase::Terminal(_) => "final_certification",
        });
        let result = match &pending.phase {
            Phase::Prepared(prepared) => self
                .check_prepared(prepared)
                .and_then(|()| self.calculate_gate(prepared, deadline, &mut observe))
                .map(Some),
            Phase::Gated(prepared, _) => self.check_prepared(prepared).map(|()| None),
            Phase::Terminal(proposal) => self.check_terminal(proposal).map(|()| None),
        };
        let result = match result {
            Ok(Some(gate)) => {
                let Phase::Prepared(prepared) = pending.phase else {
                    unreachable!()
                };
                pending.phase = Phase::Gated(prepared, Box::new(gate));
                Ok(false)
            }
            Ok(None) => match &pending.phase {
                Phase::Gated(prepared, gate) => {
                    match self.calculate_terminal(prepared, gate, deadline, &mut observe) {
                        Ok(terminal) => {
                            let Phase::Gated(prepared, gate) = pending.phase else {
                                unreachable!()
                            };
                            pending.phase = Phase::Terminal(Box::new(Self::assemble_proposal(
                                *prepared, *gate, terminal,
                            )));
                            Ok(false)
                        }
                        Err(error) => Err(error),
                    }
                }
                Phase::Terminal(_) => Ok(true),
                Phase::Prepared(_) => unreachable!(),
            },
            Err(error) => Err(error),
        };
        // Only earlier successful phases survive an incomplete deadline. Complete
        // remains Terminal until mailbox validation/publication retires this Work.
        if lease.is_active()
            && (result.is_ok()
                || result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.failure() == Failure::Deadline))
        {
            self.pending.push(pending);
        }
        result
    }

    pub(in super::super) fn terminal_for_job(
        &self,
        job: &RouteJob<'_>,
    ) -> Result<&SamePolygonProposal, SamePolygonError> {
        job.bind_provider(&self.identity)?;
        self.check_current_boundary(job.request())?;
        let lease = job.lease();
        if !lease.is_active() {
            return Err(Failure::Deadline.into());
        }
        let pending = self
            .pending
            .iter()
            .find(|pending| pending.lease.same_job(&lease))
            .ok_or(Failure::Binding)?;
        let Phase::Terminal(proposal) = &pending.phase else {
            return Err(Failure::Binding.into());
        };
        if proposal.context != *job.request() || !same_dispatch(&proposal.context, job.request()) {
            return Err(Failure::Binding.into());
        }
        self.check_terminal_membership(proposal)?;
        Ok(proposal)
    }
}
fn same_dispatch(a: &AuthorityRequest, b: &AuthorityRequest) -> bool {
    let (Query::SourceMotion(MotionQuery::Route(a)), Query::SourceMotion(MotionQuery::Route(b))) =
        (&a.query, &b.query)
    else {
        return false;
    };
    a.pathfinder_dispatch
        .as_ref()
        .zip(b.pathfinder_dispatch.as_ref())
        .is_some_and(|(a, b)| a.same_live_dispatch(b))
}

// These fields can only be assembled internally. Preserve their exact pairing
// explicitly when a prepared phase outlives the call that validated it.
fn check_owned_context(
    context: &AuthorityRequest,
    dispatch: &PathfinderDispatchReceipt,
    constructor: &RegisteredConstructorHandle,
    filter: &OwnedRouteFilterContext,
) -> Result<(), SamePolygonError> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::Binding.into());
    };
    if !constructor.matches_context(context)
        || !filter.matches_context(context)
        || !route
            .pathfinder_dispatch
            .as_ref()
            .is_some_and(|current| dispatch.same_live_dispatch(current))
    {
        return Err(Failure::Binding.into());
    }
    Ok(())
}
