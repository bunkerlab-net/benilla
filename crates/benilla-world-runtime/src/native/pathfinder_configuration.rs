// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// MoveMap.cpp:64-83, Creature.cpp:146/593/3065-3073, Unit.cpp:12363-12369,
// DBCStructure.h:593 and PathFinder.cpp:37-106. Upstream GPL-2.0-or-later.
// Copyright CMaNGOS contributors; Rust translation GPL-3.0-or-later. No warranty.
//! Frozen, locally selected effective configuration, not authenticated server
//! configuration or a loaded query. The constructor binding is limited to an
//! enabled, fixed outdoor map, explicit instance slot zero and ordinary NPCs.
//! A real constructor query must separately retain the source's 1024-node query
//! and its map-instance/generator/generation lineage; the bounded loader's 512
//! nodes do not establish that. No route admission is provided here.
use super::*;
use benilla_formats::MapCatalog;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PATHFINDER_CONFIGURATION_POLICY: &str =
    "cmangos-8ec338a1-native-selected-mmap-configuration-v1";
const MMAP_FORCE_ENABLE: u32 = 0x0000_2000;
const MMAP_FORCE_DISABLE: u32 = 0x0000_4000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct ConfigurationSourcePolicy {
    revision: &'static str,
    // LF-normalized complete MoveMap.cpp, Creature.cpp, Creature.h, Unit.cpp,
    // DBCStructure.h and PathFinder.cpp. No compiled-source oracle is implied.
    sources: [&'static str; 6],
}
const SOURCE_POLICY: ConfigurationSourcePolicy = ConfigurationSourcePolicy {
    revision: "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f",
    sources: [
        "c50d5d5c501613099d4d84af586afcc8cb67389c7a634a639248749c698f2e8a",
        "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
        "a92b0e95e5bf7827389fcca0acc0edaba48a1a083efbf5a01a752a4d171664fc",
        "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
        "354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50",
        "0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef",
    ],
};

/// The complete effective set after configuration selection. This is not the raw
/// comma-separated configuration: upstream reloads accumulate into the set.
/// Complete is a local capture contract, not proof of an external producer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EffectiveDisabledMMapSet {
    Missing,
    Complete(BTreeSet<u32>),
}
impl EffectiveDisabledMMapSet {
    fn require(&self) -> Result<&BTreeSet<u32>, NativeError> {
        match self {
            Self::Missing => Err(NativeError::AuthorityUnavailable(
                "complete effective disabled MMap map set",
            )),
            Self::Complete(maps) => Ok(maps),
        }
    }
}

/// Required selected inputs, including an explicit source instance slot. No
/// Default or serde defaults: zero and an empty complete set must be selected.
/// ```compile_fail
/// use benilla_world_runtime::native::SelectedMMapConfiguration;
/// let _: SelectedMMapConfiguration = Default::default();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedMMapConfiguration {
    pub enabled: bool,
    pub effective_disabled_maps: EffectiveDisabledMMapSet,
    pub source_instance_id: u32,
}
impl SelectedMMapConfiguration {
    /// MoveMap's precedence after requiring the complete selected inputs. This
    /// scalar result does not validate a constructor, current owner or query.
    pub fn pathfinding_enabled(
        &self,
        map_id: u32,
        forcing: MMapForcingStatus,
    ) -> Result<bool, NativeError> {
        let disabled = self.effective_disabled_maps.require()?;
        Ok(self.enabled
            && match forcing {
                MMapForcingStatus::Ignored => false,
                MMapForcingStatus::Forced => true,
                MMapForcingStatus::Default => !disabled.contains(&map_id),
            })
    }
}

