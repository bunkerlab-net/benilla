use super::*;
use crate::source::tests::{archive, raw_entry, Entry, Temp};
use std::{cell::Cell, fs};

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

const LIQUID: &str = "DBFilesClient\\LiquidType.dbc";
const MAP: &str = "DBFilesClient\\Map.dbc";
const WDT: &str = "World\\Maps\\Selected\\Selected.wdt";
const ADT: &str = "World\\Maps\\Selected\\Selected_0_0.adt";

fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes: Vec<_> = tag.iter().rev().copied().collect();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
fn set_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn dbc(fields: u32, rows: &[Vec<u8>], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len() as u32, fields, fields * 4, strings.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for row in rows {
        bytes.extend(row);
    }
    bytes.extend(strings);
    bytes
}
fn maps(rows: &[(u32, &str)]) -> Vec<u8> {
    let mut strings = vec![0];
    let records: Vec<_> = rows
        .iter()
        .map(|(id, name)| {
            let mut row = vec![0; 168];
            set_word(&mut row, 0, *id);
            set_word(&mut row, 4, strings.len() as u32);
            strings.extend(name.as_bytes());
            strings.push(0);
            row
        })
        .collect();
    dbc(42, &records, &strings)
}
fn prerequisites() -> Vec<Entry> {
    vec![
        raw_entry(LIQUID, dbc(4, &[vec![0; 16]], &[0])),
        raw_entry(MAP, maps(&[(0, "Selected")])),
        raw_entry(WDT, chunk(b"MVER", &18u32.to_le_bytes())),
    ]
}
fn m2(collision: bool) -> Vec<u8> {
    let mut bytes = vec![0; 324];
    bytes[..4].copy_from_slice(b"MD20");
    set_word(&mut bytes, 4, 256);
    if collision {
        set_word(&mut bytes, 0xec, 3);
        set_word(&mut bytes, 0xf0, 324);
        set_word(&mut bytes, 0xf4, 3);
        set_word(&mut bytes, 0xf8, 330);
        for value in [0u16, 1, 2] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}
fn adt(names: &[&str], placements: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = chunk(b"MVER", &18u32.to_le_bytes());
    let names: Vec<_> = names
        .iter()
        .flat_map(|name| name.bytes().chain([0]))
        .collect();
    bytes.extend(chunk(b"MMDX", &names));
    let mut records = Vec::new();
    for &(index, unique_id) in placements {
        let mut record = [0; 36];
        set_word(&mut record, 0, index);
        set_word(&mut record, 4, unique_id);
        record[32..34].copy_from_slice(&1024u16.to_le_bytes());
        records.extend(record);
    }
    bytes.extend(chunk(b"MDDF", &records));
    bytes
}
fn wmo_root(groups: u32) -> Vec<u8> {
    let mut header = [0; 64];
    set_word(&mut header, 4, groups);
    let mut bytes = chunk(b"MVER", &17u32.to_le_bytes());
    bytes.extend(chunk(b"MOHD", &header));
    bytes.extend(chunk(b"MOGN", b"room\0"));
    bytes
}
fn wmo_group(skip: bool) -> Vec<u8> {
    let mut group = vec![0; 68];
    set_word(&mut group, 8, if skip { 0x80 } else { 0 });
    group.extend(chunk(b"MOPY", &[8, 0]));
    group.extend(chunk(b"MOVI", &[0, 0, 1, 0, 2, 0]));
    group.extend(chunk(b"MOVT", &[0; 36]));
    group.extend(chunk(b"MOBA", &[]));
    let mut bytes = chunk(b"MVER", &17u32.to_le_bytes());
    bytes.extend(chunk(b"MOGP", &group));
    bytes
}
fn wmo_adt(name: &str, flags: u16, unique_id: u32) -> Vec<u8> {
    let mut bytes = chunk(b"MVER", &18u32.to_le_bytes());
    let mut name = name.as_bytes().to_vec();
    name.push(0);
    bytes.extend(chunk(b"MWMO", &name));
    let mut record = [0; 64];
    set_word(&mut record, 4, unique_id);
    record[56..58].copy_from_slice(&flags.to_le_bytes());
    bytes.extend(chunk(b"MODF", &record));
    bytes
}
fn save(data: &Path, name: &str, entries: &[Entry]) {
    fs::write(data.join(name), archive(entries)).unwrap();
}
fn wmo_entries(groups: u32) -> Vec<Entry> {
    let mut entries = prerequisites();
    entries.push(raw_entry(
        "(listfile)",
        b"World\\Wmo\\Temple.wmo\r\n\0".to_vec(),
    ));
    entries.push(raw_entry("World\\Wmo\\Temple.wmo", wmo_root(groups)));
    for index in 0..groups {
        entries.push(raw_entry(
            &format!("World\\Wmo\\Temple_{index:03}.wmo"),
            wmo_group(index == 1),
        ));
    }
    entries
}

#[test]
fn whole_map_ignores_main_and_preserves_x_major_raw_ids_and_unused_models() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries[2].bytes.extend(chunk(b"MAIN", &[0; 4096 * 8]));
    entries[2].declared = entries[2].bytes.len() as u32;
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_0_2.adt",
        adt(
            &["Models\\Used.mdx", "Models\\Unused.mdx"],
            &[(0, 900), (0, 100)],
        ),
    ));
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_1_0.adt",
        adt(&["Models\\Used.mdx"], &[(0, 900)]),
    ));
    entries.push(raw_entry("Models\\Used.m2", m2(true)));
    entries.push(raw_entry("Models\\Unused.m2", m2(true)));
    save(&data, "terrain.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().adt_attempts, 4096);
    assert_eq!(prepared.summary().adts_present, 2);
    assert_eq!(prepared.summary().adts_absent, 4094);
    assert_eq!(prepared.products().len(), 2);
    assert_eq!(prepared.summary().dependency_events, 3);
    assert_eq!(prepared.summary().member_lookups, 4102);
    let ids: Vec<_> = prepared
        .placements()
        .iter()
        .map(|p| match p.input() {
            PlacementInput::M2(m) => m.unique_id,
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, [900, 100, 900]);
    assert_eq!(prepared.placements()[0].tile(), Tile { x: 0, y: 2 });
    assert_eq!(prepared.placements()[2].tile(), Tile { x: 1, y: 0 });
    assert!(prepared
        .placements()
        .iter()
        .all(|p| p.disposition() == PlacementDisposition::Emits));
    prepared.verify_unchanged(|| false).unwrap();
    assert!(!format!("{prepared:?}").contains("Models"));
}

#[test]
fn wmo_prepass_captures_every_declared_group_including_skip_then_resolves_basename() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(2);
    // The global prepass owns the raw identity used by the matching MWMO path.
    entries.push(raw_entry(ADT, wmo_adt("World\\Wmo\\Temple.wmo", 0, 72)));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().global_root_events, 1);
    assert_eq!(prepared.summary().wmo_raw_products, 1);
    assert_eq!(prepared.products()[0].wmo_group_counts(), Some((2, 1)));
    assert_eq!(
        prepared.placements()[0].disposition(),
        PlacementDisposition::Emits
    );
    let mut changed = wmo_group(true);
    changed[32] ^= 1;
    let group = entries
        .iter_mut()
        .find(|e| e.name.ends_with("_001.wmo"))
        .unwrap();
    group.bytes = changed;
    save(&data, "wmo.MPQ", &entries);
    assert!(matches!(
        prepared.verify_unchanged(|| false),
        Err(Error::Source(SourceError::Changed))
    ));
}

