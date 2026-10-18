use super::*;

const MEMBER: &str = "World\\wmo\\Authored\\ordinary.wmo";

fn source_convert(member: &str, root: &[u8], groups: &[Vec<u8>]) -> Result<WmoCollision, WmoError> {
    let groups: Vec<_> = groups
        .iter()
        .enumerate()
        .map(|(index, bytes)| WmoGroupInput {
            group_index: index as u32,
            bytes,
        })
        .collect();
    extract_wmo_collision_from_source(member, root, &groups)
}

fn liquid_payload(heights: &[f32], flags: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 30];
    for (at, value) in [(0, 2), (4, 2), (8, 1), (12, 1)] {
        set_word(&mut bytes, at, value);
    }
    for (at, value) in [(16, -0.0f32), (20, 1.25), (24, -4.5)] {
        set_word(&mut bytes, at, value.to_bits());
    }
    bytes[28..30].copy_from_slice(&0x7fffu16.to_le_bytes());
    for &height in heights {
        bytes.extend_from_slice(&[0xff, 0xff, 0xfe, 0xff]);
        put_word(&mut bytes, height.to_bits());
    }
    bytes.extend_from_slice(flags);
    bytes
}

fn liquid_group(flags: u32, group_type: u32, payload: &[u8], nested: bool) -> Vec<u8> {
    let mut bytes = group(flags, 0, &[], &[], &[], &[]);
    set_word(&mut bytes, 20 + 52, group_type);
    let liquid = chunk(b"MLIQ", payload);
    if nested {
        let size = word(&bytes, 16);
        set_word(&mut bytes, 16, size + liquid.len() as u32);
    }
    bytes.extend(liquid);
    bytes
}

fn liquid_root(flags: u32, groups: u32) -> Vec<u8> {
    let mut bytes = root(groups, b"room\0");
    set_word(&mut bytes, 20 + 60, flags);
    bytes
}

fn ordinary_liquid() -> Vec<u8> {
    liquid_payload(&[0.0, 1.0, 2.0, 3.0], &[0])
}

#[test]
fn liquid_source_projection_preserves_header_height_bits_flags_and_advertised_size_quirk() {
    let heights = [-0.0, f32::from_bits(1), f32::MAX, f32::MIN];
    let payload = liquid_payload(&heights, &[0xa2]);
    for nested in [false, true] {
        let input = liquid_group(0x80000, 0, &payload, nested);
        let output = source_convert(MEMBER, &liquid_root(4, 1), &[input]).unwrap();
        assert_eq!(output.liquid_group_count, 1);
        assert_eq!(output.retained_group_count, 1);
        assert_eq!(output.collision_triangle_count, 0);
        let bytes = output.raw_vmap;
        let at = 20 + GROUP_FIXED_BYTES;
        assert_eq!(word(&bytes, 52), 1);
        assert_eq!(&bytes[at..at + 4], b"LIQU");
        assert_eq!(word(&bytes, at + 4), 32 + 4 * 8 + 1);
        assert_eq!(&bytes[at + 8..at + 36], &payload[..28]);
        assert_eq!(&bytes[at + 36..at + 38], &3u16.to_le_bytes());
        assert_eq!(&bytes[at + 38..at + 40], &[0, 0]);
        let expected: Vec<_> = heights.iter().flat_map(|f| f.to_le_bytes()).collect();
        assert_eq!(&bytes[at + 40..at + 56], expected);
        assert_eq!(bytes[at + 56], 0xa2);
        assert_eq!(bytes.len(), at + 8 + 32 + 4 * 4 + 1);
        assert_eq!(bytes.len() + 4 * 4, at + 8 + word(&bytes, at + 4) as usize);
    }
}

