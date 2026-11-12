use super::*;
use crate::identity::{DependencyRole as Role, DependencyState, IdentityError, ProducerIdentity};

fn pinned(data: &Path, root: &Path, map: u32) -> PreparedDiscovery {
    let output = root.join("empty-output");
    fs::create_dir(&output).unwrap();
    prepare_pinned(data, map, &output, || false).unwrap()
}

#[test]
fn pinned_m2_zero_hull_basename_alias_uses_current_file_at_placement() {
    // gameobject_extract.cpp:26-39; model.cpp:49-73,169-184.
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(
        ADT,
        adt(&["A\\Same.mdx", "B\\Same.mdx"], &[(0, 10), (1, 11)]),
    ));
    entries.push(raw_entry("A\\Same.m2", m2(false)));
    entries.push(raw_entry("B\\Same.m2", m2(true)));
    save(&data, "model.MPQ", &entries);
    let prepared = pinned(&data, &temp.0, 0);
    assert_eq!(prepared.products().len(), 2);
    assert!(prepared.products()[0].raw_vmap().is_none());
    assert_eq!(prepared.placements()[0].product_index(), 1);
    assert_eq!(prepared.placements()[1].product_index(), 1);
    assert!(prepared
        .placements()
        .iter()
        .all(|p| p.disposition() == PlacementDisposition::Emits));
    assert_eq!(prepared.summary().encounter_id_count, 2);
}

#[test]
fn pinned_existing_m2_file_blocks_later_zero_hull_lookup() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(
        ADT,
        adt(&["A\\Same.mdx", "B\\Same.mdx"], &[(1, 1)]),
    ));
    entries.push(raw_entry("A\\Same.m2", m2(true)));
    // No B member: FileExists returns before the archive lookup.
    save(&data, "model.MPQ", &entries);
    let prepared = pinned(&data, &temp.0, 0);
    assert_eq!(prepared.products().len(), 1);
    assert!(!prepared.source_receipts().any(|r| r.name() == "B\\Same.m2"));
    assert_eq!(
        prepared.placements()[0].disposition(),
        PlacementDisposition::Emits
    );
}

#[test]
fn pinned_same_basename_wmo_roots_keep_first_list_occurrence() {
    // vmapexport.cpp:160-167; ADT MWMO names never select another archive root.
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = wmo_entries(1);
    entries.retain(|entry| entry.name != "(listfile)");
    entries.push(raw_entry(
        "(listfile)",
        b"World\\Wmo\\Temple.wmo\r\nOther\\Temple.wmo\r\n\0".to_vec(),
    ));
    entries.push(raw_entry(ADT, wmo_adt("Other\\Temple.wmo", 0, 42)));
    save(&data, "wmo.MPQ", &entries);
    let prepared = pinned(&data, &temp.0, 0);
    assert_eq!(prepared.summary().global_root_events, 2);
    assert_eq!(prepared.products().len(), 1);
    assert_eq!(
        prepared.products()[0].source_member(),
        "World\\Wmo\\Temple.wmo"
    );
    assert!(!prepared
        .source_receipts()
        .any(|r| r.name() == "Other\\Temple.wmo"));
    assert_eq!(
        prepared.placements()[0].disposition(),
        PlacementDisposition::Emits
    );
    let occurrences: Vec<_> = prepared.source_occurrences().collect();
    assert_eq!(occurrences[1].ordinal, 1);
    assert_eq!(occurrences[1].listfile_archive, Some(0));
    assert_eq!(occurrences[1].receipt.name(), "(listfile)");
    assert!(occurrences[1].encoded_identity.is_some());
}

