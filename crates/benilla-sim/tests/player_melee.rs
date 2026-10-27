// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::player_melee::*;
use sha2::{Digest, Sha256};

fn sf(v: f32) -> SpellFloat {
    SpellFloat::from_bits(v.to_bits()).unwrap()
}
fn sample() -> MeleeInput {
    MeleeInput {
        attacker: MageAttack {
            level: 20,
            weapon_skill: 100,
            minimum_damage: sf(10.0),
            maximum_damage: sf(20.0),
            critical_chance: sf(5.0),
            hit_chance: sf(0.0),
        },
        defender: CreatureDefense {
            level: 20,
            armor: 0,
            can_react: true,
            casting_non_melee: false,
            can_dodge: true,
            can_parry: true,
            can_block: true,
            front_facing: true,
            shield_block_value: 3,
        },
    }
}
fn outcome(n: i64) -> MeleeOutcome {
    match n {
        1 => MeleeOutcome::Miss,
        2 => MeleeOutcome::Dodge,
        3 => MeleeOutcome::Block,
        4 => MeleeOutcome::Parry,
        5 => MeleeOutcome::Glancing,
        6 => MeleeOutcome::Critical,
        8 => MeleeOutcome::Normal,
        _ => panic!("unsupported fixture outcome {n}"),
    }
}
#[test]
fn pinned_cpp_player_mainhand_differential() {
    let fixture = include_str!("fixtures/player_melee_cmangos.tsv");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "34efb7cb1c26495041a467dffdf7e27e8579c472deeaa4e116572f505bf45bc5"
    );
    let mut seen = [false; 9];
    let mut rows = 0;
    let mut glance_without_draw = false;
    let mut glance_with_draw = false;
    let mut rear_dodge = false;
    for (line, row) in fixture.lines().enumerate() {
        let mut columns = row.split_whitespace();
        assert_eq!(columns.next(), Some("m"));
        let p: Vec<i64> = columns.map(|v| v.parse().unwrap()).collect();
        assert_eq!(p.len(), 36, "line {line}");
        let f = |i: usize| SpellFloat::from_bits(p[i] as u32).unwrap();
        let input = MeleeInput {
            attacker: MageAttack {
                level: p[0] as u8,
                weapon_skill: p[1] as u16,
                minimum_damage: f(2),
                maximum_damage: f(3),
                critical_chance: f(4),
                hit_chance: f(5),
            },
            defender: CreatureDefense {
                level: p[6] as u8,
                armor: p[7] as i32,
                can_react: p[8] != 0,
                casting_non_melee: p[9] != 0,
                can_dodge: p[10] != 0,
                can_parry: p[11] != 0,
                can_block: p[12] != 0,
                front_facing: p[13] != 0,
                shield_block_value: p[14] as u32,
            },
        };
        let plan = prepare(input).unwrap();
        let c = plan.chances();
        assert_eq!(
            [c.miss, c.dodge, c.parry, c.block, c.glance, c.critical],
            std::array::from_fn(|i| p[18 + i] as u16),
            "line {line}"
        );
        assert_eq!(
            plan.damage_range(),
            (p[30] as u32, p[31] as u32),
            "line {line}"
        );
        assert_eq!((p[32], p[33]), (1, 10000), "line {line}");
        let glance = (p[17] >= 0).then_some(p[17] as u32);
        let range = plan.glance_range(p[16] as u16).unwrap();
        assert_eq!(
            range,
            (p[29] == 3).then_some((p[34] as u32, p[35] as u32)),
            "line {line}"
        );
        assert_eq!(p[29], if glance.is_some() { 3 } else { 2 }, "line {line}");
        let result = resolve(
            plan,
            MeleeRolls {
                damage: p[15] as u32,
                outcome: p[16] as u16,
                glance,
            },
        )
        .unwrap();
        assert_eq!(result.outcome(), outcome(p[24]), "line {line}");
        assert_eq!(
            [
                result.total_damage(),
                result.clean_damage(),
                result.blocked_amount(),
                result.sub_damage(),
            ],
            std::array::from_fn(|i| p[25 + i] as u32),
            "line {line}"
        );
        seen[p[24] as usize] = true;
        glance_without_draw |= p[24] == 5 && glance.is_none();
        glance_with_draw |= p[24] == 5 && glance.is_some();
        rear_dodge |= p[24] == 2 && !input.defender.front_facing;
        rows += 1;
    }
    assert_eq!(rows, 3788);
    assert!([1, 2, 3, 4, 5, 6, 8].into_iter().all(|i| seen[i]));
    assert!(glance_without_draw && glance_with_draw && rear_dodge);
}
#[test]
fn player_mainhand_rejects_invalid_and_extra_draws() {
    let plan = prepare(sample()).unwrap();
    for roll in [0, 10001] {
        assert!(plan.outcome(roll).is_err());
    }
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 9,
            outcome: 1,
            glance: None,
        },
    )
    .is_err());
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 10,
            outcome: 1,
            glance: Some(1),
        },
    )
    .is_err());
    let glance_roll = plan.chances().miss
        + plan.chances().dodge
        + plan.chances().parry
        + plan.chances().block
        + 1;
    assert!(plan.glance_range(glance_roll).unwrap().is_some());
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 10,
            outcome: glance_roll,
            glance: None,
        },
    )
    .is_err());
    assert!(resolve(
        plan,
        MeleeRolls {
            damage: 10,
            outcome: glance_roll,
            glance: Some(101),
        },
    )
    .is_err());
    let mut invalid = sample();
    invalid.attacker.minimum_damage = sf(-1.0);
    assert!(prepare(invalid).is_err());
    invalid = sample();
    invalid.attacker.level = 0;
    assert!(prepare(invalid).is_err());
    invalid = sample();
    invalid.defender.level = 64;
    assert!(prepare(invalid).is_err());
    invalid = sample();
    invalid.attacker.weapon_skill = 1001;
    assert!(prepare(invalid).is_err());
    for bits in [0x7f800000, 0xff800000, 0x7fc00000, 1] {
        assert!(SpellFloat::from_bits(bits).is_err());
    }
}
#[test]
fn player_mainhand_exact_source_eligibility_and_zero_range() {
    let mut input = sample();
    input.defender.front_facing = false;
    let rear = prepare(input).unwrap().chances();
    assert!(rear.dodge > 0);
    assert_eq!((rear.parry, rear.block), (0, 0));
    input.defender.casting_non_melee = true;
    let casting = prepare(input).unwrap().chances();
    assert_eq!((casting.dodge, casting.parry, casting.block), (0, 0, 0));
    input.defender.level = 10;
    assert_eq!(prepare(input).unwrap().chances().glance, 0);
    input.attacker.minimum_damage = sf(0.0);
    input.attacker.maximum_damage = sf(0.0);
    assert_eq!(prepare(input).unwrap().damage_range(), (0, 5));
}
