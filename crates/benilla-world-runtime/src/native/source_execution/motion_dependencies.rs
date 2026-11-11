// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct SourceMotions {
    masters: BTreeMap<ObjectHandle, SourceMotionMaster>,
}
impl SourceMotions {
    pub fn construct(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        default: MotionDefault,
    ) -> Result<()> {
        require(
            matches!(
                objects.get(unit)?.kind,
                SourceObjectKind::Creature | SourceObjectKind::Pet | SourceObjectKind::Player
            ),
            "execution.motion.not_unit",
        )?;
        require(
            !self.masters.contains_key(&unit),
            "execution.motion.duplicate_generation",
        )?;
        self.masters
            .insert(unit, SourceMotionMaster::new(unit, default));
        Ok(())
    }
    pub fn get_mut(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
    ) -> Result<&mut SourceMotionMaster> {
        objects.get(unit)?;
        self.masters
            .get_mut(&unit)
            .ok_or_else(|| refuse("execution.motion.generation_missing"))
    }
    pub fn formation(
        &mut self,
        unit: ObjectHandle,
        command: FormationMotion,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.get_mut(context.objects, unit)?
            .formation(command, context, owner)
    }
}
impl CreatureConstruction {
    /// Creature.cpp:880, MotionMaster.cpp:57-99, before AI selection.
    /// The factory supplies the effective InitEntry movement type and path.
    pub fn initialize_motion(
        &self,
        motions: &mut SourceMotions,
        default: MotionDefault,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        motions.construct(context.objects, self.object, default)?;
        motions
            .get_mut(context.objects, self.object)?
            .initialize(context, owner)
    }
}
/// Compose this with SourceAiDependencies to retain concrete threat ownership.
pub struct SourceMotionDependencies<'a, O> {
    pub motions: &'a mut SourceMotions,
    pub owner: &'a mut O,
}
impl<O: UnitAiExecutionOwner + MotionExecutionOwner> UnitAiExecutionOwner
    for SourceMotionDependencies<'_, O>
{
    fn call(
        &mut self,
        unit: ObjectHandle,
        call: UnitAiCall,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        match call {
            UnitAiCall::Home => self.motions.get_mut(objects, unit)?.move_home(
                true,
                &mut MotionContext { objects, units },
                self.owner,
            ),
            UnitAiCall::Immobilized(apply) => self.motions.get_mut(objects, unit)?.immobilize(
                apply,
                &mut MotionContext { objects, units },
                self.owner,
            ),
            _ => self.owner.call(unit, call, units, objects),
        }
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
        who: ObjectHandle,
        query: UnitAiSightQuery,
    ) -> Result<bool> {
        self.owner.sight_query(unit, who, query)
    }
    fn add_initial_threat(
        &mut self,
        unit: ObjectHandle,
        target: ObjectHandle,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.owner.add_initial_threat(unit, target, units, objects)
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
impl<O: CreatureRandom> CreatureRandom for SourceMotionDependencies<'_, O> {
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
