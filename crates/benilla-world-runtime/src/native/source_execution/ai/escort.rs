// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

const UNIT_STAND_STATE_STAND: u8 = 0; // Unit.h:97.
const UNIT_STAND_STATE_SLEEP: u8 = 3; // Unit.h:100.
const UNIT_STAND_STATE_KNEEL: u8 = 8; // Unit.h:105.
const UNIT_FLAG_PVP: u32 = 0x1000; // Unit.h:478.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscortAi {
    pub evade_check_cooldown_ms: u32,
    pub quest: Option<u32>,
    pub player_guid: Option<SourceGuid>,
    pub player_check_timer_ms: u32,
    pub state: u32,
    pub running: bool,
    pub can_instant_respawn: bool,
    pub can_return_to_start: bool,
    pub waypoint_path_id: u32,
    pub current_escort_waypoint_path: u32,
}

impl EscortAi {
    // escort_ai.cpp:19-29. JustRespawned is inherited from CreatureAI.cpp:124-128.
    fn construct() -> Self {
        Self {
            evade_check_cooldown_ms: SCRIPTED_EVADE_CHECK_COOLDOWN_MS, // sc_creature.cpp:23.
            quest: None,                                               // escort_ai.cpp:20.
            player_guid: None,                                         // escort_ai.cpp:21.
            player_check_timer_ms: 1000,                               // escort_ai.cpp:22.
            state: 0,                        // escort_ai.cpp:23; escort_ai.h:14.
            running: false,                  // escort_ai.cpp:24.
            can_instant_respawn: false,      // escort_ai.cpp:25.
            can_return_to_start: false,      // escort_ai.cpp:26.
            waypoint_path_id: 0,             // escort_ai.cpp:27.
            current_escort_waypoint_path: 0, // escort_ai.cpp:28.
        }
    }

    pub(super) fn update_ai(&mut self, _diff: u32) -> Result<()> {
        Err(refuse("execution.ai.escort_ai_129.UpdateAI_owner"))
    }

    pub fn movement_inform(&mut self, _move_type: u32, _point_id: u32) -> Result<()> {
        Err(refuse("execution.ai.escort_ai_182.MovementInform_owner"))
    }

    pub fn set_current_waypoint(&mut self, _point_id: u32) -> Result<()> {
        Err(refuse(
            "execution.ai.escort_ai_205.SetCurrentWaypoint_owner",
        ))
    }

    pub fn get_player_for_escort(&self) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.ai.escort_ai_62.GetPlayerForEscort_owner"))
    }
}

#[derive(Clone, Debug)]
pub struct GarmentsQuestState {
    pub player_guid: Option<SourceGuid>,
    pub healed: bool,
    pub can_run: bool,
    pub run_away_timer_ms: u32,
}

#[derive(Clone, Debug)]
pub struct GarmentsOfQuestsAi {
    pub(super) binding: Arc<ScriptDevAiBinding>,
    pub escort: EscortAi,
    pub quest: GarmentsQuestState,
}

impl GarmentsOfQuestsAi {
    // npcs_special.cpp:742-745,570. The factory calls Reset after the escort base.
    pub(super) fn construct(
        binding: &Arc<ScriptDevAiBinding>,
        unit: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
    ) -> Result<Self> {
        let escort = EscortAi::construct();
        let quest = Self::reset_quest(objects, units, unit, |quest| quest)?;
        Ok(Self {
            binding: Arc::clone(binding),
            escort,
            quest,
        })
    }

    pub(super) fn reset(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        unit: ObjectHandle,
    ) -> Result<()> {
        Self::reset_quest(objects, units, unit, |quest| self.quest = quest)
    }

    // npcs_special.cpp:579-593. The AI writes precede the unit writes.
    fn reset_quest<T>(
        objects: &SourceObjects,
        units: &mut SourceUnits,
        unit: ObjectHandle,
        write_quest: impl FnOnce(GarmentsQuestState) -> T,
    ) -> Result<T> {
        let quest = write_quest(GarmentsQuestState {
            player_guid: None,       // npcs_special.cpp:581.
            healed: false,           // npcs_special.cpp:583.
            can_run: false,          // npcs_special.cpp:584.
            run_away_timer_ms: 5000, // npcs_special.cpp:586.
        });
        let live = units.get_mut(objects, unit)?;
        set_stand_state(objects, unit, live, UNIT_STAND_STATE_KNEEL, false)?;
        objects.update_flags(unit, UNIT_FIELD_FLAGS, &mut live.flags, UNIT_FLAG_PVP, true)?;
        let vitals = live
            .vitals
            .as_ref()
            .ok_or_else(|| refuse("execution.ai.npcs_special_592.GetMaxHealth_owner"))?;
        // npcs_special.cpp:592; Unit.h:1330. C++ promotes uint32 to double before int truncation.
        let scaled = f64::from(vitals.max_health) * 0.7;
        require(
            scaled < f64::from(i32::MAX) + 1.0,
            "execution.ai.npcs_special_592.health_int_out_of_range_undefined",
        )?;
        set_health(objects, unit, live, scaled as i32 as u32)?;
        Ok(quest)
    }

    pub fn spell_hit(&mut self, _caster: ObjectHandle, _spell: u32) -> Result<()> {
        Err(refuse("execution.ai.npcs_special_595.SpellHit_owner"))
    }
}

// Unit.cpp:10170-10192. A changed Kneel byte skips the standing aura scan.
fn set_stand_state(
    objects: &SourceObjects,
    unit: ObjectHandle,
    live: &mut SourceUnitFields,
    state: u8,
    acknowledge: bool,
) -> Result<()> {
    if !objects.set_update_value(unit, UNIT_FIELD_BYTES_1, &mut live.stand_state, state)? {
        return Ok(());
    }
    require(
        !matches!(
            live.stand_state,
            UNIT_STAND_STATE_STAND | UNIT_STAND_STATE_SLEEP
        ),
        "execution.unit.Unit_10184.RemoveAurasWithInterruptFlags_owner",
    )?;
    if !acknowledge && objects.get(unit)?.kind == SourceObjectKind::Player {
        return Err(refuse("execution.unit.Unit_10190.SendPacket_owner"));
    }
    Ok(())
}

// Unit.cpp:9224-9244. Group checks follow the update-field write, even on equality.
fn set_health(
    objects: &SourceObjects,
    unit: ObjectHandle,
    live: &mut SourceUnitFields,
    value: u32,
) -> Result<()> {
    let vitals = live
        .vitals
        .as_mut()
        .ok_or_else(|| refuse("execution.unit.Unit_9226.GetMaxHealth_owner"))?;
    objects.set_update_value(
        unit,
        UNIT_FIELD_HEALTH,
        &mut vitals.health,
        vitals.max_health.min(value),
    )?;
    if objects.get(unit)?.kind == SourceObjectKind::Player {
        return Err(refuse("execution.unit.Unit_9235.GetGroup_owner"));
    }
    if !live.control_guids.has_charmer() {
        return Ok(());
    }
    let Some(charmer) = live
        .control_guids
        .get_charmer(objects, unit, None, unbound_pet_loading)?
        .unit()
    else {
        return Ok(());
    };
    if objects.get(charmer)?.kind == SourceObjectKind::Player {
        return Err(refuse("execution.unit.Unit_9241.GetGroup_owner"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "escort_tests.rs"]
mod tests;
