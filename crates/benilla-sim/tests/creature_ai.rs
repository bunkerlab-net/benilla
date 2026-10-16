// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::creature_ai::{movement::*, *};
fn real(v: f32) -> Real {
    Real::new(v).unwrap()
}
fn point(x: f32) -> Point {
    Point::new(x, 0., 0.).unwrap()
}
fn profile(reaction: Reaction) -> Profile {
    Profile {
        creature_id: 1,
        map_id: 7,
        content_revision: Revision([3; 32]),
        spawn: point(0.),
        clock_origin_ms: 0,
        level: 1,
        reaction,
        neutral_to_all: false,
        detection: real(18.),
        aggro_rate: Real::ONE,
        combat_reach: real(1.5),
        bounding_radius: real(0.5),
        melee_leeway: real(2.66),
        run_speed: real(7.),
        home_speed: real(7.),
        mainhand_interval_ms: 2000,
        pursuit_ms: 15000,
        leash_radius: real(30.),
        hard_leash_radius: None,
        corpse_decay_ms: 3000,
        respawn_delay_seconds: 5,
        unsupported: vec![],
    }
}
fn target(x: f32) -> Target {
    Target {
        id: 2,
        position: point(x),
        level: 1,
        combat_reach: real(1.5),
        bounding_radius: real(0.5),
        moving_and_running: false,
        alive: true,
        online: true,
        can_attack_in_combat: true,
        can_attack_on_sight: true,
        visible: true,
        accessible: true,
        line_of_sight: true,
    }
}
fn observation(seq: u64, elapsed: u32, x: f32) -> Observation {
    Observation {
        sequence: seq,
        elapsed_ms: elapsed,
        geometry_revision: Revision([4; 32]),
        target: Some(target(x)),
        can_initiate_attack: true,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: false,
        creature_moving_and_running: false,
        movement: None,
        receipts: vec![],
    }
}
struct Authority(Observation);
impl CombatAuthority for Authority {
    fn observe(&mut self, id: u64) -> Result<Observation, AiError> {
        assert_eq!(id, 1);
        Ok(self.0.clone())
    }
}
fn step(ai: &mut SoloCreatureAi, o: Observation) -> Result<Vec<Effect>, AiError> {
    ai.step(&mut Authority(o))
}
struct Geometry {
    result: MovementResult,
}
impl GeometryAuthority for Geometry {
    fn resolve(&mut self, r: &MovementRequest) -> Result<MovementOutcome, AiError> {
        Ok(MovementOutcome {
            request_id: r.request_id,
            map_id: r.map_id,
            content_revision: r.content_revision,
            geometry_revision: r.geometry_revision,
            result: self.result.clone(),
        })
    }
}
fn checks() -> SegmentChecks {
    SegmentChecks {
        loaded_coverage: Check::Passed,
        line_of_sight: Check::Passed,
        body_sweep: Check::Passed,
        continuous_ground_support: Check::Passed,
        dry_ground: Check::Passed,
    }
}
fn request(ai: &SoloCreatureAi, id: u64, elapsed: u32, goal: f32) -> MovementRequest {
    let c = ai.motion().unwrap();
    MovementRequest {
        request_id: id,
        command_id: c.command_id,
        creature_id: 1,
        map_id: 7,
        origin: ai.position(),
        goal: point(goal),
        speed: real(7.),
        elapsed_ms: elapsed,
        content_revision: Revision([3; 32]),
        geometry_revision: Revision([4; 32]),
        purpose: c.purpose,
    }
}
fn blocked(ai: &SoloCreatureAi, id: u64, elapsed: u32) -> ValidatedMovement {
    resolve_movement(
        &mut Geometry {
            result: MovementResult::Blocked(BlockReason::Obstacle),
        },
        request(ai, id, elapsed, 100.),
    )
    .unwrap()
}
fn progress(
    ai: &SoloCreatureAi,
    id: u64,
    elapsed: u32,
    goal: f32,
    pos: f32,
    travel: f32,
    arrived: bool,
) -> ValidatedMovement {
    resolve_movement(
        &mut Geometry {
            result: MovementResult::Progress {
                position: point(pos),
                travelled: real(travel),
                arrived,
                checks: checks(),
            },
        },
        request(ai, id, elapsed, goal),
    )
    .unwrap()
}

