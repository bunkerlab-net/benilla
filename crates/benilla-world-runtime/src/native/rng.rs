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
    PlayerMeleeDamage { action: u64, npc: crate::NpcId },
    PlayerMeleeOutcome { action: u64, npc: crate::NpcId },
    PlayerMeleeGlance { action: u64, npc: crate::NpcId },
    PlayerWeaponGain { action: u64, npc: crate::NpcId },
    PlayerDurabilityChance { action: u64, npc: crate::NpcId },
    PlayerDurabilitySlot { action: u64, npc: crate::NpcId },
    WanderAngle {
        npc: crate::NpcId,
    },
    WanderRadius {
        npc: crate::NpcId,
    },
    WanderMoveCount {
        npc: crate::NpcId,
    },
    WanderPause {
        npc: crate::NpcId,
    },
    CreatureModel {
        npc: crate::NpcId,
    },
    CreatureGender {
        npc: crate::NpcId,
    },
    CreatureLevel {
        npc: crate::NpcId,
    },
    CreatureInitialRespawnDelay {
        npc: crate::NpcId,
    },
    CreatureDeathRespawnDelay {
        npc: crate::NpcId,
    },
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
        let sample = sample_inclusive_v2(self.world, self.seed, &mut self.rng_counter, low, high)?;
        out.push(Draw {
            cast_id,
            purpose,
            first_counter: sample.first_counter,
            consumed: sample.consumed_words,
            low,
            high,
            value: sample.value,
        });
        Ok(sample.value)
    }
}

/// A successful semantic request, including every rejected raw word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SampleSpan {
    pub first_counter: u64,
    pub consumed_words: u32,
    pub value: i64,
}

pub(super) fn word_v2(world: crate::WorldId, seed: u64, counter: u64) -> u64 {
    let mut hash = Sha256::new();
    hash.update(RNG_POLICY.as_bytes());
    hash.update(world);
    hash.update(seed.to_le_bytes());
    hash.update(counter.to_le_bytes());
    let bytes = hash.finalize();
    u64::from_le_bytes(bytes[..8].try_into().unwrap())
}

pub(super) fn sample_inclusive_v2(
    world: crate::WorldId,
    seed: u64,
    cursor: &mut u64,
    low: i64,
    high: i64,
) -> Result<SampleSpan, NativeError> {
    sample_inclusive(cursor, low, high, |counter| word_v2(world, seed, counter))
}

// Both native genesis and gameplay use this identical bounded rejection core.
// A failed request publishes neither a partially advanced cursor nor a receipt.
fn sample_inclusive(
    cursor: &mut u64,
    low: i64,
    high: i64,
    mut word: impl FnMut(u64) -> u64,
) -> Result<SampleSpan, NativeError> {
    if low > high {
        return Err(NativeError::InvalidState);
    }
    let width = high
        .checked_sub(low)
        .and_then(|v| v.checked_add(1))
        .ok_or(NativeError::InvalidState)? as u64;
    let threshold = width.wrapping_neg() % width;
    let first = *cursor;
    let mut next = first;
    for consumed_words in 1..=64 {
        let counter = next;
        next = next.checked_add(1).ok_or(NativeError::Limit)?;
        let n = word(counter);
        if n >= threshold {
            let value = low + (n % width) as i64;
            *cursor = next;
            return Ok(SampleSpan {
                first_counter: first,
                consumed_words,
                value,
            });
        }
    }
    Err(NativeError::Limit)
}

/// Full32 requests have width 2^32, threshold zero and consume one counter.
pub(super) fn full_u32(world: crate::WorldId, seed: u64, counter: u64) -> u32 {
    word_v2(world, seed, counter) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_rejection_core_counts_words_and_rolls_back_failure() {
        let mut cursor = 0;
        let sample = sample_inclusive(&mut cursor, 0, 6, |i| [0, 1, 9][i as usize]).unwrap();
        assert_eq!(
            sample,
            SampleSpan {
                first_counter: 0,
                consumed_words: 3,
                value: 2
            }
        );
        assert_eq!(cursor, 3);
        let before = cursor;
        assert_eq!(
            sample_inclusive(&mut cursor, 0, 6, |_| 0),
            Err(NativeError::Limit)
        );
        assert_eq!(cursor, before);
        let sample =
            sample_inclusive(&mut cursor, 0, 6, |i| if i == before + 63 { 8 } else { 0 }).unwrap();
        assert_eq!(sample.consumed_words, 64);
        assert_eq!(cursor, before + 64);
        let mut exhausted = u64::MAX;
        assert_eq!(
            sample_inclusive(&mut exhausted, 0, 6, |_| 9),
            Err(NativeError::Limit)
        );
        assert_eq!(exhausted, u64::MAX);
    }
    #[test]
    fn real_v2_words_and_wide_rejection_vector_are_unchanged() {
        assert_eq!(word_v2([8; 16], 7, 0), 0xeddc4fde402dfc2d);
        assert_eq!(word_v2([8; 16], 7, 1), 0xd3f45c88cd86539d);
        for (seed, value) in [(7, 5), (13, 0), (1, 6)] {
            let mut cursor = 0;
            assert_eq!(
                sample_inclusive_v2([8; 16], seed, &mut cursor, 0, 6)
                    .unwrap()
                    .value,
                value
            );
            assert_eq!(cursor, 1);
        }
        let mut cursor = 0;
        let sample = sample_inclusive_v2([8; 16], 1, &mut cursor, 0, 1i64 << 62).unwrap();
        assert_eq!(
            sample,
            SampleSpan {
                first_counter: 0,
                consumed_words: 2,
                value: 2699572827467448382
            }
        );
        assert_eq!(cursor, 2);
    }
}
