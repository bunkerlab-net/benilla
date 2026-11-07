// SPDX-License-Identifier: GPL-3.0-or-later
//! Only completed source stages survive a yield; physics clearance never does.
use super::super::core::{JobLease, RouteJob};
use super::*;

const MAX_PENDING_JOBS: usize = 16;

pub(super) struct PendingCalculation {
    lease: JobLease,
    first_ticket: u64,
    pub(super) phase: Phase,
}
pub(super) enum Phase {
    Validated(Box<ValidatedCalculation>),
    Prepared(Box<PreparedCalculation>),
    Gated(Box<PreparedCalculation>, Box<GateDependencies>),
    Terminal(Box<SamePolygonProposal>),
    WanderPrepared(Rc<wander::WanderPreparation>),
}
impl Phase {
    fn context(&self) -> &AuthorityRequest {
        match self {
            Self::Validated(validation) => &validation.context,
            Self::Prepared(prepared) | Self::Gated(prepared, _) => &prepared.validation.context,
            Self::Terminal(proposal) => &proposal.context,
            Self::WanderPrepared(prefix) => &prefix.prepared.validation.context,
        }
    }
    fn reservation(&self, npc: NpcId, construction: motion::QueryBinding) -> bool {
        let prepared = match self {
            Self::Prepared(prepared) | Self::Gated(prepared, _) => prepared.as_ref(),
            Self::WanderPrepared(prefix) => &prefix.prepared,
            Self::Validated(_) | Self::Terminal(_) => return false,
        };
        prepared.dependencies.reserved
            && prepared.validation.context.binding.npc == Some(npc)
            && prepared.validation.dispatch.construction_binding() == construction
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

    pub(super) fn check_validated(
        &self,
        validation: &ValidatedCalculation,
    ) -> Result<(), SamePolygonError> {
        self.check_current_boundary(&validation.context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &validation.context.query else {
            return Err(Failure::Binding.into());
        };
        if !validation
            .filter_context
            .matches_context(&validation.context)
            || !route
                .pathfinder_dispatch
                .as_ref()
                .is_some_and(|dispatch| validation.dispatch.same_live_dispatch(dispatch))
        {
            return Err(Failure::Binding.into());
        }
        self.filter.reborrow(&validation.filter_context)?;
        Ok(())
    }

    pub(super) fn check_prepared(
        &mut self,
        prepared: &PreparedCalculation,
    ) -> Result<(), SamePolygonError> {
        check_owned_context(
            &prepared.validation.context,
            &prepared.validation.dispatch,
            &prepared.dependencies.constructor,
            &prepared.validation.filter_context,
        )?;
        self.filter.reborrow(&prepared.validation.filter_context)?;
        self.queries.reborrow(&prepared.dependencies.constructor)?;
        let entry = self
            .candidates
            .iter()
            .find(|entry| {
                Some(entry.npc) == prepared.validation.context.binding.npc
                    && entry.construction == prepared.validation.dispatch.construction_binding()
            })
            .ok_or(Failure::Binding)?;
        if prepared.dependencies.prior.as_ref().is_some_and(|prior| {
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
            job.record_phase("adopt_validated_context");
            let validation = self.validate_job_calculation(job, first_ticket, deadline)?;
            if !lease.is_active() {
                return Err(Failure::Deadline.into());
            }
            self.pending.push(PendingCalculation {
                lease,
                first_ticket,
                phase: Phase::Validated(Box::new(validation)),
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
            Phase::Validated(_) => "prepare_constructor",
            Phase::Prepared(_) => "gate",
            Phase::Gated(..) => "native_terminal",
            Phase::Terminal(_) => "final_certification",
            Phase::WanderPrepared(_) => "wander_prepared_validation",
        });
        let result = match &pending.phase {
            Phase::Validated(validation) => {
                let cached = if is_wander_prepare(&validation.context) {
                    self.cached_wander_preparation(validation)
                } else {
                    Ok(None)
                };
                match cached {
                    Ok(Some(prefix)) => {
                        require_time(deadline)
                            .map_err(SamePolygonError::from)
                            .map(|()| {
                                pending.phase = Phase::WanderPrepared(prefix);
                                false
                            })
                    }
                    Ok(None) => match self.prepare_constructor(
                        validation,
                        Some(job),
                        deadline,
                        &mut observe,
                    ) {
                        Ok(dependencies) => {
                            let Phase::Validated(validation) = pending.phase else {
                                unreachable!()
                            };
                            pending.phase = Phase::Prepared(Box::new(PreparedCalculation {
                                validation: *validation,
                                dependencies,
                            }));
                            Ok(false)
                        }
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(error),
                }
            }
            Phase::Prepared(prepared) => {
                match self
                    .check_prepared(prepared)
                    .and_then(|()| self.calculate_gate(prepared, deadline, &mut observe))
                {
                    Ok(gate) => {
                        let Phase::Prepared(prepared) = pending.phase else {
                            unreachable!()
                        };
                        pending.phase = Phase::Gated(prepared, Box::new(gate));
                        Ok(false)
                    }
                    Err(error) => Err(error),
                }
            }
            Phase::Gated(prepared, gate) if is_wander_prepare(&prepared.validation.context) => {
                match self.calculate_wander_prefix(prepared, gate, deadline, &mut observe) {
                    Ok((filter, native)) if lease.is_active() => {
                        let Phase::Gated(prepared, gate) = pending.phase else {
                            unreachable!()
                        };
                        let prefix = self.retain_wander_prefix(*prepared, *gate, filter, native);
                        pending.phase = Phase::WanderPrepared(prefix);
                        Ok(false)
                    }
                    Ok(_) => Err(Failure::Deadline.into()),
                    Err(error) => Err(error),
                }
            }
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
            Phase::Terminal(proposal) => self.check_terminal(proposal).map(|()| true),
            Phase::WanderPrepared(prefix) => self.check_wander_preparation(prefix).map(|()| true),
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

    fn check_wander_preparation(
        &mut self,
        prefix: &Rc<wander::WanderPreparation>,
    ) -> Result<(), SamePolygonError> {
        if !self
            .wander_prefixes
            .iter()
            .any(|saved| Rc::ptr_eq(saved, prefix))
        {
            return Err(Failure::Binding.into());
        }
        self.check_prepared(&prefix.prepared)
    }

    pub(in super::super) fn wander_prepared_for_job(
        &mut self,
        job: &RouteJob<'_>,
    ) -> Result<Facts, SamePolygonError> {
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
        let Phase::WanderPrepared(prefix) = &pending.phase else {
            return Err(Failure::Binding.into());
        };
        if prefix.prepared.validation.context != *job.request()
            || !same_dispatch(&prefix.prepared.validation.context, job.request())
        {
            return Err(Failure::Binding.into());
        }
        let prefix = Rc::clone(prefix);
        self.check_wander_preparation(&prefix)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &job.request().query else {
            return Err(Failure::Binding.into());
        };
        let facts = Facts::SourceMotion(benilla_world_runtime::native::MotionFacts::Spatial(
            Box::new(motion::Reply {
                request: motion::Query::Route(route.request.clone()),
                fact: motion::Fact::WanderPrepared,
            }),
        ));
        job.validate_facts(&facts)?;
        Ok(facts)
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
fn is_wander_prepare(context: &AuthorityRequest) -> bool {
    matches!(&context.query, Query::SourceMotion(MotionQuery::Route(route))
        if route.request.kind == motion::RouteKind::WanderPrepare)
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
