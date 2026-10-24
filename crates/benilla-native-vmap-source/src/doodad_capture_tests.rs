// SPDX-License-Identifier: GPL-3.0-or-later
// Authored MPQ bytes only, checked against the directly read CMaNGOS source at
// 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f: vmapextract/wmo.cpp::WMORoot::open,
// vmapexport.cpp::ExtractSingleWmo, gameobject_extract.cpp::ExtractSingleModel,
// model.cpp::Doodad::ExtractSet, and adtfile.cpp's MODF dispatch.
// Membership cases and explicit GCC14 ordering cases are separate. Neither
// establishes transformed placement values or generated IDs.

use super::*;
use std::collections::BTreeSet;

const ROOT: &str = "World\\Wmo\\Temple.wmo";
const SOLID: &str = "Models\\Solid.m2";
const EMPTY: &str = "Models\\Empty.m2";

fn dc_names(paths: &[&str]) -> (Vec<u8>, Vec<u32>) {
    let mut bytes = Vec::new();
    let mut offsets = Vec::new();
    for path in paths {
        offsets.push(bytes.len() as u32);
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
    }
    (bytes, offsets)
}

fn dc_sets(ranges: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for &(start, count) in ranges {
        let mut record = [0; 32];
        record[..18].copy_from_slice(b"PRIVATE_SET_MARKER");
        set_word(&mut record, 20, start);
        set_word(&mut record, 24, count);
        record[28..].copy_from_slice(&[0x17, 0xa4, 0xe3, 0x20]);
        bytes.extend_from_slice(&record);
    }
    bytes
}

fn dc_spawn(name_word: u32) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    set_word(&mut bytes, 0, name_word);
    // Deliberately nonunit quaternion, negative scale, and signed zero: these
    // are finite raw MODD values, not inputs to a transform in this layer.
    for (index, value) in [3.0_f32, -2.0, -0.0, 0.25, -0.5, 2.0, -4.0, -0.75]
        .into_iter()
        .enumerate()
    {
        set_word(&mut bytes, 4 + index * 4, value.to_bits());
    }
    set_word(&mut bytes, 36, 0xe1a2_b3c4);
    bytes
}

fn dc_root(groups: u32, paths: &[u8], sets: &[(u32, u32)], spawns: &[u32]) -> Vec<u8> {
    let mut bytes = wmo_root(groups);
    set_word(
        &mut bytes,
        20 + 16,
        paths.iter().filter(|&&b| b == 0).count() as u32,
    );
    set_word(&mut bytes, 20 + 20, spawns.len() as u32);
    set_word(&mut bytes, 20 + 24, sets.len() as u32);
    bytes.extend(chunk(b"MODN", paths));
    bytes.extend(chunk(b"MODS", &dc_sets(sets)));
    let records: Vec<_> = spawns.iter().flat_map(|&index| dc_spawn(index)).collect();
    bytes.extend(chunk(b"MODD", &records));
    bytes
}

fn dc_group(skip: bool, collision: bool, references: &[u16]) -> Vec<u8> {
    let mut bytes = wmo_group(skip);
    // MVER (12), MOGP header (8 + 68), then the MOPY header (8).
    bytes[12 + 8 + 68 + 8] = if collision { 8 } else { 0 };
    let references: Vec<_> = references
        .iter()
        .flat_map(|reference| reference.to_le_bytes())
        .collect();
    let child = chunk(b"MODR", &references);
    let length = word(&bytes, 16) + child.len() as u32;
    set_word(&mut bytes, 16, length);
    bytes.extend(child);
    bytes
}

fn dc_adt(flags: u16, set: u16, unique_id: u32) -> Vec<u8> {
    let mut bytes = wmo_adt(ROOT, flags, unique_id);
    let start = bytes.len() - 64;
    bytes[start + 58..start + 60].copy_from_slice(&set.to_le_bytes());
    for (index, value) in [900.0_f32, 800.0, 700.0, 60.0, 120.0, 180.0]
        .into_iter()
        .enumerate()
    {
        set_word(&mut bytes, start + 8 + index * 4, value.to_bits());
    }
    bytes
}

fn dc_replace(entries: &mut [Entry], name: &str, bytes: Vec<u8>) {
    let entry = entries.iter_mut().find(|entry| entry.name == name).unwrap();
    *entry = raw_entry(name, bytes);
}

fn dc_membership(prepared: &PreparedDiscovery) -> BTreeSet<(usize, u16)> {
    prepared
        .doodad_candidates()
        .iter()
        .map(|candidate| (candidate.wmo_placement_index, candidate.spawn_index))
        .collect()
}

