// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored executable linking fixtures, not a sealed source execution.
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Default)]
struct Rows {
    templates: VecDeque<LinkingTemplateRow>,
    guids: VecDeque<LinkingGuidRow>,
}
impl LinkingRows for Rows {
    fn next_template(&mut self) -> Result<Option<LinkingTemplateRow>> {
        Ok(self.templates.pop_front())
    }
    fn next_guid(&mut self) -> Result<Option<LinkingGuidRow>> {
        Ok(self.guids.pop_front())
    }
}
#[derive(Default)]
struct Definitions {
    spawns: BTreeMap<u32, LinkingSpawnData>,
    unique: BTreeMap<(u32, u32), Vec<u32>>,
    queries: Vec<(u32, u32)>,
    missing_templates: BTreeSet<u32>,
    missing_maps: BTreeSet<u32>,
    calls: Vec<(u8, u32)>,
    query_error: bool,
}
impl LinkingLoadOwner for Definitions {
    fn template_exists(&mut self, entry: u32) -> Result<bool> {
        self.calls.push((0, entry));
        Ok(!self.missing_templates.contains(&entry))
    }
    fn map_exists(&mut self, map: u32) -> Result<bool> {
        self.calls.push((1, map));
        Ok(!self.missing_maps.contains(&map))
    }
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        self.calls.push((2, guid));
        Ok(self.spawns.get(&guid).copied())
    }
    fn query_unique_master(&mut self, entry: u32, map: u32) -> Result<LinkingMasterQuery> {
        if self.query_error {
            return Err(refuse("authored.unique_sql_owner"));
        }
        self.queries.push((entry, map));
        Ok(match self.unique.get(&(entry, map)) {
            Some(rows) => LinkingMasterQuery::Rows(rows.clone()),
            None => LinkingMasterQuery::NoResult,
        })
    }
}
fn template(slave: u32, map: u32, master: u32, flags: u16, range: u16) -> LinkingTemplateRow {
    LinkingTemplateRow {
        ordinal: 0,
        slave,
        map,
        master,
        flags,
        range,
    }
}
fn spawn(entry: u32, map: u32, x: f32) -> LinkingSpawnData {
    LinkingSpawnData {
        entry,
        map,
        respawn_xy: [x.to_bits(), 0.0_f32.to_bits()],
    }
}
#[test]
fn guid_lookup_precedes_entry_and_ignores_the_callers_map() {
    let mut definitions = Definitions::default();
    definitions.spawns.insert(1, spawn(7, 0, 0.0));
    definitions.spawns.insert(2, spawn(8, 0, 0.0));
    let mut rows = Rows {
        templates: [template(7, 0, 9, 1, 20), template(7, 1, 10, 1, 20)].into(),
        guids: [LinkingGuidRow {
            ordinal: 0,
            slave: 1,
            master: 2,
            flags: 1,
        }]
        .into(),
    };
    let mut links = SourceCreatureLinking::default();
    links.load(&mut rows, &mut definitions).unwrap();
    // CreatureLinkingMgr.cpp:309-321 checks GUID first without a map filter.
    assert_eq!(links.lookup(7, 1, 1).unwrap().master, 2);
    assert_eq!(links.lookup(7, 99, 1).unwrap().master, 10);
    assert!(links.lookup(7, 99, 2).is_none());
    assert!(definitions.queries.is_empty());
}

