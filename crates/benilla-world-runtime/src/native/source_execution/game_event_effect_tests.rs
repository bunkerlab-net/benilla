// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored callback owners, not production F5 gameplay or sealed equivalence.
use super::*;
use benilla_sim::resident_map::CellPair;

fn map(id: u32) -> MapIdentity {
    MapIdentity {
        map: id,
        instance: 0,
        creation_generation: 1,
    }
}
fn key(kind: GridSpawnKind, guid: u32) -> SpawnKey {
    SpawnKey { kind, guid }
}
fn data(guid: u32) -> GameEventSubstitution {
    GameEventSubstitution {
        guid,
        entry: 12,
        model: 13,
        equipment: 14,
        start_spell: 15,
        end_spell: 16,
    }
}
fn definition(guid: u32, map: u32) -> EventSpawnDefinition {
    EventSpawnDefinition {
        map,
        cell: CellPair::from_indices(512, 512).unwrap(),
        entry: 10,
        guid: u64::from(guid),
    }
}
struct Host {
    calendar: Owner<'static>,
    pools: SourcePoolManager,
    states: SourceManagers,
    effects: SourceEventEffects,
    definitions: BTreeMap<SpawnKey, EventSpawnDefinition>,
    maps: Vec<MapIdentity>,
    queues: BTreeMap<MapIdentity, SourceMessager>,
    calls: Vec<String>,
    mails: Vec<(u64, EventMailTask)>,
    sql: Vec<(u64, String)>,
    message_causes: Vec<(u64, u64)>,
    live: BTreeSet<(MapIdentity, u64)>,
    instances: BTreeSet<MapIdentity>,
    refused: Option<EventPending>,
    update_false: bool,
    nested_update: bool,
    next_cause: u64,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            calendar: Owner::default(),
            pools: pools(false),
            states: SourceManagers::default(),
            effects: SourceEventEffects::default(),
            definitions: BTreeMap::new(),
            maps: vec![],
            queues: BTreeMap::new(),
            calls: vec![],
            mails: vec![],
            sql: vec![],
            message_causes: vec![],
            live: BTreeSet::new(),
            instances: BTreeSet::new(),
            refused: None,
            update_false: false,
            nested_update: false,
            next_cause: 100,
        }
    }
}
impl Host {
    fn load(&mut self, rows: &mut Rows) -> SourceGameEventManager {
        let mut manager = SourceGameEventManager::default();
        let mut pools = std::mem::take(&mut self.pools);
        manager.load(rows, &mut pools, self).unwrap();
        self.pools = pools;
        manager
    }
    fn apply(&mut self, manager: &SourceGameEventManager, effect: GameEventEffect) -> Result<()> {
        let mut effects = std::mem::take(&mut self.effects);
        let mut pools = std::mem::take(&mut self.pools);
        let mut states = std::mem::take(&mut self.states);
        self.next_cause += 1;
        let result = effects.execute(
            manager,
            effect,
            self.next_cause,
            &mut pools,
            &mut states,
            self,
        );
        self.effects = effects;
        self.pools = pools;
        self.states = states;
        result
    }
    fn world(&mut self) -> Result<Vec<MessageCompletion>> {
        let mut effects = std::mem::take(&mut self.effects);
        let batch = effects.world.take_batch(MessageOwner::World)?;
        let done = batch.execute(
            MessageOwner::World,
            1,
            &mut EventMessageDispatch::new(&mut effects, self),
        );
        self.effects = effects;
        done
    }
    fn deliver(&mut self, map: MapIdentity) -> Result<Vec<MessageCompletion>> {
        let batch = self
            .queues
            .get_mut(&map)
            .unwrap()
            .take_batch(MessageOwner::Map(map))?;
        let mut effects = std::mem::take(&mut self.effects);
        let done = batch.execute(
            MessageOwner::Map(map),
            2,
            &mut EventMessageDispatch::new(&mut effects, self),
        );
        self.effects = effects;
        done
    }
    fn gate(&self, pending: EventPending) -> Result<()> {
        if self.refused == Some(pending) {
            Err(pending.refusal())
        } else {
            Ok(())
        }
    }
}
impl GameEventOwner for Host {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample> {
        self.calendar.clock(site)
    }
    fn reference(&mut self, kind: EventReference, id: u32) -> Result<bool> {
        self.calendar.reference(kind, id)
    }
    fn set_quest_active(&mut self, quest: u32, active: bool) -> Result<()> {
        self.event_quest(quest, active)
    }
    fn check_pool_event(
        &mut self,
        _: &SourcePoolManager,
        _: u16,
        _: i16,
        _: &BTreeMap<u32, i16>,
        _: &BTreeMap<u32, i16>,
    ) -> Result<()> {
        Ok(())
    }
    fn effect(
        &mut self,
        manager: &mut SourceGameEventManager,
        effect: GameEventEffect,
    ) -> Result<()> {
        self.calendar.effects.push(effect);
        if let GameEventEffect::Notify { event, active, .. } = effect {
            assert_eq!(manager.active(u32::from(event)), active);
        }
        self.apply(manager, effect)
    }
    fn observe(&mut self, event: GameEventObservation) -> Result<()> {
        self.calendar.observe(event)
    }
}
impl PoolWork for Host {
    fn engine_access(&mut self) -> Result<()> {
        self.calls.push("engine".into());
        Ok(())
    }
    fn chance(&mut self) -> Result<f64> {
        Ok(0.0)
    }
    fn shuffle(&mut self, _: &mut [usize]) -> Result<()> {
        Ok(())
    }
    fn can_spawn(
        &mut self,
        _: &mut SourcePoolManager,
        _: &mut SourceManagers,
        _: PersistentStateIdentity,
        _: PoolKind,
        _: u32,
    ) -> Result<bool> {
        Ok(true)
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        _: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        self.calls
            .push(format!("pool:{:?}:{}", effect.action, effect.guid));
        Ok(())
    }
    fn observe(&mut self, _: PoolOperation) -> Result<()> {
        Ok(())
    }
}
impl PoolStateOwner for Host {
    fn pool_map_entry(&mut self, id: u32) -> Result<Option<PersistentMapEntry>> {
        Ok(Some(PersistentMapEntry { id, map_type: 0 }))
    }
    fn pool_next_instance_state(
        &mut self,
        _: &SourceManagers,
        _: u32,
        _: Option<PersistentStateIdentity>,
    ) -> Result<Option<PersistentStateIdentity>> {
        panic!("world map")
    }
}
impl EventEffectOwner for Host {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>> {
        Ok(self.definitions.get(&key).copied())
    }
    fn event_next_map(
        &mut self,
        id: Option<u32>,
        after: Option<MapIdentity>,
    ) -> Result<Option<MapIdentity>> {
        let start = after.map_or(0, |map| {
            self.maps.iter().position(|v| *v == map).unwrap() + 1
        });
        Ok(self.maps[start..]
            .iter()
            .copied()
            .find(|map| id.is_none_or(|id| map.map == id)))
    }
    fn event_post_map(&mut self, map: MapIdentity, message: MessageEnvelope) -> Result<()> {
        self.queues
            .entry(map)
            .or_insert_with(|| SourceMessager::new(MessageOwner::Map(map)))
            .enqueue(message)
    }
    fn event_sql(&mut self, cause: u64, sql: &str) -> Result<()> {
        self.calls.push(sql.into());
        self.sql.push((cause, sql.into()));
        Ok(())
    }
    fn event_announce(&mut self, event: u16, _: &[u8]) -> Result<()> {
        self.calls.push(format!("announce:{event}"));
        Ok(())
    }
    fn event_quest(&mut self, quest: u32, active: bool) -> Result<()> {
        self.gate(EventPending::Quest)?;
        self.calls.push(format!("quest:{quest}:{active}"));
        self.calendar.quest_flags.insert(quest, active);
        Ok(())
    }
    fn event_mail(&mut self, cause: u64, task: EventMailTask) -> Result<()> {
        self.gate(EventPending::Mail)?;
        self.calls.push(format!("mail:{}", task.template));
        self.mails.push((cause, task));
        Ok(())
    }
    fn event_has_instance(&mut self, map: MapIdentity) -> Result<bool> {
        Ok(self.instances.contains(&map))
    }
    fn event_notify(
        &mut self,
        map: MapIdentity,
        event: u16,
        active: bool,
        resume: bool,
    ) -> Result<()> {
        self.gate(EventPending::Instance)?;
        self.calls
            .push(format!("notify:{}:{event}:{active}:{resume}", map.map));
        Ok(())
    }
}
impl EventCreatureOwner for Host {
    fn event_update_entry(
        &mut self,
        map: MapIdentity,
        update: EventCreatureUpdate,
    ) -> Result<bool> {
        self.gate(EventPending::UpdateEntry)?;
        self.calls.push(format!(
            "entry:{}:{}:{:?}",
            map.map,
            update.definition.entry,
            update.activate.then_some(update.data.entry)
        ));
        if update.activate && !self.update_false {
            apply_event_spells(map, update.definition.guid, update.data, true, self)?;
        }
        Ok(!self.update_false)
    }
    fn event_spell_aura(&mut self, spell: u32) -> Result<bool> {
        self.gate(EventPending::Spell)?;
        self.calls.push(format!("lookup-aura:{spell}"));
        Ok(spell != 99)
    }
    fn event_remove_aura(&mut self, _: MapIdentity, guid: u64, spell: u32) -> Result<()> {
        self.calls.push(format!("remove-aura:{guid}:{spell}"));
        Ok(())
    }
    fn event_cast_triggered(&mut self, _: MapIdentity, guid: u64, spell: u32) -> Result<()> {
        self.calls.push(format!("cast-triggered:{guid}:{spell}"));
        Ok(())
    }
}
impl EventMessageOwner for Host {
    fn event_message_begin(&mut self, message: &MessageEnvelope) -> Result<()> {
        self.message_causes.push((message.id, message.cause));
        Ok(())
    }
    fn event_map_message(
        &mut self,
        _: u64,
        map: MapIdentity,
        creatures: &[u32],
        objects: &[u32],
        spawn: bool,
    ) -> Result<()> {
        self.calls
            .push(format!("map:{}:{creatures:?}:{objects:?}:{spawn}", map.map));
        for &guid in creatures {
            if spawn {
                self.live.insert((map, u64::from(guid)));
            } else {
                self.live.remove(&(map, u64::from(guid)));
            }
        }
        if self.nested_update {
            self.nested_update = false;
            self.queues
                .get_mut(&map)
                .unwrap()
                .enqueue(MessageEnvelope {
                    id: 1000,
                    cause: 2,
                    payload: SourceMessage::UpdateCreatureEvent {
                        update: EventCreatureUpdate {
                            definition: definition(7, map.map),
                            data: data(7),
                            activate: true,
                        },
                    },
                })?;
        }
        Ok(())
    }
    fn event_creature_present(&self, map: MapIdentity, guid: u64) -> bool {
        self.live.contains(&(map, guid))
    }
}
fn members() -> Rows {
    let mut rows = rows(1);
    for (ordinal, guid, event) in [(0, 7, 1), (1, 8, -1)] {
        rows.creatures.push_back(GameEventMemberRow {
            ordinal,
            exists: true,
            guid,
            event,
        });
    }
    rows.objects.push_back(GameEventMemberRow {
        ordinal: 0,
        exists: true,
        guid: 9,
        event: 1,
    });
    rows
}
fn host_definitions(host: &mut Host) {
    for (kind, guid) in [
        (GridSpawnKind::Creature, 7),
        (GridSpawnKind::Creature, 8),
        (GridSpawnKind::GameObject, 9),
    ] {
        host.definitions
            .insert(key(kind, guid), definition(guid, 0));
    }
}
#[test]
fn start_stop_signed_membership_and_noninverse_callback_order() {
    // Active/status precede spawn/unspawn, and both transitions finish with notify; GameEventMgr.cpp:714-765.
    let mut host = Host::default();
    host_definitions(&mut host);
    host.maps.push(map(0));
    host.instances.insert(map(0));
    let mut rows = members();
    rows.quests.push_back((0, 20, 1));
    rows.mails.push_back(GameEventMailRow {
        ordinal: 0,
        event: 1,
        mail: GameEventMail {
            race_mask: 5,
            quest: 0,
            template: 30,
            sender: 40,
        },
    });
    let mut manager = host.load(&mut rows);
    host.calls.clear();
    manager
        .start_event(1, false, false, true, &mut host)
        .unwrap();
    assert_eq!(
        host.calls,
        [
            "INSERT INTO game_event_status (event) VALUES (1)",
            "announce:1",
            "quest:20:true",
            "mail:30",
            "notify:0:1:true:false"
        ]
    );
    assert_eq!(host.effects.world.pending(), 2);
    assert!(host
        .effects
        .dynamic_guids(0, GridSpawnKind::Creature)
        .is_empty());
    let world = host.world().unwrap();
    assert_eq!(
        world.iter().map(|m| m.cause).collect::<Vec<_>>(),
        [103, 104]
    );
    assert_eq!(host.effects.dynamic_guids(0, GridSpawnKind::Creature), [7]);
    assert_eq!(
        host.effects.dynamic_guids(0, GridSpawnKind::GameObject),
        [9]
    );
    let done = host.deliver(map(0)).unwrap();
    assert_eq!(done.iter().map(|m| m.cause).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(host.message_causes, [(1, 103), (2, 104), (3, 1), (4, 2)]);
    host.calls.clear();
    manager.stop_event(1, false, &mut host).unwrap();
    assert_eq!(
        host.calls,
        [
            "DELETE FROM game_event_status WHERE event = 1",
            "quest:20:false",
            "notify:0:1:false:false"
        ]
    );
    host.world().unwrap();
    host.deliver(map(0)).unwrap();
    assert_eq!(host.effects.dynamic_guids(0, GridSpawnKind::Creature), [8]);
    assert!(host
        .effects
        .dynamic_guids(0, GridSpawnKind::GameObject)
        .is_empty());
    assert!(!manager.active(1));
    assert_eq!(
        host.calendar.quest_flags[&20],
        manager.inventory().quest_active[&20]
    );
    assert_eq!(
        host.sql[0],
        (
            101,
            "INSERT INTO game_event_status (event) VALUES (1)".into()
        )
    );
}
#[test]
fn world_delivery_sees_new_map_and_map_batch_sees_new_creature_later() {
    // Map traversal and creature lookup happen when their stages execute; GameEventMgr.cpp:837-849,1026.
    let mut host = Host::default();
    host_definitions(&mut host);
    let manager = host.load(&mut members());
    host.apply(&manager, GameEventEffect::Spawn(1)).unwrap();
    assert!(host.maps.is_empty());
    host.maps.push(map(0));
    host.nested_update = true;
    host.world().unwrap();
    assert!(!host.live.contains(&(map(0), 7)));
    assert_eq!(host.queues[&map(0)].pending(), 1);
    let done = host.deliver(map(0)).unwrap();
    assert_eq!(done.len(), 1);
    assert!(host.live.contains(&(map(0), 7)));
    assert_eq!(host.queues[&map(0)].pending(), 1);
    assert!(!host.calls.iter().any(|s| s.starts_with("entry:")));
    host.deliver(map(0)).unwrap();
    assert_eq!(
        &host.calls[1..],
        [
            "entry:0:10:Some(12)",
            "lookup-aura:16",
            "remove-aura:7:16",
            "cast-triggered:7:15"
        ]
    );
}
#[test]
fn one_world_closure_groups_maps_in_key_order_and_preserves_duplicates() {
    // The source map is ordered and ObjectMgr appends duplicates; GameEventMgr.cpp:783,833; ObjectMgr.cpp:928-942.
    let mut host = Host::default();
    let mut rows = members();
    rows.creatures.push_back(GameEventMemberRow {
        ordinal: 2,
        exists: true,
        guid: 7,
        event: 1,
    });
    host_definitions(&mut host);
    host.maps = vec![map(2), map(0)];
    host.definitions
        .get_mut(&key(GridSpawnKind::Creature, 7))
        .unwrap()
        .map = 2;
    let manager = host.load(&mut rows);
    host.apply(&manager, GameEventEffect::Spawn(1)).unwrap();
    assert_eq!(host.effects.world.pending(), 1);
    host.world().unwrap();
    assert_eq!(
        host.effects.dynamic_guids(2, GridSpawnKind::Creature),
        [7, 7]
    );
    assert_eq!(host.deliver(map(2)).unwrap()[0].id, 3);
    assert_eq!(host.deliver(map(0)).unwrap()[0].id, 2);
    host.apply(&manager, GameEventEffect::Unspawn(1)).unwrap();
    host.world().unwrap();
    assert!(host
        .effects
        .dynamic_guids(2, GridSpawnKind::Creature)
        .is_empty());
}
#[test]
fn concurrent_substitutions_use_first_occurrence_not_lowest_event() {
    // equal_range insertion order wins, then the first per-event GUID row; GameEventMgr.cpp:993-1012.
    let mut host = Host::default();
    let mut rows = rows(2);
    for (ordinal, event, entry) in [(0, 2, 22), (1, 1, 11), (2, 2, 23)] {
        let mut value = data(7);
        value.entry = entry;
        rows.substitutions.push_back(GameEventSubstitutionRow {
            ordinal,
            exists: true,
            event,
            data: value,
        });
    }
    let mut manager = host.load(&mut rows);
    manager
        .start_event(1, false, true, false, &mut host)
        .unwrap();
    manager
        .start_event(2, false, true, false, &mut host)
        .unwrap();
    assert_eq!(manager.creature_event_data(7).unwrap().unwrap().entry, 22);
    assert_eq!(
        manager.creature_event_data(7).unwrap().unwrap().start_spell,
        15
    );
    manager.stop_event(2, false, &mut host).unwrap();
    assert_eq!(manager.creature_event_data(7).unwrap().unwrap().entry, 11);
}
#[test]
fn substitution_messages_capture_each_duplicate_row_and_skip_absent_guid() {
    // Worker captures the specific row and source object GUID, not a current first-match answer; GameEventMgr.cpp:1024-1054.
    let mut host = Host::default();
    host.maps.push(map(0));
    host.definitions
        .insert(key(GridSpawnKind::Creature, 7), definition(7, 0));
    let mut rows = rows(1);
    for (ordinal, entry) in [(0, 11), (1, 22)] {
        let mut value = data(7);
        value.entry = entry;
        rows.substitutions.push_back(GameEventSubstitutionRow {
            ordinal,
            exists: true,
            event: 1,
            data: value,
        });
    }
    let manager = host.load(&mut rows);
    host.apply(
        &manager,
        GameEventEffect::Substitutions {
            event: 1,
            active: true,
        },
    )
    .unwrap();
    let absent = host.deliver(map(0)).unwrap();
    assert_eq!(absent.len(), 2);
    assert!(absent
        .iter()
        .all(|m| m.outcome == MessageOutcome::CreatureAbsent));
    host.live.insert((map(0), 7));
    host.apply(
        &manager,
        GameEventEffect::Substitutions {
            event: 1,
            active: true,
        },
    )
    .unwrap();
    host.deliver(map(0)).unwrap();
    assert_eq!(
        host.calls
            .iter()
            .filter(|s| s.starts_with("entry:"))
            .cloned()
            .collect::<Vec<_>>(),
        ["entry:0:10:Some(11)", "entry:0:10:Some(22)"]
    );
}
#[test]
fn unapply_ignores_update_false_then_removes_start_aura_and_casts_end() {
    // A false UpdateEntry does not suppress end spells; GameEventMgr.cpp:1028-1032; Creature.cpp:2713-2722.
    let mut host = Host {
        update_false: true,
        ..Host::default()
    };
    execute_creature_event(
        map(0),
        EventCreatureUpdate {
            definition: definition(7, 0),
            data: data(7),
            activate: false,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.calls,
        [
            "entry:0:10:None",
            "lookup-aura:15",
            "remove-aura:7:15",
            "cast-triggered:7:16"
        ]
    );
    host.calls.clear();
    let mut value = data(7);
    value.start_spell = 99;
    value.end_spell = 0;
    apply_event_spells(map(0), 7, value, false, &mut host).unwrap();
    assert_eq!(host.calls, ["lookup-aura:99"]);
}
#[test]
fn resume_suppresses_only_mail_and_negative_dispatch_uses_its_own_slot() {
    // Resume skips SendEventMails, stop still calls the negative slot; GameEventMgr.cpp:729,760-764.
    let mut host = Host::default();
    let mut rows = rows(1);
    rows.mails.push_back(GameEventMailRow {
        ordinal: 0,
        event: 1,
        mail: GameEventMail {
            race_mask: 17,
            quest: 0,
            template: 30,
            sender: 40,
        },
    });
    let mut manager = host.load(&mut rows);
    manager
        .start_event(1, false, true, false, &mut host)
        .unwrap();
    assert!(host.mails.is_empty());
    host.apply(&manager, GameEventEffect::Mail(-1)).unwrap();
    assert!(host.mails.is_empty());
    manager.stop_event(1, false, &mut host).unwrap();
    manager
        .start_event(1, false, false, false, &mut host)
        .unwrap();
    assert_eq!(host.mails.len(), 1);
    assert_eq!(host.mails[0].1.selection, EventMailSelection::Race(17));
}
#[test]
fn mail_quest_and_race_tasks_preserve_exact_selection_sql() {
    // The mail loader reads uint16, so negative catalog IDs cannot populate negative slots; GameEventMgr.cpp:576-596.
    let mut host = Host::default();
    let mut rows = rows(1);
    for (ordinal, event, quest, template) in [(0, 1, 0, 30), (1, 1, 99, 31), (2, 65535, 99, 32)] {
        rows.mails.push_back(GameEventMailRow {
            ordinal,
            event,
            mail: GameEventMail {
                race_mask: 129,
                quest,
                template,
                sender: 40,
            },
        });
    }
    let manager = host.load(&mut rows);
    host.apply(&manager, GameEventEffect::Mail(1)).unwrap();
    host.apply(&manager, GameEventEffect::Mail(-1)).unwrap();
    assert_eq!(host.mails.len(), 2);
    assert_eq!(host.mails[0].1.selection, EventMailSelection::Race(129));
    assert_eq!(host.mails[1].1.selection,EventMailSelection::RewardedQuest("SELECT characters.guid FROM characters, character_queststatus WHERE (1 << (characters.race - 1)) & 129 AND characters.deleteDate IS NULL AND character_queststatus.guid = characters.guid AND character_queststatus.quest = 99 AND character_queststatus.rewarded <> 0".into()));
}
#[test]
fn notify_skips_maps_without_instance_data_and_retains_source_map_order() {
    // InstanceData guards the whole map notification, not only the instance callback; GameEventMgr.cpp:1155-1158.
    let mut host = Host {
        maps: vec![map(2), map(0), map(1)],
        instances: BTreeSet::from([map(2), map(1)]),
        ..Host::default()
    };
    let manager = host.load(&mut rows(1));
    host.apply(
        &manager,
        GameEventEffect::Notify {
            event: 1,
            active: false,
            resume: false,
        },
    )
    .unwrap();
    assert_eq!(
        host.calls,
        ["notify:2:1:false:false", "notify:1:1:false:false"]
    );
}
#[test]
fn reached_f5_refusals_stop_before_later_effects() {
    // Reached missing handlers are errors, never empty success; GameEventMgr.cpp:1028,1069,1091,1158.
    for pending in [
        EventPending::Quest,
        EventPending::Mail,
        EventPending::Instance,
    ] {
        let mut host = Host::default();
        let mut rows = rows(1);
        rows.quests.push_back((0, 20, 1));
        rows.mails.push_back(GameEventMailRow {
            ordinal: 0,
            event: 1,
            mail: GameEventMail {
                race_mask: 1,
                quest: 0,
                template: 30,
                sender: 40,
            },
        });
        let mut manager = host.load(&mut rows);
        host.maps.push(map(0));
        host.instances.insert(map(0));
        host.refused = Some(pending);
        assert_eq!(
            manager
                .start_event(1, false, false, false, &mut host)
                .unwrap_err(),
            pending.refusal()
        );
        assert!(manager.active(1));
        assert!(manager.inventory().stopped);
        assert_eq!(
            host.effects
                .execute(
                    &manager,
                    GameEventEffect::Mail(1),
                    1,
                    &mut SourcePoolManager::default(),
                    &mut SourceManagers::default(),
                    &mut Host::default()
                )
                .unwrap_err()
                .id,
            "execution.event.effects_stopped"
        );
    }
}
#[test]
fn event_987_refuses_missing_shutdown_before_status_or_spawns() {
    // Event 987 branches before status INSERT and all ordinary effects; GameEventMgr.cpp:738-744; World.cpp:2081-2103.
    let mut host = Host::default();
    let mut rows = Rows {
        maximum: Some(987),
        definitions: VecDeque::from([super::definition(987)]),
        ..Rows::default()
    };
    rows.definitions[0].schedule = 0;
    rows.definitions[0].length = 2;
    let mut manager = host.load(&mut rows);
    assert_eq!(
        manager
            .start_event(987, false, false, true, &mut host)
            .unwrap_err(),
        EventPending::Shutdown.refusal()
    );
    assert!(manager.active(987));
    assert!(host.sql.is_empty());
    assert!(host.calls.is_empty());
    assert_eq!(
        host.calendar.effects,
        [GameEventEffect::Restart { seconds: 120 }]
    );
}
#[test]
fn negative_pool_members_toggle_exclusion_and_enter_real_global_pool_owner() {
    // Negative spawn permits next pool update; negative unspawn excludes and replaces the member; GameEventMgr.cpp:791-798,903-909.
    let mut host = Host {
        pools: SourcePoolManager::default(),
        ..Host::default()
    };
    let mut pool_rows = PoolRowsFixture {
        templates: VecDeque::from([PoolTemplateRow {
            ordinal: 0,
            id: 1,
            limit: 1,
            description: vec![],
        }]),
        members: VecDeque::from([
            PoolMembershipRow {
                ordinal: 0,
                guid: 7,
                pool: 1,
                chance: 0.0,
            },
            PoolMembershipRow {
                ordinal: 1,
                guid: 8,
                pool: 1,
                chance: 0.0,
            },
        ]),
    };
    host.pools.load(&mut pool_rows, &mut PoolOwner).unwrap();
    host_definitions(&mut host);
    let mut rows = rows(1);
    rows.creatures.push_back(GameEventMemberRow {
        ordinal: 0,
        exists: true,
        guid: 7,
        event: -1,
    });
    let manager = host.load(&mut rows);
    let mut states = SourceManagers::default();
    states
        .create_state(
            PersistentMapEntry { id: 0, map_type: 0 },
            0,
            0,
            false,
            true,
            0,
            &mut host,
        )
        .unwrap();
    host.states = states;
    host.apply(&manager, GameEventEffect::Spawn(-1)).unwrap();
    assert_eq!(host.effects.world.pending(), 0);
    assert!(host.calls.iter().any(|s| s.starts_with("pool:Spawn")));
    host.calls.clear();
    host.apply(&manager, GameEventEffect::Unspawn(-1)).unwrap();
    assert!(host.calls.iter().any(|s| s == "pool:Despawn:7"));
    assert!(host
        .calls
        .iter()
        .any(|s| s.starts_with("pool:Spawn") && s.ends_with(":8")));
    assert!(host.pools.pool_template(1).unwrap().groups[2].equal[0].excluded);
    assert_eq!(host.effects.world.pending(), 0);
}

impl PersistentEffects for Host {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Ok(())
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        panic!("not a query")
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        panic!("not a dungeon")
    }
}

#[test]
fn map_event_pools_use_current_active_set_and_source_iteration_order() {
    struct Maps;
    impl PoolLoadOwner for Maps {
        fn spawn_map(&mut self, _: PoolKind, guid: u32) -> Result<Option<u32>> {
            Ok(Some(u32::from(guid == 9)))
        }
        fn map_exists(&mut self, _: u32) -> Result<bool> {
            Ok(true)
        }
    }
    let mut pool_rows = PoolRowsFixture {
        templates: (1..=4)
            .map(|id| PoolTemplateRow {
                ordinal: u64::from(id - 1),
                id,
                limit: 1,
                description: vec![],
            })
            .collect(),
        members: (1..=4)
            .map(|pool| PoolMembershipRow {
                ordinal: u64::from(pool - 1),
                guid: u32::from(pool) + 6,
                pool,
                chance: 0.0,
            })
            .collect(),
    };
    let mut pools = SourcePoolManager::default();
    pools.load(&mut pool_rows, &mut Maps).unwrap();
    let mut event_rows = rows(2);
    event_rows.creatures = [(8, 1), (7, 1), (9, 2), (10, 2)]
        .into_iter()
        .enumerate()
        .map(|(ordinal, (guid, event))| GameEventMemberRow {
            ordinal: ordinal as u64,
            exists: true,
            guid,
            event,
        })
        .collect();
    let mut calendar = Owner::default();
    let mut manager = SourceGameEventManager::default();
    manager
        .load(&mut event_rows, &mut pools, &mut calendar)
        .unwrap();
    assert_eq!(manager.inventory().pools[1], [2, 1]);
    assert_eq!(manager.inventory().pools[2], [3, 4]);
    let mut host = Host::default();
    let mut states = SourceManagers::default();
    let state = states
        .create_state(
            PersistentMapEntry { id: 0, map_type: 0 },
            0,
            0,
            false,
            true,
            0,
            &mut host,
        )
        .unwrap();
    manager
        .initialize_state(&mut states, state, &mut pools, &mut host)
        .unwrap();
    assert!(host.calls.is_empty());
    // GameEventMgr.h:95,128-130,150-152: ascending event set, insertion-ordered pool lists.
    manager
        .start_event(2, false, false, false, &mut calendar)
        .unwrap();
    manager
        .start_event(1, false, false, false, &mut calendar)
        .unwrap();
    manager
        .initialize_state(&mut states, state, &mut pools, &mut host)
        .unwrap();
    let leaves = host
        .calls
        .iter()
        .filter(|call| call.starts_with("pool:"))
        .collect::<Vec<_>>();
    assert_eq!(
        leaves,
        [
            "pool:Spawn { instantly: true }:8",
            "pool:Spawn { instantly: true }:7",
            "pool:Spawn { instantly: true }:10",
        ]
    );
    host.calls.clear();
    let other = states
        .create_state(
            PersistentMapEntry { id: 1, map_type: 0 },
            0,
            0,
            false,
            true,
            0,
            &mut host,
        )
        .unwrap();
    manager
        .initialize_state(&mut states, other, &mut pools, &mut host)
        .unwrap();
    let leaves = host
        .calls
        .iter()
        .filter(|call| call.starts_with("pool:"))
        .collect::<Vec<_>>();
    assert_eq!(leaves, ["pool:Spawn { instantly: true }:9"]);
    assert_eq!(manager.inventory().active, BTreeSet::from([1, 2]));
    assert!(!manager.inventory().initialized);
    assert_eq!(host.effects.world.pending(), 0);
    assert!(host.sql.is_empty() && host.calendar.effects.is_empty());
    assert!(!pools.inventory().stopped);
}

#[test]
fn retained_event_effect_projection_or_explicit_absence() {
    let explicit = std::env::var_os("BENILLA_F4_EVENT_EFFECTS_REPORT");
    let path = explicit
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
                "../../benilla-config/full-server/f4/event-effects/retained-effect-projection.json",
            )
        });
    if explicit.is_none() && !path.try_exists().unwrap() {
        println!("SKIP unsealed P0/PL/ME event effects: set BENILLA_F4_EVENT_EFFECTS_REPORT");
        return;
    }
    let (_, raw) = metadata(&path, 1024 * 1024).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(report["reference_execution_sealed"], false);
    assert_eq!(report["whole_execution_accepted"], false);
    assert_eq!(report["independently_matched_effects"], 0);
    for recording in report["recordings"].as_array().unwrap() {
        println!("{} {}", recording["name"], recording["comparison"]);
    }
}

