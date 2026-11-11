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
type CallbackHook = fn(
    &mut Work<'_>,
    &mut SourceSpawnGroups,
    &mut SourceManagers,
    GroupIdentity,
    GroupCallback,
) -> Result<()>;
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
    pub(super) removed: BTreeMap<ObjectHandle, GroupRemovedObject>,
    live: BTreeMap<SpawnKey, ObjectHandle>,
    pub(super) fail_sql: bool,
    pub(super) fail_observe: bool,
    wall_kind: Option<ClockKind>,
    callbacks: Vec<GroupCallback>,
    allow_callbacks: bool,
    instance: bool,
    wall_calls: Vec<GroupClockCall>,
    loaded_calls: Vec<[u32; 3]>,
    units: BTreeMap<ObjectHandle, GroupUnit>,
    pub(super) callback_hook: Option<CallbackHook>,
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
            units: BTreeMap::new(),
            callback_hook: None,
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
        groups: &mut SourceSpawnGroups,
        states: &mut SourceManagers,
        group: GroupIdentity,
        callback: GroupCallback,
    ) -> Result<()> {
        require(self.allow_callbacks, "execution.group.callback_owner")?;
        self.callbacks.push(callback);
        if let Some(hook) = self.callback_hook {
            return hook(self, groups, states, group, callback);
        }
        Ok(())
    }
    fn group_operation(&mut self, operation: GroupOperation) -> Result<()> {
        self.operations.push(operation);
        require(!self.fail_observe, "authored.group_observer")
    }
}
impl GroupEventOwner for Work<'_> {
    fn group_unit(&mut self, _: MapIdentity, object: ObjectHandle) -> Result<GroupUnit> {
        self.units
            .get(&object)
            .copied()
            .ok_or_else(|| refuse("authored.group_unit_absent"))
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
pub(super) fn formation(group: u32) -> Formation {
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
    let cases: [Case; 6] = [
        (|s| s.version = 2, "execution.group.inventory_version"),
        (|s| s.version = 4, "execution.group.inventory_version"),
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

fn unit(guid: u32) -> GroupUnit {
    GroupUnit {
        database_guid: guid,
        has_charmer: false,
        alive: true,
        in_combat: false,
        evading_home: false,
    }
}
fn live_member(work: &mut Work<'_>, guid: u32) -> ObjectHandle {
    let object = ObjectHandle {
        thread: 1,
        construction: u64::from(guid),
    };
    work.live.insert(key(guid), object);
    work.units.insert(object, unit(guid));
    work.removed.insert(
        object,
        GroupRemovedObject {
            key: key(guid),
            respawn_delay: 9,
        },
    );
    object
}
fn event_fixture(
    flags: u32,
) -> (
    SourceSpawnGroups,
    SourceManagers,
    GroupIdentity,
    Work<'static>,
) {
    let mut input = template(1, 0, 2);
    input.flags = flags;
    let (mut groups, mut states, group, mut work) =
        fixture(input, &[(7, 0), (8, 0)], false, |_, _| {});
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    live_member(&mut work, 7);
    live_member(&mut work, 8);
    work.units.insert(object(), unit(99));
    (groups, states, group, work)
}
#[test]
fn aggro_link_cycle_uses_actual_target_and_local_then_link_occurrence_order() {
    // SpawnGroup.cpp:537-560 guards Unit pointers per group, not the event or database GUID.
    let mut input = template(1, 0, 1);
    input.flags = 1;
    let (mut groups, mut states, first, mut work) =
        fixture(input, &[(7, 0)], false, |rows, owner| {
            let mut second = template(2, 0, 1);
            second.flags = 1;
            rows.push(GroupPhase::Templates, GroupInputData::Template(second));
            rows.push(
                GroupPhase::Members,
                GroupInputData::Member {
                    group: 2,
                    guid: 8,
                    slot: -1,
                    chance: 0,
                },
            );
            owner.spawns.insert(key(8), spawn(10));
            for (group, linked) in [(1, 2), (1, 2), (2, 1)] {
                rows.push(GroupPhase::Links, GroupInputData::Link { group, linked });
            }
        });
    let second = groups
        .create(
            first.map,
            groups.state(first).unwrap().persistent,
            2,
            &mut states,
            &mut work,
        )
        .unwrap();
    for group in [first, second] {
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
    }
    let a = live_member(&mut work, 7);
    let b = live_member(&mut work, 8);
    work.units.insert(object(), unit(99));
    let other = ObjectHandle {
        thread: 1,
        construction: 100,
    };
    work.units.insert(other, unit(99));
    work.callback_hook = Some(|work, groups, states, group, callback| {
        if let GroupCallback::EngageInCombat { target, .. } = callback {
            groups.event(group, states, GroupEvent::Aggro, Some(target), work)?;
            if target == object() && work.clock == 100_000 {
                work.clock = 0;
                groups.event(
                    group,
                    states,
                    GroupEvent::Aggro,
                    Some(ObjectHandle {
                        thread: 1,
                        construction: 100,
                    }),
                    work,
                )?;
            }
        }
        Ok(())
    });
    groups
        .event(
            first,
            &mut states,
            GroupEvent::Aggro,
            Some(object()),
            &mut work,
        )
        .unwrap();
    let engagements: Vec<_> = work
        .callbacks
        .iter()
        .filter_map(|c| match c {
            GroupCallback::EngageInCombat { object, target } => Some((*object, *target)),
            _ => None,
        })
        .collect();
    assert_eq!(
        engagements,
        [
            (a, object()),
            (a, other),
            (b, other),
            (b, other),
            (b, object()),
            (b, object())
        ]
    );
    assert!(groups.state(first).unwrap().linkage_targets.is_empty());
    assert!(groups.state(second).unwrap().linkage_targets.is_empty());
    groups.inventory().require_quiescent().unwrap();
}
#[test]
fn own_group_target_is_excluded_unless_charmed_and_null_or_disabled_aggro_skips_lookup() {
    // SpawnGroup.cpp:530-535 compares the target DB GUID even when a different live identity has it.
    let (mut groups, mut states, group, mut work) = event_fixture(1);
    work.units.insert(object(), unit(7));
    groups
        .event(group, &mut states, GroupEvent::Aggro, None, &mut work)
        .unwrap();
    groups
        .event(
            group,
            &mut states,
            GroupEvent::Aggro,
            Some(object()),
            &mut work,
        )
        .unwrap();
    assert!(work.callbacks.is_empty());
    work.units.get_mut(&object()).unwrap().has_charmer = true;
    groups
        .event(
            group,
            &mut states,
            GroupEvent::Aggro,
            Some(object()),
            &mut work,
        )
        .unwrap();
    assert_eq!(work.callbacks.len(), 4);
    assert!(matches!(
        work.callbacks[0],
        GroupCallback::SetNoCallAssistance { .. }
    ));
    assert!(matches!(
        work.callbacks[1],
        GroupCallback::EngageInCombat { .. }
    ));
    let (mut groups, mut states, group, mut work) = event_fixture(0);
    work.units.clear();
    groups
        .event(
            group,
            &mut states,
            GroupEvent::Aggro,
            Some(object()),
            &mut work,
        )
        .unwrap();
    assert!(work.callbacks.is_empty());
}
#[test]
fn last_evade_home_callback_precedes_respawn_fallthrough_and_dead_members_do_not_block_home() {
    // SpawnGroup.cpp:574-583,671-696 excludes dead evaders and saves every definition member.
    let mut input = template(1, 0, 3);
    input.flags = 2;
    let (mut groups, mut states, group, mut work) =
        fixture(input, &[(7, 0), (8, 0), (9, 0)], false, |rows, _| {
            rows.push(
                GroupPhase::Formations,
                GroupInputData::Formation(formation(1)),
            );
        });
    groups
        .spawn(group, &mut states, false, false, &mut work)
        .unwrap();
    let a = live_member(&mut work, 7);
    let b = live_member(&mut work, 8);
    work.units.get_mut(&a).unwrap().evading_home = true;
    work.callbacks.clear();
    groups
        .event(group, &mut states, GroupEvent::Home, Some(b), &mut work)
        .unwrap();
    assert!(work.callbacks.is_empty());
    assert_eq!(work.wall_calls.last(), Some(&GroupClockCall::ClearRespawns));
    work.units.get_mut(&a).unwrap().alive = false;
    work.callback_hook = Some(|work, _, _, _, callback| {
        if callback == GroupCallback::FormationHome {
            assert_eq!(work.wall_calls.len(), 2);
        }
        Ok(())
    });
    groups
        .event(group, &mut states, GroupEvent::Home, Some(b), &mut work)
        .unwrap();
    assert_eq!(work.callbacks, [GroupCallback::FormationHome]);
    assert_eq!(
        work.sql
            .iter()
            .filter(|s| matches!(s, PersistentSql::Delete { .. }))
            .count(),
        6
    );
    assert!(groups.is_out_of_combat(group, &mut work).unwrap());
    work.units.get_mut(&b).unwrap().in_combat = true;
    assert!(!groups.is_out_of_combat(group, &mut work).unwrap());
}
#[test]
fn evade_calls_only_non_home_members_and_member_death_has_no_alive_filter() {
    // SpawnGroup.cpp:565-570,585-590 does not filter alive in either callback loop.
    let (mut groups, mut states, group, mut work) = event_fixture(4);
    let a = work.live[&key(7)];
    let b = work.live[&key(8)];
    work.units.get_mut(&a).unwrap().evading_home = true;
    work.units.get_mut(&b).unwrap().alive = false;
    groups
        .event(group, &mut states, GroupEvent::Evade, Some(a), &mut work)
        .unwrap();
    assert_eq!(
        work.callbacks,
        [GroupCallback::EnterEvadeMode { object: b }]
    );
    work.callbacks.clear();
    groups
        .event(
            group,
            &mut states,
            GroupEvent::MemberDied,
            Some(b),
            &mut work,
        )
        .unwrap();
    assert_eq!(
        work.callbacks,
        [
            GroupCallback::MemberDied {
                object: a,
                target: Some(b)
            },
            GroupCallback::MemberDied {
                object: b,
                target: Some(b)
            }
        ]
    );
}
#[test]
fn full_wipe_constant_cooldown_still_draws_and_false_chances_clear_only_on_last_world_removal() {
    // SpawnGroup.cpp:78-97 draws urand(5,5), keeps false cache until the last world removal.
    for dungeon in [false, true] {
        let mut input = template(1, 0, 3);
        input.respawn_min = Some(5);
        let (mut groups, mut states, group, mut work) =
            fixture(input, &[(7, 100), (8, 100), (9, 1)], dungeon, |_, _| {});
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        assert_eq!(
            groups.state(group).unwrap().chosen_spawns.get(&9),
            Some(&false)
        );
        let a = live_member(&mut work, 7);
        let b = live_member(&mut work, 8);
        let before = work.cursor.raw;
        groups
            .remove_object(group, &mut states, a, &mut work)
            .unwrap();
        assert_eq!(
            groups.state(group).unwrap().chosen_spawns.get(&9),
            Some(&false)
        );
        groups
            .event(group, &mut states, GroupEvent::Respawn, Some(b), &mut work)
            .unwrap();
        assert_eq!(
            groups.state(group).unwrap().chosen_spawns.get(&9),
            Some(&false)
        );
        groups
            .remove_object(group, &mut states, b, &mut work)
            .unwrap();
        assert_eq!(work.cursor.raw, before + u64::from(!dungeon));
        assert_eq!(
            groups.state(group).unwrap().cooldown_millis,
            if dungeon { 0 } else { 105_000 }
        );
        assert_eq!(
            groups.state(group).unwrap().chosen_spawns.contains_key(&9),
            dungeon
        );
    }
}
#[test]
fn copied_despawn_iteration_permits_current_and_other_member_removal_for_both_types() {
    // SpawnGroup.cpp:633-652,713-732 copies membership before callbacks mutate the original.
    for kind in [0, 1] {
        let (mut groups, mut states, group, mut work) =
            fixture(template(1, kind, 2), &[(7, 0), (8, 0)], false, |_, _| {});
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        let spawn_kind = if kind == 0 {
            GridSpawnKind::Creature
        } else {
            GridSpawnKind::GameObject
        };
        for guid in [7, 8] {
            let object = ObjectHandle {
                thread: 1,
                construction: guid.into(),
            };
            let key = SpawnKey {
                kind: spawn_kind,
                guid,
            };
            work.live.insert(key, object);
            work.removed.insert(
                object,
                GroupRemovedObject {
                    key,
                    respawn_delay: 1,
                },
            );
        }
        work.callback_hook = Some(|work, groups, states, group, callback| {
            if let GroupCallback::ForcedDespawn { object, .. } = callback {
                groups.remove_object(group, states, object, work)?;
            }
            Ok(())
        });
        groups.despawn(group, &mut states, 0, 0, &mut work).unwrap();
        assert!(groups.state(group).unwrap().objects.is_empty());
        assert_eq!(
            work.callbacks
                .iter()
                .filter(|c| matches!(c, GroupCallback::ForcedDespawn { .. }))
                .count(),
            2
        );
    }
}
#[test]
fn callback_other_key_erasure_is_defined_but_held_key_erasure_refuses_in_each_live_loop() {
    // SpawnGroup.cpp:542,565,586 increments the current std::map iterator only after callbacks.
    for event in [GroupEvent::Aggro, GroupEvent::Evade, GroupEvent::MemberDied] {
        for held in [false, true] {
            let (mut groups, mut states, group, mut work) = event_fixture(5);
            work.callback_hook = Some(|work, groups, states, group, callback| {
                let object = match callback {
                    GroupCallback::EngageInCombat { object, .. }
                    | GroupCallback::EnterEvadeMode { object }
                    | GroupCallback::MemberDied { object, .. } => object,
                    _ => return Ok(()),
                };
                let held = work.instance;
                let removed = if held { object } else { work.live[&key(8)] };
                groups.remove_object(group, states, removed, work)
            });
            work.instance = held;
            let result = groups.event(group, &mut states, event, Some(object()), &mut work);
            if held {
                assert_eq!(
                    result.unwrap_err().id,
                    "execution.group.callback_iterator_undefined"
                );
                assert_eq!(groups.state(group).unwrap().objects.len(), 2);
                assert!(groups.inventory().require_quiescent().is_err());
            } else {
                result.unwrap();
                assert_eq!(
                    groups
                        .state(group)
                        .unwrap()
                        .objects
                        .keys()
                        .copied()
                        .collect::<Vec<_>>(),
                    [7]
                );
            }
        }
    }
}
#[test]
fn callback_clear_of_held_map_refuses_but_delayed_despawn_does_not_clear() {
    // SpawnGroup.cpp:651-652 clears only immediate despawn, which invalidates held range-for iterators.
    for delay in [0, 1] {
        let (mut groups, mut states, group, mut work) = event_fixture(0);
        work.clock = delay;
        work.callback_hook = Some(|work, groups, states, group, callback| {
            if matches!(callback, GroupCallback::MemberDied { .. }) {
                groups.despawn(group, states, work.clock as u32, 0, work)?;
            }
            Ok(())
        });
        let result = groups.event(group, &mut states, GroupEvent::MemberDied, None, &mut work);
        if delay == 0 {
            assert_eq!(
                result.unwrap_err().id,
                "execution.group.callback_iterator_undefined"
            );
        } else {
            result.unwrap();
        }
        assert_eq!(groups.state(group).unwrap().objects.len(), 2);
    }
}
#[test]
fn instance_callback_runs_before_formation_reset_and_refusal_prevents_reset() {
    // SpawnGroup.cpp:517-522 calls instance first, and only inside the instance-present branch resets WP.
    for refuse_reset in [false, true] {
        let (mut groups, mut states, group, mut work) =
            fixture(template(1, 0, 1), &[(7, 0)], true, |rows, _| {
                rows.push(
                    GroupPhase::Formations,
                    GroupInputData::Formation(formation(1)),
                );
            });
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        let object = live_member(&mut work, 7);
        work.instance = true;
        work.callbacks.clear();
        work.callback_hook = Some(|_, _, _, _, callback| {
            if callback == GroupCallback::ResetLastWaypoint {
                return Err(callback.missing_owner());
            }
            Ok(())
        });
        if !refuse_reset {
            work.callback_hook = Some(|_, _, _, _, callback| Err(callback.missing_owner()));
        }
        assert_eq!(
            groups
                .remove_object(group, &mut states, object, &mut work)
                .unwrap_err()
                .id,
            if refuse_reset {
                "execution.group.f4_13_formation_owner"
            } else {
                "execution.group.f5_instance_group_despawn_owner"
            }
        );
        assert_eq!(
            work.callbacks[0],
            GroupCallback::LastCreatureDespawn { object }
        );
        assert_eq!(work.callbacks.len(), if refuse_reset { 2 } else { 1 });
    }
}
#[test]
fn missing_group_callback_owners_have_distinct_refusals() {
    // SpawnGroup.cpp:548-550,570,590,519,575 delegates these effects rather than substituting successful no-ops.
    for (callback, id) in [
        (
            GroupCallback::SetNoCallAssistance { object: object() },
            "execution.group.f5_assistance_owner",
        ),
        (
            GroupCallback::EngageInCombat {
                object: object(),
                target: object(),
            },
            "execution.group.f5_combat_engage_owner",
        ),
        (
            GroupCallback::EnterEvadeMode { object: object() },
            "execution.group.f5_ai_evade_owner",
        ),
        (
            GroupCallback::MemberDied {
                object: object(),
                target: None,
            },
            "execution.group.f5_ai_member_died_owner",
        ),
        (
            GroupCallback::FormationHome,
            "execution.group.f4_13_formation_owner",
        ),
    ] {
        assert_eq!(callback.missing_owner().id, id);
    }
}

#[test]
fn immediate_unloaded_despawn_writes_deadlines_for_both_types_and_delayed_does_not() {
    // SpawnGroup.cpp:632,644-652,712,724-732 writes unloaded members using one fresh wall sample.
    for kind in [0, 1] {
        let (mut groups, mut states, group, mut work) =
            fixture(template(1, kind, 2), &[(7, 0), (8, 0)], false, |_, _| {});
        groups
            .spawn(group, &mut states, false, false, &mut work)
            .unwrap();
        let persistent = groups.state(group).unwrap().persistent;
        let kind = if kind == 0 {
            GridSpawnKind::Creature
        } else {
            GridSpawnKind::GameObject
        };
        groups.despawn(group, &mut states, 1, 5, &mut work).unwrap();
        assert!(work.sql.is_empty());
        groups.despawn(group, &mut states, 0, 5, &mut work).unwrap();
        assert_eq!(
            work.wall_calls,
            [
                GroupClockCall::Spawn,
                GroupClockCall::Despawn,
                GroupClockCall::Despawn
            ]
        );
        for guid in [7, 8] {
            assert_eq!(states.state(persistent).unwrap().respawn(kind, guid), 105);
        }
        assert_eq!(
            work.sql
                .iter()
                .filter(|s| matches!(s, PersistentSql::Insert { deadline: 105, .. }))
                .count(),
            2
        );
        assert!(groups.state(group).unwrap().objects.is_empty());
    }
}

#[test]
fn missing_live_linked_group_refuses_at_source_dereference_not_definition_loading() {
    // SpawnGroup.cpp:555-557 dereferences a linked group's map-local pointer without a null guard.
    let mut input = template(1, 0, 1);
    input.flags = 1;
    let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |rows, _| {
        rows.push(
            GroupPhase::Templates,
            GroupInputData::Template(template(2, 0, 1)),
        );
        rows.push(
            GroupPhase::Links,
            GroupInputData::Link {
                group: 1,
                linked: 2,
            },
        );
    });
    work.units.insert(object(), unit(99));
    assert_eq!(
        groups
            .event(
                group,
                &mut states,
                GroupEvent::Aggro,
                Some(object()),
                &mut work
            )
            .unwrap_err()
            .id,
        "execution.group.linked_group_absent_undefined"
    );
    assert_eq!(
        groups
            .state(group)
            .unwrap()
            .linkage_targets
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [object()]
    );
}
#[test]
fn explicit_creature_despawn_retains_only_alive_in_scheduled_callback() {
    // SpawnGroup.h:97-98 keeps the explicit onlyAlive overload distinct from its default true.
    let (mut groups, mut states, group, mut work) = event_fixture(0);
    groups
        .despawn_creatures(group, &mut states, 25, false, 5, &mut work)
        .unwrap();
    assert!(work.callbacks.iter().all(|callback| matches!(
        callback,
        GroupCallback::ForcedDespawn {
            delay_millis: 25,
            only_alive: false,
            forced_seconds: 5,
            ..
        }
    )));
    assert_eq!(work.callbacks.len(), 2);
}
#[test]
fn retained_group_callback_projection_is_explicitly_unsealed_or_skipped() {
    let Ok(root) = std::env::var("BENILLA_F1_GROUP_LINKS_FRAMING") else {
        eprintln!("SKIP unsealed MG/MR/MPR group callback comparison: BENILLA_F1_GROUP_LINKS_FRAMING absent");
        return;
    };
    let path = Path::new(&root).join("source-sites.json");
    let sites: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(path).unwrap()).unwrap();
    let group_sites = sites
        .as_object()
        .unwrap()
        .values()
        .filter(|site| {
            site["callsite"]["source_file"].as_str() == Some("src/game/Maps/SpawnGroup.cpp")
        })
        .count();
    eprintln!("Unsealed group callback dictionary sites={group_sites}");
    eprintln!("SKIP unsealed MG/MR/MPR group callback comparison: no independent callback-state projection owner");
}

#[test]
fn linked_gameobject_group_refuses_before_invalid_creature_group_cast() {
    // SpawnGroup.cpp:556 casts GetSpawnGroup to CreatureGroup without checking its dynamic type.
    let mut input = template(1, 0, 1);
    input.flags = 1;
    let (mut groups, mut states, group, mut work) = fixture(input, &[(7, 0)], false, |rows, _| {
        rows.push(
            GroupPhase::Templates,
            GroupInputData::Template(template(2, 1, 1)),
        );
        rows.push(
            GroupPhase::Links,
            GroupInputData::Link {
                group: 1,
                linked: 2,
            },
        );
    });
    let persistent = groups.state(group).unwrap().persistent;
    groups
        .create(group.map, persistent, 2, &mut states, &mut work)
        .unwrap();
    work.units.insert(object(), unit(99));
    assert_eq!(
        groups
            .event(
                group,
                &mut states,
                GroupEvent::Aggro,
                Some(object()),
                &mut work
            )
            .unwrap_err()
            .id,
        "execution.group.linked_group_type_undefined"
    );
}

#[test]
fn catalog_group_owner_matches_fixture_definitions_and_detaches_real_spawn_sets() {
    use super::super::production_compiler::VerifiedCatalog;
    VerifiedCatalog::with_authored_group_test_catalog(|mut catalog| {
        fn rows() -> Rows {
            let mut rows = Rows::default();
            for id in [1, 2] {
                rows.push(
                    GroupPhase::Templates,
                    GroupInputData::Template(template(id, 0, 0)),
                );
            }
            rows.push(
                GroupPhase::Formations,
                GroupInputData::Formation(Formation {
                    group: 1,
                    kind: 0,
                    spread_bits: 2.0_f32.to_bits(),
                    options: 0,
                    path: 5,
                    movement: 0,
                    comment: b"authored".to_vec(),
                }),
            );
            rows.push(
                GroupPhase::Members,
                GroupInputData::Member {
                    group: 1,
                    guid: 7,
                    slot: 3,
                    chance: 0,
                },
            );
            rows.push(
                GroupPhase::Entries,
                GroupInputData::Entry {
                    group: 1,
                    entry: RandomEntry {
                        entry: 10,
                        min: 1,
                        max: 1,
                        chance: 0,
                    },
                },
            );
            rows.push(
                GroupPhase::Links,
                GroupInputData::Link {
                    group: 1,
                    linked: 2,
                },
            );
            rows.push(
                GroupPhase::Squads,
                GroupInputData::Squad {
                    group: 1,
                    squad: 4,
                    guid: 7,
                    entry: 10,
                },
            );
            rows
        }
        let key = SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: 7,
        };
        let definition = spawn(10);
        let mut fixture = Owner::default();
        fixture.spawns.insert(key, definition);
        fixture.templates.insert((key.kind, 10));
        fixture.static_spawns.insert(key);
        fixture.dynamic_spawns = vec![key, key];
        fixture.random.insert(key);
        let mut expected = SourceSpawnGroups::default();
        expected.load(&mut rows(), &mut fixture).unwrap();

        let definitions = BTreeMap::from([(key, definition)]);
        let random_entries = BTreeSet::from([key]);
        let mut grids = BTreeMap::from([(
            0,
            GridSpawnSets::new(MapIdentity {
                map: 0,
                instance: 0,
                creation_generation: 1,
            }),
        )]);
        grids
            .get_mut(&0)
            .unwrap()
            .register(
                definition.cell,
                key.kind,
                GridSpawnOrigin::Static,
                key.guid,
                0,
            )
            .unwrap();
        let mut dynamic = BTreeMap::from([((0, key.kind), vec![7, 8, 7])]);
        let mut owner = CatalogGroupLoadOwner::new(
            &mut catalog,
            Some(EffectiveGroupSpawns {
                definitions: &definitions,
                random_entries: &random_entries,
                static_grids: &mut grids,
                dynamic_guids: &mut dynamic,
            }),
            Some(false),
        );
        let mut actual = SourceSpawnGroups::default();
        actual.load(&mut rows(), &mut owner).unwrap();
        assert_eq!(
            actual.definitions().unwrap(),
            expected.definitions().unwrap()
        );
        assert_eq!(actual.load_records(), expected.load_records());
        drop(owner);
        assert_eq!(dynamic[&(0, key.kind)], [8]);
        // An ordinal change succeeds only if the original static registration was erased.
        grids
            .get_mut(&0)
            .unwrap()
            .register(
                definition.cell,
                key.kind,
                GridSpawnOrigin::Static,
                key.guid,
                9,
            )
            .unwrap();
        assert!(fixture.static_spawns.is_empty());
        assert!(fixture.dynamic_spawns.is_empty());
    });
}

