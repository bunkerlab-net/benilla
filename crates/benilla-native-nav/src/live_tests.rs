// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const A: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
const B: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
fn tile(grid: [u8; 2], bytes: &[u8]) -> TileInput<'_> {
    TileInput { grid, bytes }
}

#[test]
fn source_order_assigns_slots_and_remove_reinsert_increments_salt() {
    // DetourNavMesh.cpp:246-251,931-938; DT64 bit widths in DetourNavMesh.h:43-45.
    let mut ab = MapMesh::new(PARAMS).unwrap();
    let a = ab.add_tile(tile([32, 32], A)).unwrap();
    let b = ab.add_tile(tile([32, 31], B)).unwrap();
    assert_eq!((a, b), (0x0010_0000_0000_0000, 0x0010_0000_8000_0000));
    let mut ba = MapMesh::new(PARAMS).unwrap();
    assert_eq!(ba.add_tile(tile([32, 31], B)).unwrap(), a);
    assert_eq!(ba.add_tile(tile([32, 32], A)).unwrap(), b);
    let mut links = [0; 16];
    let n = ab.links(a, 1, &mut links).unwrap();
    assert_eq!(&links[..n], &[b, a]);
    assert!(ab.remove_tile([32, 31]).unwrap());
    let n = ab.links(a, 1, &mut links).unwrap();
    assert_eq!(&links[..n], &[a]);
    let replacement = ab.add_tile(tile([32, 31], B)).unwrap();
    assert_eq!(replacement, 0x0020_0000_8000_0000);
    assert_eq!(ab.links(b, 0, &mut links), Err(LiveError::StaleTile));
    let n = ab.links(a, 1, &mut links).unwrap();
    assert_eq!(&links[..n], &[replacement, a]);
}
#[test]
fn instances_share_mesh_not_query_and_unload_fences_stale_allocations() {
    let mut mesh = MapMesh::new(PARAMS).unwrap();
    assert!(mesh.query(1).is_none());
    mesh.load_instance(1).unwrap();
    mesh.load_instance(2).unwrap();
    let one = mesh.query(1).unwrap();
    let two = mesh.query(2).unwrap();
    assert!(one.same_mesh(&two));
    assert!(!one.same_query(&two));
    assert_eq!(mesh.query_capacity(&one), Ok(1024));
    assert_eq!(mesh.query_capacity(&two), Ok(1024));
    mesh.load_instance(1).unwrap();
    assert!(one.same_query(&mesh.query(1).unwrap()));
    mesh.add_tile(tile([32, 32], A)).unwrap();
    assert_eq!(mesh.query_capacity(&one), Ok(1024));
    assert!(mesh.unload_instance(1));
    mesh.load_instance(1).unwrap();
    assert_eq!(mesh.query_capacity(&one), Err(LiveError::StaleQuery));
    assert_eq!(mesh.query_capacity(&two), Ok(1024));
    let mut foreign = MapMesh::new(PARAMS).unwrap();
    foreign.load_instance(2).unwrap();
    assert_eq!(foreign.query_capacity(&two), Err(LiveError::StaleQuery));
}
#[test]
fn malformed_duplicate_and_full_mesh_loads_preserve_links_and_free_slots() {
    let mut params = PARAMS.to_vec();
    params[20..24].copy_from_slice(&1u32.to_le_bytes());
    let mut mesh = MapMesh::new(&params).unwrap();
    assert!(mesh.add_tile(tile([32, 32], &A[..A.len() - 1])).is_err());
    assert_eq!(mesh.tiles().count(), 0);
    let a = mesh.add_tile(tile([32, 32], A)).unwrap();
    assert_eq!(a, 0x0010_0000_0000_0000);
    assert!(mesh.add_tile(tile([32, 32], A)).is_err());
    assert_eq!(
        mesh.add_tile(tile([32, 31], B)),
        Err(LiveError::Native(0x8000_0004))
    );
    assert_eq!(mesh.tiles().collect::<Vec<_>>(), vec![([32, 32], a)]);
    let mut links = [0; 16];
    assert_eq!(mesh.links(a, 0, &mut links), Ok(1));
    assert_eq!(links[0], a | 1);
    mesh.remove_tile([32, 32]).unwrap();
    assert_eq!(
        mesh.add_tile(tile([32, 31], B)).unwrap(),
        0x0020_0000_0000_0000
    );
}

#[test]
fn free_slot_salts_and_links_keep_source_history_after_failed_validation() {
    let mut mesh = MapMesh::new(PARAMS).unwrap();
    assert_eq!(
        mesh.slots().next(),
        Some(SlotState {
            salt: 1,
            occupied: false,
            next_slot: Some(1)
        })
    );
    mesh.add_tile(tile([32, 32], A)).unwrap();
    let before = mesh.slots().collect::<Vec<_>>();
    assert!(mesh.add_tile(tile([32, 31], &B[..B.len() - 1])).is_err());
    assert_eq!(mesh.slots().collect::<Vec<_>>(), before);
    let b = mesh.add_tile(tile([32, 31], B)).unwrap();
    mesh.remove_tile([32, 31]).unwrap();
    assert_eq!(
        mesh.slots().nth(1),
        Some(SlotState {
            salt: 2,
            occupied: false,
            next_slot: Some(2)
        })
    );
    assert_eq!(mesh.add_tile(tile([32, 31], B)).unwrap(), b + (1 << 52));
}

#[test]
fn detour_header_failures_keep_source_status_and_offmesh_numerics_are_not_approximated() {
    let mut mesh = MapMesh::new(PARAMS).unwrap();
    let before = mesh.slots().collect::<Vec<_>>();
    for (offset, word, expected) in [
        (20, 0, LiveError::Native(0x8000_0001)),
        (24, 8, LiveError::Native(0x8000_0002)),
        (72, 1, LiveError::OffMeshNumericBinding),
    ] {
        let mut bytes = A.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(word));
        assert_eq!(mesh.add_tile(tile([32, 32], &bytes)), Err(expected));
        assert_eq!(mesh.slots().collect::<Vec<_>>(), before);
    }
    assert!(mesh.add_tile(tile([32, 32], A)).is_ok());
}
