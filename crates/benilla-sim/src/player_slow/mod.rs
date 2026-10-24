// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Unit.cpp and
// SpellAuras.cpp. Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Bounded living, unmounted player decrease-speed arithmetic. This is not aura
//! admission: the host must exclude positive/mounted/normalization/ghost bonuses,
//! custom speed ratios and all unrepresented movement modifiers.
mod numeric;
use crate::creature_ai::Real;

pub const PLAYER_SLOW_POLICY: &str = "cmangos-8ec338a1-player-slow-softfloat-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlowError {
    Invalid(&'static str),
    Numeric(&'static str),
}
impl std::fmt::Display for SlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bounded player slow: {self:?}")
    }
}
impl std::error::Error for SlowError {}

/// Source UnitMoveType speed selection. Walk and backward swim are unaffected
/// by HandleAuraModDecreaseSpeed, which updates only RUN/RUN_BACK/SWIM.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MoveType {
    Walk,
    Run,
    RunBack,
    Swim,
    SwimBack,
}
impl MoveType {
    /// Source GetXYFlagBasedSpeed's mode priority, after proving lateral motion.
    pub const fn from_mode(swimming: bool, walking: bool, backward: bool) -> Self {
        if swimming {
            if backward {
                Self::SwimBack
            } else {
                Self::Swim
            }
        } else if walking {
            Self::Walk
        } else if backward {
            Self::RunBack
        } else {
            Self::Run
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedRates {
    pub run: Real,
    pub run_back: Real,
    pub swim: Real,
}
impl SpeedRates {
    pub const fn normal() -> Self {
        Self {
            run: Real::ONE,
            run_back: Real::ONE,
            swim: Real::ONE,
        }
    }
    /// Strongest negative modifier, never a product or sum of slow holders.
    /// Inputs are already source-derived signed aura amounts; positive amounts
    /// and more than 16 holders are outside this deliberately bounded kernel.
    pub fn from_amounts(amounts: impl IntoIterator<Item = i32>) -> Result<Self, SlowError> {
        let mut strongest = 0;
        for (index, amount) in amounts.into_iter().enumerate() {
            if index >= 16 || !(-1_000_000..=0).contains(&amount) {
                return Err(SlowError::Invalid("bounded nonpositive slow holders"));
            }
            strongest = strongest.min(amount);
        }
        let rate = numeric::rate(strongest)?;
        Ok(Self {
            run: rate,
            run_back: rate,
            swim: rate,
        })
    }
    /// Source GetSpeed: stored binary32 rate multiplied by baseMoveSpeed.
    pub fn speed(self, mode: MoveType) -> Result<Real, SlowError> {
        numeric::speed(self, mode)
    }
}

#[cfg(test)]
mod tests;
