// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_native_vmap::live::{FileState, LiquidOutput};
use benilla_sim::source_rng::{DrawCursor, Mt19937};
use std::time::{Duration, Instant};

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../benilla-native-geometry-tests/tools/fixtures")
}
fn bytes(path: &str) -> Vec<u8> {
    std::fs::read(root().join(path)).unwrap()
}
fn rows(path: &str) -> Vec<Vec<u32>> {
    let b = bytes(path);
    assert_eq!(&b[..4], b"WDBC");
    let word = |i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as usize;
    let (count, width) = (word(4), word(12));
    b[20..20 + count * width]
        .chunks_exact(width)
        .map(|r| {
            r.as_chunks::<4>()
                .0
                .iter()
                .map(|w| u32::from_le_bytes(*w))
                .collect()
        })
        .collect()
}
struct Stores {
    liquid: SourceLiquidDbc,
    areas: SourceTerrainDbc,
    wmo: vmap_scene::SourceWmoAreas,
}
fn stores(prefix: &str) -> Stores {
    let l = rows(&format!("{prefix}/LiquidType.dbc.test-fixture"));
    let a = rows(&format!("{prefix}/AreaTable.dbc.test-fixture"));
    let m = rows(&format!("{prefix}/Map.dbc.test-fixture"));
    let w = rows(&format!("{prefix}/WMOAreaTable.dbc.test-fixture"));
    Stores {
        liquid: SourceLiquidDbc {
            types: Some(l.into_iter().map(|r| (r[0], r[2])).collect()),
            areas: Some(
                a.iter()
                    .map(|r| {
                        (
                            r[0],
                            Area {
                                zone: r[2],
                                liquid_override: r[24],
                            },
                        )
                    })
                    .collect(),
            ),
        },
        areas: SourceTerrainDbc {
            areas: a
                .into_iter()
                .map(|r| {
                    (
                        r[0],
                        SourceAreaEntry {
                            id: r[0],
                            map: r[1],
                            zone: r[2],
                            flag: r[3],
                        },
                    )
                })
                .collect(),
            linked_zones: m.into_iter().map(|r| (r[0], r[19])).collect(),
        },
        wmo: vmap_scene::SourceWmoAreas::from_rows(w.into_iter().map(|r| {
            vmap_scene::SourceWmoAreaRow {
                id: r[0],
                root: r[1] as i32,
                adt: r[2] as i32,
                group: r[3] as i32,
                flags: r[9],
                area: r[10],
            }
        })),
    }
}
fn scene(dataset: &str, bound: bool) -> Result<SourceVMapManager> {
    let directory = if dataset.starts_with("terrain-wet-vmap") {
        dataset.to_owned()
    } else {
        format!("vmap_source/{dataset}")
    };
    let mut scene = SourceVMapManager::default();
    if bound {
        let receipt = rotation_receipt();
        scene.bind_rotation_dispatch(14, &receipt.source_acquisition_bindings, &receipt)?;
    }
    for entry in std::fs::read_dir(root().join(directory)).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            scene
                .collision
                .files
                .observe(
                    entry.file_name().as_encoded_bytes(),
                    FileState::Present(std::fs::read(entry.path()).unwrap().into()),
                )
                .unwrap();
        }
    }
    scene.load_map(0, 32, 32)?;
    Ok(scene)
}
fn rotation_receipt() -> benilla_sim::source_sine::ProcessReceipt {
    use benilla_sim::source_trig_fma as fma;
    let mut receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../benilla-sim/tools/fixtures/source_sine/process-environment.json"
    ))
    .unwrap();
    receipt["resolved_functions_before_gameplay"] = serde_json::json!([
        {"function":"sinf", "module_sha256":fma::LIBM_SHA256, "relative_offset":fma::SINF_OFFSET},
        {"function":"cosf", "module_sha256":fma::LIBM_SHA256, "relative_offset":fma::COSF_OFFSET}
    ]);
    serde_json::from_value(receipt).unwrap()
}

