// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

#[derive(Clone, Debug)]
pub struct EmeraldDragonAi {
    pub event_counter: u32,
    pub seeping_fog_timer_ms: u32,
    pub noxious_breath_timer_ms: u32,
    pub tailsweep_timer_ms: u32,
}

impl EmeraldDragonAi {
    // bosses_emerald_dragons.cpp:65-71.
    fn reset(random: &mut dyn CreatureRandom) -> Result<Self> {
        Ok(Self {
            event_counter: 1,
            seeping_fog_timer_ms: 30000,
            noxious_breath_timer_ms: random.creature_urand(9000, 12000)?,
            tailsweep_timer_ms: random.creature_urand(10000, 20000)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BossTaerarAi {
    pub(super) binding: Arc<ScriptDevAiBinding>,
    pub evade_check_cooldown_ms: u32,
    pub dragon: EmeraldDragonAi,
    pub arcane_blast_timer_ms: u32,
    pub bellowing_roar_timer_ms: u32,
    pub shades_timeout_timer_ms: u32,
    pub shades_dead: u8,
}

impl BossTaerarAi {
    // bosses_emerald_dragons.cpp:57,373,482-485. Each constructor calls its own Reset.
    pub(super) fn construct(
        binding: &Arc<ScriptDevAiBinding>,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
        owner: &mut dyn ScriptedAiResetOwner,
    ) -> Result<Self> {
        let mut ai = Self {
            evade_check_cooldown_ms: SCRIPTED_EVADE_CHECK_COOLDOWN_MS,
            dragon: EmeraldDragonAi::reset(random)?,
            binding: Arc::clone(binding),
            arcane_blast_timer_ms: 0,
            bellowing_roar_timer_ms: 0,
            shades_timeout_timer_ms: 0,
            shades_dead: 0,
        };
        ai.reset(objects, units, base, random, owner)?;
        Ok(ai)
    }

    // bosses_emerald_dragons.cpp:380-395.
    pub(super) fn reset(
        &mut self,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
        owner: &mut dyn ScriptedAiResetOwner,
    ) -> Result<()> {
        self.dragon = EmeraldDragonAi::reset(random)?;
        self.arcane_blast_timer_ms = 12000;
        self.bellowing_roar_timer_ms = 30000;
        self.shades_timeout_timer_ms = 0;
        self.shades_dead = 0;
        let unit = base.unit;
        let live = units.get_mut(objects, unit)?;
        const UNINTERACTIBLE: u32 = 0x0200_0000; // Unit.h:490.
        if live.flags & UNINTERACTIBLE != 0 {
            live.set_flag(objects, unit, UNINTERACTIBLE, false)?;
        }
        // bosses_emerald_dragons.cpp:366,394 and UnitAI.h:67.
        owner.taerar_reset_cast(base, objects, units, random, 24886, 2)
    }
}