#[test]
fn every_modn_chunk_extracts_unused_models_before_any_required_group() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    let mut root = wmo_root(1);
    root.extend(chunk(b"MODN", b"Models\\First.mdx\0"));
    root.extend(chunk(b"MODN", b"Models\\Later.mdl\0"));
    dc_replace(&mut entries, ROOT, root);
    entries.retain(|entry| !entry.name.ends_with("_000.wmo"));
    entries.push(raw_entry("Models\\First.m2", m2(true)));
    entries.push(raw_entry("Models\\Later.m2", m2(true)));
    save(&data, "wmo.MPQ", &entries);

    let error = prepare(&data, 0, || false).unwrap_err();
    assert!(matches!(error.category(), Error::RequiredMemberAbsent));
    match error {
        Error::Preparation {
            stage: PreparationStage::GlobalWmoRoot,
            global_root_ordinal: Some(0),
            progress,
            ..
        } => {
            // LiquidType, listfile, root, both models, missing group. No root
            // product or map traversal escapes this incomplete group closure.
            assert_eq!(progress.member_lookups, 6);
            assert_eq!(progress.complete_raw_products, 2);
            assert_eq!(progress.adt_attempts, 0);
        }
        other => panic!("unexpected safe diagnostic: {other:?}"),
    }

    entries.push(raw_entry(
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[]),
    ));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    let names: Vec<_> = prepared
        .products()
        .iter()
        .map(RawProduct::extracted_name)
        .collect();
    // Product insertion follows extraction calls, independently of reference
    // sets. The first MODN is not discarded merely because Paths is replaced.
    assert_eq!(names, ["First.m2", "Later.m2", "Temple.wmo"]);
    assert_eq!(prepared.summary().doodad_extraction_events, 2);
    assert_eq!(prepared.summary().doodad_candidates, 0);
    assert!(prepared.doodad_candidates().is_empty());
    prepared.verify_unchanged(|| false).unwrap();
}

#[test]
fn unused_missing_modn_model_fails_before_an_also_missing_group() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(1, b"Models\\Missing.mdx\0", &[], &[]),
    );
    entries.retain(|entry| !entry.name.ends_with("_000.wmo"));
    save(&data, "wmo.MPQ", &entries);
    let error = prepare(&data, 0, || false).unwrap_err();
    assert!(matches!(error.category(), Error::RequiredMemberAbsent));
    let progress = error.diagnostic().progress.unwrap();
    assert_eq!(progress.member_lookups, 4);
    assert_eq!(progress.complete_raw_products, 0);
    assert_eq!(progress.adt_attempts, 0);
}

#[test]
fn duplicate_modn_names_cache_emitted_files_retry_zero_hulls_and_keep_nul_offsets() {
    let temp = Temp::new();
    let data = temp.data();
    let (paths, offsets) = dc_names(&[
        "Models\\Solid.mdx",
        "",
        "Models\\Solid.mdx",
        "Models\\Empty.mdx",
        "Models\\Empty.mdx",
    ]);
    let mut entries = wmo_entries(1);
    dc_replace(&mut entries, ROOT, dc_root(1, &paths, &[(0, 5)], &offsets));
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0, 1, 2, 3, 4]),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    entries.push(raw_entry(EMPTY, m2(false)));
    entries.push(raw_entry(ADT, dc_adt(0, 0, 77)));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();

    assert_eq!(prepared.summary().doodad_extraction_events, 5);
    assert_eq!(prepared.summary().dependency_events, 6);
    // 4102 prerequisite/root/group/map/ADT reads plus one solid read and two
    // empty-hull reads. The empty name has an event but performs no lookup.
    assert_eq!(prepared.summary().member_lookups, 4105);
    assert_eq!(prepared.summary().m2_raw_products, 1);
    assert_eq!(prepared.summary().m2_no_collision_products, 1);
    assert_eq!(prepared.summary().wmo_raw_products, 1);
    assert_eq!(prepared.summary().doodad_candidates, 2);
    assert_eq!(dc_membership(&prepared), BTreeSet::from([(0, 0), (0, 2)]));
    let solid = prepared
        .products()
        .iter()
        .position(|product| product.source_member() == SOLID)
        .unwrap();
    for candidate in prepared.doodad_candidates() {
        assert_eq!(candidate.product_index, solid);
        assert_eq!(
            candidate.spawn.name_index,
            offsets[candidate.spawn_index as usize]
        );
    }
    let empty = prepared
        .products()
        .iter()
        .find(|product| product.source_member() == EMPTY)
        .unwrap();
    assert!(empty.raw_vmap().is_none());
    prepared.verify_unchanged(|| false).unwrap();
}

