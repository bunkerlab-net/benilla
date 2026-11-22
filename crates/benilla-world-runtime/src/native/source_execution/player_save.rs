// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::source_rows::{RowDelete, RowInsert, RowOperation};
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
    pub objects: &'a SourceObjects,
    pub units: &'a SourceUnits,
    pub items: &'a SourceItems,
    pub session: &'a SourceWorldSession,
    pub world: &'a SourceWorld,
    pub calendar: Calendar,
    pub maintenance_weekday: i32,
    pub save_interval: u32,
    pub world_work: &'a mut W,
    pub wall_seconds: &'a mut dyn FnMut() -> Result<i64>,
}

impl SourcePlayer {
    /// CharacterHandler.cpp:506-512 continues after detached Create succeeds.
    pub fn finish_detached_create(
        &mut self,
        have_same_race: bool,
        skip_cinematics: u32,
        inputs: &mut SourcePlayerSaveInputs<'_, impl WorldStartupWork>,
        observe: impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        // CharacterHandler.cpp:55-59,506-507.
        if skip_cinematics == CINEMATICS_SKIP_ALL
            || (skip_cinematics == CINEMATICS_SKIP_SAME_RACE && have_same_race)
        {
            self.save_fields.cinematic = 1;
        }
        self.save_fields.at_login |= AT_LOGIN_FIRST; // Player.h:2093.
        self.save_to_db_prefix(inputs, observe)
    }

    /// Player.cpp:15602-15766. This prefix never commits a transaction.
    pub fn save_to_db_prefix(
        &mut self,
        inputs: &mut SourcePlayerSaveInputs<'_, impl WorldStartupWork>,
        mut observe: impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
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
        let object = inputs.objects.get(self.object)?;
        let guid = object
            .guid
            .ok_or_else(|| refuse("execution.player_save.Player_15622.character_guid_owner"))?
            .counter();
        observe(SourcePlayerSaveObservation::Row(RowOperation::Delete(
            RowDelete::Character(guid),
        )))?;
        let row = self.character_row(inputs, object, guid)?;
        observe(SourcePlayerSaveObservation::Row(RowOperation::Insert(
            RowInsert::Character(Box::new(row)),
        )))?;
        require(
            !self.save_fields.mail_dirty,
            "execution.player_save.Player_15763.mail_owner",
        )?;
        require(
            !self.save_fields.bg_dirty,
            "execution.player_save.Player_19840.battleground_save_owner",
        )?;
        Err(refuse("execution.player_save.Player_15766.inventory_owner"))
    }
}
