// SPDX-License-Identifier: GPL-3.0-or-later
//! Global effective dynamic lists are continuation state.
use super::*;
#[derive(Clone, Debug)]
pub struct EventEffectsInventory {
    pub dynamic: BTreeMap<u32, [Vec<u32>; 2]>,
    pub stopped: bool,
}
impl EventEffectsInventory {
    pub fn validate(&self) -> Result<()> {
        require(!self.stopped, "execution.event.effects_stopped")
    }
}
impl SourceEventEffects {
    pub fn inventory(&self) -> EventEffectsInventory {
        EventEffectsInventory {
            dynamic: self.dynamic.clone(),
            stopped: self.stopped,
        }
    }
}
