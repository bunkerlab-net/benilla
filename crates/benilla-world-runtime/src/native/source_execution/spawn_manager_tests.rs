// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-mechanism tests, not recorded-execution acceptance.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
fn key(guid: u32) -> SpawnKey {
    SpawnKey {
        kind: GridSpawnKind::Creature,
        guid,
    }
}
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
#[derive(Default)]
struct Work {
    wall: i64,
    clock: i64,
    rosters: BTreeMap<GridSpawnKind, Vec<u32>>,
    definitions: BTreeMap<SpawnKey, DynamicSpawnDefinition>,
    group_definitions: BTreeMap<u32, DynamicSpawnGroup>,
    attempts: Vec<DynamicSpawnRequest>,
    failures: BTreeSet<SpawnKey>,
    group_updates: Vec<u32>,
    operations: Vec<SpawnOperation>,
    action: Option<Action>,
    blocked: BTreeSet<u32>,
    wall_kind: Option<ClockKind>,
    pending: Option<SpawnPending>,
    group_add: Option<u32>,
    phases: Vec<MapPreludePhase>,
    message_add: Option<u32>,
    wall_calls: Vec<SpawnClockCall>,
    invalid_group: bool,
    fail_sql: bool,
    fail_observation: bool,
}
enum Action {
    Add(u32),
    Respawn(u32, u32),
    Remove(u32),
    Update,
    RespawnAll,
    BindCapacity,
    NestedFailureThenRespawn(u32),
    NestedFailureThenAdd(u32),
    NestedFailureThenUpdate,
    NestedFailureThenAddThenUpdate(u32),
}
impl PersistentEffects for Work {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        if self.fail_sql {
            Err(Refusal {
                id: "authored.sql_failure".into(),
            })
        } else {
            Ok(())
        }
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(Refusal {
            id: "authored.query_unavailable".into(),
        })
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(Refusal {
            id: "authored.reset_unavailable".into(),
        })
    }
}
impl SpawnWork for Work {
    fn thread(&self) -> u64 {
        1
    }
    fn wall_seconds(&mut self, call: SpawnClockCall) -> Result<ClockSample> {
        self.wall_calls.push(call);
        Ok(ClockSample::authored(
            self.wall_kind.unwrap_or(ClockKind::WallSeconds),
            self.wall,
        ))
    }
    fn world_clock_millis(&mut self) -> Result<i64> {
        Ok(self.clock)
    }
    fn world_game_seconds(&mut self) -> Result<i64> {
        Ok(100)
    }
    fn dynamic_guids(&mut self, _: u32, kind: GridSpawnKind) -> Result<Arc<[u32]>> {
        Ok(self
            .rosters
            .get(&kind)
            .map_or_else(|| Arc::from([]), |guids| Arc::from(guids.as_slice())))
    }
    fn definition(&mut self, key: SpawnKey) -> Result<Option<DynamicSpawnDefinition>> {
        Ok(self.definitions.get(&key).copied())
    }
    fn linking_can_spawn(&mut self, guid: u32) -> Result<bool> {
        Ok(!self.blocked.contains(&guid))
    }
    fn groups(&mut self) -> Result<Arc<BTreeMap<u32, DynamicSpawnGroup>>> {
        Ok(Arc::new(self.group_definitions.clone()))
    }
    fn respawn_group_in_vicinity(
        &mut self,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
        group: SpawnGroupHandle,
        _: [u32; 4],
        _: u32,
    ) -> Result<()> {
        self.group_updates.push(group.id);
        Ok(())
    }
    fn create_group(&mut self, group: DynamicSpawnGroup) -> Result<SpawnGroupHandle> {
        Ok(SpawnGroupHandle {
            id: group.id,
            generation: u64::from(!self.invalid_group),
        })
    }
    fn update_group(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        group: SpawnGroupHandle,
    ) -> Result<()> {
        self.group_updates.push(group.id);
        if let Some(guid) = self.group_add.take() {
            manager.add(persistent, key(guid), self)?;
        }
        Ok(())
    }
    fn construct(
        &mut self,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        request: DynamicSpawnRequest,
    ) -> Result<bool> {
        self.attempts.push(request);
        if let Some(pending) = self.pending {
            return Err(pending.refusal());
        }
        if let Some(action) = self.action.take() {
            match action {
                Action::Add(guid) => manager.add(persistent, key(guid), self)?,
                Action::Respawn(guid, delay) => {
                    manager.respawn(persistent, key(guid), delay, self)?
                }
                Action::Remove(guid) => manager.remove_spawn(key(guid), self)?,
                Action::Update => manager.update(persistent, self)?,
                Action::NestedFailureThenRespawn(guid) => {
                    manager.respawn_all(persistent, self)?;
                    manager.respawn(persistent, key(guid), 0, self)?;
                }
                Action::NestedFailureThenAdd(guid) => {
                    manager.respawn_all(persistent, self)?;
                    manager.add(persistent, key(guid), self)?;
                }
                Action::NestedFailureThenUpdate => {
                    manager.respawn_all(persistent, self)?;
                    manager.update(persistent, self)?;
                }
                Action::NestedFailureThenAddThenUpdate(guid) => {
                    manager.respawn_all(persistent, self)?;
                    manager.add(persistent, key(guid), self)?;
                    manager.update(persistent, self)?;
                }
                Action::RespawnAll => manager.respawn_all(persistent, self)?,
                Action::BindCapacity => manager.bind_active_capacity(10)?,
            }
        }
        Ok(!self.failures.contains(&request.key))
    }
    fn spawn_operation(&mut self, _: MapIdentity, operation: SpawnOperation) -> Result<()> {
        if self.fail_observation {
            return Err(Refusal {
                id: "authored.observation_failure".into(),
            });
        }
        self.operations.push(operation);
        Ok(())
    }
}
fn world() -> (SourceSpawnManager, SourceManagers, Work) {
    let mut work = Work {
        wall: 100,
        clock: 100_000,
        ..Work::default()
    };
    let mut persistent = SourceManagers::default();
    let id = persistent
        .create_state(
            PersistentMapEntry { id: 0, map_type: 0 },
            0,
            0,
            false,
            true,
            0,
            &mut work,
        )
        .unwrap();
    persistent.bind_map(id, Some(map()), &mut work).unwrap();
    (
        SourceSpawnManager::new(map(), id).unwrap(),
        persistent,
        work,
    )
}
fn definition() -> DynamicSpawnDefinition {
    DynamicSpawnDefinition {
        map: 0,
        cell: CellPair::from_indices(12, 34).unwrap(),
        spawn_mask: 1,
    }
}
#[test]
fn initial_strict_less_than_differs_from_update_inclusive_due() {
    // SpawnManager.cpp:58-65 versus :269-277.
    let (mut manager, mut persistent, mut work) = world();
    work.rosters.insert(GridSpawnKind::Creature, vec![7, 8, 9]);
    for (guid, deadline) in [(7, 99), (8, 100), (9, 101)] {
        work.definitions.insert(key(guid), definition());
        persistent
            .save_respawn(
                manager.persistent(),
                GridSpawnKind::Creature,
                guid,
                deadline,
                0,
                &mut work,
            )
            .unwrap();
    }
    manager.initialize(&mut persistent, &mut work).unwrap();
    assert_eq!(
        manager
            .inventory()
            .active
            .iter()
            .map(|s| s.key.guid)
            .collect::<Vec<_>>(),
        vec![8, 9]
    );
    assert!(persistent
        .state_mut(manager.persistent())
        .unwrap()
        .cell_guids(definition().cell)
        .creatures
        .contains(&7));
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(
        work.attempts.iter().map(|r| r.key.guid).collect::<Vec<_>>(),
        vec![8]
    );
    assert_eq!(work.attempts[0].requested_guid(), 0);
    assert_eq!(manager.inventory().active[0].key.guid, 9);
}