#[test]
fn exact_extracted_cpp_numeric_and_swing_branches() {
    let mut rows = 0;
    for line in include_str!("fixtures/creature_ai_cmangos.tsv").lines() {
        rows += 1;
        let mut words = line.split_whitespace();
        let kind = words.next().unwrap();
        let v: Vec<u32> = words.map(|s| s.parse().unwrap()).collect();
        let r = |n| Real::from_bits(v[n]).unwrap();
        match kind {
            "a" => assert_eq!(
                attack_distance(v[0] as u8, v[1] as u8, r(2), r(3))
                    .unwrap()
                    .bits(),
                v[4],
                "{line}"
            ),
            "r" => assert_eq!(combat_reach(r(0), r(1)).unwrap().bits(), v[2], "{line}"),
            "d" => {
                let p = Point {
                    x: r(0),
                    y: r(1),
                    z: Real::ZERO,
                };
                let q = Point {
                    x: r(2),
                    y: r(3),
                    z: Real::ZERO,
                };
                assert_eq!(distance_2d(p, q, r(4)).unwrap().bits(), v[5], "{line}");
            }
            "t" => assert_eq!(
                pulls_aggro(r(0), r(1), v[2] != 0).unwrap(),
                v[3] != 0,
                "{line}"
            ),
            "m" => {
                let leeway = if v[1] & 1 != 0 && v[1] & 2 == 0 && v[1] & 4 != 0 && v[1] & 8 == 0 {
                    real(2.66)
                } else {
                    Real::ZERO
                };
                assert_eq!(
                    in_melee_range(
                        point(0.),
                        Point {
                            x: r(0),
                            y: Real::ZERO,
                            z: real(100.)
                        },
                        real(5.),
                        leeway
                    )
                    .unwrap(),
                    v[2] != 0,
                    "{line}"
                );
            }
            "s" => {
                let mask = v[0];
                let c = SwingConditions {
                    has_victim: mask & 1 != 0,
                    casting_non_melee: mask & 2 != 0,
                    can_initiate: mask & 4 != 0,
                    in_range: mask & 8 != 0,
                    in_arc: mask & 16 != 0,
                    target_alive: mask & 32 != 0,
                    can_attack: mask & 64 != 0,
                };
                let d = melee_decision(v[1], c);
                let (timer, swings, error) = match d {
                    SwingDecision::Wait => (v[1], 0, 0),
                    SwingDecision::Swing => (2000, 1, 0),
                    SwingDecision::Retry(SwingError::OutOfRange) => (v[1], 0, 0),
                    SwingDecision::Retry(_) => (100, 0, 1),
                };
                assert_eq!((timer, swings, error), (v[2], v[3], v[4]), "{line}");
            }
            _ => panic!("unknown fixture kind"),
        }
    }
    assert_eq!(rows, 52394);
}

#[test]
fn aggressive_detection_inclusive_boundary_and_authority_gates() {
    for (reaction, expected) in [
        (Reaction::Aggressive, true),
        (Reaction::Defensive, false),
        (Reaction::Passive, false),
    ] {
        let mut ai = SoloCreatureAi::new(profile(reaction)).unwrap();
        let e = step(&mut ai, observation(1, 20, 18.)).unwrap();
        assert_eq!(
            e.iter().any(|e| matches!(e, Effect::CombatStarted { .. })),
            expected
        );
    }
    for index in 0..7 {
        let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
        let mut o = observation(1, 20, 17.);
        let t = o.target.as_mut().unwrap();
        match index {
            0 => t.line_of_sight = false,
            1 => t.accessible = false,
            2 => t.visible = false,
            3 => t.can_attack_on_sight = false,
            4 => t.alive = false,
            5 => t.online = false,
            _ => o.can_initiate_attack = false,
        }
        assert!(step(&mut ai, o).unwrap().is_empty());
        assert_eq!(ai.phase(), Phase::Idle);
    }
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    assert!(step(&mut ai, observation(1, 20, 18.0001))
        .unwrap()
        .is_empty());
}

#[test]
fn defensive_retaliation_threat_periodic_and_direct_pursuit() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Defensive)).unwrap();
    let mut o = observation(1, 20, 2.);
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::AttackedBy { target: 2 },
    });
    let e = step(&mut ai, o).unwrap();
    assert!(e.contains(&Effect::MainhandSwing { target: 2 }));
    assert_eq!(ai.pursuit_timer_ms(), 14980);
    let mut o = observation(2, 1000, 2.);
    o.receipts.push(Receipt {
        id: 2,
        event: CombatEvent::Threat {
            target: 2,
            amount: real(12.),
            kind: ThreatKind::PeriodicDamage,
        },
    });
    step(&mut ai, o).unwrap();
    assert_eq!(ai.pursuit_timer_ms(), 13980);
    assert_eq!(ai.threat(), real(12.));
    let mut o = observation(3, 1000, 2.);
    o.receipts.push(Receipt {
        id: 3,
        event: CombatEvent::Threat {
            target: 2,
            amount: real(7.),
            kind: ThreatKind::DirectDamage,
        },
    });
    step(&mut ai, o).unwrap();
    assert_eq!(ai.pursuit_timer_ms(), 14000);
    assert_eq!(ai.threat(), real(19.));
    let mut passive = SoloCreatureAi::new(profile(Reaction::Passive)).unwrap();
    let mut o = observation(1, 20, 2.);
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::AttackedBy { target: 2 },
    });
    assert!(step(&mut passive, o).unwrap().is_empty());
}

