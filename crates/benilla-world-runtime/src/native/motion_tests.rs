// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::creature_motion as m;
fn step(w: &mut NativeWorld, a: &mut fixture::SyntheticMotionAuthority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn input(w: &NativeWorld, command: Command) -> OwnedInput {
    OwnedInput {
        authenticated_owner: w.state().player.owner,
        input: Input {
            world: w.state().world,
            tick: w.state().tick + 1,
            player: w.state().player,
            sequence: w.state().next_sequence,
            command,
        },
    }
}
#[test]
fn source_notify_launch_progress_home_and_reopen_have_exact_clocks_and_poses() {
    let mut p = fixture::profile();
    p.npcs[0].ai.hard_leash_radius = Some(f::real(5.0));
    let mut w =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Idle);
    assert!(!w.state().npcs[&2].events.can_aggro());
    step(&mut w, &mut a);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    let motion = &w.state().npcs[&2].source_motion.as_ref().unwrap().motion;
    assert_eq!(motion.now_ms(), 200);
    assert_eq!(motion.snapshot().spline.unwrap().spline.elapsed_ms, 100);
    assert!(motion.pose().position.x.as_f32() < 16.0);
    assert!(motion.pose().position.x.as_f32() > 15.0);
    let save = w.checkpoint().unwrap();
    let mut reopened = NativeWorld::restore_synthetic_component_test(p.clone(), save).unwrap();
    let mut b = fixture::SyntheticMotionAuthority::default();
    for _ in 0..90 {
        step(&mut w, &mut a);
        step(&mut reopened, &mut b);
        assert_eq!(w.state_digest(), reopened.state_digest());
        assert_eq!(w.last_tick(), reopened.last_tick());
    }
    assert!(w.records().iter().flat_map(|r| &r.events).any(|e| matches!(
        e,
        Event::NpcMotion {
            event: m::Event::HomeReached { .. },
            ..
        }
    )));
    let mut replay = NativeWorld::restore_synthetic_component_test(p, initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn source_missing_visitor_or_partial_route_rolls_back_inputs_clocks_and_rng() {
    let mut w = fixture::world();
    let before = w.state_digest();
    let mut a = fixture::SyntheticMotionAuthority {
        missing: true,
        ..Default::default()
    };
    let i = input(
        &w,
        Command::MovementPose {
            destination: w.state().position,
            orientation: Orientation::new(0.25).unwrap(),
            movement_flags: m::GroundMovementFlags::from_bits(1).unwrap(),
        },
    );
    assert!(w.step(1, Some(i), &mut a).is_err());
    assert_eq!(w.state_digest(), before);
    assert!(w.records().is_empty());
    a.missing = false;
    step(&mut w, &mut a);
    a.truncated = true;
    let before = w.state_digest();
    assert!(w.step(2, None, &mut a).is_err());
    assert_eq!(w.state_digest(), before);
}
#[test]
fn sequenced_flags_survive_stationary_turn_stop_and_reopening() {
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    for bits in [1, 0x101, 0x10, 0] {
        let flags = m::GroundMovementFlags::from_bits(bits).unwrap();
        let i = input(
            &w,
            Command::MovementPose {
                destination: w.state().position,
                orientation: Orientation::new(0.5).unwrap(),
                movement_flags: flags,
            },
        );
        w.step(w.state().tick + 1, Some(i), &mut a).unwrap();
        assert_eq!(w.state().player_movement_flags, Some(flags));
        let copy = NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            w.checkpoint().unwrap(),
        )
        .unwrap();
        assert_eq!(copy.state_digest(), w.state_digest());
    }
}
#[test]
fn source_initial_policy_rejects_unknown_current_domain_and_legacy_cannot_claim_it() {
    let mut p = fixture::profile();
    p.source_motion
        .as_mut()
        .unwrap()
        .npc_policy
        .audit
        .speed_auras = m::Evidence::Unknown;
    assert!(NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 0).is_err());
    let mut w = f::world();
    let i = input(
        &w,
        Command::MovementPose {
            destination: w.state().position,
            orientation: w.state().orientation,
            movement_flags: m::GroundMovementFlags::STOPPED,
        },
    );
    assert!(w
        .step(1, Some(i), &mut f::SyntheticAuthority::default())
        .is_err());
}

