// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic records only. No install content or source-extracted fixture.
use super::*;

fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = tag.iter().rev().copied().collect::<Vec<_>>();
    out.extend((payload.len() as u32).to_le_bytes());
    out.extend(payload);
    out
}

fn file(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = chunk(b"MVER", &18_u32.to_le_bytes());
    for (tag, payload) in chunks {
        out.extend(chunk(tag, payload));
    }
    out
}

fn set_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_half(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_vector(bytes: &mut [u8], at: usize, values: [f32; 3]) {
    for (i, value) in values.into_iter().enumerate() {
        set_word(bytes, at + i * 4, value.to_bits());
    }
}

fn mddf(name: u32, id: u32) -> Vec<u8> {
    let mut bytes = vec![0; 36];
    set_word(&mut bytes, 0, name);
    set_word(&mut bytes, 4, id);
    set_vector(&mut bytes, 8, [1.0, 2.0, 3.0]);
    set_vector(&mut bytes, 20, [-45.0, 360.0, 0.0]);
    set_half(&mut bytes, 32, 1024);
    set_half(&mut bytes, 34, 0x8001);
    bytes
}

fn modf(name: u32, id: u32) -> Vec<u8> {
    let mut bytes = vec![0; 64];
    set_word(&mut bytes, 0, name);
    set_word(&mut bytes, 4, id);
    set_vector(&mut bytes, 8, [11.0, 12.0, 13.0]);
    set_vector(&mut bytes, 20, [14.0, 15.0, 16.0]);
    set_vector(&mut bytes, 32, [-1.0, -2.0, -3.0]);
    set_vector(&mut bytes, 44, [1.0, 2.0, 3.0]);
    set_half(&mut bytes, 56, 1);
    set_half(&mut bytes, 58, 65535);
    set_half(&mut bytes, 60, 17);
    set_half(&mut bytes, 62, 65535);
    bytes
}

fn map_dbc(rows: &[(u32, &[u8])]) -> Vec<u8> {
    let mut strings = vec![0];
    let mut records = Vec::new();
    for &(id, name) in rows {
        let mut record = [0; 168];
        set_word(&mut record, 0, id);
        set_word(&mut record, 4, strings.len() as u32);
        strings.extend(name);
        strings.push(0);
        records.extend(record);
    }
    let mut bytes = b"WDBC".to_vec();
    bytes.extend((rows.len() as u32).to_le_bytes());
    bytes.extend(42_u32.to_le_bytes());
    bytes.extend(168_u32.to_le_bytes());
    bytes.extend((strings.len() as u32).to_le_bytes());
    bytes.extend(records);
    bytes.extend(strings);
    bytes
}

fn adt_error(bytes: &[u8]) -> &'static str {
    parse_adt(bytes).err().expect("authored invalid ADT").code()
}

#[test]
fn map_rows_retain_physical_order_and_defer_unvisited_path_validation() {
    let maps = parse_map_dbc(&map_dbc(&[
        (900, b"../Unvisited"),
        (0, b"Authored_Map"),
        (31, b"Unused Name"),
    ]))
    .unwrap();
    assert_eq!(
        maps.iter().map(|row| row.id).collect::<Vec<_>>(),
        [900, 0, 31]
    );
    assert_eq!(maps[0].directory, "../Unvisited");
    assert_eq!(
        maps[0].wdt_member().unwrap_err().code(),
        "vmap.map_directory"
    );
    assert_eq!(
        maps[1].wdt_member().unwrap(),
        "World\\Maps\\Authored_Map\\Authored_Map.wdt"
    );
    assert!(maps[2].adt_members().is_err());
}

#[test]
fn map_dbc_rejects_malformed_metadata_without_logging_names() {
    for (bytes, code) in [
        (map_dbc(&[(0, b"A"), (0, b"B")]), "vmap.duplicate_map_id"),
        (map_dbc(&[(0, &[b'A'; 64])]), "vmap.map_metadata_length"),
        (map_dbc(&[(0, &[0xff])]), "vmap.map_metadata_utf8"),
    ] {
        let error = parse_map_dbc(&bytes).err().unwrap();
        assert_eq!(error.code(), code);
        assert_eq!(error.to_string(), code);
    }
    let mut bytes = map_dbc(&[(1, b"PrivateUntrustedName")]);
    set_word(&mut bytes, 24, u32::MAX);
    let error = parse_map_dbc(&bytes).err().unwrap();
    assert_eq!(error.code(), "vmap.map_dbc_string_reference");
    assert!(!format!("{error:?}").contains("PrivateUntrustedName"));
    bytes.push(0);
    assert_eq!(
        parse_map_dbc(&bytes).err().unwrap().code(),
        "vmap.map_dbc_length"
    );
}

