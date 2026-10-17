// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored current-pose integration fixtures, never native content admission.
use super::{
    synthetic_combat_fixture as combat_fixture, synthetic_fixture as f,
    synthetic_spell_fixture as spell_fixture, *,
};
use ai::movement::Check;
use benilla_sim::creature_motion as motion;

fn selected(mut profile: Profile) -> Profile {
    profile.initialize_facing_authority().unwrap();
    profile
}
fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}
fn inputs(w: &NativeWorld, commands: Vec<Command>) -> Vec<OwnedInput> {
    commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| OwnedInput {
            authenticated_owner: w.state.player.owner,
            input: Input {
                world: w.state.world,
                tick: w.state.tick + 1,
                player: w.state.player,
                sequence: w.state.next_sequence + ordinal as u64,
                command,
            },
        })
        .collect()
}
fn turn(w: &NativeWorld, radians: f32) -> Command {
    Command::MovementPose {
        destination: w.state.position,
        orientation: Orientation::new(radians).unwrap(),
        movement_flags: w.state.player_movement_flags.unwrap(),
    }
}
fn kind(query: &Query) -> Option<usize> {
    match query {
        Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(_))) => Some(0),
        Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(_))) => Some(1),
        Query::SourceSpell(SourceSpellQuery::Facing(_)) => Some(2),
        _ => None,
    }
}

#[test]
fn facing_selection_is_explicit_profile_bound_preserved_and_requires_source_inputs() {
    let mut p = spell_fixture::profile();
    let legacy = p.clone();
    let before = p.digest();
    assert!(!p.source_facing_authority());
    p.initialize_facing_authority().unwrap();
    assert!(p.source_facing_authority());
    assert_ne!(p.digest(), before);
    let selected_digest = p.digest();
    let policy = serde_json::to_value(&p).unwrap();
    assert_eq!(
        policy["interactions"]["facing_authority"]["algorithm"],
        benilla_sim::facing::POLICY
    );
    assert_eq!(
        policy["interactions"]["facing_authority"]["source_sha256"],
        serde_json::json!(benilla_sim::facing::SOURCE_SHA256)
    );
    p.initialize_interactions(combat_fixture::source_rows(&p))
        .unwrap();
    assert!(p.source_facing_authority());
    assert_eq!(p.digest(), selected_digest);
    p.initialize_facing_authority().unwrap();
    assert_eq!(p.digest(), selected_digest);
    let mut invalid_rows = combat_fixture::source_rows(&p);
    invalid_rows.audit.external_state_mutations = InteractionEvidence::Unknown;
    assert!(p.initialize_interactions(invalid_rows).is_err());
    assert!(p.source_facing_authority());
    assert_eq!(p.digest(), selected_digest);
    assert!(NativeWorld::restore_synthetic_component_test(
        p.clone(),
        world(legacy).checkpoint().unwrap()
    )
    .is_err());

    let mut no_interactions = f::initialized_profile();
    no_interactions.source_motion = Some(synthetic_motion_fixture::genesis(&no_interactions, 2));
    let before = no_interactions.clone();
    assert!(no_interactions.initialize_facing_authority().is_err());
    assert_eq!(no_interactions, before);
    let mut no_motion = f::initialized_profile();
    no_motion
        .initialize_interactions(combat_fixture::source_rows(&no_motion))
        .unwrap();
    let before = no_motion.clone();
    assert!(no_motion.initialize_facing_authority().is_err());
    assert_eq!(no_motion, before);
    p.source_motion = None;
    assert!(p.validate().is_err());
}

