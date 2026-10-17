// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned TerrainInfo/GetZoneAndAreaId reader for one selected player session.
//! Location observations neither certify spell_area absence nor open native admission.
use super::{
    core::{Failure, Session, SourceLocationPolicy},
    products::{terrain_area::TerrainAreaRequest, terrain_area::TerrainAreaSnapshot, Error},
};
use benilla_world_runtime::{
    native::{AuthorityRequest, Facts, Profile, Query, SourceLocationArea, SourceLocationFacts},
    PlayerId,
};
use std::time::Instant;

pub(crate) struct SourceLocationProvider {
    snapshot: TerrainAreaSnapshot<'static>,
    session: Session,
}
impl SourceLocationProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        snapshot: TerrainAreaSnapshot<'static>,
    ) -> Result<Self, Failure> {
        session.check_participants()?;
        let policy = SourceLocationPolicy::from_profile(profile, player)?;
        policy.check(session)?;
        if session.source_location.as_ref() != Some(&policy) {
            return Err(Failure::UnsupportedQuery);
        }
        let source = profile.source_location().ok_or(Failure::UnsupportedQuery)?;
        // Equality DTOs alone cannot answer a spatial query: retain the actual
        // readers and compare their complete byte-derived catalog at selection.
        if source.snapshot_binding() != snapshot.location_binding()
            || source.catalog() != snapshot.location_catalog()
        {
            return Err(Failure::Binding);
        }
        Ok(Self {
            snapshot,
            session: session.clone(),
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
        if session != &self.session {
            return Err(Failure::Binding);
        }
        session.body(request)?;
        let Query::SourceLocation(query) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if query.snapshot_binding() != self.snapshot.location_binding() {
            return Err(Failure::Binding);
        }
        let pose = query.pose().position;
        let product_request = TerrainAreaRequest {
            snapshot: self.snapshot.binding(),
            ticket: query.ticket(),
            position: [pose.x.bits(), pose.y.bits(), pose.z.bits()],
        };
        let reply = self
            .snapshot
            .query_location(product_request, deadline)
            .map_err(|error| match error {
                Error::Identity => Failure::Binding,
                Error::Deadline => Failure::Deadline,
                _ => Failure::MissingGeometry,
            })?;
        if reply.request != product_request {
            return Err(Failure::Binding);
        }
        let facts = SourceLocationFacts::new(
            query,
            self.snapshot.location_catalog(),
            reply.result.area_flag,
        )
        .map_err(|_| Failure::Binding)?;
        let area = reply.result.area.map(|area| SourceLocationArea {
            id: area.id,
            map_id: area.map_id,
            zone: area.zone,
            explore_flag: area.explore_flag,
            liquid_override: area.liquid_override,
        });
        if facts.area() != area
            || facts.zone_id() != reply.result.zone_id
            || facts.area_id() != reply.result.area_id
        {
            return Err(Failure::Binding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        Ok(Facts::SourceLocation(facts))
    }
}

#[cfg(test)]
#[path = "source_location/tests.rs"]
pub(super) mod tests;

/// Owned first-entry reader. A separate query prevents a tick-zero genesis fact
/// from being substituted for the normal source-location update observation.
pub(crate) struct InitialCastEntryLocationProvider {
    snapshot: TerrainAreaSnapshot<'static>,
    session: Session,
    profile: Profile,
    player: PlayerId,
}
impl InitialCastEntryLocationProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        snapshot: TerrainAreaSnapshot<'static>,
    ) -> Result<Self, Failure> {
        session.check_participants()?;
        profile.validate().map_err(|_| Failure::Binding)?;
        if session.profile != profile.digest()
            || session.map != profile.map_id
            || session.content != profile.content_digest
            || session.geometry != profile.geometry_revision
            || session.policy != profile.authority_policy
        {
            return Err(Failure::Binding);
        }
        let query = benilla_world_runtime::native::InitialCastEntryLocationQuery::new(
            profile,
            session.world,
            player,
        )
        .map_err(|_| Failure::Binding)?;
        if query.snapshot_binding() != snapshot.location_binding() {
            return Err(Failure::Binding);
        }
        // The byte-derived catalog digest is part of location_binding; the
        // snapshot retains all actual terrain/VMap readers, not asserted IDs.
        Ok(Self {
            snapshot,
            session: session.clone(),
            profile: profile.clone(),
            player,
        })
    }
    pub(crate) fn resolve(
        &self,
        session: &Session,
        query: &benilla_world_runtime::native::InitialCastEntryLocationQuery,
        deadline: Instant,
    ) -> Result<benilla_world_runtime::native::InitialCastEntryLocationFacts, Failure> {
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        if session != &self.session {
            return Err(Failure::Binding);
        }
        query
            .validate_for(&self.profile, session.world, self.player)
            .map_err(|_| Failure::Binding)?;
        if query.snapshot_binding() != self.snapshot.location_binding() {
            return Err(Failure::Binding);
        }
        let pose = query.pose().position;
        let request = TerrainAreaRequest {
            snapshot: self.snapshot.binding(),
            ticket: 1,
            position: [pose.x.bits(), pose.y.bits(), pose.z.bits()],
        };
        let reply =
            self.snapshot
                .query_location(request, deadline)
                .map_err(|error| match error {
                    Error::Identity => Failure::Binding,
                    Error::Deadline => Failure::Deadline,
                    _ => Failure::MissingGeometry,
                })?;
        if reply.request != request {
            return Err(Failure::Binding);
        }
        let facts = benilla_world_runtime::native::InitialCastEntryLocationFacts::new(
            query,
            self.snapshot.location_catalog(),
            reply.result.area_flag,
        )
        .map_err(|_| Failure::Binding)?;
        let area = reply.result.area.map(|a| SourceLocationArea {
            id: a.id,
            map_id: a.map_id,
            zone: a.zone,
            explore_flag: a.explore_flag,
            liquid_override: a.liquid_override,
        });
        if facts.area() != area
            || facts.zone_id() != reply.result.zone_id
            || facts.area_id() != reply.result.area_id
        {
            return Err(Failure::Binding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        Ok(facts)
    }
}
