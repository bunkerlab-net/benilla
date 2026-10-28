// SPDX-License-Identifier: GPL-3.0-or-later
//! Source player descriptor, not ReputationMgr state or a faction lookup.
//! CMaNGOS Classic 8ec338a1 CharacterHandler/Player.cpp; no admission capability.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_WRITES: usize = 2048;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    sequence: u64,
    value: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WatchedFactionState {
    value: i32,
    #[serde(deserialize_with = "deserialize_writes")]
    writes: Vec<Write>,
}
impl Default for WatchedFactionState {
    fn default() -> Self {
        // Player::Create, not a guessed first known reputation-list index.
        Self {
            value: -1,
            writes: vec![],
        }
    }
}
fn deserialize_writes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Write>, D::Error> {
    struct Writes;
    impl<'de> serde::de::Visitor<'de> for Writes {
        type Value = Vec<Write>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_WRITES} watched-faction writes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_WRITES) {
                return Err(serde::de::Error::custom("watched-faction write limit"));
            }
            let mut writes = vec![];
            while let Some(write) = seq.next_element()? {
                if writes.len() == MAX_WRITES {
                    return Err(serde::de::Error::custom("watched-faction write limit"));
                }
                writes.push(write);
            }
            Ok(writes)
        }
    }
    d.deserialize_seq(Writes)
}
impl WatchedFactionState {
    pub(super) fn value(&self) -> i32 {
        self.value
    }
    pub(super) fn write(&mut self, value: i32, sequence: u64) -> Result<(), NativeError> {
        // Object::SetInt32Value returns before dirtying an unchanged field.
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
    pub(super) fn validate(&self, next_sequence: u64) -> Result<(), NativeError> {
        if self.writes.len() > MAX_WRITES {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut previous_sequence = None;
        let mut value = -1;
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
