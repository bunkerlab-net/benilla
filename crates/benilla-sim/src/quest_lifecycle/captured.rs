// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, ObjectMgr.cpp:4231-4880,7940-8035; ScriptDevAIMgr.cpp:164-174,238-272,318-328.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors; Rust GPL-3.0-or-later, without warranty.
//! Pure ordinary loader adapter. Complete projections are not production admission.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedQuestGiver {
    pub spawn: u64,
    pub database_guid: u32,
    pub generation: u32,
    pub entry: u32,
    pub npc_flags: u32,
    pub unit_flags: u32,
    pub script_name: String,
    pub loaded_script: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestCallback {
    GossipHello,
    DialogStatus,
    Accept,
    Reward,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedQuestScript {
    pub id: u32,
    pub name: String,
    pub callbacks: Vec<QuestCallback>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedGossipScope {
    pub entry: u32,
    pub menu: u32,
    /// All applicable default-menu option IDs, including conditions/scripts.
    pub options: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestSpellAreaSource {
    pub spell: u32,
    pub area: u32,
    pub quest_start: u32,
    pub quest_end: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestItemDeletionSource {
    pub item: u32,
    pub spell: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestDependencySource {
    pub source: [u8; 32],
    pub mechanics: Option<Mechanics>,
    /// Complete applicable loaded registry, not a list selected by blank template names.
    pub scripts: Option<Vec<CapturedQuestScript>>,
    pub gossip: Option<Vec<CapturedGossipScope>>,
    pub allowed_areas: Vec<u32>,
    /// Complete global SpellArea table, including location-only and quest-end rows.
    pub global_spell_areas: Option<Vec<QuestSpellAreaSource>>,
    pub item_keys: Vec<u32>,
    pub item_deletion_effects: Option<Vec<QuestItemDeletionSource>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedQuestDependencies {
    source: [u8; 32],
    givers: Vec<CapturedQuestGiver>,
    item_keys: Vec<u32>,
    mechanics: Mechanics,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedOrdinaryQuests {
    pub source: [u8; 32],
    /// Raw special flags are retained here; ordinary loader flags are derived during compile.
    pub quests: Option<Vec<QuestRow>>,
    /// Complete union of queries by selected entry and selected quest.
    pub starters: Option<Vec<CreatureQuestRelation>>,
    pub finishers: Option<Vec<CreatureQuestRelation>>,
    pub givers: Option<Vec<CapturedQuestGiver>>,
}
#[derive(Clone, Debug)]
pub struct CompiledOrdinaryQuests {
    source: [u8; 32],
    catalog: Catalog,
    quests: Vec<QuestRow>,
    relations: QuestRelations,
    audit: DependencyAudit,
    relationships: RelationshipCapture,
    givers: Vec<CapturedQuestGiver>,
}
fn fail(quest: u32, field: &'static str, target: i64, reason: &'static str) -> Error {
    Error::Relationship {
        quest,
        field,
        target,
        reason,
    }
}
fn require<'a, T>(
    rows: &'a Option<Vec<T>>,
    field: &'static str,
    max: usize,
) -> Result<&'a [T], Error> {
    let rows = rows
        .as_deref()
        .ok_or(fail(0, field, 0, "unknown_coverage"))?;
    if rows.len() > max {
        return Err(fail(0, field, rows.len() as i64, "row_limit"));
    }
    Ok(rows)
}
fn item_keys(rows: &[QuestRow]) -> Vec<u32> {
    rows.iter()
        .flat_map(|q| q.required_item.into_iter().chain([q.excluded.source_item]))
        .filter(|id| *id != 0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
impl QuestDependencySource {
    pub fn resolve(
        &self,
        capture: &CapturedOrdinaryQuests,
    ) -> Result<ResolvedQuestDependencies, Error> {
        if self.source != capture.source {
            return Err(fail(0, "source", 0, "binding_mismatch"));
        }
        let quests = require(&capture.quests, "quest_template", MAX_RELATIONSHIP_QUESTS)?;
        let givers = require(&capture.givers, "creature", 4096)?;
        let scripts = require(&self.scripts, "script_registry", 4096)?;
        let gossip = require(&self.gossip, "gossip_menu", 4096)?;
        let areas = require(&self.global_spell_areas, "global_spell_area", 16384)?;
        let effects = require(&self.item_deletion_effects, "item_deletion_effects", 4096)?;
        if self.allowed_areas.is_empty()
            || self.allowed_areas.len() > 4096
            || self.allowed_areas.iter().collect::<BTreeSet<_>>().len() != self.allowed_areas.len()
        {
            return Err(fail(0, "allowed_areas", 0, "coverage_mismatch"));
        }
        // The bounded domain cannot execute SpellArea or item-deletion callbacks, even when dormant.
        if !areas.is_empty() {
            return Err(fail(
                0,
                "global_spell_area",
                i64::from(areas[0].spell),
                "unsupported_path",
            ));
        }
        if !effects.is_empty() {
            return Err(fail(
                0,
                "item_deletion_effects",
                i64::from(effects[0].item),
                "unsupported_path",
            ));
        }
        if self.item_keys != item_keys(quests) {
            return Err(fail(0, "item_keys", 0, "coverage_mismatch"));
        }
        let mut registry = BTreeMap::new();
        for script in scripts {
            if script.id == 0
                || script.name.is_empty()
                || script.name.len() > 255
                || script.name.chars().any(char::is_control)
                || script.callbacks.len() > 4
                || registry.insert(script.id, script).is_some()
            {
                return Err(fail(
                    0,
                    "script_registry",
                    i64::from(script.id),
                    "conflicting_rows",
                ));
            }
        }
        let mut menus = BTreeMap::new();
        for row in gossip {
            if row.options.len() > 64 || menus.insert(row.entry, row).is_some() {
                return Err(fail(
                    0,
                    "gossip_menu",
                    i64::from(row.entry),
                    "conflicting_rows",
                ));
            }
            if (row.menu != 0 || !row.options.is_empty())
                && self
                    .mechanics
                    .as_ref()
                    .and_then(|m| m.giver(row.entry))
                    .map(|(menu, _)| menu)
                    != Some(row.menu)
            {
                return Err(fail(
                    0,
                    "gossip_menu",
                    i64::from(row.menu),
                    "unsupported_path",
                ));
            }
        }
        let mut spawns = BTreeSet::new();
        let mut guids = BTreeSet::new();
        for giver in givers {
            if giver.spawn == 0
                || giver.database_guid == 0
                || giver.entry == 0
                || giver.entry > 0xffffff
                || !spawns.insert(giver.spawn)
                || !guids.insert(giver.database_guid)
                || giver.npc_flags & 2 == 0
                || giver.script_name.len() > 255
                || giver.script_name.chars().any(char::is_control)
            {
                return Err(fail(0, "creature", i64::from(giver.entry), "invalid_giver"));
            }
            if giver.loaded_script == 0 {
                if !giver.script_name.is_empty() {
                    return Err(fail(
                        0,
                        "ScriptName",
                        i64::from(giver.entry),
                        "unresolved_selection",
                    ));
                }
            } else {
                let script = registry.get(&giver.loaded_script).ok_or(fail(
                    0,
                    "script_registry",
                    i64::from(giver.loaded_script),
                    "unknown_lookup",
                ))?;
                if script.name != giver.script_name {
                    return Err(fail(
                        0,
                        "ScriptName",
                        i64::from(giver.entry),
                        "binding_mismatch",
                    ));
                }
                if let Some(callback) = script.callbacks.first() {
                    let field = match callback {
                        QuestCallback::GossipHello => "OnGossipHello",
                        QuestCallback::DialogStatus => "GetDialogStatus",
                        QuestCallback::Accept => "OnQuestAccept",
                        QuestCallback::Reward => "OnQuestRewarded",
                    };
                    return Err(fail(0, field, i64::from(giver.entry), "unsupported_path"));
                }
            }
            if !menus.contains_key(&giver.entry) {
                return Err(fail(
                    0,
                    "gossip_menu",
                    i64::from(giver.entry),
                    "unknown_lookup",
                ));
            }
        }
        let mechanics = self.mechanics.clone().unwrap_or_default();
        for giver in givers {
            if self.mechanics.is_some() && mechanics.giver(giver.entry).is_none() {
                return Err(fail(
                    0,
                    "gossip_giver",
                    i64::from(giver.entry),
                    "unknown_lookup",
                ));
            }
            if let Some((_, flags)) = mechanics.giver(giver.entry) {
                if flags != giver.npc_flags {
                    return Err(fail(
                        0,
                        "gossip_giver.NpcFlags",
                        i64::from(giver.entry),
                        "binding_mismatch",
                    ));
                }
                let scope = menus.get(&giver.entry).ok_or(fail(
                    0,
                    "gossip_menu",
                    i64::from(giver.entry),
                    "unknown_lookup",
                ))?;
                let mut expected = mechanics.effective_option_ids(giver.entry);
                let mut observed = scope.options.clone();
                expected.sort_unstable();
                observed.sort_unstable();
                if expected != observed {
                    return Err(fail(
                        0,
                        "gossip_menu_option",
                        i64::from(giver.entry),
                        "coverage_mismatch",
                    ));
                }
            }
        }
        if menus.keys().copied().collect::<BTreeSet<_>>()
            != givers.iter().map(|g| g.entry).collect()
        {
            return Err(fail(0, "gossip_menu", 0, "coverage_mismatch"));
        }
        Ok(ResolvedQuestDependencies {
            source: self.source,
            givers: givers.to_vec(),
            item_keys: self.item_keys.clone(),
            mechanics,
        })
    }
}
impl CapturedOrdinaryQuests {
    pub fn compile(
        &self,
        relationships: &RelationshipCapture,
        context: &ResolvedQuestDependencies,
    ) -> Result<CompiledOrdinaryQuests, Error> {
        let rows = require(&self.quests, "quest_template", MAX_RELATIONSHIP_QUESTS)?;
        let givers = require(&self.givers, "creature", 4096)?;
        if self.source != context.source
            || givers != context.givers
            || item_keys(rows) != context.item_keys
        {
            return Err(fail(0, "dependencies", 0, "binding_mismatch"));
        }
        let mut entries: BTreeSet<_> = givers.iter().map(|giver| giver.entry).collect();
        for endpoint in &relationships.outside_endpoints {
            if !entries.insert(endpoint.creature)
                || endpoint
                    .spawn_guids
                    .iter()
                    .any(|s| givers.iter().any(|g| g.spawn == *s))
            {
                return Err(fail(
                    0,
                    "outside_endpoints",
                    i64::from(endpoint.creature),
                    "selection_overlap",
                ));
            }
        }
        let ids: BTreeSet<_> = rows.iter().map(|q| q.id).collect();
        let load = |raw: &Option<Vec<CreatureQuestRelation>>, field| {
            let raw = require(raw, field, 8192)?;
            let mut canonical = raw.to_vec();
            canonical.sort_by_key(crate::canonical_order::relation);
            if canonical.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(fail(0, field, 0, "order_coverage_mismatch"));
            }
            for relation in raw {
                if !entries.contains(&relation.creature) || !ids.contains(&relation.quest) {
                    return Err(fail(
                        relation.quest,
                        field,
                        i64::from(relation.creature),
                        "unbound_giver_or_quest",
                    ));
                }
            }
            for id in &ids {
                if !raw.iter().any(|relation| relation.quest == *id) {
                    return Err(fail(*id, field, 0, "missing_spawned_relation"));
                }
            }
            Ok(canonical)
        };
        let relations = QuestRelations {
            starters: load(&self.starters, "creature_questrelation")?,
            finishers: load(&self.finishers, "creature_involvedrelation")?,
        };
        let mut quests = rows.to_vec();
        for row in &mut quests {
            // ObjectMgr::LoadQuests:4513,4618 derives delivery and creature kill/speak bits.
            // ObjectMgr.cpp:4463-4482 source-item count normalization.
            if row.excluded.source_item == 0 {
                row.excluded.source_item_count = 0;
            } else if row.excluded.source_item_count == 0 {
                row.excluded.source_item_count = 1;
            }
            for slot in 0..OBJECTIVES {
                if row.required_item[slot] != 0 {
                    row.special_flags |= DELIVER;
                }
                if row.required_creature[slot] != 0 {
                    row.special_flags |= KILL | SPEAK;
                }
            }
        }
        let audit = DependencyAudit {
            complete_relation_scan: false,
            questgiver_hooks: Evidence::Absent,
            spell_area: Evidence::Absent,
            collection_item_effects: Evidence::Absent,
        };
        let catalog = Catalog::from_relationship_capture_with_mechanics(
            quests.clone(),
            relations.clone(),
            audit.clone(),
            relationships,
            context.mechanics.clone(),
        )?;
        Ok(CompiledOrdinaryQuests {
            source: self.source,
            catalog,
            quests,
            relations,
            audit,
            relationships: relationships.clone(),
            givers: givers.to_vec(),
        })
    }
}
impl CompiledOrdinaryQuests {
    pub fn source(&self) -> [u8; 32] {
        self.source
    }
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn quests(&self) -> &[QuestRow] {
        &self.quests
    }
    pub fn relations(&self) -> &QuestRelations {
        &self.relations
    }
    pub fn givers(&self) -> &[CapturedQuestGiver] {
        &self.givers
    }
    pub fn audit(&self) -> &DependencyAudit {
        &self.audit
    }
    pub fn relationships(&self) -> &RelationshipCapture {
        &self.relationships
    }
}
