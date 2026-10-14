//! A positive continuous-area proof for a deliberately small ADT subset. This module has no
//! engine dependency: the tests author the same 145-vertex fan topology as the format adapter.

use benilla_formats::ChunkMesh;

/// Why native terrain cannot certify the requested corridor. Every variant means unknown to
/// the caller, never permission to substitute a straight-line or sampled-ground answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageError {
    InvalidInput,
    WrongMap,
    UnloadedMap,
    UnsupportedMap,
    UnloadedTile,
    UnfurnishedTile,
    UnsupportedSource,
    UnsupportedPlacements,
    ColliderUnavailable,
    MalformedTerrain,
    UnsupportedTerrain,
    OutsideSupportedTile,
}

impl std::fmt::Display for CoverageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native terrain coverage is unproven: {self:?}")
    }
}

impl std::error::Error for CoverageError {}

/// The swept disc lies inside this rectangle. Expand outward in f64, including when a tiny
/// binary32 radius is lost next to a large coordinate; no geometric tolerance opens gaps.
#[derive(Clone, Copy)]
struct Rectangle {
    min: [f64; 2],
    max: [f64; 2],
}

impl Rectangle {
    fn new(from: [f32; 3], to: [f32; 3], radius: f32) -> Result<Self, CoverageError> {
        if !from.into_iter().chain(to).all(f32::is_finite) || !radius.is_finite() || radius < 0.0 {
            return Err(CoverageError::InvalidInput);
        }
        let radius = f64::from(radius);
        Ok(Self {
            min: std::array::from_fn(|axis| {
                let value = f64::from(from[axis].min(to[axis])) - radius;
                if radius == 0.0 {
                    value
                } else {
                    value.next_down()
                }
            }),
            max: std::array::from_fn(|axis| {
                let value = f64::from(from[axis].max(to[axis])) + radius;
                if radius == 0.0 {
                    value
                } else {
                    value.next_up()
                }
            }),
        })
    }

    fn strictly_inside(self, min: [f32; 2], max: [f32; 2]) -> bool {
        (0..2).all(|axis| {
            self.min[axis] > f64::from(min[axis]) && self.max[axis] < f64::from(max[axis])
        })
    }

    /// Closed overlap deliberately includes a cell touched only on an edge or corner. In
    /// particular both authors of a chunk seam must supply the same supported height.
    fn intersects(self, min: [f32; 2], max: [f32; 2]) -> bool {
        (0..2).all(|axis| {
            self.min[axis] <= f64::from(max[axis]) && self.max[axis] >= f64::from(min[axis])
        })
    }
}

/// Same rational lattice as the format adapter's `snap_to_lattice`; native vertices are f32.
fn lattice(index: u32) -> f32 {
    let center = 32.0_f64 * 1600.0 / 3.0;
    let unit = (1600.0_f64 / 3.0) / 128.0;
    (center - f64::from(index) * unit) as f32
}

