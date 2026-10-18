// SPDX-License-Identifier: GPL-3.0-or-later
//! Real app reader integration using newly authored raw members and pinned-source vectors.
use super::super::{
    terrain::{
        AreaTable, GridLiquidData, GridLiquidResult, GridLiquidStatus, LiquidStores, LiquidTypes,
    },
    PointHeight,
};
use super::*;
use benilla_native_terrain::{prepare, writer, Tile};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

// Authored MPQ transport uses the existing format table encoder, not a mock source reader.
#[path = "../../benilla-mpq/src/crypto.rs"]
mod crypto;

const MAP: u32 = 1;
const TILE: TileId = TileId { x: 31, y: 32 };
const GRID: f32 = 533.333_3;
const MAP_MEMBER: &str = "DBFilesClient\\Map.dbc";
const AREA_MEMBER: &str = "DBFilesClient\\AreaTable.dbc";
const LIQUID_MEMBER: &str = "DBFilesClient\\LiquidType.dbc";
const WDT_MEMBER: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle.wdt";
// Source filename order reverses the product's (31, 32) grid pair.
const ADT_MEMBER: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle_32_31.adt";

fn fixture(name: &str) -> Vec<u8> {
    let name = if [".dbc", ".adt", ".wdt"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        format!("{name}.test-fixture")
    } else {
        name.to_owned()
    };
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures")
            .join(name),
    )
    .unwrap()
}

fn produce(case: &str) -> Vec<u8> {
    writer::produce(
        &fixture("Map.dbc"),
        &fixture("AreaTable.dbc"),
        &fixture("LiquidType.dbc"),
        &fixture("source.wdt"),
        &fixture(&format!("{case}.adt")),
        MAP,
        TILE.x,
        TILE.y,
    )
    .unwrap()
}

fn decoded(case: &str) -> TerrainTile {
    TerrainTile::decode(TILE, &produce(case)).unwrap()
}

fn world(x: f32, y: f32) -> (f32, f32) {
    (
        (32.0 - (f32::from(TILE.x) + x / 128.0)) * GRID,
        (32.0 - (f32::from(TILE.y) + y / 128.0)) * GRID,
    )
}

fn height(tile: &TerrainTile, x: f32, y: f32) -> PointHeight {
    let (x, y) = world(x, y);
    tile.point_height(x, y).unwrap()
}

