// SPDX-License-Identifier: GPL-3.0-or-later
//! Uses existing loaded template, spell and spawn owners. SQL membership is not a substitute.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeSet;
pub struct CreatureAddonCatalog<'a> {
    catalog: &'a mut VerifiedCatalog,
    dbc: BTreeMap<&'static str, BTreeSet<u32>>,
    pub templates: Option<&'a LoadedCreatureTemplates>,
    pub spawns: Option<&'a SourceEffectiveSpawns>,
    pub spells: Option<&'a SourceSpellStores>,
    pub strict_check_filtered: Option<bool>,
}
impl<'a> CreatureAddonCatalog<'a> {
    pub fn new(catalog: &'a mut VerifiedCatalog) -> Self {
        Self {
            catalog,
            dbc: BTreeMap::new(),
            templates: None,
            spawns: None,
            spells: None,
            strict_check_filtered: None,
        }
    }
    fn dbc_contains(&mut self, file: &'static str, id: u32) -> Result<bool> {
        if !self.dbc.contains_key(file) {
            let mut view = self.catalog.dbc_view(file)?;
            let mut keys = BTreeSet::new();
            for ordinal in 0..view.record_count() {
                keys.insert(
                    view.record(ordinal)?["key"]
                        .as_u64()
                        .and_then(|k| u32::try_from(k).ok())
                        .ok_or_else(|| refuse("execution.creature_addons.dbc_layout"))?,
                );
            }
            self.dbc.insert(file, keys);
        }
        Ok(self.dbc[file].contains(&id))
    }
}
impl CreatureAddonLoaderOwner for CreatureAddonCatalog<'_> {
    fn rows(&mut self, table: CreatureAddonTable) -> Result<Vec<CreatureAddonRow>> {
        Ok(self.catalog.execution_creature_addon_rows(table)?)
    }
    fn display(&mut self, id: u32) -> Result<bool> {
        self.dbc_contains("CreatureDisplayInfo.dbc", id)
            .map_err(|e| {
                refuse(&format!(
                    "execution.creature_addons.ObjectMgr_678.creature_display_info_owner.{}",
                    e.id
                ))
            })
    }
    fn emote(&mut self, id: u32) -> Result<bool> {
        self.dbc_contains("Emotes.dbc", id).map_err(|e| {
            refuse(&format!(
                "execution.creature_addons.ObjectMgr_688.emotes_owner.{}",
                e.id
            ))
        })
    }
    fn spell(&mut self, id: u32) -> Result<Option<&SpellTemplate>> {
        Ok(self
            .spells
            .ok_or_else(|| refuse("execution.creature_addons.ObjectMgr_622.spell_template_owner"))?
            .template(id))
    }
    fn cast_time(&mut self, id: u32) -> Result<Option<i32>> {
        self.spells
            .ok_or_else(|| {
                refuse("execution.creature_addons.ObjectMgr_636.spell_cast_times_owner")
            })?
            .dbc("SpellCastTimes.dbc", id)
            .map_err(|_| refuse("execution.creature_addons.ObjectMgr_636.spell_cast_times_owner"))?
            .map(|r| {
                r.words
                    .get(&1)
                    .copied()
                    .map(|v| v as i32)
                    .ok_or_else(|| refuse("execution.creature_addons.spell_cast_times_layout"))
            })
            .transpose()
    }
    fn strict_check_filtered(&mut self) -> Result<bool> {
        self.strict_check_filtered.ok_or_else(|| {
            refuse("execution.creature_addons.ObjectMgr_646.db_strict_log_filter_owner")
        })
    }
    fn creature_template(&mut self, id: u32) -> Result<bool> {
        Ok(self
            .templates
            .ok_or_else(|| {
                refuse("execution.creature_addons.ObjectMgr_707.loaded_creature_templates_owner")
            })?
            .get(id)
            .is_some())
    }
    fn creature_guid(&mut self, id: u32) -> Result<bool> {
        Ok(self
            .spawns
            .ok_or_else(|| {
                refuse("execution.creature_addons.ObjectMgr_715.loaded_creature_data_owner")
            })?
            .pre_terrain_data(SpawnKey {
                kind: GridSpawnKind::Creature,
                guid: id,
            })
            .map_err(|e| {
                refuse(&format!(
                    "execution.creature_addons.ObjectMgr_715.loaded_creature_data_owner.{}",
                    e.id
                ))
            })?
            .is_some())
    }
}
