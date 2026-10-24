// SPDX-License-Identifier: GPL-3.0-or-later
//! Source distance/vertical/LOS prefix with owned model witnesses. No water gate,
//! calculate, route or world admission follows from a clear diagnostic prefix.
use super::super::super::{
    products::vmap::{RayReply, StaticRay},
    source_route_chase_gate::{gate_model, ChaseGateModel},
};
use super::*;
use benilla_sim::chase_gate;
use benilla_world_runtime::PlayerId;
#[path = "water.rs"]
pub(crate) mod water;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentChaseLosDecision {
    Distance,
    Vertical,
    Blocked,
    ClearPrefix,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResidentChaseLosObservation {
    pub(crate) decision: ResidentChaseLosDecision,
    pub(crate) ray: Option<RayReply>,
}
#[derive(Clone)]
enum ResidentChaseLosRoot {
    Filter(Rc<()>),
    #[cfg(all(test, not(benilla_headless_geometry)))]
    Retained(Rc<()>),
}
#[derive(Clone)]
pub(crate) struct ResidentChaseLosDependency {
    identity: Rc<()>,
    root: ResidentChaseLosRoot,
    context: AuthorityRequest,
    player: PlayerId,
    model: ChaseGateModel,
    expires: Instant,
    observation: ResidentChaseLosObservation,
}
impl ResidentFilterProvider<'_, '_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_chase_los(
        &self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentChaseLosDependency, ResidentFilterError> {
        self.los_inner(prior, context, player, ticket, deadline, cancelled, |_| {})
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_chase_los_observed(
        &self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<ResidentChaseLosDependency, ResidentFilterError> {
        self.los_inner(prior, context, player, ticket, deadline, cancelled, observe)
    }
    #[allow(clippy::too_many_arguments)]
    fn los_inner(
        &self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        player: PlayerId,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<ResidentChaseLosDependency, ResidentFilterError> {
        self.filter_for(prior, context, deadline, &cancelled)?;
        let deadline = deadline.min(prior.expires);
        if ticket == 0 || ticket <= prior.observations[1].request().ticket {
            return Err(Failure::Binding.into());
        }
        self.los_core(
            ResidentChaseLosRoot::Filter(Rc::clone(&prior.observation_identity)),
            context,
            player,
            ticket,
            deadline,
            cancelled,
            observe,
        )
    }
    #[cfg(all(test, not(benilla_headless_geometry)))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe_retained_chase_los(
        &self,
        root: &super::retained::ResidentRetainedRoot,
        context: &AuthorityRequest,
        player: PlayerId,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<ResidentChaseLosDependency, ResidentFilterError> {
        let deadline = root.check_current(self, context, deadline, &cancelled)?;
        if ticket == 0 {
            return Err(Failure::Binding.into());
        }
        self.los_core(
            ResidentChaseLosRoot::Retained(Rc::clone(root.identity())),
            context,
            player,
            ticket,
            deadline,
            cancelled,
            observe,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn los_core(
        &self,
        root: ResidentChaseLosRoot,
        context: &AuthorityRequest,
        player: PlayerId,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(u8),
    ) -> Result<ResidentChaseLosDependency, ResidentFilterError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if !matches!(
            route.request.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) {
            return Err(Failure::UnsupportedQuery.into());
        }
        // Existing source-selected model/width, generation/height and actual
        // starter-owned target-height validation. Body scalars are not operands.
        let model = gate_model(&self.profile, self.session.world, player, route)?;
        observe(0);
        self.check_context(context, deadline, &cancelled)?;
        let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
        let owner = bits(route.owner_position);
        let goal = bits(route.request.goal);
        let numeric =
            chase_gate::numeric_gate_bits(owner, goal, model.selection.combat_reach().bits())
                .map_err(|_| Failure::Invalid)?;
        let observation = match numeric {
            chase_gate::NumericGate::Distance => ResidentChaseLosObservation {
                decision: ResidentChaseLosDecision::Distance,
                ray: None,
            },
            chase_gate::NumericGate::Vertical => ResidentChaseLosObservation {
                decision: ResidentChaseLosDecision::Vertical,
                ray: None,
            },
            chase_gate::NumericGate::Pass => {
                observe(1);
                self.check_context(context, deadline, &cancelled)?;
                let [from, to] = chase_gate::los_endpoints_bits(
                    owner,
                    goal,
                    model.owner_height.height().bits(),
                    model.target_height.height().bits(),
                )
                .map_err(|_| Failure::Invalid)?;
                let ray = self
                    .height
                    .chase_ray(ticket, from, to, deadline, &cancelled)
                    .map_err(ResidentFilterError::Source)?;
                observe(2);
                ResidentChaseLosObservation {
                    decision: if matches!(ray.result, StaticRay::Miss) {
                        ResidentChaseLosDecision::ClearPrefix
                    } else {
                        ResidentChaseLosDecision::Blocked
                    },
                    ray: Some(ray),
                }
            }
        };
        self.check_context(context, deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(ResidentChaseLosDependency {
            identity: Rc::clone(&self.identity),
            root,
            context: context.clone(),
            player,
            model,
            expires: deadline,
            observation,
        })
    }
    pub(crate) fn los_for(
        &self,
        dependency: &ResidentChaseLosDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentChaseLosObservation, ResidentFilterError> {
        if !Rc::ptr_eq(&self.identity, &dependency.identity) || dependency.context != *context {
            return Err(Failure::Binding.into());
        }
        let saved = self.dispatch_for(&dependency.context, deadline, &cancelled)?;
        let current = self.dispatch_for(context, deadline, &cancelled)?;
        if !saved.same_live_dispatch(current) {
            return Err(Failure::Binding.into());
        }
        let deadline = deadline.min(dependency.expires);
        self.check_context(context, deadline, &cancelled)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if gate_model(&self.profile, self.session.world, dependency.player, route)?
            != dependency.model
        {
            return Err(Failure::Binding.into());
        }
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(dependency.observation)
    }
}
