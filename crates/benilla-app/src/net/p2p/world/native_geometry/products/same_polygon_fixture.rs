// SPDX-License-Identifier: GPL-3.0-or-later
//! Coordinate-consistent authored inputs, not a source compiler or admission proof.
//! Raw ADT coverage/Avian checks live in the app's real_corridor_fixture module.

use super::*;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use super::super::source_pathfinder_query::test_archive as archive;

pub(crate) const MAP: u32 = 0;
pub(crate) const DIRECTORY: &str = "Fixture";
pub(crate) const TILE: TileId = TileId { x: 31, y: 31 };
pub(crate) const FROM: [f32; 3] = [5., 5., 0.];
pub(crate) const TO: [f32; 3] = [5., 6., 0.];

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum TerrainKind {
    #[default]
    Flat,
    Hole,
    Nonflat,
}

fn chunk(magic: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut result = magic.to_vec();
    result.extend((data.len() as u32).to_le_bytes());
    result.extend(data);
    result
}

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}

fn map_dbc() -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [1, 42, 168, 9] {
        word(&mut bytes, value);
    }
    let mut row = [0; 42];
    row[0] = MAP;
    row[1] = 1;
    for value in row {
        word(&mut bytes, value);
    }
    bytes.extend(b"\0Fixture\0");
    bytes
}

fn raw_wdt() -> Vec<u8> {
    let mut bytes = chunk(b"REVM", &18_u32.to_le_bytes());
    bytes.extend(chunk(b"DHPM", &[0; 32]));
    let mut grid = vec![0; 64 * 64 * 8];
    for y in 30..=32 {
        for x in 30..=32 {
            grid[(y * 64 + x) * 8] = 1;
        }
    }
    bytes.extend(chunk(b"NIAM", &grid));
    bytes
}

/// Same raw MCNK/MCVT authoring as formats::terrain::native_coverage_tests.
/// The parser snaps these bases onto its canonical rational XY lattice.
fn raw_adt(tile: (i32, i32), kind: TerrainKind) -> Vec<u8> {
    let mut bytes = vec![];
    for row in 0..16_u32 {
        for col in 0..16_u32 {
            let mut header = vec![0; 128];
            header[4..8].copy_from_slice(&col.to_le_bytes());
            header[8..12].copy_from_slice(&row.to_le_bytes());
            header[0x14..0x18].copy_from_slice(&128_u32.to_le_bytes());
            let base = |tile, chunk| {
                ((32. - f64::from(tile)) * (1600. / 3.) - f64::from(chunk) * (100. / 3.)) as f32
            };
            header[0x68..0x6c].copy_from_slice(&base(tile.1, row).to_le_bytes());
            header[0x6c..0x70].copy_from_slice(&base(tile.0, col).to_le_bytes());
            let mut heights = vec![0; 145 * 4];
            if tile == (31, 31) && row == 15 && col == 15 {
                match kind {
                    TerrainKind::Flat => {}
                    // Both endpoints occupy the final 2x2 hole block of chunk (15,15).
                    TerrainKind::Hole => {
                        header[0x3c..0x40].copy_from_slice(&(1_u32 << 15).to_le_bytes())
                    }
                    TerrainKind::Nonflat => {
                        // Cell (6,6) includes (5,5); change its actual fan centre.
                        let center = 6 * 17 + 9 + 6;
                        heights[center * 4..center * 4 + 4]
                            .copy_from_slice(&0.125_f32.to_le_bytes());
                    }
                }
            }
            header.extend(chunk(b"TVCM", &heights));
            bytes.extend(chunk(b"KNCM", &header));
        }
    }
    bytes
}

/// Owns an authored MPQ, read through the ordinary Chain and Bevy MPQ asset reader.
/// Complete absence of placements is established by the full WDT inventory read.
pub(crate) struct RawMap {
    pub(crate) path: PathBuf,
    pub(crate) manifest: Hash,
}
impl RawMap {
    pub(crate) fn new(kind: TerrainKind) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "benilla-real-corridor-{}-{}.MPQ",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut entries = vec![
            ("DBFilesClient\\Map.dbc".to_string(), map_dbc()),
            ("World\\Maps\\Fixture\\Fixture.wdt".to_string(), raw_wdt()),
        ];
        for y in 30..=32 {
            for x in 30..=32 {
                entries.push((
                    format!("World\\Maps\\Fixture\\Fixture_{x}_{y}.adt"),
                    raw_adt((x, y), kind),
                ));
            }
        }
        let entries: Vec<_> = entries
            .iter()
            .map(|(p, b)| (p.as_str(), b.clone()))
            .collect();
        let bytes = archive::archive(&entries);
        let manifest = Sha256::digest(&bytes).into();
        fs::write(&path, bytes).unwrap();
        Self { path, manifest }
    }

    pub(crate) fn inventory(&self) -> benilla_formats::NativeM2MapInventory {
        benilla_formats::NativeM2MapInventory::load(
            &mut benilla_formats::Chain::open(&self.path).unwrap(),
            DIRECTORY,
        )
        .unwrap()
    }

    pub(crate) fn maps(&self) -> benilla_formats::MapCatalog {
        benilla_formats::load_map_catalog(&mut benilla_formats::Chain::open(&self.path).unwrap())
            .unwrap()
    }
}
impl Drop for RawMap {
    fn drop(&mut self) {
        fs::remove_file(&self.path).unwrap();
    }
}

