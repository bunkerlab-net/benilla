// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit live operations on the privately paired provider and ledger.
use super::super::super::{
    products::source_height::{
        inventory::InventoryCompatibility, ResidentInitializationConfiguration,
    },
    source_route_resident_local::ledger::WarmLiveness,
};
use super::*;
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::{
    native::{AuthorityRequest, Facts, MotionFacts, MotionQuery, NativeError, Query},
    PlayerId,
};
use std::{rc::Weak, time::Instant};

pub(in super::super::super) struct OwnerLiveness {
    provider: (Weak<()>, Weak<()>),
    warm: Option<WarmLiveness>,
}
impl OwnerLiveness {
    pub(in super::super::super) fn has_retained_lineage(&self) -> bool {
        self.warm.is_some()
    }

    pub(in super::super::super) fn released(&self) -> bool {
        self.provider.0.strong_count() == 0
            && self.provider.1.strong_count() == 0
            && self
                .warm
                .as_ref()
                .is_none_or(|warm| !warm.is_alive() && !warm.cold_is_alive())
    }
}
fn binding<T>(_: T) -> NativeError {
    NativeError::AuthorityBinding
}
impl ResidentReconstructionOwner<'_, '_, '_, '_> {
    pub(in super::super::super) fn liveness(&self) -> OwnerLiveness {
        OwnerLiveness {
            provider: self.provider.liveness_for_reconstruction_test(),
            warm: self.ledger.committed_warm_liveness_for_test().ok(),
        }
    }
    pub(in super::super::super) fn constructor_count(&self) -> usize {
        self.provider.retained_constructor_count()
    }
    pub(in super::super::super) fn ledger_counts(&self) -> (usize, usize) {
        self.ledger.counts()
    }
    /// No copied replies enter this boundary. Every Route uses the current live dispatch.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super::super) fn resolve_live(
        &mut self,
        world: &mut bevy::prelude::World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        session: &Session,
        player: PlayerId,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<Facts>, NativeError> {
        if self.session.as_ref() != Some(session) {
            return Err(NativeError::AuthorityBinding);
        }

        let facts = match &request.query {
            Query::PlayerEnvironment(_) => self
                .provider
                .player_environment(player, deadline, &cancelled)
                .map_err(binding)?
                .resolve(session, request, deadline, &cancelled)
                .map_err(binding)?,
            Query::SourceMotion(MotionQuery::NearPoint(_)) => self
                .provider
                .resolve_dry_primary_near_point(
                    session,
                    request,
                    player,
                    compatibility,
                    Some(ResidentInitializationConfiguration::VmapHeightEnabled),
                    deadline,
                    &cancelled,
                )
                .map_err(binding)?,
            Query::SourceMotion(MotionQuery::Route(route)) => match route
                .request
                .prior_cache
                .as_deref()
                .map(|cache| cache.revision)
            {
                None => {
                    let constructor = self
                        .provider
                        .observe(request, 101, deadline, &cancelled)
                        .map_err(binding)?;
                    let los = self
                        .provider
                        .observe_chase_los(&constructor, request, player, 103, deadline, &cancelled)
                        .map_err(binding)?;
                    let water = self
                        .provider
                        .observe_chase_water(&los, request, 104, deadline, &cancelled)
                        .map_err(binding)?;
                    self.ledger
                        .certify(
                            world,
                            compatibility,
                            &mut self.provider,
                            request,
                            constructor,
                            water,
                            105,
                            deadline,
                            &cancelled,
                        )
                        .map_err(binding)?
                }
                Some(1) => self
                    .ledger
                    .certify_retained(
                        world,
                        compatibility,
                        &mut self.provider,
                        request,
                        201,
                        deadline,
                        &cancelled,
                    )
                    .map_err(binding)?,
                Some(2) => self
                    .ledger
                    .certify_second_retained(
                        world,
                        compatibility,
                        &mut self.provider,
                        request,
                        401,
                        deadline,
                        &cancelled,
                    )
                    .map_err(binding)?,
                _ => {
                    return Err(NativeError::AuthorityUnavailable(
                        "resident restart supports only existing three epochs",
                    ));
                }
            },
            Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(bearing))) => {
                let angle = benilla_sim::facing::bearing_bits(
                    [bearing.origin.x.bits(), bearing.origin.y.bits()],
                    [bearing.toward.x.bits(), bearing.toward.y.bits()],
                )
                .map_err(binding)?;
                Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
                    request: motion::Query::Bearing(bearing.clone()),
                    fact: motion::Fact::Bearing(ai::Real::from_bits(angle).map_err(binding)?),
                })))
            }
            Query::SourceMotion(MotionQuery::Spatial(_)) | Query::CreatureInitialization(_) => {
                return Err(NativeError::AuthorityUnavailable(
                    "unsupported resident restart query",
                ));
            }
            _ => return Ok(None),
        };
        Ok(Some(facts))
    }
}
