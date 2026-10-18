// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete authored scene execution, not a loaded map-visitor implementation.
use super::{synthetic_fixture as f, synthetic_two_owner_fixture as fixture, *};
use crate::NpcId;
use ai::movement::Check;

fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}
fn step(world: &mut NativeWorld, authority: &mut impl Authority) -> Result<(), NativeError> {
    world
        .step(world.state.tick + 1, None, authority)
        .map(|_| ())
}
fn creature(spawn: u64) -> VisitorParticipant {
    VisitorParticipant::Creature(NpcId {
        spawn,
        generation: 0,
    })
}
fn sight(reply: &AuthorityReply) -> Option<(&VisitorSightRequest, &SightDisposition)> {
    match (&reply.request.query, &reply.facts) {
        (
            Query::SourceMotion(MotionQuery::VisitorSight(request)),
            Facts::SourceMotion(MotionFacts::VisitorSight(disposition)),
        ) => Some((request, disposition)),
        _ => None,
    }
}
fn reopen(world: &NativeWorld) -> NativeWorld {
    let checkpoint = Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap();
    NativeWorld::restore_synthetic_component_test(world.profile.clone(), checkpoint).unwrap()
}
#[test]
fn two_owner_authored_order_and_bilateral_grid_then_world_are_exact() {
    for order in [[2, 3], [3, 2]] {
        for grid in [vec![2, 3], vec![3, 2]] {
            let mut w = world(fixture::profile_with_order(order));
            let initial = w.clone();
            let mut a = fixture::CompleteAuthority {
                grid_order: Some(grid.clone()),
                ..Default::default()
            };
            step(&mut w, &mut a).unwrap();
            step(&mut w, &mut a).unwrap();
            let record = w.last_tick().unwrap();
            let scene_queries = record
                .replies
                .iter()
                .filter_map(|r| match &r.request.query {
                    Query::SourceMotion(MotionQuery::SceneVisitor(request)) => Some((r, request)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(scene_queries.len(), 2);
            for ((reply, request), owner) in scene_queries.iter().zip(order) {
                assert_eq!(request.notification_owner, creature(owner));
                assert_eq!(
                    reply.request.binding.npc,
                    Some(NpcId {
                        spawn: owner,
                        generation: 0
                    })
                );
                assert_eq!(request.owner_order, order);
                assert_eq!(request.creatures.len(), 2);
                let Facts::SourceMotion(MotionFacts::SceneVisitor(SceneVisitorFacts::Complete {
                    grid_creatures,
                    world_players,
                    not_visited,
                    ..
                })) = &reply.facts
                else {
                    panic!("complete membership");
                };
                assert_eq!(
                    grid_creatures.iter().map(|id| id.spawn).collect::<Vec<_>>(),
                    grid
                );
                assert_eq!(world_players, &[w.state.player]);
                assert!(not_visited.is_empty());
            }
            let callbacks = record
                .replies
                .iter()
                .filter_map(sight)
                .map(|(r, _)| {
                    (
                        r.notification_owner,
                        r.container,
                        r.visit_index,
                        r.direction,
                        r.observer,
                        r.target,
                    )
                })
                .collect::<Vec<_>>();
            let expected = order
                .into_iter()
                .flat_map(|owner| {
                    let peer = if owner == 2 { 3 } else { 2 };
                    let index = grid.iter().position(|spawn| *spawn == peer).unwrap() as u32;
                    [
                        (
                            creature(owner),
                            VisitorContainer::GridCreatures,
                            index,
                            VisitorDirection::VisitedToOwner,
                            creature(peer),
                            creature(owner),
                        ),
                        (
                            creature(owner),
                            VisitorContainer::GridCreatures,
                            index,
                            VisitorDirection::OwnerToVisited,
                            creature(owner),
                            creature(peer),
                        ),
                        (
                            creature(owner),
                            VisitorContainer::WorldPlayers,
                            0,
                            VisitorDirection::OwnerToVisited,
                            creature(owner),
                            VisitorParticipant::Player(w.state.player),
                        ),
                    ]
                })
                .collect::<Vec<_>>();
            assert_eq!(callbacks, expected);
            for (ordinal, reply) in record.replies.iter().enumerate() {
                let witness = match &reply.request.query {
                    Query::SourceMotion(MotionQuery::SceneVisitor(r)) => Some(&r.witness),
                    Query::SourceMotion(
                        MotionQuery::VisitorVisibility(r) | MotionQuery::VisitorSight(r),
                    ) => Some(&r.witness),
                    _ => None,
                };
                if let Some(witness) = witness {
                    assert_eq!(
                        witness.frame.moment,
                        InteractionMoment::Frame {
                            action_ordinal: ordinal as u32
                        }
                    );
                }
            }
            assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Combat);
            assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Idle);
            assert_eq!(w.profile.npcs[1].ai.reaction, ai::Reaction::Aggressive);
            assert!(w.profile.npc_motion(3).unwrap().npc_can_enter_combat);
            assert_eq!(
                w.state.npcs[&3]
                    .source_motion
                    .as_ref()
                    .unwrap()
                    .motion
                    .pose()
                    .position,
                w.profile.npcs[1].ai.spawn
            );
            if order == [2, 3] {
                assert!(record
                    .replies
                    .iter()
                    .filter_map(sight)
                    .any(|(r, d)| r.notification_owner == creature(3)
                        && r.observer == creature(2)
                        && *d
                            == SightDisposition::Peer(
                                PeerSightDisposition::ExistingOutdoorVictim.into()
                            )));
                assert!(scene_queries[1]
                    .1
                    .creatures
                    .iter()
                    .any(|npc| npc.id.spawn == 2 && npc.phase == ai::Phase::Combat));
            }
            let mut replay = initial;
            replay.replay(w.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
            assert_eq!(reopen(&w).state_digest(), w.state_digest());
        }
    }
}
#[test]
fn two_owner_exact_inventory_rejects_missing_duplicate_foreign_stale_and_wrong_kind() {
    let ids = [
        NpcId {
            spawn: 2,
            generation: 0,
        },
        NpcId {
            spawn: 3,
            generation: 0,
        },
    ];
    for case in 0..9 {
        let mut w = world(fixture::profile());
        let mut a = fixture::CompleteAuthority::default();
        step(&mut w, &mut a).unwrap();
        let before = w.checkpoint().unwrap();
        let mut grid = ids.to_vec();
        let mut players = vec![f::player()];
        let mut outside = vec![];
        match case {
            0 => {
                grid.pop();
            }
            1 => grid[1] = grid[0],
            2 => grid[1].spawn = 99,
            3 => grid[1].generation = 1,
            4 => players[0].character = [99; 16],
            5 => {
                grid.pop();
                players.push(f::player());
            }
            6 => outside.push(creature(3)),
            7 => {
                grid.clear();
                outside = vec![creature(2), creature(2)];
            }
            _ => {
                players.clear();
            }
        }
        a.membership = Some(SceneVisitorFacts::Complete {
            loaded_membership: Check::Passed,
            grid_creatures: grid,
            world_players: players,
            not_visited: outside,
        });
        assert_eq!(
            step(&mut w, &mut a),
            Err(NativeError::AuthorityBinding),
            "case {case}"
        );
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records().len(), 1);
    }
    for facts in [
        SceneVisitorFacts::Missing,
        SceneVisitorFacts::UnrepresentedParticipants,
        SceneVisitorFacts::Complete {
            loaded_membership: Check::Unknown,
            grid_creatures: ids.into(),
            world_players: vec![f::player()],
            not_visited: vec![],
        },
    ] {
        let mut w = world(fixture::profile());
        let mut a = fixture::CompleteAuthority::default();
        step(&mut w, &mut a).unwrap();
        a.membership = Some(facts);
        let before = w.checkpoint().unwrap();
        assert!(step(&mut w, &mut a).is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}
#[test]
fn two_owner_explicit_outside_membership_and_dead_self_membership_are_distinct() {
    let mut w = world(fixture::profile());
    let mut a = fixture::CompleteAuthority {
        not_visited: vec![creature(3)],
        ..Default::default()
    };
    step(&mut w, &mut a).unwrap();
    step(&mut w, &mut a).unwrap();
    assert!(w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .filter_map(sight)
        .all(|(r, _)| !(r.notification_owner == creature(2) && r.target == creature(3))));
    assert!(w.last_tick().unwrap().replies.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::SceneVisitor(SceneVisitorFacts::Complete { not_visited, .. })) if not_visited == &[creature(3)])));

    let mut w = world(fixture::profile_with_order([3, 2]));
    // An authored staged death tests dispatcher membership, not a new damage API.
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = fixture::CompleteAuthority::default();
    step(&mut w, &mut a).unwrap();
    step(&mut w, &mut a).unwrap();
    assert!(w.last_tick().unwrap().replies.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::SceneVisitor(SceneVisitorFacts::Complete { grid_creatures, .. })) if grid_creatures == &[NpcId {spawn:2,generation:0},NpcId {spawn:3,generation:0}])));
    assert!(w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .filter_map(sight)
        .all(|(r, _)| r.observer != creature(2) && r.target != creature(2)));
}
#[test]
fn two_owner_directional_visibility_precedes_neutral_semantics_and_is_not_los() {
    for check in [Check::Failed, Check::Unknown] {
        let mut w = world(fixture::profile());
        let mut a = fixture::CompleteAuthority {
            visibility_overrides: vec![(creature(3), creature(2), check)],
            ..Default::default()
        };
        step(&mut w, &mut a).unwrap();
        let before = w.checkpoint().unwrap();
        let result = step(&mut w, &mut a);
        if check == Check::Unknown {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "selected directional AI visibility"
                ))
            );
            assert_eq!(w.checkpoint().unwrap(), before);
        } else {
            result.unwrap();
            assert!(w
                .last_tick()
                .unwrap()
                .replies
                .iter()
                .filter_map(sight)
                .any(|(r, d)| r.observer == creature(3)
                    && r.target == creature(2)
                    && *d == SightDisposition::NotVisible));
            assert!(w
                .last_tick()
                .unwrap()
                .replies
                .iter()
                .filter_map(sight)
                .any(|(r, d)| r.observer == creature(2)
                    && r.target == creature(3)
                    && matches!(d, SightDisposition::Peer(_))));
            assert!(w.last_tick().unwrap().replies.iter().all(|r| !matches!(
                r.request.query,
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcVisibility(_)))
            )));
        }
    }
}
#[test]
fn two_owner_late_second_owner_visibility_or_receipt_failure_rolls_back_rng_and_input() {
    for derived in [false, true] {
        let mut p = fixture::profile();
        p.npcs[0].ai.spawn = f::point(2.0);
        let mut w = world(p);
        let mut a = fixture::CompleteAuthority::default();
        step(&mut w, &mut a).unwrap();
        let before = w.checkpoint().unwrap();
        let records = w.records().to_vec();
        let input = f::cast(&w, 60001);
        let mut clean = w.clone();
        clean
            .step(
                2,
                Some(input.clone()),
                &mut fixture::CompleteAuthority::default(),
            )
            .unwrap();
        assert!(!clean.last_tick().unwrap().draws.is_empty());
        if derived {
            a.fail_derived_owner = Some(3);
        } else {
            a.fail_visibility_owner = Some(3);
        }
        assert!(w.step(2, Some(input.clone()), &mut a).is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records(), records);
        assert!(a.derived.iter().any(|r| matches!(
            r.request.query,
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeCombatEntry(_)))
        )));
        a.fail_derived_owner = None;
        a.fail_visibility_owner = None;
        w.step(2, Some(input), &mut a).unwrap();
        assert_eq!(w.last_tick(), clean.last_tick());
        assert_eq!(w.checkpoint().unwrap(), clean.checkpoint().unwrap());
    }
}
#[test]
fn two_owner_player_notification_uses_none_binding_and_every_live_grid_creature() {
    let mut w = world(fixture::profile());
    let input = OwnedInput {
        authenticated_owner: f::player().owner,
        input: Input {
            world: w.state.world,
            tick: 1,
            player: f::player(),
            sequence: 0,
            command: Command::MovementPose {
                destination: f::point(0.1),
                orientation: Orientation::ZERO,
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            },
        },
    };
    let mut a = fixture::CompleteAuthority {
        grid_order: Some(vec![3, 2]),
        ..Default::default()
    };
    w.step(1, Some(input), &mut a).unwrap();
    for _ in 0..3 {
        step(&mut w, &mut a).unwrap();
    }
    let reply = w.records().iter().flat_map(|r| &r.replies).find(|r| matches!(&r.request.query, Query::SourceMotion(MotionQuery::SceneVisitor(q)) if matches!(q.notification_owner, VisitorParticipant::Player(_)))).unwrap();
    assert_eq!(reply.request.binding.npc, None);
    let callbacks = w
        .records()
        .iter()
        .flat_map(|r| &r.replies)
        .filter_map(sight)
        .filter(|(r, _)| matches!(r.notification_owner, VisitorParticipant::Player(_)))
        .map(|(r, _)| r.observer)
        .collect::<Vec<_>>();
    assert_eq!(callbacks, vec![creature(3), creature(2)]);
}
#[test]
fn two_owner_real_spell_death_respawn_preserves_other_queue_clock_and_replays_each_tick() {
    let mut p = fixture::profile();
    p.npcs[0].max_health = 1;
    p.npcs[0].combat.as_mut().unwrap().minimum_damage = f::float(0.0);
    p.npcs[0].combat.as_mut().unwrap().maximum_damage = f::float(0.0);
    let mut w = world(p);
    let initial = w.clone();
    let old_a = w.state.npcs[&2].events.notify_handle().unwrap();
    let old_b = w.state.npcs[&3].events.notify_handle().unwrap();
    assert_eq!(old_a.event_id, old_b.event_id);
    assert_ne!(old_a.npc, old_b.npc);
    let mut a = fixture::CompleteAuthority::default();
    a.base.base.critical_chance = Some(0);
    a.base.base.resist_chance = Some(0);
    let input = f::cast(&w, 60001);
    w.step(1, Some(input), &mut a).unwrap();
    let mut phases = vec![];
    while w.state.npcs[&2].id.generation == 0 {
        assert!(w.state.tick < 100);
        step(&mut w, &mut a).unwrap();
        phases.push(w.state.npcs[&2].ai.phase());
        assert_eq!(w.state.npcs[&3].id.generation, 0);
        assert_eq!(w.state.npcs[&3].events.clock_ms(), w.state.now_ms);
        assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Idle);
        assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    }
    assert!(phases.contains(&ai::Phase::Corpse));
    assert!(phases.contains(&ai::Phase::Despawned));
    assert!(w.state.npcs[&2].events.event(old_a).is_none());
    assert_eq!(
        w.state.npcs[&2]
            .events
            .notify_handle()
            .unwrap()
            .npc
            .generation,
        1
    );
    assert_eq!(w.state.npcs[&3].id, old_b.npc);
    assert!(w.state.npcs[&2].events.clock_ms() < w.state.npcs[&3].events.clock_ms());
    let mut replay = initial;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}

