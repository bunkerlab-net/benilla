// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual authored products through the production readers and selection loop.
//! Scalar source oracles are independent expectations, never geometry answers.
use super::super::{
    core::{self, Body, NonRenderParticipants, PlayerMotion, PrimaryNearPointPolicy},
    products,
    source_near_point::SourceNearPointProvider,
};
use super::*;
use benilla_sim::creature_ai::{self as ai, Point};
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as fixture, Authority, AuthorityReply,
    NativeError, NativeWorld,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn mid() -> f32 {
    0.5f32 * (64.0 * 533.333_3)
}
fn loaded() -> (MapHeightSnapshot<'static>, SourceNearPointConfiguration) {
    let (products, height) = products::authored_ground_height();
    let proof = NonRenderParticipants::authored_fixture(height.content, height.map);
    let snapshot = Arc::new(products)
        .load_map_height_owned(height, Some(&proof))
        .unwrap();
    (
        snapshot,
        SourceNearPointConfiguration {
            height,
            detect_position_collision: true,
            vmap_line_of_sight_enabled: true,
        },
    )
}
fn setup_selected(
    snapshot: &MapHeightSnapshot<'_>,
    config: SourceNearPointConfiguration,
    empty: bool,
) -> (NativeWorld, Session, AuthorityRequest) {
    setup_selected_with_player_height(snapshot, config, empty, 2.)
}
fn setup_selected_with_player_height(
    snapshot: &MapHeightSnapshot<'_>,
    config: SourceNearPointConfiguration,
    empty: bool,
    player_height: f32,
) -> (NativeWorld, Session, AuthorityRequest) {
    let b = snapshot.binding();
    let mut profile = fixture::profile();
    profile
        .source_motion
        .as_mut()
        .unwrap()
        .player_collision_height = ai::Real::new(player_height).unwrap();
    profile.map_id = b.map;
    profile.content_digest = b.content;
    profile.geometry_revision = if empty {
        geometry_revision(snapshot, config).unwrap()
    } else {
        source_near_point::geometry_revision(snapshot, config).unwrap()
    };
    profile.npcs[0].ai.map_id = b.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(b.content);
    profile.player.start = Point::new(mid() - 50., mid() - 1., 10.).unwrap();
    profile.npcs[0].ai.spawn = Point::new(mid() - 66., mid() - 1., 10.).unwrap();
    profile
        .initialize_creature_capabilities(fixture::creature_capability_sources(&profile))
        .unwrap();
    let m = profile.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: b.content,
        geometry: profile.geometry_revision,
        map: b.map,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: profile.npcs[0].ai.bounding_radius,
                height: m.npc(2).unwrap().npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: m.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(b.content, b.map)),
        static_los: None,
        primary_near_point: (!empty)
            .then(|| PrimaryNearPointPolicy::from_profile(&profile).unwrap()),
        player_environment: None,
        same_polygon_route: None,
        empty_selector_near_point: empty
            .then(|| EmptySelectorNearPointPolicy::from_profile(&profile).unwrap()),
    };
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let mut authority = fixture::SyntheticMotionAuthority::default();
    for tick in 1..=2 {
        world.step(tick, None, &mut authority).unwrap();
    }
    let request = authority
        .queries
        .into_iter()
        .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap();
    (world, session, request)
}
fn setup(
    snapshot: &MapHeightSnapshot<'_>,
    config: SourceNearPointConfiguration,
) -> (NativeWorld, Session, AuthorityRequest) {
    setup_selected(snapshot, config, true)
}
fn inner_mut(request: &mut AuthorityRequest) -> &mut motion::NearPointRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut request.query else {
        unreachable!()
    };
    &mut near.request
}
fn point(facts: Facts) -> (motion::Query, Point) {
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = facts else {
        panic!("expected source motion")
    };
    let motion::Fact::NearPoint(point) = reply.fact else {
        panic!("expected near point")
    };
    (reply.request, point)
}
fn assert_echo(request: &AuthorityRequest, echo: motion::Query) {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
        unreachable!()
    };
    assert_eq!(echo, motion::Query::NearPoint(near.request.clone()));
}
#[derive(Default)]
struct Trace {
    ground: Vec<SequentialGroundReply>,
    rays: Vec<(RayRequest, StaticRay)>,
    order: String,
}
impl Trace {
    fn observe(&mut self, event: QueryObservation<'_>) {
        match event {
            QueryObservation::Ground(reply) => {
                self.order.push('H');
                self.ground.push(reply.clone());
            }
            QueryObservation::Ray(request, result) => {
                self.order.push('L');
                self.rays.push((*request, *result));
            }
        }
    }
    fn assert_context(&self, request: &AuthorityRequest) {
        for candidate in &self.ground {
            assert_eq!(candidate.ground.request.context, *request);
        }
    }
}
fn source_rows(data: &str) -> Vec<BTreeMap<&str, &str>> {
    let columns: Vec<_> = data
        .lines()
        .find(|line| line.starts_with("# case\t"))
        .unwrap()
        .trim_start_matches("# ")
        .split('\t')
        .collect();
    data.lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), columns.len());
            columns.iter().copied().zip(fields).collect()
        })
        .collect()
}
fn assert_source_case(
    trace: &Trace,
    request: &AuthorityRequest,
    label: &str,
    source_result: [u32; 3],
) {
    let cases = source_rows(include_str!(
        "../../../../../../../benilla-native-geometry-tests/tools/fixtures/near_point_empty/cases.test-fixture"
    ));
    let heights = source_rows(include_str!(
        "../../../../../../../benilla-native-geometry-tests/tools/fixtures/near_point_empty/heights.test-fixture"
    ));
    let case = cases.iter().find(|row| row["label"] == label).unwrap();
    let hex =
        |row: &BTreeMap<&str, &str>, field: &str| u32::from_str_radix(row[field], 16).unwrap();
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
        unreachable!()
    };
    let q = &near.request;
    let target = q.target.pose.position;
    assert_eq!(
        [target.x, target.y, target.z].map(|v| v.bits()),
        [hex(case, "pos_x"), hex(case, "pos_y"), hex(case, "pos_z")]
    );
    assert_eq!(
        [target.x, target.y, target.z].map(|v| v.bits()),
        [
            hex(case, "target_x"),
            hex(case, "target_y"),
            hex(case, "target_z")
        ]
    );
    let motion::NearPointAngle::Absolute(angle) = q.angle else {
        panic!("the full-source scalar companion has an explicit absolute angle");
    };
    assert_eq!(angle.bits(), hex(case, "angle"));
    assert_eq!(q.distance.bits(), hex(case, "distance"));
    assert_eq!(q.owner_bounding_radius.bits(), hex(case, "searcher_radius"));
    assert_eq!(q.target.bounding_radius.bits(), hex(case, "target_radius"));
    assert_eq!(
        q.target.collision_height.bits(),
        hex(case, "collision_height")
    );
    assert_eq!(
        source_result,
        [
            hex(case, "result_x"),
            hex(case, "result_y"),
            hex(case, "result_z")
        ]
    );
    assert_eq!(
        trace.ground.len(),
        case["height_calls"].parse::<usize>().unwrap()
    );
    assert_eq!(
        trace.rays.len(),
        case["static_calls"].parse::<usize>().unwrap()
    );
    let expected_order: String = case["trace"]
        .chars()
        .filter_map(|c| match c {
            'H' => Some('H'),
            'S' => Some('L'),
            _ => None,
        })
        .collect();
    assert_eq!(trace.order, expected_order);
    let expected: Vec<_> = heights
        .iter()
        .filter(|row| row["case"] == case["case"])
        .collect();
    assert_eq!(expected.len(), trace.ground.len());
    let mut ray_index = 0;
    for (index, (actual, expected)) in trace.ground.iter().zip(expected).enumerate() {
        assert_eq!(index, expected["height_index"].parse::<usize>().unwrap());
        assert_eq!(
            actual.phase,
            if expected["xy_index"] == "-1" {
                CandidatePhase::Final
            } else if index == 0 {
                CandidatePhase::Primary
            } else {
                CandidatePhase::Free
            }
        );
        assert_eq!(
            actual.ground.request.position,
            [
                hex(expected, "query_x"),
                hex(expected, "query_y"),
                hex(expected, "query_z")
            ]
        );
        assert_eq!(actual.ground.ground_height, hex(expected, "ground_reply"));
        assert_eq!(actual.ground.allowed_z, hex(expected, "adjusted_z"));
        assert_eq!(expected["swim"], "0");
        if expected["static_called"] == "1" {
            let (ray, result) = trace.rays[ray_index];
            ray_index += 1;
            assert_eq!(ray.ticket, actual.los_ticket.unwrap());
            assert_eq!(
                ray.from,
                [
                    hex(expected, "los_src_x"),
                    hex(expected, "los_src_y"),
                    hex(expected, "los_src_z")
                ]
            );
            assert_eq!(
                ray.to,
                [
                    hex(expected, "los_dest_x"),
                    hex(expected, "los_dest_y"),
                    hex(expected, "los_dest_z")
                ]
            );
            assert_eq!(
                matches!(result, StaticRay::Miss),
                expected["static_reply"] == "1"
            );
            assert_eq!(
                expected["dynamic_called"] == "1",
                matches!(result, StaticRay::Miss)
            );
            assert_eq!(expected["ignore_m2"], "0");
            assert!(!ray.ignore_m2);
        } else {
            assert_eq!(expected["dynamic_called"], "0");
        }
    }
    assert_eq!(ray_index, trace.rays.len());
    // Original source callback traces are scalar evidence only. Every compared
    // height and ray above came from the real loaded products, independently.
}
fn set_case(request: &mut AuthorityRequest, internal: [f32; 3], distance: f32) {
    let q = inner_mut(request);
    q.target.pose.position =
        Point::new(mid() - internal[0], mid() - internal[1], internal[2]).unwrap();
    q.distance = ai::Real::new(distance).unwrap();
    q.angle = motion::NearPointAngle::Absolute(ai::Real::ZERO);
}

