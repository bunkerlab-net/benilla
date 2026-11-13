// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937};

fn config(visibility: &str, aggro: &str, expiry: i32) -> Vec<u8> {
    format!("Visibility.Distance.Continents = {visibility}\nRate.Creature.Aggro = {aggro}\nGridCleanUpDelay = {expiry}\nActivateWeather = yes\nChangeWeatherInterval = 600000\n").into_bytes()
}
fn entries() -> [PersistentMapEntry; 2] {
    [
        PersistentMapEntry { id: 0, map_type: 0 },
        PersistentMapEntry { id: 1, map_type: 0 },
    ]
}
fn construct(terrain: &mut SourceTerrainManager, maximum: GuidMaximum) -> (SourceContinents, u64) {
    let mut mt = Mt19937::from_seed(7);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    let maps = SourceContinents::construct(
        entries(),
        maximum,
        terrain,
        &ContinentConfig::parse(&config("120", "1", 300000)).unwrap(),
        &mut random,
    )
    .unwrap();
    (maps, cursor.raw)
}

#[test]
fn constructors_create_both_world_maps_and_reuse_existing_terrain_without_draws() {
    let mut terrain = SourceTerrainManager::default();
    let (maps, words) = construct(&mut terrain, GuidMaximum::Value(17));
    assert_eq!(words, 2); // GridMap.cpp:708, one constructor draw per new TerrainInfo.
    assert_eq!(maps.max_instance_id, 17);
    assert_eq!(
        maps.grid_states,
        [
            NGridPhase::Invalid,
            NGridPhase::Active,
            NGridPhase::Idle,
            NGridPhase::Removal
        ]
    );
    for (id, map) in maps.maps.iter().enumerate() {
        assert_eq!(
            map.identity,
            MapIdentity {
                map: id as u32,
                instance: 0,
                creation_generation: 1
            }
        );
        assert_eq!(map.entry, entries()[id]);
        assert_eq!(map.grid_expiry, 300000);
        assert_eq!(map.unload_timer, 0);
        assert_eq!(map.visibility, 100.0);
        assert_eq!(map.script_id, 0);
        assert!(map.persistent.is_none() && map.guids.is_none());
        assert!(map.variables.is_empty() && !map.spawn_updated);
        assert!(map.weather.inventory().zones_in_source_order.is_empty());
        assert_eq!(terrain.terrain(id as u32).unwrap().references(), 0);
        assert!(!map.prelude_complete());
        assert_eq!(
            map.grid_loaded(0, 0).unwrap_err().id,
            "execution.continents.grids_uninitialized"
        );
    }
    let (reused, words) = construct(&mut terrain, GuidMaximum::NoResult);
    assert_eq!(words, 0);
    assert_eq!(reused.max_instance_id, 0);
    assert_eq!(
        construct(&mut terrain, GuidMaximum::Value(0))
            .0
            .max_instance_id,
        0
    );
}

#[test]
fn independent_preludes_accept_either_order_and_seed_counters_before_addref() {
    let cfg = ContinentConfig::parse(&config("120", "1", 300000)).unwrap();
    for order in [[0, 1], [1, 0]] {
        let mut terrain = SourceTerrainManager::default();
        let (mut maps, _) = construct(&mut terrain, GuidMaximum::NoResult);
        for index in order {
            maps.maps[index]
                .initialize_prelude((90001, 70001), &mut terrain, &cfg)
                .unwrap();
        }
        for map in &mut maps.maps {
            assert!(map.prelude_complete());
            assert_eq!(map.visibility.to_bits(), 120.0f32.to_bits());
            assert_eq!(terrain.terrain(map.identity.map).unwrap().references(), 1);
            for y in 0..64 {
                for x in 0..64 {
                    assert!(!map.grid_loaded(x, y).unwrap());
                    assert!(map.cells.grids.identity(x, y).is_none());
                }
            }
            let guids = map.guids.as_mut().unwrap();
            for (high, counter) in [
                (HighGuid::Creature, 90001),
                (HighGuid::GameObject, 70001),
                (HighGuid::DynamicObject, 1),
                (HighGuid::Pet, 1),
            ] {
                assert_eq!(guids.allocate(map.identity, high).unwrap().counter, counter);
            }
            assert_eq!(
                map.grid_loaded(64, 0).unwrap_err().id,
                "execution.grid_coordinates"
            );
            assert_eq!(
                map.initialize_prelude((1, 1), &mut terrain, &cfg)
                    .unwrap_err()
                    .id,
                "execution.continents.prelude_repeated"
            );
            assert_eq!(terrain.terrain(map.identity.map).unwrap().references(), 1);
        }
    }
}