#[test]
fn catalog_group_owner_refuses_unbound_views_effective_spawns_and_config() {
    use super::super::production_compiler::VerifiedCatalog;
    VerifiedCatalog::with_authored_group_test_catalog(|mut catalog| {
        let mut owner = CatalogGroupLoadOwner::new(&mut catalog, None, None);
        for (kind, name) in [
            (GridSpawnKind::Creature, "creatures"),
            (GridSpawnKind::GameObject, "gameobjects"),
        ] {
            assert_eq!(
                owner.spawn(SpawnKey { kind, guid: 7 }).unwrap_err().id,
                format!("execution.spawn_groups.objectmgr_load_{name}_effective_spawn_owner")
            );
        }
        assert_eq!(
            owner.condition_exists(1).unwrap_err().id,
            "execution.spawn_groups.conditions_view_owner"
        );
        assert_eq!(
            owner.expression_exists(1).unwrap_err().id,
            "execution.spawn_groups.worldstate_expression_view_owner"
        );
        assert_eq!(
            owner.string_exists(1).unwrap_err().id,
            "execution.spawn_groups.string_id_view_owner"
        );
        assert_eq!(
            owner.creature_template_flags(10).unwrap_err().id,
            "execution.spawn_groups.creature_template_view_owner"
        );
        assert_eq!(
            owner.autoload_active().unwrap_err().id,
            "execution.spawn_groups.autoload_active_config_owner"
        );
        assert!(owner.template_exists(GridSpawnKind::Creature, 10).unwrap());
        assert!(!owner.template_exists(GridSpawnKind::Creature, 11).unwrap());
        drop(owner);
        assert_eq!(
            SourceSpawnGroups::default()
                .load_bound_catalog(&mut catalog, Path::new("."), None, None)
                .unwrap_err()
                .id,
            "execution.spawn_groups.spawn_group_view_owner"
        );
    });
}

