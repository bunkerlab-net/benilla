// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored executable owners, not captured group-selection answers.
use super::*;
use benilla_sim::resident_map::CellPair;
use benilla_sim::source_spawn_groups::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Default)]
pub(super) struct Rows {
    phases: BTreeMap<usize, VecDeque<GroupInput>>,
}
impl Rows {
    pub(super) fn push(&mut self, phase: GroupPhase, data: GroupInputData) {
        let rows = self.phases.entry(phase as usize).or_default();
        rows.push_back(GroupInput {
            ordinal: rows.len() as u64,
            data,
        });
    }
}
impl GroupRows for Rows {
    fn next(&mut self, phase: GroupPhase) -> Result<Option<GroupInput>> {
        Ok(self.phases.entry(phase as usize).or_default().pop_front())
    }
}
pub(super) fn template(id: u32, kind: u32, max_count: u32) -> GroupTemplate {
    GroupTemplate {
        id,
        name: b"authored".to_vec(),
        kind,
        max_count,
        condition: 0,
        expression: 0,
        flags: 0,
        string_id: 0,
        respawn_min: None,
        respawn_max: None,
    }
}
pub(super) fn spawn(entry: u32) -> GroupSpawnDefinition {
    GroupSpawnDefinition {
        map: 0,
        cell: CellPair::from_indices(512, 512).unwrap(),
        position_bits: [0; 3],
        entry,
        spawn_mask: 1,
        game_event: 0,
        guid_pool: 0,
        entry_pool: 0,
    }
}
#[derive(Default)]
pub(super) struct Owner {
    pub(super) spawns: BTreeMap<SpawnKey, GroupSpawnDefinition>,
    templates: BTreeSet<(GridSpawnKind, u32)>,
    static_spawns: BTreeSet<SpawnKey>,
    dynamic_spawns: Vec<SpawnKey>,
    conditions: BTreeSet<i32>,
    expressions: BTreeSet<i32>,
    strings: BTreeSet<u32>,
    active: BTreeMap<u32, u32>,
    random: BTreeSet<SpawnKey>,
    autoload: bool,
    vanishing: bool,
}
impl GroupLoadOwner for Owner {
    fn condition_exists(&mut self, id: i32) -> Result<bool> {
        Ok(self.conditions.contains(&id))
    }
    fn expression_exists(&mut self, id: i32) -> Result<bool> {
        Ok(self.expressions.contains(&id))
    }
    fn string_exists(&mut self, id: u32) -> Result<bool> {
        Ok(self.strings.contains(&id))
    }
    fn spawn(&mut self, key: SpawnKey) -> Result<Option<GroupSpawnDefinition>> {
        Ok(if self.vanishing {
            self.spawns.remove(&key)
        } else {
            self.spawns.get(&key).copied()
        })
    }
    fn template_exists(&mut self, kind: GridSpawnKind, entry: u32) -> Result<bool> {
        Ok(self.templates.contains(&(kind, entry)))
    }
    fn autoload_active(&mut self) -> Result<bool> {
        Ok(self.autoload)
    }
    fn creature_template_flags(&mut self, entry: u32) -> Result<Option<u32>> {
        Ok(self.active.get(&entry).copied())
    }
    fn has_random_entries(&mut self, key: SpawnKey) -> Result<bool> {
        Ok(self.random.contains(&key))
    }
    fn detach_spawn(&mut self, key: SpawnKey, _: GroupSpawnDefinition) -> Result<()> {
        self.static_spawns.remove(&key);
        self.dynamic_spawns.retain(|v| *v != key);
        Ok(())
    }
}
#[test]
fn effective_pool_event_exclusions_and_duplicate_domains_precede_final_membership() {
    // ObjectMgr.cpp:1275-1323 uses source spawn metadata, not active-event state.
    let mut rows = Rows::default();
    for (id, kind) in [(1, 0), (2, 1), (3, 0)] {
        rows.push(
            GroupPhase::Templates,
            GroupInputData::Template(template(id, kind, 0)),
        );
    }
    for (id, guid) in [(1, 7), (1, 8), (1, 9), (2, 7), (3, 7)] {
        rows.push(
            GroupPhase::Members,
            GroupInputData::Member {
                group: id,
                guid,
                slot: -1,
                chance: 0,
            },
        );
    }
    let mut owner = Owner::default();
    for (kind, guid) in [
        (GridSpawnKind::Creature, 7),
        (GridSpawnKind::Creature, 8),
        (GridSpawnKind::Creature, 9),
        (GridSpawnKind::GameObject, 7),
    ] {
        let key = SpawnKey { kind, guid };
        let mut data = spawn(10);
        if guid == 8 {
            data.guid_pool = 1;
        }
        if guid == 9 {
            data.game_event = -2;
        }
        owner.spawns.insert(key, data);
        owner.static_spawns.insert(key);
        owner.dynamic_spawns.push(key);
    }
    let mut groups = SourceSpawnGroups::default();
    groups.load(&mut rows, &mut owner).unwrap();
    let definitions = groups.definitions().unwrap();
    assert_eq!(
        definitions[&1]
            .members
            .iter()
            .map(|v| v.guid)
            .collect::<Vec<_>>(),
        [7]
    );
    assert_eq!(
        definitions[&2]
            .members
            .iter()
            .map(|v| v.guid)
            .collect::<Vec<_>>(),
        [7]
    );
    assert!(definitions[&3].members.is_empty());
    assert_eq!(owner.static_spawns.len(), 2);
    assert_eq!(owner.dynamic_spawns.len(), 2);
    assert_eq!(
        groups
            .load_records()
            .iter()
            .filter(|v| v.decision == GroupLoadDecision::PoolOrEvent)
            .count(),
        2
    );
    assert_eq!(
        groups
            .load_records()
            .iter()
            .filter(|v| v.decision == GroupLoadDecision::DuplicateMember)
            .count(),
        1
    );
}

type Constructor<'a> =
    dyn FnMut(&mut SourceSpawnGroups, &mut SourceManagers, GroupSpawnRequest) -> Result<bool> + 'a;
