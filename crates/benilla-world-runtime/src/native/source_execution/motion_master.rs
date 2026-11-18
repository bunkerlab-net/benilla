// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; MotionMaster.cpp:57-277,295-309,599-620.
use super::*;
use std::rc::Rc;

pub const MOTION_CANNOT_MOVE: u32 = 0x2000_001e;
pub const MOTION_NOT_MOVE: u32 = MOTION_CANNOT_MOVE | 0x800;
const MOTION_MOVING: u32 = 0x0055_0000;
const MOTION_LOST_CONTROL: u32 = 0x0020_2040;

/// Actual live operands after Creature::InitEntry, not raw SQL defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionDefault {
    pub movement_type: u32,
    pub default_path: u32,
}
impl MotionDefault {
    /// Creature.cpp:453 converts the loaded uint8 to the non-fixed enum.
    pub fn from_spawn(movement_type: u8, default_path: u32) -> Result<Self> {
        require(
            movement_type <= 31,
            "execution.motion.default_enum_conversion_undefined",
        )?;
        Ok(Self {
            movement_type: u32::from(movement_type),
            default_path,
        })
    }
}
/// CreatureAISelector.cpp:109-135; CreatureAIRegistry.cpp:45-48.
/// FOLLOW has no registered factory and therefore falls back to static Idle.
pub fn source_default_motion(movement_type: u32, player_owner: bool) -> Result<u32> {
    match if player_owner { 7 } else { movement_type } {
        1..=4 => Ok(movement_type),
        _ => Ok(0),
    }
}