/// Explicit empty tiled VMap: one zero-primitive leaf, and a real zero-spawn tile.
/// Its finite root encloses the fixture rays, so misses traverse the loaded leaf.
fn empty_vmap_tree() -> Vec<u8> {
    let mut bytes = b"VMAP_7.0\x01NODE".to_vec();
    let mid = 0.5_f32 * 64. * 533.333_3_f32;
    for value in [mid - 534., mid - 534., -100., mid, mid, 100.] {
        word(&mut bytes, value.to_bits());
    }
    for value in [3, 3 << 30, 0, 0, 0] {
        word(&mut bytes, value);
    }
    bytes.extend(b"GOBJ");
    bytes
}

pub(crate) fn fixture(raw: &RawMap) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, _) = tests::fixture();
    spec.map = MAP;
    spec.tiles = vec![TILE];
    spec.products.clear();
    spec.binding.raw_source_manifest = raw.manifest;
    spec.binding.raw_static_inventory = raw.inventory().source_digest();
    let mut files = BTreeMap::new();
    let mut add = |id: ProductId, bytes: Vec<u8>, dependencies| {
        spec.products.push(ProductSpec {
            id: id.clone(),
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes).into(),
            dependencies,
        });
        files.insert(id.filename(MAP).unwrap(), bytes);
    };
    // Flat GridMap at exactly the raw ADT feet height; absent MLIQ is parsed dryness.
    let mut terrain = tests::terrain_bytes(0);
    terrain[56..64].fill(0);
    add(ProductId::Terrain(TILE), terrain, vec![]);
    add(ProductId::VmapTree, empty_vmap_tree(), vec![]);
    let mut tile = b"VMAP_7.0".to_vec();
    word(&mut tile, 0);
    add(ProductId::VmapTile(TILE), tile, vec![ProductId::VmapTree]);
    add(
        ProductId::MmapParams,
        include_bytes!("../../../../../../../benilla-native-nav/tools/fixtures/direct-corridor/params.test-fixture").to_vec(),
        vec![],
    );
    add(
        ProductId::MmapTile(TILE),
        include_bytes!("../../../../../../../benilla-native-nav/tools/fixtures/warm-cache/precision.test-fixture").to_vec(),
        vec![ProductId::MmapParams, ProductId::Terrain(TILE), ProductId::VmapTile(TILE)],
    );
    (spec, files)
}

pub(crate) fn products() -> Arc<VmapLiquidProducts> {
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, files) = fixture(&raw);
    dry_route::tests::load(spec, &files)
}

pub(crate) fn height_configuration(
    products: &VmapLiquidProducts,
) -> ground_height::SourceHeightConfiguration {
    ground_height::SourceHeightConfiguration {
        map: products.inventory.map,
        content: products.inventory.content,
        source_epoch: products.inventory.source_epoch,
        effective_config: products.inventory.effective_config,
        vmap_height_enabled: true,
    }
}

/// Exact same immutable inventory profile as the source-query projection, without
/// allocating an unrelated legacy navigation query merely to obtain its VMap.
pub(crate) fn static_vmap(raw: &RawMap) -> vmap::VmapSnapshot {
    let (spec, files) = fixture(raw);
    dry_route::tests::load(spec, &files)
        .inventory
        .load_dry_liquid_and_vmap()
        .unwrap()
        .1
}

#[test]
fn raw_inventory_and_products_use_the_same_positive_grid_and_feet_height() {
    let raw = RawMap::new(TerrainKind::Flat);
    let inventory = raw.inventory();
    assert_eq!(inventory.placements().count(), 0);
    assert_eq!(inventory.wmo_placements().count(), 0);
    for y in 30..=32 {
        for x in 30..=32 {
            assert!(inventory.tile_digest((x, y)).is_some());
        }
    }
    assert_eq!(raw.maps().directory(MAP), Some(DIRECTORY));
    let (spec, files) = fixture(&raw);
    assert_eq!(spec.binding.raw_static_inventory, inventory.source_digest());
    let products = tests::load(spec, &files).unwrap();
    for point in [FROM, TO] {
        assert_eq!(benilla_formats::world_to_tile(point[0], point[1]), (31, 31));
        assert_eq!(
            products
                .terrain(MAP, TILE)
                .unwrap()
                .point_height(point[0], point[1]),
            Ok(PointHeight::Height(0.))
        );
    }
}