pub(super) struct Work<'a> {
    engine: benilla_sim::source_rng::Mt19937,
    pub(super) cursor: benilla_sim::source_rng::DrawCursor,
    pub(super) operations: Vec<GroupOperation>,
    pub(super) persistent_operations: Vec<PersistentOperation>,
    pub(super) sql: Vec<PersistentSql>,
    pub(super) definitions: BTreeMap<SpawnKey, GroupSpawnDefinition>,
    pub(super) conditions: BTreeMap<(bool, i32), i32>,
    random: BTreeMap<SpawnKey, Vec<u32>>,
    pub(super) clock: i64,
    pub(super) wall: i64,
    pub(super) world: i64,
    pub(super) loaded: bool,
    pub(super) constructor: Option<&'a mut Constructor<'a>>,
    removed: BTreeMap<ObjectHandle, GroupRemovedObject>,
    live: BTreeMap<SpawnKey, ObjectHandle>,
    pub(super) fail_sql: bool,
    pub(super) fail_observe: bool,
    wall_kind: Option<ClockKind>,
    callbacks: Vec<GroupCallback>,
    allow_callbacks: bool,
    instance: bool,
    wall_calls: Vec<GroupClockCall>,
    loaded_calls: Vec<[u32; 3]>,
}
impl Default for Work<'_> {
    fn default() -> Self {
        Self {
            engine: Default::default(),
            cursor: Default::default(),
            operations: vec![],
            persistent_operations: vec![],
            sql: vec![],
            definitions: BTreeMap::new(),
            conditions: BTreeMap::new(),
            random: BTreeMap::new(),
            clock: 100_000,
            wall: 100,
            world: 100,
            loaded: false,
            constructor: None,
            removed: BTreeMap::new(),
            live: BTreeMap::new(),
            fail_sql: false,
            fail_observe: false,
            wall_kind: None,
            callbacks: vec![],
            allow_callbacks: false,
            instance: false,
            wall_calls: vec![],
            loaded_calls: vec![],
        }
    }
}
fn random_error(_: benilla_sim::source_rng::Error) -> Refusal {
    refuse("authored.group_rng")
}
impl GroupRandom for Work<'_> {
    type Error = Refusal;
    fn urand(&mut self, min: u32, max: u32) -> Result<u32> {
        let mut sink = |_| {};
        benilla_sim::source_rng::SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .map_err(random_error)?
            .urand(min, max)
            .map_err(random_error)
    }
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<()> {
        let mut sink = |_| {};
        benilla_sim::source_rng::SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .map_err(random_error)?
            .shuffle(indices)
            .map_err(random_error)
    }
}
impl PersistentEffects for Work<'_> {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        self.sql.push(operation);
        require(!self.fail_sql, "authored.group_sql")
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("authored.group_query_unreached"))
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.persistent_operations.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("authored.group_reset_unreached"))
    }
}
impl GroupWork for Work<'_> {
    fn thread(&self) -> u64 {
        1
    }
    fn wall_seconds(&mut self, call: GroupClockCall) -> Result<ClockSample> {
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
        Ok(self.world)
    }
    fn condition(
        &mut self,
        _: &mut SourceSpawnGroups,
        _: &mut SourceManagers,
        _: GroupIdentity,
        id: i32,
        expression: bool,
    ) -> Result<bool> {
        self.conditions
            .get(&(expression, id))
            .map(|v| *v != 0)
            .ok_or_else(|| refuse("authored.group_condition_absent"))
    }
    fn roll_chance_i(&mut self, chance: i32) -> Result<bool> {
        let mut sink = |_| {};
        benilla_sim::source_rng::SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .map_err(random_error)?
            .roll_chance_i(chance)
            .map_err(random_error)
    }
    fn random_entry(&mut self, key: SpawnKey) -> Result<u32> {
        let entries = self
            .random
            .get(&key)
            .ok_or_else(|| refuse("authored.group_random_entries"))?;
        require(!entries.is_empty(), "authored.group_random_entries")?;
        let mut sink = |_| {};
        let index = benilla_sim::source_rng::SourceRandom::new(
            1,
            &mut self.engine,
            &mut self.cursor,
            &mut sink,
        )
        .map_err(random_error)?
        .irand(0, entries.len() as i32 - 1)
        .map_err(random_error)?;
        entries
            .get(index as usize)
            .copied()
            .ok_or_else(|| refuse("authored.group_random_index"))
    }
    fn spawn_definition(&mut self, key: SpawnKey) -> Result<Option<GroupSpawnDefinition>> {
        Ok(self.definitions.get(&key).copied())
    }
    fn is_loaded(&mut self, _: MapIdentity, position: [u32; 3]) -> Result<bool> {
        self.loaded_calls.push(position);
        Ok(self.loaded)
    }
    fn construct(
        &mut self,
        groups: &mut SourceSpawnGroups,
        states: &mut SourceManagers,
        request: GroupSpawnRequest,
    ) -> Result<bool> {
        self.constructor
            .as_mut()
            .ok_or_else(|| refuse("execution.group.constructor_owner"))?(
            groups, states, request
        )
    }
    fn live_object(&mut self, _: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>> {
        Ok(self.live.get(&key).copied())
    }
    fn removed_object(&mut self, object: ObjectHandle) -> Result<GroupRemovedObject> {
        self.removed
            .get(&object)
            .copied()
            .ok_or_else(|| refuse("authored.group_removed_object"))
    }
    fn has_instance(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.instance)
    }
    fn callback(
        &mut self,
        _: &mut SourceSpawnGroups,
        _: &mut SourceManagers,
        _: GroupIdentity,
        callback: GroupCallback,
    ) -> Result<()> {
        require(self.allow_callbacks, "execution.group.callback_owner")?;
        self.callbacks.push(callback);
        Ok(())
    }
    fn group_operation(&mut self, operation: GroupOperation) -> Result<()> {
        self.operations.push(operation);
        require(!self.fail_observe, "authored.group_observer")
    }
}
pub(super) fn fixture<'a>(
    group_template: GroupTemplate,
    members: &[(u32, u32)],
    dungeon: bool,
    extra: impl FnOnce(&mut Rows, &mut Owner),
) -> (SourceSpawnGroups, SourceManagers, GroupIdentity, Work<'a>) {
    let mut rows = Rows::default();
    rows.push(
        GroupPhase::Templates,
        GroupInputData::Template(group_template.clone()),
    );
    let kind = if group_template.kind == 0 {
        GridSpawnKind::Creature
    } else {
        GridSpawnKind::GameObject
    };
    let map = MapIdentity {
        map: if dungeon { 33 } else { 0 },
        instance: if dungeon { 1 } else { 0 },
        creation_generation: 1,
    };
    let mut owner = Owner::default();
    for &(guid, chance) in members {
        rows.push(
            GroupPhase::Members,
            GroupInputData::Member {
                group: group_template.id,
                guid,
                slot: -1,
                chance,
            },
        );
        let mut data = spawn(10);
        data.map = map.map;
        owner.spawns.insert(SpawnKey { kind, guid }, data);
    }
    extra(&mut rows, &mut owner);
    let mut groups = SourceSpawnGroups::default();
    groups.load(&mut rows, &mut owner).unwrap();
    let mut states = SourceManagers::default();
    let mut work = Work {
        definitions: owner.spawns.clone(),
        allow_callbacks: true,
        ..Work::default()
    };
    let persistent = states
        .create_state(
            PersistentMapEntry {
                id: map.map,
                map_type: u32::from(dungeon),
            },
            map.instance,
            500,
            true,
            true,
            0,
            &mut work,
        )
        .unwrap();
    states.bind_map(persistent, Some(map), &mut work).unwrap();
    let group = groups
        .create(map, persistent, group_template.id, &mut states, &mut work)
        .unwrap();
    (groups, states, group, work)
}
#[test]
fn unloaded_selection_inserts_real_overlay_and_group_before_any_constructor_and_keeps_exact_shuffle(
) {
    // SpawnGroup.cpp:313,344,401-420; GCC 12.2 seed 5489 shuffles [7,8,9] to [7,9,8].
    let (mut groups, mut states, group, mut work) = fixture(
        template(1, 0, 2),
        &[(7, 0), (8, 0), (9, 0)],
        false,
        |_, _| {},
    );
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(
        groups.state(group).unwrap().objects,
        BTreeMap::from([(7, 10), (9, 10)])
    );
    assert_eq!(work.cursor.raw, 1);
    let persistent = groups.state(group).unwrap().persistent;
    assert_eq!(
        states
            .state_mut(persistent)
            .unwrap()
            .cell_guids(spawn(10).cell)
            .creatures
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [7, 9]
    );
    assert!(!work
        .operations
        .iter()
        .any(|o| matches!(o, GroupOperation::Constructed { .. })));
    assert_eq!(groups.guid_entry(group, 7).unwrap(), 10);
    assert_eq!(groups.guid_entry(group, 8).unwrap(), 0);
    assert_eq!(groups.creature_group(group.map, 7).unwrap().unwrap().id, 1);
    assert!(groups.gameobject_group(group.map, 7).unwrap().is_none());
    groups.inventory().require_quiescent().unwrap();
    assert_eq!(
        groups.inventory().require_continuation().unwrap_err().id,
        "execution.managers.f7.continuation_owner"
    );
}
#[test]
fn condition_failure_despawns_before_full_count_and_condition_precedes_expression() {
    let mut input = template(1, 0, 1);
    input.condition = 1;
    input.expression = 2;
    input.flags = 8;
    let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |_, o| {
        o.conditions.insert(1);
        o.expressions.insert(2);
    });
    work.conditions.insert((false, 1), 1);
    work.conditions.insert((true, 2), 0);
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(groups.state(group).unwrap().objects.len(), 1);
    work.conditions.insert((false, 1), 0);
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert!(groups.state(group).unwrap().objects.is_empty());
    assert!(!work.operations.iter().any(|o| matches!(
        o,
        GroupOperation::Condition {
            expression: true,
            ..
        }
    )));
    let persistent = groups.state(group).unwrap().persistent;
    assert_eq!(
        states
            .state(persistent)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        101
    );
    assert!(states
        .state_mut(persistent)
        .unwrap()
        .cell_guids(spawn(10).cell)
        .creatures
        .is_empty());
}
#[test]
fn false_chance_cache_counts_toward_the_early_stop_without_new_attempts() {
    // SpawnGroup.cpp:181,318-339 counts cached false entries, not only successful selections.
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 1)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(
        groups.state(group).unwrap().chosen_spawns.get(&7),
        Some(&false)
    );
    assert!(groups.state(group).unwrap().objects.is_empty());
    assert_eq!(work.cursor.raw, 1);
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(work.cursor.raw, 1);
    assert_eq!(
        work.operations
            .iter()
            .filter(|o| matches!(o, GroupOperation::Chance { .. }))
            .count(),
        1
    );
}
#[test]
fn maximum_one_is_blocked_by_any_future_member_until_ignore_respawn_saves_now() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0), (8, 0)], false, |_, _| {});
    let persistent = groups.state(group).unwrap().persistent;
    // Fresh wall time and the world snapshot have independent owners.
    work.world = 99;
    states
        .save_respawn(
            persistent,
            GridSpawnKind::Creature,
            8,
            101,
            work.world,
            &mut work,
        )
        .unwrap();
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert!(groups.state(group).unwrap().objects.is_empty());
    assert_eq!(work.cursor.raw, 0);
    groups
        .spawn(group, &mut states, false, true, &mut work)
        .unwrap();
    assert_eq!(
        states
            .state(persistent)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 8),
        100
    );
    assert_eq!(groups.state(group).unwrap().objects.len(), 1);
}
#[test]
fn squads_preserve_first_insert_entries_and_can_exceed_maximum_without_guid_shuffle() {
    // SpawnGroup.cpp:278-289,369-398 does not apply the non-squad resize or existing-object filter.
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 1, 1), &[(7, 0), (8, 0)], false, |rows, _| {
            for (guid, entry) in [(7, 11), (7, 99), (8, 12)] {
                rows.push(
                    GroupPhase::Squads,
                    GroupInputData::Squad {
                        group: 1,
                        squad: 5,
                        guid,
                        entry,
                    },
                );
            }
        });
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(
        groups.state(group).unwrap().objects,
        BTreeMap::from([(7, 11), (8, 12)])
    );
    assert_eq!(groups.state(group).unwrap().chosen_squad, 0);
    assert_eq!(work.cursor.raw, 1);
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(work.cursor.raw, 1);
}

