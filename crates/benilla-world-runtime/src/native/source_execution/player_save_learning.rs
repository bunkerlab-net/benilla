// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{player_skills::SkillState, session::TutorialState, *};
use crate::native::source_execution::source_rows::{
    ActionRow, RowDelete, RowInsert, RowOperation, RowUpdate, SkillRow, TutorialRow,
};

impl SourcePlayer {
    /// Player.cpp:16252-16279. Dirty spell state is retained until publication.
    pub fn save_spells(&self) -> Result<()> {
        use SourcePlayerSpellState::{Changed, New, Removed};
        for spell in self.spells.values() {
            // Player.cpp:16264-16269. The dependent filter applies only to inserts.
            require(
                !matches!(spell.state, Removed | Changed),
                "execution.player_save.Player_16265.spell_delete_owner",
            )?;
            require(
                spell.dependent || spell.state != New,
                "execution.player_save.Player_16269.spell_insert_owner",
            )?;
        }
        Ok(())
    }

    /// Player.cpp:3679-3716. GCD and school lockouts are outside the spell map.
    pub fn save_spell_cooldowns(
        &self,
        objects: &SourceObjects,
        units: &SourceUnits,
        guid: u32,
        clock: &mut impl SpellCooldownClock,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        observe(SourcePlayerSaveObservation::Row(RowOperation::Delete(
            RowDelete::CooldownsByGuid(guid),
        )))?;
        let now = clock.map_clock()?;
        for entry in units.get(objects, self.object)?.cooldowns.spells.values() {
            if !entry.permanent && (entry.spell_expiry > now || entry.category_expiry > now) {
                return Err(refuse(
                    "execution.player_save.Player_3707.cooldown_insert_owner",
                ));
            }
        }
        Ok(())
    }

    /// Player.cpp:15806-15854. Button keys are in numeric order.
    pub fn save_actions(
        &self,
        guid: u32,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        use SourcePlayerActionState::{Changed, Deleted, New, Unchanged};
        for (&button, action) in &self.action_buttons {
            let op = match action.state {
                Unchanged => continue,
                Deleted => RowOperation::Delete(RowDelete::Action { guid, button }),
                New | Changed => {
                    let (action_id, kind) = action.action_and_type();
                    let row = ActionRow {
                        guid,
                        button,
                        action: action_id,
                        kind,
                    };
                    if action.state == New {
                        RowOperation::Insert(RowInsert::Action(row))
                    } else {
                        RowOperation::Update(RowUpdate::Action(row))
                    }
                }
            };
            observe(SourcePlayerSaveObservation::Row(op))?;
        }
        Ok(())
    }

    /// Player.cpp:15856-15923. A skipped holder or effect leaves only the delete.
    pub fn save_auras(
        &self,
        objects: &SourceObjects,
        units: &SourceUnits,
        spells: &SourceSpellStores,
        guid: u32,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        observe(SourcePlayerSaveObservation::Row(RowOperation::Delete(
            RowDelete::AurasByGuid(guid),
        )))?;
        for (id, holder) in units.get(objects, self.object)?.auras.aura_holders() {
            // SpellAuras.cpp:5868-5895; SpellMgr.h:1371-1373; SpellDefines.h:68,72,381.
            if holder.passive {
                continue;
            }
            let spell = spells.template(id.spell()).ok_or_else(|| {
                refuse("execution.player_save.SpellAuras_5870.spell_template_owner")
            })?;
            if spell.attributesex & 0x44 != 0
                || holder.tracked
                || spell.aurainterruptflags & 0x0008_0000 != 0
                || (holder.permanent
                    && matches!(
                        id.spell(),
                        1386 | 11413..=11415 | 29520 | 30682 | 30683 | 30880
                    ))
            {
                continue;
            }
            // SpellAuras.cpp:666-674. Area effects must be self-cast; persistent effects never save.
            if holder.effects.iter().flatten().any(|effect| {
                !effect.persistent && (effect.area.is_none() || holder.caster == self.object)
            }) {
                return Err(refuse(
                    "execution.player_save.Player_15904.aura_insert_owner",
                ));
            }
        }
        Ok(())
    }

    /// Player.cpp:16188-16250. Visit the live status map, then the forgotten map.
    pub fn save_skills(
        &self,
        guid: u32,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        for (id, status) in self.skill_status.iter() {
            let skill = id as u32;
            let op = match status.state {
                SkillState::Unchanged => continue,
                SkillState::Deleted => RowOperation::Delete(RowDelete::Skill { guid, skill }),
                SkillState::New | SkillState::Changed => {
                    let [value, max] = self.skills[status.slot].value_max;
                    let row = SkillRow {
                        guid,
                        skill,
                        value,
                        max,
                    };
                    if status.state == SkillState::New {
                        RowOperation::Insert(RowInsert::Skill(row))
                    } else {
                        RowOperation::Update(RowUpdate::Skill(row))
                    }
                }
            };
            observe(SourcePlayerSaveObservation::Row(op))?;
        }
        // Player.cpp:16242-16247 saves only forgotten values greater than one.
        require(
            !self.forgotten_skills.values().any(|&value| value > 1),
            "execution.player_save.Player_16246.forgotten_skill_owner",
        )
    }

    /// Player.cpp:20472-20490 saves the current map without filtering its timestamps.
    pub fn save_entered_instance_timers(
        &self,
        session: &SourceWorldSession,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        observe(SourcePlayerSaveObservation::Row(RowOperation::Delete(
            RowDelete::EnteredInstancesByAccount(session.account_id()),
        )))?;
        require(
            self.entered_instances.first_key().is_none(),
            "execution.player_save.Player_20482.instance_timer_insert_owner",
        )
    }

    /// Player.cpp:20446-20450. Re-entering an instance does not extend its timer.
    pub fn add_new_instance_id(
        &mut self,
        instance: u32,
        wall_ms: impl FnOnce() -> Result<i64>,
    ) -> Result<()> {
        let key = u64::from(instance);
        if self.entered_instances.get(key).is_none() {
            let expiry = wall_ms()?.checked_add(3_600_000).ok_or_else(|| {
                refuse("execution.player.Player_20449.instance_timer_add_undefined")
            })?;
            self.entered_instances.insert(key, expiry, usize::MAX)?;
        }
        Ok(())
    }

    /// Player.cpp:20493-20502. Keep entries at the exact expiry time until the next walk.
    pub fn update_new_instance_id_timers(&mut self, now: i64) {
        let mut cursor = self.entered_instances.first_key();
        while let Some(id) = cursor {
            cursor = self.entered_instances.next_key(id);
            if self
                .entered_instances
                .get(id)
                .is_some_and(|&expiry| expiry < now)
            {
                self.entered_instances.erase(id);
            }
        }
    }
}

impl SourceWorldSession {
    /// WorldSession.cpp:1150-1184. Retain dirty state until publication.
    pub fn save_tutorials_data(
        &self,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        let row = TutorialRow {
            account: u64::from(self.account_id()),
            words: self.tutorial_words,
        };
        let op = match self.tutorial_state {
            TutorialState::Unchanged => return Ok(()),
            TutorialState::Changed => RowOperation::Update(RowUpdate::Tutorial(row)),
            TutorialState::New => RowOperation::Insert(RowInsert::Tutorial(row)),
        };
        observe(SourcePlayerSaveObservation::Row(op))
    }
}

#[cfg(test)]
#[path = "player_save_learning_tests.rs"]
pub(super) mod tests;
