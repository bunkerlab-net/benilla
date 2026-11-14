// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::time::Duration;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/collinear/params.mmap");
const CORNER: &[u8] = include_bytes!("../tools/fixtures/collinear/corner.mmtile");
fn load(bytes: &[u8]) -> Result<Mesh, Error> {
    Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes,
        }],
    )
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn assert_original_queries(bytes: &[u8], oracle: &str) {
    let mut mesh = load(bytes).unwrap();
    let mut lines = oracle.lines().skip(1);
    let mut inputs = Vec::new();
    for _ in 0..2 {
        let row: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        let bits =
            |at: usize| std::array::from_fn(|i| u32::from_str_radix(row[at + i], 16).unwrap());
        assert_eq!(row[3].parse::<u32>().unwrap(), SUCCESS);
        inputs.push(bits(0));
        assert_eq!(
            mesh.nearest(bits(0), 5., until()).unwrap(),
            Nearest {
                polygon: row[4].parse().unwrap(),
                point: bits(5),
                over: row[8] == "1",
            }
        );
    }
    let path: Vec<_> = lines.next().unwrap().split_whitespace().collect();
    let route = mesh.route(inputs[0], inputs[1], until()).unwrap();
    assert_eq!(route.polygons.len(), path[1].parse::<usize>().unwrap());
    assert_eq!(route.corners.len(), path[2].parse::<usize>().unwrap());
    for corner in route.corners {
        let row: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(
            corner.point,
            std::array::from_fn(|i| u32::from_str_radix(row[i], 16).unwrap())
        );
    }
    // The independent source oracle also executes raycast on these endpoints.
    assert!(lines.next().unwrap().starts_with("raycast "));
    assert!(lines.next().is_none());
}
#[test]
fn collinear_corner_loads_and_matches_original_nearest_and_funnel() {
    assert_original_queries(
        CORNER,
        include_str!("../tools/fixtures/collinear/corner.tsv"),
    );
}
#[test]
fn reflex_corner_still_refuses_before_native_loading() {
    let mut bytes = CORNER.to_vec();
    // Move the middle of the left boundary inward, creating a reflex vertex.
    let x = -validate::GRID + 20. * (validate::GRID / 100.);
    bytes[132..136].copy_from_slice(&x.to_le_bytes());
    assert!(matches!(load(&bytes), Err(Error::Unsupported)));
}
#[test]
fn fully_collinear_triangle_matches_original_edge_fallback_and_funnel() {
    assert_original_queries(
        include_bytes!("../tools/fixtures/collinear/flat.mmtile"),
        include_str!("../tools/fixtures/collinear/flat.tsv"),
    );
}
#[test]
fn inverted_detail_triangle_still_refuses_before_native_loading() {
    let mut bytes = CORNER.to_vec();
    let triangles = 120 + 5 * 12 + 32 + 5 * 16 + 12;
    bytes.swap(triangles, triangles + 1);
    assert!(matches!(load(&bytes), Err(Error::Unsupported)));
}
#[test]
fn original_detail_bv_rounding_matches_original_nearest_and_funnel() {
    assert_original_queries(
        include_bytes!("../tools/fixtures/collinear/rounded.mmtile"),
        include_str!("../tools/fixtures/collinear/rounded.tsv"),
    );
}
