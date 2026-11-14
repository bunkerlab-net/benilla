// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.h:643-646,
// Creature.cpp:155/435-452/1699-1745/1912-1945/2296-2346 and ObjectMgr.cpp:520.
// Upstream GPL-2.0-or-later.
// Copyright CMaNGOS contributors; Rust translation GPL-3.0-or-later. No warranty.
//! Explicit source closure and owned capability-relevant movement flags. This
//! projection is not a complete MovementInfo, geometry provider or admission token.
use super::*;
use benilla_sim::{creature_capabilities as source, creature_init::ExternalFeature};
use serde::{Deserialize, Serialize};

pub const CREATURE_CAPABILITY_POLICY: &str = "cmangos-8ec338a1-native-creature-capability-v3";
/// LF-normalized complete Creature.cpp and Unit.cpp. These source guards do not
/// claim the scalar predicate oracle executes constructor or lifecycle callbacks.
pub const CREATURE_CAPABILITY_RUNTIME_SOURCE_SHA256: [&str; 2] = [
    "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct CreatureCapabilitySourcePolicy {
    predicate: &'static str,
    predicate_sources: [&'static str; 3],
    runtime_sources: [&'static str; 2],
}
const SOURCE_POLICY: CreatureCapabilitySourcePolicy = CreatureCapabilitySourcePolicy {
    predicate: source::POLICY,
    predicate_sources: source::SOURCE_SHA256,
    runtime_sources: CREATURE_CAPABILITY_RUNTIME_SOURCE_SHA256,
};
const FLAG_MASK: u32 =
    source::MOVEFLAG_LEVITATING | source::MOVEFLAG_HOVER | source::MOVEFLAG_CAN_FLY;

/// Complete is a typed capture contract, not proof of a producer's completeness.
/// Missing never supplies an empty lookup, a default row or a ground habitat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureCapabilityRows<T> {
    Missing,
    Complete(Vec<T>),
}
impl<T> CreatureCapabilityRows<T> {
    fn require(&self, name: &'static str) -> Result<&[T], NativeError> {
        match self {
            Self::Missing => Err(NativeError::AuthorityUnavailable(name)),
            Self::Complete(rows) => Ok(rows),
        }
    }
    fn one(&self, name: &'static str) -> Result<&T, NativeError> {
        match self.require(name)? {
            [row] => Ok(row),
            [] => Err(NativeError::AuthorityUnavailable(name)),
            _ => Err(NativeError::InvalidProfile),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCapabilityTemplateSource {
    pub entry: u32,
    /// Raw uint32 creature_template.InhabitType, before ObjectMgr repairs.
    pub inhabit_type: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCapabilitySpawnTemplateSource {
    pub entry: u32,
    pub spawn_flags: u32,
    pub relay_id: u32,
    pub string_id: u32,
}
/// Addon closure is either explicit absence or the exact immutable generation
/// source that owns both addon queries. A digest alone cannot install that owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureCapabilityAddonClosure {
    /// Capture contract: neither the spawn nor the template addon row exists.
    Absent,
    /// Exact source retained by the profile's validated generation catalog.
    GenerationSource { source_identity: crate::Digest },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCapabilitySource {
    pub initialization: CreatureInitializationClosure,
    /// Runtime stable spawn key (NpcId.spawn / ai.creature_id), not an independently
    /// authenticated imported database GUID.
    pub spawn_id: u64,
    /// Raw spawn entry must equal the effective entry in this no-substitution domain.
    pub spawn_entry: u32,
    /// Raw SpawnDataId, including genuine zero; missing positive rows cannot fall back.
    pub requested_spawn_template_entry: u32,
    pub templates: CreatureCapabilityRows<CreatureCapabilityTemplateSource>,
    pub spawn_templates: CreatureCapabilityRows<CreatureCapabilitySpawnTemplateSource>,
    pub addon_closure: CreatureCapabilityAddonClosure,
    /// Complete coverage of creature_init's exclusions, including unowned addon
    /// effects, all pet/client-control/possession, DB and compiled scripts,
    /// creation hooks, entry overrides/random/event/group selection and respawn
    /// re-evaluation. All other captured fields/lookups must be classified here;
    /// the small raw projections above do not independently prove their absence.
    pub external_features: CreatureCapabilityRows<ExternalFeature>,
}
impl CreatureCapabilitySource {
    fn validate(&self) -> Result<(), NativeError> {
        let template = self
            .templates
            .one("effective creature capability template")?;
        self.spawn_templates
            .one("effective creature capability spawn template")?;
        if self.spawn_id <= SOLO_UNIT
            || template.entry == 0
            || template.entry > 0xff_ffff
            || template.entry != self.spawn_entry
        {
            return Err(NativeError::InvalidProfile);
        }
        source::InhabitType::try_from_raw(template.inhabit_type)
            .map_err(|_| NativeError::UnsupportedMechanic("raw creature habitat loader repair"))?;
        if !self
            .external_features
            .require("creature capability source exclusions")?
            .is_empty()
        {
            return Err(NativeError::UnsupportedMechanic(
                "creature capability source exclusions",
            ));
        }
        let spawn = self
            .spawn_templates
            .one("effective creature capability spawn template")?;
        if spawn.entry != self.requested_spawn_template_entry {
            return Err(NativeError::UnsupportedMechanic(
                "creature spawn-template fallback or substitution",
            ));
        }
        if spawn.spawn_flags & !1 != 0 || spawn.relay_id != 0 || spawn.string_id != 0 {
            return Err(NativeError::UnsupportedMechanic(
                "creature capability spawn flags or scripts",
            ));
        }
        Ok(())
    }
    pub(super) fn binding(&self) -> crate::Digest {
        crate::digest(
            b"benilla-creature-capability-source-v3\0",
            &(
                CREATURE_CAPABILITY_POLICY,
                CREATURE_INITIALIZATION_POLICY,
                benilla_sim::creature_swimming::POLICY,
                benilla_sim::creature_swimming::SOURCE_SHA256,
                SOURCE_POLICY,
                self,
            ),
        )
    }
}

/// Validated retained source catalog. No Deserialize and no caller-writable entries.
/// ```compile_fail
/// use benilla_world_runtime::native::CreatureCapabilityCatalog;
/// let _: CreatureCapabilityCatalog = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCapabilityCatalog {
    policy: &'static str,
    source_policy: CreatureCapabilitySourcePolicy,
    spawns: BTreeMap<u64, CreatureCapabilitySource>,
}
impl CreatureCapabilityCatalog {
    pub fn validate_sources(
        rows: CreatureCapabilityRows<CreatureCapabilitySource>,
    ) -> Result<Self, NativeError> {
        let rows = rows.require("creature capability sources")?;
        if rows.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        let mut spawns = BTreeMap::new();
        let mut templates = BTreeMap::new();
        let mut spawn_templates = BTreeMap::new();
        for row in rows {
            row.validate()?;
            let template = row
                .templates
                .one("effective creature capability template")?;
            let spawn_template = row
                .spawn_templates
                .one("effective creature capability spawn template")?;
            if templates
                .insert(template.entry, template.inhabit_type)
                .is_some_and(|previous| previous != template.inhabit_type)
                || spawn_templates
                    .insert(spawn_template.entry, spawn_template)
                    .is_some_and(|previous| previous != spawn_template)
                || spawns.insert(row.spawn_id, row.clone()).is_some()
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(Self {
            policy: CREATURE_CAPABILITY_POLICY,
            source_policy: SOURCE_POLICY,
            spawns,
        })
    }
    pub fn source(&self, spawn_id: u64) -> Option<&CreatureCapabilitySource> {
        self.spawns.get(&spawn_id)
    }
    /// Immutable predicate only; this does not construct live movement state or
    /// claim an initialization liquid result.
    pub fn source_capabilities(
        &self,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<source::MovementCapabilities, NativeError> {
        self.validate(profile)?;
        let habitat = self
            .source(npc.spawn)
            .ok_or(NativeError::AuthorityBinding)?
            .templates
            .one("effective creature capability template")?
            .inhabit_type;
        Ok(source::evaluate(
            source::InhabitType::try_from_raw(habitat).map_err(|_| NativeError::InvalidProfile)?,
            0,
        ))
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != CREATURE_CAPABILITY_POLICY
            || self.source_policy != SOURCE_POLICY
            || profile.source_motion.is_none()
        {
            return Err(NativeError::InvalidProfile);
        }
        if self.spawns.len() != profile.npcs.len() {
            return Err(NativeError::AuthorityUnavailable(
                "complete creature capability spawn bindings",
            ));
        }
        // Provider witnesses can validate without Profile::validate. Recheck the
        // retained generation catalog before relying on its addon source closure.
        if let Some(generations) = &profile.creature_generations {
            generations.validate(profile)?;
        }
        for npc in &profile.npcs {
            let row = self
                .source(npc.ai.creature_id)
                .ok_or(NativeError::AuthorityUnavailable(
                    "creature capability spawn binding",
                ))?;
            row.validate()?;
            let template = row
                .templates
                .one("effective creature capability template")?;
            if template.entry != npc.entry {
                return Err(NativeError::InvalidProfile);
            }
            row.initialization
                .validate(profile, row, template.inhabit_type)?;
            creature_generation_binding::validate_capability_source(
                profile
                    .creature_generations
                    .as_ref()
                    .and_then(|catalog| catalog.source(row.spawn_id)),
                row,
            )?;
        }
        Ok(())
    }
}
impl Profile {
    /// Select the bounded source host atomically. This does not open native admission.
    pub fn initialize_creature_capabilities(
        &mut self,
        rows: CreatureCapabilityRows<CreatureCapabilitySource>,
    ) -> Result<(), NativeError> {
        let catalog = CreatureCapabilityCatalog::validate_sources(rows)?;
        let mut next = self.clone();
        next.creature_capabilities = Some(catalog);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Read-only source witness; predicates are computed, never independently writable.
/// Deserialization is only transcript data and cannot construct the live state owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureCapabilityWitness {
    npc: crate::NpcId,
    source_binding: crate::Digest,
    inhabit_type: u32,
    capability_movement_flags: u32,
    swimming_movement_flags: u32,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    initialization: Option<Box<CreatureInitializationFacts>>,
}
impl CreatureCapabilityWitness {
    /// Request consistency against the selected immutable source catalog and the
    /// closed ground or positively-dry habitat-3 lifecycle invariant. This does not prove liveness or
    /// authenticate deserialized transcripts. Mutable capability domains must
    /// replace this check rather than silently reusing constructor defaults.
    pub fn validate_for(&self, profile: &Profile, npc: crate::NpcId) -> Result<(), NativeError> {
        let expected =
            CreatureCapabilityState::new_bound(profile, npc, self.initialization.clone())?.ok_or(
                NativeError::AuthorityUnavailable("creature capability source catalog"),
            )?;
        if self != expected.witness() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    /// Bind initialization evidence to the actual outer world/query clock. The
    /// profile-only check above cannot authorize copying a same-profile witness
    /// between worlds or before its generation was created.
    pub fn validate_context(
        &self,
        profile: &Profile,
        binding: &Binding,
    ) -> Result<(), NativeError> {
        let npc = binding.npc.ok_or(NativeError::AuthorityBinding)?;
        self.validate_for(profile, npc)?;
        if binding.world == [0; 16]
            || binding.profile != profile.digest()
            || binding.authority_policy != profile.authority_policy
            || binding.geometry != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        if self.initialization.as_ref().is_some_and(|facts| {
            facts.request.binding.world != binding.world
                || facts.request.binding.tick > binding.tick
                || facts.request.binding.npc != Some(npc)
        }) {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub fn npc(&self) -> crate::NpcId {
        self.npc
    }
    pub fn source_binding(&self) -> crate::Digest {
        self.source_binding
    }
    pub fn inhabit_type(&self) -> u32 {
        self.inhabit_type
    }
    pub fn swimming_movement_flags(&self) -> u32 {
        self.swimming_movement_flags
    }
    pub fn initialization(&self) -> Option<&CreatureInitializationFacts> {
        self.initialization.as_deref()
    }
    /// Only LEVITATING/HOVER/CAN_FLY are projected here, not complete MovementInfo.
    pub fn capability_movement_flags(&self) -> u32 {
        self.capability_movement_flags
    }
    pub fn capabilities(&self) -> Result<source::MovementCapabilities, NativeError> {
        if self.capability_movement_flags & !FLAG_MASK != 0 || self.swimming_movement_flags != 0 {
            return Err(NativeError::InvalidState);
        }
        let habitat = source::InhabitType::try_from_raw(self.inhabit_type)
            .map_err(|_| NativeError::InvalidState)?;
        Ok(source::evaluate(habitat, self.capability_movement_flags))
    }
    pub fn allowed_position_z_branch(&self) -> Result<AllowedPositionZBranch, NativeError> {
        let capabilities = self.capabilities()?;
        Ok(if capabilities.can_fly() {
            AllowedPositionZBranch::Flying
        } else if capabilities.can_swim() {
            AllowedPositionZBranch::Swimming
        } else {
            AllowedPositionZBranch::Ground
        })
    }
}
/// Unit::UpdateAllowedPositionZ's capability branch only. World configuration,
/// height, liquid, water-walk and LOS answers remain explicit provider dependencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllowedPositionZBranch {
    Ground,
    Swimming,
    Flying,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureNearPointRequest {
    pub request: benilla_sim::creature_motion::NearPointRequest,
    pub searcher: CreatureCapabilityWitness,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub target_player_height: Option<PlayerCollisionHeightTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureRouteRequest {
    /// Actual first/retained Chase dispatch, before the provider's straight gate.
    /// Null is explicit unsupported constructor ownership, including Home.
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub pathfinder_dispatch: Option<PathfinderDispatchReceipt>,
    /// Single-bit lifecycle receipt minted from the executing staged NPC. Explicit
    /// absence is not a constructor default or source proof.
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub owner_ignore_pathfinding: Option<OwnerIgnorePathfindingReceipt>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub target_player_water: Option<PlayerEnvironmentTarget>,
    pub request: benilla_sim::creature_motion::RouteRequest,
    pub owner: CreatureCapabilityWitness,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub target_player_height: Option<PlayerCollisionHeightTarget>,
    /// Frozen current-generation width; absent until a genuine model selection exists.
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub collision_width: Option<CreatureCollisionWidthWitness>,
    /// Frozen initial-model NPC height; explicit absence is not source ownership.
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub collision_height: Option<CreatureCollisionHeightWitness>,
    /// Current Unit map position used by PathFinder::updateFilter, distinct from
    /// Home's potentially newer computed spline launch position.
    pub owner_position: ai::Point,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureCapabilityState {
    witness: CreatureCapabilityWitness,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatureCapabilitySnapshot {
    witness: CreatureCapabilityWitness,
}
impl CreatureCapabilityState {
    pub fn witness(&self) -> &CreatureCapabilityWitness {
        &self.witness
    }
    pub(super) fn new(profile: &Profile, npc: crate::NpcId) -> Result<Option<Self>, NativeError> {
        Self::new_bound(profile, npc, None)
    }
    pub(super) fn new_generation(
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<Option<Self>, NativeError> {
        if owner.npc_id() != npc || owner.profile_identity() != profile.digest() {
            return Err(NativeError::AuthorityBinding);
        }
        owner.validate_initializations(profile)?;
        Self::new_bound(profile, npc, owner.initialization().cloned().map(Box::new))
    }
    fn new_bound(
        profile: &Profile,
        npc: crate::NpcId,
        initialization: Option<Box<CreatureInitializationFacts>>,
    ) -> Result<Option<Self>, NativeError> {
        let Some(catalog) = &profile.creature_capabilities else {
            return Ok(None);
        };
        catalog.validate(profile)?;
        let row = catalog.source(npc.spawn).ok_or(NativeError::InvalidState)?;
        let template = row
            .templates
            .one("effective creature capability template")?;
        match (&row.initialization, &initialization) {
            (CreatureInitializationClosure::Ground, None) => {}
            (CreatureInitializationClosure::DryWater { .. }, Some(facts)) => {
                facts.request.validate_for(profile)?;
                facts.validate_for(&facts.request)?;
                if facts.request.binding.npc != Some(npc)
                    || facts.request.capability_source != row.binding()
                {
                    return Err(NativeError::AuthorityBinding);
                }
            }
            _ => {
                return Err(NativeError::AuthorityUnavailable(
                    "generation-bound dry creature initialization",
                ))
            }
        }
        // MovementInfo starts at zero. Creature's constructor SetWalk(true) and
        // optional RUN_ON_SPAWN only touch WALK_MODE. Actual GROUND SetLevitate(false),
        // absent hover/gravity flags and complete no-control/no-script closure
        // leave all three capability bits zero. The generation-owned addon domain
        // only writes sheath; its retained move_flags are unused by the source.
        // No full flags are inferred.
        Ok(Some(Self {
            witness: CreatureCapabilityWitness {
                npc,
                source_binding: row.binding(),
                inhabit_type: template.inhabit_type,
                capability_movement_flags: 0,
                swimming_movement_flags: 0,
                initialization,
            },
        }))
    }
    pub(super) fn validate(&self, profile: &Profile, npc: crate::NpcId) -> Result<(), NativeError> {
        let expected = Self::new_bound(profile, npc, self.witness.initialization.clone())?
            .ok_or(NativeError::InvalidState)?;
        if *self != expected {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn snapshot(&self) -> CreatureCapabilitySnapshot {
        CreatureCapabilitySnapshot {
            witness: self.witness.clone(),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        snapshot: Option<CreatureCapabilitySnapshot>,
        profile: &Profile,
        npc: crate::NpcId,
        generation: Option<&CreatureGenerationOwner>,
    ) -> Result<Option<Self>, NativeError> {
        let state = snapshot.map(|snapshot| Self {
            witness: snapshot.witness,
        });
        if state.is_some() != profile.creature_capabilities.is_some() {
            return Err(NativeError::InvalidCheckpoint);
        }
        if let Some(state) = &state {
            state
                .validate(profile, npc)
                .map_err(|_| NativeError::InvalidCheckpoint)?;
            if let Some(owner) = generation {
                let expected = Self::new_generation(profile, npc, owner)?
                    .ok_or(NativeError::InvalidCheckpoint)?;
                if state != &expected {
                    return Err(NativeError::InvalidCheckpoint);
                }
            } else if state.witness.initialization.is_some() {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(state)
    }
    pub(super) fn respawn(
        &mut self,
        profile: &Profile,
        npc: crate::NpcId,
        generation: Option<&CreatureGenerationOwner>,
    ) -> Result<(), NativeError> {
        if npc.spawn != self.witness.npc.spawn
            || self.witness.npc.generation.checked_add(1) != Some(npc.generation)
        {
            return Err(NativeError::InvalidState);
        }
        self.validate(profile, self.witness.npc)?;
        // JUST_ALIVED SetWalk/ResetEntry/LoadCreatureAddon cannot change these
        // bits within the same complete no-substitution and addon source closure.
        let next = match generation {
            Some(owner) => Self::new_generation(profile, npc, owner)?,
            None => Self::new(profile, npc)?,
        }
        .ok_or(NativeError::InvalidState)?;
        // Source initialization only ORs SWIMMING. Every supported historical life
        // is positively dry, so this preserves zero; it must never clear a wet bit.
        if self.witness.swimming_movement_flags != next.witness.swimming_movement_flags {
            return Err(NativeError::UnsupportedMechanic(
                "swimming lifecycle continuity",
            ));
        }
        *self = next;
        Ok(())
    }
}
impl Npc {
    pub fn creature_capabilities(&self) -> Option<&CreatureCapabilityState> {
        self.source_capabilities.as_ref()
    }
}