#[test]
fn missing_skipped_group_cannot_return_partial_raw_product() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(2);
    entries.pop();
    save(&data, "wmo.MPQ", &entries);
    let error = prepare(&data, 0, || false).unwrap_err();
    assert!(matches!(error.category(), Error::RequiredMemberAbsent));
    assert!(matches!(
        error,
        Error::Preparation {
            stage: PreparationStage::GlobalWmoRoot,
            global_root_ordinal: Some(0),
            ..
        }
    ));
}

#[test]
fn global_prepass_winner_and_listfile_order_are_receipted_before_map() {
    let temp = Temp::new();
    let data = temp.data();
    let entries = wmo_entries(1);
    save(&data, "wmo.MPQ", &entries);
    save(
        &data,
        "patch-2.MPQ",
        &[
            raw_entry("(listfile)", b"World\\Wmo\\Second.wmo\r\n\0".to_vec()),
            raw_entry("World\\Wmo\\Second.wmo", wmo_root(0)),
            raw_entry("World\\Wmo\\Temple.wmo", wmo_root(0)),
        ],
    );
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.products()[0].extracted_name(), "Second.wmo");
    assert_eq!(prepared.products()[1].extracted_name(), "Temple.wmo");
    assert_eq!(prepared.products()[1].wmo_group_counts(), Some((0, 0)));
    assert!(!prepared.products()[1].source_has_collision());
    save(
        &data,
        "patch-2.MPQ",
        &[
            raw_entry(
                "(listfile)",
                b"World\\Wmo\\Temple.wmo\r\nWorld\\Wmo\\Second.wmo\r\n\0".to_vec(),
            ),
            raw_entry("World\\Wmo\\Second.wmo", wmo_root(0)),
            raw_entry("World\\Wmo\\Temple.wmo", wmo_root(0)),
        ],
    );
    assert!(prepared.verify_unchanged(|| false).is_err());
}