#[test]
fn source_co_due_player_notify_and_cast_share_the_source_queue() {
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority::default();
    let cast = f::cast(&w, 60001);
    w.step(1, Some(cast), &mut a).unwrap();
    assert_eq!(w.state().casts.len(), 1);
    assert_eq!(w.state().player_events.queue_snapshot().queued.len(), 1);
    assert_eq!(w.state().player_events.clock_ms(), 100);
    let before = w.state_digest();
    for command in [
        Command::Move {
            destination: f::point(1.0),
        },
        Command::Pose {
            destination: f::point(0.0),
            orientation: Orientation::new(1.0).unwrap(),
        },
    ] {
        let i = input(&w, command);
        assert!(matches!(
            w.step(2, Some(i), &mut a),
            Err(NativeError::InvalidInput)
        ));
        assert_eq!(w.state_digest(), before);
    }
}

fn damage_profile(health: u32, direct: i32, periodic: Option<i32>) -> Profile {
    let mut p = fixture::profile();
    p.npcs[0].max_health = health;
    p.npcs[0].ai.reaction = ai::Reaction::Defensive;
    p.npcs[0].ai.corpse_decay_ms = 0;
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        panic!()
    };
    let spell::MageEffect::SchoolDamage(mut value) = spec.effects[0] else {
        panic!()
    };
    value.base_points = direct - 1;
    value.base_dice = 1;
    value.die_sides = 1;
    spec.effects = [
        spell::MageEffect::SchoolDamage(value),
        periodic
            .map(|amount| {
                let mut dot = value;
                dot.base_points = amount - 1;
                spell::MageEffect::PeriodicDamage {
                    value: dot,
                    amplitude_ms: 500,
                    duration_ms: 2000,
                }
            })
            .unwrap_or(spell::MageEffect::Empty),
        spell::MageEffect::Empty,
    ];
    p
}
fn cast_after_initial_notify(p: Profile) -> (NativeWorld, fixture::SyntheticMotionAuthority) {
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
    let mut a = fixture::SyntheticMotionAuthority {
        critical_chance: Some(0),
        ..Default::default()
    };
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let i = f::cast(&w, 60001);
    w.step(3, Some(i), &mut a).unwrap();
    (w, a)
}
#[test]
fn hostile_hit_launch_uses_pre_damage_health_and_replan_uses_current_wounded_health() {
    let (mut w, mut a) = cast_after_initial_notify(damage_profile(1000, 800, None));
    for _ in 0..100 {
        step(&mut w, &mut a);
        let record = w.last_tick().unwrap();
        if let Some((launch_index, velocity)) =
            record.events.iter().enumerate().find_map(|(i, e)| match e {
                Event::NpcMotion {
                    event: m::Event::SplineLaunched { velocity, .. },
                    ..
                } => Some((i, *velocity)),
                _ => None,
            })
        {
            let damage_index = record
                .events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        Event::Spell {
                            event: spell::SpellEvent::DirectDamage { .. },
                            ..
                        }
                    )
                })
                .unwrap();
            assert!(launch_index < damage_index);
            assert_eq!(velocity, w.profile().npcs[0].ai.run_speed);
            assert_eq!(w.state().npcs[&2].health, 200);
            assert!(record.replies.iter().any(|r| matches!(&r.request.query,
                Query::SourceMotion(MotionQuery::HostileSpellVisibility(q)) if q.creature_health == 1000 && q.success)));
            assert!(w.state().npcs[&2].ai.position().x.as_f32() < 16.0);
            let reopened = NativeWorld::restore_synthetic_component_test(
                w.profile().clone(),
                w.checkpoint().unwrap(),
            )
            .unwrap();
            assert_eq!(reopened.state_digest(), w.state_digest());
            let i = input(
                &w,
                Command::MovementPose {
                    destination: f::point(-0.7),
                    orientation: w.state().orientation,
                    movement_flags: m::GroundMovementFlags::from_bits(1).unwrap(),
                },
            );
            w.step(w.state().tick + 1, Some(i), &mut a).unwrap();
            for _ in 0..10 {
                if let Some(velocity) = w.last_tick().unwrap().events.iter().find_map(|e| match e {
                    Event::NpcMotion {
                        event: m::Event::SplineLaunched { velocity, .. },
                        ..
                    } => Some(*velocity),
                    _ => None,
                }) {
                    assert_eq!(
                        velocity,
                        m::chase_speed(w.profile().npcs[0].ai.run_speed, false, 200, 1000, false)
                            .unwrap()
                    );
                    assert_ne!(velocity, w.profile().npcs[0].ai.run_speed);
                    return;
                }
                let i = input(
                    &w,
                    Command::MovementPose {
                        destination: f::point(w.state().position.x.as_f32() - 0.5),
                        orientation: w.state().orientation,
                        movement_flags: m::GroundMovementFlags::from_bits(1).unwrap(),
                    },
                );
                w.step(w.state().tick + 1, Some(i), &mut a).unwrap();
            }
            panic!("no current-health replan");
        }
    }
    panic!("no source chase after hit");
}
#[test]
fn lethal_first_hit_launches_before_damage_then_stops_and_removes_zero_delay_corpse() {
    let (mut w, mut a) = cast_after_initial_notify(damage_profile(10, 100, None));
    while w.state().npcs[&2].health != 0 {
        assert!(w.state().tick < 100);
        step(&mut w, &mut a);
    }
    let record = w.last_tick().unwrap();
    let index = |predicate: fn(&Event) -> bool| record.events.iter().position(predicate).unwrap();
    let launch = index(|e| {
        matches!(
            e,
            Event::NpcMotion {
                event: m::Event::SplineLaunched { .. },
                ..
            }
        )
    });
    let damage = index(|e| {
        matches!(
            e,
            Event::Spell {
                event: spell::SpellEvent::DirectDamage { .. },
                ..
            }
        )
    });
    let stop = index(|e| {
        matches!(
            e,
            Event::NpcMotion {
                event: m::Event::SplineInterrupted { .. },
                ..
            }
        )
    });
    let removed = index(|e| {
        matches!(
            e,
            Event::Npc {
                effect: ai::Effect::CorpseRemoved,
                ..
            }
        )
    });
    assert!(launch < damage && damage < stop && stop < removed);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Despawned);
    NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap();
}
#[test]
fn resisted_hostile_spell_can_launch_chase_without_damage_and_unknown_detection_rolls_back() {
    for visibility in [ai::movement::Check::Passed, ai::movement::Check::Unknown] {
        let p = damage_profile(1000, 800, None);
        let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
        let mut a = fixture::SyntheticMotionAuthority {
            resist_chance: Some(10000),
            hostile_visibility: Some(visibility),
            ..Default::default()
        };
        step(&mut w, &mut a);
        step(&mut w, &mut a);
        let i = f::cast(&w, 60001);
        w.step(3, Some(i), &mut a).unwrap();
        loop {
            assert!(w.state().tick < 100);
            let before = w.state_digest();
            let result = w.step(w.state().tick + 1, None, &mut a);
            if visibility == ai::movement::Check::Unknown && result.is_err() {
                assert_eq!(w.state_digest(), before);
                break;
            }
            result.unwrap();
            if w.last_tick().unwrap().events.iter().any(|e| {
                matches!(
                    e,
                    Event::Spell {
                        event: spell::SpellEvent::Resisted,
                        ..
                    }
                )
            }) {
                assert_eq!(visibility, ai::movement::Check::Passed);
                assert_eq!(w.state().npcs[&2].health, 1000);
                let reference = w.state().npcs[&2].ai.hostile_reference().unwrap();
                assert_eq!(reference.target_guid(), SOLO_UNIT);
                assert!(reference.valid() && reference.online());
                assert_eq!(reference.threat(), ai::Real::ZERO);
                assert_eq!(w.state().npcs[&2].ai.target(), Some(SOLO_UNIT));
                let restored = NativeWorld::restore_synthetic_component_test(
                    w.profile().clone(),
                    w.checkpoint().unwrap(),
                )
                .unwrap();
                assert_eq!(restored.state_digest(), w.state_digest());
                assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
                    e,
                    Event::NpcMotion {
                        event: m::Event::SplineLaunched { .. },
                        ..
                    }
                )));
                break;
            }
        }
    }
}
#[test]
fn player_phase_and_dot_phase_zero_corpse_deaths_reopen_with_distinct_deferral() {
    for (p, expected) in [
        (damage_profile(10, 100, None), ai::Phase::Despawned),
        (damage_profile(3, 1, Some(3)), ai::Phase::Corpse),
    ] {
        let (mut w, mut a) = cast_after_initial_notify(p);
        let mut killed = false;
        for _ in 0..150 {
            step(&mut w, &mut a);
            if w.last_tick().unwrap().events.iter().any(|e| {
                matches!(
                    e,
                    Event::Npc {
                        effect: ai::Effect::Died,
                        ..
                    }
                )
            }) {
                assert_eq!(w.state().npcs[&2].ai.phase(), expected);
                let reopened = NativeWorld::restore_synthetic_component_test(
                    w.profile().clone(),
                    w.checkpoint().unwrap(),
                )
                .unwrap();
                assert_eq!(reopened.state_digest(), w.state_digest());
                if expected == ai::Phase::Corpse {
                    step(&mut w, &mut a);
                    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Despawned);
                }
                killed = true;
                break;
            }
        }
        assert!(killed);
        let frozen = w.state().npcs[&2].events.clock_ms();
        while w.state().npcs[&2].id.generation == 0 {
            step(&mut w, &mut a);
            assert_eq!(w.state().npcs[&2].events.clock_ms(), frozen);
        }
        assert!(!w.state().npcs[&2].events.can_aggro());
        let restored = NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            w.checkpoint().unwrap(),
        )
        .unwrap();
        assert_eq!(restored.state_digest(), w.state_digest());
    }
}
#[test]
fn source_set_in_front_precedes_native_melee_and_does_not_relocate() {
    struct Combined {
        spatial: fixture::SyntheticMotionAuthority,
        melee: f::KernelAuthority,
        bearing: ai::Real,
    }
    impl Authority for Combined {
        fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if let Query::SourceMotion(MotionQuery::Spatial(query @ m::Query::Bearing(_))) =
                &r.query
            {
                return Ok(AuthorityReply {
                    request: r.clone(),
                    facts: Facts::SourceMotion(MotionFacts::Spatial(Box::new(m::Reply {
                        request: query.clone(),
                        fact: m::Fact::Bearing(self.bearing),
                    }))),
                });
            }
            if matches!(r.query, Query::NativeMelee { .. }) {
                self.melee.resolve(r)
            } else {
                self.spatial.resolve(r)
            }
        }
    }
    // This test verifies callback order and exact propagation of a supplied spatial
    // fact. Host atan2 is deliberately outside the deterministic runtime contract.
    // Exercise both adjacent values observed on native CI without an epsilon.
    for bearing_bits in [0x4049_0fda, 0x4049_0fdb] {
        let bearing = ai::Real::from_bits(bearing_bits).unwrap();
        let mut p = f::initialized_profile();
        p.npcs[0].ai.spawn = f::point(2.0);
        p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
        let mut g = fixture::genesis(&p);
        g.creature_initial_aggro_delay_ms = 0;
        p.source_motion = Some(g);
        let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
        let mut a = Combined {
            spatial: Default::default(),
            melee: Default::default(),
            bearing,
        };
        w.step(1, None, &mut a).unwrap();
        let record = w.last_tick().unwrap();
        let face = record
            .replies
            .iter()
            .position(|r| {
                matches!(
                    r.request.query,
                    Query::SourceMotion(MotionQuery::Spatial(m::Query::Bearing(_)))
                )
            })
            .unwrap();
        let melee = record
            .replies
            .iter()
            .position(|r| {
                matches!(
                    r.request.query,
                    Query::NativeMelee {
                        npc_orientation: Some(_),
                        ..
                    }
                )
            })
            .unwrap();
        assert!(face < melee);
        let source = &w.state().npcs[&2].source_motion.as_ref().unwrap().motion;
        assert_eq!(source.last_move_ms(), 0);
        assert_eq!(source.pose().position, f::point(2.0));
        assert_eq!(source.pose().orientation, bearing);
        let Query::NativeMelee {
            npc_orientation: Some(orientation),
            ..
        } = record.replies[melee].request.query
        else {
            panic!("missing ordered melee orientation")
        };
        assert_eq!(orientation.bits(), bearing.bits());
    }
}

