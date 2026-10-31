use super::*;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../benilla-native-geometry-tests/tools/fixtures/wmo_doodad_transform_source")
        .join(name)
}
fn rows(name: &str) -> Vec<Vec<String>> {
    fs::read_to_string(fixture(name))
        .unwrap()
        .lines()
        .filter(|row| !row.starts_with('#') && !row.is_empty())
        .map(|row| row.split('\t').map(str::to_owned).collect())
        .collect()
}
fn bits(text: &str) -> u32 {
    u32::from_str_radix(text, 16).unwrap()
}
fn vector<const N: usize>(row: &[String], at: usize) -> [f32; N] {
    std::array::from_fn(|i| f32::from_bits(bits(&row[at + i])))
}
fn order(bits: u32) -> u32 {
    if bits >> 31 == 0 {
        bits | 0x8000_0000
    } else {
        !bits
    }
}

#[test]
fn portable_profile_quantifies_original_native_record_coordinate_bits() {
    let results: BTreeMap<_, _> = rows("results.tsv")
        .into_iter()
        .map(|row| (row[0].clone(), row))
        .collect();
    let mut admitted = 0;
    let mut rejected = 0;
    let mut mismatches = Vec::new();
    let mut max_ulp = 0;
    let mut max_absolute_error = 0.0f64;
    for input in rows("cases.tsv") {
        assert_eq!(input.len(), 33);
        let expected = &results[&input[0]];
        assert_eq!(expected.len(), 23);
        if expected[5] != "1" {
            continue;
        }
        let original_parent = vector::<3>(&input, 17);
        let disposition = if expected[1] == "1" {
            PlacementDisposition::Emits
        } else if input[1].parse::<u16>().unwrap() & 1 != 0 {
            PlacementDisposition::Destructible
        } else {
            PlacementDisposition::NoCollision
        };
        let parent = parent_position(original_parent, disposition);
        assert_eq!(
            parent.map(f32::to_bits),
            vector::<3>(expected, 2).map(f32::to_bits),
            "{} parent state",
            input[0]
        );
        let result = math::project(
            parent,
            vector(&input, 20),
            vector(&input, 23),
            vector(&input, 26),
        );
        let Ok((position, rotation)) = result else {
            rejected += 1;
            let q = vector::<4>(&input, 26);
            let norm = ((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]) + q[3] * q[3];
            assert!(
                expected[18] == "0"
                    || norm == 0.0
                    || !norm.is_finite()
                    || vector::<3>(&input, 20).iter().any(|x| x.abs() > 360.0),
                "unexpected rejection {} {result:?}",
                input[0]
            );
            continue;
        };
        admitted += 1;
        for (axis, (actual, want)) in position
            .into_iter()
            .chain(rotation)
            .zip(vector::<6>(expected, 11))
            .enumerate()
        {
            let ulp = order(actual.to_bits()).abs_diff(order(want.to_bits()));
            max_ulp = max_ulp.max(ulp);
            max_absolute_error =
                max_absolute_error.max((f64::from(actual) - f64::from(want)).abs());
            if actual.to_bits() != want.to_bits() {
                mismatches.push(format!(
                    "{} axis{axis}: {:08x} != {:08x} ({ulp} ULP)",
                    input[0],
                    actual.to_bits(),
                    want.to_bits()
                ));
            }
        }
    }
    eprintln!("Original-source admitted={admitted} rejected={rejected}; differing fields={} max_ulp={max_ulp} max_absolute_error={max_absolute_error:e}",mismatches.len());
    assert_eq!(admitted, 72);
    assert_eq!(rejected, 7);
    // Explicit exact corpus receipt, not a universal native-libm claim or epsilon.
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn native_or_unknown_numeric_policy_cannot_silently_enter_portable_projection() {
    let profile = DoodadProjectionProfile::from_policy_id(math::POLICY).unwrap();
    assert_eq!(profile.policy_id(), "g3d-portable-libm-0.2.16-v1");
    for other in [
        "",
        "native",
        "g3d-native-linux",
        "g3d-portable-libm-0.2.17-v1",
    ] {
        assert_eq!(
            DoodadProjectionProfile::from_policy_id(other),
            Err(DoodadProjectionError::PolicyMismatch)
        );
        assert_eq!(
            profile.require_match(other),
            Err(DoodadProjectionError::PolicyMismatch)
        );
    }
}
