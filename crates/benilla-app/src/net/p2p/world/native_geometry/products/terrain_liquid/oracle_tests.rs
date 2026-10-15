// SPDX-License-Identifier: GPL-3.0-or-later
// Full original-source composite over authored serialized MAPS/WDBC/VMap bytes.
use super::super::{terrain::GridLiquidData, tests as products, PointHeight, ProductId, TileId};
use super::*;
use std::time::Duration;

const MAPS: [&[u8]; 29] = [
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/0.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/1.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/2.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/3.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/4.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/5.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/6.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/7.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/8.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/9.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/10.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/11.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/12.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/13.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/14.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/15.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/16.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/17.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/18.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/19.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/20.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/21.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/22.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/23.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/24.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/25.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/26.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/27.map.test-fixture"),
    include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/28.map.test-fixture"),
];
const TYPES: &[u8] = include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/LiquidType.dbc.test-fixture");
const AREAS: &[u8] = include_bytes!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid/AreaTable.dbc.test-fixture");
const CASES: &str = include_str!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-liquid.tsv"
);

fn bits(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn number(value: &str) -> u32 {
    value.parse().unwrap()
}
fn data(row: &[&str], start: usize) -> GridLiquidData {
    GridLiquidData {
        entry: number(row[start]),
        type_flags: number(row[start + 1]),
        level: bits(row[start + 2]),
        depth_level: bits(row[start + 3]),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn loaded_composite_matches_full_original_source_and_intermediate_facts() {
    let mut count = 0;
    let mut retained = 0;
    let mut filtered = 0;
    let mut overwritten = 0;
    let mut no_location = 0;
    let mut statuses = [false; 16];
    let sentinel = GridLiquidData {
        entry: 0xdead_beef,
        type_flags: 0xdeca_fbad,
        level: 0x4a12_3456,
        depth_level: 0xca65_4321,
    };
    for (profile, bytes) in MAPS.iter().enumerate() {
        let (mut spec, mut files) = super::tests::fixture();
        let tile_id = TileId { x: 0, y: 0 };
        products::mutate(&mut spec, &mut files, &ProductId::Terrain(tile_id), |b| {
            *b = bytes.to_vec();
        });
        let products = products::load(spec, &files).unwrap();
        let snapshot = products
            .load_terrain_liquid_with_areas(TYPES, AREAS)
            .unwrap();
        let tile = products.terrain(0, tile_id).unwrap();
        let mut profile_count = 0;
        for line in CASES
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 25, "{line}");
            if number(row[0]) as usize != profile {
                continue;
            }
            let label = format!("profile{profile}:{}", row[1]);
            assert_eq!((number(row[23]), number(row[24])), (0, 0), "{label}");
            let query = TerrainLiquidRequest {
                snapshot: snapshot.binding(),
                ticket: count + 1,
                position: [bits(row[2]), bits(row[3]), bits(row[4])],
                required_type: number(row[6]).try_into().unwrap(),
                collision_height: bits(row[5]),
            };
            let reply = snapshot
                .query(query, deadline())
                .unwrap_or_else(|error| panic!("{label}: {error:?}"));
            assert_eq!(reply.request, query, "{label}");
            assert_eq!(reply.result.status as u32, number(row[8]), "{label}");
            let status = number(row[8]);
            let wants_data = number(row[7]) != 0;
            if status == 0 {
                assert_eq!(reply.result.data, None, "{label}");
            } else {
                // Typed replies always carry surviving raw data. Source null-output
                // calls prove status parity and leave its separate output untouched.
                assert_eq!(reply.result.data, Some(data(&row, 18)), "{label}");
                retained += 1;
            }
            assert_eq!(
                data(&row, 9),
                if status != 0 && wants_data {
                    data(&row, 18)
                } else {
                    sentinel
                },
                "{label}"
            );
            let p = query.position.map(f32::from_bits);
            let raw = tile
                .grid_liquid_status_with_stores(
                    p[0],
                    p[1],
                    p[2],
                    query.required_type,
                    f32::from_bits(query.collision_height),
                    LiquidStores {
                        types: &snapshot.types,
                        areas: snapshot.areas.as_ref(),
                    },
                )
                .unwrap();
            assert_eq!(raw.status as u32, number(row[17]), "{label}");
            if raw.status == GridLiquidStatus::NoWater {
                assert_eq!(raw.data, None, "{label}");
                assert_eq!(data(&row, 18), sentinel, "{label}");
            } else {
                assert_eq!(raw.data, Some(data(&row, 18)), "{label}");
            }
            let mut budget = QueryBudget::new(deadline());
            let ground = height::height(
                &products,
                &snapshot.vmap,
                p,
                DEFAULT_WATER_SEARCH,
                &mut budget,
            )
            .unwrap();
            assert_eq!(ground.to_bits(), bits(row[15]), "{label}");
            let location = snapshot.vmap.location(p, &mut budget).unwrap();
            let effective = match location {
                StaticLocation::None => {
                    assert_eq!(number(row[13]), 0, "{label}");
                    assert_eq!(bits(row[14]), f32::NEG_INFINITY.to_bits(), "{label}");
                    no_location += 1;
                    ground.to_bits()
                }
                StaticLocation::Floor(floor) => {
                    assert_eq!(number(row[13]), 1, "{label}");
                    assert_eq!(floor.floor, bits(row[14]), "{label}");
                    overwritten += usize::from(floor.floor != ground.to_bits());
                    floor.floor
                }
            };
            assert_eq!(effective, bits(row[16]), "{label}");
            let raw_height = match tile.point_height(p[0], p[1]).unwrap() {
                PointHeight::Height(height) => height,
                PointHeight::Hole => -200_000.0,
            };
            assert_eq!(raw_height.to_bits(), bits(row[22]), "{label}");
            filtered += usize::from(number(row[17]) != 0 && status == 0);
            statuses[status as usize] = true;
            profile_count += 1;
            count += 1;
        }
        assert!(profile_count > 0, "profile{profile} was not exercised");
    }
    assert_eq!(
        (count, retained, filtered, overwritten, no_location),
        (5048, 1725, 857, 2998, 1463)
    );
    for status in [0, 1, 2, 4, 8] {
        assert!(statuses[status]);
    }
}
