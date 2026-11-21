// SPDX-License-Identifier: GPL-3.0-or-later
//! Join only live formation references. Detached source slots remain retained.
use super::*;
impl SourceFormations {
    pub fn validate_manager_owners(
        &self,
        groups: &SpawnGroupsInventory,
        objects: &SourceObjects,
    ) -> Result<()> {
        require(!self.stopped, "execution.managers.formation_stopped")?;
        for group in &groups.states {
            require(
                group.has_formation == self.states.contains_key(&group.identity),
                "execution.managers.group_formation_owner",
            )?;
        }
        for (&identity, state) in &self.states {
            require(
                state.group == identity
                    && self.definitions.contains_key(&state.definition)
                    && groups
                        .states
                        .iter()
                        .any(|g| g.identity == identity && g.has_formation),
                "execution.managers.formation_group_owner",
            )?;
            for (&id, slot) in &state.slots {
                if let Some(slot) = slot {
                    let entry = self.slot(*slot)?;
                    require(
                        slot.id == id
                            && slot.group == identity
                            && entry.identity == *slot
                            && slot.allocation > 0
                            && slot.allocation <= self.next_allocation,
                        "execution.managers.formation_slot_owner",
                    )?;
                    if let Some(owner) = entry.owner {
                        let object = objects.get(owner)?;
                        require(
                            object.map == Some(identity.map) && self.binding(owner) == Some(*slot),
                            "execution.managers.formation_unit_owner",
                        )?;
                    }
                }
            }
        }
        for (&object, &slot) in &self.bindings {
            objects.get(object)?;
            require(
                self.slot(slot)?.owner == Some(object)
                    && self
                        .states
                        .get(&slot.group)
                        .is_some_and(|state| state.slots.get(&slot.id) == Some(&Some(slot))),
                "execution.managers.formation_binding_owner",
            )?;
        }
        Ok(())
    }
}
