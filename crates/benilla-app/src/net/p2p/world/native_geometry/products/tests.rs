// SPDX-License-Identifier: GPL-3.0-or-later
// All bytes below are authored format fixtures, never extracted game assets.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn float(bytes: &mut Vec<u8>, value: f32) {
    word(bytes, value.to_bits());
}
fn replace(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn shape(k: usize, x: f32, y: f32, center: bool) -> f32 {
    match k {
        0 | 7 => 7.0,
        1 | 6 => x / 8.0 + y / 16.0,
        2 => (x - 64.0).abs() / 4.0,
        3 => ((x - 64.0) * (x - 64.0) + (y - 64.0) * (y - 64.0)) / 512.0,
        4 => {
            if x < 64.0 {
                0.0
            } else {
                20.0
            }
        }
        5 => {
            if center {
                20.0
            } else {
                0.0
            }
        }
        _ => unreachable!(),
    }
}
fn terrain_bytes(k: usize) -> Vec<u8> {
    let mut values = vec![];
    if k != 0 && k != 7 {
        for i in 0..129 {
            for j in 0..129 {
                values.push(shape(k, i as f32, j as f32, false));
            }
        }
        for i in 0..128 {
            for j in 0..128 {
                values.push(shape(k, i as f32 + 0.5, j as f32 + 0.5, true));
            }
        }
    }
    let low = values.iter().copied().reduce(f32::min).unwrap_or(7.0);
    let high = values.iter().copied().reduce(f32::max).unwrap_or(7.0);
    let height_len = 16 + values.len() as u32 * 4;
    let mut bytes = b"MAPSz1.4".to_vec();
    for value in [40, 8, 48, height_len, 0, 0, 48 + height_len, 512] {
        word(&mut bytes, value);
    }
    bytes.extend_from_slice(b"AREA\x01\x00\x00\x00MHGT");
    word(&mut bytes, u32::from(values.is_empty()));
    float(&mut bytes, low);
    float(&mut bytes, high);
    for value in values {
        float(&mut bytes, value);
    }
    let mut holes = [0u16; 256];
    if k >= 6 {
        holes[0] = 0x8001;
        holes[7 * 16 + 7] = 0x8421;
        holes[255] = 0x8000;
    }
    for hole in holes {
        bytes.extend_from_slice(&hole.to_le_bytes());
    }
    bytes
}
fn bih(bytes: &mut Vec<u8>) {
    for v in [0., 0., 0., 1., 1., 1.] {
        float(bytes, v);
    }
    for v in [3, 3 << 30, 0, 1, 1, 0] {
        word(bytes, v);
    }
}
fn tree_bytes() -> Vec<u8> {
    let mut bytes = b"VMAP_7.0\x01NODE".to_vec();
    bih(&mut bytes);
    bytes.extend_from_slice(b"GOBJ");
    bytes
}
fn vmap_tile_bytes() -> Vec<u8> {
    let mut bytes = b"VMAP_7.0".to_vec();
    word(&mut bytes, 1);
    word(&mut bytes, 5); // MOD_M2 | MOD_HAS_BOUND
    bytes.extend_from_slice(&0u16.to_le_bytes());
    word(&mut bytes, 1);
    for v in [0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 1., 1., 1.] {
        float(&mut bytes, v);
    }
    word(&mut bytes, 11);
    bytes.extend_from_slice(b"authored.m2");
    word(&mut bytes, 0);
    bytes
}
fn model_bytes() -> Vec<u8> {
    let mut bytes = b"VMAP_7.0WMOD".to_vec();
    word(&mut bytes, 8);
    word(&mut bytes, 0); // explicit empty assembled model; not a missing file
    bytes
}
fn params_bytes() -> Vec<u8> {
    let mut bytes = vec![];
    for v in [0., 0., 0., terrain::GRID_SIZE, terrain::GRID_SIZE] {
        float(&mut bytes, v);
    }
    word(&mut bytes, 4096);
    word(&mut bytes, 0x8000_0000); // exact pinned maxPolys raw bits, not a positive i32
    bytes
}
fn mmap_tile_bytes() -> Vec<u8> {
    let mut bytes = vec![];
    // One triangle, six reserved links, one detail triangle, two BV nodes, no offmesh.
    let size = 100 + 3 * 12 + 32 + 6 * 16 + 12 + 4 + 2 * 16;
    for value in [
        0x4d4d4150, 7, 8, size, 1, 0x444e4156, 7, 13, 17, 0, 0, 1, 3, 6, 1, 0, 1, 2, 0, 1,
    ] {
        word(&mut bytes, value);
    }
    for v in [2., 0.5, 0.4, 0., 0., 0., 1., 1., 1., 1.] {
        float(&mut bytes, v);
    }
    bytes.resize(20 + size as usize, 0);
    bytes
}
fn binding() -> SourceBinding {
    SourceBinding {
        source_epoch: [1; 32],
        raw_source_manifest: [2; 32],
        raw_static_inventory: [3; 32],
        content: [4; 32],
        tool_binaries: [[5; 32], [6; 32], [7; 32], [8; 32]],
        effective_config: [9; 32],
        offmesh_input: [10; 32],
        build_configuration: [11; 32],
    }
}
pub(super) fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let tiles = vec![
        TileId { x: 31, y: 30 },
        TileId { x: 32, y: 30 },
        TileId { x: 33, y: 30 },
    ];
    let model = ProductId::VmapModel("authored.m2.vmo".into());
    let mut specs = vec![];
    let mut files = BTreeMap::new();
    let mut add = |id: ProductId, data: Vec<u8>, dependencies| {
        specs.push(ProductSpec {
            id: id.clone(),
            bytes: data.len() as u64,
            sha256: Sha256::digest(&data).into(),
            dependencies,
        });
        files.insert(id.filename(0).unwrap(), data);
    };
    add(ProductId::VmapTree, tree_bytes(), vec![]);
    add(ProductId::MmapParams, params_bytes(), vec![]);
    add(model.clone(), model_bytes(), vec![]);
    for tile in &tiles {
        add(ProductId::Terrain(*tile), terrain_bytes(0), vec![]);
        add(
            ProductId::VmapTile(*tile),
            vmap_tile_bytes(),
            vec![ProductId::VmapTree, model.clone()],
        );
        add(
            ProductId::MmapTile(*tile),
            mmap_tile_bytes(),
            vec![
                ProductId::MmapParams,
                ProductId::Terrain(*tile),
                ProductId::VmapTile(*tile),
            ],
        );
    }
    (
        InventorySpec {
            map: 0,
            compatibility: Compatibility::pinned(),
            binding: binding(),
            tiles,
            products: specs,
        },
        files,
    )
}
pub(super) fn load(
    spec: InventorySpec,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<InventoriedProducts, Error> {
    InventoriedProducts::materialize(spec, |name, _| {
        files.get(name).cloned().ok_or(Error::Missing)
    })
}
fn result(spec: InventorySpec, files: &BTreeMap<String, Vec<u8>>) -> Result<(), Error> {
    load(spec, files).map(|_| ())
}
pub(super) fn mutate(
    spec: &mut InventorySpec,
    files: &mut BTreeMap<String, Vec<u8>>,
    id: &ProductId,
    f: impl FnOnce(&mut Vec<u8>),
) {
    let bytes = files.get_mut(&id.filename(spec.map).unwrap()).unwrap();
    f(bytes);
    let product = spec.products.iter_mut().find(|p| &p.id == id).unwrap();
    product.bytes = bytes.len() as u64;
    product.sha256 = Sha256::digest(bytes).into();
}

#[test]
fn complete_bundle_is_immutable_and_has_canonical_transposed_vmap_identity() {
    let (spec, mut files) = fixture();
    let snapshot = load(spec, &files).unwrap();
    assert_eq!(
        ProductId::VmapTile(TileId { x: 31, y: 30 })
            .filename(0)
            .unwrap(),
        "vmaps/000_30_31.vmtile"
    );
    assert_eq!(
        ProductId::Terrain(TileId { x: 31, y: 30 })
            .filename(0)
            .unwrap(),
        "maps/0003130.map"
    );
    assert_eq!(
        ProductId::MmapTile(TileId { x: 31, y: 30 })
            .filename(0)
            .unwrap(),
        "mmaps/0003130.mmtile"
    );
    files.clear();
    assert_eq!(
        snapshot
            .terrain(0, TileId { x: 32, y: 30 })
            .unwrap()
            .point_height(-10., 1050.)
            .unwrap(),
        PointHeight::Height(7.)
    );
    assert!(matches!(
        snapshot.terrain(1, TileId { x: 32, y: 30 }),
        Err(Error::Identity)
    ));
    assert!(matches!(
        snapshot.terrain(0, TileId { x: 32, y: 29 }),
        Err(Error::Missing)
    ));
}

#[test]
fn missing_or_corrupt_middle_tile_fails_with_valid_endpoints() {
    for id in [
        ProductId::Terrain(TileId { x: 32, y: 30 }),
        ProductId::VmapTile(TileId { x: 32, y: 30 }),
        ProductId::MmapTile(TileId { x: 32, y: 30 }),
    ] {
        let (spec, mut files) = fixture();
        let name = id.filename(0).unwrap();
        files.remove(&name);
        assert_eq!(result(spec.clone(), &files), Err(Error::Missing));
        let (_, mut files) = fixture();
        files.get_mut(&name).unwrap()[0] ^= 1;
        assert_eq!(result(spec.clone(), &files), Err(Error::Hash));
        let (mut spec, files) = fixture();
        spec.products.retain(|p| p.id != id);
        assert_eq!(result(spec, &files), Err(Error::Missing));
    }
}

#[test]
fn budgets_and_incomplete_dependencies_fail_before_any_read() {
    let (base, _) = fixture();
    for kind in 0..8 {
        let mut spec = base.clone();
        match kind {
            0 => spec.products[0].bytes = MAX_PRODUCT_BYTES + 1,
            1 => {
                for p in &mut spec.products {
                    p.bytes = MAX_PRODUCT_BYTES;
                }
            }
            2 => spec.products = vec![spec.products[0].clone(); MAX_PRODUCTS + 1],
            3 => spec.products[0].dependencies = vec![ProductId::MmapParams; MAX_EDGES + 1],
            4 => {
                spec.products.pop();
            }
            5 => spec.compatibility.poly_ref_bytes = 4,
            6 => spec.compatibility.source_pin = "moving-branch".into(),
            _ => spec.tiles = vec![spec.tiles[0]; MAX_TILES + 1],
        }
        let rejected =
            InventoriedProducts::materialize(spec, |_, _| panic!("must reject before I/O"));
        assert!(rejected.is_err());
    }
}

#[test]
fn declared_closure_rejects_cycles_duplicates_unreferenced_and_missing_models() {
    let (base, files) = fixture();
    let mut spec = base.clone();
    spec.products[0].dependencies.push(ProductId::VmapTree);
    assert_eq!(result(spec, &files), Err(Error::Dependency));
    let mut spec = base.clone();
    spec.products.push(spec.products[0].clone());
    assert_eq!(result(spec, &files), Err(Error::Duplicate));
    let mut spec = base.clone();
    spec.tiles.push(spec.tiles[0]);
    assert_eq!(result(spec, &files), Err(Error::Duplicate));
    let mut spec = base.clone();
    spec.products
        .retain(|p| !matches!(p.id, ProductId::VmapModel(_)));
    assert_eq!(result(spec, &files), Err(Error::Missing));
    let mut spec = base;
    let mut model = spec.products[2].clone();
    model.id = ProductId::VmapModel("unused.vmo".into());
    spec.products.push(model);
    assert_eq!(result(spec, &files), Err(Error::Unexpected));
}

#[test]
fn canonical_names_reject_traversal_unsupported_coordinates_and_case_aliases() {
    for name in [
        "../a.vmo",
        "a/b.vmo",
        "a\\b.vmo",
        "a..vmo",
        ".a.vmo",
        "a:stream.vmo",
        "a.VMO",
        "a\0.vmo",
    ] {
        assert_eq!(
            ProductId::VmapModel(name.into()).filename(0),
            Err(Error::Identity)
        );
    }
    assert_eq!(
        ProductId::Terrain(TileId { x: 64, y: 0 }).filename(0),
        Err(Error::Identity)
    );
    assert_eq!(ProductId::MmapParams.filename(1000), Err(Error::Identity));
    let (mut spec, files) = fixture();
    let mut duplicate = spec.products[2].clone();
    duplicate.id = ProductId::VmapModel("AUTHORED.m2.vmo".into());
    spec.products.push(duplicate);
    assert_eq!(result(spec, &files), Err(Error::Duplicate));
}

#[test]
fn revision_binds_source_configuration_expected_coverage_bytes_and_dependency_edges() {
    let (spec, files) = fixture();
    let revision = load(spec.clone(), &files).unwrap().revision();
    for field in 0..11 {
        let mut changed = spec.clone();
        match field {
            0 => changed.binding.source_epoch[0] ^= 1,
            1 => changed.binding.raw_source_manifest[0] ^= 1,
            2 => changed.binding.raw_static_inventory[0] ^= 1,
            3 => changed.binding.content[0] ^= 1,
            4..=7 => changed.binding.tool_binaries[field - 4][0] ^= 1,
            8 => changed.binding.effective_config[0] ^= 1,
            9 => changed.binding.offmesh_input[0] ^= 1,
            _ => changed.binding.build_configuration[0] ^= 1,
        }
        assert_ne!(load(changed, &files).unwrap().revision(), revision);
    }
    let mut reordered = spec.clone();
    reordered.products.reverse();
    reordered.tiles.reverse();
    for product in &mut reordered.products {
        product.dependencies.reverse();
    }
    assert_eq!(load(reordered, &files).unwrap().revision(), revision);
    let mut changed = spec.clone();
    let index = changed
        .products
        .iter()
        .position(|p| matches!(p.id, ProductId::MmapTile(_)))
        .unwrap();
    changed.products[index]
        .dependencies
        .push(ProductId::Terrain(TileId { x: 32, y: 30 }));
    assert_ne!(load(changed, &files).unwrap().revision(), revision);
    let mut changed = spec;
    let mut files = files;
    mutate(
        &mut changed,
        &mut files,
        &ProductId::VmapModel("authored.m2.vmo".into()),
        |b| replace(b, 16, 42),
    );
    assert_ne!(load(changed, &files).unwrap().revision(), revision);
}

#[test]
fn exact_native_envelopes_reject_bad_versions_flags_lengths_and_missing_model_edges() {
    let tile = TileId { x: 32, y: 30 };
    for (id, at, value, error) in [
        (ProductId::MmapTile(tile), 4, 6, Error::Format),
        (ProductId::MmapTile(tile), 8, 9, Error::Format),
        (ProductId::MmapTile(tile), 16, 2, Error::Unsupported),
        (ProductId::MmapTile(tile), 72, 1, Error::Unsupported),
        (ProductId::MmapParams, 24, 1024, Error::Format),
        (ProductId::VmapTile(tile), 12, 8, Error::Unsupported),
        (ProductId::Terrain(tile), 52, 8, Error::Unsupported),
    ] {
        let (mut spec, mut files) = fixture();
        mutate(&mut spec, &mut files, &id, |b| replace(b, at, value));
        assert_eq!(result(spec, &files), Err(error));
    }
    for id in [
        ProductId::MmapTile(tile),
        ProductId::MmapParams,
        ProductId::VmapTree,
        ProductId::VmapTile(tile),
        ProductId::VmapModel("authored.m2.vmo".into()),
        ProductId::Terrain(tile),
    ] {
        let (base, base_files) = fixture();
        for append in [false, true] {
            let mut spec = base.clone();
            let mut files = base_files.clone();
            mutate(&mut spec, &mut files, &id, |b| {
                if append {
                    b.push(0)
                } else {
                    b.truncate(b.len() - 1)
                }
            });
            assert!(result(spec, &files).is_err(), "{id:?} append={append}");
        }
    }
    let (mut spec, files) = fixture();
    spec.products
        .iter_mut()
        .find(|p| p.id == ProductId::VmapTile(tile))
        .unwrap()
        .dependencies
        .retain(|p| !matches!(p, ProductId::VmapModel(_)));
    assert_eq!(result(spec, &files), Err(Error::Dependency));
}

#[test]
fn envelope_inventory_does_not_claim_safe_detour_or_bih_loading() {
    let (spec, files) = fixture();
    let inventory = load(spec, &files).unwrap();
    // Authored Detour polygon bytes are all zero, including invalid vertex count.
    // Framing alone accepts their extent; no FFI/geometry query consumes them here.
    let bytes = inventory
        .inventoried_bytes(0, &ProductId::MmapTile(TileId { x: 32, y: 30 }))
        .unwrap();
    assert!(bytes[156..188].iter().all(|b| *b == 0));
    let authority = include_str!("../core.rs");
    assert!(!authority.contains("InventoriedProducts"));
}

#[test]
fn terrain_parser_rejects_every_truncated_prefix_and_unsupported_or_nonfinite_sections() {
    let tile = TileId { x: 32, y: 32 };
    let valid = terrain_bytes(1);
    for length in 0..valid.len() {
        assert!(TerrainTile::decode(tile, &valid[..length]).is_err());
    }
    for (at, value) in [
        (8, 41),
        (12, 0),
        (16, 40),
        (20, 16),
        (24, 64),
        (28, 16),
        (32, 48),
        (36, 0),
        (52, 2),
        (52, 4),
        (52, 8),
        (56, f32::NAN.to_bits()),
        (60, f32::INFINITY.to_bits()),
        (64, f32::NAN.to_bits()),
    ] {
        let mut bytes = valid.clone();
        replace(&mut bytes, at, value);
        assert!(
            TerrainTile::decode(tile, &bytes).is_err(),
            "at={at} value={value}"
        );
    }
    let mut bytes = valid.clone();
    bytes.push(0);
    assert!(TerrainTile::decode(tile, &bytes).is_err());
    let mut bytes = valid;
    bytes[44] = 2;
    assert!(TerrainTile::decode(tile, &bytes).is_err());
}

#[test]
fn pinned_gridmap_oracle_matches_authored_ramps_ridges_bowls_cliffs_centers_holes_and_seams() {
    let mut decoded = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("terrain-oracle.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split_whitespace().collect();
        let shape: usize = f[0].parse().unwrap();
        let tile = TileId {
            x: f[1].parse().unwrap(),
            y: f[2].parse().unwrap(),
        };
        let grid = decoded
            .entry((shape, tile))
            .or_insert_with(|| TerrainTile::decode(tile, &terrain_bytes(shape)).unwrap());
        let bits = |i| u32::from_str_radix(f[i], 16).unwrap();
        let wx = f32::from_bits(bits(3));
        let wy = f32::from_bits(bits(4));
        let height = grid.point_height(wx, wy).unwrap();
        let actual = match height {
            PointHeight::Height(h) => h.to_bits(),
            PointHeight::Hole => (-100000.0f32).to_bits(),
        };
        assert_eq!(actual, bits(5), "{line}");
        count += 1;
    }
    assert_eq!(count, 6272);
}

#[test]
fn source_flat_dispatch_ignores_holes_but_float_dispatch_rejects_them() {
    let tile = TileId { x: 32, y: 32 };
    let flat = TerrainTile::decode(tile, &terrain_bytes(7)).unwrap();
    let float = TerrainTile::decode(tile, &terrain_bytes(6)).unwrap();
    assert_eq!(flat.point_height(-1., -1.), Ok(PointHeight::Height(7.)));
    assert_eq!(float.point_height(-1., -1.), Ok(PointHeight::Hole));
    for (x, y) in [(f32::NAN, 0.), (0., f32::INFINITY), (1., -1.), (-540., -1.)] {
        assert_eq!(flat.point_height(x, y), Err(Error::OutsideTile));
    }
}

struct Stage(std::path::PathBuf);
impl Stage {
    fn new(files: &BTreeMap<String, Vec<u8>>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "benilla-authored-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        for (name, bytes) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        Self(root)
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn private_directory_load_is_all_or_nothing_rejects_extras_and_survives_source_removal() {
    let (spec, files) = fixture();
    let stage = Stage::new(&files);
    let snapshot = InventoriedProducts::load(&stage.0, spec.clone()).unwrap();
    fs::write(stage.0.join("maps/extra.map"), [0]).unwrap();
    assert_eq!(
        InventoriedProducts::load(&stage.0, spec.clone()).map(|_| ()),
        Err(Error::Unexpected)
    );
    fs::remove_file(stage.0.join("maps/extra.map")).unwrap();
    fs::remove_file(stage.0.join("maps/0003230.map")).unwrap();
    assert_eq!(
        InventoriedProducts::load(&stage.0, spec).map(|_| ()),
        Err(Error::Missing)
    );
    drop(stage);
    assert_eq!(
        snapshot
            .terrain(0, TileId { x: 32, y: 30 })
            .unwrap()
            .point_height(-10., 1050.),
        Ok(PointHeight::Height(7.))
    );
}

#[cfg(unix)]
#[test]
fn loader_rejects_symlinked_products_and_roots() {
    let (spec, files) = fixture();
    let stage = Stage::new(&files);
    let file = stage.0.join("maps/0003230.map");
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink("0003130.map", &file).unwrap();
    assert_eq!(
        InventoriedProducts::load(&stage.0, spec.clone()).map(|_| ()),
        Err(Error::Unexpected)
    );
    let alias = stage.0.join("alias");
    std::os::unix::fs::symlink(&stage.0, &alias).unwrap();
    assert_eq!(
        InventoriedProducts::load(&alias, spec).map(|_| ()),
        Err(Error::Identity)
    );
}

#[test]
fn full_area_table_and_half_open_source_grid_edge_are_explicit() {
    let mut bytes = terrain_bytes(0);
    let old_holes = u32_at(&bytes, 32).unwrap();
    bytes.splice(48..48, [0u8; 512]);
    replace(&mut bytes, 12, 520);
    replace(&mut bytes, 16, 560);
    replace(&mut bytes, 32, old_holes + 512);
    bytes[44] = 0;
    let left = TerrainTile::decode(TileId { x: 31, y: 32 }, &bytes).unwrap();
    let right = TerrainTile::decode(TileId { x: 32, y: 32 }, &bytes).unwrap();
    assert_eq!(left.point_height(0., -1.), Err(Error::OutsideTile));
    assert_eq!(right.point_height(0., -1.), Ok(PointHeight::Height(7.)));
    assert_eq!(left.point_height(1., -1.), Ok(PointHeight::Height(7.)));
    assert_eq!(right.point_height(1., -1.), Err(Error::OutsideTile));
    // A raw source sentinel remains a point value in flat mode; it is not support.
    replace(&mut bytes, 568, (-100000.0f32).to_bits());
    replace(&mut bytes, 572, (-100000.0f32).to_bits());
    let sentinel = TerrainTile::decode(TileId { x: 32, y: 32 }, &bytes).unwrap();
    assert_eq!(
        sentinel.point_height(-1., -1.),
        Ok(PointHeight::Height(-100000.))
    );
}

#[test]
fn mmap_count_limits_do_not_confuse_signed_raw_max_polys_with_array_counts() {
    let tile = TileId { x: 32, y: 30 };
    let (spec, files) = fixture();
    assert!(load(spec.clone(), &files).is_ok()); // maxPolys 0x80000000 is required
    for (id, offset, value) in [
        (ProductId::MmapParams, 20, 0),
        (ProductId::MmapParams, 20, 4097),
        (ProductId::MmapParams, 20, 0x80000000),
        (ProductId::MmapParams, 24, 0),
        (ProductId::MmapTile(tile), 44, 0),
        (ProductId::MmapTile(tile), 44, 0x80000000),
        (ProductId::MmapTile(tile), 48, 65535),
        (ProductId::MmapTile(tile), 52, u32::MAX),
        (ProductId::MmapTile(tile), 56, u32::MAX),
    ] {
        let mut spec = spec.clone();
        let mut files = files.clone();
        mutate(&mut spec, &mut files, &id, |b| replace(b, offset, value));
        assert!(
            result(spec, &files).is_err(),
            "{id:?} offset={offset} value={value}"
        );
    }
}

fn nonempty_model_bytes() -> Vec<u8> {
    let mut bytes = model_bytes();
    bytes.extend_from_slice(b"GMOD");
    word(&mut bytes, 1);
    for v in [0., 0., 0., 1., 1., 1.] {
        float(&mut bytes, v);
    }
    word(&mut bytes, 0);
    word(&mut bytes, 0); // opaque MOGP flags/group ID
    bytes.extend_from_slice(b"VERT");
    word(&mut bytes, 40);
    word(&mut bytes, 3);
    for v in [0., 0., 0., 1., 0., 0., 0., 1., 0.] {
        float(&mut bytes, v);
    }
    bytes.extend_from_slice(b"TRIM");
    word(&mut bytes, 16);
    word(&mut bytes, 1);
    for v in [0, 1, 2] {
        word(&mut bytes, v);
    }
    bytes.extend_from_slice(b"MBIH");
    bih(&mut bytes);
    bytes.extend_from_slice(b"LIQU");
    word(&mut bytes, 0);
    bytes.extend_from_slice(b"GBIH");
    bih(&mut bytes);
    bytes
}

#[test]
fn nonempty_vmap_group_frames_and_indices_are_bounded_without_native_loads() {
    let model = ProductId::VmapModel("authored.m2.vmo".into());
    let (mut spec, mut files) = fixture();
    mutate(&mut spec, &mut files, &model, |b| {
        *b = nonempty_model_bytes()
    });
    assert!(result(spec.clone(), &files).is_ok());
    let model_data = nonempty_model_bytes();
    assert_eq!(model_data.len(), 244);
    for length in 0..model_data.len() {
        // Exactly 20 bytes is separately the source's explicit empty model format.
        if length != 20 {
            assert!(envelopes::validate(&model, &model_data[..length]).is_err());
        }
    }
    for (offset, value) in [
        (64, 41),
        (68, u32::MAX),
        (72, f32::NAN.to_bits()),
        (112, 17),
        (120, 3),
        (140, f32::INFINITY.to_bits()),
        (160, u32::MAX),
        (188, 1),
    ] {
        let mut spec = spec.clone();
        let mut files = files.clone();
        mutate(&mut spec, &mut files, &model, |b| replace(b, offset, value));
        assert!(
            result(spec, &files).is_err(),
            "offset={offset} value={value}"
        );
    }
    mutate(&mut spec, &mut files, &model, |b| b.push(0));
    assert!(result(spec, &files).is_err());
}

#[test]
fn filesystem_metadata_rejects_oversized_sparse_product_without_reading_it() {
    let (mut spec, files) = fixture();
    let stage = Stage::new(&files);
    let path = stage.0.join("maps/0003230.map");
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(MAX_PRODUCT_BYTES + 1)
        .unwrap();
    assert_eq!(
        InventoriedProducts::load(&stage.0, spec.clone()).map(|_| ()),
        Err(Error::Framing)
    );
    spec.products[0].bytes = MAX_PRODUCT_BYTES + 1;
    assert_eq!(
        InventoriedProducts::load(&stage.0.join("absent"), spec).map(|_| ()),
        Err(Error::Budget)
    );
}
