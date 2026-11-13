// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn section(flags: u32, base: f32, maximum: f32, samples: &[u32]) -> Arc<[u8]> {
    let mut bytes = b"MHGT".to_vec();
    for word in [flags, base.to_bits(), maximum.to_bits()] {
        bytes.extend(word.to_le_bytes());
    }
    for &value in samples {
        match flags & 6 {
            2 | 6 => bytes.extend((value as u16).to_le_bytes()),
            4 => bytes.push(value as u8),
            _ => bytes.extend(value.to_le_bytes()),
        }
    }
    bytes.into()
}
fn value(data: &HeightData, local: [f32; 2], holes: &[u16; 256]) -> HeightValue {
    let world = local.map(|v| -v * (GRID_SIZE / 128.0));
    assert_eq!(coordinates(world[0], world[1]).unwrap(), (local, [0, 0]));
    data.get(world[0], world[1], holes).unwrap()
}

#[test]
fn every_center_fan_edge_and_triangle_uses_the_source_plane_in_all_encodings() {
    // Worked source planes for h1=0,h2=8,h3=16,h4=32,center=20:
    // (8,32,0), (24,16,0), (0,24,8), (16,8,8).
    let cases = [
        ([0.0, 0.0], 0.0),
        ([0.5, 0.0], 4.0),
        ([0.0, 0.5], 8.0),
        ([0.25, 0.25], 10.0),
        ([0.75, 0.25], 14.0),
        ([0.25, 0.75], 18.0),
        ([0.5, 0.5], 20.0),
        ([0.75, 0.75], 26.0),
        ([0.375, 0.125], 7.0),
        ([0.125, 0.375], 9.0),
        ([0.875, 0.625], 23.0),
        ([0.625, 0.875], 25.0),
    ];
    for flags in [0, 4, 2] {
        let mut samples = vec![0; V9 + V8];
        for (index, height) in [(129, 8.0f32), (1, 16.0), (130, 32.0), (V9, 20.0)] {
            samples[index] = if flags == 0 {
                height.to_bits()
            } else {
                height as u32
            };
        }
        let max = if flags == 2 { 65535.0 } else { 255.0 };
        let data = HeightData::decode(section(flags, 0.0, max, &samples), 0).unwrap();
        for (local, expected) in cases {
            assert_eq!(
                value(&data, local, &[0; 256]),
                HeightValue::Height(expected),
                "flags={flags}, local={local:?}"
            );
        }
        // The other outer edges belong to the adjacent cell. Its V9 seam must
        // reproduce the same corner/edge value without consulting the new V8.
        for (local, expected) in [
            ([1.0, 0.0], 8.0),
            ([0.0, 1.0], 16.0),
            ([1.0, 1.0], 32.0),
            ([1.0, 0.5], 20.0),
            ([0.5, 1.0], 24.0),
        ] {
            let world = local.map(|v| -v * (GRID_SIZE / 128.0));
            assert_eq!(
                data.get(world[0], world[1], &[0; 256]).unwrap(),
                HeightValue::Height(expected)
            );
        }
    }
}

#[test]
fn flat_and_packed_bypass_holes_but_float_checks_the_exact_row_column_mask() {
    for flags in [0, 1, 2, 4] {
        let sample = if flags == 0 { 7.0f32.to_bits() } else { 7 };
        let samples = if flags == 1 {
            vec![]
        } else {
            vec![sample; V9 + V8]
        };
        let max = if flags == 2 { 65542.0 } else { 262.0 };
        let data = HeightData::decode(section(flags, 7.0, max, &samples), 0).unwrap();
        assert_eq!(
            value(&data, [0.25, 0.25], &[u16::MAX; 256]),
            if flags == 0 {
                HeightValue::Hole
            } else {
                HeightValue::Height(if flags == 1 { 7.0 } else { 14.0 })
            }
        );
    }
    for row in 0..128 {
        for col in 0..128 {
            let mut holes = [0; 256];
            holes[row / 8 * 16 + col / 8] = 1 << (row % 8 / 2 * 4 + col % 8 / 2);
            assert!(is_hole(&holes, [row, col]));
            assert!(!is_hole(&holes, [row, col ^ 2]));
            assert!(!is_hole(&holes, [row ^ 2, col]));
        }
    }
}

#[test]
fn no_height_and_absent_sections_preserve_defaults_and_negative_zero_bits() {
    assert_eq!(
        HeightData::default()
            .get(f32::NAN, f32::INFINITY, &[u16::MAX; 256])
            .unwrap(),
        HeightValue::Height(INVALID_HEIGHT_VALUE)
    );
    for flags in [1, 3, 5, 7] {
        let data = HeightData::decode(section(flags, -0.0, 0.005, &[]), 0).unwrap();
        let HeightValue::Height(height) = data.get(f32::NAN, f32::NAN, &[u16::MAX; 256]).unwrap()
        else {
            panic!()
        };
        assert_eq!(height.to_bits(), 0x8000_0000);
    }
    for flags in [0, 2, 4] {
        let samples = vec![if flags == 0 { (-0.0f32).to_bits() } else { 0 }; V9 + V8];
        let data = HeightData::decode(section(flags, -0.0, -0.0, &samples), 0).unwrap();
        let HeightValue::Height(height) = value(&data, [0.25, 0.25], &[0; 256]) else {
            panic!()
        };
        assert_eq!(
            height.to_bits(),
            0,
            "interpolation converts equal -0 inputs to +0, flags {flags}"
        );
    }
}

