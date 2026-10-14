// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; GPL-2.0-or-later upstream,
// GPL-3.0-or-later Rust translation, without warranty. See ../../PROVENANCE.md.
use super::{Error, POLICY_ID};
use crate::quest::{QuestEligibilityInput, QuestRewardInput};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const LOG_SIZE: usize = 20;
pub const OBJECTIVES: usize = 4;
pub const MAX_MONEY: u32 = 0x7fff_fffe;
pub const DELIVER: u16 = 0x008;
pub const KILL: u16 = 0x020;
/// ObjectMgr sets SPEAKTO together with KILL for every creature objective.
pub const SPEAK: u16 = 0x010;

/// No defaults: the compiler must inspect complete rows and dependency scans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    Absent,
    Present,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyAudit {
    /// A full quest relation scan, including inbound NextQuestId/chain references.
    pub complete_relation_scan: bool,
    /// Includes all selected giver/finisher ScriptDevAI and database scripts.
    pub questgiver_hooks: Evidence,
    /// Includes zone, area and global SpellArea records for every permitted location.
    pub spell_area: Evidence,
    /// Required item templates and deletion effects are resolved and inert.
    pub collection_item_effects: Evidence,
}

/// Gameplay columns outside the supported domain, retained to reject, never defaulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExcludedFields {
    pub required_skill: u32,
    pub required_skill_value: u32,
    pub required_condition: u32,
    pub rep_objective_faction: u32,
    pub rep_objective_value: i32,
    pub required_min_rep_faction: u32,
    pub required_min_rep_value: i32,
    pub required_max_rep_faction: u32,
    pub required_max_rep_value: i32,
    pub quest_type: u32,
    pub suggested_players: u32,
    pub limit_time: u32,
    pub exclusive_group: i32,
    pub breadcrumb_for_quest: u32,
    pub source_item: u32,
    pub source_item_count: u32,
    pub source_spell: u32,
    pub required_source: [u32; OBJECTIVES],
    pub required_source_count: [u32; OBJECTIVES],
    pub required_spell: [u32; OBJECTIVES],
    pub reward_choice_item: [u32; 6],
    pub reward_choice_count: [u32; 6],
    pub reward_item: [u32; OBJECTIVES],
    pub reward_item_count: [u32; OBJECTIVES],
    pub reward_rep_faction: [u32; 5],
    pub reward_rep_value: [i32; 5],
    pub reward_faction_flags: u32,
    pub reward_spell: u32,
    pub reward_spell_cast: u32,
    pub reward_mail_template: u32,
    pub reward_mail_delay: u32,
    pub start_script: u32,
    pub complete_script: u32,
}
impl ExcludedFields {
    fn check(&self) -> Result<(), Error> {
        macro_rules! zeros { ($($field:ident),+ $(,)?) => {$(if self.$field != 0 { return Err(Error::Unsupported(stringify!($field))); })+}; }
        zeros!(
            required_skill,
            required_skill_value,
            required_condition,
            rep_objective_faction,
            rep_objective_value,
            required_min_rep_faction,
            required_min_rep_value,
            required_max_rep_faction,
            required_max_rep_value,
            quest_type,
            suggested_players,
            limit_time,
            exclusive_group,
            breadcrumb_for_quest,
            source_item,
            source_item_count,
            source_spell,
            reward_faction_flags,
            reward_spell,
            reward_spell_cast,
            reward_mail_template,
            reward_mail_delay,
            start_script,
            complete_script
        );
        macro_rules! arrays { ($($field:ident),+ $(,)?) => {$(if self.$field.iter().any(|&v| v != 0) { return Err(Error::Unsupported(stringify!($field))); })+}; }
        arrays!(
            required_source,
            required_source_count,
            required_spell,
            reward_rep_faction,
            reward_rep_value
        );
        for (items, counts) in [
            (&self.reward_choice_item[..], &self.reward_choice_count[..]),
            (&self.reward_item[..], &self.reward_item_count[..]),
        ] {
            let mut ended = false;
            for (&item, &count) in items.iter().zip(counts) {
                if (item == 0) != (count == 0) || count > 10000 {
                    return Err(Error::Invalid("reward_item_pair"));
                }
                if item == 0 {
                    ended = true;
                } else if ended {
                    return Err(Error::Unsupported("sparse_reward_items"));
                }
            }
        }
        Ok(())
    }
}