#[test]
fn runtime_rotation_binding_preserves_named_refusal_at_round13_and_binds_round14() {
    let receipt = rotation_receipt();
    let mut manager = SourceVMapManager::default();
    assert_eq!(
        manager
            .bind_rotation_dispatch(13, &receipt.source_acquisition_bindings, &receipt)
            .unwrap_err()
            .id,
        "execution.vmap.glibc236_fma_rotation_binding"
    );
    assert_eq!(
        scene("scopedliquid", false).err().unwrap().id,
        "execution.vmap.glibc236_fma_rotation_binding"
    );
    manager
        .bind_rotation_dispatch(14, &receipt.source_acquisition_bindings, &receipt)
        .unwrap();
    assert!(
        scene("scopedliquid", true)
            .unwrap()
            .collision
            .loaded_trees()
            > 0
    );
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
/// Query-only seam, like the existing C++ oracle: grids are already loaded.
/// Production TerrainInfo still refuses its unowned full GetGrid/MMap operation.
struct Loaded<'a> {
    grid: &'a SourceGridMap,
    stores: &'a Stores,
    scene: &'a SourceVMapManager,
    map: u32,
}
impl Lookup for Loaded<'_> {
    type Error = Refusal;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>> {
        self.stores.liquid.liquid_type(id)
    }
    fn area(&self, id: u32) -> Result<Option<Area>> {
        self.stores.liquid.area(id)
    }
}
impl TerrainOwner for Loaded<'_> {
    fn can_check(&mut self, _: [f32; 3]) -> Result<bool> {
        Ok(true)
    }
    fn height_static(&mut self, p: [f32; 3]) -> Result<f32> {
        grid_height::select_height(
            self.grid.height(p[0], p[1])?,
            p[2],
            true,
            true,
            50.0,
            |z, search| {
                self.scene
                    .collision
                    .height(self.map, [p[0], p[1], z], search, deadline())
                    .map_err(vmap_manager::vmap_error)
            },
        )
    }
    fn vmap_liquid(&mut self, p: [f32; 3], mask: u8, out: &mut LiquidData) -> Result<bool> {
        let mut raw = LiquidOutput {
            level: out.level,
            floor: out.depth_level,
            kind: out.entry,
        };
        let found = self
            .scene
            .collision
            .liquid_level(self.map, p, mask, &mut raw, deadline())
            .map_err(vmap_manager::vmap_error)?;
        out.level = raw.level;
        out.depth_level = raw.floor;
        out.entry = raw.kind;
        Ok(found)
    }
    fn liquid_area(&mut self, p: [f32; 3]) -> Result<Option<Area>> {
        let mut flag = None;
        if let Some(info) = self.scene.area_info(self.map, p)? {
            if !vmap_scene::area_hidden_by_terrain(p[2], self.grid.height(p[0], p[1])?, info.floor)
            {
                flag = self
                    .stores
                    .wmo
                    .flag(info, &self.stores.areas.areas, self.map)?;
            }
        }
        let flag = flag.map_or_else(|| self.grid.area(p[0], p[1]), Ok)?;
        self.stores
            .areas
            .entry(self.map, flag)
            .map(|a| self.area(a.id))
            .transpose()
            .map(Option::flatten)
    }
    fn grid_liquid(
        &mut self,
        p: [f32; 3],
        mask: u8,
        collision: f32,
        out: &mut LiquidData,
    ) -> Result<Status> {
        self.grid
            .liquid_status(p, mask, collision, Some(out), &self.stores.liquid)
    }
}
fn sentinel() -> LiquidData {
    LiquidData {
        entry: 0xdeadbeef,
        type_flags: 0xdecafbad,
        level: f32::from_bits(0x4a123456),
        depth_level: f32::from_bits(0xca654321),
    }
}
fn output(data: LiquidData) -> (u32, u32, u32, u32) {
    (
        data.entry,
        data.type_flags,
        data.level.to_bits(),
        data.depth_level.to_bits(),
    )
}