#[test]
fn no_collision_m2_is_receipted_and_reread_because_it_emits_no_cached_file() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(
        ADT,
        adt(
            &["Models\\Empty.mdx", "Models\\Empty.mdx"],
            &[(0, 1), (1, 2)],
        ),
    ));
    entries.push(raw_entry("Models\\Empty.m2", m2(false)));
    save(&data, "model.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.products().len(), 1);
    assert_eq!(prepared.summary().member_lookups, 4102);
    assert_eq!(prepared.summary().m2_no_collision_products, 1);
    assert_eq!(prepared.summary().no_collision_placements, 2);
    assert!(prepared.products()[0].raw_vmap().is_none());
    assert_eq!(
        prepared.placements()[0].disposition(),
        PlacementDisposition::NoCollision
    );
}

#[test]
fn wmo_zero_triangle_and_destructible_filter_order_match_source() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(0);
    entries.push(raw_entry(ADT, wmo_adt("World\\Wmo\\Temple.wmo", 1, 20)));
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_0_1.adt",
        wmo_adt("World\\Wmo\\Temple.wmo", 0, 21),
    ));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.products()[0].raw_vmap().unwrap().len(), 20);
    assert_eq!(
        prepared.placements()[0].disposition(),
        PlacementDisposition::Destructible
    );
    assert_eq!(
        prepared.placements()[1].disposition(),
        PlacementDisposition::NoCollision
    );
}

#[test]
fn selected_wdt_absent_or_dummy_gates_all_adt_attempts_and_is_rechecked() {
    for dummy in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = prerequisites();
        entries.pop();
        if dummy {
            entries.push(raw_entry(WDT, vec![1]));
        }
        save(&data, "terrain.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();
        assert_eq!(prepared.summary().adt_attempts, 0);
        assert_eq!(
            prepared.summary().state,
            if dummy {
                MapDiscoveryState::WdtDummy
            } else {
                MapDiscoveryState::WdtAbsent
            }
        );
        save(
            &data,
            "patch.MPQ",
            &[raw_entry(WDT, chunk(b"MVER", &18u32.to_le_bytes()))],
        );
        assert!(prepared.verify_unchanged(|| false).is_err());
    }
}

#[test]
fn dummy_adt_blocks_lower_archive_fallback_but_does_not_skip_attempt_receipt() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(ADT, adt(&[], &[])));
    save(&data, "terrain.MPQ", &entries);
    save(&data, "patch.MPQ", &[raw_entry(ADT, vec![])]);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().adts_present, 0);
    assert_eq!(prepared.summary().adts_dummy, 1);
    assert_eq!(prepared.summary().adt_attempts, 4096);
    save(&data, "patch.MPQ", &[]);
    assert!(prepared.verify_unchanged(|| false).is_err());
}

#[test]
fn distinct_global_and_m2_flat_name_collisions_fail_closed() {
    for wmo in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = prerequisites();
        if wmo {
            entries.push(raw_entry(
                "(listfile)",
                b"A\\Temple.wmo\r\nB\\Temple.wmo\r\n\0".to_vec(),
            ));
            entries.push(raw_entry("A\\Temple.wmo", wmo_root(0)));
            entries.push(raw_entry("B\\Temple.wmo", wmo_root(0)));
        } else {
            entries.push(raw_entry(ADT, adt(&["A\\Same.mdx", "B\\Same.mdx"], &[])));
            entries.push(raw_entry("A\\Same.m2", m2(true)));
            entries.push(raw_entry("B\\Same.m2", m2(true)));
        }
        save(&data, "terrain.MPQ", &entries);
        assert!(matches!(
            prepare(&data, 0, || false).unwrap_err().category(),
            Error::NameCollision
        ));
    }
}