fn near_height(tile: &TerrainTile, x: f32, y: f32, expected: f32) {
    let PointHeight::Height(actual) = height(tile, x, y) else {
        panic!("unexpected hole")
    };
    assert!(
        (actual - expected).abs() < 0.0002,
        "({x}, {y}): {actual} != {expected}"
    );
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn replace(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn produced_source_vectors_load_the_exact_app_terrain_reader() {
    for case in [
        "flat",
        "varied",
        "seams",
        "clamp_mixed",
        "clamp_flat",
        "tiny_range",
        "area_full",
        "area_fallback",
        "holes",
        "upper_sentinel",
        "missing_mcvt",
        "missing_mcvt_signed_zero",
    ] {
        let raw = produce(case);
        assert_eq!(raw, fixture(&format!("{case}.map")), "{case}");
        let tile = TerrainTile::decode(TILE, &raw).unwrap();
        assert!(!tile.has_liquid(), "{case}");
        for (x, y) in [(0.0, 0.0), (8.0, 8.0), (63.5, 71.25), (127.5, 127.5)] {
            assert!(
                matches!(height(&tile, x, y), PointHeight::Height(_)),
                "{case}"
            );
        }
    }
}

#[test]
fn generated_heights_preserve_base_interleave_axes_seams_clamp_and_flat_dispatch() {
    near_height(&decoded("flat"), 3.25, 96.75, 7.25);
    near_height(&decoded("clamp_flat"), 63.5, 64.5, -500.0);
    near_height(&decoded("missing_mcvt"), 3.25, 96.75, 7.25);
    let PointHeight::Height(zero) = height(&decoded("missing_mcvt_signed_zero"), 3.25, 96.75)
    else {
        panic!("flat base-only signed-zero tile must have a height")
    };
    assert_eq!(zero.to_bits(), (-0.0f32).to_bits());
    let varied = decoded("varied");
    // Authored MCVT rows advance 17/32, columns 1/32; center row starts at index 9.
    near_height(&varied, 0.0, 0.0, -12.25);
    near_height(&varied, 0.5, 0.5, -11.96875);
    near_height(&varied, 2.25, 3.75, -10.9375);
    near_height(&varied, 3.75, 2.25, -10.1875);
    near_height(&varied, 0.75, 0.25, -11.84375);
    near_height(&varied, 0.25, 0.75, -12.09375);
    near_height(&varied, 0.875, 0.625, -11.765625);
    near_height(&varied, 0.625, 0.875, -11.890625);
    // MCIN row-major wins shared corners even though MCNK's ix/iy are inverted.
    let seams = decoded("seams");
    near_height(&seams, 0.0, 8.0, 10.0);
    near_height(&seams, 8.0, 0.0, 160.0);
    near_height(&seams, 8.0, 8.0, 170.0);
    let tiny = decoded("tiny_range");
    near_height(&tiny, 0.5, 0.5, 7.0 + 9.0 / 65536.0);
    let upper = decoded("upper_sentinel");
    near_height(&upper, 0.0, 0.0, 25000.0);
    near_height(&upper, 8.0, 8.0, 25017.0);
    let (x, y) = world(128.0, 1.0);
    assert_eq!(varied.point_height(x, y), Err(Error::OutsideTile));
    assert_eq!(varied.point_height(f32::NAN, 0.0), Err(Error::OutsideTile));
}

#[test]
fn generated_area_flags_use_raw_narrowed_dbc_values_and_source_grid_axes() {
    let tile = decoded("area_full");
    let flags = [0xffff, 0x1234, 2, 0xffff, 0, 0xfffe, 0x1234, 0xffff, 0xffff];
    for row in 0..16 {
        for col in 0..16 {
            let (x, y) = world(row as f32 * 8.0 + 4.0, col as f32 * 8.0 + 4.0);
            assert_eq!(
                tile.area_flag(x, y),
                Ok(flags[(row * 16 + col) % flags.len()])
            );
        }
    }
    let fallback = decoded("area_fallback");
    let (x, y) = world(117.0, 11.0);
    assert_eq!(fallback.area_flag(x, y), Ok(0xffff));
    assert_eq!(tile.area_flag(f32::INFINITY, 0.0), Err(Error::OutsideTile));
}

#[test]
fn generated_holes_follow_all_source_mask_bits_and_flat_height_bypasses_them() {
    let flat = decoded("holes");
    let raw = produce("nonflat_holes");
    assert_eq!(raw, fixture("nonflat_holes.map"));
    let tile = TerrainTile::decode(TILE, &raw).unwrap();
    for slot in 0..256 {
        let mask = if slot < 128 {
            1 << (slot % 16)
        } else {
            (slot * 257) & 0xffff
        };
        for bit in 0..16 {
            let x = ((slot / 16) * 8 + (bit / 4) * 2) as f32 + 0.5;
            let y = ((slot % 16) * 8 + (bit % 4) * 2) as f32 + 0.5;
            assert_eq!(height(&flat, x, y), PointHeight::Height(7.25));
            assert_eq!(
                matches!(height(&tile, x, y), PointHeight::Hole),
                mask & (1 << bit) != 0,
                "slot {slot}, bit {bit}"
            );
        }
    }
}

#[test]
fn actual_reader_rejects_corrupted_generated_framing_flags_and_height_payloads() {
    let raw = produce("varied");
    let h = word(&raw, 16) as usize;
    for (at, value) in [
        (0, 0),
        (4, 0),
        (8, 41),
        (12, 9),
        (16, h as u32 + 1),
        (20, 16),
        (24, 1),
        (28, 1),
        (32, 40),
        (36, 511),
        (40, 0),
        (44, 0x1234_0002),
        (h, 0),
        (h + 4, 2),
        (h + 8, f32::NAN.to_bits()),
        (h + 12, f32::INFINITY.to_bits()),
        (h + 16, f32::NAN.to_bits()),
        (h + 16, 900_000.0f32.to_bits()),
    ] {
        let mut changed = raw.clone();
        replace(&mut changed, at, value);
        assert!(
            TerrainTile::decode(TILE, &changed).is_err(),
            "offset {at}, value {value:x}"
        );
    }
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(TerrainTile::decode(TILE, &trailing).is_err());
    assert!(TerrainTile::decode(TILE, &raw[..raw.len() - 1]).is_err());
    assert!(TerrainTile::decode(TileId { x: 64, y: 32 }, &raw).is_err());
}

// The writer's AreaTable fixture deliberately tests duplicate physical rows. Query
// with a separate complete, empty authored area store, so no area override is implied.
fn query_liquid(tile: &TerrainTile, x: f32, y: f32, z: f32, required_mask: u8) -> GridLiquidResult {
    let types = LiquidTypes::decode(&fixture("LiquidType.dbc")).unwrap();
    let mut area = b"WDBC".to_vec();
    for value in [0_u32, 25, 100, 1] {
        area.extend(value.to_le_bytes());
    }
    area.push(0);
    let areas = AreaTable::decode(&area).unwrap();
    let (x, y) = world(x, y);
    tile.grid_liquid_status_with_stores(
        x,
        y,
        z,
        required_mask,
        2.0,
        LiquidStores {
            types: &types,
            areas: Some(&areas),
        },
    )
    .unwrap()
}

fn no_water(tile: &TerrainTile, x: f32, y: f32) {
    assert_eq!(
        query_liquid(tile, x, y, 10.0, 0),
        GridLiquidResult {
            status: GridLiquidStatus::NoWater,
            data: None
        },
        "({x}, {y})"
    );
}

fn liquid_data(tile: &TerrainTile, x: f32, y: f32, entry: u32, flags: u32, level: f32) {
    assert_eq!(
        query_liquid(tile, x, y, 7.25, 0),
        GridLiquidResult {
            status: GridLiquidStatus::UnderWater,
            data: Some(GridLiquidData {
                entry,
                type_flags: flags,
                level: level.to_bits(),
                depth_level: 7.25_f32.to_bits(),
            }),
        },
        "({x}, {y})"
    );
}

#[test]
fn actual_reader_loads_generated_liquid_vectors_with_all_mliq_packing_branches() {
    let mut seen_flags = [false; 4];
    for case in [
        "liquid_water_flat",
        "liquid_ocean_flat",
        "liquid_magma_flat",
        "liquid_zero_size",
        "liquid_full_types",
        "liquid_flag_bytes",
        "liquid_deep_only",
        "liquid_nonsymmetric",
        "liquid_seams",
        "liquid_outer_edges",
        "liquid_sparse_flat",
    ] {
        let raw = produce(case);
        assert_eq!(raw, fixture(&format!("{case}.map")), "{case}");
        let at = word(&raw, 24) as usize;
        assert_ne!(at, 0, "{case}");
        assert_eq!(&raw[at..at + 4], b"MLIQ", "{case}");
        seen_flags[raw[at + 4] as usize] = true;
        let tile = TerrainTile::decode(TILE, &raw).unwrap();
        assert!(tile.has_liquid(), "{case}");
        near_height(&tile, 63.5, 71.25, 7.25);
    }
    assert_eq!(seen_flags, [true; 4]);
}

#[test]
fn source_skip_branches_and_untyped_mclq_do_not_create_liquid_evidence() {
    for case in [
        "liquid_absent_flags",
        "liquid_empty",
        "liquid_size8",
        "liquid_untyped",
    ] {
        let raw = produce(case);
        assert_eq!(raw, fixture("flat.map"), "{case}");
        let tile = TerrainTile::decode(TILE, &raw).unwrap();
        assert!(!tile.has_liquid(), "{case}");
        no_water(&tile, 1.5, 1.5);
    }
}

#[test]
fn generated_scalar_liquid_queries_retain_catalog_mask_height_and_status_semantics() {
    let tile = decoded("liquid_water_flat");
    for (z, status) in [
        (20.5, GridLiquidStatus::UnderWater),
        (22.5, GridLiquidStatus::InWater),
        (23.5, GridLiquidStatus::WaterWalk),
        (24.0, GridLiquidStatus::WaterWalk),
        (24.5, GridLiquidStatus::AboveWater),
    ] {
        assert_eq!(
            query_liquid(&tile, 63.5, 71.25, z, 1),
            GridLiquidResult {
                status,
                data: Some(GridLiquidData {
                    entry: 1,
                    type_flags: 1,
                    level: 23.5_f32.to_bits(),
                    depth_level: 7.25_f32.to_bits(),
                }),
            }
        );
    }
    assert_eq!(query_liquid(&tile, 63.5, 71.25, 20.5, 2).data, None);
    assert_eq!(query_liquid(&tile, 63.5, 71.25, 5.0, 0).data, None);
    liquid_data(&decoded("liquid_magma_flat"), 127.5, 127.5, 3, 1, 31.75);
    // Presence alone is not water: this source ocean surface is below the ground.
    let ocean = decoded("liquid_ocean_flat");
    assert!(ocean.has_liquid());
    no_water(&ocean, 63.5, 71.25);
}

#[test]
fn generated_full_types_and_deep_flags_follow_real_grid_liquid_queries() {
    let tile = decoded("liquid_full_types");
    for (slot, (entry, flags)) in [(1, 1), (2, 2), (3, 1), (2, 10), (3, 9), (3, 3), (3, 11)]
        .into_iter()
        .enumerate()
    {
        liquid_data(&tile, 0.5, slot as f32 * 8.0 + 0.5, entry, flags, 13.25);
    }
    for (case, entry, flags) in [
        ("liquid_flag_bytes", 1, 0x11),
        ("liquid_deep_only", 0, 0x10),
    ] {
        let tile = decoded(case);
        // Only exact 0x0f hides source cells; 0x1f, 0x8f and 0xff remain visible.
        // One deep byte contributes a chunk-wide flag to every visible sample.
        for x in 1..=6 {
            liquid_data(
                &tile,
                10.5,
                16.5 + x as f32,
                entry,
                flags,
                44.0 + x as f32 / 4.0,
            );
        }
        no_water(&tile, 10.5, 16.5);
        no_water(&tile, 10.5, 23.5);
        no_water(&tile, 11.5, 17.5);
    }
}

#[test]
fn generated_crop_axes_seam_winners_and_owned_outer_edges_reach_actual_queries() {
    let tile = decoded("liquid_nonsymmetric");
    for (x, y, level) in [
        (26.5, 57.5, 104.25),
        (27.5, 60.5, 107.0),
        (28.5, 61.5, 109.25),
    ] {
        liquid_data(&tile, x, y, 1, 1, level);
    }
    for (x, y) in [
        (25.5, 57.5),
        (26.5, 56.5),
        (57.5, 26.5),
        (29.5, 61.5),
        (28.5, 62.5),
    ] {
        no_water(&tile, x, y);
    }
    let seams = decoded("liquid_seams");
    for (x, y, level) in [(0.5, 8.5, 10.0), (8.5, 0.5, 160.0), (8.5, 8.5, 170.0)] {
        liquid_data(&seams, x, y, 1, 1, level);
    }
    let edges = decoded("liquid_outer_edges");
    liquid_data(&edges, 122.5, 121.5, 3, 1, 404.25);
    liquid_data(&edges, 127.5, 127.5, 3, 1, 415.75);
    no_water(&edges, 121.5, 121.5);
    no_water(&edges, 122.5, 120.5);
    let (x, y) = world(128.0, 127.5);
    assert_eq!(edges.point_height(x, y), Err(Error::OutsideTile));
    // The source omits heights for this exactly flat sparse crop. Unknown outer
    // heights are not serialized and therefore do not invalidate its MLIQ data.
    let sparse = decoded("liquid_sparse_flat");
    assert!(sparse.has_liquid());
    no_water(&sparse, 19.5, 127.5);
    no_water(&sparse, 127.5, 29.5);
}

#[test]
fn actual_reader_rejects_corrupted_generated_mliq_before_queries() {
    let raw = produce("liquid_nonsymmetric");
    let at = word(&raw, 24) as usize;
    for (offset, value) in [
        (4, 4),
        (5, 0x80),
        (8, 128),
        (9, 128),
        (10, 0),
        (11, 0),
        (10, 129),
        (11, 129),
    ] {
        let mut changed = raw.clone();
        changed[at + offset] = value;
        assert!(
            TerrainTile::decode(TILE, &changed).is_err(),
            "{offset}: {value}"
        );
    }
    for offset in [12, 16 + 768] {
        let mut changed = raw.clone();
        replace(&mut changed, at + offset, f32::NAN.to_bits());
        assert!(TerrainTile::decode(TILE, &changed).is_err(), "{offset}");
    }
    let mut changed = raw.clone();
    changed[at + 16 + 512] = 0x80;
    assert!(TerrainTile::decode(TILE, &changed).is_err());
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "terrain-reader-production-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn data(&self, case: &str) -> PathBuf {
        let data = self.0.join("authored-install/Data");
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join("terrain.MPQ"), archive(&members(case))).unwrap();
        data
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn members(case: &str) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (MAP_MEMBER, fixture("Map.dbc")),
        (AREA_MEMBER, fixture("AreaTable.dbc")),
        (LIQUID_MEMBER, fixture("LiquidType.dbc")),
        (WDT_MEMBER, fixture("source.wdt")),
        (ADT_MEMBER, fixture(&format!("{case}.adt"))),
    ]
}

