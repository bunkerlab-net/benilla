// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::{
    tests as product_tests, InventoriedProducts, InventorySpec, ProductId, ProductSpec,
};
use super::super::{liquid::LiquidRequest, liquid_fixture, location_fixture, tests as vmap_tests};
use super::*;
use std::{collections::BTreeMap, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
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

fn snapshot() -> AreaSnapshot {
    let (spec, files) = vmap_tests::fixture();
    products(spec, &files).unwrap().load_vmap_area().unwrap()
}

fn request(snapshot: &AreaSnapshot, internal: Vector) -> AreaRequest {
    AreaRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: to_internal(internal).map(f32::to_bits),
    }
}

fn wet_fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = vmap_tests::fixture();
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("ramp.wmo.vmo".into()),
        |bytes| {
            let at = bytes.windows(4).position(|word| word == b"LIQU").unwrap() + 4;
            let mut liquid = Vec::new();
            // An authored wet payload whose source liquid index cast would be
            // outside the defined uint32 domain at our valid area-query point.
            for value in [
                1,
                1,
                100.0f32.to_bits(),
                100.0f32.to_bits(),
                0,
                1,
                2.0f32.to_bits(),
                2.0f32.to_bits(),
                2.0f32.to_bits(),
                2.0f32.to_bits(),
            ] {
                liquid.extend_from_slice(&value.to_le_bytes());
            }
            liquid.push(0);
            bytes[at..at + 4].copy_from_slice(&((liquid.len() - 4) as u32).to_le_bytes());
            bytes.splice(at + 4..at + 4, liquid);
        },
    );
    (spec, files)
}

#[test]
fn area_policy_and_every_identity_field_are_bound_and_pose_bits_are_echoed() {
    let (spec, files) = vmap_tests::fixture();
    let products = products(spec, &files).unwrap();
    let snapshot = products.load_vmap_area().unwrap();
    let liquid = products.load_vmap_liquid().unwrap();
    let original = request(&snapshot, [0.0, 0.0, 0.0]);
    for foreign in [
        snapshot.geometry.binding(),
        snapshot.geometry.location_binding(),
        liquid.binding(),
    ] {
        assert_ne!(snapshot.binding(), foreign);
        assert_eq!(
            snapshot.query(
                AreaRequest {
                    snapshot: foreign,
                    ..original
                },
                deadline()
            ),
            Err(Error::Identity)
        );
    }
    for field in 0..6 {
        let mut changed = original;
        match field {
            0 => changed.snapshot.map += 1,
            1 => changed.snapshot.content[0] ^= 1,
            2 => changed.snapshot.source_epoch[0] ^= 1,
            3 => changed.snapshot.inventory[0] ^= 1,
            4 => changed.snapshot.policy[0] ^= 1,
            _ => changed.ticket = 0,
        }
        assert_eq!(snapshot.query(changed, deadline()), Err(Error::Identity));
    }
    for position in [[0; 3], [(-0.0f32).to_bits(); 3], original.position] {
        let query = AreaRequest {
            position,
            ticket: u64::MAX,
            ..original
        };
        assert_eq!(snapshot.query(query, deadline()).unwrap().request, query);
    }
}

#[test]
fn nonfinite_out_of_profile_and_expired_requests_never_become_absence() {
    let snapshot = snapshot();
    let original = request(&snapshot, [0.0, 0.0, 0.0]);
    for axis in 0..3 {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut query = original;
            query.position[axis] = invalid.to_bits();
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Format));
        }
        for invalid in [100_000.0f32.next_up(), -100_000.0f32.next_up()] {
            let mut query = original;
            query.position[axis] = invalid.to_bits();
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Unsupported));
        }
    }
    assert_eq!(snapshot.query(original, Instant::now()), Err(Error::Budget));
}

