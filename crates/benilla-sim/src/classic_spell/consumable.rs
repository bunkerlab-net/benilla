// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 SpellAuras.cpp:395-407,3617-3646,5666-5683.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Food/drink periodic counters. The host owns health, mana and heartbeat callbacks.
use super::{SpellError, MAX_TIME};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableAura {
    duration_ms: u32,
    remaining_ms: u32,
    amplitude_ms: u32,
    periodic_debt_ms: i64,
    periodic_ticks: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumableUpdate {
    pub periodic: bool,
    pub expired: bool,
}
impl ConsumableAura {
    pub fn new(duration_ms: u32, amplitude_ms: u32) -> Result<Self, SpellError> {
        let aura = Self {
            duration_ms,
            remaining_ms: duration_ms,
            amplitude_ms,
            periodic_debt_ms: 5000,
            periodic_ticks: 0,
        };
        aura.validate()?;
        Ok(aura)
    }
    pub fn validate(&self) -> Result<(), SpellError> {
        if self.duration_ms == 0
            || self.duration_ms > MAX_TIME
            || self.remaining_ms > self.duration_ms
            || self.amplitude_ms == 0
            || self.amplitude_ms > MAX_TIME
            || self.periodic_debt_ms > i64::from(self.amplitude_ms.max(5000))
            || self.periodic_debt_ms < -i64::from(MAX_TIME)
            || self.periodic_ticks > MAX_TIME
        {
            return Err(SpellError::State("consumable aura counters"));
        }
        Ok(())
    }
    pub fn remaining_ms(&self) -> u32 {
        self.remaining_ms
    }
    pub fn periodic_debt_ms(&self) -> i64 {
        self.periodic_debt_ms
    }
    pub fn periodic_ticks(&self) -> u32 {
        self.periodic_ticks
    }
    pub fn update(&mut self, diff_ms: u32) -> Result<ConsumableUpdate, SpellError> {
        self.validate()?;
        if self.remaining_ms == 0 || diff_ms > MAX_TIME {
            return Err(SpellError::State("consumable aura elapsed domain"));
        }
        self.periodic_debt_ms -= i64::from(diff_ms);
        let periodic = self.periodic_debt_ms <= 0;
        if periodic {
            self.periodic_debt_ms += i64::from(self.amplitude_ms);
            self.periodic_ticks += 1;
        }
        self.remaining_ms = self.remaining_ms.saturating_sub(diff_ms);
        Ok(ConsumableUpdate {
            periodic,
            expired: self.remaining_ms == 0,
        })
    }
    pub fn matches_fixed_steps(&self, fresh: &Self, updates: u64, diff_ms: u32) -> bool {
        let Some(elapsed) = updates.checked_mul(u64::from(diff_ms)) else {
            return false;
        };
        if diff_ms == 0 || elapsed >= u64::from(fresh.duration_ms) {
            return false;
        }
        let ticks = if diff_ms > fresh.amplitude_ms {
            updates.saturating_sub(5000u64.div_ceil(u64::from(diff_ms)) - 1)
        } else if elapsed >= 5000 {
            (elapsed - 5000) / u64::from(fresh.amplitude_ms) + 1
        } else {
            0
        };
        self.duration_ms == fresh.duration_ms
            && self.amplitude_ms == fresh.amplitude_ms
            && self.remaining_ms == fresh.duration_ms - elapsed as u32
            && u64::from(self.periodic_ticks) == ticks
            && self.periodic_debt_ms
                == 5000 - elapsed as i64 + ticks as i64 * i64::from(fresh.amplitude_ms)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_tick_precedes_expiry_and_debt_survives_serialization() {
        let mut food = ConsumableAura::new(10_000, 5000).unwrap();
        assert_eq!(
            food.update(5000).unwrap(),
            ConsumableUpdate {
                periodic: true,
                expired: false
            }
        );
        assert_eq!(
            food.update(5000).unwrap(),
            ConsumableUpdate {
                periodic: true,
                expired: true
            }
        );
        let mut drink = ConsumableAura::new(20_000, 2000).unwrap();
        assert!(drink.update(10_000).unwrap().periodic);
        assert_eq!(drink.periodic_debt_ms(), -3000);
        let bytes = serde_json::to_vec(&drink).unwrap();
        let mut reopened: ConsumableAura = serde_json::from_slice(&bytes).unwrap();
        assert!(reopened.update(100).unwrap().periodic);
        assert_eq!(reopened.periodic_debt_ms(), -1100);
        assert_eq!(reopened.periodic_ticks(), 2);
    }
}
