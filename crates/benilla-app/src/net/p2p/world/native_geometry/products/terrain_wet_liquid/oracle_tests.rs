// SPDX-License-Identifier: GPL-3.0-or-later
// Complete original reader/query closure over numeric, authored native fixtures.
use super::super::{
    envelopes, terrain_area, tests as products, vmap::area::AreaResult, InventorySpec, PointHeight,
    ProductId, ProductSpec, TileId,
};
use super::*;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

const CASES: &str = include_str!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-wet-liquid.tsv"
);
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../benilla-native-geometry-tests/tools/fixtures")
}
fn read(relative: &str) -> Vec<u8> {
    std::fs::read(fixture_root().join(relative)).unwrap()
}
fn bits(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn number(value: &str) -> u32 {
    value.parse().unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn fixture(
    dataset: &str,
    profile: u32,
    tile: TileId,
) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    let (spec, files) = super::tests::fixture();
    let (mut spec, mut files) = super::tests::remap(spec, files, tile);
    let directory = if dataset.starts_with("terrain-wet-vmap") {
        dataset.to_owned()
    } else {
        format!("vmap_source/{dataset}")
    };
    let native_tile = read(&format!("{directory}/000_32_32.vmtile"));
    let dependencies: Vec<_> = std::iter::once(ProductId::VmapTree)
        .chain(
            envelopes::validate_with_vmap_liquid(&ProductId::VmapTile(tile), &native_tile).unwrap(),
        )
        .collect();
    for product in &spec.products {
        if matches!(product.id, ProductId::VmapModel(_)) {
            files.remove(&product.id.filename(0).unwrap());
        }
    }
    spec.products
        .retain(|product| !matches!(product.id, ProductId::VmapModel(_)));
    products::mutate(&mut spec, &mut files, &ProductId::VmapTree, |bytes| {
        *bytes = read(&format!("{directory}/000.vmtree"));
    });
    let terrain = read(&format!("terrain-wet-liquid/{profile}.map.test-fixture"));
    for tile in spec.tiles.clone() {
        let id = ProductId::VmapTile(tile);
        products::mutate(&mut spec, &mut files, &id, |bytes| {
            *bytes = native_tile.clone();
        });
        spec.products
            .iter_mut()
            .find(|product| product.id == id)
            .unwrap()
            .dependencies = dependencies.clone();
        products::mutate(&mut spec, &mut files, &ProductId::Terrain(tile), |bytes| {
            *bytes = terrain.clone();
        });
    }
    for id in dependencies {
        if let ProductId::VmapModel(ref name) = id {
            let bytes = read(&format!("{directory}/{name}"));
            spec.products.push(ProductSpec {
                id: id.clone(),
                bytes: bytes.len() as u64,
                sha256: Sha256::digest(&bytes).into(),
                dependencies: vec![],
            });
            files.insert(id.filename(0).unwrap(), bytes);
        }
    }
    (spec, files)
}
fn expected_data(
    row: &[&str],
    status: usize,
    entry: usize,
    flags: usize,
    level: usize,
) -> GridLiquidResult {
    let status = match number(row[status]) {
        0 => GridLiquidStatus::NoWater,
        1 => GridLiquidStatus::AboveWater,
        2 => GridLiquidStatus::WaterWalk,
        4 => GridLiquidStatus::InWater,
        8 => GridLiquidStatus::UnderWater,
        value => panic!("unknown source liquid status {value}"),
    };
    GridLiquidResult {
        status,
        data: (status != GridLiquidStatus::NoWater).then(|| GridLiquidData {
            entry: number(row[entry]),
            type_flags: number(row[flags]),
            level: bits(row[level]),
            depth_level: bits(row[level + 1]),
        }),
    }
}
fn resolved(row: &[&str], start: usize) -> Option<terrain_area::ResolvedArea> {
    (row[start] == "1").then(|| terrain_area::ResolvedArea {
        id: number(row[start + 1]),
        map_id: number(row[start + 2]),
        zone: number(row[start + 3]),
        explore_flag: number(row[start + 4]),
        liquid_override: number(row[start + 5]),
    })
}

#[test]
fn complete_original_wet_terrain_reader_chain_matches_every_authored_source_row() {
    let mut groups: BTreeMap<(&str, u32, u8, u8), Vec<Vec<&str>>> = BTreeMap::new();
    for line in CASES
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 54);
        groups
            .entry((
                row[0],
                number(row[2]),
                number(row[52]) as u8,
                number(row[53]) as u8,
            ))
            .or_default()
            .push(row);
    }
    let mut data_present = 0;
    let mut status_only = 0;
    let mut source_only = 0;
    let mut source_only_null_data = 0;
    let mut rejected_wet = 0;
    let mut fallback = 0;
    let mut unknown_default = 0;
    let mut wide_flags = 0;
    let mut high_fallback = 0;
    let mut strict_z = 0;
    let mut no_mask_recheck = 0;
    for ((dataset, profile, x, y), rows) in groups {
        let tile = TileId { x, y };
        let (spec, files) = fixture(dataset, profile, tile);
        let products = super::tests::load(spec, &files).unwrap();
        for store in 0..5 {
            let selected: Vec<_> = rows.iter().filter(|row| number(row[1]) == store).collect();
            if selected.is_empty() {
                continue;
            }
            let prefix = format!("terrain-wet-liquid/store{store}");
            let types = read(&format!("{prefix}/LiquidType.dbc.test-fixture"));
            let areas = read(&format!("{prefix}/AreaTable.dbc.test-fixture"));
            let wmos = read(&format!("{prefix}/WMOAreaTable.dbc.test-fixture"));
            let maps = read(&format!("{prefix}/Map.dbc.test-fixture"));
            let snapshot = products
                .load_terrain_wet_liquid(&types, &areas, &wmos, &maps)
                .unwrap();
            for row in selected {
                let context = format!("{dataset} store{store} profile{profile} {}", row[3]);
                let query = TerrainLiquidRequest {
                    snapshot: SnapshotBinding {
                        map: number(row[4]),
                        ..snapshot.binding()
                    },
                    ticket: data_present + status_only + source_only + 1,
                    position: [bits(row[5]), bits(row[6]), bits(row[7])],
                    required_type: number(row[9]).try_into().unwrap(),
                    collision_height: bits(row[8]),
                };
                let position = query.position.map(f32::from_bits);
                if world_tile(position[0], position[1]) == Err(Error::OutsideTile) {
                    // Defined source truncation maps a negative fractional grid
                    // coordinate to grid0. The existing private terrain profile
                    // rejects that outer-edge domain; do not relax it for parity.
                    assert_eq!(
                        snapshot.query(query, deadline()),
                        Err(Error::OutsideTile),
                        "{context} source-only outer edge"
                    );
                    source_only += 1;
                    source_only_null_data += u64::from(row[10] == "0");
                    continue;
                }
                let actual = snapshot
                    .query(query, deadline())
                    .unwrap_or_else(|error| panic!("{context}: {error:?}"));
                assert_eq!(actual.request, query, "{context}");
                let expected = expected_data(row, 11, 12, 13, 14);
                if row[10] == "1" {
                    assert_eq!(actual.result, expected, "{context}");
                    data_present += 1;
                    wide_flags += usize::from(number(row[13]) == 1u32 << 31);
                    unknown_default +=
                        usize::from(row[17] == "1" && row[20] == "305419896" && row[13] == "1");
                    high_fallback +=
                        usize::from(row[17] == "0" && row[12] == "65536" && row[13] == "16");
                    no_mask_recheck += usize::from(
                        row[17] == "1"
                            && query.required_type != 0
                            && actual.result.data.is_some_and(|data| {
                                data.type_flags & u32::from(query.required_type) == 0
                            }),
                    );
                } else {
                    // Source nullptr skips wet DBC/area work; the data-present API
                    // is compared for status only, not claimed to model nullptr.
                    assert_eq!(actual.result.status, expected.status, "{context}");
                    status_only += 1;
                }
                if row[10] == "0" || row[11] == "0" {
                    assert_eq!(
                        (
                            number(row[12]),
                            number(row[13]),
                            bits(row[14]),
                            bits(row[15])
                        ),
                        (0xdeadbeef, 0xdecafbad, 0x4a123456, 0xca654321),
                        "{context} caller writes"
                    );
                }
                let position = query.position.map(f32::from_bits);
                assert_eq!(world_tile(position[0], position[1]).unwrap(), tile);
                let initial = snapshot
                    .liquid
                    .static_height(
                        &products.inventory,
                        position,
                        50.0,
                        &mut QueryBudget::new(deadline()),
                    )
                    .unwrap();
                assert_eq!(
                    initial.to_bits(),
                    bits(row[16]),
                    "{context} initial static height"
                );
                let raw_liquid = snapshot
                    .liquid
                    .liquid(
                        position,
                        query.required_type,
                        &mut QueryBudget::new(deadline()),
                    )
                    .unwrap();
                match raw_liquid {
                    LiquidResult::NoLocation => assert_eq!(row[48], "0", "{context}"),
                    LiquidResult::Location {
                        location,
                        liquid_type,
                        level,
                    } => {
                        assert_eq!(row[48], "1", "{context}");
                        assert_eq!(
                            (
                                location.floor,
                                location.spawn_id,
                                location.root_id,
                                location.group_id,
                                liquid_type
                            ),
                            (
                                bits(row[19]),
                                number(row[49]),
                                row[50].parse::<i64>().unwrap() as u32,
                                number(row[51]),
                                number(row[20])
                            ),
                            "{context} raw liquid"
                        );
                        assert_eq!(
                            level,
                            (row[17] == "1").then(|| bits(row[18])),
                            "{context} raw level"
                        );
                    }
                }
                let terrain = products.inventory.terrain(0, tile).unwrap();
                let ground = match terrain.point_height(position[0], position[1]).unwrap() {
                    PointHeight::Height(height) => height.to_bits(),
                    PointHeight::Hole => (-200000f32).to_bits(),
                };
                assert_eq!(ground, bits(row[26]), "{context} raw height");
                assert_eq!(
                    u32::from(terrain.area_flag(position[0], position[1]).unwrap()),
                    number(row[27]),
                    "{context} raw AREA"
                );
                let raw = terrain
                    .grid_liquid_status_with_budget(
                        position,
                        query.required_type,
                        f32::from_bits(query.collision_height),
                        LiquidStores {
                            types: &snapshot.types,
                            areas: Some(snapshot.area.areas()),
                        },
                        &mut QueryBudget::new(deadline()),
                    )
                    .unwrap();
                assert_eq!(
                    raw,
                    expected_data(row, 21, 22, 23, 24),
                    "{context} raw GridMap"
                );
                let area = snapshot
                    .area
                    .query(
                        terrain_area::TerrainAreaRequest {
                            snapshot: snapshot.area.binding(),
                            ticket: query.ticket,
                            position: query.position,
                        },
                        deadline(),
                    )
                    .unwrap()
                    .result;
                let raw_area = if row[28] == "1" {
                    AreaResult::Area {
                        floor: bits(row[29]),
                        flags: number(row[30]),
                        adt_id: number(row[31]).try_into().unwrap(),
                        root_id: number(row[32]),
                        group_id: number(row[33]),
                    }
                } else {
                    AreaResult::None
                };
                assert_eq!(area.raw_area, raw_area, "{context} independent raw area");
                assert_eq!(
                    area.area_info,
                    if row[34] == "1" {
                        raw_area
                    } else {
                        AreaResult::None
                    },
                    "{context} terrain occlusion"
                );
                assert_eq!(
                    u32::from(area.area_flag),
                    number(row[35]),
                    "{context} selected AREA flag"
                );
                assert_eq!(
                    (area.area, area.parent_area),
                    (resolved(row, 36), resolved(row, 42)),
                    "{context} original DBC resolution"
                );
                rejected_wet += usize::from(row[17] == "1" && row[11] == "0");
                fallback += usize::from(row[17] == "0" && row[11] != "0");
                strict_z += usize::from(
                    row[3] == "z_boundary_scale"
                        && row[17] == "1"
                        && row[11] == "0"
                        && row[21] != "0",
                );
            }
        }
    }
    assert_eq!((data_present, status_only), (25836, 235));
    assert_eq!((source_only, source_only_null_data), (1495, 25));
    assert_eq!((rejected_wet, fallback), (555, 9288));
    assert!(
        unknown_default > 0
            && wide_flags > 0
            && high_fallback > 0
            && strict_z > 0
            && no_mask_recheck > 0
    );
}