#[test]
fn raw_negative_variants_reach_strict_parser_as_holes_and_nonflat_vertices() {
    for kind in [TerrainKind::Flat, TerrainKind::Hole, TerrainKind::Nonflat] {
        let raw = RawMap::new(kind);
        let mut chain = benilla_formats::Chain::open(&raw.path).unwrap();
        let bytes = chain
            .read_file("World\\Maps\\Fixture\\Fixture_31_31.adt")
            .unwrap();
        let tile = benilla_formats::adt_to_tile_mesh(&bytes).unwrap();
        assert!(tile.native_static_coverage_safe);
        assert_eq!(tile.chunks.len(), 256);
        let chunk = &tile.chunks[255];
        assert_eq!((chunk.index_x, chunk.index_y), (15, 15));
        assert_eq!(chunk.positions[144], [0., 0., 0.]);
        assert_eq!(
            chunk.holes,
            if matches!(kind, TerrainKind::Hole) {
                1 << 15
            } else {
                0
            }
        );
        assert_eq!(
            chunk.positions[6 * 17 + 9 + 6][2],
            if matches!(kind, TerrainKind::Nonflat) {
                0.125
            } else {
                0.
            }
        );
    }
}

#[test]
fn absent_or_changed_product_member_does_not_become_an_empty_dependency() {
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, mut files) = fixture(&raw);
    let name = ProductId::VmapTile(TILE).filename(MAP).unwrap();
    let complete = files.remove(&name).unwrap();
    assert!(matches!(
        tests::load(spec.clone(), &files),
        Err(Error::Missing)
    ));
    let mut changed = complete;
    changed[8] = 1;
    files.insert(name, changed);
    assert!(matches!(tests::load(spec, &files), Err(Error::Hash)));
}

#[test]
fn loaded_empty_vmap_and_dry_gridmap_are_observed_not_supplied_answers() {
    use super::{
        liquid::{LiquidRequest, LiquidStatus},
        vmap::{QueryBudget, RayRequest, StaticRay},
    };
    use std::time::{Duration, Instant};
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, files) = fixture(&raw);
    let products = tests::load(spec, &files).unwrap();
    let dry = products.load_dry_liquid().unwrap();
    let vmap = products.load_static_vmap().unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    let query = LiquidRequest {
        snapshot: dry.binding(),
        ticket: 1,
        position: FROM.map(f32::to_bits),
    };
    assert_eq!(
        dry.query(query, until).unwrap().status,
        LiquidStatus::NoWater
    );
    let eye = |p: [f32; 3]| [p[0], p[1], p[2] + 2.].map(f32::to_bits);
    let request = RayRequest {
        snapshot: vmap.binding(),
        ticket: 1,
        from: eye(FROM),
        to: eye(TO),
        ignore_m2: false,
    };
    let mut budget = QueryBudget::new(until);
    let before = budget.remaining_work_for_test();
    let reply = vmap.query_with_budget(request, &mut budget).unwrap();
    assert_eq!(reply.request, request);
    assert_eq!(reply.result, StaticRay::Miss);
    assert!(
        budget.remaining_work_for_test() < before,
        "actual loaded BIH traversal spends work"
    );
}

#[test]
fn authored_nav_points_are_real_same_polygon_on_the_source1024_query() {
    use benilla_native_nav::{
        SourceMapQuery, SourceMapQuerySlot, SourceSamePolygonBudget, SourceSamePolygonConfig,
        TileInput,
    };
    use std::time::{Duration, Instant};
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, files) = fixture(&raw);
    let products = tests::load(spec, &files).unwrap();
    let slot = SourceMapQuerySlot {
        map_id: MAP,
        instance_id: 0,
    };
    let query = SourceMapQuery::load(
        slot,
        products
            .inventoried_bytes(MAP, &ProductId::MmapParams)
            .unwrap(),
        &[TileInput {
            grid: [31, 31],
            bytes: products
                .inventoried_bytes(MAP, &ProductId::MmapTile(TILE))
                .unwrap(),
        }],
    )
    .unwrap();
    assert_eq!(query.query_capacity(), 1024);
    let mut retained = query.retain_default();
    let selected = retained
        .select_for_calculate(slot, FROM.map(f32::to_bits), TO.map(f32::to_bits))
        .unwrap();
    let terminal = selected
        .loaded_cold_same_polygon(
            1,
            SourceSamePolygonConfig {
                normalize_z: false,
                ignore_normalization: false,
            },
            SourceSamePolygonBudget::default(),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    assert_eq!(
        terminal.points(),
        &[FROM.map(f32::to_bits), TO.map(f32::to_bits)]
    );
    assert_eq!(terminal.path_type(), 1);
    // No body/support/Route assertion belongs in this engine-free reader test.
}