fn no_damage_profile() -> Profile {
    let mut p = fixture::profile();
    for npc in &mut p.npcs {
        npc.combat.as_mut().unwrap().minimum_damage = f::float(0.0);
        npc.combat.as_mut().unwrap().maximum_damage = f::float(0.0);
    }
    p
}
fn cast_at(w: &NativeWorld, spawn: u64) -> OwnedInput {
    let mut input = f::cast(w, 60001);
    let Command::Cast { target, .. } = &mut input.input.command else {
        unreachable!()
    };
    *target = w.state.npcs[&spawn].id;
    input
}
#[test]
fn two_owner_retaliation_and_two_real_auras_advance_once_with_independent_motion() {
    let mut w = world(no_damage_profile());
    let initial = w.clone();
    let mut a = fixture::CompleteAuthority::default();
    a.base.base.critical_chance = Some(0);
    a.base.base.resist_chance = Some(0);
    let input = cast_at(&w, 2);
    w.step(1, Some(input), &mut a).unwrap();
    while w.state.tick < 5 {
        step(&mut w, &mut a).unwrap();
    }
    let input = cast_at(&w, 3);
    w.step(6, Some(input), &mut a).unwrap();
    while w.state.npcs.values().any(|npc| npc.auras.is_empty()) {
        assert!(w.state.tick < 45, "both real casts must overlap");
        step(&mut w, &mut a).unwrap();
    }
    // B's original Aggressive/neutral profile still retaliates to a deliberate hit.
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Combat);
    assert_eq!(w.profile.npcs[1].ai.reaction, ai::Reaction::Aggressive);
    assert!(w.profile.npcs[1].ai.neutral_to_all);
    assert!(w.records().iter().flat_map(|record| &record.events).any(|event| matches!(event,Event::NpcMotion{id,event:benilla_sim::creature_motion::Event::SplineLaunched{..}} if id.spawn==3)));
    let first_splines = [2, 3].map(|owner| {
        w.records()
            .iter()
            .flat_map(|record| &record.events)
            .find_map(|event| match event {
                Event::NpcMotion {
                    id,
                    event: benilla_sim::creature_motion::Event::SplineLaunched { spline_id, .. },
                } if id.spawn == owner => Some(*spline_id),
                _ => None,
            })
            .unwrap()
    });
    assert_eq!(
        first_splines,
        [1, 1],
        "spline IDs are independent owner-local counters"
    );
    let mut expected = w
        .state
        .npcs
        .iter()
        .map(|(&spawn, npc)| (spawn, npc.auras[&60001].clone()))
        .collect::<BTreeMap<_, _>>();
    for target in expected.values_mut() {
        target
            .update_aura_with_evade(w.state.now_ms + 100, 100, false, false)
            .unwrap();
    }
    step(&mut w, &mut a).unwrap();
    for spawn in [2, 3] {
        assert_eq!(
            w.state.npcs[&spawn].auras[&60001].aura(),
            expected[&spawn].aura()
        );
        assert_eq!(w.state.npcs[&spawn].events.clock_ms(), w.state.now_ms);
    }
    let mut replay = initial;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn two_owner_reached_core_help_rejects_frame_then_visibility_retry_matches_clean_run() {
    let mut w = world(no_damage_profile());
    let mut a = fixture::CompleteAuthority::default();
    a.base.base.critical_chance = Some(0);
    a.base.base.resist_chance = Some(0);
    a.visibility_overrides = vec![
        (
            creature(2),
            VisitorParticipant::Player(w.state.player),
            Check::Failed,
        ),
        (creature(2), creature(3), Check::Failed),
    ];
    let input = cast_at(&w, 3);
    w.step(1, Some(input), &mut a).unwrap();
    while w.state.npcs[&3].ai.phase() != ai::Phase::Combat {
        assert!(w.state.tick < 50);
        step(&mut w, &mut a).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    w.state
        .npcs
        .get_mut(&3)
        .unwrap()
        .events
        .schedule_notify(0, true)
        .unwrap();
    let before = w.checkpoint().unwrap();
    let mut expected = w.clone();
    let mut clean = fixture::CompleteAuthority::default();
    clean.base.base.critical_chance = Some(0);
    clean.base.base.resist_chance = Some(0);
    clean.visibility_overrides = a.visibility_overrides.clone();
    step(&mut expected, &mut clean).unwrap();
    a.visibility_overrides.pop();
    assert_eq!(
        step(&mut w, &mut a),
        Err(NativeError::AuthorityUnavailable(
            "peer help selected assistance owners"
        ))
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    a.visibility_overrides = clean.visibility_overrides;
    step(&mut w, &mut a).unwrap();
    assert_eq!(w.last_tick(), expected.last_tick());
    assert_eq!(w.checkpoint().unwrap(), expected.checkpoint().unwrap());
}
#[test]
fn two_owner_reached_peer_attack_never_installs_peer_targets_or_commits() {
    let mut p = no_damage_profile();
    p.npcs[1].ai.neutral_to_all = false;
    let mut rows = synthetic_combat_fixture::source_rows(&p);
    rows.factions.push(InteractionFactionSource {
        template_id: 3,
        faction: 96003,
        template_flags: 0,
        group_mask: 4,
        friend_group_mask: 4,
        enemy_group_mask: 2,
        enemies: [0; 4],
        friends: [0; 4],
        reputation_list_id: Some(-1),
    });
    let motion = p.source_motion.take();
    let visitors = p.source_visitors.take();
    let capabilities = p.creature_capabilities.take();
    p.interactions = None;
    p.initialize_interactions(rows).unwrap();
    p.source_motion = motion;
    p.source_visitors = visitors;
    p.creature_capabilities = capabilities;
    p.initialize_combat_authority(
        p.npcs
            .iter()
            .map(|npc| NpcCombatAuthoritySource {
                entry: npc.entry,
                extra_flags: 0,
                static_flags1: 0,
                static_flags2: 0,
                static_flags3: 0,
                static_flags4: 0,
            })
            .collect(),
    )
    .unwrap();
    let mut w = world(p);
    let mut a = fixture::CompleteAuthority {
        visibility_overrides: vec![(
            creature(2),
            VisitorParticipant::Player(w.state.player),
            Check::Failed,
        )],
        ..Default::default()
    };
    step(&mut w, &mut a).unwrap();
    let before = w.checkpoint().unwrap();
    assert_eq!(
        step(&mut w, &mut a),
        Err(NativeError::UnsupportedMechanic(
            "peer sight attack/accessibility continuation"
        ))
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(w.state.npcs.values().all(|npc| npc.ai.target().is_none()));
}

fn rehash_checkpoint(value: serde_json::Value) -> Checkpoint {
    let mut checkpoint: Checkpoint = serde_json::from_value(value).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v50\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v50\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    checkpoint
}
#[test]
fn two_owner_rehashed_queue_motion_generation_and_derived_receipt_swaps_reject() {
    let mut w = world(fixture::profile());
    let initial = w.clone();
    let mut a = fixture::CompleteAuthority::default();
    for _ in 0..3 {
        step(&mut w, &mut a).unwrap();
    }
    let original = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for field in ["events", "source_motion", "source_capabilities", "id", "ai"] {
        let mut value = original.clone();
        let a = value["state"]["npcs"]["2"][field].clone();
        value["state"]["npcs"]["2"][field] = value["state"]["npcs"]["3"][field].clone();
        value["state"]["npcs"]["3"][field] = a;
        let rehashed = rehash_checkpoint(value);
        Checkpoint::decode(&rehashed.encode().unwrap()).unwrap();
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehashed).is_err(),
            "{field}"
        );
    }
    for case in 0..5 {
        let mut records = w.records().to_vec();
        let reply = records
            .iter_mut()
            .flat_map(|record| &mut record.replies)
            .find(|r| {
                matches!(
                    r.request.query,
                    Query::SourceMotion(MotionQuery::VisitorSight(_))
                )
            })
            .unwrap();
        let Query::SourceMotion(MotionQuery::VisitorSight(request)) = &mut reply.request.query
        else {
            unreachable!()
        };
        match case {
            0 => request.direction = VisitorDirection::OwnerToVisited,
            1 => {
                request.witness.frame.moment = InteractionMoment::Frame {
                    action_ordinal: 999,
                }
            }
            2 => request.witness.frame.draft = [9; 32],
            3 => request.observer = creature(99),
            _ => {
                reply.facts =
                    Facts::SourceMotion(MotionFacts::VisitorSight(SightDisposition::Evading))
            }
        }
        let mut replay = initial.clone();
        let before = replay.checkpoint().unwrap();
        assert!(replay.replay(&records).is_err(), "case {case}");
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
}

#[test]
fn two_owner_codue_player_notify_and_spell_keep_event_insertion_order() {
    for notify_first in [false, true] {
        let mut p = fixture::profile();
        p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms =
            if notify_first { 101 } else { 1 };
        let mut w = world(p);
        let initial = w.clone();
        let mut a = fixture::CompleteAuthority {
            visibility: Some(Check::Failed),
            ..Default::default()
        };
        let input = f::cast(&w, 60001);
        w.step(1, Some(input), &mut a).unwrap();
        let notify_tick = if notify_first { 2 } else { 3 };
        for tick in 2..=3 {
            a.queries.clear();
            let input = (tick == notify_tick).then(|| OwnedInput {
                authenticated_owner: f::player().owner,
                input: Input {
                    world: w.state.world,
                    player: w.state.player,
                    tick,
                    sequence: w.state.next_sequence,
                    command: Command::MovementPose {
                        destination: w.state.position,
                        orientation: Orientation::new(0.5).unwrap(),
                        movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
                    },
                },
            });
            w.step(tick, input, &mut a).unwrap();
        }
        let order = a
            .queries
            .iter()
            .filter_map(|request| match &request.query {
                Query::Spell {
                    phase: CastPhase::Release,
                    ..
                } => Some("spell"),
                Query::SourceMotion(MotionQuery::SceneVisitor(scene))
                    if matches!(scene.notification_owner, VisitorParticipant::Player(_)) =>
                {
                    Some("notify")
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            if notify_first {
                vec!["notify", "spell"]
            } else {
                vec!["spell", "notify"]
            }
        );
        let mut replay = initial;
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    }
}
#[test]
fn two_owner_actual_lethal_player_damage_detaches_before_later_peer_callback_and_rolls_back() {
    use benilla_sim::player_death as death;
    let mut p = fixture::profile();
    p.npcs[0].ai.spawn = f::point(2.0);
    p.npcs[0].combat.as_mut().unwrap().minimum_damage = f::float(10000.0);
    p.npcs[0].combat.as_mut().unwrap().maximum_damage = f::float(10000.0);
    p.configure_player_death(
        death::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        death::DeathSupport::checked(
            &death::REQUIRED_ABSENCES.map(|dependency| (dependency, Some(false))),
        )
        .unwrap(),
    )
    .unwrap();
    let mut w = world(p);
    let mut a = fixture::CompleteAuthority::default();
    step(&mut w, &mut a).unwrap();
    let before = w.checkpoint().unwrap();
    let mut clean = w.clone();
    step(&mut clean, &mut fixture::CompleteAuthority::default()).unwrap();
    assert_eq!(clean.state.health, 0);
    let killer = &clean.state.npcs[&2];
    assert_eq!(killer.ai.target(), Some(SOLO_UNIT));
    assert!(killer.ai.victim_detached_at_ms().is_some());
    assert!(killer.ai.hostile_reference().is_none());
    assert!(clean
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .filter_map(sight)
        .any(|(request, disposition)| {
            request.notification_owner == creature(3)
                && request.observer == creature(2)
                && *disposition
                    == SightDisposition::Peer(PeerSightDisposition::CannotAttackOnSight.into())
        }));
    a.fail_derived_owner = Some(3);
    assert!(step(&mut w, &mut a).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(w.state.health > 0);
    a.fail_derived_owner = None;
    step(&mut w, &mut a).unwrap();
    assert_eq!(w.checkpoint().unwrap(), clean.checkpoint().unwrap());
    assert_eq!(w.last_tick(), clean.last_tick());
    assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
}

#[test]
fn two_owner_reversed_numeric_role_ids_obey_authored_schedule() {
    for order in [[3, 2], [2, 3]] {
        let mut w = world(fixture::profile_with_spawns(3, 2, order));
        let mut a = fixture::CompleteAuthority {
            grid_order: Some(vec![3, 2]),
            ..Default::default()
        };
        for _ in 0..4 {
            step(&mut w, &mut a).unwrap();
        }
        assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Combat);
        assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
        let queries = w.records()[1]
            .replies
            .iter()
            .filter_map(|reply| match &reply.request.query {
                Query::SourceMotion(MotionQuery::SceneVisitor(request)) => {
                    Some(request.notification_owner)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(queries, order.map(creature));
        assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    }
}
#[test]
fn two_owner_runtime_cache_restore_and_native_guard_check_each_owner() {
    struct Cache {
        complete: fixture::CompleteAuthority,
        cache: super::motion::route_cache_tests::SyntheticCacheAuthority,
        owner: u64,
    }
    impl Authority for Cache {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(&request.query,Query::SourceMotion(MotionQuery::Route(route)) if route.owner.npc().spawn==self.owner)
            {
                self.cache.resolve(request)
            } else {
                self.complete.resolve(request)
            }
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.complete.observe_derived(reply)
        }
    }
    for owner in [2, 3] {
        let mut w = world(no_damage_profile());
        let mut complete = fixture::CompleteAuthority::default();
        complete.base.base.critical_chance = Some(0);
        complete.base.base.resist_chance = Some(0);
        if owner == 3 {
            complete.visibility_overrides = vec![
                (
                    creature(2),
                    VisitorParticipant::Player(w.state.player),
                    Check::Failed,
                ),
                (creature(2), creature(3), Check::Failed),
            ];
        }
        let mut a = Cache {
            complete,
            cache: Default::default(),
            owner,
        };
        if owner == 3 {
            let input = cast_at(&w, 3);
            w.step(1, Some(input), &mut a).unwrap();
        }
        while !a.cache.calculated() {
            assert!(w.state.tick < 100);
            step(&mut w, &mut a).unwrap();
        }
        assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
        let npc = &w.state.npcs[&owner];
        let mut snapshot = npc.source_motion.as_ref().unwrap().snapshot();
        let benilla_sim::creature_motion::Generator::Chase(chase) = &mut snapshot.motion.generator
        else {
            panic!("chase cache")
        };
        chase.pathfinder_cache.as_mut().unwrap().source =
            benilla_sim::creature_motion::RouteSource::OwnerPathFinder;
        let spline = snapshot.motion.spline.as_mut().unwrap();
        spline.route.source = benilla_sim::creature_motion::RouteSource::OwnerPathFinder;
        spline.route_source = benilla_sim::creature_motion::RouteSource::OwnerPathFinder;
        benilla_sim::creature_motion::MotionState::restore(snapshot.motion.clone()).unwrap();
        assert_eq!(
            NpcMotion::restore(
                snapshot.clone(),
                w.profile(),
                &npc.ai,
                w.state.now_ms,
                npc.id
            ),
            Err(NativeError::InvalidCheckpoint)
        );
        w.state
            .npcs
            .get_mut(&owner)
            .unwrap()
            .source_motion
            .as_mut()
            .unwrap()
            .motion = benilla_sim::creature_motion::MotionState::restore(snapshot.motion).unwrap();
        assert_eq!(
            w.validate_saved_session_reopen(),
            Err(NativeError::UnsupportedMechanic(
                "saved native PathFinder cache rehydration"
            ))
        );
    }
}

#[test]
fn two_owner_identical_failed_spell_ids_remain_queue_owned_through_save_and_destroy() {
    struct AuthorityWithFailedStarts {
        visitors: fixture::CompleteAuthority,
        spells: synthetic_slow_fixture::SyntheticNpcSpellAuthority,
    }
    impl Authority for AuthorityWithFailedStarts {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(request.query, Query::NpcSpell(_)) {
                self.spells.resolve(request)
            } else {
                self.visitors.resolve(request)
            }
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.visitors.observe_derived(reply)
        }
    }
    let mut p = fixture::profile();
    let mut slow = PlayerSlowGenesis::from_sources(
        vec![player_slow::source::synthetic_source(61004, -40, 400)],
        player_slow::host::synthetic_host(&p),
    )
    .unwrap();
    // Existing explicit test-only SpellStart seam. Both casts fail their first
    // source check; no scripted AI or successful peer combat behavior is added.
    slow.fixture_triggers = [2, 3]
        .map(|spawn| npc_spell::FixtureTrigger {
            tick: 1,
            caster: crate::NpcId {
                spawn,
                generation: 0,
            },
            spell_id: 61004,
            after_npc_update: true,
        })
        .into();
    p.player_slow = Some(slow);
    let mut w = world(p);
    let initial = w.clone();
    let mut a = AuthorityWithFailedStarts {
        visitors: Default::default(),
        spells: synthetic_slow_fixture::SyntheticNpcSpellAuthority {
            fail: Some(NpcSpellCheck::TargetNotTaxiFlying),
            ..Default::default()
        },
    };
    step(&mut w, &mut a).unwrap();
    let lifetimes =
        [2, 3].map(|spawn| w.state.npcs[&spawn].events.spells().next().unwrap().clone());
    assert_eq!(lifetimes[0].cast_id, lifetimes[1].cast_id);
    assert_eq!(lifetimes[0].event_id, lifetimes[1].event_id);
    assert_ne!(lifetimes[0].caster, lifetimes[1].caster);
    assert!(lifetimes
        .iter()
        .all(|lifetime| lifetime.terminal == Some(NpcSpellTerminal::BadTargets)));
    assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut malformed = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    malformed["state"]["npcs"]["3"]["events"]["spells"]["1"]["caster"] =
        serde_json::to_value(lifetimes[0].caster).unwrap();
    let malformed = rehash_checkpoint(malformed);
    Checkpoint::decode(&malformed.encode().unwrap()).unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), malformed).is_err());
    step(&mut w, &mut a).unwrap();
    assert!(w
        .state
        .npcs
        .values()
        .all(|npc| npc.events.spells().next().is_none()));
    for lifetime in lifetimes {
        assert!(w.last_tick().unwrap().events.iter().any(|event| matches!(event,Event::NpcSpell(NpcSpellEvent::Destroyed {lifetime:destroyed,aborted:false}) if *destroyed==lifetime)));
    }
    let mut replay = initial;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
