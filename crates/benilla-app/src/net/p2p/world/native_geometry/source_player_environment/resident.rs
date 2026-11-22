// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine consumed-source point absence replies for existing environment reducers.
//! A query transcript is not proof of a live frame; the runtime owns consumption.
use super::super::products::source_height::{
    CompositionError, PointNoWaterRequest, PreparedSourceHeight,
};
use super::*;
use benilla_world_runtime::native::environment::{
    BoundLiquidResult, EnvironmentSnapshotBinding, LiquidStatus,
};

#[derive(Debug)]
pub(crate) enum ResidentEnvironmentError {
    Context(Failure),
    Source(CompositionError),
}
impl From<Failure> for ResidentEnvironmentError {
    fn from(value: Failure) -> Self {
        Self::Context(value)
    }
}
impl std::fmt::Display for ResidentEnvironmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(error) => write!(f, "resident environment context: {error:?}"),
            Self::Source(error) => write!(f, "resident environment source: {error:?}"),
        }
    }
}
pub(crate) struct ResidentPlayerEnvironmentProvider<'a, 'terrain, 'bundle, 'source> {
    height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
    binding: EnvironmentSnapshotBinding,
    policy: PlayerEnvironmentPolicy,
    world: WorldId,
}
impl<'a, 'terrain, 'bundle, 'source>
    ResidentPlayerEnvironmentProvider<'a, 'terrain, 'bundle, 'source>
{
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, ResidentEnvironmentError> {
        poll(deadline, &cancelled)?;
        session.check_participants()?;
        let policy = PlayerEnvironmentPolicy::from_profile(profile, player)?;
        policy.check(session)?;
        if session.player_environment.as_ref() != Some(&policy) {
            return Err(Failure::Binding.into());
        }
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(Failure::Binding)?;
        let binding = height
            .environment_binding(profile.content_digest, source.catalog(), &cancelled)
            .map_err(ResidentEnvironmentError::Source)?;
        if binding.map != session.map || binding != source.snapshot_binding() {
            return Err(Failure::Binding.into());
        }
        poll(deadline, &cancelled)?;
        Ok(Self {
            height,
            binding,
            policy,
            world: session.world,
        })
    }
    pub(crate) fn resolve(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentEnvironmentError> {
        self.resolve_inner(session, request, deadline, cancelled, |_| {})
    }
    #[cfg(test)]
    pub(crate) fn resolve_observed(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(u8),
    ) -> Result<Facts, ResidentEnvironmentError> {
        self.resolve_inner(session, request, deadline, cancelled, observe)
    }
    fn resolve_inner(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(u8),
    ) -> Result<Facts, ResidentEnvironmentError> {
        poll(deadline, &cancelled)?;
        session.check_participants()?;
        self.policy.check(session)?;
        if session.world != self.world || session.player_environment.as_ref() != Some(&self.policy)
        {
            return Err(Failure::Binding.into());
        }
        // This checks owner, source-selected height, sequence/ticket, mask15 and
        // exact pose equality within the transcript. Current-frame consumption
        // remains NativeWorld's responsibility, not an inference from equality.
        session.body(request)?;
        let Query::PlayerEnvironment(query) = &request.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        let liquid = query.liquid_query();
        if liquid.snapshot != self.binding || liquid.required_type != 15 {
            return Err(Failure::Binding.into());
        }
        observe(0);
        poll(deadline, &cancelled)?;
        let expected = PointNoWaterRequest {
            binding: self.height.binding(),
            ticket: liquid.ticket,
            position: liquid.position,
            mask: liquid.required_type,
        };
        let absence = self
            .height
            .point_no_water(expected, deadline, &cancelled)
            .map_err(ResidentEnvironmentError::Source)?;
        if absence.request() != expected {
            return Err(Failure::Binding.into());
        }
        // Absence precedes height-dependent classification. Retain the exact
        // source-owned collision height in the reply's original query anyway.
        let result = BoundLiquidResult {
            query: liquid,
            status: LiquidStatus::NoWater,
            data: None,
        };
        let facts = PlayerEnvironmentFacts::new(query, result).map_err(|_| Failure::Binding)?;
        observe(1);
        poll(deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentEnvironmentError::Source)?;
        poll(deadline, &cancelled)?;
        Ok(Facts::PlayerEnvironment(facts))
    }
}
fn poll(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), ResidentEnvironmentError> {
    if cancelled() {
        return Err(ResidentEnvironmentError::Source(
            CompositionError::Cancelled,
        ));
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline.into());
    }
    Ok(())
}