#[test]
fn modr_uses_retained_groups_even_with_zero_triangles_and_deduplicates_valid_spawns() {
    let temp = Temp::new();
    let data = temp.data();
    let (paths, offsets) = dc_names(&["Models\\Solid.mdx", "Models\\Empty.mdx"]);
    let spawns = [
        offsets[0],
        offsets[0],
        offsets[0],
        offsets[1],
        1,
        0x00ff_ffff,
    ];
    let mut entries = wmo_entries(3);
    dc_replace(&mut entries, ROOT, dc_root(3, &paths, &[(0, 6)], &spawns));
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0, 0, 3, 4, 5, 6, u16::MAX]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_001.wmo",
        dc_group(true, true, &[1]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_002.wmo",
        dc_group(false, false, &[0, 2, 2]),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    entries.push(raw_entry(EMPTY, m2(false)));
    entries.push(raw_entry(ADT, dc_adt(0, 0, 88)));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();

    let parent = &prepared.products()[prepared.placements()[0].product_index()];
    assert_eq!(parent.wmo_group_counts(), Some((3, 2)));
    assert!(parent.source_has_collision());
    assert_eq!(prepared.doodad_candidates().len(), 2);
    assert_eq!(dc_membership(&prepared), BTreeSet::from([(0, 0), (0, 2)]));
    // Spawn 1 is referenced only by the skipped group; spawn 2 is referenced
    // only by a retained zero-triangle group. Invalid name offsets and MODR
    // indices, zero-hull names, and repeated references add no candidates.
}

#[test]
fn mods_selects_only_the_requested_half_open_interval_without_a_default_union() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(
            1,
            b"Models\\Solid.mdx\0",
            &[(0, 2), (2, 2), (4, 0)],
            &[0; 5],
        ),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0, 1, 2, 3, 4]),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    for (tile, set) in [1, u16::MAX, 0, 2].into_iter().enumerate() {
        entries.push(raw_entry(
            &format!("World\\Maps\\Selected\\Selected_0_{tile}.adt"),
            dc_adt(0, set, 100 + tile as u32),
        ));
    }
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();

    assert_eq!(prepared.placements().len(), 4);
    assert_eq!(prepared.summary().doodad_candidates, 4);
    assert_eq!(
        dc_membership(&prepared),
        BTreeSet::from([(0, 2), (0, 3), (2, 0), (2, 1)])
    );
    assert!(prepared
        .doodad_candidates()
        .iter()
        .all(|candidate| candidate.wmo_placement_index != 1 && candidate.wmo_placement_index != 3));
}

#[test]
fn parent_destructible_and_no_collision_dispositions_do_not_suppress_raw_doodads() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(1, b"Models\\Solid.mdx\0", &[(0, 1)], &[0xa900_0000]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, false, &[0]),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    entries.push(raw_entry(ADT, dc_adt(1, 0, 123)));
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_0_1.adt",
        dc_adt(0, 0, 456),
    ));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();

    assert_eq!(
        prepared.placements()[0].disposition(),
        PlacementDisposition::Destructible
    );
    assert_eq!(
        prepared.placements()[1].disposition(),
        PlacementDisposition::NoCollision
    );
    assert_eq!(prepared.summary().emitting_placements, 0);
    assert_eq!(prepared.summary().doodad_candidates, 2);
    assert_eq!(dc_membership(&prepared), BTreeSet::from([(0, 0), (1, 0)]));
    for candidate in prepared.doodad_candidates() {
        assert_eq!(candidate.spawn.name_index, 0);
        assert_eq!(candidate.spawn.ignored_name_high_byte, 0xa9);
        assert_eq!(candidate.spawn.position, [3.0, -2.0, -0.0]);
        assert_eq!(candidate.spawn.position[2].to_bits(), (-0.0_f32).to_bits());
        assert_eq!(candidate.spawn.rotation, [0.25, -0.5, 2.0, -4.0]);
        assert_eq!(candidate.spawn.scale, -0.75);
        assert_eq!(candidate.spawn.color, 0xe1a2_b3c4);
        assert_eq!(
            prepared.products()[candidate.product_index].source_member(),
            SOLID
        );
    }
}

#[test]
fn replaced_modn_paths_choose_the_final_name_without_losing_earlier_extractions() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    let mut root = dc_root(1, b"Models\\First.mdx\0", &[(0, 1)], &[0]);
    root.extend(chunk(b"MODN", b"Models\\Later.mdx\0"));
    dc_replace(&mut entries, ROOT, root);
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0]),
    );
    entries.push(raw_entry("Models\\First.m2", m2(true)));
    entries.push(raw_entry("Models\\Later.m2", m2(true)));
    entries.push(raw_entry(ADT, dc_adt(0, 0, 900)));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();

    assert_eq!(prepared.summary().doodad_extraction_events, 2);
    assert_eq!(prepared.summary().m2_raw_products, 2);
    assert_eq!(prepared.doodad_candidates().len(), 1);
    let candidate = &prepared.doodad_candidates()[0];
    assert_eq!(candidate.spawn.name_index, 0);
    assert_eq!(
        prepared.products()[candidate.product_index].source_member(),
        "Models\\Later.m2"
    );
}