/// IsIgnoringMMAP status, distinct from UNIT_STAT_IGNORE_PATHFINDING. The
/// configuration truth table accepts all three; retained NPC sources derive it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MMapForcingStatus {
    Default,
    Forced,
    Ignored,
}
fn creature_forcing(raw_extra_flags: u32, unit_forcing: MMapForcingStatus) -> MMapForcingStatus {
    if raw_extra_flags & MMAP_FORCE_DISABLE != 0 {
        MMapForcingStatus::Ignored
    } else if raw_extra_flags & MMAP_FORCE_ENABLE != 0 {
        MMapForcingStatus::Forced
    } else {
        unit_forcing
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct SelectedMapSource {
    map_id: u32,
    raw_instance_type: u32,
}
impl SelectedMapSource {
    fn from_catalog(map_id: u32, catalog: &MapCatalog) -> Result<Self, NativeError> {
        Ok(Self {
            map_id,
            // The server requires an actual Map.dbc row. Client-side missing=>0
            // and is_party_dungeon (type 1 only) are not this source predicate.
            raw_instance_type: catalog.instance_type(map_id).ok_or(
                NativeError::AuthorityUnavailable("selected Map.dbc instance type row"),
            )?,
        })
    }
    fn is_dungeon(self) -> bool {
        matches!(self.raw_instance_type, 1 | 2)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct OwnerForcingSource {
    spawn_id: u64,
    entry: u32,
    raw_extra_flags: u32,
    capability_source: crate::Digest,
    forcing: MMapForcingStatus,
}
fn owner_sources(profile: &Profile) -> Result<BTreeMap<u64, OwnerForcingSource>, NativeError> {
    let capabilities =
        profile
            .creature_capabilities
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "pathfinder creature capability closure",
            ))?;
    capabilities.validate(profile)?;
    profile
        .source_motion
        .as_ref()
        .ok_or(NativeError::AuthorityUnavailable(
            "pathfinder nontransport source motion domain",
        ))?
        .validate(profile)?;
    let mut owners = BTreeMap::new();
    for npc in &profile.npcs {
        let raw_extra_flags = profile.source_pathfinding_extra_flags(npc.ai.creature_id)?;
        let capability =
            capabilities
                .source(npc.ai.creature_id)
                .ok_or(NativeError::AuthorityUnavailable(
                    "pathfinder creature capability spawn binding",
                ))?;
        // Creature starts false and InitEntry assigns m_ignoreMMAP from 0x4000.
        // The only other pinned writes use SetIgnoreMMAP: Ashenvale ScriptDevAI
        // waypoints (true/false) and JustSummoned (true). Complete capability
        // exclusions reject scripts, summons/control and entry substitution;
        // their lifecycle coverage includes ResetEntry/respawn. Motion validation
        // excludes transports. No NPC player-control transition is admitted.
        // Unit::IsIgnoringMMAP therefore returns DEFAULT; the retained complete
        // combat flags admit only 0x80000, so both MMap override bits are absent.
        // This derives no UNIT_STAT_IGNORE_PATHFINDING value.
        let forcing = creature_forcing(raw_extra_flags, MMapForcingStatus::Default);
        if forcing != MMapForcingStatus::Default {
            return Err(NativeError::UnsupportedMechanic(
                "pathfinder mutable owner forcing",
            ));
        }
        owners.insert(
            npc.ai.creature_id,
            OwnerForcingSource {
                spawn_id: npc.ai.creature_id,
                entry: npc.entry,
                raw_extra_flags,
                capability_source: crate::digest(
                    b"benilla-pathfinder-capability-closure-v1\0",
                    capability,
                ),
                forcing,
            },
        );
    }
    Ok(owners)
}

/// Private retained fields, created only from explicit local settings, the actual
/// MapCatalog row and the profile's complete owner catalogs. No Deserialize.
/// ```compile_fail
/// use benilla_world_runtime::native::PathfinderConfigurationSource;
/// let _: PathfinderConfigurationSource = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PathfinderConfigurationSource {
    policy: &'static str,
    source_policy: ConfigurationSourcePolicy,
    selected: SelectedMMapConfiguration,
    map: SelectedMapSource,
    content: crate::Digest,
    authority_policy: crate::Digest,
    owners: BTreeMap<u64, OwnerForcingSource>,
}
impl PathfinderConfigurationSource {
    pub fn selected(&self) -> &SelectedMMapConfiguration {
        &self.selected
    }
    pub fn map_id(&self) -> u32 {
        self.map.map_id
    }
    pub fn raw_instance_type(&self) -> u32 {
        self.map.raw_instance_type
    }
    pub fn source_is_dungeon(&self) -> bool {
        self.map.is_dungeon()
    }
    pub fn binding(&self) -> crate::Digest {
        crate::digest(b"benilla-pathfinder-configuration-source-v1\0", self)
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        self.selected.effective_disabled_maps.require()?;
        if self.policy != PATHFINDER_CONFIGURATION_POLICY
            || self.source_policy != SOURCE_POLICY
            || self.content != profile.content_digest
            || self.authority_policy != profile.authority_policy
            || self.map.map_id != profile.map_id
            || self.owners != owner_sources(profile)?
        {
            return Err(NativeError::InvalidProfile);
        }
        // Fixed-map outdoor lifecycle only; raw type 3 is also outside this domain
        // even though source IsDungeon is false. Slot zero is explicit, not inferred.
        if self.map.raw_instance_type != 0 || self.selected.source_instance_id != 0 {
            return Err(NativeError::UnsupportedMechanic(
                "pathfinder map-instance configuration domain",
            ));
        }
        Ok(())
    }
    /// Configuration-only prerequisite for an enabled constructor branch. It is
    /// neither a constructor execution receipt nor a live/loaded query lease.
    /// Disabled configurations may be retained, but cannot create this witness:
    /// disabled-at-construction then enabled on the same map would read source's
    /// uninitialized m_defaultNavMeshQuery. Reselection never repairs that branch.
    pub fn constructor_binding(
        &self,
        profile: &Profile,
        spawn_id: u64,
    ) -> Result<PathfinderConfigurationWitness, NativeError> {
        profile.validate()?;
        self.validate(profile)?;
        if profile.pathfinder_configuration.as_ref() != Some(self) {
            return Err(NativeError::AuthorityBinding);
        }
        let owner = self
            .owners
            .get(&spawn_id)
            .ok_or(NativeError::UnknownTarget)?;
        if !self
            .selected
            .pathfinding_enabled(self.map.map_id, owner.forcing)?
        {
            return Err(NativeError::UnsupportedMechanic(
                "pathfinder disabled constructor configuration",
            ));
        }
        Ok(PathfinderConfigurationWitness {
            profile: profile.digest(),
            source: self.binding(),
            map: self.map,
            source_instance_id: self.selected.source_instance_id,
            owner: owner.clone(),
        })
    }
}
impl Profile {
    pub fn pathfinder_configuration(&self) -> Option<&PathfinderConfigurationSource> {
        self.pathfinder_configuration.as_ref()
    }

    /// Freeze selected local effective configuration without claiming native
    /// admission or an external server's configuration. Identical reselection is
    /// allowed; changes, missing dependencies and invalid profiles roll back.
    /// Profile is a caller-editable draft. Replacing its source produces a new
    /// digest; it cannot mutate the immutable Profile owned by a live NativeWorld
    /// or make evidence from the old draft valid against the replacement.
    pub fn initialize_pathfinder_configuration(
        &mut self,
        selected: SelectedMMapConfiguration,
        catalog: &MapCatalog,
    ) -> Result<(), NativeError> {
        if self.pathfinder_configuration.is_some() {
            self.validate()?;
        }
        selected.effective_disabled_maps.require()?;
        let source = PathfinderConfigurationSource {
            policy: PATHFINDER_CONFIGURATION_POLICY,
            source_policy: SOURCE_POLICY,
            selected,
            map: SelectedMapSource::from_catalog(self.map_id, catalog)?,
            content: self.content_digest,
            authority_policy: self.authority_policy,
            owners: owner_sources(self)?,
        };
        source.validate(self)?;
        if self
            .pathfinder_configuration
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.pathfinder_configuration = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Read-only consistency evidence from retained configuration. There is no NPC
/// generation, generator or query identity here, and no current-liveness claim.
/// ```compile_fail
/// use benilla_world_runtime::native::PathfinderConfigurationWitness;
/// let _: PathfinderConfigurationWitness = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PathfinderConfigurationWitness {
    profile: crate::Digest,
    source: crate::Digest,
    map: SelectedMapSource,
    source_instance_id: u32,
    owner: OwnerForcingSource,
}
impl PathfinderConfigurationWitness {
    pub fn map_id(&self) -> u32 {
        self.map.map_id
    }
    pub fn raw_instance_type(&self) -> u32 {
        self.map.raw_instance_type
    }
    pub fn source_is_dungeon(&self) -> bool {
        self.map.is_dungeon()
    }
    pub fn source_instance_id(&self) -> u32 {
        self.source_instance_id
    }
    pub fn spawn_id(&self) -> u64 {
        self.owner.spawn_id
    }
    pub fn raw_extra_flags(&self) -> u32 {
        self.owner.raw_extra_flags
    }
    pub fn forcing(&self) -> MMapForcingStatus {
        self.owner.forcing
    }
    pub fn source_binding(&self) -> crate::Digest {
        self.source
    }
    pub fn validate_for(&self, profile: &Profile, spawn_id: u64) -> Result<(), NativeError> {
        let source =
            profile
                .pathfinder_configuration
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "selected pathfinder configuration",
                ))?;
        if *self != source.constructor_binding(profile, spawn_id)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "pathfinder_configuration_tests.rs"]
mod tests;
