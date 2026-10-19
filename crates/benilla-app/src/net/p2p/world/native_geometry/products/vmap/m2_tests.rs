// SPDX-License-Identifier: GPL-3.0-or-later
// Original authored M2 inputs passed through the exact pinned extractor/assembler.
use super::super::{tests as product_tests, InventorySpec, ProductSpec};
use super::*;
use std::{ops::Range, time::Duration};

const TREE: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/products/000.vmtree"
);
const TILE: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/products/000_32_32.vmtile"
);
const MODELS: [(&str, &[u8]); 2] = [
    (
        "authored.m2.vmo",
        include_bytes!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/products/authored.m2.vmo"
        ),
    ),
    (
        "signed-zero.m2.vmo",
        include_bytes!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/products/signed-zero.m2.vmo"
        ),
    ),
];

const RAW_MODELS: [&[u8]; 2] = [
    include_bytes!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.raw.vmap"
    ),
    include_bytes!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/signed-zero-m2.raw.vmap"
    ),
];

fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = product_tests::fixture();
    spec.products
        .retain(|p| !matches!(p.id, ProductId::VmapModel(_)));
    files.remove("vmaps/authored.m2.vmo");
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |b| {
        *b = TREE.to_vec()
    });
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            MODELS
                .iter()
                .map(|(name, _)| ProductId::VmapModel((*name).into())),
        )
        .collect();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |b| *b = TILE.to_vec());
        spec.products
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
    }
    for ((name, expected), raw) in MODELS.into_iter().zip(RAW_MODELS) {
        // Exercise the Rust raw loader, model framing and BIHs through the real
        // consumer/query path while retaining independent original-byte checks.
        let bytes = benilla_native_vmap::assemble_raw_model(raw, || false).unwrap();
        assert_eq!(bytes, expected, "{name}");
        let id = ProductId::VmapModel(name.into());
        files.insert(id.filename(0).unwrap(), bytes.to_vec());
        spec.products.push(ProductSpec {
            id,
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes).into(),
            dependencies: vec![],
        });
    }
    (spec, files)
}

pub(super) fn snapshot() -> VmapSnapshot {
    let (spec, files) = fixture();
    product_tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap()
}

fn budget() -> QueryBudget {
    QueryBudget::new(Instant::now() + Duration::from_secs(10))
}

fn bits(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}

fn number(value: &str) -> f32 {
    f32::from_bits(bits(value))
}

#[test]
fn exact_pinned_extractor_and_assembler_m2_rays_match_source_bits() {
    let snapshot = snapshot();
    let mut count = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/rays.tsv"
    ).lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        let ray = Ray {
            origin: [number(row[2]), number(row[3]), number(row[4])],
            direction: [number(row[5]), number(row[6]), number(row[7])],
        };
        let mut distance = number(row[8]);
        let ignore_m2 = row[9] == "1";
        let hit = if row[0] == "map" {
            matches!(snapshot.intersect_internal(ray, &mut distance, ignore_m2, &mut budget()).unwrap(),
                StaticRay::Hit { .. })
        } else {
            let index = match row[0] { "authored" | "rotated" => 0, "signed-zero" => 1, _ => panic!("unknown scope") };
            // WorldModel's runtime modelFlags comes from the reconciled spawns.
            let spawn = snapshot.spawns.iter().flatten().find(|s| s.model == index).unwrap();
            assert_eq!(spawn.classification(), ModelClassification::M2);
            !(ignore_m2 && spawn.classification() == ModelClassification::M2)
                && snapshot.models[index].intersect(ray, &mut distance, &mut budget()).unwrap()
        };
        assert_eq!(hit, row[10] == "1", "{} {}", row[0], row[1]);
        assert_eq!(distance.to_bits(), bits(row[11]), "{} {} distance", row[0], row[1]);
        count += 1;
    }
    assert_eq!(count, 1530);
}

