// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

#[derive(Clone, Debug)]
pub struct ScriptGuardAi {
    pub combat: CombatAi,
    pub global_cooldown_ms: u32,
    pub buff_timer_ms: u32,
}

impl ScriptGuardAi {
    // guard_ai.cpp:29-32. The live CreatureAI base already has zero actions.
    pub(super) fn construct(random: &mut dyn CreatureRandom) -> Result<Self> {
        Ok(Self {
            combat: CombatAi::construct(random)?,
            global_cooldown_ms: 0,
            buff_timer_ms: 0,
        })
    }

    // guard_ai.cpp:34-38 does not delegate to CombatAI::Reset.
    pub fn reset(&mut self) {
        self.global_cooldown_ms = 0;
        self.buff_timer_ms = 0;
    }

    pub(crate) fn reset_timers(
        &mut self,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
    ) -> Result<()> {
        self.combat.reset_timers(base, random)
    }
    // guard_ai.cpp:66-81 follows CreatureAI::JustRespawned and its virtual Reset.
    pub(crate) fn after_inherited_respawn(
        base: &mut SourceUnitAi,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        stores: Option<&SourceSpellStores>,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let live = units.get(objects, base.unit)?;
        let spells = live.spell_list.as_ref();
        // Deviation: CreatureSpellList.h:84-91 leaves ChanceRangedAttack unwritten; the owner permits guard_ai.cpp:68 because both branches do nothing when Spells is empty.
        let Some(list) = spells.filter(|_| live.spell_list_disabled.is_some()) else {
            return require(
                spells.is_none_or(|list| list.spells.is_empty()),
                "execution.ai.guard_ai_68.unwritten_chance_ranged_attack",
            );
        };
        if list.chance_ranged == 0 {
            return Ok(());
        }
        for spell in list.spells.values().filter(|spell| spell.flags & 2 != 0) {
            let stores = stores.ok_or_else(|| {
                refuse("execution.ai.guard_ai_73.SpellTemplate_LookupEntry_owner")
            })?;
            if let Some(template) = stores.template(spell.spell_id) {
                let range = base.calculate_spell_range(objects, units, stores, template)?;
                // UnitAI.h:104, TYPE_PROXIMITY.
                base.set_ranged_mode(true, range * 0.8, 2, objects, units, owner)?;
                break;
            }
        }
        Ok(())
    }

    pub fn aggro(&mut self, _enemy: ObjectHandle) -> Result<()> {
        Err(refuse("execution.ai.guard_ai_40.Aggro_owner"))
    }

    pub fn just_died(&mut self, _killer: Option<ObjectHandle>) -> Result<()> {
        Err(refuse("execution.ai.guard_ai_53.JustDied_owner"))
    }
}

#[derive(Clone, Debug)]
pub struct GuardStormwindAi {
    pub(super) binding: Arc<ScriptDevAiBinding>,
    pub guard: ScriptGuardAi,
    pub salute_wait_timer_ms: u32,
}

impl GuardStormwindAi {
    // guards.cpp:99-102 calls the concrete Reset after constructing guardAI.
    pub(super) fn construct(
        binding: &Arc<ScriptDevAiBinding>,
        random: &mut dyn CreatureRandom,
    ) -> Result<Self> {
        let guard = ScriptGuardAi::construct(random)?;
        let mut ai = Self {
            binding: Arc::clone(binding),
            guard,
            salute_wait_timer_ms: 0,
        };
        ai.reset();
        Ok(ai)
    }

    // guards.cpp:106-109 leaves guardAI and CombatAI fields alone.
    pub(crate) fn reset(&mut self) {
        self.salute_wait_timer_ms = 0;
    }

    pub fn spell_hit(&mut self, _caster: ObjectHandle, _spell: u32) -> Result<()> {
        Err(refuse("execution.ai.guards_111.SpellHit_owner"))
    }
}
