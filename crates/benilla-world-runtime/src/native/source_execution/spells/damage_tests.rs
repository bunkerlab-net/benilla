// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn creature_weapon_damage_tracks_disarm_dual_wield_and_form() {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut equipment = SourceCreatureEquipment::default();
    let mut damage = SourceCreatureDamage {
        base_ap: [14.0, 28.0],
        ..Default::default()
    };
    let normal = [0x4000_0000, 0x4040_0000];
    let ranged = [0x4040_0000, 0x4080_0000];
    let halved = [0x3f80_0000, 0x3fc0_0000];
    let ap_only = [0x3f00_0000; 2];
    let update = |state: &mut SourceCreatureDamage, weapon: CreatureWeaponState<'_>| {
        state
            .update_attack_power_and_damage(&objects, unit, 1, 0, weapon, false)
            .unwrap();
        state
            .update_attack_power_and_damage(&objects, unit, 1, 0, weapon, true)
            .unwrap();
        state.damage_bits()
    };
    // Unit.cpp:9190-9195 takes zero weapon damage only for unavailable offhand.
    let weapon = CreatureWeaponState {
        equipment: &equipment,
        can_dual_wield: false,
        flags: 0,
        form: 0,
    };
    assert_eq!(update(&mut damage, weapon), [normal, ap_only, ranged]);
    assert_eq!(
        update(
            &mut damage,
            CreatureWeaponState {
                can_dual_wield: true,
                ..weapon
            },
        ),
        [normal, halved, ranged],
    );
    equipment.slots.iter_mut().for_each(|item| item.info[0] = 2);
    // Unit.h:1191-1204 disarms the main hand only. StatSystem.cpp:803-804
    // halves an existing weapon's damage when its attack is unavailable.
    assert_eq!(
        update(
            &mut damage,
            CreatureWeaponState {
                equipment: &equipment,
                can_dual_wield: false,
                flags: 0x0020_0000,
                form: 0,
            },
        ),
        [halved, halved, ranged],
    );
    for form in [1, 5, 8] {
        // Cat, bear and dire bear block all weapons (Unit.cpp:10889-10904).
        // Offhand first loses weapon damage, then halves its AP contribution.
        assert_eq!(
            update(
                &mut damage,
                CreatureWeaponState {
                    equipment: &equipment,
                    can_dual_wield: false,
                    flags: 0,
                    form,
                },
            ),
            [halved, [0x3e80_0000; 2], [0x3fc0_0000, 0x4000_0000]],
        );
    }
}

#[test]
fn total_attack_power_uses_signed_halves_only_for_melee() {
    let mut state = SourceCreatureDamage {
        ap: [100, 131_100],
        ap_mod_words: [0xfffe_000a; 2],
        ap_multipliers: [0.125, 0.25],
        ..Default::default()
    };
    // Unit.cpp:9179 adds -131062, while :9184 adds +10 and -2.
    assert_eq!(state.total_attack_power(0).unwrap().to_bits(), 0x42f3_0000);
    assert_eq!(state.total_attack_power(1).unwrap().to_bits(), 0x42f3_0000);
    assert_eq!(state.total_attack_power(2).unwrap().to_bits(), 0x423e_0000);
    state.ap = [-9, 100];
    assert_eq!(state.total_attack_power(0).unwrap().to_bits(), 0);
    assert_eq!(state.total_attack_power(2).unwrap().to_bits(), 0);

    // The subsequent negative half does not make the first signed add valid.
    state.ap = [i32::MAX; 2];
    state.ap_mod_words = [0xffff_0001, 1];
    assert_eq!(
        state.total_attack_power(0).unwrap_err().id,
        "execution.spells.Unit_9184.melee_attack_power_add_undefined",
    );
    assert_eq!(
        state.total_attack_power(2).unwrap_err().id,
        "execution.spells.Unit_9179.ranged_attack_power_add_undefined",
    );
}