#[test]
fn exact_source_m2_heights_keep_collision_and_map_miss_semantics() {
    let snapshot = snapshot();
    let mut count = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/heights.tsv"
    ).lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        let internal = [number(row[1]), number(row[2]), number(row[3])];
        let position = to_internal(internal);
        assert_eq!(to_internal(position).map(f32::to_bits), internal.map(f32::to_bits));
        let request = HeightRequest {
            snapshot: snapshot.binding(), ticket: 1, position: position.map(f32::to_bits),
            max_search: bits(row[4]),
        };
        let expected = if number(row[5]) == f32::INFINITY {
            (-200_000.0f32).to_bits() // VMapManager2 converts the tree's miss sentinel.
        } else { bits(row[5]) };
        let reply = snapshot.query_height(request, budget().deadline).unwrap();
        assert_eq!(reply, HeightReply { request, height: expected }, "{}", row[0]);
        count += 1;
    }
    assert_eq!(count, 366);
}

fn word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn chunk(bytes: &[u8], name: &[u8; 4]) -> usize {
    bytes.windows(4).position(|b| b == name).unwrap()
}

fn tile_records() -> Vec<Range<usize>> {
    tile_spawns().into_iter().map(|(range, _)| range).collect()
}

fn tile_spawns() -> Vec<(Range<usize>, Spawn)> {
    let ids = BTreeMap::from([
        (ProductId::VmapModel("authored.m2.vmo".into()), 0),
        (ProductId::VmapModel("signed-zero.m2.vmo".into()), 1),
    ]);
    let mut c = Cursor::new(TILE);
    c.magic(b"VMAP_7.0").unwrap();
    let count = c.count().unwrap();
    let mut ranges = Vec::new();
    for _ in 0..count {
        let start = c.at;
        let (_, spawn) = Spawn::decode(&mut c, &ids, false).unwrap();
        ranges.push((start..c.at, spawn));
    }
    c.finish().unwrap();
    ranges
}

fn mutate_tiles(
    spec: &mut InventorySpec,
    files: &mut BTreeMap<String, Vec<u8>>,
    mutate: impl Fn(&mut Vec<u8>),
) {
    for tile in spec.tiles.clone() {
        product_tests::mutate(spec, files, &ProductId::VmapTile(tile), &mutate);
    }
}

#[test]
fn exact_extractor_m2_zero_group_bounds_load_only_with_validated_spawn_context() {
    let snapshot = snapshot();
    assert_eq!(snapshot.models.len(), 2);
    for (model, (_, bytes)) in snapshot.models.iter().zip(MODELS) {
        assert_eq!(model.root_id, 0);
        assert_eq!(model.groups.len(), 1);
        let group = &model.groups[0];
        assert_eq!((group.flags, group.id), (0, 0));
        assert_eq!(group.bounds.low.map(f32::to_bits), [0; 3]);
        assert_eq!(group.bounds.high.map(f32::to_bits), [0; 3]);
        let group_tree = model.tree.as_ref().unwrap();
        assert_eq!(group_tree.bounds.low.map(f32::to_bits), [0; 3]);
        assert_eq!(group_tree.bounds.high.map(f32::to_bits), [0; 3]);
        assert!(group.vertices.iter().any(|&p| !group.bounds.contains(p)));
        let at = chunk(bytes, b"VERT") + 12;
        for (index, value) in group.vertices.iter().flatten().enumerate() {
            let offset = at + index * 4;
            let source = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            assert_eq!(value.to_bits(), source);
        }
        assert!(group.liquid.is_none());
        assert!(matches!(Model::decode(bytes), Err(Error::Format)));
        assert!(matches!(
            Model::decode_profile(bytes, true),
            Err(Error::Format)
        ));
        assert!(matches!(
            Model::decode_classified(bytes, false, Some(ModelClassification::Wmo)),
            Err(Error::Format)
        ));
    }
    assert!(snapshot.models[1].groups[0]
        .vertices
        .iter()
        .flatten()
        .any(|v| v.to_bits() == (-0.0f32).to_bits()));
    // The source's one-group ray path bypasses GBIH; MBIH bounds still enclose the hull.
    assert_eq!(
        snapshot.segment_internal([10., 14., 14.], [10., 14., -6.], false, &mut budget()),
        Ok(StaticRay::Hit {
            distance: 10f32.to_bits()
        })
    );
    assert_eq!(
        snapshot.segment_internal([10., 14., 14.], [10., 14., -6.], true, &mut budget()),
        Ok(StaticRay::Miss)
    );
}