#[test]
fn mail_index_undefined_boundaries_do_not_repair_the_signed_slot() {
    // SendEventMails directly indexes the signed slot without a bounds check; GameEventMgr.cpp:1075-1077.
    let mut host = Host::default();
    let manager = host.load(&mut rows(1));
    for event in [-1, 1] {
        host.apply(&manager, GameEventEffect::Mail(event)).unwrap();
    }
    for event in [-2, 2] {
        let mut host = Host::default();
        let manager = host.load(&mut rows(1));
        assert_eq!(
            host.apply(&manager, GameEventEffect::Mail(event))
                .unwrap_err()
                .id,
            "execution.event.mail_index_undefined"
        );
    }
}

#[test]
fn global_event_changes_reuse_preexisting_objectmgr_dynamic_lists() {
    // ObjectMgr.cpp:925-942 mutates lists that also contain ordinary dynamic definitions.
    let mut host = Host {
        effects: SourceEventEffects::new(BTreeMap::from([(0, [vec![42, 7], vec![43]])])),
        ..Host::default()
    };
    host_definitions(&mut host);
    let manager = host.load(&mut members());
    host.apply(&manager, GameEventEffect::Spawn(1)).unwrap();
    host.world().unwrap();
    assert_eq!(
        host.effects.dynamic_guids(0, GridSpawnKind::Creature),
        [42, 7, 7]
    );
    host.apply(&manager, GameEventEffect::Unspawn(1)).unwrap();
    host.world().unwrap();
    assert_eq!(host.effects.dynamic_guids(0, GridSpawnKind::Creature), [42]);
    assert_eq!(
        host.effects.dynamic_guids(0, GridSpawnKind::GameObject),
        [43]
    );
}

