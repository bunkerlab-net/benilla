// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual immutable TerrainInfo products for one explicitly selected player owner.
//! An observation does not execute environment callbacks or open native admission.
use super::{
    core::{Failure, PlayerEnvironmentPolicy, Session},
    products::{terrain_wet_liquid::TerrainLiquidSnapshot, Error},
};
use benilla_world_runtime::{
    native::{AuthorityRequest, Facts, PlayerEnvironmentFacts, Profile, Query},
    PlayerId, WorldId,
};
use std::time::Instant;

pub(crate) struct SourcePlayerEnvironmentProvider {
    snapshot: TerrainLiquidSnapshot<'static>,
    policy: PlayerEnvironmentPolicy,
    world: WorldId,
}
impl SourcePlayerEnvironmentProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        snapshot: TerrainLiquidSnapshot<'static>,
    ) -> Result<Self, Failure> {
        session.check_participants()?;
        let policy = PlayerEnvironmentPolicy::from_profile(profile, player)?;
        policy.check(session)?;
        let selected = session
            .player_environment
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?;
        if selected != &policy {
            return Err(Failure::UnsupportedQuery);
        }
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?;
        // Public binding DTOs only express equality. Retain the actual loaded reader
        // and compare the complete catalog used by that reader, not just its digest.
        if source.snapshot_binding() != snapshot.environment_binding()
            || source.catalog() != snapshot.environment_catalog()
        {
            return Err(Failure::Binding);
        }
        Ok(Self {
            snapshot,
            policy: selected.clone(),
            world: session.world,
        })
    }

    pub(crate) fn resolve(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<Facts, Failure> {
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        if session.world != self.world || session.player_environment.as_ref() != Some(&self.policy)
        {
            return Err(Failure::Binding);
        }
        session.body(request)?;
        let Query::PlayerEnvironment(query) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if query.snapshot_binding() != self.snapshot.environment_binding() {
            return Err(Failure::Binding);
        }
        let result = self
            .snapshot
            .query_environment(query.liquid_query(), deadline)
            .map_err(|error| match error {
                Error::Identity => Failure::Binding,
                Error::Deadline => Failure::Deadline,
                _ => Failure::MissingGeometry,
            })?;
        let facts = PlayerEnvironmentFacts::new(query, result).map_err(|_| Failure::Binding)?;
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        Ok(Facts::PlayerEnvironment(facts))
    }
}

#[cfg(test)]
#[path = "source_player_environment/tests.rs"]
pub(super) mod tests;
