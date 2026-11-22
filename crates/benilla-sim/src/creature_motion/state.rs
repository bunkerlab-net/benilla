// SPDX-License-Identifier: GPL-3.0-or-later
// Source-derived Unit::UpdateSplineMovement/Position and ordinary chase/home branches.
use super::{
    protocol::{ask, validate_route},
    *,
};
use serde::{Deserialize, Serialize};
mod wander;
pub use wander::{
    wander_normalized_word, DefaultMovement, WanderDraw, WanderFailedRouteHead,
    WanderNormalRouteHead, WanderSnapshot, WANDER_POLICY_ID,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundSettings {
    pub combat_reach: Real,
    pub bounding_radius: Real,
    pub collision_height: Real,
    pub no_wounded_slowdown: bool,
    pub backpedal_enabled: bool,
    pub fanning_enabled: bool,
}
impl GroundSettings {
    fn validate(self) -> Result<(), MotionError> {
        for n in [
            self.combat_reach,
            self.bounding_radius,
            self.collision_height,
        ] {
            n.nonnegative("owner bounds")?;
        }
        Ok(())
    }
}
/// An unresolved source fact is not evidence that an unsupported branch is absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Evidence {
    Absent,
    Present,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionAudit {
    pub flight_swimming_transport: Evidence,
    pub immobilization: Evidence,
    pub non_melee_casting: Evidence,
    pub speed_auras: Evidence,
    pub combat_scripts: Evidence,
    pub target_airborne_or_transport: Evidence,
}
impl MotionAudit {
    fn validate(self) -> Result<(), MotionError> {
        let values = [
            self.flight_swimming_transport,
            self.immobilization,
            self.non_melee_casting,
            self.speed_auras,
            self.combat_scripts,
            self.target_airborne_or_transport,
        ];
        if values.contains(&Evidence::Unknown) {
            return Err(MotionError::Missing("current movement domain evidence"));
        }
        if values.contains(&Evidence::Present) {
            return Err(MotionError::Unsupported(
                "current movement mode/aura/script",
            ));
        }
        Ok(())
    }
}
/// A generator launched by a Unit event before this frame's spline update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStart {
    Chase {
        target: Target,
        walking: bool,
    },
    Home {
        destination: Pose,
        walking: bool,
        was_active: bool,
    },
}
/// Snapshot of live staged Unit values. The caller must reject unsupported flight,
/// transport, immobilization, casts and movement auras before entering this domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitInputs {
    pub audit: MotionAudit,
    pub health: u32,
    pub maximum_health: u32,
    pub walk_speed: Real,
    pub run_speed: Real,
    pub settings: GroundSettings,
}
impl UnitInputs {
    pub fn validate(self) -> Result<(), MotionError> {
        self.audit.validate()?;
        self.settings.validate()?;
        chase_speed(
            self.run_speed,
            false,
            self.health,
            self.maximum_health,
            self.settings.no_wounded_slowdown,
        )?;
        chase_speed(
            self.walk_speed,
            true,
            self.health,
            self.maximum_health,
            self.settings.no_wounded_slowdown,
        )?;
        Ok(())
    }
    fn speed(self, walking: bool, home: bool) -> Result<Real, MotionError> {
        let base = if walking {
            self.walk_speed
        } else {
            self.run_speed
        };
        if home {
            Ok(base)
        } else {
            chase_speed(
                base,
                walking,
                self.health,
                self.maximum_health,
                self.settings.no_wounded_slowdown,
            )
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ChaseMode {
    Normal,
    Backpedal,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChaseSnapshot {
    pub target: u64,
    pub walking: bool,
    pub recheck_ms: u32,
    pub reached: bool,
    pub reachable: bool,
    pub speed_changed: bool,
    pub last_target: Point,
    pub closeness_ms: u32,
    pub closeness_expired: bool,
    pub mode: ChaseMode,
    pub requested_goal: Option<Point>,
    pub pathfinder_cache: Option<Box<PathFinderCache>>,
    /// First actual Chase dispatch, retained even when calculate returns NoPath.
    #[serde(deserialize_with = "super::pathfinder_dispatch::required_option")]
    pub pathfinder_construction: Option<QueryBinding>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Generator {
    Idle,
    Wander(WanderSnapshot),
    Chase(ChaseSnapshot),
    Home { destination: Pose, was_active: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveSplineSnapshot {
    pub route_request: RouteRequest,
    pub route: CompleteRoute,
    /// Immutable result evidence after Home's local PathFinder was destroyed.
    /// Never a retained generator cache or authority to rehydrate a native query.
    #[serde(deserialize_with = "super::pathfinder_dispatch::required_option")]
    pub local_home_cache: Option<Box<PathFinderCache>>,
    pub spline_id: u64,
    pub path_identity: [u8; 32],
    pub route_source: RouteSource,
    pub spline: SplineSnapshot,
    pub facing: Facing,
    pub walking: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionSnapshot {
    pub policy: String,
    pub identity: Identity,
    pub now_ms: u64,
    pub processed_map_clock_ms: u64,
    pub last_move_ms: u64,
    pub map_pose: Pose,
    pub relocation_timer_ms: u32,
    pub generator: Generator,
    #[serde(deserialize_with = "super::pathfinder_dispatch::required_option")]
    pub suspended_wander: Option<WanderSnapshot>,
    pub spline: Option<ActiveSplineSnapshot>,
    pub query_id: u64,
    pub spline_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Dispatch {
    Launched(Option<Box<WanderNormalRouteHead>>),
    NoPath(Option<Box<WanderFailedRouteHead>>),
    Unreachable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionState {
    state: MotionSnapshot,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    SplineLaunched {
        path: Vec<Point>,
        spline_id: u64,
        path_identity: [u8; 32],
        source: RouteSource,
        duration_ms: u32,
        velocity: Real,
        facing: Facing,
        walking: bool,
    },
    SplineAdvanced {
        spline_id: u64,
        event: SplineEvent,
    },
    Relocated {
        pose: Pose,
        at_ms: u64,
        notify_map: bool,
    },
    FacingChanged {
        orientation: Real,
    },
    ChaseReached,
    ChaseUnreachable,
    SplineInterrupted {
        spline_id: u64,
        pose: Pose,
    },
    HomeStarted {
        was_active: bool,
    },
    /// Caller runs source Finalize lifecycle/addon/faction/home events and clears
    /// combat-start state here. This event alone does not perform those callbacks.
    HomeReached {
        restore_inactive: bool,
    },
}
impl MotionState {
    pub fn new(
        identity: Identity,
        pose: Pose,
        now_ms: u64,
        last_move_ms: u64,
    ) -> Result<Self, MotionError> {
        pose.validate()?;
        if identity.creature == 0
            || identity.content.0 == [0; 32]
            || identity.geometry.0 == [0; 32]
            || last_move_ms > now_ms
        {
            return Err(MotionError::Invalid("motion identity/clock"));
        }
        Ok(Self {
            state: MotionSnapshot {
                policy: CREATURE_MOTION_POLICY_ID.into(),
                identity,
                now_ms,
                processed_map_clock_ms: now_ms,
                last_move_ms,
                map_pose: pose,
                relocation_timer_ms: 0,
                generator: Generator::Idle,
                suspended_wander: None,
                spline: None,
                query_id: 0,
                spline_id: 0,
            },
        })
    }
    pub fn restore(state: MotionSnapshot) -> Result<Self, MotionError> {
        Self::new(
            state.identity,
            state.map_pose,
            state.now_ms,
            state.last_move_ms,
        )?;
        if state.policy != CREATURE_MOTION_POLICY_ID
            || state.processed_map_clock_ms > state.now_ms
            || state.now_ms - state.processed_map_clock_ms > 60_000
            || state.relocation_timer_ms > 400
            || state.spline_id > u64::from(u32::MAX)
        {
            return Err(MotionError::Invalid("relocation timer"));
        }
        if let Some(s) = &state.spline {
            LinearSpline::restore(s.spline.clone())?;
            validate_route(&s.route_request, &s.route)?;
            s.route_request.validate_pathfinder_dispatch()?;
            if s.local_home_cache.is_some()
                && s.route_request.kind != RouteKind::HomeForceDestination
            {
                return Err(MotionError::Invalid("local Home cache on another route"));
            }
            if let Some(cache) = &s.local_home_cache {
                RouteCalculation {
                    wander_no_path: None,
                    source: s.route.source,
                    outcome: RouteOutcome::Complete(s.route.clone()),
                    prior_cache: None,
                    after_cache: (**cache).clone(),
                }
                .validate_for(&s.route_request)?;
            }
            if s.route.source == RouteSource::OwnerPathFinder {
                s.route_request.validate_source_cache_dispatch()?;
                let source_cache = match s.route_request.kind {
                    RouteKind::HomeForceDestination => s.local_home_cache.as_ref(),
                    RouteKind::Wander => match &state.generator {
                        Generator::Wander(w) => w.pathfinder_cache.as_ref(),
                        Generator::Chase(_) => state
                            .suspended_wander
                            .as_ref()
                            .and_then(|w| w.pathfinder_cache.as_ref()),
                        _ => None,
                    },
                    _ => match &state.generator {
                        Generator::Chase(c) => c.pathfinder_cache.as_ref(),
                        _ => None,
                    },
                };
                if source_cache.is_none_or(|cache| cache.source != RouteSource::OwnerPathFinder) {
                    return Err(MotionError::Invalid(
                        "saved source route without source cache",
                    ));
                }
            }
            if let Some(cache) = &s.route_request.prior_cache {
                cache.validate_for(state.identity)?;
                if cache.revision >= s.route_request.binding.query_id
                    || cache.source != s.route.source
                {
                    return Err(MotionError::Invalid("saved route prior cache sequence"));
                }
            }
            let mut corrected = s.route.points.clone();
            corrected[0] = s.route_request.launch_origin;
            if s.route_request.binding.identity != state.identity
                || s.route_request.binding.query_id == 0
                || s.route_request.binding.query_id > state.query_id
                || s.route_request.binding.now_ms > state.now_ms
                || s.spline.points != corrected
                || s.route.path_identity != s.path_identity
                || s.route.source != s.route_source
            {
                return Err(MotionError::Invalid("saved spline route evidence binding"));
            }
            if s.spline_id == 0 || s.spline_id != state.spline_id || s.path_identity == [0; 32] {
                return Err(MotionError::Invalid("saved path identity"));
            }
            if let Facing::Angle(angle) = s.facing {
                if !angle.less(Real::constant(0x40c9_0fdb)) {
                    return Err(MotionError::Invalid("saved half-open spline facing"));
                }
                Pose {
                    position: state.map_pose.position,
                    orientation: angle,
                }
                .validate()?;
            }
        }
        if let Some(wander) = &state.suspended_wander {
            wander.validate(&state)?;
            if let Some(spline) = state
                .spline
                .as_ref()
                .filter(|s| s.route_request.kind == RouteKind::Wander)
            {
                // Interrupt leaves an already-finalized Unit spline in place when
                // Chase requires no route; its evidence still belongs to Wander.
                if !matches!(state.generator, Generator::Chase(_)) || !spline.spline.finalized {
                    return Err(MotionError::Invalid("suspended Wander live spline"));
                }
                wander.validate_spline(spline, &state)?;
            }
            if let Some(normal) = &wander.normal_route {
                if state.spline.as_ref().is_some_and(|spline| {
                    spline.route_request.kind != RouteKind::Wander
                        && (spline.route_request.binding.query_id
                            <= normal.request.binding.query_id
                            || spline.route_request.binding.now_ms < normal.request.binding.now_ms)
                }) {
                    return Err(MotionError::Invalid(
                        "suspended Wander NORMAL route chronology",
                    ));
                }
            }
            if !matches!(
                state.generator,
                Generator::Chase(_) | Generator::Home { .. }
            ) {
                return Err(MotionError::Invalid(
                    "suspended default owner without overlay",
                ));
            }
        }
        match &state.generator {
            Generator::Idle => {}
            Generator::Wander(wander) => {
                wander.validate(&state)?;
                if wander.normal_route.is_some() && state.spline.is_none() {
                    return Err(MotionError::Invalid("active Wander NORMAL without spline"));
                }
                if let Some(failed) = &wander.failed_route {
                    if failed.request.origin != state.map_pose.position
                        || failed
                            .request
                            .binding
                            .now_ms
                            .checked_add(u64::from(500 - wander.next_move_timer_ms))
                            != Some(state.processed_map_clock_ms)
                        || state.spline.as_ref().is_some_and(|spline| {
                            !spline.spline.finalized
                                || spline.route_request.binding.query_id
                                    >= failed.request.binding.query_id
                        })
                    {
                        return Err(MotionError::Invalid("saved Wander failed route chronology"));
                    }
                }
                if let Some(spline) = &state.spline {
                    wander.validate_spline(spline, &state)?;
                }
            }
            Generator::Chase(c) => {
                let chase_spline = state.spline.as_ref().filter(|s| {
                    matches!(
                        s.route_request.kind,
                        RouteKind::ChaseInitial | RouteKind::ChaseReplan | RouteKind::Backpedal
                    )
                });
                c.last_target.validate()?;
                if c.target == 0 || c.recheck_ms > 1000 || c.closeness_ms > 2000 {
                    return Err(MotionError::Invalid("chase snapshot"));
                }
                if let Some(p) = c.requested_goal {
                    p.validate()?;
                }
                if let Some(construction) = c.pathfinder_construction {
                    if construction.identity != state.identity
                        || construction.query_id == 0
                        || construction.query_id > state.query_id
                        || construction.now_ms > state.now_ms
                    {
                        return Err(MotionError::Invalid(
                            "saved pathfinder construction binding",
                        ));
                    }
                }
                if c.pathfinder_construction.is_some() != c.requested_goal.is_some() {
                    return Err(MotionError::Invalid(
                        "saved constructor dispatch/goal presence",
                    ));
                }
                if c.pathfinder_cache.is_some() && c.pathfinder_construction.is_none() {
                    return Err(MotionError::Invalid("saved cache without construction"));
                }
                if let Some(dispatch) =
                    chase_spline.and_then(|s| s.route_request.pathfinder_dispatch.as_ref())
                {
                    if c.pathfinder_construction != Some(dispatch.construction_binding()) {
                        return Err(MotionError::Invalid(
                            "saved pathfinder construction lineage",
                        ));
                    }
                }
                if c.pathfinder_cache.is_none()
                    && chase_spline.is_some_and(|s| s.route_request.prior_cache.is_some())
                {
                    return Err(MotionError::Invalid("saved chase lost retained cache"));
                }
                if let Some(cache) = &c.pathfinder_cache {
                    cache.validate_for(state.identity)?;
                    if cache.source == RouteSource::OwnerPathFinder {
                        if cache.revision == 1 && (cache.path_type != 1 || cache.active_len == 0) {
                            return Err(MotionError::Invalid("saved Chase initial source cache"));
                        }
                        let construction = c.pathfinder_construction.unwrap();
                        // Every accepted calculation consumes its own dispatch
                        // query; unrelated queries cannot reset this sequence.
                        if cache.revision - 1 > state.query_id - construction.query_id
                            || chase_spline
                                .is_some_and(|s| s.route.source != RouteSource::OwnerPathFinder)
                        {
                            return Err(MotionError::Invalid("saved source cache sequence/source"));
                        }
                    }
                    if let Some(prior) =
                        chase_spline.and_then(|s| s.route_request.prior_cache.as_ref())
                    {
                        if prior.binding != cache.binding
                            || (prior.revision.checked_add(1) == Some(cache.revision)
                                && (cache.path_type != 1
                                    || cache.active_len == 0
                                    || cache.requested_end
                                        != chase_spline.unwrap().route_request.goal))
                            || prior.source != cache.source
                            || prior.include_flags != cache.include_flags
                            || prior.exclude_flags != cache.exclude_flags
                            || prior.revision >= cache.revision
                            || prior.poly_refs.len() > cache.poly_refs.len()
                        {
                            return Err(MotionError::Invalid("saved pathfinder cache lineage"));
                        }
                    }
                    if let Some(spline) = chase_spline.filter(|s| {
                        s.route.source == RouteSource::OwnerPathFinder
                            && s.route_request.prior_cache.is_none()
                    }) {
                        if cache.revision == 1 {
                            // The first current cache is the spline's complete
                            // calculation. Later NoPath or unreachable Complete
                            // may retain this spline with a newer current cache;
                            // neither restores a loaded query.
                            RouteCalculation {
                                wander_no_path: None,
                                source: spline.route.source,
                                outcome: RouteOutcome::Complete(spline.route.clone()),
                                prior_cache: None,
                                after_cache: (**cache).clone(),
                            }
                            .validate_for(&spline.route_request)?;
                        } else if state.query_id <= spline.route_request.binding.query_id {
                            return Err(MotionError::Invalid("saved source first cache sequence"));
                        }
                    }
                    if cache.revision > state.query_id
                        || Some(cache.requested_end) != c.requested_goal
                    {
                        return Err(MotionError::Invalid(
                            "saved pathfinder cache sequence/endpoint",
                        ));
                    }
                }
            }
            Generator::Home { destination, .. } => {
                destination.validate()?;
                let spline = state
                    .spline
                    .as_ref()
                    .ok_or(MotionError::Invalid("Home without source spline"))?;
                if spline.spline.finalized
                    || spline.spline.points.last() != Some(&destination.position)
                    || spline.facing != Facing::Angle(destination.orientation)
                {
                    return Err(MotionError::Invalid("Home spline/destination/facing"));
                }
            }
        }
        Ok(Self { state })
    }
    /// Bind an early external Unit event to the current map frame without
    /// consuming this creature's later Unit diff. Only one frame may be open.
    pub fn begin_map_frame(&mut self, now_ms: u64) -> Result<(), MotionError> {
        if now_ms < self.state.now_ms
            || now_ms - self.state.processed_map_clock_ms > 60_000
            || (self.state.now_ms != self.state.processed_map_clock_ms
                && now_ms != self.state.now_ms)
        {
            return Err(MotionError::Invalid("motion event map clock"));
        }
        self.state.now_ms = now_ms;
        Ok(())
    }
    fn consume_map_frame(&mut self, elapsed_ms: u32) -> Result<(), MotionError> {
        let next = self
            .state
            .processed_map_clock_ms
            .checked_add(u64::from(elapsed_ms))
            .ok_or(MotionError::Invalid("motion clock"))?;
        if self.state.now_ms != self.state.processed_map_clock_ms && self.state.now_ms != next {
            return Err(MotionError::Invalid("motion frame diff binding"));
        }
        self.state.now_ms = next;
        self.state.processed_map_clock_ms = next;
        Ok(())
    }
    /// Shared map time continues while Creature::DEAD skips Unit::Update.
    /// No spline, Unit timer, event timer or lastMoveTime advances here.
    pub fn advance_without_unit(&mut self, elapsed_ms: u32) -> Result<(), MotionError> {
        if self.is_moving() || !matches!(self.state.generator, Generator::Idle) {
            return Err(MotionError::Invalid("inactive creature retains motion"));
        }
        self.consume_map_frame(elapsed_ms)
    }
    /// Source corpse-removal map relocation changes pose but not lastMoveTime.
    /// The caller schedules OnRelocated and preserves the separate event clock.
    pub fn lifecycle_relocate(&mut self, pose: Pose) -> Result<(), MotionError> {
        pose.validate()?;
        if self.is_moving() || !matches!(self.state.generator, Generator::Idle) {
            return Err(MotionError::Invalid(
                "lifecycle relocation with live motion",
            ));
        }
        self.state.map_pose = pose;
        Ok(())
    }
    /// Unit::SetInFront only updates orientation. It does not launch a spline,
    /// relocate, schedule AI notification, or update lastMoveTime.
    pub fn face_target(
        &mut self,
        target: Point,
        authority: &mut impl Authority,
    ) -> Result<Event, MotionError> {
        let mut next = self.clone();
        let orientation = next.bearing(next.pose().position, target, authority)?;
        next.state.map_pose.orientation = orientation;
        *self = next;
        Ok(Event::FacingChanged { orientation })
    }
    pub fn snapshot(&self) -> MotionSnapshot {
        self.state.clone()
    }
    pub fn pose(&self) -> Pose {
        self.state.map_pose
    }
    pub fn now_ms(&self) -> u64 {
        self.state.now_ms
    }
    pub fn last_move_ms(&self) -> u64 {
        self.state.last_move_ms
    }
    pub fn generator(&self) -> &Generator {
        &self.state.generator
    }
    pub fn spline_position(&self) -> Result<Point, MotionError> {
        match &self.state.spline {
            Some(s) => LinearSpline::restore(s.spline.clone())?.position(),
            None => Ok(self.state.map_pose.position),
        }
    }
    pub fn moving_and_running(&self) -> bool {
        self.state
            .spline
            .as_ref()
            .is_some_and(|s| !s.spline.finalized && !s.walking)
    }
    pub fn is_moving(&self) -> bool {
        self.state
            .spline
            .as_ref()
            .is_some_and(|s| !s.spline.finalized)
    }
    pub fn stationary_melee_refresh_eligible(&self, target_is_moving: bool) -> bool {
        !target_is_moving
            && self
                .state
                .last_move_ms
                .checked_add(3000)
                .is_some_and(|at| at < self.state.now_ms)
    }
    pub fn reachable(&self) -> Option<bool> {
        match &self.state.generator {
            Generator::Chase(c) => Some(!c.reached || c.reachable),
            _ => None,
        }
    }
    fn binding(&mut self) -> Result<QueryBinding, MotionError> {
        self.state.query_id = self
            .state
            .query_id
            .checked_add(1)
            .ok_or(MotionError::Invalid("query counter"))?;
        Ok(QueryBinding {
            identity: self.state.identity,
            query_id: self.state.query_id,
            now_ms: self.state.now_ms,
        })
    }
    fn bearing(
        &mut self,
        from: Point,
        toward: Point,
        authority: &mut impl Authority,
    ) -> Result<Real, MotionError> {
        let query = Query::Bearing(BearingRequest {
            binding: self.binding()?,
            origin: from,
            toward,
        });
        let Fact::Bearing(angle) = ask(authority, query)? else {
            return Err(MotionError::Invalid("bearing fact kind"));
        };
        Pose {
            position: from,
            orientation: angle,
        }
        .validate()?;
        Ok(angle)
    }
    fn relocate(
        &mut self,
        notify_map: bool,
        target: Option<Target>,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        let Some(s) = &self.state.spline else {
            return Ok(());
        };
        let position = LinearSpline::restore(s.spline.clone())?.position()?;
        let facing = s.facing;
        let face_position = if let Facing::Target(id) = facing {
            if let Some(target) = target.filter(|target| target.id == id) {
                Some(target.pose.position)
            } else {
                let request = Query::FacingTarget(FacingTargetRequest {
                    binding: self.binding()?,
                    target: id,
                });
                let Fact::ObjectPosition(position) = ask(authority, request)? else {
                    return Err(MotionError::Invalid("facing object fact kind"));
                };
                if let Some(point) = position {
                    point.validate()?;
                }
                position
            }
        } else {
            None
        };
        let orientation = if let Facing::Angle(angle) = facing {
            angle
        } else if let Some(toward) = face_position {
            self.bearing(self.state.map_pose.position, toward, authority)?
        } else if position.x == self.state.map_pose.position.x
            && position.y == self.state.map_pose.position.y
        {
            self.state.map_pose.orientation
        } else {
            self.bearing(self.state.map_pose.position, position, authority)?
        };
        self.state.map_pose = Pose {
            position,
            orientation,
        };
        self.state.last_move_ms = self.state.now_ms;
        events.push(Event::Relocated {
            pose: self.state.map_pose,
            at_ms: self.state.now_ms,
            notify_map,
        });
        Ok(())
    }
    fn progress_spline(
        &mut self,
        elapsed_ms: u32,
        target: Option<Target>,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        let Some(s) = self.state.spline.as_mut().filter(|s| !s.spline.finalized) else {
            return Ok(());
        };
        let mut spline = LinearSpline::restore(s.spline.clone())?;
        events.extend(
            spline
                .advance(elapsed_ms)
                .into_iter()
                .map(|event| Event::SplineAdvanced {
                    spline_id: s.spline_id,
                    event,
                }),
        );
        s.spline = spline.snapshot();
        self.state.relocation_timer_ms = self.state.relocation_timer_ms.saturating_sub(elapsed_ms);
        if self.state.relocation_timer_ms == 0 || spline.finalized() {
            self.state.relocation_timer_ms = 400;
            self.relocate(true, target, authority, events)?;
        }
        Ok(())
    }
    fn dispatch(
        &mut self,
        goal: Point,
        cache: &mut Option<Box<PathFinderCache>>,
        construction: &mut Option<QueryBinding>,
        kind: RouteKind,
        wander: Option<WanderRequest>,
        walking: bool,
        facing: Facing,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<Dispatch, MotionError> {
        // DispatchSplineToPosition forces relocation before querying PathFinder.
        // Home MoveSplineInit corrects its initial vertex too, so both use the
        // actual computed spline origin instead of a stale renderer/map point.
        if self.is_moving() && kind != RouteKind::HomeForceDestination {
            self.relocate(true, target, authority, events)?;
        }
        let launch_origin = if self.is_moving() {
            self.spline_position()?
        } else {
            self.pose().position
        };
        let binding = self.binding()?;
        // TargetedMovementGenerator.cpp:498-499: lazy construction follows forced
        // relocation and precedes the straight gate/calculation in the authority.
        let pathfinder_dispatch = if kind == RouteKind::HomeForceDestination {
            Some(PathfinderDispatch::home(binding)?)
        } else if kind == RouteKind::Wander {
            Some(PathfinderDispatch::wander(
                construction.ok_or(MotionError::Missing("Wander initialized PathFinder"))?,
                binding,
                false,
            )?)
        } else {
            let event = PathfinderDispatch::new(*construction, binding)?;
            *construction = Some(event.construction_binding());
            Some(event)
        };
        let request = RouteRequest {
            binding,
            origin: self.state.map_pose.position,
            launch_origin,
            goal,
            kind,
            wander: wander.map(Box::new),
            target,
            owner_collision_height: input.settings.collision_height,
            point_limit: MAX_PATH_POINTS as u16,
            prior_cache: cache.clone(),
            pathfinder_dispatch,
        };
        if let Some(prior) = cache.as_ref() {
            prior.validate_for(request.binding.identity)?;
        }
        let (result, after_cache, failed_route) =
            match ask(authority, Query::Route(request.clone()))? {
                Fact::RouteCalculation(calculation) => {
                    calculation.validate_for(&request)?;
                    let failed_route = calculation.wander_no_path.map(|reason| {
                        Box::new(WanderFailedRouteHead {
                            request: request.clone(),
                            reason,
                        })
                    });
                    (
                        calculation.outcome,
                        Some(Box::new(calculation.after_cache)),
                        failed_route,
                    )
                }
                // Legacy authored fixtures remain cache-free. They cannot erase a cache.
                Fact::Route(result) if cache.is_none() => {
                    if matches!(&result, RouteOutcome::Complete(path)
                    if path.source == RouteSource::OwnerPathFinder)
                    {
                        return Err(MotionError::Missing("source pathfinder calculation state"));
                    }
                    (result, None, None)
                }
                _ => return Err(MotionError::Invalid("route fact kind/cache")),
            };
        // The surrounding MotionState/native frame transaction owns rollback,
        // including failures after a valid calculation but before frame completion.
        *cache = after_cache;
        let path = match result {
            RouteOutcome::Complete(path) => path,
            RouteOutcome::NoPath if kind != RouteKind::HomeForceDestination => {
                return Ok(Dispatch::NoPath(failed_route));
            }
            RouteOutcome::Missing => return Err(MotionError::Missing("owner pathfinder/route")),
            RouteOutcome::NoPath | RouteOutcome::Shortcut => {
                return Err(MotionError::Unsupported(
                    "home 400-speed shortcut without source geometry closure",
                ));
            }
            RouteOutcome::Incomplete => {
                return Err(MotionError::Unsupported("incomplete source route"));
            }
            RouteOutcome::Truncated => return Err(MotionError::Invalid("truncated route")),
        };
        validate_route(&request, &path)?;
        if kind == RouteKind::ChaseReplan {
            let t = target.ok_or(MotionError::Invalid("chase target"))?;
            let reach =
                crate::creature_ai::combat_reach(input.settings.combat_reach, t.combat_reach)?;
            if goal
                .square_distance(t.pose.position, false)?
                .greater(reach.mul(reach)?)
            {
                return Ok(Dispatch::Unreachable);
            }
        }
        let speed = if kind == RouteKind::Wander {
            chase_speed(
                input.walk_speed,
                false,
                input.health,
                input.maximum_health,
                input.settings.no_wounded_slowdown,
            )?
        } else {
            input.speed(walking, kind == RouteKind::HomeForceDestination)?
        };
        let spline = LinearSpline::launch(path.points.clone(), launch_origin, speed)?;
        self.state.spline_id = self
            .state
            .spline_id
            .checked_add(1)
            .ok_or(MotionError::Invalid("spline counter"))?;
        if self.state.spline_id > u64::from(u32::MAX) {
            return Err(MotionError::Invalid("source spline id domain"));
        }
        events.push(Event::SplineLaunched {
            path: spline.snapshot().points,
            spline_id: self.state.spline_id,
            path_identity: path.path_identity,
            source: path.source,
            duration_ms: spline.duration_ms(),
            velocity: speed,
            facing,
            walking,
        });
        // RandomMovementGenerator::_setLocation launches the accepted path;
        // retain its exact calculation head independently of Unit spline replacement.
        let normal_route = (kind == RouteKind::Wander && cache.is_some()).then(|| {
            Box::new(WanderNormalRouteHead {
                request: request.clone(),
                route: path.clone(),
            })
        });
        self.state.spline = Some(ActiveSplineSnapshot {
            route_request: request,
            route: path.clone(),
            local_home_cache: if kind == RouteKind::HomeForceDestination {
                cache.clone()
            } else {
                None
            },
            spline_id: self.state.spline_id,
            path_identity: path.path_identity,
            route_source: path.source,
            spline: spline.snapshot(),
            facing,
            walking,
        });
        Ok(Dispatch::Launched(normal_route))
    }
    fn near_point(
        &mut self,
        target: Target,
        distance: Real,
        angle: NearPointAngle,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<Point, MotionError> {
        let query = Query::NearPoint(NearPointRequest {
            binding: self.binding()?,
            owner: self.state.map_pose,
            target,
            owner_bounding_radius: input.settings.bounding_radius,
            distance,
            angle,
        });
        let Fact::NearPoint(point) = ask(authority, query)? else {
            return Err(MotionError::Invalid("near-point fact kind"));
        };
        point.validate()?;
        Ok(point)
    }
    fn requires_position(
        &self,
        target: Target,
        point: Point,
        input: UnitInputs,
    ) -> Result<bool, MotionError> {
        let reach =
            crate::creature_ai::combat_reach(input.settings.combat_reach, target.combat_reach)?;
        Ok(target
            .pose
            .position
            .square_distance(point, true)?
            .greater(reach.mul(reach)?))
    }
    /// Ordinary UnitAI default: offset=angle=0, moveFurther=false, combat=true.
    /// Called after AI AttackStart; launches but never consumes elapsed frame time.
    pub fn start_chase(
        &mut self,
        target: Target,
        walking: bool,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<Vec<Event>, MotionError> {
        let mut staged = self.clone();
        input.validate()?;
        target.validate()?;
        let mut events = vec![];
        if let Generator::Chase(c) = &mut staged.state.generator {
            // MotionMaster::MoveChase reuses an existing chase generator.
            c.target = target.id;
            *self = staged;
            return Ok(events);
        }
        staged.suspend_wander(Some(target), authority, &mut events)?;
        let mut chase = ChaseSnapshot {
            target: target.id,
            walking,
            recheck_ms: 0,
            reached: false,
            reachable: true,
            speed_changed: false,
            last_target: target.pose.position,
            closeness_ms: 0,
            closeness_expired: false,
            mode: ChaseMode::Normal,
            requested_goal: None,
            pathfinder_cache: None,
            pathfinder_construction: None,
        };
        if staged.requires_position(target, staged.pose().position, input)? {
            let distance =
                crate::creature_ai::combat_reach(input.settings.combat_reach, target.combat_reach)?
                    .mul(Real::constant(0x3f00_0000))?;
            let goal = staged.near_point(
                target,
                distance,
                NearPointAngle::CurrentTargetToOwner,
                input,
                authority,
            )?;
            staged.dispatch(
                goal,
                &mut chase.pathfinder_cache,
                &mut chase.pathfinder_construction,
                RouteKind::ChaseInitial,
                None,
                walking,
                Facing::Target(target.id),
                Some(target),
                input,
                authority,
                &mut events,
            )?;
            chase.requested_goal = Some(goal);
        }
        staged.state.generator = Generator::Chase(chase);
        *self = staged;
        Ok(events)
    }
    /// Immediate Unit::InterruptMoving relocation is separate from the next
    /// Unit update. The opaque proof synchronizes the AI mirror at this boundary.
    pub fn start_chase_with_relocation(
        &mut self,
        target: Target,
        walking: bool,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<(Vec<Event>, Option<SourceRelocation>), MotionError> {
        let origin = self.pose().position;
        let events = self.start_chase(target, walking, input, authority)?;
        let relocated = events
            .iter()
            .any(|event| matches!(event, Event::Relocated { .. }));
        let receipt = relocated.then_some(SourceRelocation {
            identity: self.state.identity,
            origin,
            position: self.pose().position,
            now_ms: self.now_ms(),
            purpose: crate::creature_ai::movement::MovementPurpose::Chase { target: target.id },
            home: None,
        });
        Ok((events, receipt))
    }
    pub fn speed_changed(&mut self) {
        if let Generator::Chase(c) = &mut self.state.generator {
            c.speed_changed = true;
        }
    }
    pub fn delay_chase(&mut self) {
        if let Generator::Chase(c) = &mut self.state.generator {
            c.recheck_ms = 1000;
        }
    }
    pub fn start_home(
        &mut self,
        mut destination: Pose,
        walking: bool,
        was_active: bool,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<Vec<Event>, MotionError> {
        destination.validate()?;
        // MoveSplineInit::SetFacing(float) uses G3D::wrap's half-open interval.
        if destination.orientation == Real::constant(0x40c9_0fdb) {
            destination.orientation = Real::ZERO;
        }
        input.validate()?;
        let mut staged = self.clone();
        let mut events = vec![];
        if staged.state.suspended_wander.is_some() {
            // Clear(false) removes Chase; Mutate(Home) interrupts the underlying
            // Wander again before Home Initialize (MotionMaster.cpp:300,614).
            staged.interrupt(None, authority, &mut events)?;
        } else {
            staged.suspend_wander(None, authority, &mut events)?;
        }
        // Home::_setTargetLocation observes the map pose after Mutate interrupted
        // the underlying default, including any synchronous spline relocation.
        if staged
            .pose()
            .position
            .square_distance(destination.position, true)?
            .greater(Real::integer(150).mul(Real::integer(150))?)
        {
            return Err(MotionError::Unsupported(
                "long-distance outdoor home forced despawn",
            ));
        }
        events.push(Event::HomeStarted { was_active });
        // Home owns a fresh stack-local PathFinder and discards it after launch.
        let mut home_cache = None;
        let mut home_construction = None;
        staged.dispatch(
            destination.position,
            &mut home_cache,
            &mut home_construction,
            RouteKind::HomeForceDestination,
            None,
            walking,
            Facing::Angle(destination.orientation),
            None,
            input,
            authority,
            &mut events,
        )?;
        staged.state.generator = Generator::Home {
            destination,
            was_active,
        };
        *self = staged;
        Ok(events)
    }
    /// Home can synchronously interrupt the retained default Wander beneath Chase.
    /// Keep that source relocation bound to the current Home command for the AI mirror.
    pub fn start_home_with_relocation(
        &mut self,
        destination: Pose,
        walking: bool,
        was_active: bool,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<(Vec<Event>, Option<SourceRelocation>), MotionError> {
        let origin = self.pose().position;
        let events = self.start_home(destination, walking, was_active, input, authority)?;
        let relocated = events
            .iter()
            .any(|event| matches!(event, Event::Relocated { .. }));
        let receipt = relocated.then_some(SourceRelocation {
            identity: self.state.identity,
            origin,
            position: self.pose().position,
            now_ms: self.now_ms(),
            purpose: crate::creature_ai::movement::MovementPurpose::Home,
            home: Some(destination.position),
        });
        Ok((events, receipt))
    }
    fn interrupt(
        &mut self,
        target: Option<Target>,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        if self.is_moving() {
            self.relocate(false, target, authority, events)?;
            self.state.spline = None;
            let spline_id = self.next_spline_id()?;
            events.push(Event::SplineInterrupted {
                spline_id,
                pose: self.pose(),
            });
        }
        Ok(())
    }
    /// Unit::Update spline first, then MotionMaster generator. Atomic on any
    /// missing/invalid fact; callbacks themselves must remain read-only proposals.
    pub fn advance(
        &mut self,
        elapsed_ms: u32,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<Vec<Event>, MotionError> {
        self.advance_inner(elapsed_ms, target, input, authority, None)
    }
    fn advance_inner(
        &mut self,
        elapsed_ms: u32,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
        start: Option<FrameStart>,
    ) -> Result<Vec<Event>, MotionError> {
        if elapsed_ms == 0 || elapsed_ms > 60_000 {
            return Err(MotionError::Invalid("motion elapsed"));
        }
        input.validate()?;
        if let Some(t) = target {
            t.validate()?;
        }
        let mut staged = self.clone();
        staged.consume_map_frame(elapsed_ms)?;
        let mut events = if let Some(start) = start {
            match start {
                FrameStart::Chase {
                    target: new_target,
                    walking,
                } => {
                    if target != Some(new_target) {
                        return Err(MotionError::Invalid("event-phase target binding"));
                    }
                    staged.start_chase(new_target, walking, input, authority)?
                }
                FrameStart::Home {
                    destination,
                    walking,
                    was_active,
                } => staged.start_home(destination, walking, was_active, input, authority)?,
            }
        } else {
            vec![]
        };
        staged.progress_spline(elapsed_ms, target, authority, &mut events)?;
        match staged.state.generator.clone() {
            Generator::Idle => {}
            Generator::Wander(mut wander) => {
                staged.update_wander(&mut wander, elapsed_ms, input, authority, &mut events)?;
                staged.state.generator = Generator::Wander(wander);
            }
            Generator::Home { was_active, .. } => {
                if !staged.is_moving() {
                    events.push(Event::HomeReached {
                        restore_inactive: !was_active,
                    });
                    staged.reset_default()?;
                }
            }
            Generator::Chase(mut chase) => {
                if let Some(t) = target.filter(|t| t.id == chase.target) {
                    staged.update_chase(
                        &mut chase,
                        elapsed_ms,
                        t,
                        input,
                        authority,
                        &mut events,
                    )?;
                }
                staged.state.generator = Generator::Chase(chase);
            }
        }
        *self = staged;
        Ok(events)
    }
    fn update_chase(
        &mut self,
        chase: &mut ChaseSnapshot,
        elapsed_ms: u32,
        target: Target,
        input: UnitInputs,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        chase.recheck_ms = chase.recheck_ms.saturating_sub(elapsed_ms);
        if chase.closeness_ms != 0 {
            chase.closeness_ms = chase.closeness_ms.saturating_sub(elapsed_ms);
            if chase.closeness_ms == 0 {
                chase.closeness_expired = true;
            }
        }
        if chase.recheck_ms == 0 {
            chase.recheck_ms = 250;
            let destination = self
                .state
                .spline
                .as_ref()
                .map(|s| *s.spline.points.last().unwrap())
                .filter(|p| {
                    *p != Point {
                        x: Real::ZERO,
                        y: Real::ZERO,
                        z: Real::ZERO,
                    }
                })
                .unwrap_or(self.pose().position);
            let target_moved = self.requires_position(target, destination, input)?;
            if (chase.speed_changed && self.is_moving()) || target_moved {
                let goal = match chase.requested_goal {
                    Some(goal) if !target_moved => goal,
                    _ => {
                        let reach = crate::creature_ai::combat_reach(
                            input.settings.combat_reach,
                            target.combat_reach,
                        )?
                        .mul(Real::constant(0x3f00_0000))?;
                        self.near_point(
                            target,
                            reach,
                            NearPointAngle::CurrentTargetToOwner,
                            input,
                            authority,
                        )?
                    }
                };
                if self
                    .pose()
                    .position
                    .square_distance(goal, true)?
                    .greater(Real::constant(0x3e99_999a))
                {
                    let result = self.dispatch(
                        goal,
                        &mut chase.pathfinder_cache,
                        &mut chase.pathfinder_construction,
                        RouteKind::ChaseReplan,
                        None,
                        chase.walking,
                        Facing::Target(target.id),
                        Some(target),
                        input,
                        authority,
                        events,
                    )?;
                    chase.requested_goal = Some(goal);
                    match result {
                        Dispatch::Launched(_) => {
                            chase.reached = false;
                            chase.speed_changed = false;
                            chase.last_target = target.pose.position;
                            chase.closeness_ms = 0;
                            return self.finalize_chase(chase, target, authority, events);
                        }
                        Dispatch::Unreachable => {
                            chase.reachable = false;
                            events.push(Event::ChaseUnreachable);
                            return self.finalize_chase(chase, target, authority, events);
                        }
                        Dispatch::NoPath(_) => {}
                    }
                }
                let reach = crate::creature_ai::combat_reach(
                    input.settings.combat_reach,
                    target.combat_reach,
                )?;
                if self
                    .pose()
                    .position
                    .square_distance(target.pose.position, false)?
                    .greater(reach.mul(reach)?)
                {
                    chase.reachable = false;
                    events.push(Event::ChaseUnreachable);
                }
            } else {
                chase.reachable = true;
                if !chase.reached
                    && self.is_moving()
                    && chase.mode == ChaseMode::Normal
                    && target.pose.position != chase.last_target
                {
                    let from_destination =
                        self.pose().position.square_distance(destination, true)?;
                    let from_target = self
                        .pose()
                        .position
                        .square_distance(target.pose.position, true)?;
                    let reach = crate::creature_ai::combat_reach(
                        input.settings.combat_reach,
                        target.combat_reach,
                    )?;
                    if from_destination.greater(from_target) && from_target.less(reach.mul(reach)?)
                    {
                        self.interrupt(Some(target), authority, events)?;
                    }
                } else if chase.reached && chase.closeness_expired {
                    self.reposition(chase, target, input, authority, events)?;
                }
            }
        }
        self.finalize_chase(chase, target, authority, events)
    }
    fn finalize_chase(
        &mut self,
        chase: &mut ChaseSnapshot,
        target: Target,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        if !self.is_moving() && !chase.reached {
            chase.reached = true;
            let angle = self.bearing(self.pose().position, target.pose.position, authority)?;
            // HasInArc(target, .01f): compare a normalized relative source angle.
            let mut difference = angle.sub(self.pose().orientation)?;
            let tau = Real::constant(0x40c9_0fdb);
            if difference.less(Real::ZERO) {
                difference = difference.add(tau)?;
            }
            if difference.greater(Real::constant(0x4049_0fdb)) {
                difference = difference.sub(tau)?;
            }
            let half_arc = Real::constant(0x3ba3_d70a); // .01f / 2.f
            if difference.greater(half_arc) || difference.less(Real::ZERO.sub(half_arc)?) {
                self.state.map_pose.orientation = angle;
                events.push(Event::FacingChanged { orientation: angle });
            }
            chase.closeness_ms = 2000;
            chase.last_target = target.pose.position;
            chase.mode = ChaseMode::Normal;
            chase.reachable = true;
            events.push(Event::ChaseReached);
        }
        Ok(())
    }
    fn reposition(
        &mut self,
        chase: &mut ChaseSnapshot,
        target: Target,
        input: UnitInputs,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        if !input.settings.backpedal_enabled {
            return Ok(());
        }
        chase.closeness_expired = false;
        chase.closeness_ms = 2000;
        let reach = input.settings.combat_reach.add(target.combat_reach)?;
        let target_distance = if reach.less(Real::constant(0x4070_0000)) {
            reach
        } else {
            Real::constant(0x4070_0000)
        };
        let threshold = target_distance
            .mul(target_distance)?
            .mul(Real::constant(0x3ea8_f5c3))?;
        if target
            .pose
            .position
            .square_distance(self.pose().position, true)?
            .less(threshold)
        {
            let mut angle = self.pose().orientation.add(Real::constant(0x4049_0fdb))?;
            let tau = Real::constant(0x40c9_0fdb);
            if !angle.less(tau) {
                angle = angle.sub(tau)?;
            }
            let goal = self.near_point(
                target,
                target_distance.mul(Real::constant(0x3f40_0000))?,
                NearPointAngle::Absolute(angle),
                input,
                authority,
            )?;
            let result = self.dispatch(
                goal,
                &mut chase.pathfinder_cache,
                &mut chase.pathfinder_construction,
                RouteKind::Backpedal,
                None,
                true,
                Facing::Tangent,
                Some(target),
                input,
                authority,
                events,
            )?;
            chase.requested_goal = Some(goal);
            if matches!(result, Dispatch::Launched(_)) {
                chase.reached = false;
                chase.mode = ChaseMode::Backpedal;
                chase.speed_changed = false;
                chase.closeness_ms = 0;
            }
        } else if input.settings.fanning_enabled {
            let query = Query::Collider(CollisionRequest {
                binding: self.binding()?,
                owner: self.pose(),
                victim: target.id,
                radius_squared: Real::ONE,
            });
            let Fact::Collider(collider) = ask(authority, query)? else {
                return Err(MotionError::Invalid("collider fact kind"));
            };
            if collider.is_some() {
                return Err(MotionError::Unsupported(
                    "fanning random direction/angle and occupied-cell visitor",
                ));
            }
        }
        Ok(())
    }
    fn next_spline_id(&mut self) -> Result<u64, MotionError> {
        self.state.spline_id = self
            .state
            .spline_id
            .checked_add(1)
            .filter(|id| *id <= u64::from(u32::MAX))
            .ok_or(MotionError::Invalid("spline identity"))?;
        Ok(self.state.spline_id)
    }
    /// Unit::SetDeathState calls StopMoving(true): stop at computed spline XYZ,
    /// but retain map position/lastMoveTime until later source relocation.
    pub fn stop_for_death(&mut self) -> Result<Event, MotionError> {
        let pose = Pose {
            position: self.spline_position()?,
            orientation: self.pose().orientation,
        };
        let spline_id = self.next_spline_id()?;
        self.clear();
        Ok(Event::SplineInterrupted { spline_id, pose })
    }
    /// Death/explicit motion expiry clears movement without creating travelled
    /// distance. Caller supplies the authoritative lifecycle transition separately.
    pub fn clear(&mut self) {
        self.state.spline = None;
        self.state.generator = Generator::Idle;
        self.state.suspended_wander = None;
    }
}

/// Exact synchronous source relocation, never an external geometry outcome or
/// checkpoint payload. Only a completed MotionState mutation can mint it.
/// ```compile_fail
/// use benilla_sim::creature_motion::SourceRelocation;
/// let _ = SourceRelocation {};
/// ```
/// ```compile_fail
/// fn serializable<T: serde::Serialize>() {}
/// serializable::<benilla_sim::creature_motion::SourceRelocation>();
/// ```
/// ```compile_fail
/// let _: benilla_sim::creature_motion::SourceRelocation = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRelocation {
    pub(crate) identity: Identity,
    pub(crate) origin: Point,
    pub(crate) position: Point,
    pub(crate) now_ms: u64,
    pub(crate) purpose: crate::creature_ai::movement::MovementPurpose,
    pub(crate) home: Option<Point>,
}

/// An internal source completion and its ordered events. No serde implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAdvance {
    pub movement: crate::creature_ai::movement::ValidatedMovement,
    pub events: Vec<Event>,
}
impl MotionState {
    /// Advance the actual source state and bind its completed result to an
    /// existing AI command. This is the only source-token constructor; the
    /// ordinary external endpoint authority never bypasses its own speed checks.
    pub fn advance_for_command(
        &mut self,
        command: &crate::creature_ai::MotionCommand,
        request_id: u64,
        elapsed_ms: u32,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<SourceAdvance, MotionError> {
        self.advance_command(
            command, request_id, elapsed_ms, target, input, authority, None,
        )
    }
    /// Event-phase AttackStart precedes UpdateSplineMovement. This path advances
    /// the map clock once, launches at that clock, then consumes exactly one diff.
    pub fn advance_for_event_command(
        &mut self,
        command: &crate::creature_ai::MotionCommand,
        request_id: u64,
        elapsed_ms: u32,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
        start: FrameStart,
    ) -> Result<SourceAdvance, MotionError> {
        self.advance_command(
            command,
            request_id,
            elapsed_ms,
            target,
            input,
            authority,
            Some(start),
        )
    }
    fn advance_command(
        &mut self,
        command: &crate::creature_ai::MotionCommand,
        request_id: u64,
        elapsed_ms: u32,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
        start: Option<FrameStart>,
    ) -> Result<SourceAdvance, MotionError> {
        use crate::creature_ai::movement::{
            MovementPurpose, MovementRequest, MovementResult, SourceMotion, ValidatedMovement,
        };
        if request_id == 0 || command.command_id == 0 {
            return Err(MotionError::Invalid("source command identity"));
        }
        if let MovementPurpose::Chase { target: expected } = command.purpose {
            if target.is_some_and(|target| target.id != expected) {
                return Err(MotionError::Invalid("source command target identity"));
            }
        }
        let origin = self.pose().position;
        let goal = if let Some(start) = start {
            match (start, command.purpose) {
                (
                    FrameStart::Chase {
                        target: new_target, ..
                    },
                    MovementPurpose::Chase { target: id },
                ) if new_target.id == id => None,
                (FrameStart::Home { destination, .. }, MovementPurpose::Home)
                    if command.home == Some(destination.position) =>
                {
                    Some(destination.position)
                }
                _ => return Err(MotionError::Invalid("event-phase command binding")),
            }
        } else {
            match (&self.state.generator, command.purpose) {
                (Generator::Chase(chase), MovementPurpose::Chase { target })
                    if chase.target == target =>
                {
                    Some(
                        self.state
                            .spline
                            .as_ref()
                            .map(|s| *s.spline.points.last().unwrap())
                            .unwrap_or(self.pose().position),
                    )
                }
                (Generator::Home { destination, .. }, MovementPurpose::Home)
                    if command.home == Some(destination.position) =>
                {
                    Some(destination.position)
                }
                _ => return Err(MotionError::Invalid("source generator/AI command binding")),
            }
        };
        let events = self.advance_inner(elapsed_ms, target, input, authority, start)?;
        let goal = goal.unwrap_or_else(|| {
            self.state
                .spline
                .as_ref()
                .map(|s| *s.spline.points.last().unwrap())
                .unwrap_or(self.pose().position)
        });
        let request = MovementRequest {
            request_id,
            command_id: command.command_id,
            creature_id: self.state.identity.creature,
            map_id: self.state.identity.map,
            origin,
            goal,
            // Source Unit rate for the current spline's walk/run mode; the actual
            // wounded launch velocity remains in the validated spline state.
            speed: if self.state.spline.as_ref().is_some_and(|s| s.walking) {
                input.walk_speed
            } else {
                input.run_speed
            },
            elapsed_ms,
            content_revision: self.state.identity.content,
            geometry_revision: self.state.identity.geometry,
            purpose: command.purpose,
        };
        let source = SourceMotion {
            position: self.pose().position,
            relocated: events
                .iter()
                .any(|event| matches!(event, Event::Relocated { .. })),
            home_arrived: events
                .iter()
                .any(|event| matches!(event, Event::HomeReached { .. })),
            reachable: self.reachable(),
            moving_and_running: self.moving_and_running(),
            last_move_ms: self.last_move_ms(),
            now_ms: self.now_ms(),
            stationary_melee_refresh_eligible: target.is_some_and(|target| {
                self.stationary_melee_refresh_eligible(target.movement_flags.is_moving())
            }),
            target_moving_and_running: target
                .is_some_and(|target| target.movement_flags.moving_and_running()),
        };
        Ok(SourceAdvance {
            movement: ValidatedMovement {
                request,
                result: MovementResult::SourceSpline(source),
            },
            events,
        })
    }
}
