// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete-scene guards for the pinned source quest/loot interaction path.
//! Player.cpp:2414, Relations.cpp:617, QuestHandler.cpp:115/345 at 8ec338a1.
//! These tests do not establish production admission or loaded map traversal.
use super::{
    synthetic_distinct_quest_fixture as fixture, synthetic_fixture as f,
    synthetic_two_owner_fixture as two, *,
};
use benilla_sim::{player_progression as pg, quest_lifecycle as q};

fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}
fn step(
    world: &mut NativeWorld,
    command: Option<Command>,
    authority: &mut impl Authority,
) -> Result<(), NativeError> {
    let input = command.map(|command| fixture::command(world, command));
    world
        .step(world.state.tick + 1, input, authority)
        .map(|_| ())
}
fn progress(world: &NativeWorld) -> &QuestProgressionState {
    world.state.quest_progression.as_ref().unwrap()
}
fn quest(world: &NativeWorld) -> &q::Progress {
    progress(world).quests.progress(fixture::QUEST).unwrap()
}
fn rejected(world: &NativeWorld, expected: Rejection) {
    assert!(world
        .last_tick()
        .unwrap()
        .events
        .contains(&Event::Rejected(expected)));
    assert!(world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(
            event,
            Event::InputResult {
                accepted: false,
                ..
            }
        )));
}
fn accept(world: &mut NativeWorld, giver: u64, authority: &mut impl Authority) {
    let giver = world.state.npcs[&giver].id;
    step(
        world,
        Some(Command::AcceptQuest {
            quest: fixture::QUEST,
            giver,
        }),
        authority,
    )
    .unwrap();
    assert_eq!(quest(world).status, q::Status::Incomplete);
}
fn kill(world: &mut NativeWorld, hostile: u64, authority: &mut impl Authority) {
    for _ in 0..80 {
        if world.state.npcs[&hostile].health == 0 {
            return;
        }
        let command = world.state.casts.is_empty().then_some(Command::Cast {
            ability: fixture::SPELL,
            target: world.state.npcs[&hostile].id,
        });
        step(world, command, authority).unwrap();
    }
    panic!("source spell did not kill the authored hostile");
}
fn collect(world: &mut NativeWorld, hostile: u64, authority: &mut impl Authority) {
    let corpse = world.state.npcs[&hostile].id;
    for command in [
        Command::OpenLoot { corpse },
        Command::LootItem { corpse, slot: 0 },
        Command::ReleaseLoot { corpse },
    ] {
        step(world, Some(command), authority).unwrap();
    }
    assert_eq!(quest(world).items, [1, 0, 0, 0]);
    assert_eq!(quest(world).status, q::Status::Complete);
}
fn semantic(query: &Query) -> bool {
    matches!(
        query,
        Query::QuestStatus { .. }
            | Query::QuestInteraction { .. }
            | Query::LootInteraction { .. }
            | Query::Spell { .. }
            | Query::Npc { .. }
            | Query::NativeMelee { .. }
            | Query::SourceSpell(SourceSpellQuery::Semantics(_) | SourceSpellQuery::Range { .. })
            | Query::SourceMotion(
                MotionQuery::HostileSpellRelation(_)
                    | MotionQuery::VisitorSight(_)
                    | MotionQuery::Combat(
                        CombatQuery::NpcSemantics(_)
                            | CombatQuery::MeleeDamage(_)
                            | CombatQuery::MeleeCombatEntry(_)
                    )
            )
    )
}