#[test]
fn selected_live_queries_derive_all_three_and_replay_rejects_altered_numeric_receipts() {
    let mut w = world(selected(spell_fixture::profile()));
    let genesis = w.clone();
    // A conflicting external answer cannot replace a selected numeric predicate.
    let mut a = spell_fixture::SpatialOnly {
        facing: Some(Check::Failed),
        base: combat_fixture::SpatialOnly {
            front: Some(Check::Failed),
            ..Default::default()
        },
        ..Default::default()
    };
    w.step(1, None, &mut a).unwrap();
    let cast = f::cast(&w, 133);
    w.step(2, Some(cast), &mut a).unwrap();
    for tick in 3..=5 {
        w.step(tick, None, &mut a).unwrap();
    }
    for expected in 0..3 {
        assert!(a
            .derived
            .iter()
            .any(|r| kind(&r.request.query) == Some(expected)));
    }
    assert!(a.queries.iter().all(|r| kind(&r.query).is_none()));
    assert!(a.queries.iter().any(|r| matches!(
        r.query,
        Query::SourceSpell(SourceSpellQuery::LineOfSight(_))
    )));
    assert!(a.derived.iter().any(|r| matches!(
        r.request.query,
        Query::SourceSpell(SourceSpellQuery::Range { .. })
    )));
    assert!(a
        .queries
        .iter()
        .all(|r| !matches!(r.query, Query::SourceSpell(SourceSpellQuery::Range { .. }))));
    assert!(a
        .queries
        .iter()
        .any(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Visitor(_)))));
    assert!(a.queries.iter().any(|r| matches!(
        r.query,
        Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcVisibility(_)))
    )));
    // Runtime-derived observations retain their position in the ordinary receipt stream.
    let recorded: Vec<_> = w
        .records()
        .iter()
        .flat_map(|r| &r.replies)
        .filter(|r| kind(&r.request.query).is_some())
        .collect();
    let observed: Vec<_> = a
        .derived
        .iter()
        .filter(|r| kind(&r.request.query).is_some())
        .collect();
    assert_eq!(recorded, observed);
    let mut replay = genesis.clone();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    for altered in 0..3 {
        for corruption in 0..4 {
            let mut records = w.records().to_vec();
            let reply = records
                .iter_mut()
                .flat_map(|r| &mut r.replies)
                .find(|r| kind(&r.request.query) == Some(altered))
                .unwrap();
            match corruption {
                0 => match &mut reply.facts {
                    Facts::SourceMotion(MotionFacts::Spatial(reply)) => {
                        reply.fact = motion::Fact::Bearing(f::real(1.0));
                    }
                    Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeFront(value)))
                    | Facts::SourceSpell(SourceSpellFacts::Facing(value)) => {
                        *value = if *value == Check::Passed {
                            Check::Failed
                        } else {
                            Check::Passed
                        };
                    }
                    _ => unreachable!(),
                },
                1 => reply.request.binding.tick += 1,
                2 => match &mut reply.request.query {
                    // Z is absent from the numeric formula, so the original answer
                    // remains correct, yet this is not the executing request.
                    Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(q))) => {
                        q.origin.z = f::real(1.0);
                    }
                    Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(q))) => {
                        q.creature_pose.position.z = f::real(1.0);
                    }
                    Query::SourceSpell(SourceSpellQuery::Facing(q)) => {
                        q.player_position.z = f::real(1.0);
                    }
                    _ => unreachable!(),
                },
                _ => match &mut reply.request.query {
                    Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(q))) => {
                        q.binding.query_id += 1;
                    }
                    Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(q))) => {
                        q.witness.working_npc[0] ^= 1;
                    }
                    Query::SourceSpell(SourceSpellQuery::Facing(q)) => {
                        q.history[0] ^= 1;
                    }
                    _ => unreachable!(),
                },
            }
            let mut rejected = genesis.clone();
            assert_eq!(
                rejected.replay(&records),
                Err(NativeError::ReplayMismatch),
                "query {altered}, corruption {corruption}"
            );
            assert_eq!(
                rejected.checkpoint().unwrap(),
                genesis.checkpoint().unwrap()
            );
        }
    }
}