fn archive(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    let slots = 64u32;
    let hash_pos = 32u32;
    let block_pos = hash_pos + slots * 16;
    let mut data_pos = block_pos + entries.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; slots as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, bytes)) in entries.iter().enumerate() {
        let mut slot = (hash_string(name, hash_type::TABLE_OFFSET) & (slots - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (slots as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash_string(name, hash_type::NAME_A),
            hash_string(name, hash_type::NAME_B),
            0,
            index as u32,
        ]);
        blocks.extend([
            data_pos,
            bytes.len() as u32,
            bytes.len() as u32,
            0x8000_0000,
        ]);
        data_pos += bytes.len() as u32;
    }
    encrypt_block(
        &mut hashes,
        hash_string("(hash table)", hash_type::FILE_KEY),
    );
    encrypt_block(
        &mut blocks,
        hash_string("(block table)", hash_type::FILE_KEY),
    );
    let mut bytes = vec![0u8; 32];
    for (at, value) in [
        (0, 0x1a51_504d_u32),
        (4, 32),
        (8, data_pos),
        (16, hash_pos),
        (20, block_pos),
        (24, slots),
        (28, entries.len() as u32),
    ] {
        replace(&mut bytes, at, value);
    }
    for value in hashes.into_iter().chain(blocks) {
        bytes.extend(value.to_le_bytes());
    }
    for (_, member) in entries {
        bytes.extend(member);
    }
    bytes
}