#[test]
fn distinct_quest_source_guards_reject_wrong_giver_stale_generation_and_range() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            let profile = fixture::profile_with_spawns(hostile, giver, order);
            let mut world = world(profile.clone());
            let mut authority = fixture::CompleteAuthority::default();
            let before = world.checkpoint().unwrap();
            let current = world.state.npcs[&giver].id;
            assert_eq!(
                world
                    .preview_quest_interaction(current, fixture::QUEST, false, &mut authority)
                    .unwrap(),
                None
            );
            for (target, expected) in [
                (world.state.npcs[&hostile].id, Rejection::QuestInteraction),
                (
                    crate::NpcId {
                        generation: current.generation + 1,
                        ..current
                    },
                    Rejection::UnknownTarget,
                ),
            ] {
                for reward in [false, true] {
                    assert_eq!(
                        world
                            .preview_quest_interaction(
                                target,
                                fixture::QUEST,
                                reward,
                                &mut authority
                            )
                            .unwrap(),
                        Some(expected.clone())
                    );
                }
                let mut rejected_world = world.clone();
                step(
                    &mut rejected_world,
                    Some(Command::AcceptQuest {
                        quest: fixture::QUEST,
                        giver: target,
                    }),
                    &mut authority,
                )
                .unwrap();
                rejected(&rejected_world, expected);
                assert!(progress(&rejected_world)
                    .quests
                    .progress(fixture::QUEST)
                    .is_none());
            }
            assert_eq!(world.checkpoint().unwrap(), before);
            assert!(world.records().is_empty());
            // Initial location is authored separately; the status query has no range gate.
            let mut distant = profile;
            distant.player.start = f::point(100.0);
            world = self::world(distant);
            let before = world.checkpoint().unwrap();
            assert!(world.preview_quest_status(current, &mut authority).unwrap());
            for reward in [false, true] {
                assert_eq!(
                    world
                        .preview_quest_interaction(current, fixture::QUEST, reward, &mut authority)
                        .unwrap(),
                    Some(Rejection::QuestInteraction)
                );
            }
            assert_eq!(world.checkpoint().unwrap(), before);
        }
    }
}

#[test]
fn distinct_quest_relations_flags_and_source_reaction_are_independent_gates() {
    for case in 0..4 {
        let (mut profile, input, mut data) = fixture::setup_with_spawns(2, 3, [3, 2]);
        let mut rows = fixture::interaction_rows(&profile);
        match case {
            0 => data.relations.starters.clear(),
            1 => {
                data.npc_rows
                    .iter_mut()
                    .find(|row| row.entry == fixture::GIVER_ENTRY)
                    .unwrap()
                    .npc_flags = 1;
                rows.npcs
                    .iter_mut()
                    .find(|row| row.entry == fixture::GIVER_ENTRY)
                    .unwrap()
                    .npc_flags = 1;
            }
            2 => {
                // Raw faction zero still selects neutrality; its explicit hostile reaction
                // to the player independently denies CanInteract.
                rows.factions
                    .iter_mut()
                    .find(|row| row.template_id == 3)
                    .unwrap()
                    .enemy_group_mask = 1;
            }
            _ => data.relations.finishers.clear(),
        }
        profile.initialize_interactions(rows).unwrap();
        profile.initialize_quest_progression(&input, data).unwrap();
        let mut world = world(profile);
        let mut authority = fixture::CompleteAuthority::default();
        let giver = world.state.npcs[&fixture::GIVER].id;
        if case == 3 {
            accept(&mut world, fixture::GIVER, &mut authority);
            kill(&mut world, fixture::HOSTILE, &mut authority);
            collect(&mut world, fixture::HOSTILE, &mut authority);
            step(
                &mut world,
                Some(Command::TurnInQuest {
                    quest: fixture::QUEST,
                    giver,
                }),
                &mut authority,
            )
            .unwrap();
            rejected(&world, Rejection::Quest(q::Gate::Giver));
            assert!(!quest(&world).rewarded);
            assert_eq!(quest(&world).items, [1, 0, 0, 0]);
        } else {
            step(
                &mut world,
                Some(Command::AcceptQuest {
                    quest: fixture::QUEST,
                    giver,
                }),
                &mut authority,
            )
            .unwrap();
            rejected(
                &world,
                if case == 0 {
                    Rejection::Quest(q::Gate::Giver)
                } else {
                    Rejection::QuestInteraction
                },
            );
            assert!(progress(&world).quests.progress(fixture::QUEST).is_none());
            if case == 2 {
                assert!(authority.derived.iter().any(|reply| matches!(&reply.facts, Facts::QuestInteraction(facts) if facts.npc_reaction == benilla_formats::Reaction::Hostile as u8)));
            }
        }
    }
}

