// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Unit.cpp:484-556 and Object.cpp:1183-1190, on the existing creature update.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitUpdateCall {
    Cooldowns,
    Spells,
    CleanupDeletedAuras,
    AttackAndReactiveTimers,
    SplineMovement,
    MotionMaster,
    AliveAi,
    CombatManager,
    Healthless20AuraState,
}
impl UnitUpdateCall {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::Cooldowns | Self::Spells => "execution.unit_update.f5_08_spell_owner",
            Self::CleanupDeletedAuras | Self::Healthless20AuraState => {
                "execution.unit_update.f5_12_aura_owner"
            }
            Self::AttackAndReactiveTimers => "execution.unit_update.attack_reactive_timer_owner",
            Self::SplineMovement => "execution.unit_update.f6_spline_owner",
            Self::MotionMaster => "execution.unit_update.f5_05_motion_dispatch_owner",
            Self::AliveAi => "execution.unit_update.f5_06c_ai_dispatch_owner",
            Self::CombatManager => "execution.unit_update.combat_dispatch_owner",
        })
    }
}

/// The aura owner retains multimap insertion order and validates its iterator
/// after callbacks. Unit.h:1076; Unit.cpp:544-550; SpellAuras.h:99.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartbeatHolder {
    pub spell: u32,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartbeatProcArguments {
    pub attacker: ObjectHandle,
    pub victim: Option<ObjectHandle>,
    pub attacker_flags: u32,
    pub victim_flags: u32,
    pub proc_extra: u32,
    pub amount: u32,
    pub absorb: u32,
    pub attack_type: u8,
    pub spell_info: Option<u32>,
    pub spell: Option<u64>,
    pub health_gain: u32,
    pub is_heal: bool,
}
pub trait UnitHeartbeatOwner {
    fn first_holder(&mut self, _unit: ObjectHandle) -> Result<Option<HeartbeatHolder>> {
        Err(refuse("execution.unit_heartbeat.f5_12_aura_holder_owner"))
    }
    fn next_holder(
        &mut self,
        _unit: ObjectHandle,
        _holder: HeartbeatHolder,
    ) -> Result<Option<HeartbeatHolder>> {
        Err(refuse("execution.unit_heartbeat.f5_12_aura_iterator_owner"))
    }
    fn aura_at(
        &mut self,
        _unit: ObjectHandle,
        _holder: HeartbeatHolder,
        _effect: u8,
    ) -> Result<Option<u64>> {
        Err(refuse("execution.unit_heartbeat.f5_12_aura_effect_owner"))
    }
    fn aura_heartbeat(&mut self, _unit: ObjectHandle, _aura: u64) -> Result<()> {
        // Aura::OnHeartbeat dispatches only a selected script, SpellAuras.cpp:6178-6182.
        Err(refuse("execution.unit_heartbeat.f5_12_aura_script_owner"))
    }
    fn heartbeat_proc_count(&mut self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.unit_heartbeat.f5_12_proc_counter_owner"))
    }
    fn heartbeat_proc(&mut self, _arguments: HeartbeatProcArguments) -> Result<()> {
        // Unit.cpp:552-553: self, null victim, HEARTBEAT, NONE, NONE, 0, 0.
        Err(refuse("execution.unit_heartbeat.f5_14_proc_owner"))
    }
    fn has_ai(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.unit_heartbeat.ai_binding_owner"))
    }
    fn ai_heartbeat(&mut self, _unit: ObjectHandle) -> Result<()> {
        Err(refuse("execution.unit_heartbeat.f5_06c_ai_callback_owner"))
    }
}

