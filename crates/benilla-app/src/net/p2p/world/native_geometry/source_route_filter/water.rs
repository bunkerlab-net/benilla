// SPDX-License-Identifier: GPL-3.0-or-later
//! Conditional source Chase water branches. No route or movement admission.
use super::*;
use benilla_world_runtime::native::{
    environment::EnvironmentSnapshotBinding, PlayerEnvironmentTarget,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentChaseWaterDecision {
    PrefixRejected(ResidentChaseLosDecision),
    TargetWater,
    ClearConditional,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResidentChaseWaterObservation {
    pub(crate) decision: ResidentChaseWaterDecision,
    pub(crate) owner: Option<PointNoWaterReply>,
    /// Source operand annotation: absence is proven before height classification.
    pub(crate) owner_collision_height_bits: Option<u32>,
    pub(crate) target: Option<PlayerEnvironmentTarget>,
}
pub(crate) struct ResidentChaseWaterDependency {
    prefix: ResidentChaseLosDependency,
    environment: Option<EnvironmentSnapshotBinding>,
    expires: Instant,
    observation: ResidentChaseWaterObservation,
}
impl ResidentFilterProvider<'_, '_, '_, '_> {
    pub(crate) fn observe_chase_water(
        &self,
        prefix: &ResidentChaseLosDependency,
        context: &AuthorityRequest,
        owner_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentChaseWaterDependency, ResidentFilterError> {
        self.water_inner(prefix, context, owner_ticket, deadline, cancelled, |_| {})
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_chase_water_observed(
        &self,
        prefix: &ResidentChaseLosDependency,
        context: &AuthorityRequest,
        owner_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<ResidentChaseWaterDependency, ResidentFilterError> {
        self.water_inner(prefix, context, owner_ticket, deadline, cancelled, observe)
    }
    #[allow(clippy::too_many_arguments)]
    fn water_inner(
        &self,
        prefix: &ResidentChaseLosDependency,
        context: &AuthorityRequest,
        owner_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(u8),
    ) -> Result<ResidentChaseWaterDependency, ResidentFilterError> {
        let los = self.los_for(prefix, context, deadline, &cancelled)?;
        let deadline = deadline.min(prefix.expires);
        observe(0);
        self.check_context(context, deadline, &cancelled)?;
        let mut dependency = ResidentChaseWaterDependency {
            prefix: prefix.clone(),
            environment: None,
            expires: deadline,
            observation: ResidentChaseWaterObservation {
                decision: ResidentChaseWaterDecision::PrefixRejected(los.decision),
                owner: None,
                owner_collision_height_bits: None,
                target: None,
            },
        };
        if los.decision == ResidentChaseLosDecision::ClearPrefix {
            let ray = los.ray.ok_or(Failure::Binding)?;
            if owner_ticket == 0 || owner_ticket <= ray.request.ticket {
                return Err(Failure::Binding.into());
            }
            let environment = self.compatible_environment(&cancelled)?;
            self.check_context(context, deadline, &cancelled)?;
            let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                return Err(Failure::UnsupportedQuery.into());
            };
            let owner = self
                .height
                .point_no_water(
                    PointNoWaterRequest {
                        binding: self.height.binding(),
                        ticket: owner_ticket,
                        position: [
                            route.owner_position.x.bits(),
                            route.owner_position.y.bits(),
                            route.owner_position.z.bits(),
                        ],
                        mask: chase_gate::LIQUID_MASK,
                    },
                    deadline,
                    &cancelled,
                )
                .map_err(ResidentFilterError::Source)?;
            observe(1);
            self.check_context(context, deadline, &cancelled)?;
            // Source Player::IsInWater reads its owned cache, including its older
            // cached query position. Never sample the current target here.
            let target = route
                .require_target_water(&self.profile, self.session.world, prefix.player)
                .map_err(|_| Failure::Binding)?;
            dependency.environment = Some(environment);
            dependency.observation = ResidentChaseWaterObservation {
                decision: if target.in_water() {
                    ResidentChaseWaterDecision::TargetWater
                } else {
                    ResidentChaseWaterDecision::ClearConditional
                },
                owner: Some(owner),
                owner_collision_height_bits: Some(chase_gate::LIQUID_COLLISION_HEIGHT_BITS),
                target: Some(target.clone()),
            };
            observe(2);
        }
        self.los_for(prefix, context, deadline, &cancelled)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(dependency)
    }
    pub(crate) fn water_for(
        &self,
        dependency: &ResidentChaseWaterDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentChaseWaterObservation, ResidentFilterError> {
        let deadline = deadline.min(dependency.expires);
        self.los_for(&dependency.prefix, context, deadline, &cancelled)?;
        if let Some(expected) = dependency.environment {
            if self.compatible_environment(&cancelled)? != expected {
                return Err(Failure::Binding.into());
            }
            let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                return Err(Failure::UnsupportedQuery.into());
            };
            let target = route
                .require_target_water(&self.profile, self.session.world, dependency.prefix.player)
                .map_err(|_| Failure::Binding)?;
            if dependency.observation.target.as_ref() != Some(target) {
                return Err(Failure::Binding.into());
            }
        }
        self.check_context(context, deadline, &cancelled)?;
        Ok(dependency.observation.clone())
    }
    fn compatible_environment(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<EnvironmentSnapshotBinding, ResidentFilterError> {
        let source = self
            .profile
            .player_environment
            .as_ref()
            .ok_or(Failure::Binding)?;
        let expected = self
            .height
            .environment_binding(self.profile.content_digest, source.catalog(), cancelled)
            .map_err(ResidentFilterError::Source)?;
        if source.snapshot_binding() != expected {
            return Err(Failure::Binding.into());
        }
        Ok(expected)
    }
}

impl ResidentChaseWaterDependency {
    /// The private prefix retains the exact constructor observation identity;
    /// equal flags, copied diagnostics and another observation are insufficient.
    pub(crate) fn cold_prerequisites(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(Instant, u64), ResidentFilterError> {
        let deadline = deadline
            .min(prior.expires)
            .min(self.expires)
            .min(self.prefix.expires);
        provider.filter_for(prior, context, deadline, &cancelled)?;
        if !matches!(&self.prefix.root, ResidentChaseLosRoot::Filter(identity)
            if Rc::ptr_eq(identity, &prior.observation_identity))
        {
            return Err(Failure::Binding.into());
        }
        let observation = provider.water_for(self, context, deadline, cancelled)?;
        if observation.decision != ResidentChaseWaterDecision::ClearConditional {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok((
            deadline,
            observation.owner.ok_or(Failure::Binding)?.request().ticket,
        ))
    }
}

#[cfg(all(test, not(benilla_headless_geometry)))]
impl ResidentChaseWaterDependency {
    pub(in super::super) fn retained_prerequisites(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        root: &super::super::retained::ResidentRetainedRoot,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(Instant, u64), ResidentFilterError> {
        let deadline = root
            .check_current(provider, context, deadline, &cancelled)?
            .min(self.expires)
            .min(self.prefix.expires);
        if !matches!(&self.prefix.root, ResidentChaseLosRoot::Retained(identity)
            if Rc::ptr_eq(identity, root.identity()))
        {
            return Err(Failure::Binding.into());
        }
        let observation = provider.water_for(self, context, deadline, cancelled)?;
        if observation.decision != ResidentChaseWaterDecision::ClearConditional {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok((
            deadline,
            observation.owner.ok_or(Failure::Binding)?.request().ticket,
        ))
    }
    pub(in super::super) fn require_retained_model(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        player: PlayerId,
    ) -> Result<(), ResidentFilterError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if player != self.prefix.player
            || gate_model(&provider.profile, provider.session.world, player, route)?
                != self.prefix.model
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(in super::super) fn same_retained_model(
        &self,
        prefix: &ResidentChaseLosDependency,
    ) -> bool {
        self.prefix.player == prefix.player && self.prefix.model == prefix.model
    }
    pub(in super::super) fn same_water_model(&self, other: &Self) -> bool {
        self.same_retained_model(&other.prefix)
    }
}