#[test]
fn distinct_quest_missing_or_inconsistent_progression_dependencies_do_not_initialize() {
    for case in 0..14 {
        let (mut profile, input, mut data) = fixture::setup_with_spawns(3, 2, [2, 3]);
        match case {
            0 => data.quest_audit.complete_relation_scan = false,
            1 => data.quest_audit.questgiver_hooks = q::Evidence::Unknown,
            2 => data.quest_audit.collection_item_effects = q::Evidence::Unknown,
            3 => data
                .kill
                .creatures
                .retain(|row| row.entry != fixture::GIVER_ENTRY),
            4 => data
                .loot
                .as_mut()
                .unwrap()
                .creatures
                .retain(|row| row.entry != fixture::GIVER_ENTRY),
            5 => data.loot.as_mut().unwrap().captures.clear(),
            6 => data.loot = None,
            7 => data.npc_rows.clear(),
            8 => {
                data.npc_rows
                    .iter_mut()
                    .find(|row| row.entry == fixture::GIVER_ENTRY)
                    .unwrap()
                    .npc_flags = 1
            }
            9 => {
                data.loot.as_mut().unwrap().audit.just_died_and_owner_hooks = pg::Evidence::Unknown
            }
            10 => data.kill.audit.tap_overrides = pg::Evidence::Unknown,
            11 => data.quests[0].required_item[0] = fixture::ITEM + 10,
            12 => {
                data.abilities
                    .iter_mut()
                    .find(|row| row.ability.id == fixture::SPELL)
                    .unwrap()
                    .cast_time
                    .as_mut()
                    .unwrap()
                    .id += 1
            }
            _ => data.quests[0].special_flags = q::KILL | q::DELIVER,
        }
        let before = profile.digest();
        assert!(
            profile.initialize_quest_progression(&input, data).is_err(),
            "dependency case {case}"
        );
        assert_eq!(profile.digest(), before, "partial genesis in case {case}");
        assert!(profile.quest_progression.is_none());
    }
}

#[test]
fn distinct_quest_requires_complete_source_owners_and_combat_capable_neutral_giver() {
    for case in 0..8 {
        let mut profile = fixture::profile_with_spawns(3, 2, [2, 3]);
        match case {
            0 => profile.interactions = None,
            1 => profile.source_visitors = None,
            2 => profile.creature_capabilities = None,
            3 => {
                profile.source_motion.as_mut().unwrap().npcs.remove(&2);
            }
            4 => {
                profile
                    .npcs
                    .iter_mut()
                    .find(|npc| npc.ai.creature_id == 2)
                    .unwrap()
                    .ai
                    .reaction = ai::Reaction::Passive
            }
            5 => {
                profile
                    .source_motion
                    .as_mut()
                    .unwrap()
                    .npcs
                    .get_mut(&2)
                    .unwrap()
                    .npc_can_enter_combat = false
            }
            6 => {
                profile
                    .npcs
                    .iter_mut()
                    .find(|npc| npc.ai.creature_id == 2)
                    .unwrap()
                    .ai
                    .neutral_to_all = false
            }
            _ => profile.combat = None,
        }
        assert!(profile.validate().is_err(), "source dependency case {case}");
        assert!(NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).is_err());
    }
    let profile = fixture::profile();
    for case in 0..5 {
        let mut input = two::visitor_input(&profile, vec![fixture::HOSTILE, fixture::GIVER]);
        let SourceVisitorRows::Complete(owners) = &mut input.owners else {
            unreachable!()
        };
        let giver = owners
            .iter_mut()
            .find(|owner| owner.spawn_id == fixture::GIVER)
            .unwrap();
        match case {
            0 => giver.entry_events = SourceVisitorRows::Missing,
            1 => giver.guid_events = SourceVisitorRows::Unknown,
            2 => giver.overrides = SourceVisitorRows::Missing,
            3 => giver.entry_events = SourceVisitorRows::Complete(vec![1]),
            _ => giver.ai_name = "NullAI".into(),
        }
        assert!(
            SourceVisitorCatalog::validate_sources(input).is_err(),
            "visitor dependency case {case}"
        );
    }
}

