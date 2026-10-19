// SPDX-License-Identifier: GPL-3.0-or-later
//! Unmodified pinned source MapBuilder output, generated from authored client-shaped inputs.
use super::*;

fn captured_ridge() -> (Vec<u8>, Vec<u8>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/native-navigation/fixtures/ridge_wmo");
    (
        std::fs::read(root.join("000.mmap")).unwrap(),
        std::fs::read(root.join("0003130.mmtile")).unwrap(),
    )
}

fn word(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

fn short(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn point(bytes: &[u8], offset: usize) -> [f32; 3] {
    std::array::from_fn(|axis| {
        f32::from_le_bytes(
            bytes[offset + axis * 4..offset + axis * 4 + 4]
                .try_into()
                .unwrap(),
        )
    })
}

fn set_real(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

struct Layout {
    polygons: usize,
    polygon_count: usize,
    links: usize,
    link_count: usize,
    details: usize,
    detail_vertices: usize,
    bv: usize,
    bv_count: usize,
}

impl Layout {
    fn read(tile: &[u8]) -> Self {
        let polygon_count = word(tile, 44);
        let polygons = 120 + word(tile, 48) * 12;
        let links = polygons + polygon_count * 32;
        let link_count = word(tile, 52);
        let details = links + link_count * 16;
        let detail_vertices = details + polygon_count * 12;
        let detail_triangles = detail_vertices + word(tile, 60) * 12;
        let bv = detail_triangles + word(tile, 64) * 4;
        let bv_count = word(tile, 68);
        assert_eq!(bv + bv_count * 16, tile.len());
        Self {
            polygons,
            polygon_count,
            links,
            link_count,
            details,
            detail_vertices,
            bv,
            bv_count,
        }
    }

    fn base_vertex(&self, tile: &[u8], polygon: usize, index: usize) -> [f32; 3] {
        assert!(polygon < self.polygon_count);
        let at = self.polygons + polygon * 32;
        assert!(index < usize::from(tile[at + 30]));
        let vertex = usize::from(short(tile, at + 4 + index * 2));
        point(tile, 120 + vertex * 12)
    }

    fn extra_vertex_offset(&self, tile: &[u8], polygon: usize, index: usize) -> usize {
        let at = self.details + polygon * 12;
        assert!(index < usize::from(tile[at + 8]));
        self.detail_vertices + (word(tile, at) + index) * 12
    }

    fn leaf_offset(&self, tile: &[u8], polygon: usize) -> usize {
        (0..self.bv_count - 1)
            .map(|index| self.bv + index * 16)
            .find(|&at| word(tile, at + 12) == polygon)
            .expect("captured source BV leaf must exist")
    }
}

fn assert_format_rejection(params: &[u8], tile: &[u8]) {
    match Mesh::load(
        params,
        &[TileInput {
            grid: [31, 30],
            bytes: tile,
        }],
    ) {
        Err(Error::Format) => {}
        Err(other) => panic!("mutation must fail safe format validation, got {other:?}"),
        Ok(_) => panic!("mutated source tile reached native admission"),
    }
}

fn json_point(value: &serde_json::Value) -> [u32; 3] {
    let fields = value.as_array().unwrap();
    assert_eq!(fields.len(), 3);
    std::array::from_fn(|axis| u32::from_str_radix(fields[axis].as_str().unwrap(), 16).unwrap())
}

#[test]
fn source_generated_tiles_match_same_input_original_scalar_queries() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/native-navigation/fixtures");
    let mut query_count = 0;
    for case in ["ridge_wmo", "ridge_neighbors_wmo", "ridge_xminus_wmo"] {
        let params = std::fs::read(fixtures.join(case).join("000.mmap")).unwrap();
        let tile = std::fs::read(fixtures.join(case).join("0003130.mmtile")).unwrap();
        let expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixtures.join("scalar-nearest").join(format!("{case}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(expected["schema"], 1);
        assert_eq!(expected["include_flags"], 1);
        assert_eq!(expected["exclude_flags"], 0);
        assert_eq!(json_point(&expected["extents_bits"]), [5.0f32.to_bits(); 3]);
        let mut mesh = Mesh::load(
            &params,
            &[TileInput {
                grid: [31, 30],
                bytes: &tile,
            }],
        )
        .unwrap_or_else(|error| panic!("original {case} must load: {error:?}"));
        let queries = expected["queries"].as_array().unwrap();
        assert_eq!(queries.len(), 25, "{case}");
        for query in queries {
            // point_bits is the independently measured original scalar query input.
            let input = json_point(&query["point_bits"]);
            let nearest = json_point(&query["nearest_bits"]);
            assert_eq!(query["status"].as_u64().unwrap(), 0x4000_0000, "{case}");
            let result = mesh
                .nearest(
                    [input[2], input[0], input[1]],
                    5.0,
                    Instant::now() + std::time::Duration::from_secs(5),
                )
                .unwrap();
            assert_eq!(
                result.polygon,
                query["poly_ref"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(result.point, [nearest[2], nearest[0], nearest[1]], "{case}");
            assert_eq!(
                result.over,
                query["is_over_poly"].as_bool().unwrap(),
                "{case}"
            );
            query_count += 1;
        }
    }
    assert_eq!(query_count, 75);
}

#[test]
fn serialized_link_cache_is_rebuilt_before_use() {
    let (params, mut tile) = captured_ridge();
    let layout = Layout::read(&tile);
    tile[layout.links..layout.links + layout.link_count * 16].fill(0xff);
    for i in 0..layout.polygon_count {
        tile[layout.polygons + i * 32..layout.polygons + i * 32 + 4].fill(0xff);
    }
    let mut mesh = Mesh::load(
        &params,
        &[TileInput {
            grid: [31, 30],
            bytes: &tile,
        }],
    )
    .unwrap();
    let result = mesh
        .nearest(
            [0x42b1c71c, 0x441b8e38, 0x40ecccc9],
            5.0,
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(result.polygon, 4503599627370600);
    assert_eq!(result.point, [0x42b1c71c, 0x441b8e38, 0x40ecccc9]);
}

#[test]
fn two_ulp_outward_axis_detail_displacement_is_rejected() {
    let (params, mut tile) = captured_ridge();
    let layout = Layout::read(&tile);
    let a = layout.base_vertex(&tile, 56, 0);
    let b = layout.base_vertex(&tile, 56, 1);
    let extra = layout.extra_vertex_offset(&tile, 56, 0);
    let p = point(&tile, extra);
    assert_eq!(a[2], b[2]);
    assert_eq!(p[2], a[2].next_up());
    assert!(p[0] > a[0] && p[0] < b[0]);
    set_real(&mut tile, extra + 8, a[2].next_up().next_up());
    assert_format_rejection(&params, &tile);
}

#[test]
fn adjacent_axis_detail_cannot_extend_past_the_edge_endpoint() {
    let (params, mut tile) = captured_ridge();
    let layout = Layout::read(&tile);
    let a = layout.base_vertex(&tile, 56, 0);
    let b = layout.base_vertex(&tile, 56, 1);
    let extra = layout.extra_vertex_offset(&tile, 56, 0);
    assert_eq!(point(&tile, extra)[2], a[2].next_up());
    assert!(a[0] < b[0]);
    set_real(&mut tile, extra, b[0].next_up());
    assert_format_rejection(&params, &tile);
}

#[test]
fn extreme_but_finite_detail_y_is_rejected() {
    let (params, mut tile) = captured_ridge();
    let layout = Layout::read(&tile);
    let extra = layout.extra_vertex_offset(&tile, 56, 0);
    // Still within the general coordinate decoder limit, far outside source span tops.
    set_real(&mut tile, extra + 4, 100_000.0);
    assert_format_rejection(&params, &tile);
}

#[test]
fn bv_base_reconstruction_cannot_justify_unrelated_leaf_bounds() {
    let (params, mut tile) = captured_ridge();
    let layout = Layout::read(&tile);
    let leaf = layout.leaf_offset(&tile, 50);
    let origin = point(&tile, 92);
    let q = f32::from_bits(word(&tile, 116) as u32);
    let y = layout.base_vertex(&tile, 50, 0)[1];
    assert_eq!(((y - origin[1]) * q) as u16, 24);
    assert_eq!((short(&tile, leaf + 2), short(&tile, leaf + 8)), (26, 26));
    let reconstructed_high = (((y + 1.0 / q).next_up() - origin[1]) * q) as u16;
    assert_eq!(reconstructed_high, 26);
    // One integer past the source reconstruction cannot be accepted as the same leaf.
    tile[leaf + 2..leaf + 4].copy_from_slice(&27u16.to_le_bytes());
    tile[leaf + 8..leaf + 10].copy_from_slice(&27u16.to_le_bytes());
    assert_format_rejection(&params, &tile);
}

#[test]
fn serialized_extra_detail_cannot_borrow_base_vertex_bv_reconstruction() {
    let (params, mut tile) = captured_ridge();
    let layout = Layout::read(&tile);
    let extra = layout.extra_vertex_offset(&tile, 56, 0);
    let leaf = layout.leaf_offset(&tile, 56);
    let origin = point(&tile, 92);
    let q = f32::from_bits(word(&tile, 116) as u32);
    let y = layout.base_vertex(&tile, 56, 0)[1];
    let lo = short(&tile, leaf + 2);
    let hi = short(&tile, leaf + 8);
    assert!(y >= origin[1] && y < point(&tile, 104)[1]);
    assert!((((y - origin[1]) * q) as u16) < lo);
    let reconstructed_high = (((y + 1.0 / q).next_up() - origin[1]) * q) as u16;
    assert!((lo..=hi).contains(&reconstructed_high));
    // X/Z and all projected detail topology stay unchanged. Only an illicit
    // base-vertex +cell fallback could admit this serialized extra outside its BV leaf.
    set_real(&mut tile, extra + 4, y);
    assert_format_rejection(&params, &tile);
}

#[test]
fn source_quantized_portal_rejects_one_ulp_interior_displacement() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/native-navigation/fixtures/ridge_neighbors_wmo");
    let params = std::fs::read(root.join("000.mmap")).unwrap();
    let mut tile = std::fs::read(root.join("0003130.mmtile")).unwrap();
    let layout = Layout::read(&tile);
    let (polygon, edge, axis) = (0..layout.polygon_count)
        .find_map(|polygon| {
            let at = layout.polygons + polygon * 32;
            (0..usize::from(tile[at + 30])).find_map(|edge| {
                match short(&tile, at + 16 + edge * 2) {
                    0x8000 => Some((polygon, edge, 0)),
                    0x8002 => Some((polygon, edge, 2)),
                    _ => None,
                }
            })
        })
        .expect("neighbor capture has positive-side source portals");
    let at = layout.polygons + polygon * 32;
    let count = usize::from(tile[at + 30]);
    let source_plane = point(&tile, 92)[axis] + 2000.0 * 0.2666666_f32;
    let header_plane = point(&tile, 104)[axis];
    assert_eq!(word(&tile, 116) as u32, (1.0 / 0.2666666_f32).to_bits());
    assert!(source_plane < header_plane);
    let neighbor_at = at + 16 + edge * 2;
    let original_side = short(&tile, neighbor_at);
    // The source positive-plane exception must not admit an edge claiming the
    // opposite side. Keep the geometry intact for this independent mutation.
    tile[neighbor_at..neighbor_at + 2].copy_from_slice(&(original_side ^ 4).to_le_bytes());
    assert_format_rejection(&params, &tile);
    tile[neighbor_at..neighbor_at + 2].copy_from_slice(&original_side.to_le_bytes());
    for endpoint in [edge, (edge + 1) % count] {
        let vertex = usize::from(short(&tile, at + 4 + endpoint * 2));
        let offset = 120 + vertex * 12 + axis * 4;
        assert_eq!(point(&tile, 120 + vertex * 12)[axis], source_plane);
        // This is one float toward the interior from the source plane itself,
        // rather than a value between that plane and the wider header boundary.
        set_real(&mut tile, offset, source_plane.next_down());
    }
    assert_format_rejection(&params, &tile);
}
