// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn resistance_aura_and_removal_write_the_live_armor_field() {
    let mut state = SourceUnitResistances::default();
    // SQL:448365 and :427381: 2,255 raw armor, 65 agility, multiplier 1.
    state.select_level(2_125.0, 65).unwrap();
    assert_eq!(state.armor(), 2_255);
    state.handle_aura(505, 1, true, true, false).unwrap(); // SQL:838502.
    assert_eq!(state.armor(), 2_760);
    assert_eq!(state.groups[0].total_value, 505.0);
    state.handle_aura(505, 1, false, true, false).unwrap();
    assert_eq!(state.armor(), 2_255);
    assert_eq!(state.groups[0].total_value, 0.0);
}

#[test]
fn disabled_stat_updates_keep_the_modifier_and_leave_armor_untouched() {
    let mut state = SourceUnitResistances::default();
    state.select_level(100.0, 30).unwrap();
    state.handle_aura(70, 1, true, false, false).unwrap();
    assert_eq!(state.armor(), 160);
    assert_eq!(state.groups[0].total_value, 70.0);
    state.update_armor().unwrap();
    assert_eq!(state.armor(), 230);
}

#[test]
fn resistance_create_words_preserve_negative_vulnerability() {
    let mut state = SourceUnitResistances::default();
    state.set_creature_resistances([(-12i32) as u32, 0, 1, 2, 3, 4]);
    assert_eq!(state.create_nonphysical(), [(-12i32) as u32, 0, 1, 2, 3, 4]);
    assert_eq!(state.create[0], 0);
    state.handle_aura(-4, 0b110, true, false, false).unwrap();
    assert_eq!(state.total(1), -16.0);
    assert_eq!(state.total(2), 0.0);
    assert_eq!(state.total(3), 1.0);
}

#[test]
fn stat_modifier_percent_branches_apply_the_source_factors() {
    let mut state = SourceUnitResistances::default();
    let group = &mut state.groups[0];
    group.modify(ResistanceModifier::BaseValue, 18.0, true);
    group.modify(ResistanceModifier::BasePercent, 50.0, true);
    group.modify(ResistanceModifier::BaseValue, 6.0, false);
    group.modify(ResistanceModifier::TotalValue, 6.0, true);
    group.modify(ResistanceModifier::TotalPercent, -100.0, true);
    assert_eq!(state.total(0), 0.0);
    state.groups[0].modify(ResistanceModifier::TotalPercent, -100.0, false);
    assert_eq!(state.total(0), 24.0);
}

#[test]
fn stat_refusals_run_after_the_total_modifier_write() {
    for (mask, can_modify, player, school, next) in [
        (
            2,
            true,
            false,
            1,
            "execution.spells.Unit_9033.UpdateResistances_owner",
        ),
        (
            1,
            true,
            true,
            0,
            "execution.spells.Unit_9019.Player_UpdateArmor_owner",
        ),
        (
            1,
            false,
            true,
            0,
            "execution.spells.Aura_3384.ApplyResistanceBuffModsMod_owner",
        ),
    ] {
        let mut state = SourceUnitResistances::default();
        state
            .handle_aura(0, mask, true, can_modify, player)
            .unwrap();
        assert_eq!(state.groups[school].total_value, 0.0);
        assert_eq!(
            state
                .handle_aura(19, mask, true, can_modify, player)
                .unwrap_err()
                .id,
            next
        );
        assert_eq!(state.groups[school].total_value, 19.0);
        assert_eq!(state.armor(), 0);
    }
}

#[test]
fn armor_dynamic_bonus_uses_two_f32_writes() {
    let mut state = SourceUnitResistances::default();
    state.select_level(1.0, 1).unwrap();
    state.groups[0].total_value = 33_554_432.0;
    state.update_armor().unwrap();
    assert_eq!(state.armor(), 33_554_432);
    assert_eq!(state.groups[0].total_value, 33_554_430.0);
}

#[test]
fn signed_armor_conversion_refuses_on_both_sides_of_the_defined_range() {
    assert_eq!(armor_int(-2_147_483_648.0).unwrap(), i32::MIN);
    assert_eq!(armor_int(2_147_483_520.0).unwrap(), 2_147_483_520);
    assert_eq!(armor_int(1.75).unwrap(), 1);
    for value in [2_147_483_648.0, -2_147_483_904.0, f32::NAN] {
        assert_eq!(
            armor_int(value).unwrap_err().id,
            "execution.spells.StatSystem_669.armor_float_to_int_undefined"
        );
    }
}
