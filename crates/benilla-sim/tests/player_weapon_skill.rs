// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::npc_melee::{MeleeError, PercentRoll};
use benilla_sim::player_melee::{resolve_weapon_gain, weapon_gain_chance, WeaponSkill};
use sha2::{Digest, Sha256};

#[test]
fn weapon_skill_matches_pinned_source_chances_draws_and_callbacks() {
    let fixture = include_str!("fixtures/player_weapon_skill_cmangos.tsv");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "abbeefd3a910e64175df90f04c16803c2deb8de624bf6e0fbe58b42719aa8d1d"
    );
    let mut count = 0;
    let mut levels = [false; 60];
    let mut unchanged_callback = false;
    let mut capped_without_draw = false;
    for row in fixture.lines() {
        let p: Vec<u64> = row.split_whitespace().map(|v| v.parse().unwrap()).collect();
        assert_eq!(p.len(), 10);
        let level = p[0] as u8;
        let intellect = p[1] as u32;
        let skill = WeaponSkill {
            maximum: p[2] as u16,
            value: p[3] as u16,
            gain: p[4] as u16,
        };
        let chance = weapon_gain_chance(level, intellect, skill).unwrap();
        assert_eq!(chance.map(|c| u64::from(c.bits())).unwrap_or(0), p[6], "{row}");
        assert_eq!(u64::from(chance.is_some()), p[7], "{row}");
        let roll = PercentRoll::from_bits(p[5]).unwrap();
        let result = resolve_weapon_gain(level, intellect, skill, chance.map(|_| roll)).unwrap();
        assert_eq!(u64::from(result.value()), p[8], "{row}");
        // The oracle supplies a valid ordinary non-PvP/non-fishing weapon,
        // so each successful UpdateWeaponSkill reaches its crit callback.
        assert_eq!(u64::from(result.update_weapon_skill()), p[9], "{row}");
        let unexpected = if chance.is_some() { None } else { Some(roll) };
        assert!(matches!(
            resolve_weapon_gain(level, intellect, skill, unexpected),
            Err(MeleeError::Roll(_))
        ));
        levels[usize::from(level - 1)] = true;
        unchanged_callback |= result.update_weapon_skill() && result.value() == skill.value;
        capped_without_draw |= chance.is_none();
        count += 1;
    }
    assert_eq!(count, 9241);
    assert!(levels.into_iter().all(|seen| seen));
    assert!(unchanged_callback && capped_without_draw);
}

#[test]
fn weapon_skill_rejects_unsupported_domain_without_guessing_chance() {
    let skill = WeaponSkill {
        value: 1,
        maximum: 300,
        gain: 1,
    };
    for level in [0, 61, u8::MAX] {
        assert!(weapon_gain_chance(level, 17, skill).is_err());
    }
    assert!(weapon_gain_chance(1, 1_000_001, skill).is_err());
    assert!(weapon_gain_chance(
        1,
        17,
        WeaponSkill {
            maximum: 301,
            ..skill
        }
    )
    .is_err());
    let wide = WeaponSkill {
        value: u16::MAX - 6,
        maximum: u16::MAX,
        gain: u16::MAX,
    };
    assert!(weapon_gain_chance(60, 0, wide).unwrap().is_some());
    let no_draw = WeaponSkill {
        value: 0,
        maximum: 0,
        gain: 0,
    };
    let result = resolve_weapon_gain(1, 0, no_draw, None).unwrap();
    assert_eq!(result.value(), 0);
    assert!(!result.update_weapon_skill());
}