#[test]
fn final_modn_suffix_lookup_can_be_seeded_by_a_later_global_wmo_root() {
    for later_root_extracted in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = wmo_entries(1);
        let mut root = dc_root(1, b"\0\0\0Foo.mdx\0", &[(0, 1)], &[3]);
        root.extend(chunk(b"MODN", b"Ab_Bar.mdx\0"));
        dc_replace(&mut entries, ROOT, root);
        dc_replace(
            &mut entries,
            "World\\Wmo\\Temple_000.wmo",
            dc_group(false, true, &[0]),
        );
        if later_root_extracted {
            dc_replace(
                &mut entries,
                "(listfile)",
                b"World\\Wmo\\Temple.wmo\r\nWorld\\Wmo\\Second.wmo\r\n\0".to_vec(),
            );
        }
        entries.push(raw_entry(
            "World\\Wmo\\Second.wmo",
            dc_root(0, b"Bar.mdx\0", &[], &[]),
        ));
        for member in ["Foo.m2", "Ab_Bar.m2", "Bar.m2"] {
            entries.push(raw_entry(member, m2(true)));
        }
        entries.push(raw_entry(ADT, dc_adt(0, 0, 1000)));
        save(&data, "wmo.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();

        // WMORoot's valid offset 3 survives the final Paths replacement. That
        // offset now addresses Bar.mdx inside Ab_Bar.mdx. ExtractSet consults
        // files after the entire global WMO pass, not just Temple's capture.
        assert_eq!(
            prepared.summary().doodad_extraction_events,
            if later_root_extracted { 6 } else { 5 }
        );
        assert_eq!(
            prepared.doodad_candidates().len(),
            usize::from(later_root_extracted)
        );
        if later_root_extracted {
            let candidate = &prepared.doodad_candidates()[0];
            assert_eq!(candidate.spawn.name_index, 3);
            assert_eq!(candidate.spawn_index, 0);
            assert_eq!(
                prepared.products()[candidate.product_index].source_member(),
                "Bar.m2"
            );
        }
    }
}

#[test]
fn unselected_references_do_not_dereference_replaced_empty_modn_paths() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    let mut root = dc_root(1, b"Models\\Solid.mdx\0", &[(0, 0), (1, 1), (0, 1)], &[0]);
    root.extend(chunk(b"MODN", &[]));
    dc_replace(&mut entries, ROOT, root);
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0]),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    for (tile, set) in [0, u16::MAX, 1].into_iter().enumerate() {
        entries.push(raw_entry(
            &format!("World\\Maps\\Selected\\Selected_0_{tile}.adt"),
            dc_adt(0, set, 2000 + tile as u32),
        ));
    }
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().doodad_extraction_events, 1);
    assert_eq!(prepared.placements().len(), 3);
    assert!(prepared.doodad_candidates().is_empty());
    // ExtractSet checks invalid set indices and the selected interval before
    // touching Paths. Empty sets and excluded references are defined even
    // when a final MODN replacement has removed the earlier valid offset.
    prepared.verify_unchanged(|| false).unwrap();

    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_0_3.adt",
        dc_adt(0, 2, 2003),
    ));
    save(&data, "wmo.MPQ", &entries);
    let error = prepare(&data, 0, || false).unwrap_err();
    assert!(matches!(error.category(), Error::Doodad(_)));
    assert_eq!(error.diagnostic().category, "vmap.doodad_name_offset");
    assert_eq!(
        error.diagnostic().stage,
        Some(PreparationStage::AdtTraversal)
    );
}