#[test]
fn wdt_presence_not_main_flags_precedes_all_4096_x_major_attempts() {
    let map = parse_map_dbc(&map_dbc(&[(0, b"Authored")]))
        .unwrap()
        .remove(0);
    let missing_main = parse_wdt(&file(&[])).unwrap();
    let all_main_zero = parse_wdt(&file(&[(b"MAIN", vec![0; 4096 * 8])])).unwrap();
    let main_arbitrary = parse_wdt(&file(&[(b"MAIN", vec![0xff; 3])])).unwrap();
    assert!(missing_main == all_main_zero && all_main_zero == main_arbitrary);
    let attempts = map.adt_members().unwrap().collect::<Vec<_>>();
    assert_eq!(attempts.len(), 4096);
    for (ordinal, attempt) in attempts.iter().enumerate() {
        assert_eq!(attempt.x, ordinal as u32 / 64);
        assert_eq!(attempt.y, ordinal as u32 % 64);
    }
    assert_eq!(
        attempts[64].member,
        "World\\Maps\\Authored\\Authored_1_0.adt"
    );
    assert_eq!(
        attempts[4095].member,
        "World\\Maps\\Authored\\Authored_63_63.adt"
    );
}

#[test]
fn name_arrays_and_records_preserve_source_order_across_repeated_chunks() {
    let bytes = file(&[
        (
            b"MMDX",
            b"WORLD\\ART\\FIRST.MDX\0WORLD\\ART\\SECOND.M2\0".to_vec(),
        ),
        (b"MDDF", mddf(1, 50)),
        (b"MWMO", b"World\\Wmo\\HUT.WMO\0".to_vec()),
        (b"MODF", modf(0, 9)),
        (b"MMDX", b"WORLD\\ART\\THIRD.MDL\0".to_vec()),
        (b"MDDF", mddf(2, 12)),
    ]);
    let inputs = parse_adt(&bytes).unwrap();
    assert_eq!(
        inputs.events,
        [
            DiscoveryEvent::ExtractModel(0),
            DiscoveryEvent::ExtractModel(1),
            DiscoveryEvent::Placement(0),
            DiscoveryEvent::ReferenceWmo(2),
            DiscoveryEvent::Placement(1),
            DiscoveryEvent::ExtractModel(3),
            DiscoveryEvent::Placement(2),
        ]
    );
    let PlacementInput::M2(last) = &inputs.placements[2] else {
        panic!("M2")
    };
    assert_eq!(last.dependency, 3);
    assert_eq!(last.name_index, 2);
    assert_eq!(last.unique_id, 12);
    assert_eq!(inputs.dependencies[3].member, "World\\Art\\Third.m2");
}

#[test]
fn real_reversed_fourcc_and_direct_array_references_ignore_offset_tables() {
    let bytes = file(&[
        (b"MMDX", b"A.MDX\0B.MDX\0".to_vec()),
        (b"MMID", vec![0xff; 5]),
        (b"MWMO", b"A.WMO\0B.WMO\0".to_vec()),
        (b"MWID", vec![0xff; 1]),
        (b"MDDF", mddf(1, 1)),
        (b"MODF", modf(1, 2)),
    ]);
    let input = parse_adt(&bytes).unwrap();
    let PlacementInput::M2(model) = &input.placements[0] else {
        panic!("M2")
    };
    let PlacementInput::Wmo(wmo) = &input.placements[1] else {
        panic!("WMO")
    };
    assert_eq!(input.dependencies[model.dependency].extracted_name, "B.m2");
    assert_eq!(input.dependencies[wmo.dependency].extracted_name, "B.wmo");
    assert_eq!(
        adt_error(&file(&[
            (b"MDDF", mddf(0, 1)),
            (b"MMDX", b"A.MDX\0".to_vec())
        ])),
        "vmap.mddf_name_reference"
    );
    let mut reversed = file(&[]);
    reversed[..4].copy_from_slice(b"MVER");
    assert_eq!(adt_error(&reversed), "vmap.missing_version");
}