fn key(guid: u32) -> SpawnKey {
    SpawnKey {
        kind: GridSpawnKind::Creature,
        guid,
    }
}
fn object() -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: 1,
    }
}
fn formation(group: u32) -> Formation {
    Formation {
        group,
        kind: 0,
        spread_bits: 1_f32.to_bits(),
        options: 0,
        path: 0,
        movement: 0,
        comment: vec![],
    }
}

#[test]
fn loader_validates_templates_formations_entries_links_and_corrects_only_nonnegative_slots() {
    // ObjectMgr.cpp:1130-1262,1326-1455.
    let mut rows = Rows::default();
    let mut owner = Owner::default();
    let mut empty = template(1, 0, 0);
    empty.name.clear();
    let mut bad_kind = template(2, 9, 0);
    let mut missing_condition = template(3, 0, 0);
    missing_condition.condition = 7;
    let mut missing_expression = template(4, 0, 0);
    missing_expression.expression = 8;
    let mut cleared_string = template(5, 0, 0);
    cleared_string.string_id = 9;
    for input in [
        empty,
        bad_kind.clone(),
        missing_condition,
        missing_expression,
        cleared_string,
    ] {
        rows.push(GroupPhase::Templates, GroupInputData::Template(input));
    }
    bad_kind.id = 5;
    bad_kind.kind = 1;
    rows.push(GroupPhase::Templates, GroupInputData::Template(bad_kind));
    for input in [
        formation(99),
        Formation {
            kind: 7,
            ..formation(5)
        },
        Formation {
            movement: 5,
            ..formation(5)
        },
        Formation {
            spread_bits: 16_f32.to_bits(),
            ..formation(5)
        },
        formation(5),
    ] {
        rows.push(GroupPhase::Formations, GroupInputData::Formation(input));
    }
    for (group, guid, slot) in [(99, 1, 0), (5, 1, -1), (5, 2, 8), (5, 3, 3), (5, 4, 0)] {
        rows.push(
            GroupPhase::Members,
            GroupInputData::Member {
                group,
                guid,
                slot,
                chance: 0,
            },
        );
        if guid != 4 {
            owner.spawns.insert(key(guid), spawn(10));
        }
    }
    for (group, entry) in [(99, 10), (5, 99), (5, 10)] {
        rows.push(
            GroupPhase::Entries,
            GroupInputData::Entry {
                group,
                entry: RandomEntry {
                    entry,
                    min: 0,
                    max: 2,
                    chance: 100,
                },
            },
        );
    }
    owner.templates.insert((GridSpawnKind::Creature, 10));
    for (group, linked) in [(99, 5), (5, 99), (5, 5)] {
        rows.push(GroupPhase::Links, GroupInputData::Link { group, linked });
    }
    rows.push(
        GroupPhase::Squads,
        GroupInputData::Squad {
            group: 99,
            squad: 1,
            guid: 1,
            entry: 10,
        },
    );
    let mut groups = SourceSpawnGroups::default();
    groups.load(&mut rows, &mut owner).unwrap();
    let definition = &groups.definitions().unwrap()[&5];
    assert_eq!(definition.template.kind, 0);
    assert_eq!(definition.template.string_id, 0);
    assert_eq!(
        definition
            .members
            .iter()
            .map(|m| (m.guid, m.slot))
            .collect::<Vec<_>>(),
        [(1, -1), (3, 0), (2, 1)]
    );
    assert_eq!(definition.linked, [5]);
    assert_eq!(definition.entries.len(), 1);
    for decision in [
        GroupLoadDecision::EmptyName,
        GroupLoadDecision::InvalidType,
        GroupLoadDecision::MissingCondition,
        GroupLoadDecision::MissingExpression,
        GroupLoadDecision::StringCleared,
        GroupLoadDecision::FormationType,
        GroupLoadDecision::MovementType,
        GroupLoadDecision::FormationSpread,
        GroupLoadDecision::MissingGroup,
        GroupLoadDecision::MissingSpawn,
        GroupLoadDecision::MissingTemplate,
        GroupLoadDecision::MissingLinkedGroup,
        GroupLoadDecision::SlotCorrected,
    ] {
        assert!(
            groups.load_records().iter().any(|r| r.decision == decision),
            "{decision:?}"
        );
    }
}