#[test]
fn changed_used_unused_zero_hull_and_skipped_group_dependencies_invalidate_capture() {
    for changed_name in [
        SOLID,
        "Models\\Unused.m2",
        EMPTY,
        "World\\Wmo\\Temple_001.wmo",
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let (paths, _) = dc_names(&[
            "Models\\Solid.mdx",
            "Models\\Unused.mdx",
            "Models\\Empty.mdx",
        ]);
        let mut entries = wmo_entries(2);
        dc_replace(&mut entries, ROOT, dc_root(2, &paths, &[(0, 1)], &[0]));
        dc_replace(
            &mut entries,
            "World\\Wmo\\Temple_000.wmo",
            dc_group(false, true, &[0]),
        );
        dc_replace(
            &mut entries,
            "World\\Wmo\\Temple_001.wmo",
            dc_group(true, true, &[0]),
        );
        entries.push(raw_entry(SOLID, m2(true)));
        entries.push(raw_entry("Models\\Unused.m2", m2(true)));
        entries.push(raw_entry(EMPTY, m2(false)));
        entries.push(raw_entry(ADT, dc_adt(0, 0, 44)));
        save(&data, "wmo.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();
        prepared.verify_unchanged(|| false).unwrap();

        let changed = entries
            .iter_mut()
            .find(|entry| entry.name == changed_name)
            .unwrap();
        *changed.bytes.last_mut().unwrap() ^= 1;
        save(&data, "wmo.MPQ", &entries);
        assert!(matches!(
            prepared.verify_unchanged(|| false),
            Err(Error::Source(SourceError::Changed))
        ));
    }
}

#[test]
fn original_extraction_basename_must_match_later_normalized_lookup_explicitly() {
    for (path, member) in [
        ("Models\\lower.mdx", "Models\\lower.m2"),
        ("Models\\Space Name.mdl", "Models\\Space Name.m2"),
        ("Models\\LOUD.MDX", "Models\\LOUD.M2"),
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let (paths, _) = dc_names(&[path]);
        let mut entries = wmo_entries(1);
        dc_replace(&mut entries, ROOT, dc_root(1, &paths, &[(0, 1)], &[0]));
        dc_replace(
            &mut entries,
            "World\\Wmo\\Temple_000.wmo",
            dc_group(false, true, &[0]),
        );
        entries.push(raw_entry(member, m2(true)));
        entries.push(raw_entry(ADT, dc_adt(0, 0, 123)));
        save(&data, "wmo.MPQ", &entries);
        let error = prepare(&data, 0, || false).unwrap_err();
        assert!(matches!(error.category(), Error::DoodadNameIdentity));
        assert_eq!(
            error.diagnostic().stage,
            Some(PreparationStage::GlobalWmoRoot)
        );
        let diagnostic = format!(
            "{error:?} {error} {}",
            serde_json::to_string(&error.diagnostic()).unwrap()
        );
        assert!(!diagnostic.contains(path));
        assert!(!diagnostic.contains(member));
        assert!(!diagnostic.contains("Temple"));
        assert!(!diagnostic.contains(&data.display().to_string()));
    }
}

#[test]
fn doodad_success_and_malformed_input_diagnostics_do_not_disclose_asset_or_set_text() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(1, b"Models\\Secret.mdx\0", &[(0, 1)], &[0]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0]),
    );
    entries.push(raw_entry("Models\\Secret.m2", m2(true)));
    entries.push(raw_entry(ADT, dc_adt(0, 0, 321)));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().doodad_candidates, 1);
    let diagnostic = format!(
        "{prepared:?} {:?} {}",
        prepared.products(),
        serde_json::to_string(prepared.summary()).unwrap()
    );
    for private in [
        "Secret",
        "Temple",
        "Models",
        "PRIVATE_SET_MARKER",
        &data.display().to_string(),
    ] {
        assert!(!diagnostic.contains(private));
    }

    let mut malformed = wmo_root(1);
    malformed.extend(chunk(b"MODN", b"PRIVATE_MALFORMED_ASSET"));
    dc_replace(&mut entries, ROOT, malformed);
    save(&data, "wmo.MPQ", &entries);
    let error = prepare(&data, 0, || false).unwrap_err();
    assert!(matches!(error.category(), Error::Doodad(_)));
    let diagnostic = format!(
        "{error:?} {error} {}",
        serde_json::to_string(&error.diagnostic()).unwrap()
    );
    assert!(!diagnostic.contains("PRIVATE_MALFORMED_ASSET"));
    assert!(!diagnostic.contains("Temple"));
    assert!(!diagnostic.contains(&data.display().to_string()));
}

#[test]
fn capture_preserves_profile_order_before_set_and_late_raw_model_filters() {
    for late_empty in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let (paths, offsets) = dc_names(&["Models\\Solid.mdx", "Models\\Empty.mdx"]);
        let mut spawns = vec![offsets[0]; 17];
        spawns[5] = offsets[1];
        spawns[8] = offsets[1];
        let mut entries = wmo_entries(2);
        let mut root = if late_empty {
            // The first MODN successfully extracts both offsets. The final
            // MODN replaces Paths, but leaves both valid-name offsets present.
            let (early, early_offsets) = dc_names(&["Models\\Solid.mdx", "Models\\Solid.mdx"]);
            assert_eq!(early_offsets, offsets);
            dc_root(2, &early, &[(2, 14)], &spawns)
        } else {
            dc_root(2, &paths, &[(2, 14)], &spawns)
        };
        if late_empty {
            root.extend(chunk(b"MODN", &paths));
        }
        dc_replace(&mut entries, ROOT, root);
        dc_replace(
            &mut entries,
            "World\\Wmo\\Temple_000.wmo",
            dc_group(false, true, &(0..10).collect::<Vec<_>>()),
        );
        dc_replace(
            &mut entries,
            "World\\Wmo\\Temple_001.wmo",
            dc_group(false, false, &(8..17).collect::<Vec<_>>()),
        );
        entries.push(raw_entry(SOLID, m2(true)));
        entries.push(raw_entry(EMPTY, m2(false)));
        entries.push(raw_entry(ADT, dc_adt(0, 0, 88)));
        save(&data, "wmo.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();
        // Literal GCC14 probe expectations: early extraction failure omits
        // keys 5/8 before insertion; a late raw-file exclusion omits them only
        // after reference rehash and enumeration. MODS excludes 0/1/16 later.
        let order: Vec<_> = prepared
            .doodad_candidates()
            .iter()
            .map(|x| x.spawn_index)
            .collect();
        let expected = if late_empty {
            vec![15, 14, 13, 2, 3, 4, 6, 7, 9, 10, 11, 12]
        } else {
            vec![15, 13, 14, 2, 3, 4, 6, 7, 9, 10, 11, 12]
        };
        assert_eq!(order, expected, "late_empty={late_empty}");
        assert_eq!(
            prepared.summary().doodad_reference_order_policy,
            "gcc14-libstdcxx-u16-identity-prime-load1-fresh-insert-v1"
        );
        prepared.verify_unchanged(|| false).unwrap();
    }
}