#[test]
fn normalization_matches_distinct_mmdx_adt_mwmo_and_wdt_mwmo_paths() {
    let inputs = parse_adt(&file(&[
        (
            b"MMDX",
            b"wORLD\\aRT assets\\aBC dEF.MDX\0aBC.M2\0x.MDL\0".to_vec(),
        ),
        (b"MWMO", b"wORLD\\wMO\\aBC dEF.WMO\0".to_vec()),
    ]))
    .unwrap();
    assert_eq!(
        inputs.dependencies[0].source_member,
        "wORLD\\aRT assets\\aBC dEF.MDX"
    );
    assert_eq!(
        inputs.dependencies[0].member,
        "World\\Art Assets\\Abc_Def.m2"
    );
    assert_eq!(inputs.dependencies[1].member, "Abc.m2");
    assert_eq!(inputs.dependencies[2].member, "X.m2");
    assert_eq!(inputs.dependencies[3].member, "wORLD\\wMO\\Abc_Def.wmo");
    let global = parse_wdt(&file(&[(b"MWMO", b"wORLD\\wMO\\aBC dEF.WMO\0".to_vec())])).unwrap();
    assert_eq!(global.dependencies[0].extracted_name, "Abc Def.wmo");
    assert_eq!(global.events, [DiscoveryEvent::ReferenceWmo(0)]);
}

#[test]
fn unreferenced_and_duplicate_names_are_retained_not_deduplicated() {
    let input = parse_adt(&file(&[(b"MMDX", b"A.MDX\0A.MDX\0UNUSED.MDX\0".to_vec())])).unwrap();
    assert_eq!(input.dependencies.len(), 3);
    assert!(input.dependencies[0] == input.dependencies[1]);
    assert!(input.placements.is_empty());
    assert_eq!(input.events.len(), 3);
}

#[test]
fn transforms_flags_original_ids_and_bounds_are_not_assembled() {
    let mut model = mddf(0, 0);
    set_half(&mut model, 32, 0);
    let input = parse_adt(&file(&[
        (b"MMDX", b"A.MDX\0".to_vec()),
        (b"MDDF", [model.clone(), model].concat()),
        (b"MWMO", b"A.WMO\0".to_vec()),
        (b"MODF", modf(0, u32::MAX)),
    ]))
    .unwrap();
    let PlacementInput::M2(model) = &input.placements[0] else {
        panic!("M2")
    };
    assert_eq!(model.position, [1.0, 2.0, 3.0]);
    assert_eq!(model.rotation, [-45.0, 360.0, 0.0]);
    assert_eq!((model.unique_id, model.scale, model.flags), (0, 0, 0x8001));
    assert_eq!(input.placements[0], input.placements[1]);
    let PlacementInput::Wmo(wmo) = &input.placements[2] else {
        panic!("WMO")
    };
    assert_eq!(wmo.position, [11.0, 12.0, 13.0]);
    assert_eq!(wmo.rotation, [14.0, 15.0, 16.0]);
    assert_eq!(wmo.bounds, [[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]]);
    assert_eq!((wmo.unique_id, wmo.flags), (u32::MAX, 1));
    assert_eq!(
        (wmo.doodad_set, wmo.name_set, wmo.scale),
        (65535, 17, 65535)
    );
}