#[test]
fn autoload_uses_creature_templates_even_for_gameobjects_and_any_zero_spawn_mask_disables_group() {
    // ObjectMgr.cpp:1532-1548 is deliberately not repaired to query GO templates.
    let mut rows = Rows::default();
    let mut owner = Owner {
        autoload: true,
        ..Owner::default()
    };
    rows.push(
        GroupPhase::Templates,
        GroupInputData::Template(template(1, 1, 0)),
    );
    for guid in [7, 8] {
        let key = SpawnKey {
            kind: GridSpawnKind::GameObject,
            guid,
        };
        let mut data = spawn(0);
        if guid == 8 {
            data.spawn_mask = 0;
        }
        owner.spawns.insert(key, data);
        rows.push(
            GroupPhase::Members,
            GroupInputData::Member {
                group: 1,
                guid,
                slot: -1,
                chance: 0,
            },
        );
    }
    rows.push(
        GroupPhase::Entries,
        GroupInputData::Entry {
            group: 1,
            entry: RandomEntry {
                entry: 11,
                min: 0,
                max: 0,
                chance: 0,
            },
        },
    );
    owner.templates.insert((GridSpawnKind::GameObject, 11));
    owner.active.insert(11, 0x1000);
    let mut groups = SourceSpawnGroups::default();
    groups.load(&mut rows, &mut owner).unwrap();
    let definition = &groups.definitions().unwrap()[&1];
    assert!(definition.active);
    assert!(!definition.enabled);
    assert_eq!(definition.template.max_count, 2);
}

