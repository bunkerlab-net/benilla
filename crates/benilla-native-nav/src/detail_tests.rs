// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::time::Duration;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/detail/params.mmap");
const TILE: &[u8] = include_bytes!("../tools/fixtures/detail/near-edge.mmtile");
fn load(bytes: &[u8]) -> Result<Mesh, Error> {
    Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes,
        }],
    )
}
fn word(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn sections() -> (usize, usize, usize) {
    let details = 120 + word(TILE, 48) * 12 + word(TILE, 44) * 32 + word(TILE, 52) * 16;
    let vertices = details + word(TILE, 56) * 12;
    let triangles = vertices + word(TILE, 60) * 12;
    (details, vertices, triangles)
}
fn assert_original_queries(tile: &[u8], expected: &str) {
    let mut mesh = load(tile).unwrap();
    let mut count = 0;
    for line in expected.lines().skip(1) {
        let row: Vec<_> = line.split_whitespace().collect();
        assert_eq!(row.len(), 10);
        let bits =
            |at: usize| std::array::from_fn(|i| u32::from_str_radix(row[at + i], 16).unwrap());
        assert_eq!(row[4].parse::<u32>().unwrap(), SUCCESS);
        let result = mesh
            .nearest(bits(1), 5., Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(result.polygon, row[5].parse::<u64>().unwrap(), "{}", row[0]);
        assert_eq!(result.point, bits(6), "{}", row[0]);
        assert_eq!(result.over, row[9] == "1", "{}", row[0]);
        count += 1;
    }
    assert_eq!(count, 2);
}
#[test]
fn near_edge_detail_outside_a_diagonal_polygon_matches_original_queries() {
    assert_original_queries(TILE, include_str!("../tools/fixtures/detail/queries.tsv"));
}
#[test]
fn unflagged_sampled_boundary_segments_match_original_queries() {
    assert_original_queries(
        include_bytes!("../tools/fixtures/detail/unflagged.mmtile"),
        include_str!("../tools/fixtures/detail/unflagged.tsv"),
    );
}
#[test]
fn absent_boundary_flags_still_refuse_the_null_native_fallback() {
    let (_, _, triangles) = sections();
    let mut bytes = TILE.to_vec();
    for triangle in 0..3 {
        bytes[triangles + triangle * 4 + 3] = 0;
    }
    assert!(matches!(load(&bytes), Err(Error::Format)));
}
#[test]
fn detail_triangle_indices_and_ranges_still_refuse_before_native_loading() {
    let (details, _, triangles) = sections();
    for triangle in 0..3 {
        for corner in 0..3 {
            let mut bytes = TILE.to_vec();
            bytes[triangles + triangle * 4 + corner] = 5;
            assert!(matches!(load(&bytes), Err(Error::Format)));
        }
    }
    for offset in [details, details + 4] {
        let mut bytes = TILE.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(load(&bytes), Err(Error::Format)));
    }
    let mut bytes = TILE.to_vec();
    bytes[details + 8] = 2;
    assert!(matches!(load(&bytes), Err(Error::Format)));
}
#[test]
fn nonfinite_base_and_detail_vertices_remain_refused() {
    let (_, vertices, _) = sections();
    for start in [120, vertices] {
        for axis in 0..3 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut bytes = TILE.to_vec();
                let offset = start + axis * 4;
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                assert!(matches!(load(&bytes), Err(Error::Format)));
            }
        }
    }
}
