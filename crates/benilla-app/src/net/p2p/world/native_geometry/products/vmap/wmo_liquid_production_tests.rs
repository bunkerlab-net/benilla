// SPDX-License-Identifier: GPL-3.0-or-later
// Authored MLIQ roots pass through the padding-v1 extractor and original assembler.
// These queries exercise the existing VMap-only reader, never TerrainInfo or admission.
use super::super::{
    envelopes, tests as product_tests, InventorySpec, ProductSpec, VmapLiquidProducts,
};
use super::liquid::{LiquidRequest, LiquidResult, LiquidSnapshot};
use super::*;
use std::{collections::BTreeSet, fs, ops::Range, path::PathBuf, time::Duration};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../benilla-native-geometry-tests/tools/fixtures/wmo_liquid_source")
        .join(name)
}

fn models() -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(fixture_path("products"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "vmo"))
        .map(|path| {
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = product_tests::fixture();
    for product in &spec.products {
        if matches!(product.id, ProductId::VmapModel(_)) {
            files.remove(&product.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    let tree = fs::read(fixture_path("products/000.vmtree")).unwrap();
    let tile = fs::read(fixture_path("products/000_32_32.vmtile")).unwrap();
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        *bytes = tree.clone();
    });
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            envelopes::validate_with_vmap_liquid(&ProductId::VmapTile(spec.tiles[0]), &tile)
                .unwrap(),
        )
        .collect();
    for tile_id in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile_id);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| *bytes = tile.clone());
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
    }
    for (name, bytes) in models() {
        let id = ProductId::VmapModel(name);
        spec.products.push(ProductSpec {
            id: id.clone(),
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(&bytes).into(),
            dependencies: vec![],
        });
        files.insert(id.filename(0).unwrap(), bytes);
    }
    (spec, files)
}