#[test]
fn same_frame_turn_cast_turn_recomputes_release_from_current_orientation() {
    let mut w = world(selected(spell_fixture::profile()));
    let mut a = spell_fixture::spatial_only();
    let commands = inputs(
        &w,
        vec![
            turn(&w, 0.0),
            Command::Cast {
                ability: 133,
                target: w.state.npcs[&2].id,
            },
            turn(&w, std::f32::consts::PI),
        ],
    );
    w.step_batch(1, commands, &mut a).unwrap();
    for tick in 2..=4 {
        w.step(tick, None, &mut a).unwrap();
    }
    let front: Vec<_> = a
        .derived
        .iter()
        .filter_map(|r| match (&r.request.query, &r.facts) {
            (
                Query::SourceSpell(SourceSpellQuery::Facing(q)),
                Facts::SourceSpell(SourceSpellFacts::Facing(value)),
            ) => Some((q, *value)),
            _ => None,
        })
        .collect();
    assert!(front
        .iter()
        .any(|(q, value)| q.phase == SourceSpellPhase::Prepare
            && q.player_orientation == Orientation::ZERO
            && *value == Check::Passed));
    assert!(front
        .iter()
        .any(|(q, value)| q.phase == SourceSpellPhase::ReleaseCheck
            && q.player_orientation == Orientation::new(std::f32::consts::PI).unwrap()
            && *value == Check::Failed));
    assert!(a.derived.iter().any(|r| matches!(&r.facts,
        Facts::SourceSpell(SourceSpellFacts::Semantics(f)) if f.other_failure == Some(0x7c))));
    assert!(!w
        .records()
        .iter()
        .flat_map(|r| &r.events)
        .any(|event| matches!(
            event,
            Event::Spell {
                event: spell::SpellEvent::Launched { .. },
                ..
            }
        )));
}

#[test]
fn current_player_turn_changes_melee_front_and_unselected_policy_stays_external() {
    for selected_policy in [false, true] {
        let p = combat_fixture::profile();
        let mut w = world(if selected_policy { selected(p) } else { p });
        let mut a = combat_fixture::SpatialOnly::default();
        w.step(1, None, &mut a).unwrap();
        w.step(2, None, &mut a).unwrap();
        let commands = inputs(&w, vec![turn(&w, std::f32::consts::PI)]);
        let before = w.checkpoint().unwrap();
        let result = w.step_batch(3, commands, &mut a).map(|_| ());
        if selected_policy {
            assert_eq!(
                result,
                Err(NativeError::Melee(
                    benilla_sim::npc_melee::MeleeError::Unsupported(
                        "rear/non-facing melee and daze callback"
                    )
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
            let values: Vec<_> = a
                .derived
                .iter()
                .filter_map(|r| match (&r.request.query, &r.facts) {
                    (
                        Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(q))),
                        Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeFront(value))),
                    ) => Some((q, *value)),
                    _ => None,
                })
                .collect();
            assert!(
                values
                    .iter()
                    .any(|(q, value)| q.player_pose.orientation.bits() == 0
                        && *value == Check::Passed)
            );
            assert!(values
                .iter()
                .any(|(q, value)| q.player_pose.orientation.bits() == 0x4049_0fdb
                    && *value == Check::Failed));
            assert!(a.queries.iter().all(|r| kind(&r.query).is_none()));
        } else {
            result.unwrap();
            assert!(a.queries.iter().any(|r| kind(&r.query) == Some(0)));
            assert!(a.queries.iter().any(|r| kind(&r.query) == Some(1)));
        }
    }
}

#[test]
fn late_failure_after_numeric_observations_rolls_back_input_rng_clocks_and_history() {
    let mut w = world(selected(combat_fixture::profile()));
    let mut a = combat_fixture::SpatialOnly::default();
    w.step(1, None, &mut a).unwrap();
    let before = w.checkpoint().unwrap();
    let mut expected = w.clone();
    a.fail_post_damage = true;
    let commands = inputs(&w, vec![turn(&w, 0.25)]);
    assert_eq!(
        w.step_batch(2, commands.clone(), &mut a).map(|_| ()),
        Err(NativeError::AuthorityUnavailable(
            "authored post-health receipt fault"
        ))
    );
    assert!(a.derived.iter().any(|r| kind(&r.request.query) == Some(0)));
    assert!(a.derived.iter().any(|r| kind(&r.request.query) == Some(1)));
    assert_eq!(w.checkpoint().unwrap(), before);
    a.fail_post_damage = false;
    w.step_batch(2, commands.clone(), &mut a).unwrap();
    expected
        .step_batch(2, commands, &mut combat_fixture::SpatialOnly::default())
        .unwrap();
    assert_eq!(w.last_tick(), expected.last_tick());
    assert_eq!(w.checkpoint().unwrap(), expected.checkpoint().unwrap());
}