/// Loader-resolved gameplay fields. No real world rows or quest text are bundled.
/// Prev/Next relationships are reconstructed together, not guessed from one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestRow {
    pub id: u32,
    pub method: u32,
    pub active: bool,
    pub eligibility: QuestEligibilityInput,
    pub reward: QuestRewardInput,
    pub special_flags: u16,
    pub previous_quest: i32,
    pub next_quest: i32,
    pub next_in_chain: u32,
    pub required_item: [u32; OBJECTIVES],
    pub required_item_count: [u32; OBJECTIVES],
    pub required_creature: [i32; OBJECTIVES],
    pub required_creature_count: [u32; OBJECTIVES],
    pub excluded: ExcludedFields,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CreatureQuestRelation {
    pub creature: u32,
    pub quest: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestRelations {
    pub starters: Vec<CreatureQuestRelation>,
    pub finishers: Vec<CreatureQuestRelation>,
}
#[derive(Clone, Debug)]
pub struct Catalog {
    pub(super) rows: BTreeMap<u32, QuestRow>,
    pub(super) previous: BTreeMap<u32, Vec<i32>>,
    pub(super) previous_chain: BTreeMap<u32, Vec<u32>>,
    pub(super) relations: QuestRelations,
    hash: [u8; 32],
}
impl Catalog {
    pub fn new(
        rows: Vec<QuestRow>,
        mut relations: QuestRelations,
        audit: DependencyAudit,
    ) -> Result<Self, Error> {
        if !audit.complete_relation_scan {
            return Err(Error::Unsupported("incomplete_relation_scan"));
        }
        for (value, name) in [
            (audit.questgiver_hooks, "questgiver_hooks"),
            (audit.spell_area, "spell_area"),
            (audit.collection_item_effects, "collection_item_effects"),
        ] {
            if value != Evidence::Absent {
                return Err(Error::Unsupported(name));
            }
        }
        let mut map = BTreeMap::new();
        for q in rows {
            if q.id == 0 || q.id > i32::MAX as u32 {
                return Err(Error::Invalid("quest_id"));
            }
            if q.method != 2 {
                return Err(Error::Unsupported("quest_method"));
            }
            q.excluded.check()?;
            // Sharable is accepted only on this solo, creature-giver path. Sharing is unsupported.
            if q.reward.quest_flags & !(0x0008 | 0x0100 | 0x0200) != 0 {
                return Err(Error::Unsupported("quest_flags"));
            }
            if q.special_flags & !(DELIVER | KILL | SPEAK) != 0 {
                return Err(Error::Unsupported("special_flags"));
            }
            if q.reward.xp.quest_level > 255 || q.reward.reward_or_required_money == i32::MIN {
                return Err(Error::Invalid("reward"));
            }
            if q.eligibility.minimum_level > q.eligibility.maximum_level
                || q.eligibility.maximum_level > 255
            {
                return Err(Error::Invalid("level_range"));
            }
            if q.eligibility.required_classes & !0x05df != 0
                || q.eligibility.required_races & !0xff != 0
            {
                return Err(Error::Invalid("identity_masks"));
            }
            let mut flags = 0;
            let mut items = BTreeSet::new();
            for i in 0..OBJECTIVES {
                if (q.required_item[i] == 0) != (q.required_item_count[i] == 0)
                    || q.required_item_count[i] >= 64
                {
                    return Err(Error::Invalid("item_objective"));
                }
                if q.required_item[i] != 0 {
                    flags |= DELIVER;
                    // Native removal behavior for duplicate objective entries needs separate auditing.
                    if !items.insert(q.required_item[i]) {
                        return Err(Error::Unsupported("duplicate_item_objective"));
                    }
                }
                if q.required_creature[i] < 0 {
                    return Err(Error::Unsupported("gameobject_objective"));
                }
                if (q.required_creature[i] == 0) != (q.required_creature_count[i] == 0)
                    || q.required_creature_count[i] > 63
                {
                    return Err(Error::Invalid("creature_objective"));
                }
                if q.required_creature[i] > 0 {
                    flags |= KILL | SPEAK;
                }
            }
            if flags != q.special_flags {
                return Err(Error::Invalid("loader_special_flags"));
            }
            if map.insert(q.id, q).is_some() {
                return Err(Error::Invalid("duplicate_quest"));
            }
        }
        let mut previous: BTreeMap<u32, Vec<i32>> = BTreeMap::new();
        let mut previous_chain: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for q in map.values() {
            for value in [q.previous_quest, q.next_quest] {
                if value == i32::MIN || (value != 0 && !map.contains_key(&value.unsigned_abs())) {
                    return Err(Error::Invalid("missing_quest_relation"));
                }
            }
            if q.previous_quest != 0 {
                previous.entry(q.id).or_default().push(q.previous_quest);
            }
            if q.next_quest != 0 {
                previous
                    .entry(q.next_quest.unsigned_abs())
                    .or_default()
                    .push(if q.next_quest < 0 {
                        -(q.id as i32)
                    } else {
                        q.id as i32
                    });
            }
            if q.next_in_chain != 0 {
                if !map.contains_key(&q.next_in_chain) {
                    return Err(Error::Invalid("missing_chain_quest"));
                }
                previous_chain
                    .entry(q.next_in_chain)
                    .or_default()
                    .push(q.id);
            }
        }
        for list in [&mut relations.starters, &mut relations.finishers] {
            list.sort();
            if list.windows(2).any(|p| p[0] == p[1]) {
                return Err(Error::Invalid("duplicate_giver_relation"));
            }
            for relation in list.iter() {
                if relation.creature == 0 || !map.contains_key(&relation.quest) {
                    return Err(Error::Invalid("giver_relation"));
                }
            }
        }
        // Versioned canonical Debug of sorted integer-only schema; no floats or hash-map ordering.
        let hash =
            Sha256::digest(format!("{POLICY_ID}:{map:?}:{relations:?}:{audit:?}").as_bytes())
                .into();
        Ok(Self {
            rows: map,
            previous,
            previous_chain,
            relations,
            hash,
        })
    }
    pub fn row(&self, id: u32) -> Result<&QuestRow, Error> {
        self.rows.get(&id).ok_or(Error::Invalid("unknown_quest"))
    }
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.rows.keys().copied()
    }
    pub fn relations(&self) -> &QuestRelations {
        &self.relations
    }
    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Status {
    Incomplete,
    Complete,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub status: Status,
    pub rewarded: bool,
    pub items: [u32; OBJECTIVES],
    pub creatures: [u32; OBJECTIVES],
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub policy: String,
    pub catalog_hash: [u8; 32],
    pub next_sequence: u64,
    pub log: [Option<u32>; LOG_SIZE],
    pub records: BTreeMap<u32, Progress>,
}
/// Immutable callback-time view. May represent a transient in-transaction state;
/// deliberately not a restorable checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestView {
    pub log: [Option<u32>; LOG_SIZE],
    pub records: BTreeMap<u32, Progress>,
}
/// State is private and can be reconstructed only through validated snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub(super) snapshot: Snapshot,
}
impl State {
    pub fn new(catalog: &Catalog) -> Self {
        Self {
            snapshot: Snapshot {
                policy: POLICY_ID.into(),
                catalog_hash: catalog.hash,
                next_sequence: 0,
                log: [None; LOG_SIZE],
                records: BTreeMap::new(),
            },
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.clone()
    }
    pub fn progress(&self, id: u32) -> Option<&Progress> {
        self.snapshot.records.get(&id)
    }
    pub fn view(&self) -> QuestView {
        QuestView {
            log: self.snapshot.log,
            records: self.snapshot.records.clone(),
        }
    }
    pub fn next_sequence(&self) -> u64 {
        self.snapshot.next_sequence
    }
    pub fn restore(catalog: &Catalog, snapshot: Snapshot) -> Result<Self, Error> {
        if snapshot.policy != POLICY_ID || snapshot.catalog_hash != catalog.hash {
            return Err(Error::Invalid("snapshot_rules"));
        }
        let mut logged = BTreeSet::new();
        for &id in snapshot.log.iter().flatten() {
            if !logged.insert(id) {
                return Err(Error::Invalid("duplicate_log_slot"));
            }
            if snapshot.records.get(&id).is_none_or(|p| p.rewarded) {
                return Err(Error::Invalid("log_record"));
            }
        }
        for (&id, p) in &snapshot.records {
            let q = catalog.row(id)?;
            if p.rewarded != !logged.contains(&id) || (p.rewarded && p.status != Status::Complete) {
                return Err(Error::Invalid("rewarded_history"));
            }
            for i in 0..OBJECTIVES {
                // ItemRemovedQuestCheck can leave a present objective ABOVE its
                // requirement (e.g. carried 4, required 2, remove 3 => counter 3).
                // Preserve its uint32 domain; absent slots must still remain zero.
                if (q.required_item[i] == 0 && p.items[i] != 0)
                    || p.creatures[i] > q.required_creature_count[i]
                {
                    return Err(Error::Invalid("objective_progress"));
                }
                // Reward consumes items but never resets creature credit.
                if p.status == Status::Complete
                    && ((!p.rewarded && p.items[i] < q.required_item_count[i])
                        || p.creatures[i] < q.required_creature_count[i])
                {
                    return Err(Error::Invalid("completed_objectives"));
                }
            }
        }
        Ok(Self { snapshot })
    }
}
