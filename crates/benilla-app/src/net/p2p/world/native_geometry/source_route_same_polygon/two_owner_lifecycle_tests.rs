// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact lifecycle scenario inputs shared with the installed native-owner test.
//! Headless routes remain explicitly synthetic; only environment facts are loaded.
use super::*;
use benilla_sim::classic_spell::{CastState, SpellEvent};
use benilla_world_runtime::native::{
    AbilityAdmission, CancelCastTarget, Command, Event, Input, OwnedInput,
};

pub(crate) fn configure_lifecycle_profile(profile: &mut Profile) {
    assert_eq!(profile.tick_ms, 100);
    let AbilityAdmission::Available(spec) = &profile
        .abilities
        .iter()
        .find(|ability| ability.id == 60001)
        .unwrap()
        .admission
    else {
        panic!("authored lifecycle projectile");
    };
    assert_eq!(spec.cast_ms, 200);
    assert_eq!(spec.projectile_speed, f::float(4.));
    for npc in &mut profile.npcs {
        // Authored scenario cadence: source 0..0 damage becomes 0..5, and a
        // 100ms swing can keep pushing a 200ms cast back indefinitely.
        let combat = npc.combat.as_mut().unwrap();
        combat.minimum_damage = f::float(1.);
        combat.maximum_damage = f::float(1.);
        npc.ai.mainhand_interval_ms = 2_000;
        assert_eq!(npc.ai.corpse_decay_ms, 500);
        assert_eq!(npc.ai.respawn_delay_seconds, 2);
    }
    profile
        .npcs
        .iter_mut()
        .find(|npc| npc.ai.creature_id == 2)
        .unwrap()
        .max_health = 1;
}

struct EnvironmentOnly<'a, A = scene::CompleteAuthority> {
    session: &'a Session,
    providers: &'a mut SamePolygonProviders,
    other: A,
}
impl<A: Authority> Authority for EnvironmentOnly<'_, A> {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.other.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::PlayerEnvironment(_)) {
            let facts = self
                .providers
                .environment
                .resolve(
                    self.session,
                    request,
                    Instant::now() + Duration::from_secs(10),
                )
                .unwrap();
            core::validate_facts(self.session, request, &facts).unwrap();
            return Ok(AuthorityReply {
                request: request.clone(),
                facts,
            });
        }
        self.other.resolve(request)
    }
}