#[test]
fn loaded_constructor_sees_selection_and_overlay_and_failure_keeps_membership() {
    // SpawnGroup.cpp:406-420 records selection before calling the real F3 constructor.
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    let mut construct = |groups: &mut SourceSpawnGroups,
                         states: &mut SourceManagers,
                         request: GroupSpawnRequest| {
        assert_eq!(request.entry, 10);
        assert_eq!(groups.guid_entry(group, 7)?, 10);
        let persistent = groups.state(group)?.persistent;
        assert!(states
            .state_mut(persistent)?
            .cell_guids(spawn(10).cell)
            .creatures
            .contains(&7));
        Ok(false)
    };
    work.constructor = Some(&mut construct);
    work.loaded = true;
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(groups.state(group).unwrap().objects.len(), 1);
    assert_eq!(work.loaded_calls, [[0; 3]]);
    assert!(work
        .operations
        .iter()
        .any(|o| matches!(o, GroupOperation::Constructed { success: false, .. })));
}

#[test]
fn disabled_cooldown_condition_and_forced_paths_keep_the_source_early_return_order() {
    let mut input = template(1, 0, 1);
    input.expression = 2;
    let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |_, o| {
        o.expressions.insert(2);
    });
    groups.set_enabled(group, false, &mut work).unwrap();
    work.clock = -1;
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    groups
        .spawn(group, &mut states, true, false, &mut work)
        .unwrap();
    assert!(work.wall_calls.is_empty());
    assert!(work
        .operations
        .iter()
        .all(|o| !matches!(o, GroupOperation::Condition { .. })));
    work.clock = 0;
    work.conditions.insert((true, 2), 0);
    groups
        .spawn(group, &mut states, true, false, &mut work)
        .unwrap();
    assert!(work.wall_calls.is_empty());
    work.conditions.insert((true, 2), 1);
    let mut construct =
        |_: &mut SourceSpawnGroups, _: &mut SourceManagers, _: GroupSpawnRequest| Ok(true);
    work.constructor = Some(&mut construct);
    groups
        .spawn(group, &mut states, true, false, &mut work)
        .unwrap();
    assert_eq!(groups.state(group).unwrap().objects.len(), 1);
    assert!(work.loaded_calls.is_empty());
}

#[test]
fn cached_success_and_zero_chances_are_not_rerolled_and_larger_groups_filter_future_members() {
    let (mut groups, mut states, group, mut work) = fixture(
        template(1, 0, 3),
        &[(7, 100), (8, 0), (9, 100)],
        false,
        |_, _| {},
    );
    let persistent = groups.state(group).unwrap().persistent;
    states
        .save_respawn(persistent, GridSpawnKind::Creature, 9, 101, 100, &mut work)
        .unwrap();
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(
        groups.state(group).unwrap().chosen_spawns,
        BTreeMap::from([(7, true), (8, true)])
    );
    assert!(!groups.state(group).unwrap().objects.contains_key(&9));
    groups.despawn(group, &mut states, 0, 0, &mut work).unwrap();
    let draws = work.cursor.raw;
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(work.cursor.raw, draws + 1);
    assert!(work.operations.iter().any(|operation| matches!(
        operation,
        GroupOperation::Chance {
            result: true,
            cached: true,
            ..
        }
    )));
}

#[test]
fn dungeon_entries_pick_own_and_random_members_before_minima_and_do_not_persist_selection_to_sql() {
    let (mut groups, mut states, group, mut work) = fixture(
        template(1, 0, 3),
        &[(7, 0), (8, 0), (9, 0)],
        true,
        |rows, o| {
            o.spawns.get_mut(&key(8)).unwrap().entry = 0;
            o.random.insert(key(7));
            rows.push(
                GroupPhase::Entries,
                GroupInputData::Entry {
                    group: 1,
                    entry: RandomEntry {
                        entry: 20,
                        min: 1,
                        max: 1,
                        chance: 100,
                    },
                },
            );
            o.templates.insert((GridSpawnKind::Creature, 20));
        },
    );
    work.random.insert(key(7), vec![20]);
    work.sql.clear();
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    let state = groups.state(group).unwrap();
    assert_eq!(
        state.chosen_entries,
        BTreeMap::from([(7, 20), (8, 0), (9, 10)])
    );
    assert!(work.sql.is_empty());
    assert_eq!(state.objects, state.chosen_entries);
    groups.despawn(group, &mut states, 0, 0, &mut work).unwrap();
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(
        groups.state(group).unwrap().chosen_entries,
        BTreeMap::from([(7, 20), (8, 0), (9, 10)])
    );
}

#[test]
fn world_squads_reroll_while_dungeon_squads_reuse_the_first_index() {
    for dungeon in [false, true] {
        let (mut groups, mut states, group, mut work) =
            fixture(template(1, 0, 2), &[(7, 0)], dungeon, |rows, _| {
                for squad in [1, 2] {
                    rows.push(
                        GroupPhase::Squads,
                        GroupInputData::Squad {
                            group: 1,
                            squad,
                            guid: 7,
                            entry: 10 + squad,
                        },
                    );
                }
            });
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        assert_eq!(work.cursor.raw, 1);
        groups.despawn(group, &mut states, 0, 0, &mut work).unwrap();
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        assert_eq!(work.cursor.raw, if dungeon { 1 } else { 2 });
    }
}

#[test]
fn delayed_despawn_keeps_unloaded_selection_and_live_objects_receive_typed_callbacks() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 2), &[(7, 0), (8, 0)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    work.live.insert(key(7), object());
    groups
        .despawn(group, &mut states, 10, 5, &mut work)
        .unwrap();
    assert_eq!(groups.state(group).unwrap().objects.len(), 2);
    assert_eq!(
        work.callbacks,
        [GroupCallback::ForcedDespawn {
            object: object(),
            delay_millis: 10,
            forced_seconds: 5,
            only_alive: true
        }]
    );
    let persistent = groups.state(group).unwrap().persistent;
    assert_eq!(
        states
            .state(persistent)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 8),
        0
    );
    groups.despawn(group, &mut states, 0, 5, &mut work).unwrap();
    assert!(groups.state(group).unwrap().objects.is_empty());
    assert_eq!(
        states
            .state(persistent)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 8),
        105
    );
}

