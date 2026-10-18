// SPDX-License-Identifier: GPL-3.0-or-later
// Original authored bytes only; no install content or source-extracted fixture.
use super::*;

fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = tag.iter().rev().copied().collect::<Vec<_>>();
    out.extend((payload.len() as u32).to_le_bytes());
    out.extend(payload);
    out
}

fn file(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = chunk(b"MVER", &17_u32.to_le_bytes());
    for (tag, payload) in chunks {
        out.extend(chunk(tag, payload));
    }
    out
}

fn root(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = file(&[(b"MOHD", vec![0; 64])]);
    for (tag, payload) in chunks {
        out.extend(chunk(tag, payload));
    }
    out
}

fn group(children: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut header = vec![0; MOGP_HEADER_BYTES];
    for (tag, payload) in children {
        header.extend(chunk(tag, payload));
    }
    file(&[(b"MOGP", header)])
}

fn put_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set(start: u32, count: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[..20].copy_from_slice(b"AuthoredSetNoNulHere");
    put_word(&mut bytes, 20, start);
    put_word(&mut bytes, 24, count);
    bytes[28..].copy_from_slice(&[0x81, 0x23, 0xff, 0x00]);
    bytes
}

fn spawn(name_word: u32) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    put_word(&mut bytes, 0, name_word);
    for (index, value) in [3.0_f32, -2.0, -0.0, 0.25, -0.5, 2.0, -4.0, -0.75]
        .into_iter()
        .enumerate()
    {
        put_word(&mut bytes, 4 + index * 4, value.to_bits());
    }
    put_word(&mut bytes, 36, 0xe1a2_b3c4);
    bytes
}

fn refs(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn root_error(bytes: &[u8]) -> &'static str {
    parse_root_doodads(bytes).err().unwrap().code()
}

fn group_error(bytes: &[u8]) -> &'static str {
    parse_group_doodad_references(bytes).err().unwrap().code()
}

#[test]
fn original_modn_path_bytes_are_copied_without_normalization_or_reencoding() {
    let paths = b"WORLD\\ART\\Mixed CASE.MDX\0World\\Art\\Odd\xff.MDL\0";
    let bytes = root(&[(b"MODN", paths.to_vec())]);
    let parsed = parse_root_doodads(&bytes).unwrap();
    assert_eq!(parsed.paths, paths);
    assert_eq!(parsed.events.len(), 2);
    assert_eq!(parsed.events[0].path, b"WORLD\\ART\\Mixed CASE.MDX");
    assert_eq!(parsed.events[1].path, b"World\\Art\\Odd\xff.MDL");
    assert_eq!(parsed.events[0].name_index, 0);
    let second = parsed.events[0].path.len() as u32 + 1;
    assert_eq!(parsed.events[1].name_index, second);
    assert_eq!(parsed.names[1].name_index, second);
    assert_eq!(parsed.names[1].path, parsed.events[1].path);
    assert_eq!(parsed.path_at(second).unwrap(), b"World\\Art\\Odd\xff.MDL");
    assert_eq!(bytes, root(&[(b"MODN", paths.to_vec())]));
}

#[test]
fn nul_padding_and_short_names_preserve_every_source_extraction_offset() {
    let parsed = parse_root_doodads(&root(&[(b"MODN", b"\0A\0\0BC\0\0".to_vec())])).unwrap();
    assert_eq!(
        parsed
            .events
            .iter()
            .map(|event| event.name_index)
            .collect::<Vec<_>>(),
        [0, 1, 3, 4, 7]
    );
    assert_eq!(
        parsed
            .events
            .iter()
            .map(|event| event.path.as_slice())
            .collect::<Vec<_>>(),
        [b"".as_slice(), b"A", b"", b"BC", b""]
    );
    assert_eq!(parsed.path_at(7).unwrap(), b"");
    assert_eq!(
        parsed.path_at(8).unwrap_err().code(),
        "vmap.doodad_name_offset"
    );
}