#[test]
fn packed_endpoints_half_and_equal_height_ranges_round_after_interpolation() {
    for (flags, max) in [(4, 255), (2, 65535)] {
        for (sample, expected) in [
            (0, -8.0),
            (max, max as f32 - 8.0),
            (max / 2, (max / 2) as f32 - 8.0),
            (max / 2 + 1, (max / 2 + 1) as f32 - 8.0),
        ] {
            let samples = vec![sample; V9 + V8];
            let data =
                HeightData::decode(section(flags, -8.0, max as f32 - 8.0, &samples), 0).unwrap();
            assert_eq!(
                value(&data, [0.25, 0.25], &[0; 256]),
                HeightValue::Height(expected)
            );
            let equal = HeightData::decode(section(flags, 7.0, 7.0, &samples), 0).unwrap();
            assert_eq!(
                value(&equal, [0.25, 0.25], &[0; 256]),
                HeightValue::Height(7.0)
            );
        }
    }
}

#[test]
fn reader_uses_uint16_before_uint8_and_rejects_unknown_flag_combinations_by_name() {
    let samples = vec![65535; V9 + V8];
    let both = HeightData::decode(section(6, -8.0, 65527.0, &samples), 0).unwrap();
    assert_eq!(
        value(&both, [0.5, 0.5], &[u16::MAX; 256]),
        HeightValue::Height(65527.0)
    );
    for flags in [8, 9, 10, 12, 15, u32::MAX] {
        assert_eq!(
            HeightData::decode(section(flags, 0.0, 0.0, &[]), 0).unwrap_err(),
            HeightReadError::InvalidHeaderFlags(flags)
        );
    }
}

#[test]
fn partial_header_v9_and_v8_reads_never_invent_uninitialized_samples() {
    let full = section(0, 0.0, 1.0, &vec![0; V9 + V8]);
    for n in 0..16 {
        assert_eq!(
            HeightData::decode(Arc::from(&full[..n]), 0).unwrap_err(),
            HeightReadError::HeaderReadUndefined
        );
    }
    for (flags, width, expected) in [
        (0, 4, HeightQueryError::FloatSampleReadUndefined),
        (4, 1, HeightQueryError::Uint8SampleReadUndefined),
        (2, 2, HeightQueryError::Uint16SampleReadUndefined),
    ] {
        let full = section(flags, 0.0, 1.0, &vec![0; V9 + V8]);
        for n in [16, 16 + V9 * width - 1, 16 + V9 * width] {
            let partial = HeightData::decode(Arc::from(&full[..n]), 0).unwrap();
            assert_eq!(partial.get(0.0, 0.0, &[0; 256]), Err(expected));
            if flags == 0 {
                assert_eq!(
                    partial.get(0.0, 0.0, &[u16::MAX; 256]).unwrap(),
                    HeightValue::Hole
                );
            }
        }
        let partial = HeightData::decode(Arc::from(&full[..full.len() - 1]), 0).unwrap();
        assert!(
            partial.get(0.0, 0.0, &[0; 256]).is_ok(),
            "unreached last V8 sample"
        );
        let far = -127.5 * (GRID_SIZE / 128.0);
        assert_eq!(partial.get(far, far, &[0; 256]), Err(expected));
        let complete = HeightData::decode(full, 0).unwrap();
        assert!(complete.get(far, far, &[0; 256]).is_ok());
    }
}

#[test]
fn coordinate_conversion_refuses_the_cpp_undefined_domain_without_saturating() {
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        -f32::MAX,
    ] {
        assert_eq!(
            coordinates(value, 0.0),
            Err(HeightQueryError::CoordinateConversionUndefined)
        );
        assert_eq!(
            coordinates(0.0, value),
            Err(HeightQueryError::CoordinateConversionUndefined)
        );
    }
    assert_eq!(coordinates(0.0, -0.0).unwrap(), ([0.0, 0.0], [0, 0]));
    // Source truncation permits negative fractions and wraps the integer index.
    assert_eq!(
        coordinates(32.0 * GRID_SIZE + GRID_SIZE / 256.0, 0.0).unwrap(),
        ([-0.5, 0.0], [0, 0])
    );
}

