// SPDX-License-Identifier: GPL-3.0-or-later
// Authored complete roots passed through the pinned WMO extractor and assembler.
use super::super::{tests as product_tests, InventorySpec, ProductSpec};
use super::*;
use std::{ops::Range, time::Duration};

const TREE: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/products/000.vmtree"
);
const TILE: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/products/000_32_32.vmtile"
);
const MODEL: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/products/Authored.wmo.vmo"
);
const RAW: &[u8] = include_bytes!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo.raw.vmap"
);
const MODEL_NAME: &str = "Authored.wmo.vmo";

fn fixture() -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (mut spec, mut files) = product_tests::fixture();
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
    let model = ProductId::VmapModel(MODEL_NAME.into());
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| *bytes = TILE.to_vec());
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = vec![ProductId::VmapTree, model.clone()];
    }
    files.insert(model.filename(0).unwrap(), MODEL.to_vec());
    spec.products.push(ProductSpec {
        id: model,
        bytes: MODEL.len() as u64,
        sha256: Sha256::digest(MODEL).into(),
        dependencies: vec![],
    });
    (spec, files)
}

fn snapshot() -> VmapSnapshot {
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

fn word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn chunk(bytes: &[u8], name: &[u8; 4]) -> usize {
    bytes.windows(4).position(|bytes| bytes == name).unwrap()
}

fn group_ranges() -> Vec<Range<usize>> {
    let mut cursor = Cursor::new(MODEL);
    cursor.magic(b"VMAP_7.0WMOD").unwrap();
    assert_eq!(cursor.word().unwrap(), 8);
    assert_eq!(cursor.word().unwrap(), 4242);
    cursor.magic(b"GMOD").unwrap();
    let count = cursor.count().unwrap();
    let mut ranges = Vec::new();
    for _ in 0..count {
        let start = cursor.at;
        Group::decode(&mut cursor, false, false).unwrap();
        ranges.push(start..cursor.at);
    }
    cursor.magic(b"GBIH").unwrap();
    Bih::decode(&mut cursor).unwrap();
    cursor.finish().unwrap();
    ranges
}

fn tile_spawns() -> Vec<(Range<usize>, Spawn)> {
    let ids = BTreeMap::from([(MODEL_NAME, 0)]);
    let mut cursor = Cursor::new(TILE);
    cursor.magic(b"VMAP_7.0").unwrap();
    let count = cursor.count().unwrap();
    let mut result = Vec::new();
    for _ in 0..count {
        let start = cursor.at;
        let (_, spawn) = Spawn::decode(&mut cursor, &ids).unwrap();
        result.push((start..cursor.at, spawn));
    }
    cursor.finish().unwrap();
    result
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
fn exact_source_wmo_keeps_nonzero_bounds_and_retained_empty_group() {
    let snapshot = snapshot();
    snapshot.require_complete_dry_map().unwrap();
    assert_eq!(snapshot.models.len(), 1);
    assert_eq!(snapshot.spawns.len(), 2);
    assert!(snapshot
        .spawns
        .iter()
        .flatten()
        .all(|spawn| { spawn.classification() == ModelClassification::Wmo }));
    let model = &snapshot.models[0];
    assert_eq!(model.root_id, 4242);
    assert_eq!(
        model
            .groups
            .iter()
            .map(|group| group.id)
            .collect::<Vec<_>>(),
        [701, 702, 703]
    );
    // The complete six-group input omits unreachable/antiportal groups 704..706.
    // Group 703 is retained even though material filtering selects no triangles.
    assert!(model.groups[2].vertices.is_empty());
    assert!(model.groups[2].triangles.is_empty());
    assert!(model.groups[2].tree.is_none());
    for classification in [
        None,
        Some(ModelClassification::Wmo),
        Some(ModelClassification::M2),
    ] {
        let decoded = Model::decode_classified(MODEL, false, classification).unwrap();
        assert_eq!(decoded.groups.len(), 3);
    }
    let mut raw = Cursor::new(RAW);
    raw.magic(b"VMAPs05\0").unwrap();
    let triangle_count = raw.count().unwrap();
    assert_eq!(raw.count().unwrap(), model.groups.len());
    assert_eq!(raw.word().unwrap(), model.root_id);
    assert_eq!(
        triangle_count,
        model
            .groups
            .iter()
            .map(|group| group.triangles.len())
            .sum::<usize>()
    );
    for group in &model.groups {
        assert_eq!(raw.word().unwrap(), group.flags);
        assert_eq!(raw.word().unwrap(), group.id);
        let bounds = raw.bounds().unwrap();
        assert_eq!(
            group.bounds.low.map(f32::to_bits),
            bounds.low.map(f32::to_bits)
        );
        assert_eq!(
            group.bounds.high.map(f32::to_bits),
            bounds.high.map(f32::to_bits)
        );
        assert_ne!(group.bounds.low.map(f32::to_bits), [0; 3]);
        assert_ne!(group.bounds.high.map(f32::to_bits), [0; 3]);
        assert!(group
            .vertices
            .iter()
            .all(|&vertex| group.bounds.contains(vertex)));
        assert!(model.tree.as_ref().unwrap().bounds.encloses(group.bounds));
        assert!(group.liquid.is_none());
        assert_eq!(raw.word().unwrap(), 0); // Dry source liquid flags.
        raw.magic(b"GRP ").unwrap();
        let length = raw.count().unwrap();
        raw.take(length).unwrap();
        raw.magic(b"INDX").unwrap();
        let length = raw.count().unwrap();
        let count = raw.count().unwrap();
        assert_eq!(length, 4 + count * 2);
        assert_eq!(count, group.triangles.len() * 3);
        for expected in group.triangles.iter().flatten() {
            let index = u16::from_le_bytes(raw.take(2).unwrap().try_into().unwrap());
            assert_eq!(u32::from(index), *expected);
        }
        raw.magic(b"VERT").unwrap();
        let length = raw.count().unwrap();
        let count = raw.count().unwrap();
        assert_eq!(length, 4 + count * 12);
        assert_eq!(count, group.vertices.len());
        for expected in group.vertices.iter().flatten() {
            assert_eq!(raw.word().unwrap(), expected.to_bits());
        }
    }
    raw.finish().unwrap();
}

#[test]
fn exact_pinned_extractor_and_assembler_wmo_rays_match_source_bits() {
    let snapshot = snapshot();
    let mut count = 0;
    let mut hits = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/rays.tsv"
    ).lines().filter(|line| !line.starts_with('#') && !line.is_empty()) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 12, "{line}");
        let ray = Ray {
            origin: [number(row[2]), number(row[3]), number(row[4])],
            direction: [number(row[5]), number(row[6]), number(row[7])],
        };
        let mut distance = number(row[8]);
        let hit = match row[0] {
            "map" => matches!(snapshot.intersect_internal(ray, &mut distance, row[9] == "1", &mut budget()).unwrap(), StaticRay::Hit { .. }),
            "authored" | "rotated" => snapshot.models[0].intersect(ray, &mut distance, &mut budget()).unwrap(),
            scope => panic!("unknown WMO oracle scope {scope}"),
        };
        assert_eq!(hit, row[10] == "1", "{} {}", row[0], row[1]);
        assert_eq!(distance.to_bits(), bits(row[11]), "{} {} distance", row[0], row[1]);
        hits += usize::from(hit);
        count += 1;
    }
    assert_eq!(count, 2208);
    assert_eq!(hits, 1350);
}