#[test]
fn published_native_format11_checkpoint_record_and_replay_are_refused() {
    let mut w = world(selected(combat_fixture::profile()));
    let initial = w.checkpoint().unwrap();
    let mut old = serde_json::to_value(&initial).unwrap();
    old["version"] = 11.into();
    old["state"]["version"] = 11.into();
    old["state"]["policy"] = "benilla-native-source-spell-authority-v11".into();
    let old: Checkpoint = serde_json::from_value(old).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), old).is_err());
    w.step(1, None, &mut combat_fixture::SpatialOnly::default())
        .unwrap();
    let mut records = w.records().to_vec();
    records[0].version = 11;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone()).unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), initial);
    assert_eq!(NATIVE_FORMAT_VERSION, 38);
    assert_eq!(crate::FORMAT_VERSION, 2);
}

#[test]
fn source_spline_bearing_uses_old_map_origin_instead_of_new_spline_pose() {
    // A bent authored route makes the old/new bearings materially different;
    // a straight chase would be collinear and could hide a substituted origin.
    #[derive(Default)]
    struct BentRoute(combat_fixture::SpatialOnly);
    impl Authority for BentRoute {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let mut reply = self.0.resolve(request)?;
            if let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &mut reply.facts {
                if let motion::Fact::Route(motion::RouteOutcome::Complete(route)) =
                    &mut spatial.fact
                {
                    let origin = route.points[0];
                    route.points.insert(
                        1,
                        ai::Point::new(
                            origin.x.as_f32() - 1.0,
                            origin.y.as_f32() + 2.0,
                            origin.z.as_f32(),
                        )?,
                    );
                    route.segments.push(route.segments[0]);
                }
            }
            Ok(reply)
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.0.observe_derived(reply)
        }
    }

    let mut p = combat_fixture::profile();
    p.player.start = ai::Point::new(0.0, 1.0, 0.0).unwrap();
    p.npcs[0].ai.spawn = f::point(16.0);
    let mut w = world(selected(p));
    let genesis = w.clone();
    let mut a = BentRoute::default();
    w.step(1, None, &mut a).unwrap();
    w.step(2, None, &mut a).unwrap();
    // Source relocates its map pose on a 400 ms timer while the spline advances
    // every tick. Wait for the next relocation rather than inventing a cadence.
    for tick in 3..=5 {
        w.step(tick, None, &mut a).unwrap();
    }
    let old_pose = w.state.npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .pose();
    w.step(6, None, &mut a).unwrap();
    let new_pose = w.state.npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .pose();
    assert_ne!(old_pose.position, new_pose.position);
    let (query, angle) = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find_map(|reply| match (&reply.request.query, &reply.facts) {
            (
                Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(query))),
                Facts::SourceMotion(MotionFacts::Spatial(reply)),
            ) if query.origin == old_pose.position => {
                let motion::Fact::Bearing(angle) = reply.fact else {
                    return None;
                };
                Some((query, angle))
            }
            _ => None,
        })
        .expect("continuing spline must ask from its old map pose");
    let xy = |p: ai::Point| [p.x.bits(), p.y.bits()];
    assert_eq!(
        angle.bits(),
        benilla_sim::facing::bearing_bits(xy(query.origin), xy(query.toward)).unwrap()
    );
    assert_ne!(
        angle.bits(),
        benilla_sim::facing::bearing_bits(xy(new_pose.position), xy(query.toward)).unwrap()
    );
    let mut replay = genesis;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