#[test]
fn loaded_quest_profile_completes_selected_spell_loot_reward_in_both_roles_and_orders() {
    use benilla_world_runtime::native::Orientation;
    let (products, revision, maps) = loaded_fixture();
    for (hostile, giver, order) in [
        (2, 3, [2, 3]),
        (2, 3, [3, 2]),
        (3, 2, [2, 3]),
        (3, 2, [3, 2]),
    ] {
        let (profile, session, inputs) = quest_inputs(
            Arc::clone(&products),
            revision,
            &maps,
            hostile,
            giver,
            order,
        );
        let mut providers = inputs.bind(&session, &profile, f::player()).unwrap();
        let mut world =
            NativeWorld::synthetic_component_test(profile, session.world, f::player(), 7).unwrap();
        let mut replay = world.clone();
        let mut authority = EnvironmentOnly {
            session: &session,
            providers: &mut providers,
            other: quest::CompleteAuthority::default(),
        };
        step(&mut world, &mut authority, vec![]);
        step(&mut world, &mut authority, vec![]);
        let giver_id = world.state().npcs[&giver].id;
        step(
            &mut world,
            &mut authority,
            vec![Command::AcceptQuest {
                quest: quest::QUEST,
                giver: giver_id,
            }],
        );
        assert!(world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(quest::QUEST)
            .is_some());
        while world.state().npcs[&hostile].ai.phase() != ai::Phase::Combat
            || world.state().npcs[&hostile]
                .source_motion
                .as_ref()
                .unwrap()
                .motion
                .is_moving()
        {
            assert!(
                world.state().tick < 50,
                "quest chase state: {:?}",
                world.state()
            );
            step(&mut world, &mut authority, vec![]);
        }
        let player = world.state().position;
        let target = world.state().npcs[&hostile].ai.position();
        let angle = (target.y.as_f32() - player.y.as_f32())
            .atan2(target.x.as_f32() - player.x.as_f32())
            .rem_euclid(std::f32::consts::TAU);
        step(
            &mut world,
            &mut authority,
            vec![Command::MovementPose {
                destination: player,
                orientation: Orientation::new(angle).unwrap(),
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            }],
        );
        let started = world.state().tick;
        let mut pushbacks = 0;
        while world.state().npcs[&hostile].health > 0 && world.state().tick < 100 {
            let commands = if world.state().casts.is_empty() {
                vec![Command::Cast {
                    ability: quest::SPELL,
                    target: world.state().npcs[&hostile].id,
                }]
            } else {
                vec![]
            };
            step(&mut world, &mut authority, commands);
            pushbacks += world
                .last_tick()
                .unwrap()
                .events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        Event::Spell {
                            event: SpellEvent::Pushback(_),
                            ..
                        }
                    )
                })
                .count();
        }
        eprintln!(
            "quest hostile={hostile}, giver={giver}, order={order:?}, start={started}, end={}, health={}, pushbacks={pushbacks}",
            world.state().tick,
            world.state().npcs[&hostile].health
        );
        assert_eq!(
            world.state().npcs[&hostile].health,
            0,
            "quest source spell state: {:?}",
            world.state()
        );
        assert_eq!(world.state().npcs[&giver].ai.phase(), ai::Phase::Idle);
        let corpse = world.state().npcs[&hostile].id;
        for commands in [
            vec![Command::OpenLoot { corpse }],
            vec![
                Command::LootMoney { corpse },
                Command::LootItem { corpse, slot: 0 },
            ],
            vec![Command::ChooseQuestReward {
                quest: quest::QUEST,
                giver: giver_id,
                choice: 0,
            }],
        ] {
            step(&mut world, &mut authority, commands);
        }
        let progress = world.state().quest_progression.as_ref().unwrap();
        assert!(progress.quests.progress(quest::QUEST).unwrap().rewarded);
        assert_eq!(progress.player.view().level, 3);
        assert_eq!(progress.money, 21);
        assert_eq!(
            progress
                .inventory
                .as_ref()
                .unwrap()
                .item_count(quest::ITEM)
                .unwrap(),
            0
        );
        step(
            &mut world,
            &mut authority,
            vec![Command::ChooseQuestReward {
                quest: quest::QUEST,
                giver: giver_id,
                choice: 0,
            }],
        );
        assert_eq!(world.state().quest_progression.as_ref().unwrap().money, 21);
        assert_eq!(world.state().npcs[&giver].id, giver_id);
        assert_eq!(world.state().npcs[&giver].ai.phase(), ai::Phase::Idle);
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    }
}
fn cast(world: &NativeWorld, spawn: u64) -> Command {
    Command::Cast {
        ability: 60001,
        target: world.state().npcs[&spawn].id,
    }
}
fn step(world: &mut NativeWorld, authority: &mut impl Authority, commands: Vec<Command>) {
    let inputs = commands
        .into_iter()
        .enumerate()
        .map(|(offset, command)| OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                player: world.state().player,
                tick: world.state().tick + 1,
                sequence: world.state().next_sequence + offset as u64,
                command,
            },
        })
        .collect();
    world
        .step_batch(world.state().tick + 1, inputs, authority)
        .unwrap();
}