#[test]
fn loaded_primary_acceptance_preserves_exact_request_and_owner() {
    let (snapshot, config) = loaded();
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let before = world.checkpoint().unwrap();
    let mut trace = Trace::default();
    let (echo, result) = point(
        provider
            .resolve_observed(&session, &request, deadline(), |e| trace.observe(e))
            .unwrap(),
    );
    assert_echo(&request, echo);
    assert_eq!(result, Point::new(mid() - 52.5, mid() - 1., 7.).unwrap());
    assert_eq!(trace.order, "HL");
    assert_eq!(trace.ground[0].phase, CandidatePhase::Primary);
    assert_eq!(trace.rays[0].1, StaticRay::Miss);
    trace.assert_context(&request);
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn actual_loaded_wall_selects_earliest_clear_free_candidate() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    set_case(&mut request, [42., 1., 7.], 2.5);
    inner_mut(&mut request).owner.position = Point::new(mid() - 26., mid() - 1., 7.).unwrap();
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let before = world.checkpoint().unwrap();
    let mut trace = Trace::default();
    let (echo, result) = point(
        provider
            .resolve_observed(&session, &request, deadline(), |e| trace.observe(e))
            .unwrap(),
    );
    assert_echo(&request, echo);
    assert_source_case(
        &trace,
        &request,
        "loaded_wall_scalar_companion",
        [result.x, result.y, result.z].map(ai::Real::bits),
    );
    assert_eq!(trace.ground.len(), 6);
    assert_eq!(trace.order, "HLHLHLHLHLHL");
    assert_eq!(trace.ground[0].phase, CandidatePhase::Primary);
    assert!(trace.ground[1..]
        .iter()
        .all(|p| p.phase == CandidatePhase::Free));
    assert!(trace.rays[..5]
        .iter()
        .all(|(_, hit)| matches!(hit, StaticRay::Hit { .. })));
    assert_eq!(trace.rays[5].1, StaticRay::Miss);
    let accepted = trace.ground.last().unwrap();
    let bits = accepted.ground.request.position;
    assert_eq!(
        [result.x, result.y, result.z].map(|v| v.as_f32().to_bits()),
        [bits[0], bits[1], accepted.ground.allowed_z]
    );
    for (candidate, (ray, _)) in trace.ground.iter().zip(&trace.rays) {
        assert_eq!(candidate.ground.ground_height, 7f32.to_bits());
        assert_eq!(ray.from, [mid() - 42., mid() - 1., 9.].map(f32::to_bits));
        assert_eq!(ray.ticket, candidate.los_ticket.unwrap());
        assert!(!ray.ignore_m2);
    }
    trace.assert_context(&request);
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn vertical_exhaustion_returns_source_first_xy_without_any_final_los() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    set_case(&mut request, [50., 1., 11.], 2.5);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let before = world.checkpoint().unwrap();
    let mut trace = Trace::default();
    let (echo, result) = point(
        provider
            .resolve_observed(&session, &request, deadline(), |e| trace.observe(e))
            .unwrap(),
    );
    assert_echo(&request, echo);
    assert_eq!(result, Point::new(mid() - 47.5, mid() - 1., 7.).unwrap());
    assert_source_case(
        &trace,
        &request,
        "loaded_flat_exhaustion_scalar_companion",
        [result.x, result.y, result.z].map(ai::Real::bits),
    );
    assert!(trace.rays.is_empty());
    assert_eq!(trace.ground.first().unwrap().phase, CandidatePhase::Primary);
    let last = trace.ground.last().unwrap();
    assert_eq!(last.phase, CandidatePhase::Final);
    assert_eq!(last.los_ticket, None);
    assert_eq!(last.ground.request.position[2], 7f32.to_bits());
    assert!(trace.ground[1..trace.ground.len() - 1]
        .iter()
        .all(|p| p.phase == CandidatePhase::Free));
    assert!(trace.ground[..trace.ground.len() - 1]
        .iter()
        .all(|p| p.ground.request.position[2] == 11f32.to_bits()));
    assert!(trace
        .ground
        .iter()
        .all(|p| p.ground.allowed_z == 7f32.to_bits()));
    trace.assert_context(&request);
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn actual_stack_exhaustion_carries_last_z_instead_of_resetting_target_z() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    set_case(&mut request, [24., 1., 11.], 0.5);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let mut trace = Trace::default();
    let (echo, result) = point(
        provider
            .resolve_observed(&session, &request, deadline(), |e| trace.observe(e))
            .unwrap(),
    );
    assert_echo(&request, echo);
    assert_source_case(
        &trace,
        &request,
        "loaded_carried_z_scalar_companion",
        [result.x, result.y, result.z].map(ai::Real::bits),
    );
    assert_eq!(trace.ground.len(), 8); // Primary, six source free angles, final.
    assert!(trace.rays.is_empty());
    let initial = &trace.ground[0];
    let predecessor = &trace.ground[6];
    let last = &trace.ground[7];
    assert_eq!(initial.phase, CandidatePhase::Primary);
    assert_eq!(initial.ground.request.position[2], 11f32.to_bits());
    assert_eq!(initial.ground.allowed_z, 9f32.to_bits());
    assert_eq!(predecessor.phase, CandidatePhase::Free);
    assert_eq!(predecessor.ground.request.position[2], 11f32.to_bits());
    assert_eq!(predecessor.ground.allowed_z, 7f32.to_bits());
    assert_eq!(last.phase, CandidatePhase::Final);
    assert_eq!(last.los_ticket, None);
    assert_eq!(
        last.ground.request.position[..2],
        initial.ground.request.position[..2]
    );
    assert_eq!(
        last.ground.request.position[2],
        predecessor.ground.allowed_z
    );
    assert_eq!(last.ground.allowed_z, 7f32.to_bits());
    assert_eq!(result, Point::new(mid() - 23.5, mid() - 1., 7.).unwrap());
    // The real initial-candidate reader provides the counterfactual reset at the
    // same XY, so this is observable loaded geometry, not just carried metadata.
    let reset = provider
        .ground
        .begin(&session, &request)
        .unwrap()
        .derive_initial_candidate(99, deadline())
        .unwrap();
    assert_eq!(
        reset.ground.request.position[..2],
        last.ground.request.position[..2]
    );
    assert_eq!(reset.ground.allowed_z, 9f32.to_bits());
    assert_ne!(reset.ground.allowed_z, last.ground.allowed_z);
    trace.assert_context(&request);
}

