// SPDX-License-Identifier: GPL-3.0-or-later
// Authored fixtures only: no install assets or production AreaTable rows.
use super::super::super::TileId;
use super::super::{GridLiquidStatus, LiquidStores, LiquidTypes, TerrainTile};
use super::*;

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}

fn replace(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn row(id: u32, zone: u32, liquid: u32) -> [u32; 25] {
    let mut fields = [0; 25];
    fields[0] = id;
    fields[1] = 999; // Deliberately unrelated map and exploreFlag, never lookup keys.
    fields[2] = zone;
    fields[3] = 888;
    fields[24] = liquid;
    fields
}

fn dbc(rows: &[[u32; 25]], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len() as u32, 25, 100, strings.len() as u32] {
        word(&mut bytes, value);
    }
    for row in rows {
        for value in row {
            word(&mut bytes, *value);
        }
    }
    bytes.extend(strings);
    bytes
}

fn table(rows: &[[u32; 25]]) -> AreaTable {
    AreaTable::decode(&dbc(rows, &[0])).unwrap()
}

fn types(rows: &[(u32, u32)]) -> LiquidTypes {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len() as u32, 4, 16, 1] {
        word(&mut bytes, value);
    }
    for &(id, kind) in rows {
        for value in [id, 123, kind, 456] {
            word(&mut bytes, value);
        }
    }
    bytes.push(0);
    LiquidTypes::decode(&bytes).unwrap()
}

fn area_bytes(global: u16, cells: Option<&[u16; 256]>) -> Vec<u8> {
    let mut bytes = b"AREA".to_vec();
    bytes.extend(u16::from(cells.is_none()).to_le_bytes());
    bytes.extend(global.to_le_bytes());
    if let Some(cells) = cells {
        for cell in cells {
            bytes.extend(cell.to_le_bytes());
        }
    }
    bytes
}

fn tile(area: &[u8], entry: u16, flags: u8) -> TerrainTile {
    let mut bytes = b"MAPSz1.4".to_vec();
    let height_at = 40 + area.len() as u32;
    let liquid_at = height_at + 16;
    for value in [
        40,
        area.len() as u32,
        height_at,
        16,
        liquid_at,
        16,
        liquid_at + 16,
        512,
    ] {
        word(&mut bytes, value);
    }
    bytes.extend(area);
    bytes.extend(b"MHGT");
    for value in [1, 7.0f32.to_bits(), 7.0f32.to_bits()] {
        word(&mut bytes, value);
    }
    bytes.extend(b"MLIQ");
    bytes.extend([3, flags]); // Global type and level.
    bytes.extend(entry.to_le_bytes());
    bytes.extend([0, 0, 129, 129]);
    word(&mut bytes, 10.0f32.to_bits());
    bytes.extend([0; 512]);
    TerrainTile::decode(TileId { x: 32, y: 32 }, &bytes).unwrap()
}

fn world(cell: f32) -> f32 {
    -cell * (GRID_SIZE / 128.0)
}

#[test]
fn complete_area_table_preserves_raw_ids_and_hashes_unused_fields_and_strings() {
    let mut named = row(70_001, u32::MAX, u32::MAX);
    named[11..=18].fill(2); // Interior bytes are valid strings, as are non-UTF8 bytes.
    let bytes = dbc(&[row(0, 0, 0), named], &[0, b'a', 0xff, 0]);
    let areas = AreaTable::decode(&bytes).unwrap();
    assert_eq!(areas.entries[&70_001].zone, u32::MAX);
    assert_eq!(areas.entries[&70_001].liquid_override, u32::MAX);
    assert_eq!(
        areas.entries[&0],
        AreaEntry {
            zone: 0,
            liquid_override: 0
        }
    );
    assert!(!areas.entries.contains_key(&888));
    assert_eq!(areas.digest(), Hash::from(Sha256::digest(&bytes)));
    for at in [24, 32, 36, 40, 20 + 10 * 4, 20 + 19 * 4, 20 + 21 * 4] {
        let mut changed = bytes.clone();
        replace(&mut changed, at, 123_456);
        assert_ne!(
            areas.digest(),
            AreaTable::decode(&changed).unwrap().digest()
        );
    }
    let mut changed = bytes.clone();
    *changed.iter_mut().rev().nth(1).unwrap() = 0xfe;
    assert_ne!(
        areas.digest(),
        AreaTable::decode(&changed).unwrap().digest()
    );
    assert!(table(&[]).entries.is_empty());
}

