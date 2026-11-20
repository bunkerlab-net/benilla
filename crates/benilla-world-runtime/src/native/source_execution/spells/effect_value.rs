// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use rustc_apfloat::{ieee::Single, Float, Round, Status};

// Object.cpp:2923-2965. The self-hit caller has a unit caster and uses fixed dice.
pub(super) fn effect_value(
    t: &SpellTemplate,
    i: usize,
    current: i32,
    caster_level: Option<u32>,
    caster_is_player: bool,
) -> Result<i32> {
    let dice = [t.effectbasedice1, t.effectbasedice2, t.effectbasedice3][i] as i32;
    let base = current
        .checked_sub(dice)
        .ok_or_else(|| refuse("execution.spells.Object_2927.base_points_sub_undefined"))?;
    let mut base = Single::from_i128(i128::from(base)).value;
    let mut random_points = [t.effectdiesides1, t.effectdiesides2, t.effectdiesides3][i];
    let points_per_level = Single::from_bits(u128::from(
        [
            t.effectrealpointsperlevel1,
            t.effectrealpointsperlevel2,
            t.effectrealpointsperlevel3,
        ][i]
            .0,
    ));
    if !points_per_level.is_zero() {
        require(
            !caster_is_player,
            "execution.spells.Player_5885.GetSpellRank_owner",
        )?;
        let caster_level = caster_level
            .ok_or_else(|| refuse("execution.spells.Object_2934.effect_caster_level_owner"))?;
        let mut level = (creature_power_rank(caster_level, t) / 5) as i32;
        if level > t.maxlevel as i32 && t.maxlevel > 0 {
            level = t.maxlevel as i32;
        } else if level < t.baselevel as i32 {
            level = t.baselevel as i32;
        }
        let level = level
            .checked_sub(t.spelllevel as i32)
            .ok_or_else(|| refuse("execution.spells.Object_2939.effect_level_sub_undefined"))?;
        let level = Single::from_i128(i128::from(level)).value;
        // The recorded build uses separate binary32 multiply and add at :2940.
        base = (base + (level * points_per_level).value).value;
        let dice_per_level = Single::from_bits(u128::from(
            [
                t.effectdiceperlevel1,
                t.effectdiceperlevel2,
                t.effectdiceperlevel3,
            ][i]
                .0,
        ));
        let extra_dice = effect_int(
            (level * dice_per_level).value,
            "execution.spells.Object_2941.effect_dice_float_to_int_undefined",
        )?;
        random_points = random_points
            .checked_add(extra_dice)
            .ok_or_else(|| refuse("execution.spells.Object_2941.effect_dice_add_undefined"))?;
    }
    require(
        matches!(random_points, 0 | 1),
        "execution.spells.Object_2955.effect_dice_rng_owner",
    )?;
    let value = (base + Single::from_i128(i128::from(dice)).value).value;
    // Object.cpp:3014-3024. Stop before unowned scaling or an undefined cast.
    require(
        t.attributes & 0x0008_0000 == 0
            || t.spelllevel == 0
            || !matches!(t.effects()[i].aura, 3 | 53 | 69 | 162 | 64),
        "execution.spells.Object_3014.GetCreatureClassLvlStats_owner",
    )?;
    effect_int(
        value,
        "execution.spells.Object_3024.effect_float_to_int_undefined",
    )
}

fn effect_int(value: Single, id: &str) -> Result<i32> {
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    require(!converted.status.contains(Status::INVALID_OP), id)?;
    Ok(converted.value as i32)
}

#[test]
fn fixed_effect_value_checks_signed_subtraction_before_float_conversion() {
    let t = SpellTemplate {
        effectbasedice1: 1,
        effectdiesides1: 1,
        ..Default::default()
    };
    assert_eq!(
        effect_value(&t, 0, i32::MIN + 1, None, false).unwrap(),
        i32::MIN
    );
    assert_eq!(
        effect_value(&t, 0, i32::MIN, None, false).unwrap_err().id,
        "execution.spells.Object_2927.base_points_sub_undefined"
    );
    assert_eq!(
        effect_value(&t, 0, 2_147_483_520, None, false).unwrap(),
        2_147_483_520
    );
    assert_eq!(
        effect_value(&t, 0, i32::MAX, None, false).unwrap_err().id,
        "execution.spells.Object_3024.effect_float_to_int_undefined"
    );
}

#[test]
fn negative_zero_points_skip_level_and_dice_scaling() {
    let t = SpellTemplate {
        effectbasedice1: 1,
        effectdiesides1: 1,
        effectrealpointsperlevel1: SpellFloat((-0.0f32).to_bits()),
        effectdiceperlevel1: SpellFloat(10.0f32.to_bits()),
        ..Default::default()
    };
    assert_eq!(effect_value(&t, 0, 13, None, false).unwrap(), 13);
}

