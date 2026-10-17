// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded authored products; no installed world or production admission.
use super::super::{
    core::{self, Body, NonRenderParticipants, PlayerMotion, PrimaryNearPointPolicy},
    products,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as fixture, Authority, AuthorityReply,
    NativeError, NativeWorld,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
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
fn setup(
    snapshot: &MapHeightSnapshot<'_>,
    config: SourceNearPointConfiguration,
) -> (NativeWorld, Session, AuthorityRequest) {
    let b = snapshot.binding();
    let mut profile = fixture::profile();
    profile.map_id = b.map;
    profile.content_digest = b.content;
    profile.geometry_revision = geometry_revision(snapshot, config).unwrap();
    profile.npcs[0].ai.map_id = b.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(b.content);
    let mid = 0.5f32 * (64.0 * 533.333_3);
    profile.player.start = ai::Point::new(mid - 50., mid - 1., 10.).unwrap();
    profile.npcs[0].ai.spawn = ai::Point::new(mid - 66., mid - 1., 10.).unwrap();
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
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
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
    let motion::Fact::NearPoint(p) = reply.fact else {
        panic!("expected near point")
    };
    (reply.request, p)
}
#[test]
fn loaded_primary_fact_echoes_real_request_and_leaves_owner_unchanged() {
    let (snapshot, config) = loaded();
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let before = world.checkpoint().unwrap();
    let (echo, p) = point(provider.resolve(&session, &request, deadline()).unwrap());
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
        unreachable!()
    };
    assert_eq!(echo, motion::Query::NearPoint(near.request.clone()));
    let mid = 0.5f32 * (64.0 * 533.333_3);
    assert_eq!(p, ai::Point::new(mid - 52.5, mid - 1., 7.).unwrap());
    assert_eq!(world.checkpoint().unwrap(), before);
    assert_eq!(
        provider.resolve(&session, &request, Instant::now()),
        Err(Failure::Deadline)
    );
}
#[test]
fn primary_config_identity_and_participant_exclusion_are_not_inferred() {
    for field in 0..8 {
        let (snapshot, config) = loaded();
        let (world, session, _) = setup(&snapshot, config);
        let mut wrong = config;
        match field {
            0 => wrong.detect_position_collision = false,
            1 => wrong.vmap_line_of_sight_enabled = false,
            2 => wrong.height.vmap_height_enabled = false,
            3 => wrong.height.map += 1,
            4 => wrong.height.content[0] ^= 1,
            5 => wrong.height.source_epoch[0] ^= 1,
            6 => wrong.height.effective_config[0] ^= 1,
            _ => {}
        }
        let mut session = session;
        if field == 7 {
            session.participants = None;
        }
        assert!(
            SourceNearPointProvider::bind(&session, world.profile(), snapshot, wrong).is_err(),
            "field {field}"
        );
    }
}
#[test]
fn vertical_failure_and_blocked_los_require_fallback_without_publishing_negative_fact() {
    let (snapshot, config) = loaded();
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let before = world.checkpoint().unwrap();
    let mut vertical = request.clone();
    inner_mut(&mut vertical).target.pose.position.z = ai::Real::new(20.).unwrap();
    assert_eq!(
        provider.resolve(&session, &vertical, deadline()),
        Err(Failure::UnsupportedNearPointFallback)
    );
    // Authored wall at internal x40 intercepts the target-eye to candidate-feet
    // ray. A failed first candidate is no fact.
    let mut blocked = request;
    let q = inner_mut(&mut blocked);
    let mid = 0.5f32 * (64.0 * 533.333_3);
    q.owner.position = Point::new(mid - 26., mid - 1., 7.).unwrap();
    q.target.pose.position = Point::new(mid - 42., mid - 1., 7.).unwrap();
    assert_eq!(
        provider.resolve(&session, &blocked, deadline()),
        Err(Failure::UnsupportedNearPointFallback)
    );
    assert_eq!(world.checkpoint().unwrap(), before);
}
#[test]
fn missing_coverage_changed_bindings_and_unrepresentable_points_fail_closed() {
    let (snapshot, config) = loaded();
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    for field in 0..7 {
        let mut wrong = request.clone();
        match field {
            0 => wrong.binding.world[0] ^= 1,
            1 => wrong.binding.profile[0] ^= 1,
            2 => wrong.binding.geometry[0] ^= 1,
            3 => wrong.binding.npc.as_mut().unwrap().generation += 1,
            4 => inner_mut(&mut wrong).binding.now_ms += 1,
            5 => inner_mut(&mut wrong).binding.identity.content.0[0] ^= 1,
            _ => inner_mut(&mut wrong).binding.identity.map += 1,
        }
        assert_eq!(
            provider.resolve(&session, &wrong, deadline()),
            Err(Failure::Binding)
        );
    }
    let mut missing = request;
    let q = inner_mut(&mut missing);
    q.target.pose.position.x = ai::Real::ZERO;
    q.target.pose.position.y = ai::Real::ZERO;
    assert_eq!(
        provider.resolve(&session, &missing, deadline()),
        Err(Failure::MissingGeometry)
    );
    for bits in [
        [0x8000_0000, 0, 0],
        [1, 0, 0],
        [0, 0x7fc0_0000, 0],
        [32761f32.to_bits(), 0, 0],
    ] {
        assert_eq!(
            exact_point(bits),
            Err(Failure::UnsupportedPointRepresentation)
        );
    }
    assert_eq!(exact_point([0; 3]), Ok(ai::Point::new(0., 0., 0.).unwrap()));
}

