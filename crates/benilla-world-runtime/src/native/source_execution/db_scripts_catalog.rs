// SPDX-License-Identifier: GPL-3.0-or-later
//! Narrow retained F2 inputs for the source DB-script loader.
use super::super::production_compiler::VerifiedCatalog;
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[path = "quest_flags.rs"]
mod quest_flags;
pub use quest_flags::*;

struct QuestFlagCatalog<'a> {
    spells: Option<&'a SourceSpellStores>,
    triggers: BTreeSet<u32>,
}
impl QuestFlagLoaderOwner for QuestFlagCatalog<'_> {
    fn spell_effects(&mut self, id: u32) -> Result<Option<[(u32, i32); 3]>> {
        let spells = self
            .spells
            .ok_or_else(|| refuse("execution.quest_flags.spell_template_owner"))?;
        Ok(spells.template(id).map(|spell| {
            [
                (spell.effect1, spell.effectmiscvalue1),
                (spell.effect2, spell.effectmiscvalue2),
                (spell.effect3, spell.effectmiscvalue3),
            ]
        }))
    }
    fn area_trigger(&mut self, id: u32) -> Result<bool> {
        Ok(self.triggers.contains(&id))
    }
}
impl SourceQuestFlags {
    /// Projection of only flag-affecting reads, not full quest validation.
    pub fn load_catalog(
        catalog: &mut VerifiedCatalog,
        spells: Option<&SourceSpellStores>,
    ) -> Result<Self> {
        let rows = catalog.execution_quest_flag_operands(
            "quest_template",
            [
                "entry",
                "SpecialFlags",
                "ReqItemId1",
                "ReqItemId2",
                "ReqItemId3",
                "ReqItemId4",
                "ReqSpellCast1",
                "ReqSpellCast2",
                "ReqSpellCast3",
                "ReqSpellCast4",
                "ReqCreatureOrGOId1",
                "ReqCreatureOrGOId2",
                "ReqCreatureOrGOId3",
                "ReqCreatureOrGOId4",
                "LimitTime",
            ],
        )?;
        let mut owner = QuestFlagCatalog {
            spells,
            triggers: BTreeSet::new(),
        };
        let mut result = Self::load_quests(
            rows.into_iter().map(|v| QuestFlagRow {
                id: v[0],
                special_flags: v[1] as u16,
                required_items: v[2..6].try_into().unwrap(),
                required_spells: v[6..10].try_into().unwrap(),
                required_targets: v[10..14]
                    .try_into()
                    .map(|v: [u32; 4]| v.map(|v| v as i32))
                    .unwrap(),
                limit_time: v[14],
            }),
            &mut owner,
        )?;
        let triggers = catalog
            .execution_quest_flag_operands("areatrigger_involvedrelation", ["id", "quest"])?;
        if !triggers.is_empty() {
            let mut view = catalog.dbc_view("AreaTrigger.dbc").map_err(|error| {
                if error.id == "dbc.mandatory_file_missing" {
                    refuse("execution.quest_flags.area_trigger_dbc_owner")
                } else {
                    error.into()
                }
            })?;
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                let id = row["values"]
                    .as_array()
                    .and_then(|r| r.iter().find(|v| v[0] == 0))
                    .and_then(|v| v[2].as_u64())
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| refuse("execution.quest_flags.area_trigger_dbc_layout"))?;
                owner.triggers.insert(id);
            }
        }
        result.load_area_triggers(triggers, &mut owner)?;
        Ok(result)
    }
}

impl DbScriptLoaderOwner for SourceSpawnGroups {
    fn contains(&mut self, kind: DbScriptLookup, id: u32) -> Result<bool> {
        if kind == DbScriptLookup::SpawnGroup {
            // ScriptMgr.cpp:246-254 consumes the actual F4.05 loaded container.
            // spawn_groups.rs:149-155,440-442 does not expose raw SQL as loaded.
            return Ok(self.definitions()?.contains_key(&id));
        }
        Err(refuse(&format!(
            "execution.db_scripts.{}_owner",
            kind.owner()
        )))
    }
}

impl SourceWaypointPaths {
    /// Global WaypointManager::Load consumes validated ScriptMgr definitions.
    pub fn load_catalog_with_db_scripts(
        catalog: &mut VerifiedCatalog,
        creatures: &BTreeSet<u32>,
        templates: &BTreeSet<u32>,
        scripts: &SourceDbScriptMaps,
    ) -> Result<Self> {
        let projection = scripts.waypoint_maps()?;
        Self::load_catalog(catalog, creatures, templates, Some(&projection))
    }
}