#[test]
fn area_reads_actual_dry_and_wet_bytes_without_querying_liquid_cells() {
    let dry = snapshot();
    let (spec, files) = wet_fixture();
    assert!(matches!(
        product_tests::load(spec.clone(), &files),
        Err(Error::Unsupported)
    ));
    let products = products(spec, &files).unwrap();
    let wet = products.load_vmap_area().unwrap();
    let liquid = products.load_vmap_liquid().unwrap();
    let query = request(&wet, [0.0, 0.0, 0.0]);
    let expected = dry
        .query(request(&dry, [0.0, 0.0, 0.0]), deadline())
        .unwrap()
        .result;
    assert!(matches!(expected, AreaResult::Area { .. }));
    assert_eq!(wet.query(query, deadline()).unwrap().result, expected);
    assert_eq!(
        liquid.query(
            LiquidRequest {
                snapshot: liquid.binding(),
                ticket: 1,
                position: query.position,
                required_mask: 0,
            },
            deadline()
        ),
        Err(Error::Unsupported)
    );
    assert!(matches!(
        products.inventory.load_static_vmap(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        products.inventory.load_static_height(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        products.inventory.load_dry_liquid(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        products.inventory.load_navigation(),
        Err(Error::Unsupported)
    ));
    assert_eq!(
        wet.geometry.require_complete_dry_map(),
        Err(Error::Unsupported)
    );
    assert_ne!(dry.binding(), wet.binding());
}

#[test]
fn all_declared_slots_are_required_at_load_and_before_an_outside_map_miss() {
    let (mut spec, mut files) = vmap_tests::fixture();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            bytes.truncate(12);
            bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
        });
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = vec![ProductId::VmapTree];
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    let incomplete = products(spec, &files).unwrap();
    assert!(matches!(incomplete.load_vmap_area(), Err(Error::Missing)));

    let mut snapshot = snapshot();
    let query = AreaRequest {
        position: [100_000.0f32.to_bits(); 3],
        ..request(&snapshot, [0.0, 0.0, 0.0])
    };
    assert_eq!(
        snapshot.query(query, deadline()).unwrap().result,
        AreaResult::None
    );
    snapshot.geometry.spawns[0] = None;
    assert_eq!(snapshot.query(query, deadline()), Err(Error::Missing));
}

#[test]
fn one_budget_covers_all_steps_and_never_publishes_partial_area_outputs() {
    let snapshot = source_area(false);
    let position = to_internal([0.0, 0.0, 7.0]);
    let mut full = QueryBudget::new(deadline());
    let initial = full.remaining;
    let expected = snapshot.area(position, &mut full).unwrap();
    assert_eq!(
        expected,
        AreaResult::Area {
            floor: 6.0f32.to_bits(),
            adt_id: 0x8101,
            root_id: 0xf222_2222,
            group_id: 0xe222_2222,
            flags: 0x4000_2000,
        }
    );
    let cost = initial - full.remaining;
    assert!(cost > snapshot.geometry.spawns.len());
    for allowance in 0..cost {
        let mut budget = QueryBudget {
            remaining: allowance,
            deadline: deadline(),
        };
        assert_eq!(
            snapshot.area(position, &mut budget),
            Err(Error::Budget),
            "{allowance}/{cost}"
        );
    }
    let mut budget = QueryBudget {
        remaining: cost * 2 - 1,
        deadline: deadline(),
    };
    assert_eq!(snapshot.area(position, &mut budget), Ok(expected));
    assert_eq!(snapshot.area(position, &mut budget), Err(Error::Budget));
    assert_eq!(budget.remaining, 0);
    budget.remaining = cost;
    assert_eq!(snapshot.area(position, &mut budget), Ok(expected));
    assert_eq!(budget.remaining, 0);
    budget.deadline = Instant::now();
    assert_eq!(budget.complete(), Err(Error::Budget));
    assert_eq!(snapshot.area(position, &mut budget), Err(Error::Budget));
}

#[test]
fn metadata_bits_are_raw_revision_bound_and_owned_after_source_bytes_disappear() {
    let original = snapshot();
    let (mut spec, mut files) = vmap_tests::fixture();
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("ramp.wmo.vmo".into()),
        |bytes| {
            for (offset, value) in [(16, 0xfedc_ba98u32), (52, 0xaabb_ccdd), (56, 0xdead_beef)] {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
        },
    );
    let snapshot = products(spec, &files).unwrap().load_vmap_area().unwrap();
    let query = request(&snapshot, [0.0, 0.0, 0.0]);
    let result = snapshot.query(query, deadline()).unwrap().result;
    assert!(matches!(
        result,
        AreaResult::Area {
            root_id: 0xfedc_ba98,
            group_id: 0xdead_beef,
            flags: 0xaabb_ccdd,
            ..
        }
    ));
    assert_ne!(original.binding(), snapshot.binding());
    assert_eq!(original.query(query, deadline()), Err(Error::Identity));
    files.values_mut().for_each(Vec::clear);
    assert_eq!(snapshot.query(query, deadline()).unwrap().result, result);
}

#[test]
fn malformed_or_missing_actual_bytes_cannot_publish_an_area_snapshot() {
    let (spec, files) = wet_fixture();
    let id = ProductId::VmapModel("ramp.wmo.vmo".into());
    let name = id.filename(0).unwrap();
    let mut missing = files.clone();
    missing.remove(&name);
    assert!(matches!(
        products(spec.clone(), &missing),
        Err(Error::Missing)
    ));
    let mut changed = files.clone();
    changed.get_mut(&name).unwrap()[16] ^= 1;
    assert!(matches!(products(spec.clone(), &changed), Err(Error::Hash)));
    for prefix in [0, 8, 16, files[&name].len() - 1] {
        let mut spec = spec.clone();
        let mut files = files.clone();
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| bytes.truncate(prefix));
        assert!(products(spec, &files)
            .and_then(|products| products.load_vmap_area())
            .is_err());
    }
}