#[test]
fn independent_source_first_path_scalar_oracle_preserves_order_and_boundaries() {
    use super::super::products::ground_height::allowed_ground_z;
    let data = include_str!(
        "../../../../../../../benilla-native-geometry-tests/tools/fixtures/near_point_primary/primary-acceptance.test-fixture"
    );
    let header = data
        .lines()
        .find(|line| line.starts_with("# case\t"))
        .unwrap();
    let columns: BTreeMap<_, _> = header
        .trim_start_matches("# ")
        .split('\t')
        .enumerate()
        .map(|(i, name)| (name, i))
        .collect();
    let mut count = 0;
    for line in data.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), columns.len());
        let get = |name| row[columns[name]];
        let bits = |name| u32::from_str_radix(get(name), 16).unwrap();
        let flag = |name| get(name) == "1";
        let z = allowed_ground_z(
            f32::from_bits(bits("pos_z")),
            f32::from_bits(bits("ground")),
        )
        .to_bits();
        assert_eq!(bits("result_z"), z, "{line}");
        assert_eq!(bits("result_x"), bits("candidate_x"));
        assert_eq!(bits("result_y"), bits("candidate_y"));
        let vertical = primary::vertical_acceptance_bits(
            bits("pos_z"),
            z,
            bits("distance"),
            bits("searcher_radius"),
            bits("target_radius"),
        )
        .unwrap();
        let los = flag("detect") && vertical;
        assert_eq!(flag("static_calls"), los, "{line}");
        assert_eq!(flag("dynamic_calls"), los && flag("static_los"), "{line}");
        let accepted = vertical && flag("static_los") && flag("dynamic_los");
        let outcome = if !flag("detect") {
            "collision-disabled"
        } else if accepted {
            "primary-accepted"
        } else {
            "fallback-required"
        };
        assert_eq!(get("outcome"), outcome, "{line}");
        if los {
            assert_eq!(
                primary::target_eye_bits(
                    [bits("target_x"), bits("target_y"), bits("target_z")],
                    bits("collision_height")
                )
                .unwrap(),
                [bits("los_src_x"), bits("los_src_y"), bits("los_src_z")]
            );
            assert_eq!(
                [bits("los_dest_x"), bits("los_dest_y"), bits("los_dest_z")],
                [bits("candidate_x"), bits("candidate_y"), z]
            );
            assert_eq!(get("ignore_m2"), "0");
        }
        let mut trace = String::from("IC");
        if flag("detect") {
            trace.push('R');
        }
        trace.push('H');
        if los {
            trace.push_str("PS");
            if flag("static_los") {
                trace.push('D');
            }
        }
        if flag("detect") && !accepted {
            trace.push('F');
        }
        assert_eq!(get("trace"), trace, "{line}");
        count += 1;
    }
    assert_eq!(count, 301);
    // Authored callback observations do not establish real geometry or dynamic participants.
}