#[test]
fn root_chunk_order_keeps_all_extractions_but_final_arrays_replace() {
    let mut header = vec![0; 64];
    put_word(&mut header, 4, 3);
    let parsed = parse_root_doodads(&file(&[
        (b"MODN", b"First.MDX\0Same.M2\0".to_vec()),
        (b"MODS", set(0, 1)),
        (b"MODD", spawn(0)),
        (b"JUNK", vec![0x99]),
        (b"MOHD", header),
        (b"MODN", b"Replacement.MDL\0".to_vec()),
        (b"MODD", spawn(0x9a00_0000)),
        (b"MODS", set(4, 7)),
    ]))
    .unwrap();
    assert_eq!(parsed.group_count, 3);
    assert_eq!(
        parsed
            .events
            .iter()
            .map(|event| event.chunk_ordinal)
            .collect::<Vec<_>>(),
        [1, 1, 6]
    );
    assert_eq!(
        parsed
            .events
            .iter()
            .map(|event| event.name_index)
            .collect::<Vec<_>>(),
        [0, 10, 0]
    );
    assert_eq!(parsed.paths, b"Replacement.MDL\0");
    assert_eq!(parsed.names.len(), 1);
    assert_eq!(parsed.sets.len(), 1);
    assert_eq!(parsed.sets[0].start_index, 4);
    assert_eq!(parsed.sets[0].count, 7);
    assert_eq!(parsed.spawns.len(), 1);
    assert_eq!(parsed.spawns[0].ignored_name_high_byte, 0x9a);
}

#[test]
fn empty_replacement_chunks_clear_final_arrays_without_erasing_extraction_history() {
    let parsed = parse_root_doodads(&root(&[
        (b"MODN", b"Old.M2\0".to_vec()),
        (b"MODS", set(0, 1)),
        (b"MODD", spawn(0)),
        (b"MODN", vec![]),
        (b"MODS", vec![]),
        (b"MODD", vec![]),
    ]))
    .unwrap();
    assert!(parsed.paths.is_empty() && parsed.names.is_empty());
    assert!(parsed.sets.is_empty() && parsed.spawns.is_empty());
    assert_eq!(parsed.events.len(), 1);
    assert_eq!(parsed.events[0].path, b"Old.M2");
}

#[test]
fn mohd_doodad_counts_do_not_override_actual_chunk_payloads() {
    let mut header = vec![0; 64];
    for at in [16, 20, 24] {
        put_word(&mut header, at, u32::MAX);
    }
    let parsed = parse_root_doodads(&file(&[
        (b"MOHD", header),
        (b"MODN", b"Only.M2\0".to_vec()),
        (b"MODS", set(0, 1)),
        (b"MODD", spawn(0)),
    ]))
    .unwrap();
    assert_eq!(parsed.names.len(), 1);
    assert_eq!(parsed.sets.len(), 1);
    assert_eq!(parsed.spawns.len(), 1);
}

#[test]
fn raw_mods_and_modd_retain_all_storage_fields_and_mask_only_name_high_byte() {
    let parsed = parse_root_doodads(&root(&[
        (b"MODS", set(12, 8)),
        (b"MODD", spawn(0xef12_3456)),
    ]))
    .unwrap();
    assert_eq!(parsed.sets[0].name, *b"AuthoredSetNoNulHere");
    assert_eq!(parsed.sets[0].padding, [0x81, 0x23, 0xff, 0x00]);
    assert_eq!(parsed.sets[0].start_index, 12);
    assert_eq!(parsed.sets[0].count, 8);
    assert!(!format!("{:?}", parsed.sets[0]).contains("Authored"));
    let value = &parsed.spawns[0];
    assert_eq!(value.name_index, 0x0012_3456);
    assert_eq!(value.ignored_name_high_byte, 0xef);
    assert_eq!(value.position, [3.0, -2.0, -0.0]);
    assert_eq!(value.position[2].to_bits(), (-0.0_f32).to_bits());
    assert_eq!(value.rotation, [0.25, -0.5, 2.0, -4.0]);
    assert_eq!(value.scale, -0.75);
    assert_eq!(value.color, 0xe1a2_b3c4);
}

