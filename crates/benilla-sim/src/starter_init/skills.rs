// SPDX-License-Identifier: GPL-3.0-or-later
// Pinned Player::LearnDefaultSkills/SetSkill/SetSkillStep/training and ObjectMgr range.
use super::{missing, unsupported, Init, InitError, InitEvent, Result, SpellRow};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefaultSkill {
    pub id: u16,
    pub step: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillLineRow {
    pub id: u16,
    pub category: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillRaceClassRow {
    pub row_id: u32,
    pub skill: u16,
    pub race_mask: u32,
    pub class_mask: u32,
    pub flags: u32,
    pub tier: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillAbilityRow {
    pub row_id: u32,
    pub skill: u16,
    pub spell: u32,
    /// Raw SkillLineAbility.forward_spellid, distinct from spell_chain ranks.
    pub forward_spell_id: u32,
    pub race_mask: u32,
    pub class_mask: u32,
    pub required_value: u32,
    pub learn_on_get_skill: u32,
    pub max_value: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillTierRow {
    pub id: u32,
    pub values: [u32; 16],
    pub maxima: [u32; 16],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitializedSkill {
    pub id: u16,
    pub value: u16,
    pub maximum: u16,
    pub step: u16,
    pub permanent_bonus: i16,
    pub temporary_bonus: i16,
}
impl InitializedSkill {
    pub fn current_value(self) -> u16 {
        (i32::from(self.value) + i32::from(self.permanent_bonus) + i32::from(self.temporary_bonus))
            .max(0) as u16
    }
}
#[derive(Clone, Copy)]
enum Range {
    Language,
    Level,
    Mono,
    Rank,
    None,
}
fn range(row: SkillLineRow, racial: bool) -> Range {
    match row.category {
        10 => Range::Language,
        6 => {
            if row.id == 473 {
                Range::Mono
            } else {
                Range::Level
            }
        }
        7 | 8 => {
            if matches!(row.id, 40 | 633) {
                Range::Level
            } else {
                Range::Mono
            }
        }
        9 | 11 => {
            if row.category == 11 || matches!(row.id, 129 | 185 | 356 | 762) {
                Range::Rank
            } else if racial {
                Range::None
            } else {
                Range::Mono
            }
        }
        _ => Range::None,
    }
}
impl Init<'_> {
    fn skill_line(&self, id: u16) -> Result<SkillLineRow> {
        self.input
            .skill_lines
            .iter()
            .find(|s| s.id == id)
            .copied()
            .ok_or(missing("SkillLine", u32::from(id)))
    }
    fn matching_skill_info(&self, id: u16) -> Result<Vec<SkillRaceClassRow>> {
        let race = 1u32 << (self.input.race - 1);
        let class = 1u32 << (self.input.class - 1);
        let mut rows: Vec<_> = self
            .input
            .skill_race_class
            .require("SkillRaceClassInfo", id.into())?
            .iter()
            .filter(|r| r.skill == id && r.race_mask & race != 0 && r.class_mask & class != 0)
            .copied()
            .collect();
        rows.sort_by_key(|r| r.row_id);
        Ok(rows)
    }
    fn tier_max(&self, id: u32, step: u16) -> Result<u16> {
        if !(1..=16).contains(&step) {
            return Err(InitError::Invalid("skill tier step"));
        }
        let tier = self
            .input
            .skill_tiers
            .iter()
            .find(|t| t.id == id)
            .ok_or(missing("SkillTiers", id))?;
        // Native field narrows uint32 to uint16.
        Ok(tier.maxima[usize::from(step - 1)] as u16)
    }
    pub(super) fn default_skill(&mut self, row: DefaultSkill) -> Result<()> {
        if row.id == 0 {
            return Err(InitError::Invalid("default skill zero"));
        }
        if self.skills.contains_key(&row.id) {
            return Ok(());
        }
        let line = self.skill_line(row.id)?;
        let (value, max, step) = match range(line, false) {
            Range::Language => (300, 300, 0),
            Range::Level => (1, 5, 0),
            Range::None => (0, 0, 0),
            Range::Mono | Range::Rank => {
                let step = if row.step == 0 { 1 } else { row.step };
                let mut selected = None;
                for info in self.matching_skill_info(row.id)? {
                    if info.tier != 0 {
                        let max = self.tier_max(info.tier, step)?;
                        selected = Some((if info.flags & 0x10 != 0 { max } else { 1 }, max, step));
                        break;
                    }
                    if info.flags & 0x10 != 0 {
                        selected = Some((5, 5, step));
                        break;
                    }
                }
                selected.ok_or(missing(
                    "usable default SkillRaceClassInfo/tier",
                    row.id.into(),
                ))?
            }
        };
        self.set_skill(row.id, value, max, step)
    }
    fn set_skill(&mut self, id: u16, value: u16, max: u16, step: u16) -> Result<()> {
        self.step()?;
        self.skill_line(id)?;
        if id == 0 {
            return Err(InitError::Invalid("zero skill"));
        }
        if value == 0 {
            if self.skills.contains_key(&id) {
                return Err(unsupported("initial skill removal", id.into(), 0));
            }
            return Ok(());
        }
        if self.skills.len() >= 127 && !self.skills.contains_key(&id) {
            return Err(InitError::Invalid("skill capacity"));
        }
        let new = !self.skills.contains_key(&id);
        let row = self.skills.entry(id).or_insert(InitializedSkill {
            id,
            value,
            maximum: max,
            step,
            permanent_bonus: 0,
            temporary_bonus: 0,
        });
        row.value = value;
        row.maximum = max;
        if step != 0 {
            row.step = step;
        }
        self.trace.push(InitEvent::SkillSet {
            skill: id,
            value,
            max,
            step: row.step,
        });
        self.skill_trained_spells(id, value)?;
        if new {
            let row = self
                .skills
                .get_mut(&id)
                .ok_or(InitError::Invalid("skill removed during learning"))?;
            row.permanent_bonus = 0;
            row.temporary_bonus = 0;
            // SetSkill resets fields AFTER recursively training spells, then reapplies aura groups.
            for permanent in [false, true] {
                for (skill, amount, kind) in self.skill_auras.clone() {
                    if skill == id && kind == permanent {
                        self.skill_bonus(id, amount, kind)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn skill_bonus(&mut self, id: u16, amount: i16, permanent: bool) -> Result<()> {
        if let Some(row) = self.skills.get_mut(&id) {
            let field = if permanent {
                &mut row.permanent_bonus
            } else {
                &mut row.temporary_bonus
            };
            *field = field.wrapping_add(amount);
        }
        Ok(())
    }
    pub(super) fn set_skill_step(&mut self, id: u16, step: u16) -> Result<()> {
        if id == 0 || step > 16 {
            return Err(InitError::Invalid("skill step target"));
        }
        self.skill_line(id)?;
        if step == 0 {
            return self.set_skill(id, 0, 0, 0);
        }
        for info in self.matching_skill_info(id)? {
            if info.tier == 0 {
                continue;
            }
            let max = self.tier_max(info.tier, step)?;
            if max == 0 {
                return Ok(());
            }
            let value = if info.flags & 0x10 != 0 {
                max
            } else {
                self.skills.get(&id).map_or(0, |r| r.value)
            }
            .max(1);
            return self.set_skill(id, value, max, step);
        }
        Err(missing(
            "usable skill-step SkillRaceClassInfo/tier",
            id.into(),
        ))
    }
    fn ability_rows(&self) -> Result<Vec<SkillAbilityRow>> {
        let mut rows = self
            .input
            .skill_abilities
            .require("SkillLineAbility", 0)?
            .to_vec();
        rows.sort_by_key(|r| r.row_id);
        Ok(rows)
    }
    fn skill_trained_spells(&mut self, id: u16, value: u16) -> Result<()> {
        let race = 1u32 << (self.input.race - 1);
        let class = 1u32 << (self.input.class - 1);
        let training_step = self.skills[&id].step;
        for row in self.ability_rows()?.into_iter().filter(|r| r.skill == id) {
            if row.race_mask != 0 && row.race_mask & race == 0
                || row.class_mask != 0 && row.class_mask & class == 0
            {
                continue;
            }
            if row.learn_on_get_skill == 0 {
                // Profession repair requires the source SpellLearnSkill map even on non-auto rows.
                let spell = self.spell(row.spell)?;
                if let Some((skill, step)) = trained_skill(spell)? {
                    if skill == id && step < training_step && !self.known.contains_key(&row.spell) {
                        self.learn(row.spell, true)?;
                    }
                }
            } else if u32::from(value) >= row.required_value {
                self.learn(row.spell, true)?;
            } else if self.known.contains_key(&row.spell) {
                return Err(unsupported(
                    "skill-threshold spell removal",
                    row.spell,
                    row.required_value,
                ));
            }
        }
        Ok(())
    }
    pub(super) fn spell_trained_skills(&mut self, spell: &SpellRow) -> Result<()> {
        if let Some((id, step)) = trained_skill(spell)? {
            return self.set_skill_step(id, step);
        }
        for row in self
            .ability_rows()?
            .into_iter()
            .filter(|r| r.spell == spell.id)
        {
            let line = self.skill_line(row.skill)?;
            if matches!(row.skill, 40 | 633) {
                if row.max_value != 0 {
                    continue;
                }
            } else if row.learn_on_get_skill != 2 {
                continue;
            }
            if self.skills.contains_key(&row.skill) {
                continue;
            }
            let infos = self.matching_skill_info(row.skill)?;
            let Some(info) = infos.first() else {
                return Err(missing(
                    "spell-trained SkillRaceClassInfo",
                    row.skill.into(),
                ));
            };
            match range(line, row.race_mask != 0) {
                Range::Language => self.set_skill(row.skill, 300, 300, 0)?,
                Range::Level => self.set_skill(row.skill, 1, 5, 0)?,
                Range::Mono => {
                    if info.flags & 0x10 != 0 {
                        self.set_skill(row.skill, 5, 5, 0)?
                    } else {
                        self.set_skill(row.skill, 1, 1, 0)?
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn skill_value(&self, id: u16) -> u16 {
        self.skills.get(&id).map_or(0, |v| v.current_value())
    }
}
fn trained_skill(row: &SpellRow) -> Result<Option<(u16, u16)>> {
    for i in 0..3 {
        if row.words[61 + i] == 118 {
            let id = row.words[106 + i] as u16;
            let step = (row.words[76 + i] as i32)
                .checked_add(row.words[67 + i] as i32)
                .ok_or(InitError::Numeric("learn skill simple value"))?
                as u16;
            if id != 0 && step != 0 {
                return Ok(Some((id, step)));
            }
        }
    }
    Ok(None)
}

/// Bounded fresh-character first entry. Default and skill-trained spells are
/// dependent and are NOT saved by `_SaveSpells`. `_LoadSpells` trains all loaded
/// skills first; then default skills/spells are learned again. The source skill
/// map is unordered, so this subset proves training decisions and constant skill
/// setters independent of its iteration order. A canonical order is used only
/// after that proof. Nondependent effect-36 learning remains unsupported here.
/// This also checks static-modifier reload invariants and equipped-item skill
/// eligibility. It does not certify uncaptured script/talent/learning rows.
pub fn first_entry(
    input: &super::StarterInput,
    created: &super::InitializedMage,
) -> Result<FirstEntryState> {
    if input.level != 1 || input.class != 8 || !(1..=8).contains(&input.race) {
        return Err(InitError::Invalid("first-entry mage identity"));
    }
    super::equipment::verify_first_entry_modifiers(input, created)?;
    let mut infos = input
        .skill_race_class
        .require("first-entry SkillRaceClassInfo", 0)?
        .to_vec();
    infos.sort_by_key(|i| i.row_id);
    let race = 1u32 << (input.race - 1);
    let class = 1u32 << (input.class - 1);
    let matching = |id: u16| {
        infos
            .iter()
            .filter(move |s| s.skill == id && s.race_mask & race != 0 && s.class_mask & class != 0)
    };
    let mut result = created.skills.clone();
    for (&id, current) in &created.skills {
        let line = input
            .skill_lines
            .iter()
            .find(|s| s.id == id)
            .copied()
            .ok_or(missing("first-entry SkillLine", id.into()))?;
        let mut value = current.value;
        let mut max = current.maximum;
        match range(line, false) {
            Range::Language => {
                value = 300;
                max = 300;
            }
            Range::Mono => {
                value = if matching(id).next().is_some_and(|s| s.flags & 0x10 != 0) {
                    5
                } else {
                    1
                };
                max = value;
            }
            Range::Level => max = 5,
            _ => {}
        }
        if value == 0 {
            return Err(unsupported("first-entry zero skill removal", id.into(), 0));
        }
        let mut step = 0;
        for entry in matching(id) {
            if entry.flags & 0x10 != 0 {
                if entry.flags & 0x400 != 0 {
                    max = 5;
                }
                value = max;
            }
            if entry.tier != 0 {
                let tiers = input
                    .skill_tiers
                    .iter()
                    .find(|t| t.id == entry.tier)
                    .ok_or(missing("first-entry SkillTiers", entry.tier))?;
                for (i, &limit) in tiers.maxima.iter().enumerate() {
                    if limit == 0 {
                        break;
                    }
                    if limit == u32::from(max) {
                        step = (i + 1) as u16;
                        break;
                    }
                }
            }
            if step != 0 {
                break;
            }
        }
        let row = result.get_mut(&id).expect("copied skill");
        row.value = value;
        row.maximum = max;
        row.step = step;
    }
    let mut normalized = result.clone();
    for skill in normalized.values_mut() {
        skill.permanent_bonus = 0;
        skill.temporary_bonus = 0;
    }
    let mut setters = std::collections::BTreeMap::<u16, (u16, u32)>::new();
    let mut proficiencies = std::collections::BTreeMap::<u32, Vec<u32>>::new();
    for known in created.abilities.values() {
        let row = input
            .spells
            .iter()
            .find(|s| s.id == known.spell_id)
            .ok_or(missing("first-entry spell row", known.spell_id))?;
        let w = &row.words;
        let has_step = w[61..64].contains(&44);
        let passive_cast = w[7] & 0x80000000 != 0
            || (w[6] & 0x40 != 0 && (w[11] == 0 || w[8] & 0x80000 != 0) && w[16] == 0);
        if (passive_cast || has_step) && w[61..64].contains(&36) {
            return Err(unsupported(
                "first-entry nondependent learn-spell replay",
                row.id,
                36,
            ));
        }
        let mut operations = vec![];
        if passive_cast || has_step {
            if w[61..64].contains(&60) {
                let masks = proficiencies.entry(w[58]).or_default();
                if masks
                    .iter()
                    .any(|mask| *mask != w[59] && *mask & w[59] != 0)
                {
                    return Err(unsupported(
                        "first-entry order-dependent proficiency masks",
                        row.id,
                        w[59],
                    ));
                }
                masks.push(w[59]);
            }
            for i in 0..3 {
                if w[61 + i] == 44 {
                    let amount = super::stats::constant_effect_amount(w[76 + i] as i32, w[67 + i])?;
                    if amount < 0 {
                        return Err(unsupported(
                            "first-entry negative skill step",
                            row.id,
                            i as u32,
                        ));
                    }
                    operations.push((w[106 + i] as u16, amount as u16));
                }
            }
        }
        // Non-passive skill-step addSpell returns before UpdateSpellTrainedSkills.
        if !has_step || passive_cast {
            if let Some(op) = trained_skill(row)? {
                operations.push(op);
            }
        }
        for (id, step) in operations {
            if id == 0 || step > 16 {
                return Err(unsupported(
                    "first-entry skill-step domain",
                    row.id,
                    u32::from(step),
                ));
            }
            if let Some((old, _)) = setters.insert(id, (step, row.id)) {
                if old != step {
                    return Err(unsupported(
                        "first-entry conflicting skill steps",
                        id.into(),
                        u32::from(step),
                    ));
                }
            }
        }
    }
    let mut abilities = input
        .skill_abilities
        .require("first-entry SkillLineAbility", 0)?
        .to_vec();
    abilities.sort_by_key(|r| r.row_id);
    for (id, (step, owner)) in setters {
        if step == 0 {
            if result.contains_key(&id) {
                return Err(unsupported("first-entry skill removal", owner, id.into()));
            }
            continue;
        }
        let entry = matching(id)
            .find(|e| e.tier != 0)
            .ok_or(missing("first-entry skill-step tier mapping", id.into()))?;
        let tier = input
            .skill_tiers
            .iter()
            .find(|t| t.id == entry.tier)
            .ok_or(missing("first-entry SkillTiers", entry.tier))?;
        let max = tier.maxima[usize::from(step - 1)] as u16;
        if max == 0 {
            continue;
        }
        let value = if entry.flags & 0x10 != 0 {
            max
        } else {
            result.get(&id).map_or(0, |s| s.value)
        }
        .max(1);
        let row = result.entry(id).or_insert(InitializedSkill {
            id,
            value,
            maximum: max,
            step,
            permanent_bonus: 0,
            temporary_bonus: 0,
        });
        row.value = value;
        row.maximum = max;
        row.step = step;
        // Every initial skill aura is retained. Source SetSkill reapplication and
        // saved passive recasts produce this same modulo-16-bit aggregate.
        row.permanent_bonus = 0;
        row.temporary_bonus = 0;
        for aura in &created.initial_auras {
            if aura.misc_value as u16 == id && matches!(aura.aura_type, 30 | 98) {
                let bonus = if aura.aura_type == 98 {
                    &mut row.permanent_bonus
                } else {
                    &mut row.temporary_bonus
                };
                *bonus = bonus.wrapping_add(aura.amount as i16);
            }
        }
    }
    // Every skill can have only its loaded state or the one agreed setter state.
    // Training predicates must be invariant across those states; otherwise the
    // unordered source map can change recursive learn/remove order.
    for (&id, loaded) in &normalized {
        let final_skill = result
            .get(&id)
            .ok_or(InitError::Invalid("first-entry removed skill"))?;
        for ability in abilities.iter().filter(|a| a.skill == id) {
            if ability.race_mask != 0 && ability.race_mask & race == 0
                || ability.class_mask != 0 && ability.class_mask & class == 0
            {
                continue;
            }
            let decision = |skill: &InitializedSkill| -> Result<bool> {
                if ability.learn_on_get_skill != 0 {
                    Ok(u32::from(skill.value) >= ability.required_value)
                } else {
                    let row =
                        input
                            .spells
                            .iter()
                            .find(|s| s.id == ability.spell)
                            .ok_or(missing(
                                "first-entry profession-repair spell",
                                ability.spell,
                            ))?;
                    Ok(trained_skill(row)?
                        .is_some_and(|(target, step)| target == id && step < skill.step))
                }
            };
            if decision(loaded)? != decision(final_skill)? {
                return Err(unsupported(
                    "first-entry order-dependent skill training",
                    ability.spell,
                    id.into(),
                ));
            }
        }
    }
    // Replay the actual fresh-login phases. There are no saved spell rows or
    // saved auras in this admitted dependent-spell/passive-aura subset.
    let mut replay = Init {
        input,
        stats: super::stats::Stats::new(
            input.create_attributes,
            input.create_health,
            input.create_mana,
        )?,
        skills: normalized,
        known: Default::default(),
        trace: vec![InitEvent::LoadSavedSkills],
        skill_auras: vec![],
        applied_auras: Default::default(),
        initial_auras: vec![],
        weapon_proficiency: 0,
        armor_proficiency: 0,
        can_parry: false,
        can_block: false,
        budget: 4096,
    };
    let loaded_ids: Vec<_> = replay.skills.keys().copied().collect();
    for id in &loaded_ids {
        let value = replay.skills[id].value;
        replay
            .trace
            .push(InitEvent::TrainLoadedSkill { skill: *id, value });
        replay.skill_trained_spells(*id, value)?;
        if !replay.skills.keys().copied().eq(loaded_ids.iter().copied()) {
            return Err(unsupported(
                "first-entry skill-map mutation during unordered training",
                (*id).into(),
                0,
            ));
        }
    }
    replay.trace.push(InitEvent::DefaultSkills);
    for row in input.default_skills.require("playercreateinfo_skills", 0)? {
        replay.default_skill(*row)?;
    }
    replay.trace.push(InitEvent::DefaultSpells);
    for &id in input.default_spells.require("playercreateinfo_spell", 0)? {
        replay.learn(id, true)?;
    }
    if replay.known != created.abilities {
        let id = replay
            .known
            .keys()
            .chain(created.abilities.keys())
            .copied()
            .find(|id| replay.known.get(id) != created.abilities.get(id))
            .unwrap_or(0);
        return Err(unsupported(
            "first-entry changed trained spell closure",
            id,
            0,
        ));
    }
    if replay.initial_auras.len() != created.initial_auras.len()
        || replay
            .initial_auras
            .iter()
            .any(|a| !created.initial_auras.contains(a))
        || replay.weapon_proficiency != created.weapon_proficiency
        || replay.armor_proficiency != created.armor_proficiency
        || replay.can_parry != created.can_parry
        || replay.can_block != created.can_block
    {
        return Err(unsupported(
            "first-entry changed passive/proficiency state",
            0,
            0,
        ));
    }
    if replay.skills.keys().ne(result.keys())
        || replay.skills.iter().any(|(id, s)| {
            let expected = &result[id];
            (s.value, s.maximum, s.step) != (expected.value, expected.maximum, expected.step)
        })
    {
        return Err(unsupported("first-entry unproven skill replay state", 0, 0));
    }
    let result = replay.skills;
    for equipped in created.equipped.values() {
        let item = input
            .items
            .iter()
            .find(|i| i.id == equipped.item)
            .ok_or(missing("first-entry equipped item", equipped.item))?;
        let current = |id: u16| result.get(&id).map_or(0, |s| s.current_value());
        let proficiency = if item.class == 2 && item.subclass == 13 {
            473
        } else {
            super::items::skill(item)
        };
        if item.required_skill != 0
            && (current(item.required_skill) == 0
                || u32::from(current(item.required_skill)) < item.required_skill_rank)
            || proficiency != 0 && current(proficiency) == 0
        {
            return Err(unsupported(
                "first-entry changed equipment skill eligibility",
                item.id,
                0,
            ));
        }
    }
    Ok(FirstEntryState {
        skills: result,
        trace: replay.trace,
    })
}

/// Canonical trace of source phases after proving unordered training commutes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstEntryState {
    pub skills: std::collections::BTreeMap<u16, InitializedSkill>,
    pub trace: Vec<InitEvent>,
}

pub fn first_entry_skills(
    input: &super::StarterInput,
    created: &super::InitializedMage,
) -> Result<std::collections::BTreeMap<u16, InitializedSkill>> {
    Ok(first_entry(input, created)?.skills)
}