#[test]
fn partial_records_invalid_references_and_nonfinite_values_fail_closed() {
    assert_eq!(
        adt_error(&file(&[(b"MDDF", vec![0; 35])])),
        "vmap.mddf_record_length"
    );
    assert_eq!(
        adt_error(&file(&[(b"MODF", vec![0; 63])])),
        "vmap.modf_record_length"
    );
    assert_eq!(
        adt_error(&file(&[
            (b"MMDX", b"A.MDX\0".to_vec()),
            (b"MDDF", mddf(1, 0))
        ])),
        "vmap.mddf_name_reference"
    );
    assert_eq!(
        adt_error(&file(&[(b"MODF", modf(0, 0))])),
        "vmap.modf_name_reference"
    );
    for offset in [8, 12, 16, 20, 24, 28] {
        let mut model = mddf(0, 1);
        set_word(&mut model, offset, f32::NAN.to_bits());
        assert_eq!(
            adt_error(&file(&[(b"MMDX", b"A.MDX\0".to_vec()), (b"MDDF", model)])),
            "vmap.nonfinite_transform"
        );
    }
    for offset in [8, 20, 32, 44, 52] {
        let mut wmo = modf(0, 1);
        set_word(&mut wmo, offset, f32::INFINITY.to_bits());
        assert_eq!(
            adt_error(&file(&[(b"MWMO", b"A.WMO\0".to_vec()), (b"MODF", wmo)])),
            "vmap.nonfinite_transform"
        );
    }
    let mut wmo = modf(0, 0);
    set_vector(&mut wmo, 32, [2.0, -2.0, -3.0]);
    assert_eq!(
        adt_error(&file(&[(b"MWMO", b"A.WMO\0".to_vec()), (b"MODF", wmo)])),
        "vmap.modf_bounds_order"
    );
}

#[test]
fn globals_are_explicitly_unsupported_and_zero_position_is_not_rewritten() {
    let bytes = file(&[(b"MWMO", b"A.WMO\0".to_vec()), (b"MODF", modf(0, 1))]);
    assert_eq!(
        parse_wdt(&bytes).err().unwrap().code(),
        "vmap.unsupported_global_wmo"
    );
    let mut wmo = modf(0, 1);
    set_vector(&mut wmo, 8, [0.0, 8.0, 0.0]);
    let input = parse_adt(&file(&[(b"MWMO", b"A.WMO\0".to_vec()), (b"MODF", wmo)])).unwrap();
    let PlacementInput::Wmo(wmo) = &input.placements[0] else {
        panic!("WMO")
    };
    assert_eq!(wmo.position, [0.0, 8.0, 0.0]);
    assert!(parse_wdt(&file(&[(b"MODF", vec![])])).is_ok());
}

#[test]
fn malformed_names_paths_and_extensions_have_safe_fixed_errors() {
    for (name, code) in [
        (&b"A.MDX"[..], "vmap.name_nul"),
        (&b"\0"[..], "vmap.member_name_length"),
        (&b"..\\A.MDX\0"[..], "vmap.member_path"),
        (&b"\\A.MDX\0"[..], "vmap.member_path"),
        (&b"A\\\\B.MDX\0"[..], "vmap.member_path"),
        (&b"A/B.MDX\0"[..], "vmap.member_name_profile"),
        (&b"C:\\A.MDX\0"[..], "vmap.member_name_profile"),
        (&b"A\xff.MDX\0"[..], "vmap.member_name_profile"),
        (&b"A.TXT\0"[..], "vmap.unsupported_model_extension"),
    ] {
        assert_eq!(adt_error(&file(&[(b"MMDX", name.to_vec())])), code);
    }
    assert_eq!(
        adt_error(&file(&[(b"MWMO", b"A.MDX\0".to_vec())])),
        "vmap.unsupported_wmo_extension"
    );
}

#[test]
fn chunk_framing_and_version_are_complete_even_for_ignored_chunks() {
    let mut bytes = file(&[]);
    bytes.extend([0; 7]);
    assert_eq!(adt_error(&bytes), "vmap.chunk_header");
    let mut bytes = file(&[(b"MAIN", vec![0])]);
    bytes.pop();
    assert_eq!(adt_error(&bytes), "vmap.chunk_payload");
    assert_eq!(
        adt_error(&chunk(b"MVER", &17_u32.to_le_bytes())),
        "vmap.version"
    );
    assert_eq!(
        adt_error(&file(&[(b"MVER", 18_u32.to_le_bytes().to_vec())])),
        "vmap.duplicate_version"
    );
    assert_eq!(adt_error(&[]), "vmap.placement_bytes");
    let mut bytes = file(&[]);
    set_word(&mut bytes, 4, u32::MAX);
    assert_eq!(adt_error(&bytes), "vmap.chunk_payload");
}

