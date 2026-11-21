// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::native::source_execution::source_rows::{
    RespawnKind, RowDelete, RowInsert, RowOperation, RowUpdate,
};

const INVENTORY_SLOTS: u8 = 39; // Player.h:592
const ACTION_BUTTONS: u8 = 120; // Player.h:183
const SKILL_SLOTS: usize = 127; // Player.h:76
const MAX_QUEST_ROWS: usize = 256;
const MAX_REPUTATION_ROWS: usize = 128;
const MAX_RESPAWN_ROWS: usize = 32_768;
const MEDIUMINT_MAX: u32 = 0xff_ffff;

/// Existing rows matched and SQL affected rows, in statement order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationCount {
    pub matched: u32,
    pub affected: u32,
}

impl OperationCount {
    fn change(changed: bool) -> Self {
        Self {
            matched: 1,
            affected: u32::from(changed),
        }
    }

    fn delete(deleted: bool) -> Self {
        let count = u32::from(deleted);
        Self {
            matched: count,
            affected: count,
        }
    }
}

const INSERTED: OperationCount = OperationCount {
    matched: 0,
    affected: 1,
};
const DUPLICATE: Error = Error::Invalid("source INSERT target already exists");
const MISSING: Error = Error::Invalid("source UPDATE target does not exist");
const UNSUPPORTED: Error = Error::Invalid("unsupported nonempty source table");

impl Rows {
    pub fn inventory(&self) -> &[InventoryRow] {
        &self.inventory
    }
    pub fn items(&self) -> &[ItemInstanceRow] {
        &self.items
    }
    pub fn skills(&self) -> &[SkillRow] {
        &self.skills
    }
    pub fn actions(&self) -> &[ActionRow] {
        &self.actions
    }
    pub fn quests(&self) -> &[QuestRow] {
        &self.quests
    }
    pub fn reputation(&self) -> &[RepRow] {
        &self.reputation
    }
    pub fn tutorial(&self) -> Option<&TutorialRow> {
        self.tutorial.as_ref()
    }
    pub fn homebind(&self) -> Option<&HomebindRow> {
        self.homebind.as_ref()
    }
    pub fn respawns(&self) -> &[RespawnRow] {
        &self.respawns
    }
    pub fn dragons(&self) -> Option<&DragonStateRow> {
        self.dragons.as_ref()
    }

    pub(super) fn validate(&self, selected: &Identity) -> Result<(), Error> {
        self.validate_character(selected)?;
        if !self.spells.is_empty()
            || !self.cooldowns.is_empty()
            || !self.auras.is_empty()
            || !self.entered_instances.is_empty()
        {
            return Err(UNSUPPORTED);
        }
        let guid = self.character().map(|row| row.guid);
        check_table(&self.inventory, usize::from(INVENTORY_SLOTS), |_| true)?;
        check_table(&self.items, usize::from(INVENTORY_SLOTS), |_| true)?;
        if self.inventory.len() != self.items.len() {
            return Err(Error::Invalid("source inventory and item rows do not join"));
        }
        let mut slots = 0u64;
        for (inventory, item) in self.inventory.iter().zip(&self.items) {
            if inventory.item_guid != item.guid
                || inventory.item_template != item.entry
                || Some(inventory.guid) != guid
                || Some(item.owner_guid) != guid
            {
                return Err(Error::Invalid("source inventory and item rows do not join"));
            }
            if item.guid == 0
                || item.entry == 0
                || item.entry > MEDIUMINT_MAX
                || item.count == 0
                || inventory.bag_guid != 0
                || inventory.slot >= INVENTORY_SLOTS
            {
                return Err(Error::Invalid("invalid source inventory row"));
            }
            let slot = 1 << inventory.slot;
            if slots & slot != 0 {
                return Err(Error::Invalid("duplicate source inventory slot"));
            }
            slots |= slot;
        }
        check_table(&self.skills, SKILL_SLOTS, |r| {
            Some(r.guid) == guid && (1..=MEDIUMINT_MAX).contains(&r.skill) && r.value <= r.max
        })?;
        check_table(&self.actions, usize::from(ACTION_BUTTONS), |r| {
            Some(r.guid) == guid
                && r.button < ACTION_BUTTONS
                && (1..=0x00ff_ffff).contains(&r.action)
        })?;
        check_table(&self.quests, MAX_QUEST_ROWS, |r| {
            Some(r.guid) == guid && r.quest != 0 && r.status <= 5
        })?;
        check_table(&self.reputation, MAX_REPUTATION_ROWS, |r| {
            Some(r.guid) == guid && r.faction != 0
        })?;
        if self
            .tutorial
            .is_some_and(|r| r.account != u64::from(selected.account))
        {
            return Err(Error::Incompatible);
        }
        if self.homebind.is_some_and(|r| {
            Some(r.guid) != guid || r.map != 0 || r.xyz.iter().any(|x| !x.as_float().is_finite())
        }) {
            return Err(Error::Invalid("invalid source homebind row"));
        }
        check_table(&self.respawns, MAX_RESPAWN_ROWS, |r| {
            (1..=MEDIUMINT_MAX).contains(&r.guid) && r.instance == 0 && r.deadline != 0
        })?;
        if self.dragons.is_some_and(|r| {
            // WorldState.cpp:574-577,600-611 chooses a permutation of these entries.
            let mut positions = r.positions;
            positions.sort_unstable();
            r.state > 0xf || positions != [14887, 14888, 14889, 14890]
        }) {
            return Err(Error::Invalid("invalid source dragon state"));
        }
        Ok(())
    }

