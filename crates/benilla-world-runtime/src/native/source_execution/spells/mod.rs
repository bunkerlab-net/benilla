// SPDX-License-Identifier: GPL-3.0-or-later
//! Live SpellStart and typed requests to the owners reached by Spell.cpp.
use super::*;
use std::collections::BTreeMap;
mod callers;
mod caster_auras;
mod checks;
mod items;
mod location;
mod power;
mod range;
pub use caster_auras::*;
pub use location::*;
pub use power::*;
pub use range::*;
mod db_callers;
mod events;
mod start;
mod state;
mod unit_event_owner;
pub use callers::*;
pub use checks::*;
pub use events::*;
pub use start::*;
pub use state::*;
pub use unit_event_owner::*;
#[cfg(test)]
mod caster_auras_tests;
#[cfg(test)]
mod census_tests;
#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod items_tests;
#[cfg(test)]
mod power_tests;
#[cfg(test)]
mod range_tests;
#[cfg(test)]
mod tests;

/// Exact live scalar operands, not a pristine-caster assumption.
#[derive(Clone, Copy, Debug)]
pub struct SpellUnitState {
    pub player: bool,
    pub gameobject: bool,
    pub alive: bool,
    pub standing: bool,
    pub player_controlled: bool,
    pub combat: bool,
    pub battleground_wait_leave: bool,
    pub gm: bool,
    pub moving: bool,
    pub falling_far: bool,
    pub stealth: bool,
    pub form: u32,
    pub class: u32,
    pub combo_target: Option<ObjectHandle>,
    pub mounted: bool,
    pub taxi: bool,
    pub totem: bool,
    pub untargetable: bool,
    pub has_ai: bool,
    pub position: [u32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellCheck {
    IsSpellReady,
    HasGcd,
    IsOutdoors,
    Shapeshift,
    OverrideScript(u32),
    LowLevelRank,
    Tapped,
    Visible,
    TargetCreatureType,
    PositiveImmunity(u32),
    FacingBack,
    FacingFront,
    SpiritOfRedemption,
    StrongerAura(u32),
    Dispel,
    TargetLevel,
    PetTarget(u8),
    TargetFilter(u8),
    AllowedInLocation([u32; 2]),
    Items,
    Focus,
    Range(bool),
    Power(bool),
    CasterAuras,
    Effect(u8),
    Aura(u8),
    Trade,
    NoActivePets,
    CaptureSpirit,
    ScreecherSpirit,
    OnCheckCast(bool),
}
impl SpellCheck {
    /// The missing owner is named at the first source query, not at SpellStart.
    pub fn missing_owner(self) -> Refusal {
        let name = match self {
            Self::IsSpellReady => "WorldObject_IsSpellReady_cooldown",
            Self::HasGcd => "WorldObject_HasGCD",
            Self::IsOutdoors => "f6_TerrainInfo_IsOutdoors",
            Self::Shapeshift => "GetErrorAtShapeshiftedCast",
            Self::OverrideScript(_) => "f5_12_Unit_HasOverrideScript",
            Self::LowLevelRank => "SpellMgr_SelectAuraRankForLevel",
            Self::Tapped => "Creature_IsTappedBy",
            Self::Visible => "f6_Unit_IsVisibleForOrDetect",
            Self::TargetCreatureType => "Unit_GetCreatureTypeMask",
            Self::PositiveImmunity(_) => "f5_14_Unit_IsImmuneToSpell",
            Self::FacingBack | Self::FacingFront => "f6_WorldObject_facing",
            Self::SpiritOfRedemption => "f5_12_Unit_HasAuraType",
            Self::StrongerAura(_) | Self::Dispel => "f5_12_Unit_GetSpellAuraHolderMap",
            Self::TargetLevel => "Unit_GetLevel",
            Self::PetTarget(_) => "f5_15_Unit_GetPet_GetCharm",
            Self::TargetFilter(_) => "Unit_CanAttackSpell_CanAssistSpell",
            Self::AllowedInLocation(_) => "SpellMgr_GetSpellAllowedInLocationError",
            Self::Items => "Unit_check_operands",
            Self::Focus => "f6_Cell_VisitGridObjects_spell_focus",
            Self::Range(_) => "f3_WorldObject_range",
            Self::Power(_) | Self::CasterAuras => "f3_WorldObject_power",
            Self::Effect(_) => "Spell_CheckCast_effect_operands",
            Self::Aura(_) => "f5_12_Spell_CheckCast_aura_operands",
            Self::Trade => "f5_25_Player_GetTradeData",
            Self::NoActivePets => "f5_15_Player_GetPetGuid_TryLoadFromDB",
            Self::CaptureSpirit => "WorldObject_GetObjectGuid_GetEntry",
            Self::ScreecherSpirit => "Creature_HasBeenHitBySpell_RegisterHitBySpell",
            Self::OnCheckCast(_) => "f5_21_Spell_OnCheckCast",
        };
        refuse(&format!("execution.spells.{name}_owner"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellCall {
    Init,
    PrepareTriggerData,
    RemoveCastAuras(u32),
    AiCastState(bool),
    SuccessfulStart,
    UnsummonPet,
    SendStart,
    SendResult(SpellCastResult),
    ResetMods,
    Interrupted,
    AutoRepeatCancel,
    AiInterrupt,
    ChannelCancel,
    RemoveOwnedObjects,
    Cast(bool),
    HandleDelayed(u64),
    HandleImmediate,
    FillTargetMap,
    ExecuteEffects,
    ChannelUpdate,
    ChannelPushback(u32),
    Pushback(u32),
}
impl SpellCall {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::Init | Self::SuccessfulStart => "execution.spells.f5_21_spell_script_owner",
            Self::PrepareTriggerData => "execution.spells.prepareDataForTriggerSystem_owner",
            Self::RemoveCastAuras(_) => "execution.spells.f5_12_RemoveAurasOnCast_owner",
            Self::AiCastState(_) => "execution.spells.UnitAI_OnSpellCastStateChange_owner",
            Self::AiInterrupt => "execution.spells.UnitAI_OnSpellInterrupt_owner",
            Self::UnsummonPet => "execution.spells.f5_15_Pet_Unsummon_owner",
            Self::Cast(_) => "execution.spells.f5_10_Spell_cast_owner",
            Self::FillTargetMap => "execution.spells.f5_09_Spell_FillTargetMap_owner",
            Self::ExecuteEffects => "execution.spells.f5_11_Spell_ExecuteEffects_owner",
            Self::HandleDelayed(_) => "execution.spells.f5_10_Spell_handle_delayed_owner",
            Self::HandleImmediate => "execution.spells.f5_10_Spell_handle_immediate_owner",
            Self::ChannelUpdate => "execution.spells.Unit_channel_update_operands_owner",
            Self::ChannelCancel | Self::ChannelPushback(_) => {
                "execution.spells.Spell_UniqueTargetInfo_channel_owner"
            }
            Self::ResetMods => "execution.spells.Unit_GetSpellModOwner_cancel_owner",
            Self::RemoveOwnedObjects => {
                "execution.spells.Unit_RemoveDynObject_RemoveGameObject_RemoveCreature_owner"
            }
            Self::SendStart => "execution.spells.SendSpellStart_owner",
            Self::SendResult(_) => "execution.spells.SendCastResult_owner",
            Self::Interrupted => "execution.spells.SendInterrupted_owner",
            Self::AutoRepeatCancel => "execution.spells.Player_SendAutoRepeatCancel_owner",
            Self::Pushback(_) => {
                "execution.spells.Player_SendDirectMessage_SMSG_SPELL_DELAYED_owner"
            }
        })
    }
}
/// External state queries are fallible. An unbound query never becomes false/OK.
pub trait SpellExecutionOwner {
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Err(refuse("execution.spells.f3_WorldObject_power_owner"))
    }
    fn power_is_unit(&mut self, caster: ObjectHandle) -> Result<bool> {
        Ok(matches!(
            self.power_objects()?.get(caster)?.kind,
            SourceObjectKind::Player | SourceObjectKind::Creature | SourceObjectKind::Pet
        ))
    }
    fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
        Err(refuse("execution.spells.Unit_GetPower_fields_owner"))
    }
    fn power_health(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.power_context()?.health(caster)
    }
    fn power_current(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        self.power_context()?.current(caster, ty)
    }
    fn power_maximum(&mut self, caster: ObjectHandle, ty: u32) -> Result<u32> {
        self.power_context()?.maximum(caster, ty)
    }
    fn power_level(&mut self, _caster: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.spells.CreatureStats_GetLevel_owner"))
    }
    fn power_rank(&mut self, caster: ObjectHandle, t: &SpellTemplate) -> Result<u32> {
        if self.power_objects()?.get(caster)?.kind == SourceObjectKind::Player {
            return Err(refuse("execution.spells.Player_GetSpellRank_skill_owner"));
        }
        Ok(creature_power_rank(self.power_level(caster)?, t))
    }
    fn power_create_health(&mut self, _caster: ObjectHandle) -> Result<u32> {
        Err(refuse(
            "execution.spells.CreatureStats_GetCreateHealth_owner",
        ))
    }
    fn power_create_mana(&mut self, _caster: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.spells.CreatureStats_GetCreateMana_owner"))
    }
    fn power_flat_modifier(&mut self, _caster: ObjectHandle, _school: u32) -> Result<i32> {
        Err(refuse(
            "execution.spells.UNIT_FIELD_POWER_COST_MODIFIER_owner",
        ))
    }
    fn power_mod_owner(&mut self, _caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.spells.Unit_GetSpellModOwner_cost_owner"))
    }
    fn apply_cost_mod(
        &mut self,
        _player: ObjectHandle,
        _spell: u32,
        _cost: i32,
        _final_use: bool,
    ) -> Result<i32> {
        Err(refuse("execution.spells.Player_ApplySpellMod_COST_owner"))
    }
    fn power_multiplier(&mut self, _caster: ObjectHandle, _school: u32) -> Result<u32> {
        Err(refuse(
            "execution.spells.UNIT_FIELD_POWER_COST_MULTIPLIER_owner",
        ))
    }
    fn caster_aura_flags(&mut self, caster: ObjectHandle) -> Result<u32> {
        let c = self.power_context()?;
        Ok(c.units.get(c.objects, caster)?.flags)
    }
    fn caster_charmer(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        let c = self.power_context()?;
        Ok(c.units.get(c.objects, caster)?.charmer)
    }
    fn caster_auras(&mut self, _caster: ObjectHandle, _ty: u32) -> Result<&[SpellCasterAura]> {
        Err(refuse(
            "execution.spells.Unit_GetAurasByType_CheckCasterAuras_owner",
        ))
    }
    /// F3 objects supply actual identity, kind, map and pose.
    fn range_objects(&mut self) -> Result<&SourceObjects> {
        Err(refuse("execution.spells.f3_WorldObject_range_owner"))
    }
    fn range_stores(&mut self) -> Result<&SourceSpellStores> {
        Err(refuse("execution.spells.f5_07_spell_stores_range_owner"))
    }
    fn range_entry(&mut self, index: u32) -> Result<Option<SpellRangeEntry>> {
        self.range_stores()?.range_entry(index)
    }
    fn range_facing(&mut self, spell: u32) -> Result<u32> {
        Ok(self.range_stores()?.facing(spell))
    }
    fn combat_reach(&mut self, _object: ObjectHandle) -> Result<f32> {
        Err(refuse("execution.spells.WorldObject_GetCombatReach_owner"))
    }
    fn moving_forward(&mut self, _object: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.spells.Unit_IsMovingForward_owner"))
    }
    fn walking(&mut self, _object: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.spells.Unit_IsWalking_owner"))
    }
    fn range_corpse(&mut self, spell: &SourceSpell) -> Result<bool> {
        Ok(spell.targets.corpse.is_some())
    }
    fn ranged_mod_range(&mut self, _caster: ObjectHandle) -> Result<Option<f32>> {
        Err(refuse(
            "execution.spells.Player_GetWeaponForAttack_RangedModRange_owner",
        ))
    }
    fn range_mod_owner(&mut self, _caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.spells.Unit_GetSpellModOwner_range_owner"))
    }
    fn apply_range_mod(&mut self, _player: ObjectHandle, _spell: u32, _range: f32) -> Result<f32> {
        Err(refuse("execution.spells.Player_ApplySpellMod_RANGE_owner"))
    }
    fn range_pet(&mut self, _caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.spells.Unit_GetPet_range_owner"))
    }
    fn range_charm(&mut self, _caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.spells.Unit_GetCharm_range_owner"))
    }
    fn range_collision_height(&mut self, _caster: ObjectHandle) -> Result<f32> {
        Err(refuse(
            "execution.spells.WorldObject_GetCollisionHeight_owner",
        ))
    }
    fn range_vmap(&mut self) -> Result<&SourceVMapManager> {
        Err(refuse(
            "execution.spells.f6_VMapManager_IsInLineOfSight_owner",
        ))
    }
    fn range_dynamic_los(
        &mut self,
        _caster: ObjectHandle,
        _from: [f32; 3],
        _to: [f32; 3],
    ) -> Result<bool> {
        Err(refuse(
            "execution.spells.Map_dynamic_tree_isInLineOfSight_owner",
        ))
    }
    /// Object.cpp:1387-1391; Map.cpp:2668-2672, static query before dynamic.
    fn range_point_los(&mut self, caster: ObjectHandle, end: [f32; 3]) -> Result<bool> {
        let object = self.range_objects()?.get(caster)?;
        let from: [u32; 3] = object
            .pose
            .ok_or_else(|| refuse("execution.spells.WorldObject_position_owner"))?
            .0[..3]
            .try_into()
            .unwrap();
        let map = object
            .map
            .ok_or_else(|| refuse("execution.spells.WorldObject_GetMap_owner"))?
            .map;
        let mut origin = from.map(f32::from_bits);
        origin[2] += self.range_collision_height(caster)?;
        if !self.range_vmap()?.line_of_sight(map, origin, end, false)? {
            return Ok(false);
        }
        self.range_dynamic_los(caster, origin, end)
    }
    fn unit(&mut self, _unit: ObjectHandle) -> Result<SpellUnitState> {
        Err(refuse("execution.spells.Unit_check_operands_owner"))
    }
    fn check(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        check: SpellCheck,
    ) -> Result<SpellCastResult> {
        if let SpellCheck::AllowedInLocation(location) = check {
            return self.location_context()?.check(t, spell.caster, location);
        }
        if check == SpellCheck::Items {
            let player = self.unit(spell.caster)?.player;
            return spell.check_items(t, player, self);
        }
        if let SpellCheck::Range(strict) = check {
            return spell.check_range(t, strict, self);
        }
        if let SpellCheck::Power(strict) = check {
            return spell.check_power(t, strict, self);
        }
        if check == SpellCheck::CasterAuras {
            return spell.check_caster_auras(t, self);
        }
        Err(check.missing_owner())
    }
    /// Spell.cpp:5100-5101. The binding supplies real Unit ownership state.
    fn location_context(&mut self) -> Result<SpellLocationContext<'_>> {
        Err(SpellCheck::AllowedInLocation([0; 2]).missing_owner())
    }
    fn call(&mut self, _spell: &mut SourceSpell, call: SpellCall) -> Result<()> {
        Err(call.missing_owner())
    }
    /// Spell.cpp:5098. This must bind the F6 live-object query, never map-only area.
    fn zone_area(&mut self, caster: ObjectHandle) -> Result<[u32; 2]> {
        let (objects, terrain) = self.terrain_geometry()?;
        terrain.object_zone_and_area(objects, caster)
    }
    /// Borrow the real F3 object and F6 geometry owners. No caller-supplied pose.
    fn terrain_geometry(&mut self) -> Result<(&SourceObjects, &mut SourceTerrainManager)> {
        Err(refuse(
            "execution.spells.f6_WorldObject_GetZoneAndAreaId_owner",
        ))
    }
    fn cast_time(&mut self, _spell: &mut SourceSpell, _t: &SpellTemplate) -> Result<i32> {
        Err(refuse(
            "execution.spells.GetSpellCastTime_Unit_modifiers_owner",
        ))
    }
    fn map_time(&mut self, _caster: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.spells.Map_GetCurrentMSTime_owner"))
    }
    fn add_gcd(&mut self, _spell: &SourceSpell, _t: &SpellTemplate) -> Result<()> {
        Err(refuse("execution.spells.WorldObject_AddGCD_owner"))
    }
    fn reset_gcd(&mut self, _spell: &SourceSpell, _t: &SpellTemplate) -> Result<()> {
        Err(refuse("execution.spells.WorldObject_ResetGCD_owner"))
    }
    fn set_current(&mut self, _spell: &mut SourceSpell, _t: &SpellTemplate) -> Result<()> {
        Err(refuse("execution.spells.Unit_SetCurrentCastedSpell_owner"))
    }
    fn update_pointers(&mut self, _spell: &mut SourceSpell) -> Result<bool> {
        Err(refuse("execution.spells.Spell_UpdatePointers_owner"))
    }
    /// Spell.cpp:6816-6819,6848-6851. Must use the live modifier and thread RNG.
    fn resist_pushback(&mut self, _spell: &SourceSpell) -> Result<bool> {
        Err(refuse(
            "execution.spells.f5_12_pushback_modifiers_rng_owner",
        ))
    }
    fn line_of_sight(&mut self, _caster: ObjectHandle, _target: ObjectHandle) -> Result<()> {
        Err(refuse(
            "execution.spells.f6_WorldObject_IsWithinLOSInMap_owner",
        ))
    }
    fn selection_target(&mut self, _spell: &mut SourceSpell) -> Result<()> {
        Err(refuse(
            "execution.spells.Map_GetUnit_Player_GetSelectionGuid_owner",
        ))
    }
    fn reset_auto_repeat(&mut self) -> Result<()> {
        Err(refuse("execution.spells.Unit_ResetAutoRepeatSpells_owner"))
    }
    fn other_non_melee_cast(&mut self, _spell: &SourceSpell) -> Result<bool> {
        Err(refuse("execution.spells.Unit_IsNonMeleeSpellCasted_owner"))
    }
    fn handle_delayed(&mut self, _spell: &mut SourceSpell, _elapsed: u64) -> Result<u64> {
        Err(refuse("execution.spells.f5_10_Spell_handle_delayed_owner"))
    }
    fn casting_source(&mut self, _spell: &SourceSpell) -> Result<Option<[u32; 3]>> {
        Err(refuse("execution.spells.GetCastingObject_owner"))
    }
    fn ai_cast_state(
        &mut self,
        _original: ObjectHandle,
        _caster: ObjectHandle,
    ) -> Result<(bool, bool)> {
        Err(refuse("execution.spells.UnitAI_order_current_cast_owner"))
    }
    fn has_aura(&mut self, _unit: ObjectHandle, _spell: u32) -> Result<bool> {
        Err(refuse("execution.spells.f5_12_Unit_HasAura_owner"))
    }
    fn ai_target_creature_type(
        &mut self,
        _target: ObjectHandle,
        _t: &SpellTemplate,
    ) -> Result<bool> {
        Err(refuse("execution.spells.Unit_GetCreatureTypeMask_owner"))
    }
    fn active_auto_spell(&mut self, _unit: ObjectHandle) -> Result<Option<u32>> {
        Err(refuse(
            "execution.spells.Unit_GetCurrentSpell_autorepeat_owner",
        ))
    }
    fn interrupt_for_ai(&mut self, _caster: ObjectHandle, _melee: bool) -> Result<()> {
        Err(refuse("execution.spells.Unit_InterruptSpell_owner"))
    }
    fn interrupt_slot(&mut self, _caster: ObjectHandle, _slot: u32) -> Result<()> {
        Err(refuse(
            "execution.spells.Unit_InterruptSpell_current_slot_owner",
        ))
    }
    fn indoor_policy(&mut self) -> Result<bool> {
        Err(refuse(
            "execution.spells.World_vmap_indoor_configuration_owner",
        ))
    }
    fn wand_school(&mut self, _caster: ObjectHandle) -> Result<u32> {
        Err(refuse(
            "execution.spells.Unit_GetRangedDamageSchoolMask_owner",
        ))
    }
    fn original_caster(
        &mut self,
        _caster: ObjectHandle,
        _original: ObjectHandle,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse(
            "execution.spells.Spell_UpdateOriginalCasterPointer_owner",
        ))
    }
    fn positive_definition(&mut self, _t: &SpellTemplate) -> Result<bool> {
        Err(refuse("execution.spells.IsPositiveSpell_definition_owner"))
    }
    /// Spell.cpp:482; SpellTargets.cpp:235-238. A bound lookup must refuse a
    /// missing map entry as undefined, not supply default filtering schemes.
    fn targeting_data(&mut self, _spell: u32) -> Result<[[u32; 2]; 3]> {
        Err(refuse(
            "execution.spells.f5_09_SpellTargetMgr_GetSpellTargetingData_owner",
        ))
    }
}
