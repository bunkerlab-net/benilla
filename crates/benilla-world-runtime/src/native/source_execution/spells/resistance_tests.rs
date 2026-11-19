// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn subject(in_world: bool) -> (SourceObjects, ObjectHandle, MapIdentity) {
    let map = MapIdentity {
        map: 1,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let mut objects = SourceObjects::default();
    let unit = objects.construct(7, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(
            unit,
            41,
            SourceGuid::with_entry(HighGuid::Creature, 901, 41),
        )
        .unwrap();
    objects.bind_map(unit, map, &cells).unwrap();
    if in_world {
        objects.add_to_world(unit).unwrap();
    }
    (objects, unit, map)
}

#[test]
fn client_update_armor_aura_marks_only_a_changed_clamped_value() {
    for (in_world, amount, armor) in [(true, -505, 0), (true, 505, 505), (false, 505, 505)] {
        let (objects, unit, map) = subject(in_world);
        let mut state = SourceUnitResistances::default();
        state
            .handle_aura(&objects, unit, amount, 1, true, true, false)
            .unwrap();
        assert_eq!(state.armor(), armor);
        let changed = armor != 0;
        assert_eq!(
            objects
                .get(unit)
                .unwrap()
                .field_changed(UNIT_FIELD_RESISTANCES)
                .unwrap(),
            changed
        );
        assert_eq!(
            objects.client_update_pending(map, unit),
            in_world && changed
        );
    }
}

#[test]
fn resistance_aura_and_removal_write_the_live_armor_field() {
    let (objects, unit, _) = subject(false);
    let mut state = SourceUnitResistances::default();
    // SQL:448365 and :427381: 2,255 raw armor, 65 agility, multiplier 1.
    state.select_level(&objects, unit, 2_125.0, 65).unwrap();
    assert_eq!(state.armor(), 2_255);
    state
        .handle_aura(&objects, unit, 505, 1, true, true, false)
        .unwrap(); // SQL:838502.
    assert_eq!(state.armor(), 2_760);
    assert_eq!(state.groups[0].total_value, 505.0);
    state
        .handle_aura(&objects, unit, 505, 1, false, true, false)
        .unwrap();
    assert_eq!(state.armor(), 2_255);
    assert_eq!(state.groups[0].total_value, 0.0);
}

#[test]
fn disabled_stat_updates_keep_the_modifier_and_leave_armor_untouched() {
    let (objects, unit, _) = subject(false);
    let mut state = SourceUnitResistances::default();
    state.select_level(&objects, unit, 100.0, 30).unwrap();
    state
        .handle_aura(&objects, unit, 70, 1, true, false, false)
        .unwrap();
    assert_eq!(state.armor(), 160);
    assert_eq!(state.groups[0].total_value, 70.0);
    state.update_armor(&objects, unit).unwrap();
    assert_eq!(state.armor(), 230);
}

#[test]
fn resistance_create_words_preserve_negative_vulnerability() {
    let (objects, unit, _) = subject(false);
    let mut state = SourceUnitResistances::default();
    state.set_creature_resistances([(-12i32) as u32, 0, 1, 2, 3, 4]);
    assert_eq!(state.create_nonphysical(), [(-12i32) as u32, 0, 1, 2, 3, 4]);
    assert_eq!(state.create[0], 0);
    state
        .handle_aura(&objects, unit, -4, 0b110, true, false, false)
        .unwrap();
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
    let (objects, unit, _) = subject(false);
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
            .handle_aura(&objects, unit, 0, mask, true, can_modify, player)
            .unwrap();
        assert_eq!(state.groups[school].total_value, 0.0);
        assert_eq!(
            state
                .handle_aura(&objects, unit, 19, mask, true, can_modify, player)
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
    let (objects, unit, _) = subject(false);
    let mut state = SourceUnitResistances::default();
    state.select_level(&objects, unit, 1.0, 1).unwrap();
    state.groups[0].total_value = 33_554_432.0;
    state.update_armor(&objects, unit).unwrap();
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
