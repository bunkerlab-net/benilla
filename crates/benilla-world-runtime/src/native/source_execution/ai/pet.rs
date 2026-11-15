// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! PetAI.cpp:41-65,102-118,144-167,266-270,429-545,564-587.
use super::*;

/// Effect-indexed aura reads are not the spell-id-only Unit::HasAura overload.
pub trait PetAuraExecutionOwner: UnitAiExecutionOwner {
    fn pet_has_aura(&mut self, _unit: ObjectHandle, _spell: u32, _effect: u8) -> Result<bool> {
        Err(refuse("execution.pet_ai.Unit_HasAura_effect_owner"))
    }
    fn pet_aura_stack(
        &mut self,
        _unit: ObjectHandle,
        _spell: u32,
        _effect: u8,
    ) -> Result<Option<u32>> {
        Err(refuse("execution.pet_ai.Unit_GetAura_stack_owner"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetControlField {
    Charmer,
    Creator,
}

pub struct PetAiConstructionOwners<'a> {
    pub units: &'a mut SourceUnits,
    pub ais: &'a mut SourceCreatureAis,
    pub created_by_spell: Option<u32>,
    pub config: &'a EventAiLoaderConfig,
    pub attack_from_behind: Option<&'a PetAttackFromBehindConfig>,
    pub execution: &'a mut dyn UnitAiExecutionOwner,
}

/// Runtime-only derived fields. Native save/Open remains refused by the builtin inventory.
#[derive(Clone, Debug)]
pub struct SourcePetAi {
    unit: ObjectHandle,
    is_pet: bool,
    in_combat: bool,
}
impl SourcePetAi {
    pub(super) fn construct(
        selected: &SelectedCreatureAi,
        objects: &SourceObjects,
        ais: &mut SourceCreatureAis,
        created_by_spell: Option<u32>,
        config: &EventAiLoaderConfig,
        attack_from_behind: Option<&PetAttackFromBehindConfig>,
        owner: &mut (impl UnitAiExecutionOwner + ?Sized),
    ) -> Result<Self> {
        let unit = selected.object();
        let ai = ais.get_mut(objects, unit)?;
        // PetAI.cpp:41-45 writes these before reading any derived dependency.
        let value = Self {
            unit,
            is_pet: selected.inputs().control.pet.is_some(),
            in_combat: false,
        };
        ai.follow_angle = std::f32::consts::FRAC_PI_2;
        ai.follow_distance = 1.5;
        if selected.inputs().control.pet == Some(CreaturePetKind::Controlled) {
            // PetAI.cpp:49 precedes RequestFollow(:53), even with no owner.
            let behind = attack_from_behind.ok_or_else(|| {
                refuse("execution.pet_ai.CONFIG_BOOL_PET_ATTACK_FROM_BEHIND_owner")
            })?;
            if behind.enabled() {
                ai.base.attack_angle = std::f32::consts::PI;
            }
            if let Some(owner) = resolve_control(objects, unit, PetControlField::Creator, owner)? {
                return Err(refuse(
                    if objects.get(owner)?.kind == SourceObjectKind::Player {
                        "execution.pet_ai.Player_RequestFollowData_owner"
                    } else {
                        "execution.pet_ai.MotionMaster_MoveFollow_owner"
                    },
                ));
            }
        }
        // PetAI.cpp:56. None means explicitly unwritten, not spell zero.
        let spell = created_by_spell
            .ok_or_else(|| refuse("execution.pet_ai.UNIT_CREATED_BY_SPELL_owner"))?;
        if spell == 688 {
            ai.base.attack_distance = 25.0;
            ai.base.melee_enabled = false;
        }
        // PetAI.cpp:64 short-circuits GetCharmInfo only for an actual guard.
        if !ai.guard {
            return Err(refuse("execution.pet_ai.Unit_GetCharmInfo_owner"));
        }
        ai.base.visibility_distance = f32::from_bits(config.guard_sight()?);
        Ok(value)
    }
    pub fn inventory(&self) -> serde_json::Value {
        serde_json::json!({"version":1,"unit":self.unit,"is_pet":self.is_pet,
            "in_combat":self.in_combat,"requested_follower":null,
            "restore":"execution.pet_ai.f7_restore_owner"})
    }
    pub(super) fn update(
        &mut self,
        diff: u32,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        // PetAI inherits CanHandleCharm=false, UnitAI.h:478. No inherited AI update.
        let master = resolve_master(objects, self.unit, owner)?; // PetAI.cpp:146.
        let ai = ais.get_mut(objects, self.unit)?;
        update_timers(&mut ai.base, diff)?; // PetAI.cpp:154, before the owner test.
        if master.is_none() {
            return Ok(());
        }
        if ai.base.combat_script {
            self.in_combat = false;
            return Ok(());
        }
        if self.is_pet {
            return Err(refuse("execution.pet_ai.Pet_HasActionsDisabled_owner"));
            // :165.
        }
        let _victim = units.get(objects, self.unit)?.victim;
        // :167 is before victim evade/CC, AttackStop, and MotionMaster reads.
        Err(refuse("execution.pet_ai.Unit_GetCharmInfo_owner"))
    }
    pub(super) fn attack_start(
        &mut self,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if self.is_pet {
            return Err(refuse("execution.pet_ai.Pet_HasActionsDisabled_owner"));
            // :104.
        }
        let ai = &ais.get(objects, self.unit)?.base;
        if ai.combat_script {
            return Ok(());
        }
        // Unit::Attack(nullptr, melee) returns false, Unit.cpp:6261-6262.
        if let Some(target) = target {
            if ai.attack_unit(target, objects, units, owner)? {
                // No CreatureAI engagement, passive check, or target-change inference.
                ai.movement_on_attack(target, false, objects, units, owner)?;
                self.in_combat = true;
            }
        }
        Ok(())
    }
    pub fn pick_spell_with_target(&self) -> Result<Vec<(u32, Option<ObjectHandle>, bool)>> {
        // PetAI.cpp:270 is the first read. Do not skip the opener to inspect a spell list.
        Err(refuse("execution.pet_ai.CharmInfo_GetSpellOpener_owner"))
    }
    pub fn can_auto_cast_area_aura(
        &self,
        spell: &SpellTemplate,
        target: Option<ObjectHandle>,
        owner: &mut impl PetAuraExecutionOwner,
    ) -> Result<bool> {
        let effects = [spell.effect1, spell.effect2, spell.effect3];
        let targets = [
            spell.effectimplicittargeta1,
            spell.effectimplicittargeta2,
            spell.effectimplicittargeta3,
        ];
        for (i, effect) in effects.into_iter().enumerate() {
            // SpellMgr.h:895-900, SpellEffects enum: APPLY_AREA_AURA_PARTY/PET.
            if matches!(effect, 35 | 119) {
                let unit = if matches!(targets[i], 0 | 1) {
                    self.unit
                } else if let Some(target) = target {
                    target
                } else {
                    return Ok(false);
                };
                if owner.pet_has_aura(unit, spell.id, i as u8)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    pub fn can_auto_cast(
        &self,
        spell: &SpellTemplate,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &SourceUnits,
        owner: &mut impl PetAuraExecutionOwner,
    ) -> Result<bool> {
        if !self.can_auto_cast_area_aura(spell, target, owner)? {
            return Ok(false);
        }
        for (i, effect) in [spell.effect1, spell.effect2, spell.effect3]
            .into_iter()
            .enumerate()
        {
            if effect == 6 {
                // PetAI.cpp:465,470 dereferences target without a null check.
                let target =
                    target.ok_or_else(|| refuse("execution.pet_ai.null_aura_target_undefined"))?;
                if spell.stackamount <= 1 {
                    if owner.pet_has_aura(target, spell.id, i as u8)? {
                        return Ok(false);
                    }
                } else if owner
                    .pet_aura_stack(target, spell.id, i as u8)?
                    .is_some_and(|stack| stack >= spell.stackamount)
                {
                    return Ok(false);
                }
            }
        }
        Ok(matches!(
            self.check_pet_cast(spell, target, objects, units, owner)?,
            SpellCastResult::Ok | SpellCastResult::UnitNotInfront
        ))
    }
    pub fn check_pet_cast(
        &self,
        spell: &SpellTemplate,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<SpellCastResult> {
        let fields = units.get(objects, self.unit)?;
        if fields.casting_non_melee && spell.attributesex4 & 0x80 == 0 {
            return Ok(SpellCastResult::SpellInProgress); // PetAI.cpp:486-487.
        }
        if fields.in_combat() && spell.attributes & 0x1000_0000 != 0 {
            return Ok(SpellCastResult::AffectingCombat);
        }
        if self.is_pet
            || owner
                .pet_control_guid(self.unit, PetControlField::Charmer)?
                .raw()
                != 0
        {
            if let Some(master) = resolve_master(objects, self.unit, owner)? {
                if !objects.get(master)?.alive {
                    return Ok(SpellCastResult::CasterDead);
                }
            }
            let mut need = false;
            let mut script = false;
            for id in [
                spell.effectimplicittargeta1,
                spell.effectimplicittargeta2,
                spell.effectimplicittargeta3,
            ] {
                // PetAI.cpp:501 indexes the fixed source table before any target test.
                let data = SPELL_TARGET_INFO_TABLE.get(id as usize).ok_or_else(|| {
                    refuse("execution.pet_ai.SpellTargetInfoTable_index_undefined")
                })?;
                if data.filter == SpellTargetFilter::Script {
                    script = true;
                } else if matches!(
                    data.target_type,
                    SpellTargetImplicitType::Unit | SpellTargetImplicitType::Player
                ) {
                    need = true;
                    if target.is_none() {
                        return Ok(SpellCastResult::BadImplicitTargets);
                    }
                    break;
                }
            }
            if script || !need {
                return Ok(SpellCastResult::Ok);
            }
            if let Some(target) = target {
                if !objects.get(target)?.alive && spell.attributesex2 & 1 == 0 {
                    return Ok(SpellCastResult::BadTargets);
                }
                // :524 requires the caster/target-sensitive overload, not definition positivity.
                return Err(refuse("execution.pet_ai.IsPositiveSpell_target_owner"));
            }
        }
        Ok(SpellCastResult::Ok)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn cast(
        &self,
        spell: u32,
        target: Option<ObjectHandle>,
        opener: bool,
        objects: &SourceObjects,
        units: &SourceUnits,
        context: &mut SpellStartContext<'_>,
    ) -> Result<bool> {
        let controlled = opener && units.get(objects, self.unit)?.flags & 8 != 0;
        context.pet_cast(self.unit, spell, target, opener && controlled, self.is_pet)
    }
}

// TimerAI.cpp:26-42,136-143,171-179. PetAI has zero combat actions.
fn update_timers(ai: &mut SourceUnitAi, diff: u32) -> Result<()> {
    for timer in &mut ai.generic_timers {
        if timer.disabled {
            continue;
        }
        if timer.remaining > diff {
            timer.remaining -= diff;
        } else {
            timer.remaining = 0;
            timer.disabled = true;
            match timer.id {
                2000 => ai.distancing_cooldown = false,
                2001 => ai.spell_list_cooldown = false,
                _ => return Err(refuse("execution.builtin_ai.f5_17_timer_override_owner")),
            }
        }
    }
    require(
        ai.action_count() == 0,
        "execution.pet_ai.combat_action_timer_owner",
    )?;
    Ok(())
}

// Unit.cpp:6580-6635,6696-6717. Never treat selected owner handles as GUID fields.
fn resolve_master(
    objects: &SourceObjects,
    unit: ObjectHandle,
    owner: &mut (impl UnitAiExecutionOwner + ?Sized),
) -> Result<Option<ObjectHandle>> {
    match resolve_control(objects, unit, PetControlField::Charmer, owner)? {
        Some(charmer) => Ok(Some(charmer)),
        None => resolve_control(objects, unit, PetControlField::Creator, owner),
    }
}
fn resolve_control(
    objects: &SourceObjects,
    unit: ObjectHandle,
    field: PetControlField,
    owner: &mut (impl UnitAiExecutionOwner + ?Sized),
) -> Result<Option<ObjectHandle>> {
    let guid = owner.pet_control_guid(unit, field)?;
    if guid.raw() == 0 {
        return Ok(None);
    }
    let source = objects.get(unit)?;
    if !source.in_world {
        if guid.high() == HighGuid::Player as u16
            && source.kind == SourceObjectKind::Pet
            && owner.pet_is_loading(unit)?
        {
            return objects.find_player(guid);
        }
        return Ok(None);
    }
    let map = source
        .map
        .ok_or_else(|| refuse("execution.pet_ai.GetMap_owner"))?;
    Ok(match guid.high() {
        0 | 0xf130 | 0xf140 => objects.lookup_world_object(map, guid),
        _ => None,
    })
}

#[cfg(test)]
#[path = "pet_tests.rs"]
mod tests;