#[test]
fn pinned_output_requires_observed_empty_directory_not_leftovers_or_missing() {
    let temp = Temp::new();
    let data = temp.data();
    save(&data, "terrain.MPQ", &prerequisites());
    let output = temp.0.join("output");
    assert_eq!(
        prepare_pinned(&data, 0, &output, || false)
            .unwrap_err()
            .diagnostic()
            .category,
        "vmap.output_root_unavailable"
    );
    fs::create_dir(&output).unwrap();
    fs::write(output.join("Same.m2"), b"leftover").unwrap();
    assert_eq!(
        prepare_pinned(&data, 0, &output, || false)
            .unwrap_err()
            .diagnostic()
            .category,
        "vmap.output_root_not_empty"
    );
    fs::remove_file(output.join("Same.m2")).unwrap();
    let prepared = prepare_pinned(&data, 0, &output, || false).unwrap();
    assert_eq!(
        prepared.producer_policy(),
        ProducerPolicy::PinnedBasenameFirstWins
    );
    assert_eq!(
        prepare(&data, 0, || false).unwrap().producer_policy(),
        ProducerPolicy::CanonicalComponent
    );
}

#[test]
fn pinned_doodad_terminal_nul_and_c_string_output_remain_distinct() {
    // model.cpp:243-253 retains original nlen, TileAssembler.cpp:313-325 uses %s.
    for raw in [b"Models\\thing.mdx".as_slice(), b"Models\\thing.mdl"] {
        let name = crate::producer_policy::doodad_instance_name(raw).unwrap();
        assert_eq!(name, b"Thing.m2\0");
        let identity = benilla_native_vmap::SourceModelName::new(&name).unwrap();
        assert_eq!(identity.effective_source_basename().unwrap(), b"Thing.m2");
        let regular = benilla_native_vmap::SourceModelName::new(b"Thing.m2").unwrap();
        assert_eq!(
            regular.effective_source_basename().unwrap(),
            b"Thing.m2.vmo"
        );
        assert_ne!(identity, regular);
    }
}

#[test]
fn pinned_case_and_percent_encoded_names_are_not_canonical_aliases() {
    let policy = ProducerPolicy::PinnedBasenameFirstWins;
    let dependency = |name: &str| ModelDependency {
        kind: ModelKind::M2,
        member: format!("Models\\{name}"),
        source_member: format!("Models\\{name}"),
        extracted_name: name.to_owned(),
    };
    assert_ne!(
        policy.key(&dependency("Same.m2")),
        policy.key(&dependency("same.m2"))
    );
    assert_ne!(
        policy.key(&dependency("a%5cb.m2")),
        policy.key(&dependency("b.m2"))
    );
    assert_eq!(policy.key(&dependency("a%5cb.m2")), "a%5cb.m2");
    let canonical = ProducerPolicy::CanonicalComponent;
    assert_eq!(
        canonical.key(&dependency("Same.m2")),
        canonical.key(&dependency("same.m2"))
    );
}

#[test]
fn pinned_listfile_requires_captured_nul_even_after_final_lf() {
    // mpq_libmpq04.h:51-67 calls strtok(NULL) unconditionally after every token.
    for bytes in [b"A.wmo\r\n".as_slice(), b"\n", b"A.wmo\r"] {
        assert_eq!(
            discovery::parse_pinned_wmo_listfile(bytes)
                .err()
                .unwrap()
                .code(),
            "vmap.listfile_undefined_suffix_read"
        );
    }
    assert_eq!(
        discovery::parse_pinned_wmo_listfile(b"A.wmo\r\n\0ignored.wmo\r\n")
            .unwrap()
            .len(),
        1
    );
    assert!(discovery::parse_wmo_listfile(b"A.wmo\r\n").is_ok());
}