    pub(super) fn apply(
        &mut self,
        selected: &Identity,
        operation: RowOperation,
    ) -> Result<OperationCount, Error> {
        match operation {
            RowOperation::Insert(row) => self.insert(selected, row),
            RowOperation::ReplaceItemInstance(row) => {
                match self.items.binary_search_by_key(&row.key(), Keyed::key) {
                    Ok(index) => {
                        self.items[index] = row;
                        Ok(OperationCount {
                            matched: 1,
                            affected: 2,
                        })
                    }
                    Err(index) => {
                        insert_at(&mut self.items, usize::from(INVENTORY_SLOTS), index, row)
                    }
                }
            }
            RowOperation::Update(row) => self.update(selected, row),
            RowOperation::Delete(key) => self.delete(selected, key),
        }
    }

    fn insert(&mut self, selected: &Identity, row: RowInsert) -> Result<OperationCount, Error> {
        match row {
            RowInsert::Character(row) => {
                insert_one(&mut self.character, row)?;
                self.validate_character(selected)?;
                Ok(INSERTED)
            }
            RowInsert::Inventory(row) => {
                insert(&mut self.inventory, usize::from(INVENTORY_SLOTS), row)
            }
            RowInsert::Skill(row) => insert(&mut self.skills, SKILL_SLOTS, row),
            RowInsert::Action(row) => insert(&mut self.actions, usize::from(ACTION_BUTTONS), row),
            RowInsert::Quest(row) => insert(&mut self.quests, MAX_QUEST_ROWS, row),
            RowInsert::Reputation(row) => insert(&mut self.reputation, MAX_REPUTATION_ROWS, row),
            RowInsert::Tutorial(row) => insert_one(&mut self.tutorial, row),
            RowInsert::Homebind(row) => insert_one(&mut self.homebind, row),
            RowInsert::Respawn(row) => insert(&mut self.respawns, MAX_RESPAWN_ROWS, row),
            RowInsert::DragonState(row) => insert_one(&mut self.dragons, row),
            RowInsert::Spell(_)
            | RowInsert::Cooldown(_)
            | RowInsert::Aura(_)
            | RowInsert::EnteredInstance(_) => Err(UNSUPPORTED),
        }
    }

    fn update(&mut self, selected: &Identity, row: RowUpdate) -> Result<OperationCount, Error> {
        match row {
            RowUpdate::OnlineByGuid { guid, online } if guid == selected.guid => {
                self.online(online)
            }
            RowUpdate::OnlineByAccount { account, online } if account == selected.account => {
                self.online(online)
            }
            RowUpdate::OnlineByGuid { .. } | RowUpdate::OnlineByAccount { .. } => {
                Err(Error::Incompatible)
            }
            RowUpdate::ClearOnline => match self.character.as_mut() {
                Some(row) if row.online => {
                    row.online = false;
                    Ok(OperationCount::change(true))
                }
                _ => Ok(OperationCount::default()),
            },
            RowUpdate::Inventory(row) => update(&mut self.inventory, row),
            RowUpdate::ItemInstance(row) => update(&mut self.items, row),
            RowUpdate::Skill(row) => update(&mut self.skills, row),
            RowUpdate::Action(row) => update(&mut self.actions, row),
            RowUpdate::Quest(row) => update(&mut self.quests, row),
            RowUpdate::Tutorial(row) if row.account == u64::from(selected.account) => {
                Ok(change(self.tutorial.as_mut().ok_or(MISSING)?, row))
            }
            RowUpdate::Tutorial(_) => Err(Error::Incompatible),
        }
    }