#[test]
fn catalog04_spawn_group_load_counts_and_effective_spawn_refusals_or_explicit_absence() {
    use super::super::production_compiler::VerifiedCatalog;
    let (Some(root), Some(occurrences)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES"),
    ) else {
        println!("SKIP spawn-group catalog-04 load: set BENILLA_F2_CATALOG and BENILLA_F2_GROUP_OCCURRENCES");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let mut rows = catalog
        .execution_spawn_group_rows(Path::new(&occurrences))
        .unwrap();
    let mut counts = [0; 6];
    let mut kinds = BTreeMap::new();
    let mut members = Vec::new();
    for phase in GroupPhase::ALL {
        while let Some(row) = GroupRows::next(&mut rows, phase).unwrap() {
            counts[phase as usize] += 1;
            match row.data {
                GroupInputData::Template(template) => {
                    kinds.insert(template.id, template.kind);
                }
                GroupInputData::Member { group, guid, .. } => members.push((group, guid)),
                _ => {}
            }
        }
    }
    assert_eq!(counts, [1346, 231, 4582, 2173, 7, 0]);
    let mut groups = SourceSpawnGroups::default();
    let refusal = groups
        .load_bound_catalog(&mut catalog, Path::new(&occurrences), None, None)
        .unwrap_err();
    assert_eq!(
        refusal.id,
        "execution.spawn_groups.objectmgr_load_gameobjects_effective_spawn_owner"
    );
    assert!(groups.definitions().is_err());
    let mut owner = CatalogGroupLoadOwner::new(&mut catalog, None, None);
    let mut refused = BTreeMap::<String, usize>::new();
    let mut passed = 0;
    for (group, guid) in members {
        let kind = match kinds[&group] {
            0 => GridSpawnKind::Creature,
            1 => GridSpawnKind::GameObject,
            _ => panic!("invalid stock group type"),
        };
        match owner.spawn(SpawnKey { kind, guid }) {
            Ok(_) => passed += 1,
            Err(error) => *refused.entry(error.id).or_default() += 1,
        }
    }
    assert_eq!(passed, 0);
    assert_eq!(
        refused.values().sum::<usize>(),
        counts[GroupPhase::Members as usize]
    );
    assert_eq!(
        refused,
        BTreeMap::from([
            (
                "execution.spawn_groups.objectmgr_load_creatures_effective_spawn_owner".into(),
                1950
            ),
            (
                "execution.spawn_groups.objectmgr_load_gameobjects_effective_spawn_owner".into(),
                2632
            ),
        ])
    );
    let accepted = groups
        .load_records()
        .iter()
        .filter(|record| record.decision == GroupLoadDecision::Accepted)
        .count();
    println!("spawn-group catalog-04 query rows [groups, formations, members, entries, links, squads]={counts:?}; accepted prefix rows={accepted}; effective container unpublished; first refusal={}", refusal.id);
    println!("spawn-group catalog-04 member prerequisite probes passed={passed}, refused={refused:?}; these are not accepted-member counts");
}