impl SourceMapPreludeWork for Work {
    fn map_prelude_phase(
        &mut self,
        phase: MapPreludePhase,
        manager: &mut SourceSpawnManager,
        persistent: &mut SourceManagers,
        _: u32,
    ) -> Result<()> {
        self.phases.push(phase);
        if phase == MapPreludePhase::Messages {
            if let Some(guid) = self.message_add.take() {
                manager.add(persistent, key(guid), self)?;
            }
        }
        if phase == MapPreludePhase::Transports {
            assert!(!self.attempts.is_empty());
        }
        Ok(())
    }
}
fn ready() -> (SourceSpawnManager, SourceManagers, Work) {
    let (mut manager, mut persistent, mut work) = world();
    manager.initialize(&mut persistent, &mut work).unwrap();
    (manager, persistent, work)
}
fn queue(
    manager: &mut SourceSpawnManager,
    persistent: &mut SourceManagers,
    work: &mut Work,
    key: SpawnKey,
    deadline: i64,
) {
    persistent
        .save_respawn(manager.persistent(), key.kind, key.guid, deadline, 0, work)
        .unwrap();
    manager.add(persistent, key, work).unwrap();
}
#[test]
fn due_entries_keep_insertion_order_and_failures_retry_without_becoming_used() {
    // SpawnManager.cpp:260-277 does not use operator< at :25-28.
    let (mut manager, mut persistent, mut work) = ready();
    for (guid, deadline) in [(7, 90), (8, 70), (9, 80)] {
        queue(
            &mut manager,
            &mut persistent,
            &mut work,
            key(guid),
            deadline,
        );
    }
    work.failures.insert(key(8));
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(
        work.attempts.iter().map(|r| r.key.guid).collect::<Vec<_>>(),
        vec![7, 8, 9]
    );
    assert_eq!(manager.inventory().active.len(), 1);
    assert!(!manager.inventory().active[0].used);
    assert!(!manager.inventory().active[0].in_use);
    manager.update(&mut persistent, &mut work).unwrap();
    work.failures.clear();
    manager.update(&mut persistent, &mut work).unwrap();
    assert!(manager.inventory().active.is_empty());
    assert_eq!(
        work.attempts.iter().map(|r| r.key.guid).collect::<Vec<_>>(),
        vec![7, 8, 9, 8, 8]
    );
}
#[test]
fn update_construction_additions_are_deferred_and_removal_does_not_cancel_them() {
    // SpawnManager.cpp:115-118,124-127,182-210,262-278.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    work.action = Some(Action::Add(8));
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(work.attempts.len(), 1);
    assert!(manager.inventory().active.is_empty());
    assert_eq!(manager.inventory().deferred[0].key, key(8));
    manager.remove_spawn(key(8), &mut work).unwrap();
    manager.remove_spawns(&[8], &[8], &mut work).unwrap();
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(work.attempts[1].key, key(8));
    assert!(manager.inventory().deferred.is_empty());
}
#[test]
fn recursive_respawn_skips_in_use_entry_instead_of_reentering_it() {
    // SpawnManager.h:40 and .cpp:137-153.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    work.action = Some(Action::Respawn(7, 0));
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(work.attempts.len(), 1);
    assert_eq!(manager.inventory().deferred[0].key, key(7));
    assert!(!manager.inventory().deferred[0].in_use);
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(work.attempts.len(), 2);
}
#[test]
fn zero_delay_found_constructs_immediately_but_not_found_only_schedules() {
    // SpawnManager.cpp:130-180. Found-zero retains its previous timepoint.
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        let (mut manager, mut persistent, mut work) = ready();
        let found = SpawnKey { kind, guid: 7 };
        let missing = SpawnKey { kind, guid: 8 };
        queue(&mut manager, &mut persistent, &mut work, found, 500);
        manager
            .respawn(&mut persistent, found, 0, &mut work)
            .unwrap();
        assert_eq!(work.attempts.len(), 1);
        assert!(manager.inventory().active[0].used);
        assert_eq!(manager.inventory().active[0].deadline_millis, 500_000);
        manager
            .respawn(&mut persistent, missing, 0, &mut work)
            .unwrap();
        assert_eq!(work.attempts.len(), 1);
        assert_eq!(manager.inventory().active[1].deadline_millis, 0);
        manager.update(&mut persistent, &mut work).unwrap();
        assert_eq!(work.attempts[1].key, missing);
    }
}
#[test]
fn positive_respawn_uses_distinct_wall_save_and_world_timepoint_clocks() {
    // SpawnManager.cpp:140-142,166-168; Map.cpp:83-85.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 200);
    queue(&mut manager, &mut persistent, &mut work, key(7), 300);
    manager.remove_spawn(key(7), &mut work).unwrap();
    work.wall = 110;
    work.clock = 120_555;
    manager
        .respawn(&mut persistent, key(7), 3, &mut work)
        .unwrap();
    assert_eq!(
        persistent
            .state(manager.persistent())
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        113
    );
    assert_eq!(manager.inventory().active[0].deadline_millis, 200_000);
    assert_eq!(manager.inventory().active[1].deadline_millis, 123_555);
    assert_eq!(
        work.wall_calls,
        vec![SpawnClockCall::Initialize, SpawnClockCall::Respawn(key(7))]
    );
}
#[test]
fn used_entries_erase_before_due_and_single_removal_only_marks_first_duplicate() {
    // SpawnManager.cpp:182-210,273-275.
    let (mut manager, mut persistent, mut work) = ready();
    let go = SpawnKey {
        kind: GridSpawnKind::GameObject,
        guid: 7,
    };
    for key in [key(7), key(7), go] {
        queue(&mut manager, &mut persistent, &mut work, key, 500);
    }
    manager.remove_spawn(key(7), &mut work).unwrap();
    assert!(manager.inventory().active[0].used);
    assert!(!manager.inventory().active[1].used);
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(manager.inventory().active.len(), 2);
    assert!(work.attempts.is_empty());
    manager.remove_spawns(&[7], &[], &mut work).unwrap();
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(manager.inventory().active[0].key, go);
    manager.remove_spawns(&[], &[7], &mut work).unwrap();
    manager.update(&mut persistent, &mut work).unwrap();
    assert!(manager.inventory().active.is_empty());
    assert!(work.attempts.is_empty());
}
#[test]
fn failed_constructor_marked_used_during_callback_survives_until_next_update() {
    // SpawnManager.cpp:273-277 checks IsUsed only before construction.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    work.failures.insert(key(7));
    work.action = Some(Action::Remove(7));
    manager.update(&mut persistent, &mut work).unwrap();
    assert!(manager.inventory().active[0].used);
    manager.update(&mut persistent, &mut work).unwrap();
    assert!(manager.inventory().active.is_empty());
    assert_eq!(work.attempts.len(), 1);
}
#[test]
fn immediate_recursive_add_requires_bound_sufficient_source_vector_capacity() {
    // Undefined reference invalidation: SpawnManager.cpp:30-40,112-127,152-153.
    for capacity in [None, Some(1), Some(2)] {
        let (mut manager, mut persistent, mut work) = ready();
        queue(&mut manager, &mut persistent, &mut work, key(7), 500);
        if let Some(capacity) = capacity {
            manager.bind_active_capacity(capacity).unwrap();
        }
        work.action = Some(Action::Add(8));
        let result = manager.respawn(&mut persistent, key(7), 0, &mut work);
        if capacity == Some(2) {
            result.unwrap();
            assert_eq!(manager.inventory().active.len(), 2);
            assert_eq!(manager.inventory().active[1].key, key(8));
            manager.update(&mut persistent, &mut work).unwrap();
            assert_eq!(work.attempts.len(), 2);
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spawn.active_vector_lifetime_unbound"
            );
            assert!(manager.inventory().active[0].in_use);
            assert_eq!(
                manager.add(&persistent, key(9), &mut work).unwrap_err().id,
                "execution.spawn.stopped"
            );
        }
    }
}
#[test]
fn event_guid_sets_deduplicate_with_separate_domains_and_ignore_other_highs() {
    // SpawnManager.cpp:213-241.
    let (mut manager, _, mut work) = world();
    for high in [HighGuid::Creature, HighGuid::GameObject] {
        manager.set_event_guid(7, high, true, &mut work).unwrap();
        manager.set_event_guid(7, high, true, &mut work).unwrap();
        assert!(manager.event_guid(7, high));
    }
    manager
        .set_event_guid(7, HighGuid::Player, true, &mut work)
        .unwrap();
    assert!(!manager.event_guid(7, HighGuid::Player));
    manager
        .set_event_guid(7, HighGuid::Creature, false, &mut work)
        .unwrap();
    manager
        .set_event_guid(7, HighGuid::Creature, false, &mut work)
        .unwrap();
    assert!(!manager.event_guid(7, HighGuid::Creature));
    assert!(manager.event_guid(7, HighGuid::GameObject));
    let changes: Vec<_> = work
        .operations
        .iter()
        .filter_map(|operation| {
            if let SpawnOperation::EventGuid { changed, .. } = operation {
                Some(*changed)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(changes, vec![true, false, true, false, true, false]);
}
#[test]
fn initial_linking_and_go_mask_gates_do_not_schedule_rejected_guids() {
    // SpawnManager.cpp:58-80.
    let (mut manager, mut persistent, mut work) = world();
    work.rosters.insert(GridSpawnKind::Creature, vec![7, 8]);
    work.rosters.insert(GridSpawnKind::GameObject, vec![7, 8]);
    work.blocked.insert(8);
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        for guid in [7, 8] {
            work.definitions.insert(
                SpawnKey { kind, guid },
                DynamicSpawnDefinition {
                    spawn_mask: u8::from(guid == 8),
                    ..definition()
                },
            );
        }
    }
    manager.initialize(&mut persistent, &mut work).unwrap();
    let state = persistent.state_mut(manager.persistent()).unwrap();
    assert_eq!(
        state
            .cell_guids(definition().cell)
            .creatures
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![7]
    );
    assert_eq!(
        state
            .cell_guids(definition().cell)
            .gameobjects
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![8]
    );
    assert!(manager.inventory().active.is_empty());
    assert!(work.attempts.is_empty());
}
#[test]
fn group_map_order_filters_and_first_string_alias_are_literal() {
    // SpawnManager.cpp:84-109,279-283. Group additions use the active lane.
    let (mut manager, mut persistent, mut work) = world();
    work.definitions.insert(key(7), definition());
    work.definitions.insert(
        key(8),
        DynamicSpawnDefinition {
            map: 1,
            ..definition()
        },
    );
    for (id, first_guid) in [(20, Some(7)), (10, Some(7)), (30, None), (40, Some(8))] {
        work.group_definitions.insert(
            id,
            DynamicSpawnGroup {
                id,
                first_guid,
                kind: GridSpawnKind::Creature,
                string_id: 5,
            },
        );
    }
    manager.initialize(&mut persistent, &mut work).unwrap();
    assert_eq!(manager.string_group(5).unwrap().id, 10);
    assert!(manager.group(30).is_none());
    assert!(manager.group(40).is_none());
    assert!(manager.string_group(999).is_none());
    work.group_add = Some(9);
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(work.group_updates, vec![10, 20]);
    assert_eq!(manager.inventory().active[0].key, key(9));
    assert!(manager.inventory().deferred.is_empty());
    assert!(work.attempts.is_empty());
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(work.attempts[0].key, key(9));
}
#[test]
fn respawn_all_ignores_used_and_future_time_and_only_erases_successes() {
    // SpawnManager.cpp:244-257.
    let (mut manager, mut persistent, mut work) = ready();
    for guid in [7, 8] {
        queue(&mut manager, &mut persistent, &mut work, key(guid), 500);
    }
    manager.remove_spawn(key(7), &mut work).unwrap();
    work.failures.insert(key(7));
    manager.respawn_all(&mut persistent, &mut work).unwrap();
    assert_eq!(
        work.attempts.iter().map(|r| r.key.guid).collect::<Vec<_>>(),
        vec![7, 8]
    );
    assert_eq!(manager.inventory().active.len(), 1);
    assert_eq!(
        persistent
            .state(manager.persistent())
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        0
    );
    manager.update(&mut persistent, &mut work).unwrap();
    assert!(manager.inventory().active.is_empty());
    assert_eq!(work.attempts.len(), 2);
}
#[test]
fn map_prelude_runs_messages_before_real_spawn_update_before_transports() {
    // Map.cpp:702-729.
    let (mut manager, mut persistent, mut work) = ready();
    work.message_add = Some(7);
    update_source_map_prelude(&mut manager, &mut persistent, &mut work, 17).unwrap();
    assert_eq!(
        work.phases,
        vec![
            MapPreludePhase::Calendar,
            MapPreludePhase::CollisionTree,
            MapPreludePhase::Messages,
            MapPreludePhase::Transports
        ]
    );
    assert_eq!(work.attempts[0].key, key(7));
}

#[test]
fn inventory_is_diagnostic_and_refuses_invalid_or_nonquiescent_continuation() {
    let (manager, _, _) = ready();
    let valid = manager.inventory();
    valid.validate().unwrap();
    assert_eq!(
        valid.require_quiescent().unwrap_err().id,
        "execution.managers.f7_continuation_owner"
    );
    type InventoryCase = (fn(&mut SpawnInventory), &'static str);
    let cases: [InventoryCase; 9] = [
        (|s| s.version = 2, "execution.spawn.inventory_version"),
        (
            |s| s.constructing_depth = 1,
            "execution.spawn.inventory_construction",
        ),
        (
            |s| {
                s.constructing_depth = 1;
                s.held_max_index = Some(0);
            },
            "execution.spawn.inventory_construction",
        ),
        (
            |s| s.map.creation_generation = 0,
            "execution.spawn.inventory_identity",
        ),
        (
            |s| s.initialization_started = false,
            "execution.spawn.inventory_initialization",
        ),
        (
            |s| {
                s.bound_active_capacity = Some(0);
                s.active.push(SpawnInfo {
                    key: key(7),
                    deadline_millis: 0,
                    used: false,
                    in_use: false,
                });
            },
            "execution.spawn.inventory_capacity",
        ),
        (
            |s| {
                s.deferred.push(SpawnInfo {
                    key: key(7),
                    deadline_millis: 0,
                    used: false,
                    in_use: true,
                })
            },
            "execution.spawn.inventory_in_use",
        ),
        (
            |s| {
                s.groups.insert(
                    7,
                    SpawnGroupHandle {
                        id: 7,
                        generation: 0,
                    },
                );
            },
            "execution.spawn.inventory_group",
        ),
        (
            |s| {
                s.string_groups.insert(
                    7,
                    SpawnGroupHandle {
                        id: 7,
                        generation: 0,
                    },
                );
            },
            "execution.spawn.inventory_group",
        ),
    ];
    for (change, expected) in cases {
        let mut state = valid.clone();
        change(&mut state);
        assert_eq!(state.validate().unwrap_err().id, expected);
    }
    for change in [0, 1, 2] {
        let mut state = valid.clone();
        match change {
            0 => state.updating = true,
            1 => state.stopped = true,
            _ => state.active.push(SpawnInfo {
                key: key(7),
                deadline_millis: 0,
                used: false,
                in_use: true,
            }),
        }
        assert_eq!(
            state.require_quiescent().unwrap_err().id,
            "execution.spawn.nonquiescent"
        );
    }
    let mut held = valid.clone();
    held.active.push(SpawnInfo {
        key: key(7),
        deadline_millis: 0,
        used: false,
        in_use: false,
    });
    held.constructing_depth = 1;
    held.held_max_index = Some(0);
    held.validate().unwrap();
    assert_eq!(
        held.require_quiescent().unwrap_err().id,
        "execution.spawn.nonquiescent"
    );
}
#[test]
fn identity_initialization_and_clock_refusals_stop_before_later_operations() {
    let (manager, _, _) = world();
    let mut wrong_map = map();
    wrong_map.creation_generation = 0;
    assert_eq!(
        SourceSpawnManager::new(wrong_map, manager.persistent())
            .err()
            .unwrap()
            .id,
        "execution.spawn.inventory_identity"
    );
    let (mut manager, mut persistent, mut work) = world();
    assert_eq!(
        manager.update(&mut persistent, &mut work).unwrap_err().id,
        "execution.spawn.not_initialized"
    );
    let (mut manager, mut persistent, mut work) = ready();
    assert_eq!(
        manager
            .initialize(&mut persistent, &mut work)
            .unwrap_err()
            .id,
        "execution.spawn.initialization_repeated"
    );
    let (mut manager, mut persistent, mut work) = world();
    work.wall_kind = Some(ClockKind::RelativeSteadyMilliseconds);
    assert_eq!(
        manager
            .initialize(&mut persistent, &mut work)
            .unwrap_err()
            .id,
        "execution.spawn.wall_clock_owner"
    );
    let (mut manager, mut persistent, mut work) = ready();
    persistent
        .bind_map(manager.persistent(), None, &mut work)
        .unwrap();
    assert_eq!(
        manager.add(&persistent, key(7), &mut work).unwrap_err().id,
        "execution.spawn.map_binding"
    );
}
#[test]
fn missing_definitions_invalid_groups_and_unbound_owners_do_not_become_false_success() {
    for group in [false, true] {
        let (mut manager, mut persistent, mut work) = world();
        if group {
            work.group_definitions.insert(
                7,
                DynamicSpawnGroup {
                    id: 7,
                    kind: GridSpawnKind::Creature,
                    first_guid: Some(8),
                    string_id: 0,
                },
            );
        } else {
            work.rosters.insert(GridSpawnKind::Creature, vec![7]);
        }
        assert_eq!(
            manager
                .initialize(&mut persistent, &mut work)
                .unwrap_err()
                .id,
            if group {
                "execution.spawn.group_definition_absent"
            } else {
                "execution.spawn.initial_definition_absent"
            }
        );
    }
    let (mut manager, mut persistent, mut work) = world();
    work.definitions.insert(key(7), definition());
    work.group_definitions.insert(
        7,
        DynamicSpawnGroup {
            id: 7,
            kind: GridSpawnKind::Creature,
            first_guid: Some(7),
            string_id: 0,
        },
    );
    work.invalid_group = true;
    assert_eq!(
        manager
            .initialize(&mut persistent, &mut work)
            .unwrap_err()
            .id,
        "execution.spawn.group_binding"
    );
    for (pending, expected) in [
        (
            SpawnPending::EffectiveDefinitions,
            "execution.spawn.effective_definitions_owner",
        ),
        (SpawnPending::LinkingOwner, "execution.spawn.linking_owner"),
        (SpawnPending::GroupOwner, "execution.spawn.group_owner"),
        (
            SpawnPending::ConstructorComposition,
            "execution.spawn.constructor_composition_owner",
        ),
        (
            SpawnPending::ClockLineage,
            "execution.spawn.clock_lineage_owner",
        ),
        (
            SpawnPending::SealedExecution,
            "execution.f1_binding_pending",
        ),
        (
            SpawnPending::DurableContinuation,
            "execution.managers.f7_continuation_owner",
        ),
        (
            SpawnPending::ActiveVectorLifetime,
            "execution.spawn.active_vector_lifetime_unbound",
        ),
    ] {
        let (mut manager, mut persistent, mut work) = ready();
        queue(&mut manager, &mut persistent, &mut work, key(7), 0);
        work.pending = Some(pending);
        assert_eq!(
            manager.update(&mut persistent, &mut work).unwrap_err().id,
            expected
        );
        assert!(manager.inventory().stopped);
        assert!(manager.inventory().active[0].in_use);
    }
}
#[test]
fn recursion_capacity_sql_observation_and_overflow_refusals_keep_the_reached_state() {
    for (action, expected) in [
        (Action::Update, "execution.spawn.recursive_update"),
        (
            Action::RespawnAll,
            "execution.spawn.recursive_respawn_all_lifetime",
        ),
        (Action::BindCapacity, "execution.spawn.capacity_binding"),
    ] {
        let (mut manager, mut persistent, mut work) = ready();
        queue(&mut manager, &mut persistent, &mut work, key(7), 0);
        work.action = Some(action);
        assert_eq!(
            manager.update(&mut persistent, &mut work).unwrap_err().id,
            expected
        );
    }
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    assert_eq!(
        manager.bind_active_capacity(0).unwrap_err().id,
        "execution.spawn.capacity_binding"
    );
    let (mut manager, mut persistent, mut work) = ready();
    work.wall = i64::MAX;
    assert_eq!(
        manager
            .respawn(&mut persistent, key(7), 1, &mut work)
            .unwrap_err()
            .id,
        "execution.spawn.deadline_overflow"
    );
    let (mut manager, mut persistent, mut work) = ready();
    persistent
        .save_respawn(
            manager.persistent(),
            GridSpawnKind::Creature,
            7,
            i64::MAX,
            0,
            &mut work,
        )
        .unwrap();
    assert_eq!(
        manager.add(&persistent, key(7), &mut work).unwrap_err().id,
        "execution.spawn.timepoint_overflow"
    );
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    work.clock = i64::MAX;
    assert_eq!(
        manager
            .respawn(&mut persistent, key(7), 1, &mut work)
            .unwrap_err()
            .id,
        "execution.spawn.timepoint_overflow"
    );
    let (mut manager, mut persistent, mut work) = ready();
    work.fail_sql = true;
    assert_eq!(
        manager
            .respawn(&mut persistent, key(7), 1, &mut work)
            .unwrap_err()
            .id,
        "authored.sql_failure"
    );
    assert_eq!(
        persistent
            .state(manager.persistent())
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        101
    );
    let (mut manager, persistent, mut work) = ready();
    work.fail_observation = true;
    assert_eq!(
        manager.add(&persistent, key(7), &mut work).unwrap_err().id,
        "authored.observation_failure"
    );
    assert_eq!(manager.inventory().active.len(), 1);
}

#[test]
fn vicinity_dispatch_keeps_group_key_order_and_exact_numeric_bits() {
    // SpawnManager.cpp:332-336; actual range computation belongs to F4.05.
    let (mut manager, mut persistent, mut work) = world();
    let go = SpawnKey {
        kind: GridSpawnKind::GameObject,
        guid: 7,
    };
    work.definitions.insert(go, definition());
    for id in [20, 10] {
        work.group_definitions.insert(
            id,
            DynamicSpawnGroup {
                id,
                first_guid: Some(7),
                kind: GridSpawnKind::GameObject,
                string_id: 0,
            },
        );
    }
    manager.initialize(&mut persistent, &mut work).unwrap();
    let position_bits = [0x80000000, 0x7fc00001, 4_f32.to_bits(), 1_f32.to_bits()];
    manager
        .respawn_groups_in_vicinity(&mut persistent, position_bits, 0x80000000, &mut work)
        .unwrap();
    assert_eq!(work.group_updates, vec![10, 20]);
    assert!(work.operations.iter().any(|operation| matches!(operation, SpawnOperation::GroupVicinity { position_bits: actual, range_bits: 0x80000000, .. } if *actual == position_bits)));
}

#[test]
fn initial_future_queues_creatures_before_gameobjects_in_each_rosters_order() {
    // SpawnManager.cpp:53-82 preserves both vector orders and the creature-before-GO phase.
    let (mut manager, mut persistent, mut work) = world();
    work.rosters.insert(GridSpawnKind::Creature, vec![8, 7]);
    work.rosters.insert(GridSpawnKind::GameObject, vec![9, 7]);
    for key in [
        key(8),
        key(7),
        SpawnKey {
            kind: GridSpawnKind::GameObject,
            guid: 9,
        },
        SpawnKey {
            kind: GridSpawnKind::GameObject,
            guid: 7,
        },
    ] {
        persistent
            .save_respawn(manager.persistent(), key.kind, key.guid, 100, 0, &mut work)
            .unwrap();
    }
    manager.initialize(&mut persistent, &mut work).unwrap();
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(
        work.attempts
            .iter()
            .map(|request| request.key)
            .collect::<Vec<_>>(),
        vec![
            key(8),
            key(7),
            SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: 9
            },
            SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: 7
            }
        ]
    );
}

#[test]
fn ordinary_growth_and_deferred_reserve_invalidate_outdated_capacity_evidence() {
    // SpawnManager.cpp:112-127,263-267 can change active-vector storage.
    let (mut manager, mut persistent, mut work) = ready();
    manager.bind_active_capacity(1).unwrap();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    queue(&mut manager, &mut persistent, &mut work, key(8), 0);
    assert_eq!(manager.inventory().bound_active_capacity, None);
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 0);
    manager.bind_active_capacity(1).unwrap();
    work.failures.insert(key(7));
    work.action = Some(Action::Add(8));
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(manager.inventory().bound_active_capacity, Some(1));
    manager.update(&mut persistent, &mut work).unwrap();
    assert_eq!(manager.inventory().bound_active_capacity, None);
    manager.inventory().validate().unwrap();
}
#[test]
fn respawn_all_with_bound_capacity_visits_synchronous_new_entries_without_deferral() {
    // SpawnManager.cpp:244-257 reevaluates the vector end, unlike Update's deferred insertion.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 500);
    manager.bind_active_capacity(2).unwrap();
    work.action = Some(Action::Add(8));
    manager.respawn_all(&mut persistent, &mut work).unwrap();
    assert_eq!(
        work.attempts
            .iter()
            .map(|request| request.key)
            .collect::<Vec<_>>(),
        vec![key(7), key(8)]
    );
    assert!(manager.inventory().active.is_empty());
    assert!(manager.inventory().deferred.is_empty());
}

