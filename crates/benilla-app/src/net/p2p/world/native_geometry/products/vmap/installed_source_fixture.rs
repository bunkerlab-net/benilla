// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only dual-parser input: nine canonical dry guards and one exterior WMO.
//! Captured source products and the complete raw inventory are built independently.
use super::super::{
    nav_generation::{GeneratedNavigation, PreparedNavigationInput},
    TileId,
};
use super::captured_map_tests::{self as captured, archive::raw_entry};
use benilla_formats::{Chain, NativeM2MapInventory};
use benilla_native_terrain::{PreparedTerrain, Tile};
use benilla_native_vmap_source::{AssembledSourceMap, DoodadProjectionProfile};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

pub(crate) const MAP_DIRECTORY: &str = "Selected";
pub(crate) const CENTER: (u32, u32) = (40, 40);
pub(crate) const HEIGHT: f32 = -50.;
pub(crate) const WDT: &str = "World\\Maps\\Selected\\Selected.wdt";
pub(crate) const EXTERIOR_ADT: &str = "World\\Maps\\Selected\\Selected_30_31.adt";

pub(crate) fn guard_tiles() -> Vec<(u32, u32)> {
    (39..=41)
        .flat_map(|y| (39..=41).map(move |x| (x, y)))
        .collect()
}

pub(crate) fn guard_source_tiles() -> Vec<Tile> {
    guard_tiles()
        .into_iter()
        .map(|(x, y)| Tile {
            x: y as u8,
            y: x as u8,
        })
        .collect()
}

pub(crate) fn support() -> ([f32; 3], [f32; 3]) {
    ([-4276., -4276., HEIGHT], [-4280., -4276., HEIGHT])
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn put_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Preserve the complete source MVER/MHDR/MCIN/MCVT envelope and relative offsets.
/// Source extraction uses MCIN; renderer placement uses each actual MCNK header.
fn guard_adt(tile: (u32, u32)) -> Vec<u8> {
    let mut bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures/flat.adt.test-fixture"),
    )
    .unwrap();
    for slot in 0..256 {
        let header = word(&bytes, 92 + slot * 16) as usize + 8;
        let column = word(&bytes, header + 4);
        let row = word(&bytes, header + 8);
        let base = |tile, chunk| {
            ((32. - f64::from(tile)) * (1600. / 3.) - f64::from(chunk) * (100. / 3.)) as f32
        };
        put_word(&mut bytes, header + 0x68, base(tile.1, row).to_bits());
        put_word(&mut bytes, header + 0x6c, base(tile.0, column).to_bits());
        put_word(&mut bytes, header + 0x70, HEIGHT.to_bits());
    }
    bytes
}

pub(crate) fn entries() -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<_> = captured::inventory_compatibility_tests::compatible_entries()
        .into_iter()
        .map(|entry| (entry.name, entry.bytes))
        .collect();
    let wdt = &mut entries.iter_mut().find(|(name, _)| name == WDT).unwrap().1;
    assert_eq!(&wdt[52..56], b"NIAM");
    wdt[60..60 + 64 * 64 * 8].fill(0);
    for (x, y) in guard_tiles().into_iter().chain([(30, 31)]) {
        put_word(wdt, 60 + (y * 64 + x) as usize * 8, 1);
    }
    for tile in guard_tiles() {
        entries.push((
            format!("World\\Maps\\Selected\\Selected_{}_{}.adt", tile.0, tile.1),
            guard_adt(tile),
        ));
    }
    entries
}

/// A bounded steep terrain feature changes the genuine flat-tile Exact625 result.
/// Its canonical fan vertices remain dry and far outside the support footprint.
pub(crate) fn navigation_entries() -> Vec<(String, Vec<u8>)> {
    let mut entries = entries();
    let adt = &mut entries
        .iter_mut()
        .find(|(name, _)| name == "World\\Maps\\Selected\\Selected_40_40.adt")
        .unwrap()
        .1;
    for slot in 0..256 {
        let start = word(adt, 92 + slot * 16) as usize;
        let header = start + 8;
        let column = word(adt, header + 4);
        let row = word(adt, header + 8);
        let heights = start + word(adt, header + 0x14) as usize + 8;
        assert_eq!(&adt[heights - 8..heights - 4], b"TVCM");
        let value = |x: f32, y: f32| {
            let distance = (x - 64.).abs().max((y - 64.).abs());
            80. * (1. - distance / 6.).max(0.)
        };
        for local_y in 0..9 {
            for local_x in 0..9 {
                let height = value((column * 8 + local_x) as f32, (row * 8 + local_y) as f32);
                put_word(
                    adt,
                    heights + (local_y * 17 + local_x) as usize * 4,
                    height.to_bits(),
                );
            }
            if local_y < 8 {
                for local_x in 0..8 {
                    let height = value(
                        (column * 8 + local_x) as f32 + 0.5,
                        (row * 8 + local_y) as f32 + 0.5,
                    );
                    put_word(
                        adt,
                        heights + (local_y * 17 + 9 + local_x) as usize * 4,
                        height.to_bits(),
                    );
                }
            }
        }
    }
    entries
}

pub(crate) fn navigation_install() -> Install {
    Install::new(&navigation_entries())
}

