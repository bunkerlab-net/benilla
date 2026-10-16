// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Player.cpp
// InitDisplayIds, Object.h GetObjectScale and Unit.cpp GetCollisionHeight.
// Upstream GPL-2.0-or-later. Initial unshapeshifted player ownership only.
use super::*;
use benilla_formats::{CharCreateCatalog, CreatureCatalog};
use benilla_sim::{creature_motion as motion, player_model as source};
use serde::{Deserialize, Serialize};

pub const PLAYER_COLLISION_HEIGHT_POLICY: &str = "cmangos-8ec338a1-native-initial-player-height-v1";

/// Retained only by the successful source starter initializer, never a caller tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct PlayerStarterIdentity {
    pub(super) race: u8,
    pub(super) class: u8,
    pub(super) gender: u8,
}

/// Selected loaded ChrRaces/CDI/CMD operands, not a native-content admission token.
/// No public constructor accepts asserted identity, rows, model ID or final height.
/// ```compile_fail
/// use benilla_world_runtime::native::PlayerCollisionHeightSource;
/// let _: PlayerCollisionHeightSource = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerCollisionHeightSource {
    policy: &'static str,
    identity: PlayerStarterIdentity,
    raw_race_id: u32,
    inputs: source::InitialPlayerModelInputs,
    initial_height: source::InitialPlayerCollisionHeight,
}
impl PlayerCollisionHeightSource {
    fn from_catalogs(
        profile: &Profile,
        characters: &CharCreateCatalog,
        creatures: &CreatureCatalog,
    ) -> Result<Self, NativeError> {
        let identity = profile
            .combat
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "source-initialized player identity",
            ))?
            .starter_identity;
        let selected = characters
            .native_player_display(identity.race, identity.class, identity.gender)
            .ok_or(NativeError::AuthorityUnavailable(
                "strict loaded native player display selection",
            ))?;
        let row = creatures
            .collision_height_row(selected.display_id())
            .ok_or(NativeError::AuthorityUnavailable(
                "complete loaded player model height row",
            ))?;
        if selected.raw_race_id() != u32::from(identity.race)
            || selected.class() != identity.class
            || selected.gender() != identity.gender
            || row.display_id != selected.display_id()
        {
            return Err(NativeError::AuthorityBinding);
        }
        let inputs = source::InitialPlayerModelInputs {
            native_display_id: row.display_id,
            model_id: row.model_id,
            display_scale: spell::SpellFloat::from_bits(row.display_scale.to_bits())?,
            model_scale: spell::SpellFloat::from_bits(row.model_scale.to_bits())?,
            raw_height: spell::SpellFloat::from_bits(row.collision_height.to_bits())?,
        };
        let initial_height = Self::derive(inputs)?;
        Ok(Self {
            policy: PLAYER_COLLISION_HEIGHT_POLICY,
            identity,
            raw_race_id: selected.raw_race_id(),
            inputs,
            initial_height,
        })
    }
    fn derive(
        inputs: source::InitialPlayerModelInputs,
    ) -> Result<source::InitialPlayerCollisionHeight, NativeError> {
        // The motion body's domain is nonnegative; the arithmetic kernel separately
        // preserves the signed source semantics, including either exact zero.
        if [inputs.display_scale, inputs.model_scale, inputs.raw_height]
            .iter()
            .any(|f| f.bits() > 0x8000_0000)
        {
            return Err(NativeError::InvalidProfile);
        }
        source::initial_collision_height(inputs).map_err(|_| NativeError::InvalidProfile)
    }
    pub fn initial_height(&self) -> &source::InitialPlayerCollisionHeight {
        &self.initial_height
    }
    pub fn race(&self) -> u8 {
        self.identity.race
    }
    pub fn class(&self) -> u8 {
        self.identity.class
    }
    pub fn gender(&self) -> u8 {
        self.identity.gender
    }
    pub fn binding(&self) -> crate::Digest {
        crate::digest(b"benilla-initial-player-height-source-v1\0", self)
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        let combat = profile.combat.as_ref().ok_or(NativeError::InvalidProfile)?;
        if self.policy != PLAYER_COLLISION_HEIGHT_POLICY
            || self.identity != combat.starter_identity
            || self.raw_race_id != u32::from(self.identity.race)
            || !(1..=8).contains(&self.identity.race)
            || self.identity.class != 8
            || self.identity.gender > 1
            || self.initial_height != Self::derive(self.inputs)?
            || self.initial_height.policy() != source::PLAYER_MODEL_POLICY_ID
            || profile
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidProfile)?
                .player_collision_height
                .bits()
                != self.initial_height.height().bits()
        {
            return Err(NativeError::InvalidProfile);
        }
        // This exhaustive executable domain cannot change native display/object scale.
        // Unknown initial auras, transforms, shapeshift and scale effects fail closed.
        if combat
            .initial_auras
            .iter()
            .any(|a| !matches!(a.aura_type, 22 | 29 | 30 | 80 | 83 | 98 | 101 | 137 | 142))
        {
            return Err(NativeError::UnsupportedMechanic(
                "initial player model-changing aura",
            ));
        }
        for ability in &profile.abilities {
            if let AbilityAdmission::Available(spec) = &ability.admission {
                for effect in &spec.effects {
                    match effect {
                        spell::MageEffect::Empty
                        | spell::MageEffect::SchoolDamage(_)
                        | spell::MageEffect::PeriodicDamage { .. } => {}
                        spell::MageEffect::Unsupported { .. } => {
                            return Err(NativeError::UnsupportedMechanic(
                                "player model-changing executable effect",
                            ))
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
impl Profile {
    /// Explicit optional ownership from the actual initialized player's identity and
    /// loaded catalogs. Client race/class eligibility does not prove server row closure.
    pub fn initialize_player_collision_height(
        &mut self,
        characters: &CharCreateCatalog,
        creatures: &CreatureCatalog,
    ) -> Result<(), NativeError> {
        if self.player_collision_height.is_some() {
            self.validate()?;
        }
        let source = PlayerCollisionHeightSource::from_catalogs(self, characters, creatures)?;
        if self
            .player_collision_height
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.source_motion
            .as_mut()
            .ok_or(NativeError::InvalidProfile)?
            .player_collision_height = ai::Real::from_bits(source.initial_height.height().bits())?;
        next.player_collision_height = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Untrusted serialized transcript, rederived against the complete current owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerCollisionHeightWitness {
    world: crate::WorldId,
    player: crate::PlayerId,
    content: crate::Digest,
    profile: crate::Digest,
    geometry: crate::Digest,
    map: u32,
    source_binding: crate::Digest,
    native_display_id: u32,
    model_id: u32,
    stored_object_scale: spell::SpellFloat,
    effective_object_scale: spell::SpellFloat,
    raw_height: spell::SpellFloat,
    height: spell::SpellFloat,
}
impl PlayerCollisionHeightWitness {
    pub fn player(&self) -> crate::PlayerId {
        self.player
    }
    pub fn world(&self) -> crate::WorldId {
        self.world
    }
    pub fn source_binding(&self) -> crate::Digest {
        self.source_binding
    }
    pub fn native_display_id(&self) -> u32 {
        self.native_display_id
    }
    pub fn model_id(&self) -> u32 {
        self.model_id
    }
    pub fn stored_object_scale(&self) -> spell::SpellFloat {
        self.stored_object_scale
    }
    pub fn effective_object_scale(&self) -> spell::SpellFloat {
        self.effective_object_scale
    }
    pub fn raw_height(&self) -> spell::SpellFloat {
        self.raw_height
    }
    pub fn height(&self) -> spell::SpellFloat {
        self.height
    }
    pub fn validate_for(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        profile.validate()?;
        let expected = PlayerCollisionHeightState::new(profile, world, player)?.ok_or(
            NativeError::AuthorityUnavailable("current player model collision height"),
        )?;
        if self != expected.witness() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerCollisionHeightState {
    witness: PlayerCollisionHeightWitness,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerCollisionHeightSnapshot {
    witness: PlayerCollisionHeightWitness,
}
impl PlayerCollisionHeightState {
    pub fn witness(&self) -> &PlayerCollisionHeightWitness {
        &self.witness
    }
    pub(super) fn new(
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Option<Self>, NativeError> {
        let Some(source) = &profile.player_collision_height else {
            return Ok(None);
        };
        source.validate(profile)?;
        let height = source.initial_height();
        Ok(Some(Self {
            witness: PlayerCollisionHeightWitness {
                world,
                player,
                content: profile.content_digest,
                profile: profile.digest(),
                geometry: profile.geometry_revision,
                map: profile.map_id,
                source_binding: source.binding(),
                native_display_id: height.native_display_id(),
                model_id: height.model_id(),
                stored_object_scale: height.stored_object_scale(),
                effective_object_scale: height.effective_object_scale(),
                raw_height: height.raw_height(),
                height: height.height(),
            },
        }))
    }
    pub(super) fn validate_optional(
        state: Option<&Self>,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        if state != Self::new(profile, world, player)?.as_ref() {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn snapshot(&self) -> PlayerCollisionHeightSnapshot {
        PlayerCollisionHeightSnapshot {
            witness: self.witness.clone(),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        snapshot: Option<PlayerCollisionHeightSnapshot>,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Option<Self>, NativeError> {
        let state = snapshot.map(|s| Self { witness: s.witness });
        Self::validate_optional(state.as_ref(), profile, world, player)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(state)
    }
}
impl State {
    pub fn player_collision_height(&self) -> Option<&PlayerCollisionHeightState> {
        self.source_player_collision_height.as_ref()
    }
}

/// Frozen full target pose and movement state sampled from this frame's player.
/// Replay still verifies the actual historical frame; a hash is not authentication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerCollisionHeightTarget {
    witness: Box<PlayerCollisionHeightWitness>,
    target: motion::Target,
}
impl PlayerCollisionHeightTarget {
    pub(super) fn new(source: &PlayerCollisionHeightState, target: motion::Target) -> Self {
        Self {
            witness: Box::new(source.witness.clone()),
            target,
        }
    }
    pub fn witness(&self) -> &PlayerCollisionHeightWitness {
        &self.witness
    }
    pub fn target(&self) -> &motion::Target {
        &self.target
    }
    fn validate(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
        target: &motion::Target,
        binding: &motion::QueryBinding,
    ) -> Result<(), NativeError> {
        self.witness.validate_for(profile, world, player)?;
        if target != &self.target
            || target.id != SOLO_UNIT
            || target.collision_height.bits() != self.witness.height.bits()
            || target.combat_reach != profile.player.combat_reach
            || target.bounding_radius != profile.player.bounding_radius
            || binding.identity.map != profile.map_id
            || binding.identity.content.0 != profile.content_digest
            || binding.identity.geometry.0 != profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl CreatureNearPointRequest {
    pub fn require_target_collision_height(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<&PlayerCollisionHeightWitness, NativeError> {
        self.searcher.validate_for(profile, self.searcher.npc())?;
        let target =
            self.target_player_height
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "current player model collision height",
                ))?;
        target.validate(
            profile,
            world,
            player,
            &self.request.target,
            &self.request.binding,
        )?;
        if self.request.binding.identity.creature != self.searcher.npc().spawn {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(target.witness())
    }
}
impl CreatureRouteRequest {
    pub fn require_target_collision_height(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<&PlayerCollisionHeightWitness, NativeError> {
        self.owner.validate_for(profile, self.owner.npc())?;
        if self.request.kind == motion::RouteKind::HomeForceDestination {
            return Err(
                if self.request.target.is_none() && self.target_player_height.is_none() {
                    NativeError::AuthorityUnavailable("route has no player target")
                } else {
                    NativeError::AuthorityBinding
                },
            );
        }
        let raw = self
            .request
            .target
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "route has no player target",
            ))?;
        let target =
            self.target_player_height
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "current player model collision height",
                ))?;
        target.validate(profile, world, player, raw, &self.request.binding)?;
        if self.request.binding.identity.creature != self.owner.npc().spawn {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(target.witness())
    }
}