#[test]
fn filesystem_area_snapshot_is_epoch_bound_and_owns_deleted_source_bytes() {
    let (spec, files) = wet_fixture();
    let stage = product_tests::Stage::new(&files);
    let root = &stage.0;
    let products = VmapLiquidProducts::load(root, spec.clone()).unwrap();
    let snapshot = products.load_vmap_area().unwrap();
    let query = request(&snapshot, [0.0, 0.0, 0.0]);
    let original = snapshot.query(query, deadline()).unwrap();
    assert!(matches!(original.result, AreaResult::Area { .. }));

    let mut new_spec = spec.clone();
    new_spec.binding.source_epoch[0] ^= 1;
    let new_epoch = VmapLiquidProducts::load(root, new_spec)
        .unwrap()
        .load_vmap_area()
        .unwrap();
    assert_ne!(snapshot.binding(), new_epoch.binding());
    assert_eq!(new_epoch.query(query, deadline()), Err(Error::Identity));
    assert_eq!(
        snapshot.query(
            AreaRequest {
                snapshot: new_epoch.binding(),
                ..query
            },
            deadline()
        ),
        Err(Error::Identity)
    );

    let model_path = root.join("vmaps/ramp.wmo.vmo");
    let mut changed = files["vmaps/ramp.wmo.vmo"].clone();
    changed[0] ^= 1;
    std::fs::write(&model_path, changed).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(root, spec.clone()),
        Err(Error::Hash)
    ));
    std::fs::remove_file(model_path).unwrap();
    assert!(matches!(
        VmapLiquidProducts::load(root, spec),
        Err(Error::Missing)
    ));
    assert_eq!(snapshot.query(query, deadline()).unwrap(), original);
    drop(stage);
    assert_eq!(snapshot.query(query, deadline()).unwrap(), original);
    assert_eq!(
        products
            .load_vmap_area()
            .unwrap()
            .query(query, deadline())
            .unwrap(),
        original
    );
}