/// Generate against the caller's exact source owners, then prove loaded endpoints.
pub(crate) fn with_navigation<T>(
    terrain: &PreparedTerrain,
    bundle: &AssembledSourceMap<'_>,
    test: impl FnOnce(&PreparedNavigationInput<'_, '_, '_>, &GeneratedNavigation) -> T,
) -> T {
    let input = PreparedNavigationInput::from_source(
        terrain,
        bundle,
        TileId {
            x: CENTER.1 as u8,
            y: CENTER.0 as u8,
        },
        || false,
    )
    .unwrap();
    let mut helper = std::env::current_exe().unwrap();
    helper.pop();
    helper.pop();
    helper.push(format!(
        "benilla-native-navigation{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        helper.is_file(),
        "build benilla-native-nav-generator before scoped tests"
    );
    let output = input
        .generate(&helper, Instant::now() + Duration::from_secs(60), || false)
        .unwrap();
    assert_eq!(
        output.summary().status,
        benilla_native_nav_generator::GenerationStatus::Generated
    );
    let (from, to) = support();
    let route = output
        .load()
        .unwrap()
        .route(
            from.map(f32::to_bits),
            to.map(f32::to_bits),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert!(route.start.over && route.end.over);
    assert_eq!(route.start.point[..2], from.map(f32::to_bits)[..2]);
    assert_eq!(route.end.point[..2], to.map(f32::to_bits)[..2]);
    assert_eq!(route.start.polygon, route.end.polygon);
    assert_eq!(route.polygons, [route.start.polygon]);
    test(&input, &output)
}

/// Move the genuine exterior placement and its declared source bounds into the guard.
/// Its declaring ADT stays exterior; consumers must inspect transformed geometry.
pub(crate) fn overlapping_entries() -> Vec<(String, Vec<u8>)> {
    let mut entries = entries();
    let adt = &mut entries
        .iter_mut()
        .find(|(name, _)| name == EXTERIOR_ADT)
        .unwrap()
        .1;
    let placement = 20 + word(adt, 20 + 32) as usize + 8;
    let middle = 0.5_f32 * 64. * 533.333_3_f32;
    let target = [middle + 4276., HEIGHT - 1., middle + 4278.];
    for (axis, target) in target.into_iter().enumerate() {
        let old = f32::from_bits(word(adt, placement + 8 + axis * 4));
        let delta = target - old;
        for start in [8, 32, 44] {
            let at = placement + start + axis * 4;
            let value = f32::from_bits(word(adt, at)) + delta;
            put_word(adt, at, value.to_bits());
        }
    }
    entries
}

pub(crate) struct Install(captured::Install);
impl Install {
    pub(crate) fn new(entries: &[(String, Vec<u8>)]) -> Self {
        let entries: Vec<_> = entries
            .iter()
            .map(|(name, bytes)| raw_entry(name, bytes.clone()))
            .collect();
        Self(captured::Install::new(&entries))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0 .0
    }

    pub(crate) fn bytes(&self) -> BTreeMap<String, Vec<u8>> {
        self.0.bytes()
    }

    pub(crate) fn write(&self, entries: &[(String, Vec<u8>)]) {
        let entries: Vec<_> = entries
            .iter()
            .map(|(name, bytes)| raw_entry(name, bytes.clone()))
            .collect();
        self.0.write(&entries);
    }
}

pub(crate) fn install() -> Install {
    Install::new(&entries())
}

pub(crate) fn raw_inventory(install: &Install) -> NativeM2MapInventory {
    NativeM2MapInventory::load(&mut Chain::open(install.path()).unwrap(), MAP_DIRECTORY)
        .expect("complete strict ten-tile inventory")
}

pub(crate) fn profile() -> DoodadProjectionProfile {
    captured::profile()
}

#[test]
fn authored_navigation_guard_generates_and_preserves_complete_raw_sources() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let install = navigation_install();
    let before = install.bytes();
    let inventory = raw_inventory(&install);
    assert_eq!(inventory.wmo_placements().count(), 1);
    assert_eq!(inventory.placements().count(), 0);
    let mut chain = Chain::open(install.path()).unwrap();
    let mut raised = 0;
    for (x, y) in guard_tiles() {
        let bytes = chain
            .read_file(&format!("World\\Maps\\Selected\\Selected_{x}_{y}.adt"))
            .unwrap();
        let raw = benilla_formats::adt_to_tile_mesh(&bytes).unwrap();
        assert!(raw.native_static_coverage_safe && raw.native_coverage_safe);
        assert_eq!(raw.chunks.len(), 256);
        assert!(raw.doodads.is_empty() && raw.wmos.is_empty());
        for chunk in raw.chunks {
            assert_eq!(chunk.holes, 0);
            assert!(chunk.liquids.is_empty());
            for point in chunk.positions {
                if point[2] != HEIGHT {
                    assert_eq!((x, y), CENTER);
                    assert!((HEIGHT..=HEIGHT + 80.).contains(&point[2]));
                    assert!((0..2).all(|axis| (point[axis] - support().0[axis]).abs() > 200.));
                    raised += 1;
                }
            }
        }
    }
    assert!(raised > 0);
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &guard_source_tiles()).unwrap();
    let capture = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(terrain.tiles().count(), 9);
    assert_eq!(bundle.models().len(), 1);
    let height =
        super::super::source_height::PreparedSourceHeight::from_source(&terrain, &bundle, || false)
            .unwrap();
    height
        .match_raw_inventory(&inventory, Instant::now() + Duration::from_secs(30), || {
            false
        })
        .unwrap();
    for (index, point) in [support().0, support().1].into_iter().enumerate() {
        let reply = height
            .query(
                super::super::source_height::HeightRequest {
                    binding: height.binding(),
                    ticket: index as u64 + 1,
                    position: point.map(f32::to_bits),
                    max_search: 10_f32.to_bits(),
                },
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(reply.height, HEIGHT.to_bits());
    }
    with_navigation(&terrain, &bundle, |input, _| {
        input.verify_unchanged(|| false).unwrap()
    });
    assert_eq!(install.bytes(), before);
}