/// Validate the entire tile, not only the promising chunk: every triangle must remain inside
/// its distinct canonical footprint. Missing or duplicate chunks never imply complete terrain.
///
/// Every canonical cell whose closed footprint intersects the rectangle must have four
/// triangles at exactly the feet height. Their canonical XY outer edges and interior fan
/// centers partition the rectangle continuously. This admits chunk seams and flat patches
/// within otherwise nonplanar chunks, but never slopes, height steps or endpoint sampling.
/// Unrelated holes/heights cannot enter the rectangle because their XY footprints are proved.
pub(super) fn prove_planar_tile(
    chunks: &[ChunkMesh],
    tile: (u32, u32),
    from: [f32; 3],
    to: [f32; 3],
    radius: f32,
) -> Result<(), CoverageError> {
    let rectangle = Rectangle::new(from, to, radius)?;
    if tile.0 >= 64 || tile.1 >= 64 || chunks.len() != 256 {
        return Err(CoverageError::MalformedTerrain);
    }
    let mut seen = [false; 256];
    for chunk in chunks {
        if chunk.index_x >= 16 || chunk.index_y >= 16 {
            return Err(CoverageError::MalformedTerrain);
        }
        let slot = (chunk.index_y * 16 + chunk.index_x) as usize;
        if std::mem::replace(&mut seen[slot], true) {
            return Err(CoverageError::MalformedTerrain);
        }
        validate_chunk(chunk, tile)?;
    }
    let min = [lattice((tile.1 + 1) * 128), lattice((tile.0 + 1) * 128)];
    let max = [lattice(tile.1 * 128), lattice(tile.0 * 128)];
    if !rectangle.strictly_inside(min, max) {
        return Err(CoverageError::OutsideSupportedTile);
    }
    if from[2] != to[2] {
        return Err(CoverageError::UnsupportedTerrain);
    }
    for chunk in chunks {
        let max = [chunk.positions[0][0], chunk.positions[0][1]];
        let min = [chunk.positions[144][0], chunk.positions[144][1]];
        if !rectangle.intersects(min, max) {
            continue;
        }
        if chunk.impassable || !chunk.liquids.is_empty() {
            return Err(CoverageError::UnsupportedTerrain);
        }
        for row in 0..8usize {
            for col in 0..8usize {
                let tl = row * 17 + col;
                let (tr, bl, br, center) = (tl + 1, tl + 17, tl + 18, row * 17 + 9 + col);
                let nw = chunk.positions[tl];
                let se = chunk.positions[br];
                if !rectangle.intersects([se[0], se[1]], [nw[0], nw[1]]) {
                    continue;
                }
                if chunk.holes & (1 << ((row >> 1) * 4 + (col >> 1))) != 0
                    || [tl, tr, bl, br, center]
                        .into_iter()
                        .any(|i| chunk.positions[i][2] != from[2])
                {
                    return Err(CoverageError::UnsupportedTerrain);
                }
            }
        }
    }
    Ok(())
}