#[test]
fn m2_exception_requires_exact_extractor_identity_and_positive_zero_bounds() {
    let bytes = MODELS[0].1;
    let mut mutations = vec![(16, 1), (24, 2), (52, 1), (56, 1), (40, 1f32.to_bits())];
    mutations.extend((0..6).map(|i| (28 + i * 4, (-0.0f32).to_bits())));
    for (at, value) in mutations {
        let mut changed = bytes.to_vec();
        word(&mut changed, at, value);
        assert!(
            Model::decode_classified(&changed, false, Some(ModelClassification::M2)).is_err(),
            "M2 signature mutation {at}: {value:08x}"
        );
    }
    // A completely framed two-group model must not acquire the single-group exception.
    let end = chunk(bytes, b"GBIH");
    let mut two_groups = bytes[..end].to_vec();
    two_groups.extend_from_slice(&bytes[28..end]);
    word(&mut two_groups, 24, 2);
    two_groups.extend_from_slice(b"GBIH");
    two_groups.extend_from_slice(&[0; 24]);
    for value in [3u32, 3 << 30, 2, 0, 2, 0, 1] {
        two_groups.extend_from_slice(&value.to_le_bytes());
    }
    assert!(matches!(
        Model::decode_classified(&two_groups, false, Some(ModelClassification::M2)),
        Err(Error::Format)
    ));
}

#[test]
fn m2_exception_retains_vertex_index_and_both_bih_validations() {
    let bytes = MODELS[0].1;
    let vertex = chunk(bytes, b"VERT") + 12;
    let triangle = chunk(bytes, b"TRIM") + 12;
    let mesh = chunk(bytes, b"MBIH") + 4;
    let groups = chunk(bytes, b"GBIH") + 4;
    for (at, value) in [
        (vertex, f32::NAN.to_bits()),
        (vertex, f32::INFINITY.to_bits()),
        (vertex, f32::NEG_INFINITY.to_bits()),
        (triangle, 25),
        (mesh, 8f32.next_up().to_bits()),
        (mesh + 32, 16f32.next_down().to_bits()),
        (mesh + 28, 0x2000_0000),
        (groups + 28, 0x2000_0000),
        (bytes.len() - 4, 1),
    ] {
        let mut changed = bytes.to_vec();
        word(&mut changed, at, value);
        assert!(
            Model::decode_classified(&changed, false, Some(ModelClassification::M2)).is_err(),
            "M2 validation mutation {at}: {value:08x}"
        );
    }
    let mut changed = bytes.to_vec();
    word(&mut changed, groups, 1f32.to_bits());
    word(&mut changed, groups + 12, 1f32.to_bits());
    assert!(matches!(
        Model::decode_classified(&changed, false, Some(ModelClassification::M2)),
        Err(Error::Format)
    ));
}

#[test]
fn m2_exception_never_allows_liquid_even_in_opt_in_liquid_profile() {
    let mut bytes = MODELS[0].1.to_vec();
    let at = chunk(&bytes, b"LIQU") + 4;
    // Source WmoLiquid size excludes the type word: 20 + four heights + one flag.
    let mut payload = Vec::new();
    for value in [1u32, 1, 0, 0, 0, 1, 0, 0, 0, 0] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.push(0);
    word(&mut bytes, at, (payload.len() - 4) as u32);
    bytes.splice(at + 4..at + 4, payload);
    for allow_liquid in [false, true] {
        assert!(matches!(
            Model::decode_classified(&bytes, allow_liquid, Some(ModelClassification::M2)),
            Err(Error::Unsupported)
        ));
    }
}

#[test]
fn m2_classification_is_reconciled_before_geometry_and_never_inferred_from_names() {
    let first = tile_records()[0].clone();
    let authored: Vec<_> = tile_spawns()
        .into_iter()
        .filter_map(|(range, spawn)| (spawn.model == 0).then_some(range))
        .collect();
    for first_flags in [4, 5] {
        let (mut spec, mut files) = fixture();
        mutate_tiles(&mut spec, &mut files, |b| {
            for range in &authored {
                word(b, range.start, first_flags);
            }
        });
        let tile = ProductId::VmapTile(spec.tiles[1]);
        product_tests::mutate(&mut spec, &mut files, &tile, |b| {
            word(b, first.start, first_flags ^ 1)
        });
        assert!(matches!(
            product_tests::load(spec, &files)
                .unwrap()
                .load_static_vmap(),
            Err(Error::Dependency)
        ));
    }

    let (mut spec, mut files) = fixture();
    mutate_tiles(&mut spec, &mut files, |b| {
        for range in &authored {
            word(b, range.start, 4);
        }
    });
    assert!(matches!(
        product_tests::load(spec, &files)
            .unwrap()
            .load_static_vmap(),
        Err(Error::Format)
    ));
}