/// Foreign operations execute synchronously, including mutations from callbacks.
pub trait MotionExecutionOwner {
    fn spline_thread(&mut self) -> Result<&mut SourceSplineThread> {
        Err(refuse("execution.spline.thread_local_counter_owner_102"))
    }
    fn send_spline(&mut self, packet: &SplineOutbound, context: &MotionContext<'_>) -> Result<()> {
        context
            .objects
            .send_message_to_all_who_see_me_move(packet, |player, packet| {
                self.spline_send_session(player, packet)
            })
    }
    fn spline_send_session(
        &mut self,
        _player: ObjectHandle,
        _packet: &SplineOutbound,
    ) -> Result<()> {
        Err(refuse(
            "execution.spline.player_GetSession_SendPacket_owner_11222_11226",
        ))
    }
    fn spline_debug_spawn(
        &mut self,
        _unit: ObjectHandle,
        _point: [u32; 3],
        _waypoint_id: u32,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        Err(refuse("execution.spline.debug_waypoint_summon_owner_49"))
    }
    fn spline_passengers_mut(&mut self) -> Result<&mut SourcePassengers> {
        Err(refuse(
            "execution.spline.transport_movement_info_owner_11038",
        ))
    }
    /// Object.h:794 updates only t_pos in the existing passenger movement owner.
    fn spline_transport_offset(&mut self, unit: ObjectHandle, pose: SourcePose) -> Result<()> {
        self.spline_passengers_mut()?.movement_mut(unit)?.offset = pose;
        Ok(())
    }
    fn spline_get_angle(
        &mut self,
        _unit: ObjectHandle,
        _point: [f32; 2],
        _context: &MotionContext<'_>,
    ) -> Result<f32> {
        Err(refuse("execution.spline.world_object_angle_owner_11049"))
    }
    /// Map.cpp:83-85 reads the existing World TimePoint, not a fresh clock sample.
    fn spline_current_clock_ms(
        &mut self,
        _unit: ObjectHandle,
        _context: &MotionContext<'_>,
    ) -> Result<i64> {
        Err(refuse("execution.spline.map_current_clock_owner_11077"))
    }
    fn spline_relocate(
        &mut self,
        _relocation: SplineRelocation,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        Err(refuse(if _relocation.relocate_only {
            "execution.spline.plain_relocate_owner_11081"
        } else {
            "execution.spline.f3_creature_relocation_context_owner_11088"
        }))
    }
    fn motion_call(
        &mut self,
        _master: &mut SourceMotionMaster,
        call: MotionCall,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        match call {
            MotionCall::StopSpline => SourceMoveSplineInit::construct(_master.unit, _context)?
                .stop(_master.unit, false, _context, self),
            MotionCall::StopMovingForced => source_stop_moving(_master.unit, true, _context, self),
            MotionCall::InterruptMoving => {
                source_interrupt_moving(_master.unit, false, _context, self)
            }
            _ => Err(call.missing_owner()),
        }
    }
    fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
        Err(refuse("execution.pathfinder.live_geometry_owner"))
    }
    /// The same Unit movement state used by transport boarding and disembarking.
    fn pathfinder_passengers(&self) -> Result<&SourcePassengers> {
        Err(refuse("execution.pathfinder.transport_owner"))
    }
    /// HomeMovementGenerator.cpp:66 uses local movement offsets only when attached.
    fn home_pathfinder_position(
        &self,
        context: &MotionContext<'_>,
        unit: ObjectHandle,
    ) -> Result<[f32; 3]> {
        let passengers = self.pathfinder_passengers()?;
        let pose = if passengers.transport(unit)?.is_some() {
            passengers.movement(unit)?.offset
        } else {
            context.objects.pose(unit)?
        };
        let p = pose.0.map(f32::from_bits);
        Ok([p[0], p[1], p[2]])
    }
    fn calculate_pathfinder(
        &mut self,
        path: &mut SourcePathFinder,
        context: &MotionContext<'_>,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<bool> {
        let attachment = self
            .pathfinder_passengers()
            .and_then(|passengers| passengers.transport(path.unit));
        path.calculate(
            self.pathfinder_geometry()?,
            context,
            attachment,
            start,
            end,
            true,
            false,
        )
    }
    /// WaypointMovementGenerator.cpp:261-262 reads movement flags before speed.
    fn waypoint_speed(&mut self, unit: ObjectHandle, context: &MotionContext<'_>) -> Result<f32> {
        context
            .units
            .get(context.objects, unit)?
            .movement
            .as_ref()
            .ok_or_else(|| refuse("execution.waypoint.movement_speed_owner_261"))?
            .speed()
    }
    fn wander_melee_attack_stop(
        &mut self,
        _unit: ObjectHandle,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        Err(refuse("execution.motion.wander_melee_attack_stop_owner"))
    }
    fn relinquish_follow(&mut self, _unit: ObjectHandle, _follower: SourceGuid) -> Result<()> {
        Err(refuse(
            "execution.movement.TargetedMovementGenerator_829.RelinquishFollow_owner",
        ))
    }
    fn combat_start_position(&mut self, _unit: ObjectHandle) -> Result<[f32; 4]> {
        Err(refuse("execution.motion.combat_start_position_owner"))
    }
    fn home_distance(&mut self, _unit: ObjectHandle, _target: [f32; 4]) -> Result<f32> {
        Err(refuse("execution.motion.f6_transport_distance_owner"))
    }
    fn dungeon(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.motion.map_dungeon_binding"))
    }
    fn spline_finalized(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<bool> {
        context.spline_finalized(unit)
    }
    fn spline_compute_position(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<[f32; 4]> {
        context.spline_position(unit)
    }
    fn spline_current_index(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<i32> {
        context.spline_current_index(unit)
    }
    fn restore_faction_at_home(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.motion.temporary_faction_owner"))
    }
    fn temporary_summoner(&mut self, _unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.motion.temporary_summoner_owner"))
    }
    fn ai_order(&mut self, _unit: ObjectHandle, _order: u32) -> Result<()> {
        Err(refuse("execution.motion.base_ai_order_owner"))
    }
    fn controlled_evade(&mut self, _unit: ObjectHandle, _home: bool) -> Result<()> {
        Err(refuse("execution.motion.controlled_evade_owner"))
    }
    fn client_controlled(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.motion.client_controlled_owner"))
    }
    fn client_controlling(
        &mut self,
        _unit: ObjectHandle,
        _context: &MotionContext<'_>,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.motion.client_controlling_owner"))
    }
    /// MotionMaster.cpp:85-94 reads the actual group's leader slot and path.
    fn waypoint_formation_path(&mut self, _unit: ObjectHandle) -> Result<Option<u32>> {
        Err(refuse("execution.waypoint.formation_path_binding_owner"))
    }
    fn waypoint_paths(&mut self) -> Result<Rc<SourceWaypointPaths>> {
        Err(refuse("execution.waypoint.loaded_waypoint_manager_owner"))
    }
    /// WaypointMovementGenerator.cpp:132; Object.h:877,1129.
    fn waypoint_position(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<[f32; 4]> {
        let passengers = self.pathfinder_passengers()?;
        let pose = if passengers.transport(unit)?.is_some() {
            passengers.movement(unit)?.offset
        } else {
            context.objects.pose(unit)?
        };
        Ok(pose.0.map(f32::from_bits))
    }
    /// Unit.cpp:4343-4381, IsNonMeleeSpellCasted(false,false,true,true).
    fn waypoint_casting_for_movement(
        &mut self,
        _unit: ObjectHandle,
        _context: &MotionContext<'_>,
    ) -> Result<bool> {
        Err(refuse(
            "execution.waypoint.f5_spell_movement_cast_query_owner",
        ))
    }
    fn waypoint_ai_path(&mut self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.waypoint.ai_current_path_owner"))
    }
    /// Includes the temporary summoner notification, cpp:417-426.
    fn waypoint_inform(
        &mut self,
        _generator: &SourceWaypointGenerator,
        _master: &mut SourceMotionMaster,
        _context: &mut MotionContext<'_>,
        _kind: u32,
        _point: u32,
    ) -> Result<()> {
        Err(refuse("execution.waypoint.f5_ai_movement_inform_owner"))
    }
    fn waypoint_script(
        &mut self,
        _generator: &SourceWaypointGenerator,
        _master: &mut SourceMotionMaster,
        _context: &mut MotionContext<'_>,
        _script: u32,
        _target: Option<SourceGuid>,
    ) -> Result<()> {
        Err(refuse(
            "execution.waypoint.f5_dbscript_movement_execution_owner",
        ))
    }
    /// Checks the actual formation slot, then calls OnWPStartNode/OnWPEndNode.
    /// An explicitly bound absent slot skips the hook, cpp:628-642.
    fn linear_waypoint_endpoint(&mut self, _unit: ObjectHandle, _returning: bool) -> Result<()> {
        Err(refuse("execution.waypoint.formation_linear_endpoint_owner"))
    }
    fn fixed_path_spline(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<FixedPathSplineState> {
        context.fixed_path_spline(unit)
    }
    /// PathMovementGenerator.cpp:155,168 re-evaluates the index on every loop test.
    fn fixed_path_current_index(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<i32> {
        context.fixed_path_current_index(unit)
    }
    /// Includes optional unit AI and temporary-summoner AI notifications.
    fn fixed_path_inform(
        &mut self,
        _master: &mut SourceMotionMaster,
        _context: &mut MotionContext<'_>,
        _kind: u32,
        _point: u32,
    ) -> Result<()> {
        Err(refuse("execution.fixed_path.f5_ai_movement_inform_owner"))
    }
    fn fixed_path_script(
        &mut self,
        _generator: &SourceFixedPathGenerator,
        _master: &mut SourceMotionMaster,
        _context: &mut MotionContext<'_>,
        _script: u32,
        _target: Option<SourceGuid>,
    ) -> Result<()> {
        Err(refuse(
            "execution.waypoint.f5_dbscript_movement_execution_owner",
        ))
    }
}
pub struct MotionContext<'a> {
    pub objects: &'a SourceObjects,
    pub units: &'a mut SourceUnits,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionCall {
    StopSpline,
    SetActive(bool),
    ClearTemporaryFaction,
    LoadAddon,
    TriggerHome,
    SummonedHome(ObjectHandle),
    ClearCombatStart,
    RespawnDelayFive,
    ForcedDespawnOne,
    SendRoot(bool),
    InterruptMoving,
    StopMovingForced,
    RestoreClientControl(ObjectHandle),
    RemoveClientControl(ObjectHandle),
    RestoreWalk(bool),
}
impl MotionCall {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::StopSpline => "execution.motion.f6_spline_stop_owner",
            Self::SetActive(_) => "execution.motion.active_residency_owner",
            Self::ClearTemporaryFaction => "execution.motion.temporary_faction_owner",
            Self::LoadAddon => "execution.creature.addon_owner",
            Self::TriggerHome | Self::SummonedHome(_) => "execution.motion.home_callback_owner",
            Self::ClearCombatStart => "execution.motion.combat_start_position_owner",
            Self::RespawnDelayFive | Self::ForcedDespawnOne => {
                "execution.motion.f5_06_despawn_owner"
            }
            Self::SendRoot(_) => "execution.motion.root_movement_info_output_owner",
            Self::InterruptMoving => "execution.motion.f6_interrupt_moving_owner",
            Self::StopMovingForced => "execution.motion.f6_stop_moving_forced_owner",
            Self::RestoreClientControl(_) | Self::RemoveClientControl(_) => {
                "execution.motion.client_control_output_owner"
            }
            Self::RestoreWalk(_) => "execution.motion.walk_movement_info_output_owner",
        })
    }
}

/// Shared references permit Clear/Expire to Finalize the updating generator.
/// Destruction is independently deferred, as in MotionMaster.cpp:179-201,242-271.
pub trait SourceMovementGenerator {
    fn kind(&self) -> u32;
    fn as_follow(&self) -> Option<&SourceFollowGenerator> {
        None
    }
    fn initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()>;
    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()>;
    fn interrupt(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()>;
    fn reset(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()>;
    fn update(
        &self,
        master: &mut SourceMotionMaster,
        diff: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool>;
    fn removed_on_expire(&self) -> bool {
        false
    }
    fn reset_position(&self) -> Result<Option<[f32; 4]>> {
        Ok(None)
    }
    fn pause(&self, _delta: i32, _force: bool) -> Result<()> {
        Err(refuse("execution.motion.pause_generator_owner"))
    }
}
#[derive(Clone)]
enum Generator {
    Idle,
    Owned(Rc<dyn SourceMovementGenerator>),
}
impl Generator {
    fn kind(&self) -> u32 {
        match self {
            Self::Idle => 0,
            Self::Owned(g) => g.kind(),
        }
    }
    fn identity(&self) -> *const () {
        match self {
            Self::Idle => std::ptr::null(),
            Self::Owned(g) => Rc::as_ptr(g).cast(),
        }
    }
    fn removed(&self) -> bool {
        matches!(self, Self::Owned(g) if g.removed_on_expire())
    }
}
pub struct SourceMotionMaster {
    pub unit: ObjectHandle,
    pub default: MotionDefault,
    current_path: Option<u32>,
    respawn_radius: Option<f32>,
    stack: Vec<Generator>,
    expired: Option<Vec<Generator>>,
    updating: bool,
    needs_reset: bool,
}
impl SourceMotionMaster {
    pub fn new(unit: ObjectHandle, default: MotionDefault) -> Self {
        Self {
            unit,
            default,
            current_path: None,
            respawn_radius: None,
            stack: Vec::new(),
            expired: None,
            updating: false,
            needs_reset: false,
        }
    }
    /// Bind the live Creature::m_respawnradius, Creature.cpp:1688,2493-2506.
    pub fn bind_respawn_radius(&mut self, radius: f32) {
        self.respawn_radius = Some(radius);
    }
    pub fn len(&self) -> usize {
        self.stack.len()
    }
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
    pub fn current_type(&self) -> u32 {
        self.stack.last().map_or(0, Generator::kind)
    }
    pub fn follow(&self) -> Option<&SourceFollowGenerator> {
        match self.stack.last()? {
            Generator::Owned(generator) => generator.as_follow(),
            Generator::Idle => None,
        }
    }
    /// MotionMaster.h:104,173,195-196 leave this uninitialized until the creature branch.
    pub fn path_id(&self) -> Result<u32> {
        self.current_path
            .ok_or_else(|| refuse("execution.motion.path_id_uninitialized_undefined"))
    }
    pub fn reset_position(&self) -> Result<Option<[f32; 4]>> {
        match self.stack.last() {
            Some(Generator::Owned(g)) => g.reset_position(),
            _ => Ok(None),
        }
    }
    pub fn stop_moving(
        &mut self,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let fields = context.units.get_mut(context.objects, self.unit)?;
        // Unit.cpp:9908-9920. No spline operation on the stopped or out-of-world paths.
        if fields.state & MOTION_MOVING == 0 {
            return Ok(());
        }
        fields.state &= !MOTION_MOVING;
        if context.objects.get(self.unit)?.in_world {
            owner.motion_call(self, MotionCall::StopSpline, context)?;
        }
        Ok(())
    }
    pub fn initialize(
        &mut self,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.stop_moving(context, owner)?;
        self.clear(false, true, context, owner)?;
        let object = context.objects.get(self.unit)?;
        let fields = context.units.get(context.objects, self.unit)?;
        if matches!(
            object.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) && fields.state & 0x40 == 0
        {
            self.current_path = Some(self.default.default_path);
            let player_owner = fields
                .owner
                .map(|o| {
                    context.objects.get(o).map(|o| {
                        o.guid
                            .is_some_and(|g| g.raw() != 0 && g.high() == HighGuid::Player as u16)
                    })
                })
                .transpose()?
                .unwrap_or(false);
            let kind = source_default_motion(self.default.movement_type, player_owner)?;
            if kind == 1 {
                let radius = self
                    .respawn_radius
                    .ok_or_else(|| refuse("execution.motion.respawn_radius_owner"))?;
                let generator = Rc::new(SourceWanderGenerator::from_creature(
                    self.unit, context, radius,
                )?);
                // MotionMaster.cpp:80-81 pushes before calling Initialize.
                self.stack.push(Generator::Owned(generator.clone()));
                return generator.initialize(self, context, owner);
            }
            if kind == 3 {
                let generator = Rc::new(SourceFixedPathGenerator::load(
                    self,
                    context,
                    owner,
                    self.path_id()? as i32,
                    WaypointOrigin::None,
                    FixedPathOptions {
                        flying: true,
                        cyclic: true,
                        ..Default::default()
                    },
                )?);
                self.stack.push(Generator::Owned(generator.clone()));
                return generator.initialize(self, context, owner);
            }
            if kind == 2 || kind == 4 {
                let generator = Rc::new(if kind == 4 {
                    SourceWaypointGenerator::linear()
                } else {
                    SourceWaypointGenerator::default()
                });
                self.stack.push(Generator::Owned(generator.clone()));
                generator.initialize(self, context, owner)?;
                let formation_path = owner.waypoint_formation_path(self.unit)?;
                let origin = if let Some(path) = formation_path {
                    self.current_path = Some(path);
                    WaypointOrigin::WaypointPath
                } else {
                    WaypointOrigin::None
                };
                let request = WaypointPathRequest {
                    path_id: self.path_id()? as i32,
                    origin,
                    ..Default::default()
                };
                return generator.initialize_path(self, context, owner, request);
            }
        }
        self.stack.push(Generator::Idle);
        Ok(())
    }
    /// MoveIdle pushes the static singleton without Interrupt or Initialize.
    pub fn move_idle(&mut self) {
        if !matches!(self.stack.last(), Some(Generator::Idle)) {
            self.stack.push(Generator::Idle);
        }
    }
    pub fn mutate<G: SourceMovementGenerator + 'static>(
        &mut self,
        generator: G,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.mutate_owned(Rc::new(generator), context, owner)
    }
    fn mutate_owned(
        &mut self,
        generator: Rc<dyn SourceMovementGenerator>,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        // MotionMaster.cpp:599-615 expires Home/Distract, then interrupts the surviving top.
        if matches!(self.current_type(), 5 | 12) {
            self.expire(false, context, owner)?;
        }
        if let Some(Generator::Owned(g)) = self.stack.last().cloned() {
            g.interrupt(self, context, owner)?;
        }
        // MotionMaster.cpp:618-619 initializes before pushing.
        generator.initialize(self, context, owner)?;
        self.stack.push(Generator::Owned(generator));
        Ok(())
    }
    pub fn clear(
        &mut self,
        reset: bool,
        all: bool,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if self.updating {
            self.needs_reset = reset;
            if self.stack.is_empty() || (!all && self.stack.len() == 1) {
                return Ok(());
            }
            self.expired.get_or_insert_with(Vec::new);
        }
        while if all {
            !self.stack.is_empty()
        } else {
            self.stack.len() > 1
        } {
            let current = self.stack.pop().unwrap();
            self.finalize(&current, context, owner)?;
            // MotionMaster.cpp:168-169,198-199 delete now or retain until UpdateMotion.
            if self.updating && matches!(current, Generator::Owned(_)) {
                self.expired.as_mut().unwrap().push(current);
            }
        }
        if !self.updating && !all && reset {
            self.reset_top(context, owner)?;
        }
        Ok(())
    }
    fn finalize(
        &mut self,
        generator: &Generator,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if let Generator::Owned(g) = generator {
            g.finalize(self, context, owner)?;
        }
        Ok(())
    }
    fn reset_top(
        &mut self,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let top = self
            .stack
            .last()
            .cloned()
            .ok_or_else(|| refuse("execution.motion.empty_reset_assertion"))?;
        if let Generator::Owned(g) = top {
            g.reset(self, context, owner)?;
        }
        Ok(())
    }
    pub fn expire(
        &mut self,
        reset: bool,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if self.updating {
            self.needs_reset = reset;
        }
        if self.stack.len() <= 1 {
            return Ok(());
        }
        let current = self.stack.pop().unwrap();
        if self.updating {
            self.expired.get_or_insert_with(Vec::new);
        }
        // Direct EFFECT expiry removes one; delayed expiry deliberately does not.
        if self.updating || current.kind() != 15 {
            while self.stack.last().is_some_and(Generator::removed) {
                let removed = self.stack.pop().unwrap();
                self.finalize(&removed, context, owner)?;
                // MotionMaster.cpp:222,264 delete now or retain until UpdateMotion.
                if self.updating {
                    self.expired.as_mut().unwrap().push(removed);
                }
            }
        }
        let previous = self.stack.last().map(Generator::identity);
        self.finalize(&current, context, owner)?;
        if self.updating {
            if matches!(current, Generator::Owned(_)) {
                self.expired.as_mut().unwrap().push(current);
            }
        } else {
            // MotionMaster.cpp:231-232 deletes the generator and its follower list element.
            drop(current);
            if self.stack.is_empty() {
                self.initialize(context, owner)?;
            }
            if reset && previous == self.stack.last().map(Generator::identity) {
                self.reset_top(context, owner)?;
            }
        }
        Ok(())
    }
    pub fn update(
        &mut self,
        diff: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if context.units.get(context.objects, self.unit)?.state & MOTION_CANNOT_MOVE != 0 {
            return Ok(());
        }
        let top = self
            .stack
            .last()
            .cloned()
            .ok_or_else(|| refuse("execution.motion.empty_update_assertion"))?;
        require(!self.updating, "execution.motion.recursive_update")?;
        self.updating = true;
        let result = match &top {
            Generator::Idle => Ok(true),
            Generator::Owned(g) => g.update(self, diff, context, owner),
        };
        self.updating = false;
        // The callback reference must be released before deferred deletion.
        drop(top);
        if !result? {
            self.expire(true, context, owner)?;
        }
        if let Some(expired) = self.expired.take() {
            // MotionMaster.cpp:138-147 deletes in deferred-list order.
            for generator in expired {
                drop(generator);
            }
            if self.stack.is_empty() {
                self.initialize(context, owner)?;
            }
            if self.needs_reset {
                self.reset_top(context, owner)?;
                self.needs_reset = false;
            }
        }
        Ok(())
    }
    pub fn move_home(
        &mut self,
        run: bool,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if context.units.get(context.objects, self.unit)?.state & MOTION_LOST_CONTROL != 0 {
            return Ok(());
        }
        self.clear(false, false, context, owner)?;
        if matches!(
            context.objects.get(self.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) {
            self.mutate(SourceHomeGenerator::new(run), context, owner)?;
        }
        Ok(())
    }
    /// MotionMaster.cpp:349-365. Creature linking has already resolved its target.
    pub fn move_follow<'a>(
        unit: ObjectHandle,
        motion: impl FnOnce() -> Result<&'a mut Self>,
        request: FollowRequest,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
        on_linked: impl FnOnce(FollowerReferenceId, &SourceFollowers),
    ) -> Result<()> {
        if context.units.get(context.objects, unit)?.state & MOTION_LOST_CONTROL != 0 {
            return Ok(());
        }
        let motion = motion()?;
        // MotionMaster.cpp:354-357 clears before constructing the generator.
        motion.clear(
            !request.main && !motion.is_empty(),
            request.main,
            context,
            owner,
        )?;
        // MotionMaster.cpp:365 checks the control flag only for players.
        let possess = context.objects.get(unit)?.kind == SourceObjectKind::Player
            && context.units.get(context.objects, unit)?.flags & 8 == 0;
        let generator =
            SourceFollowGenerator::construct(unit, request, possess, context, on_linked)?;
        motion.mutate_owned(generator, context, owner)
    }
    /// MotionMaster.cpp:466-484. An existing waypoint is not replaced.
    pub fn move_waypoint(
        &mut self,
        request: WaypointPathRequest,
        forced_movement: u32,
        target: Option<SourceGuid>,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if !matches!(
            context.objects.get(self.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) || self.current_type() == 2
        {
            return Ok(());
        }
        self.current_path = Some(request.path_id as u32);
        let generator = Rc::new(SourceWaypointGenerator::default());
        generator.forced_movement.set(forced_movement);
        generator.target_guid.set(target);
        if matches!(self.current_type(), 5 | 12) {
            self.expire(false, context, owner)?;
        }
        if let Some(Generator::Owned(g)) = self.stack.last().cloned() {
            g.interrupt(self, context, owner)?;
        }
        generator.initialize(self, context, owner)?;
        self.stack.push(Generator::Owned(generator.clone()));
        generator.initialize_path(self, context, owner, request)
    }
    /// MotionMaster.cpp:492-510 checks enum 4 although Linear reports enum 2.
    pub fn move_linear_waypoint(
        &mut self,
        request: WaypointPathRequest,
        forced_movement: u32,
        target: Option<SourceGuid>,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if !matches!(
            context.objects.get(self.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) || self.current_type() == 4
        {
            return Ok(());
        }
        self.current_path = Some(request.path_id as u32);
        let generator = Rc::new(SourceWaypointGenerator::linear());
        generator.forced_movement.set(forced_movement);
        generator.target_guid.set(target);
        if matches!(self.current_type(), 5 | 12) {
            self.expire(false, context, owner)?;
        }
        if let Some(Generator::Owned(g)) = self.stack.last().cloned() {
            g.interrupt(self, context, owner)?;
        }
        generator.initialize(self, context, owner)?;
        self.stack.push(Generator::Owned(generator.clone()));
        generator.initialize_path(self, context, owner, request)
    }
    /// MotionMaster.cpp:425-444. Empty point paths do not mutate the stack.
    pub fn move_path_points(
        &mut self,
        points: &[[f32; 3]],
        orientation: f32,
        options: FixedPathOptions,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        self.mutate(
            SourceFixedPathGenerator::from_points(points, Some(orientation), options),
            context,
            owner,
        )
    }
    pub fn move_path(
        &mut self,
        path_id: i32,
        origin: WaypointOrigin,
        options: FixedPathOptions,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.current_path = Some(path_id as u32);
        let generator =
            SourceFixedPathGenerator::load(self, context, owner, path_id, origin, options)?;
        self.mutate(generator, context, owner)
    }
    /// MotionMaster.cpp:629-675. PATH pauses clear the retained spline.
    pub fn pause_waypoints(
        &mut self,
        time: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if !matches!(
            context.objects.get(self.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) {
            return Ok(());
        }
        self.stop_moving(context, owner)?;
        if time == 0 {
            context.units.get_mut(context.objects, self.unit)?.state |= WAYPOINT_PAUSED;
        } else if matches!(self.current_type(), 1..=3) {
            if let Some(Generator::Owned(g)) = self.stack.last().cloned() {
                g.pause(time as i32, true)?;
            }
        }
        Ok(())
    }
    pub fn unpause_waypoints(&mut self, context: &mut MotionContext<'_>) -> Result<()> {
        if !matches!(
            context.objects.get(self.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) {
            return Ok(());
        }
        context.units.get_mut(context.objects, self.unit)?.state &= !WAYPOINT_PAUSED;
        if matches!(self.current_type(), 2 | 3) {
            if let Some(Generator::Owned(g)) = self.stack.last().cloned() {
                g.pause(0, true)?;
            }
        }
        Ok(())
    }
    pub fn formation(
        &mut self,
        command: FormationMotion,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        match command {
            FormationMotion::Initialize => self.initialize(context, owner),
            FormationMotion::Clear { reset, all } => self.clear(reset, all, context, owner),
            FormationMotion::Idle => {
                context.objects.get(self.unit)?;
                self.move_idle();
                Ok(())
            }
            // SpawnGroup.cpp:890 calls MoveWaypoint with PATH_FROM_WAYPOINT_PATH.
            FormationMotion::Waypoint {
                path,
                linear: false,
            } => self.move_waypoint(
                WaypointPathRequest {
                    path_id: path as i32,
                    origin: WaypointOrigin::WaypointPath,
                    ..Default::default()
                },
                0,
                None,
                context,
                owner,
            ),
            FormationMotion::Waypoint { path, linear: true } => self.move_linear_waypoint(
                WaypointPathRequest {
                    path_id: path as i32,
                    origin: WaypointOrigin::WaypointPath,
                    ..Default::default()
                },
                0,
                None,
                context,
                owner,
            ),
            _ => Err(command.missing_owner()),
        }
    }
    /// Unit.cpp:10070-10092; AI_ROOT is independently owned by UnitAI.
    pub fn immobilize(
        &mut self,
        apply: bool,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if apply {
            context.units.get_mut(context.objects, self.unit)?.state |= 0x10;
            // Unit.cpp:6496-6520, not beneficiary or generic player ownership.
            let f = context.units.get(context.objects, self.unit)?;
            let client_controlled = if f.flags & 8 == 0 || f.flags & 0x00c0_0004 != 0 {
                false
            } else if let Some(charmer) = f.charmer {
                f.flags & 0x0100_0000 != 0
                    && context
                        .objects
                        .get(charmer)?
                        .guid
                        .is_some_and(|g| g.raw() != 0 && g.high() == HighGuid::Player as u16)
            } else {
                context.objects.get(self.unit)?.kind == SourceObjectKind::Player
            };
            if !client_controlled {
                self.stop_moving(context, owner)?;
            }
            owner.motion_call(self, MotionCall::SendRoot(true), context)?;
        } else {
            context.units.get_mut(context.objects, self.unit)?.state &= !0x10;
            if !context.units.get(context.objects, self.unit)?.immobilized() {
                owner.motion_call(self, MotionCall::SendRoot(false), context)?;
            }
            context
                .units
                .get_mut(context.objects, self.unit)?
                .ignore_ranged_targets = false;
        }
        Ok(())
    }
    /// CombatManager.cpp:161-176, including equality and charm guards.
    pub fn set_evade_home(
        &mut self,
        home: bool,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let f = context.units.get_mut(context.objects, self.unit)?;
        if f.evade_home == home && !f.evade_combat {
            return Ok(());
        }
        if !home {
            f.evade_timer = 0;
        }
        if home || f.evade_home {
            owner.ai_order(self.unit, if home { 4 } else { 0 })?;
        }
        f.evade_home = home;
        f.evade_combat = false;
        if f.charmer.is_none() {
            owner.controlled_evade(self.unit, home)?;
        }
        Ok(())
    }
}
impl Drop for SourceMotionMaster {
    fn drop(&mut self) {
        // MotionMaster.cpp:101-110: top-down destruction without owner callbacks.
        while let Some(generator) = self.stack.pop() {
            drop(generator);
        }
    }
}