fn products(
    spec: InventorySpec,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<VmapLiquidProducts, Error> {
    Ok(VmapLiquidProducts {
        inventory: InventoriedProducts::materialize_profile(spec, true, |name, _| {
            files.get(name).cloned().ok_or(Error::Missing)
        })?,
    })
}

fn snapshot() -> LiquidSnapshot {
    let (spec, files) = fixture();
    products(spec, &files).unwrap().load_vmap_liquid().unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn bits(text: &str) -> u32 {
    u32::from_str_radix(text, 16).unwrap()
}

fn rows() -> Vec<Vec<String>> {
    fs::read_to_string(fixture_path("liquid.tsv"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect()
}

fn request(snapshot: &LiquidSnapshot, row: &[String], ticket: u64) -> LiquidRequest {
    assert_eq!(row.len(), 23);
    LiquidRequest {
        snapshot: snapshot.binding(),
        ticket,
        position: [bits(&row[2]), bits(&row[3]), bits(&row[4])],
        required_mask: row[15].parse().unwrap(),
    }
}

#[test]
fn source_produced_wmo_liquid_matches_original_manager_floor_type_mask_and_level_bits() {
    let snapshot = snapshot();
    let mut locations = 0;
    let mut wet = 0;
    let mut masked = 0;
    let mut absent_level = 0;
    let mut dry_selected = 0;
    let mut types = BTreeSet::new();
    let mut spawns = BTreeSet::new();
    let rows = rows();
    for (index, row) in rows.iter().enumerate() {
        let query = request(&snapshot, row, index as u64 + 1);
        assert_eq!(
            to_internal(query.position.map(f32::from_bits)).map(f32::to_bits),
            [bits(&row[5]), bits(&row[6]), bits(&row[7])],
            "{} source coordinate conversion",
            row[1]
        );
        let actual = snapshot.query(query, deadline()).unwrap();
        assert_eq!(actual.request, query);
        let expected = if row[8] == "1" {
            locations += 1;
            let level = (row[16] == "1").then(|| bits(&row[17]));
            let liquid_type = row[19].parse().unwrap();
            let location = LocationFloor {
                floor: bits(&row[18]),
                spawn_id: row[10].parse().unwrap(),
                root_id: row[11].parse::<i64>().unwrap() as u32,
                group_id: row[12].parse().unwrap(),
                group_flags: row[13].parse().unwrap(),
                adt_id: row[14].parse().unwrap(),
            };
            assert_eq!(bits(&row[9]), location.floor, "{} floor write", row[1]);
            wet += usize::from(level.is_some());
            dry_selected += usize::from(liquid_type == 0 && level.is_none());
            absent_level += usize::from(query.required_mask == 0 && level.is_none());
            masked += usize::from(row[20] == "1" && level.is_none());
            types.insert(liquid_type);
            spawns.insert(location.spawn_id);
            if level.is_none() {
                assert_eq!(bits(&row[17]), 123.25f32.to_bits());
            }
            LiquidResult::Location {
                location,
                liquid_type,
                level,
            }
        } else {
            assert_eq!(row[16], "0");
            assert_eq!(bits(&row[17]), 123.25f32.to_bits());
            assert_eq!(bits(&row[18]), (-100_000.0f32).to_bits());
            assert_eq!(row[19], "305419896");
            LiquidResult::NoLocation
        };
        assert_eq!(actual.result, expected, "{}", row[1]);
    }
    assert_eq!(rows.len(), 2205);
    assert_eq!(
        (locations, wet, masked, absent_level, dry_selected),
        (1904, 342, 596, 138, 286)
    );
    assert_eq!(
        types.into_iter().collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 21, 22, 41, 0xffff_8000, u32::MAX]
    );
    assert_eq!(spawns.len(), 21);
    assert!(spawns.contains(&999)); // Original rotated/scaled ModelInstance path.
}

#[cfg(benilla_headless_geometry)]
#[test]
fn produced_raw_matches_padding_v1_inputs_of_original_assembled_liquid_products() {
    use benilla_native_vmap::{
        extract_wmo_collision, extract_wmo_collision_from_source, WmoError, WmoGroupInput,
        WMO_LIQUID_HEADER_ZERO_PADDING_V1,
    };

    let manifest = fs::read_to_string(fixture_path("cases.tsv")).unwrap();
    assert_eq!(
        WMO_LIQUID_HEADER_ZERO_PADDING_V1,
        "wmo-liquid-header-zero-padding-v1"
    );
    let mut count = 0;
    for line in manifest
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 17, "{line}");
        let root = fs::read(fixture_path(row[11])).unwrap();
        let groups: Vec<_> = row[12]
            .split(';')
            .map(|name| fs::read(fixture_path(name)).unwrap())
            .collect();
        let inputs: Vec<_> = groups
            .iter()
            .enumerate()
            .map(|(index, bytes)| WmoGroupInput {
                group_index: index as u32,
                bytes,
            })
            .collect();
        let produced = extract_wmo_collision_from_source(row[1], &root, &inputs).unwrap();
        // Equality includes the named zero-padding profile, not the source's two
        // uninitialized ABI bytes. Its unchanged assembler consumed these raw bytes.
        assert_eq!(
            produced.raw_vmap,
            fs::read(fixture_path(row[13])).unwrap(),
            "{} padding-v1 raw input to original assembler",
            row[0]
        );
        assert_eq!(
            produced.declared_group_count,
            row[14].parse::<u32>().unwrap()
        );
        assert_eq!(
            produced.retained_group_count,
            row[14].parse::<u32>().unwrap()
        );
        assert_eq!(
            produced.collision_triangle_count,
            row[15].parse::<u32>().unwrap()
        );
        assert_eq!(produced.liquid_group_count, row[16].parse::<u32>().unwrap());
        let mut liquids = 0;
        for at in produced
            .raw_vmap
            .windows(4)
            .enumerate()
            .filter_map(|(at, tag)| (tag == b"LIQU").then_some(at))
        {
            assert_eq!(produced.raw_vmap[at + 8 + 30..at + 8 + 32], [0, 0]);
            liquids += 1;
        }
        assert_eq!(liquids, produced.liquid_group_count);
        assert!(matches!(
            extract_wmo_collision(&root, &inputs),
            Err(WmoError::UnsupportedLiquid { group: 0 })
        ));
        count += 1;
    }
    assert_eq!(count, 20);
    let snapshot = snapshot();
    let row = rows().into_iter().find(|row| row[16] == "1").unwrap();
    assert!(matches!(
        snapshot
            .query(request(&snapshot, &row, 1), deadline())
            .unwrap()
            .result,
        LiquidResult::Location { level: Some(_), .. }
    ));
}

#[test]
fn source_produced_wet_wmo_remains_separate_from_existing_dry_readers() {
    let (spec, files) = fixture();
    assert!(matches!(
        product_tests::load(spec.clone(), &files),
        Err(Error::Unsupported)
    ));
    let wet = products(spec, &files).unwrap();
    assert!(wet.load_vmap_liquid().is_ok());
    assert!(matches!(
        wet.inventory.load_static_vmap(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_static_height(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_dry_liquid(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        wet.inventory.load_navigation(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        VmapSnapshot::load_profile(&wet.inventory, true)
            .unwrap()
            .require_complete_dry_map(),
        Err(Error::Unsupported)
    ));
}

#[test]
fn source_produced_vmap_liquid_floor_is_independent_of_terrain_height_bytes() {
    let (mut spec, mut files) = fixture();
    let original = products(spec.clone(), &files)
        .unwrap()
        .load_vmap_liquid()
        .unwrap();
    for tile in spec.tiles.clone() {
        product_tests::mutate(&mut spec, &mut files, &ProductId::Terrain(tile), |bytes| {
            *bytes = product_tests::terrain_bytes(3);
        });
    }
    let changed = products(spec, &files).unwrap().load_vmap_liquid().unwrap();
    assert_ne!(original.binding(), changed.binding());
    for (index, row) in rows().iter().enumerate() {
        let first = request(&original, row, index as u64 + 1);
        let second = request(&changed, row, index as u64 + 1);
        assert_eq!(
            original.query(first, deadline()).unwrap().result,
            changed.query(second, deadline()).unwrap().result,
            "{}",
            row[1]
        );
    }
}

fn group_ranges(bytes: &[u8]) -> Vec<(Range<usize>, Group)> {
    let mut cursor = Cursor::new(bytes);
    cursor.magic(b"VMAP_7.0WMOD").unwrap();
    assert_eq!(cursor.word().unwrap(), 8);
    cursor.word().unwrap();
    cursor.magic(b"GMOD").unwrap();
    let count = cursor.count().unwrap();
    let mut groups = Vec::new();
    for _ in 0..count {
        let start = cursor.at;
        let group = Group::decode(&mut cursor, true, false).unwrap();
        groups.push((start..cursor.at, group));
    }
    cursor.magic(b"GBIH").unwrap();
    Bih::decode(&mut cursor).unwrap();
    cursor.finish().unwrap();
    groups
}

fn replace(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn source_produced_wet_models_reject_malformed_liquid_lengths_dimensions_and_bounds() {
    let (spec, files) = fixture();
    let mut checked = 0;
    for (name, bytes) in models() {
        let id = ProductId::VmapModel(name.clone());
        for (range, group) in group_ranges(&bytes) {
            if group.liquid.is_none() {
                continue;
            }
            let at = range.start
                + bytes[range.clone()]
                    .windows(4)
                    .position(|word| word == b"LIQU")
                    .unwrap();
            let declared = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap());
            // The source LIQU size excludes its type word, but the payload includes it.
            assert_eq!(range.end - at - 8, declared as usize + 4);
            let mut mutations = vec![
                (at + 4, 1),
                (at + 4, declared + 4),
                (at + 8, u32::MAX),
                (at + 12, u32::MAX),
                (at + 16, f32::NAN.to_bits()),
                (at + 20, f32::INFINITY.to_bits()),
                (at + 24, f32::NEG_INFINITY.to_bits()),
                (at + 32, f32::NAN.to_bits()),
            ];
            for axis in 0..3 {
                mutations.push((range.start + axis * 4, f32::NAN.to_bits()));
                mutations.push((
                    range.start + axis * 4,
                    group.bounds.high[axis].next_up().to_bits(),
                ));
                let minimum = group
                    .vertices
                    .iter()
                    .map(|vertex| vertex[axis])
                    .reduce(f32::min)
                    .unwrap();
                mutations.push((range.start + axis * 4, minimum.next_up().to_bits()));
            }
            for (offset, value) in mutations {
                let (mut changed_spec, mut changed_files) = (spec.clone(), files.clone());
                product_tests::mutate(&mut changed_spec, &mut changed_files, &id, |bytes| {
                    replace(bytes, offset, value);
                });
                assert!(
                    products(changed_spec, &changed_files)
                        .and_then(|products| products.load_vmap_liquid())
                        .is_err(),
                    "{name}: mutation {offset}: {value:08x}"
                );
                checked += 1;
            }
        }
        for prefix in 0..bytes.len() {
            if prefix != 20 {
                assert!(
                    Model::decode_profile(&bytes[..prefix], true).is_err(),
                    "{name}: prefix {prefix}"
                );
            }
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(Model::decode_profile(&trailing, true).is_err(), "{name}");
    }
    assert!(checked > 0);
}

#[test]
fn source_produced_liquid_queries_keep_snapshot_identity_and_finite_deadline_checks() {
    let snapshot = snapshot();
    let row = rows().into_iter().find(|row| row[16] == "1").unwrap();
    let query = request(&snapshot, &row, 1);
    assert!(matches!(
        snapshot.query(query, deadline()).unwrap().result,
        LiquidResult::Location { level: Some(_), .. }
    ));
    for field in 0..6 {
        let mut stale = query;
        match field {
            0 => stale.snapshot.map += 1,
            1 => stale.snapshot.content = [0; 32],
            2 => stale.snapshot.source_epoch = [0; 32],
            3 => stale.snapshot.inventory = [0; 32],
            4 => stale.snapshot.policy = [0; 32],
            _ => stale.ticket = 0,
        }
        assert_eq!(snapshot.query(stale, deadline()), Err(Error::Identity));
    }
    for axis in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut invalid = query;
            invalid.position[axis] = value.to_bits();
            assert_eq!(snapshot.query(invalid, deadline()), Err(Error::Format));
        }
    }
    assert_eq!(snapshot.query(query, Instant::now()), Err(Error::Deadline));
}
