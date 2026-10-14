// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Creature.cpp InspectingLoot/ReduceCorpseDecayTimer, pinned 8ec338a1.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later, without warranty.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseTimerCall {
    pub at_ms: u64,
    pub delay_ms: u32,
    pub inspecting: bool,
}
impl SoloCreatureAi {
    pub fn adjust_loot_corpse_timer(&mut self, call: CorpseTimerCall) -> Result<(), AiError> {
        if self.phase != Phase::Corpse
            || call.delay_ms == 0
            || call.at_ms < self.now_ms
            || call.at_ms < self.corpse_timer_last_ms
        {
            return Err(AiError::Invalid("corpse timer callback"));
        }
        let until = call
            .at_ms
            .checked_add(call.delay_ms.into())
            .ok_or(AiError::Invalid("corpse timer overflow"))?;
        let previous = self.corpse_at_ms;
        self.corpse_at_ms = if call.inspecting {
            previous.max(until)
        } else {
            previous.min(until)
        };
        self.corpse_timer_last_ms = call.at_ms;
        if previous != self.corpse_at_ms {
            self.corpse_timer_previous_ms = previous;
            self.corpse_timer_calls.clear();
            self.corpse_timer_calls.push(call);
        }
        Ok(())
    }
    pub fn corpse_timer_calls(&self) -> &[CorpseTimerCall] {
        &self.corpse_timer_calls
    }
    pub fn corpse_expiration_ms(&self) -> u64 {
        self.corpse_at_ms
    }
    /// Actual source corpse-removal update, before the DEAD branch starts skipping Unit time.
    pub fn despawned_at_ms(&self) -> Option<u64> {
        self.despawned_at_ms
    }
    pub fn death_at_ms(&self) -> Option<u64> {
        self.death_at_ms
    }
}
/// Source LootHandler inclusive range; GetDistance(...DIST_CALC_NONE) is squared 3D.
pub fn loot_interaction_distance(
    player: Point,
    creature: Point,
    player_reach: Real,
    creature_reach: Real,
) -> Result<bool, AiError> {
    let range = creature_reach
        .add(Real::constant(0x3faa_aaab))?
        .add(player_reach)?;
    let range = if range.less(Real::constant(0x40a0_0000)) {
        Real::constant(0x40a0_0000)
    } else {
        range
    };
    Ok(!player
        .distance_squared(creature)?
        .greater(range.mul(range)?))
}