fn loaded(
    templates: Vec<LinkingTemplateRow>,
    guids: Vec<LinkingGuidRow>,
    definitions: &mut Definitions,
) -> SourceCreatureLinking {
    let mut links = SourceCreatureLinking::default();
    links
        .load(
            &mut Rows {
                templates: templates.into(),
                guids: guids.into(),
            },
            definitions,
        )
        .unwrap();
    links
}
fn guid_row(slave: u32, master: u32, flags: u16) -> LinkingGuidRow {
    LinkingGuidRow {
        ordinal: 0,
        slave,
        master,
        flags,
    }
}
#[test]
fn unique_master_limit_two_absent_single_duplicate_and_zero_keep_source_dispositions() {
    // cpp:234-249 queries FOLLOW too, rejects nonunique rows and accepts GUID zero.
    let mut d = Definitions::default();
    d.unique.insert((8, 0), vec![42]);
    d.unique.insert((9, 0), vec![43, 44]);
    d.unique.insert((10, 0), vec![0]);
    let links = loaded(
        vec![
            template(1, 0, 7, 0x200, 0),
            template(2, 0, 8, LINKING_BOSS_DEAD, 0),
            template(3, 0, 9, LINKING_BOSS_DEAD, 0),
            template(4, 0, 10, LINKING_BOSS_DEAD, 0),
        ],
        vec![],
        &mut d,
    );
    assert_eq!(d.queries, vec![(7, 0), (8, 0), (9, 0), (10, 0)]);
    assert!(links.lookup(1, 1, 0).is_none());
    assert_eq!(links.lookup(2, 2, 0).unwrap().master_guid, 42);
    assert!(links.lookup(3, 3, 0).is_none());
    assert!(!links.lookup(4, 4, 0).unwrap().spawning_linked());
    assert_eq!(
        links
            .inventory()
            .rejected
            .iter()
            .map(|r| r.reason)
            .collect::<Vec<_>>(),
        vec![
            LinkingRejection::MissingUniqueMaster,
            LinkingRejection::NonuniqueMaster
        ]
    );
}
#[test]
fn loader_copies_map_zero_and_eager_lookup_quirks_without_correcting_rows() {
    // cpp:177-178,198-199 obtains both operands before checking either; :190 skips map 0.
    let mut d = Definitions::default();
    d.missing_templates.insert(1);
    d.missing_maps.extend([0, 99]);
    d.spawns.insert(2, spawn(8, 0, 0.0));
    let links = loaded(
        vec![
            template(1, 0, 8, 1, 1),
            template(3, 0, 8, 1, 1),
            template(4, 99, 8, 1, 1),
        ],
        vec![guid_row(99, 2, 1)],
        &mut d,
    );
    assert_eq!(&d.calls[..4], &[(0, 1), (0, 8), (0, 3), (0, 8)]);
    assert!(!d.calls.contains(&(1, 0)));
    assert_eq!(&d.calls[d.calls.len() - 2..], &[(2, 99), (2, 2)]);
    assert!(links.lookup(3, 3, 0).is_some());
    assert_eq!(
        links
            .inventory()
            .rejected
            .iter()
            .map(|r| r.reason)
            .collect::<Vec<_>>(),
        vec![
            LinkingRejection::SlaveTemplate,
            LinkingRejection::Map,
            LinkingRejection::SlaveGuid
        ]
    );
}
#[test]
fn loader_rejects_cross_map_guid_and_invalid_flags_but_retains_equal_key_order() {
    let mut d = Definitions::default();
    d.spawns.insert(1, spawn(7, 0, 0.0));
    d.spawns.insert(2, spawn(8, 1, 0.0));
    let links = loaded(
        vec![
            template(7, 0, 8, 0, 1),
            template(7, 0, 8, 0x8000, 1),
            template(7, 0, 7, 0x100, 1),
            template(7, 1, 10, 1, 1),
            template(7, 0, 9, 1, 1),
            template(7, 0, 8, 1, 1),
        ],
        vec![guid_row(1, 2, 1)],
        &mut d,
    );
    assert_eq!(links.lookup(7, 1, 0).unwrap().master, 9);
    assert_eq!(links.lookup(7, 1, 1).unwrap().master, 10);
    assert_eq!(links.inventory().entries[&7].len(), 3);
    assert_eq!(
        links
            .inventory()
            .rejected
            .iter()
            .map(|r| r.reason)
            .collect::<Vec<_>>(),
        vec![
            LinkingRejection::Flags,
            LinkingRejection::Flags,
            LinkingRejection::SelfDespawnOnRespawn,
            LinkingRejection::DifferentMaps
        ]
    );
}
#[test]
fn missing_unique_query_owner_stops_and_cannot_be_retried_as_empty() {
    let mut d = Definitions {
        query_error: true,
        ..Definitions::default()
    };
    let mut links = SourceCreatureLinking::default();
    let mut rows = Rows {
        templates: [template(7, 0, 8, LINKING_BOSS_DEAD, 0)].into(),
        ..Rows::default()
    };
    assert_eq!(
        links.load(&mut rows, &mut d).unwrap_err().id,
        "authored.unique_sql_owner"
    );
    assert_eq!(
        links.ensure_loaded().unwrap_err().id,
        "execution.linking.stopped"
    );
    assert_eq!(
        links
            .load(&mut Rows::default(), &mut Definitions::default())
            .unwrap_err()
            .id,
        "execution.linking.stopped"
    );
    for invalid in [vec![], vec![1, 2, 3]] {
        let mut d = Definitions::default();
        d.unique.insert((8, 0), invalid);
        let mut links = SourceCreatureLinking::default();
        let mut rows = Rows {
            templates: [template(7, 0, 8, LINKING_BOSS_DEAD, 0)].into(),
            ..Rows::default()
        };
        assert_eq!(
            links.load(&mut rows, &mut d).unwrap_err().id,
            "execution.linking.unique_query_shape"
        );
    }
}
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Call {
    Data(u32),
    Wall(u32),
    Pool(u32),
    Encounter,
}
#[derive(Default)]
struct Gate {
    spawns: BTreeMap<u32, LinkingSpawnData>,
    pools: BTreeMap<u32, u16>,
    clocks: VecDeque<ClockSample>,
    encounter: Option<bool>,
    calls: Vec<Call>,
    refuse_encounter: bool,
}
impl LinkingGateOwner for Gate {
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        self.calls.push(Call::Data(guid));
        Ok(self.spawns.get(&guid).copied())
    }
    fn wall_time(&mut self, _: MapIdentity, guid: u32) -> Result<ClockSample> {
        self.calls.push(Call::Wall(guid));
        self.clocks
            .pop_front()
            .ok_or_else(|| refuse("authored.wall_owner"))
    }
    fn creature_pool(&mut self, guid: u32) -> Result<Option<u16>> {
        self.calls.push(Call::Pool(guid));
        Ok(self.pools.get(&guid).copied())
    }
    fn encounter_in_progress(&mut self, _: MapIdentity) -> Result<Option<bool>> {
        self.calls.push(Call::Encounter);
        if self.refuse_encounter {
            return Err(refuse("authored.instance_owner"));
        }
        Ok(self.encounter)
    }
}
struct Scene {
    objects: SourceObjects,
    cells: SourceCells,
    state: SourcePersistentState,
    holder: SourceCreatureLinkingHolder,
}
impl Scene {
    fn new() -> Self {
        let map = map();
        let mut state = SourcePersistentState::new(
            PersistentStateIdentity {
                map: map.map,
                instance: map.instance,
                generation: 1,
            },
            PersistentMapEntry {
                id: map.map,
                map_type: 0,
            },
            0,
            true,
            0,
        );
        state.used_by_map = Some(map);
        Self {
            objects: SourceObjects::default(),
            cells: SourceCells::new(map).unwrap(),
            state,
            holder: SourceCreatureLinkingHolder::new(map).unwrap(),
        }
    }
    fn actor(
        &mut self,
        entry: u32,
        database: u32,
        runtime: u32,
        x: f32,
        alive: bool,
        pet: bool,
    ) -> ObjectHandle {
        let kind = if pet {
            SourceObjectKind::Pet
        } else {
            SourceObjectKind::Creature
        };
        let object = self.objects.construct(1, kind).unwrap();
        let high = if pet {
            HighGuid::Pet
        } else {
            HighGuid::Creature
        };
        self.objects
            .assign_guid(
                object,
                database,
                SourceGuid::with_entry(high, entry, runtime),
            )
            .unwrap();
        self.objects.bind_map(object, map(), &self.cells).unwrap();
        self.objects
            .set_respawn_definition(
                object,
                SourcePose([
                    x.to_bits(),
                    0.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                ]),
                true,
            )
            .unwrap();
        self.objects
            .set_pose(
                object,
                SourcePose([
                    999.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                    0.0_f32.to_bits(),
                ]),
            )
            .unwrap();
        self.objects.add_to_world(object).unwrap();
        if !alive {
            self.objects.mark_dead(object).unwrap();
        }
        object
    }
}
#[test]
fn first_in_range_master_uses_registration_order_respawn_xy_and_strict_radius() {
    // cpp:736-745 never sorts by GUID, distance or alive status.
    let links = loaded(
        vec![template(7, 0, 8, LINKING_BOSS_DEAD, 10)],
        vec![],
        &mut Definitions::default(),
    );
    let mut s = Scene::new();
    let slave = s.actor(7, 1, 1, 0.0, true, false);
    let first = s.actor(8, 2, 90, 5.0, false, false);
    let second = s.actor(8, 3, 2, 1.0, true, false);
    s.holder.add_master(&links, &s.objects, first, 8).unwrap();
    s.holder.add_master(&links, &s.objects, second, 8).unwrap();
    let mut gate = Gate::default();
    assert!(!s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    assert!(gate.calls.is_empty());
    s.objects
        .set_respawn_definition(first, SourcePose([10.0_f32.to_bits(), 0, 0, 0]), true)
        .unwrap();
    assert!(s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    assert_eq!(
        s.holder.inventory().masters[&8]
            .iter()
            .map(|v| v.counter())
            .collect::<Vec<_>>(),
        vec![90, 2]
    );
}
#[test]
fn missing_nearby_master_and_retained_stale_full_guid_allow_spawn_then_resolve_reuse() {
    let links = loaded(
        vec![template(7, 0, 8, LINKING_BOSS_DEAD, 10)],
        vec![],
        &mut Definitions::default(),
    );
    let mut s = Scene::new();
    let slave = s.actor(7, 1, 1, 0.0, true, false);
    let mut gate = Gate::default();
    assert!(s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    let master = s.actor(8, 2, 9, 1.0, false, false);
    s.holder.add_master(&links, &s.objects, master, 8).unwrap();
    let saved = s.holder.inventory();
    s.objects.remove_from_world(master).unwrap();
    s.objects.destroy(master, &mut s.cells).unwrap();
    assert_eq!(s.holder.inventory(), saved);
    assert!(s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    let next = s.actor(8, 2, 9, 1.0, false, false);
    assert_ne!(next, master);
    s.holder.add_master(&links, &s.objects, next, 8).unwrap();
    assert_eq!(s.holder.inventory(), saved);
    assert!(!s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
}
#[test]
fn duplicate_slave_registration_appends_while_master_registration_deduplicates_and_skips_pets() {
    let links = loaded(
        vec![template(7, 0, 8, 1, 10)],
        vec![],
        &mut Definitions::default(),
    );
    let mut s = Scene::new();
    let slave = s.actor(777, 1, 1, 0.0, true, false);
    // The current entry field can differ from the entry bits in its full GUID.
    assert!(lookup_source_creature_linking(&links, &s.objects, slave, 7)
        .unwrap()
        .is_some());
    register_source_creature_linking(&links, &mut s.holder, &s.objects, slave, 7).unwrap();
    register_source_creature_linking(&links, &mut s.holder, &s.objects, slave, 7).unwrap();
    let master = s.actor(8, 2, 2, 0.0, true, false);
    assert!(
        register_source_creature_linking(&links, &mut s.holder, &s.objects, master, 8).unwrap()
    );
    register_source_creature_linking(&links, &mut s.holder, &s.objects, master, 8).unwrap();
    let pet = s.actor(8, 3, 3, 0.0, true, true);
    register_source_creature_linking(&links, &mut s.holder, &s.objects, pet, 8).unwrap();
    let inventory = s.holder.inventory();
    assert_eq!(inventory.entries[&8][0].slaves.len(), 2);
    assert!(!inventory.entries[&8][0].in_use);
    assert_eq!(inventory.masters[&8].len(), 1);
}
#[test]
fn guid_only_event_masters_do_not_enter_the_entry_master_index() {
    let mut d = Definitions::default();
    d.spawns.insert(1, spawn(7, 0, 0.0));
    d.spawns.insert(2, spawn(8, 0, 0.0));
    let links = loaded(vec![], vec![guid_row(1, 2, 1)], &mut d);
    let mut s = Scene::new();
    let master = s.actor(8, 2, 2, 0.0, true, false);
    assert!(
        register_source_creature_linking(&links, &mut s.holder, &s.objects, master, 8).unwrap()
    );
    assert!(s.holder.inventory().masters.is_empty());
}
#[test]
fn deadline_equality_reads_fresh_wall_only_for_nonzero_deadlines() {
    let mut d = Definitions::default();
    d.unique.insert((8, 0), vec![2]);
    let links = loaded(
        vec![template(7, 0, 8, LINKING_BOSS_DEAD, 0)],
        vec![],
        &mut d,
    );
    let mut s = Scene::new();
    let slave = s.actor(7, 1, 1, 0.0, true, false);
    let mut gate = Gate::default();
    assert!(s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    assert_eq!(
        gate.calls,
        vec![Call::Encounter, Call::Data(2), Call::Pool(2)]
    );
    s.state.set_respawn(GridSpawnKind::Creature, 2, 100, 0);
    gate.calls.clear();
    gate.clocks
        .push_back(ClockSample::authored(ClockKind::WallSeconds, 100));
    assert!(s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    assert_eq!(
        gate.calls,
        vec![Call::Encounter, Call::Wall(2), Call::Data(2), Call::Pool(2)]
    );
    gate.calls.clear();
    gate.clocks
        .push_back(ClockSample::authored(ClockKind::WallSeconds, 99));
    assert!(!s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    assert_eq!(gate.calls, vec![Call::Encounter, Call::Wall(2)]);
}
#[test]
fn readiness_uses_actual_per_state_typed_pool_membership_after_recursive_readiness() {
    let links = loaded(vec![], vec![], &mut Definitions::default());
    let mut s = Scene::new();
    let mut gate = Gate::default();
    gate.pools.insert(2, 7);
    s.state.pools_mut().add(PoolMember::GameObject(2), 7);
    assert!(!s
        .holder
        .respawn_ready(&links, &s.objects, &s.state, &mut gate, 2)
        .unwrap());
    s.state.pools_mut().add(PoolMember::Creature(2), 7);
    assert!(s
        .holder
        .respawn_ready(&links, &s.objects, &s.state, &mut gate, 2)
        .unwrap());
    s.state.pools_mut().remove(PoolMember::Creature(2), 7);
    assert!(!s
        .holder
        .respawn_ready(&links, &s.objects, &s.state, &mut gate, 2)
        .unwrap());
}
#[test]
fn instance_gate_is_only_on_map_wide_boss_dead_and_dead_flag_wins() {
    for flags in [
        LINKING_BOSS_DEAD,
        LINKING_BOSS_DEAD | LINKING_BOSS_ALIVE,
        LINKING_BOSS_ALIVE,
    ] {
        let mut d = Definitions::default();
        d.unique.insert((8, 0), vec![2]);
        let links = loaded(vec![template(7, 0, 8, flags, 0)], vec![], &mut d);
        let mut s = Scene::new();
        let slave = s.actor(7, 1, 1, 0.0, true, false);
        let mut gate = Gate {
            encounter: Some(true),
            ..Gate::default()
        };
        assert!(!s
            .holder
            .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
            .unwrap());
        if flags & LINKING_BOSS_DEAD != 0 {
            assert_eq!(gate.calls, vec![Call::Encounter]);
        } else {
            assert_eq!(gate.calls, vec![Call::Data(2), Call::Pool(2)]);
        }
    }
}
#[test]
fn acyclic_readiness_preserves_unwind_order_and_cycles_refuse_instead_of_recursing_forever() {
    let mut d = Definitions::default();
    for guid in 1..=3 {
        d.spawns.insert(guid, spawn(guid + 6, 0, 0.0));
    }
    let links = loaded(
        vec![],
        vec![
            guid_row(3, 1, LINKING_BOSS_DEAD),
            guid_row(1, 2, LINKING_BOSS_DEAD),
        ],
        &mut d,
    );
    let s = Scene::new();
    let mut gate = Gate {
        spawns: d.spawns.clone(),
        ..Gate::default()
    };
    assert!(s
        .holder
        .can_spawn_guid(&links, &s.objects, &s.state, &mut gate, 3)
        .unwrap());
    assert_eq!(
        gate.calls,
        vec![
            Call::Data(3),
            Call::Encounter,
            Call::Data(1),
            Call::Encounter,
            Call::Data(2),
            Call::Pool(2),
            Call::Pool(1)
        ]
    );
    let cyclic = loaded(
        vec![],
        vec![
            guid_row(1, 2, LINKING_BOSS_DEAD),
            guid_row(2, 1, LINKING_BOSS_DEAD),
        ],
        &mut d,
    );
    let mut gate = Gate {
        spawns: d.spawns,
        ..Gate::default()
    };
    assert_eq!(
        s.holder
            .can_spawn_guid(&cyclic, &s.objects, &s.state, &mut gate, 1)
            .unwrap_err()
            .id,
        "execution.linking.recursive_readiness_undefined"
    );
}
#[test]
fn range_square_defined_boundary_and_first_undefined_neighbour() {
    // uint16 operands promote to signed int before the product; cpp:659.
    assert!(linking_in_range([0, 0], [0, 0], 46340).unwrap());
    assert!(!linking_in_range([0, 0], [46340.0_f32.to_bits(), 0], 46340).unwrap());
    assert_eq!(
        linking_in_range([0, 0], [0, 0], 46341).unwrap_err().id,
        "execution.linking.range_square_undefined"
    );
    assert!(!linking_in_range([0, 0], [6.0_f32.to_bits(), 8.0_f32.to_bits()], 10).unwrap());
    assert!(linking_in_range([f32::NAN.to_bits(), 0], [0, 0], 0).unwrap());
}
#[test]
fn range_overflow_is_not_evaluated_until_a_live_local_master_is_reached() {
    let links = loaded(
        vec![template(7, 0, 8, LINKING_BOSS_DEAD, 46341)],
        vec![],
        &mut Definitions::default(),
    );
    let mut s = Scene::new();
    let slave = s.actor(7, 1, 1, 0.0, true, false);
    let mut gate = Gate::default();
    assert!(s
        .holder
        .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
        .unwrap());
    let master = s.actor(8, 2, 2, 0.0, true, false);
    s.holder.add_master(&links, &s.objects, master, 8).unwrap();
    assert_eq!(
        s.holder
            .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
            .unwrap_err()
            .id,
        "execution.linking.range_square_undefined"
    );
}
#[test]
fn missing_instance_and_clock_owners_and_wrong_map_binding_refuse() {
    let mut d = Definitions::default();
    d.unique.insert((8, 0), vec![2]);
    let links = loaded(
        vec![template(7, 0, 8, LINKING_BOSS_DEAD, 0)],
        vec![],
        &mut d,
    );
    let mut s = Scene::new();
    let slave = s.actor(7, 1, 1, 0.0, true, false);
    let mut gate = Gate {
        refuse_encounter: true,
        ..Gate::default()
    };
    assert_eq!(
        s.holder
            .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
            .unwrap_err()
            .id,
        "authored.instance_owner"
    );
    gate.refuse_encounter = false;
    s.state.set_respawn(GridSpawnKind::Creature, 2, 100, 0);
    assert_eq!(
        s.holder
            .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
            .unwrap_err()
            .id,
        "authored.wall_owner"
    );
    gate.clocks
        .push_back(ClockSample::authored(ClockKind::MonotonicNanoseconds, 100));
    assert_eq!(
        s.holder
            .can_spawn_creature(&links, &s.objects, &s.state, &mut gate, slave, 7)
            .unwrap_err()
            .id,
        "execution.linking.wall_clock_kind"
    );
    s.state.used_by_map = None;
    assert_eq!(
        s.holder
            .can_spawn_guid(&links, &s.objects, &s.state, &mut gate, 1)
            .unwrap_err()
            .id,
        "execution.linking.persistent_map_binding"
    );
    assert_eq!(
        SourceCreatureLinking::default()
            .ensure_loaded()
            .unwrap_err()
            .id,
        "execution.linking.loader_owner"
    );
}

#[test]
fn defined_nonpassive_self_links_are_retained_not_repaired_as_readiness_cycles() {
    let mut d = Definitions::default();
    let links = loaded(
        vec![
            template(11460, 429, 11460, 3, 0),
            template(10316, 229, 10316, 3, 0),
            template(1725, 36, 1725, 3, 30),
        ],
        vec![],
        &mut d,
    );
    assert!(links.inventory().rejected.is_empty());
    assert_eq!(links.inventory().entries.len(), 3);
    assert!(d.queries.is_empty());
    for (entry, map) in [(11460, 429), (10316, 229), (1725, 36)] {
        assert!(!links.lookup(entry, 0, map).unwrap().spawning_linked());
    }
}
#[test]
fn retained_catalog04_query_streams_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        println!("SKIP retained linking catalog rows: set BENILLA_F2_CATALOG and BENILLA_F2_GROUP_OCCURRENCES");
        return;
    };
    let occurrences = std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES")
        .expect("explicit catalog requires occurrences");
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let mut rows = catalog
        .execution_linking_rows(Path::new(&occurrences))
        .unwrap();
    let mut templates = 0;
    let mut guids = 0;
    let mut maximum = 0;
    while let Some(row) = LinkingRows::next_template(&mut rows).unwrap() {
        assert_eq!(row.ordinal, templates);
        templates += 1;
        maximum = maximum.max(row.range);
    }
    while let Some(row) = LinkingRows::next_guid(&mut rows).unwrap() {
        assert_eq!(row.ordinal, guids);
        guids += 1;
    }
    assert_eq!((templates, guids), (155, 1188));
    assert!(maximum <= 46340);
    assert!(matches!(
        query_catalog_linking_master(&mut catalog, u32::MAX, 0).unwrap(),
        LinkingMasterQuery::NoResult
    ));
    let LinkingMasterQuery::Rows(single) =
        query_catalog_linking_master(&mut catalog, 330, 0).unwrap()
    else {
        panic!("retained unique master absent")
    };
    assert_eq!(single, vec![80849]);
    let LinkingMasterQuery::Rows(multiple) =
        query_catalog_linking_master(&mut catalog, 8920, 230).unwrap()
    else {
        panic!("retained nonunique master absent")
    };
    assert_eq!(multiple.len(), 2);
    println!("Unsealed diagnostic catalog-04 linking query rows verified: {templates} templates, {guids} GUIDs; no effective-manager or F1 acceptance");
}

#[path = "creature_linking_event_tests.rs"]
mod event_tests;
