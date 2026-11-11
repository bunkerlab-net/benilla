// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureAI.cpp:28-99, UnitAI.h:451-458. Component state, not native continuation.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct SourceCreatureAi {
    pub base: SourceUnitAi,
    pub follow_angle: f32,
    pub follow_distance: f32,
    pub guard: bool,
    pub civilian: bool,
}
impl SourceCreatureAi {
    #[allow(clippy::too_many_arguments)]
    pub fn construct(
        objects: &SourceObjects,
        unit: ObjectHandle,
        units: &mut SourceUnits,
        template: &CreatureAiTemplateOperands,
        charmed: bool,
        combat_actions: u32,
        config: &EventAiLoaderConfig,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<Self> {
        require(
            units.get(objects, unit)?.entry == template.entry,
            "execution.unit_ai.effective_entry",
        )?;
        let mut base = SourceUnitAi::construct(objects, unit, combat_actions)?;
        base.dismount_on_aggro =
            template.type_flags & 0x800 == 0 && template.static_flags[1] & 0x40000 == 0;
        base.set_melee(
            units.get(objects, unit)?.static_flags[0] & 0x100000 == 0
                && units.get(objects, unit)?.static_flags[3] & 0x200 == 0
                && template.extra_flags & 0x20000 == 0,
            objects,
            units,
            owner,
        )?;
        if units.get(objects, unit)?.static_flags[0] & 0x100 != 0 {
            base.set_immobilized(true, false, objects, units, owner)?;
        }
        if template.extra_flags & 2 != 0 {
            base.react_state = 1;
        }
        if units.get(objects, unit)?.static_flags[1] & 0x4000000 != 0 {
            base.react_state = 1;
        } else if units.get(objects, unit)?.static_flags[0] & 0x2000000 != 0 {
            units.get_mut(objects, unit)?.can_enter_combat = false;
        }
        let guard = template.extra_flags & 0x400 != 0;
        if guard || charmed {
            base.visibility_distance = f32::from_bits(config.guard_sight()?);
        }
        Ok(Self {
            base,
            follow_angle: 0.,
            follow_distance: 0.,
            guard,
            civilian: template.civilian,
        })
    }
    pub fn reset(&mut self) {
        self.base.current_ranged_mode = self.base.ranged_mode;
        self.base.attack_distance = self.base.chase_distance;
    }
    /// UnitAI.cpp:137-140 dispatches the virtual CreatureAI::AttackStart override.
    pub fn attacked_by(
        &mut self,
        attacker: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let fields = units.get(objects, self.base.unit)?;
        if !fields.in_combat() && fields.victim.is_none() {
            self.attack_start(attacker, objects, units, owner)?;
        }
        Ok(())
    }
    pub fn enter_combat(
        &mut self,
        enemy: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.base.enter_combat(objects, units, owner)?;
        if let Some(enemy) = enemy {
            if self.guard || self.civilian {
                if let Some(player) = units.get(objects, enemy)?.beneficiary {
                    owner.call(
                        self.base.unit,
                        UnitAiCall::ZoneUnderAttack(player),
                        units,
                        objects,
                    )?;
                }
            }
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
        let unit = self.base.unit;
        // The ping precedes even the null/passive checks, CreatureAI.cpp:75-87.
        if units.get(objects, unit)?.static_flags[0] & 0x20000 != 0 {
            if let Some(spawner) = units.get(objects, unit)?.spawner {
                if objects.get(spawner)?.kind == SourceObjectKind::Player {
                    owner.call(unit, UnitAiCall::MinimapPing, units, objects)?;
                }
            }
        }
        let Some(target) = target else {
            return Ok(());
        };
        if self.base.react_state == 0 {
            return Ok(());
        }
        let previous = units.get(objects, unit)?.victim;
        if self.base.attack_unit(target, objects, units, owner)? {
            self.base.engage_in_combat(target, objects, units, owner)?;
            if units.get(objects, unit)?.static_flags[0] & 0x8000000 != 0 {
                self.spawn_guard(objects, units, owner)?;
            }
            self.base.movement_on_attack(
                target,
                previous.is_some_and(|v| v != target),
                objects,
                units,
                owner,
            )?;
        }
        Ok(())
    }
    pub fn spawn_guard(
        &self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        let area = owner.area(self.base.unit)?;
        let entry = guard_entry(area, owner)?;
        // Even an unrecognized area reaches GetRandomPoint before guardEntry is checked.
        owner.call(
            self.base.unit,
            UnitAiCall::GuardPosition(entry),
            units,
            objects,
        )
    }
    pub fn enter_evade(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.base.enter_evade(objects, units, owner)?;
        self.reset_timers(false)?;
        self.reset();
        Ok(())
    }
    pub fn reset_timers(&mut self, all: bool) -> Result<()> {
        // EventAI has zero combat actions and only these two disabled ALWAYS timers.
        require(
            self.base.action_count() == 0,
            "execution.unit_ai.f5_17_combat_timer_owner",
        )?;
        if all {
            for timer in &mut self.base.generic_timers {
                timer.remaining = 0;
                timer.disabled = true;
            }
        }
        Ok(())
    }
}
/// CreatureAI.cpp:294-358. Only areas 221/222 consume the process C stream.
pub fn guard_entry(area: u32, owner: &mut impl UnitAiExecutionOwner) -> Result<u32> {
    Ok(match area {
        9 | 24 => 1642,
        42 => 10038,
        69 => 10037,
        87 => 1423,
        108 => 8096,
        131 => 727,
        132 => 853,
        144 => 8055,
        152 | 154 | 159 => 7980,
        188 => 4844,
        186 => 3571,
        221 | 222 => [3210, 3211, 3213, 3214][(owner.c_rand()? % 4) as usize],
        228 => 7489,
        271 => 2386,
        272 => 2405,
        320 => 10696,
        321 => 2621,
        340 => 8155,
        362 => 5953,
        363 => 5952,
        367 => 8017,
        380 => 3501,
        415 => 6087,
        431 | 2897 => 12903,
        442 => 6086,
        460 => 7730,
        484 => 9525,
        513 => 4979,
        597 => 8154,
        608 => 8151,
        1099 => 8147,
        1116 => 7939,
        1497 => 5624,
        1519 => 68,
        1537 => 5595,
        1637 => 3296,
        1638 => 3084,
        1657 => 4262,
        2408 => 12338,
        3462 | 3487 | 3488 | 3665 => 16222,
        3527 => 16921,
        3557 => 16733,
        3576 => 18038,
        3584 => 17549,
        _ => 0,
    })
}
#[derive(Clone, Debug, Default)]
pub struct SourceCreatureAis {
    instances: BTreeMap<ObjectHandle, SourceCreatureAi>,
}
impl SourceCreatureAis {
    pub fn install(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ai: SourceCreatureAi,
        combat: &mut SourceCombat,
    ) -> Result<()> {
        let unit = ai.base.unit;
        require(
            !self.instances.contains_key(&unit),
            "execution.unit_ai.duplicate_generation",
        )?;
        units.get_mut(objects, unit)?.has_ai = true;
        combat.reset_death_prevented(unit);
        self.instances.insert(unit, ai);
        Ok(())
    }
    pub fn get(&self, objects: &SourceObjects, unit: ObjectHandle) -> Result<&SourceCreatureAi> {
        objects.get(unit)?;
        self.instances
            .get(&unit)
            .ok_or_else(|| refuse("execution.unit_ai.generation_missing"))
    }
    pub fn get_mut(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
    ) -> Result<&mut SourceCreatureAi> {
        objects.get(unit)?;
        self.instances
            .get_mut(&unit)
            .ok_or_else(|| refuse("execution.unit_ai.generation_missing"))
    }
    pub fn inventory(&self, objects: &SourceObjects) -> Result<serde_json::Value> {
        for unit in self.instances.keys() {
            self.get(objects, *unit)?;
        }
        Ok(
            serde_json::json!({"version":1,"instances":self.instances.iter().collect::<Vec<_>>(),"restore":"execution.unit_ai.f7_restore_owner"}),
        )
    }
}
/// Concrete live fields for the F5.04a owner. Geometry/aura/modifier queries remain refused.
pub struct SourceUnitQueries<'a> {
    pub objects: &'a SourceObjects,
    pub units: &'a mut SourceUnits,
}
impl ThreatExecutionOwner for SourceUnitQueries<'_> {
    fn can_enter_combat(&mut self, unit: ObjectHandle) -> Result<bool> {
        let f = self.units.get(self.objects, unit)?;
        Ok(f.can_enter_combat && !f.evade_home)
    }
    fn crowd_controlled(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.units.get(self.objects, unit)?.crowd_controlled())
    }
    fn threat_unit(&mut self, unit: ObjectHandle) -> Result<ThreatUnit> {
        let object = self.objects.get(unit)?;
        let f = self.units.get(self.objects, unit)?;
        Ok(ThreatUnit {
            guid: object
                .guid
                .ok_or_else(|| refuse("execution.unit.guid_absent"))?
                .raw(),
            player: object.kind == SourceObjectKind::Player,
            gm: f.gm,
            alive: object.alive,
            no_threat: f.no_threat,
            master: f.owner,
            feign_death: f.state & 4 != 0,
            can_have_threat_list: self.units.can_have_threat_list(self.objects, unit)?,
        })
    }
    fn ignoring_ranged(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.units.get(self.objects, unit)?.ignore_ranged_targets)
    }
    fn combat_timer(&mut self, source: ObjectHandle, target: ObjectHandle) -> Result<()> {
        self.units
            .trigger_target_combat_timer(source, target, self.objects)
    }
    fn threat_lookup(&mut self, source: ObjectHandle, guid: u64) -> Result<Option<ObjectHandle>> {
        let map = self
            .objects
            .get(source)?
            .map
            .ok_or_else(|| refuse("execution.unit.map_absent"))?;
        let guid = SourceGuid::from_raw(guid);
        // Map.cpp:2331-2340,2371-2376 permits registered NPCs before base AddToWorld.
        Ok(match guid.high() {
            0 | 0xf130 | 0xf140 => self.objects.lookup_world_object(map, guid),
            _ => None,
        })
    }
}
impl CombatExecutionOwner for SourceUnitQueries<'_> {
    fn combat_unit(&mut self, unit: ObjectHandle) -> Result<CombatUnit> {
        let object = self.objects.get(unit)?;
        require(
            object.kind != SourceObjectKind::Player,
            "execution.unit.player_extra_flags_owner",
        )?;
        let f = self.units.get(self.objects, unit)?;
        let vitals = f
            .vitals
            .ok_or_else(|| refuse("execution.unit.select_level_stats_owner"))?;
        Ok(CombatUnit {
            health: vitals.health,
            alive: object.alive,
            taxi: f.state & 0x80 != 0,
            evade: f.in_evade(),
            has_ai: f.has_ai,
            unkillable: f.static_flags[0] & 8 != 0,
            preventing_death: f.static_flags[0] & 8 != 0,
        })
    }
}
/// F5.03 dispatch reads the same generation-owned base and Unit state.
pub struct SourceAiExecution<'a, O: UnitAiExecutionOwner + CreatureRandom> {
    pub queries: SourceUnitQueries<'a>,
    pub ais: &'a mut SourceCreatureAis,
    pub threats: &'a mut SourceThreats,
    pub owner: &'a mut O,
}
impl<O: UnitAiExecutionOwner + CreatureRandom> CreatureRandom for SourceAiExecution<'_, O> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.owner.creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.owner.creature_urand(min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.owner.creature_urand_default()
    }
}
impl<O: UnitAiExecutionOwner + CreatureRandom> EventAiExecutionOwner for SourceAiExecution<'_, O> {
    fn threat_context(&mut self) -> Result<ThreatContext<'_>> {
        Ok(ThreatContext {
            state: self.threats,
            owner: &mut self.queries,
        })
    }
    fn unit(&mut self, unit: ObjectHandle) -> Result<EventAiUnit> {
        self.queries.units.event_unit(self.queries.objects, unit)
    }
    fn combat_state(&mut self, unit: ObjectHandle) -> Result<(bool, bool)> {
        let f = self.queries.units.get(self.queries.objects, unit)?;
        Ok((f.in_combat(), f.in_evade()))
    }
    fn victim(&mut self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.queries.units.get(self.queries.objects, unit)?.victim)
    }
    fn current_ranged_mode(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self
            .ais
            .get(self.queries.objects, unit)?
            .base
            .current_ranged_mode)
    }
    fn can_execute_combat_action(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.ais
            .get(self.queries.objects, unit)?
            .base
            .can_execute(self.queries.objects, self.queries.units)
    }
    fn combat_movement(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.queries.units.get(self.queries.objects, unit)?.state
            & UNIT_STATE_NO_COMBAT_MOVEMENT
            == 0)
    }
    fn unit_flag(&mut self, unit: ObjectHandle, value: u32, set: bool) -> Result<()> {
        self.queries
            .units
            .get_mut(self.queries.objects, unit)?
            .set_flag(value, set);
        Ok(())
    }
    fn spawn_map(&mut self, unit: ObjectHandle) -> Result<u32> {
        Ok(self
            .queries
            .objects
            .get(unit)?
            .map
            .ok_or_else(|| refuse("execution.unit.map_absent"))?
            .map)
    }
    fn lifecycle(&mut self, state: &mut SourceEventAi, call: EventAiCallback) -> Result<()> {
        let unit = state.selected.object();
        let ai = self.ais.get_mut(self.queries.objects, unit)?;
        match call {
            EventAiCallback::ResetAllBaseTimers => ai.reset_timers(true)?,
            EventAiCallback::RestoreRangedModeAndAttackDistance => ai.reset(),
            EventAiCallback::SetAiOrderNone => ai.base.order = 0,
            EventAiCallback::ClearCombatScriptStatus => ai.base.combat_script = false,
            EventAiCallback::ClearCombatOnlyRoot => {
                ai.base
                    .clear_combat_root(self.queries.objects, self.queries.units, self.owner)?
            }
            EventAiCallback::SetReactDefensive(defensive) => {
                ai.base.react_state = if defensive { 1 } else { 2 }
            }
            EventAiCallback::DisableCombatEntry => {
                self.queries
                    .units
                    .get_mut(self.queries.objects, unit)?
                    .can_enter_combat = false
            }
            EventAiCallback::CalculateMainSpell(spell) => {
                if ai.base.main_spell_id == 0 {
                    ai.base.main_spell_id = spell;
                }
                return Err(call.missing_owner());
            }
            EventAiCallback::BaseEnterCombat(enemy) => {
                ai.enter_combat(enemy, self.queries.objects, self.queries.units, self.owner)?;
            }
        }
        Ok(())
    }
    fn dependent_action(
        &mut self,
        state: &mut SourceEventAi,
        action: &EventAiAction,
        call: EventAiInvocation,
        event_target: Option<ObjectHandle>,
        _: u32,
    ) -> Result<bool> {
        let unit = state.selected.object();
        let p = action
            .params
            .ok_or_else(|| refuse("execution.event_ai.action_parameters"))?;
        match action.kind {
            20 => self
                .ais
                .get_mut(self.queries.objects, unit)?
                .base
                .set_melee(
                    p[0] != 0,
                    self.queries.objects,
                    self.queries.units,
                    self.owner,
                )?,
            24 => self.ais.get_mut(self.queries.objects, unit)?.enter_evade(
                self.queries.objects,
                self.queries.units,
                self.owner,
            )?,
            50 => self
                .ais
                .get_mut(self.queries.objects, unit)?
                .base
                .set_react_state(p[0])?,
            55 => {
                let target = state.action_target(p[0], call, event_target, self)?;
                if target.is_none() {
                    return Ok(false);
                }
                if let Some(target) = target {
                    let mut dependencies = SourceAiDependencies {
                        threats: self.threats,
                        owner: self.owner,
                    };
                    self.ais.get_mut(self.queries.objects, unit)?.attack_start(
                        Some(target),
                        self.queries.objects,
                        self.queries.units,
                        &mut dependencies,
                    )?;
                }
            }
            57 => {
                let ai = self.ais.get_mut(self.queries.objects, unit)?;
                if ai.base.main_spell_info.is_some() {
                    ai.base.set_ranged_mode(
                        p[0] != 0,
                        p[1] as f32,
                        p[0],
                        self.queries.objects,
                        self.queries.units,
                        self.owner,
                    )?;
                }
            }
            // CreatureEventAI.cpp:1347-1350, with the real Unit root owner.
            64 => self
                .ais
                .get_mut(self.queries.objects, unit)?
                .base
                .set_immobilized(
                    p[0] != 0,
                    p[1] != 0,
                    self.queries.objects,
                    self.queries.units,
                    self.owner,
                )?,
            _ => return Err(refuse(event_ai_action_owner(action.kind))),
        }
        Ok(true)
    }
}