#[test]
fn every_nonfinite_raw_transform_component_is_rejected_without_normalizing_valid_values() {
    for index in 0..8 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut raw = spawn(0);
            put_word(&mut raw, 4 + index * 4, value.to_bits());
            assert_eq!(
                root_error(&root(&[(b"MODD", raw)])),
                "vmap.doodad_nonfinite_transform"
            );
        }
    }
    // A finite, nonunit quaternion and negative scale remain raw metadata.
    assert!(parse_root_doodads(&root(&[(b"MODD", spawn(0))])).is_ok());
}

#[test]
fn set_ranges_use_checked_source_width_without_inventing_a_spawn_array_bound() {
    assert_eq!(
        root_error(&root(&[(b"MODS", set(u32::MAX, 1))])),
        "vmap.doodad_set_range"
    );
    for (start, count) in [(u32::MAX, 0), (u32::MAX - 1, 1), (200, 500)] {
        let parsed = parse_root_doodads(&root(&[(b"MODS", set(start, count))])).unwrap();
        assert_eq!(parsed.sets[0].start_index, start);
        assert_eq!(parsed.sets[0].count, count);
        assert!(parsed.spawns.is_empty());
    }
}

#[test]
fn unused_name_offsets_are_raw_and_used_offsets_are_checked_against_final_path_copy() {
    let parsed = parse_root_doodads(&root(&[
        (b"MODN", b"ABCDEFG.M2\0".to_vec()),
        (b"MODD", spawn(0xff12_3456)),
    ]))
    .unwrap();
    assert_eq!(parsed.spawns[0].name_index, 0x12_3456);
    assert_eq!(
        parsed.path_at(0x12_3456).unwrap_err().code(),
        "vmap.doodad_name_offset"
    );
    // Accumulated successful offsets can come from an earlier MODN chunk.
    assert_eq!(parsed.path_at(2).unwrap(), b"CDEFG.M2");
    assert_eq!(parsed.path_at(10).unwrap(), b"");
    assert_eq!(
        parsed.path_at(11).unwrap_err().code(),
        "vmap.doodad_name_offset"
    );
}

#[test]
fn incomplete_consumed_records_are_never_floored_or_read_into_neighboring_chunks() {
    for (tag, stride, code) in [
        (b"MODS", 32, "vmap.doodad_set_record"),
        (b"MODD", 40, "vmap.doodad_spawn_record"),
    ] {
        for length in [1, stride - 1, stride + 1] {
            assert_eq!(root_error(&root(&[(tag, vec![0; length])])), code);
        }
    }
    for length in [1, 3, 5] {
        assert_eq!(
            group_error(&group(&[(b"MODR", vec![0; length])])),
            "vmap.doodad_reference_record"
        );
    }
}

#[test]
fn modn_requires_bounded_terminated_entries_and_diagnostics_never_include_path_bytes() {
    let error = parse_root_doodads(&root(&[(b"MODN", b"PrivateAssetName.M2".to_vec())]))
        .err()
        .unwrap();
    assert_eq!(error.code(), "vmap.doodad_name_nul");
    assert_eq!(error.to_string(), error.code());
    assert!(!format!("{error:?}").contains("PrivateAssetName"));
    for length in [MAX_DOODAD_NAME_BYTES, MAX_DOODAD_NAME_BYTES + 1] {
        let mut path = vec![b'A'; length];
        path.push(0);
        let result = parse_root_doodads(&root(&[(b"MODN", path)]));
        if length == MAX_DOODAD_NAME_BYTES {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.err().unwrap().code(), "vmap.doodad_name_length");
        }
    }
}