#[test]
fn cancellation_during_root_reference_preparation_never_returns_partial_candidates() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(1, b"Models\\Solid.mdx\0", &[(0, 30)], &[0; 30]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &(0..30).collect::<Vec<_>>()),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    entries.push(raw_entry(ADT, dc_adt(0, 0, 88)));
    save(&data, "wmo.MPQ", &entries);
    // Sweep early preparation polling boundaries, including the bounded link
    // initialization, group insertions and rehash. Every partial state is lost.
    let mut root_cancellations = 0;
    let mut reached_map_metadata = false;
    for cutoff in 0..2048 {
        let polls = std::cell::Cell::new(0usize);
        let error = prepare(&data, 0, || {
            let n = polls.get();
            polls.set(n + 1);
            n >= cutoff
        })
        .unwrap_err();
        assert!(matches!(
            error.category(),
            Error::Cancelled | Error::Source(SourceError::Cancelled)
        ));
        match error {
            Error::Preparation {
                stage: PreparationStage::GlobalWmoRoot,
                ..
            } => root_cancellations += 1,
            Error::Preparation {
                stage: PreparationStage::MapMetadata,
                ..
            } => {
                reached_map_metadata = true;
                break;
            }
            _ => {}
        }
    }
    assert!(reached_map_metadata, "sweep must cover the full root phase");
    assert!(
        root_cancellations >= 100,
        "reference work must be included in cancellation sweep"
    );
    // Cancelled attempts cannot poison a later independent capture.
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.doodad_candidates().len(), 30);
    prepared.verify_unchanged(|| false).unwrap();
}

