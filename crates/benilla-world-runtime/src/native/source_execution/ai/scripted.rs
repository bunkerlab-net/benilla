// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ScriptDev classes that share the live CreatureAI base.
use super::*;
use std::sync::Arc;

pub(crate) const SCRIPTDEV_GET_AI_OWNER: &str = "execution.ai.ScriptDevAIMgr_370.GetAI_owner";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceScriptedAiKind {
    ChickenCluck,
}
impl SourceScriptedAiKind {
    pub(crate) fn for_registered_name(name: &str) -> Result<Self> {
        match name {
            "npc_chicken_cluck" => Ok(Self::ChickenCluck),
            _ => Err(refuse(&format!("{SCRIPTDEV_GET_AI_OWNER}.{name}"))),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SourceScriptedAi {
    ChickenCluck(ChickenCluckAi),
}

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
        unit: ObjectHandle,
    ) -> Result<Self> {
        match kind {
            SourceScriptedAiKind::ChickenCluck => {
                let evade_check_cooldown_ms = 2500;
                let reset_flag_timer_ms = Self::reset_chicken(objects, units, unit, |timer| timer)?;
                Ok(Self::ChickenCluck(ChickenCluckAi {
                    binding: Arc::clone(binding),
                    evade_check_cooldown_ms,
                    reset_flag_timer_ms,
                }))
            }
        }
    }

    pub fn binding(&self) -> &ScriptDevAiBinding {
        match self {
            Self::ChickenCluck(ai) => &ai.binding,
        }
    }

    pub(crate) fn reset(
        &mut self,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        unit: ObjectHandle,
    ) -> Result<()> {
        match self {
            Self::ChickenCluck(ai) => Self::reset_chicken(objects, units, unit, |timer| {
                ai.reset_flag_timer_ms = timer;
            }),
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
        live.set_client_update_field(objects, unit, SourceUnitUpdateField::FactionTemplate, 31)?;
        let npc_flags = live.loaded_creature.npc_flags & !2;
        live.set_client_update_field(objects, unit, SourceUnitUpdateField::NpcFlags, npc_flags)?;
        Ok(timer)
    }

    /// ChatHandler.cpp:798 dispatches the override at npcs_special.cpp:74-88.
    pub fn receive_emote(&self, _player: ObjectHandle, _text_emote: u32) -> Result<()> {
        match self {
            Self::ChickenCluck(_) => Err(refuse(
                "execution.ai.ChatHandler_798.ReceiveEmote_owner.npc_chicken_cluck",
            )),
        }
    }

    /// Unit.cpp:528 dispatches the override at npcs_special.cpp:90-103.
    pub fn update_ai(&mut self, _diff: u32) -> Result<()> {
        match self {
            Self::ChickenCluck(_) => Err(refuse(
                "execution.ai.Unit_528.UpdateAI_owner.npc_chicken_cluck",
            )),
        }
    }
}
