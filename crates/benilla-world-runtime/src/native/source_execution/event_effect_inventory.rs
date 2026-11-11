// SPDX-License-Identifier: GPL-3.0-or-later
//! Global effective dynamic lists and event message allocation are continuation state.
use super::*;
#[derive(Clone, Debug)]
pub struct EventEffectsInventory {
    pub world: MessageInventory,
    pub dynamic: BTreeMap<u32, [Vec<u32>; 2]>,
    pub next_message: u64,
    pub stopped: bool,
}
impl EventEffectsInventory {
    pub fn validate(&self) -> Result<()> {
        self.world.validate()?;
        require(
            !self.stopped
                && self.world.owner == MessageOwner::World
                && self.world.last_id <= self.next_message,
            "execution.managers.event_message_owner",
        )
    }
}
impl SourceEventEffects {
    pub fn inventory(&self) -> EventEffectsInventory {
        EventEffectsInventory {
            world: self.world.inventory(),
            dynamic: self.dynamic.clone(),
            next_message: self.next_message,
            stopped: self.stopped,
        }
    }
}
