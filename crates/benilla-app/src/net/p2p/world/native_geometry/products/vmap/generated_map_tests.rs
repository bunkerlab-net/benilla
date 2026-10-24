// SPDX-License-Identifier: GPL-3.0-or-later
//! Generated source map, tile and model bytes through the unchanged inventory and
//! static-query consumers. Expected bytes and public query results are captured
//! independently from the pinned original CMaNGOS assembler and VMapManager2.
use super::super::{tests as product_tests, InventorySpec, ProductSpec, TileId};
use super::*;
use benilla_native_vmap::SourceModelName;
use serde_json::Value;
use std::time::Duration;

const CASES: [&str; 2] = ["query_wmo_plain_name", "query_wmo_shortened_name"];
const TILE: TileId = TileId { x: 31, y: 30 };
const PRODUCTS: &str = include_str!(
    "../../../../../../../../benilla-native-vmap-source/tools/fixtures/map-products.tsv"
);
const QUERIES: &str = include_str!(
    "../../../../../../../../benilla-native-vmap-source/tools/fixtures/map-queries.tsv"
);

fn unhex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn bits(value: &Value) -> u32 {
    u32::from_str_radix(value.as_str().unwrap(), 16).unwrap()
}

fn point_bits(value: &Value) -> [u32; 3] {
    let values = value.as_array().unwrap();
    assert_eq!(values.len(), 3);
    std::array::from_fn(|axis| bits(&values[axis]))
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn add(
    spec: &mut InventorySpec,
    files: &mut BTreeMap<String, Vec<u8>>,
    id: ProductId,
    bytes: Vec<u8>,
    dependencies: Vec<ProductId>,
) {
    let filename = id.filename(spec.map).unwrap();
    spec.products.push(ProductSpec {
        id,
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(&bytes).into(),
        dependencies,
    });
    assert!(files.insert(filename, bytes).is_none());
}

fn fixture(case: &str) -> (InventorySpec, BTreeMap<String, Vec<u8>>, SourceModelName) {
    let rows: Vec<_> = PRODUCTS
        .lines()
        .filter(|line| line.split('\t').next() == Some(case))
        .collect();
    assert_eq!(rows.len(), 1, "{case}");
    let fields: Vec<_> = rows[0].split('\t').collect();
    assert_eq!(fields.len(), 5);
    let dir_bin = unhex(fields[1]);
    // These two authored cases each contain exactly one complete bounded WMO
    // record. The helper still performs the real first-wins index and map BIH.
    let full_name = match case {
        "query_wmo_plain_name" => b"Hull.wmo".as_slice(),
        "query_wmo_shortened_name" => b"Hull.wmo\0".as_slice(),
        _ => panic!("unknown generated-map case {case}"),
    };
    assert_eq!(&dir_bin[78..], full_name);
    let name = SourceModelName::new(full_name).unwrap();
    let generated = benilla_native_vmap_source::assemble_authored_wmo_map(dir_bin, 1, || false)
        .unwrap()
        .unwrap();
    assert_eq!(generated.map_id(), 0);
    assert_eq!(generated.tiles().len(), 1);
    assert_eq!(generated.tiles()[0].coordinates(), (30, 31));
    assert_eq!(
        generated.spawn_model_names().collect::<Vec<_>>(),
        [full_name]
    );
    let raw: BTreeMap<String, String> = serde_json::from_str(fields[2]).unwrap();
    assert_eq!(raw.len(), 1);
    let model =
        benilla_native_vmap::assemble_raw_model(&unhex(&raw["Hull.wmo"]), || false).unwrap();
    let expected: BTreeMap<String, String> = serde_json::from_str(fields[3]).unwrap();
    let expected: BTreeMap<_, _> = expected
        .into_iter()
        .map(|(filename, bytes)| (filename, unhex(&bytes)))
        .collect();
    let effective_name = String::from_utf8(name.effective_source_basename().unwrap()).unwrap();
    let actual = BTreeMap::from([
        ("000.vmtree".to_owned(), generated.tree_bytes().to_vec()),
        (
            "000_30_31.vmtile".to_owned(),
            generated.tiles()[0].bytes().to_vec(),
        ),
        (effective_name, model.clone()),
    ]);
    // Full equality includes the original filenames, every map/tile byte and
    // every assembled model byte. None of these products is rewritten below.
    assert_eq!(actual, expected, "{case}: original complete products");
    let lookups: Vec<Value> = serde_json::from_str(fields[4]).unwrap();
    assert_eq!(lookups.len(), 2);
    assert_eq!(
        lookups.iter().filter(|row| row["loaded"] == true).count(),
        1
    );
    for row in lookups {
        let queried_name = SourceModelName::new(&unhex(row["name_hex"].as_str().unwrap())).unwrap();
        assert_eq!(
            queried_name.effective_source_basename().unwrap(),
            unhex(row["fopen_basename_hex"].as_str().unwrap())
        );
        assert_eq!(queried_name == name, row["loaded"].as_bool().unwrap());
    }

    let (mut spec, mut files) = product_tests::fixture();
    spec.tiles = vec![TILE];
    spec.products.retain(|product| {
        matches!(product.id, ProductId::MmapParams)
            || matches!(product.id, ProductId::Terrain(tile) | ProductId::MmapTile(tile) if tile == TILE)
    });
    files.retain(|filename, _| {
        spec.products
            .iter()
            .any(|product| product.id.filename(0).unwrap() == *filename)
    });
    let model_id = ProductId::VmapSourceModel(name.clone());
    add(
        &mut spec,
        &mut files,
        ProductId::VmapTree,
        generated.tree_bytes().to_vec(),
        vec![],
    );
    add(
        &mut spec,
        &mut files,
        ProductId::VmapTile(TILE),
        generated.tiles()[0].bytes().to_vec(),
        vec![ProductId::VmapTree, model_id.clone()],
    );
    add(&mut spec, &mut files, model_id, model, vec![]);
    (spec, files, name)
}

fn snapshot(case: &str) -> VmapSnapshot {
    let (spec, files, _) = fixture(case);
    let inventoried = product_tests::load(spec, &files).unwrap();
    let snapshot = inventoried.load_static_vmap().unwrap();
    snapshot.require_complete_dry_map().unwrap();
    snapshot
}

#[test]
fn generated_map_products_load_unchanged_with_exact_source_identities() {
    let mut filenames = Vec::new();
    let mut revisions = Vec::new();
    for case in CASES {
        let (spec, files, name) = fixture(case);
        assert_eq!(spec.tiles, [TILE]);
        assert_eq!(spec.products.len(), 6);
        let model_id = ProductId::VmapSourceModel(name.clone());
        let filename = model_id.filename(0).unwrap();
        assert_eq!(filename, format!("vmaps/{}", name.safe_basename().unwrap()));
        assert!(filename.starts_with("vmaps/source-"));
        assert!(!files.contains_key("vmaps/Hull.wmo"));
        assert!(!files.contains_key("vmaps/Hull.wmo.vmo"));
        assert_eq!(
            ProductId::VmapTile(TILE).filename(0).unwrap(),
            "vmaps/000_30_31.vmtile"
        );
        let inventoried = product_tests::load(spec, &files).unwrap();
        for id in [&ProductId::VmapTree, &ProductId::VmapTile(TILE), &model_id] {
            assert_eq!(
                inventoried.inventoried_bytes(0, id).unwrap(),
                files[&id.filename(0).unwrap()]
            );
        }
        assert!(matches!(
            inventoried.inventoried_bytes(0, &ProductId::VmapTile(TileId { x: 30, y: 31 })),
            Err(Error::Missing)
        ));
        let snapshot = inventoried.load_static_vmap().unwrap();
        snapshot.require_complete_dry_map().unwrap();
        assert_eq!(snapshot.models.len(), 1);
        assert_eq!(snapshot.spawns.len(), 1);
        assert_eq!(snapshot.models[0].root_id, 5875);
        assert_eq!(snapshot.models[0].groups.len(), 1);
        assert_eq!(snapshot.models[0].groups[0].id, 23);
        let spawn = snapshot.spawns[0].as_ref().unwrap();
        assert_eq!((spawn.id, spawn.adt, spawn.flags), (23, 11, 4));
        assert_eq!(spawn.classification(), ModelClassification::Wmo);
        assert_eq!(spawn.position, [16600.0, 16100.0, 4.0]);
        filenames.push(filename);
        revisions.push(inventoried.revision());
    }
    assert_ne!(filenames[0], filenames[1]);
    assert_ne!(revisions[0], revisions[1]);
}

// Reconstruct StaticMapTree::getObjectHitPos from the actual production query's
// unrounded distance. Subtracting the public rounded point from the start would
// lose source precision and would not test the distance-to-hit-point contract.
fn public_hit_point(from: [u32; 3], to: [u32; 3], result: StaticRay, modifier: f32) -> Vector {
    let from = to_internal(from.map(f32::from_bits));
    let to = to_internal(to.map(f32::from_bits));
    let internal = match result {
        StaticRay::Miss => to,
        StaticRay::Hit { distance } => {
            let delta = sub(to, from);
            let direction = scale(delta, 1.0 / dot(delta, delta).sqrt());
            let offset = scale(direction, f32::from_bits(distance));
            let hit: Vector = std::array::from_fn(|axis| from[axis] + offset[axis]);
            let traveled = sub(hit, from);
            if modifier < 0.0 && dot(traveled, traveled).sqrt() <= -modifier {
                from
            } else {
                let offset = scale(direction, modifier);
                std::array::from_fn(|axis| hit[axis] + offset[axis])
            }
        }
    };
    to_internal(internal)
}

#[test]
fn generated_map_public_height_los_and_hit_points_match_original_bits() {
    assert_public_queries(CASES.map(|case| (case, snapshot(case))), 132);
}

// Only the public query surface is shared; the oracle cannot obtain or invent
// terrain/nav products, convert a prepared view, or grant gameplay authority.
pub(super) trait PublicQueries {
    fn binding(&self) -> SnapshotBinding;
    fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error>;
    fn query_height(&self, request: HeightRequest, deadline: Instant)
        -> Result<HeightReply, Error>;
}
impl PublicQueries for VmapSnapshot {
    fn binding(&self) -> SnapshotBinding {
        VmapSnapshot::binding(self)
    }
    fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error> {
        VmapSnapshot::query(self, request, deadline)
    }
    fn query_height(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        VmapSnapshot::query_height(self, request, deadline)
    }
}

pub(super) fn assert_public_queries<'a, S: PublicQueries>(
    snapshots: impl IntoIterator<Item = (&'a str, S)>,
    expected_total: u64,
) {
    let mut total = 0;
    for (case, snapshot) in snapshots {
        let mut points = BTreeMap::new();
        let mut counts = [0; 5]; // load, coordinates, height, LOS, hit point
        let mut positives = [0; 3]; // height hits, blocked LOS, hit points
        for line in QUERIES
            .lines()
            .filter(|line| line.split('\t').next() == Some(case))
        {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 4);
            let query = fields[1];
            let request: Value = serde_json::from_str(fields[2]).unwrap();
            let expected: Value = serde_json::from_str(fields[3]).unwrap();
            if query == "load" {
                assert_eq!(request["map_id"], 0);
                assert_eq!(request["tile_x"], 31);
                assert_eq!(request["tile_y"], 30);
                assert_eq!(snapshot.binding().map, 0);
                assert_eq!(expected["load_result"], 1);
                assert_eq!(expected["tile_loaded"], true);
                assert_eq!(expected["transposed_tile_loaded"], false);
                counts[0] += 1;
            } else if let Some(name) = query.strip_prefix("coordinates_") {
                let internal = point_bits(&request["internal_bits"]).map(f32::from_bits);
                let world = to_internal(internal);
                assert_eq!(
                    world.map(f32::to_bits),
                    point_bits(&expected["world_bits"]),
                    "{case} {query}"
                );
                assert_eq!(
                    to_internal(world).map(f32::to_bits),
                    point_bits(&expected["roundtrip_internal_bits"]),
                    "{case} {query} roundtrip"
                );
                assert!(points
                    .insert(name.to_owned(), world.map(f32::to_bits))
                    .is_none());
                counts[1] += 1;
            } else if query.starts_with("height_") {
                let request = HeightRequest {
                    snapshot: snapshot.binding(),
                    ticket: total + 1,
                    position: points[request["position"].as_str().unwrap()],
                    max_search: bits(&request["max_search_dist_bits"]),
                };
                let reply = snapshot.query_height(request, deadline()).unwrap();
                assert_eq!(
                    reply,
                    HeightReply {
                        request,
                        height: bits(&expected["height_bits"])
                    },
                    "{case} {query}"
                );
                positives[0] += usize::from(reply.height != (-200_000.0f32).to_bits());
                counts[2] += 1;
            } else if query.starts_with("los_") || query.starts_with("hit_") {
                let is_los = query.starts_with("los_");
                let ray = RayRequest {
                    snapshot: snapshot.binding(),
                    ticket: total + 1,
                    from: points[request["start"].as_str().unwrap()],
                    to: points[request["end"].as_str().unwrap()],
                    ignore_m2: is_los && request["ignore_m2"].as_bool().unwrap(),
                };
                let reply = snapshot.query(ray, deadline()).unwrap();
                assert_eq!(reply.request, ray);
                let hit = matches!(reply.result, StaticRay::Hit { .. });
                if is_los {
                    assert_eq!(!hit, expected["clear"].as_bool().unwrap(), "{case} {query}");
                    positives[1] += usize::from(hit);
                    counts[3] += 1;
                } else {
                    assert_eq!(hit, expected["hit"].as_bool().unwrap(), "{case} {query}");
                    let world = public_hit_point(
                        ray.from,
                        ray.to,
                        reply.result,
                        f32::from_bits(bits(&request["modify_dist_bits"])),
                    );
                    assert_eq!(
                        world.map(f32::to_bits),
                        point_bits(&expected["world_bits"]),
                        "{case} {query} public hit point"
                    );
                    assert_eq!(
                        to_internal(world).map(f32::to_bits),
                        point_bits(&expected["internal_bits"]),
                        "{case} {query} hit roundtrip"
                    );
                    positives[2] += usize::from(hit);
                    counts[4] += 1;
                }
            } else {
                panic!("unknown original query {query}");
            }
            total += 1;
        }
        assert_eq!(counts, [1, 6, 10, 14, 35], "{case}");
        // Missing geometry cannot satisfy this oracle by returning only misses.
        assert_eq!(positives, [2, 4, 10], "{case}");
    }
    assert_eq!(total, expected_total);
}

