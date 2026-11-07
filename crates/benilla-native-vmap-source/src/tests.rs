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
fn unavailable_global_roots_keep_receipts_without_emitting_or_caching_empty_products() {
    const ROOT: &str = "World\\Wmo\\Temple.wmo";
    for placeholder in [None, Some(vec![]), Some(vec![0])] {
        let temp = Temp::new();
        let data = temp.data();
        let mut base = wmo_entries(1);
        base.retain(|entry| {
            entry.name != "(listfile)" && (placeholder.is_some() || entry.name != ROOT)
        });
        save(&data, "wmo.MPQ", &base);
        let mut patch = vec![raw_entry(
            "(listfile)",
            b"World\\Wmo\\Temple.wmo\r\nWorld\\Wmo\\Temple.wmo\r\n\0".to_vec(),
        )];
        if let Some(bytes) = &placeholder {
            patch.push(raw_entry(ROOT, bytes.clone()));
        }
        save(&data, "patch.MPQ", &patch);
        let prepared = prepare(&data, 0, || false).unwrap();
        assert_eq!(prepared.summary().global_root_events, 2);
        assert_eq!(prepared.summary().wmo_raw_products, 0);
        assert!(prepared.products().is_empty());
        assert!(prepared.placements().is_empty());
        assert!(prepared.doodad_candidates().is_empty());
        let roots: Vec<_> = prepared
            .source_receipts()
            .filter(|receipt| receipt.name() == ROOT)
            .collect();
        assert_eq!(roots.len(), 2);
        for receipt in roots {
            match &placeholder {
                Some(bytes) => assert!(
                    matches!(receipt.outcome(), SourceOutcome::Dummy { winning_archive: "patch.MPQ", bytes: count } if count == bytes.len())
                ),
                None => assert!(matches!(receipt.outcome(), SourceOutcome::Absent)),
            }
        }
        assert!(!prepared
            .source_receipts()
            .any(|receipt| receipt.name().ends_with("_000.wmo")));
        prepared.verify_unchanged(|| false).unwrap();
        patch.retain(|entry| entry.name != ROOT);
        patch.push(raw_entry(ROOT, wmo_root(1)));
        save(&data, "patch.MPQ", &patch);
        assert!(matches!(
            prepared.verify_unchanged(|| false),
            Err(Error::Source(SourceError::Changed))
        ));
    }
}

#[test]
fn referenced_unavailable_global_root_still_refuses_instead_of_becoming_empty_collision() {
    const ROOT: &str = "World\\Wmo\\Temple.wmo";
    for placeholder in [None, Some(vec![]), Some(vec![0])] {
        let temp = Temp::new();
        let data = temp.data();
        let mut base = wmo_entries(1);
        if placeholder.is_none() {
            base.retain(|entry| entry.name != ROOT);
        }
        base.push(raw_entry(ADT, wmo_adt(ROOT, 0, 72)));
        save(&data, "wmo.MPQ", &base);
        if let Some(bytes) = placeholder {
            save(&data, "patch.MPQ", &[raw_entry(ROOT, bytes)]);
        }
        let error = prepare(&data, 0, || false).unwrap_err();
        assert_eq!(error.diagnostic().category, "vmap.unresolved_wmo_reference");
        assert_eq!(
            error.diagnostic().stage,
            Some(PreparationStage::AdtTraversal)
        );
    }
}

