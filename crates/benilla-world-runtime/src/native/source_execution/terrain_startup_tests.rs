// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored F6.01 owner tests, not world admission.
use super::*;
use benilla_sim::source_rng::{DrawCursor, EventKind, Mt19937, RawGenerator, SourceRandom};
use std::collections::{BTreeMap, VecDeque};

struct Words(VecDeque<u64>);
impl RawGenerator for Words {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        self.0.pop_front().expect("unexpected RNG draw")
    }
}
fn map_bytes(flag: u16, cells: bool) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(b"MAPSz1.4");
    bytes[8..12].copy_from_slice(&40u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&(if cells { 520u32 } else { 8u32 }).to_le_bytes());
    bytes.extend_from_slice(b"AREA");
    bytes.extend_from_slice(&u16::from(!cells).to_le_bytes());
    bytes.extend_from_slice(&flag.to_le_bytes());
    if cells {
        for value in 0..256u16 {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}
fn dbc() -> SourceTerrainDbc {
    SourceTerrainDbc {
        areas: [
            SourceAreaEntry {
                id: 7,
                map: 0,
                zone: 900,
                flag: 6,
            },
            SourceAreaEntry {
                id: 8,
                map: 1,
                zone: 0,
                flag: 6,
            },
        ]
        .into_iter()
        .map(|a| (a.id, a))
        .collect(),
        linked_zones: BTreeMap::new(),
    }
}
#[test]
fn lookup_constructs_once_in_reached_order_and_failed_draw_precedes_vmap_creation() {
    // GridMap.cpp:693-714,1348-1359.
    let mut manager = SourceTerrainManager::default();
    let mut words = Words([u64::MAX].into());
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut words, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        manager.load_terrain(0, &mut random).err().unwrap().id,
        "execution.residency.terrain_random_owner"
    );
    assert!(manager.is_empty());
    assert!(!manager.vmap.created());
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        manager
            .load_terrain(0, &mut random)
            .unwrap()
            .residency
            .cleanup_current(),
        37000
    );
    manager.load_terrain(0, &mut random).unwrap();
    assert_eq!(random.cursor().raw, 1);
    assert_eq!(
        manager
            .load_terrain(1, &mut random)
            .unwrap()
            .residency
            .cleanup_current(),
        22000
    );
    assert_eq!(random.cursor().raw, 2);
    assert_eq!(manager.len(), 2);
    assert!(manager.vmap.created());
    assert_eq!(manager.vmap.loaded_trees(), 0);
}
#[test]
fn constructor_rejection_span_and_map_only_queries_need_no_map_identity() {
    // GridMap.cpp:708 and GetGrid :1222-1238. Rejection is real engine work.
    let mut words = Words([0, 1].into());
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    let mut sink = |event| events.push(event);
    let mut random = SourceRandom::new(1, &mut words, &mut cursor, &mut sink).unwrap();
    let mut files = SourceTerrainFiles::default();
    files.bytes(0, 32, 32, map_bytes(6, false));
    let mut manager = SourceTerrainManager::new(files, dbc());
    assert_eq!(
        manager.original_zone(0, [0.0; 3], &mut random).unwrap(),
        900
    );
    assert_eq!(random.cursor().raw, 2);
    assert_eq!(
        manager.terrain(0).unwrap().residency.cleanup_current(),
        20000
    );
    let info = manager.terrain(0).unwrap();
    assert_eq!(info.references(), 0);
    assert_eq!(info.residency.reference_count(32, 32).unwrap(), 0);
    assert!(info.residency.load_attempted(32, 32).unwrap());
    let tile = info.residency.tile(32, 32).unwrap();
    assert_eq!(
        manager.query_grid(0, [0.0; 3], false).unwrap_err().id,
        "execution.terrain.vmap_tile_load_owner"
    );
    assert_eq!(
        manager.terrain(0).unwrap().residency.tile(32, 32).unwrap(),
        tile
    );
    assert_eq!(manager.zone_and_area(0, [0.0; 3]).unwrap(), [900, 7]);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Raw { .. }))
            .count(),
        2
    );
    assert!(events.iter().any(|event| matches!(
        event.kind,
        EventKind::Rejected {
            begin: 0,
            end: 1,
            reason: benilla_sim::source_rng::Rejection::Lemire32,
            ..
        }
    )));
}
#[test]
fn absent_source_file_has_real_defaults_but_missing_acquisition_and_partial_area_refuse() {
    // GridMap.cpp:43-69,83-89,155-168,1253-1271.
    let mut files = SourceTerrainFiles::default();
    files.retain_source_absence(0, 32, 32);
    let mut manager = SourceTerrainManager::new(files, dbc());
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    assert_eq!(manager.original_zone(0, [0.0; 3], &mut random).unwrap(), 0);
    let grid = manager.terrain(0).unwrap().grid(32, 32).unwrap();
    assert!(grid.bytes.is_none());
    assert!(grid.load_success);
    assert_eq!(grid.flat_height.to_bits(), (-200000.0f32).to_bits());
    assert_eq!(grid.holes, [0; 256]);
    assert_eq!(
        (grid.liquid_entry, grid.liquid_flags, grid.liquid_size),
        (0, 0, [0, 0])
    );
    assert_eq!(grid.liquid_level.to_bits(), (-200000.0f32).to_bits());
    assert_eq!(
        manager
            .original_zone(1, [0.0; 3], &mut random)
            .unwrap_err()
            .id,
        "execution.terrain.map_product_unavailable"
    );
    manager
        .files
        .bytes(1, 32, 32, map_bytes(6, true)[..48].to_vec());
    assert_eq!(
        manager
            .original_zone(1, [0.0; 3], &mut random)
            .unwrap_err()
            .id,
        "execution.terrain.area_partial_read_owner"
    );
    assert_eq!(random.cursor().raw, 2);
    manager.files.bytes(1, 32, 32, vec![0; 40]);
    assert_eq!(manager.original_zone(1, [0.0; 3], &mut random).unwrap(), 0);
    assert!(
        !manager
            .terrain(1)
            .unwrap()
            .grid(32, 32)
            .unwrap()
            .load_success
    );
}
#[test]
fn area_lookup_uses_first_matching_map_last_wrong_map_linked_zero_and_no_parent_lookup() {
    // DBCStores.cpp:676-704. GridMap.cpp:1406-1412 never validates the parent.
    let mut dbc = dbc();
    dbc.areas.insert(
        0,
        SourceAreaEntry {
            id: 0,
            map: 99,
            zone: 123456,
            flag: 0,
        },
    );
    dbc.areas.insert(
        2,
        SourceAreaEntry {
            id: 2,
            map: 0,
            zone: 111,
            flag: 6,
        },
    );
    dbc.areas.insert(
        9,
        SourceAreaEntry {
            id: 9,
            map: 9,
            zone: 0,
            flag: 6,
        },
    );
    dbc.linked_zones.insert(5, 0);
    assert_eq!(dbc.location(0, 6), [111, 2]);
    assert_eq!(dbc.location(1, 6), [8, 8]);
    assert_eq!(dbc.location(3, 6), [9, 9]);
    assert_eq!(dbc.location(5, 0), [123456, 0]);
    assert_eq!(dbc.location(6, 0), [0, 0]);
    assert_eq!(dbc.location(0, 65535), [0, 0]);
    assert_eq!(benilla_native_terrain::grid_area::narrow_flag(65536), 0);
}
#[test]
fn timer_wrap_cleanup_and_reference_history_do_not_force_evict() {
    // Timer.h:105-129, GridMap.cpp:716-724,763-795,1362-1395.
    let mut files = SourceTerrainFiles::default();
    files.bytes(0, 32, 32, map_bytes(6, false));
    let mut manager = SourceTerrainManager::new(files, dbc());
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    manager.original_zone(0, [0.0; 3], &mut random).unwrap();
    let old = manager.query_grid(0, [0.0; 3], true).unwrap();
    manager.update(u32::MAX).unwrap();
    assert_eq!(
        manager.terrain(0).unwrap().residency.cleanup_current(),
        36999
    );
    assert!(manager.terrain(0).unwrap().grid(32, 32).is_some());
    manager.update(180000).unwrap();
    assert_eq!(
        manager.terrain(0).unwrap().residency.cleanup_current(),
        156999
    );
    assert!(manager.terrain(0).unwrap().grid(32, 32).is_none());
    assert!(manager.query_grid(0, [0.0; 3], true).unwrap().generation > old.generation);
    manager.add_ref(0).unwrap();
    manager.unload_terrain(0, true).unwrap();
    assert_eq!(manager.len(), 1);
    assert!(manager.release(0).unwrap());
    manager.unload_terrain(0, false).unwrap();
    assert_eq!(manager.len(), 1);
    manager.unload_terrain(0, true).unwrap();
    assert!(manager.is_empty());
    manager.load_terrain(0, &mut random).unwrap();
    assert_eq!(random.cursor().raw, 2);
    manager.unload_all().unwrap();
    assert!(manager.is_empty());
}
#[test]
fn loaded_vmap_tree_refuses_before_terrain_query_instead_of_reusing_cold_answer() {
    // VMapManager2.cpp:258-269 reaches a tree query only when one exists.
    let mut manager = SourceTerrainManager::default();
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    manager.load_terrain(0, &mut random).unwrap();
    manager.vmap.install_tree(0, 1);
    assert_eq!(
        manager.zone_and_area(0, [0.0; 3]).unwrap_err().id,
        "execution.terrain.live_vmap_area_owner"
    );
    assert!(!manager
        .terrain(0)
        .unwrap()
        .residency
        .load_attempted(32, 32)
        .unwrap());
}