fn source_fixture(
    tree: &[u8],
    tile_bytes: &[u8],
    models: &[(&str, &[u8])],
) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = vmap_tests::fixture();
    for product in &spec.products {
        if matches!(product.id, ProductId::VmapModel(_)) {
            files.remove(&product.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        *bytes = tree.to_vec()
    });
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            super::super::super::envelopes::validate_with_vmap_liquid(
                &ProductId::VmapTile(spec.tiles[0]),
                tile_bytes,
            )
            .unwrap(),
        )
        .collect();
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            *bytes = tile_bytes.to_vec()
        });
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
    }
    for &(name, bytes) in models {
        let id = ProductId::VmapModel(name.into());
        files.insert(id.filename(0).unwrap(), bytes.to_vec());
        spec.products.push(ProductSpec {
            id,
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            dependencies: vec![],
        });
    }
    (spec, files)
}

macro_rules! native_bytes {
    ($path:literal) => {
        include_bytes!(concat!(
            "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/",
            $path
        ))
    };
}

fn source_area(reverse: bool) -> AreaSnapshot {
    let tree: &[u8] = if reverse {
        native_bytes!("scopedarea/reverse/000.vmtree")
    } else {
        native_bytes!("scopedarea/forward/000.vmtree")
    };
    // Source serializes identical tile/model bytes; only map BIH order changes.
    let (spec, files) = source_fixture(
        tree,
        native_bytes!("scopedarea/forward/000_32_32.vmtile"),
        &[
            (
                "high.wmo.vmo",
                native_bytes!("scopedarea/forward/high.wmo.vmo"),
            ),
            (
                "low.wmo.vmo",
                native_bytes!("scopedarea/forward/low.wmo.vmo"),
            ),
            (
                "equal.wmo.vmo",
                native_bytes!("scopedarea/forward/equal.wmo.vmo"),
            ),
            (
                "multi.wmo.vmo",
                native_bytes!("scopedarea/forward/multi.wmo.vmo"),
            ),
            (
                "wet.wmo.vmo",
                native_bytes!("scopedarea/forward/wet.wmo.vmo"),
            ),
            (
                "high.m2.vmo",
                native_bytes!("scopedarea/forward/high.m2.vmo"),
            ),
        ],
    );
    products(spec, &files).unwrap().load_vmap_area().unwrap()
}

fn bits(text: &str) -> u32 {
    u32::from_str_radix(text, 16).unwrap()
}

