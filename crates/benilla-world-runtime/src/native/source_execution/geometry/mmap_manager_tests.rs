// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
const PARAMS: &[u8] =
    include_bytes!("../../../../../benilla-native-nav/tools/fixtures/authored.mmap");
const A: &[u8] = include_bytes!("../../../../../benilla-native-nav/tools/fixtures/0003232.mmtile");
fn configured() -> SourceMMapManager {
    let mut manager = SourceMMapManager::default();
    let bytes = b"mmap.enabled = 1\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\n";
    manager
        .initialize_recorded_world(bytes, &Sha256Id::of(bytes))
        .unwrap();
    manager
}
fn mesh(manager: &mut SourceMMapManager) {
    manager.files.map_bytes(0, PARAMS.to_vec());
    assert_eq!(manager.load_map_instance(0, 0).unwrap(), MMapLoad::Loaded);
}
#[test]
fn source_load_false_branches_are_not_native_refusals_and_preserve_state() {
    let mut manager = configured();
    manager.files.source_map_absent(1).unwrap();
    assert_eq!(
        manager.load_map_instance(1, 0).unwrap(),
        MMapLoad::MapAbsent
    );
    assert_eq!(manager.mesh_count(), 0);
    mesh(&mut manager);
    let query = manager.query(0, 0).unwrap();
    manager.files.source_tile_absent(0, 1, 1).unwrap();
    assert_eq!(manager.load_tile(0, 1, 1).unwrap(), MMapLoad::TileAbsent);
    assert!(!manager.is_tile_loaded(0, 1, 1));
    for (x, mut bytes, expected) in [
        (2, A.to_vec(), MMapLoad::BadMagic),
        (3, A.to_vec(), MMapLoad::BadVersion),
        (4, A[..20].to_vec(), MMapLoad::ReadFailed),
    ] {
        if x == 2 {
            bytes[..4].fill(0);
        }
        if x == 3 {
            bytes[8..12].copy_from_slice(&9u32.to_le_bytes());
        }
        manager.files.tile_bytes(0, x, 1, bytes);
        assert_eq!(manager.load_tile(0, x, 1).unwrap(), expected);
    }
    assert_eq!(manager.tile_count(), 0);
    assert!(query.same_query(&manager.query(0, 0).unwrap()));
    manager.files.tile_bytes(0, 32, 32, A.to_vec());
    assert_eq!(manager.load_tile(0, 32, 32).unwrap(), MMapLoad::Loaded);
    assert_eq!(
        manager.load_tile(0, 32, 32).unwrap(),
        MMapLoad::AlreadyLoaded
    );
    assert_eq!(
        manager.mesh(0).unwrap().tile_reference([32, 32]),
        Some(0x0010_0000_0000_0000)
    );
    manager.unload_map(0).unwrap();
    assert_eq!(
        manager.query_capacity(0, &query).unwrap_err().id,
        "execution.mmap.stale_query"
    );
    manager.load_map_instance(0, 0).unwrap();
    assert_eq!(
        manager.query_capacity(0, &query).unwrap_err().id,
        "execution.mmap.stale_query"
    );
}
#[test]
fn missing_expected_products_and_unknown_acquisition_never_become_source_absence() {
    let mut manager = configured();
    assert_eq!(
        manager.load_map_instance(0, 0).unwrap_err().id,
        "execution.mmap.map_product_unavailable"
    );
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("not-an-acquired-mmap-product");
    assert_eq!(
        manager
            .files
            .retain_map(0, &missing, Sha256Id::of(b"x"))
            .unwrap_err()
            .id,
        "execution.mmap.expected_map_product_unavailable"
    );
    assert_eq!(
        manager
            .files
            .retain_tile(0, 1, 1, &missing, Sha256Id::of(b"x"))
            .unwrap_err()
            .id,
        "execution.mmap.expected_tile_product_unavailable"
    );
    mesh(&mut manager);
    assert_eq!(
        manager.load_tile(0, 1, 1).unwrap_err().id,
        "execution.mmap.tile_product_unavailable"
    );
    assert_eq!(manager.tile_count(), 0);
}
#[test]
fn config_source_precedence_and_undefined_atoi_boundary_are_explicit() {
    let mut manager = configured();
    let bytes = b"mmap.enabled = 1\nmmap.ignoreMapIds = \"1,, +2suffix,-1,not-a-number\"\nmmap.preload = 0\n";
    manager
        .initialize_recorded_world(bytes, &Sha256Id::of(bytes))
        .unwrap();
    for map in [0, 1, 2, u32::MAX] {
        assert!(!manager
            .pathfinding_enabled(map, MMapForcing::Default)
            .unwrap());
        assert!(manager
            .pathfinding_enabled(map, MMapForcing::Forced)
            .unwrap());
    }
    assert!(!manager
        .pathfinding_enabled(3, MMapForcing::Ignored)
        .unwrap());
    let bytes = b"mmap.enabled = 0\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\n";
    manager
        .initialize_recorded_world(bytes, &Sha256Id::of(bytes))
        .unwrap();
    assert!(!manager.pathfinding_enabled(3, MMapForcing::Forced).unwrap());
    for (value, valid) in [
        ("2147483647", true),
        ("2147483648", false),
        ("-2147483648", true),
        ("-2147483649", false),
    ] {
        let bytes =
            format!("mmap.enabled = 1\nmmap.ignoreMapIds = \"{value}\"\nmmap.preload = 0\n");
        let result =
            manager.initialize_recorded_world(bytes.as_bytes(), &Sha256Id::of(bytes.as_bytes()));
        if valid {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.mmap.ignore_atoi_overflow"
            );
        }
    }
}
#[test]
fn short_params_and_partial_headers_refuse_but_empty_keeps_constructor_defaults() {
    let mut manager = configured();
    for length in [0, 27] {
        manager.files.map_bytes(0, PARAMS[..length].to_vec());
        assert_eq!(
            manager.load_map_instance(0, 0).unwrap_err().id,
            "execution.mmap.undefined_short_params_read"
        );
    }
    manager.files.map_bytes(0, PARAMS.to_vec());
    manager.load_map_instance(0, 0).unwrap();
    for length in 1..20 {
        manager.files.tile_bytes(0, 32, 32, A[..length].to_vec());
        assert_eq!(
            manager.load_tile(0, 32, 32).unwrap_err().id,
            "execution.mmap.undefined_partial_header_read"
        );
    }
    for length in [0, 20] {
        manager.files.tile_bytes(0, 32, 32, A[..length].to_vec());
        assert_eq!(manager.load_tile(0, 32, 32).unwrap(), MMapLoad::ReadFailed);
    }
}
#[test]
fn map_only_upgrades_in_place_and_source_no_nav_false_still_marks_fully_loaded() {
    let mut manager = SourceTerrainManager::default();
    manager.files.retain_source_absence(0, 32, 32);
    manager
        .vmap
        .collision
        .files
        .observe(b"000.vmtree", benilla_native_vmap::live::FileState::Absent)
        .unwrap();
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    manager.load_terrain(0, &mut random).unwrap();
    let tile = manager.query_grid(0, [0.0; 3], true).unwrap();
    assert!(!manager.mmap.created());
    mesh(&mut manager.mmap);
    manager.mmap.files.source_tile_absent(0, 32, 32).unwrap();
    assert_eq!(manager.query_grid(0, [0.0; 3], false).unwrap(), tile);
    assert!(manager.mmap.created());
    assert!(!manager.mmap.is_tile_loaded(0, 32, 32));
    // A subsequent full lookup must not retry an immutable known source absence.
    assert_eq!(manager.query_grid(0, [0.0; 3], false).unwrap(), tile);
    assert_eq!(manager.mmap.mesh_count(), 1);
    assert_eq!(manager.mmap.tile_count(), 0);
    assert_eq!(random.cursor().raw, 1);
}
#[test]
fn map_startup_navigation_dispatch_uses_concrete_owner_and_ignored_maps_still_load() {
    struct Work(SourceMMapManager);
    impl ManagerStartupWork for Work {
        fn navigation_manager(&mut self) -> Result<&mut SourceMMapManager> {
            Ok(&mut self.0)
        }
    }
    let mut work = Work(configured());
    work.0.files.map_bytes(0, PARAMS.to_vec());
    let map = MapIdentity {
        map: 0,
        instance: 7,
        creation_generation: 1,
    };
    work.initialize_map(map, MapManagerStartupPhase::Navigation)
        .unwrap();
    let query = work.0.query(0, 7).unwrap();
    assert_eq!(work.0.query_capacity(0, &query).unwrap(), 1024);
    work.initialize_map(map, MapManagerStartupPhase::Navigation)
        .unwrap();
    assert!(query.same_query(&work.0.query(0, 7).unwrap()));
    assert!(work.0.unload_instance(0, 7));
    assert_eq!(work.0.mesh_count(), 1);
}
