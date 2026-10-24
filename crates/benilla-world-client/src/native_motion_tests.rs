// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-event traces, with no source database or game assets.
use super::*;
use n::synthetic_fixture::{point as at, real};

fn npc(spawn: u64, generation: u64) -> NpcId {
    NpcId { spawn, generation }
}
fn launch(id: u64, facing: source::Facing, walking: bool) -> source::Event {
    source::Event::SplineLaunched {
        path: vec![
            at(16.0),
            ai::Point::new(12.0, 3.0, 0.0).unwrap(),
            at(6.0),
            at(2.0),
        ],
        spline_id: id,
        path_identity: [11; 32],
        source: source::RouteSource::SyntheticFixture,
        duration_ms: 5432,
        velocity: real(2.5),
        facing,
        walking,
    }
}

#[test]
fn full_source_trace_preserves_launch_and_stop_without_per_tick_motion_packets() {
    let id = npc(2, 0);
    let represented = BTreeMap::from([(id, 77)]);
    let pose = source::Pose {
        position: at(7.25),
        orientation: real(1.75),
    };
    let trace = [
        launch(5, source::Facing::Target(n::SOLO_UNIT), true),
        source::Event::Relocated {
            pose,
            at_ms: 400,
            notify_map: true,
        },
        source::Event::FacingChanged {
            orientation: real(2.5),
        },
        source::Event::SplineAdvanced {
            spline_id: 5,
            event: source::SplineEvent::NextSegment { segment: 1 },
        },
        source::Event::SplineInterrupted { pose, spline_id: 6 },
        source::Event::HomeStarted { was_active: false },
        launch(7, source::Facing::Angle(real(0.75)), false),
        source::Event::SplineAdvanced {
            spline_id: 7,
            event: source::SplineEvent::Arrived,
        },
        source::Event::Relocated {
            pose,
            at_ms: 800,
            notify_map: true,
        },
        source::Event::HomeReached {
            restore_inactive: true,
        },
        source::Event::ChaseReached,
        source::Event::ChaseUnreachable,
    ];
    let projected = trace
        .iter()
        .map(|event| project(id, event, 55, &represented).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(projected.iter().filter_map(Option::as_ref).count(), 3);
    assert!(matches!(&projected[0], Some(SessionEvent::MonsterMove {
        guid: 77, transport: None, start, spline_id: 5, path,
        facing: p::MonsterMoveFacing::Target(55), stop: false,
        duration_ms: 5432, flying: false, run_mode: true,
    }) if *start == [16.0, 0.0, 0.0] && path == &vec![
        [16.0, 0.0, 0.0], [12.0, 3.0, 0.0], [6.0, 0.0, 0.0], [2.0, 0.0, 0.0]
    ]));
    assert!(matches!(&projected[4], Some(SessionEvent::MonsterMove {
        guid: 77, transport: None, start, spline_id: 6, path,
        facing: p::MonsterMoveFacing::None, stop: true,
        duration_ms: 0, flying: false, ..
    }) if *start == [7.25, 0.0, 0.0] && path.is_empty()));
    assert!(matches!(&projected[6], Some(SessionEvent::MonsterMove {
        spline_id: 7, facing: p::MonsterMoveFacing::Angle(angle), run_mode: true, ..
    }) if *angle == 0.75));
}

#[test]
fn facing_target_uses_only_current_represented_incarnation_and_rejects_missing_target() {
    let id = npc(2, 0);
    let target = npc(3, 7);
    let mut represented = BTreeMap::from([(id, 77), (target, 88)]);
    let event = launch(1, source::Facing::Target(3), false);
    assert!(matches!(
        project(id, &event, 55, &represented).unwrap(),
        Some(SessionEvent::MonsterMove {
            facing: p::MonsterMoveFacing::Target(88),
            ..
        })
    ));
    represented.remove(&target);
    assert!(matches!(
        project(id, &event, 55, &represented),
        Err(Error::UnknownTarget)
    ));
    represented.insert(npc(3, 8), 99);
    assert!(matches!(
        project(id, &event, 55, &represented).unwrap(),
        Some(SessionEvent::MonsterMove {
            facing: p::MonsterMoveFacing::Target(99),
            ..
        })
    ));
    represented.remove(&id);
    assert!(matches!(
        project(id, &event, 55, &represented),
        Err(Error::StaleProjection)
    ));
}

#[test]
fn tangent_facing_and_u32_spline_boundary_are_exact() {
    let id = npc(2, 0);
    let represented = BTreeMap::from([(id, 77)]);
    let event = launch(u64::from(u32::MAX), source::Facing::Tangent, false);
    assert!(matches!(
        project(id, &event, 55, &represented).unwrap(),
        Some(SessionEvent::MonsterMove {
            spline_id: u32::MAX,
            facing: p::MonsterMoveFacing::None,
            ..
        })
    ));
    let event = launch(u64::from(u32::MAX) + 1, source::Facing::Tangent, false);
    assert!(matches!(
        project(id, &event, 55, &represented),
        Err(Error::Unrepresentable)
    ));
}

fn source_world() -> n::NativeWorld {
    let mut profile = n::synthetic_fixture::profile();
    let mut genesis = n::synthetic_motion_fixture::genesis(&profile, 2);
    genesis.npcs.get_mut(&2).unwrap().npc_spawn_orientation = n::Orientation::new(1.25).unwrap();
    profile.source_motion = Some(genesis);
    n::NativeWorld::synthetic_component_test(profile, [8; 16], n::synthetic_fixture::player(), 7)
        .unwrap()
}
fn source_input() -> source::UnitInputs {
    source::UnitInputs {
        audit: source::MotionAudit {
            flight_swimming_transport: source::Evidence::Absent,
            immobilization: source::Evidence::Absent,
            non_melee_casting: source::Evidence::Absent,
            speed_auras: source::Evidence::Absent,
            combat_scripts: source::Evidence::Absent,
            target_airborne_or_transport: source::Evidence::Absent,
        },
        health: 1000,
        maximum_health: 1000,
        walk_speed: real(2.5),
        run_speed: real(2.5),
        settings: source::GroundSettings {
            combat_reach: real(1.5),
            bounding_radius: real(0.5),
            collision_height: real(2.0),
            no_wounded_slowdown: false,
            backpedal_enabled: false,
            fanning_enabled: false,
        },
    }
}
struct OriginalRoute;
impl source::Authority for OriginalRoute {
    fn resolve(&mut self, query: &source::Query) -> Result<source::Reply, source::MotionError> {
        let fact = match query {
            source::Query::NearPoint(request) => source::Fact::NearPoint(
                ai::Point::new(
                    request.target.pose.position.x.as_f32() + request.distance.as_f32(),
                    request.target.pose.position.y.as_f32(),
                    request.target.pose.position.z.as_f32(),
                )
                .unwrap(),
            ),
            source::Query::Route(request) => {
                use ai::movement::{Check, SegmentChecks};
                let checks = SegmentChecks {
                    loaded_coverage: Check::Passed,
                    line_of_sight: Check::Passed,
                    body_sweep: Check::Passed,
                    continuous_ground_support: Check::Passed,
                    dry_ground: Check::Passed,
                };
                source::Fact::Route(source::RouteOutcome::Complete(source::CompleteRoute {
                    source: source::RouteSource::SyntheticFixture,
                    path_identity: [12; 32],
                    points: vec![
                        request.origin,
                        ai::Point::new(12.0, 3.0, 0.0).unwrap(),
                        at(6.0),
                        request.goal,
                    ],
                    segments: vec![checks; 3],
                }))
            }
            source::Query::Bearing(request) => {
                let mut angle = (request.toward.y.as_f32() - request.origin.y.as_f32())
                    .atan2(request.toward.x.as_f32() - request.origin.x.as_f32());
                if angle < 0.0 {
                    angle += std::f32::consts::TAU;
                }
                source::Fact::Bearing(real(angle))
            }
            _ => panic!("unexpected source request: {query:?}"),
        };
        Ok(source::Reply {
            request: query.clone(),
            fact,
        })
    }
}

#[test]
fn creation_uses_source_orientation_and_rejoins_validated_active_spline_clock() {
    let world = source_world();
    let projection =
        NativeProjection::new(&world, world.state().player, &super::super::tests::Catalog).unwrap();
    let initial = projection.initial_events(&world).unwrap();
    assert!(initial
        .iter()
        .any(|event| matches!(event, SessionEvent::ObjectCreate {
        orientation, spline: None, ..
    } if *orientation == 1.25)));
    let mut npc = world.state().npcs[&2].clone();
    let movement = &mut npc.source_motion.as_mut().unwrap().motion;
    let launch = movement
        .start_home(
            source::Pose {
                position: at(2.0),
                orientation: real(0.75),
            },
            true,
            false,
            source_input(),
            &mut OriginalRoute,
        )
        .unwrap();
    assert!(launch
        .iter()
        .any(|event| matches!(event, source::Event::SplineLaunched { walking: true, .. })));
    movement
        .advance(400, None, source_input(), &mut OriginalRoute)
        .unwrap();
    // Exercise the same validated snapshot reconstruction used on reopen.
    *movement = source::MotionState::restore(movement.snapshot()).unwrap();
    let snapshot = movement.snapshot();
    let represented = BTreeMap::from([(npc.id, projection.npc_guid(npc.id).unwrap())]);
    assert!(
        launch.iter().any(|event| matches!(
            project(npc.id, event, projection.player_guid(), &represented).unwrap(),
            Some(SessionEvent::MonsterMove {
                run_mode: true,
                stop: false,
                ..
            })
        )),
        "walking launch still receives the source fake Runmode bit"
    );
    let active = snapshot.spline.as_ref().unwrap();
    let events = projection.create(&world, &npc).unwrap();
    let created = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectCreate {
                position,
                orientation,
                spline: Some(spline),
                ..
            } => Some((position, orientation, spline)),
            _ => None,
        })
        .expect("active source spline must resume in ObjectCreate");
    assert_eq!(*created.0, point(snapshot.map_pose.position));
    assert_eq!(*created.1, snapshot.map_pose.orientation.as_f32());
    let resumed = created.2;
    assert_eq!(
        resumed.path,
        active
            .spline
            .points
            .iter()
            .copied()
            .map(point)
            .collect::<Vec<_>>()
    );
    assert_eq!(resumed.id, active.spline_id as u32);
    assert_eq!(resumed.time_passed_ms, 400);
    assert_eq!(
        resumed.duration_ms,
        *active.spline.timestamps_ms.last().unwrap()
    );
    assert!(!resumed.flying && !resumed.cyclic && !resumed.run_mode);
    assert_ne!(
        resumed.path[0], *created.0,
        "resume retains the original path origin"
    );
    let movement = &mut npc.source_motion.as_mut().unwrap().motion;
    movement
        .advance(10_000, None, source_input(), &mut OriginalRoute)
        .unwrap();
    assert!(create_spline(&npc).unwrap().is_none());
}

