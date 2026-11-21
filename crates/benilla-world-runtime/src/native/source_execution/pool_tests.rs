// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored executable owners. These are not production or retained F1 outcomes.
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[path = "pools/effect_tests.rs"]
mod effects;
#[derive(Default)]
struct Rows {
    maximum: Option<u16>,
    templates: VecDeque<PoolTemplateRow>,
    members: BTreeMap<usize, VecDeque<PoolMembershipRow>>,
}
impl PoolRows for Rows {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(self.maximum)
    }
    fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
        Ok(self.templates.pop_front())
    }
    fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>> {
        Ok(self.members.entry(slot).or_default().pop_front())
    }
}
fn rows(limits: &[(u16, u32)], members: &[(usize, u32, u16, f32)]) -> Rows {
    let mut rows = Rows {
        maximum: limits.iter().map(|r| r.0).max(),
        ..Rows::default()
    };
    rows.templates = limits
        .iter()
        .enumerate()
        .map(|(i, &(id, limit))| PoolTemplateRow {
            ordinal: i as u64,
            id,
            limit,
            description: vec![],
        })
        .collect();
    for &(slot, guid, pool, chance) in members {
        let list = rows.members.entry(slot).or_default();
        list.push_back(PoolMembershipRow {
            ordinal: list.len() as u64,
            guid,
            pool,
            chance,
        });
    }
    rows
}
#[derive(Default)]
struct Definitions {
    spawns: BTreeMap<(PoolKind, u32), u32>,
    maps: BTreeSet<u32>,
    fail: bool,
}
impl PoolLoadOwner for Definitions {
    fn spawn_map(&mut self, kind: PoolKind, guid: u32) -> Result<Option<u32>> {
        require(!self.fail, "authored.effective_owner")?;
        Ok(self.spawns.get(&(kind, guid)).copied())
    }
    fn map_exists(&mut self, map: u32) -> Result<bool> {
        Ok(self.maps.contains(&map))
    }
}
fn definitions(members: &[(usize, u32, u16, f32)]) -> Definitions {
    Definitions {
        spawns: members
            .iter()
            .filter(|r| r.0 != 6)
            .map(|r| {
                (
                    (
                        if r.0 <= 3 {
                            PoolKind::Creature
                        } else {
                            PoolKind::GameObject
                        },
                        r.1,
                    ),
                    0,
                )
            })
            .collect(),
        maps: BTreeSet::from([0, 1]),
        fail: false,
    }
}
#[derive(Default)]
struct Effects;
impl PersistentEffects for Effects {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("authored.query_unreached"))
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("authored.reset_unreached"))
    }
}
fn state(loaded: bool) -> (SourceManagers, PersistentStateIdentity) {
    let mut states = SourceManagers::default();
    let mut effects = Effects;
    let id = states
        .create_state(
            PersistentMapEntry { id: 0, map_type: 0 },
            0,
            0,
            false,
            true,
            0,
            &mut effects,
        )
        .unwrap();
    if loaded {
        states
            .bind_map(
                id,
                Some(MapIdentity {
                    map: 0,
                    instance: 0,
                    creation_generation: 1,
                }),
                &mut effects,
            )
            .unwrap();
    }
    (states, id)
}
#[derive(Default)]
struct Work {
    engine: Mt19937,
    cursor: DrawCursor,
    accesses: usize,
    blocked: BTreeSet<u32>,
    effects: Vec<PoolLeafEffect>,
    operations: Vec<PoolOperation>,
    gates: Vec<(PoolKind, u32)>,
    fail_gate: bool,
    fail_effect: bool,
    fail_observe: bool,
    nested_spawn: bool,
}
impl PoolWork for Work {
    fn engine_access(&mut self) -> Result<()> {
        self.accesses += 1;
        Ok(())
    }
    fn chance(&mut self) -> Result<f64> {
        let mut sink = |_| {};
        SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .unwrap()
            .rand_chance()
            .map_err(|_| refuse("authored.rng"))
    }
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<()> {
        let mut sink = |_| {};
        SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .unwrap()
            .shuffle(indices)
            .map_err(|_| refuse("authored.rng"))
    }
    fn can_spawn(
        &mut self,
        _: &mut SourcePoolManager,
        _: &mut SourceManagers,
        _: PersistentStateIdentity,
        kind: PoolKind,
        guid: u32,
    ) -> Result<bool> {
        self.gates.push((kind, guid));
        require(!self.fail_gate, "authored.gate_owner")?;
        Ok(!self.blocked.contains(&guid))
    }
    fn leaf_effect(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        require(!self.fail_effect, &PoolPending::LeafEffects.refusal().id)?;
        assert!(states
            .state(effect.state)?
            .pools()
            .contains(match effect.kind {
                PoolKind::Creature => PoolMember::Creature(effect.guid),
                PoolKind::GameObject => PoolMember::GameObject(effect.guid),
                PoolKind::Child => panic!("child effects execute inside manager"),
            }));
        self.effects.push(effect);
        if self.nested_spawn {
            self.nested_spawn = false;
            manager.spawn_pool(states, effect.state, 1, true, self)?;
        }
        Ok(())
    }
    fn observe(&mut self, operation: PoolOperation) -> Result<()> {
        require(
            !self.fail_observe || !matches!(operation, PoolOperation::Selected { .. }),
            "authored.observer",
        )?;
        self.operations.push(operation);
        Ok(())
    }
}
fn loaded(limits: &[(u16, u32)], members: &[(usize, u32, u16, f32)]) -> SourcePoolManager {
    let mut manager = SourcePoolManager::default();
    manager
        .load(&mut rows(limits, members), &mut definitions(members))
        .unwrap();
    manager
}
#[test]
fn loader_uses_effective_spawns_and_keeps_direct_duplicates_first_insert_and_zero_sentinel() {
    // PoolManager.cpp:680-709,741-780,816-845,878-917.
    let members = [
        (2, 7, 1, 0.0),
        (2, 7, 2, 0.0),
        (3, 7, 2, 0.0),
        (4, 7, 1, 0.0),
        (5, 7, 2, 0.0),
        (2, 8, 0, 0.0),
        (3, 8, 1, 0.0),
        (2, 99, 1, 0.0),
    ];
    let mut definitions = definitions(&members);
    definitions.spawns.remove(&(PoolKind::Creature, 99));
    let mut manager = SourcePoolManager::default();
    manager
        .load(
            &mut rows(&[(0, 1), (1, 1), (2, 2)], &members),
            &mut definitions,
        )
        .unwrap();
    assert_eq!(manager.part_of(PoolKind::Creature, 7).unwrap(), 1);
    assert_eq!(manager.part_of(PoolKind::GameObject, 7).unwrap(), 1);
    assert_eq!(manager.part_of(PoolKind::Creature, 8).unwrap(), 0);
    let inventory = manager.inventory();
    assert_eq!(inventory.templates[2].groups[2].equal.len(), 1);
    assert_eq!(
        inventory.templates[1].groups[2]
            .equal
            .iter()
            .map(|v| v.guid)
            .collect::<Vec<_>>(),
        [7, 8]
    );
    assert_eq!(
        inventory
            .records
            .iter()
            .filter(|v| v.decision == PoolLoadDecision::DuplicateGuid)
            .count(),
        2
    );
    assert!(inventory
        .records
        .iter()
        .any(|v| v.guid == 99 && v.decision == PoolLoadDecision::MissingSpawn));
}
#[test]
fn loader_preserves_map_rejections_holes_and_last_template_overwrite() {
    let members = [
        (2, 7, 1, 0.0),
        (2, 8, 1, 0.0),
        (2, 9, 2, 0.0),
        (2, 10, 3, 0.0),
        (2, 11, 2, -1.0),
        (4, 12, 2, 101.0),
        (6, 2, 1, 0.0),
        (6, 4, 1, 0.0),
        (6, 1, 1, 0.0),
    ];
    let mut owner = definitions(&members);
    owner.spawns.insert((PoolKind::Creature, 8), 1);
    owner.spawns.insert((PoolKind::Creature, 9), 42);
    let mut input = rows(&[(1, 1), (1, 2), (2, 1)], &members);
    let mut manager = SourcePoolManager::default();
    manager.load(&mut input, &mut owner).unwrap();
    let inventory = manager.inventory();
    assert_eq!(inventory.templates[1].limit, 2);
    assert!(!inventory.templates[0].auto_spawn);
    for decision in [
        PoolLoadDecision::DifferentMap,
        PoolLoadDecision::MissingMap,
        PoolLoadDecision::PoolOutOfRange,
        PoolLoadDecision::InvalidChance,
        PoolLoadDecision::ChildOutOfRange,
        PoolLoadDecision::SelfCycle,
    ] {
        assert!(
            inventory.records.iter().any(|v| v.decision == decision),
            "{decision:?}"
        );
    }
}
#[test]
fn hierarchy_removes_cycle_or_cross_map_relation_without_restoring_child_auto_spawn() {
    // PoolManager.cpp:983-1022. Cycle traversal deliberately excludes maximum.
    let members = [
        (2, 7, 1, 0.0),
        (2, 8, 2, 0.0),
        (6, 1, 2, 0.0),
        (6, 2, 1, 0.0),
    ];
    let manager = loaded(&[(1, 1), (2, 1)], &members);
    let inventory = manager.inventory();
    assert!(!inventory.templates[1].auto_spawn && !inventory.templates[2].auto_spawn);
    assert!(inventory
        .records
        .iter()
        .any(|v| v.decision == PoolLoadDecision::CycleRemoved));
    assert_eq!(manager.top_pool(PoolKind::Creature, 7).unwrap(), 2);
    let mut owner = definitions(&members);
    owner.spawns.insert((PoolKind::Creature, 8), 1);
    let mut manager = SourcePoolManager::default();
    manager
        .load(&mut rows(&[(1, 1), (2, 1)], &members), &mut owner)
        .unwrap();
    assert!(manager
        .inventory()
        .records
        .iter()
        .any(|v| v.decision == PoolLoadDecision::RelationMapRemoved));
    assert!(!manager.inventory().templates[1].auto_spawn);
}
#[test]
fn integrity_disables_only_auto_spawn_and_preserves_binary32_membership_classes() {
    let manager = loaded(&[(1, 1), (2, 2)], &[(2, 7, 1, 25.0), (4, 8, 2, 25.0)]);
    let inventory = manager.inventory();
    assert!(!inventory.templates[1].auto_spawn);
    assert!(inventory.templates[2].auto_spawn);
    assert_eq!(
        inventory.templates[1].groups[2].explicit[0].chance_bits,
        25.0f32.to_bits()
    );
    assert_eq!(inventory.templates[2].groups[1].equal[0].guid, 8);
}
#[test]
fn initialize_matches_exact_map_and_does_not_own_initpools_flag() {
    let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    let (mut states, id) = state(false);
    let mut work = Work::default();
    manager.initialize(&mut states, id, &mut work).unwrap();
    assert!(states
        .state(id)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    assert!(!states.state(id).unwrap().pools().initialized());
    assert!(work.gates.is_empty());
    assert_eq!(work.effects.len(), 1);
}
#[test]
fn mixed_child_gameobject_creature_counts_and_effect_order_share_real_state() {
    // PoolManager.cpp:62-107,1087-1104.
    let mut manager = loaded(
        &[(1, 3), (2, 1)],
        &[
            (6, 2, 1, 0.0),
            (2, 20, 2, 0.0),
            (4, 7, 1, 0.0),
            (2, 7, 1, 0.0),
        ],
    );
    let (mut states, id) = state(true);
    let mut work = Work::default();
    manager
        .spawn_pool(&mut states, id, 1, true, &mut work)
        .unwrap();
    let data = states.state(id).unwrap().pools();
    assert_eq!((data.count(1), data.count(2)), (3, 1));
    assert!(data.contains(PoolMember::Creature(7)) && data.contains(PoolMember::GameObject(7)));
    assert_eq!(
        work.effects
            .iter()
            .map(|v| (v.kind, v.guid))
            .collect::<Vec<_>>(),
        [
            (PoolKind::Creature, 20),
            (PoolKind::GameObject, 7),
            (PoolKind::Creature, 7)
        ]
    );
    manager.despawn_pool(&mut states, id, 1, &mut work).unwrap();
    assert_eq!(
        work.effects[3..]
            .iter()
            .map(|v| (v.kind, v.guid))
            .collect::<Vec<_>>(),
        [
            (PoolKind::Creature, 7),
            (PoolKind::GameObject, 7),
            (PoolKind::Creature, 20)
        ]
    );
    assert_eq!(states.state(id).unwrap().pools().count(1), 0);
    assert!(!states
        .state(id)
        .unwrap()
        .pools()
        .contains(PoolMember::Pool(2)));
}
#[test]
fn creature_can_spawn_boolean_is_discarded_but_go_boolean_and_refusals_are_not() {
    // PoolManager.cpp:368-381. This surprising branch is intentional source parity.
    for kind in [PoolKind::Creature, PoolKind::GameObject] {
        let slot = if kind == PoolKind::Creature { 2 } else { 4 };
        let mut manager = loaded(&[(1, 1)], &[(slot, 7, 1, 0.0)]);
        let (mut states, id) = state(true);
        let mut work = Work {
            blocked: BTreeSet::from([7]),
            ..Work::default()
        };
        manager
            .spawn_pool(&mut states, id, 1, true, &mut work)
            .unwrap();
        assert_eq!(work.effects.len(), usize::from(kind == PoolKind::Creature));
        assert_eq!(work.gates, [(kind, 7)]);
        let mut manager = loaded(&[(1, 1)], &[(slot, 7, 1, 0.0)]);
        let (mut states, id) = state(true);
        work.fail_gate = true;
        assert_eq!(
            manager
                .spawn_pool(&mut states, id, 1, true, &mut work)
                .unwrap_err()
                .id,
            "authored.gate_owner"
        );
        assert!(manager.inventory().stopped);
    }
}
#[test]
fn same_trigger_respawns_without_count_change_and_child_respawn_is_source_noop() {
    let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    let (mut states, id) = state(true);
    let mut work = Work::default();
    manager
        .spawn_pool(&mut states, id, 1, true, &mut work)
        .unwrap();
    manager
        .update_pool(&mut states, id, 1, PoolKind::Creature, 7, &mut work)
        .unwrap();
    assert_eq!(states.state(id).unwrap().pools().count(1), 1);
    assert_eq!(
        work.effects.last().unwrap().action,
        PoolEffectAction::Respawn
    );
    let mut manager = loaded(&[(1, 1), (2, 1)], &[(6, 2, 1, 0.0), (2, 7, 2, 0.0)]);
    let (mut states, id) = state(false);
    let mut work = Work::default();
    manager
        .spawn_pool(&mut states, id, 1, true, &mut work)
        .unwrap();
    manager
        .update_pool(&mut states, id, 2, PoolKind::Creature, 7, &mut work)
        .unwrap();
    assert_eq!(work.effects.len(), 1);
    assert!(work
        .operations
        .iter()
        .any(|v| matches!(v, PoolOperation::ChildRespawnNoop { pool: 2, .. })));
}
#[test]
fn last_despawned_roll_is_skipped_without_rerolling_or_filling_remaining_slots() {
    // cpp:395-436; GCC 12.2 paired shuffle from seed 5489 gives [7,9,8], then [9,7,8], then [7,8,9].
    let mut manager = loaded(&[(1, 2)], &[(2, 7, 1, 0.0), (2, 8, 1, 0.0), (2, 9, 1, 0.0)]);
    let (mut states, id) = state(false);
    states
        .state_mut(id)
        .unwrap()
        .pools_mut()
        .add(PoolMember::Creature(7), 1);
    let mut work = Work::default();
    let mut earlier = [7, 8, 9];
    work.shuffle(&mut earlier).unwrap();
    assert_eq!(earlier, [7, 9, 8]);
    manager
        .update_pool(&mut states, id, 1, PoolKind::Creature, 7, &mut work)
        .unwrap();
    assert_eq!(
        work.effects
            .iter()
            .map(|v| (v.guid, v.action))
            .collect::<Vec<_>>(),
        [
            (9, PoolEffectAction::Spawn { instantly: false }),
            (7, PoolEffectAction::Despawn)
        ]
    );
    assert_eq!(states.state(id).unwrap().pools().count(1), 1);
    assert_eq!(work.cursor.raw, 3);
    assert!(manager.inventory().frames.is_empty());
}
#[test]
fn zero_remaining_count_does_not_access_engine_and_finite_reentry_is_not_refused() {
    let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    let (mut states, id) = state(false);
    let mut work = Work {
        nested_spawn: true,
        ..Work::default()
    };
    manager
        .spawn_pool(&mut states, id, 1, true, &mut work)
        .unwrap();
    assert_eq!((work.accesses, work.effects.len()), (1, 1));
    manager
        .spawn_pool(&mut states, id, 1, true, &mut work)
        .unwrap();
    assert_eq!(work.accesses, 1);
    manager.inventory().require_quiescent().unwrap();
    assert_eq!(
        manager
            .inventory()
            .require_durable_checkpoint()
            .unwrap_err()
            .id,
        "execution.managers.f7_continuation_owner"
    );
}
#[test]
fn missing_leaf_owner_keeps_selection_and_transient_frame_then_stops() {
    let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    let (mut states, id) = state(false);
    let mut work = Work {
        fail_effect: true,
        ..Work::default()
    };
    assert_eq!(
        manager
            .spawn_pool(&mut states, id, 1, true, &mut work)
            .unwrap_err()
            .id,
        "execution.pool.leaf_effect_owner"
    );
    assert!(states
        .state(id)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    assert_eq!(manager.inventory().frames.len(), 1);
    assert!(manager.inventory().require_quiescent().is_err());
    assert_eq!(
        manager
            .spawn_pool(&mut states, id, 1, true, &mut work)
            .unwrap_err()
            .id,
        "execution.pool.stopped"
    );
}
#[test]
fn count_increment_refusal_occurs_before_roll_and_retains_selected_trigger() {
    let mut manager = loaded(&[(1, 0x8000_0000)], &[(2, 7, 1, 0.0)]);
    let (mut states, id) = state(false);
    states
        .state_mut(id)
        .unwrap()
        .pools_mut()
        .add(PoolMember::Creature(7), 1);
    let mut work = Work::default();
    assert_eq!(
        manager
            .update_pool(&mut states, id, 1, PoolKind::Creature, 7, &mut work)
            .unwrap_err()
            .id,
        "execution.pool.count_increment_undefined"
    );
    assert_eq!(work.accesses, 0);
    assert_eq!(states.state(id).unwrap().pools().count(1), 1);
}
#[test]
fn maximum_u16_iteration_refuses_only_unbounded_boundary_and_template_index_is_guarded() {
    for maximum in [65534, 65535] {
        let mut manager = SourcePoolManager::default();
        let result = manager.load(&mut rows(&[(maximum, 0)], &[]), &mut Definitions::default());
        if maximum == 65534 {
            result.unwrap();
            assert_eq!(manager.inventory().templates.len(), 65535);
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.pool.template_loop_unbounded"
            );
        }
    }
    let mut input = rows(&[(2, 0)], &[]);
    input.maximum = Some(1);
    assert_eq!(
        SourcePoolManager::default()
            .load(&mut input, &mut Definitions::default())
            .unwrap_err()
            .id,
        "execution.pool.template_index_undefined"
    );
}
#[test]
fn absent_maximum_empty_template_and_loader_owner_failures_remain_distinct() {
    let (mut states, id) = state(false);
    let mut work = Work::default();
    let mut manager = SourcePoolManager::default();
    assert_eq!(
        manager.max_pool_id().unwrap_err().id,
        "execution.db_scripts.pool_max_id_owner"
    );
    assert_eq!(
        manager
            .initialize(&mut states, id, &mut work)
            .unwrap_err()
            .id,
        "execution.pool.loader_owner"
    );
    for maximum in [None, Some(2), Some(65535)] {
        let mut manager = SourcePoolManager::default();
        manager
            .load(
                &mut Rows {
                    maximum,
                    ..Rows::default()
                },
                &mut Definitions::default(),
            )
            .unwrap();
        assert!(manager.inventory().loaded && manager.inventory().templates.is_empty());
        assert_eq!(
            manager.max_pool_id().unwrap(),
            u32::from(maximum.unwrap_or(0))
        );
        manager.initialize(&mut states, id, &mut work).unwrap();
        assert_eq!(
            manager
                .load(&mut Rows::default(), &mut Definitions::default())
                .unwrap_err()
                .id,
            "execution.pool.loader_repeated"
        );
    }
    let mut input = rows(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    let mut owner = Definitions {
        fail: true,
        ..Definitions::default()
    };
    assert_eq!(
        SourcePoolManager::default()
            .load(&mut input, &mut owner)
            .unwrap_err()
            .id,
        "authored.effective_owner"
    );
}
#[test]
fn exclusions_remove_auto_spawn_observer_and_state_refusals_do_not_synthesize_effects() {
    let mut manager = loaded(&[(1, 1)], &[(4, 7, 1, 100.0)]);
    manager
        .set_excluded(PoolKind::GameObject, 1, 7, true)
        .unwrap();
    manager.remove_auto_spawn(1).unwrap();
    let (mut states, id) = state(false);
    let mut work = Work::default();
    manager.initialize(&mut states, id, &mut work).unwrap();
    assert_eq!(work.accesses, 0);
    manager
        .spawn_pool(&mut states, id, 1, true, &mut work)
        .unwrap();
    assert!(work.effects.is_empty());
    let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    work.fail_observe = true;
    assert_eq!(
        manager
            .spawn_pool(&mut states, id, 1, true, &mut work)
            .unwrap_err()
            .id,
        "authored.observer"
    );
    assert!(states
        .state(id)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    assert_eq!(
        manager
            .spawn_pool(
                &mut states,
                PersistentStateIdentity {
                    generation: id.generation + 1,
                    ..id
                },
                1,
                true,
                &mut Work::default()
            )
            .unwrap_err()
            .id,
        "execution.persistent.stale_state"
    );
}

#[test]
fn public_chance_checks_default_holes_and_missing_templates_keep_source_results() {
    let manager = loaded(&[(1, 1), (2, 2)], &[(2, 7, 1, 25.0), (4, 8, 2, 25.0)]);
    assert!(manager.check_pool(0).unwrap());
    assert!(!manager.check_pool(1).unwrap());
    assert!(manager.check_pool(2).unwrap());
    assert!(!manager.check_pool(3).unwrap());
    assert_eq!(manager.pool_template(1).unwrap().limit, 1);
    assert_eq!(
        manager.pool_template(3).unwrap_err().id,
        "execution.pool.template_index_undefined"
    );
    assert_eq!(manager.part_of(PoolKind::Creature, 99).unwrap(), 0);
    assert_eq!(manager.top_pool(PoolKind::Creature, 99).unwrap(), 0);
}

#[test]
fn initialization_does_not_spawn_a_pool_associated_with_another_world_map() {
    let members = [(2, 7, 1, 0.0)];
    let mut owner = definitions(&members);
    owner.spawns.insert((PoolKind::Creature, 7), 1);
    let mut manager = SourcePoolManager::default();
    manager
        .load(&mut rows(&[(1, 1)], &members), &mut owner)
        .unwrap();
    let (mut states, id) = state(false);
    let mut work = Work::default();
    manager.initialize(&mut states, id, &mut work).unwrap();
    assert_eq!(work.accesses, 0);
    assert!(work.effects.is_empty());
    assert_eq!(states.state(id).unwrap().pools().count(1), 0);
}

#[test]
fn unselected_trigger_is_cleared_before_roll_and_two_states_never_share_pool_selection() {
    let members = [(2, 7, 1, 0.0)];
    let mut owner = definitions(&members);
    owner.maps.insert(33);
    owner.spawns.insert((PoolKind::Creature, 7), 33);
    let mut manager = SourcePoolManager::default();
    manager
        .load(&mut rows(&[(1, 1)], &members), &mut owner)
        .unwrap();
    let mut states = SourceManagers::default();
    let entry = PersistentMapEntry {
        id: 33,
        map_type: 1,
    };
    let first_id = states
        .create_state(entry, 1, 500, true, true, 0, &mut Effects)
        .unwrap();
    let second_id = states
        .create_state(entry, 2, 500, true, true, 0, &mut Effects)
        .unwrap();
    let mut work = Work::default();
    manager
        .update_pool(&mut states, first_id, 1, PoolKind::Creature, 99, &mut work)
        .unwrap();
    assert_eq!(
        work.effects[0].action,
        PoolEffectAction::Spawn { instantly: false }
    );
    assert!(work.operations.iter().any(|v| matches!(
        v,
        PoolOperation::Roll {
            trigger: 0,
            guid: Some(7),
            ..
        }
    )));
    assert!(states
        .state(first_id)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    assert!(!states
        .state(second_id)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    manager
        .spawn_pool(&mut states, second_id, 1, true, &mut work)
        .unwrap();
    assert_eq!(states.state(second_id).unwrap().pools().count(1), 1);
}

#[test]
fn real_rng_cursor_failure_never_becomes_a_missing_or_selected_object() {
    for chance in [0.0, 100.0] {
        let mut manager = loaded(&[(1, 1)], &[(2, 7, 1, chance)]);
        let (mut states, id) = state(false);
        let mut work = Work::default();
        work.cursor.next_span = u64::MAX;
        assert_eq!(
            manager
                .spawn_pool(&mut states, id, 1, true, &mut work)
                .unwrap_err()
                .id,
            "authored.rng"
        );
        assert_eq!(work.cursor.raw, 0);
        assert!(work.effects.is_empty());
        assert!(!states
            .state(id)
            .unwrap()
            .pools()
            .contains(PoolMember::Creature(7)));
    }
}

#[test]
fn inventory_version_and_foreign_pool_references_are_not_checkpoint_certificates() {
    let manager = loaded(&[(1, 1)], &[(2, 7, 1, 0.0)]);
    let mut inventory = manager.inventory();
    inventory.require_quiescent().unwrap();
    inventory.version = 2;
    assert_eq!(
        inventory.require_quiescent().unwrap_err().id,
        "execution.pool.nonquiescent"
    );
    let mut inventory = manager.inventory();
    inventory.membership[2].insert(7, 65535);
    assert_eq!(
        inventory.require_quiescent().unwrap_err().id,
        "execution.pool.inventory"
    );
}

#[test]
fn oversized_loader_occurrences_refuse_instead_of_clipping_or_resetting() {
    struct Bulk {
        remaining: u64,
        ordinal: u64,
    }
    impl PoolRows for Bulk {
        fn maximum(&mut self) -> Result<Option<u16>> {
            Ok(Some(1))
        }
        fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
            if self.remaining == 0 {
                return Ok(None);
            }
            self.remaining -= 1;
            let ordinal = self.ordinal;
            self.ordinal += 1;
            Ok(Some(PoolTemplateRow {
                ordinal,
                id: 1,
                limit: 1,
                description: vec![],
            }))
        }
        fn member(&mut self, _: usize) -> Result<Option<PoolMembershipRow>> {
            panic!("budget must refuse before membership queries")
        }
    }
    let mut manager = SourcePoolManager::default();
    let mut input = Bulk {
        remaining: 2_000_001,
        ordinal: 0,
    };
    assert_eq!(
        manager
            .load(&mut input, &mut Definitions::default())
            .unwrap_err()
            .id,
        "execution.pool.row_budget"
    );
    assert_eq!(input.remaining, 0);
    let (mut states, id) = state(false);
    assert_eq!(
        manager
            .initialize(&mut states, id, &mut Work::default())
            .unwrap_err()
            .id,
        "execution.pool.stopped"
    );
}
