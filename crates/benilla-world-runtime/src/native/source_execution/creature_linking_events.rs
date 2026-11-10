// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, CreatureLinkingMgr.cpp:401-638,753-782.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkingEvent {
    Aggro,
    Evade,
    Die,
    Respawn,
    Despawn,
}
impl LinkingEvent {
    fn masks(self) -> (u16, u16) {
        // DIE includes FOLLOW although ProcessSlave does nothing with it; CreatureLinkingMgr.cpp:259-265.
        match self {
            Self::Aggro => (0x0001, 0x0002),
            Self::Evade => (0x0004 | 0x1000 | 0x4000, 0x0008),
            Self::Die => (0x0010 | 0x0020 | 0x0040 | FOLLOW, 0),
            Self::Respawn => (0x0080 | 0x0100 | FOLLOW, FOLLOW),
            Self::Despawn => (0x2000, 0),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkingLookup {
    Database(u32),
    Runtime(SourceGuid),
}
impl LinkingLookup {
    pub fn resolve(self, map: MapIdentity, objects: &SourceObjects) -> Option<ObjectHandle> {
        match self {
            Self::Database(guid) => {
                objects.lookup_database(map, DatabaseLookupKind::Creature, guid)
            }
            Self::Runtime(guid) => objects.lookup(map, LiveLookupKind::Creature, guid),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct LinkingCreature {
    pub object: ObjectHandle,
    pub entry: u32,
    pub database: u32,
    pub respawn_xy: [u32; 2],
    pub new_spawning: bool,
    pub death: CreatureDeathState,
    pub pet: bool,
    pub controlled: bool,
    pub combat: bool,
    pub respawn_delay: u32,
    pub respawn_time: i64,
}
impl LinkingCreature {
    /// Control/combat predicates come from the real Unit owner, not SoloCreatureAi defaults.
    pub fn read(
        objects: &SourceObjects,
        creature: &SourceCreatureLifecycle,
        controlled: bool,
        combat: bool,
    ) -> Result<Self> {
        let object = objects.get(creature.object())?;
        let pose = object
            .respawn_pose
            .ok_or_else(|| refuse("execution.linking.respawn_pose_owner"))?;
        let runtime = object
            .guid
            .ok_or_else(|| refuse("execution.linking.guid_owner"))?;
        Ok(Self {
            object: creature.object(),
            entry: creature.construction.template.entry,
            database: object.database_guid,
            respawn_xy: [pose.0[0], pose.0[1]],
            new_spawning: source_creature_new_spawning(object.database_guid, runtime.counter()),
            death: creature.death_state,
            pet: object.kind == SourceObjectKind::Pet,
            controlled,
            combat,
            respawn_delay: creature.construction.respawn_delay,
            respawn_time: creature.construction.respawn_time,
        })
    }
    fn alive(self) -> bool {
        self.death == CreatureDeathState::Alive
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkingAction {
    Respawn,
    ForcedDespawn,
    Suicide,
    AttackStart(ObjectHandle),
    AddThreat(ObjectHandle),
    SetInCombatWith(ObjectHandle),
    TriggerCombatTimer(ObjectHandle),
    EnterEvadeMode,
    Follow(ObjectHandle),
}
impl LinkingAction {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::Respawn | Self::ForcedDespawn => "execution.linking.lifecycle_dispatch_owner",
            Self::Suicide => "execution.linking.f5_combat_suicide_owner",
            Self::AttackStart(_) => "execution.linking.f5_ai_attack_start_owner",
            Self::AddThreat(_) => "execution.linking.f5_threat_owner",
            Self::SetInCombatWith(_) | Self::TriggerCombatTimer(_) => {
                "execution.linking.f5_combat_owner"
            }
            Self::EnterEvadeMode => "execution.linking.f5_ai_evade_owner",
            Self::Follow(_) => "execution.linking.f5_motion_follow_owner",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkingActionRequest {
    pub map: MapIdentity,
    pub object: ObjectHandle,
    pub action: LinkingAction,
}
impl LinkingActionRequest {
    /// Dispatch through retained F4.07 state, including its pool/link/removal callbacks.
    pub fn lifecycle<D, P, F>(
        self,
        creature: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    {
        require(
            creature.object() == self.object && context.grid.cells.map() == self.map,
            "execution.linking.lifecycle_binding",
        )?;
        match self.action {
            LinkingAction::Respawn => creature.respawn(context, owner),
            LinkingAction::ForcedDespawn => creature.forced_despawn(context, owner),
            _ => Err(self.action.missing_owner()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkingScheduleRequest {
    pub map: MapIdentity,
    pub database: u32,
}
impl LinkingScheduleRequest {
    pub fn execute(
        self,
        manager: &mut SourceSpawnManager,
        states: &mut SourceManagers,
        owner: &mut impl SpawnWork,
    ) -> Result<()> {
        require(
            manager.map() == self.map,
            "execution.linking.scheduler_binding",
        )?;
        manager.respawn(
            states,
            SpawnKey {
                kind: GridSpawnKind::Creature,
                guid: self.database,
            },
            0,
            owner,
        )
    }
}
/// Synchronous callbacks may register slaves/masters and recursively enter other lists.
pub trait LinkingEventOwner {
    fn linking_creature(
        &mut self,
        map: MapIdentity,
        object: ObjectHandle,
    ) -> Result<LinkingCreature>;
    fn linking_lookup(
        &mut self,
        map: MapIdentity,
        lookup: LinkingLookup,
    ) -> Result<Option<ObjectHandle>>;
    fn linking_wall(&mut self) -> Result<ClockSample>;
    fn linking_schedule(
        &mut self,
        request: LinkingScheduleRequest,
        holder: &mut SourceCreatureLinkingHolder,
    ) -> Result<()>;
    fn linking_action(
        &mut self,
        request: LinkingActionRequest,
        _holder: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        Err(request.action.missing_owner())
    }
}
struct SlaveList {
    by_guid: bool,
    key: u32,
    group: usize,
    flags: u16,
    range: u16,
}
impl SourceCreatureLinkingHolder {
    pub fn event(
        &mut self,
        links: &SourceCreatureLinking,
        owner: &mut impl LinkingEventOwner,
        event: LinkingEvent,
        source: ObjectHandle,
        enemy: Option<ObjectHandle>,
    ) -> Result<()> {
        require(!self.stopped, "execution.linking.events_stopped")?;
        let result = self.event_inner(links, owner, event, source, enemy);
        self.stopped |= result.is_err();
        result
    }
    fn event_inner(
        &mut self,
        links: &SourceCreatureLinking,
        owner: &mut impl LinkingEventOwner,
        event: LinkingEvent,
        source: ObjectHandle,
        enemy: Option<ObjectHandle>,
    ) -> Result<()> {
        links.ensure_loaded()?;
        let actor = owner.linking_creature(self.map, source)?;
        if !links.event_trigger(actor.entry, actor.database, self.map.map)
            || actor.controlled
            || (event == LinkingEvent::Aggro && enemy.is_none())
        {
            return Ok(());
        }
        let (mask, reverse) = event.masks();
        // Independent guards and insertion order, entry before GUID before reverse; cpp:427-454.
        for (by_guid, key) in [(false, actor.entry), (true, actor.database)] {
            let mut index = 0;
            loop {
                let groups = if by_guid {
                    &mut self.guids
                } else {
                    &mut self.entries
                };
                let Some(group) = groups.get_mut(&key).and_then(|v| v.get_mut(index)) else {
                    break;
                };
                if !group.in_use {
                    group.in_use = true;
                    let flags = group.flags & mask;
                    let range = group.range;
                    self.slave_list(
                        owner,
                        event,
                        source,
                        enemy,
                        SlaveList {
                            by_guid,
                            key,
                            group: index,
                            flags,
                            range,
                        },
                    )?;
                    let groups = if by_guid {
                        &mut self.guids
                    } else {
                        &mut self.entries
                    };
                    groups.get_mut(&key).unwrap()[index].in_use = false;
                }
                index += 1;
            }
        }
        // Re-read after callbacks, as GetLinkedTriggerInformation does at cpp:452.
        let actor = owner.linking_creature(self.map, source)?;
        let Some(info) = links
            .lookup(actor.entry, actor.database, self.map.map)
            .filter(|i| i.flags & reverse != 0)
        else {
            return Ok(());
        };
        let master = self.find_master(owner, actor, info, false)?;
        if event == LinkingEvent::Evade
            && actor.new_spawning
            && master.is_none_or(|m| m.death == CreatureDeathState::Corpse)
        {
            owner.linking_schedule(
                LinkingScheduleRequest {
                    map: self.map,
                    database: info.master_guid,
                },
                self,
            )?;
        } else if let Some(master) = master {
            match event {
                LinkingEvent::Aggro => self.aggro(owner, master, enemy.unwrap())?,
                LinkingEvent::Evade if !master.alive() => {
                    self.action(owner, master.object, LinkingAction::Respawn)?
                }
                LinkingEvent::Respawn if master.alive() => {
                    self.action(owner, source, LinkingAction::Follow(master.object))?
                }
                _ => (),
            }
        }
        Ok(())
    }
    fn action(
        &mut self,
        owner: &mut impl LinkingEventOwner,
        object: ObjectHandle,
        action: LinkingAction,
    ) -> Result<()> {
        owner.linking_action(
            LinkingActionRequest {
                map: self.map,
                object,
                action,
            },
            self,
        )
    }
    fn aggro(
        &mut self,
        owner: &mut impl LinkingEventOwner,
        actor: LinkingCreature,
        enemy: ObjectHandle,
    ) -> Result<()> {
        if actor.controlled {
            return Ok(());
        }
        if actor.combat {
            // Threat is bilateral, but combat/timer calls belong to the enemy; cpp:565-570,485-490.
            self.action(owner, actor.object, LinkingAction::AddThreat(enemy))?;
            self.action(owner, enemy, LinkingAction::AddThreat(actor.object))?;
            self.action(owner, enemy, LinkingAction::SetInCombatWith(actor.object))?;
            self.action(
                owner,
                enemy,
                LinkingAction::TriggerCombatTimer(actor.object),
            )
        } else {
            self.action(owner, actor.object, LinkingAction::AttackStart(enemy))
        }
    }
    fn slave_list(
        &mut self,
        owner: &mut impl LinkingEventOwner,
        event: LinkingEvent,
        source: ObjectHandle,
        enemy: Option<ObjectHandle>,
        list: SlaveList,
    ) -> Result<()> {
        let SlaveList {
            by_guid,
            key,
            group,
            flags,
            range,
        } = list;
        if flags == 0 {
            return Ok(());
        }
        let actor = owner.linking_creature(self.map, source)?;
        let preprocess = if actor.new_spawning {
            flags & (0x0004 | 0x0040 | 0x0080)
        } else {
            0
        };
        let postprocess = flags & !preprocess;
        let mut index = 0;
        loop {
            let groups = if by_guid { &self.guids } else { &self.entries };
            let Some(&(database, guid)) = groups[&key][group].slaves.get(index) else {
                break;
            };
            let lookup = if database != 0 {
                LinkingLookup::Database(database)
            } else {
                LinkingLookup::Runtime(guid)
            };
            let slave = owner.linking_lookup(self.map, lookup)?;
            let state = slave
                .map(|s| owner.linking_creature(self.map, s))
                .transpose()?;
            if preprocess != 0 && state.is_none_or(|s| s.death == CreatureDeathState::Corpse) {
                // Scheduling precedes pruning, pet checks and range checks; cpp:533-549.
                owner.linking_schedule(
                    LinkingScheduleRequest {
                        map: self.map,
                        database,
                    },
                    self,
                )?;
            }
            if slave.is_none() {
                let groups = if by_guid {
                    &mut self.guids
                } else {
                    &mut self.entries
                };
                groups.get_mut(&key).unwrap()[group].slaves.remove(index);
                continue;
            }
            let groups = if by_guid { &self.guids } else { &self.entries };
            // Increment before callbacks: an end iterator does not visit a later append; cpp:542.
            let at_end = index + 1 == groups[&key][group].slaves.len();
            index += 1;
            let state = owner.linking_creature(self.map, slave.unwrap())?;
            if !state.pet {
                let master = owner.linking_creature(self.map, source)?;
                if linking_in_range(state.respawn_xy, master.respawn_xy, range)? {
                    self.slave(owner, event, source, state.object, enemy, postprocess)?;
                }
            }
            if at_end {
                break;
            }
        }
        Ok(())
    }
    fn slave(
        &mut self,
        owner: &mut impl LinkingEventOwner,
        event: LinkingEvent,
        source: ObjectHandle,
        slave: ObjectHandle,
        enemy: Option<ObjectHandle>,
        flags: u16,
    ) -> Result<()> {
        match event {
            LinkingEvent::Aggro if flags & 1 != 0 => {
                let actor = owner.linking_creature(self.map, slave)?;
                self.aggro(owner, actor, enemy.unwrap())?;
            }
            LinkingEvent::Evade => {
                if flags & 0x1000 != 0 && owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::ForcedDespawn)?;
                }
                if flags & 0x0004 != 0 && !owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::Respawn)?;
                }
                if flags & 0x4000 != 0 && owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::EnterEvadeMode)?;
                }
            }
            LinkingEvent::Die => {
                if flags & 0x0020 != 0 && owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::Suicide)?;
                }
                if flags & 0x0010 != 0 && owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::ForcedDespawn)?;
                }
                if flags & 0x0040 != 0 && !owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::Respawn)?;
                }
            }
            LinkingEvent::Respawn => {
                if flags & 0x0080 != 0 {
                    let actor = owner.linking_creature(self.map, slave)?;
                    if !actor.alive() {
                        let due = if actor.respawn_delay == 0 {
                            true
                        } else {
                            let wall = owner.linking_wall()?;
                            require(
                                wall.kind() == ClockKind::WallSeconds,
                                "execution.linking.wall_clock_kind",
                            )?;
                            actor.respawn_time > wall.value()
                        };
                        if due {
                            self.action(owner, slave, LinkingAction::Respawn)?
                        }
                    }
                } else if flags & 0x0100 != 0 && owner.linking_creature(self.map, slave)?.alive() {
                    self.action(owner, slave, LinkingAction::ForcedDespawn)?;
                }
                let actor = owner.linking_creature(self.map, slave)?;
                if flags & FOLLOW != 0 && actor.alive() && !actor.combat {
                    self.action(owner, slave, LinkingAction::Follow(source))?;
                }
            }
            LinkingEvent::Despawn
                if flags & 0x2000 != 0
                    && owner.linking_creature(self.map, slave)?.death
                        != CreatureDeathState::Dead =>
            {
                self.action(owner, slave, LinkingAction::ForcedDespawn)?;
            }
            _ => (),
        }
        Ok(())
    }
    fn find_master(
        &self,
        owner: &mut impl LinkingEventOwner,
        actor: LinkingCreature,
        info: LinkingInfo,
        retain_last: bool,
    ) -> Result<Option<LinkingCreature>> {
        if info.map == u32::MAX {
            return owner
                .linking_lookup(self.map, LinkingLookup::Database(info.master_guid))?
                .map(|m| owner.linking_creature(self.map, m))
                .transpose();
        }
        let mut last = None;
        if let Some(masters) = self.masters.get(&info.master) {
            for &guid in masters {
                let master = owner
                    .linking_lookup(self.map, LinkingLookup::Runtime(guid))?
                    .map(|m| owner.linking_creature(self.map, m))
                    .transpose()?;
                if retain_last {
                    last = master
                }
                if let Some(master) = master {
                    if linking_in_range(actor.respawn_xy, master.respawn_xy, info.range)? {
                        return Ok(Some(master));
                    }
                }
            }
        }
        // TryFollowMaster retains the last lookup even when out of range; cpp:766-776.
        Ok(last)
    }
    pub fn try_follow_master(
        &mut self,
        links: &SourceCreatureLinking,
        owner: &mut impl LinkingEventOwner,
        source: ObjectHandle,
    ) -> Result<bool> {
        require(!self.stopped, "execution.linking.events_stopped")?;
        let result = (|| {
            links.ensure_loaded()?;
            let actor = owner.linking_creature(self.map, source)?;
            let Some(info) = links
                .lookup(actor.entry, actor.database, self.map.map)
                .filter(|i| i.flags & FOLLOW != 0)
            else {
                return Ok(false);
            };
            if let Some(master) = self
                .find_master(owner, actor, info, true)?
                .filter(|m| m.alive())
            {
                self.action(owner, source, LinkingAction::Follow(master.object))?;
                return Ok(true);
            }
            Ok(false)
        })();
        self.stopped |= result.is_err();
        result
    }
}
