// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{SourceOwnerFilter, SourceSmoothingBudget, TileInput};
use std::time::Duration;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const LEFT: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn mesh(both: bool) -> Mesh {
    let mut tiles = vec![TileInput {
        grid: [32, 32],
        bytes: LEFT,
    }];
    if both {
        tiles.push(TileInput {
            grid: [32, 31],
            bytes: RIGHT,
        });
    }
    Mesh::load(PARAMS, &tiles).unwrap()
}
#[test]
fn source_coordinate_domain_destination_first_is_not_native_safety_domain() {
    let valid = point(0., 0., f32::MAX);
    assert_eq!(source_map_coord_pair(valid, valid), Ok(()));
    for word in [0x4685_5455, 0xc685_5455, 0, 0x8000_0000, 1, 0x8000_0001] {
        assert_eq!(source_map_coord_pair([word, word, 0], valid), Ok(()));
    }
    for word in [
        0x4685_5456,
        0xc685_5456,
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        0x7f80_0001,
    ] {
        let invalid = [word, 0, 0];
        assert_eq!(
            source_map_coord_pair(invalid, invalid),
            Err(SourceMapCoordError::DestinationInvalid)
        );
        assert_eq!(
            source_map_coord_pair(invalid, valid),
            Err(SourceMapCoordError::StartInvalid)
        );
        assert_eq!(
            source_map_coord_pair(valid, invalid),
            Err(SourceMapCoordError::DestinationInvalid)
        );
    }
    for word in [0x7f80_0000, 0xff80_0000, 0x7fc0_0000, 0x7f80_0001] {
        assert_eq!(
            source_map_coord_pair(valid, [0, 0, word]),
            Err(SourceMapCoordError::DestinationInvalid)
        );
    }
    let mesh = mesh(false);
    assert_eq!(
        mesh.source_tile_presence_pair(valid, valid, until()),
        Err(Error::Unsupported)
    );
    assert_eq!(
        mesh.source_tile_presence_pair(valid, point(17067., 0., 0.), until()),
        Err(Error::Format)
    );
}
#[test]
fn nearby_and_cached_polygon_do_not_prove_original_endpoint_tile() {
    let mut mesh = mesh(false);
    let from = point(-400., -10., 0.);
    let to = point(-400., 1., 0.);
    let filter = SourceOwnerFilter {
        include_flags: 1,
        exclude_flags: 0,
    };
    let cache = mesh
        .loaded_source_corridor(
            from,
            point(-400., -1., 0.),
            filter,
            74,
            SourceSmoothingBudget::default(),
            until(),
        )
        .unwrap()
        .warm_cache_seed()
        .unwrap();
    let before = cache.clone();
    assert!(mesh.nearest(to, 5., until()).is_ok());
    let warm = mesh
        .loaded_source_warm_cache(
            &cache,
            from,
            to,
            crate::SourceWarmCacheBudget::default(),
            until(),
        )
        .unwrap();
    assert_eq!(warm.phase(), crate::SourceWarmPolyPhase::SamePolygon);
    assert_eq!(warm.end().origin, crate::SourceWarmLookupOrigin::Active);
    let observed = mesh.source_tile_presence_pair(from, to, until()).unwrap();
    assert_eq!(observed.from(), from);
    assert_eq!(observed.to(), to);
    assert_eq!(observed.start().coordinates(), [0, 0]);
    assert!(observed.start().present());
    assert_eq!(observed.end().unwrap().coordinates(), [1, 0]);
    assert!(!observed.end().unwrap().present());
    assert!(!observed.both_present());
    assert_eq!(observed.check_mesh(&mesh), Ok(()));
    assert_eq!(cache, before);
    assert_eq!(cache.check_mesh(&mesh), Ok(()));
}
#[test]
fn binary32_boundaries_axis_order_and_height_are_exact() {
    let mesh = mesh(false);
    let start = point(-400., -10., 0.);
    let grid = crate::validate::GRID;
    for (y, expected) in [
        (-grid, 0),
        (f32::from_bits((-grid).to_bits() + 1), -1),
        (f32::from_bits((-grid).to_bits() - 1), 0),
        (f32::from_bits(0xb800_0001), 0),
        (f32::from_bits(0xb800_0000), 0),
        (f32::from_bits(0xb7ff_ffff), 1),
        (f32::from_bits(0x8000_0001), 1),
        (-0., 1),
        (0., 1),
        (f32::from_bits(1), 1),
    ] {
        let result = mesh
            .source_tile_presence_pair(start, point(-400., y, 0.), until())
            .unwrap();
        assert_eq!(
            result.end().unwrap().coordinates(),
            [expected, 0],
            "y={:08x}",
            y.to_bits()
        );
        assert_eq!(result.end().unwrap().present(), expected == 0);
    }
    for z in [-100000., -0., 100000.] {
        let result = mesh
            .source_tile_presence_pair(start, point(-400., -10., z), until())
            .unwrap();
        assert!(result.both_present());
    }
    assert_eq!(
        mesh.source_tile_presence_pair(start, point(100., -400., 0.), until())
            .unwrap()
            .end()
            .unwrap()
            .coordinates(),
        [0, 1]
    );
    assert!(self::mesh(true)
        .source_tile_presence_pair(start, point(-400., 1., 0.), until())
        .unwrap()
        .both_present());
}
#[test]
fn missing_start_short_circuits_and_loaded_tile_needs_no_near_polygon() {
    let mut mesh = mesh(false);
    let from = point(-400., 1., 0.);
    let to = point(-400., -10., 0.);
    let result = mesh.source_tile_presence_pair(from, to, until()).unwrap();
    assert!(!result.start().present());
    assert_eq!(result.end(), None);
    assert!(!result.both_present());
    let outside_polygons = point(-1., -1., 1000.);
    assert_eq!(
        mesh.nearest(outside_polygons, 5., until()),
        Err(Error::Unavailable)
    );
    assert!(mesh
        .source_tile_presence_pair(to, outside_polygons, until())
        .unwrap()
        .both_present());
}
#[test]
fn exactly_minus_one_is_rejected_but_other_loaded_negatives_are_looked_up() {
    for coordinate in [-1i32, -2] {
        let mut params = PARAMS.to_vec();
        let origin = -(coordinate as f32) * crate::validate::GRID;
        params[..4].copy_from_slice(&origin.to_le_bytes());
        let mut tile = LEFT.to_vec();
        tile[28..32].copy_from_slice(&coordinate.to_le_bytes());
        let mesh = Mesh::load(
            &params,
            &[TileInput {
                grid: [32, 32],
                bytes: &tile,
            }],
        )
        .unwrap();
        let p = point(-400., 1., 0.);
        let result = mesh.source_tile_presence_pair(p, p, until()).unwrap();
        assert_eq!(result.start().coordinates(), [coordinate, 0]);
        assert_eq!(result.start().present(), coordinate != -1);
        assert_eq!(result.end().is_some(), coordinate != -1);
    }
}
#[test]
fn layer_one_is_outside_loaded_validator_and_identity_deadline_fail_closed() {
    let mut layer = LEFT.to_vec();
    layer[36..40].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(
        Mesh::load(
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes: &layer
            }]
        ),
        Err(Error::Unsupported)
    ));
    let mesh = mesh(false);
    let p = point(-400., -10., -0.);
    let observation = mesh.source_tile_presence_pair(p, p, until()).unwrap();
    let fresh = self::mesh(false);
    assert_eq!(observation.check_mesh(&fresh), Err(Error::Identity));
    assert_eq!(
        mesh.source_tile_presence_pair(p, p, Instant::now()),
        Err(Error::Deadline)
    );
    for bad in [
        point(f32::NAN, 0., 0.),
        point(0., f32::INFINITY, 0.),
        point(0., 0., f32::NEG_INFINITY),
    ] {
        assert_eq!(
            mesh.source_tile_presence_pair(p, bad, until()),
            Err(Error::Format)
        );
    }
}