#[test]
fn actual_chase_progress_is_required_never_stationary_or_teleport() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let first = step(&mut ai, observation(1, 100, 10.)).unwrap();
    assert!(first.iter().any(|e| matches!(e, Effect::Motion(_))));
    assert_eq!(ai.position(), point(0.));
    let before = ai.clone();
    assert_eq!(
        step(&mut ai, observation(2, 100, 10.)),
        Err(AiError::Movement(BlockReason::MissingGeometry))
    );
    assert_eq!(ai, before);
    let mut o = observation(2, 1000, 10.);
    o.movement = Some(progress(&ai, 1, 1000, 7.5, 7., 7., false));
    let e = step(&mut ai, o).unwrap();
    assert_eq!(ai.position(), point(7.));
    assert!(e.contains(&Effect::MainhandSwing { target: 2 }));
    let mut o = observation(3, 1, 10.);
    o.movement = Some(progress(&ai, 2, 1, 7., 7., 0., true));
    assert!(!step(&mut ai, o)
        .unwrap()
        .contains(&Effect::MainhandSwing { target: 2 }));
}

#[test]
fn melee_wait_casting_no_catchup_and_range_error_zero_quirk() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    assert!(step(&mut ai, observation(1, 20, 2.))
        .unwrap()
        .contains(&Effect::MainhandSwing { target: 2 }));
    let mut o = observation(2, 2000, 2.);
    o.casting_non_melee = true;
    assert!(!step(&mut ai, o)
        .unwrap()
        .contains(&Effect::MainhandSwing { target: 2 }));
    assert_eq!(ai.mainhand_timer_ms(), 0);
    let e = step(&mut ai, observation(3, 60000, 2.)).unwrap();
    assert_eq!(
        e.iter()
            .filter(|e| matches!(e, Effect::MainhandSwing { .. }))
            .count(),
        1
    );
    assert_eq!(ai.mainhand_timer_ms(), 2000);
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let e = step(&mut ai, observation(1, 20, 10.)).unwrap();
    assert!(e.contains(&Effect::SwingDelayed {
        error: SwingError::OutOfRange,
        retry_ms: 0
    }));
}

#[test]
fn unreachable_ten_seconds_is_cleared_when_reachable() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 20, 10.)).unwrap();
    let mut o = observation(2, 2000, 10.);
    o.movement = Some(blocked(&ai, 1, 2000));
    step(&mut ai, o).unwrap();
    assert_eq!(ai.unreachable_timer_ms(), 8000);
    let mut o = observation(3, 1000, 10.);
    o.movement = Some(progress(&ai, 2, 1000, 7.5, 7., 7., false));
    step(&mut ai, o).unwrap();
    assert_eq!(ai.unreachable_timer_ms(), 0);
    let mut o = observation(4, 10000, 20.);
    o.movement = Some(blocked(&ai, 3, 10000));
    let e = step(&mut ai, o).unwrap();
    assert!(e.contains(&Effect::EvadeStarted(EvadeReason::Unreachable)));
    assert_eq!(ai.phase(), Phase::ReturningHome);
    assert_eq!(ai.threat(), Real::ZERO);
}

#[test]
fn leash_uses_target_bounding_radius_and_waits_for_pursuit_expiry() {
    let mut p = profile(Reaction::Aggressive);
    p.pursuit_ms = 1200;
    let mut ai = SoloCreatureAi::new(p).unwrap();
    step(&mut ai, observation(1, 20, 2.)).unwrap();
    let mut o = observation(2, 1180, 30.5);
    o.movement = Some(blocked(&ai, 1, 1180));
    let e = step(&mut ai, o).unwrap();
    assert!(!e.iter().any(|e| matches!(e, Effect::EvadeStarted(_))));
    assert_eq!(ai.pursuit_timer_ms(), 0);
    let mut o = observation(3, 1200, 30.501);
    o.movement = Some(blocked(&ai, 2, 1200));
    let e = step(&mut ai, o).unwrap();
    assert!(e.contains(&Effect::EvadeStarted(EvadeReason::PursuitLeash)));
}

