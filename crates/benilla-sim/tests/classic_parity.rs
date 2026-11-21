// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::classic::*;

#[test]
fn armor_matches_independent_pinned_cpp_bits_and_melee_cast() {
    let mut count = 0;
    for line in include_str!("fixtures/armor_cmangos.tsv").lines().skip(1) {
        let v: Vec<i64> = line
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let actual =
            cmangos_armor_reduction(v[0] as u8, v[1] as i32, v[2] as i32, v[3] as u32).unwrap();
        assert_eq!(actual.remaining_damage_bits, v[4] as u32, "{line}");
        assert_eq!(actual.melee_damage, v[5] as u32, "{line}");
        let software =
            software_armor_reduction(v[0] as u8, v[1] as i32, v[2] as i32, v[3] as u32).unwrap();
        assert_eq!(software, actual, "software binary32 disagrees: {line}");
        count += 1;
    }
    assert_eq!(count, 5376);
}

#[test]
fn stats_match_independent_pinned_cpp_oracle() {
    let mut count = 0;
    for line in include_str!("fixtures/stats_cmangos.tsv").lines().skip(1) {
        let v: Vec<i64> = line
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let f = |i: usize| f32::from_bits(v[i] as u32);
        let m = StatModifiers {
            base_value: f(5),
            base_percent: f(6),
            total_value: f(7),
            total_percent: f(8),
        };
        let actual = match v[0] {
            0 => player_max_health(v[1] as u32, v[2] as i32, m),
            1 => maximum_mana(v[1] as u32, v[2] as i32, m),
            2 => creature_max_health(v[1] as u32, v[2] as i32, f(3), f(4), m),
            _ => panic!("unknown oracle kind"),
        }
        .unwrap();
        assert_eq!(actual, v[9] as u32, "{line}");
        count += 1;
    }
    assert_eq!(count, 1011);
}

#[test]
fn every_rational_vs_source_difference_in_declared_domain_is_reviewed() {
    let mut differences = Vec::new();
    for level in 1..=10 {
        for armor in 0..=2000 {
            for damage in 0..=64 {
                let rational = armor_reduced_damage(level, armor, 0, damage).unwrap();
                let upstream = cmangos_armor_reduction(level, armor, 0, damage)
                    .unwrap()
                    .melee_damage;
                assert_eq!(
                    software_armor_reduced_damage(level, armor, 0, damage).unwrap(),
                    upstream
                );
                if rational != upstream {
                    differences.push(format!(
                        "{level}\t{armor}\t{damage}\t{rational}\t{upstream}"
                    ));
                }
            }
        }
    }
    let expected: Vec<&str> = include_str!("fixtures/armor_rounding_differences.tsv")
        .lines()
        .skip(1)
        .collect();
    assert_eq!(differences.len(), 168);
    assert_eq!(
        differences, expected,
        "unreviewed native-float rounding deviation"
    );
}

#[test]
fn rational_armor_cap_penetration_and_integer_boundaries() {
    assert_eq!(NUMERIC_POLICY_VERSION, 1);
    for level in 1..=63 {
        let k = 85 * i32::from(level) + 400;
        assert_eq!(armor_reduced_damage(level, k, 0, 101).unwrap(), 50);
        assert_eq!(
            armor_reduced_damage(level, 3 * k - 1, 0, 4 * k as u32).unwrap(),
            k as u32
        );
        for armor in [3 * k, 3 * k + 1, 1_000_000_000] {
            assert_eq!(armor_reduced_damage(level, armor, 0, 101).unwrap(), 25);
            assert_eq!(armor_reduced_damage(level, armor, 0, 1).unwrap(), 1);
        }
    }
    assert_eq!(armor_reduced_damage(1, -1, 0, 100).unwrap(), 100);
    assert_eq!(armor_reduced_damage(1, 100, 101, 100).unwrap(), 100);
    assert_eq!(armor_reduced_damage(1, 0, -485, 100).unwrap(), 50);
    assert_eq!(armor_reduced_damage(1, 0, 0, 0).unwrap(), 1);
    assert_eq!(
        armor_reduced_damage(63, 1_000_000_000, -1_000_000_000, 1_000_000_000).unwrap(),
        250_000_000
    );
    // The integer authority intentionally preserves values beyond f32's exact-integer range.
    assert_eq!(
        armor_reduced_damage(1, 0, 0, 16_777_217).unwrap(),
        16_777_217
    );
    assert_eq!(
        cmangos_armor_reduction(1, 0, 0, 16_777_217)
            .unwrap()
            .melee_damage,
        16_777_216
    );
}

