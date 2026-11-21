// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn binding() -> QueryBinding {
    QueryBinding {
        identity: Identity {
            creature: 1,
            map: 0,
            content: Revision([1; 32]),
            geometry: Revision([2; 32]),
        },
        query_id: 2,
        now_ms: 100,
    }
}
fn point_bits(p: Point) -> [u32; 3] {
    [p.x.bits(), p.y.bits(), p.z.bits()]
}
#[test]
fn sampled_wander_endpoints_match_independent_complete_source_method() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../scripts/native_wander_source_oracle/vectors.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
    );
    assert_eq!(
        fixture["compatibility_policy"],
        "cmangos-computepathtorandompoint-zero-distance-v1"
    );
    let mut count = 0;
    for snapshot in fixture["snapshots"].as_array().unwrap() {
        if !snapshot["case"]
            .as_str()
            .unwrap()
            .starts_with("sample_angle_")
        {
            continue;
        }
        let trace = snapshot["trace"].as_array().unwrap();
        let draws: Vec<u32> = trace
            .iter()
            .filter_map(|v| {
                v.as_str()
                    .unwrap()
                    .strip_prefix("rng:normalized:")
                    .map(|word| word.parse().unwrap())
            })
            .collect();
        assert_eq!(draws.len(), 2);
        let center: Vec<u32> = snapshot["home_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        let request = WanderRequest {
            prepared: binding(),
            center: Point {
                x: Real::from_bits(center[0]).unwrap(),
                y: Real::from_bits(center[1]).unwrap(),
                z: Real::from_bits(center[2]).unwrap(),
            },
            radius: Real::from_bits(snapshot["radius_bits"].as_u64().unwrap() as u32).unwrap(),
            angle_draw: Real::from_bits(draws[0]).unwrap(),
            radius_draw: Real::from_bits(draws[1]).unwrap(),
        };
        let expected: Vec<u32> = snapshot["endpoint_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            point_bits(request.endpoint().unwrap()).as_slice(),
            expected,
            "{}",
            snapshot["case"]
        );
        count += 1;
    }
    assert_eq!(count, 5);
}
#[test]
fn radius_is_linear_and_zero_angle_preserves_source_center_height() {
    let request = WanderRequest {
        prepared: binding(),
        center: Point::new(10., 20., 3.).unwrap(),
        radius: Real::new(8.).unwrap(),
        angle_draw: Real::ZERO,
        radius_draw: Real::new(0.25).unwrap(),
    };
    assert_eq!(
        request.endpoint().unwrap(),
        Point::new(12., 20., 3.).unwrap()
    );
    for (angle, range) in [
        (Real::ONE, Real::ZERO),
        (Real::ZERO, Real::ONE),
        (Real::new(-0.5).unwrap(), Real::ZERO),
    ] {
        assert!(WanderRequest {
            angle_draw: angle,
            radius_draw: range,
            ..request
        }
        .endpoint()
        .is_err());
    }
}