#[test]
fn all_loaded_los_candidates_blocked_still_returns_the_obstructed_final_point() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) =
        setup_selected_with_player_height(&snapshot, config, true, 3.);
    set_case(&mut request, [20., 1., 7.], 0.5);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let before = world.checkpoint().unwrap();
    let mut trace = Trace::default();
    let (echo, result) = point(
        provider
            .resolve_observed(&session, &request, deadline(), |e| trace.observe(e))
            .unwrap(),
    );
    assert_echo(&request, echo);
    assert_source_case(
        &trace,
        &request,
        "loaded_blocked_exhaustion_scalar_companion",
        [result.x, result.y, result.z].map(ai::Real::bits),
    );
    assert_eq!(trace.ground.len(), 8);
    assert_eq!(trace.rays.len(), 7);
    assert_eq!(trace.order, "HLHLHLHLHLHLHLH");
    assert!(trace
        .rays
        .iter()
        .all(|(_, result)| matches!(result, StaticRay::Hit { .. })));
    assert!(trace
        .ground
        .iter()
        .all(|p| p.ground.allowed_z == 7f32.to_bits()));
    let final_ground = trace.ground.last().unwrap();
    assert_eq!(final_ground.phase, CandidatePhase::Final);
    assert_eq!(final_ground.los_ticket, None);
    assert_eq!(result, Point::new(mid() - 19.5, mid() - 1., 7.).unwrap());
    // Height starts at z+2=9 and skips that exact-origin plane. The real target
    // eye at z+3=10 crosses plane9 on every LOS, including this returned point.
    let final_ray = RayRequest {
        ticket: 2001,
        to: [result.x, result.y, result.z].map(ai::Real::bits),
        ..trace.rays[0].0
    };
    assert!(matches!(
        provider
            .ground
            .static_vmap()
            .query(final_ray, deadline())
            .unwrap()
            .result,
        StaticRay::Hit { .. }
    ));
    trace.assert_context(&request);
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn missing_intermediate_coverage_never_becomes_selector_exhaustion() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    set_case(&mut request, [532., 10., 20.], 4.);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let before = world.checkpoint().unwrap();
    let mut trace = Trace::default();
    assert_eq!(
        provider.resolve_observed(&session, &request, deadline(), |e| trace.observe(e)),
        Err(Failure::MissingGeometry)
    );
    assert!(trace.ground.len() > 1);
    assert_eq!(trace.ground[0].phase, CandidatePhase::Primary);
    assert!(trace.ground[1..]
        .iter()
        .all(|p| p.phase == CandidatePhase::Free));
    assert!(trace.rays.is_empty());
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn expired_original_deadline_is_not_renewed_between_rejected_candidates() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    set_case(&mut request, [50., 1., 11.], 2.5);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let mut trace = Trace::default();
    assert_eq!(
        provider.resolve_observed(&session, &request, Instant::now(), |e| trace.observe(e)),
        Err(Failure::Deadline)
    );
    assert!(trace.ground.is_empty());
    assert!(trace.rays.is_empty());
    let expires = Instant::now() + Duration::from_millis(100);
    assert_eq!(
        provider.resolve_observed(&session, &request, expires, |event| {
            trace.observe(event);
            std::thread::sleep(expires.saturating_duration_since(Instant::now()));
        }),
        Err(Failure::Deadline)
    );
    assert_eq!(trace.ground.len(), 1);
    assert_eq!(trace.ground[0].phase, CandidatePhase::Primary);
    assert!(trace.rays.is_empty());
}

