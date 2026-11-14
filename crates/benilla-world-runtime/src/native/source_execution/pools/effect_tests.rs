// SPDX-License-Identifier: GPL-3.0-or-later
// Authored owners exercise PoolManager.cpp:305-335,442-552, not retained MP acceptance.
use super::*;
use benilla_sim::resident_map::compute_cell_pair;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Load(PoolLoadRequest),
    Live(PoolLiveAction),
    Lookup(MapIdentity),
    Random([u32; 2]),
    Wall(PoolLeafClock),
    Schedule(PoolScheduleRequest),
    Sql(PersistentSql),
    Overlay(PersistentStateIdentity, bool),
}
struct Leaf {
    work: Work,
    calls: Vec<Call>,
    loaded: bool,
    fail_load: bool,
    missing: bool,
    data_map: u32,
    min: i32,
    dynamic: Option<bool>,
    save: bool,
    boss: bool,
    now: i64,
    bounds: Option<[u32; 2]>,
}
impl Default for Leaf {
    fn default() -> Self {
        Self {
            work: Work::default(),
            calls: vec![],
            loaded: false,
            fail_load: false,
            missing: false,
            data_map: 0,
            min: 20,
            dynamic: Some(true),
            save: false,
            boss: false,
            now: 100,
            bounds: None,
        }
    }
}
fn pair() -> benilla_sim::resident_map::CellPair {
    compute_cell_pair(0_f32.to_bits(), 0_f32.to_bits()).unwrap()
}
fn object() -> ObjectHandle {
    ObjectHandle {
        thread: 2,
        construction: 1,
    }
}
impl PersistentEffects for Leaf {
    fn sql(&mut self, s: PersistentSql) -> Result<()> {
        self.calls.push(Call::Sql(s));
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        panic!("not startup")
    }
    fn observe(&mut self, s: PersistentOperation) -> Result<()> {
        if let PersistentOperation::Overlay { state, insert, .. } = s {
            self.calls.push(Call::Overlay(state, insert));
        }
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Ok(999)
    }
}
impl PoolLeafOwner for Leaf {
    fn pool_spawn(&mut self, _: GridSpawnKind, _: u32) -> Result<Option<PoolSpawnDefinition>> {
        Ok((!self.missing).then_some(PoolSpawnDefinition {
            map: self.data_map,
            cell: pair(),
            entry: 7,
            respawn: self.bounds.unwrap_or([self.min as u32; 2]),
            gameobject_min: self.min,
        }))
    }
    fn pool_grid_loaded(
        &mut self,
        _: MapIdentity,
        _: benilla_sim::resident_map::CellPair,
    ) -> Result<bool> {
        Ok(self.loaded)
    }
    fn pool_random_respawn(&mut self, b: [u32; 2]) -> Result<u32> {
        self.calls.push(Call::Random(b));
        Ok(b[0])
    }
    fn pool_wall(&mut self, site: PoolLeafClock) -> Result<ClockSample> {
        self.calls.push(Call::Wall(site));
        Ok(ClockSample::authored(ClockKind::WallSeconds, self.now))
    }
    fn pool_world_seconds(&mut self) -> Result<i64> {
        Ok(100)
    }
    fn pool_load(
        &mut self,
        r: PoolLoadRequest,
        s: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome> {
        let member = match r.key.kind {
            GridSpawnKind::Creature => PoolMember::Creature(r.key.guid),
            GridSpawnKind::GameObject => PoolMember::GameObject(r.key.guid),
        };
        assert!(s.state(r.state)?.pools().contains(member));
        assert!(
            s.state_mut(r.state)?
                .cell_guids(pair())
                .creatures
                .contains(&r.key.guid)
                || s.state_mut(r.state)?
                    .cell_guids(pair())
                    .gameobjects
                    .contains(&r.key.guid)
        );
        self.calls.push(Call::Load(r));
        Ok(if self.fail_load {
            GridCandidateOutcome::FailedAndDestroyed(object())
        } else {
            GridCandidateOutcome::Loaded(object())
        })
    }
    fn pool_lookup(&mut self, m: MapIdentity, _: SpawnKey) -> Result<Option<ObjectHandle>> {
        self.calls.push(Call::Lookup(m));
        Ok(Some(object()))
    }
    fn pool_live(&mut self, r: PoolLiveRequest, _: &mut SourceManagers) -> Result<()> {
        self.calls.push(Call::Live(r.action));
        Ok(())
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        Ok(self.save)
    }
    fn pool_creature_world_boss(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.boss)
    }
    fn pool_gameobject_dynamic(&mut self, _: u32) -> Result<Option<bool>> {
        Ok(self.dynamic)
    }
    fn pool_schedule(&mut self, r: PoolScheduleRequest, _: &mut SourceManagers) -> Result<()> {
        self.calls.push(Call::Schedule(r));
        Ok(())
    }
}
impl PoolWork for Leaf {
    fn engine_access(&mut self) -> Result<()> {
        self.work.engine_access()
    }
    fn chance(&mut self) -> Result<f64> {
        self.work.chance()
    }
    fn shuffle(&mut self, v: &mut [usize]) -> Result<()> {
        self.work.shuffle(v)
    }
    fn can_spawn(
        &mut self,
        m: &mut SourcePoolManager,
        s: &mut SourceManagers,
        i: PersistentStateIdentity,
        k: PoolKind,
        g: u32,
    ) -> Result<bool> {
        self.work.can_spawn(m, s, i, k, g)
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        s: &mut SourceManagers,
        e: PoolLeafEffect,
    ) -> Result<()> {
        execute_pool_leaf(s, e, self)
    }
    fn observe(&mut self, o: PoolOperation) -> Result<()> {
        self.work.observe(o)
    }
}
#[test]
fn selected_before_loaded_constructor_and_delayed_setter_order() {
    for (kind, slot) in [(PoolKind::Creature, 2), (PoolKind::GameObject, 4)] {
        let mut pools = loaded(&[(1, 1)], &[(slot, 40, 1, 0.0)]);
        let (mut states, id) = state(true);
        let mut leaf = Leaf {
            loaded: true,
            save: true,
            ..Leaf::default()
        };
        leaf.work.blocked.insert(40);
        pools
            .spawn_pool(&mut states, id, 1, false, &mut leaf)
            .unwrap();
        if kind == PoolKind::GameObject {
            assert!(leaf.calls.is_empty());
            leaf.work.blocked.clear();
            pools
                .spawn_pool(&mut states, id, 1, false, &mut leaf)
                .unwrap();
        }
        let actions: Vec<_> = leaf
            .calls
            .iter()
            .filter_map(|c| {
                if let Call::Live(a) = c {
                    Some(*a)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            actions,
            if kind == PoolKind::Creature {
                vec![
                    PoolLiveAction::SetCreatureDelay(20),
                    PoolLiveAction::SetRespawnTime(20),
                    PoolLiveAction::SaveRespawn,
                ]
            } else {
                vec![
                    PoolLiveAction::SetRespawnTime(20),
                    PoolLiveAction::SaveRespawn,
                ]
            }
        );
        assert!(matches!(leaf.calls[0], Call::Overlay(_, true)));
        assert!(matches!(leaf.calls[1], Call::Load(_)));
        assert_eq!(leaf.calls[2], Call::Random([20, 20]));
        assert!(!leaf.calls.iter().any(|c| matches!(c, Call::Wall(_))));
    }
}
#[test]
fn failed_loaded_candidate_keeps_selection_and_overlay_without_replacement_draw() {
    let mut pools = loaded(&[(1, 1)], &[(2, 40, 1, 0.0)]);
    let (mut states, id) = state(true);
    let mut leaf = Leaf {
        loaded: true,
        fail_load: true,
        ..Leaf::default()
    };
    pools
        .spawn_pool(&mut states, id, 1, false, &mut leaf)
        .unwrap();
    assert!(states
        .state(id)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(40)));
    assert_eq!(leaf.calls.len(), 2);
    assert!(matches!(leaf.calls[1], Call::Load(_)));
}
#[test]
fn unloaded_deadline_draw_clock_order_and_dynamic_go_schedule() {
    for (kind, slot) in [(GridSpawnKind::Creature, 2), (GridSpawnKind::GameObject, 4)] {
        for loaded_map in [false, true] {
            let mut pools = loaded(&[(1, 1)], &[(slot, 40, 1, 0.0)]);
            let (mut states, id) = state(loaded_map);
            let mut leaf = Leaf::default();
            pools
                .spawn_pool(&mut states, id, 1, false, &mut leaf)
                .unwrap();
            assert_eq!(states.state(id).unwrap().respawn(kind, 40), 120);
            assert_eq!(
                &leaf.calls[1..3],
                if kind == GridSpawnKind::Creature {
                    &[
                        Call::Wall(PoolLeafClock::UnloadedCreature),
                        Call::Random([20, 20]),
                    ]
                } else {
                    &[
                        Call::Random([20, 20]),
                        Call::Wall(PoolLeafClock::UnloadedGameObject),
                    ]
                }
            );
            let sql: Vec<_> = leaf
                .calls
                .iter()
                .filter_map(|c| if let Call::Sql(s) = c { Some(*s) } else { None })
                .collect();
            assert_eq!(
                sql,
                [
                    PersistentSql::Begin,
                    PersistentSql::Delete {
                        kind,
                        guid: 40,
                        instance: 0
                    },
                    PersistentSql::Insert {
                        kind,
                        guid: 40,
                        deadline: 120,
                        instance: 0
                    },
                    PersistentSql::Commit
                ]
            );
            assert_eq!(
                leaf.calls
                    .iter()
                    .filter(|c| matches!(c, Call::Schedule(_)))
                    .count(),
                usize::from(loaded_map && kind == GridSpawnKind::GameObject)
            );
        }
    }
}
#[test]
fn negative_go_unloaded_skips_delay_but_loaded_keeps_unsigned_draw() {
    for loaded_grid in [false, true] {
        let mut pools = loaded(&[(1, 1)], &[(4, 40, 1, 0.0)]);
        let (mut states, id) = state(true);
        let mut leaf = Leaf {
            loaded: loaded_grid,
            min: -20,
            ..Leaf::default()
        };
        pools
            .spawn_pool(&mut states, id, 1, false, &mut leaf)
            .unwrap();
        assert_eq!(
            leaf.calls
                .iter()
                .filter(|c| matches!(c, Call::Random(_)))
                .count(),
            usize::from(loaded_grid)
        );
        if loaded_grid {
            assert!(leaf.calls.contains(&Call::Random([(-20_i32) as u32; 2])));
        } else {
            assert_eq!(leaf.calls, [Call::Overlay(id, true)]);
        }
    }
}
#[test]
fn zero_default_go_still_draws_saves_and_schedules_and_static_go_does_not_schedule() {
    for dynamic in [None, Some(true), Some(false)] {
        let mut pools = loaded(&[(1, 1)], &[(4, 40, 1, 0.0)]);
        let (mut states, id) = state(true);
        let mut leaf = Leaf {
            min: 0,
            dynamic,
            ..Leaf::default()
        };
        pools
            .spawn_pool(&mut states, id, 1, false, &mut leaf)
            .unwrap();
        assert_eq!(
            states
                .state(id)
                .unwrap()
                .respawn(GridSpawnKind::GameObject, 40),
            0
        );
        assert!(leaf.calls.contains(&Call::Random([0, 0])));
        assert_eq!(
            leaf.calls
                .iter()
                .filter(|c| matches!(c, Call::Schedule(_)))
                .count(),
            usize::from(dynamic != Some(false))
        );
        assert!(!leaf
            .calls
            .iter()
            .any(|c| matches!(c, Call::Sql(PersistentSql::Insert { .. }))));
    }
}
#[test]
fn same_member_readd_uses_live_lookup_without_delay_or_new_constructor() {
    let mut pools = loaded(&[(1, 1)], &[(2, 40, 1, 0.0)]);
    let (mut states, id) = state(true);
    let mut leaf = Leaf {
        loaded: true,
        ..Leaf::default()
    };
    pools
        .spawn_pool(&mut states, id, 1, true, &mut leaf)
        .unwrap();
    leaf.calls.clear();
    pools
        .update_pool(&mut states, id, 1, PoolKind::Creature, 40, &mut leaf)
        .unwrap();
    assert_eq!(
        leaf.calls,
        [
            Call::Lookup(states.state(id).unwrap().map_binding().unwrap()),
            Call::Live(PoolLiveAction::MapAdd)
        ]
    );
    assert_eq!(states.state(id).unwrap().pools().count(1), 1);
}
#[test]
fn leaf_despawn_and_readd_use_other_map_but_spawn_overlay_stays_on_supplied_state() {
    for kind in [PoolKind::Creature, PoolKind::GameObject] {
        let (mut states, id) = state(true);
        let other = states
            .create_state(
                PersistentMapEntry { id: 1, map_type: 0 },
                0,
                0,
                false,
                true,
                0,
                &mut Effects,
            )
            .unwrap();
        let map = MapIdentity {
            map: 1,
            instance: 0,
            creation_generation: 2,
        };
        states.bind_map(other, Some(map), &mut Effects).unwrap();
        let mut leaf = Leaf {
            data_map: 1,
            ..Leaf::default()
        };
        let effect = PoolLeafEffect {
            state: id,
            kind,
            guid: 40,
            action: PoolEffectAction::Spawn { instantly: true },
        };
        execute_pool_leaf(&mut states, effect, &mut leaf).unwrap();
        assert_eq!(leaf.calls, [Call::Overlay(id, true)]);
        leaf.calls.clear();
        execute_pool_leaf(
            &mut states,
            PoolLeafEffect {
                action: PoolEffectAction::Despawn,
                ..effect
            },
            &mut leaf,
        )
        .unwrap();
        assert_eq!(
            leaf.calls,
            [
                Call::Overlay(other, false),
                Call::Lookup(map),
                Call::Live(PoolLiveAction::Remove)
            ]
        );
        leaf.calls.clear();
        execute_pool_leaf(
            &mut states,
            PoolLeafEffect {
                action: PoolEffectAction::Respawn,
                ..effect
            },
            &mut leaf,
        )
        .unwrap();
        assert_eq!(
            leaf.calls,
            [Call::Lookup(map), Call::Live(PoolLiveAction::MapAdd)]
        );
        leaf.calls.clear();
        leaf.data_map = 99;
        execute_pool_leaf(
            &mut states,
            PoolLeafEffect {
                action: PoolEffectAction::Despawn,
                ..effect
            },
            &mut leaf,
        )
        .unwrap();
        assert!(leaf.calls.is_empty());
    }
}
#[test]
fn recursive_type_order_reaches_real_overlays_and_loaded_leaf_calls() {
    let mut pools = loaded(
        &[(1, 3), (2, 1)],
        &[
            (6, 2, 1, 0.0),
            (2, 20, 2, 0.0),
            (4, 7, 1, 0.0),
            (2, 7, 1, 0.0),
        ],
    );
    let (mut states, id) = state(true);
    let mut leaf = Leaf {
        loaded: true,
        ..Leaf::default()
    };
    pools
        .spawn_pool(&mut states, id, 1, true, &mut leaf)
        .unwrap();
    let loads: Vec<_> = leaf
        .calls
        .iter()
        .filter_map(|call| {
            if let Call::Load(r) = call {
                Some((r.key.kind, r.key.guid))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        loads,
        [
            (GridSpawnKind::Creature, 20),
            (GridSpawnKind::GameObject, 7),
            (GridSpawnKind::Creature, 7)
        ]
    );
    assert_eq!(
        leaf.calls
            .iter()
            .filter(|c| matches!(c, Call::Overlay(_, true)))
            .count(),
        3
    );
    leaf.calls.clear();
    pools.despawn_pool(&mut states, id, 1, &mut leaf).unwrap();
    let leaves: Vec<_> = leaf
        .work
        .operations
        .iter()
        .filter_map(|op| {
            if let PoolOperation::Leaf(e) = op {
                (e.action == PoolEffectAction::Despawn).then_some((e.kind, e.guid))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        leaves,
        [
            (PoolKind::Creature, 7),
            (PoolKind::GameObject, 7),
            (PoolKind::Creature, 20)
        ]
    );
    assert_eq!(
        leaf.calls
            .iter()
            .filter(|c| matches!(c, Call::Live(PoolLiveAction::Remove)))
            .count(),
        3
    );
    assert!(states
        .state_mut(id)
        .unwrap()
        .cell_guids(pair())
        .creatures
        .is_empty());
    assert!(states
        .state_mut(id)
        .unwrap()
        .cell_guids(pair())
        .gameobjects
        .is_empty());
}

struct MapLeaf {
    leaf: Leaf,
    entry: Option<PersistentMapEntry>,
    order: Vec<PersistentStateIdentity>,
    next_calls: Vec<Option<PersistentStateIdentity>>,
}
impl PoolWork for MapLeaf {
    fn engine_access(&mut self) -> Result<()> {
        self.leaf.engine_access()
    }
    fn chance(&mut self) -> Result<f64> {
        self.leaf.chance()
    }
    fn shuffle(&mut self, v: &mut [usize]) -> Result<()> {
        self.leaf.shuffle(v)
    }
    fn can_spawn(
        &mut self,
        m: &mut SourcePoolManager,
        s: &mut SourceManagers,
        i: PersistentStateIdentity,
        k: PoolKind,
        g: u32,
    ) -> Result<bool> {
        self.leaf.can_spawn(m, s, i, k, g)
    }
    fn leaf_effect(
        &mut self,
        m: &mut SourcePoolManager,
        s: &mut SourceManagers,
        e: PoolLeafEffect,
    ) -> Result<()> {
        self.leaf.leaf_effect(m, s, e)
    }
    fn observe(&mut self, o: PoolOperation) -> Result<()> {
        PoolWork::observe(&mut self.leaf, o)
    }
}
impl PoolStateOwner for MapLeaf {
    fn pool_map_entry(&mut self, _: u32) -> Result<Option<PersistentMapEntry>> {
        Ok(self.entry)
    }
    fn pool_next_instance_state(
        &mut self,
        _: &SourceManagers,
        _: u32,
        after: Option<PersistentStateIdentity>,
    ) -> Result<Option<PersistentStateIdentity>> {
        self.next_calls.push(after);
        Ok(match after {
            None => self.order.first().copied(),
            Some(id) => self
                .order
                .iter()
                .position(|v| *v == id)
                .and_then(|i| self.order.get(i + 1))
                .copied(),
        })
    }
}
#[test]
fn global_pool_operations_enumerate_world_and_instance_states_in_source_order() {
    let entry = PersistentMapEntry { id: 0, map_type: 1 };
    let mut states = SourceManagers::default();
    let ids: [PersistentStateIdentity; 2] = [1, 2].map(|instance| {
        states
            .create_state(entry, instance, 999, false, true, 0, &mut Effects)
            .unwrap()
    });
    let mut owner = MapLeaf {
        leaf: Leaf::default(),
        entry: Some(entry),
        order: vec![ids[1], ids[0]],
        next_calls: vec![],
    };
    let mut pools = loaded(&[(1, 1)], &[(2, 40, 1, 0.0)]);
    pools
        .spawn_pool_in_maps(&mut states, 1, true, &mut owner)
        .unwrap();
    let overlays: Vec<_> = owner
        .leaf
        .calls
        .iter()
        .filter_map(|call| {
            if let Call::Overlay(id, true) = call {
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(overlays, [ids[1], ids[0]]);
    assert_eq!(owner.next_calls, [None, Some(ids[1]), Some(ids[0])]);
    pools
        .update_pool_in_maps(&mut states, 1, PoolKind::Creature, 40, &mut owner)
        .unwrap();
    pools
        .despawn_pool_in_maps(&mut states, 1, &mut owner)
        .unwrap();
    for id in ids {
        assert_eq!(states.state(id).unwrap().pools().count(1), 0);
    }
    let (mut states, id) = state(false);
    owner.entry = Some(PersistentMapEntry { id: 0, map_type: 0 });
    owner.next_calls.clear();
    owner.leaf.calls.clear();
    pools
        .spawn_pool_in_maps(&mut states, 1, true, &mut owner)
        .unwrap();
    assert_eq!(owner.leaf.calls, [Call::Overlay(id, true)]);
    assert!(owner.next_calls.is_empty());
    owner.entry = None;
    owner.leaf.calls.clear();
    pools
        .despawn_pool_in_maps(&mut states, 1, &mut owner)
        .unwrap();
    assert!(owner.leaf.calls.is_empty());
}

#[test]
fn pool_delay_guards_refuse_only_reached_inversion_or_signed_time_overflow() {
    for kind in [PoolKind::Creature, PoolKind::GameObject] {
        for (now, success) in [(i64::MAX - 20, true), (i64::MAX - 19, false)] {
            let (mut states, id) = state(false);
            let mut owner = Leaf {
                now,
                ..Leaf::default()
            };
            let result = execute_pool_leaf(
                &mut states,
                PoolLeafEffect {
                    state: id,
                    kind,
                    guid: 40,
                    action: PoolEffectAction::Spawn { instantly: false },
                },
                &mut owner,
            );
            if success {
                result.unwrap();
                let grid_kind = if kind == PoolKind::Creature {
                    GridSpawnKind::Creature
                } else {
                    GridSpawnKind::GameObject
                };
                assert_eq!(states.state(id).unwrap().respawn(grid_kind, 40), i64::MAX);
            } else {
                assert_eq!(result.unwrap_err().id, "execution.pool.time_add_undefined");
                assert!(!owner.calls.iter().any(|c| matches!(c, Call::Sql(_))));
            }
        }
        for (bounds, success) in [([20, 20], true), ([21, 20], false)] {
            let (mut states, id) = state(false);
            let mut owner = Leaf {
                bounds: Some(bounds),
                ..Leaf::default()
            };
            let result = execute_pool_leaf(
                &mut states,
                PoolLeafEffect {
                    state: id,
                    kind,
                    guid: 40,
                    action: PoolEffectAction::Spawn { instantly: false },
                },
                &mut owner,
            );
            if success {
                result.unwrap();
            } else {
                assert_eq!(
                    result.unwrap_err().id,
                    "execution.pool.respawn_bounds_undefined"
                );
                assert!(!owner
                    .calls
                    .iter()
                    .any(|c| matches!(c, Call::Random(_) | Call::Sql(_))));
            }
        }
    }
}
