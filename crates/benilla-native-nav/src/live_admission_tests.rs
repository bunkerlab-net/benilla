// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const TILE: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
fn word(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn triangles(bytes: &[u8]) -> usize {
    120 + word(bytes, 48) * 12
        + word(bytes, 44) * 32
        + word(bytes, 52) * 16
        + word(bytes, 56) * 12
        + word(bytes, 60) * 12
}
fn insert(bytes: &[u8]) -> Result<MapMesh, LiveError> {
    let mut mesh = MapMesh::new(PARAMS)?;
    mesh.add_tile(TileInput {
        grid: [32, 32],
        bytes,
    })?;
    Ok(mesh)
}
#[test]
fn live_detail_winding_preserves_both_source_determinant_signs() {
    // DetourCommon.cpp:214-225 normalizes negative determinants before interpolation.
    let mut bytes = TILE.to_vec();
    let at = triangles(&bytes);
    bytes.swap(at, at + 1);
    bytes[at + 3] = 0x15;
    assert!(insert(TILE).is_ok());
    assert!(insert(&bytes).is_ok());
}

fn put_word(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn put_short(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes());
}
fn polys(bytes: &[u8]) -> usize {
    120 + word(bytes, 48) * 12
}
fn details(bytes: &[u8]) -> usize {
    polys(bytes) + word(bytes, 44) * 32 + word(bytes, 52) * 16
}
fn assert_undefined(bytes: &[u8], expected: UndefinedTile) {
    let mut mesh = MapMesh::new(PARAMS).unwrap();
    let before = mesh.slots().collect::<Vec<_>>();
    assert_eq!(
        mesh.add_tile(TileInput {
            grid: [32, 32],
            bytes
        })
        .err(),
        Some(LiveError::UndefinedTile(expected))
    );
    assert_eq!(mesh.slots().collect::<Vec<_>>(), before);
    assert!(mesh
        .add_tile(TileInput {
            grid: [32, 32],
            bytes: TILE
        })
        .is_ok());
}

#[test]
fn live_structural_ub_guards_preserve_slots_and_accept_the_safe_side() {
    // DetourNavMesh.cpp:1004,540-543,646-648,673,705-708,829-855.
    let ps = polys(TILE);
    let ds = details(TILE);
    let ts = triangles(TILE);
    let bv = ts + word(TILE, 64) * 4;
    for (at, value, expected) in [
        (52, 0, UndefinedTile::ZeroMaxLinks),
        (ds + 4, word(TILE, 64), UndefinedTile::DetailRange),
        (bv + 12, (-9i32) as u32 as usize, UndefinedTile::BvEscape),
        (bv + 28, word(TILE, 44), UndefinedTile::BvIndex),
    ] {
        let mut bytes = TILE.to_vec();
        put_word(&mut bytes, at, value);
        assert_undefined(&bytes, expected);
    }
    for (at, value, expected) in [
        (ps + 4, word(TILE, 48), UndefinedTile::CoarseVertexIndex),
        (
            ps + 16,
            word(TILE, 44) + 1,
            UndefinedTile::InternalPolygonIndex,
        ),
        (ps + 16, 0x8001, UndefinedTile::NonCardinalPortal),
    ] {
        let mut bytes = TILE.to_vec();
        put_short(&mut bytes, at, value);
        assert_undefined(&bytes, expected);
    }
    let mut bytes = TILE.to_vec();
    bytes[ps + 30] = 7;
    assert_undefined(&bytes, UndefinedTile::PolygonVertexCount);
    bytes[ps + 30] = 6;
    assert!(insert(&bytes).is_ok());
    let mut bytes = TILE.to_vec();
    bytes[ts] = 4;
    assert_undefined(&bytes, UndefinedTile::DetailVertexIndex);
    let mut bytes = TILE.to_vec();
    bytes[ts + 3] = 0;
    bytes[ts + 7] = 0;
    assert_undefined(&bytes, UndefinedTile::MissingDetailBoundary);
    for side in [0x8000, 0x8002, 0x8004, 0x8006, 0x8100] {
        let mut bytes = TILE.to_vec();
        put_short(&mut bytes, ps + 16, side);
        assert!(insert(&bytes).is_ok());
    }
}

#[test]
fn live_preserves_reflex_and_repeated_coarse_indices() {
    // DetourNavMesh.cpp:524-557 connects indices; DetourCommon.cpp:238-251 uses parity.
    let ps = polys(TILE);
    for indices in [[0, 2, 1, 3], [0, 0, 2, 3]] {
        let mut bytes = TILE.to_vec();
        for (i, value) in indices.into_iter().enumerate() {
            put_short(&mut bytes, ps + 4 + i * 2, value);
        }
        assert!(insert(&bytes).is_ok());
        assert!(crate::Mesh::load(
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes: &bytes
            }]
        )
        .is_err());
    }
    assert!(insert(TILE).is_ok());
}