#[test]
fn unrepresentable_source_return_stops_without_trying_another_candidate() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    let q = inner_mut(&mut request);
    q.target.pose.position = Point::new(-f32::MIN_POSITIVE, 800., 7.).unwrap();
    q.distance = ai::Real::from_bits(0x0080_0001).unwrap();
    q.angle = motion::NearPointAngle::Absolute(ai::Real::ZERO);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let mut trace = Trace::default();
    assert_eq!(
        provider.resolve_observed(&session, &request, deadline(), |e| trace.observe(e)),
        Err(Failure::UnsupportedPointRepresentation)
    );
    assert_eq!(trace.order, "HL");
    assert_eq!(trace.ground[0].phase, CandidatePhase::Primary);
    assert_source_case(
        &trace,
        &request,
        "loaded_subnormal_scalar_companion",
        [1, 800f32.to_bits(), 7f32.to_bits()],
    );
    assert_eq!(trace.ground[0].ground.request.position[0], 1);
    assert_eq!(trace.ground[0].ground.allowed_z, 7f32.to_bits());
    assert_eq!(trace.rays[0].1, StaticRay::Miss);
    // Actual later probes exist and can produce a representable point. They are
    // deliberately not visited by resolve after the source's first return.
    let mut sequence = provider
        .ground
        .begin(&session, &request)
        .unwrap()
        .empty_selector_sequence()
        .unwrap();
    let mut budget = QueryBudget::new(deadline());
    let mut representable_later = false;
    while let Some(candidate) = sequence.next_with_budget(&mut budget).unwrap() {
        if candidate.phase != CandidatePhase::Free {
            continue;
        }
        let p = candidate.ground.request.position;
        if source_near_point::exact_point([p[0], p[1], candidate.ground.allowed_z]).is_ok() {
            representable_later = true;
            break;
        }
    }
    assert!(representable_later);
}