#[test]
fn existing_wet_oracle_matches_all_grid_rows_and_every_rotation_bound_composition() {
    compare_composition(false);
}
#[test]
fn bound_rotation_matches_every_withheld_liquid_composition_row() {
    compare_composition(true);
}
fn compare_composition(bound: bool) {
    let cases = include_str!(
        "../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-wet-liquid.tsv"
    );
    let stores: Vec<_> = (0..5)
        .map(|i| stores(&format!("terrain-wet-liquid/store{i}")))
        .collect();
    let grids: Vec<_> = (0..26)
        .map(|i| {
            SourceGridMap::load(Some(
                bytes(&format!("terrain-wet-liquid/{i}.map.test-fixture")).into(),
            ))
            .unwrap()
        })
        .collect();
    let scenes: BTreeMap<_, _> = [
        "terrain-wet-vmap",
        "terrain-wet-vmap/reverse",
        "scopedliquid",
        "scopedarea/forward",
        "scopedarea/reverse",
    ]
    .into_iter()
    .map(|s| (s, scene(s, bound)))
    .collect();
    let (mut count, mut rejects, mut fallback, mut nulls) = (0, 0, 0, 0);
    let mut refused = 0;
    for line in cases
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let r: Vec<_> = line.split('\t').collect();
        let n = |i: usize| r[i].parse::<u32>().unwrap();
        let b = |i: usize| u32::from_str_radix(r[i], 16).unwrap();
        let p = [5, 6, 7].map(|i| f32::from_bits(b(i)));
        let grid = &grids[n(2) as usize];
        let stores = &stores[n(1) as usize];
        let mut raw = sentinel();
        let status = grid
            .liquid_status(
                p,
                n(9) as u8,
                f32::from_bits(b(8)),
                Some(&mut raw),
                &stores.liquid,
            )
            .unwrap();
        assert_eq!(status as u32, n(21), "grid: {line}");
        assert_eq!(
            output(raw),
            (n(22), n(23), b(24), b(25)),
            "grid out: {line}"
        );
        let scene = match &scenes[r[0]] {
            Ok(scene) => scene,
            Err(error) => {
                assert_eq!(error.id, "execution.vmap.glibc236_fma_rotation_binding");
                refused += 1;
                continue;
            }
        };
        let mut world = Loaded {
            grid,
            stores,
            scene,
            map: n(4),
        };
        assert_eq!(
            world.height_static(p).unwrap().to_bits(),
            b(16),
            "height: {line}"
        );
        let mut vmap = LiquidData {
            level: grid_height::INVALID_HEIGHT_VALUE,
            depth_level: world.height_static(p).unwrap(),
            ..LiquidData::default()
        };
        let wet = world.vmap_liquid(p, n(9) as u8, &mut vmap).unwrap();
        assert_eq!(
            (
                wet,
                vmap.level.to_bits(),
                vmap.depth_level.to_bits(),
                vmap.entry
            ),
            (n(17) != 0, b(18), b(19), n(20)),
            "vmap: {line}"
        );
        let mut out = sentinel();
        let status = liquid::terrain_status(
            &mut world,
            p,
            n(9) as u8,
            f32::from_bits(b(8)),
            (n(10) != 0).then_some(&mut out),
        )
        .unwrap();
        assert_eq!(status as u32, n(11), "status: {line}");
        assert_eq!(output(out), (n(12), n(13), b(14), b(15)), "out: {line}");
        rejects += usize::from(n(17) == 1 && n(11) == 0);
        fallback += usize::from(n(17) == 0 && n(11) != 0);
        nulls += usize::from(n(10) == 0);
        count += 1;
    }
    let expected = if bound {
        (27566, 705, 9557, 260, 0)
    } else {
        (22511, 555, 8024, 155, 5055)
    };
    assert_eq!((count, rejects, fallback, nulls, refused), expected);
    println!("LIQUID_COMPOSITION bound={bound} matched={count} refused={refused}");
}

#[test]
fn wrappers_preserve_above_water_shallow_swim_and_failed_water_z_outputs() {
    let scene = scene("terrain-wet-vmap", false).unwrap();
    let stores = stores("terrain-wet-liquid/store0");
    let grid =
        SourceGridMap::load(Some(bytes("terrain-wet-liquid/0.map.test-fixture").into())).unwrap();
    let mut world = Loaded {
        grid: &grid,
        stores: &stores,
        scene: &scene,
        map: 0,
    };
    let xy = f32::from_bits(0x46855555);
    let x = xy - 20.0;
    let mut data = sentinel();
    assert!(liquid::is_in_water(&mut world, [x, xy, 8.0], Some(&mut data)).unwrap());
    assert_eq!(data.level, 5.0);
    for (radius, expected) in [
        (f32::from_bits(5.0f32.to_bits() - 1), true),
        (5.0, false),
        (f32::from_bits(5.0f32.to_bits() + 1), false),
    ] {
        assert_eq!(
            liquid::is_swimmable(&mut world, [x, xy, 4.0], radius, None).unwrap(),
            expected
        );
    }
    let mut z = 123.0;
    assert!(!liquid::is_under_water(&mut world, [x, xy, 8.0], Some(&mut z)).unwrap());
    assert_eq!(z, 123.0);
    assert!(liquid::is_under_water(&mut world, [x, xy, 0.0], Some(&mut z)).unwrap());
    assert_eq!(z, 5.0);
    for (minimum, expected) in [
        (
            f32::from_bits(5.0f32.to_bits() - 1),
            f32::from_bits(0x35000000),
        ),
        (5.0, 0.0),
        (f32::from_bits(5.0f32.to_bits() + 1), 0.0),
    ] {
        assert_eq!(
            liquid::water_or_ground(&mut world, [x, xy, 900.0], 0.0, true, minimum)
                .unwrap()
                .to_bits(),
            expected.to_bits()
        );
    }
    let mut ground = 123.0;
    assert_eq!(
        liquid::water_level(&mut world, [x, xy, 4.0], Some(&mut ground)).unwrap(),
        5.0
    );
    assert_eq!(ground, 0.0);
}