#[test]
fn unchanged_source_have_tile_and_coordinate_observations_match_loaded_meshes() {
    let fixture = include_str!("../tools/fixtures/tile-presence/cases.test-fixture");
    let header = fixture
        .lines()
        .find(|line| line.starts_with("# label\t"))
        .unwrap();
    let headings: Vec<_> = header.strip_prefix("# ").unwrap().split('\t').collect();
    let mut source_rows = 0;
    let mut actual_loaded = 0;
    let mut invalid = 0;
    let mut unsupported = 0;
    for line in fixture
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let values: Vec<_> = line.split('\t').collect();
        assert_eq!(values.len(), headings.len());
        let get = |name: &str| values[headings.iter().position(|&field| field == name).unwrap()];
        let bits = |name: &str| -> [u32; 3] {
            get(name)
                .split(',')
                .map(|word| u32::from_str_radix(word, 16).unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        };
        let from = bits("from");
        let to = bits("to");
        let valid = source_map_coord_pair(from, to);
        assert_eq!(valid.is_ok(), get("pair_valid") == "1", "{}", get("label"));
        if get("destination_valid") == "0" {
            assert_eq!(valid, Err(SourceMapCoordError::DestinationInvalid));
            assert_eq!(get("start_valid"), "-");
        } else if get("start_valid") == "0" {
            assert_eq!(valid, Err(SourceMapCoordError::StartInvalid));
        }
        source_rows += 1;
        let shifted = |params: &[u8], tile: &[u8]| {
            Mesh::load(
                params,
                &[TileInput {
                    grid: [32, 32],
                    bytes: tile,
                }],
            )
            .unwrap()
        };
        let mut mesh = match get("geometry") {
            "both" => mesh(true),
            "left" => mesh(false),
            "right" => Mesh::load(
                PARAMS,
                &[TileInput {
                    grid: [32, 31],
                    bytes: RIGHT,
                }],
            )
            .unwrap(),
            "minus_one" => shifted(
                include_bytes!("../tools/fixtures/tile-presence/params-minus-one.test-fixture"),
                include_bytes!("../tools/fixtures/tile-presence/tile-minus-one.test-fixture"),
            ),
            "minus_two" => shifted(
                include_bytes!("../tools/fixtures/tile-presence/params-minus-two.test-fixture"),
                include_bytes!("../tools/fixtures/tile-presence/tile-minus-two.test-fixture"),
            ),
            "empty" => {
                assert_eq!(get("scope"), "raw");
                assert!(matches!(Mesh::load(PARAMS, &[]), Err(Error::Budget)));
                unsupported += 1;
                continue;
            }
            "layer_one" => {
                assert_eq!(get("scope"), "raw");
                assert!(matches!(
                    Mesh::load(
                        PARAMS,
                        &[TileInput {
                            grid: [32, 32],
                            bytes: include_bytes!(
                                "../tools/fixtures/tile-presence/tile-layer-one.test-fixture"
                            )
                        }]
                    ),
                    Err(Error::Unsupported)
                ));
                unsupported += 1;
                continue;
            }
            other => panic!("unknown tile source geometry {other}"),
        };
        let result = mesh.source_tile_presence_pair(from, to, until());
        if valid.is_err() {
            assert_eq!(result, Err(Error::Format), "{}", get("label"));
            invalid += 1;
            continue;
        }
        if get("native_safe") == "0" {
            assert_eq!(result, Err(Error::Unsupported), "{}", get("label"));
            unsupported += 1;
            continue;
        }
        let observed = result.unwrap();
        assert_eq!(observed.from(), from);
        assert_eq!(observed.to(), to);
        assert_eq!(observed.check_mesh(&mesh), Ok(()));
        let check = |prefix: &str, endpoint: SourceTilePresence| {
            assert_eq!(get(&format!("{prefix}_checked")), "1");
            assert_eq!(
                endpoint.coordinates(),
                [
                    get(&format!("{prefix}_tx")).parse::<i32>().unwrap(),
                    get(&format!("{prefix}_ty")).parse::<i32>().unwrap()
                ],
                "{}",
                get("label")
            );
            assert_eq!(
                endpoint.present(),
                get(&format!("{prefix}_have")) == "1",
                "{}",
                get("label")
            );
        };
        check("start", observed.start());
        if get("end_checked") == "1" {
            check("end", observed.end().unwrap());
        } else {
            assert_eq!(observed.end(), None);
        }
        assert_eq!(observed.both_present(), get("pair_have") == "1");
        if get("nearest_status") != "-" {
            assert_eq!(
                u32::from_str_radix(get("nearest_status"), 16).unwrap(),
                crate::SUCCESS
            );
            let nearest = mesh.nearest(from, 5., until()).unwrap();
            assert_eq!(nearest.polygon, get("nearest_ref").parse::<u64>().unwrap());
            assert!(!observed.start().present());
        }
        actual_loaded += 1;
    }
    assert!(source_rows >= 91 && actual_loaded >= 55 && invalid >= 20 && unsupported == 4,
        "source={source_rows}, loaded={actual_loaded}, invalid={invalid}, unsupported={unsupported}");
}
