// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::npc_melee::*;
fn sf(v: f32) -> SpellFloat {
    SpellFloat::from_bits(v.to_bits()).unwrap()
}
fn sample() -> MeleeInput {
    MeleeInput {
        attacker: CreatureAttack {
            level: 1,
            minimum_damage: sf(2.0),
            maximum_damage: sf(5.0),
            no_crushing_blows: false,
            creature_is_critter: false,
        },
        defender: MageDefense {
            level: 1,
            agility: 17,
            defense_skill: 1,
            armor: 20,
            standing: true,
            casting_non_melee: false,
            has_channeled_spell: false,
            has_auto_repeat_spell: false,
        },
        health: 30,
        front_facing: true,
    }
}
fn outcome(n: u32) -> MeleeOutcome {
    match n {
        1 => MeleeOutcome::Miss,
        2 => MeleeOutcome::Dodge,
        6 => MeleeOutcome::Critical,
        7 => MeleeOutcome::Crushing,
        8 => MeleeOutcome::Normal,
        _ => panic!("unsupported fixture outcome"),
    }
}
#[test]
fn pinned_cpp_differential() {
    let mut counts = [0; 4];
    let mut mage_levels = [false; 60];
    let mut crushing_flags = [false; 2];
    let mut outcome_counts = [0; 5];
    let mut die_residues = [false; 4];
    for (line, row) in include_str!("fixtures/npc_melee_cmangos.tsv")
        .lines()
        .enumerate()
    {
        let mut columns = row.split_whitespace();
        let kind = columns.next().unwrap();
        let p: Vec<i64> = columns.map(|v| v.parse().unwrap()).collect();
        match kind {
            "w" => {
                counts[3] += 1;
                let class = match p[0] {
                    1 => CreatureStatClass::Warrior,
                    2 => CreatureStatClass::Paladin,
                    4 => CreatureStatClass::Rogue,
                    8 => CreatureStatClass::Mage,
                    _ => panic!("class"),
                };
                let f = |n| SpellFloat::from_bits(n as u32).unwrap();
                let i = CreatureDamageInputs {
                    class,
                    strength: p[1] as u32,
                    agility: p[2] as u32,
                    create_strength: f(p[3]),
                    create_agility: f(p[4]),
                    base_attack_power: f(p[5]),
                    damage_multiplier: f(p[6]),
                    base_weapon_minimum: f(p[7]),
                    base_weapon_maximum: f(p[8]),
                    weapon_unusable: p[9] != 0,
                };
                let r = creature_damage_range(i).unwrap();
                assert_eq!(
                    (r.attack_power, r.minimum.bits(), r.maximum.bits()),
                    (p[10] as u32, p[11] as u32, p[12] as u32),
                    "line {line}"
                );
            }
            "d" => {
                counts[0] += 1;
                assert_eq!(
                    mage_base_dodge(p[0] as u8, p[1] as u32).unwrap().bits(),
                    p[2] as u32,
                    "line {line}"
                );
            }
            "s" => {
                counts[2] += 1;
                let skill = DefenseSkill {
                    value: (p[0] - p[1]) as u16,
                    maximum: p[0] as u16,
                    gain: 1,
                };
                assert_eq!(
                    defense_gain_chance(1, skill).unwrap().unwrap().bits(),
                    p[2] as u32,
                    "line {line}"
                );
            }
            "m" => {
                counts[1] += 1;
                mage_levels[p[1] as usize - 1] = true;
                crushing_flags[usize::from(p[7] != 0)] = true;
                die_residues[p[11] as usize % 4] = true;
                let mut input = sample();
                input.attacker.level = p[0] as u8;
                input.defender.level = p[1] as u8;
                input.defender.defense_skill = p[2] as u16;
                input.defender.agility = p[3] as u32;
                input.defender.armor = p[4] as i32;
                input.defender.standing = p[5] != 0;
                input.defender.casting_non_melee = p[6] != 0;
                input.attacker.no_crushing_blows = p[7] != 0;
                input.attacker.minimum_damage = SpellFloat::from_bits(p[8] as u32).unwrap();
                input.attacker.maximum_damage = SpellFloat::from_bits(p[9] as u32).unwrap();
                let plan = prepare(input).unwrap();
                let c = plan.chances();
                assert_eq!(
                    [c.miss, c.dodge, c.critical, c.crushing],
                    [p[12] as u16, p[13] as u16, p[14] as u16, p[15] as u16],
                    "line {line}"
                );
                assert_eq!((c.parry, c.block), (0, 0));
                let result = resolve(
                    plan,
                    MeleeRolls {
                        damage: p[10] as u32,
                        outcome: p[11] as u16,
                    },
                )
                .unwrap();
                let bucket = match result.outcome() {
                    MeleeOutcome::Miss => 0,
                    MeleeOutcome::Dodge => 1,
                    MeleeOutcome::Critical => 2,
                    MeleeOutcome::Crushing => 3,
                    MeleeOutcome::Normal => 4,
                };
                outcome_counts[bucket] += 1;
                assert_eq!(
                    (
                        result.outcome(),
                        result.total_damage(),
                        result.clean_damage()
                    ),
                    (outcome(p[16] as u32), p[17] as u32, p[18] as u32),
                    "line {line}"
                );
            }
            _ => panic!("unexpected fixture"),
        }
    }
    assert_eq!(counts, [420, 7022, 29, 1000]);
    assert!(mage_levels.into_iter().all(|seen| seen));
    assert!(crushing_flags.into_iter().all(|seen| seen));
    assert!(outcome_counts.into_iter().all(|count| count > 0));
    assert!(die_residues.into_iter().all(|seen| seen));
}
#[test]
fn outcome_boundaries_draw_order_and_zero_range() {
    let mut i = sample();
    i.attacker.minimum_damage = sf(0.0);
    i.attacker.maximum_damage = sf(0.0);
    let plan = prepare(i).unwrap();
    assert_eq!(plan.damage_range(), (0, 5));
    let c = plan.chances();
    for (roll, expected) in [
        (c.miss, MeleeOutcome::Miss),
        (c.miss + 1, MeleeOutcome::Dodge),
        (c.miss + c.dodge, MeleeOutcome::Dodge),
        (c.miss + c.dodge + 1, MeleeOutcome::Critical),
        (10000, MeleeOutcome::Normal),
    ] {
        let result = resolve(
            plan,
            MeleeRolls {
                damage: 0,
                outcome: roll,
            },
        )
        .unwrap();
        assert_eq!(result.outcome(), expected);
        assert_eq!(result.armored_damage(), 1);
    }
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 6,
            outcome: 1
        }
    )
    .is_err());
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 0,
            outcome: 0
        }
    )
    .is_err());
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 0,
            outcome: 10001
        }
    )
    .is_err());
}
#[test]
fn sitting_casting_skills_and_callbacks() {
    let i = sample();
    let c = prepare(i).unwrap().chances();
    assert!(c.dodge > 0);
    let mut current = i;
    current.defender.defense_skill = 5;
    assert!(prepare(current).unwrap().chances().dodge > c.dodge);
    let mut sitting = i;
    sitting.defender.standing = false;
    sitting.defender.casting_non_melee = true;
    let p = prepare(sitting).unwrap();
    assert_eq!(p.chances().critical, 10000);
    assert_eq!(p.chances().dodge, 0);
    let miss = resolve(
        p,
        MeleeRolls {
            damage: 2,
            outcome: 1,
        },
    )
    .unwrap();
    assert_eq!(miss.outcome(), MeleeOutcome::Miss);
    assert!(miss.callbacks().stand_up);
    assert!(miss.callbacks().defense_skill_attempt);
    assert!(!miss.callbacks().durability_hit_attempt);
    let crit = resolve(
        p,
        MeleeRolls {
            damage: 2,
            outcome: 10000,
        },
    )
    .unwrap();
    assert_eq!(crit.outcome(), MeleeOutcome::Critical);
    assert!(crit.callbacks().interrupt_or_delay_spell);
    assert!(crit.callbacks().durability_hit_attempt);
    let mut casting = i;
    casting.defender.casting_non_melee = true;
    assert_eq!(prepare(casting).unwrap().chances().dodge, 0);
    let dodge = resolve(
        prepare(i).unwrap(),
        MeleeRolls {
            damage: 2,
            outcome: c.miss + 1,
        },
    )
    .unwrap();
    assert!(dodge.callbacks().defense_reactive);
    assert!(dodge.callbacks().defense_skill_attempt);
    assert_eq!(dodge.clean_damage(), dodge.armored_damage() * 2);
    let mut critter = i;
    critter.attacker.creature_is_critter = true;
    let result = resolve(
        prepare(critter).unwrap(),
        MeleeRolls {
            damage: 2,
            outcome: 1,
        },
    )
    .unwrap();
    assert!(!result.callbacks().defense_skill_attempt);
}
#[test]
fn crushing_uses_capped_current_defense() {
    let mut i = sample();
    i.attacker.level = 4;
    i.defender.defense_skill = 1;
    let c = prepare(i).unwrap().chances();
    assert_eq!(c.crushing, 2300);
    i.defender.defense_skill = 500;
    assert_eq!(prepare(i).unwrap().chances().crushing, 1500);
    i.attacker.no_crushing_blows = true;
    assert_eq!(prepare(i).unwrap().chances().crushing, 0);
}
#[test]
fn health_is_transactional_and_callbacks_do_not_disappear_at_zero_damage() {
    let mut i = sample();
    i.health = 1;
    let result = resolve(
        prepare(i).unwrap(),
        MeleeRolls {
            damage: 5,
            outcome: 10000,
        },
    )
    .unwrap();
    assert!(result.callbacks().killed);
    assert!(!result.callbacks().defense_skill_attempt);
    assert!(!result.callbacks().interrupt_or_delay_spell);
    assert!(!result.callbacks().durability_hit_attempt);
    assert!(result.callbacks().attacked_by_and_combat);
    assert_eq!(result.effective_damage(), 1);
    assert_eq!(result.overkill(), result.total_damage() - 1);
    let mut hp = 2;
    assert!(apply_health(&mut hp, result).is_err());
    assert_eq!(hp, 2);
    let fresh = || {
        resolve(
            prepare(i).unwrap(),
            MeleeRolls {
                damage: 5,
                outcome: 10000,
            },
        )
        .unwrap()
    };
    hp = 1;
    apply_health(&mut hp, fresh()).unwrap();
    assert_eq!(hp, 0);
    assert!(apply_health(&mut hp, fresh()).is_err());
    assert_eq!(hp, 0);
    for mutate in [
        |x: &mut MeleeInput| x.front_facing = false,
        |x: &mut MeleeInput| x.health = 0,
        |x: &mut MeleeInput| x.attacker.level = 0,
        |x: &mut MeleeInput| x.defender.level = 61,
        |x: &mut MeleeInput| x.defender.agility = 1000001,
        |x: &mut MeleeInput| x.attacker.minimum_damage = sf(-1.0),
    ] {
        let mut bad = sample();
        mutate(&mut bad);
        assert!(prepare(bad).is_err());
    }
}
#[test]
fn exact_defense_gain_branch_and_strict_real_roll() {
    let skill = DefenseSkill {
        value: 1,
        maximum: 5,
        gain: 1,
    };
    let chance = defense_gain_chance(1, skill).unwrap().unwrap();
    assert_eq!(chance.bits(), sf(45.714287).bits());
    assert_eq!(
        resolve_defense_gain(1, skill, Some(PercentRoll::from_bits(0).unwrap())).unwrap(),
        2
    );
    assert_eq!(
        resolve_defense_gain(
            1,
            skill,
            Some(PercentRoll::from_fraction53((1 << 53) - 1).unwrap())
        )
        .unwrap(),
        1
    );
    assert!(resolve_defense_gain(1, skill, None).is_err());
    let mut capped = skill;
    capped.value = 5;
    assert_eq!(defense_gain_chance(1, capped).unwrap(), None);
    assert_eq!(resolve_defense_gain(1, capped, None).unwrap(), 5);
    assert!(resolve_defense_gain(1, capped, Some(PercentRoll::from_bits(0).unwrap())).is_err());
    let mut logarithmic = skill;
    logarithmic.maximum = 10;
    assert!(defense_gain_chance(2, logarithmic).unwrap().is_some());
    assert!(resolve_defense_gain(2, logarithmic, None).is_err());
    let mut disabled = skill;
    disabled.gain = 0;
    assert!(resolve_defense_gain(1, disabled, None).is_err());
    assert_eq!(
        resolve_defense_gain(1, disabled, Some(PercentRoll::from_bits(0).unwrap())).unwrap(),
        1
    );
    let equal = PercentRoll::from_bits(50f64.to_bits()).unwrap();
    assert!(!equal.succeeds(sf(50.0)).unwrap());
    assert!(equal.succeeds(sf(50.000004)).unwrap());
    for bits in [
        100f64.to_bits(),
        (-1f64).to_bits(),
        f64::NAN.to_bits(),
        f64::INFINITY.to_bits(),
    ] {
        assert!(PercentRoll::from_bits(bits).is_err());
    }
    assert!(PercentRoll::from_fraction53(1 << 53).is_err());
}
#[test]
fn fractional_damage_range_swaps_before_truncation() {
    let mut i = sample();
    i.attacker.minimum_damage = sf(9.99);
    i.attacker.maximum_damage = sf(2.1);
    assert_eq!(prepare(i).unwrap().damage_range(), (2, 9));
}
#[test]
fn creature_damage_derivation_truncates_ap_before_division() {
    let i = CreatureDamageInputs {
        class: CreatureStatClass::Warrior,
        strength: 10,
        agility: 7,
        create_strength: sf(9.75),
        create_agility: sf(7.0),
        base_attack_power: sf(1.25),
        damage_multiplier: sf(1.5),
        base_weapon_minimum: sf(1.25),
        base_weapon_maximum: sf(3.75),
        weapon_unusable: false,
    };
    let r = creature_damage_range(i).unwrap();
    assert_eq!(r.attack_power, 2);
    assert_eq!(r.minimum, sf(2f32 / 14f32 + 1.25f32 * 1.5f32));
    let mut bad = i;
    bad.base_attack_power = sf(-1.0);
    assert!(creature_damage_range(bad).is_err());
}