#[test]
fn nested_failed_constructor_clears_in_use_and_respawn_reuses_the_held_entry() {
    // SpawnManager.cpp:30-40 clears m_inUse even for a nested false result.
    // Respawn searches only IsUsed at :137-153, not the diagnostic held-depth record.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 500);
    work.failures.insert(key(7));
    work.action = Some(Action::NestedFailureThenRespawn(7));
    manager
        .respawn(&mut persistent, key(7), 0, &mut work)
        .unwrap();
    assert_eq!(
        work.attempts
            .iter()
            .map(|request| request.key)
            .collect::<Vec<_>>(),
        vec![key(7), key(7), key(7)]
    );
    let state = manager.inventory();
    assert_eq!(state.active.len(), 1);
    assert!(!state.active[0].in_use);
    assert!(!state.active[0].used);
    assert_eq!(state.constructing_depth, 0);
    assert_eq!(state.held_max_index, None);
    assert!(state.deferred.is_empty());
    state.validate().unwrap();
}
#[test]
fn clearing_in_use_does_not_hide_held_reference_reallocation() {
    // Add's active/deferred choice is still only m_updated; .cpp:112-127.
    for capacity in [None, Some(1), Some(2)] {
        let (mut manager, mut persistent, mut work) = ready();
        queue(&mut manager, &mut persistent, &mut work, key(7), 500);
        if let Some(capacity) = capacity {
            manager.bind_active_capacity(capacity).unwrap();
        }
        work.failures.insert(key(7));
        work.action = Some(Action::NestedFailureThenAdd(8));
        let result = manager.respawn(&mut persistent, key(7), 0, &mut work);
        let state = manager.inventory();
        assert!(!state.active[0].in_use);
        if capacity == Some(2) {
            result.unwrap();
            assert_eq!(
                state.active.iter().map(|info| info.key).collect::<Vec<_>>(),
                vec![key(7), key(8)]
            );
            assert_eq!(state.constructing_depth, 0);
            assert!(state.deferred.is_empty());
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spawn.active_vector_lifetime_unbound"
            );
            assert_eq!(state.active.len(), 1);
            assert_eq!(state.constructing_depth, 1);
            assert_eq!(state.held_max_index, Some(0));
            state.validate().unwrap();
        }
    }
}
#[test]
fn nested_update_after_in_use_clears_runs_the_defined_future_entry_case() {
    // SpawnManager.cpp:273-277 now sees false IsUsed and a future deadline.
    let (mut manager, mut persistent, mut work) = ready();
    queue(&mut manager, &mut persistent, &mut work, key(7), 500);
    work.failures.insert(key(7));
    work.action = Some(Action::NestedFailureThenUpdate);
    manager
        .respawn(&mut persistent, key(7), 0, &mut work)
        .unwrap();
    assert_eq!(work.attempts.len(), 2);
    assert_eq!(manager.inventory().active[0].deadline_millis, 500_000);
    assert!(!manager.inventory().updating);
    assert_eq!(manager.inventory().constructing_depth, 0);
}
#[test]
fn recursive_respawn_all_may_erase_after_but_never_at_or_before_a_held_entry() {
    // std::vector erase in SpawnManager.cpp:253-254 can invalidate a held element.
    for target in [7, 8] {
        let (mut manager, mut persistent, mut work) = ready();
        for guid in [7, 8] {
            queue(&mut manager, &mut persistent, &mut work, key(guid), 500);
        }
        work.failures.insert(key(target));
        work.action = Some(Action::RespawnAll);
        let result = manager.respawn(&mut persistent, key(target), 0, &mut work);
        if target == 7 {
            result.unwrap();
            assert_eq!(
                work.attempts
                    .iter()
                    .map(|request| request.key)
                    .collect::<Vec<_>>(),
                vec![key(7), key(7), key(8)]
            );
            assert_eq!(manager.inventory().active.len(), 1);
            assert_eq!(manager.inventory().active[0].key, key(7));
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spawn.recursive_respawn_all_lifetime"
            );
            assert_eq!(manager.inventory().active.len(), 2);
            assert!(manager.inventory().active[0].used);
            assert!(manager.inventory().active[1].in_use);
        }
    }
}
#[test]
fn nested_update_append_checks_capacity_without_changing_defined_deferred_order() {
    // SpawnManager.cpp:263-277 reserves/appends before scanning; nested calls reset the same flag.
    for capacity in [None, Some(2)] {
        let (mut manager, mut persistent, mut work) = ready();
        queue(&mut manager, &mut persistent, &mut work, key(7), 0);
        if let Some(capacity) = capacity {
            manager.bind_active_capacity(capacity).unwrap();
        }
        work.failures.insert(key(7));
        work.action = Some(Action::NestedFailureThenAddThenUpdate(8));
        let result = manager.update(&mut persistent, &mut work);
        if capacity.is_some() {
            result.unwrap();
            assert_eq!(
                work.attempts
                    .iter()
                    .map(|request| request.key)
                    .collect::<Vec<_>>(),
                vec![key(7), key(7), key(7), key(8)]
            );
            assert_eq!(manager.inventory().active.len(), 1);
            assert_eq!(manager.inventory().active[0].key, key(7));
            assert!(manager.inventory().deferred.is_empty());
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spawn.active_vector_lifetime_unbound"
            );
            assert_eq!(manager.inventory().active.len(), 1);
            assert_eq!(manager.inventory().deferred[0].key, key(8));
        }
    }
}

