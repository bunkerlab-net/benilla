// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{player_rows::dbc_word, *};
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeMap;

// Player.h:76; UpdateFields.h:216.
const PLAYER_MAX_SKILLS: usize = 127;
const PLAYER_SKILL_INFO: usize = 0x2ce;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourcePlayerSkill {
    pub id_step: [u16; 2],
    pub value_max: [u16; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerCreateSkill {
    pub id: u16,
    pub step: u16,
}

#[derive(Clone, Copy)]
pub(super) struct CreationSkillRow {
    pub race: u32,
    pub class: u32,
    pub skill: PlayerCreateSkill,
}

#[derive(Clone, Copy, Debug)]
struct SkillRaceClass {
    race: u32,
    class: u32,
    flags: u32,
    tier: u32,
}

#[derive(Debug)]
struct SkillAbility {
    spell: u32,
    race: u32,
    class: u32,
    required: u32,
    learn_on_skill: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum SkillRange {
    None,
    Language,
    Level,
    Mono,
    Rank,
}

#[derive(Debug, Default)]
pub(super) struct PlayerSkillInputs {
    categories: BTreeMap<u16, u32>,
    race_class: BTreeMap<u16, Vec<SkillRaceClass>>,
    tiers: BTreeMap<u32, [u32; 16]>,
    abilities: BTreeMap<u16, Vec<SkillAbility>>,
}

impl PlayerSkillInputs {
    pub fn load(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let mut inputs = Self::default();
        {
            let mut view = catalog.dbc_view("SkillLine.dbc")?;
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                inputs
                    .categories
                    .insert(dbc_word(&row, 0)? as u16, dbc_word(&row, 1)?);
            }
        }
        {
            // SpellMgr.cpp:2364-2385 builds one ordered multimap from supported lines.
            let mut view = catalog.dbc_view("SkillRaceClassInfo.dbc")?;
            let mut by_key = BTreeMap::new();
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                let skill = dbc_word(&row, 1)? as u16;
                let key = row["key"]
                    .as_u64()
                    .ok_or_else(|| refuse("execution.player.catalog_dbc_layout"))?;
                by_key.insert(
                    key,
                    (
                        skill,
                        SkillRaceClass {
                            race: dbc_word(&row, 2)?,
                            class: dbc_word(&row, 3)?,
                            flags: dbc_word(&row, 4)?,
                            tier: dbc_word(&row, 6)?,
                        },
                    ),
                );
            }
            for (skill, row) in by_key.into_values() {
                if inputs.categories.contains_key(&skill) {
                    inputs.race_class.entry(skill).or_default().push(row);
                }
            }
        }
        {
            let mut view = catalog.dbc_view("SkillTiers.dbc")?;
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                let mut max = [0; 16];
                for (index, value) in max.iter_mut().enumerate() {
                    *value = dbc_word(&row, 17 + index as u32)?;
                }
                inputs.tiers.insert(dbc_word(&row, 0)?, max);
            }
        }
        {
            // SpellMgr.cpp:2340-2357 indexes effective ability rows in DBC key order.
            let mut view = catalog.dbc_view("SkillLineAbility.dbc")?;
            let mut by_key = BTreeMap::new();
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                by_key.insert(
                    dbc_word(&row, 0)?,
                    (
                        dbc_word(&row, 1)? as u16,
                        SkillAbility {
                            spell: dbc_word(&row, 2)?,
                            race: dbc_word(&row, 3)?,
                            class: dbc_word(&row, 4)?,
                            required: dbc_word(&row, 7)?,
                            learn_on_skill: dbc_word(&row, 9)?,
                        },
                    ),
                );
            }
            for (skill, row) in by_key.into_values() {
                inputs.abilities.entry(skill).or_default().push(row);
            }
        }
        Ok(inputs)
    }

    // ObjectMgr.cpp:3225-3330. Northshire A proxy-requests.jsonl:534-535
    // proves the unordered query's MyISAM primary-key scan, not the F2 ordinal order.
    pub fn creation_rows(
        &self,
        mut rows: Vec<CreationSkillRow>,
    ) -> BTreeMap<(u8, u8), Vec<PlayerCreateSkill>> {
        rows.sort_unstable_by_key(|row| (row.race, row.class, row.skill.id));
        let mut skills = BTreeMap::<_, Vec<_>>::new();
        for row in rows {
            if row.skill.step > 16
                || (row.race != 0 && row.race & 0xff == 0)
                || (row.class != 0 && row.class & 0x5df == 0)
                || !self.categories.contains_key(&row.skill.id)
            {
                continue;
            }
            let Some(entries) = self.race_class.get(&row.skill.id) else {
                continue;
            };
            for race in (1..=8).filter(|&race| mask_applies(row.race, race)) {
                for class in (1..=11)
                    .filter(|&class| playable_class(class) && mask_applies(row.class, class))
                {
                    if entries.iter().any(|entry| {
                        entry.race & flag(race) != 0
                            && entry.class & flag(class) != 0
                            && (row.skill.step == 0
                                || self
                                    .tiers
                                    .get(&entry.tier)
                                    .is_some_and(|max| max[usize::from(row.skill.step - 1)] != 0))
                    }) {
                        skills.entry((race, class)).or_default().push(row.skill);
                    }
                }
            }
        }
        skills
    }

    // ObjectMgr.cpp:8641-8672; SpellMgr.cpp:30-40; SpellMgr.h:1926-1934.
    fn range(&self, skill: u16, racial: bool) -> SkillRange {
        match self.categories.get(&skill).copied() {
            Some(10) => SkillRange::Language,
            Some(6) if skill != 473 => SkillRange::Level,
            Some(7 | 8) if matches!(skill, 40 | 633) => SkillRange::Level,
            Some(6..=8) => SkillRange::Mono,
            Some(11) => SkillRange::Rank,
            Some(9) if matches!(skill, 129 | 185 | 356 | 762) => SkillRange::Rank,
            Some(9) if !racial => SkillRange::Mono,
            _ => SkillRange::None,
        }
    }
}