#[test]
fn exact_source_wmo_heights_match_transformed_floors_and_misses() {
    let snapshot = snapshot();
    let mut count = 0;
    let mut hits = 0;
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/heights.tsv"
    ).lines().filter(|line| !line.starts_with('#') && !line.is_empty()) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 6, "{line}");
        let internal = [number(row[1]), number(row[2]), number(row[3])];
        let world = to_internal(internal);
        assert_eq!(to_internal(world).map(f32::to_bits), internal.map(f32::to_bits));
        let request = HeightRequest {
            snapshot: snapshot.binding(),
            ticket: count + 1,
            position: world.map(f32::to_bits),
            max_search: bits(row[4]),
        };
        let height = if number(row[5]) == f32::INFINITY {
            (-200_000.0f32).to_bits()
        } else {
            hits += 1;
            bits(row[5])
        };
        assert_eq!(snapshot.query_height(request, budget().deadline).unwrap(), HeightReply { request, height }, "{}", row[0]);
        count += 1;
    }
    assert_eq!(count, 540);
    assert_eq!(hits, 171);
}

#[test]
fn exact_source_wmo_locations_keep_group_identity_and_floor_bits() {
    let snapshot = snapshot();
    let mut count = 0;
    let mut hits = 0;
    let mut groups = std::collections::BTreeSet::new();
    let mut spawns = std::collections::BTreeSet::new();
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/locations.tsv"
    ).lines().filter(|line| !line.starts_with('#') && !line.is_empty()) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 14, "{line}");
        let request = LocationRequest {
            snapshot: snapshot.location_binding(),
            ticket: count + 1,
            position: [bits(row[1]), bits(row[2]), bits(row[3])],
        };
        assert_eq!(to_internal(request.position.map(f32::from_bits)).map(f32::to_bits), [bits(row[4]), bits(row[5]), bits(row[6])], "{} conversion", row[0]);
        let result = if row[7] == "1" {
            let floor = LocationFloor {
                floor: bits(row[8]),
                spawn_id: row[9].parse().unwrap(),
                root_id: row[10].parse().unwrap(),
                group_id: row[11].parse().unwrap(),
                group_flags: row[12].parse().unwrap(),
                adt_id: row[13].parse().unwrap(),
            };
            groups.insert(floor.group_id);
            spawns.insert(floor.spawn_id);
            hits += 1;
            StaticLocation::Floor(floor)
        } else {
            assert_eq!(bits(row[8]), f32::NEG_INFINITY.to_bits());
            StaticLocation::None
        };
        assert_eq!(snapshot.query_location(request, budget().deadline).unwrap(), LocationReply { request, result }, "{}", row[0]);
        count += 1;
    }
    assert_eq!(count, 540);
    assert_eq!(hits, 170);
    assert_eq!(groups.into_iter().collect::<Vec<_>>(), [701, 702]);
    assert_eq!(spawns.len(), 2);
}