impl EventMapOwner for Work {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>> {
        Ok(self.definitions.get(&key).map(|data| EventSpawnDefinition {
            map: data.map,
            cell: data.cell,
            entry: 1,
            guid: u64::from(key.guid),
        }))
    }
    fn event_grid_loaded(&mut self, _: MapIdentity, _: CellPair) -> Result<bool> {
        Ok(true)
    }
    fn event_construct(
        &mut self,
        request: DynamicSpawnRequest,
        spawns: &mut SourceSpawnManager,
        states: &mut SourceManagers,
    ) -> Result<bool> {
        self.construct(spawns, states, request)
    }
    fn event_lookup(&mut self, _: MapIdentity, _: SpawnKey) -> Result<Option<ObjectHandle>> {
        Ok(None)
    }
    fn event_remove(
        &mut self,
        _: EventLiveRequest,
        _: &mut SourceSpawnManager,
        _: &mut SourceManagers,
    ) -> Result<()> {
        panic!("no live objects")
    }
}

#[test]
fn event_map_delivery_copies_go_unit_guid_asymmetry_and_cancels_pending_spawns() {
    // GameEventMgr.cpp:846-859,954-971 uses UNIT for GO add and GAMEOBJECT for GO remove.
    let (mut manager, mut persistent, mut work) = world();
    manager.initialize(&mut persistent, &mut work).unwrap();
    let go = SpawnKey {
        kind: GridSpawnKind::GameObject,
        guid: 9,
    };
    work.definitions.insert(key(7), definition());
    work.definitions.insert(go, definition());
    manager.add(&persistent, key(7), &mut work).unwrap();
    manager.add(&persistent, go, &mut work).unwrap();
    execute_map_game_event(&mut manager, &mut persistent, &[7], &[9], true, &mut work).unwrap();
    assert!(manager.event_guid(7, HighGuid::Creature));
    assert!(manager.event_guid(9, HighGuid::Creature));
    assert!(!manager.event_guid(9, HighGuid::GameObject));
    assert_eq!(
        work.attempts
            .iter()
            .map(|request| (request.key, request.requested_guid()))
            .collect::<Vec<_>>(),
        [(key(7), 0), (go, 0)]
    );
    let pair = definition().cell;
    assert_eq!(
        persistent
            .state_mut(manager.persistent())
            .unwrap()
            .cell_guids(pair)
            .creatures,
        BTreeSet::from([7])
    );
    execute_map_game_event(&mut manager, &mut persistent, &[7], &[9], false, &mut work).unwrap();
    assert!(!manager.event_guid(7, HighGuid::Creature));
    assert!(manager.event_guid(9, HighGuid::Creature));
    assert!(!manager.event_guid(9, HighGuid::GameObject));
    assert!(manager.inventory().active.iter().all(|entry| entry.used));
    work.attempts.clear();
    manager.update(&mut persistent, &mut work).unwrap();
    assert!(work.attempts.is_empty());
    assert!(manager.inventory().active.is_empty());
    assert!(persistent
        .state_mut(manager.persistent())
        .unwrap()
        .cell_guids(pair)
        .creatures
        .is_empty());
}

#[test]
fn event_map_delivery_refuses_missing_definition_at_source_assert() {
    // GameEventMgr.cpp:844-845,854-855 asserts the fresh ObjectMgr lookup.
    let (mut manager, mut persistent, mut work) = world();
    manager.initialize(&mut persistent, &mut work).unwrap();
    assert_eq!(
        execute_map_game_event(&mut manager, &mut persistent, &[7], &[], true, &mut work)
            .unwrap_err()
            .id,
        "execution.event.spawn_definition_assert"
    );
    assert!(work.attempts.is_empty());
}
