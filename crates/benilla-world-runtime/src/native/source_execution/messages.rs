// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed executable work, never serialized closures or callback outcomes.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageOwner {
    World,
    Map(MapIdentity),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceMessage {
    ForwardGameEvent {
        maps: Vec<EventMapGuids>,
        spawn: bool,
    },
    MapGameEvent {
        creatures: Vec<u32>,
        gameobjects: Vec<u32>,
        spawn: bool,
    },
    UpdateCreatureEvent {
        update: EventCreatureUpdate,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEnvelope {
    pub id: u64,
    pub cause: u64,
    pub payload: SourceMessage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageOutcome {
    Executed,
    CreatureAbsent,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageCompletion {
    pub id: u64,
    pub cause: u64,
    pub thread: u64,
    pub outcome: MessageOutcome,
}

/// Handlers must execute actual manager work; missing F4 owners return typed refusal.
pub trait MessageWork {
    fn begin_message(&mut self, message: &MessageEnvelope) -> Result<()>;
    fn forward_game_event(
        &mut self,
        thread: u64,
        maps: &[EventMapGuids],
        spawn: bool,
    ) -> Result<()>;
    fn map_game_event(
        &mut self,
        thread: u64,
        map: MapIdentity,
        creatures: &[u32],
        gameobjects: &[u32],
        spawn: bool,
    ) -> Result<()>;
    fn creature_present(&self, map: MapIdentity, guid: u64) -> bool;
    fn update_creature_event(
        &mut self,
        thread: u64,
        map: MapIdentity,
        update: EventCreatureUpdate,
    ) -> Result<()>;
}

pub struct SourceMessager {
    owner: MessageOwner,
    queue: Vec<MessageEnvelope>,
    last_id: u64,
    failed: bool,
}
impl SourceMessager {
    pub fn new(owner: MessageOwner) -> Self {
        Self {
            owner,
            queue: Vec::new(),
            last_id: 0,
            failed: false,
        }
    }
    // Enqueue linearization is the exclusive mutable borrow; Messager.h:30-33.
    pub fn enqueue(&mut self, message: MessageEnvelope) -> Result<()> {
        require(!self.failed, "execution.messages_stopped")?;
        require(
            message.id > self.last_id && message.cause > 0,
            "execution.message_identity",
        )?;
        let correct_owner = matches!(
            (&self.owner, &message.payload),
            (MessageOwner::World, SourceMessage::ForwardGameEvent { .. })
                | (
                    MessageOwner::Map(_),
                    SourceMessage::MapGameEvent { .. } | SourceMessage::UpdateCreatureEvent { .. }
                )
        );
        require(correct_owner, "execution.message_owner")?;
        self.last_id = message.id;
        self.queue.push(message);
        Ok(())
    }
    // Detach under the queue borrow before any handler executes; Messager.h:35-43.
    pub fn take_batch(&mut self, owner: MessageOwner) -> Result<MessageBatch> {
        require(
            !self.failed && owner == self.owner,
            "execution.message_generation",
        )?;
        Ok(MessageBatch {
            owner,
            messages: std::mem::take(&mut self.queue),
        })
    }
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
    pub fn execute<W: MessageWork>(
        &mut self,
        owner: MessageOwner,
        thread: u64,
        work: &mut W,
    ) -> Result<Vec<MessageCompletion>> {
        let batch = self.take_batch(owner)?;
        let result = batch.execute(owner, thread, work);
        self.failed |= result.is_err();
        result
    }
}
pub struct MessageBatch {
    owner: MessageOwner,
    messages: Vec<MessageEnvelope>,
}
impl MessageBatch {
    // Captured map lifetime is not a generic silent-drop branch; GameEventMgr.cpp:830-845.
    pub fn execute<W: MessageWork>(
        self,
        owner: MessageOwner,
        thread: u64,
        work: &mut W,
    ) -> Result<Vec<MessageCompletion>> {
        require(
            thread > 0 && self.owner == owner,
            "execution.message_generation",
        )?;
        let mut completions = Vec::with_capacity(self.messages.len());
        for message in self.messages {
            work.begin_message(&message)?;
            let outcome = match (owner, message.payload) {
                (MessageOwner::World, SourceMessage::ForwardGameEvent { maps, spawn }) => {
                    work.forward_game_event(thread, &maps, spawn)?;
                    MessageOutcome::Executed
                }
                (
                    MessageOwner::Map(map),
                    SourceMessage::MapGameEvent {
                        creatures,
                        gameobjects,
                        spawn,
                    },
                ) => {
                    work.map_game_event(thread, map, &creatures, &gameobjects, spawn)?;
                    MessageOutcome::Executed
                }
                (MessageOwner::Map(map), SourceMessage::UpdateCreatureEvent { update }) => {
                    // Resolve the current GUID, not the captured construction; GameEventMgr.cpp:1024-1033.
                    if work.creature_present(map, update.definition.guid) {
                        work.update_creature_event(thread, map, update)?;
                        MessageOutcome::Executed
                    } else {
                        MessageOutcome::CreatureAbsent
                    }
                }
                _ => return Err(refuse("execution.message_owner")),
            };
            completions.push(MessageCompletion {
                id: message.id,
                cause: message.cause,
                thread,
                outcome,
            });
        }
        Ok(completions)
    }
}
