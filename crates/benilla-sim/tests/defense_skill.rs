// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::npc_melee::{
    defense_gain_chance, resolve_defense_gain, DefenseSkill, MeleeError, PercentRoll,
};
use sha2::{Digest, Sha256};

fn boundary_mask(chance: SpellFloat) -> u8 {
    // Every chance is positive, so +/-1 is exactly adjacent positive binary64.
    let exact = f64::from(f32::from_bits(chance.bits())).to_bits();
    let mut mask = 0;
    for (index, bits) in [exact - 1, exact, exact + 1].into_iter().enumerate() {
        let outcome = match PercentRoll::from_bits(bits) {
            Ok(roll) => u8::from(roll.succeeds(chance).unwrap()),
            Err(_) => {
                assert!(bits >= 100f64.to_bits());
                2
            }
        };
        mask |= outcome << (2 * index);
    }
    mask
}

#[test]
fn complete_classic_defense_domain_matches_unchanged_source_and_roll_boundaries() {
    let mut levels = 0;
    let mut total = 0;
    for row in include_str!("fixtures/defense_skill_cmangos.sha256").lines() {
        if row.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = row.split_whitespace().collect();
        assert_eq!(fields.len(), 3);
        let level: u8 = fields[0].parse().unwrap();
        levels += 1;
        assert_eq!(level, levels);
        let mut hash = Sha256::new();
        let mut count = 0;
        for maximum in 1..=300 {
            for value in 0..maximum {
                let chance = defense_gain_chance(
                    level,
                    DefenseSkill {
                        value,
                        maximum,
                        gain: 1,
                    },
                )
                .unwrap()
                .unwrap();
                hash.update(chance.bits().to_le_bytes());
                hash.update([boundary_mask(chance)]);
                count += 1;
            }
        }
        assert_eq!(count, fields[1].parse::<u32>().unwrap());
        assert_eq!(format!("{:x}", hash.finalize()), fields[2], "level {level}");
        total += count;
    }
    assert_eq!(levels, 60);
    assert_eq!(total, 2_709_000);
}

#[test]
fn source_state_gain_and_adjacent_roll_edges_match() {
    let mut count = 0;
    for row in include_str!("fixtures/defense_skill_edges_cmangos.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let fields: Vec<u64> = row.split_whitespace().map(|s| s.parse().unwrap()).collect();
        assert_eq!(fields.len(), 8);
        let level = u8::try_from(fields[0]).unwrap();
        let skill = DefenseSkill {
            maximum: u16::try_from(fields[1]).unwrap(),
            value: u16::try_from(fields[2]).unwrap(),
            gain: u16::try_from(fields[3]).unwrap(),
        };
        let roll = PercentRoll::from_bits(fields[4]).unwrap();
        let chance = defense_gain_chance(level, skill).unwrap();
        assert_eq!(
            chance.map(|n| u64::from(n.bits())).unwrap_or(0),
            fields[5],
            "{row}"
        );
        assert_eq!(u64::from(chance.is_some()), fields[6], "{row}");
        let optional_roll = chance.map(|_| roll);
        assert_eq!(
            u64::from(resolve_defense_gain(level, skill, optional_roll).unwrap()),
            fields[7],
            "{row}"
        );
        // Probability consumption is independent of gain=0, value=0, and 100%.
        let wrong_roll = if chance.is_some() { None } else { Some(roll) };
        assert!(
            matches!(
                resolve_defense_gain(level, skill, wrong_roll),
                Err(MeleeError::Roll(_))
            ),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 2_451);
}

#[test]
fn expanded_catch_up_keeps_level_guards_and_wide_quadratic_domain() {
    for level in [0, 61, u8::MAX] {
        for skill in [
            DefenseSkill {
                value: 0,
                maximum: 0,
                gain: 0,
            },
            DefenseSkill {
                value: 1,
                maximum: 300,
                gain: 1,
            },
            DefenseSkill {
                value: 300,
                maximum: 300,
                gain: 1,
            },
        ] {
            assert!(matches!(
                defense_gain_chance(level, skill),
                Err(MeleeError::Domain(_))
            ));
        }
    }
    for maximum in [301, 65535] {
        for level in [1, 60] {
            for room in [7, maximum] {
                let skill = DefenseSkill {
                    value: maximum - room,
                    maximum,
                    gain: 1,
                };
                assert!(matches!(
                    defense_gain_chance(level, skill),
                    Err(MeleeError::Unsupported(_))
                ));
                assert!(matches!(
                    resolve_defense_gain(level, skill, None),
                    Err(MeleeError::Unsupported(_))
                ));
            }
            let near_cap = DefenseSkill {
                value: maximum - 6,
                maximum,
                gain: 1,
            };
            assert!(defense_gain_chance(level, near_cap).unwrap().is_some());
            for value in [maximum, u16::MAX] {
                assert_eq!(
                    defense_gain_chance(level, DefenseSkill { value, ..near_cap }).unwrap(),
                    None
                );
            }
        }
    }
}
