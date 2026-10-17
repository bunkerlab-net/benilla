// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::{tests as product_tests, InventorySpec, ProductId, ProductSpec};
use super::super::tests::{fixture as base_fixture, snapshot as base_snapshot};
use super::*;
use std::{collections::BTreeMap, time::Duration};

const TREE: &[u8] = include_bytes!(
    "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation/000.vmtree"
);
const TILE: &[u8] = include_bytes!(
    "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation/000_32_32.vmtile"
);
const MODELS: [(&str, &[u8]); 3] = [
    ("volume.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation/volume.wmo.vmo")),
    ("raised.wmo.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation/raised.wmo.vmo")),
    ("volume.m2.vmo", include_bytes!("../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation/volume.m2.vmo")),
];

pub(crate) fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = base_fixture();
    for product in &spec.products {
        if matches!(product.id, ProductId::VmapModel(_)) {
            files.remove(&product.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        *bytes = TREE.to_vec();
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
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| *bytes = TILE.to_vec());
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
    }
    for (name, bytes) in MODELS {
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

fn snapshot() -> VmapSnapshot {
    let (spec, files) = fixture();
    product_tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn request(snapshot: &VmapSnapshot, internal: Vector) -> LocationRequest {
    LocationRequest {
        snapshot: snapshot.location_binding(),
        ticket: 1,
        position: to_internal(internal).map(f32::to_bits),
    }
}

#[test]
fn location_policy_and_every_identity_field_are_bound() {
    let snapshot = base_snapshot();
    let original = request(&snapshot, [0.0, 0.0, 0.0]);
    assert_ne!(
        snapshot.binding().policy,
        snapshot.location_binding().policy
    );
    for field in 0..7 {
        let mut query = original;
        match field {
            0 => query.snapshot.map += 1,
            1 => query.snapshot.content = [0; 32],
            2 => query.snapshot.source_epoch = [0; 32],
            3 => query.snapshot.inventory = [0; 32],
            4 => query.snapshot.policy = [0; 32],
            5 => query.ticket = 0,
            _ => query.snapshot = snapshot.binding(),
        }
        assert_eq!(
            snapshot.query_location(query, deadline()),
            Err(Error::Identity)
        );
    }
    for position in [original.position, [0, (-0.0f32).to_bits(), 0]] {
        let query = LocationRequest {
            position,
            ..original
        };
        assert_eq!(
            snapshot.query_location(query, deadline()).unwrap().request,
            query
        );
    }
}

#[test]
fn nonfinite_out_of_profile_and_expired_requests_never_become_absence() {
    let snapshot = base_snapshot();
    let original = request(&snapshot, [0.0, 0.0, 0.0]);
    for axis in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut query = original;
            query.position[axis] = value.to_bits();
            assert_eq!(
                snapshot.query_location(query, deadline()),
                Err(Error::Format)
            );
        }
        for value in [100_000.0f32.next_up(), -100_000.0f32.next_up()] {
            let mut query = original;
            query.position[axis] = value.to_bits();
            assert_eq!(
                snapshot.query_location(query, deadline()),
                Err(Error::Unsupported)
            );
        }
    }
    assert_eq!(
        snapshot.query_location(original, Instant::now()),
        Err(Error::Deadline)
    );
}

#[test]
fn unrelated_unloaded_slots_preclude_even_an_outside_map_location() {
    let mut snapshot = base_snapshot();
    let query = LocationRequest {
        position: [100_000.0f32.to_bits(); 3],
        ..request(&snapshot, [0.0, 0.0, 0.0])
    };
    assert_eq!(
        snapshot.query_location(query, deadline()).unwrap().result,
        StaticLocation::None
    );
    snapshot.spawns[0] = None;
    assert_eq!(
        snapshot.query_location(query, deadline()),
        Err(Error::Missing)
    );
}

#[test]
fn location_and_height_share_work_and_absolute_deadline() {
    let snapshot = base_snapshot();
    let position = to_internal([0.0, 0.0, 0.0]);
    let mut budget = QueryBudget::new(deadline());
    let initial = budget.remaining;
    let location = snapshot.location(position, &mut budget).unwrap();
    assert!(matches!(location, StaticLocation::Floor(_)));
    let cost = initial - budget.remaining;
    assert!(cost > snapshot.spawns.len());
    budget.remaining = cost * 2 - 1;
    assert_eq!(snapshot.location(position, &mut budget).unwrap(), location);
    assert_eq!(snapshot.location(position, &mut budget), Err(Error::Budget));
    assert_eq!(budget.remaining, 0);
    assert_eq!(
        snapshot.height(position, 20.0, &mut budget),
        Err(Error::Budget)
    );
    assert_eq!(budget.complete(), Ok(()));
    budget.deadline = Instant::now();
    assert_eq!(budget.complete(), Err(Error::Deadline));
    assert_eq!(snapshot.location(position, &mut budget), Err(Error::Budget));
}

#[test]
fn raw_root_group_flags_and_ids_survive_loaded_bytes_and_bind_the_revision() {
    let original = base_snapshot();
    let (mut spec, mut files) = base_fixture();
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
    let changed = product_tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap();
    assert_ne!(original.location_binding(), changed.location_binding());
    let query = request(&changed, [0.0, 0.0, 0.0]);
    let StaticLocation::Floor(floor) = changed.query_location(query, deadline()).unwrap().result
    else {
        panic!("authored ramp's exact surface must have a location");
    };
    assert_eq!(floor.root_id, 0xfedc_ba98);
    assert_eq!(floor.group_flags, 0xaabb_ccdd);
    assert_eq!(floor.group_id, 0xdead_beef);
    assert_eq!(
        original.query_location(query, deadline()),
        Err(Error::Identity)
    );
    files.values_mut().for_each(Vec::clear);
    assert_eq!(
        changed.query_location(query, deadline()).unwrap().result,
        StaticLocation::Floor(floor)
    );
}

fn bits(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}

#[test]
fn loaded_byte_locations_match_original_cpp_selection_and_floor_bits() {
    let base = base_snapshot();
    let scoped = snapshot();
    let mut count = 0;
    let mut hits = 0;
    let mut different_heights = 0;
    for line in include_str!(
        "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation.tsv"
    ).lines().filter(|line| !line.starts_with('#') && !line.is_empty()) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 21, "{line}");
        let snapshot = match row[0] {
            "base" => &base,
            "scopedlocation" => &scoped,
            dataset => panic!("unknown oracle dataset {dataset}"),
        };
        let query = LocationRequest {
            snapshot: snapshot.location_binding(),
            ticket: count + 1,
            position: [bits(row[2]), bits(row[3]), bits(row[4])],
        };
        let internal = [bits(row[5]), bits(row[6]), bits(row[7])];
        assert_eq!(to_internal(query.position.map(f32::from_bits)).map(f32::to_bits), internal, "{} conversion", row[1]);
        let expected = if row[8] == "1" {
            hits += 1;
            different_heights += usize::from(bits(row[9]) != bits(row[20]));
            assert_eq!(row[19], "0", "{} dry source type", row[1]);
            assert_eq!(bits(row[18]), bits(row[9]), "{} dry source floor mutation", row[1]);
            StaticLocation::Floor(LocationFloor {
                floor: bits(row[9]),
                spawn_id: row[10].parse().unwrap(),
                root_id: row[11].parse::<i32>().unwrap() as u32,
                group_id: row[12].parse().unwrap(),
                group_flags: row[13].parse().unwrap(),
                adt_id: row[14].parse().unwrap(),
            })
        } else {
            assert_eq!(bits(row[9]), f32::NEG_INFINITY.to_bits(), "{} initial source ground", row[1]);
            assert_eq!(bits(row[18]), (-100_000.0f32).to_bits(), "{} no-location floor untouched", row[1]);
            assert_eq!(row[19], "305419896", "{} no-location type untouched", row[1]);
            StaticLocation::None
        };
        assert_eq!(row[16], "0", "{} dry source liquid return", row[1]);
        assert_eq!(bits(row[17]), 123.25f32.to_bits(), "{} dry source liquid level untouched", row[1]);
        let reply = snapshot.query_location(query, deadline()).unwrap();
        assert_eq!(reply.request, query, "{} binding", row[1]);
        assert_eq!(reply.result, expected, "{} {} selection", row[0], row[1]);
        count += 1;
    }
    assert_eq!(count, 2032);
    assert_eq!(hits, 1040);
    assert!(different_heights > 100);
}