#[test]
fn stats_keep_player_and_creature_stamina_and_mana_semantics_distinct() {
    let m = StatModifiers::default();
    for (value, hp, mana) in [
        (-1, -1.0, -1.0),
        (0, 0.0, 0.0),
        (19, 19.0, 19.0),
        (20, 20.0, 20.0),
        (21, 30.0, 35.0),
    ] {
        assert_eq!(health_bonus_from_stamina(value).unwrap(), hp);
        assert_eq!(mana_bonus_from_intellect(value).unwrap(), mana);
    }
    assert_eq!(player_max_health(100, 21, m).unwrap(), 130);
    assert_eq!(creature_max_health(100, 21, 21.0, 1.0, m).unwrap(), 100);
    assert_eq!(creature_max_health(100, 21, 21.5, 1.0, m).unwrap(), 95);
    assert_eq!(creature_max_health(100, 21, 21.5, 2.0, m).unwrap(), 190);
    assert_eq!(maximum_mana(0, 21, m).unwrap(), 0);
    assert_eq!(maximum_mana(100, 21, m).unwrap(), 135);
    assert_eq!(player_max_health(0, -1, m).unwrap(), 1);
    let half = StatModifiers {
        total_value: 0.5,
        ..m
    };
    assert_eq!(player_max_health(2, 0, half).unwrap(), 3);
    assert_eq!(maximum_mana(2, 0, half).unwrap(), 3);
}

#[test]
fn unsupported_inputs_are_rejected_before_integer_casts() {
    for function in [armor_reduced_damage, software_armor_reduced_damage] {
        assert_eq!(function(0, 0, 0, 1).unwrap_err().field, "attacker_level");
        assert_eq!(function(64, 0, 0, 1).unwrap_err().field, "attacker_level");
        assert_eq!(function(1, i32::MIN, 0, 1).unwrap_err().field, "armor");
        assert_eq!(
            function(1, 0, i32::MAX, 1).unwrap_err().field,
            "armor_penetration"
        );
        assert_eq!(function(1, 0, 0, u32::MAX).unwrap_err().field, "damage");
    }
    assert!(cmangos_armor_reduction(0, 0, 0, 1).is_err());
    assert!(cmangos_armor_reduction(1, i32::MIN, 0, 1).is_err());
    assert!(cmangos_armor_reduction(1, 0, i32::MAX, 1).is_err());
    assert!(cmangos_armor_reduction(1, 0, 0, u32::MAX).is_err());
    let m = StatModifiers::default();
    assert_eq!(
        player_max_health(u32::MAX, 1, m).unwrap_err().field,
        "create_health"
    );
    assert_eq!(
        maximum_mana(u32::MAX, 1, m).unwrap_err().field,
        "create_mana"
    );
    assert!(player_max_health(1, 1_000_001, m).is_err());
    assert!(maximum_mana(1, -1_000_001, m).is_err());
    assert!(maximum_mana(
        0,
        0,
        StatModifiers {
            total_value: -1.0,
            ..m
        }
    )
    .is_err());
    assert!(player_max_health(
        1_000_000_000,
        0,
        StatModifiers {
            total_percent: 1000.0,
            ..m
        }
    )
    .is_err());
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(1),
        -1.0,
        1001.0,
    ] {
        assert!(player_max_health(
            1,
            1,
            StatModifiers {
                base_percent: value,
                ..m
            }
        )
        .is_err());
        assert!(maximum_mana(
            1,
            1,
            StatModifiers {
                total_percent: value,
                ..m
            }
        )
        .is_err());
        assert!(creature_max_health(1, 1, 1.0, value, m).is_err());
    }
    for value in [f32::NAN, f32::INFINITY, -1.0, 1_000_001.0] {
        assert!(creature_max_health(1, 1, value, 1.0, m).is_err());
    }
}

#[test]
fn software_policy_preserves_source_rounding_where_rational_policy_does_not() {
    assert_eq!(
        SOFTWARE_ARMOR_POLICY_ID,
        "cmangos-8ec338a1-armor-binary32-rne-v1"
    );
    assert_eq!(software_armor_reduced_damage(1, 388, 0, 9).unwrap(), 4);
    assert_eq!(armor_reduced_damage(1, 388, 0, 9).unwrap(), 5);
    assert_eq!(
        software_armor_reduced_damage(1, 0, 0, 16_777_217).unwrap(),
        16_777_216
    );
    assert_eq!(software_armor_reduced_damage(1, 0, 0, 0).unwrap(), 1);
}
