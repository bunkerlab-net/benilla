// SPDX-License-Identifier: GPL-3.0-or-later
//! Synchronous cold Chase evidence. No Route, Facts, cache publication or admission.
use super::super::super::{
    core::Body,
    products::vmap::{RayReply, StaticRay},
};
use super::{
    chase_los::water::ResidentChaseWaterDependency,
    cold::{ResidentSamePolygonDependency, ResidentSamePolygonObservation},
    *,
};
use benilla_native_nav::{SourceSamePolygonBudget, SourceSamePolygonConfig};

/// Own every prerequisite, including the opaque ClearConditional ancestry.
/// Neither copied observations nor an unrelated equal-byte native owner can join.
pub(crate) struct ResidentJoinedColdDependency {
    constructor: ResidentFilterDependency,
    chase: ResidentChaseWaterDependency,
    cold: ResidentSamePolygonDependency,
    expires: Instant,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResidentJoinedColdObservation {
    pub(crate) terminal: ResidentSamePolygonObservation,
    pub(crate) body: Body,
}
impl ResidentJoinedColdDependency {
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn retained_cold(&self) -> &ResidentSamePolygonDependency {
        &self.cold
    }
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(super) fn retained_chase(&self) -> &ResidentChaseWaterDependency {
        &self.chase
    }
    pub(crate) fn deadline(&self, deadline: Instant) -> Instant {
        deadline.min(self.expires)
    }
}
impl ResidentFilterProvider<'_, '_, '_, '_> {
    /// Exact loader-minted owner, independent of equal raw coverage/receipt hashes.
    /// The concrete caller fetches `installed` in its one immutable ECS borrow.
    pub(crate) fn verify_installed_owner(
        &self,
        compatibility: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
        installed: &benilla_formats::NativeInventorySource,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        let policy = self
            .geometry_policy
            .installed
            .as_ref()
            .ok_or(Failure::Binding)?;
        if !policy.owner.same_owner(installed)
            || !policy.owner.same_owner(compatibility.owner())
            || compatibility.revision() != policy.revision
            || compatibility.source_binding() != (policy.height, policy.epoch)
        {
            return Err(Failure::Binding.into());
        }
        compatibility
            .verify_current_owner(installed, deadline, cancelled)
            .map_err(ResidentFilterError::Source)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_joined_cold(
        &mut self,
        constructor: ResidentFilterDependency,
        chase: ResidentChaseWaterDependency,
        context: &AuthorityRequest,
        first_ticket: u64,
        budget: SourceSamePolygonBudget,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedColdDependency, ResidentFilterError> {
        if self.geometry_policy.installed.is_none() {
            return Err(Failure::Binding.into());
        }
        // This slice has no retained-terminal/ACK migration. A later dispatch
        // cannot be silently reinterpreted as another cold first calculation.
        if !self
            .dispatch_for(context, deadline, &cancelled)?
            .first_construction()
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let (deadline, water_ticket) =
            chase.cold_prerequisites(self, &constructor, context, deadline, &cancelled)?;
        if first_ticket <= water_ticket || first_ticket.checked_add(1).is_none() {
            return Err(Failure::Binding.into());
        }
        let cold = self.observe_cold_same_polygon(
            &constructor,
            context,
            first_ticket,
            SourceSamePolygonConfig {
                normalize_z: false,
                ignore_normalization: false,
            },
            budget,
            deadline,
            &cancelled,
        )?;
        let result = ResidentJoinedColdDependency {
            constructor,
            chase,
            cold,
            expires: deadline,
        };
        self.joined_for(&result, context, deadline, cancelled)?;
        Ok(result)
    }
    /// Diagnostic copies only. Native terminal/cache ownership stays in dependency.
    pub(crate) fn joined_for(
        &self,
        dependency: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentJoinedColdObservation, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        dependency.chase.cold_prerequisites(
            self,
            &dependency.constructor,
            context,
            deadline,
            &cancelled,
        )?;
        let terminal = self.terminal_for(&dependency.cold, context, deadline, &cancelled)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        let npc = context.binding.npc.ok_or(Failure::Binding)?;
        let height = super::super::initialized_owner_height(&self.profile, route)?
            .ok_or(Failure::UnresolvedBody)?;
        // validate_route_context has checked radius against the selected initialized
        // model. A fixed fixture radius/height never becomes a corridor operand.
        let body = Body {
            radius: self
                .session
                .bodies
                .get(&npc.spawn)
                .ok_or(Failure::UnresolvedBody)?
                .radius,
            height,
        };
        self.check_context(context, deadline, &cancelled)?;
        Ok(ResidentJoinedColdObservation { terminal, body })
    }
    #[cfg(not(benilla_headless_geometry))]
    pub(crate) fn joined_cache(
        &self,
        dependency: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<motion::PathFinderCache, ResidentFilterError> {
        self.joined_for(dependency, context, deadline, &cancelled)?;
        self.cold_cache(
            &dependency.cold,
            context,
            dependency.deadline(deadline),
            cancelled,
        )
    }
    /// Separate source LOS for the actual calculated segment. Chase LOS used
    /// target height; this ray uses owner collision height at BOTH endpoints.
    pub(crate) fn joined_segment_los(
        &self,
        dependency: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<RayReply, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        let observed = self.joined_for(dependency, context, deadline, &cancelled)?;
        if ticket == 0 || ticket <= observed.terminal.liquid_tickets[1] {
            return Err(Failure::Binding.into());
        }
        let eye = |p: [u32; 3]| {
            [
                p[0],
                p[1],
                (f32::from_bits(p[2]) + observed.body.height.as_f32()).to_bits(),
            ]
        };
        let ray = self
            .height
            .chase_ray(
                ticket,
                eye(observed.terminal.points[0]),
                eye(observed.terminal.points[1]),
                deadline,
                &cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        if ray.request.ignore_m2 || !matches!(ray.result, StaticRay::Miss) {
            return Err(Failure::Obstacle.into());
        }
        self.joined_for(dependency, context, deadline, cancelled)?;
        Ok(ray)
    }
}
