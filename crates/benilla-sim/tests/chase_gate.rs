// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::chase_gate::{self, NumericError, NumericGate};

fn xyz(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x.to_bits(), y.to_bits(), z.to_bits()]
}

#[test]
fn chase_gate_distance_is_strict_owner_reach_only() {
    let owner = xyz(0.0, 0.0, 0.0);
    let edge = 201.5_f32.to_bits();
    for (x, expected) in [
        (edge - 1, NumericGate::Pass),
        (edge, NumericGate::Distance),
        (edge + 1, NumericGate::Distance),
    ] {
        assert_eq!(
            chase_gate::numeric_gate_bits(owner, [x, 0, 0], 1.5_f32.to_bits()),
            Ok(expected)
        );
    }
    // A 200.5 distance passes with 1.5 reach even when a different radius would
    // fail. Only the owned source combat reach belongs in this argument.
    let goal = xyz(200.5, 0.0, 0.0);
    assert_eq!(
        chase_gate::numeric_gate_bits(owner, goal, 1.5_f32.to_bits()),
        Ok(NumericGate::Pass)
    );
    assert_eq!(
        chase_gate::numeric_gate_bits(owner, goal, 0.25_f32.to_bits()),
        Ok(NumericGate::Distance)
    );
}

#[test]
fn chase_gate_vertical_is_strict_on_both_sides_and_after_distance() {
    for sign in [0, 0x8000_0000] {
        for (z, expected) in [
            (0x409f_ffff, NumericGate::Pass),
            (0x40a0_0000, NumericGate::Vertical),
            (0x40a0_0001, NumericGate::Vertical),
        ] {
            assert_eq!(
                chase_gate::numeric_gate_bits([0; 3], [0, 0, z | sign], 0),
                Ok(expected)
            );
        }
    }
    assert_eq!(
        chase_gate::numeric_gate_bits([0; 3], xyz(300.0, 0.0, 6.0), 0),
        Ok(NumericGate::Distance)
    );
}

#[test]
fn chase_gate_los_uses_goal_and_two_distinct_heights() {
    let owner = xyz(10.0, -20.0, 30.0);
    let goal = xyz(80.0, 90.0, 31.0);
    assert_eq!(
        chase_gate::los_endpoints_bits(owner, goal, 3.0_f32.to_bits(), 2.0_f32.to_bits()),
        Ok([xyz(10.0, -20.0, 33.0), xyz(80.0, 90.0, 33.0)])
    );
    // Source model-height products can be negative; no absolute value or clamp.
    assert_eq!(
        chase_gate::los_endpoints_bits(owner, goal, (-3.0_f32).to_bits(), (-2.0_f32).to_bits()),
        Ok([xyz(10.0, -20.0, 27.0), xyz(80.0, 90.0, 29.0)])
    );
    // Each addition uses software ties-even, preserving a subnormal endpoint.
    assert_eq!(
        chase_gate::los_endpoints_bits([0; 3], [0, 0, 1], 1, 1),
        Ok([[0, 0, 1], [0, 0, 2]])
    );
}

#[test]
fn chase_gate_raw_terrain_status_is_not_a_swimming_bit_test() {
    assert!(!chase_gate::terrain_is_in_water(0));
    for status in 1..=u8::MAX {
        assert!(chase_gate::terrain_is_in_water(status));
    }
    assert_eq!(chase_gate::LIQUID_MASK, 15);
    assert_eq!(
        chase_gate::LIQUID_COLLISION_HEIGHT_BITS,
        2.03128_f32.to_bits()
    );
}

#[test]
fn chase_gate_domains_reject_nonfinite_and_negative_reach_without_repair() {
    for invalid in [0x7f80_0000, 0xff80_0000, 0x7fc0_0001] {
        assert_eq!(
            chase_gate::numeric_gate_bits([invalid, 0, 0], [0; 3], 0),
            Err(NumericError::NonFinite)
        );
        assert_eq!(
            chase_gate::numeric_gate_bits([0; 3], [0; 3], invalid),
            Err(NumericError::NonFinite)
        );
        for heights in [[invalid, 0], [0, invalid]] {
            assert_eq!(
                chase_gate::los_endpoints_bits([0; 3], [0; 3], heights[0], heights[1]),
                Err(NumericError::NonFinite)
            );
        }
    }
    for reach in [(-1.0_f32).to_bits(), 0x8000_0001] {
        assert_eq!(
            chase_gate::numeric_gate_bits([0; 3], [0; 3], reach),
            Err(NumericError::NegativeSize)
        );
    }
    for signed_zero in [0, 0x8000_0000] {
        assert_eq!(
            chase_gate::numeric_gate_bits([signed_zero; 3], [0; 3], signed_zero),
            Ok(NumericGate::Pass)
        );
    }
    for sign in [0, 0x8000_0000] {
        let limit = chase_gate::MAX_COORDINATE_BITS | sign;
        assert!(chase_gate::numeric_gate_bits([limit, 0, 0], [limit, 0, 0], 0).is_ok());
        assert_eq!(
            chase_gate::numeric_gate_bits([limit + 1, 0, 0], [0; 3], 0),
            Err(NumericError::Coordinate)
        );
    }
}
