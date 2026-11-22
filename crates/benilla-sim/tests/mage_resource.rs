// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic probes, never native character/DBC rows or fabricated starter stats.
use benilla_sim::classic_spell::*;
use benilla_sim::mage_resource::*;

fn float(bits: u32) -> SpellFloat {
    SpellFloat::from_bits(bits).unwrap()
}
fn crit_modifiers() -> SpellCritModifiers {
    SpellCritModifiers {
        all_schools: 0,
        school: 0,
    }
}
fn regen_modifiers() -> ManaRegenModifiers {
    ManaRegenModifiers {
        spirit_multiplier: SpellFloat::ONE,
        flat_mana_per_five: 0,
        interrupted_percent: 0,
    }
}
fn frame(diff_ms: u32) -> ManaFrame {
    ManaFrame {
        diff_ms,
        in_world: true,
        alive_after_events: true,
        world_mana_rate: SpellFloat::ONE,
        exclusions: ManaUseExclusions {
            cast_item: false,
            triggered_by_aura: false,
            dont_block_mana_regen: false,
            other_mana_mutation: false,
        },
    }
}
fn state(mana: u32, regen_elapsed: u32, mana_timer: u32) -> MageManaState {
    MageManaState::from_state(
        CasterResources::new(mana).unwrap(),
        1000,
        update_mana_regen(20, regen_modifiers()).unwrap(),
        regen_elapsed,
        mana_timer,
    )
    .unwrap()
}
fn skill(id: u16, value: u16) -> CurrentSkill {
    CurrentSkill {
        id,
        value,
        permanent_bonus: 0,
        temporary_bonus: 0,
        deleted: false,
    }
}
#[test]
fn pinned_cpp_numeric_and_state_oracle() {
    let mut sequences = state(100, 0, 0);
    sequences
        .set_regen(
            update_mana_regen(
                20,
                ManaRegenModifiers {
                    flat_mana_per_five: 5,
                    ..regen_modifiers()
                },
            )
            .unwrap(),
        )
        .unwrap();
    let mut count = 0;
    for line in include_str!("fixtures/mage_resource_cmangos.tsv").lines() {
        let mut words = line.split_whitespace();
        let mode = words.next().unwrap();
        let v: Vec<i64> = words.map(|s| s.parse().unwrap()).collect();
        match mode {
            "C" => assert_eq!(
                mage_spell_crit(v[0] as u32, v[1] as u32, crit_modifiers())
                    .unwrap()
                    .bits(),
                v[2] as u32,
                "{line}"
            ),
            "S" => assert_eq!(
                mage_spirit_regen(v[0] as u32).unwrap().bits(),
                v[1] as u32,
                "{line}"
            ),
            "U" => {
                let regen = update_mana_regen(
                    v[0] as u32,
                    ManaRegenModifiers {
                        spirit_multiplier: float(v[1] as u32),
                        flat_mana_per_five: v[2] as i32,
                        interrupted_percent: v[3] as i32,
                    },
                )
                .unwrap();
                assert_eq!(
                    (
                        regen.normal_per_second.bits(),
                        regen.interrupted_per_second.bits()
                    ),
                    (v[4] as u32, v[5] as u32),
                    "{line}"
                );
            }
            "D" => assert_eq!(
                regeneration_seconds(v[0] as u32).unwrap(),
                v[1] as u32,
                "{line}"
            ),
            "R" => assert_eq!(
                regenerate_mana(
                    7,
                    1_000_000,
                    v[0] as u32,
                    v[2] != 0,
                    ManaRegen {
                        normal_per_second: float(0x3e00_0000),
                        interrupted_per_second: float(0x3c23_d70a)
                    },
                    float(v[1] as u32)
                )
                .unwrap(),
                v[3] as u32,
                "{line}"
            ),
            "K" => {
                let mut rows = Vec::new();
                if v[0] == 2 {
                    rows.push(SkillLineAbilityRow {
                        row_id: 9,
                        spell_id: 900001,
                        skill_id: 8,
                    });
                }
                if v[0] != 0 {
                    rows.push(SkillLineAbilityRow {
                        row_id: 2,
                        spell_id: 900001,
                        skill_id: 7,
                    });
                }
                let mut skills = vec![skill(8, 999)];
                if v[1] != 0 {
                    skills.push(CurrentSkill {
                        id: 7,
                        value: v[2] as u16,
                        permanent_bonus: v[3] as i16,
                        temporary_bonus: v[3] as i16,
                        deleted: v[1] == 2,
                    });
                }
                assert_eq!(
                    spell_rank(
                        900001,
                        v[4] as u32,
                        &SkillLineAbilities::new(rows).unwrap(),
                        &CurrentSkills::new(skills).unwrap()
                    )
                    .unwrap(),
                    v[5] as u32,
                    "{line}"
                );
            }
            "B" => assert_eq!(
                base_magic_miss(if v[0] == 0 && v[1] != 0 {
                    MagicMissTarget::CreatureTotem
                } else {
                    MagicMissTarget::Ordinary
                })
                .bits(),
                v[2] as u32,
                "{line}"
            ),
            "F" => {
                let mut f = frame(v[0] as u32);
                f.alive_after_events = v[2] != 0;
                let events = if v[1] != 0 {
                    sequences.caster.mana -= 10;
                    vec![SpellEvent::ManaSpent(10)]
                } else {
                    vec![]
                };
                let result = sequences.after_spell_and_aura_events(f, events).unwrap();
                assert_eq!(
                    (
                        sequences.caster.mana,
                        sequences.regen_elapsed_ms(),
                        sequences.last_mana_use_timer_ms(),
                        result.regenerated_diff_ms.unwrap_or(0)
                    ),
                    (v[3] as u32, v[4] as u32, v[5] as u32, v[6] as u32),
                    "{line}"
                );
            }
            _ => panic!("unknown oracle row: {line}"),
        }
        count += 1;
    }
    assert_eq!(count, 2082);
}
#[test]
fn first_mapping_and_current_skill_are_authoritative_even_if_zero() {
    let rows = SkillLineAbilities::new(vec![
        SkillLineAbilityRow {
            row_id: 40,
            spell_id: 900001,
            skill_id: 9,
        },
        SkillLineAbilityRow {
            row_id: 2,
            spell_id: 900001,
            skill_id: 7,
        },
    ])
    .unwrap();
    let skills = CurrentSkills::new(vec![skill(9, 100)]).unwrap();
    assert_eq!(spell_rank(900001, 0, &rows, &skills).unwrap(), 0);
    assert_eq!(spell_rank(900002, 0, &rows, &skills).unwrap(), 0);
    assert_eq!(rows.first(900001).unwrap().skill_id, 7);
    let skills = CurrentSkills::new(vec![CurrentSkill {
        value: 6,
        permanent_bonus: 4,
        temporary_bonus: 3,
        ..skill(7, 0)
    }])
    .unwrap();
    assert_eq!(spell_rank(900001, 0, &rows, &skills).unwrap(), 13);
    assert_eq!(spell_rank(900001, 2, &rows, &skills).unwrap(), 10);
}
#[test]
fn skill_narrowing_and_invalid_catalogs_are_explicit() {
    let rows = SkillLineAbilities::new(vec![SkillLineAbilityRow {
        row_id: 0,
        spell_id: 1,
        skill_id: 65537,
    }])
    .unwrap();
    let skills = CurrentSkills::new(vec![skill(1, 17), skill(0, 100)]).unwrap();
    assert_eq!(skills.value(0), 0);
    assert_eq!(spell_rank(1, 0, &rows, &skills).unwrap(), 17);
    assert!(spell_rank(1, u32::MAX, &rows, &skills).is_err());
    assert!(CurrentSkills::new(vec![skill(1, 1), skill(1, 2)]).is_err());
    let row = SkillLineAbilityRow {
        row_id: 2,
        spell_id: 1,
        skill_id: 1,
    };
    assert!(SkillLineAbilities::new(vec![row, row]).is_err());
}
#[test]
fn source_float_diff_conversion_is_not_integer_division() {
    assert_eq!(regeneration_seconds(16_777_999).unwrap(), 16_778);
    assert_eq!(16_777_999u32 / 1000, 16_777);
    assert_eq!(regeneration_seconds(u32::MAX).unwrap(), 4_294_967);
}
#[test]
fn regen_has_no_fractional_carry_and_uses_upper_only_interrupt_clamp() {
    let regen = ManaRegen {
        normal_per_second: float(0x3e00_0000),
        interrupted_per_second: SpellFloat::ZERO,
    };
    let mut mana = 0;
    for _ in 0..8 {
        mana = regenerate_mana(mana, 1000, 2000, false, regen, SpellFloat::ONE).unwrap();
    }
    assert_eq!(mana, 0);
    assert_eq!(
        regenerate_mana(0, 1000, 16000, false, regen, SpellFloat::ONE).unwrap(),
        2
    );
    let hundred = update_mana_regen(
        20,
        ManaRegenModifiers {
            interrupted_percent: 100,
            ..regen_modifiers()
        },
    )
    .unwrap();
    let above = update_mana_regen(
        20,
        ManaRegenModifiers {
            interrupted_percent: 101,
            ..regen_modifiers()
        },
    )
    .unwrap();
    assert_eq!(hundred, above);
    assert!(update_mana_regen(
        20,
        ManaRegenModifiers {
            interrupted_percent: -1,
            ..regen_modifiers()
        }
    )
    .is_err());
    let negative = update_mana_regen(
        20,
        ManaRegenModifiers {
            flat_mana_per_five: 100,
            interrupted_percent: -1,
            ..regen_modifiers()
        },
    )
    .unwrap();
    let zero = update_mana_regen(
        20,
        ManaRegenModifiers {
            flat_mana_per_five: 100,
            ..regen_modifiers()
        },
    )
    .unwrap();
    assert!(negative.interrupted_per_second.bits() < zero.interrupted_per_second.bits());
}
#[test]
fn zero_cost_failure_death_and_out_of_world_do_not_manufacture_mana_use() {
    let mut s = state(100, 1999, 0);
    s.after_spell_and_aura_events(
        frame(1),
        vec![
            SpellEvent::ManaSpent(0),
            SpellEvent::Failed(CastFailure::NoPower),
        ],
    )
    .unwrap();
    assert_eq!(s.caster.mana, 117);
    assert_eq!(s.last_mana_use_timer_ms(), 0);
    let mut dead = frame(2000);
    dead.alive_after_events = false;
    let mut s = state(100, 1999, 5000);
    s.after_spell_and_aura_events(dead, vec![]).unwrap();
    assert_eq!(
        (
            s.caster.mana,
            s.regen_elapsed_ms(),
            s.last_mana_use_timer_ms()
        ),
        (100, 1999, 3000)
    );
    let mut outside = frame(9000);
    outside.in_world = false;
    let before = s.clone();
    s.after_spell_and_aura_events(outside, vec![]).unwrap();
    assert_eq!(s, before);
    assert!(s
        .after_spell_and_aura_events(outside, vec![SpellEvent::ManaSpent(1)])
        .is_err());
    assert_eq!(s, before);
}
#[test]
fn event_reset_precedes_same_diff_decrement_and_rounded_regen_schedule() {
    let mut s = state(100, 0, 0);
    // A 5s launch frame expires its newly reset timer before regeneration.
    let result = s
        .after_spell_and_aura_events(frame(5099), vec![SpellEvent::ManaSpent(10)])
        .unwrap();
    assert_eq!(
        (
            s.last_mana_use_timer_ms(),
            s.regen_elapsed_ms(),
            s.caster.mana
        ),
        (0, 99, 143)
    );
    assert_eq!(result.regenerated_diff_ms, Some(5000));
    // Must use 100ms-rounded accumulated diff, then float division/truncation.
    let result = s.after_spell_and_aura_events(frame(2001), vec![]).unwrap();
    assert_eq!(
        (
            result.regenerated_diff_ms,
            s.regen_elapsed_ms(),
            s.caster.mana
        ),
        (Some(2100), 0, 160)
    );
    // Same timestamp does not identify whether a second cost occurred.
    s.caster.last_mana_use_ms = Some(7000);
    s.after_spell_and_aura_events(frame(0), vec![SpellEvent::ManaSpent(1)])
        .unwrap();
    s.after_spell_and_aura_events(frame(1000), vec![]).unwrap();
    assert_eq!(s.last_mana_use_timer_ms(), 4000);
    s.after_spell_and_aura_events(frame(0), vec![SpellEvent::ManaSpent(1)])
        .unwrap();
    assert_eq!(s.last_mana_use_timer_ms(), 5000);
}
#[test]
fn supported_domains_and_exclusion_failures_are_atomic() {
    assert!(mage_spell_crit(0, 20, crit_modifiers()).is_err());
    assert!(mage_spell_crit(61, 20, crit_modifiers()).is_err());
    assert!(mage_spell_crit(1, 1_000_001, crit_modifiers()).is_err());
    assert!(mage_spell_crit(
        1,
        20,
        SpellCritModifiers {
            all_schools: 1,
            school: 0
        }
    )
    .is_err());
    assert!(mage_spell_crit(
        1,
        20,
        SpellCritModifiers {
            all_schools: 0,
            school: -1
        }
    )
    .is_err());
    assert!(mage_spirit_regen(1_000_001).is_err());
    assert!(update_mana_regen(
        20,
        ManaRegenModifiers {
            spirit_multiplier: float(0xbf800000),
            ..regen_modifiers()
        }
    )
    .is_err());
    let mut s = state(100, 1, 5);
    let before = s.clone();
    for i in 0..4 {
        let mut f = frame(2000);
        match i {
            0 => f.exclusions.cast_item = true,
            1 => f.exclusions.triggered_by_aura = true,
            2 => f.exclusions.dont_block_mana_regen = true,
            _ => f.exclusions.other_mana_mutation = true,
        }
        assert!(s
            .after_spell_and_aura_events(f, vec![SpellEvent::ManaSpent(1)])
            .is_err());
        assert_eq!(s, before);
    }
    assert!(s
        .after_spell_and_aura_events(frame(u32::MAX), vec![])
        .is_err());
    assert_eq!(s, before);
    let mut f = frame(2000);
    f.world_mana_rate = float(0xbf800000);
    assert!(s.after_spell_and_aura_events(f, vec![]).is_err());
    assert_eq!(s, before);
    let too_large = ManaRegen {
        normal_per_second: float(0x49742400),
        interrupted_per_second: SpellFloat::ZERO,
    };
    assert!(regenerate_mana(0, 1000, u32::MAX, false, too_large, SpellFloat::ONE).is_err());
    assert!(MageManaState::from_state(
        CasterResources::new(10).unwrap(),
        9,
        update_mana_regen(20, regen_modifiers()).unwrap(),
        0,
        0
    )
    .is_err());
}
#[test]
fn real_spell_component_cost_crit_and_mana_adapter_compose() {
    let mappings = SkillLineAbilities::new(vec![SkillLineAbilityRow {
        row_id: 1,
        spell_id: 900001,
        skill_id: 7,
    }])
    .unwrap();
    let skills = CurrentSkills::new(vec![skill(7, 5)]).unwrap();
    let rank = u16::try_from(spell_rank(900001, 0, &mappings, &skills).unwrap()).unwrap();
    let spec = MageProjectileSpec {
        spell_id: 900001,
        effects: [
            MageEffect::SchoolDamage(EffectValue {
                base_points: 9,
                base_dice: 1,
                die_sides: 1,
                points_per_level: SpellFloat::ZERO,
                dice_per_level: SpellFloat::ZERO,
                base_level: 1,
                spell_level: 1,
                max_level: 0,
            }),
            MageEffect::Empty,
            MageEffect::Empty,
        ],
        spell_rank: rank,
        cast_ms: 2000,
        mana_cost: mana_cost(10, 0, 0, 100, rank, 1).unwrap(),
        global_category: 133,
        global_recovery_ms: 1500,
        recovery_ms: 0,
        category: 0,
        category_recovery_ms: 0,
        projectile_speed: float(0x41a00000),
        movement_interrupts: true,
        damage_pushback: true,
    };
    let checks = CastChecks {
        caster_alive: true,
        target_exists: true,
        target_alive: true,
        position_changed: false,
        other_failure: None,
    };
    let mut s = state(100, 0, 0);
    let (mut cast, _) = MageProjectileCast::prepare(spec, 1, 2, 0, &mut s.caster, checks).unwrap();
    let crit = combat_chance(mage_spell_crit(1, 20, crit_modifiers()).unwrap()).unwrap();
    let miss = zero_resistance_npc_miss(1, 1, base_magic_miss(MagicMissTarget::Ordinary)).unwrap();
    let events = cast
        .update(
            2000,
            2000,
            &mut s.caster,
            checks,
            Some(LaunchContext {
                distance_squared: float(0x42c80000),
                resist_chance: miss,
                critical_chance: crit,
                rolls: LaunchRolls {
                    hit: 401,
                    direct: [None, None, None],
                    critical: Some(10000),
                },
            }),
        )
        .unwrap();
    assert_eq!(s.caster.mana, 90);
    assert_eq!(s.caster.last_mana_use_ms, Some(2000));
    s.after_spell_and_aura_events(frame(2000), events).unwrap();
    assert_eq!((s.caster.mana, s.last_mana_use_timer_ms()), (90, 3000));
    s.after_spell_and_aura_events(frame(2000), vec![]).unwrap();
    assert_eq!((s.caster.mana, s.last_mana_use_timer_ms()), (90, 1000));
    s.after_spell_and_aura_events(frame(2000), vec![]).unwrap();
    assert_eq!((s.caster.mana, s.last_mana_use_timer_ms()), (107, 0));
}
