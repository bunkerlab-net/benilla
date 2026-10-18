use super::*;

const VERTICES: [[f32; 3]; 6] = [
    [-0.0, 0.0, -0.0],
    [1.0, -0.0, 2.0],
    [3.0, 4.0, 5.0],
    [6.0, 7.0, 8.0],
    [9.0, 10.0, 11.0],
    [12.0, 13.0, 14.0],
];

fn set_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes: Vec<_> = tag.iter().rev().copied().collect();
    put_word(&mut bytes, payload.len() as u32);
    bytes.extend_from_slice(payload);
    bytes
}

fn root(groups: u32, names: &[u8]) -> Vec<u8> {
    let mut header = [0; 64];
    set_word(&mut header, 4, groups);
    set_word(&mut header, 32, 314_159);
    let mut bytes = chunk(b"MVER", &17u32.to_le_bytes());
    bytes.extend(chunk(b"MOHD", &header));
    bytes.extend(chunk(b"MOGN", names));
    bytes
}

fn group(
    flags: u32,
    name: i32,
    materials: &[u8],
    indices: &[u16],
    vertices: &[[f32; 3]],
    batches: &[u16],
) -> Vec<u8> {
    let mut header = vec![0; 68];
    set_word(&mut header, 0, name as u32);
    set_word(&mut header, 8, flags);
    set_word(&mut header, 56, 271_828);
    for (at, value) in [
        (12, -20.0f32),
        (16, -20.0),
        (20, -20.0),
        (24, 20.0),
        (28, 20.0),
        (32, 20.0),
    ] {
        set_word(&mut header, at, value.to_bits());
    }
    let mopy: Vec<_> = materials.iter().flat_map(|&flag| [flag, 0xfe]).collect();
    header.extend(chunk(b"MOPY", &mopy));
    let movi: Vec<_> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    header.extend(chunk(b"MOVI", &movi));
    let movt: Vec<_> = vertices
        .iter()
        .flatten()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    header.extend(chunk(b"MOVT", &movt));
    let moba: Vec<_> = batches
        .iter()
        .flat_map(|&entry| {
            let mut record = [0xa5; 24];
            record[16..18].copy_from_slice(&entry.to_le_bytes());
            record
        })
        .collect();
    header.extend(chunk(b"MOBA", &moba));
    let mut bytes = chunk(b"MVER", &17u32.to_le_bytes());
    bytes.extend(chunk(b"MOGP", &header));
    bytes
}

fn ordinary_group() -> Vec<u8> {
    group(
        0x48,
        0,
        &[8, 32, 4, 36, 12],
        &[4, 2, 0, 2, 4, 3, 1, 2, 3, 5, 3, 2, 4, 0, 3],
        &VERTICES,
        &[65_535, 17],
    )
}

fn convert(root: &[u8], groups: &[Vec<u8>]) -> Result<WmoCollision, WmoError> {
    let groups: Vec<_> = groups
        .iter()
        .enumerate()
        .map(|(i, bytes)| WmoGroupInput {
            group_index: i as u32,
            bytes,
        })
        .collect();
    extract_wmo_collision(root, &groups)
}

#[test]
fn source_small_filter_compacts_in_original_order_preserving_winding_and_batches() {
    let source = ordinary_group();
    let result = convert(&root(1, b"room\0"), std::slice::from_ref(&source)).unwrap();
    assert_eq!(result.declared_group_count, 1);
    assert_eq!(result.retained_group_count, 1);
    assert_eq!(result.collision_triangle_count, 3);
    let output = result.raw_vmap;
    assert_eq!(&output[..8], b"VMAPs05\0");
    assert_eq!(word(&output, 8), 3); // Four retained vertices, three triangles.
    assert_eq!(word(&output, 12), 1);
    assert_eq!(word(&output, 16), 314_159);
    assert_eq!(word(&output, 20), 0x48);
    assert_eq!(word(&output, 24), 271_828);
    assert_eq!(&output[28..52], &source[32..56]);
    assert_eq!(word(&output, 52), 0);
    assert_eq!(&output[56..60], b"GRP ");
    assert_eq!(word(&output, 60), 12);
    assert_eq!(word(&output, 64), 2);
    assert_eq!(word(&output, 68), 65_535);
    assert_eq!(word(&output, 72), 17);
    assert_eq!(&output[76..80], b"INDX");
    assert_eq!(word(&output, 80), 22);
    assert_eq!(word(&output, 84), 9);
    let actual: Vec<_> = output[88..106]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u16::from_le_bytes(*v))
        .collect();
    assert_eq!(actual, [3, 1, 0, 1, 3, 2, 3, 0, 2]);
    assert_eq!(&output[106..110], b"VERT");
    assert_eq!(word(&output, 110), 52);
    assert_eq!(word(&output, 114), 4);
    let expected: Vec<_> = [0, 2, 3, 4]
        .into_iter()
        .flat_map(|i| VERTICES[i])
        .flat_map(|f| f.to_le_bytes())
        .collect();
    assert_eq!(&output[118..], expected);
    assert_eq!(source, ordinary_group());
}