#[test]
fn lifecycle_cadence_preserves_real_pushback_and_bounds_spell_death_respawn() {
    let (products, revision, maps) = loaded_fixture();
    for repaired in [false, true] {
        let (mut profile, _, inputs) =
            composed_inputs(Arc::clone(&products), revision, &maps, 2, 3, [2, 3]);
        profile
            .npcs
            .iter_mut()
            .find(|npc| npc.ai.creature_id == 2)
            .unwrap()
            .max_health = 1;
        if repaired {
            configure_lifecycle_profile(&mut profile);
        }
        profile.validate().unwrap();
        // Profile edits change the binding, exactly as in installed().
        let mut session = source_session(&profile);
        session.primary_near_point =
            Some(core::PrimaryNearPointPolicy::from_profile(&profile).unwrap());
        session.same_polygon_route = Some(
            core::SamePolygonRoutePolicy::from_profile(
                &profile,
                f::player(),
                inputs.binding().unwrap(),
            )
            .unwrap(),
        );
        let mut providers = inputs.bind(&session, &profile, f::player()).unwrap();
        let mut world =
            NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234)
                .unwrap();
        let mut other = scene::CompleteAuthority::default();
        other.base.base.critical_chance = Some(0);
        other.base.base.resist_chance = Some(0);
        let mut authority = EnvironmentOnly {
            session: &session,
            providers: &mut providers,
            other,
        };
        let commands = vec![
            cast(&world, 3),
            Command::CancelCast {
                spell_id: 60001,
                target: CancelCastTarget::PriorInput(world.state().next_sequence),
            },
        ];
        step(&mut world, &mut authority, commands);
        while world.state().tick < 6 {
            step(&mut world, &mut authority, vec![]);
        }
        let command = cast(&world, 3);
        step(&mut world, &mut authority, vec![command]);
        while world.state().npcs[&3].auras.is_empty()
            || world
                .state()
                .npcs
                .values()
                .any(|npc| npc.source_motion.as_ref().unwrap().motion.is_moving())
        {
            assert!(
                world.state().tick < 60,
                "settling state: {:?}",
                world.state()
            );
            step(&mut world, &mut authority, vec![]);
        }
        assert!(
            world
                .state()
                .position
                .distance_squared(world.state().npcs[&2].ai.position())
                .unwrap()
                .as_f32()
                <= 25.
        );
        let cast_id = world.state().next_cast_id;
        let command = cast(&world, 2);
        step(&mut world, &mut authority, vec![command]);
        let started = world.state().tick;
        let mut phases = Vec::new();
        let mut events = world.last_tick().unwrap().events.clone();
        let mut overdue_casting = false;
        while world.state().npcs[&2].id.generation == 0 && world.state().tick < 110 {
            step(&mut world, &mut authority, vec![]);
            phases.push(world.state().npcs[&2].ai.phase());
            events.extend(world.last_tick().unwrap().events.clone());
            overdue_casting |= world.state().tick > started + 7
                && world.state().casts.get(&cast_id).is_some_and(|cast| {
                    matches!(cast.execution.state(), CastState::Casting { .. })
                });
            assert_eq!(world.state().npcs[&3].id.generation, 0);
            assert_eq!(world.state().npcs[&3].ai.phase(), ai::Phase::Combat);
            assert_eq!(
                world.state().npcs[&3].events.clock_ms(),
                world.state().now_ms
            );
        }
        let pushbacks = events.iter().filter(|event| matches!(event,
            Event::Spell { cast_id: id, event: SpellEvent::Pushback(_), .. } if *id == cast_id
        )).count();
        eprintln!(
            "lifecycle repaired={repaired}, start={started}, end={}, pushbacks={pushbacks}, generation={}, health={}, phase={:?}, cast={:?}",
            world.state().tick,
            world.state().npcs[&2].id.generation,
            world.state().npcs[&2].health,
            world.state().npcs[&2].ai.phase(),
            world
                .state()
                .casts
                .get(&cast_id)
                .map(|cast| cast.execution.state())
        );
        if repaired {
            assert!(
                world.state().tick <= started + 40,
                "source-accounted cast/projectile/respawn window"
            );
            assert_eq!(
                world.state().npcs[&2].id.generation,
                1,
                "state: {:?}, events: {events:?}",
                world.state()
            );
            assert!(phases.contains(&ai::Phase::Corpse));
            assert!(phases.contains(&ai::Phase::Despawned));
            assert!(events
                .iter()
                .any(|event| matches!(event, Event::Melee { effective: 1, .. })));
            assert!(pushbacks <= 2);
            assert!(!overdue_casting);
        } else {
            assert_eq!(world.state().npcs[&2].id.generation, 0);
            assert!(
                pushbacks > 2,
                "original cadence must exercise repeated source pushback: {events:?}"
            );
            assert!(overdue_casting);
        }
    }
}