#[test]
fn home_waits_for_validated_arrival_and_does_not_instantly_heal() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 20, 10.)).unwrap();
    let mut o = observation(2, 1000, 10.);
    o.movement = Some(progress(&ai, 1, 1000, 7.5, 7., 7., false));
    step(&mut ai, o).unwrap();
    let mut o = observation(3, 20, 10.);
    o.target = None;
    let e = step(&mut ai, o).unwrap();
    assert!(e.contains(&Effect::EvadeStarted(EvadeReason::NoTarget)));
    assert_eq!(ai.position(), point(7.));
    assert_eq!(ai.motion().unwrap().home, Some(point(0.)));
    let before = ai.clone();
    let mut o = observation(4, 1000, 10.);
    o.target = None;
    assert!(step(&mut ai, o).is_err());
    assert_eq!(ai, before);
    let mut o = observation(4, 1000, 10.);
    o.target = None;
    o.movement = Some(progress(&ai, 2, 1000, 0., 0., 7., true));
    let e = step(&mut ai, o).unwrap();
    assert!(e.contains(&Effect::HomeReached));
    assert_eq!(ai.phase(), Phase::Idle);
}

#[test]
fn corpse_and_respawn_clocks_start_at_death_not_corpse_removal() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let mut o = observation(1, 20, 2.);
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::Died,
    });
    assert_eq!(step(&mut ai, o).unwrap(), vec![Effect::Died]);
    assert_eq!(ai.phase(), Phase::Corpse);
    assert!(step(&mut ai, observation(2, 2999, 2.)).unwrap().is_empty());
    assert_eq!(
        step(&mut ai, observation(3, 1, 2.)).unwrap(),
        vec![Effect::CorpseRemoved]
    );
    assert!(step(&mut ai, observation(4, 1979, 2.)).unwrap().is_empty());
    assert_eq!(
        step(&mut ai, observation(5, 1, 2.)).unwrap(),
        vec![Effect::Respawned {
            position: point(0.)
        }]
    );
    let mut p = profile(Reaction::Aggressive);
    p.respawn_delay_seconds = 0;
    let mut ai = SoloCreatureAi::new(p).unwrap();
    let mut o = observation(1, 20, 2.);
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::Died,
    });
    step(&mut ai, o).unwrap();
    assert_eq!(
        step(&mut ai, observation(2, 5000, 2.)).unwrap(),
        vec![Effect::CorpseRemoved]
    );
    assert_eq!(ai.phase(), Phase::Despawned);
    assert_eq!(
        step(&mut ai, observation(3, 1, 2.)).unwrap(),
        vec![Effect::Respawned {
            position: point(0.)
        }]
    );
}

#[test]
fn invalid_replayed_or_multiplayer_observations_are_atomic() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let mut o = observation(1, 20, 2.);
    o.receipts.push(Receipt {
        id: 5,
        event: CombatEvent::AttackedBy { target: 2 },
    });
    step(&mut ai, o).unwrap();
    let original = ai.clone();
    let mut o = observation(2, 20, 2.);
    o.receipts.push(Receipt {
        id: 5,
        event: CombatEvent::Threat {
            target: 2,
            amount: real(1.),
            kind: ThreatKind::DirectDamage,
        },
    });
    assert!(step(&mut ai, o).is_err());
    assert_eq!(ai, original);
    let mut o = observation(2, 20, 2.);
    o.target.as_mut().unwrap().id = 3;
    assert_eq!(
        step(&mut ai, o),
        Err(AiError::Unsupported(Unsupported::MultipleThreatTargets))
    );
    assert_eq!(ai, original);
    assert!(step(&mut ai, observation(1, 20, 2.)).is_err());
    assert_eq!(ai, original);
    for feature in [
        Unsupported::ScriptedAi,
        Unsupported::SpellList,
        Unsupported::RandomOrWaypointIdle,
        Unsupported::AssistanceOrLinking,
    ] {
        let mut p = profile(Reaction::Aggressive);
        p.unsupported.push(feature);
        assert_eq!(SoloCreatureAi::new(p), Err(AiError::Unsupported(feature)));
    }
}

#[test]
fn movement_validation_rejects_overspeed_false_arrival_unknowns_and_stationary_claims() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 20, 10.)).unwrap();
    let r = request(&ai, 1, 1000, 7.5);
    for (pos, travel, arrived) in [
        (8., 8., false),
        (7., 6., false),
        (7., 7., true),
        (0., 0., false),
    ] {
        let result = MovementResult::Progress {
            position: point(pos),
            travelled: real(travel),
            arrived,
            checks: checks(),
        };
        assert!(resolve_movement(&mut Geometry { result }, r.clone()).is_err());
    }
    for index in 0..5 {
        let mut c = checks();
        match index {
            0 => c.loaded_coverage = Check::Unknown,
            1 => c.line_of_sight = Check::Unknown,
            2 => c.body_sweep = Check::Unknown,
            3 => c.continuous_ground_support = Check::Unknown,
            _ => c.dry_ground = Check::Unknown,
        }
        let result = MovementResult::Progress {
            position: point(7.),
            travelled: real(7.),
            arrived: false,
            checks: c,
        };
        assert_eq!(
            resolve_movement(&mut Geometry { result }, r.clone()),
            Err(AiError::Movement(BlockReason::MissingGeometry))
        );
    }
    assert!(Real::new(f32::NAN).is_err());
    assert!(Real::new(f32::INFINITY).is_err());
    assert!(Real::from_bits(1).is_err());
    assert_eq!(Real::new(-0.).unwrap(), Real::ZERO);
}