#[test]
fn configuration_policy_and_independent_participant_proof_are_required() {
    for field in 0..12 {
        let (snapshot, config) = loaded();
        let (world, mut session, _) = setup(&snapshot, config);
        let mut wrong = config;
        match field {
            0 => wrong.detect_position_collision = false,
            1 => wrong.vmap_line_of_sight_enabled = false,
            2 => wrong.height.vmap_height_enabled = false,
            3 => wrong.height.map += 1,
            4 => wrong.height.content[0] ^= 1,
            5 => wrong.height.source_epoch[0] ^= 1,
            6 => wrong.height.effective_config[0] ^= 1,
            7 => session.participants = None,
            8 => {
                session.participants = Some(NonRenderParticipants::authored_fixture(
                    session.content,
                    session.map + 1,
                ))
            }
            9 => session.empty_selector_near_point = None,
            10 => {
                session.primary_near_point =
                    Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap())
            }
            _ => {
                let mut other = world.profile().clone();
                other.geometry_revision[0] ^= 1;
                session.empty_selector_near_point =
                    Some(EmptySelectorNearPointPolicy::from_profile(&other).unwrap());
            }
        }
        assert!(
            SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, wrong)
                .is_err(),
            "field {field}"
        );
    }
}

#[test]
fn request_and_retained_session_bindings_are_rechecked_before_any_query() {
    let (snapshot, config) = loaded();
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    for field in 0..11 {
        let mut wrong = request.clone();
        match field {
            0 => wrong.binding.world[0] ^= 1,
            1 => wrong.binding.profile[0] ^= 1,
            2 => wrong.binding.authority_policy[0] ^= 1,
            3 => wrong.binding.geometry[0] ^= 1,
            4 => wrong.binding.npc.as_mut().unwrap().generation += 1,
            5 => inner_mut(&mut wrong).binding.now_ms += 1,
            6 => inner_mut(&mut wrong).binding.identity.content.0[0] ^= 1,
            7 => inner_mut(&mut wrong).binding.identity.geometry.0[0] ^= 1,
            8 => inner_mut(&mut wrong).binding.identity.map += 1,
            9 => inner_mut(&mut wrong).binding.query_id = 0,
            _ => wrong.binding.tick = 0,
        }
        let mut trace = Trace::default();
        assert_eq!(
            provider.resolve_observed(&session, &wrong, deadline(), |e| trace.observe(e)),
            Err(Failure::Binding),
            "field {field}"
        );
        assert!(trace.ground.is_empty());
    }
    for field in 0..4 {
        let mut changed = session.clone();
        match field {
            0 => changed.geometry[0] ^= 1,
            1 => changed.participants = None,
            2 => changed.empty_selector_near_point = None,
            _ => changed.player.as_mut().unwrap().body.height = ai::Real::new(3.).unwrap(),
        }
        let mut trace = Trace::default();
        assert!(provider
            .resolve_observed(&changed, &request, deadline(), |e| trace.observe(e))
            .is_err());
        assert!(trace.ground.is_empty());
    }
}