fn validate_chunk(chunk: &ChunkMesh, tile: (u32, u32)) -> Result<(), CoverageError> {
    let expected_indices = (64 - chunk.holes.count_ones() as usize * 4) * 12;
    if chunk.positions.len() != 145 || chunk.indices.len() != expected_indices {
        return Err(CoverageError::MalformedTerrain);
    }
    // Checking sizes first bounds every scan, even for malformed decoded input.
    if chunk.positions.iter().flatten().any(|v| !v.is_finite()) {
        return Err(CoverageError::MalformedTerrain);
    }
    let row_base = tile.1 * 128 + chunk.index_y * 8;
    let col_base = tile.0 * 128 + chunk.index_x * 8;
    for row in 0..9usize {
        for col in 0..9usize {
            let p = chunk.positions[row * 17 + col];
            if p[0] != lattice(row_base + row as u32) || p[1] != lattice(col_base + col as u32) {
                return Err(CoverageError::MalformedTerrain);
            }
        }
    }
    let mut offset = 0;
    for row in 0..8usize {
        for col in 0..8usize {
            let tl = row * 17 + col;
            let (tr, bl, br, center) = (tl + 1, tl + 17, tl + 18, row * 17 + 9 + col);
            let p = chunk.positions[center];
            let nw = chunk.positions[tl];
            let se = chunk.positions[br];
            if !(se[0] < p[0] && p[0] < nw[0] && se[1] < p[1] && p[1] < nw[1]) {
                return Err(CoverageError::MalformedTerrain);
            }
            if chunk.holes & (1 << ((row >> 1) * 4 + (col >> 1))) != 0 {
                continue;
            }
            let expected = [
                center, tl, bl, center, bl, br, center, br, tr, center, tr, tl,
            ]
            .map(|index| index as u32);
            if chunk.indices[offset..offset + 12] != expected {
                return Err(CoverageError::MalformedTerrain);
            }
            offset += 12;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: (u32, u32) = (32, 32);
    const FROM: [f32; 3] = [-10.0, -10.0, 7.0];
    const TO: [f32; 3] = [-20.0, -20.0, 7.0];

    fn chunk(ix: u32, iy: u32) -> ChunkMesh {
        chunk_at(TILE, ix, iy)
    }

    fn chunk_at(tile: (u32, u32), ix: u32, iy: u32) -> ChunkMesh {
        let mut positions = vec![[0.0; 3]; 145];
        let row_base = tile.1 * 128 + iy * 8;
        let col_base = tile.0 * 128 + ix * 8;
        for row in 0..9usize {
            for col in 0..9usize {
                positions[row * 17 + col] = [
                    lattice(row_base + row as u32),
                    lattice(col_base + col as u32),
                    7.0,
                ];
            }
        }
        let mut indices = Vec::new();
        for row in 0..8usize {
            for col in 0..8usize {
                let tl = row * 17 + col;
                let (tr, bl, br, center) = (tl + 1, tl + 17, tl + 18, row * 17 + 9 + col);
                positions[center] = [
                    (positions[tl][0] + positions[br][0]) / 2.0,
                    (positions[tl][1] + positions[br][1]) / 2.0,
                    7.0,
                ];
                indices.extend(
                    [
                        center, tl, bl, center, bl, br, center, br, tr, center, tr, tl,
                    ]
                    .map(|index| index as u32),
                );
            }
        }
        ChunkMesh {
            positions,
            normals: Vec::new(),
            uvs: Vec::new(),
            indices,
            holes: 0,
            base_texture: None,
            layer_textures: Vec::new(),
            layer_effect_ids: Vec::new(),
            alpha_map: None,
            shadow: None,
            pred_tex: [0; 64],
            no_effect_doodad: [false; 64],
            index_x: ix,
            index_y: iy,
            area_id: 1,
            impassable: false,
            liquids: Vec::new(),
        }
    }

    fn tile() -> Vec<ChunkMesh> {
        (0..16)
            .flat_map(|y| (0..16).map(move |x| chunk(x, y)))
            .collect()
    }

    fn hole(chunk: &mut ChunkMesh, bit: u32) {
        chunk.holes |= 1 << bit;
        chunk.indices.clear();
        for row in 0..8usize {
            for col in 0..8usize {
                if chunk.holes & (1 << ((row >> 1) * 4 + (col >> 1))) != 0 {
                    continue;
                }
                let tl = row * 17 + col;
                let (tr, bl, br, center) = (tl + 1, tl + 17, tl + 18, row * 17 + 9 + col);
                chunk.indices.extend(
                    [
                        center, tl, bl, center, bl, br, center, br, tr, center, tr, tl,
                    ]
                    .map(|i| i as u32),
                );
            }
        }
    }

    fn prove(chunks: &[ChunkMesh]) -> Result<(), CoverageError> {
        prove_planar_tile(chunks, TILE, FROM, TO, 0.5)
    }

    #[test]
    fn authored_flat_fan_proves_whole_swept_rectangle_and_stationary_body() {
        let chunks = tile();
        assert_eq!(prove(&chunks), Ok(()));
        assert_eq!(prove_planar_tile(&chunks, TILE, TO, FROM, 0.5), Ok(()));
        assert_eq!(prove_planar_tile(&chunks, TILE, FROM, FROM, 0.5), Ok(()));
    }

    #[test]
    fn bowed_or_sloping_ground_is_not_promoted_from_endpoint_samples() {
        for delta in [-0.000_001, 0.000_001] {
            let mut chunks = tile();
            // The endpoints lie elsewhere; this interior fan hill/dip crosses their corridor.
            chunks[0].positions[3 * 17 + 9 + 3][2] += delta;
            assert_eq!(prove(&chunks), Err(CoverageError::UnsupportedTerrain));
        }
    }

    #[test]
    fn feet_must_match_exact_ground_height() {
        let chunks = tile();
        let from = [FROM[0], FROM[1], f32::from_bits(FROM[2].to_bits() + 1)];
        assert_eq!(
            prove_planar_tile(&chunks, TILE, from, TO, 0.5),
            Err(CoverageError::UnsupportedTerrain)
        );
    }

    #[test]
    fn whole_tile_boundary_and_radius_are_strict() {
        let chunks = tile();
        for (from, to, radius) in [
            ([-0.5, -10.0, 7.0], TO, 0.5),
            (FROM, [-534.0, -20.0, 7.0], 0.5),
            (FROM, TO, 14.0),
            ([0.0, -10.0, 7.0], TO, 0.0),
        ] {
            assert_eq!(
                prove_planar_tile(&chunks, TILE, from, to, radius),
                Err(CoverageError::OutsideSupportedTile)
            );
        }
    }

    #[test]
    fn holes_fail_even_when_a_point_height_helper_could_reconstruct_the_fan() {
        let mut chunks = tile();
        hole(&mut chunks[0], 5);
        assert_eq!(prove(&chunks), Err(CoverageError::UnsupportedTerrain));
    }

    #[test]
    fn unrelated_nonplanar_cells_and_holes_do_not_erase_a_flat_corridor() {
        let mut chunks = tile();
        chunks[0].positions[9][2] += 100.0;
        hole(&mut chunks[0], 15);
        assert_eq!(prove(&chunks), Ok(()));
    }

    #[test]
    fn both_chunk_axes_and_four_way_corner_have_continuous_flat_support() {
        let chunks = tile();
        for (from, to) in [
            ([-32.0, -10.0, 7.0], [-35.0, -10.0, 7.0]),
            ([-10.0, -32.0, 7.0], [-10.0, -35.0, 7.0]),
            ([-32.0, -32.0, 7.0], [-35.0, -35.0, 7.0]),
        ] {
            assert_eq!(prove_planar_tile(&chunks, TILE, from, to, 0.5), Ok(()));
            assert_eq!(prove_planar_tile(&chunks, TILE, to, from, 0.5), Ok(()));
        }
    }

    #[test]
    fn map_edge_tiles_preserve_displaced_f32_seams_and_coordinate_orientation() {
        for tile in [(1, 62), (62, 1)] {
            let mut chunks: Vec<_> = (0..16)
                .flat_map(|y| (0..16).map(move |x| chunk_at(tile, x, y)))
                .collect();
            let x = lattice(tile.1 * 128 + 8);
            let y = lattice(tile.0 * 128 + 8);
            let from = [x + 0.75, y + 0.75, 7.0];
            let to = [x - 0.75, y - 0.75, 7.0];
            assert_eq!(prove_planar_tile(&chunks, tile, from, to, 0.25), Ok(()));
            assert_eq!(prove_planar_tile(&chunks, tile, to, from, 0.25), Ok(()));
            let corner = [x, y, 7.0];
            assert_eq!(
                prove_planar_tile(&chunks, tile, corner, corner, 0.0),
                Ok(())
            );
            let edge = [lattice(tile.1 * 128) - 0.25, y, 7.0];
            assert_eq!(
                prove_planar_tile(&chunks, tile, edge, edge, 0.25),
                Err(CoverageError::OutsideSupportedTile)
            );
            chunks[17].positions[0][2] = 8.0;
            assert_eq!(
                prove_planar_tile(&chunks, tile, from, to, 0.25),
                Err(CoverageError::UnsupportedTerrain)
            );
        }
    }

    #[test]
    fn seam_height_mismatch_hole_and_impassable_neighbor_remain_unknown() {
        let from = [-32.0, -10.0, 7.0];
        let to = [-35.0, -10.0, 7.0];
        for mutation in 0..3 {
            let mut chunks = tile();
            match mutation {
                0 => chunks[16].positions[2][2] += 0.000_001,
                1 => hole(&mut chunks[16], 1),
                _ => chunks[16].impassable = true,
            }
            assert_eq!(
                prove_planar_tile(&chunks, TILE, from, to, 0.5),
                Err(CoverageError::UnsupportedTerrain)
            );
        }
    }

    #[test]
    fn zero_radius_stationary_seam_checks_both_closed_sides() {
        let at = [lattice(TILE.1 * 128 + 8), -10.0, 7.0];
        assert_eq!(prove_planar_tile(&tile(), TILE, at, at, 0.0), Ok(()));
        for side in [0, 16] {
            let mut chunks = tile();
            // A fan in each incident cell is independently required, not merely a seam ray.
            let row = if side == 0 { 7 } else { 0 };
            chunks[side].positions[row * 17 + 9 + 2][2] += 1.0;
            assert_eq!(
                prove_planar_tile(&chunks, TILE, at, at, 0.0),
                Err(CoverageError::UnsupportedTerrain)
            );
        }
    }

    #[test]
    fn four_way_corner_requires_all_four_duplicate_vertex_heights() {
        let seam = lattice(TILE.1 * 128 + 8);
        let at = [seam, seam, 7.0];
        assert_eq!(prove_planar_tile(&tile(), TILE, at, at, 0.0), Ok(()));
        for (slot, vertex) in [(0, 144), (1, 136), (16, 8), (17, 0)] {
            let mut chunks = tile();
            chunks[slot].positions[vertex][2] = 8.0;
            assert_eq!(
                prove_planar_tile(&chunks, TILE, at, at, 0.0),
                Err(CoverageError::UnsupportedTerrain)
            );
        }
    }

    #[test]
    fn hole_between_supported_endpoints_is_not_skipped() {
        let mut chunks = tile();
        hole(&mut chunks[0], 9);
        let (from, to) = ([-10.0, -10.0, 7.0], [-30.0, -10.0, 7.0]);
        for at in [from, to] {
            assert_eq!(prove_planar_tile(&chunks, TILE, at, at, 0.5), Ok(()));
        }
        assert_eq!(
            prove_planar_tile(&chunks, TILE, from, to, 0.5),
            Err(CoverageError::UnsupportedTerrain)
        );
    }

    #[test]
    fn tiny_nonzero_radius_expands_even_when_binary64_addition_loses_it() {
        let at = [-10.0, -10.0, 7.0];
        let rectangle = Rectangle::new(at, at, f32::from_bits(1)).unwrap();
        assert!(rectangle.min[0] < -10.0 && rectangle.max[0] > -10.0);
        let zero = Rectangle::new(at, at, 0.0).unwrap();
        assert_eq!(zero.min, [-10.0; 2]);
        assert_eq!(zero.max, [-10.0; 2]);
    }

    #[test]
    fn exhaustive_authored_corner_oracle_visits_exactly_four_incident_cells() {
        // [-35.5,-31.5]^2 intersects exactly global cells {7,8}×{7,8} in this tile.
        // Independently perturb every fan in its four chunks; only those four may reject.
        let mut chunks = tile();
        for (slot, row_base, col_base) in [(0, 0, 0), (1, 0, 8), (16, 8, 0), (17, 8, 8)] {
            for row in 0..8usize {
                for col in 0..8usize {
                    let center = row * 17 + 9 + col;
                    chunks[slot].positions[center][2] = 8.0;
                    let touched =
                        [7, 8].contains(&(row_base + row)) && [7, 8].contains(&(col_base + col));
                    assert_eq!(
                        prove_planar_tile(
                            &chunks,
                            TILE,
                            [-32.0, -32.0, 7.0],
                            [-35.0, -35.0, 7.0],
                            0.5,
                        ),
                        if touched {
                            Err(CoverageError::UnsupportedTerrain)
                        } else {
                            Ok(())
                        },
                        "chunk {slot}, cell {row},{col}"
                    );
                    chunks[slot].positions[center][2] = 7.0;
                }
            }
        }
    }

    #[test]
    fn radius_side_and_rectangle_interior_are_checked_between_endpoints() {
        // The center line stays in column 2; the body radius reaches column 3.
        let mut chunks = tile();
        chunks[0].positions[3 * 17 + 9 + 3][2] = 8.0;
        let (from, to) = ([-10.0, -12.0, 7.0], [-20.0, -12.0, 7.0]);
        assert_eq!(prove_planar_tile(&chunks, TILE, from, to, 0.0), Ok(()));
        assert_eq!(
            prove_planar_tile(&chunks, TILE, from, to, 1.0),
            Err(CoverageError::UnsupportedTerrain)
        );
        // A bounding rectangle is conservative: off-diagonal cells still have to be flat.
        chunks[0].positions[3 * 17 + 9 + 3][2] = 7.0;
        chunks[0].positions[2 * 17 + 9 + 4][2] = 8.0;
        assert_eq!(prove(&chunks), Err(CoverageError::UnsupportedTerrain));
    }

    #[test]
    fn missing_extra_duplicate_or_reversed_triangles_fail_closed() {
        for mutation in 0..4 {
            let mut chunks = tile();
            let indices = &mut chunks[0].indices;
            match mutation {
                0 => {
                    indices.truncate(indices.len() - 3);
                }
                1 => indices.extend([0, 1, 2]),
                2 => {
                    let duplicate = indices[0..3].to_vec();
                    indices[3..6].copy_from_slice(&duplicate);
                }
                _ => indices.swap(1, 2),
            }
            assert_eq!(prove(&chunks), Err(CoverageError::MalformedTerrain));
        }
    }

    #[test]
    fn malformed_vertices_or_indices_anywhere_in_tile_fail_closed() {
        for mutation in 0..7 {
            let mut chunks = tile();
            let chunk = &mut chunks[255];
            match mutation {
                0 => {
                    chunk.positions.pop();
                }
                1 => chunk.positions.push([0.0; 3]),
                2 => chunk.positions[0][0] = f32::NAN,
                3 => chunk.positions[0][2] = f32::INFINITY,
                4 => chunk.positions[9] = chunk.positions[0],
                5 => chunk.positions[1][1] += 0.001,
                _ => chunk.indices[0] = u32::MAX,
            }
            assert_eq!(prove(&chunks), Err(CoverageError::MalformedTerrain));
        }
    }

    #[test]
    fn missing_duplicate_and_wrong_tile_chunks_fail_closed() {
        let mut chunks = tile();
        chunks.pop();
        assert_eq!(prove(&chunks), Err(CoverageError::MalformedTerrain));
        chunks.push(chunk(0, 0));
        assert_eq!(prove(&chunks), Err(CoverageError::MalformedTerrain));
        assert_eq!(
            prove_planar_tile(&tile(), (31, 32), FROM, TO, 0.5),
            Err(CoverageError::MalformedTerrain)
        );
    }

    #[test]
    fn nonflat_neighbor_is_safe_because_its_triangles_cannot_enter_rectangle() {
        let mut chunks = tile();
        chunks[255].positions[9][2] += 100.0;
        assert_eq!(prove(&chunks), Ok(()));
    }

    #[test]
    fn impassable_or_even_empty_liquid_metadata_is_unsupported() {
        let mut chunks = tile();
        chunks[0].impassable = true;
        assert_eq!(prove(&chunks), Err(CoverageError::UnsupportedTerrain));
        chunks[0].impassable = false;
        chunks[0].liquids.push(benilla_formats::LiquidMesh {
            grid: [0, 0],
            wet: vec![],
            shared: vec![],
            positions: vec![],
            uvs: vec![],
            depths: vec![],
            indices: vec![],
            sound_nibble: 0,
            material_id: None,
            kind: benilla_formats::LiquidKind::Still,
        });
        assert_eq!(prove(&chunks), Err(CoverageError::UnsupportedTerrain));
    }

    #[test]
    fn invalid_inputs_are_unknown_without_scanning_unbounded_meshes() {
        for radius in [-1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                prove_planar_tile(&[], TILE, FROM, TO, radius),
                Err(CoverageError::InvalidInput)
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let from = [value, FROM[1], FROM[2]];
            assert_eq!(
                prove_planar_tile(&[], TILE, from, TO, 0.5),
                Err(CoverageError::InvalidInput)
            );
        }
        let chunks = tile();
        assert_eq!(
            prove_planar_tile(&chunks, TILE, FROM, TO, f32::MAX),
            Err(CoverageError::OutsideSupportedTile)
        );
    }
}