#[test]
fn hard_leash_uses_creature_radius_and_validated_native_motion() {
    let mut p = profile(Reaction::Aggressive);
    p.hard_leash_radius = Some(real(6.));
    let mut ai = SoloCreatureAi::new(p).unwrap();
    step(&mut ai, observation(1, 20, 10.)).unwrap();
    let mut o = observation(2, 1200, 10.);
    o.movement = Some(progress(&ai, 1, 1200, 7.5, 7., 7., false));
    let e = step(&mut ai, o).unwrap();
    assert!(e.contains(&Effect::EvadeStarted(EvadeReason::HardLeash)));
    assert_eq!(ai.position(), point(7.));
    assert!(ai.is_evading());
    assert!(ai.is_evade_regeneration());
}

#[test]
fn neutral_to_all_never_acquires_on_sight_but_can_retaliate() {
    let mut p = profile(Reaction::Aggressive);
    p.neutral_to_all = true;
    let mut ai = SoloCreatureAi::new(p).unwrap();
    assert!(step(&mut ai, observation(1, 20, 2.)).unwrap().is_empty());
    let mut o = observation(2, 20, 2.);
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::AttackedBy { target: 2 },
    });
    assert!(step(&mut ai, o)
        .unwrap()
        .contains(&Effect::MainhandSwing { target: 2 }));
}

#[test]
fn stale_movement_and_unknown_geometry_do_not_change_ai_or_start_evade() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 20, 10.)).unwrap();
    let original = ai.clone();
    for reason in [
        BlockReason::MissingGeometry,
        BlockReason::UnsupportedNavigation,
    ] {
        let token = resolve_movement(
            &mut Geometry {
                result: MovementResult::Blocked(reason),
            },
            request(&ai, 1, 1000, 7.5),
        )
        .unwrap();
        let mut o = observation(2, 1000, 10.);
        o.movement = Some(token);
        assert_eq!(step(&mut ai, o), Err(AiError::Movement(reason)));
        assert_eq!(ai, original);
        assert!(!ai.is_evading());
    }
    let mut r = request(&ai, 1, 1000, 7.5);
    r.geometry_revision = Revision([99; 32]);
    let token = resolve_movement(
        &mut Geometry {
            result: MovementResult::Blocked(BlockReason::Obstacle),
        },
        r,
    )
    .unwrap();
    let mut o = observation(2, 1000, 10.);
    o.movement = Some(token);
    assert_eq!(
        step(&mut ai, o),
        Err(AiError::Movement(BlockReason::StaleGeometry))
    );
    assert_eq!(ai, original);
    let token = blocked(&ai, 1, 1000);
    let mut o = observation(2, 1000, 10.);
    o.movement = Some(token.clone());
    step(&mut ai, o).unwrap();
    let prior = ai.clone();
    let mut o = observation(3, 1000, 10.);
    o.movement = Some(token);
    assert!(step(&mut ai, o).is_err());
    assert_eq!(ai, prior);
}

#[test]
fn wrong_provider_request_map_or_revisions_are_rejected() {
    struct WrongProvider(u8);
    impl GeometryAuthority for WrongProvider {
        fn resolve(&mut self, r: &MovementRequest) -> Result<MovementOutcome, AiError> {
            let mut out = MovementOutcome {
                request_id: r.request_id,
                map_id: r.map_id,
                content_revision: r.content_revision,
                geometry_revision: r.geometry_revision,
                result: MovementResult::Blocked(BlockReason::Obstacle),
            };
            match self.0 {
                0 => out.request_id += 1,
                1 => out.map_id += 1,
                2 => out.content_revision = Revision([99; 32]),
                _ => out.geometry_revision = Revision([99; 32]),
            }
            Ok(out)
        }
    }
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 20, 10.)).unwrap();
    for (which, reason) in [
        (0, BlockReason::StaleRequest),
        (1, BlockReason::WrongMap),
        (2, BlockReason::StaleContent),
        (3, BlockReason::StaleGeometry),
    ] {
        assert_eq!(
            resolve_movement(&mut WrongProvider(which), request(&ai, 1, 1000, 7.5)),
            Err(AiError::Movement(reason))
        );
    }
}

