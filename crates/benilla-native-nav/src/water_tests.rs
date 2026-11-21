// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::time::Duration;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const MIXED: &[u8] = include_bytes!("../tools/fixtures/water/mixed.mmtile");
const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
fn load(bytes: &[u8]) -> Result<Mesh, Error> {
    Mesh::load(
        PARAMS,
        &[
            TileInput {
                grid: [32, 32],
                bytes,
            },
            TileInput {
                grid: [32, 31],
                bytes: RIGHT,
            },
        ],
    )
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn points() -> ([u32; 3], [u32; 3], [u32; 3]) {
    let grid = validate::GRID;
    let ground = [-grid + 20. * grid / 100., -grid + 20. * grid / 100., 0.].map(f32::to_bits);
    let wet = [-grid + 20. * grid / 100., -grid + 80. * grid / 100., 0.].map(f32::to_bits);
    let beyond = [-grid + 20. * grid / 100., 20. * grid / 100., 2.].map(f32::to_bits);
    (ground, wet, beyond)
}
#[test]
fn mixed_water_loads_but_default_non_swimming_walker_cannot_enter_or_cross_it() {
    let mut mesh = load(MIXED).unwrap();
    let (ground, wet, beyond) = points();
    assert!(mesh.nearest(ground, 5., until()).is_ok());
    assert_eq!(mesh.nearest(wet, 5., until()), Err(Error::Unavailable));
    assert!(mesh.nearest(beyond, 5., until()).is_ok());
    assert!(mesh.route(ground, wet, until()).is_err());
    assert!(mesh.route(ground, beyond, until()).is_err());
}
#[test]
fn only_exact_source_area_flag_pairs_are_admitted() {
    let poly = 120 + 14 * 12 + 32;
    for (flags, area) in [
        (4u16, 11u8),
        (1, 9),
        (8, 8),
        (4, 8),
        (4, 73),
        (0, 9),
        (5, 9),
        (16, 12),
        (1, 0),
    ] {
        let mut bytes = MIXED.to_vec();
        bytes[poly + 28..poly + 30].copy_from_slice(&flags.to_le_bytes());
        bytes[poly + 31] = area;
        assert!(
            matches!(load(&bytes), Err(Error::Unsupported)),
            "accepted {flags}/{area}"
        );
    }
}

#[test]
fn explicit_filters_match_original_detour_water_and_cross_water_corridors() {
    let mut mesh = load(MIXED).unwrap();
    let (ground, wet, beyond) = points();
    let mut cases = 0;
    for line in include_str!("../tools/fixtures/water/cases.tsv")
        .lines()
        .skip(1)
    {
        let row: Vec<_> = line.split_whitespace().collect();
        assert_eq!(row.len(), 8);
        let filter = SourceOwnerFilter {
            include_flags: row[1].parse().unwrap(),
            exclude_flags: row[2].parse().unwrap(),
        };
        let wet_ref: u64 = row[3].parse().unwrap();
        let nearest = mesh.nearest_with_filter(wet, 5., filter, until());
        if wet_ref == 0 {
            assert_eq!(nearest, Err(Error::Unavailable), "{}", row[0]);
        } else {
            assert_eq!(nearest.unwrap().polygon, wet_ref, "{}", row[0]);
        }
        for (destination, status, count) in [(wet, row[4], row[5]), (beyond, row[6], row[7])] {
            let result = mesh.route_with_filter(ground, destination, filter, until());
            assert_eq!(
                result.is_ok(),
                status.parse::<u32>().unwrap() == SUCCESS,
                "{}",
                row[0]
            );
            if let Ok(route) = result {
                assert_eq!(
                    route.polygons.len(),
                    count.parse::<usize>().unwrap(),
                    "{}",
                    row[0]
                );
                assert!(route.polygons.contains(&wet_ref));
            }
        }
        // Explicit swimming/player queries must not change the next default walker.
        assert_eq!(mesh.nearest(wet, 5., until()), Err(Error::Unavailable));
        assert!(mesh.route(ground, beyond, until()).is_err());
        cases += 1;
    }
    assert_eq!(cases, 5);
}

#[test]
fn unknown_filter_bits_remain_unsupported() {
    let mut mesh = load(MIXED).unwrap();
    let (ground, wet, _) = points();
    for filter in [
        SourceOwnerFilter {
            include_flags: 16,
            exclude_flags: 0,
        },
        SourceOwnerFilter {
            include_flags: 5,
            exclude_flags: 16,
        },
    ] {
        assert_eq!(
            mesh.nearest_with_filter(wet, 5., filter, until()),
            Err(Error::Unsupported)
        );
        assert_eq!(
            mesh.route_with_filter(ground, wet, filter, until()),
            Err(Error::Unsupported)
        );
    }
}
