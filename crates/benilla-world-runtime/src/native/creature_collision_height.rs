// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.cpp
// InitEntry/Respawn and Unit.cpp GetCollisionHeight. Upstream GPL-2.0-or-later.
//! Per-life NPC height ownership; player height and production admission remain separate.
use super::*;
use benilla_sim::creature_init::{self as source, InitialCollisionHeight};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const CREATURE_COLLISION_HEIGHT_POLICY: &str =
    "cmangos-8ec338a1-native-generation-model-height-v2";

/// Derived only from the same retained initializer/DBC source that owns width.
/// No public constructor or Deserialize can substitute a second model projection.
/// ```compile_fail
/// use benilla_world_runtime::native::CreatureCollisionHeightSource;
/// let _: CreatureCollisionHeightSource = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCollisionHeightSource {
    spawn_id: u64,
    width_source_binding: crate::Digest,
    initial_height: InitialCollisionHeight,
}
impl CreatureCollisionHeightSource {
    fn from_width(source: &CreatureCollisionWidthSource) -> Result<Self, NativeError> {
        Ok(Self {
            spawn_id: source.spawn_id(),
            width_source_binding: source.binding(),
            initial_height: source.initial_height()?,
        })
    }
    pub fn spawn_id(&self) -> u64 {
        self.spawn_id
    }
    pub fn width_source_binding(&self) -> crate::Digest {
        self.width_source_binding
    }
    pub fn initial_height(&self) -> &InitialCollisionHeight {
        &self.initial_height
    }
    fn binding(&self) -> crate::Digest {
        crate::digest(
            b"benilla-generation-model-height-source-v2\0",
            &(CREATURE_COLLISION_HEIGHT_POLICY, self),
        )
    }
}