#[test]
fn all_source_type_precedence_flags_and_signed_short_bits_are_preserved() {
    for root_flags in [0, 4, 0xffff_ffff] {
        for group_flags in [0, 0x80000] {
            for group_type in [
                0,
                1,
                2,
                3,
                4,
                5,
                14,
                15,
                16,
                19,
                20,
                21,
                22,
                32767,
                32768,
                65535,
                65536,
                u32::MAX,
            ] {
                for tile in [0x0f, 0x00, 0xa1, 0x82, 0xb3, 0x0e] {
                    for special in [false, true] {
                        let member = if special {
                            "World\\Stratholme_raid.wmo"
                        } else {
                            MEMBER
                        };
                        let payload = liquid_payload(&[0.0; 4], &[tile]);
                        let input = liquid_group(group_flags, group_type, &payload, true);
                        let output =
                            source_convert(member, &liquid_root(root_flags, 1), &[input]).unwrap();
                        let mut selected = match (root_flags & 4, group_type) {
                            (0, 15) => 0,
                            (0, n) => n.wrapping_add(1),
                            (_, n) => n,
                        };
                        if selected == 0 && tile & 15 != 15 {
                            selected = u32::from(tile & 15) + 1;
                        }
                        let expected = match selected {
                            1 | 5 | 9 | 13 | 17 => {
                                if group_flags == 0 {
                                    1
                                } else {
                                    2
                                }
                            }
                            2 | 6 | 10 | 14 | 18 => 2,
                            3 | 7 | 11 | 15 | 19 => 3,
                            4 | 8 | 12 | 16 | 20 => {
                                if special {
                                    21
                                } else {
                                    4
                                }
                            }
                            other => other,
                        } as u16;
                        assert_eq!(
                            &output.raw_vmap[128..130],
                            expected.to_le_bytes(),
                            "{root_flags} {group_flags} {group_type} {tile} {member}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn filename_override_uses_exact_case_sensitive_full_source_member() {
    let input = liquid_group(0, 4, &ordinary_liquid(), true);
    for (member, expected) in [
        ("World\\Stratholme_raid.wmo", 21u16),
        ("World\\Stratholme_raid\\room.wmo", 21),
        ("World\\prefixStratholme_raidSuffix.wmo", 21),
        ("World\\stratholme_raid.wmo", 4),
        ("World\\STRATHOLME_RAID.wmo", 4),
        ("World\\Stratholme_Raid.wmo", 4),
    ] {
        let output =
            source_convert(member, &liquid_root(4, 1), std::slice::from_ref(&input)).unwrap();
        assert_eq!(&output.raw_vmap[128..130], expected.to_le_bytes());
    }
}

#[test]
fn fallback_uses_first_enabled_low_nibble_and_keeps_disabled_flags_unchanged() {
    let mut payload = liquid_payload(&[0.0; 6], &[0xff, 0xb3]);
    set_word(&mut payload, 0, 3);
    set_word(&mut payload, 8, 2);
    let input = liquid_group(0, 15, &payload, true);
    let output = source_convert(MEMBER, &liquid_root(0, 1), &[input]).unwrap();
    assert_eq!(&output.raw_vmap[128..130], 4u16.to_le_bytes());
    assert_eq!(&output.raw_vmap[output.raw_vmap.len() - 2..], &[0xff, 0xb3]);
    *payload.last_mut().unwrap() = 0x0f;
    let input = liquid_group(0, 15, &payload, true);
    let output = source_convert(MEMBER, &liquid_root(0, 1), &[input]).unwrap();
    assert_eq!(&output.raw_vmap[128..130], 0u16.to_le_bytes());
    assert_eq!(output.liquid_group_count, 1);
}

#[test]
fn every_following_chunk_resets_liquid_and_last_mliq_replaces_earlier_payloads() {
    for nested in [false, true] {
        let original = liquid_group(0, 0, &ordinary_liquid(), nested);
        for tag in [b"ZZZZ", b"MODR", b"MONR", b"MOTV"] {
            let mut reset = original.clone();
            reset.extend(chunk(tag, &[]));
            let output = source_convert(MEMBER, &liquid_root(0, 1), &[reset]).unwrap();
            assert_eq!(output.liquid_group_count, 0);
            assert_eq!(word(&output.raw_vmap, 52), 0);
            assert_eq!(
                output.raw_vmap,
                convert(&liquid_root(0, 1), &[group(0, 0, &[], &[], &[], &[])])
                    .unwrap()
                    .raw_vmap
            );
        }
        let mut replaced = original;
        replaced.extend(chunk(b"ZZZZ", &[]));
        replaced.extend(chunk(b"MLIQ", &liquid_payload(&[9.0; 4], &[0x82])));
        let output = source_convert(MEMBER, &liquid_root(4, 1), &[replaced]).unwrap();
        assert_eq!(output.liquid_group_count, 1);
        assert_eq!(&output.raw_vmap[128..130], 3u16.to_le_bytes());
        assert_eq!(
            &output.raw_vmap[132..148],
            [9.0f32.to_le_bytes(); 4].concat()
        );
    }
    // The MOGP header and each recognized geometry child also reset state.
    let original = group(0, 0, &[], &[], &[], &[]);
    for insertion in [12, 88, 96, 104, 112] {
        let mut with_liquid = original.clone();
        with_liquid.splice(insertion..insertion, chunk(b"MLIQ", &ordinary_liquid()));
        if insertion > 12 {
            let size = word(&with_liquid, 16);
            set_word(
                &mut with_liquid,
                16,
                size + (8 + ordinary_liquid().len()) as u32,
            );
        }
        let output = source_convert(MEMBER, &liquid_root(0, 1), &[with_liquid]).unwrap();
        assert_eq!(output.liquid_group_count, 0);
    }
}

#[test]
fn liquid_validation_applies_to_skipped_reset_and_replaced_chunks() {
    for flags in [0, 0x80, 0x400_0000] {
        for trailing in [None, Some(*b"ZZZZ"), Some(*b"MLIQ")] {
            for mut payload in [vec![], vec![0; 29], ordinary_liquid()] {
                if payload.len() >= 30 {
                    set_word(&mut payload, 0, 0);
                }
                let mut input = liquid_group(flags, 0, &payload, true);
                if let Some(tag) = trailing {
                    let trailing_payload = if tag == *b"MLIQ" {
                        ordinary_liquid()
                    } else {
                        vec![]
                    };
                    input.extend(chunk(&tag, &trailing_payload));
                }
                assert!(source_convert(MEMBER, &liquid_root(0, 1), &[input]).is_err());
            }
        }
        let input = liquid_group(flags, 0, &ordinary_liquid(), true);
        let output = source_convert(MEMBER, &liquid_root(0, 1), &[input]).unwrap();
        assert_eq!(output.liquid_group_count, u32::from(flags == 0));
    }
    let mut named = liquid_root(0, 1);
    named.truncate(84);
    named.extend(chunk(b"MOGN", b"antiportal\0"));
    let output = source_convert(
        MEMBER,
        &named,
        &[liquid_group(0, 0, &ordinary_liquid(), true)],
    )
    .unwrap();
    assert_eq!(output.liquid_group_count, 0);
    assert_eq!(output.raw_vmap.len(), 20);
}

#[test]
fn liquid_dimensions_reject_zero_negative_mismatched_and_overflowing_shapes() {
    for dimensions in [
        [0, 2, 1, 1],
        [2, 0, 1, 1],
        [2, 2, 0, 1],
        [2, 2, 1, 0],
        [-1, 2, 1, 1],
        [2, -1, 1, 1],
        [2, 2, -1, 1],
        [2, 2, 1, -1],
        [3, 2, 1, 1],
        [2, 3, 1, 1],
        [2, 2, i32::MAX, 1],
        [i32::MAX, 2, i32::MAX - 1, 1],
        [65537, 65537, 65536, 65536],
        [16384, 16384, 16383, 16383],
        [1025, 1024, 1024, 1023],
    ] {
        let mut payload = ordinary_liquid();
        for (i, value) in dimensions.into_iter().enumerate() {
            set_word(&mut payload, i * 4, value as u32);
        }
        let input = liquid_group(0, 0, &payload, true);
        assert!(
            matches!(
                source_convert(MEMBER, &liquid_root(0, 1), &[input]),
                Err(WmoError::InvalidLiquidDimensions { .. }
                    | WmoError::LiquidSizeOverflow { .. }
                    | WmoError::CountLimit { .. })
            ),
            "{dimensions:?}"
        );
    }
}

#[test]
fn liquid_exact_payload_length_and_all_finite_fields_are_required() {
    let original = ordinary_liquid();
    for len in 0..original.len() {
        let input = liquid_group(0, 0, &original[..len], true);
        assert!(
            source_convert(MEMBER, &liquid_root(0, 1), &[input]).is_err(),
            "prefix {len}"
        );
    }
    let mut extra = original.clone();
    extra.push(0);
    assert!(
        matches!(source_convert(MEMBER, &liquid_root(0, 1), &[liquid_group(0, 0, &extra, true)]), Err(WmoError::InvalidChunkSize { chunk: tag, .. }) if tag == *b"MLIQ")
    );
    for at in [16, 20, 24, 34, 42, 50, 58] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut payload = original.clone();
            set_word(&mut payload, at, value.to_bits());
            let input = liquid_group(0x80, 0, &payload, true);
            assert!(matches!(
                source_convert(MEMBER, &liquid_root(0, 1), &[input]),
                Err(WmoError::NonFinite { .. } | WmoError::NonFiniteLiquidHeight { .. })
            ));
        }
    }
}

#[test]
fn source_member_is_bounded_ascii_safe_root_path_without_case_rewriting() {
    for member in [
        "",
        ".wmo",
        "true",
        "a.WMO",
        "a.wmo.extra",
        "a\0.wmo",
        "a/room.wmo",
        "..\\a.wmo",
        "a\\..\\b.wmo",
        "a\\.\\b.wmo",
        "\\a.wmo",
        "a\\\\b.wmo",
        "a\\ b.wmo",
        "a \\b.wmo",
        "a.\\b.wmo",
        "é.wmo",
    ] {
        assert_eq!(
            source_convert(member, &root(0, b""), &[]),
            Err(WmoError::InvalidSourceRootMember),
            "{member:?}"
        );
    }
    let longest = format!("{}.wmo", "a".repeat(507));
    assert!(source_convert(&longest, &root(0, b""), &[]).is_ok());
    assert_eq!(
        source_convert(&format!("a{longest}"), &root(0, b""), &[]),
        Err(WmoError::InvalidSourceRootMember)
    );
}

#[test]
fn source_profile_dry_output_and_limits_remain_identical() {
    let groups = [ordinary_group()];
    let root = root(1, b"room\0");
    assert_eq!(
        source_convert(MEMBER, &root, &groups).unwrap(),
        convert(&root, &groups).unwrap()
    );
    let input = liquid_group(0, 0, &ordinary_liquid(), true);
    assert_eq!(
        convert(&root, &[input]),
        Err(WmoError::UnsupportedLiquid { group: 0 })
    );
    let input = liquid_group(0, 0, &ordinary_liquid(), true);
    let parsed = parse_group(&input, 0, LiquidProfile::Source(MEMBER)).unwrap();
    assert_eq!(
        output_size(
            MAX_WMO_OUTPUT_BYTES - GROUP_FIXED_BYTES,
            &parsed,
            CollisionCounts {
                triangles: 0,
                vertices: 0
            }
        ),
        Err(WmoError::OutputLimit {
            maximum: MAX_WMO_OUTPUT_BYTES
        })
    );
}

#[test]
fn nested_liquid_chunk_and_container_boundaries_fail_closed_on_every_truncated_prefix() {
    let original = liquid_group(0, 0, &ordinary_liquid(), true);
    let root = liquid_root(0, 1);
    for end in 0..original.len() {
        assert!(
            source_convert(MEMBER, &root, &[original[..end].to_vec()]).is_err(),
            "prefix {end}"
        );
    }
    for size in [0, 29, 30, 31, 62, 64, u32::MAX] {
        let mut input = original.clone();
        set_word(&mut input, 124, size);
        assert!(
            source_convert(MEMBER, &root, &[input]).is_err(),
            "size {size}"
        );
    }
    let mut trailing = original;
    trailing.extend_from_slice(b"abc");
    assert!(matches!(
        source_convert(MEMBER, &root, &[trailing]),
        Err(WmoError::TruncatedChunkHeader { .. })
    ));
}

#[test]
fn liquid_maximum_vertex_count_is_accepted_without_serializing_source_attributes() {
    let vertex_count = MAX_WMO_LIQUID_VERTICES;
    let tile_count = 1023 * 1023;
    let mut payload = vec![0; 30 + vertex_count * 8 + tile_count];
    for (at, value) in [(0, 1024), (4, 1024), (8, 1023), (12, 1023)] {
        set_word(&mut payload, at, value);
    }
    set_word(&mut payload, 30 + (vertex_count - 1) * 8, 0xffff_ffff);
    set_word(
        &mut payload,
        30 + (vertex_count - 1) * 8 + 4,
        (-0.0f32).to_bits(),
    );
    let input = liquid_group(0, 1, &payload, true);
    let output = source_convert(MEMBER, &liquid_root(4, 1), &[input]).unwrap();
    assert_eq!(output.liquid_group_count, 1);
    assert_eq!(
        output.raw_vmap.len(),
        20 + GROUP_FIXED_BYTES + 8 + 32 + vertex_count * 4 + tile_count
    );
    assert_eq!(
        &output.raw_vmap[132 + (vertex_count - 1) * 4..132 + vertex_count * 4],
        &(-0.0f32).to_le_bytes()
    );
}
