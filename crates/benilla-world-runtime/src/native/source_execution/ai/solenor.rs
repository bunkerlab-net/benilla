// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

pub trait SolenorResetOwner: NearTeleportOwner {
    fn solenor_respawn_clock(
        &mut self,
        _unit: ObjectHandle,
        _random: &dyn CreatureRandom,
    ) -> Result<ClockSample> {
        Err(refuse("execution.creature.Creature_801.time_owner"))
    }

    fn current_motion_type(&self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse(
            "execution.ai.silithus_88.GetCurrentMovementGeneratorType_owner",
        ))
    }

    fn reinitialize_motion(
        &mut self,
        _unit: ObjectHandle,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        Err(refuse(
            "execution.ai.silithus_91.MotionMaster_Initialize_owner",
        ))
    }
}
impl SolenorResetOwner for () {}

#[derive(Clone, Debug)]
pub struct SolenorAi {
    pub(super) binding: Arc<ScriptDevAiBinding>,
    pub evade_check_cooldown_ms: u32,
    pub transform_timer_ms: Option<u32>,
    pub transform_emote_timer_ms: Option<u32>,
    pub transform: bool,
    pub hunter: Option<SourceGuid>,
    pub dreadful_fright_timer_ms: Option<u32>,
    pub creeping_doom_timer_ms: Option<u32>,
    pub cast_soul_flame_timer_ms: Option<u32>,
    pub despawn_timer_ms: u32,
}
impl SolenorAi {
    pub fn transform_timer(&self) -> Result<u32> {
        self.transform_timer_ms.ok_or_else(|| {
            refuse("execution.ai.silithus_219.transform_timer_uninitialized_undefined")
        })
    }

    pub fn transform_emote_timer(&self) -> Result<u32> {
        self.transform_emote_timer_ms.ok_or_else(|| {
            refuse("execution.ai.silithus_208.transform_emote_timer_uninitialized_undefined")
        })
    }

    pub fn cast_soul_flame_timer(&self) -> Result<u32> {
        self.cast_soul_flame_timer_ms.ok_or_else(|| {
            refuse("execution.ai.silithus_240.cast_soul_flame_timer_uninitialized_undefined")
        })
    }

    pub fn dreadful_fright_timer(&self) -> Result<u32> {
        self.dreadful_fright_timer_ms.ok_or_else(|| {
            refuse("execution.ai.silithus_269.dreadful_fright_timer_uninitialized_undefined")
        })
    }

    pub fn creeping_doom_timer(&self) -> Result<u32> {
        self.creeping_doom_timer_ms.ok_or_else(|| {
            refuse("execution.ai.silithus_261.creeping_doom_timer_uninitialized_undefined")
        })
    }

    // silithus.cpp:63-68, sc_creature.cpp:22-25, ObjectGuid.h:82.
    pub(super) fn construct(
        binding: &Arc<ScriptDevAiBinding>,
        unit: ObjectHandle,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        random: &mut dyn CreatureRandom,
        owner: &mut dyn SolenorResetOwner,
    ) -> Result<Self> {
        let mut ai = Self {
            binding: Arc::clone(binding),
            evade_check_cooldown_ms: SCRIPTED_EVADE_CHECK_COOLDOWN_MS,
            transform_timer_ms: None,
            transform_emote_timer_ms: None,
            transform: false,
            hunter: None,
            dreadful_fright_timer_ms: None,
            creeping_doom_timer_ms: None,
            cast_soul_flame_timer_ms: None,
            despawn_timer_ms: 0,
        };
        ai.reset(unit, objects, units, random, owner)?;
        Ok(ai)
    }