#[derive(Default)]
struct MultiPointAuthority(n::synthetic_motion_fixture::SyntheticMotionAuthority);
impl n::Authority for MultiPointAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        let mut reply = n::Authority::resolve(&mut self.0, request)?;
        let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts else {
            return Ok(reply);
        };
        let source::Reply {
            request: source::Query::Route(query),
            fact: source::Fact::Route(source::RouteOutcome::Complete(route)),
        } = spatial.as_mut()
        else {
            return Ok(reply);
        };
        let midpoint = ai::Point::new(
            (query.origin.x.as_f32() + query.goal.x.as_f32()) * 0.5,
            (query.origin.y.as_f32() + query.goal.y.as_f32()) * 0.5 + 1.0,
            (query.origin.z.as_f32() + query.goal.z.as_f32()) * 0.5,
        )
        .unwrap();
        route.points.insert(1, midpoint);
        route.segments.push(route.segments[0]);
        Ok(reply)
    }
}

#[test]
fn committed_runtime_trace_projects_only_real_multi_point_launches_and_stops() {
    let mut profile = n::synthetic_motion_fixture::profile();
    profile.npcs[0].ai.hard_leash_radius = Some(real(5.0));
    let mut world = n::NativeWorld::synthetic_component_test(
        profile,
        [8; 16],
        n::synthetic_fixture::player(),
        7,
    )
    .unwrap();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::super::tests::Catalog).unwrap();
    let mut authority = MultiPointAuthority::default();
    let mut launches = 0;
    let mut relocations = 0;
    let mut multi_point_packets = 0;
    let mut home_reached = false;
    for tick in 1..=100 {
        world.step(tick, None, &mut authority).unwrap();
        let expected = world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .filter_map(|event| match event {
                n::Event::NpcMotion { id, event } => Some((*id, event)),
                _ => None,
            })
            .collect::<Vec<_>>();
        relocations += expected
            .iter()
            .filter(|(_, event)| matches!(event, source::Event::Relocated { .. }))
            .count();
        home_reached |= expected
            .iter()
            .any(|(_, event)| matches!(event, source::Event::HomeReached { .. }));
        let packets = projection.advance(&world).unwrap();
        let packets = packets
            .iter()
            .filter(|event| matches!(event, SessionEvent::MonsterMove { .. }))
            .collect::<Vec<_>>();
        let expected = expected
            .into_iter()
            .filter(|(_, event)| {
                matches!(
                    event,
                    source::Event::SplineLaunched { .. } | source::Event::SplineInterrupted { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            packets.len(),
            expected.len(),
            "tick {tick} must not synthesize a relocation spline"
        );
        for (packet, (id, event)) in packets.into_iter().zip(expected) {
            let SessionEvent::MonsterMove {
                guid,
                transport,
                start,
                spline_id,
                path,
                facing,
                stop,
                duration_ms,
                flying,
                run_mode,
            } = packet
            else {
                unreachable!()
            };
            assert_eq!(Some(*guid), projection.npc_guid(id));
            assert!(transport.is_none() && !flying);
            match event {
                source::Event::SplineLaunched {
                    path: source_path,
                    spline_id: source_id,
                    duration_ms: source_duration,
                    facing: source_facing,
                    ..
                } => {
                    launches += 1;
                    assert_eq!(path, &path::linear_path(source_path).unwrap());
                    multi_point_packets += usize::from(path.len() > 2);
                    assert!(path.len() <= source_path.len());
                    assert_eq!(*start, point(source_path[0]));
                    assert_eq!(u64::from(*spline_id), *source_id);
                    assert_eq!(*duration_ms, *source_duration);
                    assert!(!stop);
                    assert!(*run_mode, "source launch always carries Runmode");
                    match (source_facing, facing) {
                        (
                            source::Facing::Target(n::SOLO_UNIT),
                            p::MonsterMoveFacing::Target(guid),
                        ) => assert_eq!(*guid, projection.player_guid()),
                        (source::Facing::Angle(a), p::MonsterMoveFacing::Angle(b)) => {
                            assert_eq!(a.as_f32(), *b)
                        }
                        (source::Facing::Tangent, p::MonsterMoveFacing::None) => {}
                        _ => panic!("source facing changed during projection"),
                    }
                }
                source::Event::SplineInterrupted {
                    spline_id: source_id,
                    pose,
                } => {
                    assert_eq!(u64::from(*spline_id), *source_id);
                    assert_eq!(*start, point(pose.position));
                    assert!(path.is_empty() && *stop && *duration_ms == 0);
                    assert!(matches!(facing, p::MonsterMoveFacing::None));
                }
                _ => unreachable!(),
            }
        }
    }
    assert!(launches >= 2 && relocations > launches && home_reached && multi_point_packets > 0);
}

#[test]
fn source_player_pose_requires_explicit_ground_flags_and_legacy_helpers_stay_separate() {
    let mut world = source_world();
    let projection =
        NativeProjection::new(&world, world.state().player, &super::super::tests::Catalog).unwrap();
    let position = world.state().position;
    let orientation = n::Orientation::new(1.5).unwrap();
    assert!(matches!(
        projection.move_to(&world, position),
        Err(Error::UnsupportedCommand)
    ));
    assert!(matches!(
        projection.pose(&world, position, orientation),
        Err(Error::UnsupportedCommand)
    ));
    let flags = source::GroundMovementFlags::from_bits(0x101).unwrap();
    let input = projection
        .movement_pose(&world, position, orientation, flags)
        .unwrap();
    assert!(
        matches!(input.input.command, n::Command::MovementPose { movement_flags, .. } if movement_flags == flags)
    );
    world
        .step(
            1,
            Some(input),
            &mut n::synthetic_motion_fixture::SyntheticMotionAuthority {
                visits: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(world.state().player_movement_flags, Some(flags));
    assert_eq!(world.state().orientation, orientation);

    let legacy = n::synthetic_fixture::world();
    let projection = NativeProjection::new(
        &legacy,
        legacy.state().player,
        &super::super::tests::Catalog,
    )
    .unwrap();
    assert!(projection.move_to(&legacy, position).is_ok());
    assert!(projection.pose(&legacy, position, orientation).is_ok());
    assert!(matches!(
        projection.movement_pose(&legacy, position, orientation, flags),
        Err(Error::UnsupportedCommand)
    ));
}

#[test]
fn pinned_linear_packet_oracle_matches_packed_words_and_real_protocol_decode() {
    use benilla_protocol::{
        messages::{opcode, parse_server_with_tail, ServerPacket},
        wire::write_packed_guid,
    };
    let mut count = 0;
    for line in include_str!("../tools/native_linear_path_cmangos.tsv").lines() {
        let mut tokens = line.split_whitespace();
        assert_eq!(tokens.next(), Some("P"));
        let mut values = tokens.map(|word| word.parse::<u32>().unwrap());
        let length = values.next().unwrap() as usize;
        let input = (0..length)
            .map(|_| ai::Point {
                x: ai::Real::from_bits(values.next().unwrap()).unwrap(),
                y: ai::Real::from_bits(values.next().unwrap()).unwrap(),
                z: ai::Real::from_bits(values.next().unwrap()).unwrap(),
            })
            .collect::<Vec<_>>();
        let length = values.next().unwrap() as usize;
        let expected_words = values.by_ref().take(length).collect::<Vec<_>>();
        let length = values.next().unwrap() as usize;
        let expected = (0..length)
            .map(|_| std::array::from_fn::<_, 3, _>(|_| values.next().unwrap()))
            .collect::<Vec<_>>();
        assert!(values.next().is_none());
        let before = input.clone();
        assert_eq!(
            path::packed_offsets(&input).unwrap(),
            expected_words,
            "{line}"
        );
        let decoded = path::linear_path(&input).unwrap();
        assert_eq!(
            decoded
                .iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>(),
            expected,
            "{line}"
        );
        assert_eq!(
            input, before,
            "presentation must not quantize authority vertices"
        );

        // Feed the independent C++ packed words through the actual client parser.
        let mut body = vec![];
        write_packed_guid(77, &mut body).unwrap();
        for coordinate in point(input[0]) {
            body.extend_from_slice(&coordinate.to_bits().to_le_bytes());
        }
        body.extend_from_slice(&7u32.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&0x100u32.to_le_bytes());
        body.extend_from_slice(&6000u32.to_le_bytes());
        body.extend_from_slice(&((expected_words.len() + 1) as u32).to_le_bytes());
        for coordinate in point(*input.last().unwrap()) {
            body.extend_from_slice(&coordinate.to_bits().to_le_bytes());
        }
        for word in expected_words {
            body.extend_from_slice(&word.to_le_bytes());
        }
        let (packet, tail) = parse_server_with_tail(opcode::SMSG_MONSTER_MOVE, &body).unwrap();
        assert_eq!(tail, 0);
        let ServerPacket::MonsterMove { path, run_mode, .. } = packet else {
            panic!("expected linear spline packet")
        };
        assert!(run_mode);
        assert_eq!(
            path.iter()
                .map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
                .collect::<Vec<_>>(),
            expected,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 92);
}

#[test]
fn source_linear_threshold_is_strict_and_packing_truncates_signed_and_wraps_long_offsets() {
    let below = f32::from_bits(0x3eff_ffff);
    let above = f32::from_bits(0x3f00_0001);
    let input = vec![
        at(1.0),
        ai::Point::new(-below, -0.5, 0.0).unwrap(),
        ai::Point::new(-0.5, -0.5, 0.0).unwrap(),
        ai::Point::new(-above, -0.5, 0.0).unwrap(),
        at(0.0),
    ];
    assert_eq!(
        path::packed_offsets(&input).unwrap(),
        vec![2 | (2 << 11); 2]
    );
    assert_eq!(
        path::linear_path(&input).unwrap(),
        vec![
            [1.0, 0.0, 0.0],
            [-0.5, -0.5, 0.0],
            [-0.5, -0.5, 0.0],
            [0.0; 3]
        ]
    );
    let input = vec![at(0.0), ai::Point::new(0.99, -0.99, 1.99).unwrap(), at(0.0)];
    assert_eq!(
        path::packed_offsets(&input).unwrap(),
        vec![((-3i32 as u32) & 0x7ff) | (3 << 11) | (((-7i32 as u32) & 0x3ff) << 22)]
    );
    assert_eq!(path::linear_path(&input).unwrap()[1], [0.75, -0.75, 1.75]);
    let input = vec![
        at(0.0),
        ai::Point::new(-256.25, 512.25, -128.25).unwrap(),
        at(0.0),
    ];
    assert_eq!(
        path::linear_path(&input).unwrap()[1],
        [255.75, 0.25, 127.75]
    );
    assert!(path::linear_path(&[]).is_err());
    assert!(path::linear_path(&[at(0.0)]).is_err());
    assert!(path::linear_path(&vec![at(0.0); source::MAX_PATH_POINTS + 1]).is_err());
    let outside = ai::Point {
        x: real(32761.0),
        y: ai::Real::ZERO,
        z: ai::Real::ZERO,
    };
    assert!(path::linear_path(&[at(0.0), outside]).is_err());
}

#[test]
fn recorded_launch_then_death_stop_keeps_order_for_a_represented_zero_health_npc() {
    let world = source_world();
    let projection =
        NativeProjection::new(&world, world.state().player, &super::super::tests::Catalog).unwrap();
    let mut corpse = world.state().npcs[&2].clone();
    let id = corpse.id;
    let guid = projection.npc_guid(id).unwrap();
    let movement = &mut corpse.source_motion.as_mut().unwrap().motion;
    let launches = movement
        .start_chase(
            source::Target {
                movement_flags: source::GroundMovementFlags::STOPPED,
                id: n::SOLO_UNIT,
                pose: source::Pose {
                    position: world.state().position,
                    orientation: ai::Real::ZERO,
                },
                combat_reach: world.profile().player.combat_reach,
                bounding_radius: world.profile().player.bounding_radius,
                collision_height: real(2.0),
            },
            false,
            source_input(),
            &mut OriginalRoute,
        )
        .unwrap();
    corpse.health = 0;
    let stop = movement.stop_for_death().unwrap();
    assert!(!movement.is_moving());
    assert!(movement.snapshot().spline.is_none());
    let mut recorded = launches
        .into_iter()
        .map(|event| n::Event::NpcMotion { id, event })
        .collect::<Vec<_>>();
    recorded.push(n::Event::NpcMotion { id, event: stop });
    let represented = BTreeMap::from([(id, guid)]);
    for disappears in [false, true] {
        let mut frame = Frame::new(&represented, world.state(), &world).unwrap();
        let mut projected = vec![];
        if !disappears {
            // Final health is already zero; motion must come from journal events.
            let values = projection.npc_fields(&world, &corpse).unwrap();
            assert_eq!(values.unit_health(), Some(0));
        }
        let offset = projected.len();
        for event in &recorded {
            let n::Event::NpcMotion { id, event } = event else {
                unreachable!()
            };
            projected.extend(
                project(*id, event, projection.player_guid(), frame.represented()).unwrap(),
            );
        }
        assert!(matches!(
            projected[offset],
            SessionEvent::MonsterMove {
                spline_id: 1,
                stop: false,
                ..
            }
        ));
        assert!(matches!(
            projected[offset + 1],
            SessionEvent::MonsterMove {
                spline_id: 2,
                stop: true,
                ..
            }
        ));
        if disappears {
            projected.push(frame.remove(id).unwrap());
            assert!(matches!(projected[offset + 2], SessionEvent::ObjectDestroyed(g) if g == guid));
            frame.finish(&BTreeMap::new()).unwrap();
            let n::Event::NpcMotion { id, event } = &recorded[0] else {
                unreachable!()
            };
            assert!(matches!(
                project(*id, event, projection.player_guid(), frame.represented()),
                Err(Error::StaleProjection)
            ));
        } else {
            frame.finish(&represented).unwrap();
        }
    }
}

#[test]
fn source_generation_replacement_requires_ordered_removal_and_fresh_guid() {
    let world = source_world();
    let projection =
        NativeProjection::new(&world, world.state().player, &super::super::tests::Catalog).unwrap();
    let old = world.state().npcs[&2].id;
    let old_guid = projection.npc_guid(old).unwrap();
    let new = NpcId {
        generation: old.generation + 1,
        ..old
    };
    let new_guid = npc_guid(&world, new).unwrap();
    assert_ne!(old_guid, new_guid);
    let represented = BTreeMap::from([(old, old_guid)]);
    let mut frame = Frame::new(&represented, world.state(), &world).unwrap();
    assert!(matches!(
        frame.spawn(new, new_guid),
        Err(Error::StaleProjection)
    ));
    assert!(matches!(frame.remove(old), Ok(SessionEvent::ObjectDestroyed(g)) if g == old_guid));
    assert!(matches!(
        frame.spawn(new, old_guid),
        Err(Error::GuidCollision)
    ));
    assert!(matches!(
        frame.spawn(old, old_guid),
        Err(Error::StaleProjection)
    ));
    frame.spawn(new, new_guid).unwrap();
    frame.finish(&BTreeMap::from([(new, new_guid)])).unwrap();
    assert!(frame.guid(&old).is_err());
    assert_eq!(frame.guid(&new).unwrap(), new_guid);
    assert!(frame.finish(&represented).is_err());
}

#[test]
fn lethal_first_hit_projects_launch_damage_stop_before_zero_corpse_destruction_and_respawn() {
    for corpse_decay_ms in [0, 5000] {
        let mut profile = n::synthetic_fixture::profile();
        profile.npcs[0].max_health = 1;
        profile.npcs[0].ai.corpse_decay_ms = corpse_decay_ms;
        profile.source_motion = Some(n::synthetic_motion_fixture::genesis(&profile, 2));
        let mut world = n::NativeWorld::synthetic_component_test(
            profile,
            [8; 16],
            n::synthetic_fixture::player(),
            1234,
        )
        .unwrap();
        let mut projection =
            NativeProjection::new(&world, world.state().player, &super::super::tests::Catalog)
                .unwrap();
        let old_id = world.state().npcs[&2].id;
        let old_guid = projection.npc_guid(old_id).unwrap();
        let mut authority = n::synthetic_motion_fixture::SyntheticMotionAuthority {
            critical_chance: Some(0),
            visits: Some(false),
            ..Default::default()
        };
        let mut killed = false;
        let mut respawned = false;
        for tick in 1..=100 {
            let input = (tick == 3).then(|| n::synthetic_fixture::cast(&world, 60001));
            world.step(tick, input, &mut authority).unwrap();
            let death = world.last_tick().unwrap().events.iter().any(|event| {
                matches!(event,
                n::Event::Npc { id, effect: ai::Effect::Died } if *id == old_id)
            });
            let packets = projection.advance(&world).unwrap();
            if death {
                killed = true;
                let launch = packets.iter().position(|event| matches!(event, SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == old_guid)).expect("lethal first hit retains pre-damage launch");
                let damage = packets
                    .iter()
                    .position(|event| matches!(event, SessionEvent::SpellDamageLog(_)))
                    .unwrap();
                let stop = packets.iter().position(|event| matches!(event, SessionEvent::MonsterMove { guid, stop: true, .. } if *guid == old_guid)).unwrap();
                assert!(launch < damage && damage < stop);
                assert_eq!(world.state().npcs[&2].health, 0);
                if corpse_decay_ms == 0 {
                    let destruction = packets.iter().position(|event| matches!(event, SessionEvent::ObjectDestroyed(guid) if *guid == old_guid)).unwrap();
                    assert!(stop < destruction);
                    assert_eq!(projection.npc_guid(old_id), None);
                } else {
                    assert_eq!(projection.npc_guid(old_id), Some(old_guid));
                    let health = packets
                        .iter()
                        .position(|event| {
                            matches!(event,
                                SessionEvent::ObjectValues { guid, fields }
                                    if *guid == old_guid && fields.unit_health() == Some(0)
                            )
                        })
                        .expect("retained corpse publishes its final health");
                    assert!(
                        stop < health,
                        "Map value updates follow launch/damage/death callbacks"
                    );
                    assert!(!packets
                        .iter()
                        .any(|event| matches!(event, SessionEvent::ObjectDestroyed(_))));
                    break;
                }
            }
            if world.state().npcs[&2].id != old_id {
                let new_id = world.state().npcs[&2].id;
                let new_guid = projection.npc_guid(new_id).unwrap();
                assert_ne!(old_guid, new_guid);
                assert!(packets.iter().any(|event| matches!(event, SessionEvent::ObjectCreate { guid, .. } if *guid == new_guid)));
                respawned = true;
                break;
            }
        }
        assert!(killed);
        if corpse_decay_ms == 0 {
            assert!(respawned);
        }
    }
}
