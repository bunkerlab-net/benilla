// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{model::*, Error, Limits};
use std::collections::BTreeSet;

const MAX_STAT: u32 = 1_000_000_000;
const MAX_INTERVAL_MS: u32 = 86_400_000;
const MAX_COORD: i32 = 100_000_000;
const MAX_RANGE: u32 = 100_000;

fn require(ok: bool, field: &'static str, row: u64) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid { field, row })
    }
}
fn text(value: &str, max: usize, multiline: bool) -> bool {
    value.len() <= max
        && !value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
}
fn label(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && text(value, max.min(256), false)
}
fn keys(values: impl Iterator<Item = u64>, table: &'static str) -> Result<BTreeSet<u64>, Error> {
    let mut keys = BTreeSet::new();
    for row in values {
        require(row != 0, table, row)?;
        if !keys.insert(row) {
            return Err(Error::Duplicate { table, row });
        }
    }
    Ok(keys)
}
fn reference(keys: &BTreeSet<u64>, id: u64, field: &'static str, row: u64) -> Result<(), Error> {
    if keys.contains(&id) {
        Ok(())
    } else {
        Err(Error::MissingReference { field, row })
    }
}

pub(crate) fn validate(data: &WorldExport, limits: Limits) -> Result<(), Error> {
    require(data.format_version == FORMAT_VERSION, "format_version", 0)?;
    require(data.profile == PROFILE, "profile", 0)?;
    match &data.source {
        Source::OriginalFixture { revision } => require(
            label(revision, limits.max_string_bytes),
            "source.revision",
            0,
        )?,
        Source::LocalCmangosClassic {
            core_revision,
            database_revision,
            client_build,
            schema_sha256,
        } => {
            require(
                core_revision == REVIEWED_CORE_REVISION,
                "source.core_revision",
                0,
            )?;
            require(
                database_revision == REVIEWED_DATABASE_REVISION,
                "source.database_revision",
                0,
            )?;
            require(
                *client_build == CLASSIC_CLIENT_BUILD,
                "source.client_build",
                0,
            )?;
            require(
                schema_sha256.len() == 64
                    && schema_sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "source.schema_sha256",
                0,
            )?;
        }
    }
    if !data.unsupported_features.is_empty() {
        return Err(Error::UnsupportedFeatures);
    }
    let count = data.creatures.len()
        + data.spawns.len()
        + data.quests.len()
        + data.quest_relations.len()
        + data.items.len()
        + data.abilities.len();
    if count > limits.max_records {
        return Err(Error::TooManyRecords);
    }
    let creatures = keys(
        data.creatures.iter().map(|x| u64::from(x.entry)),
        "creatures",
    )?;
    keys(data.spawns.iter().map(|x| x.spawn_id), "spawns")?;
    let quests = keys(data.quests.iter().map(|x| u64::from(x.entry)), "quests")?;
    let items = keys(data.items.iter().map(|x| u64::from(x.entry)), "items")?;
    keys(
        data.abilities.iter().map(|x| u64::from(x.entry)),
        "abilities",
    )?;
    for c in &data.creatures {
        let id = u64::from(c.entry);
        require(
            label(&c.name, limits.max_string_bytes),
            "creatures.name",
            id,
        )?;
        require((1..=63).contains(&c.level), "creatures.level", id)?;
        require(
            (1..=MAX_STAT).contains(&c.max_health),
            "creatures.max_health",
            id,
        )?;
        require(c.armor <= MAX_STAT, "creatures.armor", id)?;
        require(c.melee_damage <= MAX_STAT, "creatures.melee_damage", id)?;
        require(
            (1..=MAX_INTERVAL_MS).contains(&c.melee_period_ms),
            "creatures.melee_period_ms",
            id,
        )?;
        require(
            (1..=MAX_RANGE).contains(&c.combat_reach_milliunits),
            "creatures.combat_reach_milliunits",
            id,
        )?;
        require(
            c.behavior != CreatureBehavior::StationaryMelee || c.melee_damage > 0,
            "creatures.behavior",
            id,
        )?;
    }
    for s in &data.spawns {
        reference(
            &creatures,
            u64::from(s.creature_entry),
            "spawns.creature_entry",
            s.spawn_id,
        )?;
        require(s.map_id <= 65_535, "spawns.map_id", s.spawn_id)?;
        require(
            s.position_milliunits
                .iter()
                .all(|x| (-MAX_COORD..=MAX_COORD).contains(x)),
            "spawns.position_milliunits",
            s.spawn_id,
        )?;
        require(
            s.orientation_milliradians <= 6283,
            "spawns.orientation_milliradians",
            s.spawn_id,
        )?;
        require(
            (1..=MAX_INTERVAL_MS).contains(&s.respawn_ms),
            "spawns.respawn_ms",
            s.spawn_id,
        )?;
    }
    for item in &data.items {
        let id = u64::from(item.entry);
        require(label(&item.name, limits.max_string_bytes), "items.name", id)?;
        require((1..=1000).contains(&item.max_stack), "items.max_stack", id)?;
    }
    let mut relations = BTreeSet::new();
    let mut started = BTreeSet::new();
    let mut finished = BTreeSet::new();
    for r in &data.quest_relations {
        let id = u64::from(r.quest_entry);
        reference(
            &creatures,
            u64::from(r.creature_entry),
            "quest_relations.creature_entry",
            id,
        )?;
        reference(&quests, id, "quest_relations.quest_entry", id)?;
        if !relations.insert((r.creature_entry, r.quest_entry, r.kind)) {
            return Err(Error::Duplicate {
                table: "quest_relations",
                row: id,
            });
        }
        match r.kind {
            QuestRelationKind::Starts => {
                started.insert(r.quest_entry);
            }
            QuestRelationKind::Finishes => {
                finished.insert(r.quest_entry);
            }
        }
    }
    for q in &data.quests {
        let id = u64::from(q.entry);
        require(label(&q.title, limits.max_string_bytes), "quests.title", id)?;
        require(
            text(&q.details, limits.max_string_bytes, true),
            "quests.details",
            id,
        )?;
        require(
            (1..=60).contains(&q.min_level) && (q.min_level..=60).contains(&q.max_level),
            "quests.levels",
            id,
        )?;
        require(
            (1..=4).contains(&q.objectives.len()),
            "quests.objectives",
            id,
        )?;
        let mut objective_entries = BTreeSet::new();
        for objective in &q.objectives {
            reference(
                &creatures,
                u64::from(objective.creature_entry),
                "quests.objectives.creature_entry",
                id,
            )?;
            require(
                (1..=10_000).contains(&objective.count),
                "quests.objectives.count",
                id,
            )?;
            require(
                objective_entries.insert(objective.creature_entry),
                "quests.objectives.duplicate",
                id,
            )?;
        }
        require(
            q.reward_xp <= MAX_STAT && q.reward_money <= MAX_STAT,
            "quests.rewards",
            id,
        )?;
        require(q.reward_items.len() <= 4, "quests.reward_items", id)?;
        let mut reward_entries = BTreeSet::new();
        for reward in &q.reward_items {
            reference(
                &items,
                u64::from(reward.item_entry),
                "quests.reward_items.item_entry",
                id,
            )?;
            require(
                (1..=10_000).contains(&reward.count),
                "quests.reward_items.count",
                id,
            )?;
            require(
                reward_entries.insert(reward.item_entry),
                "quests.reward_items.duplicate",
                id,
            )?;
        }
        require(
            started.contains(&q.entry) && finished.contains(&q.entry),
            "quests.start_and_finish_relations",
            id,
        )?;
    }
    for a in &data.abilities {
        let id = u64::from(a.entry);
        require(
            label(&a.name, limits.max_string_bytes),
            "abilities.name",
            id,
        )?;
        require(
            (1..=MAX_STAT).contains(&a.amount) && a.mana_cost <= MAX_STAT,
            "abilities.amount_and_cost",
            id,
        )?;
        require(
            a.cast_time_ms <= MAX_INTERVAL_MS
                && a.cooldown_ms <= MAX_INTERVAL_MS
                && a.global_cooldown_ms <= MAX_INTERVAL_MS,
            "abilities.intervals",
            id,
        )?;
        require(
            a.min_range_milliunits <= a.max_range_milliunits
                && (1..=MAX_RANGE).contains(&a.max_range_milliunits),
            "abilities.range",
            id,
        )?;
    }
    Ok(())
}
