// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Unit.cpp:8681-8807. No aura absence, geometry or motion result is inferred.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostileTargetQuery {
    MeleeReach,
    MotionReachable,
    TargetFlying,
    AboveMeleeZ,
    ChaseMotion,
    CanAttack,
    Accessible,
}
impl HostileTargetQuery {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::MeleeReach => "execution.select_hostile_target.f6_melee_reach_owner",
            Self::MotionReachable => "execution.select_hostile_target.f5_05_f6_reachability_owner",
            Self::TargetFlying => "execution.select_hostile_target.f5_05_flight_state_owner",
            Self::AboveMeleeZ => "execution.select_hostile_target.f6_distance_z_owner",
            Self::ChaseMotion => "execution.select_hostile_target.f5_05_current_motion_owner",
            Self::CanAttack => "execution.select_hostile_target.f6_attackability_owner",
            Self::Accessible => "execution.select_hostile_target.f6_accessible_place_owner",
        })
    }
}
/// Only foreign operations. Cached taunt priorities already belong to SourceThreats.
pub trait HostileTargetExecutionOwner: UnitAiExecutionOwner {
    fn hostile_query(
        &mut self,
        _unit: ObjectHandle,
        _target: Option<ObjectHandle>,
        query: HostileTargetQuery,
    ) -> Result<bool> {
        Err(query.missing_owner())
    }
    fn set_in_front(&mut self, _unit: ObjectHandle, _target: ObjectHandle) -> Result<()> {
        Err(refuse(
            "execution.select_hostile_target.f6_set_in_front_owner",
        ))
    }
    /// ThreatManager.cpp:559 reads the actual ordered SPELL_AURA_MOD_TAUNT list.
    fn update_taunt_auras(
        &mut self,
        _unit: ObjectHandle,
        _threats: &mut SourceThreats,
    ) -> Result<()> {
        Err(refuse(
            "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner",
        ))
    }
    fn propagate_evade_none(&mut self, _unit: ObjectHandle) -> Result<()> {
        Err(refuse(
            "execution.select_hostile_target.f5_15_controlled_evade_owner",
        ))
    }
}

impl SourceCreatureAi {
    /// Unit.cpp:8691-8807. The return is combat eligibility, not victim presence.
    pub fn select_hostile_target(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        threats: &mut SourceThreats,
        owner: &mut impl HostileTargetExecutionOwner,
    ) -> Result<bool> {
        let unit = self.base.unit;
        if !objects.get(unit)?.alive || !units.get(objects, unit)?.has_ai {
            return Ok(false);
        }
        if !self.base.can_execute(objects, units)? {
            if let Some(target) = map_target(objects, units, unit)? {
                if target != unit {
                    owner.set_in_front(unit, target)?;
                }
            }
            let f = units.get(objects, unit)?;
            if f.in_combat()
                && threats.manager(unit)?.online().is_empty()
                && f.crowd_controlled()
                && !self.base.combat_script
            {
                self.evade_without_fixate(objects, units, threats, owner)?;
            }
            // Re-read after virtual EnterEvadeMode, which can change combat-script state.
            return Ok(!((self.base.combat_script
                || units.get(objects, unit)?.crowd_controlled())
                && threats.manager(unit)?.online().is_empty()));
        }
        let old_target = units.get(objects, unit)?.victim;
        let mut target = threats.select_if_nonempty(
            unit,
            &mut SelectionThreatQueries {
                objects,
                units,
                owner,
            },
        )?;
        if target.is_none() {
            if let Some(old) = old_target {
                if units.get(objects, unit)?.ignore_ranged_targets {
                    if owner.hostile_query(unit, Some(old), HostileTargetQuery::MeleeReach)? {
                        target = Some(old);
                    } else {
                        // This is NOT AttackStop: no melee-state clearing or packet, :8738-8740.
                        let f = units.get_mut(objects, unit)?;
                        f.victim = None;
                        f.target = None;
                        units
                            .get_mut(objects, old)?
                            .attackers
                            .retain(|a| *a != unit);
                    }
                }
            }
        }
        if let Some(target) = target {
            if !self.base.combat_script {
                owner.set_in_front(unit, target)?;
                if old_target != Some(target) || unit_target(objects, units, unit)? != Some(target)
                {
                    self.attack_start(
                        Some(target),
                        objects,
                        units,
                        &mut SourceAiDependencies { threats, owner },
                    )?;
                }
            }
            let mut evade = false;
            if units.get(objects, unit)?.state & UNIT_STATE_NO_COMBAT_MOVEMENT == 0 {
                if !owner.hostile_query(unit, None, HostileTargetQuery::MotionReachable)? {
                    if !self.base.current_ranged_mode
                        && threats.manager(unit)?.online().len() == 1
                        && owner.hostile_query(
                            unit,
                            Some(target),
                            HostileTargetQuery::TargetFlying,
                        )?
                        && owner.hostile_query(
                            unit,
                            Some(target),
                            HostileTargetQuery::AboveMeleeZ,
                        )?
                    {
                        let guid = objects
                            .get(target)?
                            .guid
                            .ok_or_else(|| refuse("execution.unit.guid_absent"))?
                            .raw();
                        threats.modify_percent(
                            unit,
                            guid,
                            -101,
                            &mut SourceUnitQueries { objects, units },
                        )?;
                        evade = true;
                    } else if !units.get(objects, unit)?.in_evade() {
                        // CombatManager.h:46, not a timer reset on each failed path check.
                        units.get_mut(objects, unit)?.evade_timer = 10000;
                    }
                } else {
                    self.stop_reachable_evade(objects, units, owner)?;
                }
            }
            if !evade {
                return Ok(true);
            }
        }
        if units.get(objects, unit)?.ignore_ranged_targets
            && !threats.manager(unit)?.online().is_empty()
        {
            return Ok(true);
        }
        if !units.get(objects, unit)?.in_combat() {
            return Ok(false);
        }
        if !owner.hostile_query(unit, None, HostileTargetQuery::ChaseMotion)? {
            order_attackers(objects, units, unit)?;
            // Source iterates the actual attacker set; there is no nearest-target search.
            for attacker in &units.get(objects, unit)?.attackers {
                let attacker_object = objects.get(*attacker)?;
                let source = objects.get(unit)?;
                if source.in_world
                    && attacker_object.in_world
                    && source.map == attacker_object.map
                    && owner.hostile_query(unit, Some(*attacker), HostileTargetQuery::CanAttack)?
                    && owner.hostile_query(unit, Some(*attacker), HostileTargetQuery::Accessible)?
                {
                    return Ok(false);
                }
            }
        }
        self.evade_without_fixate(objects, units, threats, owner)?;
        Ok(false)
    }