/// Optional private source entries are retained by the complete profile digest.
/// ```compile_fail
/// use benilla_world_runtime::native::CreatureCollisionHeightCatalog;
/// let _: CreatureCollisionHeightCatalog = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureCollisionHeightCatalog {
    policy: &'static str,
    spawns: BTreeMap<u64, CreatureCollisionHeightSource>,
}
impl CreatureCollisionHeightCatalog {
    fn from_profile(profile: &Profile) -> Result<Self, NativeError> {
        let widths =
            profile
                .creature_collision_widths
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "retained initial creature model width sources",
                ))?;
        widths.validate(profile)?;
        let mut spawns = BTreeMap::new();
        for npc in &profile.npcs {
            let width = widths
                .source(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            spawns.insert(
                npc.ai.creature_id,
                CreatureCollisionHeightSource::from_width(width)?,
            );
        }
        Ok(Self {
            policy: CREATURE_COLLISION_HEIGHT_POLICY,
            spawns,
        })
    }
    pub fn source(&self, spawn_id: u64) -> Option<&CreatureCollisionHeightSource> {
        self.spawns.get(&spawn_id)
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self != &Self::from_profile(profile)? {
            return Err(NativeError::InvalidProfile);
        }
        // Each owner's height must agree with its retained model source.
        for npc in &profile.npcs {
            let motion = profile.npc_motion(npc.ai.creature_id)?;
            let row = self
                .source(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            if row.initial_height.policy() != source::CREATURE_COLLISION_HEIGHT_POLICY_ID {
                return Err(NativeError::InvalidProfile);
            }
            if profile.creature_generations.is_some() {
                let width = profile
                    .creature_collision_widths
                    .as_ref()
                    .and_then(|catalog| catalog.source(npc.ai.creature_id))
                    .ok_or(NativeError::InvalidProfile)?;
                for selection in width.generation_selections(profile)? {
                    let height = width.selected_height(&selection)?;
                    // Source arithmetic permits negatives; GroundSettings does not.
                    if f32::from_bits(height.height().bits()) < 0.0 {
                        return Err(NativeError::InvalidProfile);
                    }
                }
            } else if motion.npc_policy.settings.collision_height.bits()
                != row.initial_height.height().bits()
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
}
impl Profile {
    /// Explicitly opt in to height using the already-owned initial model source.
    /// The retained catalog and each owner's motion height update atomically.
    pub fn initialize_creature_collision_heights(&mut self) -> Result<(), NativeError> {
        // Existing ownership is immutable: repeat initialization may not repair
        // a changed source binding or an independently edited motion scalar.
        if self.creature_collision_heights.is_some() {
            self.validate()?;
        }
        let catalog = CreatureCollisionHeightCatalog::from_profile(self)?;
        let mut next = self.clone();
        let motion = next
            .source_motion
            .as_mut()
            .ok_or(NativeError::InvalidProfile)?;
        for npc in &next.npcs {
            let row = catalog
                .source(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            motion
                .npcs
                .get_mut(&npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?
                .npc_policy
                .settings
                .collision_height = ai::Real::from_bits(row.initial_height.height().bits())?;
        }
        next.creature_collision_heights = Some(catalog);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Untrusted transcript data; validation binds a complete profile, shared width
/// source and generation, not current liveness or authenticated database origin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureCollisionHeightWitness {
    npc: crate::NpcId,
    content: crate::Digest,
    profile: crate::Digest,
    source_binding: crate::Digest,
    width_source_binding: crate::Digest,
    native_display_id: u32,
    model_id: u32,
    object_scale: spell::SpellFloat,
    raw_height: spell::SpellFloat,
    height: spell::SpellFloat,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    generation_selection: Option<creature_collision_width::GenerationModelSelection>,
}
impl CreatureCollisionHeightWitness {
    pub fn npc(&self) -> crate::NpcId {
        self.npc
    }
    pub fn source_binding(&self) -> crate::Digest {
        self.source_binding
    }
    pub fn width_source_binding(&self) -> crate::Digest {
        self.width_source_binding
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn model_id(&self) -> u32 {
        self.model_id
    }
    pub fn object_scale(&self) -> spell::SpellFloat {
        self.object_scale
    }
    pub fn raw_height(&self) -> spell::SpellFloat {
        self.raw_height
    }
    pub fn height(&self) -> spell::SpellFloat {
        self.height
    }
    /// The height and width must name the identical generation draw evidence,
    /// even when distinct draws select models with equal collision dimensions.
    pub(in crate::native) fn validate_width_selection(
        &self,
        width: &CreatureCollisionWidthWitness,
    ) -> Result<(), NativeError> {
        if self.npc != width.npc()
            || self.width_source_binding != width.source_binding()
            || self.generation_selection.as_ref() != width.generation_selection()
            || self.native_display_id != width.native_display_id()
            || self.model_id != width.model_id()
            || self.object_scale != width.object_scale()
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub fn validate_for(&self, profile: &Profile, npc: crate::NpcId) -> Result<(), NativeError> {
        profile.validate()?;
        self.validate_for_profile(profile, npc)
    }
    fn validate_for_profile(
        &self,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<(), NativeError> {
        let expected = match &self.generation_selection {
            Some(selection) => CreatureCollisionHeightState::from_generation_selection(
                profile,
                npc,
                selection.clone(),
            )?,
            None => CreatureCollisionHeightState::new(profile, npc)?,
        }
        .ok_or(NativeError::AuthorityUnavailable(
            "current creature model height selection receipt",
        ))?;
        if self != expected.witness() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureCollisionHeightState {
    witness: CreatureCollisionHeightWitness,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatureCollisionHeightSnapshot {
    witness: CreatureCollisionHeightWitness,
}
impl CreatureCollisionHeightState {
    pub fn witness(&self) -> &CreatureCollisionHeightWitness {
        &self.witness
    }
    pub(super) fn new(profile: &Profile, npc: crate::NpcId) -> Result<Option<Self>, NativeError> {
        if profile.creature_generations.is_some() {
            return Err(NativeError::AuthorityUnavailable(
                "current creature generation height owner",
            ));
        }
        // Respawn reruns InitEntry; an initial selection is never a later-life receipt.
        if npc.generation != 0 {
            return Ok(None);
        }
        let Some(catalog) = &profile.creature_collision_heights else {
            return Ok(None);
        };
        catalog.validate(profile)?;
        let row = catalog.source(npc.spawn).ok_or(NativeError::InvalidState)?;
        let value = row.initial_height;
        Ok(Some(Self {
            witness: CreatureCollisionHeightWitness {
                npc,
                content: profile.content_digest,
                profile: profile.digest(),
                source_binding: row.binding(),
                width_source_binding: row.width_source_binding,
                native_display_id: value.native_display_id(),
                model_id: value.model_id(),
                object_scale: value.object_scale(),
                raw_height: value.raw_height(),
                height: value.height(),
                generation_selection: None,
            },
        }))
    }
    pub(super) fn new_generation(
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<Option<Self>, NativeError> {
        let width = CreatureCollisionWidthState::new_generation(profile, npc, owner)?;
        let Some(_) = &profile.creature_collision_heights else {
            return Ok(None);
        };
        let evidence = width
            .as_ref()
            .and_then(|width| width.witness().generation_selection())
            .ok_or(NativeError::AuthorityBinding)?
            .clone();
        Self::from_generation_selection(profile, npc, evidence)
    }
    fn from_generation_selection(
        profile: &Profile,
        npc: crate::NpcId,
        evidence: creature_collision_width::GenerationModelSelection,
    ) -> Result<Option<Self>, NativeError> {
        let selection = evidence.selection(profile, npc)?;
        let Some(catalog) = &profile.creature_collision_heights else {
            return Ok(None);
        };
        catalog.validate(profile)?;
        let row = catalog.source(npc.spawn).ok_or(NativeError::InvalidState)?;
        let width_row = profile
            .creature_collision_widths
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::InvalidState)?;
        let value = width_row.selected_height(&selection)?;
        let width_binding = crate::digest(
            b"benilla-generation-model-width-owner-v1\0",
            &(width_row.binding(), &evidence),
        );
        Ok(Some(Self {
            witness: CreatureCollisionHeightWitness {
                npc,
                content: profile.content_digest,
                profile: profile.digest(),
                source_binding: crate::digest(
                    b"benilla-generation-model-height-owner-v1\0",
                    &(row.binding(), &evidence),
                ),
                width_source_binding: width_binding,
                native_display_id: value.native_display_id(),
                model_id: value.model_id(),
                object_scale: value.object_scale(),
                raw_height: value.raw_height(),
                height: value.height(),
                generation_selection: Some(evidence),
            },
        }))
    }
    pub(super) fn validate_generation_optional(
        state: Option<&Self>,
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<(), NativeError> {
        if state != Self::new_generation(profile, npc, owner)?.as_ref() {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn validate_optional(
        state: Option<&Self>,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<(), NativeError> {
        if state != Self::new(profile, npc)?.as_ref() {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn snapshot(&self) -> CreatureCollisionHeightSnapshot {
        CreatureCollisionHeightSnapshot {
            witness: self.witness.clone(),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        snapshot: Option<CreatureCollisionHeightSnapshot>,
        profile: &Profile,
        npc: crate::NpcId,
    ) -> Result<Option<Self>, NativeError> {
        let state = snapshot.map(|s| Self { witness: s.witness });
        Self::validate_optional(state.as_ref(), profile, npc)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(state)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore_generation(
        snapshot: Option<CreatureCollisionHeightSnapshot>,
        profile: &Profile,
        npc: crate::NpcId,
        owner: &CreatureGenerationOwner,
    ) -> Result<Option<Self>, NativeError> {
        let state = snapshot.map(|snapshot| Self {
            witness: snapshot.witness,
        });
        Self::validate_generation_optional(state.as_ref(), profile, npc, owner)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(state)
    }
}
impl Npc {
    pub fn creature_collision_height(&self) -> Option<&CreatureCollisionHeightState> {
        self.source_collision_height.as_ref()
    }
}
impl CreatureRouteRequest {
    /// Height-independent dependencies may accept explicit None. Consuming height
    /// requires the runtime-owned height and its accompanying shared width source.
    pub fn require_collision_height(
        &self,
        profile: &Profile,
    ) -> Result<&CreatureCollisionHeightWitness, NativeError> {
        profile.validate()?;
        let width = self.require_collision_width(profile)?;
        let height = self
            .collision_height
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "current creature model collision height",
            ))?;
        height.validate_for(profile, self.owner.npc())?;
        self.validate_height_width_binding(profile, width, height)?;
        Ok(height)
    }
    fn validate_height_width_binding(
        &self,
        profile: &Profile,
        width: &CreatureCollisionWidthWitness,
        height: &CreatureCollisionHeightWitness,
    ) -> Result<(), NativeError> {
        if height.width_source_binding != width.source_binding()
            || height.generation_selection.as_ref() != width.generation_selection()
            || height.native_display_id != width.native_display_id()
            || height.model_id != width.model_id()
            || height.object_scale != width.object_scale()
            || height.height.bits() != self.request.owner_collision_height.bits()
            || (height.generation_selection.is_none()
                && profile
                    .npc_motion(self.owner.npc().spawn)
                    .map_err(|_| NativeError::AuthorityBinding)?
                    .npc_policy
                    .settings
                    .collision_height
                    .bits()
                    != height.height.bits())
            || self.owner_position != self.request.origin
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

/// A completely validated, immutable profile for bounded model-witness checks.
/// No deserialized receipt or caller-supplied digest can construct this proof.
#[derive(Clone, Debug, Eq)]
pub struct ValidatedRouteModelProfile {
    profile: Arc<Profile>,
}
impl PartialEq for ValidatedRouteModelProfile {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.profile, &other.profile) || self.profile == other.profile
    }
}
impl ValidatedRouteModelProfile {
    pub fn new(profile: Arc<Profile>) -> Result<Self, NativeError> {
        profile.validate()?;
        Ok(Self { profile })
    }
    pub fn profile(&self) -> &Profile {
        &self.profile
    }
    pub fn matches_profile(&self, profile: &Profile) -> bool {
        std::ptr::eq(self.profile.as_ref(), profile) || self.profile.as_ref() == profile
    }
    /// Transcript consistency only; the caller still binds the original live job.
    pub fn validate_owner_context(
        &self,
        route: Arc<CreatureRouteRequest>,
        binding: &Binding,
    ) -> Result<ValidatedRouteModelOwner, NativeError> {
        route.owner.validate_context(&self.profile, binding)?;
        Ok(ValidatedRouteModelOwner {
            profile: self.clone(),
            route,
        })
    }
}

/// Retained owner validation for the exact immutable profile and request allocation.
#[derive(Clone)]
pub struct ValidatedRouteModelOwner {
    profile: ValidatedRouteModelProfile,
    route: Arc<CreatureRouteRequest>,
}
impl ValidatedRouteModelOwner {
    pub(super) fn validated_profile(&self) -> &ValidatedRouteModelProfile {
        &self.profile
    }
    pub fn profile(&self) -> &Profile {
        self.profile.profile()
    }
    pub fn route(&self) -> &CreatureRouteRequest {
        &self.route
    }
    pub fn matches_route(&self, profile: &Profile, route: &CreatureRouteRequest) -> bool {
        self.profile.matches_profile(profile)
            && self.route.as_ref() == route
            && self
                .route
                .pathfinder_dispatch
                .as_ref()
                .zip(route.pathfinder_dispatch.as_ref())
                .is_some_and(|(saved, current)| saved.same_live_dispatch(current))
    }
    pub fn validate_width(&self) -> Result<ValidatedRouteModelWidth, NativeError> {
        let width = self
            .route
            .collision_width_after_owner(self.profile())?
            .clone();
        Ok(ValidatedRouteModelWidth {
            owner: self.clone(),
            width,
        })
    }
}

/// Width and owner checks were completed on these same frozen objects.
#[derive(Clone)]
pub struct ValidatedRouteModelWidth {
    owner: ValidatedRouteModelOwner,
    width: CreatureCollisionWidthWitness,
}
impl ValidatedRouteModelWidth {
    pub fn width(&self) -> &CreatureCollisionWidthWitness {
        &self.width
    }
    pub fn require_height(&self) -> Result<&CreatureCollisionHeightWitness, NativeError> {
        let profile = self.owner.profile();
        let route = self.owner.route();
        let height = route
            .collision_height
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "current creature model collision height",
            ))?;
        height.validate_for_profile(profile, route.owner.npc())?;
        route.validate_height_width_binding(profile, &self.width, height)?;
        Ok(height)
    }
}