#[test]
fn live_preserves_finite_vertices_outside_the_header_enclosure() {
    // DetourNavMesh.cpp:989-996 retains vertices; :822-834 clamps the query box only.
    let mut bytes = TILE.to_vec();
    bytes[124..128].copy_from_slice(&21f32.to_le_bytes());
    assert!(insert(&bytes).is_ok());
    bytes[124..128].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(
        insert(&bytes).err(),
        Some(LiveError::Validation(Error::Format))
    );
}

#[test]
fn live_scans_duplicate_flagged_and_disconnected_detail_edges_without_repair() {
    // DetourNavMesh.cpp:632-674 scans each triangle and its own boundary flags.
    let ts = triangles(TILE);
    for pair in [
        [[0, 1, 2, 0x15], [0, 1, 2, 0x15]],
        [[0, 1, 2, 0x15], [0, 2, 3, 0x15]],
        [[0, 1, 2, 0x15], [1, 2, 3, 0x15]],
        [[0, 1, 0, 0x15], [2, 3, 2, 0x15]],
    ] {
        let mut bytes = TILE.to_vec();
        bytes[ts..ts + 4].copy_from_slice(&pair[0]);
        bytes[ts + 4..ts + 8].copy_from_slice(&pair[1]);
        assert!(insert(&bytes).is_ok());
        assert!(crate::Mesh::load(
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes: &bytes
            }]
        )
        .is_err());
    }
    assert!(insert(TILE).is_ok());
}

#[test]
fn live_magma_and_ignored_outer_metadata_follow_source() {
    // MoveMapSharedDefines.h:48,60; MoveMap.cpp:235-267 never checks these metadata.
    let mut bytes = TILE.to_vec();
    let ps = polys(&bytes);
    put_short(&mut bytes, ps + 28, 8);
    bytes[ps + 31] = 8;
    assert!(insert(&bytes).is_ok());
    assert!(crate::Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: &bytes
        }]
    )
    .is_err());
    put_word(&mut bytes, 4, 99);
    put_word(&mut bytes, 16, 42);
    assert!(insert(&bytes).is_ok());
    put_word(&mut bytes, 0, 0);
    assert_eq!(
        insert(&bytes).err(),
        Some(LiveError::Validation(Error::Format))
    );
}

fn repeated_tile(
    template: &[u8],
    polygon: usize,
    n: usize,
    links: usize,
    extra: usize,
    tris: usize,
) -> Vec<u8> {
    let ps = polys(template);
    let mut bytes = template[..ps].to_vec();
    let p = &template[ps + polygon * 32..ps + (polygon + 1) * 32];
    for _ in 0..n {
        let mut p: [u8; 32] = p.try_into().unwrap();
        p[16..28].fill(0);
        bytes.extend_from_slice(&p);
    }
    bytes.resize(bytes.len() + links * 16, 0);
    for pi in 0..n {
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&((pi * 2) as u32).to_le_bytes());
        bytes.extend_from_slice(&[0, 2, 0, 0]);
    }
    bytes.resize(bytes.len() + extra * 12, 0);
    for _ in 0..tris / 2 {
        bytes.extend_from_slice(&[0, 1, 2, 5, 0, 2, 3, 20]);
    }
    if !tris.is_multiple_of(2) {
        bytes.extend_from_slice(&[0, 1, 2, 5]);
    }
    for (at, value) in [
        (44, n),
        (52, links),
        (56, n),
        (60, extra),
        (64, tris),
        (68, 0),
        (72, 0),
        (76, n),
    ] {
        put_word(&mut bytes, at, value);
    }
    let size = bytes.len() - 20;
    put_word(&mut bytes, 12, size);
    bytes
}

