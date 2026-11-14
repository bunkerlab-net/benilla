// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 CharacterHandler.cpp HandleShowingHelm/CloakOpcode.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Bounded current PLAYER_FLAGS ownership; not general map/PvP/ghost support.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_TOGGLES: usize = 2048;
const HELM: u32 = 0x400;
const CLOAK: u32 = 0x800;

/// Existing validated owners supply the closure, not new caller absence booleans.
/// Interaction initialization owns initial flags zero and excludes PvP/control
/// producers. The quest kill owner excludes group/rest-entry/honor dependencies
/// and retains fresh rest state. Full raw spell authority and its exhaustive
/// supported effects exclude ghost/other flag auras. Native commands do not admit
/// AFK/DND/GM/group/taxi/PvP transitions. This is NOT real-map/capital coverage.
pub(super) fn validate_profile(profile: &Profile) -> Result<(), NativeError> {
    if profile.player_appearance.is_none()
        || profile.combat.is_none()
        || profile.interactions.is_none()
        || !profile.has_source_spell_authority()
        || profile.quest_progression.is_none()
    {
        return Err(NativeError::UnsupportedMechanic(
            "player hide flags require fresh appearance, raw interaction and no-rest-entry owners",
        ));
    }
    // Every admitted NativeWorld has already validated these immutable owners,
    // including the complete kill catalog and interaction initial flags == 0.
    Ok(())
}
impl Profile {
    /// Complete PLAYER_FLAGS projection eligibility in this bounded source host.
    pub fn supports_player_hide_flags(&self) -> bool {
        validate_profile(self).is_ok()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Toggle {
    sequence: u64,
    helm: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerHideFlags {
    flags: u32,
    #[serde(deserialize_with = "deserialize_toggles")]
    toggles: Vec<Toggle>,
}
fn deserialize_toggles<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Toggle>, D::Error> {
    struct Toggles;
    impl<'de> serde::de::Visitor<'de> for Toggles {
        type Value = Vec<Toggle>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_TOGGLES} source hide toggles")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<Toggle>, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_TOGGLES) {
                return Err(serde::de::Error::custom("hide toggle limit"));
            }
            let mut toggles = Vec::new();
            while let Some(toggle) = seq.next_element()? {
                if toggles.len() == MAX_TOGGLES {
                    return Err(serde::de::Error::custom("hide toggle limit"));
                }
                toggles.push(toggle);
            }
            Ok(toggles)
        }
    }
    d.deserialize_seq(Toggles)
}
impl PlayerHideFlags {
    pub(super) fn value(&self) -> u32 {
        self.flags
    }
    pub(super) fn toggle(&mut self, helm: bool, sequence: u64) -> Result<(), NativeError> {
        if self.toggles.len() >= MAX_TOGGLES {
            return Err(NativeError::Limit);
        }
        if self
            .toggles
            .last()
            .is_some_and(|last| last.sequence >= sequence)
        {
            return Err(NativeError::InvalidState);
        }
        self.toggles.push(Toggle { sequence, helm });
        self.flags ^= if helm { HELM } else { CLOAK };
        Ok(())
    }
    pub(super) fn validate(
        &self,
        profile: &Profile,
        next_sequence: u64,
    ) -> Result<(), NativeError> {
        if self.flags != 0 || !self.toggles.is_empty() {
            validate_profile(profile).map_err(|_| NativeError::InvalidCheckpoint)?;
        }
        if self.toggles.len() > MAX_TOGGLES {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut flags = 0;
        let mut previous = None;
        for toggle in &self.toggles {
            if toggle.sequence >= next_sequence || previous.is_some_and(|n| n >= toggle.sequence) {
                return Err(NativeError::InvalidCheckpoint);
            }
            flags ^= if toggle.helm { HELM } else { CLOAK };
            previous = Some(toggle.sequence);
        }
        if self.flags != flags {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