#[test]
fn generated_map_missing_model_and_exact_name_mismatch_fail_closed() {
    for case in CASES {
        let (spec, mut files, name) = fixture(case);
        files.remove(&ProductId::VmapSourceModel(name).filename(0).unwrap());
        assert!(
            matches!(product_tests::load(spec, &files), Err(Error::Missing)),
            "{case}"
        );

        let (mut spec, mut files, _) = fixture(case);
        product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTile(TILE), |bytes| {
            let length = u32::from_le_bytes(bytes[74..78].try_into().unwrap()) as usize;
            let changed = if bytes[78 + length - 1] == 0 {
                bytes.remove(78 + length - 1);
                length - 1
            } else {
                bytes.insert(78 + length, 0);
                length + 1
            };
            bytes[74..78].copy_from_slice(&(changed as u32).to_le_bytes());
        });
        assert!(
            matches!(product_tests::load(spec, &files), Err(Error::Dependency)),
            "{case}: terminal NUL is part of the dependency identity"
        );
    }
}

#[test]
fn generated_map_altered_spawn_and_tree_bounds_fail_closed() {
    for case in CASES {
        for (id, offset, value) in [
            // Exclude the original model's x=-1 vertex after placement.
            (ProductId::VmapTile(TILE), 50, 16600.0f32),
            // Exclude the complete spawn's high x bound from the map hierarchy.
            (ProductId::VmapTree, 25, 16603.0f32),
        ] {
            let (mut spec, mut files, _) = fixture(case);
            product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            });
            let inventoried = product_tests::load(spec, &files).unwrap();
            assert!(
                matches!(inventoried.load_static_vmap(), Err(Error::Format)),
                "{case}: {id:?}"
            );
        }
    }
}
