// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn pinned_source_extracted_speed_corpus() {
    let mut cases = 0;
    for line in include_str!("../../tools/fixtures/player_slow.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 10);
        let rates = SpeedRates::from_amounts([
            fields[0].parse::<i32>().unwrap(),
            fields[1].parse::<i32>().unwrap(),
        ])
        .unwrap();
        let expected: Vec<_> = fields[2..]
            .iter()
            .map(|value| u32::from_str_radix(value, 16).unwrap())
            .collect();
        let actual = [
            rates.run.bits(),
            rates.run_back.bits(),
            rates.swim.bits(),
            rates.speed(MoveType::Run).unwrap().bits(),
            rates.speed(MoveType::RunBack).unwrap().bits(),
            rates.speed(MoveType::Swim).unwrap().bits(),
            rates.speed(MoveType::Walk).unwrap().bits(),
            rates.speed(MoveType::SwimBack).unwrap().bits(),
        ];
        assert_eq!(actual.as_slice(), expected, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 208);
}

#[test]
fn strongest_negative_has_source_binary32_order() {
    for (amount, rate, run, back, swim) in [
        (0, 0x3f80_0000, 0x40e0_0000, 0x4090_0000, 0x4097_1c71),
        (-1, 0x3f7d_70a4, 0x40dd_c290, 0x408e_8f5c, 0x4095_9999),
        (-7, 0x3f6e_147b, 0x40d0_51ec, 0x4085_eb85, 0x408c_8888),
        (-33, 0x3f2b_851f, 0x4096_147b, 0x4040_f5c3, 0x404a_7d27),
        (-37, 0x3f21_47ae, 0x408d_1eb8, 0x4035_70a4, 0x403e_6665),
        (-50, 0x3f00_0000, 0x4060_0000, 0x4010_0000, 0x4017_1c71),
        (-67, 0x3ea8_f5c3, 0x4013_d70b, 0x3fbe_147b, 0x3fc7_7777),
        (-99, 0x3c23_d70a, 0x3d8f_5c29, 0x3d38_51eb, 0x3d41_6c15),
    ] {
        let rates = SpeedRates::from_amounts([amount]).unwrap();
        assert_eq!(rates.run.bits(), rate);
        assert_eq!(rates.run_back.bits(), rate);
        assert_eq!(rates.swim.bits(), rate);
        assert_eq!(rates.speed(MoveType::Run).unwrap().bits(), run);
        assert_eq!(rates.speed(MoveType::RunBack).unwrap().bits(), back);
        assert_eq!(rates.speed(MoveType::Swim).unwrap().bits(), swim);
        assert_eq!(rates.speed(MoveType::Walk).unwrap().bits(), 0x4020_0000);
        assert_eq!(rates.speed(MoveType::SwimBack).unwrap().bits(), 0x4020_0000);
    }
}

#[test]
fn floor_is_applied_after_slow_and_survives_over_one_hundred_percent() {
    let floor = SpeedRates::from_amounts([-99]).unwrap();
    for amount in [-100, -101, -10_000, -1_000_000] {
        assert_eq!(SpeedRates::from_amounts([amount]).unwrap(), floor);
    }
}

#[test]
fn holder_order_duplicates_removal_and_restore_are_deterministic() {
    let strongest = SpeedRates::from_amounts([-67]).unwrap();
    for amounts in [[-1, -67, -50], [-67, -50, -1], [-67, -67, 0]] {
        assert_eq!(SpeedRates::from_amounts(amounts).unwrap(), strongest);
    }
    assert_eq!(SpeedRates::from_amounts([]).unwrap(), SpeedRates::normal());
    assert_eq!(
        SpeedRates::from_amounts([0, 0]).unwrap(),
        SpeedRates::normal()
    );
    assert_ne!(SpeedRates::from_amounts([-50]).unwrap(), strongest);
}

#[test]
fn movement_mode_matches_live_source_priority() {
    assert_eq!(MoveType::from_mode(false, false, false), MoveType::Run);
    assert_eq!(MoveType::from_mode(false, false, true), MoveType::RunBack);
    assert_eq!(MoveType::from_mode(false, true, true), MoveType::Walk);
    assert_eq!(MoveType::from_mode(true, true, false), MoveType::Swim);
    assert_eq!(MoveType::from_mode(true, true, true), MoveType::SwimBack);
}

#[test]
fn unsupported_modifiers_and_forged_rates_fail_closed() {
    assert!(SpeedRates::from_amounts([1]).is_err());
    assert!(SpeedRates::from_amounts([i32::MIN]).is_err());
    assert!(SpeedRates::from_amounts([-1; 17]).is_err());
    for bits in [0, 0x3c23_d709, 0x3f80_0001, 0xbf80_0000] {
        let rates = SpeedRates {
            run: Real::from_bits(bits).unwrap(),
            ..SpeedRates::normal()
        };
        assert!(rates.speed(MoveType::Run).is_err());
        assert!(rates.speed(MoveType::Walk).is_err());
    }
}