#[test]
fn every_material_byte_uses_collision_or_render_without_detail() {
    for flag in 0u8..=255 {
        let group = group(0, 0, &[flag], &[2, 1, 0], &VERTICES, &[]);
        let result = convert(&root(1, b"\0"), &[group]).unwrap();
        let expected = flag & 8 != 0 || (flag & 32 != 0 && flag & 4 == 0);
        assert_eq!(
            result.collision_triangle_count,
            u32::from(expected),
            "flag {flag:#x}"
        );
        assert_eq!(result.retained_group_count, 1);
    }
}

#[test]
fn every_root_declared_group_is_required_in_exact_source_order_even_if_skipped() {
    let bytes = ordinary_group();
    let root = root(2, b"room\0");
    for supplied in [0, 1, 3] {
        let inputs = vec![
            WmoGroupInput {
                group_index: 0,
                bytes: &bytes
            };
            supplied
        ];
        assert_eq!(
            extract_wmo_collision(&root, &inputs),
            Err(WmoError::GroupCountMismatch {
                declared: 2,
                supplied
            })
        );
    }
    for (actual, expected) in [(1, 0), (2, 0), (u32::MAX, 0)] {
        let inputs = [
            WmoGroupInput {
                group_index: actual,
                bytes: &bytes,
            },
            WmoGroupInput {
                group_index: 1,
                bytes: &bytes,
            },
        ];
        assert_eq!(
            extract_wmo_collision(&root, &inputs),
            Err(WmoError::GroupOrder { expected, actual })
        );
    }
    let inputs = [WmoGroupInput {
        group_index: 0,
        bytes: &bytes,
    }; 2];
    assert_eq!(
        extract_wmo_collision(&root, &inputs),
        Err(WmoError::GroupOrder {
            expected: 1,
            actual: 0
        })
    );
    let missing_skipped = group(0x80, -1, &[], &[], &[], &[]);
    assert!(convert(
        &root,
        &[bytes, missing_skipped[..missing_skipped.len() - 1].to_vec()]
    )
    .is_err());
}

#[test]
fn source_skip_flags_precede_name_dereference_and_names_are_case_sensitive() {
    for flags in [0x80, 0x400_0000, 0x400_0080] {
        let skipped = group(flags, -1, &[], &[], &[], &[]);
        let result = convert(&root(1, b"antiporta"), &[skipped]).unwrap();
        assert_eq!(result.retained_group_count, 0);
        assert_eq!(result.raw_vmap.len(), 20);
    }
    for (names, offset, skip) in [
        (b"antiportal\0".as_slice(), 0, true),
        (b"xantiportal\0".as_slice(), 1, true),
        (b"Antiportal\0".as_slice(), 0, false),
        (b"antiportalx\0".as_slice(), 0, false),
        (b"antiportal\0".as_slice(), 11, false),
        (b"".as_slice(), i32::MAX, false),
    ] {
        let result = convert(&root(1, names), &[group(0, offset, &[], &[], &[], &[])]).unwrap();
        assert_eq!(result.retained_group_count, u32::from(!skip));
    }
    assert_eq!(
        convert(&root(1, b""), &[group(0, -1, &[], &[], &[], &[])]),
        Err(WmoError::NegativeGroupName {
            group: 0,
            offset: -1
        })
    );
    for length in 1..=10 {
        assert_eq!(
            convert(
                &root(1, &b"antiportal"[..length]),
                &[group(0, 0, &[], &[], &[], &[])]
            ),
            Err(WmoError::UnterminatedGroupName {
                group: 0,
                offset: 0
            })
        );
    }
}