#[test]
fn real_wmo_bounds_never_acquire_the_m2_zero_bounds_exception() {
    let model = Model::decode(MODEL).unwrap();
    for (group, range) in model.groups.iter().zip(group_ranges()) {
        if group.vertices.is_empty() {
            continue;
        }
        let mut changed = MODEL.to_vec();
        changed[range.start..range.start + 24].fill(0);
        for classification in [
            None,
            Some(ModelClassification::Wmo),
            Some(ModelClassification::M2),
        ] {
            assert!(matches!(
                Model::decode_classified(&changed, false, classification),
                Err(Error::Format)
            ));
        }
        for axis in 0..3 {
            let low = group
                .vertices
                .iter()
                .map(|vertex| vertex[axis])
                .reduce(f32::min)
                .unwrap();
            let mut changed = MODEL.to_vec();
            word(
                &mut changed,
                range.start + axis * 4,
                low.next_up().to_bits(),
            );
            assert!(
                matches!(Model::decode(&changed), Err(Error::Format)),
                "group {} axis {axis}",
                group.id
            );
        }
    }
}

#[test]
fn source_wmo_rejects_nonfinite_vertices_indices_and_invalid_hierarchies() {
    let ranges = group_ranges();
    let model = Model::decode(MODEL).unwrap();
    let mut mutations = Vec::new();
    for (group, range) in model.groups.iter().zip(&ranges) {
        for axis in 0..6 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                mutations.push((range.start + axis * 4, value.to_bits()));
            }
        }
        if group.vertices.is_empty() {
            continue;
        }
        let bytes = &MODEL[range.clone()];
        let vertex = range.start + chunk(bytes, b"VERT") + 12;
        let triangle = range.start + chunk(bytes, b"TRIM") + 12;
        let mesh = range.start + chunk(bytes, b"MBIH") + 4;
        for axis in 0..3 {
            for value in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                group.bounds.high[axis].next_up(),
            ] {
                mutations.push((vertex + axis * 4, value.to_bits()));
            }
        }
        for axis in 0..3 {
            mutations.push((triangle + axis * 4, group.vertices.len() as u32));
        }
        let minimum = group
            .vertices
            .iter()
            .map(|vertex| vertex[0])
            .reduce(f32::min)
            .unwrap();
        mutations.push((mesh, minimum.next_up().to_bits()));
        mutations.push((mesh + 28, 0x2000_0000)); // Invalid node offset / structure.
    }
    let groups = ranges.last().unwrap().end + 4;
    let high = model
        .groups
        .iter()
        .map(|group| group.bounds.high[0])
        .reduce(f32::max)
        .unwrap();
    mutations.push((groups + 12, high.next_down().to_bits()));
    mutations.push((groups + 28, 0x2000_0000));
    mutations.push((MODEL.len() - 4, model.groups.len() as u32));
    for (at, value) in mutations {
        let (mut spec, mut files) = fixture();
        product_tests::mutate(
            &mut spec,
            &mut files,
            &ProductId::VmapModel(MODEL_NAME.into()),
            |bytes| word(bytes, at, value),
        );
        assert!(
            matches!(
                product_tests::load(spec, &files).and_then(|products| products.load_static_vmap()),
                Err(Error::Format)
            ),
            "WMO mutation {at}: {value:08x}"
        );
    }
}

