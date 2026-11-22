// SPDX-License-Identifier: GPL-3.0-or-later
//! Entirely AUTHORED complete WDBC bytes; no installed client-store evidence.
use super::*;
fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        rows.len() as u32,
        N as u32,
        (N * 4) as u32,
        strings.len() as u32,
    ] {
        bytes.extend(value.to_le_bytes());
    }
    for row in rows {
        for value in row {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes.extend(strings);
    bytes
}
fn area(id: u32, map: u32, zone: u32, flag: u32) -> [u32; 25] {
    let mut row = [0; 25];
    row[0] = id;
    row[1] = map;
    row[2] = zone;
    row[3] = flag;
    row
}
fn map(id: u32, linked: u32) -> [u32; 42] {
    let mut row = [0; 42];
    row[0] = id;
    row[19] = linked;
    row
}
fn catalog(areas: &[[u32; 25]], maps: &[[u32; 42]]) -> SourceLocationCatalog {
    SourceLocationCatalog::decode(
        &dbc(areas, b"\0"),
        &dbc::<20>(&[], b"\0"),
        &dbc(maps, b"\0"),
    )
    .unwrap()
}
#[test]
fn indexed_lookup_uses_first_same_map_else_last_wrong_map_before_linked_zone() {
    let c = catalog(
        &[
            area(50, 1, 0, 7),
            area(9, 0, 12, 7),
            area(3, 0, 12, 7),
            area(2, 1, 0, 7),
            area(60, 2, 0, 7),
            area(70, 0, 0, 8),
        ],
        &[map(0, 70), map(3, 70)],
    );
    assert_eq!(c.resolve(0, 7).unwrap().id, 3);
    assert_eq!(c.resolve(1, 7).unwrap().id, 2);
    assert_eq!(c.resolve(3, 7).unwrap().id, 60);
    assert_eq!(c.resolve(3, 99).unwrap().id, 70);
    assert_eq!(c.resolve(999, 7).unwrap().id, 60);
    assert_eq!(c.resolve(999, 99), None);
}
#[test]
fn flag_zero_never_scans_flags_and_linked_zero_preserves_real_row_zero() {
    let c = catalog(
        &[area(0, 99, 12, 88), area(9, 0, 12, 0)],
        &[map(0, 0), map(1, 9), map(2, 123)],
    );
    assert_eq!(c.resolve(0, 0).unwrap().id, 0);
    assert_eq!(c.resolve(0, 0).unwrap().map_id, 99);
    assert_eq!(c.resolve(1, 0).unwrap().id, 9);
    assert_eq!(c.resolve(2, 0), None);
    assert_eq!(c.resolve(3, 0), None);
    let c = catalog(&[area(9, 0, 12, 0x1_0007)], &[map(0, 123)]);
    assert_eq!(c.resolve(0, 7), None); // Flag narrows, DBC exploreFlag does not.
}
#[test]
fn all_bytes_are_retained_and_bind_each_store_including_unused_fields_and_strings() {
    let original = [
        dbc(&[area(9, 0, 12, 7)], b"\0unused\0"),
        dbc(&[[0; 20]], b"\0wmo\0"),
        dbc(&[map(0, 9)], b"\0map\0"),
    ];
    let c = SourceLocationCatalog::decode(&original[0], &original[1], &original[2]).unwrap();
    assert_eq!(c.source_bytes(), original.each_ref().map(Vec::as_slice));
    for (store, offset) in [
        (0, 20 + 4 * 5),
        (1, 20 + 4 * 5),
        (2, 20 + 4 * 16),
        (0, 20 + 100 + 1),
        (1, 20 + 80 + 1),
        (2, 20 + 168 + 1),
    ] {
        let mut changed = original.clone();
        changed[store][offset] ^= 1;
        let altered = SourceLocationCatalog::decode(&changed[0], &changed[1], &changed[2]).unwrap();
        assert_ne!(c.digest(), altered.digest());
        assert_ne!(c, altered);
    }
}
#[test]
fn exact_layout_framing_duplicates_and_source_index_bounds_are_enforced() {
    let original = [
        dbc(&[area(9, 0, 12, 7)], b"\0"),
        dbc::<20>(&[], b"\0"),
        dbc(&[map(0, 9)], b"\0"),
    ];
    for store in 0..3 {
        for end in 0..original[store].len() {
            let mut bad = original.clone();
            bad[store].truncate(end);
            assert!(
                SourceLocationCatalog::decode(&bad[0], &bad[1], &bad[2]).is_err(),
                "store{store} prefix{end}"
            );
        }
        let mut trailing = original.clone();
        trailing[store].push(0);
        assert!(SourceLocationCatalog::decode(&trailing[0], &trailing[1], &trailing[2]).is_err());
        let mut wide = original.clone();
        wide[store][4..8].copy_from_slice(&65_537u32.to_le_bytes());
        assert_eq!(
            SourceLocationCatalog::decode(&wide[0], &wide[1], &wide[2]),
            Err(NativeError::TooLarge)
        );
    }
    for (a, w, m) in [
        (
            dbc(&[area(9, 0, 0, 0), area(9, 1, 0, 1)], b"\0"),
            original[1].clone(),
            original[2].clone(),
        ),
        (
            original[0].clone(),
            dbc(&[[0; 20], [0; 20]], b"\0"),
            original[2].clone(),
        ),
        (
            original[0].clone(),
            original[1].clone(),
            dbc(&[map(0, 9), map(0, 12)], b"\0"),
        ),
    ] {
        assert!(SourceLocationCatalog::decode(&a, &w, &m).is_err());
    }
    for id in [u32::MAX - 1, u32::MAX] {
        assert!(SourceLocationCatalog::decode(
            &dbc(&[area(id, 0, 0, 0)], b"\0"),
            &original[1],
            &original[2]
        )
        .is_err());
    }
    let mut wmo = [0; 20];
    wmo[0] = u32::MAX - 1;
    assert!(SourceLocationCatalog::decode(
        &original[0],
        &dbc(&[wmo], b"\0"),
        &dbc(&[map(u32::MAX - 1, 9)], b"\0")
    )
    .is_ok());
    wmo[0] = u32::MAX;
    assert!(
        SourceLocationCatalog::decode(&original[0], &dbc(&[wmo], b"\0"), &original[2]).is_err()
    );
    assert!(SourceLocationCatalog::decode(
        &original[0],
        &original[1],
        &dbc(&[map(u32::MAX, 9)], b"\0")
    )
    .is_err());
    let mut invalid_locale = area(9, 0, 12, 7);
    invalid_locale[18] = 1;
    assert!(SourceLocationCatalog::decode(
        &dbc(&[invalid_locale], b"\0"),
        &original[1],
        &original[2]
    )
    .is_err());
}
