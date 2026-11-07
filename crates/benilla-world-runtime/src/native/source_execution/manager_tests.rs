// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-mechanism tests, not recorded-execution acceptance.
use super::*;
use benilla_sim::resident_map::CellPair;

#[derive(Default)]
struct Host {
    sql: Vec<PersistentSql>,
    events: Vec<PersistentOperation>,
    fail_sql: bool,
    queries: std::collections::BTreeMap<GridSpawnKind, CharacterQueryStatus>,
}
impl PersistentEffects for Host {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        self.sql.push(operation);
        require(!self.fail_sql, "authored.sql_failure")
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        self.sql.push(PersistentSql::QueryRespawns { kind });
        Ok(self
            .queries
            .get(&kind)
            .copied()
            .unwrap_or(CharacterQueryStatus::Empty))
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.events.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(Refusal {
            id: "authored.reset_owner".into(),
        })
    }
}
fn map(id: u32, map_type: u32) -> PersistentMapEntry {
    PersistentMapEntry { id, map_type }
}
fn world() -> (SourceManagers, PersistentStateIdentity, Host) {
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    let id = managers
        .create_state(map(0, 0), 0, 0, false, true, 0, &mut host)
        .unwrap();
    (managers, id, host)
}
#[test]
fn source_save_changes_memory_then_deletes_and_conditionally_inserts() {
    // MapPersistentStateMgr.cpp:73-155: strict World snapshot comparison, not DB time.
    let (mut managers, id, mut host) = world();
    for (time, expected) in [(99, 0), (100, 0), (101, 101), (0, 0), (-1, 0)] {
        host.sql.clear();
        managers
            .save_respawn(id, GridSpawnKind::Creature, 7, time, 100, &mut host)
            .unwrap();
        assert_eq!(
            managers
                .state(id)
                .unwrap()
                .respawn(GridSpawnKind::Creature, 7),
            expected
        );
        let mut sql = vec![
            PersistentSql::Begin,
            PersistentSql::Delete {
                kind: GridSpawnKind::Creature,
                guid: 7,
                instance: 0,
            },
        ];
        if time == 101 {
            sql.push(PersistentSql::Insert {
                kind: GridSpawnKind::Creature,
                guid: 7,
                deadline: 101,
                instance: 0,
            });
        }
        sql.push(PersistentSql::Commit);
        assert_eq!(host.sql, sql);
        assert!(
            matches!(host.events.last(), Some(PersistentOperation::Deadline { after, .. }) if *after == expected)
        );
    }
    managers
        .save_respawn(id, GridSpawnKind::GameObject, 7, 200, 100, &mut host)
        .unwrap();
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        0
    );
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::GameObject, 7),
        200
    );
}
#[test]
fn sql_failure_keeps_source_memory_and_stops_execution() {
    let (mut managers, id, mut host) = world();
    host.fail_sql = true;
    assert_eq!(
        managers
            .save_respawn(id, GridSpawnKind::Creature, 7, 200, 100, &mut host)
            .unwrap_err()
            .id,
        "authored.sql_failure"
    );
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        200
    );
    assert_eq!(host.sql, [PersistentSql::Begin]);
    assert_eq!(
        managers
            .save_respawn(id, GridSpawnKind::Creature, 8, 200, 100, &mut host)
            .unwrap_err()
            .id,
        "execution.stopped"
    );
}
#[test]
fn world_states_retain_identity_and_pool_state_across_map_detach() {
    // MapPersistentStateMgr.cpp:209-216: a world state never unloads, even empty.
    let (mut managers, id, mut host) = world();
    let first = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    managers.bind_map(id, Some(first), &mut host).unwrap();
    managers
        .state_mut(id)
        .unwrap()
        .pools_mut()
        .add(PoolMember::Creature(7), 8);
    managers.bind_map(id, None, &mut host).unwrap();
    let again = managers
        .create_state(map(0, 0), 0, 0, false, true, 0, &mut host)
        .unwrap();
    assert_eq!(again, id);
    assert!(managers
        .state(again)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    let second = MapIdentity {
        creation_generation: 2,
        ..first
    };
    managers.bind_map(id, Some(second), &mut host).unwrap();
    assert_eq!(managers.state(id).unwrap().map_binding(), Some(second));
    assert_eq!(
        managers.require_durable_checkpoint().unwrap_err().id,
        "execution.managers.f7_continuation_owner"
    );
}
#[test]
fn overlays_use_y_times_1024_plus_x_and_keep_empty_cells_and_typed_guids() {
    // MapPersistentStateMgr.cpp:166-195 and .h:107 use operator[] even on removal/read.
    let (mut managers, id, mut host) = world();
    let cell = CellPair::from_indices(12, 34).unwrap();
    assert_eq!(persistent_cell_key(cell), 34828);
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        managers
            .overlay(id, cell, kind, 7, true, &mut host)
            .unwrap();
        managers
            .overlay(id, cell, kind, 7, true, &mut host)
            .unwrap();
    }
    managers
        .overlay(id, cell, GridSpawnKind::Creature, 7, false, &mut host)
        .unwrap();
    let state = managers.state_mut(id).unwrap();
    assert!(state.cell_guids(cell).creatures.is_empty());
    assert_eq!(
        state
            .cell_guids(cell)
            .gameobjects
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [7]
    );
    let empty = CellPair::from_indices(13, 34).unwrap();
    assert!(state.cell_guids(empty).creatures.is_empty());
    let snapshot = state.inventory();
    assert_eq!(snapshot.cells.len(), 2);
    assert_eq!(snapshot.version, 1);
}
#[test]
fn pool_counts_preserve_duplicates_zero_children_and_guarded_decrements() {
    // PoolManager.cpp:62-107 increments even when set insertion is a duplicate.
    let mut pools = PersistentPoolData::default();
    assert!(!pools.initialized());
    pools.mark_initialized();
    assert!(pools.initialized());
    pools.add(PoolMember::Creature(7), 10);
    pools.add(PoolMember::Creature(7), 10);
    pools.add(PoolMember::GameObject(7), 10);
    pools.add(PoolMember::Pool(20), 10);
    assert_eq!(pools.count(10), 4);
    assert!(pools.contains(PoolMember::Pool(20)));
    assert_eq!(pools.count(20), 0);
    pools.add(PoolMember::Creature(8), 20);
    pools.add(PoolMember::Pool(20), 10);
    assert_eq!(pools.count(20), 0); // source overwrites a child count
    pools.remove(PoolMember::Creature(999), 10);
    assert_eq!(pools.count(10), 4);
    pools.remove(PoolMember::Pool(20), 10);
    assert!(!pools.contains(PoolMember::Pool(20)));
    pools.remove(PoolMember::GameObject(99), 30);
    assert!(pools.contains(PoolMember::Pool(30))); // operator[] retains zero
    assert_eq!(pools.count(30), 0);
    pools.remove(PoolMember::Creature(7), 10);
    assert!(!pools.contains(PoolMember::Creature(7)));
    assert!(pools.contains(PoolMember::GameObject(7)));
}
#[test]
fn battleground_save_mutates_memory_without_sql_and_detach_unloads() {
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    let id = managers
        .create_state(map(30, 3), 9, 0, false, true, 0, &mut host)
        .unwrap();
    managers
        .save_respawn(id, GridSpawnKind::GameObject, 7, 200, 100, &mut host)
        .unwrap();
    assert!(host.sql.is_empty());
    managers.bind_map(id, None, &mut host).unwrap();
    assert_eq!(
        managers.state(id).err().unwrap().id,
        "execution.persistent.stale_state"
    );
    let new = managers
        .create_state(map(30, 3), 9, 0, false, true, 0, &mut host)
        .unwrap();
    assert_ne!(new.generation, id.generation);
}
#[test]
fn dungeon_reset_and_unload_writes_require_their_real_owners() {
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    assert_eq!(
        managers
            .create_state(map(33, 1), 1, 0, true, true, 0, &mut host)
            .unwrap_err()
            .id,
        "authored.reset_owner"
    );
    let mut managers = SourceManagers::default();
    let id = managers
        .create_state(map(33, 1), 1, 500, true, true, 3, &mut host)
        .unwrap();
    let bound = MapIdentity {
        map: 33,
        instance: 1,
        creation_generation: 2,
    };
    managers.bind_map(id, Some(bound), &mut host).unwrap();
    managers.bind_map(id, None, &mut host).unwrap();
    assert_eq!(
        host.sql.last(),
        Some(&PersistentSql::UpdateInstanceReset {
            instance: 1,
            reset: 500
        })
    );
    assert!(managers.state(id).is_err());
}
#[test]
fn stale_or_wrong_map_binding_refuses_before_mutation() {
    let (mut managers, id, mut host) = world();
    let wrong = MapIdentity {
        map: 1,
        instance: 0,
        creation_generation: 1,
    };
    assert_eq!(
        managers
            .bind_map(id, Some(wrong), &mut host)
            .unwrap_err()
            .id,
        "execution.persistent.map_binding"
    );
    assert_eq!(managers.state(id).unwrap().map_binding(), None);
}

