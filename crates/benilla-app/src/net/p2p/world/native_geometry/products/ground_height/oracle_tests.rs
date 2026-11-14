// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact source callbacks and empty-tree observations, never fabricated geometry.
//! Extreme finite rows exercise the private scalar seam only, not product admission.
use super::*;

#[test]
fn pinned_map_and_ground_methods_match_scalar_results_and_ordered_call_bits() {
    let mut count = 0;
    let mut empty = 0;
    let mut ground = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/ground-height.test-fixture"
    ).lines().filter(|line| !line.starts_with('#') && !line.is_empty()) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 23, "{line}");
        let bits = |i| u32::from_str_radix(row[i], 16).unwrap();
        let value = |i| f32::from_bits(bits(i));
        let p = [value(5), value(6), value(7)];
        let mut static_calls = 0;
        let mut dynamic_calls = 0;
        let result = map_height(p, row[8] == "1", |p, use_vmap, search| {
            static_calls += 1;
            assert_eq!(p.map(f32::to_bits), [bits(12), bits(13), bits(14)], "{line}");
            assert_eq!(u8::from(use_vmap), row[15].parse::<u8>().unwrap(), "{line}");
            assert_eq!(search.to_bits(), bits(16), "{line}");
            Ok(value(9))
        }, |p, range| {
            dynamic_calls += 1;
            assert_eq!(p.map(f32::to_bits), [bits(18), bits(19), bits(20)], "{line}");
            assert_eq!(range.to_bits(), bits(21), "{line}");
            Ok(value(10))
        }).unwrap();
        assert_eq!(static_calls, row[11].parse::<usize>().unwrap(), "{line}");
        assert_eq!(dynamic_calls, row[17].parse::<usize>().unwrap(), "{line}");
        let result = if row[2] == "ground" {
            assert_eq!(row[8], "0");
            ground += 1;
            allowed_ground_z(p[2], result)
        } else {
            assert_eq!(row[2], "map");
            result
        };
        assert_eq!(result.to_bits(), bits(22), "{line}");
        if row[4] == "empty" { assert_eq!(value(10), f32::NEG_INFINITY); empty += 1; }
        count += 1;
    }
    assert_eq!(count, 1422);
    assert!(empty > 0 && ground > 0);
}