#[test]
fn unreferenced_m2_model_cannot_acquire_classification_from_the_inventory() {
    let unused = ProductId::VmapModel("signed-zero.m2.vmo".into());
    let ranges = tile_records();
    let mut tile = TILE[..12].to_vec();
    // The source's second record is the only placement of the signed-zero model.
    for (index, range) in ranges.iter().enumerate() {
        if index != 1 {
            tile.extend_from_slice(&TILE[range.clone()]);
        }
    }
    word(&mut tile, 8, (ranges.len() - 1) as u32);
    let (mut spec, mut files) = fixture();
    mutate_tiles(&mut spec, &mut files, |b| *b = tile.clone());
    // Keeping the dependency cannot disguise a missing serialized model reference.
    assert!(matches!(
        product_tests::load(spec.clone(), &files),
        Err(Error::Dependency)
    ));
    for product in &mut spec.products {
        product.dependencies.retain(|id| *id != unused);
    }
    // Removing that edge still cannot inventory an unreferenced model product.
    assert!(matches!(
        product_tests::load(spec, &files),
        Err(Error::Unexpected)
    ));
    assert!(matches!(Model::decode(MODELS[1].1), Err(Error::Format)));
}

#[test]
fn m2_zero_group_bounds_do_not_relax_transformed_placement_containment() {
    let mut count = 0;
    for (range, spawn) in tile_spawns()
        .into_iter()
        .filter(|(_, spawn)| spawn.model == 0)
    {
        let (mut spec, mut files) = fixture();
        mutate_tiles(&mut spec, &mut files, |b| {
            // flags/adt/id, position, rotation and scale precede the real instance bound.
            word(b, range.start + 38, spawn.bounds.low[0].next_up().to_bits());
        });
        assert!(
            matches!(
                product_tests::load(spec, &files)
                    .unwrap()
                    .load_static_vmap(),
                Err(Error::Format)
            ),
            "placement {}",
            spawn.id
        );
        count += 1;
    }
    assert_eq!(count, 2); // original and exact source rotated/scaled placements
}

#[test]
fn mixed_m2_wmo_references_never_authorize_zero_group_bounds() {
    let ranges = tile_records();
    let first = TILE[ranges[0].clone()].to_vec();
    let mut second = first.clone();
    word(&mut second, 0, 4); // same named model, now claimed as WMO
    word(&mut second, 6, 102); // unique spawn ID
    let reference = second.len() - 4;
    word(&mut second, reference, 1);
    for records in [[&first, &second], [&second, &first]] {
        let mut tile = TILE[..12].to_vec();
        word(&mut tile, 8, 2);
        for record in records {
            tile.extend_from_slice(record);
        }
        let (mut spec, mut files) = fixture();
        mutate_tiles(&mut spec, &mut files, |b| *b = tile.clone());
        let removed = ProductId::VmapModel("signed-zero.m2.vmo".into());
        spec.products.retain(|p| p.id != removed);
        for product in &mut spec.products {
            product.dependencies.retain(|id| *id != removed);
        }
        files.remove(&removed.filename(0).unwrap());
        assert!(matches!(
            product_tests::load(spec, &files)
                .unwrap()
                .load_static_vmap(),
            Err(Error::Unsupported)
        ));
    }
}

#[test]
fn classified_m2_models_reject_every_truncation_and_trailing_byte() {
    for (_, bytes) in MODELS {
        for length in 0..bytes.len() {
            if length == 20 {
                continue;
            } // source-empty model, no GMOD
            assert!(
                Model::decode_classified(&bytes[..length], false, Some(ModelClassification::M2))
                    .is_err(),
                "M2 prefix {length}"
            );
        }
        let mut appended = bytes.to_vec();
        appended.push(0);
        assert!(Model::decode_classified(&appended, false, Some(ModelClassification::M2)).is_err());
    }
}