#[test]
fn instance_data_refuses_only_the_reached_script_call_after_both_preludes() {
    let mut terrain = SourceTerrainManager::default();
    let (mut maps, _) = construct(&mut terrain, GuidMaximum::NoResult);
    let cfg = ContinentConfig::parse(&config("120", "1", 300000)).unwrap();
    assert_eq!(
        maps.maps[0].create_instance_data(9).unwrap_err().id,
        "execution.continents.prelude_missing"
    );
    for map in &mut maps.maps {
        map.initialize_prelude((10, 20), &mut terrain, &cfg)
            .unwrap();
    }
    for map in &mut maps.maps {
        map.create_instance_data(0).unwrap(); // Map.cpp:1676-1677 returns without F5.
        assert_eq!(
            map.create_instance_data(9).unwrap_err(),
            MissingManagerOwner::F5InstanceInitialization.refusal()
        );
        assert_eq!(map.script_id, 9);
        assert!(map.persistent.is_none() && map.variables.is_empty());
    }
}

#[test]
fn visibility_preserves_source_if_else_bounds_including_oversized_lower_bound() {
    for (distance, aggro, expected) in [
        ("10", "1", 45.0),
        ("100", "1", 100.0),
        ("900", "1", 533.333_3),
        ("10", "20", 900.0),
        ("10", "-1", 45.0),
        ("45", "1", 45.0),
    ] {
        assert_eq!(
            ContinentConfig::parse(&config(distance, aggro, 1))
                .unwrap()
                .visibility,
            expected
        );
    }
    assert_eq!(
        ContinentConfig::parse(&config("100", "1", 1))
            .unwrap()
            .grid_expiry,
        60000
    );
    assert_eq!(
        ContinentConfig::parse(&config("100", "1", -1))
            .unwrap()
            .grid_expiry,
        300000
    );
    for (raw, error) in [
        (config("bad", "1", 1), "execution.continents.config_float"),
        (
            config("100", "1", 1)
                .into_iter()
                .chain(b"GridCleanUpDelay = bad\n".iter().copied())
                .collect(),
            "execution.continents.config_integer",
        ),
    ] {
        assert_eq!(ContinentConfig::parse(&raw).err().unwrap().id, error);
    }
}

#[test]
fn wrong_map_entries_refuse_before_construction_or_at_the_exact_second_constructor() {
    let cfg = ContinentConfig::parse(&config("100", "1", 300000)).unwrap();
    let mut terrain = SourceTerrainManager::default();
    let mut mt = Mt19937::from_seed(7);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    let mut bad = entries();
    bad[0].id = 2;
    assert_eq!(
        SourceContinents::construct(bad, GuidMaximum::NoResult, &mut terrain, &cfg, &mut random)
            .err()
            .unwrap()
            .id,
        "execution.continents.map_entries"
    );
    assert!(terrain.is_empty());
    bad = entries();
    bad[1].map_type = 1;
    assert_eq!(
        SourceContinents::construct(bad, GuidMaximum::NoResult, &mut terrain, &cfg, &mut random)
            .err()
            .unwrap()
            .id,
        "execution.continents.world_map_entry"
    );
    assert_eq!(terrain.len(), 1);
    assert_eq!(random.cursor().raw, 1);
}

#[test]
fn missing_terrain_refuses_at_addref_after_guid_grid_and_visibility_steps() {
    let mut terrain = SourceTerrainManager::default();
    let (mut maps, _) = construct(&mut terrain, GuidMaximum::NoResult);
    terrain.unload_all().unwrap();
    let cfg = ContinentConfig::parse(&config("120", "1", 300000)).unwrap();
    let map = &mut maps.maps[0];
    assert_eq!(
        map.initialize_prelude((81, 92), &mut terrain, &cfg)
            .unwrap_err()
            .id,
        "execution.terrain.info_owner"
    );
    assert!(!map.prelude_complete());
    assert!(!map.grid_loaded(63, 63).unwrap());
    assert_eq!(map.visibility, 120.0);
    assert_eq!(
        map.guids
            .as_mut()
            .unwrap()
            .allocate(map.identity, HighGuid::Creature)
            .unwrap()
            .counter,
        81
    );
    assert_eq!(
        map.create_instance_data(9).unwrap_err().id,
        "execution.continents.prelude_missing"
    );
    assert_eq!(map.dynamic_tree_rebalance_ms, 200);
    assert_eq!(map.dynamic_tree_unbalanced, 0);
    // Cold dynamic-tree fields provide no collision query or clear-line-of-sight result.
}