#[test]
fn actual_loaded_wall_distinguishes_target_eye_from_searcher_eye() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let q = inner_mut(&mut request);
    q.owner.position = Point::new(mid - 30., mid - 1., 7.).unwrap();
    q.target.pose.position = Point::new(mid - 50., mid - 1., 7.).unwrap();
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let (_, p) = point(provider.resolve(&session, &request, deadline()).unwrap());
    assert_eq!(p, Point::new(mid - 47.5, mid - 1., 7.).unwrap());
    let vmap = provider.ground.static_vmap();
    let wrong_origin = RayRequest {
        snapshot: vmap.binding(),
        ticket: 99,
        from: [mid - 30., mid - 1., 9.].map(f32::to_bits),
        to: [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()].map(f32::to_bits),
        ignore_m2: false,
    };
    assert!(matches!(
        vmap.query(wrong_origin, deadline()).unwrap().result,
        StaticRay::Hit { .. }
    ));
}

#[test]
fn actual_loaded_wall_distinguishes_feet_destination_from_raised_destination() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let q = inner_mut(&mut request);
    q.owner.position = Point::new(mid - 26., mid - 1., 7.).unwrap();
    q.target.pose.position = Point::new(mid - 42., mid - 1., 7.).unwrap();
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    assert_eq!(
        provider.resolve(&session, &request, deadline()),
        Err(Failure::UnsupportedNearPointFallback)
    );
    let vmap = provider.ground.static_vmap();
    let raised_destination = RayRequest {
        snapshot: vmap.binding(),
        ticket: 99,
        from: [mid - 42., mid - 1., 9.].map(f32::to_bits),
        to: [mid - 39.5, mid - 1., 9.].map(f32::to_bits),
        ignore_m2: false,
    };
    assert_eq!(
        vmap.query(raised_destination, deadline()).unwrap().result,
        StaticRay::Miss
    );
}

#[test]
fn actual_loaded_m2_blocks_primary_even_when_spell_style_ray_would_ignore_it() {
    let (snapshot, config) = loaded();
    let (world, session, mut request) = setup(&snapshot, config);
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let q = inner_mut(&mut request);
    // Explicit authored positions exercise the accepted raw input domain and clamp;
    // they are not claimed to be generated by the ordinary chase fixture above.
    q.owner.position = Point::new(mid - 100., mid - 20., -1.5).unwrap();
    q.target.pose.position = Point::new(mid - 100., mid + 20., -1.5).unwrap();
    q.distance = ai::Real::new(20.).unwrap();
    q.angle = motion::NearPointAngle::Absolute(ai::Real::new(f32::from_bits(0x4096_cbe4)).unwrap());
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let candidate = provider
        .ground
        .begin(&session, &request)
        .unwrap()
        .derive_initial_candidate(1, deadline())
        .unwrap();
    let p = candidate.ground.request.position;
    let vmap = provider.ground.static_vmap();
    let ray = RayRequest {
        snapshot: vmap.binding(),
        ticket: 99,
        from: [mid - 100., mid + 20., 0.5].map(f32::to_bits),
        to: [p[0], p[1], candidate.ground.allowed_z],
        ignore_m2: false,
    };
    assert!(matches!(
        vmap.query(ray, deadline()).unwrap().result,
        StaticRay::Hit { .. }
    ));
    assert_eq!(
        vmap.query(
            RayRequest {
                ignore_m2: true,
                ..ray
            },
            deadline()
        )
        .unwrap()
        .result,
        StaticRay::Miss
    );
    assert_eq!(
        provider.resolve(&session, &request, deadline()),
        Err(Failure::UnsupportedNearPointFallback)
    );
}