#[test]
fn distinct_quest_neutral_giver_retaliates_and_current_victim_denies_interaction() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            let mut profile = fixture::profile_with_spawns(hostile, giver, order);
            // Both actors start in the player's source-facing half-plane.
            profile.player.start = f::point(-4.0);
            let mut world = world(profile);
            let mut authority = fixture::CompleteAuthority::default();
            for _ in 0..3 {
                step(&mut world, None, &mut authority).unwrap();
            }
            let id = world.state.npcs[&giver].id;
            assert_eq!(world.state.npcs[&giver].ai.phase(), ai::Phase::Idle);
            assert_eq!(world.state.npcs[&giver].ai.target(), None);
            assert_eq!(
                world
                    .preview_quest_interaction(id, fixture::QUEST, false, &mut authority)
                    .unwrap(),
                None
            );
            step(
                &mut world,
                Some(Command::Cast {
                    ability: fixture::SPELL,
                    target: id,
                }),
                &mut authority,
            )
            .unwrap();
            for _ in 0..40 {
                if world.state.npcs[&giver].ai.phase() == ai::Phase::Combat {
                    break;
                }
                step(&mut world, None, &mut authority).unwrap();
            }
            let npc = &world.state.npcs[&giver];
            assert_eq!(npc.ai.phase(), ai::Phase::Combat);
            assert_eq!(npc.ai.target(), Some(SOLO_UNIT));
            assert!(npc.ai.hostile_reference().is_some());
            assert!(npc.health > 0);
            let before = world.checkpoint().unwrap();
            for reward in [false, true] {
                assert_eq!(
                    world
                        .preview_quest_interaction(id, fixture::QUEST, reward, &mut authority)
                        .unwrap(),
                    Some(Rejection::QuestInteraction)
                );
            }
            assert_eq!(world.checkpoint().unwrap(), before);
            step(
                &mut world,
                Some(Command::AcceptQuest {
                    quest: fixture::QUEST,
                    giver: id,
                }),
                &mut authority,
            )
            .unwrap();
            rejected(&world, Rejection::QuestInteraction);
            assert!(progress(&world).quests.progress(fixture::QUEST).is_none());
        }
    }
}

#[test]
fn distinct_quest_source_semantics_never_escape_to_the_spatial_adapter() {
    let mut world = fixture::world();
    let mut authority = fixture::CompleteAuthority::default();
    let giver = world.state.npcs[&fixture::GIVER].id;
    assert!(world.preview_quest_status(giver, &mut authority).unwrap());
    accept(&mut world, fixture::GIVER, &mut authority);
    kill(&mut world, fixture::HOSTILE, &mut authority);
    assert_eq!(quest(&world).creatures, [1, 0, 0, 0]);
    assert_eq!(quest(&world).items, [0; 4]);
    assert_eq!(quest(&world).status, q::Status::Incomplete);
    collect(&mut world, fixture::HOSTILE, &mut authority);
    assert!(authority
        .queries
        .iter()
        .all(|request| !semantic(&request.query)));
    let derived = authority.derived.clone();
    for label in [
        "status", "quest", "loot", "spell", "range", "combat", "visitor", "relation",
    ] {
        let reply = derived
            .iter()
            .find(|reply| {
                matches!(
                    (&reply.request.query, label),
                    (Query::QuestStatus { .. }, "status")
                        | (Query::QuestInteraction { .. }, "quest")
                        | (Query::LootInteraction { .. }, "loot")
                        | (Query::SourceSpell(SourceSpellQuery::Semantics(_)), "spell")
                        | (Query::SourceSpell(SourceSpellQuery::Range { .. }), "range")
                        | (
                            Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(_))),
                            "combat",
                        )
                        | (Query::SourceMotion(MotionQuery::VisitorSight(_)), "visitor")
                        | (
                            Query::SourceMotion(MotionQuery::HostileSpellRelation(_)),
                            "relation"
                        )
                )
            })
            .unwrap_or_else(|| panic!("missing source-derived {label} receipt"));
        assert!(semantic(&reply.request.query));
        assert!(
            fixture::CompleteAuthority::default()
                .resolve(&reply.request)
                .is_err(),
            "external adapter accepted {label} semantics"
        );
    }
}

struct FailGiver {
    base: fixture::CompleteAuthority,
    giver: u64,
    fail: bool,
}
impl Authority for FailGiver {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.base.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.base.observe_derived(reply)?;
        if self.fail
            && reply
                .request
                .binding
                .npc
                .is_some_and(|npc| npc.spawn == self.giver)
            && matches!(&reply.request.query, Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(request))) if request.phase == CombatPhase::UnitPrelude)
        {
            return Err(NativeError::AuthorityUnavailable(
                "authored late questgiver reward fault",
            ));
        }
        Ok(())
    }
}