#[test]
fn negative_pool_unspawn_without_replacement_leaves_selected_excluded_member() {
    // No successful RollOne means no DespawnObject, even with an excluded trigger; PoolManager.cpp:412-414,430-433.
    let mut host = Host {
        pools: pools(true),
        ..Host::default()
    };
    host_definitions(&mut host);
    let mut rows = rows(1);
    rows.creatures.push_back(GameEventMemberRow {
        ordinal: 0,
        exists: true,
        guid: 7,
        event: -1,
    });
    let manager = host.load(&mut rows);
    let mut states = SourceManagers::default();
    let state = states
        .create_state(
            PersistentMapEntry { id: 0, map_type: 0 },
            0,
            0,
            false,
            true,
            0,
            &mut host,
        )
        .unwrap();
    host.states = states;
    host.apply(&manager, GameEventEffect::Spawn(-1)).unwrap();
    host.calls.clear();
    host.apply(&manager, GameEventEffect::Unspawn(-1)).unwrap();
    assert_eq!(host.calls, ["engine"]);
    assert!(host
        .states
        .state(state)
        .unwrap()
        .pools()
        .contains(PoolMember::Creature(7)));
    assert!(host.pools.pool_template(1).unwrap().groups[2].equal[0].excluded);
}

#[test]
fn manual_start_of_stock_shaped_987_hole_reaches_zero_second_shutdown() {
    // The recorded-zero hole cannot activate by time, but manual start still branches at GameEventMgr.cpp:736-741.
    let mut host = Host::default();
    let mut rows = Rows {
        maximum: Some(1024),
        definitions: VecDeque::from([super::definition(1024)]),
        ..Rows::default()
    };
    let mut manager = host.load(&mut rows);
    assert!(!manager.check_one(987, 0).unwrap());
    assert!(!manager.check_one(987, 2).unwrap());
    assert_eq!(
        manager
            .start_event(987, false, false, false, &mut host)
            .unwrap_err(),
        EventPending::Shutdown.refusal()
    );
    assert!(manager.active(987));
    assert_eq!(
        host.calendar.effects,
        [GameEventEffect::Restart { seconds: 0 }]
    );
    assert!(host.sql.is_empty());
}

#[test]
fn missing_creature_update_and_spell_owners_refuse_at_distinct_calls() {
    // UpdateEntry precedes the unapply spell continuation; GameEventMgr.cpp:1028-1032; Creature.cpp:2716-2722.
    struct Missing;
    impl EventCreatureOwner for Missing {}
    let update = EventCreatureUpdate {
        definition: definition(7, 0),
        data: data(7),
        activate: false,
    };
    assert_eq!(
        execute_creature_event(map(0), update, &mut Missing).unwrap_err(),
        EventPending::UpdateEntry.refusal()
    );
    assert_eq!(
        apply_event_spells(map(0), 7, data(7), false, &mut Missing).unwrap_err(),
        EventPending::Spell.refusal()
    );
    let empty = GameEventSubstitution {
        guid: 7,
        entry: 0,
        model: 0,
        equipment: 0,
        start_spell: 0,
        end_spell: 0,
    };
    apply_event_spells(map(0), 7, empty, false, &mut Missing).unwrap();
}