    fn online(&mut self, online: bool) -> Result<OperationCount, Error> {
        let row = self.character.as_mut().ok_or(MISSING)?;
        Ok(change(&mut row.online, online))
    }

    fn delete(&mut self, selected: &Identity, key: RowDelete) -> Result<OperationCount, Error> {
        let deleted = match key {
            RowDelete::Character(guid) if guid == selected.guid => self.character.take().is_some(),
            RowDelete::Inventory { item_guid } => remove(&mut self.inventory, item_guid),
            RowDelete::ItemInstance(guid) => remove(&mut self.items, guid),
            RowDelete::Skill { guid, skill } if guid == selected.guid => {
                remove(&mut self.skills, (guid, skill))
            }
            RowDelete::Action { guid, button } if guid == selected.guid => {
                remove(&mut self.actions, (guid, button))
            }
            RowDelete::Reputation { guid, faction } if guid == selected.guid => {
                remove(&mut self.reputation, (guid, faction))
            }
            RowDelete::Respawn {
                kind,
                guid,
                instance,
            } => remove(&mut self.respawns, (kind, guid, instance)),
            RowDelete::DragonState => self.dragons.take().is_some(),
            // Nonempty writes for these tables must refuse at their source owners.
            RowDelete::Spell { guid, .. }
            | RowDelete::CooldownsByGuid(guid)
            | RowDelete::AurasByGuid(guid)
                if guid == selected.guid =>
            {
                false
            }
            RowDelete::EnteredInstancesByAccount(account) if account == selected.account => false,
            _ => return Err(Error::Incompatible),
        };
        Ok(OperationCount::delete(deleted))
    }
}

trait Keyed {
    type Key: Ord;
    fn key(&self) -> Self::Key;
}

macro_rules! keys {
    ($($row:ty: $key:ty = |$r:ident| $value:expr;)*) => {$(
        impl Keyed for $row {
            type Key = $key;
            fn key(&self) -> Self::Key {
                let $r = self;
                $value
            }
        }
    )*};
}

keys! {
    InventoryRow: u32 = |r| r.item_guid;
    ItemInstanceRow: u32 = |r| r.guid;
    SkillRow: (u32, u32) = |r| (r.guid, r.skill);
    ActionRow: (u32, u8) = |r| (r.guid, r.button);
    QuestRow: (u32, u32) = |r| (r.guid, r.quest);
    RepRow: (u32, u32) = |r| (r.guid, r.faction);
    RespawnRow: (RespawnKind, u32, u32) = |r| (r.kind, r.guid, r.instance);
}

fn check_table<T: Keyed>(rows: &[T], cap: usize, valid: impl Fn(&T) -> bool) -> Result<(), Error> {
    if rows.len() > cap
        || !rows.iter().all(valid)
        || rows.windows(2).any(|p| p[0].key() >= p[1].key())
    {
        return Err(Error::Invalid("invalid source table rows or keys"));
    }
    Ok(())
}

fn insert_one<T>(slot: &mut Option<T>, row: T) -> Result<OperationCount, Error> {
    if slot.is_some() {
        return Err(DUPLICATE);
    }
    *slot = Some(row);
    Ok(INSERTED)
}

fn insert<T: Keyed>(rows: &mut Vec<T>, cap: usize, row: T) -> Result<OperationCount, Error> {
    match rows.binary_search_by_key(&row.key(), Keyed::key) {
        Ok(_) => Err(DUPLICATE),
        Err(index) => insert_at(rows, cap, index, row),
    }
}

fn insert_at<T>(
    rows: &mut Vec<T>,
    cap: usize,
    index: usize,
    row: T,
) -> Result<OperationCount, Error> {
    if rows.len() >= cap {
        return Err(Error::Invalid("source row limit exceeded"));
    }
    rows.insert(index, row);
    Ok(INSERTED)
}

fn update<T: Keyed + PartialEq>(rows: &mut [T], row: T) -> Result<OperationCount, Error> {
    let index = rows
        .binary_search_by_key(&row.key(), Keyed::key)
        .map_err(|_| MISSING)?;
    Ok(change(&mut rows[index], row))
}

fn change<T: PartialEq>(slot: &mut T, row: T) -> OperationCount {
    let changed = *slot != row;
    if changed {
        *slot = row;
    }
    OperationCount::change(changed)
}

fn remove<T: Keyed>(rows: &mut Vec<T>, key: T::Key) -> bool {
    if let Ok(index) = rows.binary_search_by_key(&key, Keyed::key) {
        rows.remove(index);
        true
    } else {
        false
    }
}