#[test]
fn group_references_preserve_order_duplicates_and_invalid_indices_for_coordinator_filtering() {
    let values = [7, 2, 7, u16::MAX, 0];
    assert_eq!(
        parse_group_doodad_references(&group(&[(b"MODR", refs(&values))])).unwrap(),
        values
    );
    assert!(parse_group_doodad_references(&group(&[]))
        .unwrap()
        .is_empty());
}

#[test]
fn nested_and_sibling_modr_follow_flat_source_traversal_and_final_replacement() {
    let mut bytes = file(&[(b"MODR", refs(&[9, 1]))]);
    let nested = group(&[(b"MODR", refs(&[4, 4, 2]))]);
    bytes.extend_from_slice(&nested[12..]);
    assert_eq!(parse_group_doodad_references(&bytes).unwrap(), [4, 4, 2]);
    bytes.extend(chunk(b"MODR", &refs(&[8, 3, 8])));
    assert_eq!(parse_group_doodad_references(&bytes).unwrap(), [8, 3, 8]);
    bytes.extend(chunk(b"MODR", &[]));
    assert!(parse_group_doodad_references(&bytes).unwrap().is_empty());
    // The source also accepts a header-only MOGP followed by sibling chunks.
    let flat = file(&[(b"MOGP", vec![0; 68]), (b"MODR", refs(&[1, 5]))]);
    assert_eq!(parse_group_doodad_references(&flat).unwrap(), [1, 5]);
}

#[test]
fn liquid_remains_unsupported_regardless_of_chunk_order_or_payload_size() {
    for liquid in [vec![], vec![0; 30]] {
        assert_eq!(
            group_error(&group(&[(b"MLIQ", liquid.clone()), (b"MODR", refs(&[0]))])),
            "vmap.doodad_unsupported_liquid"
        );
        assert_eq!(
            root_error(&root(&[(b"MLIQ", liquid.clone())])),
            "vmap.doodad_unsupported_liquid"
        );
        let mut flat = group(&[]);
        flat.extend(chunk(b"MLIQ", &liquid));
        assert_eq!(group_error(&flat), "vmap.doodad_unsupported_liquid");
    }
}

#[test]
fn headers_version_and_container_boundaries_match_the_bounded_dry_profile() {
    assert_eq!(root_error(&[]), "vmap.doodad_missing_root_header");
    assert_eq!(group_error(&[]), "vmap.doodad_missing_group_header");
    assert_eq!(
        root_error(&chunk(b"MOHD", &[0; 64])),
        "vmap.doodad_missing_version"
    );
    assert_eq!(root_error(&file(&[])), "vmap.doodad_missing_root_header");
    assert_eq!(group_error(&file(&[])), "vmap.doodad_missing_group_header");
    assert_eq!(
        root_error(&root(&[(b"MVER", 17_u32.to_le_bytes().to_vec())])),
        "vmap.doodad_duplicate_version"
    );
    assert_eq!(
        root_error(&root(&[(b"MOHD", vec![0; 64])])),
        "vmap.doodad_duplicate_header"
    );
    assert_eq!(
        root_error(&file(&[(b"MOHD", vec![0; 63])])),
        "vmap.doodad_root_header"
    );
    assert_eq!(
        group_error(&file(&[(b"MOGP", vec![0; 67])])),
        "vmap.doodad_group_header"
    );
    assert_eq!(
        group_error(&group(&[(b"MOGP", vec![0; 68])])),
        "vmap.doodad_unexpected_chunk"
    );
    assert_eq!(
        group_error(&group(&[(b"MVER", 17_u32.to_le_bytes().to_vec())])),
        "vmap.doodad_unexpected_chunk"
    );
    assert_eq!(
        root_error(&root(&[(b"MOGP", vec![0; 68])])),
        "vmap.doodad_unexpected_chunk"
    );
    let mut wrong_version = root(&[]);
    put_word(&mut wrong_version, 8, 18);
    assert_eq!(root_error(&wrong_version), "vmap.doodad_version");
    let mut too_many_groups = vec![0; 64];
    put_word(&mut too_many_groups, 4, MAX_WMO_GROUPS + 1);
    assert_eq!(
        root_error(&file(&[(b"MOHD", too_many_groups)])),
        "vmap.doodad_group_count"
    );
}

