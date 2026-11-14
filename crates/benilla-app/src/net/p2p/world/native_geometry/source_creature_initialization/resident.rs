// SPDX-License-Identifier: GPL-3.0-or-later
//! Resident point-only initialization observations. The runtime owns live consumption.
use super::super::products::source_height::{
    CompositionError, PointNoWaterRequest, PreparedSourceHeight,
    ResidentInitializationConfiguration,
};
use super::*;

#[derive(Debug)]
pub(crate) enum ResidentInitializationError {
    Context(Failure),
    Source(CompositionError),
}
impl From<Failure> for ResidentInitializationError {
    fn from(value: Failure) -> Self {
        Self::Context(value)
    }
}
impl std::fmt::Display for ResidentInitializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(error) => write!(f, "resident initialization context: {error:?}"),
            Self::Source(error) => write!(f, "resident initialization source: {error:?}"),
        }
    }
}
pub(crate) struct ResidentCreatureInitializationProvider<'a, 'terrain, 'bundle, 'source> {
    height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
    binding: CreatureInitializationSnapshot,
    profile: Profile,
    world: WorldId,
}
impl<'a, 'terrain, 'bundle, 'source>
    ResidentCreatureInitializationProvider<'a, 'terrain, 'bundle, 'source>
{
    pub(crate) fn bind(
        world: WorldId,
        profile: &Profile,
        height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
        configuration: Option<ResidentInitializationConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, ResidentInitializationError> {
        poll(deadline, &cancelled)?;
        profile.validate().map_err(|_| Failure::Binding)?;
        let binding = height
            .initialization_binding(profile.content_digest, configuration, deadline, &cancelled)
            .map_err(ResidentInitializationError::Source)?;
        if world == [0; 16] || binding.map != profile.map_id {
            return Err(Failure::Binding.into());
        }
        let catalog = profile
            .creature_capabilities
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?;
        if !profile.npcs.iter().any(|npc| {
            catalog.source(npc.ai.creature_id).is_some_and(|source| {
                matches!(&source.initialization,
                    CreatureInitializationClosure::DryWater { snapshot, .. }
                        if snapshot == &binding)
            })
        }) {
            return Err(Failure::Binding.into());
        }
        poll(deadline, &cancelled)?;
        Ok(Self {
            height,
            binding,
            profile: profile.clone(),
            world,
        })
    }
    pub(crate) fn resolve(
        &self,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentInitializationError> {
        self.resolve_inner(request, deadline, cancelled, |_| {})
    }
    #[cfg(test)]
    pub(crate) fn resolve_observed(
        &self,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<Facts, ResidentInitializationError> {
        self.resolve_inner(request, deadline, cancelled, observe)
    }
    fn resolve_inner(
        &self,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(u8),
    ) -> Result<Facts, ResidentInitializationError> {
        poll(deadline, &cancelled)?;
        let Query::CreatureInitialization(query) = &request.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if request.binding != query.binding
            || request.binding.world != self.world
            || request.binding.profile != self.profile.digest()
            || request.binding.authority_policy != self.profile.authority_policy
            || request.binding.geometry != self.profile.geometry_revision
            || query.snapshot != self.binding
        {
            return Err(Failure::Binding.into());
        }
        // Consistency of the immutable transcript is not proof of current RNG,
        // clock or generation ownership. NativeWorld consumes its exact request.
        query
            .validate_for(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let expected = PointNoWaterRequest {
            binding: self.height.binding(),
            ticket: query.ticket,
            position: [
                query.position.x.bits(),
                query.position.y.bits(),
                query.position.z.bits(),
            ],
            mask: query.required_type,
        };
        observe(0);
        poll(deadline, &cancelled)?;
        let absence = self
            .height
            .point_no_water(expected, deadline, &cancelled)
            .map_err(ResidentInitializationError::Source)?;
        if absence.request() != expected {
            return Err(Failure::Binding.into());
        }
        // GridMap.cpp:1147-1160: NoWater bypasses the selected-model depth
        // comparison. The inner default 0x4002007e remains request-bound but is
        // never substituted with the selected model height or bounding radius.
        observe(1);
        poll(deadline, &cancelled)?;
        let fresh = self
            .height
            .verify_unchanged(|| cancelled() || Instant::now() >= deadline);
        poll(deadline, &cancelled)?;
        fresh.map_err(ResidentInitializationError::Source)?;
        Ok(Facts::CreatureInitialization(Box::new(
            CreatureInitializationFacts {
                request: (**query).clone(),
                status: environment::LiquidStatus::NoWater,
                data: None,
            },
        )))
    }
}
fn poll(
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<(), ResidentInitializationError> {
    if cancelled() {
        return Err(ResidentInitializationError::Source(
            CompositionError::Cancelled,
        ));
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline.into());
    }
    Ok(())
}
