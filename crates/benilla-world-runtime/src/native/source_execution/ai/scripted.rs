// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ScriptDev classes that share the live CreatureAI base.
use super::*;
use std::sync::Arc;

#[path = "azuregos.rs"]
mod azuregos;
#[path = "combat_ai.rs"]
mod combat_ai;
#[path = "escort.rs"]
mod escort;
#[path = "script_guard.rs"]
mod script_guard;
#[path = "solenor.rs"]
mod solenor;
#[path = "taerar.rs"]
mod taerar;
pub use azuregos::BossAzuregosAi;
pub use combat_ai::{CombatActionInitial, CombatAi};
pub use escort::{EscortAi, GarmentsOfQuestsAi, GarmentsQuestState};
pub use script_guard::{GuardStormwindAi, ScriptGuardAi};
pub use solenor::SolenorAi;
pub use taerar::BossTaerarAi;

pub trait ScriptedAiResetOwner: NearTeleportOwner {
    fn solenor_respawn_clock(
        &mut self,
        _unit: ObjectHandle,
        _random: &dyn CreatureRandom,
    ) -> Result<ClockSample> {
        Err(refuse("execution.creature.Creature_801.time_owner"))
    }

    fn current_motion_type(&self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse(
            "execution.ai.silithus_88.GetCurrentMovementGeneratorType_owner",
        ))
    }

    fn reinitialize_motion(
        &mut self,
        _unit: ObjectHandle,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        Err(refuse(
            "execution.ai.silithus_91.MotionMaster_Initialize_owner",
        ))
    }

    fn taerar_reset_cast(
        &mut self,
        _base: &mut SourceUnitAi,
        _objects: &SourceObjects,
        _units: &mut SourceUnits,
        _random: &mut dyn CreatureRandom,
        _spell: u32,
        _flags: u32,
    ) -> Result<()> {
        Err(refuse(
            "execution.ai.bosses_emerald_dragons_394.DoCastSpellIfCan_owner",
        ))
    }
}
impl ScriptedAiResetOwner for () {}

pub(crate) const SCRIPTDEV_GET_AI_OWNER: &str = "execution.ai.ScriptDevAIMgr_370.GetAI_owner";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceScriptedAiKind {
    ChickenCluck,
    Azuregos,
    Taerar,
    Solenor,
    Guard,
    GuardStormwind,
    GarmentsOfQuests,
}
impl SourceScriptedAiKind {
    pub(crate) fn for_registered_name(name: &str) -> Result<Self> {
        match name {
            "npc_chicken_cluck" => Ok(Self::ChickenCluck),
            "boss_azuregos" => Ok(Self::Azuregos),
            // bosses_emerald_dragons.cpp:569-572 registers GetAI_boss_taerar.
            "boss_taerar" => Ok(Self::Taerar),
            "npc_solenor" => Ok(Self::Solenor),
            "guard_bluffwatcher" // guards.cpp:45-48,165-167.
            | "guard_contested" // guards.cpp:50-53,170-172.
            | "guard_darnassus" // guards.cpp:55-58,175-177.
            | "guard_dunmorogh" // guards.cpp:60-63,180-182.
            | "guard_durotar" // guards.cpp:65-68,185-187.
            | "guard_elwynnforest" // guards.cpp:70-73,190-192.
            | "guard_ironforge" // guards.cpp:75-78,195-197.
            | "guard_mulgore" // guards.cpp:80-83,200-202.
            | "guard_teldrassil" // guards.cpp:145-148,215-217.
            | "guard_tirisfal" // guards.cpp:150-153,220-222.
            | "guard_undercity" => Ok(Self::Guard), // guards.cpp:155-158,225-227.
            "guard_stormwind" => Ok(Self::GuardStormwind),
            // npcs_special.cpp:1162-1165 registers GetAI_npc_garments_of_quests.
            "npc_garments_of_quests" => Ok(Self::GarmentsOfQuests),
            _ => Err(refuse(&format!("{SCRIPTDEV_GET_AI_OWNER}.{name}"))),
        }
    }

    pub(crate) fn action_count(self) -> u32 {
        match self {
            Self::ChickenCluck
            | Self::Taerar
            | Self::Solenor
            | Self::Guard
            | Self::GuardStormwind
            | Self::GarmentsOfQuests => 0,
            Self::Azuregos => BossAzuregosAi::ACTION_COUNT,
        }
    }
}

#[derive(Clone, Debug)]
pub enum SourceScriptedAi {
    ChickenCluck(ChickenCluckAi),
    Azuregos(BossAzuregosAi),
    Taerar(BossTaerarAi),
    Solenor(SolenorAi),
    Guard {
        binding: Arc<ScriptDevAiBinding>,
        ai: ScriptGuardAi,
    },
    GuardStormwind(GuardStormwindAi),
    GarmentsOfQuests(GarmentsOfQuestsAi),
}

// sc_creature.cpp:22-25. ScriptedAI inherits CreatureAI::Reset.
const SCRIPTED_EVADE_CHECK_COOLDOWN_MS: u32 = 2500;

#[derive(Clone, Debug)]
pub struct ChickenCluckAi {
    binding: Arc<ScriptDevAiBinding>,
    pub evade_check_cooldown_ms: u32,
    pub reset_flag_timer_ms: u32,
}