#[test]
fn truncated_headers_payloads_and_nested_trailing_bytes_fail_without_panics() {
    for tail_length in 1..8 {
        let mut bytes = root(&[]);
        bytes.extend(vec![0; tail_length]);
        assert_eq!(root_error(&bytes), "vmap.doodad_chunk_header");
    }
    let names = root(&[(b"MODN", b"A.M2\0".to_vec())]);
    for length in names.len() - 5..names.len() {
        assert_eq!(root_error(&names[..length]), "vmap.doodad_chunk_payload");
    }
    let mut exaggerated = root(&[]);
    exaggerated.extend(chunk(b"JUNK", &[]));
    let size_at = exaggerated.len() - 4;
    put_word(&mut exaggerated, size_at, u32::MAX);
    assert_eq!(root_error(&exaggerated), "vmap.doodad_chunk_payload");
    let mut truncated_child = vec![0; 68];
    truncated_child.extend(b"RDOM");
    assert_eq!(
        group_error(&file(&[(b"MOGP", truncated_child)])),
        "vmap.doodad_chunk_header"
    );
    let mut incomplete_child = vec![0; 68];
    incomplete_child.extend(chunk(b"MODR", &refs(&[1, 2])));
    incomplete_child.pop();
    assert_eq!(
        group_error(&file(&[(b"MOGP", incomplete_child)])),
        "vmap.doodad_chunk_payload"
    );
}

#[test]
fn limits_charge_repeated_chunks_instead_of_only_retained_arrays() {
    assert!(file_limit(MAX_WMO_FILE_BYTES).is_ok());
    assert_eq!(
        file_limit(MAX_WMO_FILE_BYTES + 1).unwrap_err().code(),
        "vmap.doodad_file_bytes"
    );
    assert_eq!(
        root_error(&root(&[
            (b"MODN", vec![0; MAX_DOODAD_NAMES]),
            (b"MODN", vec![0])
        ])),
        "vmap.doodad_name_count"
    );
    assert_eq!(
        root_error(&root(&[
            (b"MODS", vec![0; MAX_DOODAD_SETS * 32]),
            (b"MODS", vec![0; 32])
        ])),
        "vmap.doodad_record_limit"
    );
    assert_eq!(
        root_error(&root(&[
            (b"MODD", vec![0; MAX_DOODAD_SPAWNS * 40]),
            (b"MODD", vec![0; 40])
        ])),
        "vmap.doodad_record_limit"
    );
    assert_eq!(
        group_error(&group(&[
            (b"MODR", vec![0; MAX_DOODAD_REFERENCES * 2]),
            (b"MODR", vec![0; 2])
        ])),
        "vmap.doodad_record_limit"
    );
    let mut count = usize::MAX;
    assert_eq!(
        charge(&mut count, 1, usize::MAX).unwrap_err().code(),
        "vmap.doodad_record_limit"
    );
}

#[test]
fn chunk_limits_include_unknown_root_chunks_and_nested_group_children() {
    let mut root_bytes = root(&[]);
    let mut group_header = vec![0; 68];
    for _ in 0..MAX_WMO_CHUNKS - 2 {
        root_bytes.extend(chunk(b"JUNK", &[]));
        group_header.extend(chunk(b"JUNK", &[]));
    }
    assert!(parse_root_doodads(&root_bytes).is_ok());
    assert!(parse_group_doodad_references(&file(&[(b"MOGP", group_header.clone())])).is_ok());
    root_bytes.extend(chunk(b"JUNK", &[]));
    group_header.extend(chunk(b"JUNK", &[]));
    assert_eq!(root_error(&root_bytes), "vmap.doodad_chunk_count");
    assert_eq!(
        group_error(&file(&[(b"MOGP", group_header)])),
        "vmap.doodad_chunk_count"
    );
}