fn prepared(data: &Path) -> PreparedTerrain {
    prepare(
        data,
        MAP,
        &[Tile {
            x: TILE.x,
            y: TILE.y,
        }],
    )
    .unwrap()
}

#[test]
fn captured_source_decodes_retains_epoch_and_publishes_only_real_generated_tiles() {
    let temp = Temp::new();
    let data = temp.data("varied");
    let before = fs::read(data.join("terrain.MPQ")).unwrap();
    let input = prepared(&data);
    let epoch = input.source_epoch().to_owned();
    let products = SourceTerrainProducts::from_prepared(input).unwrap();
    assert_eq!(products.source_epoch(), epoch);
    near_height(products.terrain(MAP, TILE).unwrap(), 0.0, 0.0, -12.25);
    assert_eq!(
        products.terrain(MAP + 1, TILE).map(|_| ()),
        Err(Error::Identity)
    );
    assert_eq!(
        products.terrain(MAP, TileId { x: 32, y: 31 }).map(|_| ()),
        Err(Error::Missing)
    );
    let output = temp.0.join("export");
    products.publish(&output).unwrap();
    assert_eq!(
        fs::read(output.join("maps/0013132.map")).unwrap(),
        produce("varied")
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("terrain.json")).unwrap()).unwrap();
    assert_eq!(manifest["source_epoch"], epoch);
    assert_eq!(manifest["gameplay_admitted"], false);
    assert_eq!(manifest["live_install_lease"], false);
    assert!(!output.join("vmaps").exists());
    assert!(!output.join("mmaps").exists());
    assert_eq!(fs::read(data.join("terrain.MPQ")).unwrap(), before);
}