#[test]
fn runtime_stops_at_actual_full_grid_owner_before_any_liquid_store_lookup() {
    let mut files = SourceTerrainFiles::default();
    files.bytes(0, 32, 32, bytes("terrain-wet-liquid/0.map.test-fixture"));
    let mut manager = SourceTerrainManager::new(files, SourceTerrainDbc::default());
    manager
        .vmap
        .collision
        .files
        .observe(b"000.vmtree", FileState::Absent)
        .unwrap();
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    manager.load_terrain(0, &mut random).unwrap();
    let mut out = sentinel();
    assert_eq!(
        manager
            .liquid_status(0, [0.0; 3], 0, 2.0, Some(&mut out))
            .unwrap_err()
            .id,
        "execution.mmap.load_map_without_mesh"
    );
    assert_eq!(out, sentinel());
    let grid = manager.terrain(0).unwrap().grid(32, 32).unwrap();
    assert_eq!(
        grid.liquid_status([0.0; 3], 0, 2.0, Some(&mut out), &manager.liquid_dbc)
            .unwrap_err()
            .id,
        "execution.terrain.LiquidType_dbc_owner"
    );
    assert_eq!(out, sentinel());
}

#[test]
fn existing_dry_vmap_terrain_oracle_raw_grid_rows_match_complete_stores() {
    let types = rows("terrain-liquid/LiquidType.dbc.test-fixture");
    let areas = rows("terrain-liquid/AreaTable.dbc.test-fixture");
    let stores = SourceLiquidDbc {
        types: Some(types.into_iter().map(|r| (r[0], r[2])).collect()),
        areas: Some(
            areas
                .into_iter()
                .map(|r| {
                    (
                        r[0],
                        Area {
                            zone: r[2],
                            liquid_override: r[24],
                        },
                    )
                })
                .collect(),
        ),
    };
    let grids: Vec<_> = (0..29)
        .map(|i| {
            SourceGridMap::load(Some(
                bytes(&format!("terrain-liquid/{i}.map.test-fixture")).into(),
            ))
            .unwrap()
        })
        .collect();
    let cases = include_str!(
        "../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid.tsv"
    );
    let mut count = 0;
    for line in cases
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let r: Vec<_> = line.split('\t').collect();
        let n = |i: usize| r[i].parse::<u32>().unwrap();
        let b = |i: usize| u32::from_str_radix(r[i], 16).unwrap();
        let p = [2, 3, 4].map(|i| f32::from_bits(b(i)));
        let mut out = sentinel();
        let grid = &grids[n(0) as usize];
        assert_eq!(grid.height(p[0], p[1]).unwrap().to_bits(), b(22));
        let status = grid
            .liquid_status(p, n(6) as u8, f32::from_bits(b(5)), Some(&mut out), &stores)
            .unwrap();
        assert_eq!(status as u32, n(17), "{line}");
        assert_eq!(output(out), (n(18), n(19), b(20), b(21)), "{line}");
        count += 1;
    }
    assert_eq!(count, 5048);
}

#[test]
fn complete_private_catalog_binds_liquid_and_area_operands_without_new_extraction() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        println!("SKIP F6 liquid catalog: bind BENILLA_F2_CATALOG");
        return;
    };
    let mut catalog = super::super::super::super::production_compiler::VerifiedCatalog::open(
        Path::new(&root),
        false,
    )
    .unwrap();
    let stores = SourceLiquidDbc::catalog(&mut catalog).unwrap();
    let types = stores.types.as_ref().unwrap();
    let areas = stores.areas.as_ref().unwrap();
    assert!(!types.is_empty());
    assert!(!areas.is_empty());
    assert_eq!(types.values().filter(|&&kind| kind >= 32).count(), 0);
    println!(
        "F6 liquid catalog: {} LiquidType rows, {} AreaTable rows, zero undefined shift kinds",
        types.len(),
        areas.len()
    );
}