    fn evade_without_fixate(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        threats: &mut SourceThreats,
        owner: &mut impl HostileTargetExecutionOwner,
    ) -> Result<()> {
        let unit = self.base.unit;
        // Unit.cpp:8683 gates FixateTarget only, not the following virtual evade call.
        if units.can_have_threat_list(objects, unit)? {
            threats.fixate_references(unit, None)?;
            owner.update_taunt_auras(unit, threats)?;
        }
        self.enter_evade(objects, units, owner)
    }

    fn stop_reachable_evade(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        owner: &mut impl HostileTargetExecutionOwner,
    ) -> Result<()> {
        let f = units.get_mut(objects, self.base.unit)?;
        if !f.in_evade() {
            return Ok(());
        }
        // CombatManager.cpp:144-176 clears only the timer when it is nonzero.
        if f.evade_timer != 0 {
            f.evade_timer = 0;
            return Ok(());
        }
        if f.evade_home {
            self.base.order = 0;
        }
        f.evade_home = false;
        f.evade_combat = false;
        if f.charmer.is_none() {
            owner.propagate_evade_none(self.base.unit)?;
        }
        Ok(())
    }
}

struct SelectionThreatQueries<'a, O> {
    objects: &'a SourceObjects,
    units: &'a mut SourceUnits,
    owner: &'a mut O,
}
impl<O: HostileTargetExecutionOwner> ThreatExecutionOwner for SelectionThreatQueries<'_, O> {
    fn threat_unit(&mut self, unit: ObjectHandle) -> Result<ThreatUnit> {
        SourceUnitQueries {
            objects: self.objects,
            units: self.units,
        }
        .threat_unit(unit)
    }
    fn ignoring_ranged(&mut self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.units.get(self.objects, unit)?.ignore_ranged_targets)
    }
    fn melee_reach(&mut self, unit: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        self.objects.get(target)?;
        self.owner
            .hostile_query(unit, Some(target), HostileTargetQuery::MeleeReach)
    }
    fn can_attack(&mut self, unit: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        self.objects.get(target)?;
        self.owner
            .hostile_query(unit, Some(target), HostileTargetQuery::CanAttack)
    }
}

/// Map::GetUnit at Unit.cpp:8713, including registered-before-world NPCs.
fn map_target(
    objects: &SourceObjects,
    units: &mut SourceUnits,
    unit: ObjectHandle,
) -> Result<Option<ObjectHandle>> {
    let Some(target) = units.get(objects, unit)?.target else {
        return Ok(None);
    };
    let guid = objects
        .get(target)?
        .guid
        .ok_or_else(|| refuse("execution.unit.guid_absent"))?
        .raw();
    SourceUnitQueries { objects, units }.threat_lookup(unit, guid)
}

/// Unit.cpp:6638-6664 differs from the direct map lookup above.
fn unit_target(
    objects: &SourceObjects,
    units: &mut SourceUnits,
    unit: ObjectHandle,
) -> Result<Option<ObjectHandle>> {
    if !objects.get(unit)?.in_world {
        if let Some(target) = units.get(objects, unit)?.target {
            if objects.get(unit)?.kind == SourceObjectKind::Pet
                && objects
                    .get(target)?
                    .guid
                    .ok_or_else(|| refuse("execution.unit.guid_absent"))?
                    .high()
                    == 0
            {
                return Err(refuse(
                    "execution.select_hostile_target.f5_15_loading_pet_target_owner",
                ));
            }
        }
        return Ok(None);
    }
    map_target(objects, units, unit)
}

fn order_attackers(
    objects: &SourceObjects,
    units: &mut SourceUnits,
    unit: ObjectHandle,
) -> Result<()> {
    let attackers = &mut units.get_mut(objects, unit)?.attackers;
    if attackers.len() < 2 {
        return Ok(());
    }
    // Unit.h:1075 uses std::set<Unit*>. F3's bound Object address is also Unit's
    // address through single inheritance, Unit.h:1072 and Object.h:852.
    for attacker in attackers.iter() {
        require(
            objects.get(*attacker)?.source_allocation.is_some(),
            "execution.select_hostile_target.f1_attacker_pointer_order_owner",
        )?;
    }
    // All other consumers only insert, remove or test membership. No allocation.
    attackers.sort_unstable_by_key(|attacker| {
        objects
            .get(*attacker)
            .expect("validated attacker generation")
            .source_allocation
            .expect("validated attacker address")
    });
    Ok(())
}
