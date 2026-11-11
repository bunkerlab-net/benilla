// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

pub(super) fn update(
    ai: &mut SourceCreatureAi,
    diff: u32,
    objects: &SourceObjects,
    units: &mut SourceUnits,
    owner: &mut impl BuiltinAiExecutionOwner,
) -> Result<()> {
    // GuardAI inherits UnitAI.cpp:1084-1127. TimerAI.cpp:26-42,136-143.
    for timer in &mut ai.base.generic_timers {
        if timer.disabled {
            continue;
        }
        if timer.remaining > diff {
            timer.remaining -= diff;
            continue;
        }
        timer.remaining = 0;
        timer.disabled = true;
        match timer.id {
            2000 => ai.base.distancing_cooldown = false,
            2001 => ai.base.spell_list_cooldown = false,
            _ => return Err(refuse("execution.builtin_ai.f5_17_timer_override_owner")),
        }
    }
    if !owner.select_hostile_target(ai, objects, units)? {
        return Ok(());
    }
    // Ranged distancing reads live spell prevention and F6 melee reach, :1095-1118.
    if ai.base.ranged_mode
        && units.get(objects, ai.base.unit)?.victim.is_some()
        && ai.base.can_execute(objects, units)?
        && (matches!(ai.base.ranged_setting, 2 | 4)
            || ai.base.current_ranged_mode && ai.base.melee_enabled)
    {
        return Err(refuse("execution.builtin_ai.f6_ranged_distancing_owner"));
    }
    ai.base.execute_actions(objects, units, owner)?;
    if !ai.base.spell_list_cooldown {
        return Err(refuse(
            "execution.builtin_ai.f5_08_update_spell_lists_owner",
        ));
    }
    possessed::update(&ai.base, objects, units, owner)
}

pub(super) fn move_in_line_of_sight(
    ai: &mut SourceCreatureAi,
    who: ObjectHandle,
    objects: &SourceObjects,
    units: &mut SourceUnits,
    owner: &mut impl BuiltinAiExecutionOwner,
) -> Result<()> {
    let unit = ai.base.unit;
    // GuardAI.cpp:39-43. Unlike UnitAI, no reaction, faction-neutral or dungeon gates.
    if !units.get(objects, unit)?.can_fly
        && owner.sight_query(unit, who, UnitAiSightQuery::TargetFlying)?
        && !owner.sight_query(
            unit,
            who,
            UnitAiSightQuery::WithinVerticalRange(if ai.base.current_ranged_mode {
                15f32.to_bits()
            } else {
                3f32.to_bits()
            }),
        )?
    {
        return Ok(());
    }
    if units.get(objects, unit)?.victim.is_some() {
        return Ok(());
    }
    if units.get(objects, who)?.in_combat()
        && owner.guard_query(unit, who, GuardQuery::CanAssist)?
    {
        let Some(victim) = owner.helper_attacker(who)? else {
            return Ok(());
        };
        if !eligible(unit, victim, owner)? {
            return Ok(());
        }
        let player = objects.get(who)?.kind == SourceObjectKind::Player;
        let victim_player = objects.get(victim)?.kind == SourceObjectKind::Player;
        // :54-77 preserves the player/NPC, PvP/pet and guard/civilian branch order.
        let attack = if player && !victim_player {
            owner.guard_query(unit, who, GuardQuery::WithinDistance(5f32.to_bits()))?
                && owner.guard_query(unit, victim, GuardQuery::WithinDistance(10f32.to_bits()))?
                && owner.guard_query(unit, victim, GuardQuery::LineOfSight)?
        } else if player && victim_player || player_owned_pet(victim, objects, units)? {
            owner.guard_query(unit, who, GuardQuery::WithinDistance(30f32.to_bits()))?
                && owner.guard_query(unit, who, GuardQuery::LineOfSight)?
        } else if objects
            .get(who)?
            .guid
            .is_some_and(|g| g.high() == HighGuid::Creature as u16)
        {
            // Guard/civilian metadata belongs to the effective live CreatureAI.
            owner.guard_query(unit, who, GuardQuery::GuardOrCivilian)?
                && owner.guard_query(unit, who, GuardQuery::WithinDistance(20f32.to_bits()))?
                && owner.guard_query(unit, who, GuardQuery::LineOfSight)?
        } else {
            false
        };
        if attack {
            ai.attack_start(Some(victim), objects, units, owner)?;
        }
    } else if eligible(unit, who, owner)?
        && owner.guard_query(unit, who, GuardQuery::AttackDistance)?
        && owner.guard_query(unit, who, GuardQuery::LineOfSight)?
    {
        ai.attack_start(Some(who), objects, units, owner)?;
    }
    Ok(())
}
fn eligible(
    unit: ObjectHandle,
    who: ObjectHandle,
    owner: &mut impl BuiltinAiExecutionOwner,
) -> Result<bool> {
    Ok(owner.guard_query(unit, who, GuardQuery::CanInitiateAttack)?
        && owner.guard_query(unit, who, GuardQuery::AttackOnSight)?
        && owner.guard_query(unit, who, GuardQuery::Accessible)?)
}
fn player_owned_pet(
    unit: ObjectHandle,
    objects: &SourceObjects,
    units: &SourceUnits,
) -> Result<bool> {
    // GuardAI.cpp:61 tests GUID IsCreature before the cast. Pet high GUIDs differ.
    if !objects
        .get(unit)?
        .guid
        .is_some_and(|g| g.high() == HighGuid::Creature as u16)
    {
        return Ok(false);
    }
    let fields = units.get(objects, unit)?;
    if fields.pet_kind.is_none() {
        return Ok(false);
    }
    let Some(owner) = fields.owner else {
        return Ok(false);
    };
    Ok(objects.get(owner)?.kind == SourceObjectKind::Player)
}