fn source_named_fixture(terminal_nul: bool) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = fixture();
    if terminal_nul {
        mutate_tiles(&mut spec, &mut files, |bytes| {
            for range in tile_records().into_iter().rev() {
                // These independently captured M2 records all carry source bounds.
                let length_at = range.start + 62;
                let len = u32::from_le_bytes(bytes[length_at..length_at + 4].try_into().unwrap())
                    as usize;
                bytes.insert(length_at + 4 + len, 0);
                word(bytes, length_at, (len + 1) as u32);
            }
        });
    }
    let map_id = |id: &ProductId| match id {
        ProductId::VmapModel(name) => {
            let mut raw = name.strip_suffix(".vmo").unwrap().as_bytes().to_vec();
            if terminal_nul {
                raw.push(0);
            }
            ProductId::VmapSourceModel(benilla_native_vmap::SourceModelName::new(&raw).unwrap())
        }
        _ => id.clone(),
    };
    for item in &mut spec.products {
        let old = item.id.filename(spec.map).unwrap();
        item.id = map_id(&item.id);
        let new = item.id.filename(spec.map).unwrap();
        if old != new {
            let data = files.remove(&old).unwrap();
            assert!(files.insert(new, data).is_none());
        }
        item.dependencies = item.dependencies.iter().map(&map_id).collect();
    }
    (spec, files)
}

#[test]
fn typed_source_names_preserve_original_queries_with_safe_inventory_paths() {
    for terminal_nul in [false, true] {
        let (spec, files) = source_named_fixture(terminal_nul);
        assert!(files.keys().all(|name| !name.contains('\0')));
        let products = product_tests::load(spec.clone(), &files).unwrap();
        let snapshot = products.load_static_vmap().unwrap();
        let mut reordered = spec;
        reordered.products.reverse();
        for p in &mut reordered.products {
            p.dependencies.reverse();
        }
        assert_eq!(
            products.revision,
            product_tests::load(reordered, &files).unwrap().revision
        );
        for line in include_str!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/heights.tsv").lines().filter(|line| !line.starts_with('#')) {
            let row: Vec<_> = line.split('\t').collect();
            let position = to_internal([number(row[1]), number(row[2]), number(row[3])]);
            let request = HeightRequest { snapshot: snapshot.binding(), ticket: 1, position: position.map(f32::to_bits), max_search: bits(row[4]) };
            let expected = if number(row[5]) == f32::INFINITY { (-200_000.0f32).to_bits() } else { bits(row[5]) };
            assert_eq!(snapshot.query_height(request, budget().deadline).unwrap().height, expected);
        }
    }
    let (plain, files) = source_named_fixture(false);
    let (nul, nul_files) = source_named_fixture(true);
    assert_ne!(
        product_tests::load(plain, &files).unwrap().revision,
        product_tests::load(nul, &nul_files).unwrap().revision
    );
}

#[test]
fn source_model_identity_mismatches_and_mixed_profiles_fail_closed() {
    let (mut spec, files) = source_named_fixture(true);
    let tile = spec
        .products
        .iter_mut()
        .find(|p| matches!(p.id, ProductId::VmapTile(_)))
        .unwrap();
    let dependency = tile
        .dependencies
        .iter_mut()
        .find(|id| id.is_vmap_model())
        .unwrap();
    if let ProductId::VmapSourceModel(name) = dependency {
        *name = benilla_native_vmap::SourceModelName::new(name.raw_basename()).unwrap();
    }
    assert!(product_tests::load(spec, &files).is_err());
    let (mut spec, _) = source_named_fixture(false);
    let mut legacy = spec
        .products
        .iter()
        .find(|p| p.id.is_vmap_model())
        .unwrap()
        .clone();
    legacy.id = ProductId::VmapModel("authored.m2.vmo".into());
    spec.products.push(legacy);
    assert!(
        super::super::InventoriedProducts::materialize(spec, |_, _| panic!(
            "mixed profile must reject before reads"
        ))
        .is_err()
    );
}

#[test]
fn source_effective_output_aliases_reject_before_reads() {
    let (mut spec, _) = source_named_fixture(false);
    let second = spec
        .products
        .iter_mut()
        .filter(|p| p.id.is_vmap_model())
        .nth(1)
        .unwrap();
    second.id = ProductId::VmapSourceModel(
        benilla_native_vmap::SourceModelName::new(b"authored.m2.vmo\0").unwrap(),
    );
    assert!(matches!(
        super::super::InventoriedProducts::materialize(spec, |_, _| panic!(
            "aliases must reject before reads"
        )),
        Err(super::super::Error::Duplicate)
    ));
}