fn altered_checkpoint(w: &NativeWorld, mutate: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut checkpoint = w.checkpoint().unwrap();
    let mut value = serde_json::to_value(&checkpoint.state).unwrap();
    mutate(&mut value);
    checkpoint.state = serde_json::from_value(value).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v29\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v29\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    checkpoint
}
#[test]
fn source_restore_rejects_rehashed_phase_clock_flags_and_notify_claims() {
    let mut p = fixture::profile();
    p.npcs[0].ai.hard_leash_radius = Some(f::real(5.0));
    p.source_motion.as_mut().unwrap().npc_spawn_orientation =
        Orientation::new(-std::f32::consts::TAU).unwrap();
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    for kind in 0..6 {
        let forged = altered_checkpoint(&w, |s| match kind {
            0 => s["npcs"]["2"]["source_motion"]["combat_start"] = serde_json::Value::Null,
            1 => s["npcs"]["2"]["source_motion"]["motion"]["processed_map_clock_ms"] = 100.into(),
            2 => s["player_movement_flags"] = serde_json::Value::Null,
            3 => s["player_events"]["queue"]["clock_ms"] = 0.into(),
            4 => s["npcs"]["2"]["events"]["notify"] = 10_000.into(),
            5 => s["player_events"]["notify"] = 10_000_000.into(),
            _ => unreachable!(),
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err()
        );
    }
    while w.state().npcs[&2].ai.phase() != ai::Phase::ReturningHome {
        assert!(w.state().tick < 100);
        step(&mut w, &mut a);
    }
    let forged = altered_checkpoint(&w, |s| {
        s["npcs"]["2"]["source_motion"]["active"] = false.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
    // Home canonicalizes the saved TAU facing to zero; that valid pose must reopen.
    NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap();
    let forged = altered_checkpoint(&w, |s| {
        let motion = &mut s["npcs"]["2"]["source_motion"]["motion"];
        motion["generator"]["Home"]["destination"]["orientation"] = serde_json::json!(f::real(0.5));
        motion["spline"]["facing"]["Angle"] = serde_json::json!(f::real(0.5));
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
}
#[test]
fn legacy_restore_rejects_a_rehashed_source_only_death_phase() {
    let mut p = damage_profile(10, 100, None);
    p.source_motion = None;
    p.npcs[0].ai.corpse_decay_ms = 1000;
    let (mut w, mut a) = cast_after_initial_notify(p);
    while w.state().npcs[&2].health != 0 {
        assert!(w.state().tick < 100);
        step(&mut w, &mut a);
    }
    NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap();
    let forged = altered_checkpoint(&w, |s| {
        s["npcs"]["2"]["ai"]["death_deferred"] = false.into();
    });
    assert!(matches!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged),
        Err(NativeError::InvalidCheckpoint)
    ));
}

#[test]
fn source_projectile_impact_uses_private_evade_state_and_rolls_back() {
    let mut p = fixture::profile();
    p.npcs[0].max_health = 1000;
    p.npcs[0].ai.hard_leash_radius = Some(f::real(5.0));
    let (mut w, mut a) = cast_after_initial_notify(p);
    loop {
        assert!(w.state().tick < 100);
        let before = w.state_digest();
        let health = w.state().npcs[&2].health;
        match w.step(w.state().tick + 1, None, &mut a) {
            Err(NativeError::UnsupportedMechanic(
                "source projectile release/impact during evade",
            )) => {
                assert!(w.state().npcs[&2].ai.is_evading());
                assert_eq!(w.state_digest(), before);
                assert_eq!(w.state().npcs[&2].health, health);
                break;
            }
            result => {
                result.unwrap();
            }
        }
    }
}
#[test]
fn source_raw_spell_metadata_rejects_unknown_targets_and_damage_interrupted_holders() {
    for bit in [1, 2, 0x0100_0000] {
        let mut p = fixture::profile();
        p.source_motion
            .as_mut()
            .unwrap()
            .spell_hits
            .get_mut(&60001)
            .unwrap()
            .aura_interrupt_flags = bit;
        assert!(p.validate().is_err());
    }
    let mut p = fixture::profile();
    p.source_motion
        .as_mut()
        .unwrap()
        .spell_hits
        .get_mut(&60001)
        .unwrap()
        .implicit_target_a[0] = 0;
    assert!(p.validate().is_err());
}

#[test]
fn source_initial_player_passives_keep_raw_interrupt_flags_and_reject_unported_removal() {
    use benilla_sim::starter_init as init;
    for interrupt_flags in [0, 2, 0x0100_0000, 4, 0x0001_0000, 8, 16, 0x8000_0000] {
        let mut input = f::initialized_starter_input();
        let mut words = [0; 173];
        words[0] = 60003;
        words[6] = 0x40;
        words[22] = interrupt_flags;
        words[58] = u32::MAX;
        words[61] = 6;
        words[64] = 1;
        words[67] = 1;
        words[76] = 4;
        words[82] = 1;
        words[91] = 29;
        words[106] = 3;
        input.spells.push(init::SpellRow {
            id: 60003,
            words,
            duration_ms: Some(-1),
            previous_rank: None,
            talent: false,
            learned_spells: init::Rows::Complete(vec![]),
            external_initial_hooks: false,
        });
        let init::Rows::Complete(defaults) = &mut input.default_spells else {
            unreachable!()
        };
        defaults.push(60003);
        let mut p = f::initialized_profile();
        p.initialize_player(&input, f::float(100.0), 1).unwrap();
        assert_eq!(
            p.combat.as_ref().unwrap().initial_auras[0].aura_interrupt_flags,
            interrupt_flags
        );
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
        p.source_motion = Some(fixture::genesis(&p));
        if interrupt_flags == 0 {
            NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
        } else {
            assert_eq!(
                p.validate(),
                Err(NativeError::UnsupportedMechanic(
                    "source motion interruptible initial player auras"
                ))
            );
        }
    }
}