impl SourceDbScriptMaps {
    /// Decode checked F2.7 cursors, then execute the source loader with real owners.
    pub fn load_catalog(
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
        kinds: &[DbScriptType],
        spells: Option<&SourceSpellStores>,
        external: &mut impl DbScriptLoaderOwner,
        strict_check_filtered: Option<bool>,
    ) -> Result<Self> {
        let templates = catalog_db_script_templates(catalog, occurrences)?;
        let tables = kinds
            .iter()
            .map(|&kind| {
                catalog_db_script_rows(catalog, occurrences, kind).map(|rows| (kind, rows))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::load(
            tables,
            templates,
            &mut DbScriptCatalog::new(catalog, spells, external, strict_check_filtered),
        )
    }
}

pub fn catalog_db_script_rows(
    catalog: &mut VerifiedCatalog,
    occurrences: &Path,
    kind: DbScriptType,
) -> Result<Vec<DbScriptRow>> {
    Ok(catalog
        .execution_db_script_rows::<20>(occurrences, kind.table())?
        .into_iter()
        .map(|(ordinal, v)| DbScriptRow {
            ordinal,
            id: v[0],
            delay: v[1],
            command: v[2],
            data: v[3..6].try_into().unwrap(),
            buddy: v[6],
            radius_or_guid: v[7],
            flags: v[8],
            text: [v[9] as i32, v[10] as i32, v[11] as i32, v[12] as i32],
            floats: v[13..19].try_into().unwrap(),
            condition: v[19],
        })
        .collect())
}
pub fn catalog_db_script_templates(
    catalog: &mut VerifiedCatalog,
    occurrences: &Path,
) -> Result<SourceDbScriptTemplates> {
    let rows = catalog.execution_db_script_rows::<4>(occurrences, "dbscript_random_templates")?;
    SourceDbScriptTemplates::load(rows.into_iter().map(|(ordinal, v)| DbScriptRandomRow {
        ordinal,
        id: v[0],
        kind: v[1],
        target: v[2] as i32,
        chance: v[3],
    }))
}
/// F2 existence views, loaded F5.07 SpellEntry and effective quest flags.
/// Loaded pool/group products and configuration remain external owners.
pub struct DbScriptCatalog<'a, O> {
    catalog: &'a mut VerifiedCatalog,
    external: &'a mut O,
    spells: Option<&'a SourceSpellStores>,
    ids: BTreeMap<DbScriptLookup, BTreeSet<u32>>,
    go_entries: Option<BTreeMap<u32, u32>>,
    go_types: Option<BTreeMap<u32, u32>>,
    strict_check_filtered: Option<bool>,
    quest_flags: Option<SourceQuestFlags>,
}
impl<'a, O: DbScriptLoaderOwner> DbScriptCatalog<'a, O> {
    pub fn new(
        catalog: &'a mut VerifiedCatalog,
        spells: Option<&'a SourceSpellStores>,
        external: &'a mut O,
        strict_check_filtered: Option<bool>,
    ) -> Self {
        Self {
            catalog,
            external,
            spells,
            ids: BTreeMap::new(),
            go_entries: None,
            go_types: None,
            strict_check_filtered,
            quest_flags: None,
        }
    }
    fn quest_flags(&mut self) -> Result<&mut SourceQuestFlags> {
        if self.quest_flags.is_none() {
            self.quest_flags = Some(SourceQuestFlags::load_catalog(self.catalog, self.spells)?);
        }
        Ok(self.quest_flags.as_mut().unwrap())
    }
    fn ids(&mut self, kind: DbScriptLookup) -> Result<&BTreeSet<u32>> {
        use DbScriptLookup::*;
        if !self.ids.contains_key(&kind) {
            let dbc = match kind {
                Emote => Some("Emotes.dbc"),
                Map => Some("Map.dbc"),
                Sound => Some("SoundEntries.dbc"),
                Faction => Some("FactionTemplate.dbc"),
                Display => Some("CreatureDisplayInfo.dbc"),
                TaxiPath => Some("TaxiPath.dbc"),
                MailTemplate => Some("MailTemplate.dbc"),
                Area => Some("AreaTable.dbc"),
                _ => None,
            };
            let mut ids = BTreeSet::new();
            if let Some(file) = dbc {
                let mut view = self.catalog.dbc_view(file)?;
                for ordinal in 0..view.record_count() {
                    let row = view.record(ordinal)?;
                    let id = row["values"]
                        .as_array()
                        .and_then(|r| r.iter().find(|v| v[0] == 0))
                        .and_then(|v| v[2].as_u64())
                        .and_then(|v| u32::try_from(v).ok())
                        .ok_or_else(|| refuse("execution.db_scripts.dbc_layout"))?;
                    ids.insert(id);
                }
            } else {
                let (table, column) = match kind {
                    Creature => ("creature_template", "entry"),
                    GameObject => ("gameobject_template", "entry"),
                    CreatureGuid => ("creature", "guid"),
                    GameObjectGuid => ("gameobject", "guid"),
                    Condition => ("conditions", "condition_entry"),
                    Item => ("item_template", "entry"),
                    StringId => ("string_id", "id"),
                    WorldStateName => ("worldstate_name", "id"),
                    Quest => ("quest_template", "entry"),
                    _ => return Err(refuse("execution.db_scripts.external_lookup_domain")),
                };
                for row in self.catalog.execution_db_script_operands(table, [column])? {
                    ids.insert(row[0]);
                }
            }
            self.ids.insert(kind, ids);
        }
        Ok(&self.ids[&kind])
    }
}
impl<O: DbScriptLoaderOwner> DbScriptLoaderOwner for DbScriptCatalog<'_, O> {
    fn contains(&mut self, kind: DbScriptLookup, id: u32) -> Result<bool> {
        match kind {
            DbScriptLookup::Spell => Ok(self
                .spells
                .ok_or_else(|| refuse("execution.db_scripts.spell_template_owner"))?
                .template(id)
                .is_some()),
            DbScriptLookup::CreaturePool
            | DbScriptLookup::GameObjectPool
            | DbScriptLookup::SpawnGroup => self.external.contains(kind, id),
            _ => Ok(self.ids(kind)?.contains(&id)),
        }
    }
    fn quest_exploration(&mut self, id: u32) -> Result<Option<bool>> {
        self.quest_flags()?.quest_exploration(id)
    }
    fn set_quest_exploration(&mut self, id: u32) -> Result<()> {
        self.quest_flags()?.set_quest_exploration(id)
    }
    fn gameobject_entry(&mut self, guid: u32) -> Result<Option<u32>> {
        if self.go_entries.is_none() {
            self.go_entries = Some(
                self.catalog
                    .execution_db_script_operands("gameobject", ["guid", "id"])?
                    .into_iter()
                    .map(|v| (v[0], v[1]))
                    .collect(),
            );
        }
        Ok(self.go_entries.as_ref().unwrap().get(&guid).copied())
    }
    fn gameobject_type(&mut self, entry: u32) -> Result<Option<u32>> {
        if self.go_types.is_none() {
            self.go_types = Some(
                self.catalog
                    .execution_db_script_operands("gameobject_template", ["entry", "type"])?
                    .into_iter()
                    .map(|v| (v[0], v[1]))
                    .collect(),
            );
        }
        Ok(self.go_types.as_ref().unwrap().get(&entry).copied())
    }
    fn max_pool_id(&mut self) -> Result<u32> {
        self.external.max_pool_id()
    }
    fn taxi_spell(&mut self, path: u32) -> Result<Option<u32>> {
        if self
            .strict_check_filtered
            .ok_or_else(|| refuse("execution.db_scripts.db_strict_log_filter_owner"))?
        {
            return Ok(None);
        }
        let spells = self
            .spells
            .ok_or_else(|| refuse("execution.db_scripts.spell_template_owner"))?;
        // Ascending SQLStorage index, excluding slot zero, ScriptMgr.cpp:659-669.
        Ok(spells.templates().range(1..).find_map(|(&id, spell)| {
            [
                (spell.effect1, spell.effectmiscvalue1),
                (spell.effect2, spell.effectmiscvalue2),
                (spell.effect3, spell.effectmiscvalue3),
            ]
            .iter()
            .any(|&(effect, value)| effect == 123 && value == path as i32)
            .then_some(id)
        }))
    }
}
