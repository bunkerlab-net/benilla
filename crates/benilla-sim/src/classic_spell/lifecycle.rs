// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// SpellAuras.cpp:3367-3386,3934-3941; Unit.cpp:10474-10490.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Finite Frost Armor and Chilled holders. Application and proc checks belong to the host.
use super::{numeric, SpellError, MAX_AMOUNT, MAX_TIME};
use crate::player_slow::SpeedRates;
use rustc_apfloat::{Float, Round};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AuraLifecycleSpec {
    ArmorProc {
        armor: i32,
        trigger_spell: u32,
    },
    Chilled {
        melee_haste: i32,
        movement_slow: i32,
    },
}
impl AuraLifecycleSpec {
    pub fn validate(self) -> Result<(), SpellError> {
        let valid = match self {
            Self::ArmorProc {
                armor,
                trigger_spell,
            } => (0..=MAX_AMOUNT as i32).contains(&armor) && trigger_spell == 6136,
            Self::Chilled {
                melee_haste,
                movement_slow,
            } => (-1000..=0).contains(&melee_haste) && (-100..=0).contains(&movement_slow),
        };
        if !valid {
            return Err(SpellError::Unsupported("aura lifecycle operands"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedAura {
    spec: AuraLifecycleSpec,
    duration_ms: u32,
    remaining_ms: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuraLifecycleUpdate {
    pub expired: bool,
}
impl TimedAura {
    pub fn new(spec: AuraLifecycleSpec, duration_ms: u32) -> Result<Self, SpellError> {
        let aura = Self {
            spec,
            duration_ms,
            remaining_ms: duration_ms,
        };
        aura.validate()?;
        Ok(aura)
    }
    pub fn validate(&self) -> Result<(), SpellError> {
        self.spec.validate()?;
        if self.duration_ms == 0
            || self.duration_ms > MAX_TIME
            || self.remaining_ms > self.duration_ms
        {
            return Err(SpellError::State("timed aura counters"));
        }
        Ok(())
    }
    pub fn spec(&self) -> AuraLifecycleSpec {
        self.spec
    }
    pub fn remaining_ms(&self) -> u32 {
        self.remaining_ms
    }
    pub fn duration_ms(&self) -> u32 {
        self.duration_ms
    }
    pub fn update(&mut self, diff_ms: u32) -> Result<AuraLifecycleUpdate, SpellError> {
        self.validate()?;
        if self.remaining_ms == 0 || diff_ms > MAX_TIME {
            return Err(SpellError::State("expired aura update or elapsed domain"));
        }
        self.remaining_ms = self.remaining_ms.saturating_sub(diff_ms);
        Ok(AuraLifecycleUpdate {
            expired: self.remaining_ms == 0,
        })
    }
    pub fn armor(&self) -> i32 {
        if self.remaining_ms == 0 {
            return 0;
        }
        match self.spec {
            AuraLifecycleSpec::ArmorProc { armor, .. } => armor,
            _ => 0,
        }
    }
    pub fn movement_rates(&self) -> Result<SpeedRates, SpellError> {
        let amount = if self.remaining_ms == 0 {
            0
        } else {
            match self.spec {
                AuraLifecycleSpec::Chilled { movement_slow, .. } => movement_slow,
                _ => 0,
            }
        };
        SpeedRates::from_amounts([amount])
            .map_err(|_| SpellError::Domain("chilled movement modifier"))
    }
    /// ApplyAttackTimePercentMod's negative branch in binary32 source order.
    /// The host adjusts the running timer by new minus old.
    pub fn attack_time(&self, base_ms: u32) -> Result<u32, SpellError> {
        self.spec.validate()?;
        if base_ms == 0 || base_ms > 60_000 {
            return Err(SpellError::Domain("chilled attack time"));
        }
        if self.remaining_ms == 0 {
            return Ok(base_ms);
        }
        let amount = match self.spec {
            AuraLifecycleSpec::Chilled { melee_haste, .. } => -melee_haste,
            _ => 0,
        };
        let factor = numeric::checked(
            numeric::integer(i64::from(100 + amount))?
                .div_r(numeric::integer(100)?, Round::NearestTiesToEven),
        )?;
        let value = numeric::checked(
            numeric::integer(i64::from(base_ms))?.mul_r(factor, Round::NearestTiesToEven),
        )?;
        u32::try_from(numeric::signed(value)?)
            .map_err(|_| SpellError::Domain("chilled attack time conversion"))
    }
}
#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