#[test]
fn covered_source_hole_can_accept_unchanged_z_without_inventing_ground_support() {
    let (products, height) = products::authored_hole_ground_height();
    let proof = NonRenderParticipants::authored_fixture(height.content, height.map);
    let snapshot = Arc::new(products)
        .load_map_height_owned(height, Some(&proof))
        .unwrap();
    let config = SourceNearPointConfiguration {
        height,
        detect_position_collision: true,
        vmap_line_of_sight_enabled: true,
    };
    let (world, session, request) = setup(&snapshot, config);
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let candidate = provider
        .ground
        .begin(&session, &request)
        .unwrap()
        .derive_initial_candidate(1, deadline())
        .unwrap();
    assert_eq!(candidate.ground.ground_height, (-200_000f32).to_bits());
    assert_eq!(candidate.ground.allowed_z, 10f32.to_bits());
    let (_, p) = point(provider.resolve(&session, &request, deadline()).unwrap());
    assert_eq!(p.z.as_f32().to_bits(), 10f32.to_bits());
    // This matches GetNearPointAt's first-return semantics, not a support proof.
}

fn mailbox_resolve(
    session: Session,
    request: AuthorityRequest,
    mut resolve: impl FnMut(&Session, &AuthorityRequest, Instant) -> Result<Facts, Failure>,
) -> Result<AuthorityReply, NativeError> {
    let (mut authority, service) = core::mailbox(session).unwrap();
    let expected_request = request.clone();
    let request_copy = request;
    let worker = std::thread::spawn(move || authority.resolve(&request_copy));
    let end = deadline();
    let mut serviced = 0;
    while !worker.is_finished() && Instant::now() < end {
        let frame_start = Instant::now();
        serviced += service.serve_queries_until(1, |session, request, _, deadline| {
            assert_eq!(request, &expected_request);
            assert!(deadline >= frame_start);
            assert!(deadline <= Instant::now() + Duration::from_millis(2));
            resolve(session, request, deadline)
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
fn loaded_primary_mailbox_returns_exact_fact_with_original_frame_deadline() {
    let (snapshot, config) = loaded();
    let (world, mut session, request) = setup(&snapshot, config);
    session.primary_near_point =
        Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let before = world.checkpoint().unwrap();
    let reply = mailbox_resolve(session, request.clone(), |session, request, deadline| {
        provider.resolve(session, request, deadline)
    })
    .unwrap();
    assert_eq!(reply.request, request);
    let (echo, p) = point(reply.facts);
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = request.query else {
        unreachable!()
    };
    assert_eq!(echo, motion::Query::NearPoint(near.request));
    let mid = 0.5f32 * (64.0 * 533.333_3);
    assert_eq!(p, Point::new(mid - 52.5, mid - 1., 7.).unwrap());
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn loaded_primary_mailbox_rejects_modified_echo_and_unrelated_fact_kinds() {
    for fault in 0..3 {
        let (snapshot, config) = loaded();
        let (world, mut session, request) = setup(&snapshot, config);
        session.primary_near_point =
            Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
        let provider =
            SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
        let before = world.checkpoint().unwrap();
        let result = mailbox_resolve(session, request, |session, request, deadline| {
            let mut facts = provider.resolve(session, request, deadline)?;
            let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &mut facts else {
                unreachable!()
            };
            match fault {
                0 => {
                    let motion::Query::NearPoint(query) = &mut reply.request else {
                        unreachable!()
                    };
                    query.binding.query_id += 1;
                }
                1 => reply.fact = motion::Fact::Missing,
                _ => facts = Facts::PlayerMovement { accepted: true },
            }
            Ok(facts)
        });
        assert_eq!(result, Err(NativeError::AuthorityBinding), "fault {fault}");
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}

#[test]
fn replacement_loaded_mailbox_cannot_reactivate_the_old_authority() {
    let (snapshot, config) = loaded();
    let (world, mut session, request) = setup(&snapshot, config);
    session.primary_near_point =
        Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    let (mut old_authority, old_service) = core::mailbox(session.clone()).unwrap();
    drop(old_service);
    let old_request = request.clone();
    let old_worker = std::thread::spawn(move || old_authority.resolve(&old_request));
    let replacement = mailbox_resolve(session, request, |session, request, deadline| {
        provider.resolve(session, request, deadline)
    });
    assert!(replacement.is_ok());
    assert!(matches!(
        old_worker.join().unwrap(),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}
