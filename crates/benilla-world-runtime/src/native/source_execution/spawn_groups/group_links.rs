// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, SpawnGroup.cpp:525-594,655-697.
use super::*;
use std::ops::Bound::{Excluded, Unbounded};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GroupEvent {
    Aggro,
    Evade,
    Home,
    Respawn,
    MemberDied,
}
#[derive(Clone, Copy, Debug)]
pub struct GroupUnit {
    pub database_guid: u32,
    pub has_charmer: bool,
    pub alive: bool,
    pub in_combat: bool,
    pub evading_home: bool,
}
pub trait GroupEventOwner: GroupWork {
    fn group_unit(&mut self, map: MapIdentity, object: ObjectHandle) -> Result<GroupUnit>;
}
impl SourceSpawnGroups {
    // Range-for holds its current map iterator across callbacks; SpawnGroup.cpp:542,565,586.
    pub(super) fn require_member_erase(
        &self,
        group: GroupIdentity,
        guid: Option<u32>,
    ) -> Result<()> {
        require(
            !self
                .held_members
                .iter()
                .any(|&(held, key)| held == group && guid.is_none_or(|guid| guid == key)),
            "execution.group.callback_iterator_undefined",
        )
    }
    pub fn event(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        event: GroupEvent,
        target: Option<ObjectHandle>,
        work: &mut impl GroupEventOwner,
    ) -> Result<()> {
        self.run(|this| {
            this.binding(group, states)?;
            require(
                this.definition(group.handle.id)?.kind() == GridSpawnKind::Creature,
                "execution.group.creature_group_required",
            )?;
            this.frames.push(group);
            this.event_inner(group, states, event, target, work)?;
            this.frames.pop();
            Ok(())
        })
    }
    fn event_inner(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        event: GroupEvent,
        target: Option<ObjectHandle>,
        work: &mut impl GroupEventOwner,
    ) -> Result<()> {
        let definitions = self.definitions.clone();
        let definition = &definitions[&group.handle.id];
        let flags = definition.template.flags;
        match event {
            GroupEvent::Aggro => {
                let Some(target) = target.filter(|_| flags & 1 != 0) else {
                    return Ok(());
                };
                let unit = work.group_unit(group.map, target)?;
                // Charmed own-group targets are allowed; SpawnGroup.cpp:533-540.
                if (!unit.has_charmer
                    && self.state(group)?.objects.contains_key(&unit.database_guid))
                    || self.state(group)?.linkage_targets.contains(&target)
                {
                    return Ok(());
                }
                self.state_mut(group)?.linkage_targets.insert(target);
                self.member_events(group, states, event, Some(target), work)?;
                // Definition occurrence order follows every local member; SpawnGroup.cpp:553-558.
                for &id in &definition.linked {
                    let linked = self
                        .states
                        .get(&(group.map, id))
                        .ok_or_else(|| refuse("execution.group.linked_group_absent_undefined"))?
                        .identity;
                    require(
                        self.definition(id)?.kind() == GridSpawnKind::Creature,
                        "execution.group.linked_group_type_undefined",
                    )?;
                    self.event(linked, states, event, Some(target), work)?;
                }
                self.state_mut(group)?.linkage_targets.remove(&target);
            }
            GroupEvent::Evade if flags & 4 != 0 => {
                self.member_events(group, states, event, target, work)?;
            }
            GroupEvent::Home | GroupEvent::Respawn => {
                // HOME's formation callback precedes RESPawn fallthrough; SpawnGroup.cpp:574-583.
                if event == GroupEvent::Home
                    && definition.formation.is_some()
                    && !self.is_evading(group, work)?
                {
                    work.callback(self, states, group, GroupCallback::FormationHome)?;
                }
                if flags & 2 != 0 {
                    self.clear_respawn_times(group, states, work)?;
                }
            }
            GroupEvent::MemberDied => self.member_events(group, states, event, target, work)?,
            GroupEvent::Evade => {}
        }
        Ok(())
    }
    fn member_events(
        &mut self,
        group: GroupIdentity,
        states: &mut SourceManagers,
        event: GroupEvent,
        target: Option<ObjectHandle>,
        work: &mut impl GroupEventOwner,
    ) -> Result<()> {
        let mut next = self
            .state(group)?
            .objects
            .first_key_value()
            .map(|(&guid, _)| guid);
        while let Some(guid) = next {
            self.held_members.push((group, guid));
            if let Some(object) = work.live_object(
                group.map,
                SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid,
                },
            )? {
                match event {
                    GroupEvent::Aggro => {
                        work.callback(
                            self,
                            states,
                            group,
                            GroupCallback::SetNoCallAssistance { object },
                        )?;
                        work.callback(
                            self,
                            states,
                            group,
                            GroupCallback::EngageInCombat {
                                object,
                                target: target
                                    .ok_or_else(|| refuse("execution.group.aggro_target"))?,
                            },
                        )?;
                    }
                    GroupEvent::Evade if !work.group_unit(group.map, object)?.evading_home => {
                        work.callback(
                            self,
                            states,
                            group,
                            GroupCallback::EnterEvadeMode { object },
                        )?;
                    }
                    GroupEvent::MemberDied => {
                        work.callback(
                            self,
                            states,
                            group,
                            GroupCallback::MemberDied { object, target },
                        )?;
                    }
                    _ => {}
                }
            }
            self.held_members.pop();
            // Increment after callbacks retains insertions above the held key; SpawnGroup.cpp:542,565,586.
            next = self
                .state(group)?
                .objects
                .range((Excluded(guid), Unbounded))
                .next()
                .map(|(&guid, _)| guid);
        }
        Ok(())
    }
    pub fn is_evading(
        &self,
        group: GroupIdentity,
        work: &mut impl GroupEventOwner,
    ) -> Result<bool> {
        self.member_predicate(group, work, |unit| unit.alive && unit.evading_home)
    }
    pub fn is_out_of_combat(
        &self,
        group: GroupIdentity,
        work: &mut impl GroupEventOwner,
    ) -> Result<bool> {
        Ok(!self.member_predicate(group, work, |unit| unit.alive && unit.in_combat)?)
    }
    fn member_predicate(
        &self,
        group: GroupIdentity,
        work: &mut impl GroupEventOwner,
        predicate: impl Fn(GroupUnit) -> bool,
    ) -> Result<bool> {
        require(
            self.definition(group.handle.id)?.kind() == GridSpawnKind::Creature,
            "execution.group.creature_group_required",
        )?;
        for &guid in self.state(group)?.objects.keys() {
            if let Some(object) = work.live_object(
                group.map,
                SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid,
                },
            )? {
                if predicate(work.group_unit(group.map, object)?) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    // Creature.cpp:1870-1871 and GameObject.cpp:690-691 guard the absent-minimum branch.
    pub fn lifecycle_respawn_delay(
        &self,
        map: MapIdentity,
        key: SpawnKey,
        work: &mut impl GroupWork,
    ) -> Result<Option<u32>> {
        let group = self
            .group_for(map, key)?
            .ok_or_else(|| refuse("execution.group.lifecycle_group_absent"))?;
        if self.is_respawn_overridden(group)? {
            self.random_respawn_time(group, work).map(Some)
        } else {
            Ok(None)
        }
    }
    pub fn creature_lifecycle_phase<D, P>(
        &mut self,
        phase: CreatureLifecyclePhase,
        creature: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        work: &mut impl GroupEventOwner,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: ResidencyProductOwner,
    {
        let map = context.grid.cells.map();
        let key = SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: creature.construction.spawn.database_guid,
        };
        let group = self
            .group_for(map, key)?
            .ok_or_else(|| refuse("execution.group.lifecycle_group_absent"))?;
        match phase {
            CreatureLifecyclePhase::ClearGroup => self.run(|this| {
                this.binding(group, context.persistent)?;
                this.frames.push(group);
                // ClearCreatureGroup removes formation membership first; Creature.cpp:1580-1582.
                if this.definition(group.handle.id)?.formation.is_some() {
                    work.callback(
                        this,
                        context.persistent,
                        group,
                        GroupCallback::FormationRemove {
                            object: creature.object(),
                        },
                    )?;
                }
                this.remove_object(group, context.persistent, creature.object(), work)?;
                this.frames.pop();
                Ok(())
            }),
            CreatureLifecyclePhase::GroupRespawn => self.event(
                group,
                context.persistent,
                GroupEvent::Respawn,
                Some(creature.object()),
                work,
            ),
            _ => Err(phase.missing_owner()),
        }
    }
    pub fn gameobject_clear_group(
        &mut self,
        map: MapIdentity,
        states: &mut SourceManagers,
        object: ObjectHandle,
        work: &mut impl GroupWork,
    ) -> Result<()> {
        let removed = work.removed_object(object)?;
        require(
            removed.key.kind == GridSpawnKind::GameObject,
            "execution.group.removed_object_kind",
        )?;
        let group = self
            .group_for(map, removed.key)?
            .ok_or_else(|| refuse("execution.group.lifecycle_group_absent"))?;
        self.remove_object(group, states, object, work)
    }
}
impl GroupCallback {
    pub fn creature_lifecycle<D, P, F>(
        self,
        map: MapIdentity,
        creature: &mut SourceCreatureLifecycle,
        context: &mut CreatureLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: CreatureLifecycleOwner<D, P>,
    {
        let Self::ForcedDespawn {
            object,
            delay_millis,
            forced_seconds,
            only_alive,
        } = self
        else {
            return Err(self.missing_owner());
        };
        require(
            object == creature.object() && map == context.grid.cells.map(),
            "execution.group.lifecycle_binding",
        )?;
        if forced_seconds != 0 {
            creature.set_respawn_delay(forced_seconds, true);
        }
        if delay_millis != 0 {
            return owner.lifecycle_phase(
                CreatureLifecyclePhase::ScheduleForcedDespawn {
                    delay_millis,
                    only_alive,
                },
                creature,
                context,
            );
        }
        creature.forced_despawn(context, owner)
    }
    pub fn gameobject_lifecycle<D, P, F>(
        self,
        map: MapIdentity,
        context: &mut GameObjectLifecycleContext<'_, '_, D, P>,
        owner: &mut F,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: GameObjectLifecycleOwner<D, P>,
    {
        let Self::ForcedDespawn {
            object,
            delay_millis,
            forced_seconds,
            ..
        } = self
        else {
            return Err(self.missing_owner());
        };
        require(
            map == context.grid.cells.map() && context.grid.objects.get(object)?.map == Some(map),
            "execution.group.lifecycle_binding",
        )?;
        if forced_seconds != 0 {
            owner
                .gameobjects()
                .get_mut(object)?
                .set_respawn_delay(forced_seconds, true);
        }
        force_despawn_source_gameobject(owner, object, delay_millis, context)
    }
}
