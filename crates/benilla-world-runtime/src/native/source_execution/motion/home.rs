// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; HomeMovementGenerator.cpp:26-110.
use super::super::*;
use std::cell::Cell;

pub struct SourceHomeGenerator {
    arrived: Cell<bool>,
    was_active: Cell<bool>,
    run: bool,
}
impl SourceHomeGenerator {
    pub fn new(run: bool) -> Self {
        Self {
            arrived: Cell::new(false),
            was_active: Cell::new(false),
            run,
        }
    }
    fn target(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if context.units.get(context.objects, master.unit)?.state & MOTION_NOT_MOVE != 0 {
            return Ok(());
        }
        let mut target = match master.reset_position() {
            Some(position) => position,
            None => owner.combat_start_position(master.unit)?,
        };
        // Position::IsEmpty ignores orientation, Object.h:296.
        if target[..3] == [0.; 3] {
            target = context
                .objects
                .get(master.unit)?
                .respawn_pose
                .ok_or_else(|| refuse("execution.motion.respawn_position_absent"))?
                .0
                .map(f32::from_bits);
        }
        // DIST_CALC_NONE returns squared distance, Object.cpp:1283-1306.
        if owner.home_distance(master.unit, target)? > 150. * 150.
            && (!context.objects.get(master.unit)?.in_world || !owner.dungeon(master.unit)?)
        {
            self.arrived.set(true);
            self.finalize(master, context, owner)?;
            owner.motion_call(master, MotionCall::RespawnDelayFive, context)?;
            owner.motion_call(master, MotionCall::ForcedDespawnOne, context)?;
            return Ok(());
        }
        // Home.cpp:64; PathFinder.cpp:47-53 already queries F6 before calculate.
        Err(MotionCall::HomePathfinder {
            target,
            run: self.run,
        }
        .missing_owner())
    }
}
impl SourceMovementGenerator for SourceHomeGenerator {
    fn kind(&self) -> u32 {
        12
    }
    fn initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.was_active
            .set(context.objects.get(master.unit)?.active);
        if !self.was_active.get() {
            owner.motion_call(master, MotionCall::SetActive(true), context)?;
        }
        master.set_evade_home(true, context, owner)?;
        self.target(master, context, owner)
    }
    fn reset(
        &self,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        Ok(())
    }
    fn interrupt(
        &self,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        Ok(())
    }
    fn update(
        &self,
        master: &mut SourceMotionMaster,
        _: u32,
        _: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        self.arrived.set(owner.spline_finalized(master.unit)?);
        Ok(!self.arrived.get())
    }
    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        master.set_evade_home(false, context, owner)?;
        if !self.arrived.get() {
            return Ok(());
        }
        if owner.restore_faction_at_home(master.unit)? {
            owner.motion_call(master, MotionCall::ClearTemporaryFaction, context)?;
        }
        owner.motion_call(master, MotionCall::LoadAddon, context)?;
        owner.motion_call(master, MotionCall::TriggerHome, context)?;
        if let Some(summoner) = owner.temporary_summoner(master.unit)? {
            // Missing/despawned summoners are the source's ordinary null lookup.
            if let Ok(object) = context.objects.get(summoner) {
                if matches!(
                    object.kind,
                    SourceObjectKind::Creature | SourceObjectKind::Pet
                ) && context.units.get(context.objects, summoner)?.has_ai
                {
                    owner.motion_call(master, MotionCall::SummonedHome(summoner), context)?;
                }
            }
        }
        if !self.was_active.get() && context.objects.get(master.unit)?.in_world {
            owner.motion_call(master, MotionCall::SetActive(false), context)?;
        }
        owner.motion_call(master, MotionCall::ClearCombatStart, context)
    }
}