#[test]
fn live_uses_payload_counts_not_bounded_seed_polygon_and_detail_caps() {
    // DetourNavMesh.cpp:918-923 excludes the polygon cap under DT64; :979-986 uses counts.
    for n in [16384, 16385] {
        assert!(insert(&repeated_tile(TILE, 0, n, 1, 0, n * 2)).is_ok());
    }
    for (extra, tris) in [(0, 65536), (0, 65537), (65536, 65536), (65536, 65537)] {
        assert!(insert(&repeated_tile(TILE, 0, 1, 1, extra, tris)).is_ok());
    }
    let mut bytes = repeated_tile(TILE, 0, 16385, 1, 0, 32770);
    bytes.pop();
    assert_eq!(
        insert(&bytes).err(),
        Some(LiveError::Validation(Error::Framing))
    );
}

#[test]
fn live_native_portals_keep_four_match_truncation_dropped_links_and_large_load_work() {
    // DetourNavMesh.cpp:339-347 caps at four; :418-419 skips exhausted link pools.
    const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
    for n in [1, 6, 1000, 1500] {
        let mut left = repeated_tile(TILE, 1, n, n * 4, 0, n * 2);
        let mut right = repeated_tile(RIGHT, 0, n, 1, 0, n * 2);
        let lp = polys(&left);
        let rp = polys(&right);
        for pi in 0..n {
            put_short(&mut left, lp + pi * 32 + 20, 0x8000);
            put_short(&mut right, rp + pi * 32 + 16, 0x8004);
        }
        let mut mesh = MapMesh::new(PARAMS).unwrap();
        let a = mesh
            .add_tile(TileInput {
                grid: [32, 32],
                bytes: &left,
            })
            .unwrap();
        let b = mesh
            .add_tile(TileInput {
                grid: [32, 31],
                bytes: &right,
            })
            .unwrap();
        let mut out = [0; 8];
        let len = mesh.links(a, 0, &mut out).unwrap();
        let expected: Vec<_> = (0..n.min(4)).rev().map(|pi| b | pi as u64).collect();
        assert_eq!(&out[..len], expected);
        assert_eq!(mesh.links(b, 0, &mut out).unwrap(), 1);
        assert_eq!(out[0], a);
        if n > 1 {
            assert_eq!(mesh.links(b, 1, &mut out).unwrap(), 0);
        }
        mesh.remove_tile([32, 31]).unwrap();
        assert_eq!(mesh.links(a, 0, &mut out).unwrap(), 0);
    }
}

fn offmesh_tile() -> Vec<u8> {
    let n = word(TILE, 44);
    let nv = word(TILE, 48);
    let ps = polys(TILE);
    let ls = ps + n * 32;
    let endpoints = [-400f32, 0., -400., -420., 0., -100.];
    let mut bytes = TILE[..ps].to_vec();
    for value in endpoints {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&TILE[ps..ls]);
    let mut p = [0u8; 32];
    put_short(&mut p, 4, nv);
    put_short(&mut p, 6, nv + 1);
    put_short(&mut p, 28, 1);
    p[30] = 2;
    p[31] = 0x40 | 11;
    bytes.extend_from_slice(&p);
    bytes.extend_from_slice(&TILE[ls..]);
    for value in endpoints {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&1f32.to_le_bytes());
    bytes.extend_from_slice(&(n as u16).to_le_bytes());
    bytes.extend_from_slice(&[1, 0xff]);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for (at, value) in [(44, n + 1), (48, nv + 2), (72, 1), (76, n)] {
        put_word(&mut bytes, at, value);
    }
    let size = bytes.len() - 20;
    put_word(&mut bytes, 12, size);
    bytes
}

#[test]
fn live_offmesh_uses_pinned_squared_nearest_and_rejects_out_of_range_connections() {
    // DetourNavMesh.cpp:568-615,760-805 connects both endpoints without sqrt.
    let mut bytes = offmesh_tile();
    let mesh = insert(&bytes).unwrap();
    let reference = mesh.tile_reference([32, 32]).unwrap();
    let mut links = [0; 8];
    assert_eq!(mesh.links(reference, 4, &mut links).unwrap(), 2);
    assert_eq!(&links[..2], &[reference | 2, reference]);
    let at = bytes.len() - 36 + 28;
    put_short(&mut bytes, at, 5);
    assert_undefined(&bytes, UndefinedTile::OffMeshPolygonIndex);
    let mut bytes = offmesh_tile();
    let ps = polys(&bytes);
    bytes[ps + 30] = 2;
    bytes[ps + 31] = 0x40 | 11;
    assert_undefined(&bytes, UndefinedTile::OffMeshPolygonIndex);
}