#[test]
fn listfile_logical_nul_keeps_exact_tokens_without_line_ending_repair() {
    assert_eq!(LISTFILE_POLICY, "cmangos-vmap-listfile-logical-nul-v1");
    for bytes in [&b""[..], &b"\0ignored.wmo\r\n"[..], &b"\n\n"[..]] {
        assert!(tokenize_listfile(bytes).unwrap().is_empty());
    }
    let terminated = b"A.wmo\r\nB.wmo\r\n\0";
    let unterminated = &terminated[..terminated.len() - 1];
    assert_eq!(
        tokenize_listfile(terminated).unwrap(),
        tokenize_listfile(unterminated).unwrap()
    );
    assert_eq!(
        tokenize_listfile(b"\nA.wmo\n\nB.wmo\r\n\r\n\0C.wmo\r\n").unwrap(),
        vec![&b"A.wm"[..], &b"B.wmo"[..], &b""[..]]
    );
    assert_eq!(tokenize_listfile(b"A.wmo\r").unwrap(), vec![&b"A.wmo"[..]]);
    assert_eq!(tokenize_listfile(b"A.wmo").unwrap(), vec![&b"A.wm"[..]]);
    assert!(parse_wmo_listfile(b"A.wmo\n").unwrap().is_empty());
}

#[test]
fn listfile_filter_normalization_and_group_heuristic_are_source_ordered() {
    let list = b"world\\wmo\\FIRST.wmo\r\nworld\\wmo\\UPPER.WMO\r\nworld\\wmo\\hut_001.wmo\r\nworld\\wmo\\hut_001_extra.wmo\r\nworld\\wmo\\hut 123.wmo\r\nworld\\wmo\\FIRST.wmo\r\n\0";
    let roots = parse_wmo_listfile(list).unwrap();
    assert_eq!(
        roots
            .iter()
            .map(|root| root.member.as_str())
            .collect::<Vec<_>>(),
        [
            "world\\wmo\\First.wmo",
            "world\\wmo\\Hut_001_Extra.wmo",
            "world\\wmo\\First.wmo",
        ]
    );
    assert_eq!(roots[0].source_member, "world\\wmo\\FIRST.wmo");
    assert!(roots[0] == roots[2]);
    // The source substring filter admits these candidates, then the bounded
    // final-extension profile refuses them rather than pretending they vanished.
    assert_eq!(
        parse_wmo_listfile(b"A.wmo.backup\r\n\0")
            .err()
            .unwrap()
            .code(),
        "vmap.listfile_root_extension"
    );
    assert_eq!(
        parse_wmo_listfile(b"folder.wmo\\ABCD.bin\r\n\0")
            .err()
            .unwrap()
            .code(),
        "vmap.listfile_root_extension"
    );
    // Source group heuristic runs before root opening, including odd extensions.
    assert!(parse_wmo_listfile(b"folder.wmo\\A_123.bin\r\n\0")
        .unwrap()
        .is_empty());
    assert_eq!(
        parse_wmo_listfile(b"folder.wmo\\x\r\n\0")
            .err()
            .unwrap()
            .code(),
        "vmap.listfile_root_name"
    );
}

#[test]
fn listfile_limits_and_non_ascii_candidate_errors_do_not_leak_tokens() {
    let bytes = vec![0; MAX_FILE_BYTES + 1];
    assert_eq!(
        tokenize_listfile(&bytes).err().unwrap().code(),
        "vmap.listfile_bytes"
    );
    drop(bytes);
    let bytes = b"x\n".repeat(1_000_001);
    assert_eq!(
        tokenize_listfile(&bytes).err().unwrap().code(),
        "vmap.listfile_token_count"
    );
    drop(bytes);
    let mut name = vec![b'A'; MAX_MEMBER_BYTES];
    name.extend(b".wmo\r\n");
    assert_eq!(
        parse_wmo_listfile(&name).err().unwrap().code(),
        "vmap.member_name_length"
    );
    let error = parse_wmo_listfile(b"private\xff.wmo\r\n").err().unwrap();
    assert_eq!(error.code(), "vmap.member_name_profile");
    assert!(!format!("{error:?} {error}").contains("private"));
    // Opaque non-candidate names are never normalized or treated as paths.
    assert!(parse_wmo_listfile(b"private\xff.bin\r\n")
        .unwrap()
        .is_empty());
}

