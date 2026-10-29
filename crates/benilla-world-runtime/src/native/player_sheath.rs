// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Project contributors; upstream GPL-2.0-or-later, Rust GPL-3.0-or-later.
//! Source SetSheathed and Player::SetSheath; no virtual-item fabrication.
//! CMaNGOS Classic 8ec338a1 CombatHandler/Player/Unit; no admission capability.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_WRITES: usize = 2048;
/// Source Player::Create owns bytes2 misc flags 0x28 and zero other siblings.
/// Fresh initialization creates unenchanted items; the admitted initializer,
/// spell, inventory and membership owners have no enchant/charm/pet/import path.
/// This is an existing closed-host contract, not a caller assertion of absence.
pub(super) fn validate_profile(profile: &Profile) -> Result<(), NativeError> {
    let combat = profile
        .combat
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "player sheath fresh initializer",
        ))?;
    let loot = profile
        .quest_progression
        .as_ref()
        .and_then(|q| q.loot.as_ref())
        .ok_or(NativeError::UnsupportedMechanic(
            "player sheath inventory owner",
        ))?;
    if combat.starter_identity.class != 8 || !profile.has_source_spell_authority() {
        return Err(NativeError::UnsupportedMechanic(
            "player sheath no-enchant and control owner closure",
        ));
    }
    profile
        .interactions
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "player sheath interaction owner",
        ))?
        .validate(profile)?;
    for item in &combat.equipment.items {
        // Real source prototype and creation/lifecycle audit, not display metadata.
        loot.inventory.grant_ready(item.item_id)?;
    }
    Ok(())
}
impl Profile {
    /// Eligibility for the fresh composed UNIT_FIELD_BYTES_2; not admission.
    pub fn supports_player_sheath(&self) -> bool {
        validate_profile(self).is_ok()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    sequence: u64,
    value: u8,
}
// Object::_InitValues owns initial sheath state zero; no weapon is invented.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerSheathState {
    value: u8,
    #[serde(deserialize_with = "deserialize_writes")]
    writes: Vec<Write>,
}
fn deserialize_writes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Write>, D::Error> {
    struct Writes;
    impl<'de> serde::de::Visitor<'de> for Writes {
        type Value = Vec<Write>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_WRITES} player sheath writes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_WRITES) {
                return Err(serde::de::Error::custom("player sheath write limit"));
            }
            let mut writes = vec![];
            while let Some(write) = seq.next_element()? {
                if writes.len() == MAX_WRITES {
                    return Err(serde::de::Error::custom("player sheath write limit"));
                }
                writes.push(write);
            }
            Ok(writes)
        }
    }
    d.deserialize_seq(Writes)
}
impl PlayerSheathState {
    pub(super) fn value(&self) -> u8 {
        self.value
    }
    pub(super) fn write(&mut self, value: u8, sequence: u64) -> Result<(), NativeError> {
        if value >= 3 {
            return Err(NativeError::InvalidInput);
        }
        // Player::SetSheath still evaluates virtual-item callbacks on repeated
        // values. They are inert under the qualified zero-enchantment owner;
        // Unit::SetSheath delegates to SetByteValue, which does not dirty it.
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
        if self.value >= 3 || self.writes.len() > MAX_WRITES {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut previous_sequence = None;
        let mut value = 0;
        for write in &self.writes {
            if write.value >= 3
                || write.sequence >= next_sequence
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn write_budget_and_decode_bound_preserve_same_state_noop() {
        let mut state = PlayerSheathState::default();
        for sequence in 0..MAX_WRITES as u64 {
            state.write(((sequence + 1) % 2) as u8, sequence).unwrap();
        }
        state
            .validate(
                &synthetic_equipment_fixture::weapon_profile(),
                MAX_WRITES as u64,
            )
            .unwrap();
        let before = state.clone();
        state.write(0, MAX_WRITES as u64).unwrap();
        assert_eq!(state, before);
        assert!(matches!(
            state.write(1, MAX_WRITES as u64),
            Err(NativeError::Limit)
        ));
        assert_eq!(state, before);
        assert!(state.write(3, MAX_WRITES as u64).is_err());
        assert_eq!(state, before);
        let mut encoded = serde_json::to_value(state).unwrap();
        encoded["writes"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"sequence":MAX_WRITES,"value":1}));
        assert!(serde_json::from_value::<PlayerSheathState>(encoded).is_err());
    }
}