#[test]
fn unsupported_spell_slots_fail_closed_even_when_casting_predicate_is_false() {
    for casting in [false, true] {
        for (channel, auto_repeat) in [(true, false), (false, true), (true, true)] {
            let mut i = sample();
            i.defender.casting_non_melee = casting;
            i.defender.has_channeled_spell = channel;
            i.defender.has_auto_repeat_spell = auto_repeat;
            let original = i;
            assert!(matches!(prepare(i), Err(MeleeError::Unsupported(_))));
            assert_eq!(i, original);
        }
    }
}

#[test]
fn surviving_positive_hit_requires_source_spell_slot_inspection() {
    let i = sample();
    assert!(!i.defender.casting_non_melee);
    let hit = resolve(
        prepare(i).unwrap(),
        MeleeRolls {
            damage: 2,
            outcome: 10000,
        },
    )
    .unwrap();
    assert!(hit.callbacks().interrupt_or_delay_spell);
    let miss = resolve(
        prepare(i).unwrap(),
        MeleeRolls {
            damage: 2,
            outcome: 1,
        },
    )
    .unwrap();
    assert!(!miss.callbacks().interrupt_or_delay_spell);
}

#[test]
fn defense_zero_value_clamp_and_supported_gap_boundaries() {
    let zero = PercentRoll::from_bits(0).unwrap();
    let absent = DefenseSkill {
        value: 0,
        maximum: 0,
        gain: 1,
    };
    assert_eq!(resolve_defense_gain(1, absent, None).unwrap(), 0);
    assert!(resolve_defense_gain(1, absent, Some(zero)).is_err());
    let zero_value = DefenseSkill {
        value: 0,
        maximum: 5,
        gain: 1,
    };
    assert!(resolve_defense_gain(1, zero_value, None).is_err());
    assert_eq!(resolve_defense_gain(1, zero_value, Some(zero)).unwrap(), 0);
    let almost_full = DefenseSkill {
        value: 65529,
        maximum: 65535,
        gain: 65535,
    };
    assert!(defense_gain_chance(60, almost_full).unwrap().is_some());
    assert_eq!(
        resolve_defense_gain(60, almost_full, Some(zero)).unwrap(),
        65535
    );
    let gap_seven = DefenseSkill {
        value: 65528,
        ..almost_full
    };
    assert!(matches!(
        defense_gain_chance(60, gap_seven),
        Err(MeleeError::Unsupported(_))
    ));
}

#[test]
fn bounded_extremes_reject_before_rolls_and_keep_safe_integer_products() {
    let mut i = sample();
    i.attacker.minimum_damage = sf(1_000_000.0);
    i.attacker.maximum_damage = sf(1_000_000.0);
    i.defender.armor = -1_000_000;
    i.defender.standing = false;
    let hit = resolve(
        prepare(i).unwrap(),
        MeleeRolls {
            damage: 1_000_000,
            outcome: 10000,
        },
    )
    .unwrap();
    assert_eq!(hit.total_damage(), 2_000_000);
    assert_eq!(hit.clean_damage(), 2_000_000);
    assert_eq!(hit.health_after(), 0);
    i.defender.armor = i32::MIN;
    assert!(prepare(i).is_err());
    i.defender.armor = 0;
    i.health = 1_000_001;
    assert!(prepare(i).is_err());
}
