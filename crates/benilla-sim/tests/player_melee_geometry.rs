// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::creature_ai::{Point, Real};
use benilla_sim::player_melee::{
    mainhand_parry_haste, npc_shield_block, player_in_melee_range, PlayerMeleeGeometry,
};
use sha2::{Digest, Sha256};

fn sample() -> PlayerMeleeGeometry {
    PlayerMeleeGeometry {
        player_position: Point::new(0.0, 0.0, 0.0).unwrap(),
        target_position: Point::new(0.0, 0.0, 0.0).unwrap(),
        player_reach: Real::ZERO,
        target_reach: Real::ZERO,
        player_moving: false,
        player_walking: false,
        target_moving: false,
        target_walking: false,
    }
}

#[test]
fn pinned_player_geometry_parry_and_shield_source_differential() {
    let fixture = include_str!("fixtures/player_melee_geometry_cmangos.tsv");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "d0e21770849be71488ce206bebb7616086bfb127bbe68fe3054669aa659b1ec6"
    );
    let mut counts = [0; 3];
    for row in fixture.lines() {
        let mut fields = row.split_whitespace();
        let kind = fields.next().unwrap();
        let p: Vec<u32> = fields.map(|v| v.parse().unwrap()).collect();
        match kind {
            "r" => {
                assert_eq!(p.len(), 13);
                let point = |i| Point {
                    x: Real::from_bits(p[i]).unwrap(),
                    y: Real::from_bits(p[i + 1]).unwrap(),
                    z: Real::from_bits(p[i + 2]).unwrap(),
                };
                let input = PlayerMeleeGeometry {
                    player_position: point(0),
                    target_position: point(3),
                    player_reach: Real::from_bits(p[6]).unwrap(),
                    target_reach: Real::from_bits(p[7]).unwrap(),
                    player_moving: p[8] != 0,
                    player_walking: p[9] != 0,
                    target_moving: p[10] != 0,
                    target_walking: p[11] != 0,
                };
                assert_eq!(
                    u32::from(player_in_melee_range(input).unwrap()),
                    p[12],
                    "{row}"
                );
                counts[0] += 1;
            }
            "p" => {
                assert_eq!(p.len(), 3);
                assert_eq!(mainhand_parry_haste(p[0], p[1]).unwrap(), p[2], "{row}");
                counts[1] += 1;
            }
            "b" => {
                assert_eq!(p.len(), 3);
                assert_eq!(
                    npc_shield_block(p[0] as u8, SpellFloat::from_bits(p[1]).unwrap()).unwrap(),
                    p[2],
                    "{row}"
                );
                counts[2] += 1;
            }
            _ => panic!("unknown source row {row}"),
        }
    }
    assert_eq!(counts, [560, 85, 441]);
}

#[test]
fn player_reach_is_strict_three_dimensional_and_requires_both_running() {
    let mut input = sample();
    input.target_position.z = Real::new(5.0).unwrap();
    assert!(!player_in_melee_range(input).unwrap());
    input.target_position.z = Real::new(4.999).unwrap();
    assert!(player_in_melee_range(input).unwrap());
    input.target_position.z = Real::new(7.663).unwrap();
    input.player_moving = true;
    assert!(!player_in_melee_range(input).unwrap());
    input.target_moving = true;
    // Distinguishes actual 8.0f/3.0f from an approximate 2.66f leeway.
    assert!(player_in_melee_range(input).unwrap());
    input.player_walking = true;
    assert!(!player_in_melee_range(input).unwrap());
    input.player_walking = false;
    input.target_walking = true;
    assert!(!player_in_melee_range(input).unwrap());
    input.target_walking = false;
    input.player_moving = false;
    assert!(!player_in_melee_range(input).unwrap());
}

#[test]
fn parry_thresholds_and_invalid_operands_do_not_get_defaults() {
    for (remaining, expected) in [(399, 399), (400, 400), (401, 400), (1200, 400), (1201, 401)] {
        assert_eq!(mainhand_parry_haste(remaining, 2000).unwrap(), expected);
    }
    assert!(mainhand_parry_haste(1, 0).is_err());
    assert!(mainhand_parry_haste(1_000_001, 2000).is_err());
    assert!(mainhand_parry_haste(1, 1_000_001).is_err());
    assert!(npc_shield_block(0, SpellFloat::ZERO).is_err());
    assert!(npc_shield_block(64, SpellFloat::ZERO).is_err());
    assert!(npc_shield_block(1, SpellFloat::from_bits((-1.0f32).to_bits()).unwrap()).is_err());
    let mut input = sample();
    input.player_reach = Real::new(-1.0).unwrap();
    assert!(player_in_melee_range(input).is_err());
    input = sample();
    input.player_position.x = Real::new(32761.0).unwrap();
    assert!(player_in_melee_range(input).is_err());
}