fn liquid_dbc(ids: &[u32], fields: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    bytes.extend((ids.len() as u32).to_le_bytes());
    bytes.extend(fields.to_le_bytes());
    bytes.extend((fields * 4).to_le_bytes());
    bytes.extend(1_u32.to_le_bytes());
    for &id in ids {
        let mut record = vec![0; fields as usize * 4];
        set_word(&mut record, 0, id);
        if fields >= 4 {
            set_word(&mut record, 12, u32::MAX);
        }
        bytes.extend(record);
    }
    bytes.push(0);
    bytes
}

#[test]
fn liquid_prerequisite_uses_last_physical_id_and_retains_source_overwrite_domain() {
    assert!(parse_liquid_type_dbc(&liquid_dbc(&[9, 2, 9, 12], 4)).is_ok());
    assert!(parse_liquid_type_dbc(&liquid_dbc(&[0], 5)).is_ok());
    assert_eq!(
        parse_liquid_type_dbc(&liquid_dbc(&[9, 2], 4))
            .unwrap_err()
            .code(),
        "vmap.liquid_last_row_not_max"
    );
    assert_eq!(
        parse_liquid_type_dbc(&liquid_dbc(&[], 4))
            .unwrap_err()
            .code(),
        "vmap.liquid_dbc_rows"
    );
    assert_eq!(
        parse_liquid_type_dbc(&liquid_dbc(&[0], 3))
            .unwrap_err()
            .code(),
        "vmap.liquid_dbc_layout"
    );
    assert_eq!(
        parse_liquid_type_dbc(&liquid_dbc(&[1_000_001], 4))
            .unwrap_err()
            .code(),
        "vmap.liquid_id_limit"
    );
    let mut bytes = liquid_dbc(&[0], 4);
    bytes.pop();
    assert_eq!(
        parse_liquid_type_dbc(&bytes).unwrap_err().code(),
        "vmap.liquid_dbc_length"
    );
    let mut bytes = liquid_dbc(&[0], 4);
    bytes[0] = b'X';
    assert_eq!(
        parse_liquid_type_dbc(&bytes).unwrap_err().code(),
        "vmap.liquid_dbc_magic"
    );
}

#[test]
fn listfile_root_storage_is_bounded_before_coordinator_admission() {
    let bytes = b"A.wmo\r\n".repeat(MAX_LISTFILE_ROOTS + 1);
    assert_eq!(
        parse_wmo_listfile(&bytes).err().unwrap().code(),
        "vmap.listfile_root_count"
    );
}

#[test]
fn empty_table_names_are_explicit_refusals_never_ordinal_collapsing() {
    for names in [&b"A.MDX\0\0B.MDX\0"[..], &b"A.MDX\0\0"[..]] {
        assert_eq!(
            adt_error(&file(&[(b"MMDX", names.to_vec()), (b"MDDF", mddf(2, 1))])),
            "vmap.member_name_length"
        );
    }
    assert_eq!(
        adt_error(&file(&[
            (b"MWMO", b"A.WMO\0\0B.WMO\0".to_vec()),
            (b"MODF", modf(2, 1))
        ])),
        "vmap.member_name_length"
    );
}

#[test]
fn doodad_extraction_keeps_original_spelling_until_later_lookup() {
    let dependency = doodad_model_dependency(b"World\\MODEL NAME.MDX")
        .unwrap()
        .unwrap();
    assert_eq!(dependency.member, "World\\MODEL NAME.M2");
    assert_eq!(dependency.extracted_name, "MODEL NAME.M2");
    assert_eq!(
        doodad_lookup_name(b"World\\MODEL NAME.MDX").unwrap(),
        "Model_Name.m2"
    );
    let dependency = doodad_model_dependency(b"World\\Chair.mdx")
        .unwrap()
        .unwrap();
    assert_eq!(dependency.member, "World\\Chair.m2");
    assert_eq!(
        dependency.extracted_name,
        doodad_lookup_name(b"World\\Chair.mdx").unwrap()
    );
    assert!(doodad_model_dependency(b"").unwrap().is_none());
    assert!(doodad_model_dependency(b"a").unwrap().is_none());
    assert!(doodad_model_dependency(b"World\\Chair.MdX").is_err());
    assert!(doodad_model_dependency(b"..\\Chair.mdx").is_err());
}
