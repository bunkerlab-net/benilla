// SPDX-License-Identifier: GPL-3.0-or-later
//! World/map Messager dispatch for GameEventMgr.cpp:831-861,940-972,1024-1033.
use super::*;

pub trait EventMessageOwner: EventEffectOwner + EventCreatureOwner {
    fn event_message_begin(&mut self, message: &MessageEnvelope) -> Result<()>;
    /// Enters execute_map_game_event with the current map's existing owners.
    fn event_map_message(
        &mut self,
        thread: u64,
        map: MapIdentity,
        creatures: &[u32],
        gameobjects: &[u32],
        spawn: bool,
    ) -> Result<()>;
    fn event_creature_present(&self, map: MapIdentity, guid: u64) -> bool;
}
pub struct EventMessageDispatch<'a, O> {
    pub effects: &'a mut SourceEventEffects,
    pub owner: &'a mut O,
    cause: u64,
}
impl<'a, O> EventMessageDispatch<'a, O> {
    pub fn new(effects: &'a mut SourceEventEffects, owner: &'a mut O) -> Self {
        Self {
            effects,
            owner,
            cause: 0,
        }
    }
}
impl<O: EventMessageOwner> MessageWork for EventMessageDispatch<'_, O> {
    fn begin_message(&mut self, message: &MessageEnvelope) -> Result<()> {
        self.cause = message.id;
        self.owner.event_message_begin(message)?;
        Ok(())
    }
    fn forward_game_event(&mut self, _: u64, maps: &[EventMapGuids], spawn: bool) -> Result<()> {
        self.effects.forward(maps, spawn, self.cause, self.owner)
    }
    fn map_game_event(
        &mut self,
        thread: u64,
        map: MapIdentity,
        creatures: &[u32],
        gameobjects: &[u32],
        spawn: bool,
    ) -> Result<()> {
        self.owner
            .event_map_message(thread, map, creatures, gameobjects, spawn)
    }
    fn creature_present(&self, map: MapIdentity, guid: u64) -> bool {
        self.owner.event_creature_present(map, guid)
    }
    fn update_creature_event(
        &mut self,
        _: u64,
        map: MapIdentity,
        update: EventCreatureUpdate,
    ) -> Result<()> {
        execute_creature_event(map, update, self.owner)
    }
}