#[test]
fn selector_covers_disabled_branches_each_retry_and_equal_signed_zero_selection() {
    for (use_vmaps, enabled) in [(false, false), (false, true), (true, false)] {
        assert_eq!(
            select_height::<()>(7.0, 0.0, use_vmaps, enabled, 10.0, |_, _| panic!(
                "disabled query"
            ))
            .unwrap(),
            7.0
        );
    }
    for hit_at in 0..4 {
        let mut calls = vec![];
        let result = select_height::<()>(100.0, 0.0, true, true, 50.0, |z, search| {
            calls.push([z, search]);
            Ok(if calls.len() == hit_at + 1 {
                6.0
            } else {
                INVALID_HEIGHT_VALUE
            })
        })
        .unwrap();
        assert_eq!(result, 6.0);
        assert_eq!(
            calls,
            [[2.0, 50.0], [2.0, 10000.0], [2.0, -50.0], [102.0, 10.0]][..=hit_at]
        );
    }
    let mut calls = vec![];
    assert_eq!(
        select_height::<()>(-10.0, 100.0, true, true, 10.0, |z, search| {
            calls.push([z, search]);
            Ok(-5.0)
        })
        .unwrap(),
        -5.0
    );
    assert_eq!(calls, [[102.0, 113.0]]);
    assert_eq!(
        select_height::<()>(-0.0, 0.0, true, true, 10.0, |_, _| Ok(0.0))
            .unwrap()
            .to_bits(),
        0x8000_0000
    );
    assert_eq!(
        select_height::<()>(0.0, -1.0, true, true, 10.0, |_, _| Ok(-0.0))
            .unwrap()
            .to_bits(),
        0x8000_0000
    );
    for fail_at in 0..4 {
        let mut calls = 0;
        assert_eq!(
            select_height(100.0, 0.0, true, true, 50.0, |_, _| {
                calls += 1;
                if calls == fail_at + 1 {
                    Err(fail_at)
                } else {
                    Ok(INVALID_HEIGHT_VALUE)
                }
            }),
            Err(fail_at)
        );
        assert_eq!(calls, fail_at + 1);
    }
}

#[test]
fn original_source_packed_and_absent_height_oracle_rows_match_without_new_acquisition() {
    // Existing authored source-loader oracle, not newly run C++ or game products.
    const ROOT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benilla-native-geometry-tests/tools/fixtures/"
    );
    let mut profiles = Vec::new();
    for profile in 16..=18 {
        let bytes: Arc<[u8]> =
            std::fs::read(format!("{ROOT}terrain-area/{profile}.map.test-fixture"))
                .unwrap()
                .into();
        let height = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let holes_at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
        let mut holes = [0; 256];
        if holes_at != 0 {
            for (i, hole) in holes.iter_mut().enumerate() {
                *hole = u16::from_le_bytes(
                    bytes[holes_at + i * 2..holes_at + i * 2 + 2]
                        .try_into()
                        .unwrap(),
                );
            }
        }
        let data = if height == 0 {
            HeightData::default()
        } else {
            HeightData::decode(bytes, height).unwrap()
        };
        profiles.push((data, holes));
    }
    let rows = std::fs::read_to_string(format!("{ROOT}terrain_area.tsv")).unwrap();
    let mut count = 0;
    for row in rows
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = row.split('\t').collect();
        let profile: usize = row[2].parse().unwrap();
        if !(16..=18).contains(&profile) {
            continue;
        }
        let number = |i: usize| f32::from_bits(u32::from_str_radix(row[i], 16).unwrap());
        let (data, holes) = &profiles[profile - 16];
        let result = match data.get(number(5), number(6), holes).unwrap() {
            HeightValue::Height(height) => height,
            HeightValue::Hole => INVALID_HEIGHT_VALUE,
        };
        assert_eq!(result.to_bits(), number(15).to_bits(), "{row:?}");
        count += 1;
    }
    assert_eq!(count, 3024);
}

#[test]
fn nonunit_packed_multipliers_have_exact_endpoint_and_half_bits() {
    // Worked binary32 values: 10/255 = 0x3d20a0a1 and
    // 10/65535 = 0x392000a0. Multiply before the final -3 addition.
    for (flags, cases) in [
        (
            4,
            [
                (0, 0xc0400000),
                (127, 0x3ffd7d80),
                (128, 0x40014142),
                (255, 0x40e00000),
            ],
        ),
        (
            2,
            [
                (0, 0xc0400000),
                (32767, 0x3ffffd80),
                (32768, 0x40000140),
                (65535, 0x40e00000),
            ],
        ),
    ] {
        for (sample, bits) in cases {
            let data =
                HeightData::decode(section(flags, -3.0, 7.0, &vec![sample; V9 + V8]), 0).unwrap();
            let HeightValue::Height(height) = value(&data, [0.5, 0.5], &[0; 256]) else {
                panic!()
            };
            assert_eq!(height.to_bits(), bits);
        }
    }
}

#[test]
fn adjacent_world_inputs_straddle_each_float_to_int_undefined_boundary() {
    for bits in [0xd0055544, 0x50055566] {
        assert!(coordinates(f32::from_bits(bits), 0.0).is_ok());
    }
    for bits in [0xd0055545, 0x50055567] {
        assert_eq!(
            coordinates(f32::from_bits(bits), 0.0),
            Err(HeightQueryError::CoordinateConversionUndefined)
        );
    }
}
