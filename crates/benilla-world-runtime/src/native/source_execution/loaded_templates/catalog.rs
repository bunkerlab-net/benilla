// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeSet;

/// Lazy immutable existence views; no downstream model, equipment or page fields are supplied.
pub struct TemplateCatalog<'a> {
    catalog: &'a mut VerifiedCatalog,
    ids: BTreeMap<TemplateLookup, BTreeSet<u32>>,
    displays: Option<BTreeMap<u32, u32>>,
    classes: Option<CreatureLevelDefinitions>,
}
impl<'a> TemplateCatalog<'a> {
    pub fn new(catalog: &'a mut VerifiedCatalog) -> Self {
        Self {
            catalog,
            ids: BTreeMap::new(),
            displays: None,
            classes: None,
        }
    }
    fn ids(&mut self, domain: TemplateLookup) -> Result<&BTreeSet<u32>> {
        if !self.ids.contains_key(&domain) {
            use TemplateLookup::*;
            let file = match domain {
                Faction => Some("FactionTemplate.dbc"),
                CreatureType => Some("CreatureType.dbc"),
                Family => Some("CreatureFamily.dbc"),
                PetSpell => Some("CreatureSpellData.dbc"),
                Lock => Some("Lock.dbc"),
                SpellFocus => Some("SpellFocusObject.dbc"),
                _ => None,
            };
            let ids = if let Some(file) = file {
                let mut view = self.catalog.dbc_view(file)?;
                let mut ids = BTreeSet::new();
                for ordinal in 0..view.record_count() {
                    ids.insert(dbc_word(&view.record(ordinal)?, 0)?);
                }
                ids
            } else {
                match domain {
                    Model => self
                        .catalog
                        .execution_template_reference_ids("creature_model_info", "modelid")?,
                    PageText => self
                        .catalog
                        .execution_template_reference_ids("page_text", "entry")?,
                    StringId => self
                        .catalog
                        .execution_effective_spawn_operands("string_id", ["id"])?
                        .into_iter()
                        .map(|[id]| id)
                        .collect(),
                    TaxiPath => self.catalog.execution_template_taxi_paths()?,
                    Equipment => self
                        .catalog
                        .execution_equipment_template_ids("creature_equip_template", "entry")?,
                    Spell => self
                        .catalog
                        .execution_event_ai_operands("spell_template", ["id"])?
                        .into_iter()
                        .map(|(_, [id])| id)
                        .collect(),
                    _ => unreachable!("DBC domains handled above"),
                }
            };
            self.ids.insert(domain, ids);
        }
        Ok(&self.ids[&domain])
    }
}
impl TemplateValidationOwner for TemplateCatalog<'_> {
    fn contains(&mut self, domain: TemplateLookup, id: u32) -> Result<bool> {
        Ok(self.ids(domain)?.contains(&id))
    }
    fn display_scale(&mut self, id: u32) -> Result<Option<u32>> {
        if self.displays.is_none() {
            let mut view = self.catalog.dbc_view("CreatureDisplayInfo.dbc")?;
            let mut displays = BTreeMap::new();
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                displays.insert(dbc_word(&row, 0)?, dbc_word(&row, 4)?);
            }
            self.displays = Some(displays);
        }
        Ok(self.displays.as_ref().unwrap().get(&id).copied())
    }
    fn class_stats(&mut self, level: u32, class: u32) -> Result<bool> {
        if self.classes.is_none() {
            self.classes = Some(CreatureLevelDefinitions::from_rows(
                Vec::new(),
                self.catalog.creature_level_classes()?,
            )?);
        }
        Ok(self
            .classes
            .as_ref()
            .unwrap()
            .has_loader_stats(level, class))
    }
}
fn dbc_word(row: &serde_json::Value, column: u32) -> Result<u32> {
    row["values"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v[0] == column))
        .and_then(|v| v[2].as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| refuse("execution.templates.dbc_layout"))
}
impl LoadedCreatureTemplates {
    pub fn catalog_diagnostic(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let rows = catalog.execution_loaded_creature_rows()?;
        Self::load(rows, &mut TemplateCatalog::new(catalog))
    }
}
impl LoadedGameObjectTemplates {
    pub fn catalog_diagnostic(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let rows = catalog.execution_loaded_gameobject_rows()?;
        Self::load(rows, &mut TemplateCatalog::new(catalog))
    }
}