#[test]
fn complete_area_table_rejects_truncation_duplicates_offsets_and_size_budgets() {
    let bytes = dbc(&[row(401, 402, 21), row(402, 0, 22)], &[0, b'x', 0]);
    for end in 0..bytes.len() {
        assert!(AreaTable::decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(matches!(AreaTable::decode(&extra), Err(Error::Framing)));
    for (at, value, error) in [
        (4, 65_537, Error::Budget),
        (4, u32::MAX, Error::Budget),
        (8, 24, Error::Unsupported),
        (12, 96, Error::Unsupported),
        (16, 0, Error::Framing),
        (16, 1_048_577, Error::Budget),
        (16, u32::MAX, Error::Budget),
        (20, u32::MAX, Error::Unsupported),
    ] {
        let mut bad = bytes.clone();
        replace(&mut bad, at, value);
        assert!(
            matches!(AreaTable::decode(&bad), Err(e) if e == error),
            "at {at}"
        );
    }
    for field in 11..=18 {
        for offset in [3, u32::MAX] {
            let mut bad = bytes.clone();
            replace(&mut bad, 20 + field * 4, offset);
            assert!(matches!(AreaTable::decode(&bad), Err(Error::Framing)));
        }
    }
    for at in [220, bytes.len() - 1] {
        let mut bad = bytes.clone();
        bad[at] = b'x';
        assert!(matches!(AreaTable::decode(&bad), Err(Error::Framing)));
    }
    assert!(matches!(
        AreaTable::decode(&dbc(&[row(401, 0, 21), row(401, 402, 22)], &[0])),
        Err(Error::Duplicate)
    ));
    let mut bad_magic = bytes;
    bad_magic[0] = b'X';
    assert!(matches!(AreaTable::decode(&bad_magic), Err(Error::Format)));
}

#[test]
fn area_global_and_cells_preserve_actual_values_and_source_axis_order() {
    let global = GridAreas::decode(&area_bytes(u16::MAX, None)).unwrap();
    assert_eq!(global.get(world(0.5), world(127.5)), u16::MAX);
    let cells = std::array::from_fn(|i| 1000 + i as u16);
    let decoded = GridAreas::decode(&area_bytes(77, Some(&cells))).unwrap();
    assert_eq!(decoded.global, 77);
    for x in 0..16 {
        for y in 0..16 {
            assert_eq!(
                decoded.get(world(x as f32 * 8.0 + 0.5), world(y as f32 * 8.0 + 0.5)),
                cells[x * 16 + y]
            );
        }
    }
    for x in [-0.0, -1.0, -33.333_332, -266.666_66, -533.0] {
        let source_x = (16.0 * (32.0 - x / GRID_SIZE)) as usize & 15;
        assert_eq!(decoded.get(x, -1.0), cells[source_x * 16]);
    }
}

#[test]
fn malformed_area_sections_fail_closed() {
    for cells in [None, Some(&[401; 256])] {
        let bytes = area_bytes(402, cells);
        for end in 0..bytes.len() {
            assert!(GridAreas::decode(&bytes[..end]).is_err(), "prefix {end}");
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(matches!(GridAreas::decode(&extra), Err(Error::Framing)));
        let mut mismatched = bytes.clone();
        mismatched[4] ^= 1;
        assert!(matches!(
            GridAreas::decode(&mismatched),
            Err(Error::Framing)
        ));
        let mut unknown = bytes.clone();
        unknown[4] = 2;
        assert!(matches!(
            GridAreas::decode(&unknown),
            Err(Error::Unsupported)
        ));
        let mut bad_magic = bytes;
        bad_magic[0] = b'X';
        assert!(matches!(GridAreas::decode(&bad_magic), Err(Error::Format)));
    }
}

#[test]
fn liquid_overrides_use_direct_id_one_parent_and_zero_without_guessing_closure() {
    let terrain = tile(&area_bytes(401, None), 5, 0x3f);
    let types = types(&[
        (0, 2),
        (5, 0),
        (21, 1),
        (22, 2),
        (23, 3),
        (4464, 3),
        (65_535, 3),
    ]);
    let mut wrong_key = row(999, 0, 23);
    wrong_key[3] = 401; // A matching exploreFlag must not be treated as a matching ID.
    for (rows, expected) in [
        (vec![row(401, 402, 21), row(402, 0, 22)], (21, 0x12)),
        (
            vec![row(401, 70_000, 0), row(70_000, 0, 22), row(4464, 0, 23)],
            (22, 0x14),
        ),
        (vec![wrong_key], (5, 0x11)),
        (vec![], (5, 0x11)),
        (vec![row(401, 999, 0)], (0, 0x14)),
        (vec![row(401, 0, 0)], (0, 0x14)),
        (vec![row(401, 402, 77), row(402, 0, 22)], (5, 0x11)),
        (vec![row(401, 402, 65_536), row(402, 0, 22)], (5, 0x11)),
        (vec![row(401, 402, 70_000), row(402, 0, 22)], (5, 0x11)),
        (vec![row(401, 0, u32::MAX)], (5, 0x11)),
        (vec![row(401, 0, 65_535)], (65_535, 0x18)),
        (vec![row(401, u32::MAX, 0)], (0, 0x14)),
        (
            vec![row(401, 402, 0), row(402, 403, 0), row(403, 0, 23)],
            (0, 0x14),
        ),
        (vec![row(401, 402, 0), row(402, 401, 0)], (0, 0x14)),
        (vec![row(401, 401, 0)], (0, 0x14)),
    ] {
        let areas = table(&rows);
        let result = terrain
            .grid_liquid_status_with_stores(
                world(11.5),
                world(22.5),
                10.0,
                0,
                2.0,
                LiquidStores {
                    types: &types,
                    areas: Some(&areas),
                },
            )
            .unwrap();
        assert_eq!(result.status, GridLiquidStatus::WaterWalk);
        let data = result.data.unwrap();
        assert_eq!((data.entry, data.type_flags), expected, "{rows:?}");
        assert_eq!(data.level, 10.0f32.to_bits());
        assert_eq!(data.depth_level, 7.0f32.to_bits());
    }
}

#[test]
fn selected_area_cell_controls_override_and_mask_after_entry_and_type_replacement() {
    let mut cells = [401; 256];
    cells[18] = 402; // getArea(11.5,22.5), distinct from its transposed cell.
    let terrain = tile(&area_bytes(403, Some(&cells)), 5, 0x3f);
    let areas = table(&[row(401, 0, 21), row(402, 0, 22), row(403, 0, 23)]);
    let types = types(&[(5, 0), (21, 1), (22, 2), (23, 3)]);
    let stores = LiquidStores {
        types: &types,
        areas: Some(&areas),
    };
    let query = |x, y, mask| {
        terrain
            .grid_liquid_status_with_stores(world(x), world(y), 10.0, mask, 2.0, stores)
            .unwrap()
    };
    let selected = query(11.5, 22.5, 4).data.unwrap();
    assert_eq!((selected.entry, selected.type_flags), (22, 0x14));
    let transposed = query(22.5, 11.5, 2).data.unwrap();
    assert_eq!((transposed.entry, transposed.type_flags), (21, 0x12));
    assert_eq!(query(11.5, 22.5, 1).status, GridLiquidStatus::NoWater);
    assert_eq!(query(11.5, 22.5, 1).data, None);
}

#[test]
fn absent_area_store_is_unsupported_only_on_source_branch_and_zero_row_can_be_absent() {
    let area = area_bytes(401, None);
    let types = types(&[(5, 0), (21, 1)]);
    let areas = table(&[row(401, 402, 0)]);
    let terrain = tile(&area, 5, 0x3f);
    let query = |terrain: &TerrainTile, mask, areas| {
        terrain.grid_liquid_status_with_stores(
            world(1.5),
            world(2.5),
            10.0,
            mask,
            2.0,
            LiquidStores {
                types: &types,
                areas,
            },
        )
    };
    assert_eq!(query(&terrain, 0, None), Err(Error::Unsupported));
    assert_eq!(query(&terrain, 0x80, None), Err(Error::Unsupported));
    let data = query(&terrain, 0, Some(&areas)).unwrap().data.unwrap();
    assert_eq!((data.entry, data.type_flags), (5, 0x11));
    assert_eq!(
        query(&tile(&area, 5, 0), 0, None).unwrap().status,
        GridLiquidStatus::NoWater
    );
    let missing = query(&tile(&area, 4, 0x28), 0, None).unwrap().data.unwrap();
    assert_eq!((missing.entry, missing.type_flags), (4, 0x28));
    let high = query(&tile(&area, 21, 0x3f), 0, None)
        .unwrap()
        .data
        .unwrap();
    assert_eq!((high.entry, high.type_flags), (21, 0x12));
}