#[test]
fn unchanged_authoritative_trace_produces_identical_states_and_effects() {
    let mut left = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let mut right = left.clone();
    let first = observation(1, 20, 10.);
    assert_eq!(step(&mut left, first.clone()), step(&mut right, first));
    let mut second = observation(2, 1000, 10.);
    second.movement = Some(progress(&left, 1, 1000, 7.5, 7., 7., false));
    assert_eq!(step(&mut left, second.clone()), step(&mut right, second));
    for sequence in 3..103 {
        let mut o = observation(sequence, 20, 10.);
        o.receipts.push(Receipt {
            id: sequence,
            event: CombatEvent::Threat {
                target: 2,
                amount: real(1.5),
                kind: ThreatKind::PeriodicDamage,
            },
        });
        assert_eq!(step(&mut left, o.clone()), step(&mut right, o));
        assert_eq!(left, right);
    }
}

#[test]
fn respawn_uses_shared_whole_seconds_while_corpse_uses_map_milliseconds() {
    let mut p = profile(Reaction::Aggressive);
    p.clock_origin_ms = 1980;
    p.corpse_decay_ms = 0;
    p.respawn_delay_seconds = 1;
    let mut ai = SoloCreatureAi::new(p).unwrap();
    let mut o = observation(1, 19, 2.); // death at 1999ms, respawn due at 2000ms
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::Died,
    });
    step(&mut ai, o).unwrap();
    assert_eq!(
        step(&mut ai, observation(2, 1, 2.)).unwrap(),
        vec![Effect::CorpseRemoved]
    );
    assert_eq!(
        step(&mut ai, observation(3, 1, 2.)).unwrap(),
        vec![Effect::Respawned {
            position: point(0.)
        }]
    );
}

#[test]
fn same_update_stationary_swing_refresh_precedes_leash_expiry() {
    let mut p = profile(Reaction::Aggressive);
    p.pursuit_ms = 1200;
    p.leash_radius = real(0.5);
    let mut ai = SoloCreatureAi::new(p).unwrap();
    let mut first = observation(1, 20, 2.);
    first.casting_non_melee = true;
    step(&mut ai, first).unwrap();
    let before = ai.clone();
    let mut swing = observation(2, 1180, 2.);
    swing.stationary_melee_refresh_eligible = true;
    let effects = step(&mut ai, swing.clone()).unwrap();
    assert!(effects.contains(&Effect::MainhandSwing { target: 2 }));
    assert!(!effects.iter().any(|e| matches!(e, Effect::EvadeStarted(_))));
    assert_eq!(ai.pursuit_timer_ms(), 20);
    let mut missed_refresh = before;
    swing.stationary_melee_refresh_eligible = false;
    assert!(step(&mut missed_refresh, swing)
        .unwrap()
        .contains(&Effect::EvadeStarted(EvadeReason::PursuitLeash)));
}

#[test]
fn attack_timer_ticks_during_corpse_and_is_not_reset_at_respawn() {
    let mut p = profile(Reaction::Aggressive);
    p.corpse_decay_ms = 20;
    p.respawn_delay_seconds = 0;
    let mut ai = SoloCreatureAi::new(p).unwrap();
    step(&mut ai, observation(1, 20, 2.)).unwrap();
    let mut death = observation(2, 20, 2.);
    death.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::Died,
    });
    step(&mut ai, death).unwrap();
    assert_eq!(ai.mainhand_timer_ms(), 1980);
    step(&mut ai, observation(3, 20, 2.)).unwrap();
    assert_eq!(ai.mainhand_timer_ms(), 1960);
    step(&mut ai, observation(4, 20, 2.)).unwrap();
    assert_eq!(ai.phase(), Phase::Idle);
    assert_eq!(ai.mainhand_timer_ms(), 1960);
}

#[test]
fn source_long_distance_home_branch_is_explicitly_unsupported_at_strict_boundary() {
    for distance in [150.0f32, f32::from_bits(150.0f32.to_bits() + 1)] {
        let mut p = profile(Reaction::Aggressive);
        p.pursuit_ms = u32::MAX;
        let mut ai = SoloCreatureAi::new(p).unwrap();
        step(&mut ai, observation(1, 20, 10.)).unwrap();
        let mut o = observation(2, 30000, distance + 2.);
        o.movement = Some(progress(&ai, 1, 30000, distance, distance, distance, true));
        step(&mut ai, o).unwrap();
        let before = ai.clone();
        let mut o = observation(3, 20, distance + 2.);
        o.target = None;
        if distance == 150. {
            assert!(step(&mut ai, o)
                .unwrap()
                .contains(&Effect::EvadeStarted(EvadeReason::NoTarget)));
        } else {
            assert_eq!(
                step(&mut ai, o),
                Err(AiError::Unsupported(Unsupported::LongDistanceHomeDespawn))
            );
            assert_eq!(ai, before);
        }
    }
}