#[test]
fn global_unsupported_closure_and_liquid_are_precise_and_never_leak_names() {
    for liquid in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = wmo_entries(1);
        if liquid {
            entries
                .last_mut()
                .unwrap()
                .bytes
                .extend(chunk(b"MLIQ", &[]));
        } else {
            entries
                .iter_mut()
                .find(|e| e.name == "World\\Wmo\\Temple.wmo")
                .unwrap()
                .bytes
                .extend(chunk(b"MODN", b"PRIVATE_ASSET_NAME\0"));
        }
        for entry in &mut entries {
            entry.declared = entry.bytes.len() as u32;
        }
        save(&data, "wmo.MPQ", &entries);
        let error = prepare(&data, 0, || false).unwrap_err();
        if liquid {
            assert!(matches!(
                error.category(),
                Error::Wmo(benilla_native_vmap::WmoError::InvalidChunkSize {
                    chunk: [b'M', b'L', b'I', b'Q'],
                    ..
                })
            ));
        } else {
            assert!(matches!(error.category(), Error::Discovery(_)));
        }
        let diagnostic = format!("{error:?} {error}");
        assert!(!diagnostic.contains("PRIVATE_ASSET_NAME"));
        assert!(!diagnostic.contains("Temple"));
        assert!(!diagnostic.contains(&data.display().to_string()));
    }
}

#[test]
fn nonempty_set_ranges_are_metadata_not_a_false_dependency_rejection() {
    let mut root = wmo_root(0);
    root.extend(chunk(b"MODS", &[0; 32]));
    let length = root.len();
    set_word(&mut root, length - 8, 1);
    let metadata = doodads::parse_root_doodads(&root).unwrap();
    assert_eq!(metadata.sets[0].count, 1);
}

#[test]
fn prior_physical_maps_seed_cache_and_later_unsafe_map_names_are_not_consumed() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries[1] = raw_entry(
        MAP,
        maps(&[(90, "Earlier"), (0, "Selected"), (2, "bad/path")]),
    );
    entries.push(raw_entry(
        "World\\Maps\\Earlier\\Earlier.wdt",
        chunk(b"MVER", &18u32.to_le_bytes()),
    ));
    entries.push(raw_entry(
        "World\\Maps\\Earlier\\Earlier_0_0.adt",
        adt(&["Models\\Seed.mdx"], &[(0, 100)]),
    ));
    entries.push(raw_entry(ADT, adt(&["Models\\Seed.mdx"], &[(0, 200)])));
    entries.push(raw_entry("Models\\Seed.m2", m2(true)));
    save(&data, "terrain.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().map_record_ordinal, 1);
    assert_eq!(prepared.summary().prefix_maps, 1);
    assert_eq!(prepared.summary().prefix_adt_attempts, 4096);
    assert_eq!(prepared.products().len(), 1);
    assert_eq!(prepared.placements().len(), 1);
    assert!(
        matches!(prepared.placements()[0].input(), PlacementInput::M2(m) if m.unique_id == 200)
    );
}

#[test]
fn cancellation_and_changed_input_during_full_scan_return_no_prepared_value() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(ADT, adt(&["Models\\Used.mdx"], &[(0, 1)])));
    entries.push(raw_entry("Models\\Used.m2", m2(true)));
    save(&data, "terrain.MPQ", &entries);
    let calls = Cell::new(0usize);
    let error = prepare(&data, 0, || {
        calls.set(calls.get() + 1);
        calls.get() > 1000
    })
    .unwrap_err();
    assert!(matches!(
        error.category(),
        Error::Cancelled | Error::Source(SourceError::Cancelled)
    ));
    let calls = Cell::new(0usize);
    let changed = Cell::new(false);
    entries.last_mut().unwrap().bytes[334] ^= 1;
    let replacement = archive(&entries);
    let error = prepare(&data, 0, || {
        calls.set(calls.get() + 1);
        if calls.get() == 1000 {
            fs::write(data.join("terrain.MPQ"), &replacement).unwrap();
            changed.set(true);
        }
        false
    })
    .unwrap_err();
    assert!(changed.get());
    assert!(matches!(
        error.category(),
        Error::Source(SourceError::Changed)
    ));
}

#[test]
fn retained_raw_budget_has_checked_inclusive_boundary() {
    assert_eq!(
        retained_total(MAX_RETAINED_RAW_BYTES - 1, 1).unwrap(),
        MAX_RETAINED_RAW_BYTES
    );
    assert!(matches!(
        retained_total(MAX_RETAINED_RAW_BYTES, 1),
        Err(Error::RetainedOutputLimit)
    ));
    assert!(matches!(
        retained_total(usize::MAX, 1),
        Err(Error::RetainedOutputLimit)
    ));
}