#[test]
fn exact_original_source_manager_queries_preserve_all_independent_area_writes() {
    let (location_spec, location_files) = location_fixture();
    let (liquid_spec, liquid_files) = liquid_fixture();
    let snapshots = BTreeMap::from([
        ("base", snapshot()),
        (
            "scopedlocation",
            products(location_spec, &location_files)
                .unwrap()
                .load_vmap_area()
                .unwrap(),
        ),
        (
            "scopedliquid",
            products(liquid_spec, &liquid_files)
                .unwrap()
                .load_vmap_area()
                .unwrap(),
        ),
        ("scopedarea/forward", source_area(false)),
        ("scopedarea/reverse", source_area(true)),
    ]);
    let mut count = 0;
    let mut hits = 0;
    let mut metadata_splits = 0;
    let mut mismatched_maps = 0;
    for line in include_str!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedarea.tsv")
        .lines().filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 16, "{line}");
        let snapshot = &snapshots[row[0]];
        let query = AreaRequest {
            snapshot: SnapshotBinding { map: row[2].parse().unwrap(), ..snapshot.binding() },
            ticket: count + 1,
            position: [bits(row[3]), bits(row[4]), bits(row[5])],
        };
        let expected = if row[10] == "1" {
            hits += 1;
            let area = AreaResult::Area {
                floor: bits(row[11]), flags: row[12].parse().unwrap(),
                adt_id: row[13].parse().unwrap(), root_id: row[14].parse().unwrap(),
                group_id: row[15].parse().unwrap(),
            };
            let selected = snapshot.geometry.select_location(
                query.position.map(f32::from_bits), &mut QueryBudget::new(deadline()),
            ).unwrap().unwrap().floor;
            assert_eq!(selected.floor, bits(row[11]), "{} highest floor", row[1]);
            assert_eq!(u32::from(selected.adt_id), row[13].parse::<u32>().unwrap(), "{} highest-floor ADT", row[1]);
            metadata_splits += usize::from(
                selected.root_id != row[14].parse::<u32>().unwrap()
                    || selected.group_id != row[15].parse::<u32>().unwrap()
                    || selected.group_flags != row[12].parse::<u32>().unwrap()
            );
            area
        } else {
            assert_eq!(row[5], row[11], "{} no-hit z untouched", row[1]);
            assert_eq!(&row[6..10], &row[12..16], "{} no-hit metadata untouched", row[1]);
            AreaResult::None
        };
        if query.snapshot.map != snapshot.binding().map {
            mismatched_maps += 1;
            assert_eq!(expected, AreaResult::None);
            // A bound snapshot rejects a different map instead of claiming absence.
            assert_eq!(snapshot.query(query, deadline()), Err(Error::Identity));
        } else {
            let actual = snapshot.query(query, deadline()).unwrap_or_else(|error| {
                panic!("{} {}: {error:?}", row[0], row[1])
            });
            assert_eq!(actual.request, query);
            assert_eq!(actual.result, expected, "{} {}", row[0], row[1]);
        }
        count += 1;
    }
    assert_eq!(count, 14_795);
    assert_eq!(hits, 8_382);
    assert_eq!(count - hits, 6_413);
    assert_eq!(metadata_splits, 499);
    assert_eq!(mismatched_maps, 6);
}

#[test]
fn exact_source_callback_and_candidate_order_matches_loaded_map_bih() {
    let forward = source_area(false);
    let reverse = source_area(true);
    let mut count = 0;
    for line in include_str!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedarea_trace.tsv")
        .lines().filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 12, "{line}");
        let snapshot = match row[0] {
            "scopedarea/forward" => &forward,
            "scopedarea/reverse" => &reverse,
            other => panic!("unknown trace {other}"),
        };
        let position = [bits(row[2]), bits(row[3]), bits(row[4])].map(f32::from_bits);
        let mut budget = QueryBudget::new(deadline());
        let mut callbacks = Vec::new();
        let mut candidates = Vec::new();
        snapshot.geometry.tree.intersect_point(position, &mut budget, |id, budget| {
            let spawn = snapshot.geometry.spawns[id as usize].as_ref().unwrap();
            callbacks.push(spawn.id);
            if let Some((candidate, _)) = spawn.location_candidate(
                &snapshot.geometry.models[spawn.model], position, budget,
            )? {
                candidates.push(candidate);
            }
            Ok(())
        }).unwrap();
        let parse = |column: usize| -> Vec<u32> {
            if row[column] == "-" { vec![] } else {
                row[column].split(',').map(|value| {
                    if column == 7 { bits(value) } else { value.parse().unwrap() }
                }).collect()
            }
        };
        assert_eq!(callbacks, parse(5), "{} {} callbacks", row[0], row[1]);
        for (column, actual) in [
            (6, candidates.iter().map(|candidate| candidate.spawn_id).collect::<Vec<_>>()),
            (7, candidates.iter().map(|candidate| candidate.floor).collect()),
            (8, candidates.iter().map(|candidate| u32::from(candidate.adt_id)).collect()),
            (9, candidates.iter().map(|candidate| candidate.root_id).collect()),
            (10, candidates.iter().map(|candidate| candidate.group_id).collect()),
            (11, candidates.iter().map(|candidate| candidate.group_flags).collect()),
        ] {
            assert_eq!(actual, parse(column), "{} {} column {column}", row[0], row[1]);
        }
        count += 1;
    }
    assert_eq!(count, 18);
}