    pub(super) fn reset(
        &mut self,
        unit: ObjectHandle,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        random: &mut dyn CreatureRandom,
        owner: &mut dyn SolenorResetOwner,
    ) -> Result<()> {
        match units.get(objects, unit)?.entry {
            14536 => {
                const DELAY: u32 = 35 * 60;
                units
                    .get_mut(objects, unit)?
                    .set_creature_respawn_delay(DELAY, false);
                let deadline = source_creature_respawn_deadline(DELAY, || {
                    owner.solenor_respawn_clock(unit, random)
                })?;
                units.get_mut(objects, unit)?.respawn_time = deadline;
                source_near_teleport(
                    unit,
                    SourcePose([-7724.21f32, 1676.43, 7.0571, 4.80044].map(f32::to_bits)),
                    objects,
                    units,
                    owner,
                )?;
                if owner.current_motion_type(unit)? != 2 {
                    units
                        .get_mut(objects, unit)?
                        .loaded_creature
                        .default_movement = 2;
                    owner.reinitialize_motion(unit, &mut MotionContext { objects, units })?;
                }
                let live = units.get_mut(objects, unit)?;
                objects.set_update_value(
                    unit,
                    UNIT_NPC_FLAGS,
                    &mut live.loaded_creature.npc_flags,
                    1,
                )?;
                self.transform_timer_ms = Some(10_000);
                self.transform_emote_timer_ms = Some(5_000);
                self.transform = false;
                self.despawn_timer_ms = 0;
                self.cast_soul_flame_timer_ms = Some(0);
            }
            14530 => {
                if self.despawn_timer_ms == 0 {
                    self.despawn_timer_ms = 20 * 60 * 1000;
                    self.cast_soul_flame_timer_ms = Some(150);
                    return Err(refuse("execution.ai.silithus_107.DoCastSpellIfCan_owner"));
                }
                self.hunter = None;
                self.dreadful_fright_timer_ms = Some(random.creature_urand(10_000, 15_000)?);
                self.creeping_doom_timer_ms = Some(random.creature_urand(3_000, 6_000)?);
            }
            _ => (),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::source_execution::creature_tests::{chicken_binding, Random};

    #[test]
    fn solenor_reset_respects_alternate_and_missing_entries() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        units
            .register(
                &objects,
                unit,
                CreatureConstruction::construct_loaded_unit(),
            )
            .unwrap();
        let mut random = Random::new();
        let mut binding = chicken_binding();
        binding.name = "npc_solenor".into();
        let mut ai = SolenorAi::construct(
            &Arc::new(binding),
            unit,
            &mut objects,
            &mut units,
            &mut random,
            &mut (),
        )
        .unwrap();
        assert_eq!((ai.transform, ai.despawn_timer_ms), (false, 0));
        let timers = [
            (
                SolenorAi::transform_timer as fn(&SolenorAi) -> Result<u32>,
                "execution.ai.silithus_219.transform_timer_uninitialized_undefined",
            ),
            (
                SolenorAi::transform_emote_timer,
                "execution.ai.silithus_208.transform_emote_timer_uninitialized_undefined",
            ),
            (
                SolenorAi::cast_soul_flame_timer,
                "execution.ai.silithus_240.cast_soul_flame_timer_uninitialized_undefined",
            ),
            (
                SolenorAi::dreadful_fright_timer,
                "execution.ai.silithus_269.dreadful_fright_timer_uninitialized_undefined",
            ),
            (
                SolenorAi::creeping_doom_timer,
                "execution.ai.silithus_261.creeping_doom_timer_uninitialized_undefined",
            ),
        ];
        for (read, id) in timers {
            assert_eq!(read(&ai).unwrap_err().id, id);
        }
        ai.transform_timer_ms = Some(10_000);
        ai.transform_emote_timer_ms = Some(5000);
        ai.cast_soul_flame_timer_ms = Some(0);
        ai.dreadful_fright_timer_ms = Some(10_111);
        ai.creeping_doom_timer_ms = Some(3_111);
        for ((read, _), expected) in timers.into_iter().zip([10_000, 5_000, 0, 10_111, 3_111]) {
            assert_eq!(read(&ai).unwrap(), expected);
        }
        units.get_mut(&objects, unit).unwrap().entry = 14530;
        assert_eq!(
            ai.reset(unit, &mut objects, &mut units, &mut random, &mut ())
                .unwrap_err()
                .id,
            "execution.ai.silithus_107.DoCastSpellIfCan_owner"
        );
        assert_eq!(
            (ai.despawn_timer_ms, ai.cast_soul_flame_timer().unwrap()),
            (1_200_000, 150)
        );
        assert_eq!(random.cursor.raw, 0);
        ai.reset(unit, &mut objects, &mut units, &mut random, &mut ())
            .unwrap();
        assert!((10_000..=15_000).contains(&ai.dreadful_fright_timer().unwrap()));
        assert!((3_000..=6_000).contains(&ai.creeping_doom_timer().unwrap()));
        assert_eq!(random.cursor.raw, 2);
    }
}