#[test]
fn encounter_ids_replay_prior_physical_maps_and_reuse_duplicate_parent_children() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(&mut entries, MAP, maps(&[(90, "Earlier"), (0, "Selected")]));
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(1, b"Models\\Solid.mdx\0", &[(0, 2)], &[0, 0]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &[0, 1]),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    entries.push(raw_entry(
        "World\\Maps\\Earlier\\Earlier.wdt",
        chunk(b"MVER", &18u32.to_le_bytes()),
    ));
    let mut earlier = dc_adt(0, 0, 42);
    earlier.extend_from_slice(&adt(&["Models\\Solid.mdx"], &[(0, 9)])[12..]);
    entries.push(raw_entry("World\\Maps\\Earlier\\Earlier_0_0.adt", earlier));
    let mut selected = dc_adt(0, 0, 42);
    selected.extend_from_slice(&adt(&["Models\\Solid.mdx"], &[(0, 99)])[12..]);
    entries.push(raw_entry(ADT, selected));
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_1_0.adt",
        dc_adt(1, 0, 42),
    ));
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    let ids: Vec<_> = prepared
        .placements()
        .iter()
        .map(Placement::generated_id)
        .collect();
    assert_eq!(ids, [Some(1), Some(5), None]);
    let children: Vec<_> = prepared
        .doodad_candidates()
        .iter()
        .map(|v| {
            (
                v.wmo_placement_index,
                v.spawn_index,
                v.child_ordinal,
                v.generated_id,
            )
        })
        .collect();
    assert_eq!(
        children,
        [(0, 1, 1, 2), (0, 0, 2, 3), (2, 1, 1, 2), (2, 0, 2, 3)]
    );
    let profile =
        DoodadProjectionProfile::from_policy_id(crate::doodad_transform_math::POLICY).unwrap();
    let records = prepared.placement_records(profile, || false).unwrap();
    let mut cursor = records.bytes();
    let mut serialized = Vec::new();
    while !cursor.is_empty() {
        let word = |offset| u32::from_le_bytes(cursor[offset..offset + 4].try_into().unwrap());
        let flags = word(12);
        serialized.push((word(4), word(8), word(18)));
        let name_offset = 50 + if flags & 4 != 0 { 24 } else { 0 };
        let length = name_offset + 4 + word(name_offset) as usize;
        cursor = &cursor[length..];
    }
    assert_eq!(
        serialized,
        [
            (0, 0, 1),
            (0, 0, 2),
            (0, 0, 3),
            (0, 0, 5),
            (1, 0, 2),
            (1, 0, 3)
        ]
    );
    assert_eq!(records.record_count(), 6);
    let index = records.map_spawn_index(|| false).unwrap();
    assert_eq!(index.unique_count(), 4);
    assert_eq!(index.tile_reference_count(), 6);
    assert_eq!(
        index.unique_records().map(|(id, _)| id).collect::<Vec<_>>(),
        [1, 2, 3, 5]
    );
    assert_eq!(
        index.tile_references().collect::<Vec<_>>(),
        [(0, 1), (0, 2), (0, 3), (0, 5), (65536, 2), (65536, 3)]
    );
    // The tile-1 duplicates retain references to the tile-0 first-wins record.
    for (_, record) in index.unique_records() {
        assert_eq!(&record[4..12], &[0; 8]);
    }
    assert_eq!(index.source_binding(), records.preprocessing_binding());
    let polls = std::cell::Cell::new(0usize);
    records
        .map_spawn_index(|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
    for cutoff in 0..polls.get() {
        let n = std::cell::Cell::new(0);
        let error = records
            .map_spawn_index(|| {
                let current = n.get();
                n.set(current + 1);
                current >= cutoff
            })
            .unwrap_err();
        assert_eq!(error, MapSpawnIndexError::Cancelled);
    }
    assert_eq!(records.map_spawn_index(|| false).unwrap().unique_count(), 4);

    let assembled = prepared.assembled_spawn_bounds(profile, || false).unwrap();
    assert_eq!(
        assembled
            .bounds()
            .iter()
            .map(|s| s.generated_id)
            .collect::<Vec<_>>(),
        [1, 2, 3, 5]
    );
    assert!(assembled.bounds().iter().all(|s| s.flags & 4 != 0));
    assert!(assembled.vertex_visits() > 0);
    let map_tree = assembled.map_bih(|| false).unwrap();
    let mut primitive_slots = map_tree.objects().to_vec();
    primitive_slots.sort_unstable();
    assert_eq!(primitive_slots, [0, 1, 2, 3]);
    let ordered_boxes: Vec<_> = assembled
        .bounds()
        .iter()
        .map(|spawn| spawn.bounds)
        .collect();
    let expected_tree = benilla_native_vmap::SourceBih::build(&ordered_boxes, 3, || false).unwrap();
    assert_eq!(
        map_tree.serialize(|| false).unwrap(),
        expected_tree.serialize(|| false).unwrap()
    );
    assert_eq!(
        assembled.map_bih(|| true).unwrap_err(),
        benilla_native_vmap::BihBuildError::Cancelled
    );
    let map_products = prepared.map_products(profile, || false).unwrap().unwrap();
    assert_eq!(map_products.map_id(), prepared.map_id());
    assert!(map_products.tree_bytes().starts_with(b"VMAP_7.0\x01NODE"));
    assert_eq!(map_products.spawn_model_names().count(), 4);
    assert_eq!(map_products.tiles().len(), 2);
    let bundle = prepared
        .assemble_map_bundle(profile, || false)
        .unwrap()
        .unwrap();
    let expected_names: std::collections::BTreeSet<_> = map_products.spawn_model_names().collect();
    assert_eq!(
        bundle
            .models()
            .iter()
            .map(|m| m.source_name())
            .collect::<Vec<_>>(),
        expected_names.into_iter().collect::<Vec<_>>()
    );
    assert_eq!(bundle.map().tree_bytes(), map_products.tree_bytes());
    for model in bundle.models() {
        let identity = model.source_identity().unwrap();
        assert_eq!(identity.as_bytes(), model.source_name());
        assert_eq!(
            identity.effective_source_basename().unwrap(),
            model.effective_source_basename()
        );
        assert!(identity.safe_basename().unwrap().starts_with("source-"));
        let prefix = model
            .source_name()
            .strip_suffix(&[0])
            .unwrap_or(model.source_name());
        let raw = prepared
            .products
            .iter()
            .find(|p| p.extracted_name.as_bytes() == prefix)
            .unwrap()
            .raw_vmap()
            .unwrap();
        assert_eq!(
            model.bytes(),
            benilla_native_vmap::assemble_raw_model(raw, || false).unwrap()
        );
    }
    let total_model_bytes = bundle.models().iter().map(|m| m.bytes().len()).sum();
    prepared
        .assemble_map_bundle_with_limits(profile, || false, total_model_bytes, usize::MAX)
        .unwrap()
        .unwrap();
    for (bytes, polls) in [(total_model_bytes - 1, usize::MAX), (usize::MAX, 0)] {
        assert!(matches!(
            prepared.assemble_map_bundle_with_limits(profile, || false, bytes, polls),
            Err(crate::MapBundleError::Limit)
        ));
    }
    bundle.verify_unchanged(|| false).unwrap();
    assert!(matches!(
        bundle.verify_unchanged(|| true),
        Err(crate::MapBundleError::Source(SourceError::Cancelled)
            | crate::MapBundleError::Cancelled)
    ));

    assert_eq!(assembled.records().bytes(), records.bytes());
    let polls = std::cell::Cell::new(0usize);
    prepared
        .assembled_spawn_bounds(profile, || {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
    // Prefix-map source rechecks include thousands of absent-tile lookups.
    // Sample across the full operation, including every last callback, instead
    // of quadratic rechecking at every internal archive polling point.
    let mut cutoffs: Vec<_> = (0..32.min(polls.get())).collect();
    cutoffs.extend((0..32).map(|i| polls.get() * i / 32));
    cutoffs.extend(polls.get().saturating_sub(16)..polls.get());
    cutoffs.sort_unstable();
    cutoffs.dedup();
    for cutoff in cutoffs {
        let n = std::cell::Cell::new(0);
        let error = prepared
            .assembled_spawn_bounds(profile, || {
                let current = n.get();
                n.set(current + 1);
                current >= cutoff
            })
            .unwrap_err();
        assert!(
            matches!(
                error,
                SpawnBoundsError::Cancelled
                    | SpawnBoundsError::Preparation(
                        PlacementRecordError::Cancelled
                            | PlacementRecordError::Source(SourceError::Cancelled)
                    )
                    | SpawnBoundsError::Index(MapSpawnIndexError::Cancelled)
                    | SpawnBoundsError::Source(SourceError::Cancelled)
            ),
            "cutoff={cutoff}: {error:?}"
        );
    }

    assert_eq!(prepared.summary().encounter_id_count, 5);
    assert_eq!(prepared.summary().encounter_attempts, 10);
    assert_eq!(prepared.summary().prefix_maps, 1);
    prepared.verify_unchanged(|| false).unwrap();
}

#[test]
fn filtered_prefix_replay_is_bounded_before_parent_and_child_eligibility() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    dc_replace(&mut entries, MAP, maps(&[(90, "Earlier"), (0, "Selected")]));
    dc_replace(
        &mut entries,
        ROOT,
        dc_root(1, b"Models\\Solid.mdx\0", &[(0, 0)], &[0; 30]),
    );
    dc_replace(
        &mut entries,
        "World\\Wmo\\Temple_000.wmo",
        dc_group(false, true, &(0..30).collect::<Vec<_>>()),
    );
    entries.push(raw_entry(SOLID, m2(true)));
    entries.push(raw_entry(
        "World\\Maps\\Earlier\\Earlier.wdt",
        chunk(b"MVER", &18u32.to_le_bytes()),
    ));
    entries.push(raw_entry(
        "World\\Maps\\Earlier\\Earlier_0_0.adt",
        dc_adt(1, 0, 42),
    ));
    save(&data, "wmo.MPQ", &entries);
    let mut builder = Builder::new(CapturedSource::open(&data, &|| false).unwrap(), 0, 0);
    builder.summary.replay_steps = MAX_REPLAY_STEPS - 2;
    let error = prepare_inner(&mut builder, 0, &|| false).unwrap_err();
    assert!(matches!(error, Error::ReplayStepLimit));
    assert_eq!(builder.stage, PreparationStage::PrefixMap);
    assert_eq!(builder.summary.replay_steps, MAX_REPLAY_STEPS);
    assert_eq!(builder.summary.encounter_attempts, 0);
    assert_eq!(builder.encounter_ids.len(), 0);
    assert!(builder.placements.is_empty());
    assert!(builder.doodad_candidates.is_empty());

    // A fresh preparation is unaffected; the destructible parent and all 30
    // excluded references consume steps even though no encounter IDs emit.
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().replay_steps, 31);
    assert_eq!(prepared.summary().encounter_attempts, 0);

    let mut builder = Builder::new(prepared.source, 0, 0);
    builder.products = prepared.products;
    let product = builder
        .products
        .iter()
        .position(|p| p.kind == ModelKind::Wmo)
        .unwrap();
    let polls = std::cell::Cell::new(0usize);
    let error = builder
        .select_doodads(None, product, 0, 42, &|| {
            let n = polls.get();
            polls.set(n + 1);
            n == 2
        })
        .unwrap_err();
    assert!(matches!(error, Error::Cancelled));
    assert_eq!(builder.summary.replay_steps, 2);
    assert_eq!(builder.encounter_ids.len(), 0);
    assert!(builder.doodad_candidates.is_empty());
}