#[test]
fn distinct_quest_late_giver_failure_rolls_back_reward_and_exact_retry() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        let mut world = world(fixture::profile_with_spawns(
            hostile,
            giver,
            [hostile, giver],
        ));
        let mut authority = FailGiver {
            base: Default::default(),
            giver,
            fail: false,
        };
        accept(&mut world, giver, &mut authority);
        kill(&mut world, hostile, &mut authority);
        collect(&mut world, hostile, &mut authority);
        let before = world.checkpoint().unwrap();
        let records = world.records().to_vec();
        let command = Command::TurnInQuest {
            quest: fixture::QUEST,
            giver: world.state.npcs[&giver].id,
        };
        let mut expected = world.clone();
        step(
            &mut expected,
            Some(command.clone()),
            &mut fixture::CompleteAuthority::default(),
        )
        .unwrap();
        assert!(quest(&expected).rewarded);
        assert!(progress(&expected).money > progress(&world).money);
        assert!(expected.player_level() > world.player_level());
        assert_ne!(progress(&expected).inventory, progress(&world).inventory);
        authority.fail = true;
        assert_eq!(
            step(&mut world, Some(command.clone()), &mut authority),
            Err(NativeError::AuthorityUnavailable(
                "authored late questgiver reward fault"
            ))
        );
        // Checkpoint includes both owners, progression/training, inventory, mana/casts,
        // clocks, sequence and RNG; the independent journal equality covers all output.
        assert_eq!(world.checkpoint().unwrap(), before);
        assert_eq!(world.records(), records);
        assert!(!quest(&world).rewarded);
        authority.fail = false;
        step(&mut world, Some(command), &mut authority).unwrap();
        assert_eq!(world.checkpoint().unwrap(), expected.checkpoint().unwrap());
        assert_eq!(world.last_tick(), expected.last_tick());
    }
}

#[test]
fn distinct_quest_late_giver_failure_rolls_back_lethal_xp_credit_loot_and_rng() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        let mut world = world(fixture::profile_with_spawns(
            hostile,
            giver,
            [hostile, giver],
        ));
        accept(
            &mut world,
            giver,
            &mut fixture::CompleteAuthority::default(),
        );
        let mut reached_death = false;
        for _ in 0..80 {
            let command = world.state.casts.is_empty().then_some(Command::Cast {
                ability: fixture::SPELL,
                target: world.state.npcs[&hostile].id,
            });
            let mut expected = world.clone();
            step(
                &mut expected,
                command.clone(),
                &mut fixture::CompleteAuthority::default(),
            )
            .unwrap();
            if expected.state.npcs[&hostile].health != 0 {
                world = expected;
                continue;
            }
            reached_death = true;
            assert!(world.state.npcs[&hostile].health > 0);
            assert!(expected.state.npcs[&hostile].corpse_loot.is_some());
            assert!(expected.player_level() > world.player_level());
            assert_eq!(quest(&world).creatures, [0; 4]);
            assert_eq!(quest(&expected).creatures, [1, 0, 0, 0]);
            assert_eq!(quest(&expected).items, [0; 4]);
            assert_eq!(quest(&expected).status, q::Status::Incomplete);
            assert!(expected.state.rng_counter > world.state.rng_counter);
            let events = &expected.last_tick().unwrap().events;
            let xp = events
                .iter()
                .position(|event| {
                    matches!(
                        event,
                        Event::QuestProgression(QuestProgressionEvent::KillExperience(_))
                    )
                })
                .unwrap();
            let credit = events
                .iter()
                .position(|event| {
                    matches!(
                        event,
                        Event::QuestProgression(QuestProgressionEvent::Quest {
                            event: q::Event::KillCredit { .. },
                            ..
                        })
                    )
                })
                .unwrap();
            assert!(xp < credit);
            let before = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            let mut authority = FailGiver {
                base: Default::default(),
                giver,
                fail: true,
            };
            assert_eq!(
                step(&mut world, command.clone(), &mut authority),
                Err(NativeError::AuthorityUnavailable(
                    "authored late questgiver reward fault"
                ))
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            assert!(world.state.npcs[&hostile].corpse_loot.is_none());
            assert_eq!(quest(&world).creatures, [0; 4]);
            authority.fail = false;
            step(&mut world, command, &mut authority).unwrap();
            assert_eq!(world.checkpoint().unwrap(), expected.checkpoint().unwrap());
            assert_eq!(world.last_tick(), expected.last_tick());
            break;
        }
        assert!(
            reached_death,
            "authored hostile never reached its lethal progression tick"
        );
    }
}

