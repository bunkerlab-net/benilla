// SPDX-License-Identifier: GPL-3.0-or-later
// Player::CalculateReputationGain / ReputationMgr::SetReputation, source order.
use super::{Config, Error, GainInput, Resolved, Result, RewardRates, Source};
use crate::{
    classic_spell::SpellFloat,
    xp::{self, Level},
};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
const R: Round = Round::NearestTiesToEven;
fn check<T>(v: StatusAnd<T>) -> Result<T> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric("binary32_exception"));
    }
    Ok(v.value)
}
fn n(v: i64) -> Result<Single> {
    check(Single::from_i128_r(v.into(), R))
}
fn mul(a: Single, b: Single) -> Result<Single> {
    check(a.mul_r(b, R))
}
fn trunc(v: Single) -> Result<i32> {
    i32::try_from(check(v.to_i128_r(32, Round::TowardZero, &mut false))?)
        .map_err(|_| Error::Numeric("int32_conversion"))
}
pub(super) fn finite(rate: SpellFloat) -> Result<Single> {
    SpellFloat::from_bits(rate.bits()).map_err(|_| Error::Numeric("rate_encoding"))?;
    Ok(Single::from_bits(rate.bits().into()))
}
pub(super) fn nonnegative(rate: SpellFloat) -> Result<Single> {
    let rate = finite(rate)?;
    if rate < Single::ZERO {
        return Err(Error::Invalid("negative_reward_rate"));
    }
    Ok(rate)
}
pub(super) fn spillover(standing: i32, rate: SpellFloat) -> Result<i32> {
    trunc(mul(n(standing.into())?, finite(rate)?)?)
}
pub(super) fn gain(
    config: Config,
    rates: Option<&Resolved<RewardRates>>,
    input: GainInput,
) -> Result<i32> {
    let level = Level::new(input.player_level).ok_or(Error::Invalid("player_level"))?;
    // A positive signed QuestLevel is uint32 at the source call. Check threshold addition.
    if input.target_level > i32::MAX as u32 {
        return Err(Error::Invalid("target_level"));
    }
    let mut percent = n(100)?; // Aura absence was explicitly validated by Catalog.
    let minimum = match input.source {
        Source::Kill => nonnegative(config.low_level_kill_rate)?,
        Source::Quest => nonnegative(config.low_level_quest_rate)?,
        Source::Spell => n(1)?,
    };
    let current = u32::from(input.player_level);
    if input.target_level > 0 {
        let difference = match input.source {
            Source::Kill
                if input.target_level < current
                    && xp::is_trivial(
                        level,
                        Level::new(input.target_level as u8)
                            .ok_or(Error::Invalid("target_level"))?,
                    ) =>
            {
                Some(current - xp::quest_green_range(level) - input.target_level)
            }
            Source::Quest if current > input.target_level + 5 => {
                Some(current - input.target_level - 5)
            }
            _ => None,
        };
        if let Some(difference) = difference {
            let penalty = mul(Single::from_bits(0x3e4ccccd), n(difference.into())?)?;
            let factor = check(n(1)?.sub_r(penalty, R))?.max(minimum);
            percent = mul(percent, factor)?;
        }
    }
    if percent <= Single::ZERO {
        return Ok(0);
    }
    if let Some(Resolved::Present(rates)) = rates {
        let rate = nonnegative(match input.source {
            Source::Kill => rates.creature,
            Source::Quest => rates.quest,
            Source::Spell => rates.spell,
        })?;
        if rate <= Single::ZERO {
            return Ok(0);
        }
        percent = mul(percent, rate)?;
    }
    let amount = mul(
        mul(nonnegative(config.gain_rate)?, n(input.reputation.into())?)?,
        percent,
    )?;
    trunc(check(amount.div_r(n(100)?, R))?)
}
