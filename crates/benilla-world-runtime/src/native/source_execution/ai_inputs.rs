// SPDX-License-Identifier: GPL-3.0-or-later
//! Narrow catalog operands. Raw templates and unsealed registrations do not admit gameplay.
use super::super::production_compiler::{
    CatalogCreatureAiTemplate, CatalogScriptAiBinding, VerifiedCatalog,
};
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureAiTemplateOperands {
    pub ordinal: u64,
    pub entry: u32,
    pub faction: u32,
    pub extra_flags: u32,
    pub unit_flags: u32,
    pub type_flags: u32,
    pub static_flags: [u32; 4],
    pub rank: u32,
    pub inhabit_type: u32,
    pub stats_multipliers: [u32; 3],
    pub civilian: bool,
    pub pursuit_ms: u32,
    pub call_for_help: u32,
    pub creature_type: u32,
    pub ai_name: String,
    pub script_name: String,
}
impl From<CatalogCreatureAiTemplate> for CreatureAiTemplateOperands {
    fn from(row: CatalogCreatureAiTemplate) -> Self {
        Self {
            ordinal: row.ordinal,
            entry: row.entry,
            faction: row.faction,
            extra_flags: row.extra_flags,
            unit_flags: row.unit_flags,
            type_flags: row.type_flags,
            static_flags: row.static_flags,
            rank: row.rank,
            inhabit_type: row.inhabit_type,
            stats_multipliers: row.stats_multipliers,
            civilian: row.civilian,
            pursuit_ms: row.pursuit_ms,
            call_for_help: row.call_for_help,
            creature_type: row.creature_type,
            ai_name: row.ai_name,
            script_name: row.script_name,
        }
    }
}
impl CreatureAiTemplateOperands {
    /// Supply the actual Unit/control owner operands after effective-entry selection.
    pub fn inputs(self, control: CreatureAiControl, neutral_to_all: bool) -> CreatureAiInputs {
        CreatureAiInputs {
            effective_entry: self.entry,
            ai_name: self.ai_name,
            script_name: self.script_name,
            control,
            guard: self.extra_flags & 0x400 != 0,
            no_aggro_on_sight: self.extra_flags & 2 != 0,
            neutral_to_all,
        }
    }
}
pub fn catalog_creature_ai_template(
    catalog: &mut VerifiedCatalog,
    entry: u32,
) -> Result<Option<CreatureAiTemplateOperands>> {
    let mut found = None;
    let mut duplicate = false;
    catalog.execution_ai_templates(Some(entry), |row| {
        duplicate |= found.is_some();
        found = Some(row.into());
        Ok(())
    })?;
    require(!duplicate, "execution.ai.duplicate_template")?;
    Ok(found)
}
/// Census-only raw alternatives, not a second production template store.
pub fn catalog_creature_ai_census_inputs(
    catalog: &mut VerifiedCatalog,
) -> Result<Vec<CreatureAiTemplateOperands>> {
    let mut rows = Vec::new();
    catalog.execution_ai_templates(None, |row| {
        rows.push(row.into());
        Ok(())
    })?;
    Ok(rows)
}
impl TryFrom<CatalogScriptAiBinding> for ScriptDevAiBinding {
    type Error = Refusal;
    fn try_from(row: CatalogScriptAiBinding) -> Result<Self> {
        Ok(Self {
            script_id: row.script_id,
            name: row.name,
            registration: row.registration,
            operation: row.operation,
            source_file: row.source_file,
            source_line: row.source_line,
            source_function: row.source_function,
            registry_sha256: Sha256Id::parse(&row.registry_sha256)?,
            get_ai_nonnull: row.get_ai_nonnull,
        })
    }
}
impl AiRegistryBuilder {
    pub fn load_catalog_scripts(&mut self, catalog: &VerifiedCatalog, report: &Path) -> Result<()> {
        let rows = catalog
            .execution_script_ai_bindings(report)?
            .into_iter()
            .map(ScriptDevAiBinding::try_from)
            .collect::<Result<Vec<_>>>()?;
        self.load_scripts(rows)
    }
}
/// Unit.cpp:6228-6238 and DBCStructure.h:441-446 over complete retained DBC files.
#[derive(Default)]
pub struct CreatureAiFactions {
    templates: BTreeMap<u32, (u32, bool)>,
    reputation: BTreeMap<u32, bool>,
}
impl CreatureAiFactions {
    pub fn load_catalog(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let mut this = Self::default();
        let mut view = catalog.dbc_view("Faction.dbc")?;
        for ordinal in 0..view.record_count() {
            let row = view.record(ordinal)?;
            this.reputation
                .insert(word(&row, 0)?, (word(&row, 1)? as i32) >= 0);
        }
        drop(view);
        let mut view = catalog.dbc_view("FactionTemplate.dbc")?;
        for ordinal in 0..view.record_count() {
            let row = view.record(ordinal)?;
            let mut neutral = true;
            for column in [4, 5, 6, 7, 8, 9] {
                neutral &= word(&row, column)? == 0;
            }
            this.templates
                .insert(word(&row, 0)?, (word(&row, 1)?, neutral));
        }
        Ok(this)
    }
    pub fn neutral_to_all(&self, faction: u32) -> bool {
        let Some(&(raw_faction, neutral)) = self.templates.get(&faction) else {
            return true;
        };
        raw_faction == 0
            || (!self.reputation.get(&raw_faction).copied().unwrap_or(false) && neutral)
    }
}
fn word(row: &serde_json::Value, column: u64) -> Result<u32> {
    let values = row["values"]
        .as_array()
        .ok_or_else(|| refuse("execution.ai.faction_layout"))?;
    let value = values
        .iter()
        .find(|value| value[0].as_u64() == Some(column))
        .and_then(|value| value[2].as_u64())
        .ok_or_else(|| refuse("execution.ai.faction_layout"))?;
    u32::try_from(value).map_err(|_| refuse("execution.ai.faction_layout"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn faction_neutrality_preserves_missing_zero_reputation_and_hostility_branches() {
        // Unit.cpp:6228-6238, including its reputation override of neutral masks.
        let factions = CreatureAiFactions {
            templates: [
                (1, (0, false)),
                (2, (7, true)),
                (3, (8, true)),
                (4, (9, false)),
                (5, (10, true)),
            ]
            .into(),
            reputation: [(7, true), (8, false)].into(),
        };
        assert!(factions.neutral_to_all(99));
        assert!(factions.neutral_to_all(1));
        assert!(!factions.neutral_to_all(2));
        assert!(factions.neutral_to_all(3));
        assert!(!factions.neutral_to_all(4));
        assert!(factions.neutral_to_all(5));
    }
    #[test]
    fn raw_template_operands_use_effective_entry_and_source_extra_flag_bits() {
        // Creature.h:50,59,640-641; Creature.cpp:2536-2548 reads GetEntry().
        let row = CreatureAiTemplateOperands {
            ordinal: 0,
            entry: 913,
            faction: 7,
            extra_flags: 0x402,
            unit_flags: 0,
            type_flags: 0,
            static_flags: [0; 4],
            rank: 0,
            inhabit_type: 1,
            stats_multipliers: [0x3f80_0000; 3],
            civilian: false,
            pursuit_ms: 15000,
            call_for_help: 0,
            creature_type: 0,
            ai_name: "TotemAI".into(),
            script_name: "authored_script".into(),
        };
        let input = row.inputs(CreatureAiControl::default(), true);
        assert_eq!(input.effective_entry, 913);
        assert!(input.guard && input.no_aggro_on_sight && input.neutral_to_all);
        let mut objects = SourceObjects::default();
        let creature = objects.construct(1, SourceObjectKind::Creature).unwrap();
        // No registry is a missing owner, not an absent script and false TotemAI success.
        assert_eq!(
            SourceAiRegistry::builtin()
                .select(&objects, creature, input)
                .unwrap_err()
                .id,
            "execution.ai.scriptdev_registry_owner"
        );
    }
}