#[test]
fn public_diagnostic_serialization_never_includes_inner_magic_or_chunk_bytes() {
    for error in [
        Error::M2(benilla_native_vmap::Error::UnsupportedMagic(*b"LEAK")),
        Error::Wmo(benilla_native_vmap::WmoError::UnexpectedChunk {
            file: benilla_native_vmap::WmoFile::Root,
            chunk: *b"LEAK",
        }),
    ] {
        let json = serde_json::to_string(&error.diagnostic()).unwrap();
        assert!(!json.contains("LEAK"));
        assert!(!format!("{error:?}").contains("[76, 69, 65, 75]"));
        assert!(!format!("{error}").contains("LEAK"));
    }
}

#[test]
fn wmo_reference_with_conflicting_directory_is_narrower_fail_closed_identity() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(0);
    entries.push(raw_entry(
        ADT,
        wmo_adt("Different\\Directory\\Temple.wmo", 0, 72),
    ));
    save(&data, "wmo.MPQ", &entries);
    assert!(matches!(
        prepare(&data, 0, || false).unwrap_err().category(),
        Error::NameCollision
    ));
}

#[test]
fn skipped_wmo_group_references_do_not_create_candidates() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(2);
    let skipped = entries.last_mut().unwrap();
    skipped.bytes.extend(chunk(b"MODR", &[0, 0]));
    let group_bytes = word(&skipped.bytes, 16) + 10;
    set_word(&mut skipped.bytes, 16, group_bytes);
    skipped.declared = skipped.bytes.len() as u32;
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert!(prepared.doodad_candidates().is_empty());
}

#[test]
fn doodad_metadata_and_candidate_bounds_are_checked_before_growth() {
    assert_eq!(retained_doodad_total(0, 1, 1, 1, 7).unwrap(), 81);
    assert_eq!(
        retained_doodad_total(MAX_RETAINED_DOODAD_BYTES, 0, 0, 0, 0).unwrap(),
        MAX_RETAINED_DOODAD_BYTES
    );
    assert!(retained_doodad_total(MAX_RETAINED_DOODAD_BYTES, 0, 0, 0, 1).is_err());
    assert!(retained_doodad_total(0, usize::MAX, 0, 0, 0).is_err());
    assert!(retained_doodad_total(0, 0, usize::MAX, 0, 0).is_err());
    assert!(retained_doodad_total(0, 0, 0, usize::MAX, 0).is_err());
    assert!(check_doodad_candidate_count(MAX_DOODAD_CANDIDATES - 1).is_ok());
    assert!(check_doodad_candidate_count(MAX_DOODAD_CANDIDATES).is_err());
    assert!(check_doodad_candidate_count(usize::MAX).is_err());
}

#[path = "doodad_capture_tests.rs"]
mod doodad_capture_tests;

#[path = "doodad_projection_capture_tests.rs"]
mod doodad_projection_capture_tests;

#[path = "liquid_capture_tests.rs"]
mod liquid_capture_tests;

#[test]
fn m2_record_stream_preserves_source_coordinate_scale_name_and_duplicate_ids() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    let mut input = adt(&["Models\\Solid.mdx"], &[(0, 42), (0, 42)]);
    let start = input.len() - 72;
    for record in 0..2 {
        let at = start + record * 36;
        for (i, v) in [1.25_f32, -0.0, 9.0, -45.0, 180.0, 360.0]
            .into_iter()
            .enumerate()
        {
            set_word(&mut input, at + 8 + i * 4, v.to_bits());
        }
        input[at + 32..at + 34].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    entries.push(raw_entry(ADT, input));
    entries.push(raw_entry("Models\\Solid.m2", m2(true)));
    save(&data, "model.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    let profile = DoodadProjectionProfile::from_policy_id("g3d-portable-libm-0.2.16-v1").unwrap();
    let records = prepared.placement_records(profile, || false).unwrap();
    let mut expected = Vec::new();
    for v in [0_u32, 0, 0, 1] {
        expected.extend_from_slice(&v.to_le_bytes());
    }
    expected.extend_from_slice(&0_u16.to_le_bytes());
    expected.extend_from_slice(&1_u32.to_le_bytes());
    for v in [9.0_f32, 1.25, -0.0, -45.0, 180.0, 360.0, 65535.0 / 1024.0] {
        expected.extend_from_slice(&v.to_le_bytes());
    }
    expected.extend_from_slice(&8_u32.to_le_bytes());
    expected.extend_from_slice(b"Solid.m2");
    assert_eq!(
        records.bytes(),
        [expected.as_slice(), expected.as_slice()].concat()
    );
    assert_eq!(records.record_count(), 2);
    let again = prepared.placement_records(profile, || false).unwrap();
    assert_eq!(
        records.preprocessing_binding(),
        again.preprocessing_binding()
    );
}