#[test]
fn distinct_quest_interaction_distance_uses_strict_source_binary32_boundary() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            for (position, allowed) in [
                (-10.5_f32, false),
                (f32::from_bits((-10.5_f32).to_bits() - 1), true),
            ] {
                let mut profile = fixture::profile_with_spawns(hostile, giver, order);
                let npc = profile.npc(giver).unwrap();
                assert_eq!(profile.player.combat_reach, f::real(1.5));
                assert_eq!(npc.ai.combat_reach, f::real(2.0));
                assert_eq!(npc.ai.spawn, f::point(-2.0));
                profile.player.start = f::point(position);
                // Source sums the reaches before adding five, then compares squared
                // binary32 distances strictly. The next negative float is truly inside.
                let square = profile
                    .player
                    .start
                    .distance_squared(f::point(-2.0))
                    .unwrap();
                if allowed {
                    assert!(square.as_f32() < 72.25);
                } else {
                    assert_eq!(square, f::real(72.25));
                }
                let mut world = world(profile);
                let mut authority = fixture::CompleteAuthority::default();
                let id = world.state.npcs[&giver].id;
                let before = world.checkpoint().unwrap();
                assert!(world.preview_quest_status(id, &mut authority).unwrap());
                for reward in [false, true] {
                    assert_eq!(
                        world
                            .preview_quest_interaction(id, fixture::QUEST, reward, &mut authority)
                            .unwrap(),
                        (!allowed).then_some(Rejection::QuestInteraction)
                    );
                }
                assert_eq!(world.checkpoint().unwrap(), before);
                step(
                    &mut world,
                    Some(Command::AcceptQuest {
                        quest: fixture::QUEST,
                        giver: id,
                    }),
                    &mut authority,
                )
                .unwrap();
                if allowed {
                    assert_eq!(quest(&world).status, q::Status::Incomplete);
                } else {
                    rejected(&world, Rejection::QuestInteraction);
                    assert!(progress(&world).quests.progress(fixture::QUEST).is_none());
                }
            }
        }
    }
}

#[test]
fn distinct_quest_ordinary_item_drops_and_is_collectible_before_quest_acceptance() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            let mut world = world(fixture::profile_with_spawns(hostile, giver, order));
            let mut authority = fixture::CompleteAuthority::default();
            kill(&mut world, hostile, &mut authority);
            assert!(progress(&world).quests.progress(fixture::QUEST).is_none());
            let npc = &world.state.npcs[&hostile];
            let corpse = npc.id;
            let loot = npc.corpse_loot.as_ref().unwrap();
            assert_eq!(loot.generated.items.len(), 1);
            assert_eq!(loot.generated.items[0].item, fixture::ITEM);
            assert_eq!(loot.generated.items[0].count, 1);
            assert_eq!(loot.generated.money, 3);
            assert!(!loot.generated.fake);
            for command in [
                Command::OpenLoot { corpse },
                Command::LootItem { corpse, slot: 0 },
                Command::ReleaseLoot { corpse },
            ] {
                step(&mut world, Some(command), &mut authority).unwrap();
            }
            assert_eq!(
                progress(&world)
                    .inventory
                    .as_ref()
                    .unwrap()
                    .item_count(fixture::ITEM)
                    .unwrap(),
                1
            );
            assert!(progress(&world).quests.progress(fixture::QUEST).is_none());
            accept(&mut world, giver, &mut authority);
            assert_eq!(quest(&world).items, [1, 0, 0, 0]);
            assert_eq!(quest(&world).creatures, [0; 4]);
            assert_eq!(quest(&world).status, q::Status::Incomplete);
        }
    }
}

fn participant(world: &NativeWorld, spawn: u64) -> VisitorParticipant {
    VisitorParticipant::Creature(world.state.npcs[&spawn].id)
}