#[test]
fn last_member_removal_sets_cooldown_resets_chance_cache_and_aligns_other_deadlines() {
    // SpawnGroup.cpp:74-100 then 514-522; only the world-map last removal resets caches.
    let mut input = template(1, 0, 1);
    input.respawn_min = Some(5);
    let (mut groups, mut states, group, mut work) =
        fixture(input, &[(7, 100), (8, 100)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    let guid = *groups.state(group).unwrap().objects.keys().next().unwrap();
    let persistent = groups.state(group).unwrap().persistent;
    states
        .save_respawn(
            persistent,
            GridSpawnKind::Creature,
            guid,
            150,
            100,
            &mut work,
        )
        .unwrap();
    work.removed.insert(
        object(),
        GroupRemovedObject {
            key: key(guid),
            respawn_delay: 9,
        },
    );
    work.instance = true;
    groups
        .remove_object(group, &mut states, object(), &mut work)
        .unwrap();
    assert_eq!(groups.state(group).unwrap().cooldown_millis, 105_000);
    assert!(groups.state(group).unwrap().chosen_spawns.is_empty());
    for other in [7, 8] {
        assert_eq!(
            states
                .state(persistent)
                .unwrap()
                .respawn(GridSpawnKind::Creature, other),
            150
        );
    }
    assert_eq!(
        work.callbacks,
        [GroupCallback::LastCreatureDespawn { object: object() }]
    );
    let draws = work.cursor.raw;
    work.clock = 104_999;
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(work.cursor.raw, draws);
}

#[test]
fn dungeon_removal_retains_choices_and_world_squad_fallback_uses_removed_respawn_delay() {
    for dungeon in [false, true] {
        let (mut groups, mut states, group, mut work) =
            fixture(template(1, 0, 1), &[(7, 0)], dungeon, |rows, _| {
                rows.push(
                    GroupPhase::Squads,
                    GroupInputData::Squad {
                        group: 1,
                        squad: 1,
                        guid: 7,
                        entry: 10,
                    },
                );
            });
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        work.removed.insert(
            object(),
            GroupRemovedObject {
                key: key(7),
                respawn_delay: 9,
            },
        );
        groups
            .remove_object(group, &mut states, object(), &mut work)
            .unwrap();
        let state = groups.state(group).unwrap();
        assert_eq!(state.cooldown_millis, if dungeon { 0 } else { 109_000 });
        assert_eq!(state.chosen_squad, if dungeon { 0 } else { -1 });
        assert_eq!(state.chosen_entries.len(), usize::from(dungeon));
    }
}

#[test]
fn vicinity_ignores_enabled_state_resets_every_member_and_samples_clock_before_range_failure() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 2), &[(7, 0), (8, 0)], false, |_, _| {});
    groups.set_enabled(group, false, &mut work).unwrap();
    let persistent = groups.state(group).unwrap().persistent;
    for guid in [7, 8] {
        states
            .save_respawn(
                persistent,
                GridSpawnKind::Creature,
                guid,
                200,
                100,
                &mut work,
            )
            .unwrap();
    }
    groups
        .respawn_if_in_vicinity(group, &mut states, [0; 4], 0, &mut work)
        .unwrap();
    assert_eq!(
        states
            .state(persistent)
            .unwrap()
            .respawn(GridSpawnKind::Creature, 7),
        200
    );
    work.sql.clear();
    groups
        .respawn_if_in_vicinity(group, &mut states, [0; 4], 1_f32.to_bits(), &mut work)
        .unwrap();
    for guid in [7, 8] {
        assert_eq!(
            states
                .state(persistent)
                .unwrap()
                .respawn(GridSpawnKind::Creature, guid),
            0
        );
    }
    // MapPersistentStateMgr.cpp:89,138: equality erases memory and omits INSERT.
    assert_eq!(
        work.sql,
        [
            PersistentSql::Begin,
            PersistentSql::Delete {
                kind: GridSpawnKind::Creature,
                guid: 7,
                instance: 0
            },
            PersistentSql::Commit,
            PersistentSql::Begin,
            PersistentSql::Delete {
                kind: GridSpawnKind::Creature,
                guid: 8,
                instance: 0
            },
            PersistentSql::Commit,
        ]
    );
    assert_eq!(
        work.wall_calls,
        [GroupClockCall::Vicinity, GroupClockCall::Vicinity]
    );
    assert!(groups.state(group).unwrap().objects.is_empty());
    groups
        .clear_respawn_times(group, &mut states, &mut work)
        .unwrap();
    assert_eq!(work.wall_calls.last(), Some(&GroupClockCall::ClearRespawns));
}

#[test]
fn formation_callbacks_execute_in_create_update_last_remove_order() {
    // SpawnGroup.cpp:504-522,610-615.
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |rows, _| {
            rows.push(
                GroupPhase::Formations,
                GroupInputData::Formation(formation(1)),
            );
        });
    groups.update(group, &mut states, &mut work).unwrap();
    work.instance = true;
    work.removed.insert(
        object(),
        GroupRemovedObject {
            key: key(7),
            respawn_delay: 1,
        },
    );
    groups
        .remove_object(group, &mut states, object(), &mut work)
        .unwrap();
    assert_eq!(
        work.callbacks,
        [
            GroupCallback::FormationCreate,
            GroupCallback::FormationUpdate,
            GroupCallback::LastCreatureDespawn { object: object() },
            GroupCallback::ResetLastWaypoint
        ]
    );
    assert!(
        groups
            .creature_group(group.map, 7)
            .unwrap()
            .unwrap()
            .has_formation
    );
}

