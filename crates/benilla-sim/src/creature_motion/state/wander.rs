// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 RandomMovementGenerator.cpp:27-148,177-203; upstream GPL-2.0-or-later.
use super::*;

pub const WANDER_POLICY_ID: &str = "cmangos-8ec338a1-default-wander-ground-walk-v1";
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WanderSnapshot {
    pub center: Point,
    pub radius: Real,
    pub next_move_timer_ms: u32,
    pub next_move_count: u32,
    pub pathfinder_cache: Option<Box<PathFinderCache>>,
    pub pathfinder_construction: QueryBinding,
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
            if cache.revision > state.query_id - construction.query_id {
                return Err(MotionError::Invalid("Wander cache construction sequence"));
            }
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
        if result == Dispatch::Launched {
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
            wander.next_move_timer_ms = 500;
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
    }
    impl Host {
        fn new() -> Self {
            Self {
                words: VecDeque::from([0, 0x8000_0000, 3, 3000, 0x8000_0000, 0x8000_0000]),
                calls: vec![],
                routes: vec![],
                fail_prefix: false,
                fail_route: false,
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
                        source: RouteSource::SyntheticFixture,
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
                                    source: RouteSource::SyntheticFixture,
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
                        Fact::RouteCalculation(Box::new(RouteCalculation {
                            wander_no_path: (path == RouteOutcome::NoPath)
                                .then_some(WanderNoPathReason::MinimumDistance),
                            source: RouteSource::SyntheticFixture,
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
        assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
        s.advance(499, None, input(), &mut h).unwrap();
        assert_eq!(h.calls.len(), 4);
        s.advance(1, None, input(), &mut h).unwrap();
        assert!(s.is_moving());
        assert_eq!(wander(&s).pathfinder_cache.as_ref().unwrap().revision, 2);
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
