// SPDX-License-Identifier: GPL-3.0-or-later
//! Narrow F2.6 DBC/F2.8 SQL operands for EventAI validation, not gameplay admission.
use super::super::production_compiler::VerifiedCatalog;
use super::quest_map::EXPLORATION;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn catalog_event_ai_rows(catalog: &mut VerifiedCatalog) -> Result<Vec<EventAiRow>> {
    Ok(catalog
        .execution_event_ai_operands(
            "creature_ai_scripts",
            [
                "id",
                "creature_id",
                "event_type",
                "event_inverse_phase_mask",
                "event_chance",
                "event_flags",
                "event_param1",
                "event_param2",
                "event_param3",
                "event_param4",
                "event_param5",
                "event_param6",
                "action1_type",
                "action1_param1",
                "action1_param2",
                "action1_param3",
                "action2_type",
                "action2_param1",
                "action2_param2",
                "action2_param3",
                "action3_type",
                "action3_param1",
                "action3_param2",
                "action3_param3",
            ],
        )?
        .into_iter()
        .map(|(ordinal, words)| EventAiRow { ordinal, words })
        .collect())
}
/// Caches narrow ID/metadata projections, not decoded whole catalog rows.
pub struct EventAiCatalog<'a> {
    catalog: &'a mut VerifiedCatalog,
    quests: &'a SourceQuestMap,
    ids: BTreeMap<EventAiLookup, BTreeSet<u32>>,
    spells: Option<BTreeMap<u32, EventAiSpell>>,
    config: EventAiLoaderConfig,
    spell_lists: Option<&'a LoadedCreatureSpellLists>,
    script_templates: Option<&'a SourceDbScriptTemplates>,
}
impl<'a> EventAiCatalog<'a> {
    pub fn new(
        catalog: &'a mut VerifiedCatalog,
        config: EventAiLoaderConfig,
        quests: &'a SourceQuestMap,
    ) -> Self {
        Self {
            catalog,
            quests,
            ids: BTreeMap::new(),
            spells: None,
            config,
            spell_lists: None,
            script_templates: None,
        }
    }
    pub fn bind_spell_lists(&mut self, lists: &'a LoadedCreatureSpellLists) {
        self.spell_lists = Some(lists);
    }
    pub fn bind_script_templates(&mut self, templates: &'a SourceDbScriptTemplates) {
        self.script_templates = Some(templates);
    }
    fn ids(&mut self, domain: EventAiLookup) -> Result<&BTreeSet<u32>> {
        use EventAiLookup::*;
        if !self.ids.contains_key(&domain) {
            let dbc = match domain {
                EmoteText => Some("EmotesText.dbc"),
                Faction => Some("FactionTemplate.dbc"),
                Display => Some("CreatureDisplayInfo.dbc"),
                Sound => Some("SoundEntries.dbc"),
                Emote => Some("Emotes.dbc"),
                Map => Some("Map.dbc"),
                Area => Some("AreaTable.dbc"),
                _ => None,
            };
            let mut ids = BTreeSet::new();
            if let Some(file) = dbc {
                let mut view = self.catalog.dbc_view(file)?;
                for ordinal in 0..view.record_count() {
                    let row = view.record(ordinal)?;
                    // GetAreaEntryByAreaID searches AreaTable.ID, DBCStores.cpp:219-227.
                    ids.insert(dbc_word(&row, 0)?);
                }
            } else {
                let (table, column) = match domain {
                    Creature => ("creature_template", "entry"),
                    Guid => ("creature", "guid"),
                    Condition => ("conditions", "condition_entry"),
                    Broadcast => ("broadcast_text", "id"),
                    // These lookups consume loaded manager products, not merely a raw key.
                    Summon => return Err(refuse("execution.event_ai.loader.summon_store_owner")),
                    Relay => return Err(refuse("execution.event_ai.loader.relay_store_owner")),
                    RelayTemplate => {
                        return Err(refuse("execution.event_ai.loader.relay_template_owner"))
                    }
                    SpellList | TextTemplate => {
                        unreachable!("loaded stores are resolved in contains")
                    }
                    ImmunitySet => {
                        return Err(refuse("execution.event_ai.loader.immunity_store_owner"))
                    }
                    _ => unreachable!("DBC domains handled above"),
                };
                for (_, row) in self.catalog.execution_event_ai_operands(table, [column])? {
                    ids.insert(row[0]);
                }
            }
            self.ids.insert(domain, ids);
        }
        Ok(&self.ids[&domain])
    }
}
impl EventAiLoaderOwner for EventAiCatalog<'_> {
    fn contains(&mut self, domain: EventAiLookup, id: u32, _entry: u32) -> Result<bool> {
        if domain == EventAiLookup::Quest {
            // CreatureEventAIMgr.cpp:406.
            return Ok(self.quests.template(id).is_some());
        }
        if domain == EventAiLookup::SpellList {
            return Ok(self
                .spell_lists
                .ok_or_else(|| refuse("execution.event_ai.loader.spell_list_store_owner"))?
                .get(id)
                .is_some());
        }
        if domain == EventAiLookup::TextTemplate {
            // ScriptMgr.h:679 checks membership only in STRING_TEMPLATE.
            return Ok(self
                .script_templates
                .ok_or_else(|| {
                    refuse("execution.event_ai.CreatureEventAIMgr_990.string_template_store_owner")
                })?
                .string(id)
                .is_some());
        }
        Ok(self.ids(domain)?.contains(&id))
    }
    fn spell(&mut self, id: u32) -> Result<Option<EventAiSpell>> {
        if self.spells.is_none() {
            // SQLStorages.cpp:40,55 stores these F2.8 u32 fields without SpellMgr derivation.
            let rows = self.catalog.execution_event_ai_operands(
                "spell_template",
                [
                    "id",
                    "school",
                    "targets",
                    "attributesex",
                    "attributesex3",
                    "rangeindex",
                    "effect1",
                    "effect2",
                    "effect3",
                    "effectimplicittargeta1",
                    "effectimplicittargeta2",
                    "effectimplicittargeta3",
                    "effectimplicittargetb1",
                    "effectimplicittargetb2",
                    "effectimplicittargetb3",
                ],
            )?;
            let mut spells = BTreeMap::new();
            for (_, w) in rows {
                require(
                    !spells.contains_key(&w[0]),
                    "execution.event_ai.loader.duplicate_spell_operand",
                )?;
                spells.insert(
                    w[0],
                    EventAiSpell {
                        school: w[1],
                        targets: w[2],
                        attributes_ex: w[3],
                        attributes_ex3: w[4],
                        range_index: w[5],
                        effects: w[6..9].try_into().unwrap(),
                        target_a: w[9..12].try_into().unwrap(),
                        target_b: w[12..15].try_into().unwrap(),
                    },
                );
            }
            self.spells = Some(spells);
        }
        Ok(self
            .spells
            .as_ref()
            .expect("loaded spell operands")
            .get(&id)
            .cloned())
    }
    fn quest_exploration(&mut self, id: u32) -> Result<Option<bool>> {
        // CreatureEventAIMgr.cpp:754-760,791-797.
        Ok(self
            .quests
            .template(id)
            .map(|quest| quest.has_special_flag(EXPLORATION)))
    }
    fn assistance_radius(&mut self) -> Result<u32> {
        self.config.assistance_radius()
    }
    fn dev_log_filter(&self) -> bool {
        self.config.dev_log_filter()
    }
}
fn dbc_word(row: &serde_json::Value, column: u64) -> Result<u32> {
    row["values"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v[0].as_u64() == Some(column)))
        .and_then(|v| v[2].as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| refuse("execution.event_ai.loader.dbc_layout"))
}