#[test]
fn retained_group_order_and_empty_groups_are_preserved() {
    let mut first = ordinary_group();
    set_word(&mut first, 20 + 56, 900);
    let skipped = group(0x80, -1, &[], &[], &[], &[]);
    let mut empty = group(0, 0, &[4], &[0, 0, 0], &VERTICES, &[222]);
    set_word(&mut empty, 20 + 56, 2);
    let result = convert(&root(3, b"\0"), &[first.clone(), skipped, empty]).unwrap();
    assert_eq!(result.retained_group_count, 2);
    assert_eq!(result.retained_group_indices, [0, 2]);
    assert_eq!(result.collision_triangle_count, 3);
    assert_eq!(word(&result.raw_vmap, 24), 900);
    let first_length = convert(&root(1, b"\0"), &[first]).unwrap().raw_vmap.len();
    assert_eq!(word(&result.raw_vmap, first_length + 4), 2);
    assert_eq!(result.raw_vmap.len(), first_length + GROUP_FIXED_BYTES + 4);
    assert_eq!(convert(&root(0, b""), &[]).unwrap().raw_vmap.len(), 20);
}

#[test]
fn source_mogp_68_byte_boundary_supports_nested_and_header_only_forms() {
    let nested = ordinary_group();
    let mut siblings = nested.clone();
    set_word(&mut siblings, 16, 68);
    assert_eq!(
        convert(&root(1, b"room\0"), std::slice::from_ref(&nested)),
        convert(&root(1, b"room\0"), &[siblings])
    );
    for size in [0, 1, 64, 67, 69, 70, 75, 77, u32::MAX] {
        let mut malformed = nested.clone();
        set_word(&mut malformed, 16, size);
        assert!(
            convert(&root(1, b"room\0"), &[malformed]).is_err(),
            "size {size}"
        );
    }
}

#[test]
fn all_truncated_root_and_group_prefixes_reject() {
    let root = root(1, b"room\0");
    let group = ordinary_group();
    // The MOGN is optional to the source; truncating exactly before it gives a
    // valid shorter root, so use a root whose required MOHD is the last chunk.
    let required_root = &root[..84];
    for length in 0..required_root.len() {
        assert!(
            convert(&required_root[..length], std::slice::from_ref(&group)).is_err(),
            "root prefix {length}"
        );
    }
    for length in 0..group.len() {
        assert!(
            extract_wmo_collision(
                &root,
                &[WmoGroupInput {
                    group_index: 0,
                    bytes: &group[..length]
                }]
            )
            .is_err(),
            "group prefix {length}"
        );
    }
    for suffix in [vec![0], vec![0; 7], chunk(b"ZZZZ", &[1])[..8].to_vec()] {
        let mut malformed = group.clone();
        malformed.extend(suffix);
        assert!(convert(&root, &[malformed]).is_err());
    }
}

#[test]
fn liquid_is_explicitly_rejected_even_empty_skipped_or_followed_by_state_reset() {
    for flags in [0, 0x80, 0x400_0000] {
        for payload in [vec![], vec![0; 30], vec![0xff; 61]] {
            let mut input = group(flags, 0, &[], &[], &[], &[]);
            input.extend(chunk(b"MLIQ", &payload));
            for trailing in [false, true] {
                let mut input = input.clone();
                if trailing {
                    input.extend(chunk(b"ZZZZ", &[]));
                }
                assert_eq!(
                    convert(&root(1, b"room\0"), &[input]),
                    Err(WmoError::UnsupportedLiquid { group: 0 })
                );
            }
        }
    }
    let mut input = ordinary_group();
    let expected = convert(&root(1, b"room\0"), &[input.clone()]).unwrap();
    set_word(&mut input, 20 + 52, u32::MAX);
    assert_eq!(convert(&root(1, b"room\0"), &[input]).unwrap(), expected);
}