#[test]
fn dummy_required_wmo_groups_remain_errors_without_lower_archive_fallback() {
    for size in [0, 1] {
        let temp = Temp::new();
        let data = temp.data();
        save(&data, "wmo.MPQ", &wmo_entries(1));
        save(
            &data,
            "patch.MPQ",
            &[raw_entry("World\\Wmo\\Temple_000.wmo", vec![0; size])],
        );
        let error = prepare(&data, 0, || false).unwrap_err();
        assert_eq!(error.diagnostic().category, "vmap.required_member_dummy");
        assert_eq!(
            error.diagnostic().stage,
            Some(PreparationStage::GlobalWmoRoot)
        );
    }
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
fn distinct_zero_hull_m2_aliases_keep_both_products_receipts_and_empty_placements() {
    for (names, members) in [
        (
            ["Models\\Empty.mdx", "Other\\EMPTY.MDX"],
            ["Models\\Empty.m2", "Other\\Empty.m2"],
        ),
        (
            ["Models\\Space Name.mdx", "Other\\SPACE_NAME.MDX"],
            ["Models\\Space_Name.m2", "Other\\Space_Name.m2"],
        ),
    ] {
        for reverse in [false, true] {
            let temp = Temp::new();
            let data = temp.data();
            let mut names = names;
            let mut members = members;
            if reverse {
                names.reverse();
                members.reverse();
            }
            let mut entries = prerequisites();
            entries.push(raw_entry(
                ADT,
                adt(
                    &[names[0], names[1], names[0], names[1]],
                    &[(0, 1), (1, 2), (2, 3), (3, 4)],
                ),
            ));
            for member in &members {
                entries.push(raw_entry(member, m2(false)));
            }
            save(&data, "model.MPQ", &entries);
            let prepared = prepare(&data, 0, || false).unwrap();
            assert_eq!(prepared.products().len(), 2);
            assert_eq!(prepared.summary().m2_no_collision_products, 2);
            assert_eq!(prepared.summary().no_collision_placements, 4);
            for (index, member) in members.iter().enumerate() {
                let product = &prepared.products()[index];
                assert_eq!(product.source_member(), *member);
                assert!(!product.source_has_collision());
                assert!(product.raw_vmap().is_none());
                assert_eq!(
                    prepared
                        .source_receipts()
                        .filter(|r| r.name() == *member)
                        .count(),
                    2
                );
            }
            for (index, placement) in prepared.placements().iter().enumerate() {
                assert_eq!(placement.product_index(), index % 2);
                assert_eq!(placement.disposition(), PlacementDisposition::NoCollision);
            }
            prepared.verify_unchanged(|| false).unwrap();
            let mut changed = m2(false);
            changed[8] = 1;
            save(&data, "patch.MPQ", &[raw_entry(members[1], changed)]);
            assert!(prepared.verify_unchanged(|| false).is_err());
        }
    }
}

#[test]
fn same_basename_m2_members_keep_their_own_hulls_through_assembly() {
    for hulls in [[true, false], [false, true], [true, true], [false, false]] {
        for reverse in [false, true] {
            let temp = Temp::new();
            let data = temp.data();
            let mut names = ["Models\\Shared.mdx", "Other\\SHARED.MDX"];
            let mut expected = [
                ("Models\\Shared.m2", "models%5cshared.m2", m2(hulls[0])),
                ("Other\\Shared.m2", "other%5cshared.m2", m2(hulls[1])),
            ];
            if hulls[1] {
                set_word(&mut expected[1].2, 330, 2.0_f32.to_bits());
            }
            if reverse {
                names.reverse();
                expected.reverse();
            }
            let mut entries = prerequisites();
            entries.push(raw_entry(ADT, adt(&names, &[(0, 1), (1, 2)])));
            for (member, _, bytes) in &expected {
                entries.push(raw_entry(member, bytes.clone()));
            }
            save(&data, "model.MPQ", &entries);
            let prepared = prepare(&data, 0, || false).unwrap();
            assert_eq!(prepared.products().len(), 2);
            for (index, (member, name, bytes)) in expected.iter().enumerate() {
                let product = &prepared.products()[index];
                assert_eq!(product.source_member(), *member);
                assert_eq!(product.model_name(), *name);
                let raw = match extract_m2_collision(bytes).unwrap() {
                    M2Collision::NoCollision => None,
                    M2Collision::RawVmap(bytes) => Some(bytes),
                };
                assert_eq!(product.raw_vmap(), raw.as_deref());
                let placement = &prepared.placements()[index];
                assert_eq!(placement.product_index(), index);
                assert_eq!(
                    placement.disposition(),
                    if raw.is_some() {
                        PlacementDisposition::Emits
                    } else {
                        PlacementDisposition::NoCollision
                    }
                );
            }
            let profile =
                DoodadProjectionProfile::from_policy_id("g3d-portable-libm-0.2.16-euler1080-v2")
                    .unwrap();
            let count = hulls.into_iter().filter(|hull| *hull).count();
            assert_eq!(
                prepared
                    .placement_records(profile, || false)
                    .unwrap()
                    .record_count(),
                count
            );
            if count != 0 {
                let bundle = prepared
                    .assemble_map_bundle(profile, || false)
                    .unwrap()
                    .unwrap();
                assert_eq!(bundle.models().len(), count);
                for model in bundle.models() {
                    assert!(expected
                        .iter()
                        .any(|(_, name, _)| model.source_name() == name.as_bytes()));
                }
                if count == 2 {
                    assert_ne!(bundle.models()[0].bytes(), bundle.models()[1].bytes());
                }
            }
            prepared.verify_unchanged(|| false).unwrap();
        }
    }
}

#[test]
fn same_canonical_m2_path_with_changed_bytes_refuses() {
    for hull in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = prerequisites();
        entries.push(raw_entry(ADT, adt(&["Models\\Shared.mdx"], &[(0, 1)])));
        entries.push(raw_entry("Models\\Shared.m2", m2(hull)));
        save(&data, "model.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();
        let model = entries
            .iter_mut()
            .find(|entry| entry.name == "Models\\Shared.m2")
            .unwrap();
        model.bytes[8] = 1;
        save(&data, "model.MPQ", &entries);
        assert!(matches!(
            prepared.verify_unchanged(|| false),
            Err(Error::Source(SourceError::Changed))
        ));
    }
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
fn unplaced_same_basename_m2_members_with_equal_bytes_remain_distinct() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(ADT, adt(&["A\\Same.mdx", "B\\Same.mdx"], &[])));
    entries.push(raw_entry("A\\Same.m2", m2(true)));
    entries.push(raw_entry("B\\Same.m2", m2(true)));
    save(&data, "terrain.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.products().len(), 2);
    assert_eq!(
        prepared.products()[0].raw_vmap(),
        prepared.products()[1].raw_vmap()
    );
    assert_eq!(prepared.products()[0].model_name(), "a%5csame.m2");
    assert_eq!(prepared.products()[1].model_name(), "b%5csame.m2");
    prepared.verify_unchanged(|| false).unwrap();
}

#[test]
fn same_basename_wmo_roots_keep_their_own_geometry_through_assembly() {
    for reverse in [false, true] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = prerequisites();
        let roots = ["A\\Temple.wmo", "B\\Temple.wmo"];
        let list = if reverse {
            b"B\\Temple.wmo\r\nA\\Temple.wmo\r\n\0".to_vec()
        } else {
            b"A\\Temple.wmo\r\nB\\Temple.wmo\r\n\0".to_vec()
        };
        entries.push(raw_entry("(listfile)", list));
        let mut expected = Vec::new();
        for (i, name) in roots.iter().enumerate() {
            let mut root = wmo_root(1);
            set_word(&mut root, 52, 100 + i as u32);
            let mut group = wmo_group(false);
            let movt = group.windows(4).position(|b| b == b"TVOM").unwrap() + 8;
            group[movt + 12..movt + 16].copy_from_slice(&(1.0 + i as f32).to_le_bytes());
            group[movt + 28..movt + 32].copy_from_slice(&1.0f32.to_le_bytes());
            expected.push(
                extract_wmo_collision_from_source(
                    name,
                    &root,
                    &[WmoGroupInput {
                        group_index: 0,
                        bytes: &group,
                    }],
                )
                .unwrap()
                .raw_vmap,
            );
            entries.push(raw_entry(name, root));
            entries.push(raw_entry(
                &format!("{}_{:03}.wmo", &name[..name.len() - 4], 0),
                group,
            ));
            entries.push(raw_entry(
                &format!("World\\Maps\\Selected\\Selected_0_{i}.adt"),
                wmo_adt(name, 0, 10 + i as u32),
            ));
        }
        save(&data, "wmo.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();
        assert_eq!(prepared.products().len(), 2);
        assert_ne!(expected[0], expected[1]);
        for (i, placement) in prepared.placements().iter().enumerate() {
            let product = &prepared.products()[placement.product_index()];
            assert_eq!(product.source_member(), roots[i]);
            assert_eq!(product.extracted_name(), "Temple.wmo");
            assert_eq!(product.raw_vmap().unwrap(), expected[i]);
        }
        let profile =
            DoodadProjectionProfile::from_policy_id(crate::doodad_transform_math::POLICY).unwrap();
        let bundle = prepared
            .assemble_map_bundle(profile, || false)
            .unwrap()
            .unwrap();
        assert_eq!(bundle.models().len(), 2);
        for (i, model) in bundle.models().iter().enumerate() {
            assert_eq!(
                model.source_name(),
                [b"a%5ctemple.wmo".as_slice(), b"b%5ctemple.wmo"][i]
            );
            assert_eq!(
                model.bytes(),
                benilla_native_vmap::assemble_raw_model(&expected[i], || false).unwrap(),
            );
        }
    }
}

#[test]
fn same_canonical_wmo_path_with_changed_bytes_refuses() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    let root = entries
        .iter_mut()
        .find(|e| e.name == "World\\Wmo\\Temple.wmo")
        .unwrap();
    set_word(&mut root.bytes, 52, 42);
    save(&data, "wmo.MPQ", &entries);
    assert!(matches!(
        prepared.verify_unchanged(|| false),
        Err(Error::Source(SourceError::Changed))
    ));
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
fn wmo_reference_cannot_use_another_directorys_captured_basename() {
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
        Error::UnresolvedWmoReference
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
    let profile =
        DoodadProjectionProfile::from_policy_id("g3d-portable-libm-0.2.16-euler1080-v2").unwrap();
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
    expected.extend_from_slice(&17_u32.to_le_bytes());
    expected.extend_from_slice(b"models%5csolid.m2");
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
