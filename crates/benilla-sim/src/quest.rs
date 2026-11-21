// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic QuestDef.cpp and Player.cpp at
// 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f. See ../PROVENANCE.md.
// Copyright: CMaNGOS Project contributors; see ../licenses/AUTHORS.md.
// Upstream grants GPL version 2 or (at your option) any later version.
// This modified Rust translation is distributed under GPL version 3 or later,
// WITHOUT ANY WARRANTY, including MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
//! Bounded native quest arithmetic and three independent eligibility predicates.
//!
//! This is NOT CanTakeQuest, CanRewardQuest, RewardQuest or a quest lifecycle.
//! Callers must resolve native rows, all other gates, inventory, scripts, status,
//! reward choice, GiveXP and ordered ModifyMoney semantics. See ../QUESTS.md.
//! Every floating operation uses software binary32, never host floating point.

use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};

/// A compiler/runtime must include this identity in its rules compatibility hash.
pub const QUEST_REWARD_POLICY_ID: &str = "cmangos-8ec338a1-quest-binary32-rne-v1";
/// QuestDef.h: suppress max-level money conversion, but NOT normal quest XP.
pub const QUEST_FLAGS_NO_MONEY_FROM_XP: u16 = 0x0100;
const ROUND: Round = Round::NearestTiesToEven;

/// Invalid domains and undefined upstream float casts fail instead of clipping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestError {
    pub field: &'static str,
}
impl std::fmt::Display for QuestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported Classic quest input/result: {}", self.field)
    }
}
impl std::error::Error for QuestError {}

/// A captured/resolved native float rate, stored as binary32 bits, not decimal.
/// Accepts positive normal values through 1000 and positive zero. No Default:
/// omission of a server setting is not proof that the setting equals one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binary32Rate(u32);
impl Binary32Rate {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(0x3f80_0000);

    pub fn from_bits(bits: u32) -> Result<Self, QuestError> {
        // Positive finite binary32 encoding order equals numerical order.
        if bits > 0x447a_0000 || (bits != 0 && bits < 0x0080_0000) {
            return Err(QuestError { field: "rate_bits" });
        }
        Ok(Self(bits))
    }
    pub const fn to_bits(self) -> u32 {
        self.0
    }
    fn value(self) -> Single {
        Single::from_bits(u128::from(self.0))
    }
}

/// Native quest_template fields used by Quest::XPValue, BEFORE flags/rates.
/// QuestLevel is unsigned in the pinned loader: retain zero, reject >255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestXpInput {
    pub quest_level: u32,
    pub reward_money_max_level: u32,
}

fn checked<T>(result: StatusAnd<T>, field: &'static str) -> Result<T, QuestError> {
    if !(result.status & !Status::INEXACT).is_empty() {
        Err(QuestError { field })
    } else {
        Ok(result.value)
    }
}
fn level(value: u8, field: &'static str) -> Result<(), QuestError> {
    if value == 0 {
        Err(QuestError { field })
    } else {
        Ok(())
    }
}
fn unsigned(value: u32) -> Result<Single, QuestError> {
    checked(
        Single::from_u128_r(u128::from(value), ROUND),
        "uint_to_float",
    )
}
fn uint(value: Single, field: &'static str) -> Result<u32, QuestError> {
    let mut exact = false;
    checked(value.to_u128_r(32, Round::TowardZero, &mut exact), field).map(|v| v as u32)
}
fn int(value: Single, field: &'static str) -> Result<i32, QuestError> {
    let mut exact = false;
    checked(value.to_i128_r(32, Round::TowardZero, &mut exact), field).map(|v| v as i32)
}
fn scaled(value: u32, rate: Binary32Rate) -> Result<Single, QuestError> {
    checked(unsigned(value)?.mul_r(rate.value(), ROUND), "rate_product")
}

/// Quest::XPValue, including the null-player and zero-quest-level branches.
/// Player levels are 1..=255; quest levels are 0..=255. All u32 money inputs
/// are accepted if the source's final uint32 conversion has a defined result.
/// Conversion to binary32 precedes division, each penalty multiply rounds again,
/// then ceilf rounds toward positive infinity. The final uint32 cast is checked.
pub fn quest_xp_value(input: QuestXpInput, player_level: Option<u8>) -> Result<u32, QuestError> {
    if input.quest_level > 255 {
        return Err(QuestError {
            field: "quest_level",
        });
    }
    let Some(player_level) = player_level else {
        return Ok(0);
    };
    level(player_level, "player_level")?;
    if input.reward_money_max_level == 0 || input.quest_level == 0 {
        return Ok(0);
    }
    let divisor = match input.quest_level {
        1..=60 => 0x3f19_999a, // 0.6f
        61 => 0x3f99_999a,     // 1.2f
        62 => 0x4019_999a,     // 2.4f
        63 => 0x4066_6666,     // 3.6f
        64 => 0x4099_999a,     // 4.8f
        _ => 0x40c0_0000,      // 6.0f
    };
    let full = checked(
        unsigned(input.reward_money_max_level)?.div_r(Single::from_bits(divisor), ROUND),
        "xp_division",
    )?;
    let player_level = u32::from(player_level);
    let quest_level = input.quest_level;
    let penalty = if player_level <= quest_level + 5 {
        None
    } else if player_level == quest_level + 6 {
        Some(0x3f4c_cccd) // 0.8f
    } else if player_level == quest_level + 7 {
        Some(0x3f19_999a) // 0.6f
    } else if player_level == quest_level + 8 {
        Some(0x3ecc_cccd) // 0.4f
    } else if player_level == quest_level + 9 {
        Some(0x3e4c_cccd) // 0.2f
    } else {
        Some(0x3dcc_cccd) // 0.1f
    };
    let penalized = match penalty {
        Some(bits) => checked(full.mul_r(Single::from_bits(bits), ROUND), "xp_penalty")?,
        None => full,
    };
    let ceiling = checked(
        penalized.round_to_integral(Round::TowardPositive),
        "xp_ceil",
    )?;
    uint(ceiling, "xp_value")
}