impl SourceScriptedAi {
    /// `sc_creature.cpp:22-25` runs after the CreatureAI base; `npcs_special.cpp:62`
    /// calls the concrete Reset before the factory returns the new AI.
    pub(crate) fn construct(
        kind: SourceScriptedAiKind,
        binding: &Arc<ScriptDevAiBinding>,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
        reset_owner: &mut dyn ScriptedAiResetOwner,
    ) -> Result<Self> {
        match kind {
            SourceScriptedAiKind::ChickenCluck => {
                let evade_check_cooldown_ms = SCRIPTED_EVADE_CHECK_COOLDOWN_MS;
                let reset_flag_timer_ms =
                    Self::reset_chicken(objects, units, base.unit, |timer| timer)?;
                Ok(Self::ChickenCluck(ChickenCluckAi {
                    binding: Arc::clone(binding),
                    evade_check_cooldown_ms,
                    reset_flag_timer_ms,
                }))
            }
            SourceScriptedAiKind::Azuregos => {
                BossAzuregosAi::construct(binding, base, random).map(Self::Azuregos)
            }
            SourceScriptedAiKind::Taerar => {
                BossTaerarAi::construct(binding, objects, units, base, random, reset_owner)
                    .map(Self::Taerar)
            }
            SourceScriptedAiKind::Solenor => {
                SolenorAi::construct(binding, base.unit, objects, units, random, reset_owner)
                    .map(Self::Solenor)
            }
            SourceScriptedAiKind::Guard => Ok(Self::Guard {
                binding: Arc::clone(binding),
                ai: ScriptGuardAi::construct(random)?,
            }),
            SourceScriptedAiKind::GuardStormwind => {
                GuardStormwindAi::construct(binding, random).map(Self::GuardStormwind)
            }
            SourceScriptedAiKind::GarmentsOfQuests => {
                GarmentsOfQuestsAi::construct(binding, base.unit, objects, units)
                    .map(Self::GarmentsOfQuests)
            }
        }
    }

    pub fn binding(&self) -> &ScriptDevAiBinding {
        match self {
            Self::ChickenCluck(ai) => &ai.binding,
            Self::Azuregos(ai) => &ai.binding,
            Self::Taerar(ai) => &ai.binding,
            Self::Solenor(ai) => &ai.binding,
            Self::Guard { binding, .. } => binding,
            Self::GuardStormwind(ai) => &ai.binding,
            Self::GarmentsOfQuests(ai) => &ai.binding,
        }
    }

    pub(crate) fn reset(
        &mut self,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
        reset_owner: &mut dyn ScriptedAiResetOwner,
    ) -> Result<()> {
        match self {
            Self::ChickenCluck(ai) => Self::reset_chicken(objects, units, base.unit, |timer| {
                ai.reset_flag_timer_ms = timer;
            }),
            Self::Azuregos(ai) => {
                ai.reset(base);
                Ok(())
            }
            Self::Taerar(ai) => ai.reset(objects, units, base, random, reset_owner),
            Self::Solenor(ai) => ai.reset(base.unit, objects, units, random, reset_owner),
            Self::Guard { ai, .. } => {
                ai.reset();
                Ok(())
            }
            Self::GuardStormwind(ai) => {
                ai.reset();
                Ok(())
            }
            Self::GarmentsOfQuests(ai) => ai.reset(objects, units, base.unit),
        }
    }

    pub(crate) fn guard_mut(&mut self) -> Option<&mut ScriptGuardAi> {
        match self {
            Self::Guard { ai, .. } => Some(ai),
            Self::GuardStormwind(ai) => Some(&mut ai.guard),
            _ => None,
        }
    }

    fn reset_chicken<T>(
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        unit: ObjectHandle,
        write_timer: impl FnOnce(u32) -> T,
    ) -> Result<T> {
        let timer = write_timer(20000); // npcs_special.cpp:68.
        let live = units.get_mut(objects, unit)?;
        // Unit.h:1363 and :511; npcs_special.cpp:70-71.
        live.set_faction_template(objects, unit, 31)?;
        let npc_flags = live.loaded_creature.npc_flags & !2;
        objects.set_update_value(
            unit,
            UNIT_NPC_FLAGS,
            &mut live.loaded_creature.npc_flags,
            npc_flags,
        )?;
        Ok(timer)
    }

    /// ChatHandler.cpp:798 dispatches the override at npcs_special.cpp:74-88.
    pub fn receive_emote(&self, _player: ObjectHandle, _text_emote: u32) -> Result<()> {
        match self {
            Self::ChickenCluck(_) => Err(refuse(
                "execution.ai.ChatHandler_798.ReceiveEmote_owner.npc_chicken_cluck",
            )),
            Self::GuardStormwind(_) => Err(refuse("execution.ai.guards_133.ReceiveEmote_owner")),
            // These scripts inherit the empty UnitAI.h:282 callback.
            Self::Azuregos(_)
            | Self::Taerar(_)
            | Self::Solenor(_)
            | Self::Guard { .. }
            | Self::GarmentsOfQuests(_) => Ok(()),
        }
    }

    /// Unit.cpp:528 dispatches the override at npcs_special.cpp:90-103.
    pub fn update_ai(&mut self, diff: u32) -> Result<()> {
        match self {
            Self::ChickenCluck(_) => Err(refuse(
                "execution.ai.Unit_528.UpdateAI_owner.npc_chicken_cluck",
            )),
            Self::Azuregos(_) => Err(refuse(
                "execution.ai.UnitAI_1084.UpdateAI_owner.boss_azuregos",
            )),
            Self::Taerar(_) => Err(refuse(
                "execution.ai.bosses_emerald_dragons_104.UpdateAI_owner",
            )),
            Self::Solenor(_) => Err(refuse("execution.ai.silithus_203.UpdateAI_owner")),
            Self::Guard { .. } => Err(refuse("execution.ai.UnitAI_1084.UpdateAI_owner.guardAI")),
            Self::GuardStormwind(_) => Err(refuse("execution.ai.guards_120.UpdateAI_owner")),
            Self::GarmentsOfQuests(ai) => ai.escort.update_ai(diff),
        }
    }
}
