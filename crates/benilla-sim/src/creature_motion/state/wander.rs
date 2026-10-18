// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 RandomMovementGenerator.cpp:27-148,177-203; upstream GPL-2.0-or-later.
use super::*;

pub const WANDER_POLICY_ID: &str = "cmangos-8ec338a1-default-wander-ground-walk-v3";
/// Semantic RNG requests, in source call order. Angle/radius return full u32
/// words; count/pause return their inclusive bounded integer results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WanderDraw {
    Angle,
    Radius,
    MoveCount,
    Pause,
}

/// The ordinary un-controlled, non-random-running default generator selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DefaultMovement {
    Idle,
    Wander { center: Point, radius: Real },
}
impl DefaultMovement {
    pub fn validate(self) -> Result<(), MotionError> {
        if let Self::Wander { center, radius } = self {
            center.validate()?;
            radius.nonnegative("Wander respawn radius")?;
            if radius == Real::ZERO || radius.greater(Real::integer(150)) {
                return Err(MotionError::Unsupported("Wander radius domain"));
            }
        }
        Ok(())
    }
    pub fn movement_type(self) -> u32 {
        match self {
            Self::Idle => 0,
            Self::Wander { .. } => 1,
        }
    }
}
/// Exact latest failed calculation, bound to this generator's retained cache.
/// Serialized requests and refs are evidence only, never loaded native authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WanderFailedRouteHead {
    pub request: RouteRequest,
    pub reason: WanderNoPathReason,
}
/// Exact latest successful calculation, independent of the Unit's current spline.
/// Evidence only: restoring this head never creates loaded PathFinder authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WanderNormalRouteHead {
    pub request: RouteRequest,
    pub route: CompleteRoute,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WanderSnapshot {
    pub center: Point,
    pub radius: Real,
    pub next_move_timer_ms: u32,
    pub next_move_count: u32,
    pub pathfinder_cache: Option<Box<PathFinderCache>>,
    pub pathfinder_construction: QueryBinding,
    #[serde(deserialize_with = "super::super::pathfinder_dispatch::required_option")]
    pub failed_route: Option<Box<WanderFailedRouteHead>>,
    #[serde(deserialize_with = "super::super::pathfinder_dispatch::required_option")]
    pub normal_route: Option<Box<WanderNormalRouteHead>>,
}
impl WanderSnapshot {
    pub(super) fn validate(&self, state: &MotionSnapshot) -> Result<(), MotionError> {
        DefaultMovement::Wander {
            center: self.center,
            radius: self.radius,
        }
        .validate()?;
        let construction = self.pathfinder_construction;
        if !(1..=3).contains(&self.next_move_count)
            || self.next_move_timer_ms > 10_000
            || construction.identity != state.identity
            || construction.query_id == 0
            || construction.query_id > state.query_id
            || construction.now_ms > state.now_ms
        {
            return Err(MotionError::Invalid("Wander owner snapshot"));
        }
        if let Some(cache) = &self.pathfinder_cache {
            cache.validate_for(state.identity)?;
            if !matches!(cache.path_type, 1 | 8)
                || cache.revision > state.query_id - construction.query_id
            {
                return Err(MotionError::Invalid("Wander cache construction sequence"));
            }
        }
        if self
            .pathfinder_cache
            .as_ref()
            .is_some_and(|cache| cache.path_type == 8)
            != self.failed_route.is_some()
        {
            return Err(MotionError::Invalid("Wander failed route presence"));
        }
        if let Some(failed) = &self.failed_route {
            let request = &failed.request;
            let cache = self
                .pathfinder_cache
                .as_ref()
                .ok_or(MotionError::Invalid("Wander failed route without cache"))?;
            self.validate_head_request(request, state)?;
            if self.next_move_timer_ms == 0 || self.next_move_timer_ms > 500 {
                return Err(MotionError::Invalid("Wander failed route owner binding"));
            }
            RouteCalculation {
                wander_no_path: Some(failed.reason),
                source: cache.source,
                outcome: RouteOutcome::NoPath,
                prior_cache: request.prior_cache.clone(),
                after_cache: (**cache).clone(),
            }
            .validate_for(request)?;
        }
        if self
            .pathfinder_cache
            .as_ref()
            .is_some_and(|cache| cache.path_type == 1)
            != self.normal_route.is_some()
        {
            return Err(MotionError::Invalid("Wander NORMAL route presence"));
        }
        if let Some(normal) = &self.normal_route {
            self.validate_head_request(&normal.request, state)?;
            if normal.request.binding.now_ms > state.processed_map_clock_ms {
                return Err(MotionError::Invalid("Wander NORMAL route processed clock"));
            }
            RouteCalculation {
                wander_no_path: None,
                source: normal.route.source,
                outcome: RouteOutcome::Complete(normal.route.clone()),
                prior_cache: normal.request.prior_cache.clone(),
                after_cache: (**self.pathfinder_cache.as_ref().unwrap()).clone(),
            }
            .validate_for(&normal.request)?;
        }
        Ok(())
    }
    pub(super) fn validate_spline(
        &self,
        spline: &ActiveSplineSnapshot,
        state: &MotionSnapshot,
    ) -> Result<(), MotionError> {
        if spline.route_request.kind != RouteKind::Wander
            || spline.facing != Facing::Tangent
            || !spline.walking
            || spline
                .route_request
                .wander
                .as_deref()
                .is_none_or(|request| {
                    request.center != self.center || request.radius != self.radius
                })
            || spline
                .route_request
                .pathfinder_dispatch
                .as_ref()
                .is_none_or(|d| d.construction_binding() != self.pathfinder_construction)
        {
            return Err(MotionError::Invalid("saved Wander spline owner"));
        }
        if let Some(normal) = &self.normal_route {
            if normal.request != spline.route_request || normal.route != spline.route {
                return Err(MotionError::Invalid("saved Wander NORMAL spline head"));
            }
            let duration = *spline.spline.timestamps_ms.last().unwrap();
            if u64::from(spline.spline.elapsed_ms)
                != (state.processed_map_clock_ms - normal.request.binding.now_ms)
                    .min(u64::from(duration))
            {
                return Err(MotionError::Invalid("saved Wander NORMAL route chronology"));
            }
        }
        if let Some(cache) = &self.pathfinder_cache {
            let prior_revision = spline
                .route_request
                .prior_cache
                .as_ref()
                .map_or(0, |c| c.revision);
            if cache.revision <= prior_revision
                || cache.source != spline.route.source
                || spline
                    .route_request
                    .prior_cache
                    .as_ref()
                    .is_some_and(|prior| {
                        prior.binding != cache.binding
                            || prior.poly_refs.len() > cache.poly_refs.len()
                            || prior.include_flags != cache.include_flags
                            || prior.exclude_flags != cache.exclude_flags
                    })
            {
                return Err(MotionError::Invalid("saved Wander cache lineage"));
            }
            if cache.revision == prior_revision + 1 {
                RouteCalculation {
                    wander_no_path: None,
                    source: spline.route.source,
                    outcome: RouteOutcome::Complete(spline.route.clone()),
                    prior_cache: spline.route_request.prior_cache.clone(),
                    after_cache: (**cache).clone(),
                }
                .validate_for(&spline.route_request)?;
            }
        } else if spline.route_request.prior_cache.is_some() {
            return Err(MotionError::Invalid("saved Wander lost cache"));
        }
        if self.failed_route.as_ref().is_some_and(|failed| {
            !spline.spline.finalized
                || spline.route_request.binding.query_id >= failed.request.binding.query_id
        }) {
            return Err(MotionError::Invalid(
                "saved Wander failed spline chronology",
            ));
        }
        Ok(())
    }
    fn validate_head_request(
        &self,
        request: &RouteRequest,
        state: &MotionSnapshot,
    ) -> Result<(), MotionError> {
        request.validate_pathfinder_dispatch()?;
        request.origin.validate()?;
        request
            .owner_collision_height
            .nonnegative("Wander owner height")?;
        if request.kind != RouteKind::Wander
            || request.binding.identity != state.identity
            || request.binding.query_id > state.query_id
            || request.binding.now_ms > state.now_ms
            || request.pathfinder_dispatch.as_ref().is_none_or(|dispatch| {
                dispatch.construction_binding() != self.pathfinder_construction
                    || dispatch.first_construction()
            })
            || request.wander.as_deref().is_none_or(|sample| {
                sample.center != self.center
                    || sample.radius != self.radius
                    || sample.prepared.query_id <= self.pathfinder_construction.query_id
            })
            || self.pathfinder_cache.as_ref().is_none_or(|cache| {
                request
                    .binding
                    .query_id
                    .checked_sub(self.pathfinder_construction.query_id)
                    .is_none_or(|sequence| cache.revision > sequence)
            })
        {
            return Err(MotionError::Invalid("Wander route head owner binding"));
        }
        Ok(())
    }
}
/// libstdc++ generate_canonical<float,24> over a full32 generator: round the
/// word to binary32, divide by 2^32, clamp rounded one to nextafter(1,0).
/// Native RNG ownership remains the versioned deterministic runtime stream.
pub fn wander_normalized_word(word: u32) -> Result<Real, MotionError> {
    let n = Real::integer(word).mul(Real::constant(0x2f80_0000))?;
    Ok(if n == Real::ONE {
        Real::constant(0x3f7f_ffff)
    } else {
        n
    })
}
impl MotionState {
    /// Model MotionMaster::Initialize ownership without RNG or launch.
    /// Reserve construction identity here; native allocation/filter creation is
    /// deferred to the first WanderPrepare using its current owner inputs.
    pub fn initialize_default(&mut self, selection: DefaultMovement) -> Result<(), MotionError> {
        selection.validate()?;
        if !matches!(self.state.generator, Generator::Idle)
            || self.is_moving()
            || self.state.suspended_wander.is_some()
        {
            return Err(MotionError::Invalid(
                "default initialization with active generator",
            ));
        }
        if let DefaultMovement::Wander { center, radius } = selection {
            self.state.generator = Generator::Wander(WanderSnapshot {
                center,
                radius,
                next_move_timer_ms: 0,
                next_move_count: 1,
                pathfinder_cache: None,
                pathfinder_construction: self.binding()?,
                failed_route: None,
                normal_route: None,
            });
        }
        Ok(())
    }
    pub fn default_movement(&self) -> DefaultMovement {
        match (&self.state.generator, &self.state.suspended_wander) {
            (Generator::Wander(w), _) | (_, Some(w)) => DefaultMovement::Wander {
                center: w.center,
                radius: w.radius,
            },
            _ => DefaultMovement::Idle,
        }
    }
    pub(super) fn suspend_wander(
        &mut self,
        target: Option<Target>,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        if let Generator::Wander(wander) = self.state.generator.clone() {
            // MotionMaster::Mutate calls Interrupt even if Chase needs no route.
            self.interrupt(target, authority, events)?;
            self.state.suspended_wander = Some(wander);
        }
        Ok(())
    }
    pub(super) fn reset_default(&mut self) -> Result<(), MotionError> {
        if let Some(mut wander) = self.state.suspended_wander.take() {
            // Reset preserves count, but Initialize replaces the unique PathFinder.
            wander.next_move_timer_ms = 0;
            wander.pathfinder_cache = None;
            wander.failed_route = None;
            wander.normal_route = None;
            wander.pathfinder_construction = self.binding()?;
            self.state.spline = None;
            self.state.generator = Generator::Wander(wander);
        } else {
            self.state.generator = Generator::Idle;
        }
        Ok(())
    }
    pub(super) fn update_wander(
        &mut self,
        wander: &mut WanderSnapshot,
        diff: u32,
        input: UnitInputs,
        authority: &mut impl Authority,
        events: &mut Vec<Event>,
    ) -> Result<(), MotionError> {
        if self.is_moving() {
            return Ok(());
        }
        wander.next_move_timer_ms = wander.next_move_timer_ms.saturating_sub(diff);
        if wander.next_move_timer_ms != 0 {
            return Ok(());
        }
        // ComputePathToRandomPoint clears retained storage, updates the owner
        // filter and selects the live query BEFORE obtaining either random word.
        let prepared = self.binding()?;
        let prefix = RouteRequest {
            binding: prepared,
            origin: self.pose().position,
            launch_origin: self.pose().position,
            goal: wander.center,
            kind: RouteKind::WanderPrepare,
            wander: None,
            target: None,
            owner_collision_height: input.settings.collision_height,
            point_limit: MAX_PATH_POINTS as u16,
            prior_cache: wander.pathfinder_cache.clone(),
            pathfinder_dispatch: Some(PathfinderDispatch::wander(
                wander.pathfinder_construction,
                prepared,
                wander.pathfinder_cache.is_none(),
            )?),
        };
        if !matches!(ask(authority, Query::Route(prefix))?, Fact::WanderPrepared) {
            return Err(MotionError::Invalid("Wander source prefix evidence"));
        }
        let angle_draw = wander_normalized_word(authority.wander_draw(WanderDraw::Angle)?)?;
        let radius_draw = wander_normalized_word(authority.wander_draw(WanderDraw::Radius)?)?;
        let request = WanderRequest {
            prepared,
            center: wander.center,
            radius: wander.radius,
            angle_draw,
            radius_draw,
        };
        let goal = request.endpoint()?;
        let mut construction = Some(wander.pathfinder_construction);
        let result = self.dispatch(
            goal,
            &mut wander.pathfinder_cache,
            &mut construction,
            RouteKind::Wander,
            Some(request),
            true,
            Facing::Tangent,
            None,
            input,
            authority,
            events,
        )?;
        if let Dispatch::NoPath(failed_route) = result {
            wander.failed_route = failed_route;
            wander.normal_route = None;
            wander.next_move_timer_ms = 500;
        } else if let Dispatch::Launched(normal_route) = result {
            wander.failed_route = None;
            wander.normal_route = normal_route;
            if wander.next_move_count > 1 {
                wander.next_move_count -= 1;
            } else {
                let count = authority.wander_draw(WanderDraw::MoveCount)?;
                let pause = authority.wander_draw(WanderDraw::Pause)?;
                if !(1..=3).contains(&count) || !(3000..=10000).contains(&pause) {
                    return Err(MotionError::Invalid("Wander RNG bounds"));
                }
                wander.next_move_count = count;
                wander.next_move_timer_ms = pause;
            }
        } else {
            return Err(MotionError::Invalid("Wander unreachable dispatch"));
        }
        Ok(())
    }
    /// Default motion is a Unit update, with no invented UnitAI motion command.
    pub fn advance_default(
        &mut self,
        request_id: u64,
        elapsed_ms: u32,
        target: Option<Target>,
        input: UnitInputs,
        authority: &mut impl Authority,
    ) -> Result<SourceAdvance, MotionError> {
        use crate::creature_ai::movement::{
            MovementPurpose, MovementRequest, MovementResult, SourceMotion, ValidatedMovement,
        };
        if request_id == 0 || !matches!(self.state.generator, Generator::Wander(_)) {
            return Err(MotionError::Invalid("default Wander update binding"));
        }
        let origin = self.pose().position;
        let events = self.advance(elapsed_ms, target, input, authority)?;
        let request = MovementRequest {
            request_id,
            command_id: 0,
            creature_id: self.state.identity.creature,
            map_id: self.state.identity.map,
            origin,
            goal: self
                .state
                .spline
                .as_ref()
                .and_then(|s| s.spline.points.last().copied())
                .unwrap_or(origin),
            speed: input.walk_speed,
            elapsed_ms,
            content_revision: self.state.identity.content,
            geometry_revision: self.state.identity.geometry,
            purpose: MovementPurpose::Wander,
        };
        let source = SourceMotion {
            position: self.pose().position,
            relocated: events.iter().any(|e| matches!(e, Event::Relocated { .. })),
            home_arrived: false,
            reachable: None,
            moving_and_running: self.moving_and_running(),
            last_move_ms: self.last_move_ms(),
            now_ms: self.now_ms(),
            stationary_melee_refresh_eligible: target.is_some_and(|t| {
                self.stationary_melee_refresh_eligible(t.movement_flags.is_moving())
            }),
            target_moving_and_running: target
                .is_some_and(|t| t.movement_flags.moving_and_running()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature_ai::movement::{Check, Revision, SegmentChecks};
    use std::collections::VecDeque;
    fn point(x: f32) -> Point {
        Point::new(x, 0., 0.).unwrap()
    }
    fn pose(x: f32) -> Pose {
        Pose {
            position: point(x),
            orientation: Real::ZERO,
        }
    }
    fn input() -> UnitInputs {
        UnitInputs {
            health: 100,
            maximum_health: 100,
            walk_speed: Real::new(2.5).unwrap(),
            run_speed: Real::integer(7),
            settings: GroundSettings {
                combat_reach: Real::new(1.5).unwrap(),
                bounding_radius: Real::new(0.5).unwrap(),
                collision_height: Real::integer(2),
                no_wounded_slowdown: false,
                backpedal_enabled: false,
                fanning_enabled: false,
            },
            audit: MotionAudit {
                flight_swimming_transport: Evidence::Absent,
                immobilization: Evidence::Absent,
                non_melee_casting: Evidence::Absent,
                speed_auras: Evidence::Absent,
                combat_scripts: Evidence::Absent,
                target_airborne_or_transport: Evidence::Absent,
            },
        }
    }
    fn state() -> MotionState {
        let mut s = MotionState::new(
            Identity {
                creature: 2,
                map: 0,
                content: Revision([1; 32]),
                geometry: Revision([2; 32]),
            },
            pose(0.),
            0,
            0,
        )
        .unwrap();
        s.initialize_default(DefaultMovement::Wander {
            center: point(0.),
            radius: Real::integer(4),
        })
        .unwrap();
        s
    }
    fn wander(s: &MotionState) -> &WanderSnapshot {
        let Generator::Wander(w) = s.generator() else {
            panic!("Wander required")
        };
        w
    }
    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Prepare,
        Draw(WanderDraw),
        Route,
    }
    struct Host {
        words: VecDeque<u32>,
        calls: Vec<Call>,
        routes: Vec<RouteRequest>,
        fail_prefix: bool,
        fail_route: bool,
        source: RouteSource,
        forge_cache: bool,
    }
    impl Host {
        fn new() -> Self {
            Self {
                words: VecDeque::from([0, 0x8000_0000, 3, 3000, 0x8000_0000, 0x8000_0000]),
                calls: vec![],
                routes: vec![],
                fail_prefix: false,
                fail_route: false,
                source: RouteSource::SyntheticFixture,
                forge_cache: false,
            }
        }
    }
    impl Authority for Host {
        fn wander_draw(&mut self, purpose: WanderDraw) -> Result<u32, MotionError> {
            self.calls.push(Call::Draw(purpose));
            self.words
                .pop_front()
                .ok_or(MotionError::Missing("test draw"))
        }
        fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
            let fact = match query {
                Query::Route(q) if q.kind == RouteKind::WanderPrepare => {
                    q.validate_pathfinder_dispatch()?;
                    self.calls.push(Call::Prepare);
                    if self.fail_prefix {
                        return Err(MotionError::Missing("test prefix"));
                    }
                    Fact::WanderPrepared
                }
                Query::Route(q) => {
                    self.routes.push(q.clone());
                    q.validate_pathfinder_dispatch()?;
                    self.calls.push(Call::Route);
                    if self.fail_route {
                        return Err(MotionError::Missing("test route"));
                    }
                    let path = RouteOutcome::Complete(CompleteRoute {
                        source: self.source,
                        path_identity: [8; 32],
                        points: vec![q.origin, q.goal],
                        segments: vec![SegmentChecks {
                            loaded_coverage: Check::Passed,
                            line_of_sight: Check::Passed,
                            body_sweep: Check::Passed,
                            continuous_ground_support: Check::Passed,
                            dry_ground: Check::Passed,
                        }],
                    });
                    if q.kind != RouteKind::Wander {
                        Fact::Route(path)
                    } else {
                        let mut cache =
                            q.prior_cache
                                .as_deref()
                                .cloned()
                                .unwrap_or(PathFinderCache {
                                    binding: PathFinderCacheBinding {
                                        identity: q.binding.identity,
                                        owner_generation: 1,
                                        inventory: [3; 32],
                                        source_epoch: [4; 32],
                                        navigation_policy: [5; 32],
                                    },
                                    revision: 0,
                                    source: self.source,
                                    poly_refs: vec![10; 74],
                                    active_len: 1,
                                    point_limit: PATHFINDER_CACHE_POINT_LIMIT,
                                    path_type: 1,
                                    requested_end: q.goal,
                                    include_flags: 13,
                                    exclude_flags: 0,
                                });
                        cache.revision += 1;
                        let path = if q
                            .origin
                            .distance_squared(q.goal)?
                            .less(Real::constant(0x3c23_d70a))
                        {
                            cache.requested_end = q
                                .prior_cache
                                .as_ref()
                                .map_or(point(0.), |c| c.requested_end);
                            if q.prior_cache.is_none() {
                                cache.poly_refs = vec![0; 74];
                            }
                            cache.active_len = 0;
                            cache.path_type = 8;
                            RouteOutcome::NoPath
                        } else {
                            cache.requested_end = q.goal;
                            cache.active_len = 1;
                            cache.path_type = 1;
                            cache.poly_refs.push(90 + cache.revision);
                            path
                        };
                        if self.forge_cache {
                            cache.revision += 1;
                        }
                        Fact::RouteCalculation(Box::new(RouteCalculation {
                            wander_no_path: (path == RouteOutcome::NoPath)
                                .then_some(WanderNoPathReason::MinimumDistance),
                            source: self.source,
                            outcome: path,
                            prior_cache: q.prior_cache.clone(),
                            after_cache: cache,
                        }))
                    }
                }
                Query::Bearing(_) => Fact::Bearing(Real::ZERO),
                Query::NearPoint(q) => Fact::NearPoint(q.target.pose.position),
                Query::FacingTarget(_) => Fact::ObjectPosition(None),
                Query::Collider(_) => Fact::Collider(None),
            };
            Ok(Reply {
                request: query.clone(),
                fact,
            })
        }
    }
    #[test]
    fn initialized_owner_launch_arrival_pause_second_leg_and_retained_tail() {
        let mut s = state();
        let mut h = Host::new();
        assert_eq!(s.snapshot().query_id, 1);
        assert!(!s.is_moving());
        assert_eq!(wander(&s).next_move_count, 1);
        assert_eq!(wander(&s).next_move_timer_ms, 0);
        let first = s.advance_default(1, 100, None, input(), &mut h).unwrap();
        assert_eq!(first.movement.request().command_id, 0);
        assert!(s.is_moving());
        assert_eq!(wander(&s).next_move_timer_ms, 3000);
        assert_eq!(
            h.calls,
            vec![
                Call::Prepare,
                Call::Draw(WanderDraw::Angle),
                Call::Draw(WanderDraw::Radius),
                Call::Route,
                Call::Draw(WanderDraw::MoveCount),
                Call::Draw(WanderDraw::Pause)
            ]
        );
        let construction = wander(&s).pathfinder_construction;
        s.advance(500, None, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert_eq!(wander(&s).next_move_timer_ms, 3000);
        assert_ne!(s.pose().position, point(0.));
        let arrived = s.advance(301, None, input(), &mut h).unwrap();
        assert!(arrived.iter().any(|e| matches!(
            e,
            Event::SplineAdvanced {
                event: SplineEvent::Arrived,
                ..
            }
        )));
        assert_eq!(wander(&s).next_move_timer_ms, 2699);
        assert!(!s.is_moving());
        s.advance(2698, None, input(), &mut h).unwrap();
        assert_eq!(wander(&s).next_move_timer_ms, 1);
        s.advance(1, None, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert_eq!(wander(&s).next_move_count, 2);
        assert_eq!(wander(&s).next_move_timer_ms, 0);
        assert_eq!(wander(&s).pathfinder_construction, construction);
        let cache = wander(&s).pathfinder_cache.as_ref().unwrap();
        assert_eq!(cache.revision, 2);
        assert_eq!(&cache.poly_refs[74..], &[91, 92]);
        assert_eq!(h.calls.len(), 10);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    #[test]
    fn chase_interrupt_home_reset_and_death_recreate_preserve_source_lifetimes() {
        let mut s = state();
        let mut h = Host::new();
        s.advance(100, None, input(), &mut h).unwrap();
        s.advance(500, None, input(), &mut h).unwrap();
        let old = wander(&s).clone();
        let target = Target {
            movement_flags: GroundMovementFlags::STOPPED,
            id: 1,
            pose: s.pose(),
            combat_reach: Real::new(1.5).unwrap(),
            bounding_radius: Real::new(0.5).unwrap(),
            collision_height: Real::integer(2),
        };
        let events = s.start_chase(target, false, input(), &mut h).unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::SplineInterrupted { .. })));
        assert!(!s.is_moving());
        assert_eq!(s.snapshot().suspended_wander, Some(old.clone()));
        s.start_home(pose(0.), false, false, input(), &mut h)
            .unwrap();
        s.advance(1000, None, input(), &mut h).unwrap();
        assert_eq!(wander(&s).next_move_count, old.next_move_count);
        assert_eq!(wander(&s).next_move_timer_ms, 0);
        assert!(wander(&s).pathfinder_cache.is_none());
        assert!(wander(&s).normal_route.is_none());
        assert_ne!(
            wander(&s).pathfinder_construction,
            old.pathfinder_construction
        );
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.stop_for_death().unwrap();
        assert!(matches!(s.generator(), Generator::Idle));
        assert!(s.snapshot().suspended_wander.is_none());
        s.advance_without_unit(1000).unwrap();
        s.lifecycle_relocate(pose(0.)).unwrap();
        s.initialize_default(DefaultMovement::Wander {
            center: point(0.),
            radius: Real::integer(4),
        })
        .unwrap();
        assert_eq!(wander(&s).next_move_count, 1);
        assert_eq!(wander(&s).next_move_timer_ms, 0);
        assert!(!s.is_moving());
    }
    #[test]
    fn prefix_failure_precedes_draws_and_late_failure_does_not_publish_owner() {
        let mut s = state();
        let before = s.clone();
        let mut h = Host::new();
        h.fail_prefix = true;
        assert!(s.advance(100, None, input(), &mut h).is_err());
        assert_eq!(s, before);
        assert_eq!(h.calls, vec![Call::Prepare]);
        assert_eq!(h.words.len(), 6);
        let mut h = Host::new();
        h.fail_route = true;
        assert!(s.advance(100, None, input(), &mut h).is_err());
        assert_eq!(s, before);
        assert_eq!(
            h.calls,
            vec![
                Call::Prepare,
                Call::Draw(WanderDraw::Angle),
                Call::Draw(WanderDraw::Radius),
                Call::Route
            ]
        );
    }
    #[test]
    fn real_near_zero_failure_retains_cold_cache_and_retries_after_500ms() {
        let mut s = state();
        let mut h = Host::new();
        h.words = VecDeque::from([0, 0, 0, 0x8000_0000, 3, 3000]);
        s.advance(100, None, input(), &mut h).unwrap();
        assert!(!s.is_moving());
        assert_eq!(wander(&s).next_move_timer_ms, 500);
        assert_eq!(wander(&s).next_move_count, 1);
        assert_eq!(
            h.calls,
            vec![
                Call::Prepare,
                Call::Draw(WanderDraw::Angle),
                Call::Draw(WanderDraw::Radius),
                Call::Route
            ]
        );
        let cache = wander(&s).pathfinder_cache.as_ref().unwrap();
        assert_eq!(cache.active_len, 0);
        assert_eq!(cache.path_type, 8);
        assert_eq!(cache.requested_end, point(0.));
        assert_eq!(cache.poly_refs, vec![0; 74]);
        let failed = wander(&s).failed_route.as_ref().unwrap();
        assert_eq!(failed.request, h.routes[0]);
        assert_eq!(failed.reason, WanderNoPathReason::MinimumDistance);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.advance(499, None, input(), &mut h).unwrap();
        assert_eq!(h.calls.len(), 4);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.advance(1, None, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 2);
        assert!(wander(&s).failed_route.is_none());
        assert_eq!(wander(&s).next_move_count, 3);
        assert_eq!(wander(&s).next_move_timer_ms, 3000);
    }
    #[test]
    fn source_no_path_reasons_bind_raw_end_retained_end_and_endpoint_cache_append() {
        let mut s = state();
        let mut h = Host::new();
        h.fail_route = true;
        assert!(s.advance(100, None, input(), &mut h).is_err());
        let request = h.routes.last().unwrap().clone();
        let cache = PathFinderCache {
            binding: PathFinderCacheBinding {
                identity: request.binding.identity,
                owner_generation: 1,
                inventory: [3; 32],
                source_epoch: [4; 32],
                navigation_policy: [5; 32],
            },
            revision: 1,
            source: RouteSource::OwnerPathFinder,
            poly_refs: vec![0; 74],
            active_len: 0,
            point_limit: PATHFINDER_CACHE_POINT_LIMIT,
            path_type: 8,
            requested_end: request.goal,
            include_flags: 13,
            exclude_flags: 0,
        };
        let missing = RouteCalculation {
            wander_no_path: Some(WanderNoPathReason::EndpointMissing),
            source: RouteSource::OwnerPathFinder,
            outcome: RouteOutcome::NoPath,
            prior_cache: None,
            after_cache: cache,
        };
        missing.validate_for(&request).unwrap();
        let mut height = missing.clone();
        height.wander_no_path = Some(WanderNoPathReason::EndpointHeight);
        // A cold zero vector cannot supply an existing endpoint polygon.
        assert!(height.validate_for(&request).is_err());
        height.after_cache.poly_refs.push(42);
        height.after_cache.requested_end.z = Real::ONE;
        height.validate_for(&request).unwrap();
        for case in 0..5 {
            let mut forged = height.clone();
            match case {
                0 => forged.wander_no_path = None,
                1 => forged.wander_no_path = Some(WanderNoPathReason::MinimumDistance),
                2 => forged.after_cache.requested_end.x = Real::integer(9),
                3 => forged.after_cache.poly_refs[0] = 42,
                _ => forged.after_cache.poly_refs.push(43),
            }
            assert!(forged.validate_for(&request).is_err(), "case {case}");
        }
    }
    fn failed_source_state() -> (MotionState, Host) {
        let mut s = state();
        let mut h = Host::new();
        h.source = RouteSource::OwnerPathFinder;
        h.words = VecDeque::from([0, 0]);
        s.advance(100, None, input(), &mut h).unwrap();
        (s, h)
    }
    fn saved_wander_mut(snapshot: &mut MotionSnapshot) -> &mut WanderSnapshot {
        let Generator::Wander(wander) = &mut snapshot.generator else {
            panic!("Wander required")
        };
        wander
    }
    #[test]
    fn source_failed_head_survives_pause_repeated_failure_and_serialized_retry() {
        let (mut s, mut h) = failed_source_state();
        let first = wander(&s).failed_route.clone().unwrap();
        assert_eq!(first.request, h.routes[0]);
        assert_eq!(first.reason, WanderNoPathReason::MinimumDistance);
        assert!(s.snapshot().spline.is_none());
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 1);
        s.advance(499, None, input(), &mut h).unwrap();
        assert_eq!(wander(&s).failed_route.as_deref(), Some(first.as_ref()));
        assert_eq!(wander(&s).next_move_timer_ms, 1);
        s.face_target(point(10.), &mut h).unwrap();
        assert!(s.snapshot().query_id > first.request.binding.query_id);
        assert_eq!(wander(&s).failed_route.as_deref(), Some(first.as_ref()));
        let encoded = serde_json::to_vec(&s.snapshot()).unwrap();
        let restored = serde_json::from_slice(&encoded).unwrap();
        s = MotionState::restore(restored).unwrap();
        h.words = VecDeque::from([0, 0]);
        s.advance(1, None, input(), &mut h).unwrap();
        let second = wander(&s).failed_route.clone().unwrap();
        assert_eq!(second.request, h.routes[1]);
        assert!(second.request.binding.query_id > first.request.binding.query_id);
        assert_eq!(second.request.prior_cache.as_ref().unwrap().revision, 1);
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 2);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut stale = s.snapshot();
        saved_wander_mut(&mut stale).failed_route = Some(first);
        assert!(MotionState::restore(stale).is_err());
        h.words = VecDeque::from([0, 0x8000_0000, 3, 3000]);
        s.advance(500, None, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert!(wander(&s).failed_route.is_none());
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 3);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut stale = s.snapshot();
        saved_wander_mut(&mut stale).failed_route = Some(second);
        assert!(MotionState::restore(stale).is_err());
    }
    #[test]
    fn source_failed_head_retains_previous_finalized_spline_and_exact_prior_cache() {
        let mut s = state();
        let mut h = Host::new();
        h.source = RouteSource::OwnerPathFinder;
        h.words = VecDeque::from([0, 0x8000_0000, 3, 3000, 0, 0x8000_0000]);
        s.advance(100, None, input(), &mut h).unwrap();
        s.advance(801, None, input(), &mut h).unwrap();
        assert!(!s.is_moving());
        let previous_spline = s.snapshot().spline;
        let previous_cache = wander(&s).pathfinder_cache.clone();
        let previous_normal = wander(&s).normal_route.clone();
        assert!(previous_normal.is_some());
        s.advance(2199, None, input(), &mut h).unwrap();
        assert!(wander(&s).normal_route.is_none());
        let mut forged = s.snapshot();
        saved_wander_mut(&mut forged).normal_route = previous_normal;
        assert!(MotionState::restore(forged).is_err());
        assert_eq!(s.snapshot().spline, previous_spline);
        let failed = wander(&s).failed_route.as_ref().unwrap();
        assert_eq!(failed.request.prior_cache, previous_cache);
        assert_eq!(failed.request, h.routes[1]);
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 2);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut forged = s.snapshot();
        forged.spline.as_mut().unwrap().spline.finalized = false;
        assert!(MotionState::restore(forged).is_err());
    }
    #[test]
    fn failed_head_cache_and_retry_timer_roll_back_on_missing_invalid_and_late_failure() {
        let (mut s, _) = failed_source_state();
        let before = s.clone();
        for failure in 0..3 {
            let mut h = Host::new();
            h.source = RouteSource::OwnerPathFinder;
            h.words = VecDeque::from([0, 0x8000_0000, 0, 3000]);
            h.fail_route = failure == 0;
            h.forge_cache = failure == 1;
            // Case 2 validates and launches the route, then rejects its count draw.
            assert!(s.advance(500, None, input(), &mut h).is_err());
            assert_eq!(s, before, "rollback case {failure}");
        }
        let mut h = Host::new();
        h.source = RouteSource::OwnerPathFinder;
        s.advance(500, None, input(), &mut h).unwrap();
        assert!(wander(&s).failed_route.is_none());
    }
    #[test]
    fn failed_head_is_suspended_unchanged_and_cleared_by_default_reset() {
        let (mut s, _) = failed_source_state();
        let failed = wander(&s).clone();
        let mut h = Host::new();
        let target = Target {
            movement_flags: GroundMovementFlags::STOPPED,
            id: 1,
            pose: pose(20.),
            combat_reach: Real::new(1.5).unwrap(),
            bounding_radius: Real::new(0.5).unwrap(),
            collision_height: Real::integer(2),
        };
        s.start_chase(target, false, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert!(
            s.snapshot()
                .spline
                .as_ref()
                .unwrap()
                .route_request
                .binding
                .query_id
                > failed
                    .failed_route
                    .as_ref()
                    .unwrap()
                    .request
                    .binding
                    .query_id
        );
        assert_eq!(s.snapshot().suspended_wander, Some(failed));
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.reset_default().unwrap();
        assert!(wander(&s).failed_route.is_none());
        assert!(wander(&s).pathfinder_cache.is_none());
        assert_eq!(wander(&s).next_move_timer_ms, 0);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    #[test]
    fn failed_head_rejects_forged_request_reason_cache_and_owner_bindings() {
        let (s, _) = failed_source_state();
        for case in 0..29 {
            let mut snapshot = s.snapshot();
            let w = saved_wander_mut(&mut snapshot);
            match case {
                0 => w.failed_route = None,
                1 => w.pathfinder_cache = None,
                2 => w.failed_route.as_mut().unwrap().reason = WanderNoPathReason::EndpointMissing,
                3 => w.failed_route.as_mut().unwrap().request.kind = RouteKind::ChaseInitial,
                4 => {
                    w.failed_route
                        .as_mut()
                        .unwrap()
                        .request
                        .binding
                        .identity
                        .creature += 1
                }
                5 => w.failed_route.as_mut().unwrap().request.binding.query_id += 1,
                6 => w.failed_route.as_mut().unwrap().request.binding.now_ms += 1,
                7 => w.failed_route.as_mut().unwrap().request.wander = None,
                8 => {
                    w.failed_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .center = point(1.)
                }
                9 => {
                    w.failed_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .radius = Real::integer(5)
                }
                10 => {
                    w.failed_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .angle_draw = Real::ONE
                }
                11 => {
                    w.failed_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .prepared
                        .query_id -= 1
                }
                12 => w.failed_route.as_mut().unwrap().request.pathfinder_dispatch = None,
                13 => {
                    let q = &mut w.failed_route.as_mut().unwrap().request;
                    q.pathfinder_dispatch = Some(PathfinderDispatch::home(q.binding).unwrap());
                }
                14 => w.pathfinder_construction.query_id += 1,
                15 => w.pathfinder_cache.as_mut().unwrap().revision += 1,
                16 => w.pathfinder_cache.as_mut().unwrap().poly_refs[0] = 42,
                17 => w.pathfinder_cache.as_mut().unwrap().requested_end = point(1.),
                18 => w.pathfinder_cache.as_mut().unwrap().active_len = 1,
                19 => w.next_move_timer_ms = 0,
                20 => w.next_move_timer_ms = 501,
                21 => w.failed_route.as_mut().unwrap().request.origin = point(0.01),
                22 => w.failed_route.as_mut().unwrap().request.launch_origin = point(0.01),
                23 => {
                    w.failed_route
                        .as_mut()
                        .unwrap()
                        .request
                        .owner_collision_height = Real::new(-1.).unwrap()
                }
                24 => w.failed_route.as_mut().unwrap().request.point_limit -= 1,
                25 => {
                    w.failed_route.as_mut().unwrap().request.prior_cache =
                        w.pathfinder_cache.clone()
                }
                26 => {
                    w.pathfinder_construction.query_id += 1;
                    let q = &mut w.failed_route.as_mut().unwrap().request;
                    q.pathfinder_dispatch = Some(
                        PathfinderDispatch::wander(w.pathfinder_construction, q.binding, false)
                            .unwrap(),
                    );
                }
                27 => {
                    let q = &mut w.failed_route.as_mut().unwrap().request;
                    q.binding.now_ms -= 1;
                    q.wander.as_mut().unwrap().prepared.now_ms -= 1;
                    q.pathfinder_dispatch = Some(
                        PathfinderDispatch::wander(w.pathfinder_construction, q.binding, false)
                            .unwrap(),
                    );
                }
                _ => {
                    let q = &mut w.failed_route.as_mut().unwrap().request;
                    q.pathfinder_dispatch = Some(
                        PathfinderDispatch::wander(w.pathfinder_construction, q.binding, true)
                            .unwrap(),
                    );
                }
            }
            assert!(
                MotionState::restore(snapshot).is_err(),
                "forgery case {case}"
            );
        }
    }
    #[test]
    fn failed_head_is_required_in_json_even_when_null_and_rejects_unknown_fields() {
        let (failed, _) = failed_source_state();
        for s in [state(), failed] {
            let value = serde_json::to_value(s.snapshot()).unwrap();
            let decoded = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(MotionState::restore(decoded).unwrap(), s);
            let mut omitted = value.clone();
            omitted["generator"]["Wander"]
                .as_object_mut()
                .unwrap()
                .remove("failed_route");
            assert!(serde_json::from_value::<MotionSnapshot>(omitted).is_err());
            let mut old = s.snapshot();
            old.policy = "cmangos-8ec338a1-ground-linear-home-wander-v5".into();
            assert!(MotionState::restore(old).is_err());
            if wander(&s).failed_route.is_some() {
                let mut unknown = value.clone();
                unknown["generator"]["Wander"]["failed_route"]["authority"] =
                    serde_json::json!(true);
                assert!(serde_json::from_value::<MotionSnapshot>(unknown).is_err());
                let mut null = value;
                null["generator"]["Wander"]["failed_route"] = serde_json::Value::Null;
                let decoded = serde_json::from_value(null).unwrap();
                assert!(MotionState::restore(decoded).is_err());
            }
        }
    }
    #[test]
    fn typed_endpoint_failure_heads_restore_with_their_exact_cache_transition() {
        struct EndpointHost {
            inner: Host,
            reason: WanderNoPathReason,
        }
        impl Authority for EndpointHost {
            fn wander_draw(&mut self, purpose: WanderDraw) -> Result<u32, MotionError> {
                self.inner.wander_draw(purpose)
            }
            fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
                let mut reply = self.inner.resolve(query)?;
                if let (Query::Route(request), Fact::RouteCalculation(calculation)) =
                    (query, &mut reply.fact)
                {
                    if request.kind == RouteKind::Wander {
                        calculation.wander_no_path = Some(self.reason);
                        calculation.outcome = RouteOutcome::NoPath;
                        calculation.after_cache.path_type = 8;
                        calculation.after_cache.active_len = 0;
                        calculation.after_cache.poly_refs = vec![0; 74];
                        if self.reason == WanderNoPathReason::EndpointHeight {
                            calculation.after_cache.poly_refs.push(42);
                            calculation.after_cache.requested_end.z = Real::ONE;
                        }
                    }
                }
                Ok(reply)
            }
        }
        for reason in [
            WanderNoPathReason::EndpointMissing,
            WanderNoPathReason::EndpointHeight,
        ] {
            let mut s = state();
            let mut inner = Host::new();
            inner.source = RouteSource::OwnerPathFinder;
            let mut h = EndpointHost { inner, reason };
            s.advance(100, None, input(), &mut h).unwrap();
            let head = wander(&s).failed_route.as_ref().unwrap();
            assert_eq!(head.reason, reason);
            assert_eq!(head.request, h.inner.routes[0]);
            assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        }
    }
    #[test]
    fn legacy_cache_free_no_path_does_not_manufacture_a_native_failed_head() {
        struct LegacyHost(Host);
        impl Authority for LegacyHost {
            fn wander_draw(&mut self, purpose: WanderDraw) -> Result<u32, MotionError> {
                self.0.wander_draw(purpose)
            }
            fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
                if matches!(query, Query::Route(request) if request.kind == RouteKind::Wander) {
                    return Ok(Reply {
                        request: query.clone(),
                        fact: Fact::Route(RouteOutcome::NoPath),
                    });
                }
                self.0.resolve(query)
            }
        }
        let mut s = state();
        let mut h = LegacyHost(Host::new());
        s.advance(100, None, input(), &mut h).unwrap();
        assert!(wander(&s).pathfinder_cache.is_none());
        assert!(wander(&s).failed_route.is_none());
        assert_eq!(wander(&s).next_move_timer_ms, 500);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    fn normal_source_state() -> (MotionState, Host) {
        let mut s = state();
        let mut h = Host::new();
        h.source = RouteSource::OwnerPathFinder;
        s.advance(100, None, input(), &mut h).unwrap();
        (s, h)
    }
    fn target_at(pose: Pose) -> Target {
        Target {
            movement_flags: GroundMovementFlags::STOPPED,
            id: 1,
            pose,
            combat_reach: Real::new(1.5).unwrap(),
            bounding_radius: Real::new(0.5).unwrap(),
            collision_height: Real::integer(2),
        }
    }
    #[test]
    fn normal_head_binds_accepted_calculation_and_survives_arrival_pause_and_later_queries() {
        let (mut s, mut h) = normal_source_state();
        let first = wander(&s).normal_route.clone().unwrap();
        assert_eq!(first.request, h.routes[0]);
        assert_eq!(first.route, s.snapshot().spline.unwrap().route);
        assert_eq!(first.route.source, RouteSource::OwnerPathFinder);
        assert!(wander(&s).failed_route.is_none());
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.advance(500, None, input(), &mut h).unwrap();
        assert_eq!(wander(&s).normal_route.as_ref(), Some(&first));
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.advance(301, None, input(), &mut h).unwrap();
        assert!(!s.is_moving());
        s.face_target(point(10.), &mut h).unwrap();
        assert!(s.snapshot().query_id > first.request.binding.query_id);
        s.advance(2698, None, input(), &mut h).unwrap();
        assert_eq!(wander(&s).normal_route.as_ref(), Some(&first));
        let encoded = serde_json::to_vec(&s.snapshot()).unwrap();
        s = MotionState::restore(serde_json::from_slice(&encoded).unwrap()).unwrap();
        s.advance(1, None, input(), &mut h).unwrap();
        let second = wander(&s).normal_route.as_ref().unwrap();
        assert_eq!(second.request, h.routes[1]);
        assert_ne!(second.request, first.request);
        assert_eq!(second.request.prior_cache.as_ref().unwrap().revision, 1);
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 2);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut stale = s.snapshot();
        saved_wander_mut(&mut stale).normal_route = Some(first);
        assert!(MotionState::restore(stale).is_err());
    }
    #[test]
    fn normal_head_survives_interruption_chase_and_home_then_reset_and_death_remove_it() {
        let (mut s, mut h) = normal_source_state();
        s.advance(500, None, input(), &mut h).unwrap();
        let original = wander(&s).clone();
        assert!(original.normal_route.is_some());
        let target = target_at(s.pose());
        s.start_chase(target, false, input(), &mut h).unwrap();
        assert!(s.snapshot().spline.is_none());
        assert_eq!(s.snapshot().suspended_wander.as_ref(), Some(&original));
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let encoded = serde_json::to_vec(&s.snapshot()).unwrap();
        s = MotionState::restore(serde_json::from_slice(&encoded).unwrap()).unwrap();
        // Replace the global spline with Chase, then the separate Home owner.
        h.source = RouteSource::SyntheticFixture;
        s.advance(100, Some(target_at(pose(20.))), input(), &mut h)
            .unwrap();
        assert!(s.is_moving());
        assert_eq!(s.snapshot().suspended_wander.as_ref(), Some(&original));
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.start_home(pose(0.), false, false, input(), &mut h)
            .unwrap();
        assert_eq!(s.snapshot().suspended_wander.as_ref(), Some(&original));
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut dead = s.clone();
        dead.stop_for_death().unwrap();
        assert!(dead.snapshot().suspended_wander.is_none());
        assert!(dead.snapshot().spline.is_none());
        assert!(matches!(dead.generator(), Generator::Idle));
        assert_eq!(MotionState::restore(dead.snapshot()).unwrap(), dead);
        s.advance(1000, None, input(), &mut h).unwrap();
        assert!(wander(&s).normal_route.is_none());
        assert!(wander(&s).failed_route.is_none());
        assert!(wander(&s).pathfinder_cache.is_none());
        assert_ne!(
            wander(&s).pathfinder_construction,
            original.pathfinder_construction
        );
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    #[test]
    fn normal_head_and_cache_roll_back_before_and_after_valid_successful_dispatch() {
        let (mut s, mut h) = normal_source_state();
        s.advance(801, None, input(), &mut h).unwrap();
        // First count=1 ensures the next accepted path reaches the late draw boundary.
        if let Generator::Wander(w) = &mut s.state.generator {
            w.next_move_count = 1;
        }
        let before = s.clone();
        for failure in 0..4 {
            let mut h = Host::new();
            h.source = RouteSource::OwnerPathFinder;
            h.words = VecDeque::from([0x8000_0000, 0x8000_0000, 0, 3000]);
            h.fail_prefix = failure == 0;
            h.fail_route = failure == 1;
            h.forge_cache = failure == 2;
            // Case 3 accepts the complete calculation and launches, then rejects count=0.
            assert!(s.advance(2199, None, input(), &mut h).is_err());
            assert_eq!(s, before, "normal rollback case {failure}");
        }
        h.words = VecDeque::from([0x8000_0000, 0x8000_0000, 3, 3000]);
        s.advance(2199, None, input(), &mut h).unwrap();
        assert_ne!(wander(&s).normal_route, wander(&before).normal_route);
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    #[test]
    fn normal_head_rejects_request_route_cache_constructor_and_chronology_mutations() {
        let (s, _) = normal_source_state();
        for case in 0..37 {
            let mut snapshot = s.snapshot();
            let w = saved_wander_mut(&mut snapshot);
            match case {
                0 => w.normal_route = None,
                1 => w.pathfinder_cache = None,
                2 => w.normal_route.as_mut().unwrap().request.kind = RouteKind::ChaseInitial,
                3 => w.normal_route.as_mut().unwrap().request.wander = None,
                4 => {
                    w.normal_route
                        .as_mut()
                        .unwrap()
                        .request
                        .binding
                        .identity
                        .creature += 1
                }
                5 => w.normal_route.as_mut().unwrap().request.binding.query_id += 1,
                6 => w.normal_route.as_mut().unwrap().request.binding.now_ms += 1,
                7 => w.normal_route.as_mut().unwrap().request.origin = point(1.),
                8 => w.normal_route.as_mut().unwrap().request.launch_origin = point(1.),
                9 => w.normal_route.as_mut().unwrap().request.goal = point(1.),
                10 => {
                    w.normal_route
                        .as_mut()
                        .unwrap()
                        .request
                        .owner_collision_height = Real::new(-1.).unwrap()
                }
                11 => w.normal_route.as_mut().unwrap().request.point_limit -= 1,
                12 => w.normal_route.as_mut().unwrap().request.pathfinder_dispatch = None,
                13 => {
                    w.normal_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .center = point(1.)
                }
                14 => {
                    w.normal_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .radius = Real::integer(5)
                }
                15 => {
                    w.normal_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .angle_draw = Real::ONE
                }
                16 => {
                    w.normal_route
                        .as_mut()
                        .unwrap()
                        .request
                        .wander
                        .as_mut()
                        .unwrap()
                        .prepared
                        .query_id -= 1
                }
                17 => w.pathfinder_construction.query_id += 1,
                18 => w.pathfinder_cache.as_mut().unwrap().revision += 1,
                19 => w.pathfinder_cache.as_mut().unwrap().active_len = 0,
                20 => w.pathfinder_cache.as_mut().unwrap().requested_end = point(1.),
                21 => w.pathfinder_cache.as_mut().unwrap().path_type = 9,
                22 => w.pathfinder_cache.as_mut().unwrap().source = RouteSource::SyntheticFixture,
                23 => w.normal_route.as_mut().unwrap().route.path_identity = [0; 32],
                24 => w.normal_route.as_mut().unwrap().route.points[0] = point(1.),
                25 => w.normal_route.as_mut().unwrap().route.points[1].z = Real::ONE,
                26 => {
                    w.normal_route.as_mut().unwrap().route.segments[0].line_of_sight =
                        Check::Unknown
                }
                27 => w.normal_route.as_mut().unwrap().route.segments.clear(),
                28 => {
                    w.normal_route.as_mut().unwrap().request.prior_cache =
                        w.pathfinder_cache.clone()
                }
                29 => {
                    let q = &mut w.normal_route.as_mut().unwrap().request;
                    q.pathfinder_dispatch = Some(PathfinderDispatch::home(q.binding).unwrap());
                }
                30 => {
                    let q = &mut w.normal_route.as_mut().unwrap().request;
                    q.pathfinder_dispatch = Some(
                        PathfinderDispatch::wander(w.pathfinder_construction, q.binding, true)
                            .unwrap(),
                    );
                }
                31 => {
                    w.pathfinder_construction.query_id += 1;
                    let q = &mut w.normal_route.as_mut().unwrap().request;
                    q.pathfinder_dispatch = Some(
                        PathfinderDispatch::wander(w.pathfinder_construction, q.binding, false)
                            .unwrap(),
                    );
                }
                32 => w.normal_route.as_mut().unwrap().route.path_identity = [9; 32],
                33 => {
                    let q = &mut w.normal_route.as_mut().unwrap().request;
                    q.binding.now_ms -= 1;
                    q.wander.as_mut().unwrap().prepared.now_ms -= 1;
                    q.pathfinder_dispatch = Some(
                        PathfinderDispatch::wander(w.pathfinder_construction, q.binding, false)
                            .unwrap(),
                    );
                    snapshot.spline.as_mut().unwrap().route_request = q.clone();
                }
                34 => snapshot.processed_map_clock_ms -= 1,
                35 => snapshot.spline = None,
                _ => w.normal_route.as_mut().unwrap().request.binding.query_id = 0,
            }
            assert!(
                MotionState::restore(snapshot).is_err(),
                "normal forgery case {case}"
            );
        }
    }
    #[test]
    fn normal_head_is_required_nullable_json_and_does_not_admit_unsupported_cache_shapes() {
        let (normal, _) = normal_source_state();
        let (failed, _) = failed_source_state();
        for s in [state(), normal.clone(), failed] {
            let value = serde_json::to_value(s.snapshot()).unwrap();
            let decoded = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(MotionState::restore(decoded).unwrap(), s);
            let mut omitted = value.clone();
            omitted["generator"]["Wander"]
                .as_object_mut()
                .unwrap()
                .remove("normal_route");
            assert!(serde_json::from_value::<MotionSnapshot>(omitted).is_err());
            let mut old = s.snapshot();
            old.policy = "cmangos-8ec338a1-ground-linear-home-wander-v6".into();
            assert!(MotionState::restore(old).is_err());
            if wander(&s).normal_route.is_some() {
                let mut unknown = value.clone();
                unknown["generator"]["Wander"]["normal_route"]["authority"] =
                    serde_json::json!(true);
                assert!(serde_json::from_value::<MotionSnapshot>(unknown).is_err());
                let mut null = value;
                null["generator"]["Wander"]["normal_route"] = serde_json::Value::Null;
                let decoded = serde_json::from_value(null).unwrap();
                assert!(MotionState::restore(decoded).is_err());
            }
        }
        let mut head_only = state().snapshot();
        head_only.query_id = normal.snapshot().query_id;
        head_only.now_ms = normal.now_ms();
        head_only.processed_map_clock_ms = normal.now_ms();
        saved_wander_mut(&mut head_only).normal_route = wander(&normal).normal_route.clone();
        assert!(MotionState::restore(head_only).is_err());
        for kind in [0, 2, 3, 4, 9, 16, 32] {
            let mut unsupported = normal.snapshot();
            unsupported.spline = None;
            let w = saved_wander_mut(&mut unsupported);
            w.normal_route = None;
            w.pathfinder_cache.as_mut().unwrap().path_type = kind;
            assert!(
                MotionState::restore(unsupported).is_err(),
                "unsupported type {kind}"
            );
        }
    }
    #[test]
    fn finalized_wander_spline_under_no_route_chase_uses_its_suspended_owner() {
        for phase in 0..3 {
            let (mut s, mut h) = normal_source_state();
            s.advance(801, None, input(), &mut h).unwrap();
            if phase == 1 {
                // Warm NORMAL has a prior cache in its retained spline request.
                if let Generator::Wander(w) = &mut s.state.generator {
                    w.next_move_count = 1;
                }
                h.words = VecDeque::from([0x8000_0000, 0x8000_0000, 3, 3000]);
                s.advance(2199, None, input(), &mut h).unwrap();
                s.advance(1601, None, input(), &mut h).unwrap();
            } else if phase == 2 {
                // NoPath retains the previous finalized spline with its older request.
                h.words = VecDeque::from([0, 0x8000_0000]);
                s.advance(2199, None, input(), &mut h).unwrap();
                assert!(wander(&s).failed_route.is_some());
            }
            assert!(!s.is_moving());
            let old_spline = s.snapshot().spline.unwrap();
            let old_wander = wander(&s).clone();
            let route_count = h.routes.len();
            s.start_chase(target_at(s.pose()), false, input(), &mut h)
                .unwrap();
            assert_eq!(h.routes.len(), route_count);
            assert_eq!(s.snapshot().spline.as_ref(), Some(&old_spline));
            assert_eq!(s.snapshot().suspended_wander.as_ref(), Some(&old_wander));
            let Generator::Chase(chase) = s.generator() else {
                panic!("Chase required")
            };
            assert!(chase.pathfinder_cache.is_none());
            assert!(chase.pathfinder_construction.is_none());
            assert_eq!(
                MotionState::restore(s.snapshot()).unwrap(),
                s,
                "phase {phase}"
            );
            let decoded =
                serde_json::from_slice(&serde_json::to_vec(&s.snapshot()).unwrap()).unwrap();
            assert_eq!(MotionState::restore(decoded).unwrap(), s);
            for case in 0..4 {
                let mut forged = s.snapshot();
                match case {
                    0 => forged.suspended_wander = None,
                    1 => {
                        let w = forged.suspended_wander.as_mut().unwrap();
                        w.pathfinder_cache = None;
                        w.normal_route = None;
                        w.failed_route = None;
                    }
                    2 => {
                        forged
                            .suspended_wander
                            .as_mut()
                            .unwrap()
                            .pathfinder_cache
                            .as_mut()
                            .unwrap()
                            .source = RouteSource::SyntheticFixture
                    }
                    _ => {
                        forged
                            .suspended_wander
                            .as_mut()
                            .unwrap()
                            .pathfinder_construction
                            .query_id += 1
                    }
                }
                assert!(
                    MotionState::restore(forged).is_err(),
                    "phase {phase}, wrong owner {case}"
                );
            }
            s.begin_map_frame(s.now_ms() + 100).unwrap();
            assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        }
    }
    #[test]
    fn suspended_finalized_normal_head_cannot_claim_completion_before_launch_duration() {
        let (mut s, mut h) = normal_source_state();
        s.advance(801, None, input(), &mut h).unwrap();
        s.start_chase(target_at(s.pose()), false, input(), &mut h)
            .unwrap();
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut forged = s.snapshot();
        let too_recent = forged.processed_map_clock_ms - 1;
        let wander = forged.suspended_wander.as_mut().unwrap();
        let normal = wander.normal_route.as_mut().unwrap();
        normal.request.binding.now_ms = too_recent;
        normal.request.wander.as_mut().unwrap().prepared.now_ms = too_recent;
        normal.request.pathfinder_dispatch = Some(
            PathfinderDispatch::wander(
                wander.pathfinder_construction,
                normal.request.binding,
                false,
            )
            .unwrap(),
        );
        let spline = forged.spline.as_mut().unwrap();
        spline.route_request = normal.request.clone();
        // Exact request/route equality and spline-local timing alone remain valid.
        assert_eq!(normal.route, spline.route);
        assert!(LinearSpline::restore(spline.spline.clone()).is_ok());
        assert!(MotionState::restore(forged).is_err());
    }
    #[test]
    fn normal_head_uses_processed_unit_clock_and_frozen_head_survives_open_map_frame() {
        let (mut s, mut h) = normal_source_state();
        s.begin_map_frame(200).unwrap();
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        let mut forged = s.snapshot();
        let w = saved_wander_mut(&mut forged);
        let q = &mut w.normal_route.as_mut().unwrap().request;
        q.binding.now_ms = 200;
        q.wander.as_mut().unwrap().prepared.now_ms = 200;
        q.pathfinder_dispatch =
            Some(PathfinderDispatch::wander(w.pathfinder_construction, q.binding, false).unwrap());
        forged.spline.as_mut().unwrap().route_request = q.clone();
        assert!(MotionState::restore(forged).is_err());
        s.start_chase(target_at(s.pose()), false, input(), &mut h)
            .unwrap();
        assert!(s.snapshot().spline.is_none());
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    #[test]
    fn legacy_cache_free_success_does_not_manufacture_normal_head() {
        struct LegacyHost(Host);
        impl Authority for LegacyHost {
            fn wander_draw(&mut self, purpose: WanderDraw) -> Result<u32, MotionError> {
                self.0.wander_draw(purpose)
            }
            fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
                let mut reply = self.0.resolve(query)?;
                if let Fact::RouteCalculation(calculation) = reply.fact {
                    reply.fact = Fact::Route(calculation.outcome);
                }
                Ok(reply)
            }
        }
        let mut s = state();
        let mut h = LegacyHost(Host::new());
        s.advance(100, None, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert!(wander(&s).pathfinder_cache.is_none());
        assert!(wander(&s).normal_route.is_none());
        assert!(wander(&s).failed_route.is_none());
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    }
    #[test]
    fn full32_normalized_draw_is_half_open_with_source_rounding() {
        for (word, bits) in [
            (0, 0),
            (1, 0x2f80_0000),
            (0x8000_0000, 0x3f00_0000),
            (u32::MAX, 0x3f7f_ffff),
        ] {
            assert_eq!(wander_normalized_word(word).unwrap().bits(), bits);
        }
    }
}
