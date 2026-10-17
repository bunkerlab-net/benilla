// SPDX-License-Identifier: GPL-3.0-or-later
// Original source loaders and full TerrainInfo area chain over authored numeric files.
use super::super::TileId;
use super::*;
use std::time::Duration;

macro_rules! fixture_bytes {
    ($path:literal) => {
        include_bytes!(concat!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain-area/",
            $path
        ))
    };
}
const MAPS: [&[u8]; 20] = [
    fixture_bytes!("0.map.test-fixture"),
    fixture_bytes!("1.map.test-fixture"),
    fixture_bytes!("2.map.test-fixture"),
    fixture_bytes!("3.map.test-fixture"),
    fixture_bytes!("4.map.test-fixture"),
    fixture_bytes!("5.map.test-fixture"),
    fixture_bytes!("6.map.test-fixture"),
    fixture_bytes!("7.map.test-fixture"),
    fixture_bytes!("8.map.test-fixture"),
    fixture_bytes!("9.map.test-fixture"),
    fixture_bytes!("10.map.test-fixture"),
    fixture_bytes!("11.map.test-fixture"),
    fixture_bytes!("12.map.test-fixture"),
    fixture_bytes!("13.map.test-fixture"),
    fixture_bytes!("14.map.test-fixture"),
    fixture_bytes!("15.map.test-fixture"),
    fixture_bytes!("16.map.test-fixture"),
    fixture_bytes!("17.map.test-fixture"),
    fixture_bytes!("18.map.test-fixture"),
    fixture_bytes!("19.map.test-fixture"),
];
const STORES: [[&[u8]; 3]; 6] = [
    [
        fixture_bytes!("store0/AreaTable.dbc.test-fixture"),
        fixture_bytes!("store0/WMOAreaTable.dbc.test-fixture"),
        fixture_bytes!("store0/Map.dbc.test-fixture"),
    ],
    [
        fixture_bytes!("store1/AreaTable.dbc.test-fixture"),
        fixture_bytes!("store1/WMOAreaTable.dbc.test-fixture"),
        fixture_bytes!("store1/Map.dbc.test-fixture"),
    ],
    [
        fixture_bytes!("store2/AreaTable.dbc.test-fixture"),
        fixture_bytes!("store2/WMOAreaTable.dbc.test-fixture"),
        fixture_bytes!("store2/Map.dbc.test-fixture"),
    ],
    [
        fixture_bytes!("store3/AreaTable.dbc.test-fixture"),
        fixture_bytes!("store3/WMOAreaTable.dbc.test-fixture"),
        fixture_bytes!("store3/Map.dbc.test-fixture"),
    ],
    [
        fixture_bytes!("store4/AreaTable.dbc.test-fixture"),
        fixture_bytes!("store4/WMOAreaTable.dbc.test-fixture"),
        fixture_bytes!("store4/Map.dbc.test-fixture"),
    ],
    [
        fixture_bytes!("store5/AreaTable.dbc.test-fixture"),
        fixture_bytes!("store5/WMOAreaTable.dbc.test-fixture"),
        fixture_bytes!("store5/Map.dbc.test-fixture"),
    ],
];
const CASES: &str = include_str!(
    "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/terrain_area.tsv"
);

