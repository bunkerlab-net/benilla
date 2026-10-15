// SPDX-License-Identifier: GPL-3.0-or-later
// Authored serialization fixtures only; no install assets or production DBC rows.
use super::super::super::{tests::terrain_bytes, TileId};
use super::super::GRID_SIZE;
use super::*;

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}
fn replace(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn dbc(rows: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for n in [rows.len() as u32, 4, 16, 1] {
        word(&mut bytes, n);
    }
    for &(id, kind) in rows {
        for n in [id, 123, kind, 456] {
            word(&mut bytes, n);
        }
    }
    bytes.push(0);
    bytes
}
fn section(flags: u8, entry: u16, kind: u8, rect: [u8; 4], level: f32) -> Vec<u8> {
    let mut bytes = b"MLIQ".to_vec();
    bytes.extend([flags, kind]);
    bytes.extend(entry.to_le_bytes());
    bytes.extend(rect);
    word(&mut bytes, level.to_bits());
    if flags & NO_TYPE == 0 {
        for _ in 0..256 {
            bytes.extend(entry.to_le_bytes());
        }
        bytes.extend([kind; 256]);
    }
    if flags & NO_HEIGHT == 0 {
        for _ in 0..usize::from(rect[2]) * usize::from(rect[3]) {
            word(&mut bytes, level.to_bits());
        }
    }
    bytes
}
fn map_bytes(shape: usize, section: &[u8]) -> Vec<u8> {
    let mut bytes = terrain_bytes(shape);
    let at = u32_at(&bytes, 32).unwrap() as usize;
    let holes = bytes.split_off(at);
    bytes.extend(section);
    bytes.extend(holes);
    replace(&mut bytes, 24, at as u32);
    replace(&mut bytes, 28, section.len() as u32);
    replace(&mut bytes, 32, (at + section.len()) as u32);
    bytes
}
fn tile(shape: usize, section: &[u8]) -> TerrainTile {
    TerrainTile::decode(TileId { x: 32, y: 32 }, &map_bytes(shape, section)).unwrap()
}
fn world(cell: f32) -> f32 {
    -cell * (GRID_SIZE / 128.0)
}
fn query(
    tile: &TerrainTile,
    x: f32,
    y: f32,
    z: f32,
    mask: u8,
    types: &LiquidTypes,
) -> Result<GridLiquidResult, Error> {
    tile.grid_liquid_status(world(x), world(y), z, mask, 2.0, types)
}
fn empty_types() -> LiquidTypes {
    LiquidTypes::decode(&dbc(&[])).unwrap()
}

#[test]
fn complete_niii_table_checks_framing_duplicates_limits_and_hashes_all_bytes() {
    let bytes = dbc(&[(22, 0), (33, 1), (44, 2), (55, 3)]);
    let table = LiquidTypes::decode(&bytes).unwrap();
    assert_eq!(table.digest(), Hash::from(Sha256::digest(&bytes)));
    assert_eq!(table.entries.get(&22), Some(&0));
    assert_eq!(table.entries.get(&54), None);
    for end in 0..bytes.len() {
        assert!(LiquidTypes::decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(matches!(LiquidTypes::decode(&extra), Err(Error::Framing)));
    for (at, value, error) in [
        (4, 65_537, Error::Budget),
        (8, 3, Error::Unsupported),
        (12, 12, Error::Unsupported),
        (16, 0, Error::Framing),
        (16, 1_048_577, Error::Budget),
        (20, 65_536, Error::Unsupported),
        (28, 4, Error::Unsupported),
    ] {
        let mut bad = bytes.clone();
        replace(&mut bad, at, value);
        assert!(matches!(LiquidTypes::decode(&bad), Err(e) if e == error));
    }
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() = 1;
    assert!(LiquidTypes::decode(&bad).is_err());
    assert!(matches!(
        LiquidTypes::decode(&dbc(&[(22, 0), (22, 3)])),
        Err(Error::Duplicate)
    ));
    let mut changed_unused_field = bytes.clone();
    replace(&mut changed_unused_field, 24, 789);
    assert_ne!(
        table.digest(),
        LiquidTypes::decode(&changed_unused_field).unwrap().digest()
    );
}

#[test]
fn mliq_optional_arrays_are_exact_and_writer_129_vertex_edges_are_valid() {
    for flags in 0..=3 {
        let section = section(flags, 22, 8, [0, 0, 129, 129], 10.0);
        let terrain = tile(0, &section);
        assert!(terrain.has_liquid());
        assert_eq!(
            query(&terrain, 127.5, 127.5, 10.0, 0, &empty_types())
                .unwrap()
                .status,
            GridLiquidStatus::WaterWalk
        );
        for end in [0, 3, 4, 15, 16, section.len() - 1] {
            if end < section.len() {
                assert!(GridLiquid::decode(&section[..end]).is_err());
            }
        }
        let mut extra = section;
        extra.push(0);
        assert!(matches!(GridLiquid::decode(&extra), Err(Error::Framing)));
    }
    let dry = TerrainTile::decode(TileId { x: 32, y: 32 }, &terrain_bytes(0)).unwrap();
    assert!(!dry.has_liquid());
    assert_eq!(
        query(&dry, 1.0, 1.0, 10.0, 0, &empty_types()).unwrap(),
        GridLiquidResult::no_water()
    );
    let edge = tile(0, &section(3, 22, 8, [127, 127, 2, 2], 10.0));
    assert_eq!(
        query(&edge, 127.5, 127.5, 10.0, 0, &empty_types())
            .unwrap()
            .status,
        GridLiquidStatus::WaterWalk
    );
}

#[test]
fn unknown_flags_bad_rectangles_and_nonfinite_levels_are_rejected() {
    let valid = section(0, 22, 8, [1, 2, 4, 5], 10.0);
    for (at, value) in [
        (4, 4),
        (5, 0x40),
        (8, 128),
        (9, 128),
        (10, 0),
        (11, 0),
        (10, 129),
        (11, 128),
        (16 + 512, 0x80),
    ] {
        let mut bad = valid.clone();
        bad[at] = value;
        assert!(GridLiquid::decode(&bad).is_err(), "field {at}");
    }
    for at in [12, 784] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.0] {
            let mut bad = valid.clone();
            replace(&mut bad, at, value.to_bits());
            assert!(matches!(GridLiquid::decode(&bad), Err(Error::Format)));
        }
    }
    let valid_map = map_bytes(0, &valid);
    for (at, value) in [
        (24, 0),
        (28, 0),
        (24, 65),
        (32, 64),
        (32, u32::MAX),
        (28, u32::MAX),
    ] {
        let mut bad = valid_map.clone();
        replace(&mut bad, at, value);
        assert!(TerrainTile::decode(TileId { x: 32, y: 32 }, &bad).is_err());
    }
}

#[test]
fn source_thresholds_preserve_data_and_strict_ground_comparisons() {
    let table = empty_types();
    let terrain = tile(0, &section(3, 22, 8, [0, 0, 129, 129], 10.0));
    for (z, expected) in [
        (5.0, GridLiquidStatus::UnderWater),
        (
            f32::from_bits(8.0f32.to_bits() - 1),
            GridLiquidStatus::UnderWater,
        ),
        (8.0, GridLiquidStatus::InWater),
        (
            f32::from_bits(10.0f32.to_bits() - 1),
            GridLiquidStatus::InWater,
        ),
        (10.0, GridLiquidStatus::WaterWalk),
        (
            f32::from_bits(11.0f32.to_bits() - 1),
            GridLiquidStatus::WaterWalk,
        ),
        (11.0, GridLiquidStatus::AboveWater),
    ] {
        assert_eq!(
            query(&terrain, 2.5, 3.5, z, 0, &table).unwrap(),
            GridLiquidResult {
                status: expected,
                data: Some(GridLiquidData {
                    entry: 22,
                    type_flags: 8,
                    level: 10.0f32.to_bits(),
                    depth_level: 7.0f32.to_bits()
                }),
            }
        );
    }
    for (z, mask) in [(f32::from_bits(5.0f32.to_bits() - 1), 0), (10.0, 1)] {
        assert_eq!(
            query(&terrain, 2.5, 3.5, z, mask, &table).unwrap(),
            GridLiquidResult::no_water()
        );
    }
    let below = tile(0, &section(3, 22, 8, [0, 0, 129, 129], 6.0));
    assert_eq!(
        query(&below, 2.5, 3.5, 10.0, 0, &table).unwrap(),
        GridLiquidResult::no_water()
    );
    let equal = tile(0, &section(3, 22, 8, [0, 0, 129, 129], 7.0));
    assert_eq!(
        query(&equal, 2.5, 3.5, 7.0, 0, &table).unwrap().status,
        GridLiquidStatus::WaterWalk
    );
}

#[test]
fn dbc_lookup_precedes_zero_mask_and_rectangle_and_preserves_missing_entry() {
    let table = LiquidTypes::decode(&dbc(&[(5, 3), (22, 0)])).unwrap();
    let terrain = tile(0, &section(3, 22, 0x3a, [0, 0, 129, 129], 10.0));
    let result = query(&terrain, 3.0, 4.0, 10.0, 1, &table).unwrap();
    assert_eq!(result.data.unwrap().type_flags, 0x11); // Only deep + DBC magma survive.
    assert_eq!(
        query(&terrain, 3.0, 4.0, 10.0, 8, &table).unwrap(),
        GridLiquidResult::no_water()
    );
    let low = tile(0, &section(2, 5, 0, [10, 20, 2, 2], 10.0));
    assert_eq!(
        query(&low, 3.0, 4.0, 10.0, 1, &table),
        Err(Error::Unsupported)
    );
    let global_zero = tile(0, &section(3, 5, 0, [0, 0, 129, 129], 10.0));
    assert_eq!(
        query(&global_zero, 3.0, 4.0, 10.0, 0, &table).unwrap(),
        GridLiquidResult::no_water()
    );
    let missing = tile(0, &section(3, 4, 0x28, [0, 0, 129, 129], 10.0));
    let data = query(&missing, 3.0, 4.0, 10.0, 0x20, &table)
        .unwrap()
        .data
        .unwrap();
    assert_eq!((data.entry, data.type_flags), (4, 0x28));
}

#[test]
fn offset_axes_cell_types_and_sample_stride_follow_source() {
    let mut bytes = section(0, 0, 0, [20, 10, 4, 3], 10.0);
    let idx = 18; // Source cell index: (11 >> 3) * 16 + (22 >> 3).
    bytes[16 + idx * 2..18 + idx * 2].copy_from_slice(&22u16.to_le_bytes());
    bytes[16 + 512 + idx] = 8;
    replace(&mut bytes, 784 + (4 + 2) * 4, 12.5f32.to_bits());
    let terrain = tile(0, &bytes);
    let table = empty_types();
    let result = query(&terrain, 11.5, 22.5, 12.5, 8, &table).unwrap();
    assert_eq!(result.data.unwrap().level, 12.5f32.to_bits());
    assert_eq!(result.data.unwrap().entry, 22);
    for (x, y) in [
        (9.5, 22.5),
        (13.5, 22.5),
        (11.5, 19.5),
        (11.5, 24.5),
        (22.5, 11.5),
    ] {
        assert_eq!(
            query(&terrain, x, y, 12.5, 0, &table).unwrap(),
            GridLiquidResult::no_water()
        );
    }
}

#[test]
fn float_hole_uses_invalid_height_sentinel_while_flat_dispatch_bypasses_holes() {
    let bytes = section(3, 22, 8, [0, 0, 129, 129], 10.0);
    let float = tile(6, &bytes);
    assert_eq!(
        float.point_height(world(0.5), world(0.5)).unwrap(),
        PointHeight::Hole
    );
    let data = query(&float, 0.5, 0.5, 9.0, 0, &empty_types())
        .unwrap()
        .data
        .unwrap();
    assert_eq!(data.depth_level, (-200_000.0f32).to_bits());
    let flat = tile(7, &bytes);
    assert_eq!(
        flat.point_height(world(0.5), world(0.5)).unwrap(),
        PointHeight::Height(7.0)
    );
    assert_eq!(
        query(&flat, 0.5, 0.5, 9.0, 0, &empty_types())
            .unwrap()
            .data
            .unwrap()
            .depth_level,
        7.0f32.to_bits()
    );
}

#[test]
fn query_guards_reject_invalid_coordinates_and_unbounded_arithmetic() {
    let terrain = tile(0, &section(3, 22, 8, [0, 0, 129, 129], 10.0));
    let table = empty_types();
    for x in [f32::NAN, f32::INFINITY, 1.0, -GRID_SIZE] {
        assert_eq!(
            terrain.grid_liquid_status(x, -1.0, 10.0, 0, 2.0, &table),
            Err(Error::OutsideTile)
        );
    }
    for bad in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        assert_eq!(
            terrain.grid_liquid_status(-1.0, -1.0, bad, 0, 2.0, &table),
            Err(Error::Format)
        );
        assert_eq!(
            terrain.grid_liquid_status(-1.0, -1.0, 10.0, 0, bad, &table),
            Err(Error::Format)
        );
    }
}

#[test]
fn all_four_dbc_types_replace_raw_flags_and_signed_zero_is_not_canonicalized() {
    for kind in 0..4 {
        let types = LiquidTypes::decode(&dbc(&[(22, kind)])).unwrap();
        let terrain = tile(0, &section(3, 22, 0x3f, [0, 0, 129, 129], 10.0));
        let data = query(&terrain, 3.0, 4.0, 10.0, 1 << kind, &types)
            .unwrap()
            .data
            .unwrap();
        assert_eq!(data.type_flags, 0x10 | (1 << kind));
    }
    let mut bytes = map_bytes(0, &section(3, 22, 8, [0, 0, 129, 129], -0.0));
    replace(&mut bytes, 56, (-7.0f32).to_bits());
    replace(&mut bytes, 60, (-7.0f32).to_bits());
    let terrain = TerrainTile::decode(TileId { x: 32, y: 32 }, &bytes).unwrap();
    let data = query(&terrain, 3.0, 4.0, 0.0, 0, &empty_types())
        .unwrap()
        .data
        .unwrap();
    assert_eq!(data.level, (-0.0f32).to_bits());
    assert_eq!(data.depth_level, (-7.0f32).to_bits());
}
