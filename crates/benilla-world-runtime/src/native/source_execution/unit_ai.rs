// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! UnitAI.cpp:39-140,283-348,778-803,890-911,934-1011.
use super::*;
use std::collections::BTreeMap;

pub const UNIT_STATE_MELEE: u32 = 1;
pub const UNIT_STATE_AI_ROOT: u32 = 0x2000_0000;
pub const UNIT_STATE_NO_COMBAT_MOVEMENT: u32 = 0x0100_0000;
pub const UNIT_FLAG_COMBAT: u32 = 0x0008_0000;
const CROWD_CONTROL_FLAGS: u32 = 0x00c4_0000;
const IMMOBILIZED: u32 = 0x2000_001a;
const CANNOT_REACT: u32 = 0x0020_240e;

/// Values computed by SelectLevel/UpdateEntry, never guessed from MinLevelHealth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SourceUnitVitals {
    pub health: u32,
    pub max_health: u32,
    pub mana: u32,
    pub max_mana: u32,
    pub has_mana: bool,
    pub energy: u32,
    pub max_energy: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct SourceUnitFields {
    pub entry: u32,
    pub faction: u32,
    pub static_flags: [u32; 4],
    pub has_ai: bool,
    pub can_fly: bool,
    pub neutral_to_all: Option<bool>,
    pub can_aggro: Option<bool>,
    pub force_attacking: bool,
    pub flags: u32,
    pub state: u32,
    pub vitals: Option<SourceUnitVitals>,
    pub victim: Option<ObjectHandle>,
    pub target: Option<ObjectHandle>,
    pub attackers: Vec<ObjectHandle>,
    pub owner: Option<ObjectHandle>,
    pub charmer: Option<ObjectHandle>,
    pub pet_kind: Option<CreaturePetKind>,
    pub totem: bool,
    pub beneficiary: Option<ObjectHandle>,
    pub controlling_player: Option<ObjectHandle>,
    pub spawner: Option<ObjectHandle>,
    pub can_enter_combat: bool,
    pub ignore_ranged_targets: bool,
    pub evade_home: bool,
    pub evade_combat: bool,
    pub evade_timer: u32,
    pub casting_non_melee: bool,
    pub melee_spell: bool,
    pub gm: bool,
    pub no_threat: bool,
    pub mounted: bool,
    pub offhand: bool,
    pub combat_timer: u32,
    pub pursuit_ms: u32,
    pub last_refresh_position: Option<[u32; 4]>,
    pub combat_tick: u32,
    pub damage_by_others: u32,
}
impl SourceUnitFields {
    /// Unit.cpp:330,423-425; Creature.cpp:541-579. Spawn overrides belong to UpdateEntry.
    pub fn creature(template: &CreatureAiTemplateOperands) -> Self {
        Self {
            entry: template.entry,
            faction: template.faction,
            static_flags: template.static_flags,
            has_ai: false,
            can_fly: template.inhabit_type & 4 != 0,
            neutral_to_all: None,
            can_aggro: None,
            force_attacking: template.extra_flags & 0x80000 != 0,
            flags: template.unit_flags
                | if template.static_flags[0] & 0x200 != 0 {
                    0x0200_0000
                } else {
                    0
                }
                | if template.static_flags[0] & 0x20 != 0 {
                    0x100
                } else {
                    0
                }
                | if template.static_flags[0] & 0x40 != 0 {
                    0x200
                } else {
                    0
                },
            state: 0,
            vitals: None,
            victim: None,
            target: None,
            attackers: Vec::new(),
            owner: None,
            charmer: None,
            pet_kind: None,
            totem: false,
            beneficiary: None,
            controlling_player: None,
            spawner: None,
            can_enter_combat: true,
            ignore_ranged_targets: false,
            evade_home: false,
            evade_combat: false,
            evade_timer: 0,
            casting_non_melee: false,
            gm: false,
            melee_spell: false,
            no_threat: false,
            mounted: false,
            offhand: false,
            combat_timer: 0,
            pursuit_ms: template.pursuit_ms,
            last_refresh_position: None,
            combat_tick: 1200,
            damage_by_others: 0,
        }
    }
    pub fn in_combat(&self) -> bool {
        self.flags & UNIT_FLAG_COMBAT != 0
    }
    pub fn crowd_controlled(&self) -> bool {
        self.flags & CROWD_CONTROL_FLAGS != 0
    }
    pub fn immobilized(&self) -> bool {
        self.state & IMMOBILIZED != 0
    }
    pub fn in_evade(&self) -> bool {
        self.evade_home || self.evade_combat || self.evade_timer != 0
    }
    pub fn set_flag(&mut self, mask: u32, set: bool) {
        if set {
            self.flags |= mask;
        } else {
            self.flags &= !mask;
        }
    }
    /// The existing bounded kernel already includes UpdateMaxHealth/UpdateEntry.
    pub fn apply_initialized(&mut self, value: &benilla_sim::creature_init::InitializedCreature) {
        self.vitals = Some(SourceUnitVitals {
            health: value.stats.resources.health,
            max_health: value.stats.resources.max_health,
            mana: value.stats.resources.mana,
            max_mana: value.stats.resources.max_mana,
            has_mana: value.power_type == benilla_sim::creature_init::PowerType::Mana,
            energy: 0,
            max_energy: 0,
        });
    }
}
#[derive(Clone, Debug, Default)]
pub struct SourceUnits {
    fields: BTreeMap<ObjectHandle, SourceUnitFields>,
}
impl SourceUnits {
    /// Bind the live F5.01 control operands, not a caller-selected AI identity.
    pub fn bind_ai_selection(
        &mut self,
        objects: &SourceObjects,
        selected: &SelectedCreatureAi,
    ) -> Result<()> {
        selected.validate(objects)?;
        let input = selected.inputs();
        let f = self.get_mut(objects, selected.object())?;
        require(
            f.entry == input.effective_entry,
            "execution.unit_ai.effective_entry",
        )?;
        f.neutral_to_all = Some(input.neutral_to_all);
        f.owner = input.control.owner;
        f.charmer = input.control.charmer;
        f.pet_kind = input.control.pet;
        f.totem = input.control.totem;
        f.set_flag(8, input.control.player_controlled);
        Ok(())
    }
    /// Unit.cpp:8605-8633, including player-controlled guardian and charm exceptions.
    pub fn can_have_threat_list(
        &self,
        objects: &SourceObjects,
        unit: ObjectHandle,
    ) -> Result<bool> {
        let object = objects.get(unit)?;
        if !object.alive {
            return Ok(false);
        }
        let f = self.get(objects, unit)?;
        let controlled = f.flags & 8 != 0;
        if object.kind == SourceObjectKind::Player {
            return Ok(!controlled);
        }
        if f.totem && controlled {
            return Ok(false);
        }
        if object.kind == SourceObjectKind::Pet && controlled {
            return Ok(f.pet_kind == Some(CreaturePetKind::Guardian));
        }
        if let Some(charmer) = f.charmer {
            if objects.get(charmer)?.kind == SourceObjectKind::Player {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn register(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        fields: SourceUnitFields,
    ) -> Result<()> {
        require(
            matches!(
                objects.get(unit)?.kind,
                SourceObjectKind::Creature | SourceObjectKind::Pet | SourceObjectKind::Player
            ),
            "execution.unit.not_unit",
        )?;
        require(
            !self.fields.contains_key(&unit),
            "execution.unit.duplicate_generation",
        )?;
        self.fields.insert(unit, fields);
        Ok(())
    }
    pub fn get(&self, objects: &SourceObjects, unit: ObjectHandle) -> Result<&SourceUnitFields> {
        objects.get(unit)?;
        self.fields
            .get(&unit)
            .ok_or_else(|| refuse("execution.unit.fields_generation_missing"))
    }
    pub fn get_mut(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
    ) -> Result<&mut SourceUnitFields> {
        objects.get(unit)?;
        self.fields
            .get_mut(&unit)
            .ok_or_else(|| refuse("execution.unit.fields_generation_missing"))
    }
    pub fn event_unit(&self, objects: &SourceObjects, unit: ObjectHandle) -> Result<EventAiUnit> {
        let f = self.get(objects, unit)?;
        let v = f
            .vitals
            .ok_or_else(|| refuse("execution.unit.select_level_stats_owner"))?;
        Ok(EventAiUnit {
            kind: objects.get(unit)?.kind,
            entry: f.entry,
            combat: f.in_combat(),
            evade: f.in_evade(),
            health: v.health,
            max_health: v.max_health,
            mana: v.mana,
            max_mana: v.max_mana,
            has_mana: v.has_mana,
            energy: v.energy,
            max_energy: v.max_energy,
            victim: f.victim,
            owner: f.owner,
            beneficiary: f.beneficiary,
            controlling_player: f.controlling_player,
            spawner: f.spawner,
            casting_non_melee: f.casting_non_melee,
        })
    }
    /// Reuse the bounded SelectLevel port only after its complete inputs and recorded rates agree.
    pub fn initialize_supported_creature(
        &mut self,
        objects: &SourceObjects,
        state: &CreatureConstruction,
        input: &benilla_sim::creature_init::Input,
        config: &EventAiLoaderConfig,
    ) -> Result<()> {
        config.validate_creature_rates(&input.rates)?;
        let value = resolve_supported_creature_kernel(input, state)?;
        let fields = self.get_mut(objects, state.object)?;
        require(
            fields.entry == value.entry,
            "execution.unit.initialized_entry",
        )?;
        fields.faction = value.faction;
        fields.flags = value.unit_flags;
        fields.apply_initialized(&value);
        Ok(())
    }
    pub fn inventory(&self, objects: &SourceObjects) -> Result<serde_json::Value> {
        for unit in self.fields.keys() {
            objects.get(*unit)?;
        }
        Ok(
            serde_json::json!({"version":1,"units":self.fields.iter().collect::<Vec<_>>(),
            "restore":"execution.unit.f7_restore_owner"}),
        )
    }
}

/// Foreign operations are mandatory at their actual call, including aura removal before home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAiCall {
    Immobilized(bool),
    RemoveAurasOnEvade,
    CombatStopWithPets,
    TriggerHome,
    TriggerEvade,
    Chase {
        target: ObjectHandle,
        target_change: bool,
        distance_bits: u32,
        angle_bits: u32,
        move_further: bool,
        walk: bool,
    },
    Home,
    AddInitialThreat(ObjectHandle),
    PropagateCombat(ObjectHandle),
    InterruptNonCombatCasts,
    CreatureCombatStart(ObjectHandle),
    MeleeStart(Option<ObjectHandle>),
    MeleeStop(Option<ObjectHandle>),
    EnterCombat(Option<ObjectHandle>),
    InitialSpellCooldowns,
    TeleportCast(ObjectHandle),
    MinimapPing,
    GuardPosition(u32),
    ThreatCurrent(ObjectHandle),
    OffhandTimer,
    AiReaction,
    ClearCombo,
    InterruptNonMelee,
    InterruptMelee,
    PlayerAttackCancel,
    RemoveUnattackableAuras,
    CheckForHelp {
        target: ObjectHandle,
        radius_bits: u32,
    },
    DetectOrAttack(ObjectHandle),
    ZoneUnderAttack(ObjectHandle),
}
impl UnitAiCall {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::Home | Self::Chase { .. } => "execution.unit_ai.f5_05_motion_owner",
            Self::RemoveAurasOnEvade => "execution.unit_ai.f5_12_evade_aura_owner",
            Self::InitialSpellCooldowns | Self::TeleportCast(_) => {
                "execution.unit_ai.f5_07_spell_owner"
            }
            Self::GuardPosition(_) => "execution.unit_ai.f6_geometry_owner",
            Self::CombatStopWithPets => "execution.unit_ai.combat_stop_owner",
            Self::ThreatCurrent(_) => "execution.unit_ai.threat_current_owner",
            Self::OffhandTimer => "execution.unit_ai.offhand_timer_owner",
            Self::ClearCombo => "execution.unit_ai.combo_owner",
            Self::InterruptNonMelee | Self::InterruptMelee | Self::RemoveUnattackableAuras => {
                "execution.unit_ai.f5_07_spell_owner"
            }
            Self::AiReaction | Self::PlayerAttackCancel => "execution.unit_ai.presentation_owner",
            Self::CheckForHelp { .. } => "execution.unit_ai.f5_06_assistance_owner",
            Self::DetectOrAttack(_) => "execution.unit_ai.f6_detection_owner",
            Self::ZoneUnderAttack(_) => "execution.unit_ai.presentation_owner",
            Self::AddInitialThreat(_) => "execution.unit_ai.threat_context_binding_owner",
            Self::PropagateCombat(_) => "execution.unit_ai.f5_15_controller_combat_owner",
            Self::InterruptNonCombatCasts => "execution.unit_ai.f5_07_spell_owner",
            Self::CreatureCombatStart(_) => "execution.unit_ai.combat_start_addon_callback_owner",
            Self::TriggerHome | Self::TriggerEvade | Self::EnterCombat(_) => {
                "execution.unit_ai.lifecycle_callback_owner"
            }
            Self::Immobilized(_) => "execution.unit_ai.f5_05_immobilized_owner",
            Self::MeleeStart(_) | Self::MeleeStop(_) | Self::MinimapPing => {
                "execution.unit_ai.presentation_owner"
            }
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAiSightQuery {
    TargetFlying,
    WithinVerticalRange(u32),
    Dungeon,
    AssistPlayer,
    AttackOnSight,
    Accessible,
}
pub trait UnitAiExecutionOwner {
    fn call(
        &mut self,
        _unit: ObjectHandle,
        call: UnitAiCall,
        _units: &mut SourceUnits,
        _objects: &SourceObjects,
    ) -> Result<()> {
        Err(call.missing_owner())
    }
    fn execute_action(
        &mut self,
        _ai: &mut SourceUnitAi,
        _action: u32,
        _units: &mut SourceUnits,
        _objects: &SourceObjects,
    ) -> Result<()> {
        Err(refuse("execution.unit_ai.execute_action_override_owner"))
    }
    fn sight_query(
        &mut self,
        _unit: ObjectHandle,
        _who: ObjectHandle,
        query: UnitAiSightQuery,
    ) -> Result<bool> {
        Err(refuse(match query {
            UnitAiSightQuery::TargetFlying => "execution.unit_ai.f5_05_flight_state_owner",
            UnitAiSightQuery::AssistPlayer => "execution.unit_ai.f5_06_assistance_owner",
            _ => "execution.unit_ai.f6_sight_owner",
        }))
    }
    fn add_initial_threat(
        &mut self,
        unit: ObjectHandle,
        target: ObjectHandle,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.call(unit, UnitAiCall::AddInitialThreat(target), units, objects)
    }
    fn has_unattackable_aura(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.unit_ai.f5_12_attack_aura_owner"))
    }
    fn area(&mut self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.unit_ai.f6_area_owner"))
    }
    /// C rand is process-owned, not the thread MT stream. A reached unbound call refuses.
    fn c_rand(&mut self) -> Result<u32> {
        Err(refuse("execution.unit_ai.process_c_rng_owner"))
    }
    fn spell_list_has_spells(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.unit_ai.spell_list_owner"))
    }
    fn reachable(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.unit_ai.f5_05_motion_owner"))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceUnitAi {
    pub unit: ObjectHandle,
    pub attack_distance: f32,
    pub attack_angle: f32,
    pub move_further: bool,
    pub chase_run: bool,
    pub visibility_distance: f32,
    pub combat_movement_started: bool,
    pub dismount_on_aggro: bool,
    pub melee_enabled: bool,
    pub combat_only_root: bool,
    /// Source enum is retained numerically, including EventAI's unchecked enum assignments.
    pub react_state: u32,
    pub combat_script: bool,
    pub order: u32,
    pub current_spell: Option<u32>,
    pub teleport_unreachable: bool,
    pub ranged_mode: bool,
    pub ranged_setting: u32,
    pub chase_distance: f32,
    pub current_ranged_mode: bool,
    pub main_spell_id: u32,
    pub main_spell_cost: u32,
    pub main_spell_info: Option<u32>,
    pub main_spell_min_range: f32,
    pub main_attack_mask: u32,
    pub distancing_cooldown: bool,
    pub spell_list_cooldown: bool,
    action_ready: Vec<bool>,
    pub generic_timers: [SourceAiTimer; 2],
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct SourceAiTimer {
    pub id: u32,
    pub remaining: u32,
    pub disabled: bool,
}
impl SourceUnitAi {
    /// UnitAI.cpp:39-61, TimerAI.h:120 value-initializes ready bits to false.
    pub fn construct(objects: &SourceObjects, unit: ObjectHandle, actions: u32) -> Result<Self> {
        objects.get(unit)?;
        Ok(Self {
            unit,
            attack_distance: 0.,
            attack_angle: 0.,
            move_further: false,
            chase_run: true,
            visibility_distance: 166.,
            combat_movement_started: false,
            dismount_on_aggro: true,
            melee_enabled: true,
            combat_only_root: false,
            react_state: 2,
            combat_script: false,
            order: 0,
            current_spell: None,
            teleport_unreachable: false,
            ranged_mode: false,
            ranged_setting: 0,
            chase_distance: 0.,
            current_ranged_mode: false,
            main_spell_id: 0,
            main_spell_cost: 0,
            main_spell_info: None,
            main_spell_min_range: 0.,
            main_attack_mask: 0,
            distancing_cooldown: false,
            spell_list_cooldown: true,
            action_ready: vec![false; actions as usize],
            generic_timers: [
                SourceAiTimer {
                    id: 2000,
                    remaining: 0,
                    disabled: true,
                },
                SourceAiTimer {
                    id: 2001,
                    remaining: 0,
                    disabled: true,
                },
            ],
        })
    }
    pub fn action_count(&self) -> usize {
        self.action_ready.len()
    }
    pub fn action_ready(&self, index: u32) -> Result<bool> {
        self.action_ready
            .get(index as usize)
            .copied()
            .ok_or_else(|| refuse("execution.unit_ai.action_index_undefined"))
    }
    pub fn set_action_ready(&mut self, index: u32, value: bool) -> Result<()> {
        *self
            .action_ready
            .get_mut(index as usize)
            .ok_or_else(|| refuse("execution.unit_ai.action_index_undefined"))? = value;
        Ok(())
    }
    pub fn can_execute(&self, objects: &SourceObjects, units: &SourceUnits) -> Result<bool> {
        let f = units.get(objects, self.unit)?;
        Ok(objects.get(self.unit)?.alive
            && !f.crowd_controlled()
            && !f.evade_home
            && f.flags & 0x22000 != 0x22000
            && f.state & 0x500 == 0
            && !f.casting_non_melee
            && !self.combat_script)
    }
    pub fn move_in_line_of_sight(
        &mut self,
        who: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        config: &EventAiLoaderConfig,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if self.react_state < 1 {
            return Ok(());
        }
        let f = units.get(objects, self.unit)?;
        if !f.can_fly
            && owner.sight_query(self.unit, who, UnitAiSightQuery::TargetFlying)?
            && !owner.sight_query(
                self.unit,
                who,
                UnitAiSightQuery::WithinVerticalRange(if self.current_ranged_mode {
                    15f32.to_bits()
                } else {
                    3f32.to_bits()
                }),
            )?
        {
            return Ok(());
        }
        if f.victim.is_some() && !owner.sight_query(self.unit, who, UnitAiSightQuery::Dungeon)? {
            return Ok(());
        }
        if f.neutral_to_all
            .ok_or_else(|| refuse("execution.unit_ai.faction_template_owner"))?
        {
            return Ok(());
        }
        if objects
            .get(who)?
            .guid
            .is_some_and(|guid| guid.high() == HighGuid::Creature as u16)
            && units.get(objects, who)?.in_combat()
        {
            owner.call(
                self.unit,
                UnitAiCall::CheckForHelp {
                    target: who,
                    radius_bits: config.check_for_help_radius()?,
                },
                units,
                objects,
            )?;
        }
        if self.react_state != 2 {
            return Ok(());
        }
        let f = units.get(objects, self.unit)?;
        // Creature.cpp:2271-2282, not Unit's broader CAN_NOT_REACT mask.
        let creature = matches!(
            objects.get(self.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        );
        if f.state & if creature { 0xc } else { CANNOT_REACT } != 0
            || (!creature || !f.force_attacking) && f.flags & 0x0200_0002 != 0
            || creature
                && !f
                    .can_aggro
                    .ok_or_else(|| refuse("execution.unit_ai.f4_can_aggro_owner"))?
        {
            return Ok(());
        }
        if owner.sight_query(self.unit, who, UnitAiSightQuery::AssistPlayer)? {
            return Ok(());
        }
        if !owner.sight_query(self.unit, who, UnitAiSightQuery::AttackOnSight)? {
            return Ok(());
        }
        if !owner.sight_query(self.unit, who, UnitAiSightQuery::Accessible)? {
            return Ok(());
        }
        owner.call(self.unit, UnitAiCall::DetectOrAttack(who), units, objects)
    }
    pub fn enter_combat(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if owner.spell_list_has_spells(self.unit)? {
            self.spell_list_cooldown = false;
            owner.call(self.unit, UnitAiCall::InitialSpellCooldowns, units, objects)?;
        }
        Ok(())
    }
    pub fn just_died(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.clear_combat_root(objects, units, owner)
    }
    pub fn execute_actions(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if !self.can_execute(objects, units)? {
            return Ok(());
        }
        let count = self.action_ready.len();
        for i in 0..count {
            if self.combat_script {
                return Ok(());
            }
            if self.action_ready[i] {
                owner.execute_action(self, i as u32, units, objects)?;
            }
        }
        if self.teleport_unreachable {
            let f = units.get(objects, self.unit)?;
            if let Some(victim) = f.victim {
                if f.state & UNIT_STATE_NO_COMBAT_MOVEMENT == 0 && !owner.reachable(self.unit)? {
                    owner.call(self.unit, UnitAiCall::TeleportCast(victim), units, objects)?;
                }
            }
        }
        Ok(())
    }
    pub fn set_immobilized(
        &mut self,
        apply: bool,
        combat_only: bool,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if combat_only {
            self.combat_only_root = apply;
        }
        let f = units.get_mut(objects, self.unit)?;
        if apply {
            f.state |= UNIT_STATE_AI_ROOT;
        } else {
            f.state &= !UNIT_STATE_AI_ROOT;
        }
        owner.call(self.unit, UnitAiCall::Immobilized(apply), units, objects)
    }
    pub fn clear_combat_root(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if self.combat_only_root {
            units.get_mut(objects, self.unit)?.state &= !UNIT_STATE_AI_ROOT;
            owner.call(self.unit, UnitAiCall::Immobilized(false), units, objects)?;
            self.combat_only_root = false;
        }
        Ok(())
    }
    pub fn enter_evade(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.clear_combat_root(objects, units, owner)?;
        owner.call(self.unit, UnitAiCall::RemoveAurasOnEvade, units, objects)?;
        owner.call(self.unit, UnitAiCall::CombatStopWithPets, units, objects)?;
        if self.react_state != 0 && objects.get(self.unit)?.alive {
            let call = if units.get(objects, self.unit)?.immobilized() {
                UnitAiCall::TriggerHome
            } else {
                UnitAiCall::Home
            };
            owner.call(self.unit, call, units, objects)?;
        }
        owner.call(self.unit, UnitAiCall::TriggerEvade, units, objects)
    }
    pub fn attacked_by(
        &mut self,
        attacker: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let f = units.get(objects, self.unit)?;
        if !f.in_combat() && f.victim.is_none() {
            self.attack_start(attacker, objects, units, owner)?;
        }
        Ok(())
    }
    pub fn attack_start(
        &mut self,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let Some(target) = target else {
            return Ok(());
        };
        if self.react_state == 0 {
            return Ok(());
        }
        let previous = units.get(objects, self.unit)?.victim;
        if !self.attack_unit(target, objects, units, owner)? {
            return Ok(());
        }
        self.engage_in_combat(target, objects, units, owner)?;
        self.movement_on_attack(
            target,
            previous.is_some_and(|v| v != target),
            objects,
            units,
            owner,
        )
    }
    /// Unit.cpp:6259-6336. This operation does not test faction or player-only hostility.
    pub fn attack_unit(
        &self,
        target: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<bool> {
        let unit = self.unit;
        if unit == target {
            return Ok(false);
        }
        let victim = objects.get(target)?;
        if !objects.get(unit)?.alive || !victim.in_world || !victim.alive {
            return Ok(false);
        }
        let source = units.get(objects, unit)?;
        if objects.get(unit)?.kind == SourceObjectKind::Player && source.mounted {
            return Ok(false);
        }
        let target_fields = units.get(objects, target)?;
        if if victim.kind == SourceObjectKind::Player {
            target_fields.gm
        } else {
            target_fields.in_evade()
        } {
            return Ok(false);
        }
        if owner.has_unattackable_aura(unit)? {
            owner.call(unit, UnitAiCall::RemoveUnattackableAuras, units, objects)?;
        }
        if let Some(previous) = units.get(objects, unit)?.victim {
            if previous == target {
                let mut attacked = false;
                if self.can_execute(objects, units)? {
                    let f = units.get_mut(objects, unit)?;
                    attacked = f.target.is_none();
                    f.target = Some(target);
                }
                if self.melee_enabled {
                    self.melee_start(Some(target), objects, units, owner)?;
                    return Ok(true);
                }
                return Ok(attacked);
            }
            self.attack_stop(true, false, false, false, objects, units, owner)?;
        }
        if self.can_execute(objects, units)? {
            units.get_mut(objects, unit)?.target = Some(target);
        }
        let target_fields = units.get_mut(objects, target)?;
        if target_fields.attackers.contains(&unit) {
            return Ok(false);
        }
        target_fields.attackers.push(unit);
        units.get_mut(objects, unit)?.victim = Some(target);
        if units.can_have_threat_list(objects, unit)? {
            owner.call(unit, UnitAiCall::ThreatCurrent(target), units, objects)?;
        }
        if units.get(objects, unit)?.offhand {
            owner.call(unit, UnitAiCall::OffhandTimer, units, objects)?;
        }
        if self.melee_enabled {
            self.melee_start(Some(target), objects, units, owner)?;
        }
        owner.call(unit, UnitAiCall::AiReaction, units, objects)?;
        Ok(true)
    }
    pub fn melee_start(
        &self,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let f = units.get_mut(objects, self.unit)?;
        if f.state & UNIT_STATE_MELEE == 0 {
            f.state |= UNIT_STATE_MELEE;
            if target.is_some() {
                owner.call(self.unit, UnitAiCall::MeleeStart(target), units, objects)?;
            }
        }
        Ok(())
    }
    pub fn melee_stop(
        &self,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let f = units.get_mut(objects, self.unit)?;
        if f.state & UNIT_STATE_MELEE != 0 {
            f.state &= !UNIT_STATE_MELEE;
            if f.melee_spell {
                owner.call(self.unit, UnitAiCall::InterruptMelee, units, objects)?;
            }
            if target.is_some() {
                owner.call(self.unit, UnitAiCall::MeleeStop(target), units, objects)?;
            }
        }
        Ok(())
    }
    /// Unit.cpp:6339-6362. Target switching clears target before melee stop.
    #[allow(clippy::too_many_arguments)]
    pub fn attack_stop(
        &self,
        target_switch: bool,
        including_cast: bool,
        including_combo: bool,
        client_initiated: bool,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<bool> {
        if including_combo {
            owner.call(self.unit, UnitAiCall::ClearCombo, units, objects)?;
        }
        if including_cast && units.get(objects, self.unit)?.casting_non_melee {
            owner.call(self.unit, UnitAiCall::InterruptNonMelee, units, objects)?;
        }
        let player = objects.get(self.unit)?.kind == SourceObjectKind::Player;
        if target_switch && !player {
            units.get_mut(objects, self.unit)?.target = None;
        }
        if !client_initiated && player {
            owner.call(self.unit, UnitAiCall::PlayerAttackCancel, units, objects)?;
        }
        let victim = units.get(objects, self.unit)?.victim;
        self.melee_stop(victim, objects, units, owner)?;
        if let Some(victim) = victim {
            units
                .get_mut(objects, victim)?
                .attackers
                .retain(|a| *a != self.unit);
            units.get_mut(objects, self.unit)?.victim = None;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn movement_on_attack(
        &self,
        target: ObjectHandle,
        target_change: bool,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let f = units.get_mut(objects, self.unit)?;
        if f.state & CANNOT_REACT != 0 {
            return Ok(());
        }
        if self.dismount_on_aggro {
            f.mounted = false;
        }
        if f.state & 0x100 == 0 {
            owner.call(
                self.unit,
                UnitAiCall::Chase {
                    target,
                    target_change,
                    distance_bits: self.attack_distance.to_bits(),
                    angle_bits: self.attack_angle.to_bits(),
                    move_further: self.move_further,
                    walk: false,
                },
                units,
                objects,
            )?;
        }
        Ok(())
    }
    pub fn set_melee(
        &mut self,
        enabled: bool,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if enabled == self.melee_enabled {
            return Ok(());
        }
        self.melee_enabled = enabled;
        let f = units.get(objects, self.unit)?;
        if f.in_combat() {
            if enabled && f.state & UNIT_STATE_MELEE == 0 {
                if f.victim.is_some() {
                    self.melee_start(f.victim, objects, units, owner)?;
                }
            } else if f.state & UNIT_STATE_MELEE != 0 {
                self.melee_stop(f.victim, objects, units, owner)?;
            }
        }
        Ok(())
    }
    pub fn set_current_ranged(
        &mut self,
        enabled: bool,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if !enabled && self.ranged_setting == 3 {
            return Ok(());
        }
        self.current_ranged_mode = enabled;
        self.attack_distance = if enabled { self.chase_distance } else { 0. };
        let f = units.get_mut(objects, self.unit)?;
        f.ignore_ranged_targets = !enabled && f.state & 0x18 != 0;
        if let Some(target) = f.victim {
            owner.call(
                self.unit,
                UnitAiCall::Chase {
                    target,
                    target_change: false,
                    distance_bits: self.attack_distance.to_bits(),
                    angle_bits: self.attack_angle.to_bits(),
                    move_further: self.move_further,
                    walk: !self.chase_run,
                },
                units,
                objects,
            )?;
        }
        if !enabled
            && self.melee_enabled
            && units.get(objects, self.unit)?.state & UNIT_STATE_MELEE == 0
        {
            self.melee_start(units.get(objects, self.unit)?.victim, objects, units, owner)?;
        }
        Ok(())
    }
}

impl SourceUnitAi {
    /// UnitAI.cpp:974-989 keeps current ranged mode true in the OOC branch, even when disabling.
    pub fn set_ranged_mode(
        &mut self,
        enabled: bool,
        distance: f32,
        kind: u32,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if self.ranged_mode == enabled {
            return Ok(());
        }
        self.ranged_mode = enabled;
        self.chase_distance = distance;
        self.ranged_setting = kind;
        if units.get(objects, self.unit)?.in_combat() {
            self.set_current_ranged(enabled, objects, units, owner)?;
        } else {
            self.current_ranged_mode = true;
            self.attack_distance = self.chase_distance;
        }
        Ok(())
    }
    pub fn spell_cooldown_added(
        &mut self,
        spell: u32,
        main_spells: &std::collections::BTreeSet<u32>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if self.ranged_setting == 1 && main_spells.contains(&spell) {
            self.set_current_ranged(true, objects, units, owner)?;
        }
        Ok(())
    }
}

impl SourceUnitAi {
    /// ReactStates has the non-fixed enum range 0..=3, UnitAI.h:81-86.
    pub fn set_react_state(&mut self, value: u32) -> Result<()> {
        require(
            value <= 3,
            "execution.unit_ai.react_enum_conversion_undefined",
        )?;
        self.react_state = value;
        Ok(())
    }
}

impl SourceUnitAi {
    /// Unit.cpp:8017-8023. Threat precedes both combat entries and the target timer.
    pub fn engage_in_combat(
        &self,
        target: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        owner.add_initial_threat(self.unit, target, units, objects)?;
        units.set_in_combat_with(self.unit, target, objects, owner)?;
        units.set_in_combat_with(target, self.unit, objects, owner)?;
        units.trigger_target_combat_timer(self.unit, target, objects)
    }
}

impl SourceUnits {
    /// CombatManager.cpp:190-194, Creature.h:837 and Unit.h:1631.
    fn trigger_combat_timer(
        &mut self,
        unit: ObjectHandle,
        pvp: bool,
        objects: &SourceObjects,
    ) -> Result<()> {
        let f = self.get_mut(objects, unit)?;
        f.combat_timer = if pvp {
            5000
        } else if objects.get(unit)?.kind == SourceObjectKind::Player {
            15000
        } else {
            f.pursuit_ms
        };
        f.last_refresh_position = Some(objects.pose(unit)?.0);
        Ok(())
    }
    pub fn trigger_target_combat_timer(
        &mut self,
        source: ObjectHandle,
        target: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<()> {
        if self.can_have_threat_list(objects, target)? {
            let pvp = self.get(objects, source)?.flags & 8 != 0
                && self.get(objects, target)?.flags & 8 != 0;
            self.trigger_combat_timer(target, pvp, objects)?;
        }
        Ok(())
    }
    /// Unit.cpp:7909-7971. Foreign addon/AI/instance/assistance callbacks start at :7972.
    pub fn set_in_combat_with(
        &mut self,
        unit: ObjectHandle,
        enemy: ObjectHandle,
        objects: &SourceObjects,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        if !objects.get(unit)?.alive {
            return Ok(());
        }
        let first = !self.get(objects, unit)?.in_combat();
        let creature_first = objects.get(unit)?.kind != SourceObjectKind::Player && first;
        let pvp =
            self.get(objects, unit)?.flags & 8 != 0 && self.get(objects, enemy)?.flags & 8 != 0;
        if pvp || creature_first {
            self.trigger_combat_timer(unit, pvp, objects)?;
        }
        let f = self.get_mut(objects, unit)?;
        f.flags |= UNIT_FLAG_COMBAT;
        if f.owner.is_some() || f.charmer.is_some() {
            f.flags |= 0x800;
            if first || pvp {
                owner.call(unit, UnitAiCall::PropagateCombat(enemy), self, objects)?;
            }
        }
        if self.get(objects, unit)?.casting_non_melee {
            owner.call(unit, UnitAiCall::InterruptNonCombatCasts, self, objects)?;
        }
        if first {
            self.get_mut(objects, unit)?.combat_tick = 1200;
        }
        if creature_first {
            self.get_mut(objects, unit)?.damage_by_others = 0;
            owner.call(unit, UnitAiCall::CreatureCombatStart(enemy), self, objects)?;
        }
        Ok(())
    }
}