#[test]
fn interaction_distance_uses_source_strict_three_dimensional_reach_sum() {
    assert!(!within_interaction_distance(point(0.), point(8.), real(1.5), real(1.5)).unwrap());
    assert!(within_interaction_distance(
        point(0.),
        point(f32::from_bits(8f32.to_bits() - 1)),
        real(1.5),
        real(1.5)
    )
    .unwrap());
    assert!(!within_interaction_distance(
        point(0.),
        Point::new(0., 0., 8.).unwrap(),
        real(1.5),
        real(1.5)
    )
    .unwrap());
    assert!(within_interaction_distance(point(0.), point(0.), real(-1.), real(1.5)).is_err());
}

#[test]
fn source_sight_and_resisted_retaliation_own_zero_reference_before_chase() {
    for sight in [false, true] {
        let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
        let mut o = observation(1, 100, 2.);
        let effects = if sight {
            ai.source_sight_event(&o).unwrap()
        } else {
            o.receipts.push(Receipt {
                id: 1,
                event: CombatEvent::AttackedBy { target: 2 },
            });
            ai.source_combat_event(&o).unwrap()
        };
        let reference = ai.hostile_reference().unwrap().clone();
        assert_eq!(reference.target_guid(), 2);
        assert!(reference.valid() && reference.online());
        assert_eq!(reference.threat(), Real::ZERO);
        assert_eq!(ai.current_victim(), None);
        assert_eq!(serde_json::to_value(ai.checkpoint()).unwrap()["now_ms"], 0);
        assert_eq!(effects[0], Effect::CombatStarted { target: 2 });
        assert!(matches!(effects[1], Effect::Motion(_)));
        let mut timer = observation(1, 100, 2.);
        timer.receipts.push(Receipt {
            id: 2,
            event: CombatEvent::CombatTimerTriggered { target: 2 },
        });
        ai.source_combat_event(&timer).unwrap();
        assert_eq!(ai.hostile_reference().unwrap(), &reference);
    }
    let mut idle = SoloCreatureAi::new(profile(Reaction::Defensive)).unwrap();
    let mut timer = observation(1, 100, 2.);
    timer.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::CombatTimerTriggered { target: 2 },
    });
    assert!(idle.source_combat_event(&timer).is_err());
    assert!(idle.hostile_reference().is_none());
}

#[test]
fn online_callback_preserves_owned_threat_target_chase_and_timers_without_selecting() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let mut o = observation(1, 100, 2.);
    o.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::Threat {
            target: 2,
            amount: real(7.),
            kind: ThreatKind::DirectDamage,
        },
    });
    step(&mut ai, o).unwrap();
    assert_eq!(ai.current_victim(), Some(2));
    let before = ai.checkpoint();
    assert!(ai.update_online_status(2, Some(2), true).unwrap());
    assert!(!ai.hostile_reference().unwrap().online());
    assert_eq!(ai.current_victim(), None);
    assert_eq!(ai.threat(), real(7.));
    assert_eq!(ai.target(), Some(2));
    assert_eq!(
        ai.motion().unwrap().purpose,
        MovementPurpose::Chase { target: 2 }
    );
    assert_eq!(ai.mainhand_timer_ms(), 2000);
    assert_eq!(
        serde_json::to_value(ai.checkpoint()).unwrap()["now_ms"],
        serde_json::to_value(&before).unwrap()["now_ms"]
    );
    assert_eq!(
        serde_json::to_value(ai.checkpoint()).unwrap()["sequence"],
        serde_json::to_value(&before).unwrap()["sequence"]
    );
    assert!(!ai.update_online_status(2, None, true).unwrap());
    assert!(ai.update_online_status(2, None, false).unwrap());
    assert_eq!(ai.current_victim(), None);
    let restored =
        SoloCreatureAi::restore(ai.checkpoint(), profile(Reaction::Aggressive), 100, 1, 2).unwrap();
    assert_eq!(restored, ai);
    step(&mut ai, observation(2, 100, 2.)).unwrap();
    assert_eq!(ai.current_victim(), Some(2));
}

#[test]
fn owned_reference_checkpoint_requires_fields_and_rejects_phase_link_current_forgery() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 100, 2.)).unwrap();
    let value = serde_json::to_value(ai.checkpoint()).unwrap();
    for path in ["hostile_references", "reference", "current_victim", "dirty"] {
        let mut forged = value.clone();
        if path == "hostile_references" {
            forged.as_object_mut().unwrap().remove(path);
        } else {
            forged["hostile_references"]
                .as_object_mut()
                .unwrap()
                .remove(path);
        }
        assert!(
            serde_json::from_value::<AiCheckpoint>(forged).is_err(),
            "{path}"
        );
    }
    for case in [
        "old",
        "missing",
        "wrong_target",
        "invalid_online",
        "offline_current",
        "detached",
        "idle",
    ] {
        let mut forged = value.clone();
        match case {
            "old" => forged["version"] = 4.into(),
            "missing" => forged["hostile_references"]["reference"] = serde_json::Value::Null,
            "wrong_target" => forged["hostile_references"]["reference"]["target_guid"] = 3.into(),
            "invalid_online" => forged["hostile_references"]["reference"]["valid"] = false.into(),
            "offline_current" => {
                forged["hostile_references"]["reference"]["membership"] = "Offline".into()
            }
            "detached" => forged["victim_detached_at_ms"] = 100.into(),
            "idle" => forged["phase"] = "Idle".into(),
            _ => unreachable!(),
        }
        let checkpoint = serde_json::from_value(forged).unwrap();
        assert!(
            SoloCreatureAi::restore(checkpoint, profile(Reaction::Aggressive), 100, 1, 2).is_err(),
            "{case}"
        );
    }
}