#[test]
fn all_indices_and_vertices_are_checked_even_when_filtered_or_skipped() {
    for flags in [0, 0x80] {
        for material in [4, 8] {
            let invalid = group(flags, 0, &[material], &[0, 1, 6], &VERTICES, &[]);
            assert_eq!(
                convert(&root(1, b""), &[invalid]),
                Err(WmoError::IndexOutOfRange {
                    group: 0,
                    ordinal: 2,
                    index: 6,
                    vertex_count: 6
                })
            );
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut vertices = VERTICES;
                vertices[5][2] = value;
                let invalid = group(flags, 0, &[material], &[0, 1, 2], &vertices, &[]);
                assert_eq!(
                    convert(&root(1, b""), &[invalid]),
                    Err(WmoError::NonFinite {
                        file: WmoFile::Group(0),
                        chunk: *b"MOVT",
                        ordinal: 5,
                        axis: 2
                    })
                );
            }
        }
    }
    for indices in [
        vec![],
        vec![0],
        vec![0, 1],
        vec![0, 1, 2, 0],
        vec![0, 1, 2, 0, 1, 2],
    ] {
        assert!(matches!(
            convert(
                &root(1, b""),
                &[group(0, 0, &[8], &indices, &VERTICES, &[])]
            ),
            Err(WmoError::TriangleIndexCount { .. })
        ));
    }
}

#[test]
fn source_bounds_are_checked_without_recomputing_or_replacing_them() {
    for (file, offset) in [(WmoFile::Root, 20 + 36), (WmoFile::Group(0), 20 + 12)] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut root = root(1, b"room\0");
            let mut group = ordinary_group();
            let input = if file == WmoFile::Root {
                &mut root
            } else {
                &mut group
            };
            set_word(input, offset, value.to_bits());
            assert!(
                matches!(convert(&root, &[group]), Err(WmoError::NonFinite { file: found, .. }) if found == file)
            );
        }
        let mut root = root(1, b"room\0");
        let mut group = ordinary_group();
        let input = if file == WmoFile::Root {
            &mut root
        } else {
            &mut group
        };
        set_word(input, offset, 100.0f32.to_bits());
        assert_eq!(
            convert(&root, &[group]),
            Err(WmoError::ReversedBounds { file, axis: 0 })
        );
    }
}

#[test]
fn duplicates_missing_chunks_invalid_versions_and_record_sizes_reject() {
    let root_bytes = root(1, b"room\0");
    for tag in [*b"MOPY", *b"MOVI", *b"MOVT", *b"MOBA"] {
        let mut input = ordinary_group();
        input.extend(chunk(&tag, &[]));
        assert_eq!(
            convert(&root_bytes, &[input]),
            Err(WmoError::DuplicateChunk {
                file: WmoFile::Group(0),
                chunk: tag
            })
        );
    }
    for tag in [*b"MVER", *b"MOHD", *b"MOGN"] {
        let mut input = root_bytes.clone();
        input.extend(chunk(&tag, &[]));
        assert_eq!(
            convert(&input, &[ordinary_group()]),
            Err(WmoError::DuplicateChunk {
                file: WmoFile::Root,
                chunk: tag
            })
        );
    }
    for version in [0, 16, 18, u32::MAX] {
        let mut input = ordinary_group();
        set_word(&mut input, 8, version);
        assert_eq!(
            convert(&root_bytes, &[input]),
            Err(WmoError::UnsupportedVersion {
                file: WmoFile::Group(0),
                version
            })
        );
        let mut input = root_bytes.clone();
        set_word(&mut input, 8, version);
        assert_eq!(
            convert(&input, &[ordinary_group()]),
            Err(WmoError::UnsupportedVersion {
                file: WmoFile::Root,
                version
            })
        );
    }
    let original = ordinary_group();
    for tag in [*b"MOPY", *b"MOVI", *b"MOVT", *b"MOBA"] {
        let mut wire_tag = tag;
        wire_tag.reverse();
        let offset = original.windows(4).position(|s| s == wire_tag).unwrap();
        let size = word(&original, offset + 4) as usize;
        for remove in [false, true] {
            let mut input = original.clone();
            let removed = if remove { size + 8 } else { 1 };
            if remove {
                input.drain(offset..offset + removed);
            } else {
                input.remove(offset + 8);
                set_word(&mut input, offset + 4, (size - 1) as u32);
            }
            let mogp_size = word(&input, 16);
            set_word(&mut input, 16, mogp_size - removed as u32);
            assert!(convert(&root_bytes, &[input]).is_err());
        }
    }
}