#[test]
fn recorded_world_vmap_initializes_null_slots_without_loading_collision() {
    // IVMapManager.h:53, World.cpp:817-824,912-919, VMapManager2.cpp:54-60.
    let mut manager = SourceVMapManager::default();
    assert!(manager.thread_safe());
    assert_eq!(manager.settings(), [true, true]);
    let missing = b"vmap.enableLOS=1";
    assert_eq!(
        manager
            .initialize_recorded_world([0, 1], missing, &Sha256Id::of(missing))
            .unwrap_err()
            .id,
        "execution.spawn_loader.config_missing.vmap.enableHeight"
    );
    assert!(!manager.created());
    assert_eq!(manager.map_slots(), 0);
    let bytes = b"vmap.enableLOS=0\nvmap.enableHeight=YES\n";
    assert_eq!(
        manager
            .initialize_recorded_world([0, 1], bytes, &Sha256Id::of(b"wrong"))
            .unwrap_err()
            .id,
        "execution.terrain.vmap_config_identity"
    );
    manager
        .initialize_recorded_world([0, 1], bytes, &Sha256Id::of(bytes))
        .unwrap();
    assert!(manager.created());
    assert!(!manager.thread_safe());
    assert_eq!(manager.map_slots(), 2);
    assert_eq!(manager.loaded_trees(), 0);
    assert_eq!(manager.settings(), [false, true]);
}