#[test]
fn distinct_quest_reached_peer_help_rejects_whole_tick_and_exact_visibility_retry() {
    use ai::movement::Check;
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            let mut profile = fixture::profile_with_spawns(hostile, giver, order);
            profile.player.start = f::point(-4.0);
            let mut world = world(profile);
            let a = participant(&world, hostile);
            let b = participant(&world, giver);
            let player = VisitorParticipant::Player(world.state.player);
            let mut authority = fixture::CompleteAuthority::default();
            authority.base.visibility_overrides =
                vec![(a, player, Check::Failed), (a, b, Check::Failed)];
            accept(&mut world, giver, &mut authority);
            let id = world.state.npcs[&giver].id;
            step(
                &mut world,
                Some(Command::Cast {
                    ability: fixture::SPELL,
                    target: id,
                }),
                &mut authority,
            )
            .unwrap();
            for _ in 0..40 {
                if world.state.npcs[&giver].ai.phase() == ai::Phase::Combat {
                    break;
                }
                step(&mut world, None, &mut authority).unwrap();
            }
            assert_eq!(world.state.npcs[&giver].ai.phase(), ai::Phase::Combat);
            assert_eq!(world.state.npcs[&hostile].ai.phase(), ai::Phase::Idle);
            world
                .state
                .npcs
                .get_mut(&giver)
                .unwrap()
                .events
                .schedule_notify(0, true)
                .unwrap();
            let before = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            let mut expected = world.clone();
            let mut clean = fixture::CompleteAuthority::default();
            clean.base.visibility_overrides = authority.base.visibility_overrides.clone();
            step(&mut expected, None, &mut clean).unwrap();
            authority.base.visibility_overrides.pop();
            assert_eq!(
                step(&mut world, None, &mut authority),
                Err(NativeError::UnsupportedMechanic(
                    "peer sight reached CheckForHelp"
                ))
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            assert_eq!(world.state.npcs[&hostile].ai.target(), None);
            assert_eq!(world.state.npcs[&giver].ai.target(), Some(SOLO_UNIT));
            assert!(authority.base.queries.iter().any(|request| matches!(&request.query, Query::SourceMotion(MotionQuery::SceneVisitor(scene)) if scene.notification_owner == b && scene.creatures.len() == 2)));
            authority.base.visibility_overrides = clean.base.visibility_overrides;
            step(&mut world, None, &mut authority).unwrap();
            assert_eq!(world.checkpoint().unwrap(), expected.checkpoint().unwrap());
            assert_eq!(world.last_tick(), expected.last_tick());
        }
    }
}

#[test]
fn distinct_quest_reached_peer_attack_rejects_without_installing_peer_targets() {
    use ai::movement::Check;
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            let mut profile = fixture::profile_with_spawns(hostile, giver, order);
            profile
                .npcs
                .iter_mut()
                .find(|npc| npc.ai.creature_id == giver)
                .unwrap()
                .ai
                .neutral_to_all = false;
            let mut rows = fixture::interaction_rows(&profile);
            let faction = rows
                .factions
                .iter_mut()
                .find(|row| row.template_id == 3)
                .unwrap();
            faction.faction = 96003;
            faction.reputation_list_id = Some(-1);
            faction.group_mask = 4;
            faction.friend_group_mask = 4;
            faction.enemy_group_mask = 2;
            profile.initialize_interactions(rows).unwrap();
            let mut world = world(profile);
            let a = participant(&world, hostile);
            let mut authority = fixture::CompleteAuthority::default();
            authority.base.visibility_overrides = vec![(
                a,
                VisitorParticipant::Player(world.state.player),
                Check::Failed,
            )];
            accept(&mut world, giver, &mut authority);
            let before = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            assert_eq!(
                step(&mut world, None, &mut authority),
                Err(NativeError::UnsupportedMechanic(
                    "peer sight attack/accessibility continuation"
                ))
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            assert!(world
                .state
                .npcs
                .values()
                .all(|npc| npc.ai.target().is_none()));
            assert!(authority.base.queries.iter().any(|request| matches!(&request.query, Query::SourceMotion(MotionQuery::SceneVisitor(scene)) if scene.creatures.len() == 2)));
        }
    }
}