#[test]
fn gameobject_membership_removal_and_live_despawn_keep_their_type_domain() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 1, 1), &[(7, 0)], false, |_, _| {});
    let key = SpawnKey {
        kind: GridSpawnKind::GameObject,
        guid: 7,
    };
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    assert_eq!(
        groups.gameobject_group(group.map, 7).unwrap().unwrap().id,
        1
    );
    assert!(groups.creature_group(group.map, 7).unwrap().is_none());
    work.live.insert(key, object());
    groups.despawn(group, &mut states, 1, 0, &mut work).unwrap();
    assert_eq!(
        work.callbacks,
        [GroupCallback::ForcedDespawn {
            object: object(),
            delay_millis: 1,
            forced_seconds: 0,
            only_alive: false
        }]
    );
    work.removed.insert(
        object(),
        GroupRemovedObject {
            key,
            respawn_delay: 9,
        },
    );
    groups
        .remove_object(group, &mut states, object(), &mut work)
        .unwrap();
    let persistent = groups.state(group).unwrap().persistent;
    assert!(states
        .state_mut(persistent)
        .unwrap()
        .cell_guids(spawn(10).cell)
        .gameobjects
        .is_empty());
    assert_eq!(groups.state(group).unwrap().cooldown_millis, 0);
    assert!(!groups.is_respawn_overridden(group).unwrap());
    assert_eq!(
        groups
            .clear_respawn_times(group, &mut states, &mut work)
            .unwrap_err()
            .id,
        "execution.group.creature_group_required"
    );
}

#[test]
fn override_without_maximum_does_not_draw_but_removal_always_calls_urand_with_equal_bounds() {
    // SpawnGroup.cpp:80-81 versus :479-484.
    for maximum in [None, Some(5)] {
        let mut input = template(1, 0, 1);
        input.respawn_min = Some(5);
        input.respawn_max = maximum;
        let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |_, _| {});
        assert!(groups.is_respawn_overridden(group).unwrap());
        assert_eq!(groups.random_respawn_time(group, &mut work).unwrap(), 5);
        assert_eq!(work.cursor.raw, u64::from(maximum.is_some()));
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        work.removed.insert(
            object(),
            GroupRemovedObject {
                key: key(7),
                respawn_delay: 99,
            },
        );
        let before = work.cursor.raw;
        groups
            .remove_object(group, &mut states, object(), &mut work)
            .unwrap();
        assert_eq!(work.cursor.raw, before + 1);
        assert_eq!(groups.state(group).unwrap().cooldown_millis, 105_000);
    }
    let (groups, _, group, mut work) = fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    assert_eq!(
        groups.random_respawn_time(group, &mut work).unwrap_err().id,
        "execution.group.respawn_override_absent_undefined"
    );
}

#[test]
fn active_members_construct_without_loaded_check_and_missing_constructor_is_not_false() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |rows, o| {
            rows.push(
                GroupPhase::Entries,
                GroupInputData::Entry {
                    group: 1,
                    entry: RandomEntry {
                        entry: 10,
                        min: 0,
                        max: 1,
                        chance: 100,
                    },
                },
            );
            o.templates.insert((GridSpawnKind::Creature, 10));
            o.autoload = true;
            o.active.insert(10, 0x1000);
        });
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "execution.group.constructor_owner"
    );
    assert!(groups.state(group).unwrap().objects.contains_key(&7));
    assert!(work.loaded_calls.is_empty());
    assert!(groups.inventory().stopped);
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "execution.group.stopped"
    );
}

#[test]
fn missing_condition_callback_clock_definition_and_sql_owners_stop_mutating_execution() {
    let mut input = template(1, 0, 1);
    input.condition = 1;
    let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |_, o| {
        o.conditions.insert(1);
    });
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "authored.group_condition_absent"
    );
    assert!(groups.inventory().stopped);
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    work.wall_kind = Some(ClockKind::RelativeSteadyMilliseconds);
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "execution.group.clock_kind"
    );
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    work.definitions.clear();
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "execution.group.spawn_definition_absent"
    );
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    work.live.insert(key(7), object());
    work.allow_callbacks = false;
    assert_eq!(
        groups
            .despawn(group, &mut states, 0, 0, &mut work)
            .unwrap_err()
            .id,
        "execution.group.callback_owner"
    );
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    work.fail_sql = true;
    assert_eq!(
        groups
            .despawn(group, &mut states, 0, 5, &mut work)
            .unwrap_err()
            .id,
        "authored.group_sql"
    );
    assert!(groups.inventory().stopped);
    assert!(states.inventory().stopped);
    let (mut groups, _, group, mut work) = fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    work.fail_observe = true;
    assert_eq!(
        groups.set_enabled(group, false, &mut work).unwrap_err().id,
        "authored.group_observer"
    );
    assert!(!groups.state(group).unwrap().enabled);
    assert!(groups.inventory().stopped);
}

#[test]
fn source_undefined_deadline_cooldown_and_chance_arithmetic_refuse_without_repair() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    work.wall = i64::MAX;
    assert_eq!(
        groups
            .despawn(group, &mut states, 0, 1, &mut work)
            .unwrap_err()
            .id,
        "execution.group.deadline_add_undefined"
    );
    let mut input = template(1, 0, 1);
    input.respawn_min = Some(1);
    let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    work.clock = i64::MAX;
    work.removed.insert(
        object(),
        GroupRemovedObject {
            key: key(7),
            respawn_delay: 0,
        },
    );
    assert_eq!(
        groups
            .remove_object(group, &mut states, object(), &mut work)
            .unwrap_err()
            .id,
        "execution.group.cooldown_add_undefined"
    );
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |rows, o| {
            o.spawns.get_mut(&key(7)).unwrap().entry = 0;
            o.templates.insert((GridSpawnKind::Creature, 10));
            rows.push(
                GroupPhase::Entries,
                GroupInputData::Entry {
                    group: 1,
                    entry: RandomEntry {
                        entry: 10,
                        min: 0,
                        max: 1,
                        chance: 0x8000_0000,
                    },
                },
            );
        });
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "execution.group.chance_subtract_undefined"
    );
    assert!(groups.state(group).unwrap().objects.is_empty());
    assert_eq!(work.cursor.raw, 1);
}

