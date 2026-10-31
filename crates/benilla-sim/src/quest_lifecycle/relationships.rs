// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, ObjectMgr.cpp:4790-4876.
use super::{Catalog, DependencyAudit, Error, QuestRelations, QuestRow};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_RELATIONSHIP_QUESTS: usize = 1024;

/// Raw selected projection and full-table incoming scan, not a gameplay admission token.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipCapture {
    pub roots: Vec<u32>,
    pub own: Vec<RelationshipRow>,
    /// None means the full-table incoming query was not captured.
    pub inbound: Option<Vec<RelationshipRow>>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipRow {
    pub entry: u32,
    #[serde(rename = "PrevQuestId")]
    pub previous: i32,
    #[serde(rename = "NextQuestId")]
    pub next: i32,
    #[serde(rename = "NextQuestInChain")]
    pub chain: u32,
    #[serde(rename = "ExclusiveGroup")]
    pub exclusive: i32,
    #[serde(rename = "BreadcrumbForQuestId")]
    pub breadcrumb: u32,
}
impl From<&QuestRow> for RelationshipRow {
    fn from(row: &QuestRow) -> Self {
        Self {
            entry: row.id,
            previous: row.previous_quest,
            next: row.next_quest,
            chain: row.next_in_chain,
            exclusive: row.excluded.exclusive_group,
            breadcrumb: row.excluded.breadcrumb_for_quest,
        }
    }
}
fn refusal(quest: u32, field: &'static str, target: i64, reason: &'static str) -> Error {
    Error::Relationship {
        quest,
        field,
        target,
        reason,
    }
}
impl Catalog {
    /// Requires every reachable quest to be selected with its full row and incoming coverage.
    pub fn from_relationship_capture(
        rows: Vec<QuestRow>,
        relations: QuestRelations,
        mut audit: DependencyAudit,
        capture: &RelationshipCapture,
    ) -> Result<Self, Error> {
        if rows.len() > MAX_RELATIONSHIP_QUESTS
            || capture.own.len() > MAX_RELATIONSHIP_QUESTS
            || capture.roots.len() > MAX_RELATIONSHIP_QUESTS
        {
            return Err(refusal(0, "closure", rows.len() as i64, "closure_limit"));
        }
        let inbound = capture
            .inbound
            .as_ref()
            .ok_or_else(|| refusal(0, "inbound", 0, "unknown_coverage"))?;
        if inbound.len() > MAX_RELATIONSHIP_QUESTS {
            return Err(refusal(0, "inbound", inbound.len() as i64, "closure_limit"));
        }
        let mut own = BTreeMap::new();
        for row in &capture.own {
            if own.insert(row.entry, row).is_some() {
                return Err(refusal(
                    row.entry,
                    "entry",
                    i64::from(row.entry),
                    "conflicting_rows",
                ));
            }
        }
        if capture.roots.iter().copied().ne(own.keys().copied()) || rows.len() != own.len() {
            return Err(refusal(0, "roots", 0, "coverage_mismatch"));
        }
        let mut selected = BTreeSet::new();
        for row in &rows {
            if !selected.insert(row.id)
                || own.get(&row.id).copied() != Some(&RelationshipRow::from(row))
            {
                return Err(refusal(
                    row.id,
                    "entry",
                    i64::from(row.id),
                    "conflicting_rows",
                ));
            }
            for (field, value) in [
                ("RequiredCondition", row.excluded.required_condition),
                ("StartScript", row.excluded.start_script),
                ("CompleteScript", row.excluded.complete_script),
                ("SrcItemId", row.excluded.source_item),
                ("SrcItemCount", row.excluded.source_item_count),
                ("SrcSpell", row.excluded.source_spell),
            ] {
                if value != 0 {
                    return Err(refusal(row.id, field, i64::from(value), "unsupported_path"));
                }
            }
            for slot in 0..super::OBJECTIVES {
                for (field, value) in [
                    (
                        [
                            "ReqSourceId1",
                            "ReqSourceId2",
                            "ReqSourceId3",
                            "ReqSourceId4",
                        ][slot],
                        row.excluded.required_source[slot],
                    ),
                    (
                        [
                            "ReqSourceCount1",
                            "ReqSourceCount2",
                            "ReqSourceCount3",
                            "ReqSourceCount4",
                        ][slot],
                        row.excluded.required_source_count[slot],
                    ),
                    (
                        [
                            "ReqSpellCast1",
                            "ReqSpellCast2",
                            "ReqSpellCast3",
                            "ReqSpellCast4",
                        ][slot],
                        row.excluded.required_spell[slot],
                    ),
                ] {
                    if value != 0 {
                        return Err(refusal(row.id, field, i64::from(value), "unsupported_path"));
                    }
                }
            }
        }
        let matches = |row: &RelationshipRow| {
            own.contains_key(&row.next.unsigned_abs())
                || own.contains_key(&row.chain)
                || own.contains_key(&row.breadcrumb)
        };
        let mut incoming = BTreeMap::new();
        for row in inbound {
            if !matches(row) {
                return Err(refusal(row.entry, "inbound", 0, "unselected_edge"));
            }
            if incoming.insert(row.entry, row).is_some() {
                return Err(refusal(
                    row.entry,
                    "entry",
                    i64::from(row.entry),
                    "conflicting_rows",
                ));
            }
            if own.contains_key(&row.entry) && own.get(&row.entry).copied() != Some(row) {
                return Err(refusal(
                    row.entry,
                    "entry",
                    i64::from(row.entry),
                    "conflicting_rows",
                ));
            }
            if own.get(&row.entry).copied() != Some(row) {
                for (field, target) in [
                    ("NextQuestId", i64::from(row.next)),
                    ("NextQuestInChain", i64::from(row.chain)),
                    ("BreadcrumbForQuestId", i64::from(row.breadcrumb)),
                ] {
                    if own.contains_key(&(target.unsigned_abs() as u32)) {
                        return Err(refusal(
                            row.entry,
                            field,
                            target,
                            "uncaptured_incoming_quest",
                        ));
                    }
                }
                return Err(refusal(
                    row.entry,
                    "entry",
                    i64::from(row.entry),
                    "conflicting_rows",
                ));
            }
        }
        for row in own.values() {
            if row.entry == 0 || row.entry > i32::MAX as u32 {
                return Err(refusal(
                    row.entry,
                    "entry",
                    i64::from(row.entry),
                    "numeric_domain",
                ));
            }
            for (field, target) in [
                ("PrevQuestId", i64::from(row.previous)),
                ("NextQuestId", i64::from(row.next)),
                ("NextQuestInChain", i64::from(row.chain)),
                ("ExclusiveGroup", i64::from(row.exclusive)),
                ("BreadcrumbForQuestId", i64::from(row.breadcrumb)),
            ] {
                if target < -i64::from(i32::MAX) || target > i64::from(i32::MAX) {
                    return Err(refusal(row.entry, field, target, "numeric_domain"));
                }
                if target != 0 {
                    if field == "ExclusiveGroup" || field == "BreadcrumbForQuestId" {
                        return Err(refusal(row.entry, field, target, "unsupported_path"));
                    }
                    // ObjectMgr.cpp:4793-4825 clears/ignores missing edges; bounded capture refuses instead.
                    if !own.contains_key(&(target.unsigned_abs() as u32)) {
                        return Err(refusal(row.entry, field, target, "uncaptured_reference"));
                    }
                }
            }
            if matches(row) && incoming.get(&row.entry).copied() != Some(*row) {
                let (field, target) = [
                    ("NextQuestId", i64::from(row.next)),
                    ("NextQuestInChain", i64::from(row.chain)),
                    ("BreadcrumbForQuestId", i64::from(row.breadcrumb)),
                ]
                .into_iter()
                .find(|(_, target)| own.contains_key(&(target.unsigned_abs() as u32)))
                .expect("matches checks these exact edges");
                return Err(refusal(row.entry, field, target, "missing_selected_edge"));
            }
        }
        // ObjectMgr.cpp:4804-4825 preserves signs; 4790-4800 builds a separate reverse-chain store.
        audit.complete_relation_scan = true;
        Self::new(rows, relations, audit)
    }
}
