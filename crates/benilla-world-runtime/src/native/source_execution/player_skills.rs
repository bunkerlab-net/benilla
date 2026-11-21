// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{player_rows::dbc_word, *};
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeMap;

// Player.h:76; UpdateFields.h:216.
const PLAYER_MAX_SKILLS: usize = 127;
const PLAYER_SKILL_INFO: usize = 0x2ce;
// Unit.h:1454 and Player.cpp:825. Creation currently binds level one.
const MAX_SKILL_FOR_LEVEL: u16 = 5;
pub(super) const SKILL_DEFENSE: u16 = 95; // SharedDefines.h:1335.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourcePlayerSkill {
    pub id_step: [u16; 2],
    pub value_max: [u16; 2],
    /// Player.cpp:89-91. Temporary in the low half and permanent in the high half.
    pub bonus: [i16; 2],
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
    skill: u16,
    spell: u32,
    race: u32,
    class: u32,
    required: u32,
    successor: u32,
    learn_on_skill: u32,
    max_value: u32,
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
    abilities: Vec<SkillAbility>,
    by_skill: BTreeMap<u16, Vec<usize>>,
    by_spell: BTreeMap<u32, Vec<usize>>,
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
                    SkillAbility {
                        skill: dbc_word(&row, 1)? as u16,
                        spell: dbc_word(&row, 2)?,
                        race: dbc_word(&row, 3)?,
                        class: dbc_word(&row, 4)?,
                        required: dbc_word(&row, 7)?,
                        successor: dbc_word(&row, 8)?,
                        learn_on_skill: dbc_word(&row, 9)?,
                        max_value: dbc_word(&row, 10)?,
                    },
                );
            }
            inputs.abilities.reserve(by_key.len());
            for row in by_key.into_values() {
                inputs.index_ability(row);
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

    fn index_ability(&mut self, row: SkillAbility) {
        let index = self.abilities.len();
        self.by_skill.entry(row.skill).or_default().push(index);
        self.by_spell.entry(row.spell).or_default().push(index);
        self.abilities.push(row);
    }

    fn abilities<'a, K: Ord>(
        &'a self,
        index: &'a BTreeMap<K, Vec<usize>>,
        key: K,
    ) -> impl Iterator<Item = &'a SkillAbility> {
        index
            .get(&key)
            .into_iter()
            .flatten()
            .map(|&i| &self.abilities[i])
    }

    // SpellMgr.h:2285-2297 takes the first nonzero forward spell by DBC key.
    pub(super) fn spellbook_successor(&self, spell: u32) -> Option<u32> {
        self.abilities(&self.by_spell, spell)
            .find_map(|row| (row.successor != 0).then_some(row.successor))
    }

    // Player.cpp:5887-5891. The first DBC ability defines the rank's skill.
    fn rank_skill(&self, spell: u32) -> Option<u16> {
        self.abilities(&self.by_spell, spell)
            .next()
            .map(|row| row.skill)
    }

    // SpellMgr.cpp:30-39; SharedDefines.h:1484.
    pub(super) fn is_primary_profession(&self, skill: u32) -> bool {
        u16::try_from(skill)
            .ok()
            .and_then(|id| self.categories.get(&id))
            == Some(&11)
    }

    // Player.cpp:5327-5351. Both masks must match and the first eligible row wins.
    fn skill_info(&self, skill: u16, race: u8, class: u8) -> impl Iterator<Item = &SkillRaceClass> {
        self.race_class
            .get(&skill)
            .into_iter()
            .flatten()
            .filter(move |entry| entry.race & flag(race) != 0 && entry.class & flag(class) != 0)
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

    // Player.h:1695; Player.cpp:5466-5494.
    pub(super) fn skill_value(&self, id: u16) -> u16 {
        self.skills
            .iter()
            .find(|skill| skill.id_step[0] == id)
            .map_or(0, |skill| {
                let [temp, perm] = skill.bonus;
                let value = i32::from(skill.value_max[0]) + i32::from(perm) + i32::from(temp);
                value.max(0) as u16
            })
    }

    // Player.cpp:5885-5897.
    pub(super) fn spell_rank(&self, inputs: &PlayerSkillInputs, t: &SpellTemplate) -> Result<u32> {
        let Some(id) = inputs.rank_skill(t.id) else {
            return Ok(0);
        };
        let rank = u32::from(self.skill_value(id));
        Ok(if t.maxlevel > 0 {
            rank.min(t.maxlevel.wrapping_mul(5))
        } else {
            rank
        })
    }

    // Player.cpp:5813-5883. Captured fresh player: the level is one and form is zero.
    pub(super) fn learn_default_skills(
        &mut self,
        live: &mut PlayerLearning<'_>,
        rows: &[PlayerCreateSkill],
    ) -> Result<()> {
        let inputs = live.skills;
        let race = self.race;
        let class = live.class;
        let max_for_level = MAX_SKILL_FOR_LEVEL;
        for row in rows {
            if self.skills.iter().any(|skill| skill.id_step[0] == row.id) {
                continue;
            }
            let (value, max, step) = match inputs.range(row.id, false) {
                SkillRange::Language => (300, 300, 0),
                SkillRange::Level => (1, max_for_level, 0),
                SkillRange::Mono | SkillRange::Rank => {
                    let step = row.step.max(1);
                    let index = usize::from(step - 1);
                    let values = inputs.skill_info(row.id, race, class).find_map(|entry| {
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
                live,
                SourcePlayerSkill {
                    id_step: [row.id, step],
                    value_max: [value, max],
                    bonus: [0; 2],
                },
            )?;
        }
        Ok(())
    }

    // Player.cpp:5408-5464 and :5365-5405. Use the same slots for re-entry.
    fn set_skill(&mut self, live: &mut PlayerLearning<'_>, skill: SourcePlayerSkill) -> Result<()> {
        let [id, step] = skill.id_step;
        let [value, _] = skill.value_max;
        if id == 0 {
            return Ok(());
        }
        let (index, inserted) =
            if let Some(index) = self.skills.iter().position(|entry| entry.id_step[0] == id) {
                require(
                    value != 0,
                    "execution.player.Player_5388.remove_skill_owner",
                )?;
                (index, false)
            } else {
                if value == 0
                    || !live.skills.categories.contains_key(&id)
                    || self.skills.len() == PLAYER_MAX_SKILLS
                {
                    return Ok(());
                }
                let index = self.skills.len();
                self.skills.push(SourcePlayerSkill::default());
                (index, true)
            };
        let object = self.object();
        let field = PLAYER_SKILL_INFO + index * 3;
        let entry = &mut self.skills[index];
        if inserted || step != 0 {
            live.objects
                .set_update_value(object, field, &mut entry.id_step, skill.id_step)?;
        }
        live.objects
            .set_update_value(object, field + 1, &mut entry.value_max, skill.value_max)?;
        let current_step = entry.id_step[1];
        self.update_skill_trained_spells(live, id, value, current_step)?;
        if id == SKILL_DEFENSE {
            let unit = live.units.get_mut(live.objects, object)?;
            self.update_defense_bonuses(live.objects, unit)?;
        }
        if inserted {
            // Player.cpp:5452 resets the live bonus word before applying auras.
            live.objects.set_update_value(
                object,
                field + 2,
                &mut self.skills[index].bonus,
                [0; 2],
            )?;
            require(
                !live
                    .units
                    .get(live.objects, object)?
                    .auras
                    .aura_holders()
                    .any(|(_, holder)| {
                        holder
                            .effects
                            .iter()
                            .flatten()
                            .any(|aura| matches!(aura.kind, 30 | 98) && aura.misc == i32::from(id))
                    }),
                "execution.player.Player_5458.apply_skill_aura_owner",
            )?;
        }
        Ok(())
    }

    // Player.cpp:5642-5698. Equal-key abilities follow the DBC key order.
    fn update_skill_trained_spells(
        &mut self,
        live: &mut PlayerLearning<'_>,
        id: u16,
        value: u16,
        step: u16,
    ) -> Result<()> {
        for ability in live.skills.abilities(&live.skills.by_skill, id) {
            if !mask_applies(ability.race, self.race) || !mask_applies(ability.class, live.class) {
                continue;
            }
            if ability.learn_on_skill == 0 {
                let Some((trained, trained_step, effect)) =
                    live.spells.learned_skill(ability.spell)
                else {
                    continue;
                };
                // Player.cpp:5673-5682 repairs a missing original profession spell.
                if trained == id
                    && effect == 118
                    && trained_step < step
                    && !self.has_spell(ability.spell)
                {
                    self.learn_granted_spell(live, ability.spell)?;
                }
                continue;
            }
            if u32::from(value) < ability.required {
                // Player.cpp:3483-3485. An absent row requires no removal owner.
                require(
                    !self.spells.contains_key(&ability.spell),
                    "execution.player.Player_5691.remove_spell_owner",
                )?;
            } else {
                self.learn_granted_spell(live, ability.spell)?;
            }
        }
        Ok(())
    }

    // Player.cpp:5701-5777. The addSpell path applies training; removal has a separate owner.
    pub(super) fn update_spell_trained_skills(
        &mut self,
        live: &mut PlayerLearning<'_>,
        spell: u32,
    ) -> Result<()> {
        if let Some((id, step, _)) = live.spells.learned_skill(spell) {
            return self.set_skill_step(live, id, step);
        }
        for ability in live.skills.abilities(&live.skills.by_spell, spell) {
            let id = ability.skill;
            let trains_skill = if matches!(id, 40 | 633) {
                ability.max_value == 0
            } else {
                ability.learn_on_skill == 2
            };
            if !live.skills.categories.contains_key(&id)
                || !trains_skill
                || self.skills.iter().any(|slot| slot.id_step[0] == id)
            {
                continue;
            }
            let Some(row) = live.skills.skill_info(id, self.race, live.class).next() else {
                continue;
            };
            // Player.cpp:5761-5767. No forgotten weapon skill can exist on a fresh player.
            let (value, max) = match live.skills.range(id, ability.race != 0) {
                SkillRange::Language => (300, 300),
                SkillRange::Level => (1, MAX_SKILL_FOR_LEVEL),
                SkillRange::Mono if row.flags & 0x10 != 0 => {
                    (MAX_SKILL_FOR_LEVEL, MAX_SKILL_FOR_LEVEL)
                }
                SkillRange::Mono => (1, 1),
                SkillRange::None | SkillRange::Rank => continue,
            };
            self.set_skill(
                live,
                SourcePlayerSkill {
                    id_step: [id, 0],
                    value_max: [value, max],
                    bonus: [0; 2],
                },
            )?;
        }
        Ok(())
    }

    // Player.cpp:5497-5535. A loaded learn-skill edge sets the same live skill slots.
    fn set_skill_step(&mut self, live: &mut PlayerLearning<'_>, id: u16, step: u16) -> Result<()> {
        if id == 0 || step > 16 {
            return Ok(());
        }
        let (value, max) = if step == 0 {
            (0, 0)
        } else {
            let Some((row, max)) =
                live.skills
                    .skill_info(id, self.race, live.class)
                    .find_map(|row| {
                        if row.tier == 0 {
                            return None;
                        }
                        let max = *live
                            .skills
                            .tiers
                            .get(&row.tier)?
                            .get(usize::from(step - 1))? as u16;
                        Some((row, max))
                    })
            else {
                return Ok(());
            };
            if max == 0 {
                return Ok(());
            }
            let value = if row.flags & 0x10 != 0 {
                max
            } else {
                self.skills
                    .iter()
                    .find(|slot| slot.id_step[0] == id)
                    .map_or(1, |slot| slot.value_max[0].max(1))
            };
            (value, max)
        };
        self.set_skill(
            live,
            SourcePlayerSkill {
                id_step: [id, step],
                value_max: [value, max],
                bonus: [0; 2],
            },
        )
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

#[cfg(test)]
mod learning_tests {
    use super::super::tests::{construct, test_items};
    use super::*;

    fn ability(skill: u16, spell: u32, successor: u32, required: u32) -> SkillAbility {
        SkillAbility {
            skill,
            spell,
            race: 1,
            class: 1 << 7,
            required,
            successor,
            learn_on_skill: 2,
            max_value: 0,
        }
    }

    #[test]
    fn spell_training_reenters_the_same_skill_slot_and_processes_each_ability() {
        let mut skills = PlayerSkillInputs::default();
        skills.categories.insert(10, 6);
        skills.race_class.insert(
            10,
            vec![SkillRaceClass {
                race: 1,
                class: 1 << 7,
                flags: 0,
                tier: 1,
            }],
        );
        skills.tiers.insert(1, [75; 16]);
        skills.index_ability(ability(10, 2, 0, 1));
        skills.index_ability(ability(10, 3, 0, 1));
        skills.index_ability(ability(10, 4, 0, 2));

        let mut spells = SourceSpellStores::default();
        spells
            .insert_template(SpellTemplate {
                id: 1,
                effect1: 118,
                effectmiscvalue1: 10,
                effectbasepoints1: 1,
                ..Default::default()
            })
            .unwrap();
        for id in 2..=4 {
            spells
                .insert_template(SpellTemplate {
                    id,
                    ..Default::default()
                })
                .unwrap();
        }
        spells.bind_test_learning();
        let items = test_items([]);
        let mut objects = SourceObjects::default();
        let mut units = SourceUnits::default();
        let mut player = construct(&mut objects, &mut units);
        player.race = 1;
        let mut live = PlayerLearning {
            objects: &objects,
            units: &mut units,
            cast: None,
            skills: &skills,
            spells: &spells,
            items: &items,
            class: 8,
        };
        player
            .add_spell(&mut live, 1, true, true, false, false)
            .unwrap();
        let skill = |step, value| SourcePlayerSkill {
            id_step: [10, step],
            value_max: [value, 75],
            bonus: [0; 2],
        };
        assert_eq!(player.skills(), &[skill(1, 1)]);
        assert_eq!(
            player.spellbook().keys().copied().collect::<Vec<_>>(),
            [1, 2, 3]
        );

        player.set_skill(&mut live, skill(0, 2)).unwrap();
        assert_eq!(player.skills(), &[skill(1, 2)]);
        assert!(player.spellbook()[&4].dependent);
        assert_eq!(
            player.set_skill(&mut live, skill(0, 1)).unwrap_err().id,
            "execution.player.Player_5691.remove_spell_owner"
        );
        assert_eq!(player.skills(), &[skill(1, 1)]);
        assert_eq!(
            player.set_skill(&mut live, skill(0, 0)).unwrap_err().id,
            "execution.player.Player_5388.remove_skill_owner"
        );
        assert_eq!(player.skills(), &[skill(1, 1)]);
    }

    #[test]
    fn successor_replaces_the_detached_rank_and_refuses_in_world_packets() {
        let mut skills = PlayerSkillInputs::default();
        skills.index_ability(ability(10, 1, 2, 0));
        let mut spells = SourceSpellStores::default();
        for id in 1..=2 {
            spells
                .insert_template(SpellTemplate {
                    id,
                    ..Default::default()
                })
                .unwrap();
        }
        let items = test_items([]);

        for in_world in [false, true] {
            let mut objects = SourceObjects::default();
            let mut units = SourceUnits::default();
            let mut player = construct(&mut objects, &mut units);
            let handle = player.object();
            objects.get_mut(handle).unwrap().in_world = in_world;
            let mut live = PlayerLearning {
                objects: &objects,
                units: &mut units,
                cast: None,
                skills: &skills,
                spells: &spells,
                items: &items,
                class: 8,
            };
            assert!(player
                .add_spell(&mut live, 1, true, false, false, false)
                .unwrap());
            let learned = player.add_spell(&mut live, 2, true, true, true, false);
            if in_world {
                assert_eq!(
                    learned.unwrap_err().id,
                    "execution.player.Player_3339.send_superseded_spell_owner"
                );
                assert!(player.spellbook()[&1].active);
                assert!(!player.spellbook().contains_key(&2));
                player.spells.insert(2, player.spellbook()[&1]);
                assert_eq!(
                    player
                        .add_spell(&mut live, 1, true, true, false, false)
                        .unwrap_err()
                        .id,
                    "execution.player.Player_3211.send_superseded_spell_owner"
                );
                assert!(!player.spellbook()[&1].active);
            } else {
                assert!(!learned.unwrap());
                assert_eq!(
                    player.spellbook()[&1].state,
                    SourcePlayerSpellState::Changed
                );
                assert!(player.spellbook()[&2].active);
                assert!(!player.spellbook()[&1].active);
                assert!(!player
                    .add_spell(&mut live, 1, true, true, false, false)
                    .unwrap());
                assert!(!player.spellbook()[&1].active);
            }
        }
    }
}
