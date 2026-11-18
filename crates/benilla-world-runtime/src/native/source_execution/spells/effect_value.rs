// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use rustc_apfloat::{ieee::Single, Float, Round, Status};

// Object.cpp:2923-2965. Calculate the fixed dice path without RNG.
pub(super) fn effect_value(t: &SpellTemplate, i: usize, current: i32) -> Result<i32> {
    let dice = [t.effectbasedice1, t.effectbasedice2, t.effectbasedice3][i] as i32;
    let base = current
        .checked_sub(dice)
        .ok_or_else(|| refuse("execution.spells.Object_2927.base_points_sub_undefined"))?;
    require(
        [
            t.effectrealpointsperlevel1,
            t.effectrealpointsperlevel2,
            t.effectrealpointsperlevel3,
        ][i]
            .0
            == 0,
        "execution.spells.Object_2934.effect_level_scaling_owner",
    )?;
    require(
        matches!(
            [t.effectdiesides1, t.effectdiesides2, t.effectdiesides3][i],
            0 | 1
        ),
        "execution.spells.Object_2955.effect_dice_rng_owner",
    )?;
    let value = Single::from_i128(i128::from(base))
        .value
        .add_r(
            Single::from_i128(i128::from(dice)).value,
            Round::NearestTiesToEven,
        )
        .value;
    // Object.cpp:3014-3024. Stop before unowned scaling or an undefined cast.
    require(
        t.attributes & 0x0008_0000 == 0
            || t.spelllevel == 0
            || !matches!(t.effects()[i].aura, 3 | 53 | 69 | 162 | 64),
        "execution.spells.Object_3014.GetCreatureClassLvlStats_owner",
    )?;
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    require(
        !converted.status.contains(Status::INVALID_OP),
        "execution.spells.Object_3024.effect_float_to_int_undefined",
    )?;
    Ok(converted.value as i32)
}

#[test]
fn fixed_effect_value_checks_signed_subtraction_before_float_conversion() {
    let t = SpellTemplate {
        effectbasedice1: 1,
        effectdiesides1: 1,
        ..Default::default()
    };
    assert_eq!(effect_value(&t, 0, i32::MIN + 1).unwrap(), i32::MIN);
    assert_eq!(
        effect_value(&t, 0, i32::MIN).unwrap_err().id,
        "execution.spells.Object_2927.base_points_sub_undefined"
    );
    assert_eq!(effect_value(&t, 0, 2_147_483_520).unwrap(), 2_147_483_520);
    assert_eq!(
        effect_value(&t, 0, i32::MAX).unwrap_err().id,
        "execution.spells.Object_3024.effect_float_to_int_undefined"
    );
}