#[test]
fn actual_consumer_rejects_source_drift_before_consumption_or_publication() {
    let temp = Temp::new();
    let data = temp.data("flat");
    let input = prepared(&data);
    fs::write(data.join("patch.MPQ"), archive(&members("flat"))).unwrap();
    assert_eq!(
        SourceTerrainProducts::from_prepared(input).map(|_| ()),
        Err(Error::Io)
    );
    fs::remove_file(data.join("patch.MPQ")).unwrap();
    let products = SourceTerrainProducts::from_prepared(prepared(&data)).unwrap();
    fs::write(data.join("terrain.MPQ"), archive(&members("varied"))).unwrap();
    // Retained decoded bytes remain the original snapshot, never a lazy source reload.
    near_height(products.terrain(MAP, TILE).unwrap(), 1.0, 1.0, 7.25);
    let output = temp.0.join("rejected");
    assert_eq!(products.publish(&output), Err(Error::Io));
    assert!(!output.exists());
}

#[test]
fn actual_consumer_applies_existing_reader_bounds_to_finite_producer_output() {
    let temp = Temp::new();
    let data = temp.data("flat");
    let mut source = members("flat");
    let adt = &mut source.last_mut().unwrap().1;
    let cell = word(adt, 92) as usize;
    replace(adt, cell + 8 + 112, 1_000_001.0f32.to_bits());
    fs::write(data.join("terrain.MPQ"), archive(&source)).unwrap();
    let input = prepared(&data);
    assert_eq!(
        SourceTerrainProducts::from_prepared(input).map(|_| ()),
        Err(Error::Format)
    );
}

