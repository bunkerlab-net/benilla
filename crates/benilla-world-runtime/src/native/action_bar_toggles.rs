// SPDX-License-Identifier: GPL-3.0-or-later
//! Source PLAYER_FIELD_BYTES byte2, independent of action-button contents.
//! CMaNGOS Classic 8ec338a1 MiscHandler/Object/Player.cpp; no admission capability.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_WRITES: usize = 2048;
/// Reuse actual immutable source owners, never caller-supplied absence booleans.
/// Fresh mage initialization owns zero combo/honor bytes; the kill catalog
/// rejects honor-producing victims, and all supported aura/effect producers
/// exclude tracking/combo/honor mutations. DeathSupport alone does not prove this.
pub(super) fn validate_profile(profile: &Profile) -> Result<(), NativeError> {
    let combat = profile
        .combat
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "action-bar packed-field fresh source initializer",
        ))?;
    if combat.starter_identity.class != 8
        || combat.initial_auras.iter().any(|a| a.aura_type == 151)
        || profile.quest_progression.is_none()
    {
        return Err(NativeError::UnsupportedMechanic(
            "action-bar packed-field combo/honor/tracking owner closure",
        ));
    }
    // Profile validation already verifies the complete kill catalog against all
    // NPC entries and source initialization. Neither source owner is mutable.
    Ok(())
}
impl Profile {
    /// Eligibility for the composed source player byte word, not gameplay admission.
    pub fn supports_action_bar_toggles(&self) -> bool {
        validate_profile(self).is_ok()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    sequence: u64,
    value: u8,
}
// Object::_InitValues zeroes the byte; no starter button layout is invented.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionBarTogglesState {
    value: u8,
    #[serde(deserialize_with = "deserialize_writes")]
    writes: Vec<Write>,
}
fn deserialize_writes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Write>, D::Error> {
    struct Writes;
    impl<'de> serde::de::Visitor<'de> for Writes {
        type Value = Vec<Write>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_WRITES} action-bar toggle writes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_WRITES) {
                return Err(serde::de::Error::custom("action-bar toggle write limit"));
            }
            let mut writes = vec![];
            while let Some(write) = seq.next_element()? {
                if writes.len() == MAX_WRITES {
                    return Err(serde::de::Error::custom("action-bar toggle write limit"));
                }
                writes.push(write);
            }
            Ok(writes)
        }
    }
    d.deserialize_seq(Writes)
}
impl ActionBarTogglesState {
    pub(super) fn value(&self) -> u8 {
        self.value
    }
    pub(super) fn write(&mut self, value: u8, sequence: u64) -> Result<(), NativeError> {
        // Object::SetByteValue returns before dirtying an unchanged byte.
        if value == self.value {
            return Ok(());
        }
        if self.writes.len() >= MAX_WRITES {
            return Err(NativeError::Limit);
        }
        if self
            .writes
            .last()
            .is_some_and(|last| last.sequence >= sequence)
        {
            return Err(NativeError::InvalidState);
        }
        self.writes.push(Write { sequence, value });
        self.value = value;
        Ok(())
    }
    pub(super) fn validate(
        &self,
        profile: &Profile,
        next_sequence: u64,
    ) -> Result<(), NativeError> {
        if self.value != 0 || !self.writes.is_empty() {
            validate_profile(profile).map_err(|_| NativeError::InvalidCheckpoint)?;
        }
        if self.writes.len() > MAX_WRITES {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut previous_sequence = None;
        let mut value = 0;
        for write in &self.writes {
            if write.sequence >= next_sequence
                || previous_sequence.is_some_and(|previous| previous >= write.sequence)
                || write.value == value
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            value = write.value;
            previous_sequence = Some(write.sequence);
        }
        if self.value != value {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