fn flag(value: u8) -> u32 {
    1u32 << (value - 1)
}
fn mask_applies(mask: u32, value: u8) -> bool {
    mask == 0 || mask & flag(value) != 0
}

impl SourcePlayer {
    /// Client skill slots in insertion order. Each record owns its current field values.
    pub fn skills(&self) -> &[SourcePlayerSkill] {
        &self.skills
    }

    // Player.cpp:5813-5883. Captured fresh player: the level is one and form is zero.
    pub(super) fn learn_default_skills(
        &mut self,
        objects: &SourceObjects,
        unit: &SourceUnitFields,
        rows: &PlayerCreateRows,
        class: u8,
    ) -> Result<()> {
        let inputs = rows
            .skill_inputs
            .as_ref()
            .ok_or_else(|| refuse("execution.player.Player_850.learn_default_skills_owner"))?;
        let race = self.race;
        let max_for_level = 5; // Unit.h:1454; Player_825 has already checked level one.
        for row in rows.skills(race, class) {
            if self.skills.iter().any(|skill| skill.id_step[0] == row.id) {
                continue;
            }
            let (value, max, step) = match inputs.range(row.id, false) {
                SkillRange::Language => (300, 300, 0),
                SkillRange::Level => (1, max_for_level, 0),
                SkillRange::Mono | SkillRange::Rank => {
                    let step = row.step.max(1);
                    let index = usize::from(step - 1);
                    let entries = inputs.race_class.get(&row.id).into_iter().flatten();
                    let values = entries
                        .filter(|entry| {
                            entry.race & flag(race) != 0 && entry.class & flag(class) != 0
                        })
                        .find_map(|entry| {
                            let maximized = entry.flags & 0x10 != 0;
                            if entry.tier != 0 {
                                let max = *inputs.tiers.get(&entry.tier)?.get(index)? as u16;
                                Some((if maximized { max } else { 1 }, max, step))
                            } else if maximized {
                                Some((max_for_level, max_for_level, step))
                            } else {
                                None
                            }
                        });
                    let Some(values) = values else {
                        continue;
                    };
                    values
                }
                SkillRange::None => (0, 0, 0),
            };
            self.set_skill(
                objects,
                unit,
                inputs,
                class,
                SourcePlayerSkill {
                    id_step: [row.id, step],
                    value_max: [value, max],
                },
            )?;
        }
        Ok(())
    }