/// Quest::GetRewOrReqMoney. Positive rewards use the money rate; negative
/// requirements and zero do not. This getter alone preserves i32::MIN, although
/// the reward quote rejects that value because CanRewardQuest negates it in C++.
pub fn reward_or_required_money(raw: i32, money_rate: Binary32Rate) -> Result<i32, QuestError> {
    if raw <= 0 {
        return Ok(raw);
    }
    int(scaled(raw as u32, money_rate)?, "reward_money")
}

/// Only the numerical fields: this type is NOT evidence that a quest is supported.
/// Other QuestFlags have no effect on these formulas. A native compiler must
/// separately handle/reject every flag and all omitted quest fields/dependencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestRewardInput {
    pub xp: QuestXpInput,
    pub reward_or_required_money: i32,
    pub quest_flags: u16,
}
/// Explicit resolved world configuration; capture-range limits are not level caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardContext {
    pub player_level: u8,
    pub max_player_level: u8,
    pub xp_rate: Binary32Rate,
    pub money_rate: Binary32Rate,
}

/// A pure preview of RewardQuest's numeric block, with no state mutation.
/// `display_xp` is computed even at cap. `xp_to_give` is Some below cap (including
/// Some(0)), None at/above cap; actual GiveXP eligibility/level-up remains required.
/// Apply `max_level_money` first when Some, then `reward_or_required_money` as
/// SEPARATE ModifyMoney calls: source money clamping makes summing them incorrect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestRewardQuote {
    pub display_xp: u32,
    pub xp_to_give: Option<u32>,
    pub max_level_money: Option<i32>,
    pub reward_or_required_money: i32,
    /// CanRewardQuest checks this against existing money BEFORE any award.
    pub money_required_before_reward: u32,
}

/// Isolated XP/money calculation in Player::RewardQuest, NOT authorization to
/// reward. No status, item, reputation, mail, script, spell, repeat or week logic.
pub fn reward_quote(
    input: QuestRewardInput,
    context: RewardContext,
) -> Result<QuestRewardQuote, QuestError> {
    level(context.player_level, "player_level")?;
    level(context.max_player_level, "max_player_level")?;
    if input.reward_or_required_money == i32::MIN {
        return Err(QuestError {
            field: "required_money_negation",
        });
    }
    let base_xp = quest_xp_value(input.xp, Some(context.player_level))?;
    let display_xp = uint(scaled(base_xp, context.xp_rate)?, "rated_xp")?;
    let (xp_to_give, max_level_money) = if context.player_level < context.max_player_level {
        (Some(display_xp), None)
    } else {
        let raw = if input.quest_flags & QUEST_FLAGS_NO_MONEY_FROM_XP != 0 {
            0
        } else {
            input.xp.reward_money_max_level
        };
        (
            None,
            Some(int(scaled(raw, context.money_rate)?, "max_level_money")?),
        )
    };
    let reward_or_required_money =
        reward_or_required_money(input.reward_or_required_money, context.money_rate)?;
    Ok(QuestRewardQuote {
        display_xp,
        xp_to_give,
        max_level_money,
        reward_or_required_money,
        money_required_before_reward: if reward_or_required_money < 0 {
            reward_or_required_money.unsigned_abs()
        } else {
            0
        },
    })
}

/// The loader-resolved values used by SatisfyQuestLevel/Class/Race only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestEligibilityInput {
    pub minimum_level: u32,
    pub maximum_level: u32,
    pub required_classes: u32,
    pub required_races: u32,
}
/// Native IDs, not zero-based indices. Legal race/class combinations must be
/// established by the compiler's selected playercreateinfo record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestPlayerIdentity {
    pub level: u8,
    pub race: u8,
    pub class: u8,
}
/// Three independent predicates, deliberately no overall "can take quest" result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestEligibilityGates {
    pub level: bool,
    pub class: bool,
    pub race: bool,
}

/// Player::SatisfyQuestLevel/Class/Race without optional response-packet side effects.
/// Maximum level zero is NOT treated as unlimited; pass the loader-resolved value.
/// Conditions, skills, reputation, status, prerequisite/exclusive/breadcrumb chains,
/// timed/weekly limits, active status and CanAddQuest are separate required gates.
pub fn eligibility_gates(
    input: QuestEligibilityInput,
    player: QuestPlayerIdentity,
) -> Result<QuestEligibilityGates, QuestError> {
    level(player.level, "player_level")?;
    if !(1..=8).contains(&player.race) {
        return Err(QuestError { field: "race" });
    }
    if !matches!(player.class, 1..=5 | 7..=9 | 11) {
        return Err(QuestError { field: "class" });
    }
    let class_mask = 1u32 << (player.class - 1);
    let race_mask = 1u32 << (player.race - 1);
    let level = u32::from(player.level);
    Ok(QuestEligibilityGates {
        level: level >= input.minimum_level && level <= input.maximum_level,
        class: input.required_classes == 0 || input.required_classes & class_mask != 0,
        race: input.required_races == 0 || input.required_races & race_mask != 0,
    })
}