/// No alive guard. Aura effects precede proc evaluation, then the live AI lookup.
/// WorldObject::Heartbeat is actually empty, Object.h:860; Unit.cpp:539-556.
pub fn source_unit_heartbeat(
    unit: ObjectHandle,
    owner: &mut (impl UnitHeartbeatOwner + ?Sized),
) -> Result<()> {
    let mut holder = owner.first_holder(unit)?;
    while let Some(current) = holder {
        for effect in 0..3 {
            if let Some(aura) = owner.aura_at(unit, current, effect)? {
                owner.aura_heartbeat(unit, aura)?;
            }
        }
        holder = owner.next_holder(unit, current)?;
    }
    if owner.heartbeat_proc_count(unit)? != 0 {
        owner.heartbeat_proc(HeartbeatProcArguments {
            attacker: unit,
            victim: None,
            attacker_flags: 1,
            victim_flags: 0,
            proc_extra: 0,
            amount: 0,
            absorb: 0,
            // ProcSystemArguments constructor defaults, Unit.h:749-751.
            attack_type: 0,
            spell_info: None,
            spell: None,
            health_gain: 0,
            is_heal: false,
        })?;
    }
    if owner.has_ai(unit)? {
        owner.ai_heartbeat(unit)?;
    }
    Ok(())
}

/// In-memory version-one F7 fields, not a native checkpoint representation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceUnitUpdate {
    heartbeat_expiry: u32,
    pub spell_update_happening: bool,
    stopped: bool,
}
impl SourceUnitUpdate {
    /// ShortTimeTracker saturates, so even a large diff emits only one heartbeat.
    /// Timer.h:145-158; Object.h:861 defaults to 5000, initial expiry is zero.
    pub fn world_object_update(
        &mut self,
        unit: ObjectHandle,
        diff: u32,
        owner: &mut (impl UnitHeartbeatOwner + ?Sized),
    ) -> Result<()> {
        require(!self.stopped, "execution.unit_update.stopped")?;
        self.heartbeat_expiry = self.heartbeat_expiry.saturating_sub(diff);
        if self.heartbeat_expiry == 0 {
            self.heartbeat_expiry = 5000;
            let result = source_unit_heartbeat(unit, owner);
            self.stopped |= result.is_err();
            result?;
        }
        Ok(())
    }
    pub fn heartbeat_expiry(&self) -> u32 {
        self.heartbeat_expiry
    }
}

pub fn update_source_creature_unit<D, P, F>(
    creature: &mut SourceCreatureLifecycle,
    diff: u32,
    context: &mut CreatureLifecycleContext<'_, '_, D, P>,
    owner: &mut F,
) -> Result<()>
where
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: CreatureLifecycleOwner<D, P>,
{
    require(
        !creature.unit_update.stopped,
        "execution.unit_update.stopped",
    )?;
    // Unit.cpp:465-466 returns before cooldowns, events and heartbeat.
    if !context.grid.objects.get(creature.object())?.in_world {
        return Ok(());
    }
    let result = (|| {
        // Unit.cpp:487-494: event execution MUST precede _UpdateSpells.
        creature.unit_update.spell_update_happening = true;
        owner.lifecycle_unit_call(UnitUpdateCall::Cooldowns, diff, creature, context)?;
        update_source_creature_events(creature, diff, context, owner)?;
        owner.lifecycle_unit_call(UnitUpdateCall::Spells, diff, creature, context)?;
        creature.unit_update.spell_update_happening = false;
        for call in [
            UnitUpdateCall::CleanupDeletedAuras,
            UnitUpdateCall::AttackAndReactiveTimers,
            UnitUpdateCall::SplineMovement,
            UnitUpdateCall::MotionMaster,
        ] {
            owner.lifecycle_unit_call(call, diff, creature, context)?;
        }
        // Unit.cpp:516 tests the actual installed AI before the live death state.
        if owner.lifecycle_unit_has_ai(creature, context)?
            && creature.death_state == CreatureDeathState::Alive
        {
            owner.lifecycle_unit_call(UnitUpdateCall::AliveAi, diff, creature, context)?;
        }
        owner.lifecycle_unit_call(UnitUpdateCall::CombatManager, diff, creature, context)?;
        if creature.death_state == CreatureDeathState::Alive {
            owner.lifecycle_unit_call(
                UnitUpdateCall::Healthless20AuraState,
                diff,
                creature,
                context,
            )?;
        }
        owner.lifecycle_unit_heartbeat_update(diff, creature, context)
    })();
    creature.unit_update.stopped |= result.is_err();
    result
}