fn bits(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn number(value: &str) -> u32 {
    value.parse().unwrap()
}
fn resolved(row: &[&str], start: usize) -> Option<ResolvedArea> {
    (row[start] == "1").then(|| ResolvedArea {
        id: number(row[start + 1]),
        map_id: number(row[start + 2]),
        zone: number(row[start + 3]),
        explore_flag: number(row[start + 4]),
        liquid_override: number(row[start + 5]),
    })
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn complete_original_source_area_chain_matches_all_supported_loaded_byte_profiles() {
    let rows: Vec<Vec<_>> = CASES
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split('\t').collect())
        .collect();
    let mut count = 0;
    let mut raw_hits = 0;
    let mut accepted = 0;
    let mut rejected = 0;
    let mut parents = 0;
    let mut missing = 0;
    let mut hole_heights = 0;
    let mut source_only = 0;
    for reverse in [false, true] {
        let dataset = if reverse {
            "scopedarea/reverse"
        } else {
            "scopedarea/forward"
        };
        for (profile, bytes) in MAPS.iter().enumerate() {
            let (spec, files) = super::tests::fixture(reverse, bytes);
            if matches!(profile, 16..=18) {
                // Source-only absent, uint8 and uint16 MHGT profiles are not the
                // admitted float/flat parser. Never broaden its domain for this oracle.
                assert!(
                    super::tests::load(spec, &files).is_err(),
                    "profile {profile}"
                );
                source_only += rows
                    .iter()
                    .filter(|row| row[0] == dataset && number(row[2]) as usize == profile)
                    .count();
                continue;
            }
            let products = super::tests::load(spec, &files).unwrap();
            for (store, dbcs) in STORES.iter().enumerate() {
                let snapshot = products
                    .load_terrain_area(dbcs[0], dbcs[1], dbcs[2])
                    .unwrap();
                for row in rows.iter().filter(|row| {
                    row[0] == dataset
                        && number(row[1]) as usize == store
                        && number(row[2]) as usize == profile
                }) {
                    assert_eq!(row.len(), 32, "{row:?}");
                    let query = TerrainAreaRequest {
                        snapshot: SnapshotBinding {
                            map: number(row[4]),
                            ..snapshot.binding()
                        },
                        ticket: count + 1,
                        position: [bits(row[5]), bits(row[6]), bits(row[7])],
                    };
                    let tile_id = TileId {
                        x: number(row[30]) as u8,
                        y: number(row[31]) as u8,
                    };
                    let p = query.position.map(f32::from_bits);
                    assert_eq!(world_tile(p[0], p[1]).unwrap(), tile_id);
                    let tile = products
                        .inventory
                        .terrain(query.snapshot.map, tile_id)
                        .unwrap();
                    let height = match tile.point_height(p[0], p[1]).unwrap() {
                        PointHeight::Height(height) => height,
                        PointHeight::Hole => INVALID_HEIGHT_VALUE,
                    };
                    assert_eq!(
                        height.to_bits(),
                        bits(row[15]),
                        "{} {} profile {profile} height",
                        dataset,
                        row[3]
                    );
                    assert_eq!(
                        u32::from(tile.area_flag(p[0], p[1]).unwrap()),
                        number(row[16]),
                        "{} {} profile {profile} AREA",
                        dataset,
                        row[3]
                    );
                    let raw_area = if row[8] == "1" {
                        raw_hits += 1;
                        AreaResult::Area {
                            floor: bits(row[9]),
                            flags: number(row[10]),
                            adt_id: number(row[11]).try_into().unwrap(),
                            root_id: number(row[12]),
                            group_id: number(row[13]),
                        }
                    } else {
                        assert_eq!(row[9], row[7], "source no-hit leaves z untouched");
                        AreaResult::None
                    };
                    let area_info = if row[14] == "1" {
                        accepted += 1;
                        raw_area
                    } else {
                        AreaResult::None
                    };
                    rejected += usize::from(row[8] == "1" && row[14] == "0");
                    parents += usize::from(row[24] == "1");
                    missing += usize::from(row[18] == "0");
                    hole_heights += usize::from(height == INVALID_HEIGHT_VALUE);
                    let expected = TerrainAreaResult {
                        raw_area,
                        area_info,
                        area_flag: number(row[17]).try_into().unwrap(),
                        area: resolved(row, 18),
                        parent_area: resolved(row, 24),
                    };
                    let reply = snapshot.query(query, deadline()).unwrap_or_else(|error| {
                        panic!(
                            "{} {} store {store} profile {profile}: {error:?}",
                            dataset, row[3]
                        );
                    });
                    assert_eq!(reply.request, query);
                    assert_eq!(
                        reply.result, expected,
                        "{} {} store {store} profile {profile}",
                        dataset, row[3]
                    );
                    let location = snapshot.query_location(query, deadline()).unwrap();
                    assert_eq!(location.request, query);
                    assert_eq!(location.result.area_flag, expected.area_flag);
                    assert_eq!(location.result.area, expected.area);
                    // The original-source harness verifies GetZoneAndAreaId against
                    // these same recorded rows without performing a parent lookup.
                    let area_id = expected.area.map_or(0, |area| area.id);
                    let zone_id =
                        expected.area.map_or(
                            0,
                            |area| {
                                if area.zone != 0 {
                                    area.zone
                                } else {
                                    area.id
                                }
                            },
                        );
                    assert_eq!(
                        (location.result.zone_id, location.result.area_id),
                        (zone_id, area_id)
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 25_440);
    assert_eq!(raw_hits, 8_640);
    assert_eq!(accepted, 6_564);
    assert_eq!(rejected, 2_076);
    assert_eq!(parents, 8_689);
    assert_eq!(missing, 2_061);
    assert_eq!(hole_heights, 2_016);
    assert_eq!(source_only, 3_024);
    assert_eq!(count as usize + source_only, rows.len());
}