#[test]
fn frost_armor_scales_from_the_live_creature_rank() {
    // Stock SQL :847350. The cast initializes current to 11 + 1.
    let t = SpellTemplate {
        spelllevel: 1,
        baselevel: 1,
        effectbasedice1: 1,
        effectdiesides1: 1,
        effectrealpointsperlevel1: SpellFloat(12.0f32.to_bits()),
        ..Default::default()
    };
    // Trace :61221284 selects 16, giving rank / 5 = 3 and adjusted level 2.
    assert_eq!(effect_value(&t, 0, 12, Some(16), false).unwrap(), 36);
    assert_eq!(2.0f32.mul_add(12.0, 11.0) + 1.0, 36.0);
    assert_eq!(effect_value(&t, 0, 12, Some(4), false).unwrap(), 12);
    let capped = SpellTemplate { maxlevel: 2, ..t };
    assert_eq!(effect_value(&capped, 0, 12, Some(16), false).unwrap(), 24);
    assert_eq!(
        effect_value(&capped, 0, 12, Some(16), true).unwrap_err().id,
        "execution.spells.Player_5885.GetSpellRank_owner"
    );
    assert_eq!(
        effect_value(&capped, 0, 12, None, false).unwrap_err().id,
        "execution.spells.Object_2934.effect_caster_level_owner"
    );
}

#[test]
fn effect_points_round_product_before_addition() {
    let t = SpellTemplate {
        effectrealpointsperlevel1: SpellFloat(16_777_215.0f32.to_bits()),
        ..Default::default()
    };
    // 3 * 16,777,215 rounds down to 50,331,644 before cancellation.
    assert_eq!(
        effect_value(&t, 0, -50_331_644, Some(15), false).unwrap(),
        0
    );
    assert_eq!(3.0f32.mul_add(16_777_215.0, -50_331_644.0), 1.0);
}

#[test]
fn effect_level_checks_signed_delta_at_both_limits() {
    let mut t = SpellTemplate {
        baselevel: i32::MAX as u32 - 1,
        spelllevel: u32::MAX,
        effectrealpointsperlevel1: SpellFloat(2.0f32.powi(-31).to_bits()),
        ..Default::default()
    };
    assert_eq!(effect_value(&t, 0, 0, Some(1), false).unwrap(), 1);
    t.baselevel += 1;
    let undefined = "execution.spells.Object_2939.effect_level_sub_undefined";
    assert_eq!(
        effect_value(&t, 0, 0, Some(1), false).unwrap_err().id,
        undefined
    );

    // The signed max clamp wins before the base clamp, even with baseLevel > maxLevel.
    t.maxlevel = i32::MIN as u32;
    t.spelllevel = 0;
    assert_eq!(effect_value(&t, 0, 0, Some(1), false).unwrap(), -1);
    t.spelllevel = 1;
    assert_eq!(
        effect_value(&t, 0, 0, Some(1), false).unwrap_err().id,
        undefined
    );
}

#[test]
fn effect_dice_conversion_and_addition_check_both_limits() {
    let mut t = SpellTemplate {
        effectrealpointsperlevel1: SpellFloat(1.0f32.to_bits()),
        ..Default::default()
    };
    let rng = "execution.spells.Object_2955.effect_dice_rng_owner";
    let conversion = "execution.spells.Object_2941.effect_dice_float_to_int_undefined";
    let addition = "execution.spells.Object_2941.effect_dice_add_undefined";
    let cases = [
        (2_147_483_520.0, -2_147_483_520, None),
        (2_147_483_648.0, -2_147_483_520, Some(conversion)),
        (-2_147_483_648.0, i32::MAX, Some(rng)),
        (-2_147_483_904.0, i32::MAX, Some(conversion)),
        (1.0, i32::MAX - 1, Some(rng)),
        (1.0, i32::MAX, Some(addition)),
        (-1.0, i32::MIN + 1, Some(rng)),
        (-1.0, i32::MIN, Some(addition)),
    ];
    for (coefficient, sides, error) in cases {
        t.effectdiceperlevel1 = SpellFloat(f32::to_bits(coefficient));
        t.effectdiesides1 = sides;
        let result = effect_value(&t, 0, 0, Some(5), false);
        if let Some(error) = error {
            assert_eq!(result.unwrap_err().id, error);
        } else {
            assert_eq!(result.unwrap(), 1);
        }
    }
}

#[test]
fn scaled_dice_select_rng_before_the_class_level_gate() {
    let mut t = SpellTemplate {
        attributes: 0x0008_0000,
        spelllevel: 1,
        effectapplyauraname1: 3,
        effectdiesides1: 1,
        effectrealpointsperlevel1: SpellFloat(1.0f32.to_bits()),
        effectdiceperlevel1: SpellFloat(0.75f32.to_bits()),
        ..Default::default()
    };
    assert_eq!(
        effect_value(&t, 0, 0, Some(15), false).unwrap_err().id,
        "execution.spells.Object_2955.effect_dice_rng_owner"
    );
    // Adjusted level 1 truncates 0.75 to zero, leaving the fixed die.
    assert_eq!(
        effect_value(&t, 0, 0, Some(10), false).unwrap_err().id,
        "execution.spells.Object_3014.GetCreatureClassLvlStats_owner"
    );
    // Conversely, a non-fixed die can become fixed after scaling.
    t.attributes = 0;
    t.effectdiesides1 = 2;
    t.effectdiceperlevel1 = SpellFloat((-1.0f32).to_bits());
    assert_eq!(effect_value(&t, 0, 0, Some(10), false).unwrap(), 1);
}