#[test]
fn stale_identity_rebinding_duplicate_creation_and_missing_map_group_refuse() {
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    let persistent = groups.state(group).unwrap().persistent;
    let wrong = GroupIdentity {
        handle: SpawnGroupHandle {
            generation: 2,
            ..group.handle
        },
        ..group
    };
    assert_eq!(
        groups.state(wrong).unwrap_err().id,
        "execution.group.stale_group"
    );
    assert_eq!(
        groups
            .group_for(
                MapIdentity {
                    creation_generation: 2,
                    ..group.map
                },
                key(7)
            )
            .unwrap_err()
            .id,
        "execution.group.constructor_group_absent"
    );
    assert_eq!(
        groups
            .create(group.map, persistent, 1, &mut states, &mut work)
            .unwrap_err()
            .id,
        "execution.group.creation_repeated"
    );
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    let persistent = groups.state(group).unwrap().persistent;
    states
        .bind_map(
            persistent,
            Some(MapIdentity {
                creation_generation: 2,
                ..group.map
            }),
            &mut work,
        )
        .unwrap();
    assert_eq!(
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap_err()
            .id,
        "execution.group.persistent_map_binding"
    );
    let (mut groups, mut states, group, mut work) =
        fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    work.removed.insert(
        object(),
        GroupRemovedObject {
            key: SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: 7,
            },
            respawn_delay: 0,
        },
    );
    assert_eq!(
        groups
            .remove_object(group, &mut states, object(), &mut work)
            .unwrap_err()
            .id,
        "execution.group.removed_object_kind"
    );
    let mut groups = SourceSpawnGroups::default();
    assert_eq!(
        groups.dynamic_groups().unwrap_err().id,
        "execution.group.loader_owner"
    );
    groups
        .load(&mut Rows::default(), &mut Owner::default())
        .unwrap();
    assert_eq!(
        groups
            .create(group.map, persistent, 99, &mut states, &mut work)
            .unwrap_err()
            .id,
        "execution.group.definition_absent"
    );
}

#[test]
fn inventory_is_json_serializable_but_cannot_certify_restore_or_quiescence_during_callbacks() {
    let (groups, _, _, _) = fixture(template(1, 0, 1), &[(7, 0)], false, |_, _| {});
    let valid = groups.inventory();
    valid.require_quiescent().unwrap();
    serde_json::to_vec(&valid).unwrap();
    type Case = (fn(&mut SpawnGroupsInventory), &'static str);
    let cases: [Case; 5] = [
        (|s| s.version = 2, "execution.group.inventory_version"),
        (
            |s| s.definitions.clear(),
            "execution.group.inventory_definition",
        ),
        (
            |s| s.states[0].identity.map.creation_generation = 0,
            "execution.group.inventory_identity",
        ),
        (
            |s| s.states[0].chosen_squad = 0,
            "execution.group.inventory_squad",
        ),
        (
            |s| s.by_guid[0].0.guid = 99,
            "execution.group.inventory_membership",
        ),
    ];
    for (change, id) in cases {
        let mut state = valid.clone();
        change(&mut state);
        assert_eq!(state.validate().unwrap_err().id, id);
    }
    let mut held = valid.clone();
    held.frames.push(held.states[0].identity);
    assert_eq!(
        held.require_quiescent().unwrap_err().id,
        "execution.group.nonquiescent"
    );
    let mut constructing = valid.clone();
    constructing.states[0].constructing = true;
    assert_eq!(
        constructing.require_quiescent().unwrap_err().id,
        "execution.group.nonquiescent"
    );
    assert_eq!(
        valid.require_continuation().unwrap_err().id,
        "execution.managers.f7.continuation_owner"
    );
}

#[test]
fn loader_phase_repetition_final_definition_absence_and_budget_refuse_without_clipping() {
    let mut groups = SourceSpawnGroups::default();
    groups
        .load(&mut Rows::default(), &mut Owner::default())
        .unwrap();
    assert_eq!(
        groups
            .load(&mut Rows::default(), &mut Owner::default())
            .unwrap_err()
            .id,
        "execution.group.loader_repeated"
    );
    let mut rows = Rows::default();
    rows.push(
        GroupPhase::Templates,
        GroupInputData::Link {
            group: 1,
            linked: 2,
        },
    );
    assert_eq!(
        SourceSpawnGroups::default()
            .load(&mut rows, &mut Owner::default())
            .unwrap_err()
            .id,
        "execution.group.loader_phase"
    );
    let mut rows = Rows::default();
    rows.push(
        GroupPhase::Templates,
        GroupInputData::Template(template(1, 0, 1)),
    );
    rows.push(
        GroupPhase::Members,
        GroupInputData::Member {
            group: 1,
            guid: 7,
            slot: -1,
            chance: 0,
        },
    );
    let mut owner = Owner {
        vanishing: true,
        ..Owner::default()
    };
    owner.spawns.insert(key(7), spawn(10));
    assert_eq!(
        SourceSpawnGroups::default()
            .load(&mut rows, &mut owner)
            .unwrap_err()
            .id,
        "execution.group.final_spawn_absent"
    );
    struct Oversized {
        next: u64,
    }
    impl GroupRows for Oversized {
        fn next(&mut self, phase: GroupPhase) -> Result<Option<GroupInput>> {
            if phase != GroupPhase::Templates || self.next > 2_000_000 {
                return Ok(None);
            }
            let ordinal = self.next;
            self.next += 1;
            Ok(Some(GroupInput {
                ordinal,
                data: GroupInputData::Template(template(1, 0, 0)),
            }))
        }
    }
    let mut groups = SourceSpawnGroups::default();
    assert_eq!(
        groups
            .load(&mut Oversized { next: 0 }, &mut Owner::default())
            .unwrap_err()
            .id,
        "execution.group.row_budget"
    );
    assert!(groups.inventory().stopped);
    assert_eq!(groups.load_records().len(), 2_000_000);
}