#[test]
fn primary_only_provider_keeps_rejecting_the_same_required_fallbacks() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup_selected(&snapshot, config, false);
    let empty_revision = geometry_revision(&snapshot, config).unwrap();
    assert_ne!(empty_revision, session.geometry);
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    for (position, distance) in [
        ([42., 1., 7.], 2.5),
        ([50., 1., 11.], 2.5),
        ([24., 1., 11.], 0.5),
    ] {
        set_case(&mut request, position, distance);
        assert_eq!(
            provider.resolve(&session, &request, deadline()),
            Err(Failure::UnsupportedNearPointFallback)
        );
    }
    let (snapshot, config) = loaded();
    let (world, session, _) = setup(&snapshot, config);
    assert!(matches!(
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config),
        Err(Failure::UnsupportedQuery)
    ));
}

fn mailbox_resolve(
    session: Session,
    request: AuthorityRequest,
    mut resolve: impl FnMut(&Session, &AuthorityRequest, Instant) -> Result<Facts, Failure>,
) -> Result<AuthorityReply, NativeError> {
    let (mut authority, service) = core::mailbox(session).unwrap();
    let expected = request.clone();
    let worker = std::thread::spawn(move || authority.resolve(&request));
    let end = deadline();
    let mut serviced = 0;
    while !worker.is_finished() && Instant::now() < end {
        let start = Instant::now();
        serviced += service.serve_queries_until(1, |session, request, _, expires| {
            assert_eq!(request, &expected);
            assert!(expires >= start);
            assert!(expires <= Instant::now() + Duration::from_millis(2));
            resolve(session, request, expires)
        });
        std::thread::yield_now();
    }
    assert!(
        worker.is_finished(),
        "cross-thread geometry request did not finish"
    );
    assert_eq!(serviced, 1);
    worker.join().unwrap()
}