impl<O: BuiltinAiExecutionOwner + HostileTargetExecutionOwner + CreatureRandom>
    SourceAiExecution<'_, O>
{
    /// CreatureEventAI.h:898 overrides only UnitAI.h:504, not UpdateAI.
    pub fn update_event_ai(&mut self, state: &mut SourceEventAi, diff: u32) -> Result<()> {
        state.validate(self.queries.objects)?;
        let unit = state.selected.object();
        let combat = ai::inherited_update_prefix(
            self.ais.get_mut(self.queries.objects, unit)?,
            diff,
            self.queries.objects,
            self.queries.units,
            &mut SourceAiDependencies {
                threats: self.threats,
                owner: self.owner,
            },
        )?;
        state.update_timers(diff, &mut EventAiDispatcher(self))?;
        ai::inherited_update_combat(
            self.ais.get_mut(self.queries.objects, unit)?,
            combat,
            self.queries.objects,
            self.queries.units,
            &mut SourceAiDependencies {
                threats: self.threats,
                owner: self.owner,
            },
        )
    }
}

/// Binds Unit::Attack/Engage calls to the same concrete threat managers as EventAI.
pub struct SourceAiDependencies<'a, O: UnitAiExecutionOwner> {
    pub threats: &'a mut SourceThreats,
    pub owner: &'a mut O,
}
impl<O: UnitAiExecutionOwner> UnitAiExecutionOwner for SourceAiDependencies<'_, O> {
    fn call(
        &mut self,
        unit: ObjectHandle,
        call: UnitAiCall,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        if let UnitAiCall::ThreatCurrent(target) = call {
            let guid = objects
                .get(target)?
                .guid
                .ok_or_else(|| refuse("execution.unit.guid_absent"))?
                .raw();
            return self.threats.set_current(unit, Some(guid));
        }
        self.owner.call(unit, call, units, objects)
    }
    fn add_initial_threat(
        &mut self,
        unit: ObjectHandle,
        target: ObjectHandle,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.threats.unit_add(
            unit,
            Some(target),
            ThreatValue::ZERO,
            false,
            0,
            None,
            &mut SourceUnitQueries { objects, units },
        )
    }
    fn execute_action(
        &mut self,
        ai: &mut SourceUnitAi,
        action: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.owner.execute_action(ai, action, units, objects)
    }
    fn sight_query(
        &mut self,
        unit: ObjectHandle,
        target: ObjectHandle,
        query: UnitAiSightQuery,
    ) -> Result<bool> {
        self.owner.sight_query(unit, target, query)
    }
    fn has_unattackable_aura(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.owner.has_unattackable_aura(unit)
    }
    fn area(&mut self, unit: ObjectHandle) -> Result<u32> {
        self.owner.area(unit)
    }
    fn c_rand(&mut self) -> Result<u32> {
        self.owner.c_rand()
    }
    fn spell_list_has_spells(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.owner.spell_list_has_spells(unit)
    }
    fn reachable(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.owner.reachable(unit)
    }
}

impl<O: BuiltinAiExecutionOwner + HostileTargetExecutionOwner> BuiltinAiExecutionOwner
    for SourceAiDependencies<'_, O>
{
    fn select_hostile_target(
        &mut self,
        ai: &mut SourceCreatureAi,
        objects: &SourceObjects,
        units: &mut SourceUnits,
    ) -> Result<bool> {
        ai.select_hostile_target(objects, units, self.threats, self.owner)
    }
    fn update_melee(
        &mut self,
        unit: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
    ) -> Result<()> {
        self.owner.update_melee(unit, objects, units)
    }
    fn guard_query(
        &mut self,
        unit: ObjectHandle,
        target: ObjectHandle,
        query: GuardQuery,
    ) -> Result<bool> {
        self.owner.guard_query(unit, target, query)
    }
    fn helper_attacker(&mut self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.owner.helper_attacker(unit)
    }
}
