// SPDX-License-Identifier: GPL-3.0-or-later
//! ObjectMgr.cpp:1758-1824 loads spawn overrides before LoadCreatures.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const CREATURE_SPAWN_DATA_QUERY: &str = "SELECT Entry, NpcFlags, UnitFlags, Faction, ModelId, EquipmentId, CurHealth, CurMana, SpawnFlags, RelayId, StringId, Name FROM creature_spawn_data_template";
pub const GAMEOBJECT_ADDON_QUERY: &str =
    "SELECT guid, animprogress, state, stringId FROM gameobject_addon";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureSpawnTemplateRow {
    pub values: [u64; 11],
    pub name: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedCreatureSpawnTemplate {
    pub npc_flags: i32,
    pub unit_flags: i64,
    pub faction: u32,
    pub model_id: u32,
    pub equipment_id: i32,
    pub cur_health: u32,
    pub cur_mana: u32,
    pub spawn_flags: u32,
    pub relay_id: u32,
    pub string_id: u32,
}
#[derive(Default)]
pub struct LoadedCreatureSpawnTemplates {
    rows: BTreeMap<u32, LoadedCreatureSpawnTemplate>,
    pub diagnostics: Vec<(u32, &'static str, u32)>,
}
impl LoadedCreatureSpawnTemplates {
    pub fn load(
        rows: impl IntoIterator<Item = CreatureSpawnTemplateRow>,
        strings: &BTreeSet<u32>,
    ) -> Self {
        let mut loaded = Self::default();
        for row in rows {
            let v = row.values;
            let entry = v[0] as u32;
            if row.name.is_empty() {
                loaded.diagnostics.push((entry, "empty_name", 1796));
            }
            let mut string_id = v[10] as u32;
            if string_id != 0 && !strings.contains(&string_id) {
                string_id = 0;
                loaded
                    .diagnostics
                    .push((entry, "missing_string_cleared", 1801));
            }
            loaded.rows.insert(
                entry,
                LoadedCreatureSpawnTemplate {
                    npc_flags: v[1] as u32 as i32,
                    unit_flags: v[2] as i64,
                    faction: v[3] as u32,
                    model_id: v[4] as u32,
                    equipment_id: v[5] as u32 as i32,
                    cur_health: v[6] as u32,
                    cur_mana: v[7] as u32,
                    spawn_flags: v[8] as u32,
                    relay_id: v[9] as u32,
                    string_id,
                },
            );
        }
        loaded
    }
    pub fn rows(&self) -> &BTreeMap<u32, LoadedCreatureSpawnTemplate> {
        &self.rows
    }
    pub fn bind_effective_spawns(&self, inputs: &mut EffectiveSpawnInputs) {
        inputs.spawn_templates = self.rows.keys().copied().collect();
    }
    /// Creature.cpp:666 dereferences the nullable GetCreatureSpawnTemplate result.
    pub fn model_id(&self, data: &EffectiveSpawnData) -> Result<u32> {
        let entry = data.spawn_template.ok_or_else(|| {
            refuse("execution.creature.spawn_template_null_dereference_undefined")
        })?;
        self.rows
            .get(&entry)
            .map(|row| row.model_id)
            .ok_or_else(|| refuse("execution.object_mgr.loaded_creature_spawn_data_template_owner"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(entry: u32, string: u32, name: &[u8]) -> CreatureSpawnTemplateRow {
        CreatureSpawnTemplateRow {
            values: [
                u64::from(entry),
                u32::MAX.into(),
                u64::MAX,
                35,
                12,
                u32::MAX.into(),
                100,
                20,
                7,
                4,
                string.into(),
            ],
            name: name.to_vec(),
        }
    }
    #[test]
    fn spawn_template_loader_retains_fields_and_validates_only_name_and_string() {
        let loaded = LoadedCreatureSpawnTemplates::load(
            [row(0, 0, b"default"), row(2, 5, b"valid"), row(3, 9, b"")],
            &BTreeSet::from([5]),
        );
        assert_eq!(
            loaded.rows()[&0],
            LoadedCreatureSpawnTemplate {
                npc_flags: -1,
                unit_flags: -1,
                faction: 35,
                model_id: 12,
                equipment_id: -1,
                cur_health: 100,
                cur_mana: 20,
                spawn_flags: 7,
                relay_id: 4,
                string_id: 0,
            }
        );
        assert_eq!(loaded.rows()[&2].string_id, 5);
        assert_eq!(loaded.rows()[&3].string_id, 0);
        assert_eq!(
            loaded.diagnostics,
            [(3, "empty_name", 1796), (3, "missing_string_cleared", 1801)]
        );
        let mut later = row(2, 0, b"b");
        later.values[1] = 1;
        let replaced =
            LoadedCreatureSpawnTemplates::load([row(2, 5, b"a"), later], &BTreeSet::from([5]));
        assert_eq!(replaced.rows().len(), 1);
        assert_eq!(replaced.rows()[&2].string_id, 0);
        assert_eq!(replaced.rows()[&2].npc_flags, 1);
        assert!(LoadedCreatureSpawnTemplates::load([], &BTreeSet::new())
            .rows()
            .is_empty());
    }
}