#[test]
fn captured_liquid_source_uses_existing_consumer_and_preserves_private_export_boundary() {
    for case in [
        "liquid_water_flat",
        "liquid_nonsymmetric",
        "liquid_outer_edges",
        "liquid_sparse_flat",
    ] {
        let temp = Temp::new();
        let data = temp.data(case);
        let source = fs::read(data.join("terrain.MPQ")).unwrap();
        let input = prepared(&data);
        let epoch = input.source_epoch().to_owned();
        assert_eq!(
            input.tiles().next().unwrap().1,
            fixture(&format!("{case}.map"))
        );
        let products = SourceTerrainProducts::from_prepared(input).unwrap();
        assert_eq!(products.source_epoch(), epoch);
        let tile = products.terrain(MAP, TILE).unwrap();
        assert!(tile.has_liquid());
        match case {
            "liquid_water_flat" => liquid_data(tile, 1.5, 1.5, 1, 1, 23.5),
            "liquid_nonsymmetric" => liquid_data(tile, 26.5, 57.5, 1, 1, 104.25),
            "liquid_outer_edges" => liquid_data(tile, 127.5, 127.5, 3, 1, 415.75),
            "liquid_sparse_flat" => no_water(tile, 19.5, 127.5),
            _ => unreachable!(),
        }
        let output = temp.0.join("export");
        products.publish(&output).unwrap();
        assert_eq!(
            fs::read(output.join("maps/0013132.map")).unwrap(),
            fixture(&format!("{case}.map"))
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("terrain.json")).unwrap()).unwrap();
        assert_eq!(manifest["format"], "benilla-terrain-production-v3");
        assert_eq!(
            manifest["writer_policy"],
            "cmangos-ad-f0-boundary-history-closure-v3"
        );
        // These four selected tiles own every emitted height; v3 therefore
        // records no additional absent-member dependencies for their history.
        assert_eq!(manifest["absent_members"], serde_json::json!([]));
        assert_eq!(manifest["source_epoch"], epoch);
        assert_eq!(manifest["gameplay_admitted"], false);
        assert_eq!(manifest["live_install_lease"], false);
        assert!(!output.join("vmaps").exists());
        assert!(!output.join("mmaps").exists());
        assert_eq!(fs::read(data.join("terrain.MPQ")).unwrap(), source);
    }
}

#[test]
fn captured_liquid_snapshot_remains_owned_and_source_drift_refuses_publication() {
    let temp = Temp::new();
    let data = temp.data("liquid_nonsymmetric");
    let input = prepared(&data);
    fs::write(
        data.join("patch.MPQ"),
        archive(&members("liquid_nonsymmetric")),
    )
    .unwrap();
    assert_eq!(
        SourceTerrainProducts::from_prepared(input).map(|_| ()),
        Err(Error::Io)
    );
    fs::remove_file(data.join("patch.MPQ")).unwrap();
    let products = SourceTerrainProducts::from_prepared(prepared(&data)).unwrap();
    fs::write(data.join("terrain.MPQ"), archive(&members("flat"))).unwrap();
    liquid_data(
        products.terrain(MAP, TILE).unwrap(),
        26.5,
        57.5,
        1,
        1,
        104.25,
    );
    let output = temp.0.join("rejected");
    assert_eq!(products.publish(&output), Err(Error::Io));
    assert!(!output.exists());
}
