// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric component tests, CMaNGOS Classic 8ec338a1 SpawnGroup.cpp:1541-1911.
use benilla_sim::{
    source_formations::{offset, Position},
    source_rng::{DrawCursor, Mt19937, SourceRandom},
};

#[test]
fn every_shape_and_random_tail_matches_pinned_builder_bits() {
    let expected = include_str!("../tools/fixtures/source_formations/expected.tsv");
    let inputs = include_str!("../tools/fixtures/source_formations/inputs.txt");
    let mut compared = 0;
    for line in inputs.lines() {
        let v: Vec<u32> = line
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let (case, shape, count, compact, pattern, mirror) = (v[0], v[1], v[2], v[3], v[4], v[5]);
        if count != 12 || compact != 0 || pattern != 0 || mirror != 0 {
            continue;
        }
        let mut engine = Mt19937::from_seed(5489);
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        for index in 1..count {
            let (a, d, max) =
                offset(shape, 3.25_f32.to_bits(), 2.625_f32.to_bits(), 11.0, index).unwrap();
            let mut p = Position::default();
            p.set(a, d);
            if let Some([a, d]) = max {
                p.max_angle = a;
                p.max_distance = d;
                p.add_variation(true, &mut random).unwrap();
            }
            let prefix = format!("S {case} 0 {index} ");
            let row = expected
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap();
            let want: Vec<u32> = row
                .split_whitespace()
                .skip(4)
                .map(|x| u32::from_str_radix(x, 16).unwrap())
                .collect();
            assert_eq!(
                [
                    p.real_angle,
                    p.real_distance,
                    p.angle,
                    p.distance,
                    p.max_angle,
                    p.max_distance,
                    p.destination_angle,
                    p.destination_distance,
                    p.angle(true, false)
                ],
                want.as_slice(),
                "shape={shape}, slot={index}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 77);
}
#[test]
fn negative_variation_snaps_and_threshold_equality_does_not_ease() {
    // SpawnGroup.cpp:1896-1910 compares signed diff > threshold, not abs(diff).
    for (angle, distance) in [(-1.0_f32, -1.0_f32), (0.01, 0.1)] {
        let mut p = Position {
            destination_angle: angle.to_bits(),
            destination_distance: distance.to_bits(),
            ..Position::default()
        };
        p.update();
        assert_eq!(p.angle, angle.to_bits());
        assert_eq!(p.distance, distance.to_bits());
    }
    let mut p = Position {
        destination_angle: 1_f32.to_bits(),
        destination_distance: 2_f32.to_bits(),
        ..Position::default()
    };
    p.update();
    assert_eq!(p.angle, 0.05_f32.to_bits());
    assert_eq!(p.distance, 0.1_f32.to_bits());
}
#[test]
fn front_fan_keeps_tau_at_equality_and_mirroring_does_not_normalize() {
    // SpawnGroup.cpp:1817 uses > and :1872 returns TAU-angle without modulo.
    let (angle, _, _) = offset(5, 1_f32.to_bits(), 0, 2.0, 2).unwrap();
    assert_eq!(angle, 0x40c9_0fdb);
    let p = Position::default();
    assert_eq!(p.angle(true, true), 0x40c9_0fdb);
    assert_eq!(p.angle(false, true), 0);
    assert!(offset(7, 0, 0, 1.0, 1).is_none());
}
