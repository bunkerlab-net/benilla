// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::source_rows::{RowInsert, RowOperation, RowUpdate};
use super::*;

// CharacterHandler.cpp:55-59 and Player.h:505.
const CINEMATICS_SKIP_SAME_RACE: u32 = 1;
const CINEMATICS_SKIP_ALL: u32 = 2;
const AT_LOGIN_FIRST: u32 = 0x20;

#[derive(Debug, PartialEq, Eq)]
pub enum SourcePlayerSaveObservation {
    Begin,
    Row(RowOperation),
}

pub struct SourcePlayerSaveInputs<'a, W> {
    pub objects: &'a mut SourceObjects,
    pub units: &'a mut SourceUnits,
    pub items: &'a mut SourceItems,
    pub session: &'a mut SourceWorldSession,
    pub quests: &'a SourceQuestMap,
    pub spells: &'a SourceSpellStores,
    pub clock: &'a SourceWorldClock,
    pub world: &'a SourceWorld,
    pub calendar: Calendar,
    pub maintenance_weekday: i32,
    pub save_interval: u32,
    pub world_work: &'a mut W,
    pub wall_seconds: &'a mut dyn FnMut() -> Result<i64>,
}

/// Checked publication for the sole local character.
pub trait SourcePlayerSaveStore {
    type Receipt;
    type Error;

    fn check_open(&self) -> Result<(), Self::Error>;
    fn commit_create(&mut self, rows: Vec<RowOperation>) -> Result<Self::Receipt, Self::Error>;
}

#[derive(Debug)]
pub enum SourcePlayerSaveError<E> {
    Source(Refusal),
    Storage(E),
}

pub(super) struct StagedPlayerSave {
    pub(super) guid: u32,
    pub(super) rows: Vec<RowOperation>,
    written_quests: Vec<u32>,
}

impl SourcePlayer {
    // CharacterHandler.cpp:506-509.
    pub(in crate::native::source_execution) fn set_creation_flags(
        &mut self,
        have_same_race: bool,
        skip_cinematics: u32,
    ) {
        if skip_cinematics == CINEMATICS_SKIP_ALL
            || (skip_cinematics == CINEMATICS_SKIP_SAME_RACE && have_same_race)
        {
            self.save_fields.cinematic = 1;
        }
        self.save_fields.at_login |= AT_LOGIN_FIRST; // Player.h:2093.
    }

    // Player.cpp:15779 commits the staged :15614-15777 transaction.
    pub fn save_created_to_db<S: SourcePlayerSaveStore>(
        &mut self,
        inputs: &mut SourcePlayerSaveInputs<'_, impl WorldStartupWork>,
        store: &mut S,
    ) -> Result<(S::Receipt, u32), SourcePlayerSaveError<S::Error>> {
        store.check_open().map_err(SourcePlayerSaveError::Storage)?;
        let StagedPlayerSave {
            guid,
            rows,
            written_quests,
        } = self
            .stage_save(inputs)
            .map_err(SourcePlayerSaveError::Source)?;
        let receipt = store
            .commit_create(rows)
            .map_err(SourcePlayerSaveError::Storage)?;

        // Deviation: Defer Player.cpp:15766-15777 acknowledgements because
        // a failed local publication must leave pending writes dirty.
        self.acknowledge_inventory(inputs.items);
        self.acknowledge_quest_status(written_quests);
        self.acknowledge_spells();
        self.acknowledge_actions();
        self.acknowledge_skills();
        self.acknowledge_reputation();
        self.acknowledge_honor_cp();
        inputs.session.acknowledge_tutorials();
        Ok((receipt, guid))
    }

    pub(super) fn stage_save(
        &mut self,
        inputs: &mut SourcePlayerSaveInputs<'_, impl WorldStartupWork>,
    ) -> Result<StagedPlayerSave> {
        let mut rows = Vec::new();
        let mut written_quests = Vec::new();
        let guid = self.project_save(inputs, |event| {
            if let SourcePlayerSaveObservation::Row(row) = event {
                if let RowOperation::Insert(RowInsert::Quest(quest))
                | RowOperation::Update(RowUpdate::Quest(quest)) = &row
                {
                    written_quests.push(quest.quest);
                }
                rows.push(row);
            }
            Ok(())
        })?;
        Ok(StagedPlayerSave {
            guid,
            rows,
            written_quests,
        })
    }

    // Player.cpp:15602-15777, through the last staged row.
    pub(super) fn project_save(
        &mut self,
        inputs: &mut SourcePlayerSaveInputs<'_, impl WorldStartupWork>,
        mut observe: impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<u32> {
        self.next_save_ms = inputs.save_interval;
        require(
            !self.save_fields.teleport_far,
            "execution.player_save.Player_15607.delayed_save_owner",
        )?;
        observe(SourcePlayerSaveObservation::Begin)?;
        self.update_honor(
            inputs.objects,
            inputs.world,
            inputs.calendar,
            inputs.maintenance_weekday,
            inputs.world_work,
        )?;
        let guid = self.save_character_row(inputs, &mut observe)?;
        require(
            !self.save_fields.mail_dirty,
            "execution.player_save.Player_15763.mail_owner",
        )?;
        require(
            !self.save_fields.bg_dirty,
            "execution.player_save.Player_19840.battleground_save_owner",
        )?;
        self.save_inventory(inputs.objects, inputs.items, guid, false, &mut observe)?;
        self.save_quest_status(inputs.quests, inputs.calendar, guid, &mut observe)?;
        self.save_weekly_quest_status(guid, &mut observe)?;
        self.save_spells()?;
        self.save_spell_cooldowns(
            inputs.objects,
            inputs.units,
            guid,
            &mut inputs.clock,
            &mut observe,
        )?;
        self.save_actions(guid, &mut observe)?;
        self.save_auras(
            inputs.objects,
            inputs.units,
            inputs.spells,
            guid,
            &mut observe,
        )?;
        self.save_skills(guid, &mut observe)?;
        self.save_entered_instance_timers(inputs.session, &mut observe)?;
        self.save_reputation(guid, &mut observe)?;
        self.save_honor_cp()?;
        inputs.session.save_tutorials_data(&mut observe)?;
        Ok(guid)
    }
}