#[test]
fn pinned_prefix_map_seeds_basename_winner_and_generated_ids() {
    // vmapexport.cpp:266-295 visits physical Map.dbc rows, not sorted IDs.
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries[1] = raw_entry(MAP, maps(&[(7, "Prefix"), (0, "Selected")]));
    entries.push(raw_entry(
        "World\\Maps\\Prefix\\Prefix.wdt",
        chunk(b"MVER", &18u32.to_le_bytes()),
    ));
    entries.push(raw_entry(
        "World\\Maps\\Prefix\\Prefix_0_0.adt",
        adt(&["First\\Same.mdx"], &[(0, 123)]),
    ));
    entries.push(raw_entry("First\\Same.m2", m2(true)));
    entries.push(raw_entry(ADT, adt(&["Other\\Same.mdx"], &[(0, 456)])));
    save(&data, "model.MPQ", &entries);
    let prepared = pinned(&data, &temp.0, 0);
    assert_eq!(prepared.summary().prefix_maps, 1);
    assert_eq!(prepared.summary().prefix_adt_attempts, 4096);
    assert_eq!(prepared.placements()[0].generated_id(), Some(2));
    assert_eq!(prepared.products()[0].source_member(), "First\\Same.m2");
    assert!(!prepared
        .source_receipts()
        .any(|r| r.name() == "Other\\Same.m2"));
}

#[test]
fn identity_known_absent_is_not_uncaptured_and_later_presence_is_changed() {
    let temp = Temp::new();
    let missing = temp.0.join("missing");
    let mut identity = ProducerIdentity::new();
    assert_eq!(identity.occurrence(0), DependencyState::Uncaptured);
    assert_eq!(
        identity
            .capture(Role::Member, b"missing\0", &missing)
            .unwrap(),
        DependencyState::Absent
    );
    assert_eq!(identity.occurrence(0), DependencyState::Absent);
    identity.verify_unchanged().unwrap();
    fs::write(missing, b"new winner").unwrap();
    assert!(matches!(
        identity.verify_unchanged(),
        Err(IdentityError::Changed)
    ));
}

#[test]
fn identity_transport_model_is_a_reached_dependency_not_continent_closure() {
    // contrib/mmap/src/MapBuilder.cpp:383-475 builds goNNNN.mmtile separately.
    let temp = Temp::new();
    let path = temp.0.join("go0360.mmtile");
    let mut identity = ProducerIdentity::new();
    identity
        .capture(Role::TransportModel, b"go0360.mmtile", &path)
        .unwrap();
    assert!(matches!(
        identity.require_present(&[Role::TransportModel]),
        Err(IdentityError::Missing(Role::TransportModel))
    ));
    fs::write(&path, b"authored standalone mesh").unwrap();
    let mut captured = ProducerIdentity::new();
    captured
        .capture(Role::TransportModel, b"go0360.mmtile", &path)
        .unwrap();
    captured.require_present(&[Role::TransportModel]).unwrap();
    assert_ne!(captured.digest(), identity.digest());
}

#[test]
fn identity_config_offmesh_order_raw_nul_and_winner_replacement_are_bound() {
    let temp = Temp::new();
    let file = temp.0.join("bytes");
    fs::write(&file, b"config and offmesh bytes").unwrap();
    let mut first = ProducerIdentity::new();
    first.capture(Role::Config, b"config", &file).unwrap();
    first.capture(Role::Offmesh, b"offmesh\0", &file).unwrap();
    let mut reversed = ProducerIdentity::new();
    reversed
        .capture(Role::Offmesh, b"offmesh\0", &file)
        .unwrap();
    reversed.capture(Role::Config, b"config", &file).unwrap();
    assert_ne!(first.digest(), reversed.digest());
    fs::write(&file, b"replacement winner").unwrap();
    assert!(matches!(
        first.verify_unchanged(),
        Err(IdentityError::Changed)
    ));
}

#[test]
fn pinned_winner_replacement_after_capture_refuses() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = prerequisites();
    entries.push(raw_entry(ADT, adt(&["Models\\Same.mdx"], &[(0, 1)])));
    entries.push(raw_entry("Models\\Same.m2", m2(true)));
    save(&data, "model.MPQ", &entries);
    let prepared = pinned(&data, &temp.0, 0);
    save(
        &data,
        "patch.MPQ",
        &[raw_entry("Models\\Same.m2", m2(false))],
    );
    assert!(matches!(
        prepared.verify_unchanged(|| false),
        Err(Error::Source(SourceError::Changed))
    ));
}
