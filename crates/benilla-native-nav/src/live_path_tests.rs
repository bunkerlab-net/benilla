// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn source_host() -> bool {
    if cfg!(target_arch = "x86_64") {
        return true;
    }
    assert!(matches!(
        MapMesh::new(include_bytes!("../tools/fixtures/authored.mmap")),
        Err(LiveError::NumericBinding)
    ));
    false
}

const BASE: u64 = 0x0010_0000_0000_0000;
const FILTER: SourceOwnerFilter = SourceOwnerFilter {
    include_flags: 1,
    exclude_flags: 0,
};
fn mesh() -> MapMesh {
    let mut mesh = MapMesh::new(include_bytes!(
        "../tools/fixtures/direct-corridor/params.test-fixture"
    ))
    .unwrap();
    mesh.load_instance(7).unwrap();
    mesh.add_tile(TileInput {
        grid: [31, 31],
        bytes: include_bytes!("../tools/fixtures/warm-cache/precision.test-fixture"),
    })
    .unwrap();
    mesh
}
#[test]
fn live_multi_poly_subrange_preserves_tail_and_skips_new_search_563_573() {
    if !source_host() {
        return;
    }
    let mesh = mesh();
    let handle = mesh.query(7).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    let mut cache = PathCache::new();
    let result = query
        .poly_path(
            &mut cache,
            FILTER,
            [BASE, BASE + 1],
            [5., 0., 5.],
            [15., 0., 5.],
            false,
        )
        .unwrap();
    assert_eq!(result.status, 0x4000_0000);
    assert_eq!(cache.active(), [BASE, BASE + 1]);
    let mut prior = PathDistance::new(DistanceDeclaration::BuildPolyStart);
    mesh.pathfinder_lookup(&handle, &mut cache, FILTER, [5., 0., 5.], &mut prior)
        .unwrap();
    let tail = cache.storage()[74..].to_vec();
    assert!(cache.reuse_subrange(BASE, BASE + 1));
    assert_eq!(cache.active(), [BASE, BASE + 1]);
    assert_eq!(&cache.storage()[74..], tail);
    assert!(!cache.reuse_subrange(BASE + 1, BASE));
    assert!(!cache.reuse_subrange(BASE, BASE));
}
#[test]
fn live_find_path_keeps_partial_buffer_and_out_of_nodes_bits_763_789() {
    if !source_host() {
        return;
    }
    for (label, bytes) in [
        (
            "long600_1024",
            include_bytes!("../tools/fixtures/direct-corridor/long600.test-fixture").as_slice(),
        ),
        (
            "long1100_1024",
            include_bytes!("../tools/fixtures/direct-corridor/long1100.test-fixture").as_slice(),
        ),
    ] {
        let line = include_str!("../tools/fixtures/direct-corridor/cases.test-fixture")
            .lines()
            .find(|l| l.starts_with(&format!("{label}\t")))
            .unwrap();
        let cells = line.split('\t').collect::<Vec<_>>();
        let parse = |s: &str| {
            let v = s
                .split(',')
                .map(|w| f32::from_bits(u32::from_str_radix(w, 16).unwrap()))
                .collect::<Vec<_>>();
            [v[1], v[2], v[0]]
        };
        let mut mesh = MapMesh::new(include_bytes!("../tools/fixtures/authored.mmap")).unwrap();
        mesh.load_instance(0).unwrap();
        mesh.add_tile(TileInput {
            grid: [32, 32],
            bytes,
        })
        .unwrap();
        let handle = mesh.query(0).unwrap();
        let query = mesh.pathfinder_query(&handle).unwrap();
        let mut cache = PathCache::new();
        let result = query
            .poly_path(
                &mut cache,
                FILTER,
                [cells[7].parse().unwrap(), cells[11].parse().unwrap()],
                parse(cells[2]),
                parse(cells[3]),
                false,
            )
            .unwrap();
        assert_eq!(result.status, u32::from_str_radix(cells[14], 16).unwrap());
        assert_eq!(
            cache.active(),
            cells[15]
                .split(',')
                .map(|s| s.parse::<u64>().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(cache.active_len(), 74);
    }
}
#[test]
fn live_raycast_and_straight_points_use_the_selected_query_697_866() {
    if !source_host() {
        return;
    }
    let mesh = mesh();
    let handle = mesh.query(7).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    let mut cache = PathCache::new();
    let result = query
        .poly_path(
            &mut cache,
            FILTER,
            [BASE, BASE + 1],
            [5., 0., 5.],
            [15., 0., 5.],
            true,
        )
        .unwrap();
    assert_eq!(result.status, 0x4000_0000);
    assert_eq!(result.hit, f32::MAX);
    let (mut points, mut scratch) = (Vec::new(), Vec::new());
    let (status, count) = query
        .points(
            &cache,
            FILTER,
            [5., 0., 5.],
            [15., 0., 5.],
            true,
            &mut points,
            &mut scratch,
        )
        .unwrap();
    assert_eq!((status, count), (0x4000_0000, 2));
    assert_eq!(&points[..6], [5., 0., 5., 15., 0., 5.]);
    let hit = query
        .poly_path(
            &mut cache,
            FILTER,
            [BASE, BASE],
            [5., 0., 5.],
            [5., 0., 25.],
            true,
        )
        .unwrap();
    assert_eq!(hit.hit, 0.25);
}
#[test]
fn live_smoother_keeps_original_height_offset_and_dungeon_capacity_1233_1348() {
    if !source_host() {
        return;
    }
    let mesh = mesh();
    let handle = mesh.query(7).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    let mut cache = PathCache::new();
    cache.dungeon_limit(301.);
    query
        .poly_path(
            &mut cache,
            FILTER,
            [BASE, BASE + 1],
            [5., 0., 5.],
            [15., 0., 5.],
            false,
        )
        .unwrap();
    let (mut points, mut scratch) = (Vec::new(), Vec::new());
    let (status, count) = query
        .points(
            &cache,
            FILTER,
            [5., 0., 5.],
            [15., 0., 5.],
            false,
            &mut points,
            &mut scratch,
        )
        .unwrap();
    assert_eq!((status, count), (0x4000_0000, 4));
    assert_eq!(
        &points[..12],
        [5., 0., 5., 9., 0.5, 5., 13., 0.5, 5., 15., 0., 5.]
    );
    assert_eq!((points.len(), scratch.len()), (888, 296));
}
#[test]
fn live_path_query_rejects_destroyed_instance_and_not_a_replacement_122() {
    if !source_host() {
        return;
    }
    let mut mesh = mesh();
    let handle = mesh.query(7).unwrap();
    mesh.unload_instance(7);
    mesh.load_instance(7).unwrap();
    assert!(matches!(
        mesh.pathfinder_query(&handle),
        Err(LiveError::StaleQuery)
    ));
}

#[test]
fn zero_sentinel_compaction_preserves_resized_prior_slots_917_929() {
    if !source_host() {
        return;
    }
    let mesh = mesh();
    let handle = mesh.query(7).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    let input = [0., 0., 0., 1., 0., 1., 2., 0., 2.];
    for prior in [vec![], vec![[8.; 3], [9.; 3]]] {
        let expected = if prior.is_empty() { [0.; 3] } else { [9.; 3] };
        let mut out = prior;
        assert_eq!(query.convert_points(&input, 3, true, &mut out).unwrap(), 2);
        assert_eq!(out, [[2., 2., 0.], expected]);
    }
}
#[test]
fn missing_intermediate_tiles_keep_endpoint_presence_and_partial_status_169_789() {
    if !source_host() {
        return;
    }
    let mut mesh = MapMesh::new(include_bytes!(
        "../tools/fixtures/direct-corridor/params.test-fixture"
    ))
    .unwrap();
    mesh.load_instance(0).unwrap();
    let first = mesh
        .add_tile(TileInput {
            grid: [32, 32],
            bytes: include_bytes!("../tools/fixtures/direct-corridor/grid00.test-fixture"),
        })
        .unwrap();
    let last = mesh
        .add_tile(TileInput {
            grid: [31, 31],
            bytes: include_bytes!("../tools/fixtures/direct-corridor/grid11.test-fixture"),
        })
        .unwrap();
    let handle = mesh.query(0).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    let start = [-320., 0., f32::from_bits(0xc255_5558)];
    let end = [f32::from_bits(0x43fd_5556), 0., f32::from_bits(0x41d5_5540)];
    assert!(query.have_tile(start).unwrap());
    assert!(query.have_tile(end).unwrap());
    let mut cache = PathCache::new();
    let result = query
        .poly_path(&mut cache, FILTER, [first, last], start, end, false)
        .unwrap();
    assert_eq!(result.status, 0x4000_0040);
    assert_eq!(cache.active(), [first]);
}

#[test]
fn have_tile_does_not_consume_ignored_height_detour_1191_1195() {
    if !source_host() {
        return;
    }
    let mesh = mesh();
    let handle = mesh.query(7).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    assert!(query.have_tile([5., f32::MAX, 5.]).unwrap());
    assert!(query.have_tile([5., f32::NAN, 5.]).unwrap());
    assert!(matches!(
        query.have_tile([f32::NAN, 0., 5.]),
        Err(LiveError::Validation(Error::Budget))
    ));
}

fn offmesh_transition_tile() -> Vec<u8> {
    let tile = include_bytes!("../tools/fixtures/0003232.mmtile");
    let word = |at| u32::from_le_bytes(tile[at..at + 4].try_into().unwrap()) as usize;
    let n = word(44);
    let nv = word(48);
    let ps = 120 + nv * 12;
    let ls = ps + n * 32;
    let endpoints = [-400f32, 0., -400., -420., 0., -100.];
    let mut bytes = tile[..ps].to_vec();
    for value in endpoints {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&tile[ps..ls]);
    let mut poly = [0u8; 32];
    poly[4..6].copy_from_slice(&(nv as u16).to_le_bytes());
    poly[6..8].copy_from_slice(&((nv + 1) as u16).to_le_bytes());
    poly[28..30].copy_from_slice(&1u16.to_le_bytes());
    poly[30] = 2;
    poly[31] = 0x40 | 11;
    bytes.extend_from_slice(&poly);
    bytes.extend_from_slice(&tile[ls..]);
    for value in endpoints {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&1f32.to_le_bytes());
    bytes.extend_from_slice(&(n as u16).to_le_bytes());
    bytes.extend_from_slice(&[1, 0xff]);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for (at, value) in [(44, n + 1), (48, nv + 2), (72, 1), (76, n)] {
        bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
    }
    let size = bytes.len() - 20;
    bytes[12..16].copy_from_slice(&(size as u32).to_le_bytes());
    bytes
}
#[test]
fn offmesh_transition_copies_original_arguments_not_connection_endpoints_1326_1330() {
    if !source_host() {
        return;
    }
    let mut mesh = MapMesh::new(include_bytes!("../tools/fixtures/authored.mmap")).unwrap();
    mesh.load_instance(7).unwrap();
    let bytes = offmesh_transition_tile();
    let first = mesh
        .add_tile(TileInput {
            grid: [32, 32],
            bytes: &bytes,
        })
        .unwrap();
    let handle = mesh.query(7).unwrap();
    let query = mesh.pathfinder_query(&handle).unwrap();
    let start = [-410., 0., -410.];
    let end = [-430., 0., -110.];
    let mut cache = PathCache::new();
    let result = query
        .poly_path(&mut cache, FILTER, [first, first | 2], start, end, false)
        .unwrap();
    assert_eq!(result.status, 0x4000_0000);
    assert_eq!(cache.active(), [first, first | 4, first | 2]);
    let (mut points, mut scratch) = (Vec::new(), Vec::new());
    let (status, count) = query
        .points(&cache, FILTER, start, end, false, &mut points, &mut scratch)
        .unwrap();
    assert_eq!(status, 0x4000_0000);
    assert!(count >= 3);
    assert_eq!(&points[..3], &start);
    assert_eq!(&points[(count - 2) * 3..(count - 1) * 3], &start);
    assert_eq!(&points[(count - 1) * 3..count * 3], &[-430., 0.5, -110.]);
    assert_eq!(scratch[0], first | 2);
    assert_eq!(cache.active(), [first, first | 4, first | 2]);
}