    // Player.cpp:5408-5464 and :5365-5405, fresh insertion and value update.
    fn set_skill(
        &mut self,
        objects: &SourceObjects,
        unit: &SourceUnitFields,
        inputs: &PlayerSkillInputs,
        class: u8,
        skill: SourcePlayerSkill,
    ) -> Result<()> {
        let [id, _] = skill.id_step;
        let [value, _] = skill.value_max;
        if id == 0 {
            return Ok(());
        }
        require(
            !self.skills.iter().any(|entry| entry.id_step[0] == id),
            "execution.player.Player_5417.existing_skill_owner",
        )?;
        if value == 0
            || !inputs.categories.contains_key(&id)
            || self.skills.len() == PLAYER_MAX_SKILLS
        {
            return Ok(());
        }
        let object = self.object();
        let field = PLAYER_SKILL_INFO + self.skills.len() * 3;
        self.skills.push(SourcePlayerSkill::default());
        let entry = self
            .skills
            .last_mut()
            .ok_or_else(|| refuse("execution.player.Player_5442.skill_slot"))?;
        // SetUInt32Value marks the paired ID/step and value/max words independently.
        objects.set_update_value(object, field, &mut entry.id_step, skill.id_step)?;
        objects.set_update_value(object, field + 1, &mut entry.value_max, skill.value_max)?;
        self.update_skill_trained_spells(objects, inputs, class, skill)?;
        require(
            id != 95,
            "execution.player.Player_5404.defense_bonuses_owner",
        )?;
        // The source resets bonus words after SetSkill, then reapplies matching auras.
        require(
            !unit.auras.aura_type_ids().any(|id| matches!(id, 30 | 98)),
            "execution.player.Player_5458.apply_skill_aura_owner",
        )
    }

    // Player.cpp:5642-5698. The spell operation must own validation and side effects.
    fn update_skill_trained_spells(
        &self,
        objects: &SourceObjects,
        inputs: &PlayerSkillInputs,
        class: u8,
        skill: SourcePlayerSkill,
    ) -> Result<()> {
        let ability = inputs
            .abilities
            .get(&skill.id_step[0])
            .into_iter()
            .flatten()
            .find(|ability| {
                mask_applies(ability.race, self.race) && mask_applies(ability.class, class)
            });
        let Some(ability) = ability else {
            return Ok(());
        };
        if ability.learn_on_skill == 0 {
            return Err(refuse(
                "execution.player.Player_5673.learn_skill_spell_store_owner",
            ));
        }
        if u32::from(skill.value_max[0]) < ability.required {
            return Err(refuse("execution.player.Player_5691.remove_spell_owner"));
        }
        let owner = if objects.get(self.object())?.in_world {
            "execution.player.Player_5696.learn_spell_owner"
        } else {
            "execution.player.Player_5694.add_spell_owner"
        };
        Err(refuse(&format!("{owner}.{}", ability.spell)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_skills_follow_the_recorded_primary_key_scan() {
        let mut inputs = PlayerSkillInputs::default();
        for id in [8, 95, 162] {
            inputs.categories.insert(id, 6);
            inputs.race_class.insert(
                id,
                vec![SkillRaceClass {
                    race: 1,
                    class: 128,
                    flags: 0,
                    tier: 0,
                }],
            );
        }
        let entries = [(0, 128, 8), (0, 0, 162), (0, 0, 95)];
        let out = inputs.creation_rows(
            entries
                .into_iter()
                .map(|(race, class, id)| CreationSkillRow {
                    race,
                    class,
                    skill: PlayerCreateSkill { id, step: 0 },
                })
                .collect(),
        );
        assert_eq!(
            out[&(1, 8)]
                .iter()
                .map(|skill| skill.id)
                .collect::<Vec<_>>(),
            [95, 162, 8]
        );
        assert!(!out.contains_key(&(1, 1)));
    }
}