#[test]
fn lethal_hook_deletes_owned_reference_and_current_before_return_and_restore() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let o = observation(1, 100, 2.);
    let token = ai.begin_receipts(&o).unwrap();
    ai.finish_receipts(o, token, &mut |_, _, _| Ok(MainhandResult::TargetDied))
        .unwrap();
    assert_eq!(ai.phase(), Phase::Combat);
    assert_eq!(ai.target(), Some(2));
    assert!(ai.hostile_reference().is_none());
    assert_eq!(ai.current_victim(), None);
    assert_eq!(ai.victim_detached_at_ms(), Some(100));
    assert_eq!(ai.threat(), Real::ZERO);
    assert_eq!(
        SoloCreatureAi::restore(ai.checkpoint(), profile(Reaction::Aggressive), 100, 1, 2).unwrap(),
        ai
    );
    assert_eq!(
        serde_json::to_value(ai.checkpoint()).unwrap()["hostile_references"]["dirty"],
        true
    );
    step(&mut ai, observation(2, 100, 2.)).unwrap();
    assert_eq!(ai.phase(), Phase::ReturningHome);
    assert_eq!(
        serde_json::to_value(ai.checkpoint()).unwrap()["hostile_references"]["dirty"],
        true
    );
}

#[test]
fn independent_owned_online_callbacks_commute_without_observers_or_shared_mutation() {
    let mut a = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    let mut p = profile(Reaction::Aggressive);
    p.creature_id = 9;
    let mut b = SoloCreatureAi::new(p).unwrap();
    let o = observation(1, 100, 2.);
    a.source_sight_event(&o).unwrap();
    b.source_sight_event(&o).unwrap();
    let initial = [a, b];
    for offline in [false, true] {
        let mut forward = initial.clone();
        let mut reverse = initial.clone();
        for index in [0, 1] {
            forward[index]
                .update_online_status(2, Some(2), offline)
                .unwrap();
        }
        for index in [1, 0] {
            reverse[index]
                .update_online_status(2, Some(2), offline)
                .unwrap();
        }
        assert_eq!(forward, reverse);
        for ai in &forward {
            assert_eq!(ai.target(), Some(2));
            assert_eq!(ai.current_victim(), None);
            assert_eq!(ai.hostile_reference().unwrap().online(), !offline);
        }
    }
}

#[test]
fn acquired_reference_is_deleted_by_death_and_not_recreated_by_corpse_or_respawn() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 20, 2.)).unwrap();
    assert!(ai.hostile_reference().is_some());
    let mut death = observation(2, 20, 2.);
    death.receipts.push(Receipt {
        id: 1,
        event: CombatEvent::Died,
    });
    step(&mut ai, death).unwrap();
    for (sequence, elapsed, phase) in [(3, 3000, Phase::Despawned), (4, 2000, Phase::Idle)] {
        assert!(ai.hostile_reference().is_none());
        assert_eq!(ai.current_victim(), None);
        step(&mut ai, observation(sequence, elapsed, 2.)).unwrap();
        assert_eq!(ai.phase(), phase);
        assert!(ai.hostile_reference().is_none());
        assert_eq!(ai.threat(), Real::ZERO);
    }
}

#[test]
fn online_callback_rejects_wrong_target_or_receipt_interlude_transactionally() {
    let mut ai = SoloCreatureAi::new(profile(Reaction::Aggressive)).unwrap();
    step(&mut ai, observation(1, 100, 2.)).unwrap();
    let before = ai.clone();
    assert!(ai.update_online_status(3, Some(3), true).is_err());
    assert_eq!(ai, before);
    assert!(ai.update_online_status(2, Some(3), true).is_err());
    assert_eq!(ai, before);
    let o = observation(2, 100, 2.);
    let token = ai.begin_source_receipts(&o, false).unwrap();
    let prepared = ai.clone();
    assert!(ai.update_online_status(2, Some(2), true).is_err());
    assert_eq!(ai, prepared);
    ai.finish_receipts(o, token, &mut |_, _, _| Ok(MainhandResult::Unchanged))
        .unwrap();
}