#[test]
fn selected_candidate_does_not_hide_later_work_exhaustion() {
    let snapshot = snapshot();
    let position = to_internal([0.0, 0.0, 5.0]);
    let query = request(&snapshot, [0.0, 0.0, 5.0]);
    assert!(matches!(
        snapshot.query_location(query, deadline()).unwrap().result,
        StaticLocation::Floor(_)
    ));
    let mut full = QueryBudget::new(deadline());
    let initial = full.remaining;
    snapshot.location(position, &mut full).unwrap();
    let cost = initial - full.remaining;
    // No partial answer can escape at any cut, including after an earlier floor.
    for allowance in 0..cost {
        let mut budget = QueryBudget {
            remaining: allowance,
            deadline: deadline(),
        };
        assert_eq!(
            snapshot.location(position, &mut budget),
            Err(Error::Budget),
            "allowance {allowance}/{cost}"
        );
    }
    let mut exact = QueryBudget {
        remaining: cost,
        deadline: deadline(),
    };
    assert!(matches!(
        snapshot.location(position, &mut exact).unwrap(),
        StaticLocation::Floor(_)
    ));
    assert_eq!(exact.remaining, 0);
}

#[test]
fn native_empty_tile_bytes_cannot_turn_unresolved_map_geometry_into_no_location() {
    let (mut spec, mut files) = base_fixture();
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
    let snapshot = product_tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap();
    for position in [to_internal([0.0, 0.0, 0.0]), [100_000.0; 3]] {
        let query = LocationRequest {
            position: position.map(f32::to_bits),
            ..request(&snapshot, [0.0, 0.0, 0.0])
        };
        assert_eq!(
            snapshot.query_location(query, deadline()),
            Err(Error::Missing)
        );
    }
}

#[test]
fn nonempty_liquid_bytes_never_publish_a_location_snapshot() {
    let (mut spec, mut files) = base_fixture();
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::VmapModel("ramp.wmo.vmo".into()),
        |bytes| {
            let offset = bytes.windows(4).position(|word| word == b"LIQU").unwrap();
            bytes[offset + 4..offset + 8].copy_from_slice(&1u32.to_le_bytes());
        },
    );
    assert!(product_tests::load(spec, &files)
        .and_then(|products| products.load_static_vmap())
        .is_err());
}
