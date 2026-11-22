// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn armor_and_chilled_remain_active_until_their_exact_expiry() {
    let mut armor = TimedAura::new(
        AuraLifecycleSpec::ArmorProc {
            armor: 37,
            trigger_spell: 6136,
        },
        11000,
    )
    .unwrap();
    assert_eq!(armor.armor(), 37);
    assert!(!armor.update(10999).unwrap().expired);
    assert!(armor.update(1).unwrap().expired);
    let mut chilled = TimedAura::new(
        AuraLifecycleSpec::Chilled {
            melee_haste: -20,
            movement_slow: -40,
        },
        3000,
    )
    .unwrap();
    assert_eq!(chilled.movement_rates().unwrap().run.bits(), 0x3f19999a);
    assert_eq!(chilled.attack_time(2000).unwrap(), 2400);
    assert!(!chilled.update(2999).unwrap().expired);
    assert!(chilled.update(1).unwrap().expired);
}

#[test]
fn serialized_chilled_keeps_duration_and_modifiers() {
    let mut aura = TimedAura::new(
        AuraLifecycleSpec::Chilled {
            melee_haste: -20,
            movement_slow: -40,
        },
        12000,
    )
    .unwrap();
    aura.update(3500).unwrap();
    let bytes = serde_json::to_vec(&aura).unwrap();
    let mut reopened: TimedAura = serde_json::from_slice(&bytes).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened, aura);
    reopened.update(1500).unwrap();
    assert_eq!(reopened.remaining_ms(), 7000);
    assert_eq!(reopened.attack_time(2000).unwrap(), 2400);
}

#[test]
fn lifecycle_rejects_unknown_proc_or_invalid_modifier_domains() {
    assert!(TimedAura::new(
        AuraLifecycleSpec::ArmorProc {
            armor: 1,
            trigger_spell: 999,
        },
        1000
    )
    .is_err());
    assert!(TimedAura::new(
        AuraLifecycleSpec::Chilled {
            melee_haste: 1,
            movement_slow: -40,
        },
        1000
    )
    .is_err());
    assert!(TimedAura::new(
        AuraLifecycleSpec::ArmorProc {
            armor: 1,
            trigger_spell: 6136
        },
        0
    )
    .is_err());
}

#[test]
fn expired_holders_supply_no_armor_or_speed_modifier() {
    for spec in [
        AuraLifecycleSpec::ArmorProc {
            armor: 37,
            trigger_spell: 6136,
        },
        AuraLifecycleSpec::Chilled {
            melee_haste: -20,
            movement_slow: -40,
        },
    ] {
        let mut aura = TimedAura::new(spec, 1000).unwrap();
        assert!(aura.update(1000).unwrap().expired);
        assert_eq!(aura.armor(), 0);
        assert_eq!(aura.movement_rates().unwrap(), SpeedRates::normal());
        assert_eq!(aura.attack_time(2000).unwrap(), 2000);
    }
}

#[test]
fn instant_aura_uses_existing_cooldown_maps_and_reopens_exactly() {
    use crate::classic_spell::{CastChecks, CastFailure, CasterResources, InstantAuraResources};
    let checks = CastChecks {
        caster_alive: true,
        target_exists: true,
        target_alive: true,
        position_changed: false,
        other_failure: None,
    };
    let spec = InstantAuraResources {
        spell_id: 900001,
        mana_cost: 10,
        global_category: 901,
        global_recovery_ms: 1500,
        recovery_ms: 4000,
        category: 902,
        category_recovery_ms: 6000,
    };
    let mut caster = CasterResources::new(100).unwrap();
    caster
        .cast_instant_aura(spec, 1000, checks)
        .unwrap()
        .unwrap();
    assert_eq!(caster.mana, 90);
    assert_eq!(caster.last_mana_use_ms, Some(1000));
    assert_eq!(caster.global_ready_at(901), Some(2500));
    assert_eq!(caster.spell_ready_at(900001), Some(5000));
    let before = caster.clone();
    let second = InstantAuraResources {
        spell_id: 900002,
        global_recovery_ms: 0,
        ..spec
    };
    assert_eq!(
        caster.cast_instant_aura(second, 3000, checks).unwrap(),
        Err(CastFailure::NotReady)
    );
    assert_eq!(caster, before);
    let checkpoint = caster.checkpoint();
    let reopened = CasterResources::restore(checkpoint.clone(), 1000, &[], &[spec]).unwrap();
    assert_eq!(reopened, caster);
    assert!(CasterResources::restore(checkpoint, 1000, &[], &[]).is_err());
}

#[test]
fn zero_cost_aura_does_not_reset_the_last_mana_use() {
    use crate::classic_spell::{CastChecks, CasterResources, InstantAuraResources};
    let checks = CastChecks {
        caster_alive: true,
        target_exists: true,
        target_alive: true,
        position_changed: false,
        other_failure: None,
    };
    let paid = InstantAuraResources {
        spell_id: 900001,
        mana_cost: 10,
        global_category: 901,
        global_recovery_ms: 0,
        recovery_ms: 0,
        category: 0,
        category_recovery_ms: 0,
    };
    let mut caster = CasterResources::new(100).unwrap();
    caster
        .cast_instant_aura(paid, 1000, checks)
        .unwrap()
        .unwrap();
    let item = InstantAuraResources {
        spell_id: 900002,
        mana_cost: 0,
        ..paid
    };
    caster
        .cast_instant_aura(item, 2000, checks)
        .unwrap()
        .unwrap();
    assert_eq!(caster.mana, 90);
    assert_eq!(caster.last_mana_use_ms, Some(1000));
    let before = caster.clone();
    let overflowing = InstantAuraResources {
        global_recovery_ms: 1500,
        ..item
    };
    assert!(caster
        .cast_instant_aura(overflowing, u64::MAX - 1, checks)
        .is_err());
    assert_eq!(caster, before);
}

#[test]
fn binary_chill_miss_excludes_the_nonbinary_level_resistance_term() {
    use crate::classic_spell::{
        zero_resistance_binary_npc_miss, zero_resistance_npc_miss, SpellFloat,
    };
    let base = SpellFloat::from_bits(0x40800000).unwrap();
    assert_eq!(zero_resistance_binary_npc_miss(20, 21, base).unwrap(), 500);
    assert!(zero_resistance_npc_miss(20, 21, base).is_err());
}

#[test]
fn melee_reset_requires_combat_flag_without_suppression_or_triggering() {
    use crate::classic_spell::resets_melee_timers;
    assert!(resets_melee_timers(false, 0x08, 0));
    assert!(!resets_melee_timers(false, 0, 0));
    assert!(!resets_melee_timers(false, 0x08, 0x0002_0000));
    assert!(!resets_melee_timers(true, 0x08, 0));
}
