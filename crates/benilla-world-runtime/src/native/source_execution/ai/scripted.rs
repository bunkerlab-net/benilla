// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ScriptDev classes that share the live CreatureAI base.
use super::*;
use std::sync::Arc;

#[path = "azuregos.rs"]
mod azuregos;
#[path = "combat_ai.rs"]
mod combat_ai;
#[path = "solenor.rs"]
mod solenor;
pub use azuregos::BossAzuregosAi;
pub use combat_ai::{CombatActionInitial, CombatAi};
pub use solenor::{SolenorAi, SolenorResetOwner};

pub(crate) const SCRIPTDEV_GET_AI_OWNER: &str = "execution.ai.ScriptDevAIMgr_370.GetAI_owner";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceScriptedAiKind {
    ChickenCluck,
    Azuregos,
    Solenor,
}
impl SourceScriptedAiKind {
    pub(crate) fn for_registered_name(name: &str) -> Result<Self> {
        match name {
            "npc_chicken_cluck" => Ok(Self::ChickenCluck),
            "boss_azuregos" => Ok(Self::Azuregos),
            "npc_solenor" => Ok(Self::Solenor),
            _ => Err(refuse(&format!("{SCRIPTDEV_GET_AI_OWNER}.{name}"))),
        }
    }

    pub(crate) fn action_count(self) -> u32 {
        match self {
            Self::ChickenCluck | Self::Solenor => 0,
            Self::Azuregos => BossAzuregosAi::ACTION_COUNT,
        }
    }
}

#[derive(Clone, Debug)]
pub enum SourceScriptedAi {
    ChickenCluck(ChickenCluckAi),
    Azuregos(BossAzuregosAi),
    Solenor(SolenorAi),
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
        reset_owner: &mut dyn SolenorResetOwner,
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
            SourceScriptedAiKind::Solenor => {
                SolenorAi::construct(binding, base.unit, objects, units, random, reset_owner)
                    .map(Self::Solenor)
            }
        }
    }

    pub fn binding(&self) -> &ScriptDevAiBinding {
        match self {
            Self::ChickenCluck(ai) => &ai.binding,
            Self::Azuregos(ai) => &ai.binding,
            Self::Solenor(ai) => &ai.binding,
        }
    }

    pub(crate) fn reset(
        &mut self,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
        reset_owner: &mut dyn SolenorResetOwner,
    ) -> Result<()> {
        match self {
            Self::ChickenCluck(ai) => Self::reset_chicken(objects, units, base.unit, |timer| {
                ai.reset_flag_timer_ms = timer;
            }),
            Self::Azuregos(ai) => {
                ai.reset(base);
                Ok(())
            }
            Self::Solenor(ai) => ai.reset(base.unit, objects, units, random, reset_owner),
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
            // Azuregos inherits the empty UnitAI.h:282 callback.
            Self::Azuregos(_) | Self::Solenor(_) => Ok(()),
        }
    }

    /// Unit.cpp:528 dispatches the override at npcs_special.cpp:90-103.
    pub fn update_ai(&mut self, _diff: u32) -> Result<()> {
        match self {
            Self::ChickenCluck(_) => Err(refuse(
                "execution.ai.Unit_528.UpdateAI_owner.npc_chicken_cluck",
            )),
            Self::Azuregos(_) => Err(refuse(
                "execution.ai.UnitAI_1084.UpdateAI_owner.boss_azuregos",
            )),
            Self::Solenor(_) => Err(refuse("execution.ai.silithus_203.UpdateAI_owner")),
        }
    }
}
