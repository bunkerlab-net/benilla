// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
/// Counter SHA-256; each rejection consumes a counter; little-endian first u64;
/// reject words below 2^64 modulo inclusive width (no modulo bias).
pub const RNG_POLICY: &str = "benilla-native-sha256-counter-le64-rejection-fraction53-v2";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DrawPurpose {
    EventAiChance {
        npc: crate::NpcId,
        event_id: u32,
        occurrence: u64,
    },
    EventAiAction {
        npc: crate::NpcId,
        event_id: u32,
        occurrence: u64,
    },
    Loot {
        corpse: crate::NpcId,
    },
    LootChance {
        corpse: crate::NpcId,
    },
    LootShuffle {
        corpse: crate::NpcId,
    },
    Hit,
    Direct {
        slot: u8,
    },
    Critical,
    Periodic {
        slot: u8,
    },
    Pushback,
    MeleeDamage {
        action: u64,
        npc: crate::NpcId,
    },
    MeleeOutcome {
        action: u64,
        npc: crate::NpcId,
    },
    DurabilityChance {
        action: u64,
        npc: crate::NpcId,
    },
    DurabilitySlot {
        action: u64,
        npc: crate::NpcId,
    },
    DefenseGain {
        action: u64,
        npc: crate::NpcId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draw {
    pub cast_id: u64,
    pub purpose: DrawPurpose,
    pub first_counter: u64,
    pub consumed: u32,
    pub low: i64,
    pub high: i64,
    pub value: i64,
}
impl State {
    pub(super) fn draw(
        &mut self,
        cast_id: u64,
        purpose: DrawPurpose,
        low: i32,
        high: i32,
        out: &mut Vec<Draw>,
    ) -> Result<i32, NativeError> {
        self.draw_range(cast_id, purpose, i64::from(low), i64::from(high), out)?
            .try_into()
            .map_err(|_| NativeError::InvalidState)
    }
    pub(super) fn draw_range(
        &mut self,
        cast_id: u64,
        purpose: DrawPurpose,
        low: i64,
        high: i64,
        out: &mut Vec<Draw>,
    ) -> Result<i64, NativeError> {
        if low > high {
            return Err(NativeError::InvalidState);
        }
        let width = high
            .checked_sub(low)
            .and_then(|v| v.checked_add(1))
            .ok_or(NativeError::InvalidState)? as u64;
        let threshold = width.wrapping_neg() % width;
        let first = self.rng_counter;
        for _ in 0..64 {
            let counter = self.rng_counter;
            self.rng_counter = self.rng_counter.checked_add(1).ok_or(NativeError::Limit)?;
            let mut hash = Sha256::new();
            hash.update(RNG_POLICY.as_bytes());
            hash.update(self.world);
            hash.update(self.seed.to_le_bytes());
            hash.update(counter.to_le_bytes());
            let bytes = hash.finalize();
            let n = u64::from_le_bytes(bytes[..8].try_into().unwrap());
            if n >= threshold {
                let value = low + (n % width) as i64;
                out.push(Draw {
                    cast_id,
                    purpose,
                    first_counter: first,
                    consumed: (self.rng_counter - first)
                        .try_into()
                        .map_err(|_| NativeError::Limit)?,
                    low,
                    high,
                    value,
                });
                return Ok(value);
            }
        }
        Err(NativeError::Limit)
    }
}

/// Full32 requests have width 2^32, threshold zero and consume one counter.
pub(super) fn full_u32(world: crate::WorldId, seed: u64, counter: u64) -> u32 {
    let mut hash = Sha256::new();
    hash.update(RNG_POLICY.as_bytes());
    hash.update(world);
    hash.update(seed.to_le_bytes());
    hash.update(counter.to_le_bytes());
    let bytes = hash.finalize();
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}