#[test]
fn valid_source_inventory_rejects_different_exact_tile_name_bytes() {
    let (mut spec, mut files) = source_named_fixture(true);
    let (_, plain_files) = source_named_fixture(false);
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        let plain = plain_files[&id.filename(spec.map).unwrap()].clone();
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| *bytes = plain.clone());
    }
    assert!(matches!(
        product_tests::load(spec, &files),
        Err(super::super::Error::Dependency)
    ));
}

#[test]
fn distinct_plain_and_nul_names_coexist_without_merging_models() {
    for new_name in [b"authored.m2\0".as_slice(), b"Authored.m2".as_slice()] {
        let (mut spec, mut files) = source_named_fixture(false);
        let previous = ProductId::VmapSourceModel(
            benilla_native_vmap::SourceModelName::new(b"signed-zero.m2").unwrap(),
        );
        let replacement = ProductId::VmapSourceModel(
            benilla_native_vmap::SourceModelName::new(new_name).unwrap(),
        );
        mutate_tiles(&mut spec, &mut files, |bytes| {
            for range in tile_records().into_iter().rev() {
                let length_at = range.start + 62;
                let len = u32::from_le_bytes(bytes[length_at..length_at + 4].try_into().unwrap())
                    as usize;
                let name_at = length_at + 4;
                if &bytes[name_at..name_at + len] == b"signed-zero.m2" {
                    bytes.splice(name_at..name_at + len, new_name.iter().copied());
                    word(bytes, length_at, new_name.len() as u32);
                }
            }
        });
        let old_path = previous.filename(spec.map).unwrap();
        let new_path = replacement.filename(spec.map).unwrap();
        let data = files.remove(&old_path).unwrap();
        files.insert(new_path, data);
        for item in &mut spec.products {
            if item.id == previous {
                item.id = replacement.clone();
            }
            for dependency in &mut item.dependencies {
                if *dependency == previous {
                    *dependency = replacement.clone();
                }
            }
        }
        let snapshot = product_tests::load(spec, &files)
            .unwrap()
            .load_static_vmap()
            .unwrap();
        assert_eq!(snapshot.models.len(), 2);
        assert!(snapshot
            .spawns
            .iter()
            .flatten()
            .any(|spawn| spawn.model == 0));
        assert!(snapshot
            .spawns
            .iter()
            .flatten()
            .any(|spawn| spawn.model == 1));
    }
}

#[test]
fn source_model_missing_and_unused_closure_fail_before_reads() {
    let (mut missing, _) = source_named_fixture(false);
    let at = missing
        .products
        .iter()
        .position(|p| p.id.is_vmap_model())
        .unwrap();
    missing.products.remove(at);
    assert!(matches!(
        super::super::InventoriedProducts::materialize(missing, |_, _| panic!(
            "missing model must reject before reads"
        )),
        Err(super::super::Error::Missing)
    ));
    let (mut extra, _) = source_named_fixture(false);
    let mut unused = extra
        .products
        .iter()
        .find(|p| p.id.is_vmap_model())
        .unwrap()
        .clone();
    unused.id = ProductId::VmapSourceModel(
        benilla_native_vmap::SourceModelName::new(b"Unused.m2").unwrap(),
    );
    extra.products.push(unused);
    assert!(matches!(
        super::super::InventoriedProducts::materialize(extra, |_, _| panic!(
            "unused model must reject before reads"
        )),
        Err(super::super::Error::Unexpected)
    ));
}

#[test]
fn legacy_inventory_revision_matches_independent_native91_golden() {
    // Captured from unchanged 2d368c1c61addcb07b1931c04f087bbddda14d30.
    let (spec, files) = fixture();
    let expected: [u8; 32] = [
        0x98, 0xbc, 0x61, 0x08, 0xc4, 0xdf, 0x0b, 0xde, 0x30, 0xf3, 0xc5, 0xcf, 0x36, 0x2f, 0x60,
        0xd6, 0x88, 0xa1, 0x3d, 0x46, 0xcc, 0x34, 0xc6, 0x85, 0xb6, 0xe0, 0x58, 0xce, 0xee, 0x01,
        0x3b, 0xa7,
    ];
    assert_eq!(
        product_tests::load(spec, &files).unwrap().revision(),
        expected
    );
}
