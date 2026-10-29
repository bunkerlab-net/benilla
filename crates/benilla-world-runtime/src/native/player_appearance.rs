// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh selected-character cosmetic ownership, not corpse or native admission.
//! CMaNGOS Classic 8ec338a1 Player::ValidateAppearance and Player::Create.
use super::*;
use benilla_formats::CharCreateCatalog;
pub use benilla_sim::player_appearance::NativePlayerAppearanceChoices;
use benilla_sim::player_appearance::NativePlayerAppearanceSelection;
use serde::{Deserialize, Serialize};

const POLICY: &str = "cmangos-8ec338a1-native-fresh-player-appearance-v1";

/// Untrusted selected-character request data; not source evidence or a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerAppearanceRequest {
    pub player: crate::PlayerId,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub choices: NativePlayerAppearanceChoices,
}

/// Constructed only from the initialized identity and exact loaded appearance rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerAppearanceSource {
    policy: &'static str,
    request: PlayerAppearanceRequest,
    selection: NativePlayerAppearanceSelection,
}
impl PlayerAppearanceSource {
    pub fn request(&self) -> PlayerAppearanceRequest {
        self.request
    }
    pub fn binding(&self) -> crate::Digest {
        crate::digest(b"benilla-player-appearance-source-v1\0", self)
    }
    /// App selection must match the exact saved character, not merely its GUID.
    pub fn validate_request(&self, request: PlayerAppearanceRequest) -> Result<(), NativeError> {
        if self.request != request {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        let identity = profile
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .starter_identity;
        if self.policy != POLICY
            || self.request.player.owner == [0; 16]
            || self.request.player.character == [0; 16]
            || (self.request.race, self.request.class, self.request.gender)
                != (identity.race, identity.class, identity.gender)
            || !self.selection.validate()
            || self.selection.race() != self.request.race
            || self.selection.gender() != self.request.gender
            || self.selection.choices() != self.request.choices
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
}
impl Profile {
    pub fn initialize_player_appearance(
        &mut self,
        request: PlayerAppearanceRequest,
        catalog: &CharCreateCatalog,
    ) -> Result<(), NativeError> {
        let selection = NativePlayerAppearanceSelection::select(
            catalog,
            request.race,
            request.gender,
            request.choices,
        )
        .ok_or(NativeError::AuthorityUnavailable(
            "exact loaded player appearance rows",
        ))?;
        let source = PlayerAppearanceSource {
            policy: POLICY,
            request,
            selection,
        };
        source.validate(self)?;
        if self
            .player_appearance
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.player_appearance = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Immutable initial choices. Facial hair is retained without inventing the other
/// bytes of PLAYER_BYTES_2 (bank slots/rest state), guild or hide flags.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerAppearanceState {
    world: crate::WorldId,
    player: crate::PlayerId,
    profile: crate::Digest,
    source: crate::Digest,
    choices: NativePlayerAppearanceChoices,
}
impl PlayerAppearanceState {
    pub fn choices(&self) -> NativePlayerAppearanceChoices {
        self.choices
    }
    pub fn player_bytes(&self) -> u32 {
        u32::from_le_bytes([
            self.choices.skin,
            self.choices.face,
            self.choices.hair_style,
            self.choices.hair_color,
        ])
    }
    pub(super) fn new(
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Option<Self>, NativeError> {
        let Some(source) = &profile.player_appearance else {
            return Ok(None);
        };
        source.validate(profile)?;
        if source.request.player != player {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(Some(Self {
            world,
            player,
            profile: profile.digest(),
            source: source.binding(),
            choices: source.request.choices,
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
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        state: Option<Self>,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Option<Self>, NativeError> {
        Self::validate_optional(state.as_ref(), profile, world, player)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(state)
    }
}
impl State {
    pub fn player_appearance(&self) -> Option<&PlayerAppearanceState> {
        self.source_player_appearance.as_ref()
    }
}
