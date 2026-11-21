// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

// Conditions.h:175-181; SQLStorages.cpp:38,54 uses the int fields and skips comments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionRow {
    pub entry: u32,
    pub kind: i32,
    pub values: [u32; 4],
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionDependency {
    SpellTemplate,
    ItemPrototype,
    Area,
    Faction,
    SkillLine,
    QuestTemplate,
    GameEvent,
    LearnableAbility,
    CreatureTemplate,
    WorldStateName,
}

pub trait ConditionValidationInputs {
    /// None means the owner is unbound. Some(false) means the row is absent.
    fn exists(&mut self, _domain: ConditionDependency, _id: u32) -> Result<Option<bool>> {
        Ok(None)
    }

    fn progression(&self) -> Option<&benilla_sim::player_progression::Rules> {
        None
    }

    fn encounter_map(&mut self, _id: u32) -> Result<Option<u32>> {
        Err(refuse(
            "execution.conditions.Conditions_914.dungeon_encounter_owner",
        ))
    }
}

#[derive(Debug)]
pub struct SourceConditions {
    rows: BTreeMap<u32, ConditionRow>,
}

impl SourceConditions {
    // World.cpp:1120; ObjectMgr.cpp:5526-5543 validates in ascending storage ID order.
    pub fn load(
        rows: impl IntoIterator<Item = ConditionRow>,
        inputs: &mut impl ConditionValidationInputs,
    ) -> Result<Self> {
        let mut store = Self {
            rows: rows.into_iter().map(|row| (row.entry, row)).collect(),
        };
        let mut next = store.rows.first_key_value();
        while let Some((&id, row)) = next {
            if !store.is_valid(row, inputs)? {
                store.rows.remove(&id);
            }
            next = store.rows.range((Excluded(id), Unbounded)).next();
        }
        Ok(store)
    }

    pub fn get(&self, id: u32) -> Option<&ConditionRow> {
        self.rows.get(&id)
    }

    pub fn rows(&self) -> impl ExactSizeIterator<Item = &ConditionRow> {
        self.rows.values()
    }

    // Conditions.cpp:600-1000 validates the data before it can be looked up.
    fn is_valid(
        &self,
        row: &ConditionRow,
        inputs: &mut impl ConditionValidationInputs,
    ) -> Result<bool> {
        use ConditionDependency::*;
        let [a, b, c, d] = row.values;
        let mut exists =
            |domain, id, refusal| inputs.exists(domain, id)?.ok_or_else(|| refuse(refusal));
        Ok(match row.kind {
            -3 => a < row.entry && self.get(a).is_some(), // :604-618
            -2 | -1 => {
                a < row.entry
                    && b < row.entry
                    && self.get(a).is_some()
                    && self.get(b).is_some()
                    && (c == 0 || c < row.entry && self.get(c).is_some())
                    && (d == 0 || d < row.entry && self.get(d).is_some())
            }
            1 => {
                exists(
                    SpellTemplate,
                    a,
                    "execution.conditions.Conditions_676.spell_template_owner",
                )? && b < 3
            }
            2 | 23 => {
                exists(
                    ItemPrototype,
                    a,
                    "execution.conditions.Conditions_691.item_prototype_owner",
                )? && b >= 1
            }
            3 => exists(
                ItemPrototype,
                a,
                "execution.conditions.Conditions_707.item_prototype_owner",
            )?,
            4 => exists(Area, a, "execution.conditions.Conditions_717.area_owner")? && b <= 1,
            5 | 30 => {
                exists(
                    Faction,
                    a,
                    "execution.conditions.Conditions_734.faction_owner",
                )? && b < 8
            }
            6 => matches!(a, 469 | 67), // :748-756
            7 | 29 => {
                exists(
                    SkillLine,
                    a,
                    "execution.conditions.Conditions_760.skill_line_owner",
                )? && b >= 1
                    && b <= u32::from(
                        inputs
                            .progression()
                            .ok_or_else(|| {
                                refuse("execution.conditions.Conditions_766.max_skill_owner")
                            })?
                            .global_skill_max,
                    )
            }
            8 | 9 | 19 | 22 => exists(
                QuestTemplate,
                a,
                "execution.conditions.Conditions_778.quest_template_owner",
            )?,
            10 | 11 => true, // :789-804 only logs unused or out-of-range values.
            12 => exists(
                GameEvent,
                u32::from(a as u16),
                "execution.conditions.Conditions_807.game_event_owner",
            )?,
            13 => a != 0 || b != 0, // :814-822
            // Conditions.cpp:823-843; SharedDefines.h:52-58,91-97.
            14 => (a != 0 || b != 0) && (a == 0 || a & 0xff != 0) && (b == 0 || b & 0x5df != 0),
            15 => {
                a != 0
                    && a <= inputs
                        .progression()
                        .ok_or_else(|| {
                            refuse("execution.conditions.Conditions_846.max_player_level_owner")
                        })?
                        .max_player_level
                    && b <= 2
            }
            17 => {
                exists(
                    SpellTemplate,
                    a,
                    "execution.conditions.Conditions_862.spell_template_owner",
                )? && b <= 1
            }
            28 => {
                exists(
                    LearnableAbility,
                    a,
                    "execution.conditions.Conditions_878.skill_line_ability_owner",
                )? && (b == 0
                    || exists(
                        ItemPrototype,
                        b,
                        "execution.conditions.Conditions_888.item_prototype_owner",
                    )?)
            }
            37 => {
                exists(
                    CreatureTemplate,
                    a,
                    "execution.conditions.Conditions_900.creature_template_owner",
                )? && b != 0
            }
            31 => {
                let first = inputs.encounter_map(a)?;
                let second = inputs.encounter_map(b)?;
                first.is_some()
                    && (b == 0 || second.is_some())
                    && (second.is_none() || first == second)
            }
            33 => b <= 2, // :933-941
            35 => a < 3,  // :942-950
            36 => a < 4,  // :951-959
            39 => exists(
                CreatureTemplate,
                a,
                "execution.conditions.Conditions_962.creature_template_owner",
            )?,
            0 | 18 | 26 | 40 => true, // :969-973
            42 => {
                b <= 7
                    && exists(
                        WorldStateName,
                        a,
                        "execution.conditions.Conditions_982.world_state_name_owner",
                    )?
            }
            43 => a <= 1, // :988-994
            _ => false,
        })
    }

    // Conditions.cpp:1003-1021,54-106 checks both OR/AND operands in order.
    pub fn can_be_used_without_player(&self, mut id: u32) -> bool {
        let mut pending = Vec::new();
        loop {
            let Some(row) = self.get(id) else {
                return false;
            };
            match row.kind {
                -3 => id = row.values[0],
                -2 | -1 => {
                    pending.push(row.values[1]);
                    id = row.values[0];
                }
                2 | 3 | 6..=10 | 17 | 19 | 22 | 23 | 28..=30 => return false,
                _ => match pending.pop() {
                    Some(next) => id = next,
                    None => return true,
                },
            }
        }
    }
}

#[cfg(test)]
#[path = "conditions_tests.rs"]
mod tests;
