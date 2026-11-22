// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Player::UpdateSkillsForLevel/GetSkillInfo/existing SetSkill branch.
use super::{Error, Event, Host, Result, Rows, SkillLineRow, SkillRaceClassRow, Staged};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SkillStatus {
    Unchanged,
    Changed,
    New,
    Deleted,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillState {
    pub id: u16,
    pub position: u8,
    pub value: u16,
    pub maximum: u16,
    pub step: u16,
    pub permanent_bonus: i16,
    pub temporary_bonus: i16,
    pub status: SkillStatus,
}
/// One fully resolved lookup. `None` means proven absent SkillLine row; absence
/// of the entire SkillDependency is an error. Race/class info must be complete,
/// including a proven empty vector, and sorted by native DBC row ID here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillDependency {
    pub skill: u16,
    pub line: Option<SkillLineRow>,
    pub race_class: Rows<SkillRaceClassRow>,
}
impl SkillDependency {
    pub(super) fn validate(&self) -> Result<()> {
        if self.skill == 0 || self.line.is_some_and(|l| l.id != self.skill) {
            return Err(Error::Invalid("skill dependency ID"));
        }
        let Rows::Complete(rows) = &self.race_class else {
            return Err(Error::Missing {
                table: "SkillRaceClassInfo",
                id: self.skill.into(),
            });
        };
        let mut ids = BTreeSet::new();
        for row in rows {
            if row.skill != self.skill || !ids.insert(row.row_id) {
                return Err(Error::Invalid("SkillRaceClassInfo row"));
            }
        }
        Ok(())
    }
    fn maximized(&self, race: u8, class: u8) -> bool {
        let Rows::Complete(rows) = &self.race_class else {
            unreachable!("validated dependency")
        };
        rows.iter()
            .filter(|r| {
                r.race_mask & (1 << (race - 1)) != 0 && r.class_mask & (1 << (class - 1)) != 0
            })
            .min_by_key(|r| r.row_id)
            .is_some_and(|r| r.flags & 0x10 != 0)
    }
}
fn level_or_mono(row: SkillLineRow) -> bool {
    match row.category {
        // Weapon: level except fist mono. Class/armor: mono except lockpick/poison level.
        6..=8 => true,
        // Secondary is mono unless it is a profession or riding; professions rank.
        9 => !matches!(row.id, 129 | 185 | 356 | 762),
        _ => false,
    }
}
impl<H: Host> Staged<'_, H> {
    pub(super) fn skills(&mut self, maximize: bool) -> Result<()> {
        let maximum = ((self.state.0.level * 5) as u16).min(self.catalog.rules.global_skill_max);
        for index in 0..self.state.0.skills.len() {
            let skill = &self.state.0.skills[index];
            if skill.status == SkillStatus::Deleted {
                continue;
            }
            let dep = self.catalog.skills.get(&skill.id).ok_or(Error::Missing {
                table: "complete skill dependency",
                id: skill.id.into(),
            })?;
            let Some(line) = dep.line else {
                continue;
            };
            if !level_or_mono(line) || skill.maximum == 1 {
                continue;
            }
            let maxed = maximize
                || self.catalog.rules.always_max_skill_for_level
                || dep.maximized(self.catalog.race, self.catalog.class);
            let value = if maxed { maximum } else { skill.value };
            if maximum == skill.maximum && value == skill.value {
                continue;
            }
            let skill = &mut self.state.0.skills[index];
            skill.value = value;
            skill.maximum = maximum;
            let (id, step) = (skill.id, skill.step);
            self.emit(Event::SkillValue { id, value, maximum })?;
            let skill = &mut self.state.0.skills[index];
            if skill.status != SkillStatus::New {
                skill.status = SkillStatus::Changed;
            }
            self.emit(Event::SkillTrainedSpells { id, value, step })?;
            if id == 95 {
                self.emit(Event::SkillDefenseBonuses)?;
            }
        }
        Ok(())
    }
}
