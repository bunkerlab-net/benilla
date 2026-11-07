// SPDX-License-Identifier: GPL-3.0-or-later
//! First-entry GetZoneAndAreaId evidence, separate from ticked movement observations.
//! Complete source bytes and exact pose are bound; this does not certify asset provenance.
use super::*;
use benilla_sim::creature_motion::Pose;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InitialCastEntryLocationSource {
    snapshot: SourceLocationSnapshotBinding,
    #[serde(skip)]
    catalog: SourceLocationCatalog,
}
impl InitialCastEntryLocationSource {
    pub fn new(
        snapshot: SourceLocationSnapshotBinding,
        catalog: SourceLocationCatalog,
    ) -> Result<Self, NativeError> {
        if snapshot.catalog != catalog.digest()
            || [snapshot.source_epoch, snapshot.inventory, snapshot.policy].contains(&[0; 32])
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(Self { snapshot, catalog })
    }
    pub fn snapshot_binding(&self) -> SourceLocationSnapshotBinding {
        self.snapshot
    }
    pub fn catalog(&self) -> &SourceLocationCatalog {
        &self.catalog
    }
    pub fn identity(&self) -> crate::Digest {
        crate::digest(b"benilla-native-entry-location-source-v1\0", self)
    }
    pub fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.snapshot.catalog != self.catalog.digest()
            || [
                self.snapshot.source_epoch,
                self.snapshot.inventory,
                self.snapshot.policy,
            ]
            .contains(&[0; 32])
        {
            return Err(NativeError::AuthorityBinding);
        }
        if self.snapshot.map != profile.map_id || self.snapshot.content != profile.content_digest {
            return Err(NativeError::AuthorityBinding);
        }
        // Both epochs select the same frozen source owners. Independent entry
        // evidence must not disagree with a later ordinary location catalog.
        if profile.source_location().is_some_and(|source| {
            source.snapshot_binding() != self.snapshot || source.catalog() != &self.catalog
        }) {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCastEntryLocationQuery {
    snapshot: SourceLocationSnapshotBinding,
    source: crate::Digest,
    profile: crate::Digest,
    world: crate::WorldId,
    player: crate::PlayerId,
    pose: Pose,
    scope: crate::Digest,
}
impl InitialCastEntryLocationQuery {
    pub fn new(
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Self, NativeError> {
        let input = profile
            .initial_cast_source_input()
            .ok_or(NativeError::AuthorityUnavailable("initial cast source"))?;
        let source = input
            .entry_location
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "initial entry location source",
            ))?;
        validate_source(profile, input)?;
        let pose = Pose {
            position: profile.player.start,
            orientation: ai::Real::from_bits(profile.player.orientation.bits())?,
        };
        pose.position.distance_squared(pose.position)?;
        Ok(Self {
            snapshot: source.snapshot,
            source: source.identity(),
            profile: profile.digest(),
            world,
            player,
            pose,
            scope: input.hook_scope_identity(),
        })
    }
    pub fn snapshot_binding(&self) -> SourceLocationSnapshotBinding {
        self.snapshot
    }
    pub fn pose(&self) -> Pose {
        self.pose
    }
    pub fn validate_for(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        if self != &Self::new(profile, world, player)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
/// The provider supplies the actual source flag; zone/area are re-resolved from retained bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCastEntryLocationFacts {
    query: InitialCastEntryLocationQuery,
    area_flag: u16,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    area: Option<SourceLocationArea>,
}
impl InitialCastEntryLocationFacts {
    pub fn new(
        query: &InitialCastEntryLocationQuery,
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
    pub fn query(&self) -> &InitialCastEntryLocationQuery {
        &self.query
    }
    pub fn area_flag(&self) -> u16 {
        self.area_flag
    }
    pub fn area(&self) -> Option<SourceLocationArea> {
        self.area
    }
    pub fn area_id(&self) -> u32 {
        self.area.map_or(0, |a| a.id)
    }
    pub fn zone_id(&self) -> u32 {
        self.area
            .map_or(0, |a| if a.zone == 0 { a.id } else { a.zone })
    }
    pub fn validate(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        self.query.validate_for(profile, world, player)?;
        let source = profile
            .initial_cast_source_input()
            .and_then(|i| i.entry_location.as_ref())
            .ok_or(NativeError::AuthorityBinding)?;
        if self != &Self::new(&self.query, source.catalog(), self.area_flag)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
pub(super) fn validate_source(
    profile: &Profile,
    input: &NativeInitialCastSourceInput,
) -> Result<(), NativeError> {
    let rows = input.hooks.spell_area.complete()?;
    match &input.entry_location {
        Some(source) => source.validate(profile),
        None if rows.is_empty() => Ok(()),
        None => Err(NativeError::AuthorityUnavailable(
            "initial entry location source",
        )),
    }
}
pub(super) fn validate_entry(
    profile: &Profile,
    world: crate::WorldId,
    player: crate::PlayerId,
    input: &NativeInitialCastSourceInput,
    facts: Option<&InitialCastEntryLocationFacts>,
) -> Result<(), NativeError> {
    validate_source(profile, input)?;
    match (&input.entry_location, facts) {
        (None, None) => Ok(()),
        (Some(_), Some(facts)) => {
            facts.validate(profile, world, player)?;
            let SpellSourceRows::Complete(ids) = &input.queried_spell_ids else {
                return Err(NativeError::AuthorityUnavailable(
                    "initial entry area spell scope",
                ));
            };
            for row in input.hooks.spell_area.complete()? {
                if ids.binary_search(&row.spell).is_ok()
                    || (row.aura_spell != 0
                        && ids.binary_search(&row.aura_spell.unsigned_abs()).is_ok())
                    || (row.area != 0
                        && (row.area == facts.zone_id() || row.area == facts.area_id()))
                {
                    return Err(NativeError::UnsupportedMechanic(
                        "applicable initial entry spell_area row",
                    ));
                }
            }
            Ok(())
        }
        _ => Err(NativeError::AuthorityBinding),
    }
}

#[cfg(test)]
mod tests;
