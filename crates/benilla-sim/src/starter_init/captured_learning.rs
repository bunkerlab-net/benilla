// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, SpellMgr.cpp:1284-1770,2340-2389; ObjectMgr.cpp:3225-3330.
// src/game/Entities/Player.cpp:5642-5698, UpdateSkillTrainedSpells; Server/DBCStructure.h:653-668.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Selected loader projections, not source authentication or completed initial casts.
use super::*;

const MAX_SKILLS: usize = 512;
const MAX_ABILITIES: usize = 4096;
const MAX_REFERENCES: usize = 16_384;
const MAX_SPELLS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationSkill {
    pub race_mask: u32,
    pub class_mask: u32,
    pub skill: u32,
    pub step: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationSpell {
    pub race: u8,
    pub class: u8,
    pub spell: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedSpellChain {
    pub spell: u32,
    pub previous: u32,
    pub first: u32,
    pub rank: u32,
    pub required: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedLearnSpell {
    pub entry: u32,
    pub spell: u32,
    pub active: bool,
}
/// Numeric overlay is independently verified by the caller; original words remain bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningSpellNumeric {
    pub source_identity: [u8; 32],
    pub id: u32,
    pub client_words: [u32; 173],
    pub effective_words: [u32; 173],
    pub duration_ms: Option<i32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LearningInitialHooks {
    pub spell: u32,
    pub source_identity: [u8; 32],
    /// None is unknown, never absent. Runtime must independently revalidate source hooks.
    pub external_hooks: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillLoaderDisposition {
    Loaded(DefaultSkill),
    UnmatchedMasks,
    Unobtainable { skill: u16 },
}
/// All complete collections are verified relevant lookups, not merely selected present rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedStarterLearning {
    pub source_identity: [u8; 32],
    pub selected_spell_ids: Vec<u32>,
    pub creation_skills: Rows<CreationSkill>,
    pub creation_spells: Rows<CreationSpell>,
    pub skill_lines: Rows<[u32; 22]>,
    pub skill_race_class: Rows<[u32; 8]>,
    pub skill_tiers: Rows<[u32; 33]>,
    /// Complete zero-acquire SQL lookups: spell ID and first valid Skill effect.
    /// None proves no repair mapping, including a verified missing server spell.
    pub skill_training: Rows<(u32, Option<(u16, u16)>)>,
    /// Union by reachable skill, spell AND incoming forward spell, in any capture order.
    pub skill_abilities: Rows<[u32; 15]>,
    /// Complete talent membership and rank/dependency closure.
    pub talents: Rows<[u32; 21]>,
    pub chains: Rows<CapturedSpellChain>,
    pub learn_spells: Rows<CapturedLearnSpell>,
    /// Observed zero/one-row global probes, separate from selected relationships.
    pub global_chain_probe: Rows<CapturedSpellChain>,
    pub global_learn_probe: Rows<CapturedLearnSpell>,
    /// Complete exact server existence lookups, including verified missing records.
    pub spell_existence: Rows<(u32, bool)>,
    pub numeric: Rows<LearningSpellNumeric>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledStarterLearning {
    race: u8,
    class: u8,
    source_identity: [u8; 32],
    defaults: Vec<DefaultSkill>,
    roots: Vec<u32>,
    dispositions: Vec<SkillLoaderDisposition>,
    ability_dispositions: Vec<(u32, &'static str)>,
    lines: Vec<SkillLineRow>,
    infos: Vec<SkillRaceClassRow>,
    tiers: Vec<SkillTierRow>,
    abilities: Vec<SkillAbilityRow>,
    numeric: Vec<LearningSpellNumeric>,
    ranks: BTreeMap<u32, CapturedSpellChain>,
    talents: BTreeSet<u32>,
    learned: BTreeMap<u32, Vec<LearnSpell>>,
}
fn keyed<'a, const N: usize>(
    rows: &'a [[u32; N]],
    table: &'static str,
    limit: usize,
) -> Result<BTreeMap<u32, &'a [u32; N]>> {
    if rows.len() > limit {
        return Err(InitError::Invalid("learning collection limit"));
    }
    let mut map = BTreeMap::new();
    for row in rows {
        if map.insert(row[0], row).is_some() {
            return Err(unsupported(table, row[0], 0));
        }
    }
    Ok(map)
}
fn skill_id(id: u32) -> Result<u16> {
    u16::try_from(id)
        .ok()
        .filter(|id| *id != 0)
        .ok_or(InitError::Invalid("learning skill domain"))
}
impl CapturedStarterLearning {
    pub fn compile(&self, race: u8, class: u8) -> Result<CompiledStarterLearning> {
        if race != 1 || class != 8 || self.source_identity == [0; 32] {
            return Err(InitError::Invalid("learning identity"));
        }
        let mut skills = self.creation_skills.require("playercreateinfo_skills", 0)?.to_vec();
        skills.sort_by_key(crate::canonical_order::creation_skill);
        let mut spells = self.creation_spells.require("playercreateinfo_spell", 0)?.to_vec();
        spells.sort_by_key(crate::canonical_order::creation_spell);
        let chain_rows = self.chains.require("spell_chain", 0)?;
        let mut learn_rows = self.learn_spells.require("spell_learn_spell", 0)?.to_vec();
        learn_rows.sort_by_key(|row| (row.entry, row.spell, row.active));
        let chain_probe = self
            .global_chain_probe
            .require("global spell_chain probe", 0)?;
        let learn_probe = self
            .global_learn_probe
            .require("global spell_learn_spell probe", 0)?;
        if chain_probe.len() > 1 || learn_probe.len() > 1 {
            return Err(InitError::Invalid("learning global probe bound"));
        }
        let chain_nonempty = !chain_probe.is_empty();
        let learn_nonempty = !learn_probe.is_empty();
        if !chain_nonempty && !chain_rows.is_empty() || !learn_nonempty && !learn_rows.is_empty() {
            return Err(InitError::Invalid("learning global probe contradiction"));
        }
        if skills.len() > MAX_SKILLS
            || spells.len() > MAX_SPELLS
            || chain_rows.len() + learn_rows.len() > MAX_REFERENCES
        {
            return Err(InitError::Invalid("learning root/edge limit"));
        }
        let lines = keyed(
            self.skill_lines.require("SkillLine", 0)?,
            "SkillLine",
            MAX_SKILLS,
        )?;
        let infos = keyed(
            self.skill_race_class.require("SkillRaceClassInfo", 0)?,
            "SkillRaceClassInfo",
            MAX_ABILITIES,
        )?;
        let tiers = keyed(
            self.skill_tiers.require("SkillTiers", 0)?,
            "SkillTiers",
            MAX_SKILLS,
        )?;
        let abilities = keyed(
            self.skill_abilities.require("SkillLineAbility", 0)?,
            "SkillLineAbility",
            MAX_ABILITIES,
        )?;
        let talent_rows = keyed(self.talents.require("Talent", 0)?, "Talent", MAX_ABILITIES)?;
        let numeric = self.numeric.require("effective spell numeric overlay", 0)?;
        if numeric.len() > MAX_SPELLS || self.selected_spell_ids.len() > MAX_SPELLS {
            return Err(InitError::Invalid("learning spell limit"));
        }
        let mut selected = BTreeSet::new();
        for &id in &self.selected_spell_ids {
            if id == 0 || !selected.insert(id) {
                return Err(InitError::Invalid("learning selected spell IDs"));
            }
        }
        if chain_probe
            .iter()
            .any(|row| selected.contains(&row.spell) && !chain_rows.contains(row))
            || learn_probe
                .iter()
                .any(|row| selected.contains(&row.entry) && !learn_rows.contains(row))
        {
            return Err(InitError::Invalid(
                "learning global probe selected-row binding",
            ));
        }
        let mut existence = BTreeMap::new();
        let existence_rows = self
            .spell_existence
            .require("server SpellEntry existence", 0)?;
        if existence_rows.len() > MAX_REFERENCES {
            return Err(InitError::Invalid("learning existence limit"));
        }
        for &(id, exists) in existence_rows {
            if id == 0 || existence.insert(id, exists).is_some() {
                return Err(InitError::Invalid("duplicate/zero server existence lookup"));
            }
        }
        let exists = |id: u32| -> Result<bool> {
            existence
                .get(&id)
                .copied()
                .ok_or(missing("server SpellEntry existence", id))
        };
        let mut numerics = BTreeMap::new();
        for row in numeric {
            if row.source_identity != self.source_identity
                || row.id == 0
                || row.client_words[0] != row.id
                || row.effective_words[0] != row.id
                || numerics.insert(row.id, row).is_some()
            {
                return Err(InitError::Invalid("learning numeric binding"));
            }
            for index in [
                37, 70, 71, 72, 73, 74, 75, 97, 98, 99, 112, 113, 114, 167, 168, 169,
            ] {
                for words in [&row.client_words, &row.effective_words] {
                    let bits = words[index];
                    if bits & 0x7f80_0000 == 0x7f80_0000 || bits == 0x8000_0000 {
                        return Err(unsupported(
                            "noncanonical learning float",
                            row.id,
                            index as u32,
                        ));
                    }
                }
            }
            if !exists(row.id)? {
                return Err(missing("spell_template", row.id));
            }
        }
        if !selected.iter().copied().eq(numerics.keys().copied()) {
            return Err(InitError::Invalid("learning exact selected numeric IDs"));
        }
        let references = skills.len()
            + spells.len()
            + abilities
                .values()
                .map(|row| {
                    [row[1], row[2], row[8]]
                        .into_iter()
                        .filter(|id| *id != 0)
                        .count()
                })
                .sum::<usize>()
            + talent_rows
                .values()
                .filter(|row| row[4..9].iter().any(|id| selected.contains(id)))
                .map(|row| {
                    row[4..9].iter().filter(|id| **id != 0).count() + usize::from(row[20] != 0)
                })
                .sum::<usize>()
            + chain_rows
                .iter()
                .map(|row| {
                    [row.spell, row.previous, row.first, row.required]
                        .into_iter()
                        .filter(|id| *id != 0)
                        .count()
                })
                .sum::<usize>()
            + learn_rows.len() * 2;
        if references > MAX_REFERENCES {
            return Err(InitError::Invalid("learning reference limit"));
        }
        let mut out = CompiledStarterLearning {
            race,
            class,
            source_identity: self.source_identity,
            defaults: Vec::new(),
            roots: Vec::new(),
            dispositions: Vec::new(),
            ability_dispositions: Vec::new(),
            lines: Vec::with_capacity(lines.len()),
            infos: Vec::new(),
            tiers: Vec::new(),
            abilities: Vec::with_capacity(abilities.len()),
            numeric: numeric.to_vec(),
            ranks: BTreeMap::new(),
            talents: BTreeSet::new(),
            learned: BTreeMap::new(),
        };
        for (&id, row) in &lines {
            out.lines.push(SkillLineRow {
                id: skill_id(id)?,
                category: row[1] as i32,
            });
        }
        for row in infos.values() {
            if lines.contains_key(&row[1]) {
                out.infos.push(SkillRaceClassRow {
                    row_id: row[0],
                    skill: skill_id(row[1])?,
                    race_mask: row[2],
                    class_mask: row[3],
                    flags: row[4],
                    tier: row[6],
                });
            }
        }
        for row in tiers.values() {
            out.tiers.push(SkillTierRow {
                id: row[0],
                values: row[1..17].try_into().unwrap(),
                maxima: row[17..33].try_into().unwrap(),
            });
        }
        let mut info_by_skill: BTreeMap<u16, Vec<&SkillRaceClassRow>> = BTreeMap::new();
        for info in &out.infos {
            info_by_skill.entry(info.skill).or_default().push(info);
        }
        let mut loaded_skills = BTreeSet::new();
        for row in &skills {
            let skill = skill_id(row.skill)?;
            let step =
                u16::try_from(row.step).map_err(|_| InitError::Invalid("learning step domain"))?;
            if step > 16 {
                return Err(unsupported("creation skill step", row.skill, row.step));
            }
            if row.race_mask != 0 && row.race_mask & 1 == 0
                || row.class_mask != 0 && row.class_mask & 128 == 0
            {
                out.dispositions
                    .push(SkillLoaderDisposition::UnmatchedMasks);
                continue;
            }
            if !lines.contains_key(&row.skill) {
                return Err(missing("SkillLine", row.skill));
            }
            let mut obtainable = false;
            for info in info_by_skill
                .get(&skill)
                .into_iter()
                .flatten()
                .filter(|r| r.race_mask & 1 != 0 && r.class_mask & 128 != 0)
            {
                if step != 0 {
                    let tier = tiers
                        .get(&info.tier)
                        .ok_or(missing("SkillTiers", info.tier))?;
                    if tier[16 + usize::from(step)] == 0 {
                        continue;
                    }
                }
                obtainable = true;
                break;
            }
            if !obtainable {
                out.dispositions
                    .push(SkillLoaderDisposition::Unobtainable { skill });
                continue;
            }
            if !loaded_skills.insert(skill) {
                return Err(unsupported(
                    "duplicate effective creation skill",
                    row.skill,
                    row.step,
                ));
            }
            let default = DefaultSkill { id: skill, step };
            out.defaults.push(default);
            out.dispositions
                .push(SkillLoaderDisposition::Loaded(default));
        }
        let mut root_ids = BTreeSet::new();
        for row in &spells {
            if row.race != race
                || row.class != class
                || !selected.contains(&row.spell)
                || !root_ids.insert(row.spell)
            {
                return Err(InitError::Invalid("creation spell selection/duplicate"));
            }
            out.roots.push(row.spell);
        }
        let mut training = BTreeMap::new();
        if let Rows::Complete(rows) = &self.skill_training {
            if rows.len() > MAX_ABILITIES {
                return Err(InitError::Invalid("skill training probe limit"));
            }
            for &(id, mapping) in rows {
                if id == 0 || training.insert(id, mapping).is_some() {
                    return Err(InitError::Invalid("skill training probe IDs"));
                }
            }
        }
        let mut skill_steps = BTreeMap::<u32, u16>::new();
        for skill in skills {
            if (skill.race_mask == 0 || skill.race_mask & 1 != 0)
                && (skill.class_mask == 0 || skill.class_mask & 128 != 0)
            {
                let step = skill_steps.entry(skill.skill).or_default();
                *step = (*step).max(skill.step as u16);
            }
        }
        for numeric in numeric {
            for words in [&numeric.client_words, &numeric.effective_words] {
                for slot in 0..3 {
                    if matches!(words[61 + slot], 44 | 118) && (words[106 + slot] as i32) > 0 {
                        let step = skill_steps.entry(words[106 + slot]).or_default();
                        *step = (*step).max(words[76 + slot].wrapping_add(1) as u16);
                    }
                }
            }
        }
        for row in abilities.values() {
            let line = lines.get(&row[1]).ok_or(missing("SkillLine", row[1]))?;
            let step = skill_steps.get(&row[1]).copied().unwrap_or(0);
            let disposition = if row[3] != 0 && row[3] & 1 == 0 || row[4] != 0 && row[4] & 128 == 0
            {
                "unmatched_masks"
            } else if row[9] == 0 {
                let mapping = if let Some(numeric) = numerics.get(&row[2]) {
                    let mut mapping = None;
                    for slot in 0..3 {
                        let w = &numeric.effective_words;
                        if w[61 + slot] == 118
                            && w[106 + slot] != 0
                            && w[76 + slot].wrapping_add(1) != 0
                        {
                            mapping =
                                Some((w[106 + slot] as u16, w[76 + slot].wrapping_add(1) as u16));
                            break;
                        }
                    }
                    mapping
                } else {
                    *training
                        .get(&row[2])
                        .ok_or(missing("SQL skill training probe", row[2]))?
                };
                if mapping.is_some_and(|(skill, previous_step)| {
                    u32::from(skill) == row[1] && previous_step < step
                }) {
                    "profession_repair"
                } else {
                    "acquire_method_excluded"
                }
            } else {
                let value = match line[1] {
                    10 => 300,
                    6 if row[1] != 473 => 1,
                    _ if matches!(row[1], 40 | 633) => 1,
                    6 | 7 | 8 | 9 | 11 => infos
                        .values()
                        .find(|info| info[1] == row[1] && info[2] & 1 != 0 && info[3] & 128 != 0)
                        .map_or(65535, |info| {
                            if info[4] & 0x10 == 0 {
                                1
                            } else if info[6] == 0 {
                                5
                            } else {
                                let step = usize::from(step.max(1));
                                tiers
                                    .get(&info[6])
                                    .filter(|_| step <= 16)
                                    .map_or(65535, |tier| tier[16 + step] & 0xffff)
                            }
                        }),
                    _ => 65535,
                };
                if row[7] > value {
                    "minimum_value_excluded"
                } else {
                    "auto_training"
                }
            };
            out.ability_dispositions.push((row[0], disposition));
            if !selected.contains(&row[2]) {
                if matches!(
                    disposition,
                    "unmatched_masks" | "acquire_method_excluded" | "minimum_value_excluded"
                ) {
                    continue;
                }
                return Err(missing("selected ability spell numeric", row[2]));
            }
            out.abilities.push(SkillAbilityRow {
                row_id: row[0],
                skill: skill_id(row[1])?,
                spell: row[2],
                forward_spell_id: row[8],
                race_mask: row[3],
                class_mask: row[4],
                required_value: row[7],
                learn_on_get_skill: row[9],
                max_value: row[10],
            });
        }
        let mut talent_positions = BTreeMap::new();
        for row in talent_rows.values() {
            if !row[4..9].iter().any(|id| selected.contains(id)) {
                continue;
            }
            for (slot, &id) in row[4..9].iter().enumerate().filter(|(_, id)| **id != 0) {
                if talent_positions.insert(id, (row[0], slot)).is_some() {
                    return Err(unsupported("ambiguous talent membership", id, row[0]));
                }
                if !exists(id)? {
                    continue;
                }
                out.talents.insert(id);
                if row[5] != 0 {
                    out.ranks.insert(
                        id,
                        CapturedSpellChain {
                            spell: id,
                            previous: if slot == 0 { 0 } else { row[3 + slot] },
                            first: row[4],
                            rank: slot as u32 + 1,
                            required: 0,
                        },
                    );
                }
            }
            if row[20] != 0 {
                exists(row[20])?;
            }
        }
        let mut predecessors = BTreeMap::new();
        let ability_spell_ids: BTreeSet<_> = out.abilities.iter().map(|row| row.spell).collect();
        for row in &out.abilities {
            let forward = if row.spell == 2366 {
                2368
            } else {
                row.forward_spell_id
            };
            if forward == 0 || !exists(row.spell)? || !exists(forward)? {
                continue;
            }
            if !ability_spell_ids.contains(&forward) {
                continue;
            }
            if let Some(rank) = out.ranks.get(&forward) {
                if rank.previous != row.spell {
                    return Err(unsupported(
                        "conflicting talent/forward predecessor",
                        forward,
                        row.spell,
                    ));
                }
            } else if predecessors
                .insert(forward, row.spell)
                .is_some_and(|old| old != row.spell)
            {
                return Err(unsupported(
                    "conflicting forward predecessor",
                    forward,
                    row.spell,
                ));
            }
        }
        for &id in predecessors.keys() {
            let mut path = Vec::new();
            let mut cursor = id;
            while !out.ranks.contains_key(&cursor) {
                if path.contains(&cursor) {
                    return Err(unsupported("learning rank cycle", cursor, id));
                }
                if path.len() > 30 {
                    return Err(unsupported("learning rank depth", id, 30));
                }
                if let Some(&prev) = predecessors.get(&cursor) {
                    path.push(cursor);
                    cursor = prev;
                } else {
                    out.ranks.insert(
                        cursor,
                        CapturedSpellChain {
                            spell: cursor,
                            previous: 0,
                            first: cursor,
                            rank: 1,
                            required: 0,
                        },
                    );
                }
            }
            for current in path.into_iter().rev() {
                let prev = predecessors[&current];
                let parent = out.ranks[&prev];
                if parent.rank >= 32 {
                    return Err(unsupported("learning rank depth", current, 32));
                }
                out.ranks.insert(
                    current,
                    CapturedSpellChain {
                        spell: current,
                        previous: prev,
                        first: parent.first,
                        rank: parent.rank + 1,
                        required: 0,
                    },
                );
            }
        }
        let mut sql_ids = BTreeSet::new();
        for &row in chain_rows {
            if !selected.contains(&row.spell) || !sql_ids.insert(row.spell) || row.rank > 127 {
                return Err(InitError::Invalid("SQL chain selection/domain/duplicate"));
            }
            for id in [row.spell, row.previous, row.first, row.required]
                .into_iter()
                .filter(|id| *id != 0)
            {
                if !exists(id)? {
                    return Err(missing("spell_chain server spell", id));
                }
            }
            if let Some(derived) = out.ranks.get_mut(&row.spell) {
                if (derived.previous, derived.first, derived.rank)
                    != (row.previous, row.first, row.rank)
                {
                    return Err(unsupported(
                        "SQL/derived rank conflict",
                        row.spell,
                        row.previous,
                    ));
                }
                derived.required = row.required;
            } else {
                if row.first == 0
                    || (row.spell == row.first) != (row.rank <= 1)
                    || (row.spell == row.first) != (row.previous == 0)
                {
                    return Err(unsupported("SQL chain integrity", row.spell, row.rank));
                }
                if let Some(&(talent, slot)) = talent_positions.get(&row.spell) {
                    let t = talent_rows[&talent];
                    if row.rank != slot as u32 + 1
                        || row.first != t[4]
                        || row.required != t[20]
                        || slot > 0 && row.previous != t[3 + slot]
                    {
                        return Err(unsupported(
                            "SQL talent chain integrity",
                            row.spell,
                            row.rank,
                        ));
                    }
                }
                out.ranks.insert(row.spell, row);
            }
        }
        for row in out.ranks.values() {
            let mut seen = BTreeSet::new();
            let mut cursor = row.spell;
            while let Some(node) = out.ranks.get(&cursor) {
                if !seen.insert(cursor) {
                    return Err(unsupported("learning rank cycle", cursor, row.spell));
                }
                if node.previous == 0 {
                    break;
                }
                let prev = out
                    .ranks
                    .get(&node.previous)
                    .ok_or(missing("previous rank metadata", node.previous))?;
                if prev.first != node.first || prev.rank + 1 != node.rank {
                    return Err(unsupported("rank continuity", cursor, node.previous));
                }
                cursor = node.previous;
            }
            if row.required != 0 {
                let req = out
                    .ranks
                    .get(&row.required)
                    .ok_or(missing("required rank metadata", row.required))?;
                if req.first == row.first || req.required != 0 {
                    return Err(unsupported(
                        "required rank relationship",
                        row.spell,
                        row.required,
                    ));
                }
            }
        }
        if learn_nonempty {
            let mut pairs = BTreeSet::new();
            for row in learn_rows {
                if !selected.contains(&row.entry) || !pairs.insert((row.entry, row.spell)) {
                    return Err(InitError::Invalid("SQL learn selection/duplicate"));
                }
                if !exists(row.entry)? || !exists(row.spell)? {
                    return Err(missing("spell_learn_spell server spell", row.spell));
                }
                if out.talents.contains(&row.spell) {
                    return Err(unsupported("SQL learning talent", row.entry, row.spell));
                }
                out.learned.entry(row.entry).or_default().push(LearnSpell {
                    spell_id: row.spell,
                    active: row.active,
                    auto_learned: false,
                });
            }
            for row in numeric {
                let w = &row.effective_words;
                for slot in 0..3 {
                    if w[61 + slot] != 36 {
                        continue;
                    }
                    let target = w[109 + slot];
                    if !exists(target)? {
                        continue;
                    }
                    if !selected.contains(&target) {
                        return Err(missing("selected learned spell numeric", target));
                    }
                    let links = out.learned.entry(row.id).or_default();
                    if !links.iter().any(|link| link.spell_id == target) {
                        links.push(LearnSpell {
                            spell_id: target,
                            active: true,
                            auto_learned: w[82 + slot] == 5
                                || out.talents.contains(&row.id)
                                || w[6] & 0x40 != 0
                                || w[61..64].contains(&44),
                        });
                    }
                }
            }
        }
        if out.learned.values().map(Vec::len).sum::<usize>() + out.ranks.len() > MAX_REFERENCES {
            return Err(InitError::Invalid("learning derived edge limit"));
        }
        for links in out.learned.values() {
            for link in links {
                if !selected.contains(&link.spell_id) {
                    return Err(missing("selected learned spell numeric", link.spell_id));
                }
            }
        }
        let mut degrees: BTreeMap<_, usize> = selected.iter().map(|id| (*id, 0)).collect();
        for links in out.learned.values() {
            for link in links {
                *degrees.get_mut(&link.spell_id).unwrap() += 1;
            }
        }
        let mut ready: Vec<_> = degrees
            .iter()
            .filter_map(|(id, n)| (*n == 0).then_some(*id))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop() {
            visited += 1;
            for link in out.learned.get(&id).into_iter().flatten() {
                let n = degrees.get_mut(&link.spell_id).unwrap();
                *n -= 1;
                if *n == 0 {
                    ready.push(link.spell_id);
                }
            }
        }
        if visited != selected.len() {
            return Err(InitError::Invalid("learning spell cycle"));
        }
        Ok(out)
    }
}
impl CompiledStarterLearning {
    pub fn default_skills(&self) -> &[DefaultSkill] {
        &self.defaults
    }
    pub fn default_spells(&self) -> &[u32] {
        &self.roots
    }
    pub fn skill_dispositions(&self) -> &[SkillLoaderDisposition] {
        &self.dispositions
    }
    pub fn ability_dispositions(&self) -> &[(u32, &'static str)] {
        &self.ability_dispositions
    }
    pub fn rank(&self, spell: u32) -> Option<&CapturedSpellChain> {
        self.ranks.get(&spell)
    }
    pub fn bind_spell_rows(
        &self,
        numeric: &[LearningSpellNumeric],
        hooks: &Rows<LearningInitialHooks>,
    ) -> Result<Vec<SpellRow>> {
        if numeric != self.numeric {
            return Err(InitError::Invalid("learning numeric source equality"));
        }
        let hooks = hooks.require("initial hook dispositions", 0)?;
        if hooks.len() != numeric.len() {
            return Err(InitError::Invalid("learning exact hook IDs"));
        }
        let mut indexed = BTreeMap::new();
        for hook in hooks {
            if hook.source_identity != self.source_identity
                || indexed.insert(hook.spell, hook).is_some()
            {
                return Err(InitError::Invalid("learning hook binding"));
            }
        }
        numeric
            .iter()
            .map(|row| {
                let hook = indexed
                    .get(&row.id)
                    .ok_or(missing("initial hook disposition", row.id))?;
                let external_initial_hooks = hook
                    .external_hooks
                    .ok_or(missing("resolved initial hooks", row.id))?;
                Ok(SpellRow {
                    id: row.id,
                    words: row.effective_words,
                    duration_ms: row.duration_ms,
                    previous_rank: self
                        .ranks
                        .get(&row.id)
                        .and_then(|r| (r.previous != 0).then_some(r.previous)),
                    talent: self.talents.contains(&row.id),
                    learned_spells: Rows::Complete(
                        self.learned.get(&row.id).cloned().unwrap_or_default(),
                    ),
                    external_initial_hooks,
                })
            })
            .collect()
    }
    pub fn populate(
        &self,
        mut input: StarterInput,
        spell_rows: Vec<SpellRow>,
    ) -> Result<StarterInput> {
        if input.race != self.race
            || input.class != self.class
            || spell_rows.len() != self.numeric.len()
        {
            return Err(InitError::Invalid("learning initializer identity"));
        }
        let hooks = Rows::Complete(
            spell_rows
                .iter()
                .map(|r| LearningInitialHooks {
                    spell: r.id,
                    source_identity: self.source_identity,
                    external_hooks: Some(r.external_initial_hooks),
                })
                .collect(),
        );
        if self.bind_spell_rows(&self.numeric, &hooks)? != spell_rows {
            return Err(InitError::Invalid("learning initializer spell equality"));
        }
        input.default_skills = Rows::Complete(self.defaults.clone());
        input.default_spells = Rows::Complete(self.roots.clone());
        input.skill_lines = self.lines.clone();
        input.skill_race_class = Rows::Complete(self.infos.clone());
        input.skill_tiers = self.tiers.clone();
        input.skill_abilities = Rows::Complete(self.abilities.clone());
        input.spells = spell_rows;
        Ok(input)
    }
}

#[cfg(test)]
#[path = "captured_learning_tests.rs"]
mod tests;