#[test]
fn acquisition_identity_mismatch_does_not_become_source_absence() {
    let mut files = SourceTerrainFiles::default();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    assert_eq!(
        files
            .retain_present(0, 32, 32, &path, &Sha256Id::of(b"wrong"))
            .unwrap_err()
            .id,
        "execution.terrain.product_identity"
    );
    let mut manager = SourceTerrainManager::new(files, dbc());
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        manager
            .original_zone(0, [0.0; 3], &mut random)
            .unwrap_err()
            .id,
        "execution.terrain.map_product_unavailable"
    );
}

#[test]
fn source_header_and_partial_hole_reads_refuse_but_complete_area_cells_load() {
    // GridMap.cpp:92-94,155-168,217-224,261-270.
    for (offset, refusal) in [
        (16, "execution.terrain.height_header_read_undefined"),
        (24, "execution.terrain.liquid_header_read_undefined"),
        (32, "execution.terrain.holes_partial_read_owner"),
    ] {
        let mut bytes = map_bytes(6, false);
        bytes[offset..offset + 4].copy_from_slice(&48u32.to_le_bytes());
        let mut files = SourceTerrainFiles::default();
        files.bytes(0, 32, 32, bytes);
        let mut manager = SourceTerrainManager::new(files, dbc());
        let mut mt = Mt19937::from_seed(5489);
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
        assert_eq!(
            manager
                .original_zone(0, [0.0; 3], &mut random)
                .unwrap_err()
                .id,
            refusal
        );
        manager.files.bytes(0, 32, 32, map_bytes(6, true));
        manager
            .original_zone(0, [-50.0, -83.333_33, 0.0], &mut random)
            .unwrap();
        assert_eq!(
            manager
                .terrain(0)
                .unwrap()
                .grid(32, 32)
                .unwrap()
                .area(-50.0, -83.333_33)
                .unwrap(),
            18
        );
        assert_eq!(random.cursor().raw, 1);
    }
}

#[test]
fn unload_all_preserves_source_hash_bucket_history_for_later_construction() {
    // GridMap.cpp:1389-1394 clears the unordered map, not its bucket allocation.
    let mut manager = SourceTerrainManager::default();
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    for map in 0..15 {
        manager.load_terrain(map, &mut random).unwrap();
    }
    assert_eq!(manager.bucket_count(), 29);
    manager.unload_all().unwrap();
    assert!(manager.is_empty());
    assert_eq!(manager.bucket_count(), 29);
    manager.load_terrain(0, &mut random).unwrap();
    assert_eq!(manager.bucket_count(), 29);
    assert_eq!(random.cursor().raw, 16);
}