#[test]
fn save_after_state_unload_refuses_the_source_dangling_read() {
    // Save:75,78 / Set:142-144 / UnloadIfEmpty:65-68 / removal:723 delete `this`.
    for (entry, instance, reset) in [(map(33, 1), 1, 500), (map(30, 3), 2, 0)] {
        for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
            let mut managers = SourceManagers::default();
            let mut host = Host::default();
            let id = managers
                .create_state(entry, instance, reset, true, true, 0, &mut host)
                .unwrap();
            assert_eq!(
                managers
                    .save_respawn(id, kind, 7, 100, 100, &mut host)
                    .unwrap_err()
                    .id,
                "execution.persistent.save_after_unload_undefined"
            );
            assert!(managers.state(id).is_err());
            assert!(!host.sql.iter().any(|sql| matches!(
                sql,
                PersistentSql::Begin | PersistentSql::Delete { .. } | PersistentSql::Insert { .. }
            )));
            assert!(
                matches!(host.events.last(), Some(PersistentOperation::Unloaded { state }) if *state == id)
            );
        }
    }
}

#[derive(Default)]
struct Definitions {
    maps: Vec<PersistentMapEntry>,
    spawns: std::collections::BTreeMap<(GridSpawnKind, u32), u32>,
}
impl PersistentDefinitions for Definitions {
    fn map_entries(&mut self) -> Result<Vec<PersistentMapEntry>> {
        Ok(self.maps.clone())
    }
    fn spawn_map(&mut self, kind: GridSpawnKind, guid: u32) -> Result<Option<u32>> {
        Ok(self.spawns.get(&(kind, guid)).copied())
    }
}
#[test]
fn startup_projection_preserves_sql_row_order_and_both_clock_thresholds() {
    use super::manager_inputs::tests::{binding, snapshot};
    let raw = snapshot("(1,99,0),(2,100,0),(3,101,0),(4,102,0)", "(4,103,0)", "");
    let mut bound = binding();
    bound.creature.ordinals = vec![3, 2];
    bound.creature.world_seconds = 101;
    bound.gameobject.ordinals = vec![0];
    let inputs = ManagerInitialInputs::authored(&raw, bound).unwrap();
    let mut definitions = Definitions {
        maps: vec![map(1, 0), map(0, 0)],
        ..Definitions::default()
    };
    for guid in 1..=4 {
        definitions
            .spawns
            .insert((GridSpawnKind::Creature, guid), 0);
    }
    definitions.spawns.insert((GridSpawnKind::GameObject, 4), 0);
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    host.queries
        .insert(GridSpawnKind::Creature, CharacterQueryStatus::Rows);
    host.queries
        .insert(GridSpawnKind::GameObject, CharacterQueryStatus::Rows);
    managers
        .initialize_persistent(&inputs, &mut definitions, &mut host)
        .unwrap();
    let id = managers
        .create_state(map(0, 0), 0, 0, false, true, 0, &mut host)
        .unwrap();
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 3),
        0
    );
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 4),
        102
    );
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::GameObject, 4),
        103
    );
    assert_eq!(
        host.sql,
        [
            PersistentSql::Cleanup {
                kind: GridSpawnKind::Creature,
                database_seconds: 100
            },
            PersistentSql::QueryRespawns {
                kind: GridSpawnKind::Creature
            },
            PersistentSql::Cleanup {
                kind: GridSpawnKind::GameObject,
                database_seconds: 100
            },
            PersistentSql::QueryRespawns {
                kind: GridSpawnKind::GameObject
            },
        ]
    );
    let rows: Vec<_> = host
        .events
        .iter()
        .filter_map(|e| {
            if let PersistentOperation::InitialRow { ordinal, .. } = e {
                Some(*ordinal)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(rows, [3, 2, 0]);
    assert_eq!(
        managers
            .initialize_persistent(&inputs, &mut definitions, &mut host)
            .unwrap_err()
            .id,
        "execution.persistent.initialization_repeated"
    );
}
#[test]
fn startup_validates_effective_spawn_map_and_left_join_instance_rows() {
    use super::manager_inputs::tests::{binding, snapshot};
    let raw = snapshot(
        "(1,200,0),(2,200,0),(3,200,8),(4,200,0),(5,200,9),(6,200,8),(7,200,8)",
        "",
        "(8,33,500,3,NULL)",
    );
    let mut bound = binding();
    bound.creature.ordinals = (0..7).collect();
    let inputs = ManagerInitialInputs::authored(&raw, bound).unwrap();
    let mut definitions = Definitions {
        maps: vec![map(0, 0), map(33, 1)],
        ..Definitions::default()
    };
    for (guid, map) in [(2, 999), (3, 0), (4, 33), (5, 33), (6, 34), (7, 33)] {
        definitions
            .spawns
            .insert((GridSpawnKind::Creature, guid), map);
    }
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    host.queries
        .insert(GridSpawnKind::Creature, CharacterQueryStatus::Rows);
    managers
        .initialize_persistent(&inputs, &mut definitions, &mut host)
        .unwrap();
    let dispositions: Vec<_> = host
        .events
        .iter()
        .filter_map(|e| {
            if let PersistentOperation::InitialRow { disposition, .. } = e {
                Some(*disposition)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        dispositions,
        [
            RespawnDisposition::MissingSpawn,
            RespawnDisposition::MissingMap,
            RespawnDisposition::InvalidInstance,
            RespawnDisposition::InstanceableWithoutInstance,
            RespawnDisposition::InvalidInstance,
            RespawnDisposition::MissingMap,
            RespawnDisposition::Loaded,
        ]
    );
    let id = managers
        .create_state(map(33, 1), 8, 0, false, true, 0, &mut host)
        .unwrap();
    let inventory = managers.state(id).unwrap().inventory();
    assert_eq!(inventory.completed_encounters, 3);
    assert_eq!(inventory.reset_time, 500);
    assert!(host
        .sql
        .iter()
        .all(|s| !matches!(s, PersistentSql::CreateInstance { .. })));
}
#[test]
fn query_error_and_empty_both_load_zero_but_keep_distinct_observations() {
    use super::manager_inputs::tests::{binding, snapshot};
    let bound = binding();
    let inputs = ManagerInitialInputs::authored(&snapshot("", "", ""), bound).unwrap();
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    host.queries
        .insert(GridSpawnKind::Creature, CharacterQueryStatus::Error);
    managers
        .initialize_persistent(&inputs, &mut Definitions::default(), &mut host)
        .unwrap();
    let statuses: Vec<_> = host
        .events
        .iter()
        .filter_map(|e| {
            if let PersistentOperation::InitialQuery { status, loaded, .. } = e {
                Some((*status, *loaded))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        statuses,
        [
            (CharacterQueryStatus::Error, 0),
            (CharacterQueryStatus::Empty, 0)
        ]
    );
}
struct GridDefinitionsFixture;
impl GridDefinitions for GridDefinitionsFixture {
    type Row = ();
    fn read(&mut self, _: GridLoadAttempt) -> Result<()> {
        Ok(())
    }
}
impl PersistentGridLookup for GridDefinitionsFixture {
    fn persistent_ordinal(&mut self, _: GridSpawnKind, guid: u32) -> Result<u64> {
        Ok(u64::from(guid) + 100)
    }
}
#[test]
fn grid_adapter_reads_live_overlays_not_copied_persistent_sets() {
    let (mut managers, id, mut host) = world();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    managers.bind_map(id, Some(map), &mut host).unwrap();
    let cell = CellPair::from_indices(12, 34).unwrap();
    managers
        .overlay(id, cell, GridSpawnKind::Creature, 7, true, &mut host)
        .unwrap();
    let mut definitions = PersistentGridDefinitions {
        definitions: GridDefinitionsFixture,
        managers: &mut managers,
        state: id,
    };
    assert!(matches!(
        definitions
            .persistent_next(map, cell, GridSpawnKind::Creature, None)
            .unwrap(),
        PersistentGridLane::Source(Some((7, 107)))
    ));
    definitions
        .managers
        .overlay(id, cell, GridSpawnKind::Creature, 7, false, &mut host)
        .unwrap();
    definitions
        .managers
        .overlay(id, cell, GridSpawnKind::Creature, 8, true, &mut host)
        .unwrap();
    assert!(matches!(
        definitions
            .persistent_next(map, cell, GridSpawnKind::Creature, None)
            .unwrap(),
        PersistentGridLane::Source(Some((8, 108)))
    ));
    assert!(matches!(
        definitions
            .persistent_next(map, cell, GridSpawnKind::GameObject, None)
            .unwrap(),
        PersistentGridLane::Source(None)
    ));
    let wrong = MapIdentity {
        creation_generation: 2,
        ..map
    };
    assert_eq!(
        definitions
            .persistent_next(wrong, cell, GridSpawnKind::Creature, None)
            .err()
            .unwrap()
            .id,
        "execution.persistent.grid_map_binding"
    );
}

#[test]
fn dungeon_bind_lists_keep_state_alive_and_remove_all_matching_owners() {
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    let id = managers
        .create_state(map(33, 1), 1, 500, true, true, 3, &mut host)
        .unwrap();
    assert_eq!(managers.lookup_state(999, 1), Some(id)); // source ignores map for instances
    for kind in [PersistentBindKind::Player, PersistentBindKind::Group] {
        managers.dungeon_bind(id, kind, 7, true, &mut host).unwrap();
        managers.dungeon_bind(id, kind, 7, true, &mut host).unwrap();
    }
    managers.bind_map(id, None, &mut host).unwrap();
    managers
        .dungeon_bind(id, PersistentBindKind::Player, 7, false, &mut host)
        .unwrap();
    assert!(managers
        .state(id)
        .unwrap()
        .inventory()
        .player_binds
        .is_empty());
    assert_eq!(managers.state(id).unwrap().inventory().group_binds, [7, 7]);
    managers
        .dungeon_bind(id, PersistentBindKind::Group, 7, false, &mut host)
        .unwrap();
    assert!(managers.lookup_state(33, 1).is_none());
    let (mut managers, id, mut host) = world();
    assert_eq!(
        managers
            .dungeon_bind(id, PersistentBindKind::Player, 7, true, &mut host)
            .unwrap_err()
            .id,
        "execution.persistent.dungeon_bind_owner"
    );
}
#[test]
fn versioned_inventory_rejects_invalid_foreign_map_keys_and_lifetimes() {
    let (managers, _, _) = world();
    let inventory = managers.inventory();
    inventory.validate().unwrap();
    for change in 0..8 {
        let mut bad = inventory.clone();
        let expected = match change {
            0 => {
                bad.version = 2;
                "execution.persistent.inventory_version"
            }
            1 => {
                bad.states[0].version = 2;
                "execution.persistent.inventory_version"
            }
            2 => {
                bad.states[0].identity.generation = 0;
                "execution.persistent.inventory_identity"
            }
            3 => {
                bad.states[0].used_by_map = Some(MapIdentity {
                    map: 1,
                    instance: 0,
                    creation_generation: 1,
                });
                "execution.persistent.inventory_map"
            }
            4 => {
                bad.states[0]
                    .cells
                    .insert(1024 * 1024, PersistentCellGuids::default());
                "execution.persistent.inventory_cell"
            }
            5 => {
                bad.states[0].player_binds.push(1);
                "execution.persistent.inventory_binds"
            }
            6 => {
                bad.next_generation = 0;
                "execution.persistent.inventory_state"
            }
            _ => {
                bad.states.push(bad.states[0].clone());
                "execution.persistent.inventory_state"
            }
        };
        assert_eq!(bad.validate().unwrap_err().id, expected);
    }
}
#[test]
fn nonloaded_dungeon_and_raid_creation_preserve_the_source_sql_reset_rule() {
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    managers
        .create_state(map(33, 1), 1, 500, true, false, 3, &mut host)
        .unwrap();
    let raid = managers
        .create_state(map(409, 2), 2, 600, true, false, 4, &mut host)
        .unwrap();
    assert_eq!(
        host.sql,
        [
            PersistentSql::CreateInstance {
                instance: 1,
                map: 33,
                reset: 500,
                encounters: 3
            },
            PersistentSql::CreateInstance {
                instance: 2,
                map: 409,
                reset: 0,
                encounters: 4
            },
        ]
    );
    managers.bind_map(raid, None, &mut host).unwrap();
    assert_eq!(host.sql.len(), 2); // Raid reset times are not written at unload.
    let mut managers = SourceManagers::default();
    assert_eq!(
        managers
            .create_state(map(0, 0), 1, 0, false, true, 0, &mut host)
            .unwrap_err()
            .id,
        "execution.persistent.map_domain"
    );
}
#[test]
fn initial_expired_dungeon_state_unloads_but_source_loaded_count_still_increments() {
    use super::manager_inputs::tests::{binding, snapshot};
    let raw = snapshot(
        "(7,101,1)",
        "(8,18446744073709551615,0)",
        "(1,33,500,0,NULL)",
    );
    let mut bound = binding();
    bound.creature.ordinals = vec![0];
    bound.creature.world_seconds = 101;
    bound.gameobject.ordinals = vec![0];
    let inputs = ManagerInitialInputs::authored(&raw, bound).unwrap();
    let mut definitions = Definitions {
        maps: vec![map(0, 0), map(33, 1)],
        ..Definitions::default()
    };
    definitions.spawns.insert((GridSpawnKind::Creature, 7), 33);
    definitions.spawns.insert((GridSpawnKind::GameObject, 8), 0);
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    host.queries
        .insert(GridSpawnKind::Creature, CharacterQueryStatus::Rows);
    host.queries
        .insert(GridSpawnKind::GameObject, CharacterQueryStatus::Rows);
    managers
        .initialize_persistent(&inputs, &mut definitions, &mut host)
        .unwrap();
    assert!(managers.lookup_state(33, 1).is_none());
    let id = managers.lookup_state(0, 0).unwrap();
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::GameObject, 8),
        0
    ); // LP64 time_t conversion is -1.
    assert!(host.events.iter().any(|e| matches!(
        e,
        PersistentOperation::InitialQuery {
            kind: GridSpawnKind::Creature,
            loaded: 1,
            ..
        }
    )));
}

struct ProductsFixture;
impl GridProductOwner for ProductsFixture {
    fn load(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        Ok(true)
    }
}
#[derive(Default)]
struct PersistentFactory {
    attempts: Vec<(GridSpawnOrigin, u32, u64)>,
    cells: usize,
    effects: Host,
}
impl<'a> GridObjectFactory<PersistentGridDefinitions<'a, GridDefinitionsFixture>, ProductsFixture>
    for PersistentFactory
{
    fn load_from_db(
        &mut self,
        attempt: GridLoadAttempt,
        _: (),
        context: &mut GridFactoryContext<
            '_,
            PersistentGridDefinitions<'a, GridDefinitionsFixture>,
            ProductsFixture,
        >,
    ) -> Result<GridCandidateOutcome> {
        self.attempts
            .push((attempt.origin, attempt.guid, attempt.catalog_ordinal));
        let handle = context.objects.construct(1, SourceObjectKind::Creature)?;
        let state = context.definitions.state;
        if context
            .definitions
            .managers
            .state(state)?
            .respawn(attempt.kind, attempt.guid)
            != 0
        {
            context.definitions.managers.save_respawn(
                state,
                attempt.kind,
                attempt.guid,
                0,
                100,
                &mut self.effects,
            )?;
        }
        context.objects.destroy(handle, context.cells)?;
        Ok(GridCandidateOutcome::FailedAndDestroyed(handle))
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<
            '_,
            PersistentGridDefinitions<'a, GridDefinitionsFixture>,
            ProductsFixture,
        >,
    ) -> Result<()> {
        panic!("failed candidate cannot reach viewpoint")
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<
            '_,
            PersistentGridDefinitions<'a, GridDefinitionsFixture>,
            ProductsFixture,
        >,
    ) -> Result<()> {
        panic!("failed candidate cannot reach battleground")
    }
    fn cell_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<
            '_,
            PersistentGridDefinitions<'a, GridDefinitionsFixture>,
            ProductsFixture,
        >,
    ) -> Result<()> {
        self.cells += 1;
        Ok(())
    }
    fn resurrectable_corpses(
        &mut self,
        _: CellPair,
        _: &mut GridFactoryContext<
            '_,
            PersistentGridDefinitions<'a, GridDefinitionsFixture>,
            ProductsFixture,
        >,
    ) -> Result<()> {
        Ok(())
    }
}
#[test]
fn real_grid_loader_uses_bound_live_persistent_lane_instead_of_component_sets() {
    let (mut managers, id, mut host) = world();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    managers.bind_map(id, Some(map), &mut host).unwrap();
    let cell = CellPair::from_indices(512, 512).unwrap();
    managers
        .overlay(id, cell, GridSpawnKind::Creature, 7, true, &mut host)
        .unwrap();
    managers
        .overlay(id, cell, GridSpawnKind::Creature, 3, true, &mut host)
        .unwrap();
    managers
        .save_respawn(id, GridSpawnKind::Creature, 7, 99, 50, &mut host)
        .unwrap();
    let mut definitions = PersistentGridDefinitions {
        definitions: GridDefinitionsFixture,
        managers: &mut managers,
        state: id,
    };
    let mut cells = SourceCells::new(map).unwrap();
    cells.configure_grids(300000, true).unwrap();
    let mut sets = GridSpawnSets::new(map);
    sets.register(
        cell,
        GridSpawnKind::Creature,
        GridSpawnOrigin::Static,
        2,
        102,
    )
    .unwrap();
    sets.register(
        cell,
        GridSpawnKind::Creature,
        GridSpawnOrigin::Persistent,
        99,
        199,
    )
    .unwrap();
    let mut factory = PersistentFactory::default();
    let mut products = ProductsFixture;
    let mut objects = SourceObjects::default();
    let mut loader = SourceGridLoader {
        definitions: &mut definitions,
        factory: &mut factory,
        products: &mut products,
        sets: &mut sets,
        objects: &mut objects,
    };
    cells
        .trigger_grid_load(cell, GridLoadTrigger::Enter, &mut loader)
        .unwrap();
    assert_eq!(
        factory.attempts,
        [
            (GridSpawnOrigin::Static, 2, 102),
            (GridSpawnOrigin::Persistent, 3, 103),
            (GridSpawnOrigin::Persistent, 7, 107),
        ]
    );
    assert_eq!(factory.cells, 256);
    assert_eq!(
        definitions
            .managers
            .state(id)
            .unwrap()
            .inventory()
            .cells
            .len(),
        256
    ); // source GetCellObjectGuids operator[]
    assert_eq!(
        definitions
            .managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        0
    );
    assert_eq!(
        factory.effects.sql,
        [
            PersistentSql::Begin,
            PersistentSql::Delete {
                kind: GridSpawnKind::Creature,
                guid: 7,
                instance: 0
            },
            PersistentSql::Commit,
        ]
    );
}

#[test]
fn existing_instance_lookup_precedes_new_state_checks_and_reset_callbacks() {
    // MapPersistentStateMgr.cpp:640-641 returns the instance-keyed state first.
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    let id = managers
        .create_state(map(33, 1), 7, 500, true, true, 3, &mut host)
        .unwrap();
    assert_eq!(
        managers
            .create_state(map(999, 0), 7, 0, false, false, 0, &mut host)
            .unwrap(),
        id
    );
    assert_eq!(managers.state(id).unwrap().inventory().reset_time, 500);
}
#[test]
fn world_pool_state_is_per_persistent_state_despite_the_shared_pool_comment() {
    // MapPersistentStateMgr.h:137 and PoolManager.h:42-45, not the old shared-state comment.
    let (mut managers, first, mut host) = world();
    let second = managers
        .create_state(map(1, 0), 0, 0, false, true, 0, &mut host)
        .unwrap();
    let pools = managers.state_mut(first).unwrap().pools_mut();
    pools.mark_initialized();
    pools.add(PoolMember::Creature(7), 10);
    assert!(!managers.state(second).unwrap().pools().initialized());
    assert!(!managers
        .state(second)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    assert_eq!(managers.state(second).unwrap().pools().count(10), 0);
}

#[test]
fn left_join_missing_instance_uses_source_null_to_zero_conversion_not_inner_join() {
    // MapPersistentStateMgr.cpp:1016-1032 compares converted map=0, not join presence.
    use super::manager_inputs::tests::{binding, snapshot};
    let raw = snapshot("(7,200,42)", "", "");
    let mut bound = binding();
    bound.creature.ordinals = vec![0];
    let inputs = ManagerInitialInputs::authored(&raw, bound).unwrap();
    // Authored null-conversion operands, not a recorded Map.dbc definition.
    let mut definitions = Definitions {
        maps: vec![map(0, 3)],
        ..Definitions::default()
    };
    definitions.spawns.insert((GridSpawnKind::Creature, 7), 0);
    let mut managers = SourceManagers::default();
    let mut host = Host::default();
    host.queries
        .insert(GridSpawnKind::Creature, CharacterQueryStatus::Rows);
    managers
        .initialize_persistent(&inputs, &mut definitions, &mut host)
        .unwrap();
    let id = managers.lookup_state(0, 42).unwrap();
    assert_eq!(
        managers
            .state(id)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        200
    );
}