#[test]
fn loaded_empty_selector_mailbox_returns_exact_fact_without_moving_owner() {
    let (snapshot, config) = loaded();
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
            .unwrap();
    let before = world.checkpoint().unwrap();
    let reply = mailbox_resolve(session, request.clone(), |session, request, expires| {
        provider.resolve(session, request, expires)
    })
    .unwrap();
    assert_eq!(reply.request, request);
    let (echo, result) = point(reply.facts);
    assert_echo(&request, echo);
    assert_eq!(result, Point::new(mid() - 52.5, mid() - 1., 7.).unwrap());
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn cumulative_work_budget_covers_all_free_heights_rays_and_final_height() {
    for (position, distance) in [([42., 1., 7.], 2.5), ([24., 1., 11.], 0.5)] {
        let (snapshot, config) = loaded();
        let (world, session, mut request) = setup(&snapshot, config);
        set_case(&mut request, position, distance);
        let provider =
            SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
                .unwrap();
        let mut measured = QueryBudget::new(deadline());
        let initial_work = measured.remaining_work_for_test();
        let mut complete_trace = Trace::default();
        let expected = provider
            .resolve_budget_observed(&session, &request, &mut measured, |e| {
                complete_trace.observe(e)
            })
            .unwrap();
        let work = initial_work - measured.remaining_work_for_test();
        assert!(work > 1);
        assert!(complete_trace.ground.len() > 1);
        for limit in [work - 1, work] {
            let mut budget = QueryBudget::new(deadline());
            budget.restrict_work_for_test(limit);
            let mut trace = Trace::default();
            let result = provider
                .resolve_budget_observed(&session, &request, &mut budget, |e| trace.observe(e));
            assert_eq!(budget.remaining_work_for_test(), 0);
            if limit == work {
                assert_eq!(result, Ok(expected.clone()));
                assert_eq!(trace.order, complete_trace.order);
            } else {
                assert_eq!(result, Err(Failure::MissingGeometry));
                assert!(trace.order.len() < complete_trace.order.len());
                assert!(!trace
                    .ground
                    .iter()
                    .any(|p| p.phase == CandidatePhase::Final));
            }
        }
    }
}
