// SPDX-License-Identifier: GPL-3.0-or-later
//! Executable event effects at the pinned GameEventMgr.cpp:714-1159 calls.
use super::*;
use benilla_sim::resident_map::CellPair;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventMapGuids {
    pub map: u32,
    pub creatures: Vec<u32>,
    pub gameobjects: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventSpawnDefinition {
    pub map: u32,
    pub cell: CellPair,
    pub entry: u32,
    pub guid: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventCreatureUpdate {
    pub definition: EventSpawnDefinition,
    pub data: GameEventSubstitution,
    pub activate: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventMailSelection {
    Race(u32),
    RewardedQuest(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventMailTask {
    pub template: u32,
    /// MAIL_CREATURE sender entry, not a player GUID; GameEventMgr.cpp:1091,1094.
    pub sender: u32,
    pub selection: EventMailSelection,
}
impl From<GameEventMail> for EventMailTask {
    // The quest branch supplies SQL, not an online-player filter; GameEventMgr.cpp:1081-1094.
    fn from(mail: GameEventMail) -> Self {
        Self {
            template: mail.template,
            sender: mail.sender,
            selection: if mail.quest == 0 {
                EventMailSelection::Race(mail.race_mask)
            } else {
                EventMailSelection::RewardedQuest(format!(
                    "SELECT characters.guid FROM characters, character_queststatus WHERE (1 << (characters.race - 1)) & {} AND characters.deleteDate IS NULL AND character_queststatus.guid = characters.guid AND character_queststatus.quest = {} AND character_queststatus.rewarded <> 0",
                    mail.race_mask, mail.quest
                ))
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPending {
    Quest,
    Mail,
    UpdateEntry,
    Spell,
    Instance,
    Ai,
    Shutdown,
}
impl EventPending {
    pub fn refusal(self) -> Refusal {
        refuse(match self {
            Self::Quest => "execution.event.f5_quest_flags_owner",
            Self::Mail => "execution.event.f5_mass_mail_owner",
            Self::UpdateEntry => "execution.event.f5_creature_update_entry_owner",
            Self::Spell => "execution.event.f5_spell_owner",
            Self::Instance => "execution.event.f5_instance_event_owner",
            Self::Ai => "execution.event.f5_ai_event_owner",
            Self::Shutdown => "execution.event.world_shutdown_owner",
        })
    }
}
/// Real ObjectMgr, source map traversal, SQL and F5 owners, not replayed outcomes.
pub trait EventEffectOwner: PoolStateOwner {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>>;
    fn event_next_map(
        &mut self,
        map_id: Option<u32>,
        after: Option<MapIdentity>,
    ) -> Result<Option<MapIdentity>>;
    fn event_post_map(&mut self, map: MapIdentity, message: MessageEnvelope) -> Result<()>;
    fn event_sql(&mut self, cause: u64, sql: &str) -> Result<()>;
    fn event_announce(&mut self, event: u16, description: &[u8]) -> Result<()>;
    fn event_quest(&mut self, _quest: u32, _active: bool) -> Result<()> {
        Err(EventPending::Quest.refusal())
    }
    fn event_mail(&mut self, _cause: u64, _task: EventMailTask) -> Result<()> {
        Err(EventPending::Mail.refusal())
    }
    fn event_has_instance(&mut self, map: MapIdentity) -> Result<bool>;
    fn event_notify(
        &mut self,
        _map: MapIdentity,
        _event: u16,
        _active: bool,
        _resume: bool,
    ) -> Result<()> {
        Err(EventPending::Instance.refusal())
    }
    fn event_shutdown(&mut self, _seconds: u32) -> Result<()> {
        Err(EventPending::Shutdown.refusal())
    }
}
/// Global ObjectMgr dynamic lists and the existing world Messager, not map-local copies.
pub struct SourceEventEffects {
    pub world: SourceMessager,
    dynamic: BTreeMap<u32, [Vec<u32>; 2]>,
    next_message: u64,
    stopped: bool,
}
impl Default for SourceEventEffects {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}
impl SourceEventEffects {
    /// Takes the preceding effective ObjectMgr loader's dynamic lists, not a second roster.
    pub fn new(dynamic: BTreeMap<u32, [Vec<u32>; 2]>) -> Self {
        Self {
            world: SourceMessager::new(MessageOwner::World),
            dynamic,
            next_message: 0,
            stopped: false,
        }
    }
    pub fn dynamic_guids(&self, map: u32, kind: GridSpawnKind) -> &[u32] {
        self.dynamic.get(&map).map_or(&[], |lists| {
            &lists[if kind == GridSpawnKind::Creature {
                0
            } else {
                1
            }]
        })
    }
    fn envelope(&mut self, cause: u64, payload: SourceMessage) -> Result<MessageEnvelope> {
        require(cause > 0, "execution.event.cause")?;
        self.next_message = self
            .next_message
            .checked_add(1)
            .ok_or_else(|| refuse("execution.event.message_identity_exhausted"))?;
        Ok(MessageEnvelope {
            id: self.next_message,
            cause,
            payload,
        })
    }
    pub fn execute(
        &mut self,
        manager: &SourceGameEventManager,
        effect: GameEventEffect,
        cause: u64,
        pools: &mut SourcePoolManager,
        states: &mut SourceManagers,
        owner: &mut impl EventEffectOwner,
    ) -> Result<()> {
        require(!self.stopped, "execution.event.effects_stopped")?;
        let result = self.execute_inner(manager, effect, cause, pools, states, owner);
        self.stopped |= result.is_err();
        result
    }
    fn execute_inner(
        &mut self,
        manager: &SourceGameEventManager,
        effect: GameEventEffect,
        cause: u64,
        pools: &mut SourcePoolManager,
        states: &mut SourceManagers,
        owner: &mut impl EventEffectOwner,
    ) -> Result<()> {
        require(cause > 0, "execution.event.cause")?;
        let inventory = manager.inventory();
        match effect {
            GameEventEffect::TruncateStatus => {
                owner.event_sql(cause, "TRUNCATE TABLE game_event_status")
            }
            GameEventEffect::InsertStatus(id) => owner.event_sql(
                cause,
                &format!("INSERT INTO game_event_status (event) VALUES ({id})"),
            ),
            GameEventEffect::DeleteStatus(id) => owner.event_sql(
                cause,
                &format!("DELETE FROM game_event_status WHERE event = {id}"),
            ),
            GameEventEffect::Announce(id) => {
                owner.event_announce(id, &inventory.definitions[usize::from(id)].description)
            }
            GameEventEffect::Restart { seconds } => owner.event_shutdown(seconds),
            GameEventEffect::Spawn(event) | GameEventEffect::Unspawn(event) => {
                let spawn = matches!(effect, GameEventEffect::Spawn(_));
                let slot = inventory.definitions.len() as i64 + i64::from(event) - 1;
                // Out-of-range signed membership returns before pools; GameEventMgr.cpp:771-781,884-894.
                if slot < 0
                    || slot as usize >= inventory.creatures.len()
                    || slot as usize >= inventory.gameobjects.len()
                {
                    return Ok(());
                }
                let mut maps = BTreeMap::<u32, EventMapGuids>::new();
                for (kind, members) in [
                    (GridSpawnKind::Creature, &inventory.creatures[slot as usize]),
                    (
                        GridSpawnKind::GameObject,
                        &inventory.gameobjects[slot as usize],
                    ),
                ] {
                    for &guid in members {
                        let Some(data) = owner.event_definition(SpawnKey { kind, guid })? else {
                            continue;
                        };
                        let pool_kind = if kind == GridSpawnKind::Creature {
                            PoolKind::Creature
                        } else {
                            PoolKind::GameObject
                        };
                        let pool = if event < 0 {
                            pools.part_of(pool_kind, guid)?
                        } else {
                            0
                        };
                        if pool != 0 {
                            pools.set_excluded(pool_kind, pool, guid, !spawn)?;
                            pools.update_pool_in_maps(
                                states,
                                pool,
                                pool_kind,
                                if spawn { 0 } else { guid },
                                owner,
                            )?;
                            continue;
                        }
                        let map = maps.entry(data.map).or_insert_with(|| EventMapGuids {
                            map: data.map,
                            creatures: vec![],
                            gameobjects: vec![],
                        });
                        if kind == GridSpawnKind::Creature {
                            map.creatures.push(guid);
                        } else {
                            map.gameobjects.push(guid);
                        }
                    }
                }
                // One closure captures the complete ordered map grouping; GameEventMgr.cpp:828-864,937-975.
                if !maps.is_empty() {
                    let message = self.envelope(
                        cause,
                        SourceMessage::ForwardGameEvent {
                            maps: maps.into_values().collect(),
                            spawn,
                        },
                    )?;
                    self.world.enqueue(message)?;
                }
                if event > 0 {
                    if let Some(list) = inventory.pools.get(event as usize) {
                        for &pool in list {
                            if spawn {
                                pools.spawn_pool_in_maps(states, pool, true, owner)?;
                            } else {
                                pools.despawn_pool_in_maps(states, pool, owner)?;
                            }
                        }
                    }
                }
                Ok(())
            }
            GameEventEffect::Substitutions { event, active } => {
                for &data in &inventory.substitutions[usize::from(event)] {
                    let Some(definition) = owner.event_definition(SpawnKey {
                        kind: GridSpawnKind::Creature,
                        guid: data.guid,
                    })?
                    else {
                        continue;
                    };
                    let mut after = None;
                    while let Some(map) = owner.event_next_map(Some(definition.map), after)? {
                        let message = self.envelope(
                            cause,
                            SourceMessage::UpdateCreatureEvent {
                                update: EventCreatureUpdate {
                                    definition,
                                    data,
                                    activate: active,
                                },
                            },
                        )?;
                        owner.event_post_map(map, message)?;
                        after = Some(map);
                    }
                }
                Ok(())
            }
            GameEventEffect::Quests { event, active } => {
                for &quest in &inventory.quests[usize::from(event)] {
                    owner.event_quest(quest, active)?;
                }
                Ok(())
            }
            GameEventEffect::Mail(event) => {
                let slot = inventory.definitions.len() as i64 + i64::from(event) - 1;
                let list = inventory
                    .mails
                    .get(
                        usize::try_from(slot)
                            .map_err(|_| refuse("execution.event.mail_index_undefined"))?,
                    )
                    .ok_or_else(|| refuse("execution.event.mail_index_undefined"))?;
                for &mail in list {
                    owner.event_mail(cause, mail.into())?;
                }
                Ok(())
            }
            GameEventEffect::Notify {
                event,
                active,
                resume,
            } => {
                let mut after = None;
                while let Some(map) = owner.event_next_map(None, after)? {
                    // Maps without InstanceData skip even object notifications; GameEventMgr.cpp:1155-1158.
                    if owner.event_has_instance(map)? {
                        owner.event_notify(map, event, active, resume)?;
                    }
                    after = Some(map);
                }
                Ok(())
            }
        }
    }
    pub fn forward(
        &mut self,
        maps: &[EventMapGuids],
        spawn: bool,
        cause: u64,
        owner: &mut impl EventEffectOwner,
    ) -> Result<()> {
        require(!self.stopped, "execution.event.effects_stopped")?;
        let result = (|| {
            for data in maps {
                let lists = self.dynamic.entry(data.map).or_default();
                // Append duplicates, erase every matching occurrence; ObjectMgr.cpp:925-942.
                for (list, guids) in lists.iter_mut().zip([&data.creatures, &data.gameobjects]) {
                    if spawn {
                        list.extend_from_slice(guids);
                    } else {
                        for guid in guids {
                            list.retain(|value| value != guid);
                        }
                    }
                }
                let mut after = None;
                while let Some(map) = owner.event_next_map(Some(data.map), after)? {
                    let message = self.envelope(
                        cause,
                        SourceMessage::MapGameEvent {
                            creatures: data.creatures.clone(),
                            gameobjects: data.gameobjects.clone(),
                            spawn,
                        },
                    )?;
                    owner.event_post_map(map, message)?;
                    after = Some(map);
                }
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventLiveRequest {
    pub map: MapIdentity,
    pub key: SpawnKey,
    pub object: ObjectHandle,
}
impl EventLiveRequest {
    fn validate(self, objects: &SourceObjects) -> Result<()> {
        let value = objects.get(self.object)?;
        require(
            value.map == Some(self.map)
                && value.database_guid == self.key.guid
                && value.kind
                    == if self.key.kind == GridSpawnKind::Creature {
                        SourceObjectKind::Creature
                    } else {
                        SourceObjectKind::GameObject
                    },
            "execution.event.live_binding",
        )
    }
    pub fn creature<D: GridDefinitions, P: ResidencyProductOwner, F: ResidencyOwner<D, P>>(
        self,
        creature: &SourceCreatureLifecycle,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.validate(context.objects)?;
        require(
            self.key.kind == GridSpawnKind::Creature && creature.object() == self.object,
            "execution.event.live_binding",
        )?;
        // Keep dead lootables; GameEventMgr.cpp:956-958.
        if creature.death_state == CreatureDeathState::Alive {
            context.add_to_remove(self.object, owner)?;
        }
        Ok(())
    }
    pub fn gameobject<
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: GameObjectLifecycleOwner<D, P>,
    >(
        self,
        context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        self.validate(context.grid.objects)?;
        require(
            self.key.kind == GridSpawnKind::GameObject,
            "execution.event.live_binding",
        )?;
        delete_source_gameobject(owner, self.object, context)
    }
}
pub trait EventMapOwner: SpawnWork {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>>;
    fn event_grid_loaded(&mut self, map: MapIdentity, cell: CellPair) -> Result<bool>;
    fn event_construct(
        &mut self,
        request: DynamicSpawnRequest,
        spawns: &mut SourceSpawnManager,
        states: &mut SourceManagers,
    ) -> Result<bool>;
    fn event_lookup(&mut self, map: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>>;
    fn event_remove(
        &mut self,
        request: EventLiveRequest,
        spawns: &mut SourceSpawnManager,
        states: &mut SourceManagers,
    ) -> Result<()>;
}
/// Called by MapGameEvent on the map thread, re-reading current spawn definitions.
pub fn execute_map_game_event(
    spawns: &mut SourceSpawnManager,
    states: &mut SourceManagers,
    creatures: &[u32],
    gameobjects: &[u32],
    spawn: bool,
    owner: &mut impl EventMapOwner,
) -> Result<()> {
    let map = spawns.map();
    require(
        states.state(spawns.persistent())?.map_binding() == Some(map),
        "execution.event.map_binding",
    )?;
    for (kind, guids) in [
        (GridSpawnKind::Creature, creatures),
        (GridSpawnKind::GameObject, gameobjects),
    ] {
        for &guid in guids {
            let key = SpawnKey { kind, guid };
            let data = owner
                .event_definition(key)?
                .ok_or_else(|| refuse("execution.event.spawn_definition_assert"))?;
            states.overlay(spawns.persistent(), data.cell, kind, guid, spawn, owner)?;
            // GO Add uses UNIT, but Remove uses GAMEOBJECT; GameEventMgr.cpp:847,857,955,966.
            let high = if spawn || kind == GridSpawnKind::Creature {
                HighGuid::Creature
            } else {
                HighGuid::GameObject
            };
            spawns.set_event_guid(guid, high, spawn, owner)?;
            if spawn {
                if owner.event_grid_loaded(map, data.cell)? {
                    owner.event_construct(
                        DynamicSpawnRequest {
                            map,
                            thread: owner.thread(),
                            key,
                        },
                        spawns,
                        states,
                    )?;
                }
            } else if let Some(object) = owner.event_lookup(map, key)? {
                owner.event_remove(EventLiveRequest { map, key, object }, spawns, states)?;
            }
        }
    }
    if !spawn {
        spawns.remove_spawns(creatures, gameobjects, owner)?;
    }
    Ok(())
}

pub trait EventCreatureOwner {
    /// UpdateEntry owns active spells at Creature.cpp:617-619; its bool does not stop the worker.
    fn event_update_entry(
        &mut self,
        _map: MapIdentity,
        _update: EventCreatureUpdate,
    ) -> Result<bool> {
        Err(EventPending::UpdateEntry.refusal())
    }
    fn event_spell_aura(&mut self, _spell: u32) -> Result<bool> {
        Err(EventPending::Spell.refusal())
    }
    fn event_remove_aura(&mut self, _map: MapIdentity, _guid: u64, _spell: u32) -> Result<()> {
        Err(EventPending::Spell.refusal())
    }
    fn event_cast_triggered(&mut self, _map: MapIdentity, _guid: u64, _spell: u32) -> Result<()> {
        Err(EventPending::Spell.refusal())
    }
}
/// Shared by F5 UpdateEntry and the unapply continuation; Creature.cpp:2711-2722.
pub fn apply_event_spells(
    map: MapIdentity,
    guid: u64,
    data: GameEventSubstitution,
    active: bool,
    owner: &mut impl EventCreatureOwner,
) -> Result<()> {
    let (cast, remove) = if active {
        (data.start_spell, data.end_spell)
    } else {
        (data.end_spell, data.start_spell)
    };
    if remove != 0 && owner.event_spell_aura(remove)? {
        owner.event_remove_aura(map, guid, remove)?;
    }
    if cast != 0 {
        owner.event_cast_triggered(map, guid, cast)?;
    }
    Ok(())
}
pub fn execute_creature_event(
    map: MapIdentity,
    update: EventCreatureUpdate,
    owner: &mut impl EventCreatureOwner,
) -> Result<()> {
    // Ignore UpdateEntry's bool and still apply end spells; GameEventMgr.cpp:1028-1032.
    owner.event_update_entry(map, update)?;
    if !update.activate {
        apply_event_spells(map, update.definition.guid, update.data, false, owner)?;
    }
    Ok(())
}