#[test]
fn unrelated_and_doodad_payloads_do_not_claim_or_modify_raw_geometry() {
    let mut root = root(1, b"room\0");
    let mut group = ordinary_group();
    let expected = convert(&root, &[group.clone()]).unwrap();
    for tag in [b"MODN", b"MODD", b"MODS", b"ZZZZ"] {
        root.extend(chunk(tag, &[0xff; 31]));
    }
    for tag in [b"MODR", b"MONR", b"MOTV", b"ZZZZ"] {
        group.extend(chunk(tag, &[0xff; 31]));
    }
    assert_eq!(convert(&root, &[group]).unwrap(), expected);
}

#[test]
fn resource_caps_are_checked_before_large_allocations_or_consumption() {
    assert_eq!(
        extract_wmo_collision(&root(MAX_WMO_GROUPS + 1, b""), &[]),
        Err(WmoError::GroupCountLimit {
            count: MAX_WMO_GROUPS + 1,
            maximum: MAX_WMO_GROUPS
        })
    );
    assert_eq!(
        file_limit(WmoFile::Root, MAX_WMO_FILE_BYTES + 1),
        Err(WmoError::InputLimit {
            file: WmoFile::Root,
            bytes: MAX_WMO_FILE_BYTES + 1,
            maximum: MAX_WMO_FILE_BYTES
        })
    );
    let original = ordinary_group();
    let parsed = parse_group(&original, 0).unwrap();
    assert_eq!(
        output_size(
            usize::MAX,
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
    assert_eq!(
        output_size(
            MAX_WMO_OUTPUT_BYTES,
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
    for (tag, stride, maximum) in [
        (*b"MOPY", 2, MAX_WMO_TRIANGLES),
        (*b"MOVI", 2, MAX_WMO_TRIANGLES * 3),
        (*b"MOVT", 12, MAX_WMO_VERTICES),
        (*b"MOBA", 24, MAX_WMO_BATCHES),
    ] {
        let payload = vec![0; (maximum + 1) * stride];
        assert_eq!(
            record_count(&payload, WmoFile::Group(0), tag, stride, maximum),
            Err(WmoError::CountLimit {
                file: WmoFile::Group(0),
                chunk: tag,
                count: maximum + 1,
                maximum
            })
        );
    }
    let mut input = ordinary_group();
    for _ in 0..MAX_WMO_CHUNKS {
        input.extend(chunk(b"ZZZZ", &[]));
    }
    assert_eq!(
        convert(&root(1, b"room\0"), &[input]),
        Err(WmoError::ChunkLimit {
            file: WmoFile::Group(0),
            maximum: MAX_WMO_CHUNKS
        })
    );
}

#[test]
fn maximum_vertex_and_triangle_counts_and_last_u16_index_remain_valid() {
    let vertices = vec![[0.0; 3]; MAX_WMO_VERTICES];
    let materials = vec![8; MAX_WMO_TRIANGLES];
    let mut indices = vec![0; MAX_WMO_TRIANGLES * 3];
    for (ordinal, index) in indices.iter_mut().take(MAX_WMO_VERTICES).enumerate() {
        *index = ordinal as u16;
    }
    let input = group(0, 0, &materials, &indices, &vertices, &[]);
    let result = convert(&root(1, b""), &[input]).unwrap();
    assert_eq!(result.collision_triangle_count, MAX_WMO_TRIANGLES as u32);
    assert_eq!(
        result.raw_vmap.len(),
        20 + GROUP_FIXED_BYTES + MAX_WMO_TRIANGLES * 6 + MAX_WMO_VERTICES * 12
    );
    assert_eq!(&result.raw_vmap[80..86], &[0, 0, 1, 0, 2, 0]);
    assert_eq!(
        &result.raw_vmap[80 + 65_535 * 2..80 + 65_536 * 2],
        &[255, 255]
    );
}

#[test]
fn complete_dry_wmo_bytes_match_the_independent_pinned_cpp_extractor() {
    const ROOT: &[u8] = include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo.root.test-fixture");
    const GROUPS: [&[u8]; 6] = [
        include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo_000.group.test-fixture"),
        include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo_001.group.test-fixture"),
        include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo_002.group.test-fixture"),
        include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo_003.group.test-fixture"),
        include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo_004.group.test-fixture"),
        include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo_005.group.test-fixture"),
    ];
    const RAW: &[u8] = include_bytes!(
        "../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo.raw.vmap"
    );
    let inputs: Vec<_> = GROUPS
        .iter()
        .enumerate()
        .map(|(i, bytes)| WmoGroupInput {
            group_index: i as u32,
            bytes,
        })
        .collect();
    let output = extract_wmo_collision(ROOT, &inputs).unwrap();
    assert_eq!(output.declared_group_count, 6);
    assert_eq!(output.retained_group_count, 3);
    assert_eq!(output.retained_group_indices, [0, 1, 2]);
    assert_eq!(output.collision_triangle_count, 192);
    assert_eq!(output.raw_vmap, RAW);
    assert_eq!(extract_wmo_collision(ROOT, &inputs).unwrap(), output);
}

#[test]
fn finite_extremes_subnormals_and_signed_zero_are_copied_without_aabb_replacement() {
    let vertices = [
        [f32::MAX, f32::MIN, f32::MIN_POSITIVE],
        [f32::from_bits(1), f32::from_bits(0x8000_0001), -0.0],
        [-0.0, 0.0, -0.0],
    ];
    let input = group(0, 0, &[8], &[2, 1, 0], &vertices, &[]);
    let output = convert(&root(1, b""), std::slice::from_ref(&input))
        .unwrap()
        .raw_vmap;
    let expected: Vec<_> = vertices
        .iter()
        .flatten()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    assert_eq!(&output[output.len() - expected.len()..], expected);
    assert_eq!(&output[28..52], &input[32..56]);
}

#[test]
fn total_input_limit_charges_repeated_group_slices_before_parsing() {
    // Reuse one bounded borrowed buffer: no 256-MiB fixture allocation needed.
    let bytes = vec![0; 1024 * 1024];
    let inputs: Vec<_> = (0..256)
        .map(|group_index| WmoGroupInput {
            group_index,
            bytes: bytes.as_slice(),
        })
        .collect();
    assert_eq!(
        extract_wmo_collision(&root(256, b""), &inputs),
        Err(WmoError::TotalInputLimit {
            maximum: MAX_WMO_TOTAL_BYTES
        })
    );
}

#[test]
fn version_chunk_must_lead_both_root_and_groups() {
    let root_bytes = root(1, b"room\0");
    let group_bytes = ordinary_group();
    for (file, original, first_tag) in [
        (WmoFile::Root, root_bytes.as_slice(), *b"MOHD"),
        (WmoFile::Group(0), group_bytes.as_slice(), *b"MOGP"),
    ] {
        // MVER remains well-formed and present; only its source order changes.
        // First-chunk MOGP would reach upstream's unterminated tag comparison.
        let mut reordered = original[12..].to_vec();
        reordered.extend_from_slice(&original[..12]);
        let result = if file == WmoFile::Root {
            convert(&reordered, std::slice::from_ref(&group_bytes))
        } else {
            convert(&root_bytes, &[reordered])
        };
        assert_eq!(
            result,
            Err(WmoError::UnexpectedChunk {
                file,
                chunk: first_tag
            })
        );

        // Require the same canonical envelope even for otherwise ignored tags.
        let mut prefixed = chunk(b"ZZZZ", &[]);
        prefixed.extend_from_slice(original);
        let result = if file == WmoFile::Root {
            convert(&prefixed, std::slice::from_ref(&group_bytes))
        } else {
            convert(&root_bytes, &[prefixed])
        };
        assert_eq!(
            result,
            Err(WmoError::UnexpectedChunk {
                file,
                chunk: *b"ZZZZ"
            })
        );
    }
    assert!(convert(&root_bytes, &[group_bytes]).is_ok());
}