#[test]
fn source_wmo_requires_consistent_spawn_context_and_transformed_bounds() {
    let records = tile_spawns();
    assert_eq!(records.len(), 2);
    let (mut spec, mut files) = fixture();
    mutate_tiles(&mut spec, &mut files, |bytes| {
        word(bytes, records[0].0.start, 5)
    });
    assert!(matches!(
        product_tests::load(spec, &files)
            .unwrap()
            .load_static_vmap(),
        Err(Error::Unsupported)
    ));

    let (mut spec, mut files) = fixture();
    let tile = ProductId::VmapTile(spec.tiles[1]);
    product_tests::mutate(&mut spec, &mut files, &tile, |bytes| {
        word(bytes, records[0].0.start, 5)
    });
    assert!(matches!(
        product_tests::load(spec, &files)
            .unwrap()
            .load_static_vmap(),
        Err(Error::Dependency)
    ));

    let model = Model::decode(MODEL).unwrap();
    for (range, spawn) in records {
        let rotation = rotation(spawn.angles).unwrap();
        let minimum = model
            .groups
            .iter()
            .flat_map(|group| &group.vertices)
            .map(|vertex| apply(rotation, scale(*vertex, spawn.scale))[0] + spawn.position[0])
            .reduce(f32::min)
            .unwrap();
        let (mut spec, mut files) = fixture();
        mutate_tiles(&mut spec, &mut files, |bytes| {
            word(bytes, range.start + 38, minimum.next_up().to_bits())
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
    }
}

#[test]
fn source_wmo_model_rejects_every_partial_group_and_trailing_byte() {
    for length in 0..MODEL.len() {
        if length == 20 {
            continue; // Source-empty WMOD is a distinct valid representation.
        }
        assert!(
            Model::decode(&MODEL[..length]).is_err(),
            "WMO prefix {length}"
        );
    }
    let mut trailing = MODEL.to_vec();
    trailing.push(0);
    assert!(matches!(Model::decode(&trailing), Err(Error::Framing)));
}

#[test]
fn source_wmo_queries_reject_stale_nonfinite_and_expired_context() {
    let snapshot = snapshot();
    let position = to_internal([13., 17., 14.]).map(f32::to_bits);
    let ray = RayRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        from: position,
        to: to_internal([13., 17., 0.]).map(f32::to_bits),
        ignore_m2: true,
    };
    let height = HeightRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position,
        max_search: 30f32.to_bits(),
    };
    let location = LocationRequest {
        snapshot: snapshot.location_binding(),
        ticket: 1,
        position,
    };
    assert!(matches!(
        snapshot.query(ray, budget().deadline).unwrap().result,
        StaticRay::Hit { .. }
    ));
    assert!(matches!(
        snapshot
            .query_location(location, budget().deadline)
            .unwrap()
            .result,
        StaticLocation::Floor(_)
    ));
    for field in 0..6 {
        let mut stale = snapshot.binding();
        match field {
            0 => stale.map += 1,
            1 => stale.content = [0; 32],
            2 => stale.source_epoch = [0; 32],
            3 => stale.inventory = [0; 32],
            _ => stale.policy = [0; 32],
        }
        let ticket = if field == 5 { 0 } else { 1 };
        if field == 5 {
            stale = snapshot.binding();
        }
        assert_eq!(
            snapshot.query(
                RayRequest {
                    snapshot: stale,
                    ticket,
                    ..ray
                },
                budget().deadline
            ),
            Err(Error::Identity)
        );
        assert_eq!(
            snapshot.query_height(
                HeightRequest {
                    snapshot: stale,
                    ticket,
                    ..height
                },
                budget().deadline
            ),
            Err(Error::Identity)
        );
        let location_binding = SnapshotBinding {
            policy: if field == 4 {
                stale.policy
            } else {
                snapshot.location_binding().policy
            },
            ..stale
        };
        assert_eq!(
            snapshot.query_location(
                LocationRequest {
                    snapshot: location_binding,
                    ticket,
                    ..location
                },
                budget().deadline
            ),
            Err(Error::Identity)
        );
    }
    for axis in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut invalid = position;
            invalid[axis] = value.to_bits();
            assert_eq!(
                snapshot.query(
                    RayRequest {
                        from: invalid,
                        ..ray
                    },
                    budget().deadline
                ),
                Err(Error::Format)
            );
            assert_eq!(
                snapshot.query_height(
                    HeightRequest {
                        position: invalid,
                        ..height
                    },
                    budget().deadline
                ),
                Err(Error::Format)
            );
            assert_eq!(
                snapshot.query_location(
                    LocationRequest {
                        position: invalid,
                        ..location
                    },
                    budget().deadline
                ),
                Err(Error::Format)
            );
        }
    }
    assert_eq!(snapshot.query(ray, Instant::now()), Err(Error::Deadline));
    assert_eq!(
        snapshot.query_height(height, Instant::now()),
        Err(Error::Deadline)
    );
    assert_eq!(
        snapshot.query_location(location, Instant::now()),
        Err(Error::Deadline)
    );
}
