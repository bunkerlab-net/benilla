// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-owned GetZoneAndAreaId witness for a bounded spell_area-free execution scope.
//! This is a precondition, not an emulation of Player's area-update timer/callbacks.
use super::*;
use benilla_sim::creature_motion as motion;
use serde::{Deserialize, Serialize};
mod catalog;
pub use catalog::{SourceLocationArea, SourceLocationCatalog};

pub const SOURCE_LOCATION_POLICY: &str = "cmangos-8ec338a1-native-source-location-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocationSnapshotBinding {
    pub map: u32,
    pub content: crate::Digest,
    pub source_epoch: crate::Digest,
    pub inventory: crate::Digest,
    pub policy: crate::Digest,
    pub catalog: crate::Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceLocationSource {
    policy: &'static str,
    snapshot: SourceLocationSnapshotBinding,
    #[serde(skip)]
    catalog: SourceLocationCatalog,
}
impl SourceLocationSource {
    pub fn snapshot_binding(&self) -> SourceLocationSnapshotBinding {
        self.snapshot
    }
    pub fn catalog(&self) -> &SourceLocationCatalog {
        &self.catalog
    }
    pub fn binding(&self) -> crate::Digest {
        crate::digest(b"benilla-source-location-source-v1\0", self)
    }
    pub(in crate::native) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != SOURCE_LOCATION_POLICY
            || self.snapshot.map != profile.map_id
            || self.snapshot.content != profile.content_digest
            || self.snapshot.catalog != self.catalog.digest()
            || [
                self.snapshot.source_epoch,
                self.snapshot.inventory,
                self.snapshot.policy,
            ]
            .contains(&[0; 32])
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
}
fn spell_catalog(profile: &Profile) -> Result<&SpellAuthorityCatalog, NativeError> {
    profile
        .spell_location_catalog()
        .ok_or(NativeError::AuthorityUnavailable(
            "source location spell catalog",
        ))
}
impl Profile {
    pub fn source_location(&self) -> Option<&SourceLocationSource> {
        self.spell_location_catalog()?.source_location.as_ref()
    }
    pub fn initialize_source_location(
        &mut self,
        snapshot: SourceLocationSnapshotBinding,
        catalog: SourceLocationCatalog,
    ) -> Result<(), NativeError> {
        let source = SourceLocationSource {
            policy: SOURCE_LOCATION_POLICY,
            snapshot,
            catalog,
        };
        source.validate(self)?;
        if self.source_location().is_some_and(|old| old != &source) {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.spell_location_catalog_mut()
            .ok_or(NativeError::AuthorityUnavailable(
                "source location spell catalog",
            ))?
            .source_location = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Exact runtime-produced request. The provider must additionally match its retained
/// loaded session and actual source snapshot; this DTO cannot certify either one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocationQuery {
    snapshot: SourceLocationSnapshotBinding,
    source: crate::Digest,
    profile: crate::Digest,
    world: crate::WorldId,
    player: crate::PlayerId,
    tick: u64,
    input_sequence: u64,
    ticket: u64,
    pose: motion::Pose,
    spell_area_scope: crate::Digest,
}
impl SourceLocationQuery {
    pub fn snapshot_binding(&self) -> SourceLocationSnapshotBinding {
        self.snapshot
    }
    pub fn ticket(&self) -> u64 {
        self.ticket
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn pose(&self) -> motion::Pose {
        self.pose
    }
    pub(super) fn new(
        profile: &Profile,
        state: &State,
        ticket: u64,
        pose: motion::Pose,
    ) -> Result<Self, NativeError> {
        let source = profile
            .source_location()
            .ok_or(NativeError::AuthorityUnavailable(
                "source-owned current location",
            ))?;
        let query = Self {
            snapshot: source.snapshot,
            source: source.binding(),
            profile: profile.digest(),
            world: state.world,
            player: state.player,
            tick: state.tick,
            input_sequence: state.next_sequence,
            ticket,
            pose,
            spell_area_scope: spell_catalog(profile)?.spell_area_scope_binding()?,
        };
        query.validate_for(profile, state.world, state.player)?;
        Ok(query)
    }
    pub fn validate_for(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        let source = profile
            .source_location()
            .ok_or(NativeError::AuthorityUnavailable(
                "source-owned current location",
            ))?;
        source.validate(profile)?;
        if self.snapshot != source.snapshot
            || self.source != source.binding()
            || self.profile != profile.digest()
            || self.world != world
            || self.player != player
            || self.tick == 0
            || self.tick > crate::MAX_TICK
            || !(1..=20_000).contains(&self.ticket)
            || self.spell_area_scope != spell_catalog(profile)?.spell_area_scope_binding()?
        {
            return Err(NativeError::AuthorityBinding);
        }
        self.pose.position.distance_squared(self.pose.position)?;
        if self.pose.orientation.bits()
            != Orientation::from_bits(self.pose.orientation.bits())?.bits()
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

/// The narrowed actual source flag is resolved again over the complete bound DBC.
/// No caller-provided zone, area, permission or hook-absence boolean is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocationFacts {
    query: SourceLocationQuery,
    area_flag: u16,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    area: Option<SourceLocationArea>,
}
impl SourceLocationFacts {
    pub fn new(
        query: &SourceLocationQuery,
        catalog: &SourceLocationCatalog,
        area_flag: u16,
    ) -> Result<Self, NativeError> {
        if query.snapshot.catalog != catalog.digest() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(Self {
            query: query.clone(),
            area_flag,
            area: catalog.resolve(query.snapshot.map, area_flag),
        })
    }
    pub fn query(&self) -> &SourceLocationQuery {
        &self.query
    }
    pub fn area_flag(&self) -> u16 {
        self.area_flag
    }
    pub fn validate_with_catalog(
        &self,
        query: &SourceLocationQuery,
        catalog: &SourceLocationCatalog,
    ) -> Result<(), NativeError> {
        self.validate_for(query)?;
        if self != &Self::new(query, catalog, self.area_flag)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub fn area(&self) -> Option<SourceLocationArea> {
        self.area
    }
    pub fn area_id(&self) -> u32 {
        self.area.map_or(0, |area| area.id)
    }
    pub fn zone_id(&self) -> u32 {
        self.area
            .map_or(0, |area| if area.zone != 0 { area.zone } else { area.id })
    }
    pub fn validate_for(&self, query: &SourceLocationQuery) -> Result<(), NativeError> {
        if &self.query != query {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub(super) fn validate(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        self.query.validate_for(profile, world, player)?;
        let source = profile
            .source_location()
            .ok_or(NativeError::InvalidProfile)?;
        if self != &Self::new(&self.query, &source.catalog, self.area_flag)? {
            return Err(NativeError::AuthorityBinding);
        }
        spell_catalog(profile)?.validate_location_scope(self.zone_id(), self.area_id())
    }
}
pub(super) fn selected(profile: &Profile) -> bool {
    profile.source_location().is_some()
        || spell_catalog(profile).is_ok_and(SpellAuthorityCatalog::requires_source_location)
}
pub(super) fn validate_current(profile: &Profile, state: &State) -> Result<(), NativeError> {
    match &state.source_location {
        None if !selected(profile) || state.tick == 0 => Ok(()),
        Some(facts) if selected(profile) && state.tick > 0 => {
            facts.validate(profile, state.world, state.player)?;
            if facts.query.tick != state.tick
                || facts.query.input_sequence > state.next_sequence
                || facts.query.pose.position != state.position
                || facts.query.pose.orientation.bits() != state.orientation.bits()
            {
                return Err(NativeError::AuthorityBinding);
            }
            Ok(())
        }
        _ => Err(NativeError::AuthorityBinding),
    }
}
