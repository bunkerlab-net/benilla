// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureAIRegistry.cpp:37-43. Selected executable bodies share the live Unit owner.
use super::*;
mod guard;
mod null;
mod possessed;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
enum BuiltinBody {
    Null,
    Guard,
    Possessed(SourceUnitAi),
}
#[derive(Clone, Debug)]
pub struct SourceBuiltinAi {
    selected: SelectedCreatureAi,
    body: BuiltinBody,
}
/// Unbound calls refuse at the operation, never at selection or a fabricated result.
pub trait BuiltinAiExecutionOwner: UnitAiExecutionOwner {
    fn select_hostile_target(
        &mut self,
        ai: &mut SourceCreatureAi,
        objects: &SourceObjects,
        units: &mut SourceUnits,
    ) -> Result<bool> {
        if !objects.get(ai.base.unit)?.alive || !units.get(objects, ai.base.unit)?.has_ai {
            return Ok(false);
        }
        Err(refuse(
            "execution.select_hostile_target.threat_context_owner",
        ))
    }
    fn update_melee(
        &mut self,
        _unit: ObjectHandle,
        _objects: &SourceObjects,
        _units: &mut SourceUnits,
    ) -> Result<()> {
        Err(refuse(
            "execution.builtin_ai.f5_04_melee_update_binding_owner",
        ))
    }
    fn guard_query(
        &mut self,
        _unit: ObjectHandle,
        _target: ObjectHandle,
        query: GuardQuery,
    ) -> Result<bool> {
        Err(refuse(match query {
            GuardQuery::CanAssist
            | GuardQuery::CanInitiateAttack
            | GuardQuery::AttackOnSight
            | GuardQuery::GuardOrCivilian => {
                "execution.builtin_ai.f5_04_guard_predicate_binding_owner"
            }
            _ => "execution.builtin_ai.f6_guard_query_owner",
        }))
    }
    fn helper_attacker(&mut self, _unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.builtin_ai.f5_06_helper_attacker_owner"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardQuery {
    CanAssist,
    CanInitiateAttack,
    AttackOnSight,
    Accessible,
    WithinDistance(u32),
    LineOfSight,
    GuardOrCivilian,
    AttackDistance,
}
impl SourceBuiltinAi {
    pub fn construct(
        selected: SelectedCreatureAi,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &SourceCreatureAis,
    ) -> Result<Self> {
        selected.validate(objects)?;
        let unit = selected.object();
        let body = match selected.executable().builtin_kind() {
            Some(BuiltinCreatureAi::Null) => {
                ais.get(objects, unit)?;
                null::construct(unit, objects, units)?;
                BuiltinBody::Null
            }
            Some(BuiltinCreatureAi::Guard) => {
                ais.get(objects, unit)?;
                BuiltinBody::Guard // GuardAI.cpp:32-34 has no derived constructor actions.
            }
            Some(BuiltinCreatureAi::Possessed) => {
                BuiltinBody::Possessed(SourceUnitAi::construct(objects, unit, 0)?)
            }
            _ => return Err(selected.constructor_refusal(objects)?),
        };
        units.bind_ai_selection(objects, &selected)?;
        units.get_mut(objects, unit)?.has_ai = true;
        Ok(Self { selected, body })
    }
    pub fn selected(&self) -> &SelectedCreatureAi {
        &self.selected
    }
    pub fn validate(&self, objects: &SourceObjects, ais: &SourceCreatureAis) -> Result<()> {
        // Owners may disappear after construction. Do not revalidate their old selection handles.
        objects.get(self.selected.object())?;
        if !matches!(self.body, BuiltinBody::Possessed(_)) {
            ais.get(objects, self.selected.object())?;
        }
        Ok(())
    }
    pub fn inventory(
        &self,
        objects: &SourceObjects,
        ais: &SourceCreatureAis,
    ) -> Result<serde_json::Value> {
        self.validate(objects, ais)?;
        let base = match &self.body {
            BuiltinBody::Possessed(base) => {
                serde_json::to_value(base).map_err(|_| refuse("execution.builtin_ai.inventory"))?
            }
            _ => serde_json::to_value(ais.get(objects, self.selected.object())?)
                .map_err(|_| refuse("execution.builtin_ai.inventory"))?,
        };
        Ok(
            serde_json::json!({"version":1,"selected":self.selected,"base":base,"restore":"execution.builtin_ai.f7_restore_owner"}),
        )
    }
    pub fn update(
        &mut self,
        diff: u32,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl BuiltinAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        match &mut self.body {
            BuiltinBody::Null => Ok(()), // NullCreatureAI.h:37.
            BuiltinBody::Possessed(base) => possessed::update(base, objects, units, owner),
            BuiltinBody::Guard => guard::update(
                ais.get_mut(objects, self.selected.object())?,
                diff,
                objects,
                units,
                owner,
            ),
        }
    }
    pub fn move_in_line_of_sight(
        &mut self,
        who: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        config: &EventAiLoaderConfig,
        owner: &mut impl BuiltinAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        match &mut self.body {
            BuiltinBody::Null => Ok(()), // NullCreatureAI.h:30.
            BuiltinBody::Guard => guard::move_in_line_of_sight(
                ais.get_mut(objects, self.selected.object())?,
                who,
                objects,
                units,
                owner,
            ),
            BuiltinBody::Possessed(base) => {
                base.move_in_line_of_sight(who, objects, units, config, owner)
            }
        }
    }
    pub fn enter_combat(
        &mut self,
        enemy: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        match self.body {
            // PossessedAI.cpp:8-11's static empty spell list makes UnitAI.cpp:106 false.
            BuiltinBody::Possessed(_) => Ok(()),
            _ => ais
                .get_mut(objects, self.selected.object())?
                .enter_combat(enemy, objects, units, owner),
        }
    }
    pub fn attack_start(
        &mut self,
        target: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        match &mut self.body {
            BuiltinBody::Null => Ok(()), // NullCreatureAI.h:31.
            BuiltinBody::Guard => ais
                .get_mut(objects, self.selected.object())?
                .attack_start(target, objects, units, owner),
            BuiltinBody::Possessed(base) => base.attack_start(target, objects, units, owner),
        }
    }
    pub fn attacked_by(
        &mut self,
        attacker: Option<ObjectHandle>,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        match &mut self.body {
            BuiltinBody::Null => Ok(()), // NullCreatureAI.h:32.
            BuiltinBody::Guard => ais
                .get_mut(objects, self.selected.object())?
                .attacked_by(attacker, objects, units, owner),
            BuiltinBody::Possessed(base) => base.attacked_by(attacker, objects, units, owner),
        }
    }
    pub fn enter_evade(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        match &mut self.body {
            BuiltinBody::Null => Ok(()), // NullCreatureAI.h:33.
            BuiltinBody::Guard => ais
                .get_mut(objects, self.selected.object())?
                .enter_evade(objects, units, owner),
            BuiltinBody::Possessed(base) => base.enter_evade(objects, units, owner),
        }
    }
    pub fn just_died(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        // Neither Null nor Guard overrides UnitAI::JustDied, UnitAI.cpp:132-135.
        let base = match &mut self.body {
            BuiltinBody::Possessed(base) => base,
            _ => &mut ais.get_mut(objects, self.selected.object())?.base,
        };
        base.just_died(objects, units, owner)
    }
    pub fn just_respawned(
        &mut self,
        objects: &SourceObjects,
        ais: &mut SourceCreatureAis,
    ) -> Result<()> {
        self.validate(objects, ais)?;
        if !matches!(self.body, BuiltinBody::Possessed(_)) {
            // CreatureAI.cpp:124-128 versus the empty UnitAI.h:260 callback.
            let ai = ais.get_mut(objects, self.selected.object())?;
            ai.reset_timers(true)?;
            ai.reset();
        }
        Ok(())
    }
    pub fn on_unsummon(&self, objects: &SourceObjects, ais: &SourceCreatureAis) -> Result<()> {
        // All three inherit the actual empty UnitAI.h:425 callback.
        self.validate(objects, ais)
    }
    pub fn on_heartbeat(&self, objects: &SourceObjects, ais: &SourceCreatureAis) -> Result<()> {
        // All three inherit the actual empty UnitAI.h:415 callback.
        self.validate(objects, ais)
    }
    pub fn is_visible(&self, objects: &SourceObjects, ais: &SourceCreatureAis) -> Result<bool> {
        self.validate(objects, ais)?;
        if matches!(self.body, BuiltinBody::Null) {
            return Ok(false);
        }
        Err(refuse("execution.builtin_ai.f6_visibility_owner"))
    }
}
